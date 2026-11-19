# tools/stand — local test stand

The stand is a backend on `127.0.0.1` plus your own copy of the game: one install runs as the battle server (headless, as on
the prod battle server), another as the client. It is used to check changes from login to the end of a match. Scripts are PowerShell; machine
paths come only from `config/local.toml` (`paths.ps1`), the scripts have no paths of their own.

## Without the game

If you have no copy of the game, you can still check a change:

- **backend** — `cargo test -p dbserver-rs` in `core/backend` (how to build — `core/backend/README.md`);
- **launcher** — a mock server instead of the backend: `cargo run -p dbl-mock-server -- --demo-mod` in `apps/launcher`
  (`apps/launcher/README.md`, the section about the mock);
- **mods** — `cargo xtask mods check` in `core/backend` (checks the Lua and the catalog).

## Setting it up: `cargo ws stand`

```powershell
cargo ws stand                 # what is there and what is missing (changes nothing)
cargo ws stand --apply         # build what the kit can; --clients=2 for a second client
```

From your game reference (`docs/GAME_DATA.md`) it makes the copies `home\game\stand-server` and `stand-client` (paks
are hard links: about 1 GB each), puts the launcher's client runtime in each (Goldberg, UE4SS, the mod loader — the
artifact `launcher bundle client.zip`), gives each its own Goldberg account, installs the mods of its profile and
writes `[games]` and `[stand] host_open_id` into `local.toml`. Installs you already listed in `[games]` are left as
they are.

The local login is HTTPS: `ws stand` makes a certificate of **this machine** for the login names (`<stand work
folder>\tls\cert.pem`, `key.pem` — in home, never in the workspace) and adds it to `cacert.pem` of the stand copies
only. `run_local.ps1` gives it to the backend (`TLS_CERT`, `TLS_KEY`). The production certificate, which every
player's client trusts, is never handed out (18.11). At the end it names the two steps only a human can do, with the exact text: the DbTools task (as
administrator) and the `hosts` block.

Several workspaces on one PC: each runs `install-dbtools.ps1 -Apply` once — the task keeps all of them; the port lock
is one folder for the machine (`%ProgramData%\DungeonborneStand\locks`), so `[stand] locks` should not be set.

## What the stand needs

| What | Where it comes from |
|---|---|
| Backend build | `cargo build --release -p dbserver-rs` in `core/backend`; the stand takes `target/release/dbserver-rs.exe` from `CARGO_TARGET_DIR` |
| Game install — battle server | your own copy from Steam, in `config/local.toml [games] steam`; server paks and mods — as on the prod battle server: `cargo xtask mods install --profile=server --game=steam` in `core/backend` (the game is closed) |
| Game install — client | a second copy, `[games] client`; Goldberg (its own account), UE4SS and client mods: `cargo xtask mods install --profile=client --game=client`; a third one, if a second client is needed — `[games] client-e` |
| Stand host | Steam ID of the server install (Goldberg) in `[stand] host_open_id`: the backend creates a character for it itself |
| DbTools task | once, as administrator: `ops\machine\install-dbtools.ps1 -Apply` (`README_admin.md`). The game under anti-cheat raises UAC; the task launches it and removes the mutexes without the agent having administrator rights |
| hosts | the client goes to the game's domains: a block in `hosts` pointing to `127.0.0.1` (the same one the launcher sets). A human changes it, not the agent |
| Work folder | `[stand] work` (default `home:runs/stand`): task requests, `run/`, `logs/`, `runs/` |

There are no game or Goldberg files in the repository: the copy of the game is your own.

## How to run

```powershell
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Acquire -Owner "<who>" -Purpose "<why>"
powershell -ExecutionPolicy Bypass -File tools\stand\run_local.ps1      # backend, keep the window open
# battle server and client — via the DbTools task: run.ps1 (automated run) or play.ps1 / gui2.ps1 (with a human)
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Release -Owner "<who>"
```

The port lock (`portlock.ps1`) is needed because the client's ports are hardcoded (backend 8085, 6601–6604, battle server 7777):
only one person or one agent session can hold the stand at a time.

## Scripts

| Script | What it does |
|---|---|
| `paths.ps1` | paths from `local.toml`: `$WsRoot`, `$Work`, `$LockDir`, `$Games`, `Get-Game`, `Enter-Backend`, `$AdminLog` |
| `run_local.ps1` | backend for the stand (login without the launcher and accounts, season as on prod, waiting hall) |
| `run_launcher_dev.ps1`, `run_lobby_dev.ps1` | backend for playing through the launcher on this machine (dev login, no Discord; instance lobby) |
| `run.ps1` | automated run without a human: battle server + client, logs in `runs\<time>` |
| `usmap.ps1` | the game's mappings (usmap) from your copy: the stand client headless, UE4SS `DumpUSMAP` -> `home\re\ue4ss` (needed by `ws dump`; no backend) |
| `play.ps1`, `gui2.ps1`, `client2.ps1` | live play: battle server + game windows; second client — `client2.ps1` |
| `portlock.ps1` | shared lock for the game ports |
| `dbtools.ps1`, `killmutex.ps1` | what the DbTools task runs with administrator rights (installed by `ops\machine\install-dbtools.ps1`) |
| `server_settings.py` | server mod settings without the launcher (`server_settings.ini` from the season) |
| `bench.ps1`, `startscensus.ps1`, `connwatch.ps1`, `loginseries.ps1` | measurements and analysis: servers on one PC, spawn points, connections, a series of logins |
| `dctest.ps1`, `rctest.ps1`, `rjtest.ps1`, `resetcycle.ps1`, `hostflow.ps1`, `wrproto.ps1` | test scenarios: disconnect, reconnect, rejoining a match, match cycle, waiting hall |

Every script has a header saying what it is for, what it needs and how to run it.
