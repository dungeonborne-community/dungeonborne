# Automated run without a human: headless server (Steam install) + client (C:\Games\Dungeonborne) -> logs in runs\<time>.
# Needs: the DbTools task (README_admin.md), a running backend on 8085/6601-6604 and the port lock (portlock.ps1).
# -Match needs the backend without the launcher lobby: $env:LAUNCHER_LOBBY = '0' before run_local.ps1 (with the lobby the
# backend rejects the match button: "choose an instance in the launcher", -1028).
#   powershell -ExecutionPolicy Bypass -File tools\stand\run.ps1 [-Hold 30] [-NoClient] [-FakeDs]
#   -Hold    how many seconds to hold the match after the client connects (probes: auto after 10 s + at the end)
#   -FakeDs  IsDedicatedServer = true in the server process (file dbserver-fakeds.txt)
#   -Probe / -ProbeServer  orchestrator probe (-dbprobe) in the client / server; logs — backups\probe\logs
#   -Match native|local  the client presses the match button instead of open (native — native match via the backend: room,
#            dsInitFinish, the client travels to the server itself; local — the normal LocalDev button). For native, for the duration
#            of the run RebuildProbe\match-mode.txt = native is set (after the server starts), at the end — restored.
#   -InitMap only|before  the server opens the map via UBattleDsUtility.InitGameMap (experiment, dbserver-initmap.txt)
#   -InitMap open  the server opens the map with the console command open <map>?listen instead of StartLocalBattle
#   -Dsa ip:port   server with -dsaip/-dsaport (DSA agent for the DS SDK; the backend listens for DSA-Probe on 9090)
param([int]$Hold = 30, [switch]$NoClient, [switch]$FakeDs, [switch]$Probe, [switch]$ProbeServer,
      [ValidateSet("", "native", "local", "localthen")][string]$Match = "",
      [ValidateSet("", "only", "before", "open")][string]$InitMap = "", [string]$Dsa = "")

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
# $AdminLog — DbTools task log, from paths.ps1
$Installs = @{ server = (Get-Game 'steam'); client = (Get-Game 'client') }
New-Item -ItemType Directory -Force $Run | Out-Null

function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }

function Send-Request([string]$Json) {
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Start-Sleep -Milliseconds 1500
    Get-Content $AdminLog -Tail 3 -Encoding UTF8 | ForEach-Object { Say "  dbtools: $_" }
}

function Wait-Phase([string]$Role, [string]$Pattern, [int]$TimeoutSec, [datetime]$Since) {
    $status = Join-Path $Run "$Role-status.json"
    $deadline = (Get-Date).AddSeconds($TimeoutSec); $last = ""
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $status) -and (Get-Item $status).LastWriteTime -gt $Since) {
            $s = Get-Content $status -Raw
            if ($s -ne $last) { $last = $s; Say "  $Role $s".Trim() }
            if ($s -match "`"phase`":`"($Pattern)`"") { return $true }
        }
        Start-Sleep -Seconds 2
    }
    return $false
}

function Send-Control([string]$Role, [string]$Word) { Set-Content -Path (Join-Path $Run "$Role-control.txt") -Value $Word -Encoding ASCII }

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Say "the game is already running — exit it first"; exit 1 }
if (-not (Get-NetTCPConnection -State Listen -LocalPort 6601 -ErrorAction SilentlyContinue)) { Say "backend is not listening on 6601"; exit 1 }

$stamp = Get-Date -Format "yyyyMMdd-HHmmss"
$out = Join-Path $Work "runs\$stamp"
New-Item -ItemType Directory -Force $out | Out-Null
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value $(if ($FakeDs) { "1" } else { "0" }) -Encoding ASCII
Set-Content (Join-Path $Run "dbclient-match.txt") -Value $Match -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value $InitMap -Encoding ASCII
# match-mode.txt is read by the match button (WBP_MatchMainButton.lua) on entering the lobby; the folder is shared with the user's game
$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeWas = if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { $null }
foreach ($r in "dbserver", "dbclient") { Set-Content (Join-Path $Run "$r-control.txt") -Value "" -Encoding ASCII }

$t0 = Get-Date
Say "server (fakeds=$([bool]$FakeDs))"
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"probe":' + $(if ($ProbeServer) { 'true' } else { 'false' }) + $(if ($Dsa) { ',"dsa":"' + $Dsa + '"' } else { '' }) + '}')
$ok = Wait-Phase "dbserver" "ready|ended" 180 $t0
if ($ok -and -not $NoClient) {
    # the server has already passed the lobby with its own button mode (LocalDev) — now the mode for the client
    if ($Match -eq "native") { Set-Content $ModeFile -Value "native" -Encoding ASCII; Say "match-mode.txt = native (until the end of the run)" }
    Send-Request '{"action":"killmutex"}'
    $t1 = Get-Date
    Say "client"
    Send-Request ('{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false,"probe":' + $(if ($Probe) { 'true' } else { 'false' }) + '}')
    $ok = Wait-Phase "dbclient" "connected|ended" 200 $t1
}
if ($ok) {
    Say "holding the match for $Hold s"
    Start-Sleep -Seconds $Hold
    Send-Control "dbserver" "probe"; if (-not $NoClient) { Send-Control "dbclient" "probe" }
    Start-Sleep -Seconds 4
}

if ($Match -eq "native") {
    if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
    Say "match-mode.txt restored"
}
Say "quit"
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue)) { Start-Sleep -Seconds 1 }
$left = @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue)
if ($left.Count) {
    # 31.10: the client hung while loading UE4SS mods (before ours) — nobody to accept quit. From an administrator console
    # taskkill removes it; without rights — Access denied, then the user has to kill it.
    Say "DID NOT EXIT: pid $($left.Id -join ', ') — taskkill"
    foreach ($p in $left) { taskkill /PID $p.Id /F 2>&1 | ForEach-Object { Say "  $_" } }
}

foreach ($r in "server", "client") {
    $w = Join-Path $Installs[$r] "Dungeonborne\Binaries\Win64"
    Copy-Item (Join-Path $w "crash_ue4ss.log") (Join-Path $out "$r-crash_ue4ss.log") -ErrorAction SilentlyContinue
    Copy-Item (Join-Path $w "ue4ss\UE4SS.log") (Join-Path $out "$r-UE4SS.log") -ErrorAction SilentlyContinue
}
Get-ChildItem $Run -File | Where-Object { $_.Name -match '\.(log|txt|json)$' -and $_.Name -notmatch 'control|fakeds' } |
    ForEach-Object { Copy-Item $_.FullName $out }
Get-ChildItem (Join-Path $Work "logs") -Filter "ue-*.log" -ErrorAction SilentlyContinue | ForEach-Object { Copy-Item $_.FullName $out }
$crashes = Get-ChildItem "$env:LOCALAPPDATA\Dungeonborne\Saved\Crashes" -Directory -ErrorAction SilentlyContinue | Where-Object { $_.LastWriteTime -gt $t0 }
foreach ($c in $crashes) { Copy-Item $c.FullName (Join-Path $out $c.Name) -Recurse; Say "CRASH: $($c.Name)" }
Say "logs: $out"
