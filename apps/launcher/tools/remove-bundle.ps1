# Removes the launcher's client bundle (bundle/client.zip) from a game install.
# Files the bundle overwrote are restored from %LOCALAPPDATA%\DungeonborneLauncher\bundle_backup\<key>\;
# files it created are deleted. A file whose sha256 no longer matches the bundle was changed after it, so it is kept.
# Without -Apply nothing is touched: the script only prints what it would do.
#
#   powershell -ExecutionPolicy Bypass -File remove-bundle.ps1 -Root "C:\Steam\steamapps\common\Dungeonborne"
#   powershell -ExecutionPolicy Bypass -File remove-bundle.ps1 -Root "C:\Steam\steamapps\common\Dungeonborne" -Apply
param(
    [Parameter(Mandatory = $true)][string]$Root,
    [string]$Bundle = '',
    [switch]$Apply
)
$ErrorActionPreference = 'Stop'
if (-not $Bundle) { $Bundle = Join-Path (Split-Path -Parent $MyInvocation.MyCommand.Path) '..\bundle\client.zip' }
Add-Type -AssemblyName System.IO.Compression.FileSystem

$Root = $Root.TrimEnd('\', '/')
if (-not (Test-Path (Join-Path $Root 'Dungeonborne\Binaries\Win64\Dungeonborne.exe'))) { throw "not a game install: $Root" }
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) { throw 'close the game first' }

# Same key as profile::Install::key(): sha256 of the lower-cased root path, first 16 hex chars.
$sha = [Security.Cryptography.SHA256]::Create()
$key = (-join ($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Root.ToLower())) | ForEach-Object { $_.ToString('x2') })).Substring(0, 16)
$backup = Join-Path $env:LOCALAPPDATA "DungeonborneLauncher\bundle_backup\$key"

$zip = [IO.Compression.ZipFile]::OpenRead((Resolve-Path $Bundle))
try {
    $reader = New-Object IO.StreamReader($zip.GetEntry('bundle.json').Open())
    $meta = $reader.ReadToEnd() | ConvertFrom-Json
    $reader.Close()
} finally { $zip.Dispose() }
if ($meta.schema -ne 'dblbundle/1') { throw "unknown bundle schema $($meta.schema)" }

# Part of the game itself: without a backup of the original there is nothing to put back, so never delete them.
$core = @('cacert.pem', 'steam_api64.dll', 'steam_appid.txt') | ForEach-Object { "Dungeonborne\Binaries\Win64\$_" }

$restored = 0; $deleted = 0; $kept = 0
$parents = New-Object 'System.Collections.Generic.HashSet[string]'
foreach ($f in $meta.files.PSObject.Properties) {
    $rel = $f.Name -replace '/', '\'
    $dst = Join-Path $Root $rel
    $bak = Join-Path $backup $rel
    [void]$parents.Add((Split-Path $dst -Parent))
    if (Test-Path $bak) {
        Write-Host "restore $rel"
        if ($Apply) { Copy-Item $bak $dst -Force }
        $restored++
    } elseif (Test-Path $dst) {
        if ($core -contains $rel) {
            Write-Host "keep    $rel (game file, no backup of the original)"
            $kept++
            continue
        }
        if ((Get-FileHash $dst -Algorithm SHA256).Hash.ToLower() -ne $f.Value) {
            Write-Host "keep    $rel (changed after the bundle)"
            $kept++
            continue
        }
        Write-Host "delete  $rel"
        if ($Apply) { Remove-Item $dst -Force }
        $deleted++
    }
}
foreach ($d in $meta.dirs) { [void]$parents.Add((Join-Path $Root ($d -replace '/', '\'))) }

# Empty folders left behind, deepest first, never the root itself.
if ($Apply) {
    foreach ($dir in ($parents | Sort-Object Length -Descending)) {
        $p = $dir
        while ($p.Length -gt $Root.Length -and (Test-Path $p) -and -not (Get-ChildItem $p -Force)) {
            Remove-Item $p -Force
            $p = Split-Path $p -Parent
        }
    }
    if (Test-Path $backup) { Remove-Item $backup -Recurse -Force }
}

$mode = if ($Apply) { 'done' } else { 'dry run (add -Apply to do it)' }
Write-Host "`nbundle $($meta.version): restore $restored, delete $deleted, keep $kept - $mode"
