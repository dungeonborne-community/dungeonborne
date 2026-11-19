# Plan: backend for the "server + manifest + mods" model (for SERVER_MANIFEST_DRAFT.md)

Status: plan, 2026-11-02. Basis — `docs/SERVER_MANIFEST_DRAFT.md` (`dbserver/1`, `dbmod/1`) and the current backend
(`backups/src`). Goal: the backend is **one engine**, and a specific server (`ru.dungeonborne.main`, someone else's community server)
is **config + a set of mods**. Now behaviour is hard-coded in code and env variables; mods exist only on the client and
are installed by hand.

## 0. What exists and what gets in the way

| Exists | Gets in the way of the manifest model |
|---|---|
| protocol: tgcp → gateway → `route_rpc` → 230 delegates → domain files | server behaviour (balance, deadmoney, XP, matchmaking, loot) — env and constants in code, not a "mod" |
| HTTPS on axum (`auth_http.rs`, 443), DirServer 8085, gateway 6601–6604 | no server identity (id, name, CA, mod set) and no manifest/status endpoint |
| `cargo xtask balance build` → `core/backend/ExcelConfig/*.bytes` (server) + `zzzzzz_Balance_P.pak` (client) | paks and UE4SS mods are laid out into installs by scripts, without packages and hashes |
| `GameData` from `ExcelConfig/` (+ `_rows`), `PlayerModel` in SurrealDB | player state is a `state` JSON with keys from different domains; load→modify→save **without locks** (9 files write `players` directly) — races in multiplayer |
| `set_rpc_error`, dirty pushes, hub | each handler builds the response, the error and `ScPlayerDirtyChgSync` itself — hence discrepancies and forgotten pushes |

## 1. Target layout

```
server/                          ← one directory = one server
  server.toml                    identity, connect, policy, capacity, motd, which mods are enabled
  mods/                          dbmod/1 packages (zip): client / server / both
  ca/                            the server's CA and TLS key (ca_cert in the manifest)
  data/                          DB, rpc_dump, logs
dbserver-rs                      engine: core + built-in modules
```

**Core** (doesn't change from server to server): transport, sessions, RPC routing, storage, errors, dirty sync,
manifest/status, mod loader.
**Built-in modules** (code in the engine, enabled and configured by mods/`server.toml`): account, role, inventory, item,
battle, raid, ds, lobby/team, room, progress (mail/quests/ratings), shop, social, guild, trade, misc.
**Server mods** (`dbmod/1`, `side: server|both`) — data, not code, in v1:

| `dbmod/1` component | What it does on the server | Example from the current state |
|---|---|---|
| `tables` | overlay of `core/backend/ExcelConfig/*.bytes` on top of the originals → `GameData` | balance `content\balance\tables\*.json`, loot `loot.toml` |
| `rules` | toml with module parameters (`[raid] exp_on_death = true`, `[role] attr_level = 20`) | env `ROLE_ATTR_LEVEL`, `MATCH_*`, `deadmoney.toml` |
| `pak` / `ue4ss` | client parts — only into the manifest (`side: client|both`) | `zzzzz_RebuildPrivate_P`, `DungeonborneRuClient`, R14 |

A `both` mod = tables for the server + a pak for the client from one source: exactly what `xtask balance build` already does.
Script mods (Lua/WASM on the server) — not in v1; we design module hooks so that they can be added later.

## 2. Stages

### E0. Inventory and criteria (before any refactoring)
- A list of what is broken/wrong after multiplayer tests: for each RPC — scenario, what was expected, what
  was received (logs `probe/logs` + `rpc_dump/OUR.jsonl` → `tools/py/probe_timeline.py`). A table in `docs/HANDLERS_STATUS.md`.
- Acceptance scenarios = automated runs of `tools\stand\run.ps1` (login, lobby, party of 2 clients, match → battle → results, mail, shop,
  trade). Each stage below ends with running them.
- Result: a prioritised list + baseline "before" runs.

### E1. Server identity, manifest and status
- `server.toml` instead of scattered env (env stays as an override): `id`, `name`, `description`, `region`,
  `languages`, `connect` (address, domains, ports, CA), `mode`, `policy`, `capacity`, `motd`.
- `manifest` module: builds `dbserver/1` from `server.toml` + client mods from `mods/` (id, version, sha256);
  `GET /.well-known/dbserver.json` and `GET /dbserver/status` (`online` from `lobby::hub`, `capacity`, backend version,
  `modset_sha256` per rule 6 of the draft) on the existing axum server.
- Manifest signature (`key`/`signature`, ed25519) — a placeholder field, enabled later.
- Result: the launcher (or curl) gets the manifest and status; nothing changes in the game protocol.
- **Done 02.11:** `core/backend/src/server_toml.rs` (sections `server`, `game`, `web`, `auth`, `relay`; without the file — previous
  behaviour), manifest and status in `core/backend/src/auth/web.rs` (`mods[]` — from `[mods] dir`, src/auth/mods.rs; no signature).
- **Player login — the core module `core/backend/src/auth`** (added to E1 on 02.11): the `AuthProvider` trait is the framework's extension
  point, a provider only answers "who is this"; accounts, sessions, bans, the launcher relay, connection identity are
  the core. Providers: `discord` (our server's implementation), `dev` (local tests). SteamID is not an identity:
  the server issues the open_id. Contract with the launcher — `docs/LAUNCHER_SPEC.md` §4, check — `tools/py/auth_e2e.py`.

### E2. Mod packages and loader
- **Done 04.11 — serving client mods to the launcher** (`core/backend/src/auth/mods.rs`): `[mods] dir` in `server.toml` (default
  `mods`) with zip packages and `manifest_mods.json` from `launcher/tools/make_mods.py`; size and sha256 are checked on load,
  the manifest's `mods[]` — with absolute urls `<public_url>/mods/<file>`, files — `GET /mods/<file>`, in the status
  `modset_sha256` of the required mods; the list is re-read when `manifest_mods.json` changes. Packing — for now by the launcher's
  script (`xtask mod pack` below is not done), server components (`tables`, `rules`) — not yet.
- `xtask mod pack <folder>` → `dbmod/1` zip: `mod.json` + files + hashes. Pack what exists: `ru.rebuild-private`
  (UnLua pak), `ru.balance` (both: tables + `zzzzzz_Balance_P`), `ru.client-hooks` (DungeonborneRuClient),
  `ru.r14-fix`, `ru.client-probe` (diagnostics, required: false).
- Loader at start: reads `mods/*.zip`, checks schema and hashes, `depends`/`conflicts`, order by `priority`;
  `tables` → `GameData` overlay; `rules` → module config; client components → only into the manifest.
- Serving packages: in the draft `mods[]` has no `url` — add it (`"url"` or `mods_base_url`), otherwise the launcher has nothing
  to download. Fix the draft.
- Result: balance and rules change by replacing a mod, not by editing code/env; the manifest lists real packages.

### E3. Core: modules, player state, errors, sync (the main rework)
1. **Module registry.** `trait Module { id, enabled(cfg), configure(rules), on_login, on_logout }`; the delegates in
   `handlers.rs` stay (a generated trait), but go through the registry: module disabled (`guild` — no UI in the client)
   → a safe empty response. Module = domain file(s), already split out.
2. **Player storage with locking.** `PlayerStore::with(open_id, |p| …)` — serialised load→modify→save per player
   (a mutex per open_id in the process; with several replicas — document version + retry). Move all 9 places of direct
   `db.save("players")` and `load_me/save_me` onto it. Actions on **another** player (the host reports for
   clients in `report_action`, trade, gifts, mail) — only via their `with`.
3. **Typed state.** `PlayerModel.state` (free-form JSON) → per-module sections (`MailState`,
   `MarketState`, …) with a version and migrations (generalise `affix_schema_ver`). Old keys are read by a migration once.
4. **Errors.** A handler returns `Result<Rsp, GameError(EnErrorCode)>`; the core itself puts the code into the response's `ret_code`/
   `result_code` or into `RpcHead.error_code` (per the field description in the proto) and logs it.
5. **Changes → dirty push.** `PlayerTx` accumulates changes (items, coins, roles, …); after the response the core sends one
   `ScPlayerDirtyChgSync` to the owner (and to other affected players via the hub). Removes manual assembly in every handler
   and duplicated item layout (one `inventory::give/take`).
6. **Rules instead of constants.** Everything that is now env/behaviour constants (`ROLE_ATTR_LEVEL`, XP on death, MATCH_*,
   DS_*, deadmoney, insurance, limits) becomes module `rules` fields with defaults = current behaviour.
- Order: first 2 (races are the likely cause of "works wrong" in multiplayer), then 4–5, then 1, 3, 6.
- Result: behaviour unchanged, the E0 scenarios pass no worse; no player write races.

### E4. Fixing handlers per the E0 list
- One module at a time, in game-flow order: account/get_info → inventory/item → battle/raid/ds (match results) → lobby/team/
  match → progress (mail, quests) → shop → social → room → trade; guild — last (no UI).
- For each RPC with a reference (`rpc_dump` PVE, sniffer) — a **contract test**: our response to the reference request against
  the reference response on a set of fields (`tools/py/rpc_evidence.py` provides the data). Without a reference — an automated-run scenario +
  `probe_timeline.py --fields` (which fields the client reads but we don't send).
- Result: green E0 scenarios, a status table in `HANDLERS_STATUS.md`.

### E5. The player's mod set at login
- The client doesn't report its mod set. Option: the launcher puts `modset_sha256` into the login data (a field in the
  `auth_http`/`login` request that the client passes as is — to be found), the backend compares it with the manifest when
  `policy.allow_unlisted_client_mods = false`; otherwise only a log. Needs research of the login protocol.
- Result: the server rejects a login with a foreign mod set with a clear error (or logs a warning).

### E6. Operations
- Several backend instances (GKE): shared storage (SurrealDB server mode instead of a local kv), player locking
  via the document version, a hub between replicas (or a "sticky" replica by open_id).
- DS pool via ds-agent (`core/backend/src/ds` already exists), server registration, release by results.
- Per-module metrics/logs, `status_url` with real capacity.

## 3. What we don't touch
- The generated protocol (`generated/`, `route_rpc`), tgcp transport — the core already works.
- Client mods as such — only their packaging changes.
- The launcher — a separate project; the backend gives it the manifest, status and packages.

## 4. Decisions needed before starting
1. The list of what's broken from multiplayer tests (E0) — from the user/test sessions.
2. v1 server mods — data only (`tables` + `rules`), no scripts? (recommendation: yes)
3. Where the packages for the launcher live: on this same server (`/mods/<id>-<ver>.zip`) or a catalog? (recommendation: on the server,
   URL in the manifest)
4. One backend instance per server for the near term (in-process locking) or several right away (GKE)?
   (recommendation: one, make the locking so that we can move to the document version without rewriting handlers)
