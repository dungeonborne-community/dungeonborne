# Balance Studio

An app for external Dungeonborne balance designers. It opens a kit, edits values and saves a
`dbbalance-patch/1` patch; the server owner applies the patch with `cargo xtask balance patch`. The app doesn't touch the game or
the server, doesn't use the network and doesn't run programs.

Spec and stages: `docs/BALANCE_APP_SPEC.md`.

```
src-tauri/   Tauri 2; thin commands, the logic is in core/backend/crates/balance-kit
ui/          index.html, app.js, i18n.js, input.css (Tailwind v4), api.d.ts (contract), dev/mock-tauri.js (sandbox)
```

## Build
You need `cargo tauri`, `trunk` and `tailwindcss` (standalone) on PATH. From PowerShell, `NO_COLOR` must be `true`, otherwise
`trunk` rejects the value `1`:

```powershell
$env:NO_COLOR='true'; cd src-tauri; cargo tauri build --no-bundle
```

Result: `target\release\balance-studio.exe`, a single file.

## Release
`docs\GUIDE.md` (the designer's guide) and `docs\CHANGELOG.md` ship with the exe in the designer package
(`cargo xtask pack designer`, in the package it is `README.md`). The version is set in
`src-tauri\Cargo.toml` and `src-tauri\tauri.conf.json`.

## Sandbox
The UI works in a plain browser without Rust: `cd ui; trunk build`, then any static server over `ui\dist`.
Scenarios: `?scenario=start`, `kit`, `old` (kit without a catalog), `error` (the kit doesn't open), `rebase` (moving changes).

## Data
`%LOCALAPPDATA%\BalanceStudio\`: `settings.json` (interface language, English by default; nickname; recent kits), `kits\<version>\` (copies of kits, data only),
`drafts\<version>.json` (draft, saved after every edit). For tests, `BALANCE_STUDIO_DATA` sets another folder.

## Checks
- `cargo test -p balance-kit` in `core/backend`: zip safety, table views, patch rules in Rust and `check.py`.
- The real exe: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>` and driving the page over CDP.
- `tools\accept\run_all.ps1`: the whole acceptance on the built exe (it starts the app in Russian: the checks expect Russian texts). Its parts are below.
- `tools\accept\`: keyboard and mouse on the real exe (`cdp_m2_ui.mjs`, `cdp_m3_ui.mjs` for the card, curves and bulk edit); compatibility with the workbook and the server (`m2_gen.py`,
  `cdp_m2_export.mjs`, `m2_accept.py`): applying and building run on a copy of the project in `<CARGO_TARGET_DIR>\m2-accept`.
