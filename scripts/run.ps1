<#
  Build (if needed) and start vidzcut.   powershell -ExecutionPolicy Bypass -File scripts\run.ps1 [-Debug]
#>
param([switch]$Debug)

$root = Split-Path -Parent $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
if (-not $env:LIBCLANG_PATH -and (Test-Path 'C:\Program Files\LLVM\bin\libclang.dll')) {
    $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin'
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue) -or -not (Test-Path "$root\third_party\ffmpeg\bin\ffmpeg.exe")) {
    Write-Host 'Tools are missing. Run scripts\setup-windows.ps1 first.' -ForegroundColor Red
    exit 1
}
Set-Location $root
if ($Debug) { cargo run -p vidzcut } else { cargo run --release -p vidzcut }
