<#
  The quality gate: format, lint (strict), tests. Stops at the first failure and exits non-zero.
  Run before every commit:   powershell -ExecutionPolicy Bypass -File scripts\gate.ps1   (add -Release to also build release)
#>
param([switch]$Release)

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
if (-not $env:LIBCLANG_PATH) { $env:LIBCLANG_PATH = 'C:\Program Files\LLVM\bin' }

function Run($name, [scriptblock]$cmd) {
    Write-Host "== $name" -ForegroundColor Yellow
    $out = & $cmd 2>&1 | Out-String
    if ($LASTEXITCODE -ne 0) {
        Write-Host $out
        Write-Host "GATE FAILED at: $name" -ForegroundColor Red
        exit 1
    }
}

Run 'cargo fmt'    { cargo fmt --all }
Run 'cargo clippy' { cargo clippy --workspace --all-targets -- -D warnings }
Run 'cargo test'   { cargo test --workspace }
if ($Release) { Run 'cargo build --release' { cargo build --release } }
Write-Host 'GATE PASSED' -ForegroundColor Green
exit 0
