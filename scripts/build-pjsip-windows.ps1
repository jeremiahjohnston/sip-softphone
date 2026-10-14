# Downloads and builds PJSIP for Windows (x64) into deps\pjproject.
# Needs: Git, and Visual Studio 2022 (or Build Tools) with "Desktop development with C++".
# Run from the repo root:  powershell -ExecutionPolicy Bypass -File scripts\build-pjsip-windows.ps1

$ErrorActionPreference = "Stop"
$Version = "2.15.1"
$Root = Split-Path -Parent $PSScriptRoot
$Deps = Join-Path $Root "deps"
$Src = Join-Path $Deps "pjproject"

New-Item -ItemType Directory -Force -Path $Deps | Out-Null
if (-not (Test-Path $Src)) {
    git clone --depth 1 --branch $Version https://github.com/pjsip/pjproject $Src
}

# Our build options. Video is off; TLS comes later.
@"
#define PJMEDIA_HAS_VIDEO 0
#define PJMEDIA_AUDIO_DEV_HAS_WMME 1
"@ | Set-Content -Encoding ascii (Join-Path $Src "pjlib\include\pj\config_site.h")

$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
if (-not (Test-Path $vswhere)) { throw "Visual Studio not found. Install VS 2022 Build Tools with the C++ workload." }
$msbuild = & $vswhere -latest -products * -requires Microsoft.Component.MSBuild -find "MSBuild\**\Bin\MSBuild.exe" | Select-Object -First 1
if (-not $msbuild) { throw "MSBuild not found. Install VS 2022 Build Tools with the C++ workload." }

# PlatformToolset follows whatever VS is installed; WindowsTargetPlatformVersion uses the newest SDK.
& $msbuild (Join-Path $Src "pjproject-vs14.sln") /t:libpjproject /m `
    /p:Configuration=Release-Dynamic /p:Platform=x64 `
    /p:PlatformToolset=v143 /p:WindowsTargetPlatformVersion=10.0
if ($LASTEXITCODE -ne 0) { throw "PJSIP build failed" }

Write-Host ""
Write-Host "PJSIP built. Now run:  scripts\run-windows.ps1"
