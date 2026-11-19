# Installs the game server kit on Hetzner Robot (Windows Server, bare metal). Run as administrator
# with the game closed. The backend is on Hetzner Cloud (`core`), connected over the private network (vSwitch).
#   powershell -ExecutionPolicy Bypass -File install_gameserver.ps1 -GameRoot C:\Dungeonborne -BackendIp 10.0.0.2 -PublicIp 203.0.113.10
#   -GameRoot   game root (contains Dungeonborne\Binaries\Win64\Dungeonborne.exe)
#   -BackendIp  private address of `core` (vSwitch): game login domains, server mod HTTP (9091)
#   -PublicIp   Robot's public address: players get it (backend DS_POOL) and the mod sends it in /ds/slot (-dbslot)
#   -Instances  how many servers will run on the machine (ports 7777, 7787, … step 10), for the firewall
# Does: mod paks (all .pak/.utoc/.ucas), UE4SS + mods, Goldberg steam_api64, cacert.pem (originals kept as *.orig),
# native_data.txt (backend address), hosts block, UDP firewall rule, server.json for start_headless.ps1.
param(
    [string]$GameRoot = "C:\Dungeonborne",
    [Parameter(Mandatory = $true)][string]$BackendIp,
    [Parameter(Mandatory = $true)][string]$PublicIp,
    [int]$DsHttpPort = 9091,
    [ValidateRange(1, 8)][int]$Instances = 1
)

$ErrorActionPreference = "Stop"
$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Host "Run as administrator" -ForegroundColor Red; exit 1 }
if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) {
    Write-Host "The game is running - stop the servers (start_headless.ps1 -Quit -All) and try again" -ForegroundColor Red; exit 1
}
foreach ($ip in $BackendIp, $PublicIp) {
    if ($ip -notmatch '^\d{1,3}(\.\d{1,3}){3}$') { Write-Host "Not IPv4: $ip" -ForegroundColor Red; exit 1 }
}

$kit = $PSScriptRoot
$win64 = Join-Path $GameRoot "Dungeonborne\Binaries\Win64"
$paks = Join-Path $GameRoot "Dungeonborne\Content\Paks"
if (-not (Test-Path (Join-Path $win64 "Dungeonborne.exe"))) { Write-Host "Missing $win64\Dungeonborne.exe" -ForegroundColor Red; exit 1 }

# 1. Mod paks. The PVE paks zz_Solo*_9_P are incompatible: move them out of Paks (UE also mounts subfolders of Paks).
$off = Join-Path $GameRoot "Dungeonborne\Content\_pve_disabled_paks"
Get-ChildItem $paks -Filter "zz_Solo*_9_P.*" -ErrorAction SilentlyContinue | ForEach-Object {
    New-Item -ItemType Directory -Force $off | Out-Null
    Move-Item $_.FullName (Join-Path $off $_.Name) -Force
    Write-Host "PVE pak removed: $($_.Name)"
}
# IoStore paks are three files (.pak + .utoc + .ucas); copy them all
Copy-Item (Join-Path $kit "Paks\*") $paks -Force
Write-Host "Paks ($((Get-ChildItem (Join-Path $kit 'Paks')).Count) files) -> $paks" -ForegroundColor Green

# 2. Win64: keep the original steam_api64.dll and cacert.pem once
foreach ($name in "steam_api64.dll", "cacert.pem") {
    $dst = Join-Path $win64 $name
    if ((Test-Path $dst) -and -not (Test-Path "$dst.orig")) { Copy-Item $dst "$dst.orig" }
}
Copy-Item (Join-Path $kit "Win64\*") $win64 -Recurse -Force
Write-Host "UE4SS, mods, steam_api64 (Goldberg), cacert.pem -> $win64" -ForegroundColor Green

# 3. Backend address for the mod: line 1 is the player data URL (the server mod also uses it for /ds/slot, /ds/kill, results),
#    line 2 is realkey (results are written under the real OpenId)
$nd = Join-Path $win64 "ue4ss\Mods\DungeonborneRuClient\native_data.txt"
[IO.File]::WriteAllText($nd, "http://${BackendIp}:$DsHttpPort/ds/battle_user_data?open_id=`r`nrealkey`r`n")
Write-Host "native_data.txt -> http://${BackendIp}:$DsHttpPort" -ForegroundColor Green

# 4. hosts: authorization (INTL) and the game's dir/gateway -> backend
$hostsPath = Join-Path $env:SystemRoot "System32\drivers\etc\hosts"
$domains = @("sg.intlgame.com", "intlgame.com", "openapi.smartlink.intlgame.com", "tdm.deltaverse-intl.com",
    "f.tgf123.com", "sg.api.tgf123.com", "tgf123.com", "t01.tgf123.com",
    "dir.228918463-2-1.dmpplat.com", "tconnd-7101-702.dmpplat.com")
$begin = "# Dungeonborne server begin"
$end = "# Dungeonborne server end"
$text = [IO.File]::ReadAllText($hostsPath)
$text = [regex]::Replace($text, "(?s)\r?\n?" + [regex]::Escape($begin) + ".*?" + [regex]::Escape($end) + "\r?\n?", "`r`n").TrimEnd()
$block = @($begin) + ($domains | ForEach-Object { "$BackendIp $_" }) + @($end)
if (-not (Test-Path "$hostsPath.dungeonborne-backup")) { Copy-Item $hostsPath "$hostsPath.dungeonborne-backup" }
[IO.File]::WriteAllText($hostsPath, $text + "`r`n" + ($block -join "`r`n") + "`r`n")
Write-Host "hosts: $($domains.Count) domains -> $BackendIp" -ForegroundColor Green

# 5. Windows firewall: UDP for the game servers (7777 + 10*i)
$last = 7777 + 10 * ($Instances - 1)
$rule = "Dungeonborne gameserver UDP"
Get-NetFirewallRule -DisplayName $rule -ErrorAction SilentlyContinue | Remove-NetFirewallRule
New-NetFirewallRule -DisplayName $rule -Direction Inbound -Action Allow -Protocol UDP -LocalPort "7777-$last" | Out-Null
Write-Host "Firewall: UDP 7777-$last" -ForegroundColor Green

# 6. Server mod folder (-dbroot): watcher steps, status, control, mod logs
$run = Join-Path $kit "run"
New-Item -ItemType Directory -Force $run, (Join-Path $kit "logs") | Out-Null
@{ GameRoot = $GameRoot; BackendIp = $BackendIp; PublicIp = $PublicIp; DsHttpPort = $DsHttpPort } |
    ConvertTo-Json | Set-Content (Join-Path $kit "server.json") -Encoding UTF8
Write-Host "Done. Connectivity check: Test-NetConnection $BackendIp -Port $DsHttpPort ; start: start_headless.ps1" -ForegroundColor Green
