# ACE A/B experiment (05.11): backend for playing THROUGH THE LAUNCHER on this machine (dev login, no Discord), game server —
# local 7777 (Steam install, as in run_local.ps1). Separate DB probe/auth_test/db (new account).
# Run from any folder:  powershell -ExecutionPolicy Bypass -File tools\stand\run_launcher_dev.ps1
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/paths.ps1"
$BackendExe = Enter-Backend   # cwd = core/backend; SEASONS_DIR, MATCH_CHECKS_FILE, DB_HOST_ROOT
$env:SERVER_TOML = "$WsRoot/tools/probe/auth_test/server_launcher_dev.toml"
$env:DB_PATH = "$WsRoot/tools/probe/auth_test/db"
$env:BIND_IP = '127.0.0.2'
$env:DS_POOL = '127.0.0.1:7777@*'
$env:DS_WAITING_ROOM = '1'
$env:MATCH_OPEN_HALL = '1'
$env:MATCH_MIN_TEAMS = '1'
$env:RPC_DUMP = '1'
$env:RUST_BACKTRACE = '1'
& $BackendExe
