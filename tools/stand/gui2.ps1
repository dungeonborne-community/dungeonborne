# Manual check: headless server (Steam install) + GUI client 1 ([games] client) via the DbTools task.
# The second GUI client ([games] client-e) is started by a human: tools\stand\client2.ps1 — after this script says "done".
# Does not end the match: to exit — quit in dbtools\run\dbserver-control.txt, dbclient-control.txt and run2\dbclient-control.txt
# (or close the client windows), then quit the server.
#   powershell -ExecutionPolicy Bypass -File tools\stand\gui2.ps1 [-ServerWindow]
#   -ServerWindow  server with a window (without -nullrhi -nosound) — checks whether missing rendering affects mob spawning
#   -NoObserver    the host stays a normal player (-dbobserver=0) — checks mob spawning for clients
#   -Map /TgfMaps/...  server map (default: Classic crypt, see DungeonborneRuServer DEFAULT_MAP)
param([switch]$ServerWindow, [switch]$NoObserver, [string]$Map = "")
. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
# $AdminLog — DbTools task log, from paths.ps1

function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }

function Send-Request([string]$Json) {
    # 02.11: the task was still processing the previous request (killmutex) — Start-ScheduledTask went nowhere, the client did not start.
    # Wait until the previous request is picked up, and restart the task until the new one is picked up.
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) {
        if ((Get-ScheduledTask -TaskName DbTools).State -ne 'Running') { Start-ScheduledTask -TaskName DbTools }
        Start-Sleep -Milliseconds 500
    }
    if (Test-Path $Req) { Say "  dbtools: request not picked up within 40 s: $Json" }
    Start-Sleep -Milliseconds 1500
    Get-Content $AdminLog -Tail 2 -Encoding UTF8 | ForEach-Object { Say "  dbtools: $_" }
}

function Wait-Phase([string]$Dir, [string]$Role, [string]$Pattern, [int]$TimeoutSec, [datetime]$Since) {
    $status = Join-Path $Dir "$Role-status.json"
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

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Say "the game is already running — exit it first"; exit 1 }
if (-not (Get-NetTCPConnection -State Listen -LocalPort 6601 -ErrorAction SilentlyContinue)) { Say "backend is not listening on 6601"; exit 1 }
foreach ($f in "$Run\dbserver-control.txt", "$Run\dbclient-control.txt", "$Work\run2\dbclient-control.txt") { Set-Content $f -Value "" -NoNewline }
Set-Content (Join-Path $Run "dbclient-match.txt") -Value "" -Encoding ASCII

$t0 = Get-Date
Say "server (window: $([bool]$ServerWindow))"
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":' + $(if ($NoObserver) { 'false' } else { 'true' }) + ',"window":' + $(if ($ServerWindow) { 'true' } else { 'false' }) + $(if ($Map) { ',"map":"' + $Map + '"' } else { '' }) + ',"probe":false}')
if (-not (Wait-Phase $Run "dbserver" "ready" 180 $t0)) { Say "the server did not reach ready"; exit 1 }
Send-Request '{"action":"killmutex"}'
$t1 = Get-Date
Say "client 1 (F:, GUI)"
Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":true,"probe":false}'
if (-not (Wait-Phase $Run "dbclient" "connected" 240 $t1)) { Say "client 1 did not connect" }
Send-Request '{"action":"killmutex"}'
Say "done: start the second client — tools\stand\client2.ps1"
