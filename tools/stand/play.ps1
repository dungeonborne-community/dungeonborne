# Session for live play "listen as dedicated": headless server (Steam install, server account …949020,
# observer) + game windows with GUI, where a human presses the native match button and the backend sends them to the server itself.
#   powershell -ExecutionPolicy Bypass -File tools\stand\play.ps1            start the server and 2 windows (F:, E:)
#   powershell -ExecutionPolicy Bypass -File tools\stand\play.ps1 -Stop      shut down the server, restore match-mode.txt
#   ... play.ps1 -Map /TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01   server straight into the waiting hall
#       (backend with DS_WAITING_ROOM=1, docs/WAITING_ROOM.md)
# Needs: the DbTools task (README_admin.md), backend on 6601, a session as administrator (windows are started directly).
# Window accounts — Goldberg: C:\Games\Dungeonborne …609331 (Save_Profile_2), C:\Games\Dungeonborne …0000003 (Save_Profile_3).
param([switch]$Stop, [string[]]$Clients = @('client', 'client-e'), [string]$Map = "")

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Clients = $Clients | ForEach-Object { if ($Games.ContainsKey($_)) { $Games[$_] } else { $_ } }   # names from local.toml [games] or paths
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
$ModeFile = Join-Path $env:LOCALAPPDATA "Dungeonborne\Saved\RebuildProbe\match-mode.txt"
$ModeBackup = Join-Path $Work "match-mode.before-play.txt"

function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }

function Send-Request([string]$Json) {
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Start-Sleep -Milliseconds 1500
}

if ($Stop) {
    Set-Content (Join-Path $Run "dbserver-control.txt") -Value "quit" -Encoding ASCII
    if (Test-Path $ModeBackup) {
        $was = Get-Content $ModeBackup -Raw
        if ($was -eq "<none>") { Remove-Item $ModeFile -ErrorAction SilentlyContinue } else { Set-Content $ModeFile -Value $was -NoNewline -Encoding ASCII }
        Remove-Item $ModeBackup
    }
    Say "server: quit; match-mode.txt restored. Close the game windows yourself."
    exit 0
}

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Say "the game is already running — close it"; exit 1 }
if (-not (Get-NetTCPConnection -State Listen -LocalPort 6601 -ErrorAction SilentlyContinue)) { Say "backend is not listening on 6601"; exit 1 }
New-Item -ItemType Directory -Force $Run | Out-Null
foreach ($f in "dbserver-control.txt", "dbclient-match.txt") { Set-Content (Join-Path $Run $f) -Value "" -Encoding ASCII }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
if ($Map) {
    # like hostflow.ps1: the hall is run by the mod on the backend's assignment, without InitGameMap and without a manual hall
    Set-Content (Join-Path $Run "dbserver-initmap.txt") -Value "" -Encoding ASCII
    Remove-Item (Join-Path $Run "dbserver-waiting.txt") -ErrorAction SilentlyContinue
}

# 1. Server (before match-mode = native: on the server the button/LocalDev is in normal mode)
$t0 = Get-Date
Say "server..."
$mapArg = if ($Map) { ',"map":"' + $Map + '"' } else { "" }
Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false' + $mapArg + '}')
$status = Join-Path $Run "dbserver-status.json"; $ok = $false; $deadline = (Get-Date).AddSeconds(180)
while ((Get-Date) -lt $deadline) {
    if ((Test-Path $status) -and (Get-Item $status).LastWriteTime -gt $t0 -and (Get-Content $status -Raw) -match '"phase":"ready"') { $ok = $true; break }
    Start-Sleep -Seconds 2
}
if (-not $ok) { Say "the server did not reach ready — see $Run\dbserver.log"; exit 1 }
Say "server ready (127.0.0.1:7777)"

# 2. Native match button for the windows
Set-Content $ModeBackup -Value $(if (Test-Path $ModeFile) { Get-Content $ModeFile -Raw } else { "<none>" }) -NoNewline -Encoding ASCII
Set-Content $ModeFile -Value "native" -Encoding ASCII
Say "match-mode.txt = native"

# 3. Windows: before each one, remove the single-instance mutexes from those already running
foreach ($c in $Clients) {
    $exe = Join-Path $c "Dungeonborne\Binaries\Win64\Dungeonborne.exe"
    if (-not (Test-Path $exe)) { Say "no $exe"; continue }
    Send-Request '{"action":"killmutex"}'
    Start-Process -FilePath $exe -WorkingDirectory (Split-Path $exe) | Out-Null
    Say "window: $c"
    Start-Sleep -Seconds 45  # time to create its own mutexes before the next removal (02.11: with UE4SS 25 s was not enough — E: crashed)
}
Say "done: in each window log in and press the match button. To finish — play.ps1 -Stop"
