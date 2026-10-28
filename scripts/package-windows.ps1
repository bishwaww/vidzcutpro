<#
  Builds the Windows installer:  dist\vidzcut_<version>_x64-setup.exe   (unsigned)

  Needs: the tools from setup-windows.ps1, and `cargo install cargo-packager --locked`.
  Run from the project folder:   powershell -ExecutionPolicy Bypass -File scripts\package-windows.ps1
#>
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$env:Path = "$root\third_party\ffmpeg\bin;$env:USERPROFILE\.cargo\bin;$env:Path"
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }
Set-Location $root

function Step($t) { Write-Host "`n== $t" -ForegroundColor Yellow }
function Run($cmd) { & cmd /c "$cmd 2>&1"; if ($LASTEXITCODE -ne 0) { throw "failed: $cmd" } }

Step 'Licence list'
& powershell -ExecutionPolicy Bypass -File "$root\scripts\make-licence-list.ps1"
Copy-Item "$root\THIRD_PARTY_LICENSES.md" "$root\packaging\THIRD_PARTY_LICENSES.md" -Force

Step 'Release build'
Run 'cargo build --release -p vidzcut'

Step 'Installer'
if (Test-Path "$root\dist") { Remove-Item "$root\dist" -Recurse -Force }
# The config is given to cargo-packager as text (reading it from a file fails on this Windows version).
$config = (Get-Content "$root\packaging\installer.json" -Raw | ConvertFrom-Json | ConvertTo-Json -Depth 10 -Compress)
$arg = '"' + $config.Replace('"', '\"') + '"'
$p = Start-Process -FilePath (Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe') -ArgumentList @('packager', '--release', '-c', $arg) -NoNewWindow -Wait -PassThru
if ($p.ExitCode -ne 0) { throw 'cargo packager failed' }

$exe = Get-ChildItem "$root\dist" -Filter '*setup.exe' | Select-Object -First 1
if (-not $exe) { throw 'no installer was made' }
Write-Host ("`nDONE: {0}  ({1:N1} MB, not signed)" -f $exe.FullName, ($exe.Length / 1MB)) -ForegroundColor Green
