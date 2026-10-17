# Builds netwatch in release mode, copies it to %LOCALAPPDATA%\Programs\netwatch,
# registers it to start at login and starts it.
#
#   .\install.ps1             build and install
#   .\install.ps1 -Uninstall  stop it, unregister it and remove the copy

param([switch]$Uninstall)

$ErrorActionPreference = 'Stop'
$exe = Join-Path $PSScriptRoot 'target\release\netwatch.exe'

if (-not $Uninstall) {
    # A netwatch started straight from the build directory locks the binary
    # cargo is about to replace.
    Get-Process netwatch -ErrorAction SilentlyContinue |
        Where-Object Path -eq $exe |
        Stop-Process -Force
    Start-Sleep -Milliseconds 500
    cargo build --release --manifest-path (Join-Path $PSScriptRoot 'Cargo.toml')
    if ($LASTEXITCODE) { exit $LASTEXITCODE }
} elseif (-not (Test-Path $exe)) {
    Write-Error "$exe not found; build it first with: cargo build --release"
}

# netwatch is a GUI-subsystem app, so PowerShell only waits for it and shows its
# output when that output is piped.
& $exe ($Uninstall ? 'uninstall' : 'install') | Out-Host
exit $LASTEXITCODE
