# UE4SS mod settings

Since 12.11 a mod's settings are INI files in its folder, not a scattering of flag files. Done for `DungeonborneRuClient`; other
mods — in the list below.

## Two files

| File | Who writes it | What it holds |
|---|---|---|
| `Mods/<mod>/server_settings.ini` | the launcher, before every game start | settings from the server: season, section `[client.<mod>.<section>]` in `content/seasons/base.toml` and `seasons/<season>/season.toml` → `client_settings` of the `dbserver/1` manifest → file. Players should not edit it: it is overwritten |
| `Mods/<mod>/settings.ini` | ships in the package / edited by hand on the stand | local and debug settings |

A value is taken from `server_settings.ini`, otherwise from `settings.ini`, otherwise the default in code. Format:
`[section]`, `key = value`, comments `;` or `#` (also at the end of a line). Sections and keys — `[A-Za-z0-9_]`.

In Lua (`RuClient`): `ModSetting(section, key)` → string and source (`server` | `local`); `ModFlag(section, key, old
paths…)` — the same, and without the setting — the first line of the old flag file (transition period: the old files are
still present on Robot and the stand).

## Server → client

- Backend: `core/backend/src/season.rs` (`Active.client`, merge base → season by key), `core/backend/src/auth/web.rs` (`client_settings` in
  the manifest).
- Launcher: `manifest.rs` (`client_settings`), `profile.rs` (`server_settings_ini`, the UE4SS mod layout step) — the file
  is written into the mod folder the launcher lays out, and is removed together with it on restore. Old launchers do not read the
  field — their players get the default values from code.
- Stand without the launcher: `python -I dbtest/server_settings.py <Win64\ue4ss\Mods> [season]` — the same file.

## DungeonborneRuClient

| Section / key | Default | Where it came from before | What |
|---|---|---|---|
| `stealth` (all keys) | see `content/seasons/base.toml` | `stealth_reveal.txt` | an invisible enemy rogue is visible up close, `docs/ROGUE_STEALTH.md`; set by the server |
| `r14.mode` | `all` (package: `off`) | `r14_mode.txt` | R14 stub for pawns |
| `map.rules_keep` | 0 | `map_rules_keep.txt` | 1 — do not touch the game's map options |
| `teams.mode` | `party` | `teams.txt` | `party` / `coop` |
| `native.data_url`, `realkey`, `keepopenid`, `noclose` | none | `native_data.txt` (URL on line 1, flags as words) | match data via dbnative (battle server, stand) |
| `native.init` | 0 | `native_init.txt` | 1 — do not set `HasInitedPlayerData` |
| `herofix.enabled` | 1 | `Scripts/ru-herofix.txt` | client-side hero fixes |
| `pvpleave.enabled` | 1 | `ru-pvpleave.txt` (next to the exe) | auto-leave after PVP |
| `connect.timeout` | 60 | `ru-connect-timeout.txt` (next to the exe) | match connection timeout, 0 — off |
| `diag.projectiles` | 0 | `ru-projdiag.txt` (next to the exe) | projectile log (debug only) |

`stealth_reveal.txt` and `r14_mode.txt` were removed from the repository; `r14.mode = off` — in the package's `settings.ini`. The old files
in the installs (Robot, stand, F:/E:) are still read as a fallback — remove them on the next deploy to Robot and
move `native_data.txt` into `settings.ini` (`[native] data_url = …`).

## Not converted yet

- `DungeonborneRuServer`: `dbserver-*`/`dbclient-*` in `Saved/RebuildProbe` — mostly exchange channels (commands, status),
  not settings; the settings among them (`ru-north.txt`, `*-live.txt`, `ru-map-options*.txt`) — sort out per
  `docs/MODS_REFACTOR_PLAN.md` stage 3.
- `ClientProbe/config.txt` — already `key = value`, just a different name.
- `DungeonborneStatWatch` (`statwatch-config.txt`), `JesterSwap` (`ru-jester.txt`), `DungeonborneArenaScan`
  (`arenascan-dev.txt`) — debug mods.
