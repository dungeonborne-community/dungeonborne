# Dungeonborne Launcher

Tauri 2 + Rust; the UI is plain HTML/JS + Tailwind v4 (standalone CLI), built by `trunk`. No npm, no node.
Spec: `docs/LAUNCHER_SPEC.md`.

```
launcher/
  src-tauri/          the launcher (Rust)
    src/manifest.rs   dbserver/1 + auth/relay, dbserverlist/1, status
    src/store.rs      settings.json / servers.json, TOFU of the server key
    src/auth.rs       Discord login on the server side (4.2)
    src/creds.rs      the token — only in Windows Credential Manager
    src/relay.rs      local relay 127.0.0.1:80/443/8085/6601-6604 -> TLS 1.3 + preface (4.4)
    src/hosts.rs      the block in hosts; the same exe with `--hosts-helper install|uninstall` under UAC
    src/profile.rs    dbmod/1, laying out paks/UE4SS, cacert.pem, Goldberg; journal + rollback
    src/game.rs       launch (UAC), waiting for exit
    src/bugreport.rs  "Report a bug": log tails, versions, crash flag (contract bug-report-api.md)
    tests/mock_e2e.rs e2e against the mock
  ui/                 index.html, app.js, i18n.js, input.css (Tailwind), Trunk.toml
  mock-server/        mock of the server side of the contract (axum)
```

## Your own UI design

For designers — `ui/README.md`: the contract with the launcher (`ui/api.d.ts`), a browser sandbox without Rust
(`ui/dev/mock-tauri.js`, scenarios `?scenario=`), required states, CSP rules, how to submit.
`index.html` includes files with plain tags (trunk only copies them, `copy-file`), so it opens without trunk too.

## Build

```
cd src-tauri
cargo tauri dev      # trunk serve + window
cargo tauri build    # -> target/release/dungeonborne-launcher.exe (a single file, bundle off)
```

Required: Rust (MSVC), tauri-cli, trunk and `tailwindcss` (standalone CLI v4) **in PATH**: before every build trunk
calls `tailwindcss -i input.css -o styles.css --minify` (hook in `ui/Trunk.toml`) and downloads nothing. All three
are installed by `cargo xtask setup --apply` from the workspace root into `home\tools` (versions — `config/tools.lock`; no `cargo install`
needed). PATH to them in a PowerShell window is set by `. <home>\env.ps1` (written by `bootstrap.ps1`; `<home>` is DB_HOME,
`..\home` by default). By hand: `$env:PATH = "<home>\tools\tauri-cli\bin;<home>\tools\trunk\bin;<home>\tools\tailwindcss;$env:PATH"`.
Start the build with `cargo tauri build` in `src-tauri` (it first builds `ui/dist` via trunk), not with `cargo build`.
The environment variable `NO_COLOR=1` breaks trunk (`invalid value '1' for '--no-color'`) — unset it or set it to `true`.

## Built-in server and client files (v1.1)

- **The community server** `https://203.0.113.10/.well-known/dbserver.json` is added by itself on first launch
  (`src/builtin.rs`); while it is unreachable — a retry once a minute. Once the player removes it, it is not added again
  (`builtin_added.json` next to `servers.json`).
- **No domain** — https and the relay are checked by a certificate pin: sha256 of the leaf DER in `builtin::CERT_SHA256`
  (the server certificate and key are with the owner, not in the repository). Other servers — as before,
  a CA chain; the pin from the manifest (`relay.cert_sha256`) is still strict. Changed the certificate on the server —
  new pin in `builtin.rs` and a new build.
- **Only the runtime is embedded in the exe** (since 1.2.0): `bundle/client.zip` = the UE4SS core (`dwmapi.dll`, `ue4ss\UE4SS.dll`,
  settings, built-in `BPModLoaderMod`/`BPML_GenericFunctions`/`Keybinds`/`shared\UEHelpers`, its own `mods.txt`),
  Goldberg `steam_api64.dll` + `steam_appid.txt`, `cacert.pem` with the backend CA. UE4SS consoles are off. Before every
  "Play" (after the exe check) only files with a different sha256 are written; the original is copied once before replacement to
  `%LOCALAPPDATA%\DungeonborneLauncher\bundle_backup\<install>\`. Paks and Lua mods are network packages (below).
- `retired` in `bundle.json`: files of earlier sets (old paks, Lua mods, `CheatManagerEnablerMod`…) with their sha256.
  The launcher deletes such a file only if it is unchanged; in developer mode it deletes nothing (your own mods).
  The set of past versions is taken from the previous `bundle/client.zip`, so build the new one over the old one.
- Rebuild the set (game closed): `python tools/make_client_bundle.py --src C:\Games\Dungeonborne`, then
  `cargo tauri build`. Without `bundle/client.zip` the launcher does not build (`include_bytes!`).
- Run the build from PowerShell: from Git Bash trunk does not find `tailwindcss.exe` in PATH.

## Network mods (`dbmod/1`, since 1.2.0)

Paks and Lua mods are handed out by the server as `dbmod/1` packages; the launcher installs them for the game session (`profile.rs`): before "Play"
it refreshes the manifest, downloads missing packages (streamed to disk, 3 at a time, progress in the card), checks the hashes,
lays them out into `Paks\~dbl_<server>\` and `ue4ss\Mods\`, and removes them after exit. Cache —
`%LOCALAPPDATA%\DungeonborneLauncher\mods\`; versions that are in no manifest are deleted at start and
before a game, by hand — "Settings → Mod cache → Clear". Native code (`risk.native_code`) is asked about once
per server and package build (`%APPDATA%\DungeonborneLauncher\native_ok\<server>.json`).
Package contents — `tools/mods.json`, build:

```
python -I tools/make_mods.py --src C:\Games\Dungeonborne      # -> dist/mods/
```

- `dist/mods/<id>-<version>.zip` — packages (deterministic zip), `index.json` — build state (keep it next to them,
  by it the version changes only for packages that changed), `manifest_mods.json` — a ready `mods[]` for the server
  manifest (`url` like `/mods/<file>` — from the server root).
- Version — `YYYY.MMDD.HHMM` of the build moment (semver). `priority` keeps the old name order:
  `zz_` 10 < `zzzzz_` 50 < `zzzzzz_` 60 < BPFix 70 < ChatFix 71.
- Not in packages (development only): `CheatManagerEnablerMod`, `CrashTracer`, `NameDump`, `DSTest`,
  the server-side `DungeonborneRuServer`, types `shared/types`. `ru.client-probe` is optional.
- `ru.client-hooks` carries `dbnative.dll` → `risk.native_code: true`.
- Checking the packages with the launcher's code (download, check hashes, lay out into a fake install, roll back):
  `python -m http.server 8799 --bind 127.0.0.1 --directory dist`, then
  `cargo test -p dungeonborne-launcher --test mods_dist -- --ignored`.
  Against the backend: `DBL_MANIFEST=http://127.0.0.1:8090/.well-known/dbserver.json` (the test config
  `tools/probe/auth_test/server.toml` serves `../launcher/dist/mods`).
- Serving: the backend takes packages from `[mods] dir` (`core/backend/src/auth/mods.rs`), deploy to the VM —
  `ops/deploy/vm/README.md`, section "Client mods".

## Launch from Steam: the launcher instead of the game bootstrap (`bootstrap.rs`)

The root `<game>\Dungeonborne.exe` is UE's `BootstrapPackagedGame` (sha256 `7def8203…`, signed by Proxima Beta); it is
started by "Play" in Steam (appid 2448970). The setting "Open the launcher with Steam's Play button" puts
a copy of the launcher in its place, the original — into `Dungeonborne.bootstrap.exe` (untick it — restored).
- The launcher recognises its copy by the marker `DBL-LAUNCHER-SELF:<version>;` in the exe, the original by sha256; a foreign file is left alone.
- At start (if the setting is on) the launcher puts itself in place again: after "Verify integrity" Steam restores
  the bootstrap; an old copy of the launcher is replaced with the new one.
- Started as `<game>\Dungeonborne.exe`, the launcher adds this install to the settings by itself, highlights its
  "Play" and passes Steam's arguments to the game. The copy is written by the launcher itself (without the "from the internet" mark) —
  SmartScreen is not shown when starting from Steam.
- One instance (mutex `Local\DungeonborneLauncher`): a repeated "Play" brings the open window to the front.
- The yellow UAC window ("Publisher: Unknown") remains only for the one-time `hosts` edit — before it the launcher explains
  what it is. Removing it completely is only possible with code signing. The game (`Win64\Dungeonborne.exe`, signed by Proxima Beta,
  `asInvoker`) with "Run as administrator" gives the ordinary blue prompt.

## Launcher self-update (since 1.2.0)

The server offers a build in the manifest (`launcher`: version, url, size, sha256, signature); the launcher takes it only
if the version is newer and the signature matches the release key embedded in the exe (`builtin::RELEASE_KEY`). The server can hand out
updates but cannot make them. The banner "A new version is available" → the exe is downloaded next to the current one, the current one
is renamed to `.old.exe` (deleted on the next start), the new one is started. During a game the update is
blocked (the relay lives in the launcher). A launcher folder without write access — "download manually".

Releasing a version:
```
# 1. version in src-tauri/Cargo.toml and tauri.conf.json, then cargo tauri build (from PowerShell)
python tools/release.py sign --key <key folder>\release_key.secret   # -> dist/launcher (signed by the owner)
# 2. dist/launcher/* -> the server's [launcher] dir (owner: ops/deploy/vm/README.md)
```
- Key: `release_key.secret` (ed25519 seed, hex) — only with the owner, outside the workspace. Not in the repository and not on the
  server; losing it means releasing a launcher with a new key and handing it out manually. Make a copy.
- Run `tools/release.py` with plain `python` (without `-I`: `cryptography` is in the user site-packages).
- Check (Python ↔ Rust signature, download, exe swap): sign a test version (`--version 1.2.1-e2e`),
  backend with `probe/auth_test/server.toml`, `DBL_MANIFEST=... cargo test --test update_live -- --ignored`, then
  **delete `dist/launcher`** — the test build must not reach the server.
- 1.1.x cannot update: hand 1.2.0 out to players manually once.

## Mock and tests

```
cargo run -p dbl-mock-server -- --demo-mod          # http://127.0.0.1:8787, relay 127.0.0.1:7443
cargo test --workspace -- --include-ignored --test-threads=1
```

In the launcher: add the server `http://127.0.0.1:8787/.well-known/dbserver.json` (http is allowed only for
127.0.0.1). The "Mock Discord" page in the browser replaces Discord: nick = account.
Without `--upstream` the mock relay echoes bytes back; `--upstream 127.0.0.1 --port-offset 10000` — forward
to a local backend listening on 10443/18085/16601…

## Report a bug (`bugreport.rs`)

Contract — `docs/specs/bug-report-api.md`. The "Report a bug" button on the server page (if the server
has login) → form: type `crash` / `game` / `launcher`, title up to 100 characters, description up to 1500 →
`POST <auth.api>/bug_report` (multipart: `report` JSON + files) with the player's session → polling `GET bug_report/{id}` every
5 s for three minutes → a link to the bot's post in Discord (in the window and in a pop-up message). 429 → "no more than three per hour".
- Files — tails, always whole lines: `launcher.log`, `relay.log` (up to 2 MB), `crash_ue4ss.log` of the install
  `installs[0]` (up to 2 MB), `%LOCALAPPDATA%\Dungeonborne\Saved\Logs\Dungeonborne.log` (up to 3 MB); up to 9 MB together
  (the largest are cut). The server takes text only: UTF-16 is re-encoded to UTF-8, NUL bytes are removed.
- `versions.mods` — the modset of the last launch on this server (`last_play.json` next to the logs), otherwise — from the manifest's
  `mods[]`. `versions.game` — `manifest.game.label`; also `exe_sha256`, `os`, the launcher's `lang`.
- "The game crashed" is preselected if the game is not running, `crash_ue4ss.log` is younger than a day and either the game's exit code
  is non-zero (`last_play.json`) or there is no `Log file closed` at the end of `Dungeonborne.log`.
- `contact` from the manifest (https only) — a "Discord" button next to "Website".
- **The "SUPPORT" button in the game** (UnLua pak, LOCAL session). While a game started by the launcher is running, the launcher every
  5 s writes `%LOCALAPPDATA%\Dungeonborne\Saved\launcher_alive` (`{"ts","pid","version"}`; alive — `now − ts < 10 s`),
  and deletes it when the game exits. Once a second it picks up and deletes `Saved\bug_report_request.json`
  (`{"source":"game","scene","role","ts"}`, no older than 5 min) → launcher window to the front, event `bug_request` → form with
  `kind=game`, `source=game`.
- **After a crash.** The game exited and `crash_likely` (rule above) — the window "The game ended with an error. Send a
  report?" (event `game_crashed`) → form with `kind=crash`. Sent only by the player's button: the logs contain the nick and open_id.
  The UE4SS dump (~38 MB) is not attached; `UE4SS.log` is not in the contract — the backend would reject the whole report.
- Checks: `cargo test --test mock_e2e -- --ignored bug_report` (the mock "publishes" after 10 s, limit 3 per hour);
  against the backend — `cargo test --test backend_live -- --ignored bug_report` (dev login; with `DBL_ADMIN` the test plays
  the bot's role through the admin API). In the UI sandbox: `?bug=crash`, `?bug=limit`, `?bug=slow`.

## One exe and verifiability

- The frontend is embedded in the exe **without compression** (tauri's `compression` feature is off) — HTML/JS are visible as text:
  `strings dungeonborne-launcher.exe | findstr I18N`.
- vcruntime is static, WebView2Loader is embedded; only Windows system DLLs. WebView2 Runtime is needed
  (present in Windows 10/11).
- The `hosts` helper is the same exe (`--hosts-helper install|uninstall`), there is no separate file.
- Nothing is encrypted or packed (no UPX/protectors) — this also reduces AV false positives.

## Interface with the backend (`core/backend/src/auth/`)

The backend is provider-independent, and the launcher accepts its shape, not only the one in the spec:
`auth: {providers: [...], api, require}` (and the old `provider: "discord"`); account
`{provider, subject, display_name, avatar_url, open_id, nick, roles}` (and the old `discord_id`/`discord_name`).
Empty `game.exe_sha256` (test configs) — the server is added and login works, "Play" is blocked.
Check against a running backend: `cargo test -p dungeonborne-launcher --test backend_live -- --ignored`.

## Accepted contract (LAUNCHER_SPEC.md §10)

- The emulator profile (open_id, nick, login time) is in the launcher: `%APPDATA%\DungeonborneLauncher\profiles\<server.id>\profile.json`,
  a cache of the server's answer. For the game session `Win64\steam_settings\force_steamid.txt` and
  `force_account_name.txt` are put into the game; the player's own `force_*` are saved and restored, a created `steam_settings` is deleted.
  The global Goldberg folder is not touched.
- `steam_api64.dll` and the UE4SS runtime (`dwmapi.dll` + `ue4ss\`) are part of the install: the launcher only checks them
  before launch and lays out Lua mods into `ue4ss\Mods`.
- The hash is checked on `Dungeonborne\Binaries\Win64\Dungeonborne.exe` (the root one is the bootstrap, not checked).
- `mods[].url` — absolute or relative to the manifest URL, `size` — the package size (checked).
- `modset_sha256` = sha256 of the lines `id@version:sha256` (package sha256), sorted by bytes and joined with
  LF without a trailing LF.
- A dropped lobby connection (6601–6604) or a tunnel refusal mid-game → `GET {api}/me`; a session error (401,
  ban) — the token is erased, the card shows login with the reason.
- A login refusal `denied` comes with `reason`/`message`/`link`: for `not_in_guild` — a "Join the server" button
  (link from `link`) and "Log in again". During login `user_code` is shown for comparison with the browser.

## Other decisions

- Profile paks: `Paks\~dbl_<server.id>\zzzzzzzz_<priority:03>_<n>_<mod-id>_P.{pak,ucas,utoc,sig}`.
  Checked in the game on 04.11 (`tools/iostore-rename-test.ps1`): the ChatFix IoStore pak, renamed like this and put
  into the subfolder `Paks\~dbl_test\`, loads — the "World" tab in the lobby chat is there (`container_id` is stored in the `.utoc`).
- Foreign paks go to `Content\_dbl_disabled\` only with `policy.allow_unlisted_client_mods: false`.
- Native (DLL) mod components are not installed in v1.
- The manifest signature (`signature`) is not checked yet — only TOFU of the key.
- Mock: `/mock/ban?nick=…` — ban a nick; the "Mock Discord" page has a "Not in the guild" button.
