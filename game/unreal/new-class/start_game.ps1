# Launching separate game copies. Close the main game and the main backend first.
#   -Server  game server (needed only for a raid)
#   -Client  client
# ACE raises UAC on launch: the confirmation window must be accepted.
#   -ServerSetup  a server copy with a window: once, create a character for the server account on a new database
param([switch]$Server, [switch]$Client, [switch]$ServerSetup)
$ErrorActionPreference = 'Stop'
# Machine paths — config/local.toml [unreal]: new_class (working folder: logs, run), new_class_sandbox (game copies
# server\ and client\); the DbTools request — into the stand's working folder ([stand] work)
. "$PSScriptRoot\..\..\..\tools\stand\paths.ps1"
function Need([string]$key) {
    $v = Get-LocalValue 'unreal' $key
    if (-not $v) { throw "no [unreal] $key in config/local.toml (example — config/local.example.toml)" }
    return (Resolve-WsPath $v)
}
$root = Need 'new_class'
$sandbox = Need 'new_class_sandbox'
New-Item -ItemType Directory -Force "$root\logs", "$root\run\server" | Out-Null

function Start-Game([string]$role, [string[]]$gameArgs) {
    $win64 = "$sandbox\$role\Dungeonborne\Binaries\Win64"
    $p = Start-Process -FilePath "$win64\Dungeonborne.exe" -WorkingDirectory $win64 -ArgumentList $gameArgs -PassThru
    "$role started, pid $($p.Id)"
}

function Release-Mutex {
    # Second game instance: release the mutexes via the DbTools task (backups\dbtest\README_admin.md)
    '{"action":"killmutex"}' | Set-Content (Join-Path $Work 'request.json') -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    Start-Sleep -Seconds 3
}

if ($ServerSetup) {
    # Once on a new database: the server account has no character, without one it does not get past the login screen.
    # A server copy with a window: create any character and close the game. Auto-creation in the backend works only with the relay (Robot).
    Release-Mutex
    Start-Game 'server' @("-ABSLOG=`"$root\logs\ue-server-setup.log`"", "-log")
}
if ($Server) {
    Start-Game 'server' @("-dbserver", "-ABSLOG=`"$root\logs\ue-server.log`"", "-log", "-unattended", "-nosplash",
        "-dbdelay=5", "-dbroot=$($root -replace '\\', '/')/run/server", "-nullrhi", "-nosound")
}
if ($Client) {
    if ($Server -or (Get-Process Dungeonborne -ErrorAction SilentlyContinue)) {
        Start-Sleep -Seconds 15
        Release-Mutex
    }
    Start-Game 'client' @("-ABSLOG=`"$root\logs\ue-client.log`"", "-log")
}
