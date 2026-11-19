# Builds zz_RelicQuality_P.pak next to the mod folder. Does not install it into game installs.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$stage = Join-Path $env:TEMP 'zz_RelicQuality_P_stage'
Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item -Recurse (Join-Path $PSScriptRoot 'Dungeonborne') $stage
& (Join-Path $root 'DungeonbornePVE\files\Paks\repak.exe') pack --version V8B --mount-point ../../../ --quiet $stage (Join-Path $root 'zz_RelicQuality_P.pak')
if ($LASTEXITCODE -ne 0) { throw 'repak' }
Remove-Item -Recurse -Force $stage
Get-Item (Join-Path $root 'zz_RelicQuality_P.pak') | ForEach-Object { "{0} {1}" -f $_.Length, $_.FullName }
