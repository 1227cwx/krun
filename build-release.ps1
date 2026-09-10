param(
    [switch]$Clean
)

$ErrorActionPreference = "Stop"
$ProjectRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ProjectRoot

if ($Clean) {
    cargo clean
}

cargo build --release
if ($LASTEXITCODE -ne 0) {
    throw "Release build failed."
}

$OutputDirectory = Join-Path $ProjectRoot "release"
New-Item -ItemType Directory -Force -Path $OutputDirectory | Out-Null
Copy-Item -Force `
    (Join-Path $ProjectRoot "target\release\KRun.exe") `
    (Join-Path $OutputDirectory "KRun.exe")

$Executable = Get-Item (Join-Path $OutputDirectory "KRun.exe")
Write-Host "Created: $($Executable.FullName)"
Write-Host "Size: $([Math]::Round($Executable.Length / 1MB, 2)) MB"
