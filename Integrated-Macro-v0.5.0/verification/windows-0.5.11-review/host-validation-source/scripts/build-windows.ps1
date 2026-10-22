$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")
foreach ($tool in @("cargo", "npm.cmd")) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool is required. See README.md."
    }
}
& npm.cmd install --no-audit --no-fund
if ($LASTEXITCODE -ne 0) { throw "Dependency installation failed." }
& npm.cmd test
if ($LASTEXITCODE -ne 0) { throw "JavaScript tests failed." }
& cargo test --workspace
if ($LASTEXITCODE -ne 0) { throw "Rust tests failed." }
& npm.cmd run build -- --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "Windows bundle failed." }

