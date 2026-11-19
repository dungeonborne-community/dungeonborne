# Technical format of seasons

Agreed with the orchestrator on 07.11, user decisions — 07.11 (section at the end). Code: `core/backend/src/season.rs`. Mod rules — [MODS_DESIGN.md](MODS_DESIGN.md). [FRAMEWORK.md](FRAMEWORK.md) describes the old separate copy with season plugins; this document is now the basis.

## Principles
- **A season is one folder `seasons/<id>/`.** Its own rules (a backend module) will appear when needed; for now seasons only have core flags.
- **One way to enable it on each layer:** backend — `SEASON=<id>`, build — the xtask active season, mods — the `season` field in the catalog.
- **One source for each list.** Only `mods/catalog.toml` knows the season's mod set, only the manifest knows the flags.
- **Without a season the base game runs** with flags from `content/seasons/base.toml`.
- Season inputs live in git, build outputs do not.

## Season folder
```
seasons/
  base.toml               base game flags (no season)
  active.txt              active season for xtask (cargo xtask season use <id|none>)
  <id>/
    season.toml           manifest (required)
    <config>.toml         module rules, once there is a module
    balance/*.json        tables the season overrides entirely (the rest — base balance/)
    loot/loot.toml        season loot source
    kit.toml              balance kit for designers (optional)
    patch_notes.toml      season change list for the launcher (proposed, see below)
    <id>.md               description for people
    pak/                  BUILD: season balance pak (zzzzzz_Balance_P)
    server/ExcelConfig/   BUILD: backend tables that differ from the original
    kit/                  BUILD: balance kits
```
Folders marked "BUILD" do not go into git (`.gitignore`). What is built for prod is pinned as a list with sha256, like `manifest.json` from `cargo xtask mods build`.

## Manifest `season.toml`
```toml
id = "playtest"
name = "Плейтест"
plugin = ""                  # backend module; there are no modules yet, a non-empty value = the season will not be enabled
state_key = "playtest"       # player progress key p.state[state_key]; the core does not touch it

[features]                   # core flags; whatever is missing comes from seasons/base.toml
relic_lower_quality = true
start_materials = [{ item = 1037, count = 5000 }]

[server]                     # what the season requires from battle servers; for reference
maps = []
```
- `id` must match the folder name, otherwise the season will not be enabled.
- An unknown field in `[features]` is a parse error (protection against typos), the season will not be enabled.
- The mod list is not written into the manifest: whether a pak or mod is seasonal is set only in the catalog.

## Core flags (`[features]`)
Value: from the active season's manifest, otherwise from `content/seasons/base.toml`, otherwise off.

| Flag | What it does | Client | Base | playtest |
|---|---|---|---|---|
| `auto_sell_extracted` | On extraction sells what was carried out in the bags (own items from before the raid and equipped items are not sold) | — | off | off |
| `relic_lower_quality` | Allows crafting a relic of lower quality | Pak `zz_RelicQuality_P` | off | on |
| `kill_reward = { currency, per_kill }` | Currency for killing a player, by mail after the raid (`POST /ds/kill`). `per_kill = 0` — off | — | off | off |
| `start_materials = [{ item, count }]` | Coins and materials once on login (`state.playtest_materials`) | — | none | forge materials |

A flag that needs a client pak is described in two places, and they must match: the flag in `[features]` and the pak in the catalog with `season = "<id>"`.

On start the backend writes one line: `[Season] active "Плейтест" (playtest, …); auto-sell off, lower relics on, …` or `[Season] no season — base game; …`. If the season was not enabled, the journal has the reason and the line about the base game.

## Enabling
| Layer | How | Without a season |
|---|---|---|
| Backend | Environment variable `SEASON=<id>`; `SEASONS_DIR` — another root (by default `seasons` in the backend working folder). Not duplicated in `server.toml` | Base game, flags from `content/seasons/base.toml` |
| Tables and loot | `cargo xtask season use <id>`; `balance build`, `loot build`, `kit` take the active one, `--season=<id|none>` — a different one once | Base `balance/` |
| Mods and paks | The `season = "<id>"` field on the seasonal part in `mods/catalog.toml`. `mods build` for `client`/`server` takes the base parts and the parts of the active season. `mods check` complains about a non-existent season | Base parts only |
| Launcher | Seasonal parts arrive as ordinary packages. The backend can return the active season name (`crate::season::active().id`) | — |
| UE4SS | The season has no entry point of its own. A place is reserved: the backend gives the battle server the season flags, RuServer reads them on start. Not now: RuServer is frozen | — |

The old ways `DEADMONEY`, `DEADMONEY_CONFIG`, `PLAYTEST` and the root `deadmoney.toml` were removed on 07.11.

## Server settings, not season settings
Remain in the environment (`dbserver.env`): `TRADE_SYSTEM_BUYOUT`, `TRADE_SYSTEM_AUCTION` (awaiting the user's decision), `DS_ROOM_TTL`, `LAUNCHER_LOBBY`, `DS_*`, `MATCH_*`, `RAID_*`, and also `match_checks.toml`.

## Core or season
- **Core:** a mechanism that more than one season may need. Code in `src/`, the switch is a flag in `[features]`.
- **Season:** rules that are not needed anywhere else. When they appear — a module `src/season/<plugin>.rs` with hooks in the core (example hooks — FRAMEWORK.md), the progress key — `state_key`.
- A season does not edit shared code or other `state` keys.

## Launcher change list (proposal)
The notes are part of the season: `seasons/<id>/patch_notes.toml`, the backend serves the active season's file, without a season — the root `patch_notes.toml`. The notes code is handled by the launcher session.

## State as of 07.11
Done (`dccf08d`, `80bea8f`):
- `core/backend/src/season.rs`: base.toml, manifest, `SEASON`, flags, tests.
- Flags instead of the old switches: auto-sell (`settle_extraction_items`), lower relics (`gear_heirloom_instantiation_impl`), kill reward (`register_kill`, counter `state.kills_pending`), materials on login (`player_login`).
- Dead Money removed entirely: stages, sets, stash cleanup, map closing, GM `dm`, `seasons/dead-money`, `deadmoney.toml`, `src/deadmoney.rs`. User decision: prod can be rebuilt and the DB reset without Dead Money.
- `content/seasons/playtest` describes prod; `tools/stand/run_local.ps1` — `SEASON=playtest` instead of `PLAYTEST=1`.
- Mod catalog (`cc369dc`, orchestrator): the `season` field, build and install take the parts of the active season, the season is written to `manifest.json`, `mods check` checks the season and `requires`. `pak.relic-quality` — `season = "playtest"`. The Jester (`pak.jester`, `ue4ss.jester`) — no season, `dev = true`: local development only, not installed for players or on Robot. With playtest the launcher packages are byte-for-byte the same as before; with `--season=none` the relic pak drops out.

Remaining:
- Prod: the steps in docs/FIX_LIST.md, "Before the next backend deploy" (`seasons/` on the VM, `SEASON=playtest`, remove `PLAYTEST=1`). Without this, prod will lose the forge materials and lower relics. Deploy and DB reset — the orchestrator, on the user's "yes".
- Season launcher notes — handed over to the launcher session.

## User decisions (07.11)
Not base game (season only): reward for killing a player, crafting lower relics, auto-sell of extracted items, forge materials on login, the Jester instead of the Reaper. The current season is `playtest`, it has no auto-sell. The Jester is not in playtest (user decision 07.11): it has no season of its own yet, it was not on prod.

No decision, behaviour unchanged:

| What | Where it is now |
|---|---|
| System vendors and auction | `TRADE_SYSTEM_BUYOUT`, `TRADE_SYSTEM_AUCTION` in the environment |
| Matchmaking checks | `match_checks.toml` |
| Red portals: 50% blue on disc 5 + one guaranteed | RuServer `Spect.RedTick` |
| First-floor purple portal to the throne, only blue ones on the second floor | RuServer, floor rules |
