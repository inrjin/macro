# Requires Windows PowerShell 5.1+ in an x64 Native Tools command prompt.
# This script has been source-reviewed on macOS, not executed on Windows.
[CmdletBinding()]
param(
    [string]$SourceRoot = '',
    [string]$OutputRoot = ''
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$KitRoot = Split-Path -Parent $PSScriptRoot
$Target = 'x86_64-pc-windows-msvc'
$SourceManifest = Join-Path $KitRoot 'source-manifest.json'
$FixtureManifest = Join-Path $KitRoot 'fixture-manifest.json'
$GeneratedSchemas = @(
    'src-tauri/gen/schemas/acl-manifests.json',
    'src-tauri/gen/schemas/capabilities.json',
    'src-tauri/gen/schemas/desktop-schema.json',
    'src-tauri/gen/schemas/windows-schema.json'
)
$RunRoot = $null
$LogPath = $null
$Stage = 'preflight'
$Pushed = $false
$OriginalTargetDir = $env:CARGO_TARGET_DIR
$Report = [ordered]@{
    success = $false
    version = '0.5.11'
    target = $Target
    startedAtUtc = [DateTime]::UtcNow.ToString('o')
    finishedAtUtc = $null
    stage = $Stage
    sourceRoot = $null
    workSource = $null
    manifests = @{}
    tools = @{}
    generatedSchemas = @()
    artifacts = @()
    windowsApplicationRun = $false
    liveCaptureHotkeyInputVerified = $false
    error = $null
}

function Write-BuildLog([string]$Message) {
    Write-Host $Message
    if ($script:LogPath) { Add-Content -LiteralPath $script:LogPath -Value $Message -Encoding UTF8 }
}

function Invoke-Native([string]$Program, [string[]]$Arguments, [switch]$Capture) {
    Write-BuildLog ('> ' + $Program + ' ' + ($Arguments -join ' '))
    $lines = New-Object 'System.Collections.Generic.List[string]'
    $oldPreference = $ErrorActionPreference
    try {
        # Native stderr is diagnostic output, not a PowerShell terminating error.
        $ErrorActionPreference = 'Continue'
        $global:LASTEXITCODE = $null
        & $Program @Arguments 2>&1 | ForEach-Object {
            $line = $_.ToString()
            if ($Capture) { $lines.Add($line) }
            Write-BuildLog $line
        }
        $nativeExit = $LASTEXITCODE
    } finally { $ErrorActionPreference = $oldPreference }
    if ($null -eq $nativeExit -or $nativeExit -ne 0) {
        throw "External command did not complete successfully (exit code '${nativeExit}'): $Program"
    }
    if ($Capture) { return ($lines -join "`n") }
}

function Read-Manifest([string]$Path, [int]$ExpectedCount) {
    $json = Get-Content -LiteralPath $Path -Raw -Encoding UTF8 | ConvertFrom-Json
    $map = @{}
    foreach ($property in $json.PSObject.Properties) {
        $relative = $property.Name
        $digest = [string]$property.Value
        if ($relative -match '(^/|\\|:|(^|/)\.\.?(/|$)|//)' -or $relative.EndsWith('/')) {
            throw "Unsafe manifest path: $relative"
        }
        if ($digest -notmatch '^[0-9a-f]{64}$' -or $map.ContainsKey($relative)) {
            throw "Invalid or duplicate manifest entry: $relative"
        }
        $map.Add($relative, $digest)
    }
    if ($map.Count -ne $ExpectedCount) { throw "Unexpected manifest count in ${Path}: $($map.Count)" }
    return $map
}

function Get-TreeFiles([string]$Root, [switch]$AllowDependencies) {
    $directories = New-Object 'System.Collections.Generic.Stack[string]'
    $directories.Push($Root)
    while ($directories.Count -gt 0) {
        foreach ($entry in Get-ChildItem -LiteralPath ($directories.Pop()) -Force) {
            $relative = $entry.FullName.Substring($Root.Length + 1).Replace('\', '/')
            if ($AllowDependencies -and $relative -eq 'node_modules') { continue }
            if (($entry.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
                throw "Reparse point is not permitted in source inputs: $relative"
            }
            if ($entry.PSIsContainer) { $directories.Push($entry.FullName) }
            else { $relative }
        }
    }
}

function Assert-Tree([string]$Root, [hashtable]$Manifest, [switch]$WorkingCopy) {
    foreach ($relative in Get-TreeFiles $Root -AllowDependencies:$WorkingCopy) {
        if ($WorkingCopy -and $script:GeneratedSchemas -contains $relative) { continue }
        if (-not $Manifest.ContainsKey($relative)) { throw "Unlisted source/fixture file: $relative" }
    }
    foreach ($relative in $Manifest.Keys) {
        if ($WorkingCopy -and $script:GeneratedSchemas -contains $relative) { continue }
        $file = Join-Path $Root $relative
        if (-not (Test-Path -LiteralPath $file -PathType Leaf)) { throw "Missing source/fixture file: $relative" }
        $actual = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($actual -ne $Manifest[$relative]) { throw "Source/fixture hash mismatch: $relative" }
    }
}

function Get-PeMachine([string]$Path) {
    $stream = [IO.File]::OpenRead($Path)
    $reader = New-Object IO.BinaryReader($stream)
    try {
        if ($stream.Length -lt 64 -or $reader.ReadUInt16() -ne 0x5a4d) { throw "Not an MZ executable: $Path" }
        $stream.Position = 0x3c
        $offset = $reader.ReadInt32()
        if ($offset -lt 64 -or $offset -gt $stream.Length - 6) { throw "Invalid PE header offset: $Path" }
        $stream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x4550) { throw "Not a PE executable: $Path" }
        return $reader.ReadUInt16()
    } finally { $reader.Dispose(); $stream.Dispose() }
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT -or -not [Environment]::Is64BitProcess) {
        throw 'Run this script in a 64-bit PowerShell session on Windows x64.'
    }
    if ($env:PROCESSOR_ARCHITECTURE -ne 'AMD64') { throw 'This kit targets Windows x64, not an ARM64 build host.' }
    if ([string]::IsNullOrEmpty($SourceRoot)) { $SourceRoot = Join-Path $KitRoot 'source' }
    if ([string]::IsNullOrEmpty($OutputRoot)) { $OutputRoot = Join-Path $KitRoot 'output' }
    $SourceRoot = (Resolve-Path -LiteralPath $SourceRoot).Path.TrimEnd('\', '/')
    if (((Get-Item -LiteralPath $SourceRoot).Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
        throw 'SourceRoot must not be a reparse point.'
    }
    $OutputRoot = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\', '/')
    if ($OutputRoot -eq $SourceRoot -or $OutputRoot.StartsWith($SourceRoot + '\', [StringComparison]::OrdinalIgnoreCase)) {
        throw 'OutputRoot must be outside the immutable source directory.'
    }
    [IO.Directory]::CreateDirectory($OutputRoot) | Out-Null
    $RunRoot = Join-Path $OutputRoot ([DateTime]::UtcNow.ToString('yyyyMMdd-HHmmss') + '-' + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $RunRoot | Out-Null
    $LogPath = Join-Path $RunRoot 'build.log'
    $Report.sourceRoot = $SourceRoot
    Write-BuildLog "Build attempt: $RunRoot"

    $source = Read-Manifest $SourceManifest 138
    $fixtures = Read-Manifest $FixtureManifest 417
    $all = @{}
    foreach ($entry in $source.GetEnumerator()) { $all.Add($entry.Key, $entry.Value) }
    foreach ($entry in $fixtures.GetEnumerator()) {
        if ($all.ContainsKey($entry.Key)) { throw "Overlapping manifests: $($entry.Key)" }
        $all.Add($entry.Key, $entry.Value)
    }
    Assert-Tree $SourceRoot $all
    $Report.manifests = @{
        source = (Get-FileHash -LiteralPath $SourceManifest -Algorithm SHA256).Hash.ToLowerInvariant()
        fixtures = (Get-FileHash -LiteralPath $FixtureManifest -Algorithm SHA256).Hash.ToLowerInvariant()
        sourceFiles = $source.Count
        fixtureFiles = $fixtures.Count
    }
    foreach ($tool in @('node.exe', 'npm.cmd', 'cargo.exe', 'rustc.exe', 'rustup.exe', 'cl.exe', 'link.exe', 'rc.exe')) {
        $command = Get-Command $tool -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $command) { throw "Missing $tool. Install the prerequisites and use an x64 Native Tools command prompt; this script installs no tools." }
        $Report.tools[$tool] = $command.Source
    }
    if ($env:VSCMD_ARG_TGT_ARCH -ne 'x64') {
        throw 'Open the x64 Native Tools Command Prompt for Visual Studio (C++ Build Tools and Windows SDK required).'
    }
    $Report.tools.nodeVersion = Invoke-Native 'node.exe' @('--version') -Capture
    $nodeArch = Invoke-Native 'node.exe' @('-p', 'process.arch') -Capture
    if ($nodeArch.Trim() -ne 'x64') { throw 'Install the x64 version of Node.js.' }
    $Report.tools.npmVersion = Invoke-Native 'npm.cmd' @('--version') -Capture
    $Report.tools.cargoVersion = Invoke-Native 'cargo.exe' @('--version') -Capture
    $Report.tools.rustVersion = Invoke-Native 'rustc.exe' @('-vV') -Capture
    if ($Report.tools.rustVersion -notmatch '(?m)^host: x86_64-pc-windows-msvc\s*$') {
        throw 'The active Rust toolchain must use the x86_64-pc-windows-msvc host.'
    }
    $installedTargets = Invoke-Native 'rustup.exe' @('target', 'list', '--installed') -Capture
    if (($installedTargets -split '\s+') -notcontains $Target) { throw "Missing Rust target $Target." }

    $Stage = 'copy verified inputs'
    $WorkSource = Join-Path $RunRoot 'work-source'
    [IO.Directory]::CreateDirectory($WorkSource) | Out-Null
    foreach ($relative in $all.Keys) {
        $destination = Join-Path $WorkSource $relative
        [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
        Copy-Item -LiteralPath (Join-Path $SourceRoot $relative) -Destination $destination
    }
    Assert-Tree $WorkSource $all
    $Report.workSource = $WorkSource
    $env:CARGO_TARGET_DIR = Join-Path $RunRoot 'cargo-target'
    Push-Location -LiteralPath $WorkSource
    $Pushed = $true

    $Stage = 'npm ci'
    Invoke-Native 'npm.cmd' @('ci', '--no-audit', '--no-fund')
    Assert-Tree $WorkSource $all -WorkingCopy
    $Stage = 'JavaScript tests'
    Invoke-Native 'npm.cmd' @('test')
    Assert-Tree $WorkSource $all -WorkingCopy
    $Stage = 'Windows Rust workspace tests'
    Invoke-Native 'cargo.exe' @('test', '--workspace', '--locked', '--target', $Target)
    Assert-Tree $WorkSource $all -WorkingCopy
    $Stage = 'release NSIS build'
    $cli = Join-Path $WorkSource 'node_modules/@tauri-apps/cli/tauri.js'
    if (-not (Test-Path -LiteralPath $cli -PathType Leaf)) { throw 'The locked local Tauri CLI was not installed.' }
    Invoke-Native 'node.exe' @($cli, 'build', '--ci', '--target', $Target, '--bundles', 'nsis', '--', '--locked')
    Assert-Tree $WorkSource $all -WorkingCopy
    Assert-Tree $SourceRoot $all

    $Stage = 'verify fresh artifacts'
    $release = Join-Path $env:CARGO_TARGET_DIR "$Target/release"
    $main = Join-Path $release 'integrated-macro.exe'
    if (-not (Test-Path -LiteralPath $main -PathType Leaf)) { throw 'The main Windows executable was not produced.' }
    if ((Get-PeMachine $main) -ne 0x8664) { throw 'The main executable is not a Windows x64 PE.' }
    $nsis = Join-Path $release 'bundle/nsis'
    $installers = @(Get-ChildItem -LiteralPath $nsis -File -Filter '*-setup.exe')
    if ($installers.Count -lt 1) { throw 'No NSIS setup executable was produced.' }
    $candidateArtifacts = Join-Path $RunRoot 'candidate-artifacts'
    New-Item -ItemType Directory -Path $candidateArtifacts | Out-Null
    $artifactRecords = @()
    $files = @((Get-Item -LiteralPath $main)) + $installers
    foreach ($file in $files) {
        $machine = Get-PeMachine $file.FullName
        if ($machine -notin @(0x14c, 0x8664)) { throw "Unexpected executable machine type: $($file.Name)" }
        $destination = Join-Path $candidateArtifacts $file.Name
        if (Test-Path -LiteralPath $destination) { throw "Artifact name collision: $($file.Name)" }
        Copy-Item -LiteralPath $file.FullName -Destination $destination
        $artifactRecords += [ordered]@{
            path = 'artifacts/' + $file.Name
            bytes = $file.Length
            sha256 = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
            peMachine = ('0x{0:x4}' -f $machine)
            fileVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($destination).FileVersion
            productVersion = [Diagnostics.FileVersionInfo]::GetVersionInfo($destination).ProductVersion
            signatureStatus = [string](Get-AuthenticodeSignature -LiteralPath $destination).Status
        }
    }
    foreach ($relative in $GeneratedSchemas) {
        $file = Join-Path $WorkSource $relative
        $digest = if (Test-Path -LiteralPath $file -PathType Leaf) { (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant() } else { $null }
        $original = if ($source.ContainsKey($relative)) { $source[$relative] } else { $null }
        $Report.generatedSchemas += @{ path = $relative; before = $original; after = $digest }
    }
    # Publish only after every file and its metadata passed verification.
    Move-Item -LiteralPath $candidateArtifacts -Destination (Join-Path $RunRoot 'artifacts')
    $Report.artifacts = $artifactRecords
    $Report.success = $true
    $Stage = 'complete'
} catch {
    $Report.error = $_.Exception.Message
    Write-BuildLog ('FAILED at ' + $Stage + ': ' + $Report.error)
} finally {
    if ($Pushed) { Pop-Location }
    $env:CARGO_TARGET_DIR = $OriginalTargetDir
    $Report.stage = $Stage
    $Report.finishedAtUtc = [DateTime]::UtcNow.ToString('o')
    if ($RunRoot) {
        $Report | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $RunRoot 'report.json') -Encoding UTF8
    }
}
if (-not $Report.success) { exit 1 }
Write-BuildLog "Build and automated checks passed. Artifacts and hashes: $RunRoot\report.json"
Write-BuildLog 'Application launch, installation, WebView2 availability, live capture, hotkeys and input still require Windows validation.'
exit 0
