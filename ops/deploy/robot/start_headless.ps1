# Start/stop headless game servers (game client without a GUI as a listen server + the DungeonborneRuServer mod).
# Settings come from server.json (written by install_gameserver.ps1). Run as administrator (removing the game's mutexes).
#   powershell -ExecutionPolicy Bypass -File start_headless.ps1                 one server, port 7777
#   ... -Count 4                    four servers: 7777, 7787, 7797, 7807 (one at a time, 45 s pause)
#   ... -Port 7787                  one server on the given port
#   ... -Map /TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01            idle map (default: the mod's)
#   ... -Window                     with a window (no -nullrhi), for debugging
#   ... -Standby                    no map, waits in the lobby for an assignment, exits after the match (how ds-agent starts it)
#   ... -Status                     phases of the running servers
#   ... -Quit [-Port N | -All]      ask the server(s) to exit: taskkill cannot kill a process under ACE
# Parameters go as command-line flags: Dungeonborne.exe relaunches itself under ACE with a clean environment.
param(
    [int]$Port = 7777,
    [ValidateRange(1, 8)][int]$Count = 1,
    [string]$Map = "",
    [int]$Delay = 5,
    [switch]$Window,
    [switch]$Standby,
    [switch]$Status,
    [switch]$Quit,
    [switch]$All
)

$kit = $PSScriptRoot
$run = Join-Path $kit "run"
$logs = Join-Path $kit "logs"
$cfgPath = Join-Path $kit "server.json"
if (-not (Test-Path $cfgPath)) { Write-Host "No server.json - run install_gameserver.ps1 first" -ForegroundColor Red; exit 1 }
$cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json

# Mod files: dbserver-*.txt/json for 7777, dbserver-<port>-* for other ports
function Inst([int]$P) { if ($P -eq 7777) { "dbserver" } else { "dbserver-$P" } }

if ($Status -or $Quit) {
    $ports = if ($All -or $Status) {
        Get-ChildItem $run -Filter "dbserver*-status.json" -ErrorAction SilentlyContinue | ForEach-Object {
            if ($_.Name -match '^dbserver-(\d+)-status') { [int]$Matches[1] } else { 7777 } }
    } else { @($Port) }
    foreach ($p in $ports) {
        if ($Quit) {
            Set-Content (Join-Path $run "$(Inst $p)-control.txt") -Value "quit" -Encoding ASCII
            Write-Host "quit -> port $p"
        } else {
            $s = Join-Path $run "$(Inst $p)-status.json"
            Write-Host ("{0}: {1}" -f $p, (Get-Content $s -Raw -ErrorAction SilentlyContinue))
        }
    }
    if ($Status) { Write-Host "Dungeonborne processes: $(@(Get-Process Dungeonborne -ErrorAction SilentlyContinue).Count)" }
    exit 0
}

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Write-Host "Run as administrator" -ForegroundColor Red; exit 1 }
if ($Map -and $Map -notmatch '^/TgfMaps/[A-Za-z0-9_/]+$') { Write-Host "Map: /TgfMaps/..." -ForegroundColor Red; exit 1 }

$win64 = Join-Path $cfg.GameRoot "Dungeonborne\Binaries\Win64"
$exe = Join-Path $win64 "Dungeonborne.exe"
if (-not (Test-Path $exe)) { Write-Host "Missing $exe" -ForegroundColor Red; exit 1 }
New-Item -ItemType Directory -Force $run, $logs | Out-Null

# Pending mod updates (*.new next to the file: the dll is locked while the game runs) - install them while no game is running
if (-not (Get-Process Dungeonborne -ErrorAction SilentlyContinue)) {
    Get-ChildItem (Join-Path $win64 "ue4ss\Mods") -Recurse -Filter *.new -File -ErrorAction SilentlyContinue | ForEach-Object {
        $dst = $_.FullName.Substring(0, $_.FullName.Length - 4)
        Move-Item $_.FullName $dst -Force
        Write-Host "updated: $dst" -ForegroundColor Cyan
    }
}

$ports = if ($Count -gt 1) { 0..($Count - 1) | ForEach-Object { 7777 + 10 * $_ } } else { @($Port) }
$first = $true
foreach ($p in $ports) {
    # Port held by a previous instance (hung, did not exit): the new one would bind the next port while players went to the old one
    # (03.11: 7777 held by hung pid 6228, the new one on 7778 - the waiting room waited for 0 of 4)
    $busy = Get-NetUDPEndpoint -LocalPort $p -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($busy) {
        Write-Host "port $p is held by process $($busy.OwningProcess) - first run start_headless.ps1 -Quit -Port $p (or close it)" -ForegroundColor Red
        continue
    }
    # A second game instance does not start while running ones hold the single-instance mutexes - remove them
    if (Get-Process Dungeonborne -ErrorAction SilentlyContinue) {
        if (-not $first) { Start-Sleep -Seconds 45 }  # give it time to create its own mutexes before the next removal (02.11)
        & (Join-Path $kit "killmutex.ps1") | Out-Null
        Get-Content (Join-Path $kit "killmutex.log") -Tail 3 -ErrorAction SilentlyContinue
    }
    $first = $false
    # An old quit not read by the previous instance would otherwise stop the new one right after start
    Remove-Item (Join-Path $run "$(Inst $p)-control.txt") -ErrorAction SilentlyContinue
    $ueLog = Join-Path $logs "ue-$p.log"
    if (Test-Path $ueLog) {
        Move-Item $ueLog (Join-Path $logs ("ue-$p-" + (Get-Item $ueLog).LastWriteTime.ToString("yyyyMMdd-HHmmss") + ".log")) -Force
    }
    # UE4SS.log is one file for all ports (the name is hardcoded in UE4SS.dll) and is overwritten on every start - copy it before start,
    # otherwise crash traces are lost (04.11 01:24 crash of 7777 in UE4SS: the log was overwritten by the next start). Keep 40.
    $u4 = Join-Path $win64 "ue4ss\UE4SS.log"
    if (Test-Path $u4) {
        Copy-Item $u4 (Join-Path $logs ("ue4ss-" + (Get-Date).ToString("yyyyMMdd-HHmmss") + "-before-$p.log")) -ErrorAction SilentlyContinue
        Get-ChildItem (Join-Path $logs "ue4ss-*.log") | Sort-Object LastWriteTime -Descending | Select-Object -Skip 40 | Remove-Item -ErrorAction SilentlyContinue
    }
    $gameArgs = @("-dbserver", "-ABSLOG=`"$ueLog`"", "-log", "-unattended", "-nosplash", "-dbdelay=$Delay",
        "-dbroot=$($run -replace '\\', '/')", "-dbslot=$($cfg.PublicIp):$p")
    # with -port=7777 the gateway login failed (30.10) - pass the port only when it is not the default
    if ($p -ne 7777) { $gameArgs += "-port=$p" }
    if ($Map) { $gameArgs += "-dbmap=$Map" }
    # one process per match (ds-agent): wait for an assignment in the lobby without a map, exit after the match
    if ($Standby) { $gameArgs += "-dbstandby=1" }
    if (-not $Window) { $gameArgs += @("-nullrhi", "-nosound") }
    $proc = Start-Process -FilePath $exe -WorkingDirectory $win64 -ArgumentList $gameArgs -PassThru
    Write-Host "port $p started (pid $($proc.Id), it relaunches under ACE with another pid)" -ForegroundColor Green
}
Write-Host "Status: start_headless.ps1 -Status (phase ready after ~40 s). Mod logs: $run\dbserver*.log, UE: $logs"
