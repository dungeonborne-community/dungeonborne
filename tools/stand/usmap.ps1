# The game's mappings (usmap) from your own copy, without a human: the stand client starts headless through the DbTools task,
# the mod DungeonborneRuServer (client role, file dbclient-usmap.txt = 1) calls UE4SS DumpUSMAP and quits.
# The result goes to home\re\ue4ss, where cargo ws dump looks for it (the research dumps: icons, monsters, heroes, curves;
# the patch paks do not need it). No backend and no patch paks needed (19.11).
# Needs: the DbTools task (README_admin.md), [games] client from ws stand (the autotest mod is put there), the game closed.
#   powershell -ExecutionPolicy Bypass -File tools\stand\usmap.ps1 [-Install client] [-TimeoutSec 240] [-Load /Game/...]
#   -Load  assets loaded before the dump (/Game/Path/Asset.Asset_C): their Blueprint classes get into the usmap too
param([string]$Install = "client", [int]$TimeoutSec = 240, [string[]]$Load = @())

. "$PSScriptRoot\paths.ps1"   # $Work, $DbHome, $AdminLog, Get-Game
function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }

if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Say "the game is already running - exit it first"; exit 1 }
$game = Get-Game $Install
$ue4ss = Join-Path $game "Dungeonborne\Binaries\Win64\ue4ss"
if (-not (Test-Path (Join-Path $ue4ss "Mods\DungeonborneRuServer\enabled.txt"))) {
    Say "no enabled DungeonborneRuServer in $ue4ss\Mods - cargo ws stand --apply puts it into the client copies"; exit 1
}
# DbTools starts '[games] client' as its install 'second'
$dbInstall = if ($Install -eq "steam") { "steam" } else { "second" }

$Run = Join-Path $Work "run"
New-Item -ItemType Directory -Force $Run | Out-Null
$flag = Join-Path $Run "dbclient-usmap.txt"
$status = Join-Path $Run "dbclient-status.json"
Set-Content -Path $flag -Value (@("1") + $Load) -Encoding ASCII
Set-Content -Path (Join-Path $Run "dbclient-match.txt") -Value "" -Encoding ASCII
$since = Get-Date
try {
    Say "start: $game (headless)"
    $req = @{ action = "start"; role = "client"; install = $dbInstall; delay = 15 } | ConvertTo-Json -Compress
    Set-Content -Path (Join-Path $Work "request.json") -Value $req -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    $deadline = $since.AddSeconds($TimeoutSec); $done = $false; $last = ""
    while ((Get-Date) -lt $deadline) {
        Start-Sleep -Seconds 2
        if ((Test-Path $status) -and (Get-Item $status).LastWriteTime -gt $since) {
            $s = Get-Content $status -Raw
            if ($s -ne $last) { $last = $s; Say "  $($s.Trim())" }
            if ($s -match '"phase":"(usmap-done|quit)"') { $done = $true; break }
        }
    }
    if (-not $done) {
        Say "no usmap-done within $TimeoutSec s - logs: $(Join-Path $Run 'dbclient.log'), $ue4ss\UE4SS.log; DbTools: $AdminLog"
        Set-Content -Path (Join-Path $Run "dbclient-control.txt") -Value "quit" -Encoding ASCII
        exit 1
    }
    # wait for the process to exit (the mod quits by itself after the dump)
    $quitBy = (Get-Date).AddSeconds(60)
    while ((Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) -and (Get-Date) -lt $quitBy) { Start-Sleep -Seconds 2 }
} finally {
    Remove-Item -LiteralPath $flag -ErrorAction SilentlyContinue
}

$found = Get-ChildItem -LiteralPath $ue4ss -Filter *.usmap -File -ErrorAction SilentlyContinue |
    Where-Object { $_.LastWriteTime -gt $since } | Sort-Object LastWriteTime | Select-Object -Last 1
if (-not $found) { Say "DumpUSMAP ran but no new *.usmap in $ue4ss - see $ue4ss\UE4SS.log"; exit 1 }
$dst = Join-Path $DbHome "re\ue4ss"
New-Item -ItemType Directory -Force $dst | Out-Null
Copy-Item -LiteralPath $found.FullName -Destination (Join-Path $dst $found.Name) -Force
Say "usmap: $(Join-Path $dst $found.Name) ($([int]($found.Length / 1KB)) KB)"
Say "next: cargo ws dump paks --apply"
