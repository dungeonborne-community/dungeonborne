# Local development (05.11): backend on 127.0.0.1, same match logic as prod, but without the launcher and relay.
#   Game server — the Steam install (C:\Steam\steamapps\common\Dungeonborne, paks and mods as in the Robot kit),
#   started via the dbtools task {"action":"start","role":"server","install":"steam",...}.
#   Client — C:\Games\Dungeonborne (client mods), the game goes to 127.0.0.1 via hosts (the launcher's block).
# Run from any folder:  powershell -ExecutionPolicy Bypass -File tools\stand\run_local.ps1
# Before starting — the port lock: tools\stand\portlock.ps1 -Acquire -Owner <who> -Purpose <why>
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/paths.ps1"
$BackendExe = Enter-Backend   # cwd = core/backend; SEASONS_DIR, MATCH_CHECKS_FILE, DB_HOST_ROOT
$env:SERVER_TOML = "$WsRoot/tools/probe/no-server.toml"   # no file -> legacy: login without accounts, without the relay
if (-not $env:DS_POOL) { $env:DS_POOL = '127.0.0.1:7777@*' }   # 09.11: own pool can be set from outside (check on 7787)
$env:DS_WAITING_ROOM = '1'
$env:MATCH_OPEN_HALL = '1'
$env:MATCH_MIN_TEAMS = '1'
$env:DS_WAITING_TIMEOUT = '30'  # stand waiting hall timer (without the variable — from map_detail, usually 120). 08.11 at the
#                                  # user's request back to 30 s: with 15 s the second client did not make it into the hall
# 08.11: launcher lobby on the stand — tabs via the admin API (POST /admin/v1/lobby), at least 2 players (8 on prod),
# the hall is held until the minimum for 120 s beyond the timer. LAUNCHER_LOBBY=0 from outside — normal matchmaking: needed by
# run.ps1 -Match (the client presses the match button; with the lobby on, the backend answers "choose an instance in the
# launcher", -1028 — found 19.11 by the clean public run)
if (-not $env:LAUNCHER_LOBBY) { $env:LAUNCHER_LOBBY = '1' }
$env:LOBBY_MIN_PLAYERS = '1'
$env:LOBBY_HALL_HOLD_SECS = '120'
if (-not $env:LOBBY_WAIT_SECS) { $env:LOBBY_WAIT_SECS = '60' }   # 12.11: can be set from outside (hall 30 s)
# instance fill — LOBBY_FILL_SECS from outside (default 480; 12.11 on the stand 5 s)
$env:RPC_DUMP = '1'
$env:SEASON = 'playtest'   # season as on prod: forge materials on login, minor relics (seasons/playtest)
$env:TRADE_SYSTEM_BUYOUT = '1'   # system sellers on Buyout Trades
$env:TRADE_SYSTEM_AUCTION = '1'  # system lots on the auction
$env:TRADE_SYSTEM_BUYOUT_STOCK = '25'  # 25 of each item on Buyout (07.11, user)
$env:TRADE_SYSTEM_AUCTION_LOTS = '25'  # 25 lots of each item on the auction (07.11, user)
# Sh4g debugging (05.11) — turned off after K0 (TGCP frame number), behaviour as on prod:
# $env:RAID_DEFER_ROOM = '0'
# $env:RAID_FLUSH_ON_LEFT = '0'
# $env:RAID_PUSH_BEACON_ALL = '1'   # beacon in the lobby too (check: does receiving stall ~100 s after login?)
# $env:RAID_PUSH_BEACON_SECS = '5'   # beacon: dirty_role every 5 s while the player is in a match room (ClientProbe: RoleInfoNoti)
# stand host (Steam install — battle server): the backend creates a character for it itself (08.11). Everyone has their own ID —
# local.toml [stand] host_open_id (Goldberg: the install's steam_settings\force_steamid.txt) or DS_HOST_OPEN_IDS from outside
if (-not $env:DS_HOST_OPEN_IDS) {
    $hostId = Get-LocalValue 'stand' 'host_open_id'
    if ($hostId) { $env:DS_HOST_OPEN_IDS = $hostId }
    else { Write-Warning 'no [stand] host_open_id in config/local.toml: no character is created for the stand host' }
}
# stand clients (19.11): their Goldberg IDs (steam_settings\force_steamid.txt of [games] client, client-e) get a character on
# first login too, so run.ps1 works on a fresh database; a character made by hand stays as it is
if (-not $env:STAND_CLIENT_OPEN_IDS) {
    $ids = foreach ($k in 'client', 'client-e') {
        if ($Games.ContainsKey($k)) {
            $g = Get-Game $k
            $f = Join-Path $g 'Dungeonborne\Binaries\Win64\steam_settings\force_steamid.txt'
            if (Test-Path -LiteralPath $f) { (Get-Content -LiteralPath $f -Raw).Trim() }
        }
    }
    if ($ids) { $env:STAND_CLIENT_OPEN_IDS = ($ids -join ',') }
}
# the local login's TLS: the stand's own certificate of this machine (cargo ws stand --apply makes it, 18.11), never
# the production one; without it — cert.pem / key.pem in core/backend, as before
$tls = Join-Path $Work 'tls'
if (-not $env:TLS_CERT -and (Test-Path (Join-Path $tls 'cert.pem'))) {
    $env:TLS_CERT = Join-Path $tls 'cert.pem'
    $env:TLS_KEY = Join-Path $tls 'key.pem'
}
$env:RUST_BACKTRACE = '1'
& $BackendExe
