<#
  vidzcut: one-time setup for Windows.

  Installs (only what is missing): Rust, Microsoft C++ Build Tools, LLVM, and a pre-built
  LGPL FFmpeg 9.0 (shared DLLs + headers) into third_party\ffmpeg.
  Every download asks first. Use  -Yes  to answer "yes" to everything.

  Run from the project folder:   powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
#>
param([switch]$Yes)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$ffmpegDir = Join-Path $root 'third_party\ffmpeg'
$ffmpegUrl = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-n9.0-latest-win64-lgpl-shared-9.0.zip'

function Ask($question) {
    if ($Yes) { return $true }
    $a = Read-Host "$question [y/N]"
    return $a -match '^(y|yes)$'
}
function Step($text) { Write-Host "`n== $text" -ForegroundColor Yellow }
function Ok($text) { Write-Host "   OK  $text" -ForegroundColor Green }

# ---- Rust -------------------------------------------------------------------------------
Step 'Rust'
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Ok (cargo --version)
} elseif (Ask 'Rust is missing. Install it with winget (about 500 MB)?') {
    winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements --silent
    rustup default stable-x86_64-pc-windows-msvc
    Ok (cargo --version)
} else { throw 'Rust is required.' }

# ---- C++ build tools ----------------------------------------------------------------------
Step 'Microsoft C++ Build Tools'
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasVc = (Test-Path $vswhere) -and (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if ($hasVc) {
    Ok "found at $hasVc"
} elseif (Ask 'C++ Build Tools are missing. Install them with winget (3-6 GB, needs administrator approval)?') {
    winget install --id Microsoft.VisualStudio.2022.BuildTools -e --accept-package-agreements --accept-source-agreements --silent `
        --override '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended'
    Ok 'installed'
} else { throw 'The C++ Build Tools are required.' }

# ---- LLVM ---------------------------------------------------------------------------------
Step 'LLVM (clang, needed to read FFmpeg headers)'
$llvmBin = 'C:\Program Files\LLVM\bin'
if (Test-Path "$llvmBin\libclang.dll") {
    Ok "found at $llvmBin"
} elseif (Ask 'LLVM is missing. Install it with winget?') {
    winget install --id LLVM.LLVM -e --accept-package-agreements --accept-source-agreements --silent
    Ok 'installed'
} else { throw 'LLVM is required.' }
if (-not $env:LIBCLANG_PATH) {
    [Environment]::SetEnvironmentVariable('LIBCLANG_PATH', $llvmBin, 'User')
    $env:LIBCLANG_PATH = $llvmBin
    Ok "set LIBCLANG_PATH=$llvmBin for your user"
}

# ---- FFmpeg (LGPL, shared) ----------------------------------------------------------------
Step 'FFmpeg 9.0 (LGPL, shared)'
if (-not (Test-Path "$ffmpegDir\bin\ffmpeg.exe")) {
    if (Ask "FFmpeg is missing. Download about 74 MB from github.com/BtbN/FFmpeg-Builds into $ffmpegDir ?") {
        New-Item -ItemType Directory -Force (Split-Path $ffmpegDir) | Out-Null
        $zip = Join-Path (Split-Path $ffmpegDir) 'ffmpeg.zip'
        $ProgressPreference = 'SilentlyContinue'
        Invoke-WebRequest -Uri $ffmpegUrl -OutFile $zip -UseBasicParsing
        Expand-Archive $zip -DestinationPath (Split-Path $ffmpegDir) -Force
        Rename-Item (Join-Path (Split-Path $ffmpegDir) 'ffmpeg-n9.0-latest-win64-lgpl-shared-9.0') $ffmpegDir
        Remove-Item $zip
    } else { throw 'FFmpeg is required.' }
}
$info = (& "$ffmpegDir\bin\ffmpeg.exe" -version 2>&1 | Out-String)
if ($info -match '--enable-gpl') { throw 'This FFmpeg build is GPL. vidzcut must use an LGPL build (see docs/SPEC.md section 0).' }
Ok ($info -split "`n")[0].Trim()
Ok 'LGPL build confirmed (no --enable-gpl)'

# ---- Check it all works -------------------------------------------------------------------
Step 'Build check'
Push-Location $root
try { cargo test -p vz-media 2>&1 | Select-Object -Last 3 | ForEach-Object { Write-Host "   $_" } } finally { Pop-Location }

Write-Host "`nAll set. Start vidzcut with:  .\run-vidzcut.bat" -ForegroundColor Green
