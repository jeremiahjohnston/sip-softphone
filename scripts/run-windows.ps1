# Builds and starts the app (debug build). Run build-pjsip-windows.ps1 once first.
$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent $PSScriptRoot
$env:PJSIP_DIR = Join-Path $Root "deps\pjproject"
Set-Location $Root
cargo run -p sip-softphone @args
