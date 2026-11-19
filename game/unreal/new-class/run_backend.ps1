# Backend of the trial class: own folder, own tables (server\ExcelConfig), own database (server\surreal_saves).
# The same settings as backups\dbtest\run_local.ps1. The ports are the same, so the main backend must be stopped:
#   backups\dbtest\portlock.ps1 -Acquire -Owner new_class -Purpose "trial class"
$ErrorActionPreference = 'Stop'
Set-Location "$PSScriptRoot\server"
$env:SERVER_TOML = 'no-server.toml'   # no such file -> login without accounts, without the relay
$env:DS_POOL = '127.0.0.1:7777@*'
$env:DS_WAITING_ROOM = '1'
$env:MATCH_OPEN_HALL = '1'
$env:MATCH_MIN_TEAMS = '1'
$env:DS_WAITING_TIMEOUT = '15'
$env:RPC_DUMP = '1'
$env:RUST_BACKTRACE = '1'
& .\dbserver-rs.exe
