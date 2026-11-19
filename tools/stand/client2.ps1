# Second GUI client: auto-login and connect to the headless server 127.0.0.1:7777 (DungeonborneRuServer -dbclient).
# Install: config/local.toml [games] client-e (a second copy of the game with its own Goldberg account).
# Files go to <stand work>\run2 (OpenId in dbclient-urlopts.txt), the UE log to <stand work>\logs\ue-client2-7777.log.
# Launch AFTER the first client is in the battle (the script that started it already released the game mutexes):
#   powershell -ExecutionPolicy Bypass -File tools\stand\client2.ps1
param([string]$Connect = '127.0.0.1:7777')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\paths.ps1"
$win64 = Join-Path (Get-Game 'client-e') 'Dungeonborne\Binaries\Win64'
$work = $Work -replace '\\', '/'
$port = ($Connect -split ':')[-1]
New-Item -ItemType Directory -Force (Join-Path $Work 'logs'), (Join-Path $Work 'run2') | Out-Null
$gameArgs = @('-dbclient', "-dbconnect=$Connect", "-ABSLOG=`"$work/logs/ue-client2-$port.log`"", '-log', '-nosplash',
    '-dbdelay=5', "-dbroot=$work/run2")
$p = Start-Process -FilePath (Join-Path $win64 'Dungeonborne.exe') -WorkingDirectory $win64 -ArgumentList $gameArgs -PassThru
"client2 started, pid $($p.Id): $win64"
