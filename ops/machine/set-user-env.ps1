# Windows user variables -> DB_HOME (docs/WORKSPACE_PLAN.md, section 4). Run by a human: this is a system
# setting. Without -Apply it only shows what will change. Rollback: -Revert (restores the previous values from the rollback file).
#   powershell -ExecutionPolicy Bypass -File ops\machine\set-user-env.ps1            # view
#   powershell -ExecutionPolicy Bypass -File ops\machine\set-user-env.ps1 -Apply     # switch
#   powershell -ExecutionPolicy Bypass -File ops\machine\set-user-env.ps1 -Revert    # restore as it was
# After switching: close and reopen terminals (and the Claude app), check `rustup show` and `cargo --version`.
# The toolchains are copied into home beforehand (ws bootstrap / step 4); the old ~/.rustup and ~/.cargo are deleted only later,
# separately and after confirmation.
param([switch]$Apply, [switch]$Revert)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\..\tools\stand\paths.ps1"     # $DbHome from local.toml
$H = (Resolve-Path $DbHome).Path
$backup = Join-Path $H 'toolchains\user-env.before.json'

$want = [ordered]@{
    DB_HOME     = $H
    RUSTUP_HOME = Join-Path $H 'toolchains\rustup'
    CARGO_HOME  = Join-Path $H 'toolchains\cargo'
    SCCACHE_DIR = Join-Path $H 'cache\sccache'
    UV_CACHE_DIR = Join-Path $H 'cache\uv'
}
$oldBin = Join-Path $env:USERPROFILE '.cargo\bin'
$newBin = Join-Path $want.CARGO_HOME 'bin'

if ($Revert) {
    if (-not (Test-Path $backup)) { throw "no rollback file $backup" }
    $b = Get-Content $backup -Raw | ConvertFrom-Json
    foreach ($p in $b.PSObject.Properties) { [Environment]::SetEnvironmentVariable($p.Name, $p.Value, 'User'); "restored $($p.Name)" }
    return
}

foreach ($k in 'RUSTUP_HOME', 'CARGO_HOME') {
    if (-not (Test-Path $want[$k])) { throw "no $($want[$k]) — copy the toolchains into home first (step 4)" }
}
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
$newPath = (($path -split ';') | ForEach-Object { if ($_ -ieq $oldBin) { $newBin } else { $_ } }) -join ';'
if ($newPath -notmatch [regex]::Escape($newBin)) { $newPath = "$newBin;$newPath" }

"Will be (user variables):"
foreach ($k in $want.Keys) { "  {0,-13} {1}  (now: {2})" -f $k, $want[$k], [Environment]::GetEnvironmentVariable($k, 'User') }
"  Path: $oldBin -> $newBin"
if (-not $Apply) { "Nothing changed. To switch: -Apply"; return }

$before = [ordered]@{ Path = $path }
foreach ($k in $want.Keys) { $before[$k] = [Environment]::GetEnvironmentVariable($k, 'User') }
$before | ConvertTo-Json | Set-Content $backup -Encoding UTF8
foreach ($k in $want.Keys) { [Environment]::SetEnvironmentVariable($k, $want[$k], 'User') }
[Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
"Done. Rollback: -Revert (file $backup). Restart the terminals and the Claude app."
