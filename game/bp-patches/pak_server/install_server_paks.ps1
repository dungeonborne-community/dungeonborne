# Server BP paks (game server install only: Hetzner Robot / local Steam server, NOT clients).
#   zzzzzzz_ServerNet_P — IsDedicatedServer -> IsServer in 5 projectile BPs (DK E double hook etc., 03.11)
# An IoStore pak is three files (.pak + .utoc + .ucas), copy all of them. Only with the game closed.
#   powershell -ExecutionPolicy Bypass -File install_server_paks.ps1 -GameRoot C:\Dungeonborne
#   ... -Remove    remove the paks (restore the original BP behaviour)
param([Parameter(Mandatory = $true)][string]$GameRoot, [switch]$Remove)
$ErrorActionPreference = 'Stop'
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) { Write-Host 'The game is running — close it (quit in dbserver-control.txt) and retry.'; exit 1 }
$paks = Join-Path $GameRoot "Dungeonborne\Content\Paks"
if (-not (Test-Path $paks)) { Write-Host "no $paks"; exit 1 }
$names = Get-ChildItem $PSScriptRoot -Filter "*.utoc" | ForEach-Object { $_.BaseName }
foreach ($n in $names) {
    foreach ($ext in "pak", "utoc", "ucas") {
        $dst = Join-Path $paks "$n.$ext"
        if ($Remove) { Remove-Item $dst -ErrorAction SilentlyContinue; continue }
        Copy-Item (Join-Path $PSScriptRoot "$n.$ext") $dst -Force
    }
    Write-Host ($(if ($Remove) { "removed: " } else { "installed: " }) + "$n -> $paks")
}
