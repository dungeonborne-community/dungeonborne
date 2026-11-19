# Backend admin API (contract for dev-app), v1

Status as of 02.11: **implemented** in `core/backend/src/admin_api.rs` (client example — `tools/py/gm.py`); the document
is aligned with that implementation. Where it still differs from the contract — §8 (desirable improvements, dev-app works
without them too). dev-app's GM console goes to the backend by default (`http://127.0.0.1:9092`); the built-in mock
(`dev-app/src/mock.rs`, GM tab → "Mock") behaves the same and is needed only when the backend is not running.

Why it is needed: a developer GM console — the list of online players and the GM commands `gm_exec` /
`player_gm_command_impl` (`src/rpc/handlers/player_handlers.rs`) for a chosen player, without logging into the game.

## 1. Where it listens

- `ADMIN_BIND`, default **`127.0.0.1:9092`**. Loopback only: the backend rejects non-loopback peers, and a
  non-loopback address in `ADMIN_BIND` is never needed (on GKE — `kubectl port-forward`, on Hetzner — an SSH tunnel).
- Always listens, with or without `server.toml`. `DS_AGENT_BIND=0.0.0.0` does not affect it.

## 2. Common checks (every request, before authorization)

| Check | Answer on violation |
|---|---|
| the peer address is loopback (`127.0.0.0/8`, `::1`) | `403` |
| `Host` is `127.0.0.1:<port>` or `localhost:<port>` (protection against DNS rebinding: without it any page in the developer's browser that rebinds its domain to 127.0.0.1 can run a GM command with `require = "none"`) | `403` |
| no `Origin` header (browsers send it on requests from pages — CSRF protection) | `403` |
| `POST` has `Content-Type: application/json` (axum `Json` already checks this) | `415` |
| the body is valid JSON ≤ 16 KiB | `400` |

The API never returns CORS headers.

## 3. Authorization

- `[auth] require = "relay"` (`server.toml`): header `Authorization: Bearer <session_token>`.
  The token is a launcher web API session: issued by `GET /api/launcher/v1/login/poll` → `session_token`, checked like in
  `GET /api/launcher/v1/me` (`auth::sessions::resolve`). The account must have the **`admin`** role and not be banned.
  - no / unknown / expired token → `401`
  - account without the `admin` role or banned → `403`
- `require = "none"` or no `server.toml` (local tests): no token needed; a sent one is ignored.
- `GET /admin/v1/health` — without a token (only the §2 checks).
- The token is not written to logs.

dev-app gets the token itself: GM tab → "Log in (relay)" — the same login as in the launcher (`dbl::auth`, browser),
the manifest is taken from `{web_url}/.well-known/dbserver.json`. The token is stored in Credential Manager
(`DungeonborneLauncher:dev-app.<server.id>`). The token can also be pasted by hand.

## 4. Errors

The error body is JSON with an `error` field:

```json
{"error": "no admin role"}
{"error": "forbidden", "message": "admin role required"}
```

`error` is human-readable text **or** a code; optional `code` (a stable code) and `message`. The client takes the code from
`code`, otherwise from `error` if it is a `[a-z_]+` word, otherwise by HTTP status:

| Status | Code |
|---|---|
| 400 | `bad_request` (no `cmd`; without `open_id`, not exactly one player online) |
| 401 | `unauthorized` |
| 403 | `forbidden` (not loopback, `Host`, `Origin`, no admin role) |
| 404 | `player_not_found` (no player with this `open_id`) / unknown path |
| 415 | `bad_content_type` |
| 503 | `host_unavailable` (a host command, but there is no `dbserver-status.json` in `DB_HOST_ROOT`) |

## 5. Methods

### `GET /admin/v1/health` (recommended)

```json
{"ok": true, "server_id": "local.auth.test", "auth_require": "relay", "version": "0.1.0"}
```

`server_id` — `[server] id` or `null` without `server.toml`; `auth_require` — `"none"` | `"relay"`. By it dev-app
decides whether to ask for login. If the method is missing (404), dev-app works and learns about login from the first `401`.

### `GET /admin/v1/players`

Players **online** (`lobby::hub::online_ids()`), an array:

```json
[
  {"open_id": "76561198000000903", "user_id": 10000903, "nick": "DK", "hall_role_guid": 1000090301,
   "roles": ["admin"], "provider": "discord", "display_name": "dk", "peer_ip": "127.0.0.1",
   "online_since": 1793600000, "in_battle": false}
]
```

| Field | Type | Required | Source |
|---|---|---|---|
| `open_id` | string | yes | hub |
| `user_id` | int \| null | yes | `PlayerModel.user_id`; `null` if the player is not in the DB yet |
| `nick` | string \| null | yes | name of the hero `hall_role_guid` |
| `hall_role_guid` | int \| null | no | `PlayerModel.hall_role_guid` |
| `roles` | string[] | no | account roles (`src/auth/accounts`), `[]` without an account |
| `provider`, `display_name` | string \| null | no | account |
| `peer_ip` | string | no | connection address |
| `online_since` | int, unix s | no | gateway login — dev-app sorts by it |
| `in_battle` | bool | no | in a matchmaker match room |

Nobody online — `[]`. (dev-app also understands the wrapper `{"players": [...]}`.)

### `POST /admin/v1/gm`

```json
{"open_id": "76561198000000903", "cmd": "gold 1000"}
```

- `cmd` — a string as in the in-game GM console (leading `/` and `.` are allowed), ≤ 256 characters. Parsing is the same:
  words separated by spaces, the first is the command, case-insensitive. Commands: `help`, `gold <n>`, `coin <id> <n>`,
  `item <type_id> [count]`, `facade <id>`, `info`, `ban <open_id|name> [reason]`, `unban <open_id|name>`.
- `open_id` — whom the command is for (gold/coins/item/cosmetic go to them, their `info`). Optional: without it —
  the only player online, otherwise `400` (nobody online or more than one). This also applies to `help`, `ban`, `unban`:
  the backend runs any command in a player's context, even though the ban target is an argument. dev-app substitutes the
  selected or first online player for them.
- The player may be **offline**: the change is saved to the DB; online — also a push `ScPlayerDirtyChgSync`, as with a
  command from the game. No player with this `open_id` → `404`.
- Rights — the administrator's from §3: the player's `may_gm` check is not done. `by` of a ban — `admin:<open_id>` from the token,
  without a token — `admin-api`.

Answer `200` — `ScPlayerGmCommandRsp` as JSON plus the target:

```json
{"open_id": "76561198000000903", "online": true, "result_code": 0, "result_message": "ok", "result_list": ["gold = 1500"]}
```

An error of the command itself (unknown command, wrong parameter, "not enough gold") is also `200`, with
`result_code` = `ERR_GM_CMD_INVALID` (-501) / `ERR_GM_CMD_PARAM_INVALID` (-503) and the text in `result_message`,
as in the game. HTTP error codes — only for §2–§4 and the target.

Backend log: a line tagged `[GM]` (e.g. `[GM] admin:<open_id> → <open_id>: [...]`) — the dev-app window filters
by `[GM]`.

### Commands to the game server (Host Game)

Chain **dev-app → backend → host**. dev-app does not write these commands into `dbserver-control.txt` itself: transport is
the backend's job. For a local host it is a write to `<DB_HOST_ROOT>\dbserver-control.txt`, default
`dbtools/run`; for a remote one there will be ds-agent later. The `DungeonborneRuServer` mod runs
the command through the game's native `UTgfCheatComponent` and answers in `dbserver-command-result.json`. Authorization — as for
`/admin/v1/gm`. dev-app writes directly into the control file only `quit`: the backend does not pass it.

#### `GET /admin/v1/host/status`

```json
{"root": "dbtools/run",
 "status": {"role": "dbserver", "phase": "ready", "map": "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic", "observer": true, "updated_at": "2026-11-02 08:45:16"},
 "last_command": {"cmd": "portal", "result": "portals_spawned=0", "at": "2026-11-02 08:45:22"}}
```

`status` — `dbserver-status.json` as is, `last_command` — `dbserver-command-result.json`; either of them is `null`
if the file is missing.

#### `POST /admin/v1/host/command`

```json
{"cmd": "portal 76561198000000903"}
```

| `cmd` | What it does |
|---|---|
| `portal` / `portal all` | a blue portal (`SpawnOnePortal(5, …)`) 2 m in front of every player |
| `portal <open_id>` | only in front of one player |
| `exits` | all exits of the map: `SpawnAllEscapeDevices` + `ActivateAllEscapeDevices` |
| `probe` | mod probe (writes no result to the file) |

Answer `200`:

```json
{"sent": "portal 76561198000000903", "host_phase": "ready", "map": "/TgfMaps/…", "result": {"cmd": "…", "result": "portals_spawned=1", "at": "…"}}
```

- The backend waits up to 10 s for a new `dbserver-command-result.json`. If there is no answer, and always for `probe`, `result` = `null`.
- The server is not in phase `ready` → `result.result = "rejected_not_ready"`.
- Another command, longer than 64 characters or with control characters → `400`. dev-app cuts off such commands before
  sending. No host → `503`.

## 6. Implementation notes

- An online player: change their data **in the live session** (through the hub), not in a temporary `GatewaySession` loaded from
  the DB: the live session has its own copy of `PlayerModel`, its next `save_me` overwrites what was granted (granted gold — the player sold
  an item — the gold is gone). Offline — a temporary session is fine.
- `may_gm` remains for commands from the game; the admin API does not call it.
- A separate listener (like `ds::agent_link`), not on `[web] bind`: the web API faces outside through a proxy.

## 7. Verification

- Mock: `dungeonborne-dev.exe --mock-admin [--port 9092] [--require relay]` or the "Mock" button on the GM tab.
  In `relay` mode the mock accepts `Bearer mock-admin` (admin) and answers `403` to `Bearer mock-gm`.
- Contract tests: `cargo test -p dungeonborne-dev` (the client against the mock: list, commands, offline, 400/401/403/404/415,
  `Origin`, `Host`; and the shapes of the backend draft).
- Against the real backend. Only `host/status`, `players`, `help` and `info` are sent, no host commands; for
  `help` and `info` a player must be online:
  ```
  $env:ADMIN_URL='http://127.0.0.1:9092'; $env:ADMIN_TOKEN='<if require = relay>'
  cargo test -p dungeonborne-dev --test admin_live -- --ignored
  ```

## 8. Differences of `admin_api.rs` (02.11) from the contract

| What | Now | Needed |
|---|---|---|
| `Host` check | none | §2 — required (DNS rebinding with `require = "none"`) |
| `Origin` check | none (`Json` cuts CSRF POSTs, but not rebinding) | §2 |
| `GET /admin/v1/health` | none | §5, recommended |
| online player | temporary session from the DB | §6 — through the live session |
| fields `roles`, `online_since`… | none | optional; without `online_since` the list is not sorted by login time |
| `help`/`ban`/`unban` when nobody is online | `400` | preferably run without a player (banning an offline account with an empty server) |
