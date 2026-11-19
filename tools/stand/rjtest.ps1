# Rejoining a match with the "В бой" (To battle) button (03.11, docs/RECONNECT.md): on login the backend does not send user_battle_room, and search
# for a player with an ongoing match returns them to their room. Server in the hall (backend DS_WAITING_ROOM=1) -> client via native
# match -> match -> client quit -> after -Gone s the client is started again with a -LobbyWait s pause in the lobby (it must not travel by itself)
# -> "В бой" (MatchService:StartMatch) -> on the server again: the same PC/PS (without a new InitPlayerGameData) -> kill -> result.
#   powershell -ExecutionPolicy Bypass -File tools\stand\rjtest.ps1 [-Gone 20] [-LobbyWait 40]
param([int]$Gone = 20, [int]$LobbyWait = 40, [string]$OpenId = "76561198000000903",
      [string]$WaitingMap = "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01")

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
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
function Lines() { @(Get-Content $ServerLog -Encoding UTF8).Count }
function Wait-Log([string]$Pattern, [int]$FromLine, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $hit = Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $FromLine | Where-Object { $_ -match $Pattern } | Select-Object -First 1
        if ($hit) { Say "  > $hit"; return $hit }
        Start-Sleep -Seconds 2
    }
    return $null
}
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
$res = [ordered]@{}
$t0 = Get-Date

Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"map":"' + $WaitingMap + '"}')
if (Wait-Phase "dbserver" "ready|ended" 240 $t0) {
    Set-Content $ModeFile -Value "native" -Encoding ASCII
    Send-Request '{"action":"killmutex"}'
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false}'
    $res.battle = [bool](Wait-Log "\[HOST\] .*бой \(" 0 300)
    if ($res.battle) {
        Start-Sleep -Seconds 15
        $pc = (Get-Content $ServerLog -Encoding UTF8 | Where-Object { $_ -match "\[JOINGUARD\] соединение .*$OpenId.*PC (\S+)," } | Select-Object -Last 1)
        if ($pc -match "PC (\S+),") { $res.pc_before = $Matches[1] }
        Say "the client exits (quit)"
        Send-Control "dbclient" "quit"
        $deadline = (Get-Date).AddSeconds(40)
        while ((Get-Date) -lt $deadline -and (Count-Game) -gt 1) { Start-Sleep -Seconds 1 }
        Start-Sleep -Seconds $Gone
        $l1 = Lines
        $tc = Get-Date
        Say "client again (pause in the lobby $LobbyWait s)"
        Send-Request '{"action":"killmutex"}'
        Send-Request ('{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":' + $LobbyWait + ',"window":false}')
        $res.lobby = Wait-Phase "dbclient" "lobby|match" 180 $tc
        $tl = Get-Date
        # while the client stays in the lobby, it must not travel to the server
        while ((Get-Date) -lt $tl.AddSeconds($LobbyWait - 5)) { Start-Sleep -Seconds 2 }
        $res.self_travel = [bool](Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $l1 | Where-Object { $_ -match "\[JOINGUARD\] соединение .*$OpenId" })
        Say "  traveled to the server by itself from the lobby: $($res.self_travel)"
        $res.back = [bool](Wait-Log "\[JOINGUARD\] соединение .*$OpenId" $l1 180)
        $pc2 = (Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $l1 | Where-Object { $_ -match "\[JOINGUARD\] соединение .*$OpenId.*PC (\S+)," } | Select-Object -First 1)
        if ($pc2 -match "PC (\S+),") { $res.pc_after = $Matches[1] }
        $res.same_pc = $res.pc_before -and ($res.pc_before -eq $res.pc_after)
        Start-Sleep -Seconds 10
        Show-Since $l1 "JOINGUARD|GAMEOVER\] (PS|запись)"
        Send-Control "dbserver" "kill $OpenId"
        $res.result = [bool](Wait-Log "\[RAIDRESULT\] $OpenId" $l1 30)
        Start-Sleep -Seconds 25
    }
}

if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
if (Count-Game) { Say "DID NOT EXIT: pid $((Get-Process -Name Dungeonborne).Id -join ', ')" }
$out = Join-Path $Work ("runs\rj-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $Run -File | Where-Object { $_.Name -match '^(dbserver|dbclient)(\.log|-status\.json)$' } | ForEach-Object { Copy-Item $_.FullName $out }
Copy-Item (Join-Path (Get-Game 'steam') "Dungeonborne\Binaries\Win64\crash_ue4ss.log") (Join-Path $out "server-ruclient.log") -ErrorAction SilentlyContinue
Copy-Item (Join-Path (Get-Game 'client') "Dungeonborne\Binaries\Win64\crash_ue4ss.log") (Join-Path $out "client-ruclient.log") -ErrorAction SilentlyContinue
Say ("result: " + ($res | ConvertTo-Json -Compress))
Say "logs: $out"
