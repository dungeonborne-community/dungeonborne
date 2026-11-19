# L1 (login error 201 ext:-6): a series of automatic logins of client F: (DbTools, -dbclient), counting "Не удалось
# подключиться" (Could not connect) before the lobby. No game server needed — after the lobby the client exits right away (quit in dbclient-control.txt).
#   powershell -ExecutionPolicy Bypass -File tools\stand\loginseries.ps1 -Runs 6 -Tag both
# Needs: backend on 6601, the DbTools task, game windows closed. Result — probe\logs\loginseries_<tag>.log
param([int]$Runs = 6, [string]$Tag = "run")
. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"; $Req = Join-Path $Work "request.json"
$ClientLog = Join-Path $Run "dbclient.log"; $Control = Join-Path $Run "dbclient-control.txt"
$Out = Join-Path $PSScriptRoot "..\probe\logs\loginseries_$Tag.log"

function Send-Request([string]$Json) {
    $deadline = (Get-Date).AddSeconds(30)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    $deadline = (Get-Date).AddSeconds(40)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) {
        if ((Get-ScheduledTask -TaskName DbTools).State -ne 'Running') { Start-ScheduledTask -TaskName DbTools }
        Start-Sleep -Milliseconds 500
    }
}
function Lines() { if (Test-Path $ClientLog) { @(Get-Content $ClientLog -Encoding UTF8) } else { @() } }

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { "the game is running — close the windows"; exit 1 }
"series $Tag, logins $Runs -> $Out" | Tee-Object -FilePath $Out -Append
for ($i = 1; $i -le $Runs; $i++) {
    Set-Content $Control -Value "" -NoNewline
    $skip = (Lines).Count
    $t0 = Get-Date
    Send-Request '{"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","delay":5,"window":false,"probe":false}'
    $new = @(); $ok = $false; $deadline = (Get-Date).AddSeconds(180)
    while ((Get-Date) -lt $deadline) {
        $all = Lines; $new = if ($all.Count -gt $skip) { $all[$skip..($all.Count - 1)] } else { @() }
        if ($new -match 'phase -> lobby') { $ok = $true; break }
        Start-Sleep -Seconds 2
    }
    $fails = @($new | Where-Object { $_ -match '201--6' }).Count
    $clicks = @($new | Where-Object { $_ -match '(?:клик|click) StartGameButton' }).Count
    $secs = [int]((Get-Date) - $t0).TotalSeconds
    $line = "{0} #{1} {2}: clicks {3}, errors 201 {4}, {5} s" -f (Get-Date -Format "HH:mm:ss"), $i, $(if ($ok) { "lobby" } else { "DID NOT LOG IN" }), $clicks, $fails, $secs
    $line | Tee-Object -FilePath $Out -Append
    Set-Content $Control -Value "quit" -NoNewline
    $deadline = (Get-Date).AddSeconds(60)
    while ((Get-Date) -lt $deadline -and (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue)) { Start-Sleep -Seconds 1 }
    Set-Content $Control -Value "" -NoNewline
    if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { "the client did not exit — stopping"; exit 1 }
    Start-Sleep -Seconds 3
}
