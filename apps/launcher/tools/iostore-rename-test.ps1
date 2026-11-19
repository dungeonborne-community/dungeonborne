# Step 1 of network mods: does UE load an IoStore mod (.pak + .ucas + .utoc) renamed the way the launcher names
# server paks and placed in a Paks subfolder? Test pak: ChatFix (shows the "World" tab in the lobby chat).
#   powershell -ExecutionPolicy Bypass -File iostore-rename-test.ps1           # move + rename
#   powershell -ExecutionPolicy Bypass -File iostore-rename-test.ps1 -Undo     # put the original back
param(
    [Parameter(Mandatory)][string]$Root,   # the game install (local.toml [games] client)
    [switch]$Undo
)
$ErrorActionPreference = 'Stop'
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) { throw 'close the game first' }

$paks = Join-Path $Root 'Dungeonborne\Content\Paks'
$test = Join-Path $paks '~dbl_test'
$orig = 'zzzzzzz_ChatFix_P'
$renamed = 'zzzzzzzz_070_00_ru-chatfix_P'   # profile.rs: zzzzzzzz_<priority:03>_<n:02>_<mod-id>_P
$exts = 'pak', 'ucas', 'utoc'

if ($Undo) {
    foreach ($e in $exts) { Move-Item (Join-Path $test "$renamed.$e") (Join-Path $paks "$orig.$e") }
    Remove-Item $test
    Write-Host "restored $orig.* in $paks"
} else {
    if (Test-Path $test) { throw "$test already exists" }
    New-Item -ItemType Directory $test | Out-Null
    foreach ($e in $exts) { Move-Item (Join-Path $paks "$orig.$e") (Join-Path $test "$renamed.$e") }
    Write-Host "moved to $test :"
    Get-ChildItem $test | ForEach-Object { Write-Host "  $($_.Name)  $($_.Length)" }
}
