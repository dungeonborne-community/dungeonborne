# Waiting hall prototype (03.11, docs/WAITING_ROOM.md): the server opens the hall L_Dungeon_Matching_01 (?listen), the client
# arrives via a normal native match (backend without DS_WAITING_ROOM), the wr_ready command starts the native countdown
# of BP_TgfWaiting, on EnterGame the mod does servertravel into the match. Then leave all -> result -> reset (like resetcycle).
# Needs: the DbTools task, backend on 6601/9091.
#   powershell -ExecutionPolicy Bypass -File tools\stand\wrproto.ps1 [-Wait 15] [-Hold 20]
param([int]$Wait = 15, [int]$Hold = 20,
      [string]$WaitingMap = "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01",
      [string]$BattleMap = "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic")

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
# $AdminLog — DbTools task log, from paths.ps1
$ServerLog = Join-Path $Run "dbserver.log"

function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }
function Send-Request([string]$Json) {
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Start-Sleep -Milliseconds 1500
    Get-Content $AdminLog -Tail 2 -Encoding UTF8 | ForEach-Object { Say "  dbtools: $_" }
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
function Wait-Log([string]$Pattern, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $hit = Get-Content $ServerLog -Encoding UTF8 | Where-Object { $_ -match $Pattern } | Select-Object -First 1
        if ($hit) { return $hit }
        Start-Sleep -Seconds 2
    }
    return $null
}
function Count-Game() { @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue).Count }

if (Count-Game) { Say "the game is already running — exit it first"; exit 1 }
$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeWas = if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { $null }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
Set-Content (Join-Path $Run "dbclient-match.txt") -Value "native" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value "" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-waiting.txt") -Value $BattleMap -Encoding ASCII
foreach ($r in "dbserver", "dbclient") { Set-Content (Join-Path $Run "$r-control.txt") -Value "" -Encoding ASCII }
if (Test-Path $ServerLog) { Remove-Item $ServerLog }
$res = [ordered]@{}

$t0 = Get-Date
Say "server in the hall $WaitingMap"
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"map":"' + $WaitingMap + '"}')
$res.server_ready = Wait-Phase "dbserver" "ready|ended" 240 $t0
if ($res.server_ready) {
    Set-Content $ModeFile -Value "native" -Encoding ASCII
    Send-Request '{"action":"killmutex"}'
    $t1 = Get-Date
    Say "client (native match)"
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false}'
    $res.client_in_waiting = Wait-Phase "dbclient" "connected|ended" 240 $t1
    if ($res.client_in_waiting) {
        Start-Sleep -Seconds 10
        Say "wr_ready $Wait"
        Send-Control "dbserver" "wr_ready $Wait"
        $res.enter_game = [bool](Wait-Log "\[WAITING\] EnterGame" ($Wait + 90))
        if (-not $res.enter_game) { Say "EnterGame did not arrive — wr_travel manually"; Send-Control "dbserver" "wr_travel" }
        $res.travel_started = [bool](Wait-Log "\[WAITING\] servertravel" 30)
        $res.travel_done = [bool](Wait-Log "\[WAITING\] переход .* за" 240)
        if ($res.travel_done) {
            Start-Sleep -Seconds 25
            Say "leave all"
            Send-Control "dbserver" "leave all"
            $res.result_room_closed = [bool](Wait-Log "\[SLOT\] сброс после комнаты" 60)
            $res.reset_done = [bool](Wait-Log "\[SLOT\] сброс после комнаты .* завершён" 240)
        }
    }
}

if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
Say "quit"
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
if (Count-Game) { Say "DID NOT EXIT: pid $((Get-Process -Name Dungeonborne).Id -join ', ')" }
$out = Join-Path $Work ("runs\wr-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $Run -File | Where-Object { $_.Name -match '^(dbserver|dbclient)[^-]*\.(log|json)$|^dbserver-status|^dbclient-status' } | ForEach-Object { Copy-Item $_.FullName $out }
Copy-Item (Join-Path (Get-Game 'steam') "Dungeonborne\Binaries\Win64\crash_ue4ss.log") (Join-Path $out "server-ruclient.log") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $Work "logs\ue-server-7777.log") $out -ErrorAction SilentlyContinue
Say ("result: " + ($res | ConvertTo-Json -Compress))
Say "logs: $out"
