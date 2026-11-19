# Backend for checking the launcher (Discord login + relay + game on one machine). Run from any folder.
# Binary, without cargo (debug build). The Discord secret comes from the system variable DISCORD_CLIENT_SECRET (not printed).
. "$PSScriptRoot/../../stand/paths.ps1"
$BackendExe = (Enter-Backend) -replace 'release', 'debug'   # cwd = core/backend; SEASONS_DIR, MATCH_CHECKS_FILE
$env:SERVER_TOML = "$PSScriptRoot/server_launcher.toml"
$env:DB_PATH = "$PSScriptRoot/db"
$env:BIND_IP = '127.0.0.2'
$env:RPC_DUMP = '1'
$env:DS_WAITING_ROOM = '1'
& $BackendExe
