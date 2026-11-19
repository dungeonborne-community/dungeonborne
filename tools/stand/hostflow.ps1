# Waiting hall on the backend's assignment (03.11, docs/WAITING_ROOM.md): the backend with DS_WAITING_ROOM=1 returns in the /ds/slot response
# action=host -> the mod opens the hall, waits for the roster (members) -> entry countdown -> servertravel into the match. Rounds: client
# via native match -> hall -> match -> Hold s -> leave all -> result -> reset back to the hall. Needs: the DbTools task, backend.
#   powershell -ExecutionPolicy Bypass -File tools\stand\hostflow.ps1 [-Rounds 2] [-Hold 20]
param([int]$Rounds = 2, [int]$Hold = 20, [string]$WaitingMap = "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01")

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

if (Count-Game) { Say "the game is already running — exit it first"; exit 1 }
$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeWas = if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { $null }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
Set-Content (Join-Path $Run "dbclient-match.txt") -Value "native" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value "" -Encoding ASCII
Remove-Item (Join-Path $Run "dbserver-waiting.txt") -ErrorAction SilentlyContinue
foreach ($r in "dbserver", "dbclient") { Set-Content (Join-Path $Run "$r-control.txt") -Value "" -Encoding ASCII }
if (Test-Path $ServerLog) { Remove-Item $ServerLog }
$results = @()

$t0 = Get-Date
Say "server in the hall $WaitingMap"
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"map":"' + $WaitingMap + '"}')
if (-not (Wait-Phase "dbserver" "ready|ended" 240 $t0)) { Say "the server did not reach ready"; $Rounds = 0 }
else { Set-Content $ModeFile -Value "native" -Encoding ASCII }

for ($i = 1; $i -le $Rounds; $i++) {
    Say "=== round $i"
    $line0 = @(Get-Content $ServerLog -Encoding UTF8).Count
    Send-Request '{"action":"killmutex"}'
    $tc = Get-Date
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false}'
    $r = [ordered]@{ round = $i }
    $r.host = [bool](Wait-Log "\[HOST\] задание" $line0 240)
    $r.waiting = [bool](Wait-Log "\[HOST\] .*зал готов" $line0 120)
    $r.all_in = [bool](Wait-Log "\[HOST\] .*в зале 1 из 1" $line0 180)
    $r.battle = [bool](Wait-Log "\[HOST\] .*бой \(" $line0 120)
    if ($r.battle) {
        $r.client_secs = [int]((Get-Date) - $tc).TotalSeconds
        Start-Sleep -Seconds $Hold
        Send-Control "dbserver" "leave all"
        $r.reset_done = [bool](Wait-Log "\[SLOT\] сброс после комнаты .* завершён" $line0 240)
        $r.back_in_waiting = [bool](Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $line0 | Where-Object { $_ -match "обратно на карту простоя" })
    }
    Send-Control "dbclient" "quit"
    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline -and (Count-Game) -gt 1) { Start-Sleep -Seconds 1 }
    Start-Sleep -Seconds 5
    Say ("  result: " + ($r | ConvertTo-Json -Compress))
    $results += [pscustomobject]$r
    if (-not $r.reset_done) { break }
}

if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
Say "quit"
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
if (Count-Game) { Say "DID NOT EXIT: pid $((Get-Process -Name Dungeonborne).Id -join ', ')" }
$out = Join-Path $Work ("runs\host-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $Run -File | Where-Object { $_.Name -match '^(dbserver|dbclient)(\.log|-status\.json)$' } | ForEach-Object { Copy-Item $_.FullName $out }
Copy-Item (Join-Path (Get-Game 'steam') "Dungeonborne\Binaries\Win64\crash_ue4ss.log") (Join-Path $out "server-ruclient.log") -ErrorAction SilentlyContinue
Copy-Item (Join-Path $Work "logs\ue-server-7777.log") $out -ErrorAction SilentlyContinue
$results | Format-Table -AutoSize | Out-String | Write-Host
Say "logs: $out"
