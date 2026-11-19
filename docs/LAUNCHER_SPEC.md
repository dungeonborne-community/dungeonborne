# Spec: Dungeonborne launcher (Tauri 2 + Rust) — v1

Status: 2026-11-02, to be implemented in a separate session. Basis — `docs/SERVER_MANIFEST_DRAFT.md` (`dbserver/1`, `dbmod/1`).
The server side of the contract (section 4) is done by the backend session (`<repo>`, Rust, `src/`).

## 0. Why, and what is in v1

The game talks to fixed domains, the player's identity is currently a SteamID that the client claims itself (Goldberg,
changed with one line), the game transport is protected only by obfuscation. The game can no longer be obtained on Steam, so
**SteamID is not used as identity at all** (decision 02.11): a player = a Discord account, and the SteamID is a technical
`open_id` that the server issues and the launcher writes into the emulator before launch. There is no linking of old SteamIDs.
The launcher solves three problems:

1. **Discord login** — the server recognises the player by Discord, not by a claimed SteamID.
2. **Relay** — all game traffic goes through the launcher's local relay into a TLS tunnel to the selected server.
   The server knows the identity from the tunnel. Editing `hosts` is needed once at install, after that the server is selected in the launcher.
3. **Server profile** — manifest `dbserver/1`, exe check, mods `dbmod/1`, CA in `cacert.pem`, launch and cleanup.

Not in v1: relaying match UDP (DS — directly by the IP from the backend), the mod catalog and its signatures, game auto-update,
login via WebView2 inside the game (see section 9), macOS/Linux.

## 1. Facts about the game (verified on this machine)

| What | Value |
| --- | --- |
| Installs | `C:\Steam\steamapps\common\Dungeonborne` (main), `C:\Games\Dungeonborne` (second, for 2 clients) |
| exe | `Dungeonborne\Binaries\Win64\Dungeonborne.exe` (check the exact name), reference sha256 — `62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3` (from the manifest draft, re-check) |
| CA | `Dungeonborne\Binaries\Win64\cacert.pem` — the client verifies HTTPS against it |
| Paks | `Dungeonborne\Content\Paks` (UE mounts subfolders recursively!), override by name (`zz_` < `zzzzz_`) |
| UE4SS | `Dungeonborne\Binaries\Win64\ue4ss\Mods` |
| Steam emulator | Goldberg (both installs, no Steam client needed): `Binaries\Win64\local_save.txt` holds the profile name (`Save_Profile_2`); where the profile's SteamID and nickname live and how to set them — **to investigate** (task M6) |
| Domains | `sg.intlgame.com`, `intlgame.com`, `openapi.smartlink.intlgame.com`, `tdm.deltaverse-intl.com`, `f.tgf123.com`, `sg.api.tgf123.com`, `tgf123.com`, `dir.228918463-2-1.dmpplat.com`, `tconnd-7101-702.dmpplat.com`, `t01.tgf123.com` (source: `game/client-mod/install.ps1`) |
| TCP ports | 443 (HTTPS login/config), 80 (HTTP), 8085 (DirServer), 6601–6604 (TConnd lobby) |
| Match | UDP to the DS address that the backend hands out — bypassing the relay |
| Anti-cheat | ACE. Change game files **only with the game closed**; no injection, memory reading or process hooks |

The game runs with administrator rights (that is how the user has it) — the launcher must be able to start it with UAC.

## 2. Stack and structure

- Tauri 2, Rust backend (tokio, rustls, reqwest with rustls, serde, sha2, ed25519-dalek, keyring), UI — anything lightweight
  (Svelte/Solid/vanilla TS), Russian and English.
- Project folder: `<data>\launcher\` (a separate cargo project, not inside `backups`).
- Rust modules: `manifest` (dbserver/1, dbmod/1, TOFU), `auth` (Discord via the server), `relay` (local ports →
  tunnel), `install` (hosts, once, elevated rights), `profile` (mods, cacert, Goldberg), `game` (launch/exit).
- A separate small helper exe with administrator rights **only** for `hosts` (section 5). The main launcher
  runs without admin rights.
- A separate server mock is not needed: the backend with `tools/probe/auth_test/server.toml` (provider `dev`) implements section 4 (see §8).

## 3. Milestones (each verifiable on its own)

**M1. Servers and manifest.** Add a server by URL (`https://host/.well-known/dbserver.json`) or from a
`dbserverlist/1` list. Checks: `schema` major 1, `game.exe_sha256` = hash of the selected install (otherwise — refusal with
an explanation), the server key `key` is remembered on first connection (TOFU), on change — a warning and login is blocked
until confirmed. Show `status_url` (online, motd). Storage — `%APPDATA%\DungeonborneLauncher\servers.json`.

**M2. Login** (section 4.2). A login button per provider from the manifest (for our server — "Sign in with Discord"), the code
`user_code` on screen → system browser → the launcher polls the server → gets `session_token`. The token is stored in Windows Credential Manager (`keyring`), key — `server.id`. Show the nickname and
avatar (`account.display_name`, `avatar_url`), a "Sign out" button.

**M3. Relay** (section 4.4). On "Play" bring up listeners on `127.0.0.1` for ports 80, 443, 8085, 6601–6604;
each incoming TCP connection → a new TLS connection to the server's `relay` with a preface and token → bidirectional
byte pumping. Port busy → a clear error (who holds it — via `netstat`/`GetExtendedTcpTable`). The relay lives while
the game lives, plus 10 s. Log: time, port, bytes, duration, close reason — **no tokens**.

**M4. Install (once).** A helper with UAC writes into `hosts` a block between the markers
`# >>> dungeonborne-launcher` / `# <<< dungeonborne-launcher`: all domains → `127.0.0.1`. Foreign lines with these
domains outside the block — comment out with a note (there was a real bug: `198.51.100.1` lines sent login to someone else's
server). Back up `hosts` before the first edit. Uninstall — remove the block, uncomment our own lines. The helper does nothing
else.

**M5. Mod profile.** Download mods from `mods[]`, check the sha256 of the package and `files`, resolve `depends`/`conflicts`,
`risk.native_code` — explicit confirmation. Before launch lay them out: paks — into `Content\Paks\~dbl_<server.id>\`
with names by `priority` (the launcher adds the prefix itself), UE4SS mods — into `game\ue4ss\Mods`; inactive paks of other profiles —
outside `Paks` (`Content\_dbl_disabled\…`). Write `cacert.pem` = the game's original file + the server CA (check
`ca_sha256`), the original — backed up. After the game exits — restore everything. Compute `modset_sha256`.
Mod cache — `%LOCALAPPDATA%\DungeonborneLauncher\mods\<id>\<version>\`.

**M6. Launch.** Check that the game is not running; apply the profile; write into Goldberg the SteamID and nickname that
the server returned (`account.open_id`, `account.nick`) — the server issues a stable 17-digit open_id per Discord account
(its own on each server), it is also the save key; whatever was in the emulator before launch — restore after exit; start the exe (with UAC if needed); wait for the process to exit; remove the profile; stop the
relay. Two clients at once (party test): the second install, its own profile, a shared relay.

**M7. Developer mode.** Profile "local backend": no relay and no login, `hosts` already points to
`127.0.0.1`, the backend itself listens on 443/8085/6601 — the launcher only lays out mods/CA and starts the game.

## 4. Launcher ↔ server contract (both sessions implement it; change only by agreement)

### 4.1 Manifest — extension of `dbserver/1`

```json
{
  "auth":  { "providers": ["discord"], "api": "https://example.org/api/launcher/v1", "require": "relay" },
  "relay": { "address": "relay.example.org", "port": 7443, "cert_sha256": "…" }
}
```

Login is a framework extension point: the server chooses the providers itself (`[auth] providers` in `server.toml`), the launcher
does not know the provider and does nothing Discord-specific — it only shows a button by the provider name ("Sign in with
Discord" for `discord`, "Test login" for `dev`, otherwise "Sign in with <name>"). The first in the list is the default.
`require`: `relay` — the server will not let you in without the relay; `none` — a local server, no login needed (mode M7).
The relay preface (§4.4) is extended with the fields `caps` (what the launcher can do: `udp`) and `udp` (the connection is a match tunnel: frames `[length u16 LE][datagram]`, `port` does not matter). The launcher listens on `127.0.0.1:27777` and wraps the game's datagrams; the backend hands out the tunnel address only to players whose direct UDP is cut (`core/backend/src/auth/udp_tunnel.rs`), the rest play directly. Launcher 1.3.2+.

`relay.cert_sha256` — relay certificate pin (sha256 of the leaf certificate DER, hex). If `null` — normal verification
against root CAs (webpki-roots). `connect.address` is not used by the launcher when `relay` is present.

### 4.2 Login (provider secrets — only on the server)

1. `POST {api}/login/start` with body `{}` or `{ "provider": "discord" }` →
   `{ "login_id": "…", "user_code": "ABC-123", "provider": "discord",
      "browser_url": "https://example.org/auth/discord/start?login_id=…", "poll_interval_ms": 1000, "expires_in_s": 300 }`
2. The launcher shows `user_code` and opens `browser_url` in the system browser. The server runs the provider login
   (Discord — OAuth2, scope `identify`, with `guild_id` also `guilds.members.read`), the "Done" page shows the same
   `user_code` — the player compares it with the launcher (protection against a planted foreign login link).
3. `GET {api}/login/poll?login_id=…` → `{ "status": "pending" }` |
   `{ "status": "denied", "reason": "denied|not_in_guild|banned|error", "message": "text for the player",
      "link": "https://discord.gg/…" | null }` — show `link` as a button ("Join the server" for `not_in_guild`) |
   `{ "status": "ok", "session_token": "<64 hex>", "expires_at": 1793600000,
     "account": { "provider": "discord", "subject": "<id at the provider>", "display_name": "…", "avatar_url": "…" | null,
                  "open_id": "7656…", "nick": "…", "roles": ["player", "admin"] } }`
   A finished result is returned **once**, after that and after 5 minutes — 404 `expired`.
4. `POST {api}/session/refresh` (`Authorization: Bearer <token>`) → `{ session_token, expires_at, account }`, the old
   token is invalid immediately; `POST {api}/logout`; `GET {api}/me` → `account`.
5. Errors — HTTP 4xx with `{ "error": "code", "message": "text for the player" }`: `expired` (401/404), `banned` (403),
   `bad_provider` (400), `no_provider` (503), `busy` (429).
6. Server status: `GET <public_url>/dbserver/status` → `{ online, capacity, backend, modset_sha256, motd }`.

### 4.3 open_id

The server issues `open_id` on the first login of a Discord account: 17 digits in SteamID64 format
(`76561197960265728 + account number`), permanently bound to that account on this server. The in-game `user_id` comes from the server's own
counter, not from the open_id digits. There is no linking of old SteamIDs: test saves from before the launcher launch are not carried over
(if needed — manually by an admin).

### 4.4 Relay

- The launcher opens TLS 1.3 to `relay.address:relay.port` for each local TCP connection of the game.
- The first line from the launcher (JSON + `\n`, ≤ 4 KB):
  `{"v":1,"token":"<session_token>","port":6601,"modset":"<modset_sha256>","client":"dbl/1.0.0"}`
- Server reply (JSON + `\n`): `{"ok":true}` or `{"ok":false,"error":"expired|banned|bad_port|modset"}` and close.
- After `ok` — raw bytes in both directions, no framing. Half-close is propagated (shutdown write ↔ shutdown write).
- Allowed ports: 80, 443, 8085, 6601–6604. Preface reply timeout — 10 s. TCP keepalive is on.
- The server itself is responsible for making DirServer return the lobby address `127.0.0.1` (via the relay) for such connections.

## 5. Security (mandatory)

- The Discord secret, signing keys, other players' tokens — never in the launcher and never in logs.
- The token — only in Credential Manager; in memory — no longer than the session; never passed in a URL (except `login_id`).
- TLS — rustls, no "accept any certificate", even in debug: the relay is checked by the `relay.cert_sha256` pin; the web API over
  `http://` is allowed only for `127.0.0.1`/`localhost`.
- Manifest: TOFU key, `exe_sha256`, mod hashes are mandatory for `required`; a mod with `native_code` not from the catalog —
  only after an explicit "yes".
- The admin helper accepts only the commands `install-hosts` / `uninstall-hosts`, with no paths or arguments from outside.
- Anti-cheat: no injection, hooks or reading game memory; files — only with the game closed.
- The game's pak AES key (`<data>\aes_key.txt`) is not needed by the launcher and is not used.

## 6. Interface (minimum)

Server list (name, online, ping, tags) → server card (description, mods, button "Sign in with Discord" /
"Play") → launch screen (steps: check, mods, relay, game; log). Settings: game path (two installs),
language, developer mode, "Remove hosts edits".

## 7. v1 acceptance

1. Backend with the test config: add server `http://127.0.0.1:8090` → `dev` login (name on the page) → "Play" → the game reaches the lobby through
   the relay, the relay log shows connections 443/8085/6601.
2. Wrong `exe_sha256` → launch is forbidden with a clear message. Server key change → a warning.
3. After the game exits: `Paks`, `game\ue4ss\Mods`, `cacert.pem` — as before launch (hash comparison).
4. Port 443 held by a local backend → a clear error, developer mode still works.
5. There is no token in the launcher's logs and files (search by the token string).
6. Two clients from two installs at once — both in the lobby.

## 8. Server side — done 02.11 (backend session)

`core/backend/src/auth/` in the backend, config — `server.toml` (example `server.example.toml`):
- `provider.rs` — trait `AuthProvider` (extension point), `discord.rs` — our implementation, `dev.rs` — test login;
- `accounts.rs` — account (provider, subject) → open_id `76561197960265728 + n`, user_id `11 000 000 + n`;
- `sessions.rs` — tokens (only sha256 in the DB), pending logins; `web.rs` — API §4.1–4.2, manifest, status;
- `relay.rs` — TLS acceptor §4.4; `registry.rs` — connection identity: the gateway, DirServer (returns `127.0.0.1` for
  relay connections) and the game's HTTPS login take the player from here, the SteamID from the request is ignored; `require = "relay"` —
  direct connections are rejected.
- End-to-end check without the game: `tools/py/auth_e2e.py` with `tools/probe/auth_test/server.toml` (provider dev) — 20/20.
  **No mock is needed for the launcher**: the backend with this config is the mock (`dev` login by name).
- Full test with the game on one machine: the launcher takes `127.0.0.1:80/443/8085/6601–6604`, so the backend goes on another
  loopback address: `$env:BIND_IP='127.0.0.2'` and `tools/probe/auth_test/server_game.toml` (`[relay] upstream_ip = "127.0.0.2"`).

Not done: `modset` check (only written to the log, stage E5), mod packages in the manifest (`mods: []`, stage E2),
manifest signature (`key`/`signature`).

## 9. Later: login via WebView2 in the game

The client has `USDKWebviewService`: `OpenUrl`, `OpenUrlByExternalUI`, `Lua_OnWebViewResult` (the page's reply
goes to Lua), event `TGFSteamWebviewClosedEvent`. So there is a page → Lua bridge: a Discord login/linking page
can be opened from a Lua mod and the result returned to the game. Not needed for v1 — the launcher covers the task fully.

## 10. Backend session answers to "decided beyond the spec" (launcher README, 02.11)

Everything from the launcher README is accepted, below are clarifications. The backend has been adjusted for them where needed.

1. **`mods[].url`** — accepted: `"url"` — absolute or relative to the manifest URL; we'll also add `"size"` (bytes) for
   progress. Since 04.11 the backend serves client mods: `[mods] dir` (zip + `manifest_mods.json` from `launcher/tools/make_mods.py`), url — absolute `<public_url>/mods/<file>` (`core/backend/src/auth/mods.rs`).
2. **`modset_sha256`** — accepted as the contract definition: sha256 (hex, lowercase) of the lines
   `id@version:sha256` (package sha256), sorted by bytes and joined with a newline (LF), without
   a trailing newline. The backend computes it the same way (mod set check — E5; for now the value from the relay
   preface is written to the log).
3. **`session/refresh`** — the backend always returns `account`.
4. **Goldberg: `steam_settings\force_steamid.txt` / `force_account_name.txt`** — correct: the strings of these files are in
   `steam_api64.dll` of both installs (verified 02.11). Clarifications:
   - the emulator profile (open_id, nickname) — **the source of truth is in the launcher**: `%APPDATA%\DungeonborneLauncher\profiles\<server.id>\`,
     values come from the server at login (`account.open_id`, `account.nick`), files in the game folder — only for the duration of play;
   - the emulator's `steam_api64.dll` is a preinstalled part of the install, the launcher neither installs nor changes it;
   - there is no `steam_settings` folder in either install right now: if the launcher created it — delete it after exit; if it already
     had its own `force_*.txt` — save and restore them;
   - do not touch the emulator's global folder `%APPDATA%\Goldberg SteamEmu Saves\settings` (shared by all games).
5. **Pak names** `Paks\~dbl_<server.id>\zzzzzzzz_<priority:03>_<n>_<mod-id>_P.*` — accepted.
6. **Foreign paks** are moved away only with `allow_unlisted_client_mods: false` — accepted (our server has `true` for now).
7. **Native components are not installed in v1** — accepted. The UE4SS runtime (`dwmapi.dll` + `ue4ss\`) — like the emulator dll,
   a preinstalled part of the install: the launcher checks that it is there and installs only Lua mods into `game\ue4ss\Mods`. No UE4SS
   while the server has the `ue4ss` mod `required` — a clear error before launch.
8. **The manifest signature** is not checked — accepted, the backend does not sign yet.

Additionally:
- **exe**: hash `62d509a0…58b3` is `Dungeonborne\Binaries\Win64\Dungeonborne.exe` (identical in both installs).
  The root `Dungeonborne.exe` is a loader with a different hash (`7def8203…`), do not check it. The backend test configs
  (`tools/probe/auth_test/*.toml`) now have `exe_sha256` — "Play" is not blocked.
- **Bans are instant** (SurrealDB: `DEFINE EVENT` drops sessions, `LIVE SELECT` cuts the relay). The launcher must:
  on a relay drop mid-game — not reconnect silently, but check `GET {api}/me`; 401 → login screen; on a new login
  `login/poll` returns `denied` / `banned` with a message. The relay preface may answer `banned` or `expired`.
- **Login only for Discord server members**: `denied` / `not_in_guild` comes with `link` — show a button
  "Join the server" (invite link), then "Sign in again".
- **Live backend for `backend_live`**: `tools/probe/auth_test/server.toml` (provider dev) or `server_discord.toml`
  (real Discord), both on `http://127.0.0.1:8090`.

## 11. Launcher self-update (since 1.2.0, 04.11)

The manifest may carry `"launcher": {"version", "url", "size", "sha256", "signature"}` (backend: `[launcher] dir` with
`launcher.json` + exe, `core/backend/src/auth/release.rs`, served at `GET /launcher/<file>`). `signature` — ed25519 (hex) over the string
`"dbl-update/1
<version>
<sha256>
<size>"`, release key — `launcher/tools/release.py`; the public half is built into
the launcher (`builtin::RELEASE_KEY`). The launcher installs a build only if its semver is newer than its own version and the signature is valid; it does
not update during play. The server does not own the signature — a build cannot be swapped through the server.

## 12. Instance lobby in the launcher (spec and implementation 06.11)

Matchmaking as in FaceIt: the player picks an instance and votes in the launcher, and enters the match with the native "Start" button in the game. Enabled
by the flag `LAUNCHER_LOBBY=1`; without the flag everything is as now. Built on the open hall (`docs/WAITING_ROOM.md`) and the instance
orchestrator (`core/backend/ds-agent/README.md`).

### 12.1 How it plays

1. The launcher has 4 instance tabs. The player picks one; at any moment they are in only one.
2. The first to enter starts a **vote on the group size** (solo, duo, trio).
3. The size is decided and visible to everyone. Right away a **vote on the map** starts (all 6). During this time the player can assemble a group
   of the right size in the game or move to another tab.
4. The timer runs out: the IDs of everyone in the tab are recorded, the instance gets a job and opens the waiting hall for 5 minutes.
5. A tab can be entered while the hall is still open: the ID is recorded if there are slots.
6. A recorded player presses "Start" in the game themselves and gets into the hall. Whoever did not press is simply not in the hall: no penalty, the match goes on without them.
7. The hall always waits the full timer (WORKAROUNDS K18), then the match. After the match the tab is free again.

### 12.2 Maps and slots

Slots are computed by `matchmaking::max_teams`. There can be more recorded players than slots: slots are taken in the order of pressing "Start".

| level_id | Map | Solo | Duo | Trio |
|---|---|---|---|---|
| 40003015 | Dungeon, normal | 18 | 10 groups (20) | 6 groups (18) |
| 40003017 | Dungeon, Classic | 18 | 10 groups (20) | 6 groups (18) |
| 40003019 | Dungeon, Hard | 18 | 10 groups (20) | 6 groups (18) |
| 40003016 | Castle, normal | 14 | 8 groups (16) | 6 groups (18) |
| 40003018 | Castle, Classic | 14 | 8 groups (16) | 6 groups (18) |
| 40003020 | Castle, Hard | 13 | 8 groups (16) | 6 groups (18) |

### 12.3 Tab states

| Phase | What happens | Transition |
|---|---|---|
| `idle` | The tab is empty | First player entered: `vote_size` |
| `vote_size` | Vote on the group size, `LOBBY_SIZE_VOTE_SECS` | Everyone left: `idle`. Timer ran out: `vote_map` |
| `vote_map` | Vote on the map, `LOBBY_MAP_VOTE_SECS` | Everyone left: `idle`. Timer ran out: `starting` |
| `starting` | The room is created, the instance starts and loads the hall | The mod reported phase `waiting`: `waitroom` |
| `waitroom` | The hall is open, "Start" works | End of the hall timer: `battle`, nobody came: `idle` |
| `battle` | A match is running, no entry | Room closed: `idle` |

- **Vote result (since 09.11 — proportional):** an option wins with a chance equal to its share of the votes (3 against 1 — 75% / 25%),
  an option without votes does not win. **No votes:** a random option. Showing the chance in the launcher — with release 1.5.2.
- **The tab is logical:** any free instance is taken (`allocator::allocate`), so `failover` keeps working.
  Restarting the process after a match does not hold the tab: a new room will simply take another free instance.
- **State in memory,** like the matchmaker's: a backend restart resets the votes. Running matches live by their own rules.

### 12.3a Player minimum and matchmaking rules (08.11, user's decision)
- **Gathering (`gather`):** the tab waits for `LOBBY_MIN_PLAYERS` (default 4; 8 → 4 in the evening of 08.11) players and only then
  starts the group size vote. "Start" in the game during gathering — a window "Gathering players: X of N".
- **A started vote is carried through to the end** (08.11 evening, user's decision): if a player left or their launcher disappeared —
  the votes stay, a room opens on the result; from then on the minimum is enforced by the hall. The tab is reset only if everyone
  left. The log records who dropped out of the vote and why (left / launcher silent / maintenance).
- **Hall:** the battle server job carries `min_players`/`hold_secs`; the mod (RuServer `HallHold`) holds the native hall countdown at
  30 s while fewer than the minimum have entered, but no longer than `LOBBY_HALL_HOLD_SECS` (300) beyond the timer — then the match with those who
  came. The backend lets players into the hall all this time (`allocator::hall_timeout`).
- **Per-map rules** (`lobby_state.rules`): `skirmish` — Hard, groups of any size together; `gear: "epic"` —
  rarity limit (if `[gear_rarity]` is enabled in match_checks.toml); `challenger` — "challenge mode" for solo and duo against trio.
  The launcher shows a "Matchmaking rules" block and a hint on the map button. "Challenge mode" is also taken into account when entering
  the tab's trio room.

### 12.4 "Start" in the game with `LAUNCHER_LOBBY=1`

In `lobby_matching_search`, after the existing "Return to match" branch (it takes priority):

| Check | If not, an in-game window (`modal_message`, as in `battle_access_denied`) |
|---|---|
| The player is recorded in a tab | "Pick an instance in the launcher" |
| The tab is in `starting` or `waitroom`, the hall accepts (`hall_open`) | "The vote is still going" or "The hall has closed, wait for the next round" |
| The group size equals the chosen one, all its members are recorded in this tab | "A group of N is needed" or "Teammate is not in this instance" |
| There are slots | "No slots" |

The map is taken from the vote result, the map choice in the client is ignored. The result is written into `state.level_select` of all group
members: the server and the raid results take the map from there (`build_level_and_map_params`). The normal queue is not used.
The group stays an in-game group: the captain presses "Start" for everyone, as now. Groups of different sizes are not mixed (decision 03.11).

### 12.5 API (launcher session, `/api/launcher/v1/`)

- `GET lobby`: everything for the screen, the launcher polls it every 2 s.
  `{enabled, now, maps, sizes, instances:[{id, phase, ends_at, players, in_hall, seats, party_size, level_id, votes:{sizes:{"1":n}, maps:{level_id:n}}}], me:{instance, size_vote, map_vote, game_online}}`.
  Lobby disabled: `{enabled:false}`. Refusals: 409 with a code (`instance_battle`, `hall_full`, `hall_closed`, `bad_vote`, `vote_over`, `not_in_lobby`, `bad_instance`).
- `POST lobby/join {instance}`: enter or switch tabs, the previous vote is withdrawn. In `battle` — refused, in `waitroom` only if there are slots.
- `POST lobby/vote {party_size}` in `vote_size`, `POST lobby/vote {level_id}` in `vote_map`.
- `POST lobby/leave`: leave. Someone who left `waitroom` loses the right to enter.

`ends_at` in `waitroom` is counted from the `waiting` phase (`phase_since` + timer minus 25 s `hall_open`): the hall timer starts in the mod
when the hall is loaded, not at the moment of the vote result.

Closing the launcher does not unrecord the player immediately, so that a restart does not lose the slot: the record is removed if there is no poll for 30 s and
the game is not online (`hub::is_online`).

### 12.6 How this maps onto the orchestrator

Current state (`core/backend/ds-agent/README.md`, `DS_AUTOSCALE=1`, `DS_MIN_IDLE=1`, `standby = true`):
- An instance is a process for one match: it sits in the lobby without a map (`standby`), loads the hall only on a `host` job, after the match it exits
  (`exiting`), and ds-agent starts a new one.
- Not all 4 are running: only busy ones, `DS_MIN_IDLE` free ones and one more on demand.
- The mod is not bound to the hall roster: a job can be given with empty `members`. The mod starts the countdown (`StartWaiting`), receives new ones
  via `action: members`, by the end of the timer counts who entered. Nobody: `phase=empty`, the backend closes the room, the instance exits.
- The `waiting` phase is not treated as a hang in ds-agent (`TRANSITIONAL`: hosting, resetting, entering, exiting), so the agent will not interrupt
  a 5-minute hall, even though `stuck_secs` is also 300.

### 12.7 Backend changes

1. **`core/backend/src/lobby/launcher_lobby.rs` (new):** tabs, votes, transitions on the matchmaker tick, the list of recorded players.
2. **Room from the vote result:** create it with empty `members` and `party_size` from the vote (`capacity` and
   `option_args` depend on it). It gets a server the usual way: `WaitingServer`, then `allocate` in the tick, then a `host` job.
   One safeguard: `all_done` on an empty list returns true, and the tick would close the room immediately. Until someone has entered, closing only
   on `phase=empty`.
3. **Instance demand:** `want_flags` gets `demand` from `matchmaking::queued() > 0`, but the lobby has no tickets, and the second instance
   would not start. Add: there is a tab in `vote_size`, `vote_map` or `starting` without an instance. Demand is raised already from
   the start of voting, so the process has time to start by its end.
4. **Entering a specific room:** move the loop body out of `join_open_halls` into a shared function. Pools cannot be used: with two tabs with
   the same map and size a player would end up in someone else's hall.
5. **Hall timer:** `waiting_timeout` = `LOBBY_WAIT_SECS` only for lobby rooms. `DS_WAITING_TIMEOUT` does not fit, it
   applies to all rooms, including private ones.
6. **`web.rs`:** routes from 12.5.

Things to keep in mind:
- `hall_open` closes entry 25 s before the hall ends: the real window for "Start" is 4:35, the launcher shows exactly that.
- `DS_ROOM_TTL` (45 min) is counted from room creation, the hall and the instance start eat into it, and `battle_timeout` is 40 min.
  With a 5-minute hall the margin is almost zero: raise `DS_ROOM_TTL` to 50 min.

Flags: `LAUNCHER_LOBBY=1`, `LOBBY_SIZE_VOTE_SECS=30`, `LOBBY_MAP_VOTE_SECS=30`, `LOBBY_WAIT_SECS=300`, `DS_ROOM_TTL=3000`; already
set on prod `DS_WAITING_ROOM=1`, `MATCH_OPEN_HALL=1`, `DS_AUTOSCALE=1`.

### 12.8 Launcher

- A screen with 4 tabs, plain HTML + Tailwind + JS (`launcher/ui/app.js`), texts in `i18n.js`.
- Clicking a tab records the player in it right away (a tab in a match is only displayed). Since 1.4.2: a separate "Join" button at the bottom of the card was beyond the window edge, players clicked the tabs and were not recorded.
- Tab: phase and timer, players, first the 3 group sizes, then the 6 maps, with vote counters; your own vote is highlighted.
- In `starting` the caption "Server is starting", no timer. In `waitroom` a prominent "Press Start in the game" and a timer. If the game is not
  running, a "Play" button next to it.
- Tauri commands `lobby_state`, `lobby_join`, `lobby_vote`, `lobby_leave` (the session token stays in Rust), contract in `ui/api.d.ts`.
- The launcher polls `GET lobby`. An old backend without the lobby (error) or `enabled:false`: no card, so `mock-server` does not need changes.
- The sandbox `ui/dev/mock-tauri.js` shows all phases with real timer progress.
- New version via self-update. Old launchers do not see the lobby, so turn the flag on only after they have updated.

Without the launcher (local tests, analysis): GM command `lobby [join N | size N | map LEVEL | leave]`, for example
`python -I tools/gm.py "lobby join 1" --open-id <id>`. Local stand: `$env:LAUNCHER_LOBBY='1'` before `tools\stand\run_local.ps1`.
`tools\stand\run_lobby_dev.ps1` brings up a backend with dev login for the launcher, but the game server on this machine will not join it: the backend is on
127.0.0.2, and the server looks for it on 127.0.0.1.

### 12.9 Stages

0. **Check in game, without the launcher** (if it breaks, change the scheme before the main work):
   - a client with map A selected goes into a room with map B: does it travel, what does the UI show;
   - a 300 s hall, entering in the last minutes, late entry;
   - a hall nobody came to (`empty`), in `standby` mode: not checked in game;
   - Castle hall and match: not checked in the hall scheme, the PVPVE rules were tuned for the Dungeon.
1. Backend and tests: phases, votes, a tie, "Start" refusals, a room without members, two tabs with the same map, instance demand.
2. Launcher screen on mock-server.
3. Two clients locally, a full cycle: votes, hall, match, return to the tab.
4. Prod: update the launcher, then turn on the flag.

What was verified 06.11 on the local stand (DS + client of the second install, recording and votes via GM):
- an unrecorded player gets an in-game window on "Start" and does not enter the queue;
- size, then map; the vote result sets the map and options (duo on Dungeon Hard: `[1010302]`, 20 slots);
- a client with Classic selected gets into the Castle hall, stays there 2.5 minutes, moves to the match `L_GothicCastle_WP_Main`, the raid result is applied,
  the room is closed, the tab is `idle` again;
- the tab waits in `starting` while the server resets, then gets it;
- a hall without players closes 25 s before the end of the timer, the server is free after 16 s;
- the launcher screen in the sandbox (all phases) and the API with a real dev login.

Not verified: the real launcher window with a real backend and the game through the relay (locally the DS does not join when the backend is on 127.0.0.2, see 12.8),
a group of two or more players, two active tabs at once, starting a second instance via ds-agent on Robot, a match and the PVPVE rules on the Castle.

Prod 06.11 19:39: the launcher is published (now 1.4.1: build 1.4.0 was broken, `cargo test` replaced the exe with one without the embedded UI, pulled after 10 minutes), the flag `LAUNCHER_LOBBY=1` is on. Rolling back matchmaking: remove the flag from `/opt/dungeonborne/dbserver.env` and restart `dbserver`, the normal search comes back. Voting on prod with real players has not been verified yet.

### 12.10 Acceptance

- Two players in different tabs with the same map end up in different halls.
- Votes go in order: first size, then map, the map options match the chosen size.
- A recorded player who did not press "Start" is not in the hall, the match goes on without them, the tab is free after the match.
- Someone who entered the tab while the hall is open can press "Start" while there are slots.
- An unrecorded player gets an in-game window and does not enter the queue.
- A group enters as one team; a group of the wrong size is refused.
- Dropping out of a match: "Start" returns you to your own match.
- With 4 active tabs ds-agent brings up 4 instances, the extra ones shut down by `idle_secs`.
- Without `LAUNCHER_LOBBY` the matchmaker behaviour and tests do not change.

## 13. Server changelog (07.11)

The "Changelog" tab on the server page: what changed on the community server, per update. These are notes of the
**server**; changes to the launcher itself are in `launcher/PATCH_NOTES.md`, they do not go here.

- **File:** first `seasons/<SEASON>/patch_notes.toml` of the active season (via `SEASONS_DIR`, like the other season files),
  if there is no season or it has no file — the shared `patch_notes.toml` in the launch folder (`[patch_notes] file`). The 07.11 notes are the notes
  of the "Playtest" season: `content/seasons/playtest/patch_notes.toml`; there is no shared file in the repository, without a season the tab is empty.
  A season file with an error is not replaced by the shared one. Season notes come with the field `season {id, name}`, the launcher writes "Season "…"".
  TOML, not Markdown: an entry has fields, the backend checks them on read, the launcher shows the items as text without parsing
  markup. Entry: `[[update]]` with `date` (YYYY-MM-DD), `title`, an optional `version` and items in `new`, `fixed`, `balance`.
  A file with an error is not served (the reason is in the `[Notes]` log), it is re-read by itself without a restart.
- **Languages:** the main text is in the language `lang` at the start of the file (default `ru`). An entry's translation is a subtable
  `[update.lang.en]` with the same fields (`title`, `new`, `fixed`, `balance`); whatever is missing in the translation is taken from the main text.
  The launcher asks for its language (`?lang=en`), each entry in the reply has `lang`; an entry without a translation is marked "untranslated".
  The entry key (`latest`) does not depend on the language. On a language change the launcher requests the notes again.
- **API:** `GET /dbserver/patch_notes` → `dbnotes/1` `{latest, season?, updates[{id, version, date, title, sections[{kind, items}]}]}`,
  newest first; no file — 404 `no_patch_notes`. The path sits next to `/dbserver/status`: on the VM the proxy must pass it the same way.
- **Manifest:** optional `patch_notes: {url, latest}`; without a file — `null`. Old launchers do not read the field; a new launcher
  without the field writes "The server does not publish a changelog". A wrong link hides only the notes, the server keeps working.
- **"New":** a badge on the tab until `latest` from the manifest matches `Settings::notes_seen[id]`. Remembered when the tab
  is opened, only locally (the launcher's `settings.json`), not sent to the server.
- **Code:** backend `core/backend/src/auth/notes.rs`, `core/backend/src/auth/web.rs`; launcher `src-tauri/src/notes.rs`, commands `server_notes`, `notes_seen`,
  UI `renderTabs` / `renderNotes`. Checks: `cargo test auth::` (backend), `cargo test --lib` and
  `cargo test --test backend_live patch_notes -- --ignored` (launcher against a live backend), sandbox scenarios `ready`, `nonotes`.

## 14. ISP blocking workaround: match UDP tunnel at the player's choice (07.11)

With some Russian ISPs connections to the server freeze (memory note dungeonborne-ru-isp-freeze): the relay TLS stalls after
~1.4–2 KB and drops after about 20 s, game login fails with CURLCode 28, the lobby freezes and resets after ~30 s,
match UDP is cut after the first packets. The launcher notices this itself and offers the player the match tunnel through the relay
(`core/backend/src/auth/udp_tunnel.rs`, launcher port 127.0.0.1:27777).

- **Detection** (`launcher/src-tauri/src/netcheck.rs`): on each closed relay connection, the same as in `relay.log`.
  A "freeze" is a connection on 443 broken by an I/O error after 15–45 s, or on lobby 6601–6604 after 15–60 s
  and with less than 16 KB of data. The hint — if there are 3 or more of these within 5 minutes, spread over at least 45 s; once per relay start.
  Short game polls (↑524 ↓1494, `closed`) are not counted at all. A single burst of drops all within a couple of seconds
  (a server restart, on this machine 07.11: five in a row among dozens of normal ones) is not counted: that is not the ISP. Tests — on
  real log lines. The launcher cannot see match UDP without the tunnel: those players are caught by the server (`network_block_loading`).
- **Banner**: "It looks like your ISP is dropping the connection to the server. Enable the workaround?" + a tip about a VPN (WARP). Not
  shown if the tunnel is already on. "Not now" hides it until the next relay start.
- **Settings**: a "Network" section, a switch for each server where the player has logged in. The state is taken from the server; an old server
  without `/net` — "The server does not support this switch yet".
- **API** (`/api/launcher/v1/`, session token): `GET net` → `{udp_tunnel, capable}`; `POST net/udp_tunnel {on, reason}` → the same.
  Takes effect from the next match search: the battle server address is handed out at matchmaking.
- **Log**: a line `[Net] UDP tunnel on|off for <open_id> (<source>): <reason>` and a record in the table
  `udp_tunnel` (`on, source, reason, at`; source `launcher`, `gm`, `server`). At game login and in `GET net` the choice from the table
  takes precedence over the old `state.udp_tunnel`. To look: `SELECT * FROM udp_tunnel`.
- **Verified**: detection tests on log lines; an isolated backend — dev login, turning on, server restart
  (the choice persisted), turning off, wrong token; the launcher against it —
  `cargo test --test backend_live battle_tunnel_switch -- --ignored` with `DBL_TOKEN`; banner and settings — the UI sandbox
  (button "ISP drops the connection"). Not verified: a player with a real ISP from Russia, the real launcher window.

### 14.1 "We recommend WARP" banner and ISP reports (launcher 1.5.2, 11.11)

The match tunnel helps only with relay freezes. For a player whose ISP resets any TLS to Hetzner right after
ClientHello (one ISP, 08.11), nothing helps except a VPN or an entry point not through Hetzner (`docs/RU_INGRESS_PLAN.md`).
So now there is a single banner: "It looks like your ISP is restricting connections to our servers. We recommend installing
Cloudflare WARP (free) or any VPN and restarting the launcher." Buttons: "Install WARP" (https://one.one.one.one/
in the browser), "Enable the workaround for matches" (only with relay freezes, while the tunnel is off), "Copy report"
(version, what was found, the last lines of `relay.log` and the network lines of `launcher.log`; there are no tokens in the logs), "Close".
A closed banner is shown again on a new trigger.

- **Detection** — `launcher/src-tauri/src/netblock.rs`, a separate function `probe(client, target, control)` →
  `Outcome::{Blocked(Verdict{kind, detail}), Offline, Unreachable, Fine}`, the automatic switch to the RU entry point will reuse it.
  It runs when a request to the server (status, manifest, login start) failed at the network level, no more than once per 2 minutes per
  server. Steps: (1) `https://1.1.1.1/cdn-cgi/trace` — did not open → no internet, stay silent; (2) TCP to the server host — refused
  or no answer → the server is down or restarting, stay silent; (3) TLS with the server name — reset/drop in the handshake →
  `reset`; (4) the first 128 KB of a large server file (the launcher build from the manifest or a mod package) — a drop or 10 s without data
  before 128 KB → `freeze`. A 502/503 reply and maintenance do not start the check: the server is answering. Relay freezes (`netcheck`,
  §14) give `stall`.
- **Report to the server**: `POST /api/launcher/v1/net/report {kind, detail, at, version}` with the session token → a line
  `[Net] report <open_id> <provider:subject> "name" kind=<reset|freeze|stall> v=<version> <N> min ago: <detail>` in the log,
  without a database. No more than 20 reports per account per hour (429 `busy`). The launcher accumulates reports in
  `%LOCALAPPDATA%\DungeonborneLauncher\net_reports.json` (one per kind, no more than 20) and sends them when the server
  has answered a status request and the player has logged in: for a player with resets this happens only with a VPN. A server without the route (404) —
  the report is discarded; 5xx, no network or an expired session — it stays until next time.
- **Verified**: `netblock` tests with a fake server — freeze at 16218 bytes, reset after the first packet, full
  download, 502, no internet, server closed; a live run against prod from a working network — `Fine` in 0.9 s
  (`DBL_MANIFEST=... cargo test probe_live -- --ignored`); a test of the log line in the backend; the UI sandbox (buttons
  "ISP: relay drops", "ISP: TLS reset"). Not verified: a player with a real ISP, the real launcher window.

## 15. Server texts in the launcher's language (11.11)

Texts written by the server rather than the launcher come with translations, and the launcher shows them in its own language; if there is no
translation — the server's main language (`ru`). Type — `core/backend/src/text.rs` (`Text`): in TOML a string, as before, or a table of languages.

| Text | Where it is set | Field for the launcher |
| --- | --- | --- |
| server description | `server.toml` `[server] description` | manifest `description` + `description_i18n` |
| message of the day | `server.toml` `[server] motd` | status `motd` + `motd_i18n` |
| maintenance | `maintenance.txt`: main text, after the line `[en]` — the translation | lobby `maintenance` + `maintenance_i18n`; status `motd`/`motd_i18n` |
| season name | `seasons/<id>/season.toml` `name` | `patch_notes?lang=…` → `season.name` in the requested language |

- The old field is always a string in the main language: old launchers and the game read it as before. `*_i18n` — `{ru, en}`
  or `null` when there are no translations.
- `deploy/vm/maintenance.sh on "text" ["english"]` (and `restart [min] "text" ["english"]`) writes the file with `[en]` only
  if the backend already understands the translation (the status has `motd_i18n`); an old one would show the `[en]` line to players. Without English
  text — the default English. After a `restart` to a new backend the file is rewritten with the translation.
- The game (a chat message on "Start" during maintenance) still gets the main text.
- Verified: tests of `core/backend/src/text.rs`, maintenance, notes (147 backend tests in WSL); `maintenance.sh` on a fake status
  in WSL (old backend — without `[en]`, new — with it); the launcher sandbox (description, message of the day, maintenance in ru and en).
