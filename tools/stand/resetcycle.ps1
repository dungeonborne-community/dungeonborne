# Cycle "match -> room closed -> server reset -> next match on the same server" (03.11, M3).
# One server (Steam install) for the whole run; each round — client C:\Games\Dungeonborne via native match
# (match-mode native), Hold s of the match, then "exit" on behalf of the player (leave all) -> result -> room_closed -> server reset.
# Checks: the server reached ready with a new epoch, the slot in the backend is free again, the same player in the second round is not
# kicked by JoinGuard. Needs: the DbTools task, backend on 6601/9091.
#   powershell -ExecutionPolicy Bypass -File tools\stand\resetcycle.ps1 [-Rounds 2] [-Hold 20]
param([int]$Rounds = 2, [int]$Hold = 20)

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
# $AdminLog — DbTools task log, from paths.ps1
$Slots = "http://127.0.0.1:9091/ds/slots"
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

function Slot() { try { (Invoke-RestMethod $Slots -TimeoutSec 3).slots[0] } catch { $null } }

function Wait-Log([string]$Pattern, [int]$FromLine, [int]$TimeoutSec) {
    $deadline = (Get-Date).AddSeconds($TimeoutSec)
    while ((Get-Date) -lt $deadline) {
        $hit = Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $FromLine | Where-Object { $_ -match $Pattern } | Select-Object -First 1
        if ($hit) { return $hit }
        Start-Sleep -Seconds 2
    }
    return $null
}

function Count-Game() { @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue).Count }

if (Count-Game) { Say "the game is already running — exit it first"; exit 1 }
if (-not (Slot)) { Say "backend does not respond on $Slots"; exit 1 }

$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeWas = if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { $null }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
Set-Content (Join-Path $Run "dbclient-match.txt") -Value "native" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value "" -Encoding ASCII
foreach ($r in "dbserver", "dbclient") { Set-Content (Join-Path $Run "$r-control.txt") -Value "" -Encoding ASCII }
if (Test-Path $ServerLog) { Remove-Item $ServerLog }

$results = @()
$t0 = Get-Date
Say "server"
Send-Request '{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false}'
if (-not (Wait-Phase "dbserver" "ready|ended" 180 $t0)) { Say "the server did not reach ready"; $Rounds = 0 }
else {
    Set-Content $ModeFile -Value "native" -Encoding ASCII
    Start-Sleep -Seconds 12
    $s = Slot; Say "  slot: state=$($s.state) phase=$($s.phase) epoch=$($s.epoch)"
}

for ($i = 1; $i -le $Rounds; $i++) {
    Say "=== round $i"
    $line0 = @(Get-Content $ServerLog -Encoding UTF8).Count
    Send-Request '{"action":"killmutex"}'
    $t1 = Get-Date
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false}'
    $conn = Wait-Phase "dbclient" "connected|ended" 240 $t1
    $kick = $null
    if ($conn) {
        Say "  holding the match for $Hold s"
        Start-Sleep -Seconds $Hold
        $kick = Get-Content $ServerLog -Encoding UTF8 | Select-Object -Skip $line0 | Where-Object { $_ -match "\[JOINGUARD\].*перевод в лобби" } | Select-Object -First 1
        Send-Control "dbserver" "leave all"
        Say "  leave all"
    }
    $closed = if ($conn) { Wait-Log "\[SLOT\] сброс после комнаты" $line0 90 } else { $null }
    $done = if ($closed) { Wait-Log "\[SLOT\] сброс после комнаты .* завершён" $line0 240 } else { $null }
    Start-Sleep -Seconds 12
    $s = Slot
    Send-Control "dbclient" "quit"
    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline -and (Count-Game) -gt 1) { Start-Sleep -Seconds 1 }
    $r = [pscustomobject]@{ round = $i; client_connected = $conn; joinguard_kick = [bool]$kick; reset_started = [bool]$closed
        reset_done = [bool]$done; slot_state = $s.state; slot_phase = $s.phase; slot_epoch = $s.epoch }
    $results += $r
    Say ("  result: " + ($r | ConvertTo-Json -Compress))
    if ($kick) { Say "  JOINGUARD: $kick" }
    if (-not $done) { break }
}

if ($null -eq $ModeWas) { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $ModeWas -NoNewline -Encoding ASCII }
Say "quit"
Send-Control "dbclient" "quit"; Send-Control "dbserver" "quit"
$deadline = (Get-Date).AddSeconds(45)
while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
if (Count-Game) { Say "DID NOT EXIT: pid $((Get-Process -Name Dungeonborne).Id -join ', ')" }
$out = Join-Path $Work ("runs\reset-" + (Get-Date -Format "yyyyMMdd-HHmmss"))
New-Item -ItemType Directory -Force $out | Out-Null
Get-ChildItem $Run -File | Where-Object { $_.Name -match '\.(log|json)$' } | ForEach-Object { Copy-Item $_.FullName $out }
$results | Format-Table -AutoSize | Out-String | Write-Host
Say "logs: $out"
