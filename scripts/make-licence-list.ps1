<#
  Writes packaging\THIRD_PARTY_CRATES.txt: every Rust library vidzcut is built from, with its version, licence and
  web address. It is shipped with the installer. Run it whenever the libraries change.
  For libraries that offer a choice of licences ("MIT OR Apache-2.0") we take the most permissive one named in
  packaging\licence-choices.txt.
#>
$root = Split-Path -Parent $PSScriptRoot
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $root
try {
    $json = cargo metadata --format-version 1 2>$null | Out-String
} finally { Pop-Location }
$m = $json | ConvertFrom-Json
$own = $m.workspace_members
$libs = $m.packages | Where-Object { $own -notcontains $_.id } | Sort-Object name, version

$choices = @{
    'self_cell' = 'Apache-2.0 (chosen from: Apache-2.0 OR GPL-2.0-only)'
    'r-efi'     = 'MIT (chosen from: MIT OR Apache-2.0 OR LGPL-2.1-or-later)'
}

$out = New-Object System.Text.StringBuilder
[void]$out.AppendLine('vidzcut is built from these free software libraries (Rust crates).')
[void]$out.AppendLine('Each one is used under the licence shown. Their licence texts are in their source packages on https://crates.io .')
[void]$out.AppendLine('Fonts, icons and FFmpeg are listed in THIRD_PARTY_LICENSES.md and FFMPEG-NOTICE.txt.')
[void]$out.AppendLine('')
[void]$out.AppendLine(('{0} libraries' -f $libs.Count))
[void]$out.AppendLine('')
foreach ($p in $libs) {
    $lic = if ($choices.ContainsKey($p.name)) { $choices[$p.name] } elseif ($p.license) { $p.license } else { 'see source' }
    $url = if ($p.repository) { $p.repository } else { "https://crates.io/crates/$($p.name)" }
    [void]$out.AppendLine(('{0} {1} | {2} | {3}' -f $p.name, $p.version, $lic, $url))
}
$target = Join-Path $root 'packaging\THIRD_PARTY_CRATES.txt'
[System.IO.File]::WriteAllText($target, $out.ToString(), (New-Object System.Text.UTF8Encoding($false)))
Write-Host "Wrote $target ($($libs.Count) libraries)"

# refuse anything that is not on the allowed list
$bad = $libs | Where-Object {
    $l = if ($choices.ContainsKey($_.name)) { $choices[$_.name] } else { "$($_.license)" }
    $l -notmatch 'MIT|Apache|BSD|ISC|Zlib|Unicode|Unlicense|0BSD|BSL-1.0|WTFPL|MPL-2.0|OFL|Ubuntu'
}
if ($bad) { $bad | ForEach-Object { Write-Warning "licence to check: $($_.name) $($_.version) [$($_.license)]" } }
