$ErrorActionPreference = 'Stop'
$root = Join-Path $PSScriptRoot 'data'
$output = Join-Path $root 'clock.ini'
try {
    $settings = @{}; $section = ''
    Get-Content -LiteralPath (Join-Path $root 'EnhancedSettings.ini') | ForEach-Object {
        if ($_ -match '^\[(.+)\]$') { $section = $Matches[1] }
        elseif ($section -eq 'Options' -and $_ -match '^([^=]+)=(.*)$') { $settings[$Matches[1]] = $Matches[2] }
    }
    $uri = [uri]$settings.ClockURL
    if ($uri.Scheme -ne 'https' -or $uri.UserInfo) { throw 'HTTPS 시각 API 주소가 필요합니다.' }
    $mode = $settings.ClockMode
    if ($mode -notin @('date','milliseconds','seconds','iso')) { throw '시각 형식을 확인하세요.' }
    $target = [DateTimeOffset]::ParseExact($settings.SaleTime + ' +09:00', 'yyyy-MM-dd HH:mm:ss.fff zzz', [Globalization.CultureInfo]::InvariantCulture)
    $samples = @()
    for ($i=0; $i -lt 3; $i++) {
        $request = [Net.HttpWebRequest]::Create($uri)
        $request.Method = $(if ($mode -eq 'date') {'HEAD'} else {'GET'})
        $request.AllowAutoRedirect = $false; $request.Timeout = 5000; $request.ReadWriteTimeout = 5000
        $request.CachePolicy = New-Object Net.Cache.RequestCachePolicy([Net.Cache.RequestCacheLevel]::NoCacheNoStore)
        $start = [Diagnostics.Stopwatch]::GetTimestamp() * 1000.0 / [Diagnostics.Stopwatch]::Frequency
        $response = $request.GetResponse()
        try {
            if ([int]$response.StatusCode -lt 200 -or [int]$response.StatusCode -ge 300) { throw 'API 리디렉션/오류 응답' }
            if ($response.Headers['Age'] -and [double]$response.Headers['Age'] -gt 0) { throw '캐시된 시각 응답' }
            if ($mode -eq 'date') {
                $value = $response.Headers['Date']; $precision = 1000
            } else {
                $reader = New-Object IO.StreamReader($response.GetResponseStream())
                $buffer = New-Object char[] 65537; $count = $reader.ReadBlock($buffer,0,65537)
                if ($count -eq 0 -or $count -gt 65536) { throw 'API 응답이 비어 있거나 너무 큽니다.' }
                $text = -join $buffer[0..($count-1)]; $value = $text.Trim(); $precision = 1
                if ($settings.ClockPath) {
                    $value = $text | ConvertFrom-Json
                    foreach ($part in $settings.ClockPath.Split('.')) {
                        if ($part -notmatch '^[A-Za-z0-9_-]+$') { throw 'JSON 필드 경로 오류' }
                        $value = $value.$part
                    }
                }
            }
        } finally { $response.Close() }
        $end = [Diagnostics.Stopwatch]::GetTimestamp() * 1000.0 / [Diagnostics.Stopwatch]::Frequency
        $rtt = $end-$start
        if ($rtt -gt 3000) { throw '시각 측정 지연이 3초를 넘었습니다.' }
        if ($mode -in @('date','iso')) {
            if ($mode -eq 'iso' -and "$value" -notmatch '(Z|[+-]\d\d:\d\d)$') { throw 'ISO 시각에는 시간대가 필요합니다.' }
            $server = [DateTimeOffset]::Parse($value,[Globalization.CultureInfo]::InvariantCulture).ToUnixTimeMilliseconds()
            if ($mode -eq 'date') { $server += 500 }
            if ($mode -eq 'iso') {
                $precision = 1000
                if ("$value" -match '\.([0-9]+)') { $precision = [Math]::Pow(10,[Math]::Max(0,3-$Matches[1].Length)) }
            }
        } else {
            $server = [double]::Parse("$value",[Globalization.CultureInfo]::InvariantCulture)
            if ($mode -eq 'seconds') { $server *= 1000; $precision = 1000 }
        }
        if ([double]::IsNaN($server) -or [double]::IsInfinity($server) -or $server -lt 946684800000 -or $server -gt 4102444800000) { throw '유효한 Unix 시각이 아닙니다.' }
        $samples += [pscustomobject]@{Server=$server; Mono=($start+$end)/2; RTT=$rtt; Error=($rtt+$precision)/2}
    }
    $best = $samples | Sort-Object RTT | Select-Object -First 1
    $due = $best.Mono + $target.ToUnixTimeMilliseconds() - $best.Server
    if ($due -le $best.Mono -or $due-$best.Mono -gt 43200000) { throw '예약 시각은 측정 후 12시간 이내의 미래여야 합니다.' }
    $c = [Globalization.CultureInfo]::InvariantCulture
    @('[Clock]','OK=1',('Due='+$due.ToString('F3',$c)),('Measured='+$best.Mono.ToString('F3',$c)),('Uncertainty='+$best.Error.ToString('F1',$c)),('RTT='+$best.RTT.ToString('F1',$c))) | Set-Content -LiteralPath $output -Encoding Unicode
} catch {
    @('[Clock]','OK=0',('Error='+($_.Exception.Message -replace '[\r\n]',' '))) | Set-Content -LiteralPath $output -Encoding Unicode
    exit 1
}
