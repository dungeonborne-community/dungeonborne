# Disconnect mid-match (03.11, docs/RECONNECT.md): what the server does with a disconnected player.
# Server in the hall (backend with DS_WAITING_ROOM=1 drives hall -> match itself), client via native match; in the match the client exits (quit),
# after -Gone s we look at PS/pawn/record ([GAMEOVER] PS ... смерть=, запись ...), then kill <open_id> (death
# of the disconnected player's pawn via the native death zone GE) -> after 5 s the mod itself exits on behalf of the dead player (ServerSelfQuitGame) ->
# what reason the record has and whether the result was sent. Needs: the DbTools task, backend.
#   powershell -ExecutionPolicy Bypass -File tools\stand\dctest.ps1 [-Gone 60] [-OpenId 76561198000000903]
#   -Mode deadfirst: killstay in the match (death without auto-exit), after 10 s the client quits — is the dead player's result taken on disconnect
param([int]$Gone = 60, [string]$OpenId = "76561198000000903", [ValidateSet("alive", "deadfirst")][string]$Mode = "alive",
      [string]$WaitingMap = "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01")

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
}
function Wait-Phase([string]$Role, [string]$Pattern, [int]$TimeoutSec, [datetime]$Since) {
    $status = Join-Path $Run "$Role-status.json"
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        if ((Test-Path $status) -and (Get-Item $status).LastWriteTime -gt $Since -and (Get-Content $status -Raw) -match "`"phase`":`"($Pattern)`"") { return $true }
        Start-Sleep -Seconds 2
    }
    return $false
}
function Send-Control([string]$Role, [string]$Word) { Set-Content -Path (Join-Path $Run "$Role-control.txt") -Value $Word -Encoding ASCII }
function Wait-Log([string]$Pattern, [int]$FromLine, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $hit = Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $FromLine | Where-Object { $_ -match $Pattern } | Select-Object -First 1
        if ($hit) { Say "  > $hit"; return $hit }
        Start-Sleep -Seconds 2
    }
    return $null
}
function Lines() { @(Get-Content $ServerLog -Encoding UTF8).Count }
function Count-Game() { @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue).Count }
function Show-Since([int]$From, [string]$Pattern) {
    Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $From | Where-Object { $_ -match $Pattern } | ForEach-Object { Say "  | $_" }
}

if (Count-Game) { Say "the game is already running — exit it first"; exit 1 }
$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeWas = if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { $null }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
Set-Content (Join-Path $Run "dbclient-match.txt") -Value "native" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value "" -Encoding ASCII
Remove-Item (Join-Path $Run "dbserver-waiting.txt") -ErrorAction SilentlyContinue
foreach ($r in "dbserver", "dbclient") { Set-Content (Join-Path $Run "$r-control.txt") -Value "" -Encoding ASCII }
if (Test-Path $ServerLog) { Remove-Item $ServerLog }

$t0 = Get-Date
Say "server in the hall"
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"map":"' + $WaitingMap + '"}')
if (Wait-Phase "dbserver" "ready|ended" 240 $t0) {
    Set-Content $ModeFile -Value "native" -Encoding ASCII
    Send-Request '{"action":"killmutex"}'
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false}'
    if (Wait-Log "\[HOST\] .*бой \(" 0 300) {
        Start-Sleep -Seconds 15
        $l0 = Lines
        if ($Mode -eq "deadfirst") {
            Say "killstay $OpenId (dies while connected, without auto-exit)"
            Send-Control "dbserver" "killstay $OpenId"
            Start-Sleep -Seconds 10
        }
        Say "the client exits (quit) mid-match"
        Send-Control "dbclient" "quit"
        $deadline = (Get-Date).AddSeconds(40)
        while ((Get-Date) -lt $deadline -and (Count-Game) -gt 1) { Start-Sleep -Seconds 1 }
        Say "waiting $Gone s without the player"
        Start-Sleep -Seconds $Gone
        Send-Control "dbserver" "probe"
        Start-Sleep -Seconds 4
        Show-Since $l0 "GAMEOVER\] (PS|запись|вызов)|JOINGUARD|RAIDRESULT|SLOT\] (сброс|heartbeat)"
        if ($Mode -eq "alive") {
            $l1 = Lines
            Say "kill $OpenId (the disconnected player's pawn)"
            Send-Control "dbserver" "kill $OpenId"
            Start-Sleep -Seconds 25
            Show-Since $l1 "GAMEOVER\] (PS|запись|вызов)|HOSTCMD|RAIDRESULT|SLOT\] сброс"
        }
    } else { Say "the match did not start" }
}

if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
Say "quit"
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
if (Count-Game) { Say "DID NOT EXIT: pid $((Get-Process -Name Dungeonborne).Id -join ', ')" }
$out = Join-Path $Work ("runs\dc-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $Run -File | Where-Object { $_.Name -match '^(dbserver|dbclient)(\.log|-status\.json|-probe\.txt)$' } | ForEach-Object { Copy-Item $_.FullName $out }
Copy-Item (Join-Path (Get-Game 'steam') "Dungeonborne\Binaries\Win64\crash_ue4ss.log") (Join-Path $out "server-ruclient.log") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $Work "logs\ue-server-7777.log") $out -ErrorAction SilentlyContinue
Say "logs: $out"
