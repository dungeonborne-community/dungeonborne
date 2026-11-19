---
name: dungeonborne-workflow
description: "Dungeonborne emulator — where mods/logs live, two game installs, backlog file, how the user tests"
metadata:
  visibility: team
  node_type: memory
  type: project
---

- UE4SS mods are edited in `<repo>\ue4ss\Mods` and must be copied to BOTH game installs:
  `C:\Steam\steamapps\common\Dungeonborne\Dungeonborne\Binaries\Win64\ue4ss\Mods` (main one — user's logs come from here)
  and `C:\Games\Dungeonborne\Dungeonborne\Binaries\Win64\ue4ss\Mods`. Lua syntax check: `C:\Program Files (x86)\lua-5.4.7\lua.exe`.
- Balance tables: edit `content\balance\tables\*.json` (or `cargo xtask balance csv` → Excel → `csv-import`), then
  `cargo xtask balance build [--deploy]` (docs in BALANCE.md; code in `xtask/`). Don't hand-edit `core\backend\ExcelConfig\*.bytes`;
  `cargo xtask balance export` overwrites `balance\`.
- Client mod paks override each other's `AdditionalData\ExcelConfig\*.bytes` by name order (zz_ < zzzzz_ < zzzzzz_).
  Loot/table bugs: first list which installed paks carry the table (`repak list`). Source of `zzzzz_RebuildPrivate_P.pak`
  is the folder of the same name in backups; repack with `repak pack --version V8B --mount-point ../../../`.
  Since 2026-10-30 C:\Games\Dungeonborne carries the same mod paks as Steam (second client for dedicated tests, Goldberg
  Steam ID via local_save.txt); its PVE paks zz_Solo*_9_P moved to `Content\_pve_disabled_paks`
  (outside Paks — UE mounts Paks subfolders recursively). The Steam install's Goldberg ID is per machine:
  `config/local.toml [stand] host_open_id` (run_local.ps1 reads it).
- Open tasks / findings are tracked in `<repo>\BACKLOG.md` (started 2026-10-30); update it as work lands.
- Battle via `host_from_lobby` = NM_ListenServer (`?listen`); "localbattle" = standalone. Some GA logic (heavy charge release)
  only breaks on listen — always ask which mode the user tested.
- User runs the game and in-game checks themselves; I start the backend (`cargo run`, `PVE_SNIFF=127.0.0.1`, `RPC_DUMP=1`)
  in a terminal tab. Ports 8085/6601-6604/443/80 conflict with PVE SoloLobby/LoginService — user closes them.
- Before restarting the backend: check `Get-Process Dungeonborne` and the backend tab tail (heartbeats, logins).
  The patch-agent session or the user may be mid-run (03.11 I cut an active room). If anything is running, ask or
  warn the agent first. All room/allocator/team state is in memory and is lost on restart.

- Second game window: game runs elevated, so close mutexes from an elevated (UAC) PowerShell. Only these block a 2nd
  instance: `Sessions\N\BaseNamedObjects\Dungeonborne`, `UnrealEngine4`, `CDdf212806D6EmB31yE0c` (close in every running
  Dungeonborne.exe, then launch exe directly). Script killmutex.ps1 (NtQuerySystemInformation + DuplicateHandle CLOSE_SOURCE)
  lives in the session scratchpad — rewrite if gone. Don't use `Start-Process -Wait` (it waits for the child game).
  Preferred (no UAC): admin task DbTools (backups\dbtest\README_admin.md) — write `{"action":"killmutex"}` to
  `<data>\dbtools\request.json`, `Start-ScheduledTask DbTools`, read `C:\ProgramData\DbTools\dbtools.log`;
  then plain `Start-Process` of the wanted install's exe from my shell (DbTools `start` is only for -dbserver/-dbclient).
  Process paths are unreadable even elevated; tell which install is running by its locked `ue4ss\UE4SS.log`, and launch
  the other install so the two clients get different Goldberg Steam IDs.

**Why:** repeated confusion about which install/mode was tested cost several iterations.
**How to apply:** deploy to both installs, note mode in BACKLOG, ask for `crash_ue4ss.log` / `crashtracer_pawn_init.log` from the Steam path.
- 2026-11-04 build setup: .cargo/config.toml uses sccache (rustc-wrapper) + rust-lld on Windows; Cargo.toml profiles:
  dev debug=line-tables-only (deps none), release incremental=true. Rebuild after an edit ~16 s (Windows) / ~19 s
  (zigbuild Linux into C:/cargo-target/dbserver-linux); full build ~9 min. Old target_check/linux/release dirs deleted.
