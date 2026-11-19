# Census of spawn points per map (03.11, match limits): the server comes up on each map in turn (without
# clients and without a backend assignment), the mod writes [STARTS] lines (start sets, teams, LevelArgs, MaxPlayers).
# Result — dbtools\runs\starts-<time>.txt. Needs the DbTools task; the backend is optional.
#   powershell -ExecutionPolicy Bypass -File tools\stand\startscensus.ps1
param([string[]]$Maps = @(
    "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01",
    "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01",
    "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic",
    "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Hard",
    "/TgfMaps/Scenes/GothicCastle/Level/L_GothicCastle_Matching",
    "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main",
    "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Classic",
    "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Hard"))

# via powershell -File the list arrives as one comma-separated string
$Maps = @($Maps | ForEach-Object { $_ -split ',' } | Where-Object { $_ -ne "" })

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
function Count-Game() { @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue).Count }

if (Count-Game) { Say "the game is already running — exit it first"; exit 1 }
$out = Join-Path $Work ("runs\starts-" + (Get-Date -Format "yyyyMMdd-HHmmss") + ".txt")
foreach ($Map in $Maps) {
    Say "map $Map"
    if (Test-Path $ServerLog) { Remove-Item $ServerLog }
    Set-Content (Join-Path $Run "dbserver-control.txt") -Value "" -Encoding ASCII
    Send-Request ('{"action":"start","role":"server","install":"steam","delay":5,"observer":true,"window":false,"map":"' + $Map + '"}')
    $deadline = (Get-Date).AddSeconds(240); $got = $false
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 3
        if ((Test-Path $ServerLog) -and (Select-String -Path $ServerLog -Pattern "\[STARTS\]" -Quiet)) { Start-Sleep -Seconds 3; $got = $true; break }
    }
    if ($got) { Get-Content $ServerLog -Encoding UTF8 | Where-Object { $_ -match "\[STARTS\]" } | Add-Content -Path $out -Encoding UTF8 }
    else { Add-Content -Path $out -Value "$Map : no [STARTS] within 240 s" -Encoding UTF8 }
    Set-Content (Join-Path $Run "dbserver-control.txt") -Value "quit" -Encoding ASCII
    $deadline = (Get-Date).AddSeconds(45)
    while ((Get-Date) -lt $deadline -and (Count-Game)) { Start-Sleep -Seconds 1 }
    if (Count-Game) { Say "the server did not exit — stopping"; break }
}
Say "result: $out"
Get-Content $out -Encoding UTF8 | ForEach-Object { Write-Host $_ }
