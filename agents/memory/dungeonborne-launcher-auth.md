---
name: dungeonborne-launcher-auth
description: Decided 02.11 — login via Discord through a Tauri launcher + TLS relay; spec docs/LAUNCHER_SPEC.md; in-game WebView2 facts
metadata:
  visibility: team
  node_type: memory
  type: project
---

Decided 2026-11-02: player identity = Discord account, verified server-side; the launcher (Tauri 2 + Rust, separate
session, folder `<data>\launcher\`) does OAuth via system browser + polling, then runs a local relay
(127.0.0.1:80/443/8085/6601-6604) that tunnels each game TCP connection over TLS to the server with a JSON preface
carrying the session token. SteamID is NOT an identity at all (game no longer obtainable on Steam): the
server issues a technical 17-digit open_id per Discord account (76561197960265728 + n) and its own user_id counter;
the launcher writes it into Goldberg; no linking of old SteamIDs. Contract (manifest `auth`/`relay`, `/api/launcher/v1/*`, relay preface) is in
`docs/LAUNCHER_SPEC.md` section 4 — change only in agreement with the launcher session.

**Launcher v1 implemented 02.11** in `<data>\launcher` (see its README): user chose trunk + plain JS +
Tailwind v4 (no Leptos, no npm). Single exe, tauri `compression` off so the UI is plain text inside; hosts helper =
same exe with `--hosts-helper install|uninstall`. Goldberg identity via `Win64\steam_settings\force_steamid.txt` /
`force_account_name.txt` (overrides both installs' profiles). Launcher-side additions the backend must match:
`mods[].url`, modset = sha256 of sorted `id@version:sha256` joined by `\n`, refresh returns
`{session_token, expires_at, account?}`. Mock `launcher/mock-server` + ignored e2e `src-tauri/tests/mock_e2e.rs`.
Backend replied in LAUNCHER_SPEC.md §10 (all accepted); launcher implements it: emulator profile in
`%APPDATA%\DungeonborneLauncher\profiles\<server.id>\profile.json`, UE4SS runtime check, relative `mods[].url` + `size`,
relay drop -> `GET /me` -> login screen, denied `not_in_guild` with `link` button. No npm/node anywhere: trunk
pre_build hook runs `tailwindcss` (standalone CLI) from PATH.
04.11 network-mods plan agreed: exe bundle keeps only runtime (UE4SS core, Goldberg, cacert), paks + Lua mods become
per-session `dbmod/1` packages from the backend (E2). Step 1 verified in game: IoStore paks renamed launcher-style and
placed in `Paks\~dbl_*\` load fine. Step 2: `launcher/tools/make_mods.py` + `tools/mods.json` -> 11 packages in
`launcher/dist/mods` (+ `manifest_mods.json`). Step 3: backend `core/backend/src/auth/mods.rs`, `[mods] dir` (VM:
/opt/dungeonborne/mods), `GET /mods/<file>`, status `modset_sha256` of required mods. Step 4 done (launcher 1.2.0):
runtime-only bundle with `retired` list, streamed parallel mod downloads, cache GC, native consent per server+build,
manifest refresh before Play, self-update signed with ed25519 release key (secret
`<data>\deploy\release\release_key.secret`, public in builtin.rs; `tools/release.py sign`; backend
`[launcher] dir` + `core/backend/src/auth/release.rs`; spec §11). Not deployed to VM; 1.2.0 must be handed out manually once.
Verified in game 04.11 on C:\Games\Dungeonborne (launcher's install) via local backend `tools/probe/auth_test/server_game.toml` +
`BIND_IP=127.0.0.2` (relay on 127.0.0.1): migration removed 1785 old bundle files, 10 mods downloaded/laid/cleaned,
lobby OK. Pre-test copies of E:/F: mods: `<data>\launcher_test_backup\`.
Steam "Play" -> launcher (bootstrap.rs, setting `steam_bootstrap`): launcher copy replaces root Dungeonborne.exe
(UE BootstrapPackagedGame, sha 7def8203…), original kept as Dungeonborne.bootstrap.exe; single-instance mutex.
User verified 04.11: works, no SmartScreen, ACE fine. Player-facing release notes: `launcher/PATCH_NOTES.md`
(unpublished; user posts to Discord themselves — see [[dungeonborne-no-discord]]).
Community designs (04.11): `ui/README.md` guide, `ui/api.d.ts` contract, browser sandbox `ui/dev/mock-tauri.js`
(inert inside Tauri; scenarios via `?scenario=`). index.html uses plain tags + trunk `copy-file`. When adding a command
or event: update api.d.ts and the mock too. Live server 203.0.113.10 has `require = "relay"`: starting the game without the
launcher can't log in there — for direct-start tests use the local backend (`target\release\dbserver-rs.exe`, no
server.toml = legacy) with hosts block on 127.0.0.1.
G: is nearly full (~4.5 GB free) — put test builds on C: via CARGO_TARGET_DIR.

**Why:** today the server trusts the SteamID the client declares (Goldberg, trivially changed) and `LOCAL_FAKE_TOKEN`
is shared; TGCP AES is obfuscation (key sent in clear in 0x1002). Real Steam tickets can't be verified (no publisher key).

**How to apply:** backend side DONE 02.11 in `core/backend/src/auth/` (framework core module: `AuthProvider` trait = extension point,
`discord.rs` is just our provider, `dev.rs` for tests; accounts/sessions/web/relay/registry) + `core/backend/src/server_toml.rs`;
no server.toml = legacy behaviour. Keep it provider-agnostic (user insisted: "we have a framework"). E2E test without game:
`tools/py/auth_e2e.py` against `tools/probe/auth_test/server.toml` (20/20). In-game WebView2 (`USDKWebviewService.OpenUrl/OpenUrlByExternalUI`,
`Lua_OnWebViewResult` → Lua, `TGFSteamWebviewClosedEvent`) is a later option, not v1. See [[dungeonborne-dedicated-plan]].

**Two Discord servers (03.11):** `Discord/setup_server.py` + `Discord/.env` and `bot.toml` target the TEST server
Sirius.ai (100000000000000107); the production community server "Otryad Kotyat" ("Kitten Squad", 100000000000000101) is in
`bot.prod.toml` and the backend auth config (`tools/probe/auth_test/server_discord.toml`, guild_id). Never run
setup_server.py against prod without the user. Login whitelist: `[auth] login_roles = ["whitelist"]` + Discord role id →
"whitelist" in `[auth.discord.roles]`; refusal code `not_whitelisted` (web login, /me+refresh, relay). Role "Whitelist": prod "Otryad Kotyat"
= 100000000000000106 (already existed, mapped in server_discord.toml), test Sirius.ai = 100000000000000108.

**Launcher v1.1 (03.11):** built-in server `https://203.0.113.10/.well-known/dbserver.json` (src/builtin.rs, auto-added,
retry every 60 s); no domain → leaf cert pinned by sha256 (`<data>\deploy\tls\backend.{crt,key}`, key never
printed; deploy/tls/README.md lists what the server needs: proxy 443→8090, relay with same cert, DS_POOL 203.0.113.20:7777@*).
Client files embedded: `launcher/bundle/client.zip` from `tools/make_client_bundle.py` (all mods as in F:, UE4SS consoles
off) laid into the game before Play (src/bundle.rs). Output `<data>\dist\DungeonborneLauncher-<ver>.exe`.
Build `cargo tauri build` from PowerShell (Git Bash trunk can't find tailwindcss.exe).
Prod login whitelist removed 04.11 (user): /opt/dungeonborne/server.toml `login_roles = []` (backup server.toml.bak-whitelist; deploy/vm/server.toml too). Discord guild membership still required (require_guild); gm/admin roles unchanged.
