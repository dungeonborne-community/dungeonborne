# Dungeonborne Dev (dev-app)

An app for developers of the server emulator. Not handed out to players. Tauri 2 + Rust, the UI is plain
HTML/CSS/JS in `ui/` with no build (no trunk, no tailwind, no npm). Flow: **dev → backend → Host Game → clients**.

```
dev-app/
  src/lib.rs       Tauri commands, the "one button" scenario, app exit (stop the backend, release the lock)
  src/config.rs    settings and profiles: %APPDATA%\DungeonborneDev\settings.json
  src/backend.rs   cargo run in core/backend with the profile's env, output to a ring buffer and the window, job object
  src/ports.rs     who holds the ports (in plain words), the portlock.ps1 lock
  src/clients.rs   game launch (UAC / as administrator), arguments like dbtest\dbtools.ps1, killing mutexes
  src/host.rs      game server: dbserver-status.json, dbserver-control.txt, match-mode.txt, GET /ds/agents
  src/admin.rs     admin API client + admin login through the launcher web API (dbl::auth)
  src/mock.rs      admin API mock per ADMIN_API.md (GM tab → "Mock", or --mock-admin)
  src/tools.rs     python -I tools/py/*.py from the workspace root
  src/winx.rs      job object, rights check, open a file, hidden powershell/schtasks
  tests/           admin API contract: the client against the mock; admin_live — against the backend (--ignored)
  ui/i18n.js       UI language: English by default, Russian in Settings → Language
  ADMIN_API.md     the backend admin API contract
```

From the launcher (crate `dungeonborne-launcher`, library `dbl`) these are used without copying: `winutil` (UAC via
ShellExecute, processes, port owner), `profile::Install`, `auth` (login through the server), `manifest`, `creds`
(Credential Manager), `net` (HTTP client and URL policy), `store`, `error`. The launcher was not changed except for the
`members` line in `launcher/Cargo.toml` and `/dev-app/gen` in `.gitignore`.

## Build

```
cd <data>\launcher
cargo build --release -p dungeonborne-dev     # -> target\release\dungeonborne-dev.exe, a single file
cargo test -p dungeonborne-dev
```

`cargo tauri` is not needed: the `custom-protocol` feature is on by default, `ui/` is embedded in the exe as is.
The dependency on `dbl` also compiles the launcher's `tauri::generate_context!()` — for it `launcher/ui/dist`
must exist (built by the launcher's `cargo tauri build`; it exists now). WebView2 Runtime is needed.

Running as administrator is preferable: then the game starts without UAC. Without the rights everything works, but every game launch
(client or server) and killing mutexes without the DbTools task ask for UAC.

## What is done (v1)

**Backend profiles** ("Backend" tab). A profile: `RPC_DUMP=1`, `SERVER_TOML` (suggestions — `probe/auth_test/*.toml`,
`server*.toml`), `DB_PATH` (`surreal_saves` / `probe/auth_test/db` / any), `PVE_SNIFF=127.0.0.1`, `BIND_IP`,
`--release`, clients, client mode, Host Game. These five variables are never inherited from dev-app's environment —
only from the profile. The launch line for a terminal is shown and can be copied (`$env:RPC_DUMP='1'; cargo run`).
Ready profiles: "Local backend + 1 client" (default), "+ 2 clients", "Backend only", "Auth test",
"PVE sniffer", "Host: server + client (connect)", "DS: server + client (native match)".

- Start: check of ports 80/443/8085/6601–6604 (+8090/7443 if there is a `server.toml`) — busy ones with an explanation of who
  holds them (a backend started elsewhere, PVE SoloLobby, the player launcher's relay, the launcher mock, HTTP.sys…) and the exe path →
  `dbtest\portlock.ps1 -Acquire -Owner "dev-app"` → `cargo run`. Stop/backend exit/closing dev-app → `-Release`.
  9091 (ds-agent) and 9092 (admin API) are shown for information. In the PVE sniffer profile SoloLobby on 6601–6604 is not in the way.
- `cargo run` and the backend are in one job object: "Stop" kills the whole tree, if dev-app crashes the backend also
  exits (the backend does not catch Ctrl+C, so it is the same as Ctrl+C in a terminal).
- Output: the last 20,000 lines, filters `[RPC]` `[Auth]` `[Relay]` `[GM]` `[Admin]` `[DS]` (together with `[DS-Link]`,
  `[DSA-Probe]`) `[Room]` `[Matchmaking]`, search, stderr only, autoscroll.

**One button** "▶ Run profile": backend (or the one already running) → wait until the backend process takes 8085 →
Host Game if any (server, wait for `phase = ready` up to 180 s; DS — `match-mode.txt = native`) → clients in order.
Before each next instance — a 25 s pause and killing mutexes (like `dbtest\play.ps1`). Progress — the bar under the header.

**Game** ("Game" tab): installs 1–3 (default `C:\Steam\steamapps\common\Dungeonborne`, `C:\Games\Dungeonborne`),
"running" — by a locked `ue4ss\UE4SS.log` (process paths under ACE are hidden). Launch — game window or `-dbclient
-dbconnect=127.0.0.1:7777` (the `client-E.bat` arguments; the first client writes to `dbtools\run`, the next ones — `run2`, `run3`).
Mutexes: the DbTools task if registered (without UAC), otherwise `dbtest\killmutex.ps1` with UAC; "List mutexes" —
only via DbTools. `quit`/`probe` to a client — `dbclient-control.txt`. dev-app cannot kill the game and does not try.

**Host Game**: server `-dbserver` with the `dbtools.ps1` arguments (observer, window, map `/TgfMaps/...`, `-dbprobe`,
DSA `ip:port` for DS SDK experiments), launched with administrator rights (UAC or directly). Status from
`dbserver-status.json`, `quit` into `dbserver-control.txt`, `match-mode.txt = native` with the previous value restored
(a copy in `%APPDATA%\DungeonborneDev\match-mode.before.txt`), `GET /ds/agents` on 9091 — view only.
Commands to the server go through the backend (`/admin/v1/host/*`, `ADMIN_API.md`); dev-app does not write them into the control file:
- buttons "Portal in front of players" (`portal`), "Portal in front of player…" (player from `/admin/v1/players`), "Open all
  exits" (`exits`) and `probe`;
- the mod's answer and `last_command`, i.e. the last command, its result and time; the status refreshes every 5 s;
- clear messages for `rejected_not_ready` and 503 ("no host").

**GM console** ("GM" tab): online list (every 5 s), player choice, commands `help/info/gold/coin/item/facade/ban/unban`,
history ↑/↓. Works against the backend admin API (`core/backend/src/admin_api.rs`, `http://127.0.0.1:9092`, contract —
`ADMIN_API.md`). The backend runs any command in a player's context, so for `help/ban/unban` the
selected or first online player is substituted. When the backend is not running — a built-in mock on the same port ("Mock" button, mode
`require = relay` with tokens `mock-admin`/`mock-gm`); it behaves like the backend. Admin login for `require = relay`: "Log in (relay)" — manifest
`{web_url}/.well-known/dbserver.json` → login in the browser, as in the launcher → token in memory and in Credential Manager
(`DungeonborneLauncher:dev-app.<server.id>`); or paste a Bearer by hand. The token goes only to a loopback address.

**Tools**: `compare_heads`, `probe_timeline`, `auth_e2e` (`python -I tools/py/<name>.py <arguments>`, cwd = workspace root,
output to the window, Stop), open `rpc_dump/OUR.jsonl` (without an association — Notepad), folders `rpc_dump`, `probe/logs`,
`dbtools/logs`, `dbtools/run`.

## Verified

- `cargo test -p dungeonborne-dev`: 6 unit tests (log tags, ANSI, env line, server/client arguments, parsing
  tool arguments) + 6 contract tests (list, commands, host commands with 400/503/rejected_not_ready, offline player, ban removes from online,
  400/401/403/404/415, `Origin`, `Host`, refusing to send the token to a non-loopback address, response shapes of `admin_api.rs`).
- `admin_live` against a running backend (02.11, a player online): `host/status`, `players`, `help`, `info`, 404 for someone else's open_id — ok. Host commands (portal/exits) were not sent to a live server.
- UI: checked in a browser with a command stub (rendering, profile, ports, player choice → `gold 1000`).
- `cargo build --release -p dungeonborne-dev` → a single exe.
- **Not verified live**: launching the game and the server from dev-app (UAC, mutexes), a whole profile scenario with the game,
  admin login through the real web API, the window of the exe itself (WebView2) — the UI was checked in a browser.

## What the backend is expected to do

- **Admin API** is implemented (`core/backend/src/admin_api.rs`), dev-app works with it as is. Desirable improvements —
  `ADMIN_API.md` §8: checking `Host` (DNS rebinding with `require = "none"`) and `Origin`, `GET /admin/v1/health`,
  changes to an online player through the live session, `help/ban/unban` without a player online. Check: `cargo test -p
  dungeonborne-dev --test admin_live -- --ignored` with a running backend.
- **ds-agent** (docs/DS_PIPELINE.md): not there yet — dev-app's DS mode = server + the game's native match button, the backend works in
  the fallback mode without `init_player`. When the agent appears, it can be started from here next to the server.

## Next (not in v1)

- Starting the server through the DbTools task without UAC (it has a whitelist `start role=server`).
- A remote backend through the launcher relay (`dbl::relay`) — a "GKE/Hetzner" profile.
