# Mods refactoring: analysis and plan

Status: 07.11, code unchanged. Target rules: [MODS_DESIGN.md](MODS_DESIGN.md).

Sources: repository files, the installs `C:\Steam`, `C:\Games\Dungeonborne`, `C:\Games\Dungeonborne`, the kit `dist\dungeonborne-robot-20261103`, the packages `launcher\dist\mods`, a Robot snapshot from 07.11 (section 3).

## 1. Conclusion

The mechanisms themselves work; the problem is that they have no shared boundaries. The most expensive problems:
1. Lua runs in two threads: this is the cause of the crashes.
2. Packages for players are built from the working install, not from sources: players already get an old DLL.
3. No version history: right now a big refactoring could not be rolled back.

So first insurance and crashes, then the build, and only then splitting the files.

## 2. What exists

### UE4SS mods

| Mod | Lines | Where it runs | `LoopAsync` loops | What it does |
|---|---|---|---:|---|
| `DungeonborneRuServer` | 4463 (152 functions) | server, `-dbclient` role in automated tests | 1 | Server start and reset, results, join, hall, portals and floors, teams, diagnostics |
| `DungeonborneRuClient` | 2264 (61 functions) | client **and server** | 6 + 8 `ExecuteWithDelay` calls | Player data via `dbnative`, teams, map rules, R14, relics, connection timeout, PVP leave, diagnostics |
| `DungeonborneR14` | 68 | client | 0 | Equipment manager stub, paired with `zzzzzzz_BPFix_P` |
| `JesterSwap` | 284 | client; **also enabled on the local server** | 2 (100 ms and 2 s) | Jester instead of Reaper |
| `DungeonborneStatWatch` | 758 | disabled on the DS | 2 | Stat diagnostics |
| `ClientProbe` | 461 + table | optional package | 2 | Client diagnostics |
| `CrashTracer`, `NameDump`, `DSTest` | 465, 112, 12 | development | 0 | Diagnostics |
| Standard UE4SS | — | various | — | `BPModLoaderMod`, `BPML_GenericFunctions`, `Keybinds`, `shared`; the server kit also has `CheatManagerEnablerMod`, `ConsoleCommandsMod`, `ConsoleEnablerMod` |

### What is inside the big files

Boundaries are approximate, by sections and log tags.

| RuServer, lines | Topic | Proposed module |
|---|---|---|
| 1–160, 4440–4463 | start, state, main loop | `main.lua` |
| 160–347 | server start phases | `server/phases.lua` |
| 347–678 | `-dbclient` role, automated test | `dev/autoclient.lua` |
| 678–800, 4365–4440 | replication tracing | `dev/reptrace.lua` |
| 800–935 | tick rate, priority, measurements | `server/perf.lua` |
| 987–1453 | results, join, `JoinGuard` | `server/results.lua`, `server/joinguard.lua` |
| 1453–1641 | slot and reset between matches | `server/slot.lua` |
| 1641–1958 | waiting hall | `server/waiting.lua` |
| 1958–2169 | who killed whom | `server/kills.lua` |
| 2169–3310 | spectator, zones, floors, portals | `server/spectator.lua`, `server/floors.lua`, `server/portals.lua` |
| 3310–3515 | hero fixes | `server/herofix.lua` |
| 3515–4060 | commands and the control file | `server/commands.lua` |
| 4060–4365 | probe | `dev/probe.lua` |

| RuClient, lines | Topic | Where needed | Proposed module |
|---|---|---|---|
| 120–340 | player data, `dbnative`, closing the connection | server | `battle/native_init.lua` |
| 344–600 | R14 | client | `client/r14.lua` |
| 592–850 | charges and relics | client | `client/relic.lua` |
| 851–1040 | map rules | server | `battle/maprules.lua` |
| 1041–1250 | teams, start points | server | `battle/teams.lua` |
| 1255–1330, 1599–1700, 1925–1958 | diagnostics | development | `dev/*` |
| 1329–1600 | bodies, souls, resources, loot | client | `client/hero_visual.lua`, `client/refill.lua` |
| 1891–1925 | hero polling | server | `battle/hero_poll.lua` |
| 1959–2100 | hero fixes | client | `client/herofix.lua` |
| 2104–2264 | PVP leave, connection timeout | client | `client/pvpleave.lua`, `client/conntimeout.lua` |

### dbnative

926 lines of C, 15 operations, 11 addresses with first-bytes verification. The code lives in `game\ue4ss\native\dbnative`, the DLL in the RuClient folder. Exchange with Lua goes through the files `req.txt` and `rsp.txt`, because UE4SS does not export the Lua API. There are no separate threads, `CreateThread` was not found in the code.

### Paks

| Pak | What is inside | Where it is built from | Note |
|---|---|---|---|
| `zz_LIPatch_P` | `DebugConfig.json`, `AuthLogin*.luac` | `<data>\repack`, outside the repository, 23.10 | `DebugConfig` is also in `RebuildPrivate`, the files differ |
| `zz_Loot_P` | 4 loot tables | `repack` | All 4 are in Balance with different contents: **looks like it has no effect** |
| `zz_Market_P`, `zz_Quest_P` | tables | `repack` | No generator, hand-edited files |
| `zz_MatchLocal_P` | native `WBP_MatchMainButton.lua` | repository | Overrode an old copy. The launcher now removes foreign paks itself: **possibly not needed** |
| `zz_RelicQuality_P` | UnLua relic panel | repository | Has a `README` and `build.ps1`: a good example |
| `zz_Jester_P` | Jester model | `<data>\ue_jester` | **Not in `mods.json`**, not distributed to players |
| `zzzzz_RebuildPrivate_P` | `DebugConfig.json`, `affix_pool` | repository | `affix_pool` is overridden by Balance. Name and description are outdated: there is no Lua there since 04.11 |
| `zzzzzz_Balance_P` | tables | `cargo xtask balance build` | Three different versions, see section 3 |
| `zzzzzzz_BPFix_P`, `ChatFix_P` | BP patches and tables | `bp_patches`, sources in `research\` | Correct, but the sources are outside the repository |
| `zzzzzzz_ServerNet_P` | `IsServer` patch | `game\bp-patches\pak_server` | Server only |

Overrides were found by file contents and pak order, I did not check in game.

## 3. Problems

Ordered by the cost of a mistake.

### Crashes from two threads
Analysis in [CRASH_LONG_PLAY.md](CRASH_LONG_PLAY.md). Besides the mods from that analysis, `LoopAsync` is also in `StatWatch` and `ClientProbe`.

### Packages for players are built from the working install
`make_mods.py --src C:\Games\Dungeonborne` takes the files as they are. What has already come out of this:
- **The `ru.client-hooks` package from 06.11 contains `dbnative.dll` from 03.11** (200 704 bytes), while the source and the server DLL are from 06.11 (204 800 bytes). What this affects for players I did not check.
- The same package got `r14_mode.txt` from the working folder. The value is suitable, but it got there by accident: any debug file from this folder will go to players.

### No version history
There is no `git` in either `backups` or `launcher`. Instead there are `backup\`, `framework\`, `repack*`, `src_*.zip`. RuClient versions have already diverged: `framework` and the Robot kit differ from `backups`.

### Each install lives its own life

| File | backups | Steam (server) | F (client) | E | Robot kit |
|---|---|---|---|---|---|
| `RuClient/main.lua` | 791294DA | same | same | same, but disabled | C0A78989 |
| `RuServer/main.lua` | DD40797E | same | same | same, but disabled | 111DD718 |
| `dbnative.dll` | 75E2382C | same | **8C1660AE (03.11)** | none | 4DD9267E |
| `zzzzzz_Balance_P` | 9A0CDB6F (07.11, not installed) | 09145D2C | 09145D2C | 09145D2C | different |

Also:
- `seasons\dead-money\pak` contains a third version of Balance: BA4C19F5 from 05.11.
- `E:` is incomplete: no `zz_LIPatch_P`, `dbnative.dll`, and RuClient and RuServer are disabled there. It looks like an install for the launcher, which puts mods in place for the duration of play. **This is the likely reason the second client hung before login when started bypassing the launcher** (not checked).
- `JesterSwap` is enabled on the Steam server: a 100 ms loop for something the server does not need.
- Cheat and console mods are enabled in the server kit.
- Enabling is set in two ways at once: `mods.txt` and `enabled.txt`.

### Robot snapshot (07.11, read-only)
Taken over SSH on 07.11 at 01:03 by the Robot clock (UTC−8). Scripts and output are in the session scratchpad, `robot_snapshot*.ps1` and `*.txt`.

| What | Robot | Repository 07.11 | Conclusion |
|---|---|---|---|
| `RuClient/main.lua` | 51E4BBCA, 06.11 | 791294DA | Robot has the same file as the players' package from 06.11. The 07.11 edits are not deployed, as expected |
| `RuServer/main.lua` | 3FE3F927, 06.11 | DD40797E | The 07.11 edits are not deployed |
| `dbnative.dll` | 75E2382C | 75E2382C | Matches. Players have the DLL from 03.11, the server from 06.11 |
| `UE4SS.dll` | C273E722 | C273E722 | Matches |
| UE4SS settings | consoles disabled, hot reload disabled | — | As it should be |
| `zzzzzz_Balance_P` | 09145D2C | 9A0CDB6F, not installed | Robot has the same balance as the local installs |
| Other paks | match the local ones | — | Plus two `*.pak.bak-20261103` files: the game does not load them |
| `mods.txt` | `CheatManagerEnablerMod`, `ConsoleCommandsMod`, `ConsoleEnablerMod` enabled; `CrashTracer` disabled; no `JesterSwap` | — | Cheat and console mods are enabled on prod |

Also:
- `native\dbnative.log` is 26 MB and there are 790 `gameover-*.bin` files in the RuClient folder. No rotation, they grow without limit.
- Process `pid 11308` has been hanging since 06.11 07:49 by the Robot clock, 698 MB. This is an item from FIX_LIST, still not closed.
- Two fresh processes take 3.9–4.1 GB private each.
- `Kit` has `ds-agent.exe.new` from 03.11, older than the working `ds-agent.exe` from 04.11. `start_headless.ps1` applies `*.new` only in `game\ue4ss\Mods`, so the file is simply redundant.
- **The Robot kit scripts are not in git.** `start_headless.ps1`, `install_gameserver.ps1` and `killmutex.ps1` on Robot match the copy in `dist\dungeonborne-robot-20261103`, and `dist` is excluded from git. The file `dbserver\start_headless.ps1` in the repository is different, it is the local variant. The kit sources need to be moved into a tracked folder in stage 2.

### Robot crash baseline
Times are from the timestamps inside the logs `Kit\logs\ue-*.log`, UTC. A crash here means a `Critical error` line in the log. `abort` without this line and hangs are not counted.

| Day | Starts | Longer than 2 min | Longer than 20 min | Hours of run time | Crashes |
|---|---:|---:|---:|---:|---:|
| 03.11 | 20 | 15 | 4 | 4.6 | 0 |
| 04.11 | 132 | 103 | 18 | 24.7 | 4 |
| 06.11 | 138 | 135 | 44 | 40.1 | 3 |
| 07.11 (until 09:00) | 59 | 58 | 24 | 21.1 | 0 |
| **Total** | **349** | **311** | **90** | **90.5** | **7** |

- There are no logs for 05.11.
- The crashed runs lasted 0, 5, 6, 11, 12, 16 and 22 minutes.
- The 04.11 crashes were before the `NotifyOnNewObject` fix (B13b).
- **Baseline for stage 1:** since 06.11, 3 crashes in 193 starts and 61 hours of run time. That is roughly one crash per 20 hours of server run time. The sample is small: to see a difference after the fix, several days of run time will be needed.

### Two files for everything
- The main block of RuServer has hit Lua's limit of 200 local variables. The comment on line 531 says exactly this; new variables have to be worked around.
- A new command is edited in two places: the `or` chain on lines 3996–3997 and `HostCommand`.
- Diagnostics and product are mixed together and run for players. For example, the `RESULTPROBE` loop every 500 ms.

### Boundaries between mods do not match the names
- RuClient runs on the server and holds important logic there: player data, teams, map rules.
- RuServer takes `dbnative.dll` and `native_data.txt` from the RuClient folder, but this is not written down anywhere.
- RuServer contains the automated-test client.
- `Log` and `NativeCall` are written twice, and `NativeCall` with different arguments.

### Settings are scattered
- RuServer reads 13 `dbserver-*` files, 3 `dbclient-*` files and `ru-map-options.txt`.
- RuClient reads 9 more: `native_data`, `native_init`, `r14_mode`, `teams` and 5 `ru-*` files.
- Storage locations differ: the mod folder, the `Win64` folder and `Saved/RebuildProbe`.
- Some files are left over from finished experiments: `fixrep`, `wpstream`, `initmap`, `fakeds`, `gameover-probe`, `pvediag`, `reptrace`. There is no full list anywhere.

### Network in the game thread
`SlotTick` every 10 s and `/ds/kill` go synchronously through `dbnative` in the game thread, with WinHTTP timeouts of 3–5 s per step. If the backend stops responding, the server frame may stall for several seconds. This is a conclusion from the code; I have not yet looked for it in the logs.

### Tied to one PC
`F:\...` and `E:\...` are hard-coded in `xtask` and `deploy_paks.ps1`, and `repak.exe` is taken from `DungeonbornePVE`. Some pak sources are outside the repository. For a move to a new PC this is a separate risk.

### Documentation
There is no list of all mods anywhere. Knowledge lives in code comments, [FIX_LIST.md](FIX_LIST.md) (182 KB), [DS_PIPELINE.md](DS_PIPELINE.md) and `fix_listen.md`. In `FRAMEWORK.md` the layer table and the LocalDev references are outdated.

### What works well and stays
- `dbmod/1` packages: hashes, rollback, deterministic zips.
- `xtask balance` and `match_checks.toml`.
- First-bytes verification of functions in `dbnative`.
- The BP patch pipeline with CFG verification.
- Safety rules from code comments, they only need to be collected.
- `dbtest` automated runs.
- `zz_RelicQuality_P` as a layout example.

## 4. Plan

Each stage is deployed separately and does not require the next one. During a stage the business logic does not change, except for explicitly named edits.

### Stage 0. Insurance
Work:
- `git init` in `backups` and `launcher`, local only, without an external repository;
- `.gitignore` for keys, certificates, databases, `target`, dumps; first check that secrets did not get into the index;
- a read-only Robot snapshot: hashes of mods, paks, `mods.txt`, settings;
- a baseline crash measurement: how many per day per how many starts.

Done when: there is a first commit, the Robot table is in this document, and there are no secrets in the history.

Done 07.11: local repositories `backups` and `launcher`, first commits, keys and `DiscordCFG` excluded. We work on this PC (user's decision). The Robot snapshot and the crash baseline were taken on 07.11, the result is in section 3. Remaining: private GitHub and restic.

Rollback: not needed, the code did not change.

### Stage 1. Crashes: one thread
Only replacing the mechanism, without moving code around, so that the measurement is clean.

Work:
- check the behaviour of `LoopInGameThreadWithDelay` and `ExecuteInGameThreadWithDelay`: how the loop stops, what happens to it on a map change;
- replace all loops and delays in RuClient, RuServer, JesterSwap; StatWatch and ClientProbe too;
- read the control file from the game loop;
- the exit watchdog `op=quitwatch` in `dbnative` instead of `os.exit` from a service thread;
- disable `JesterSwap`, cheat and console mods on the server.

Check: a long raid with clients on the test stand, then one instance on Robot, then all. Comparison with the baseline: 3 crashes in 136 starts and the stage 0 measurement.

Rollback: the previous `main.lua` and DLL, from `git`.

**Done 07.11, in the repository and on the local test stand. Not deployed to prod.**

What was done:
- **Mods.** RuClient (`GameLoop`/`GameDelay` wrappers), RuServer (main loop), JesterSwap, StatWatch and ClientProbe run only in the game thread. Timers are registered via `ExecuteInGameThread`.
- **`dbnative`.** Added `op=quitwatch`, the DLL log is protected by a lock.
- **Server on the test stand.** I did not touch `JesterSwap`, cheat and console mods. Cleaning them up on Robot waits for a decision about prod.

What was checked:
- **The timer experiment** is described in [MODS_DESIGN.md](MODS_DESIGN.md), section 5.
- **The first version** registered timers right at mod load and hung the client already at startup. On the server the same passed. This is fixed.
- **`resetcycle.ps1`, 3 rounds with a 90 s hold:** 3 of 3. Connect, match, exit, result, reset, `free/ready` again.
- **A 30 s hold does not work.** At that time the server is moving from the hall into the match and rejects `leave` (`rejected_not_ready`). The script was written before the waiting hall.
- **One more run, client `F:` with `CrashTracer`:** 2 of 3. In the third round the client crashed while creating the pawn with the familiar signature `UE4SS.dll …20e4/9e2e/a0c3/b956/8d7c/d065/6aa8 ← 0x1450e0023`. It is called by `NotifyOnNewObject`; on Robot it was seen on 04.11, before these edits.
- **The exit watchdog** with a 1 s threshold terminated the server 1 s after `quit`. With a 10 s threshold the server exited by itself in 2 s, the watchdog was not needed.

What was not checked:
- a long raid, a match with several live players, Robot;
- `ClientProbe` and `StatWatch` in game, only compilation.

**Found along the way.** On the client `NotifyOnNewObject` remains a source of crashes. On the server it was removed in B13b, but on the client it is set by RuClient (`ClientNotify`) and the debug `CrashTracer`. Move the client to polling, like the server: stage 3 or 4.

### Stage 2. Catalog and build from the repository
Work:
- `catalog.toml` with the current paths, without moving folders;
- `cargo xtask mods build|check|install|verify`;
- paths to installs in settings;
- `make_mods.py` takes files from `out/client`, not from `F:`;
- the Robot kit is built by the same command.

Check: packages match the expected contents, the launcher test `mods_dist`, a clean install on the test stand.

This also closes the old DLL for players and stray files in packages.

**Done 07.11:** `mods/catalog.toml` (20 parts, 4 profiles), `cargo xtask mods build|check|install|verify` (`core/backend/xtask/src/mods.rs`), `mods/README.md`, `make_mods.py --exe`, Robot scripts in `ops/deploy/robot/`. Checked:
- **`check`:** 0 errors. It found by itself that `zz_Loot_P` is fully overridden, and 5 sources outside the repository.
- **`zz_MatchLocal_P`, `zzzzz_RebuildPrivate_P`, `zz_RelicQuality_P`** built from sources matched the installed ones byte for byte.
- **`verify`:** `F:` and `C:\Steam` match the build, except the new balance from 07.11, which is not installed yet.
- **Launcher packages from `out/mods/client`:** compared to the 06.11 release only `ru.balance`, `ru.client-hooks` (07.11 edits and a fresh `dbnative.dll`) and `ru.client-probe` (stage 1) change. The other 9 have the same contents.

Found along the way:
- **`game\bp-patches\pak` is outdated.** It has a pak from 30.10, while the installs and Robot have `research\bpfix_pak_hero`, and the catalog points to it. The `bp_patches` README also describes the old contents.
- **`r14_mode.txt`.** Without the file RuClient turns on R14 mode `all`, and players had `off` only because the file accidentally got into the package from `F:`. Now the file is in the repository.
- **`repak` writes files in a varying order.** So the build takes the pak from the cache `out/mods/.cache` if the sources have not changed.

Not done:
- the UE4SS runtime in the catalog;
- building the whole Robot kit with one command;
- `verify` on Robot (for now only a PowerShell snapshot);
- generating `launcher/tools/mods.json` from the catalog: right now these are two lists, and they have to be kept in sync by hand.

### Stage 3. Cleanup
Work:
- go through all settings files and decide: keep, move into settings, delete;
- move diagnostics out of the players' stream: it stays in the repository, but behind a `dev` flag;
- verify that `zz_Loot_P` and `zz_MatchLocal_P` have no effect, and remove them from profiles;
- remove the old copies `repack*`, `framework`, old `dist` kits only after a separate confirmation.

Check: automated runs `dbtest\run.ps1 -Match native` and `resetcycle.ps1`.

### Stage 4. `dbcore` and splitting the Lua
Work:
- `dbcore`: `log`, `sched`, `flags`, `native`, `safe`, `paths`;
- `Scripts/flags.lua` and a command table;
- splitting per the tables of section 2, one topic per edit, diagnostics first, then server, then client;
- the server parts of RuClient are moved into `battle/`, not into another mod: that way the load order does not change.

Check after each topic: `mods check`, an automated run, the same log tag in the same scenario. Check `require` with subfolders in UE4SS before starting: a plain `require` works in `ClientProbe`.

Rollback: one commit per topic.

### Stage 5. dbnative
Work:
- split `dbnative.c` into `addr.h`, `http.c`, `battle.c`, `net.c`, `main.c`;
- a worker thread and a queue for `SlotTick` and `/ds/kill`;
- a hang watchdog with a threshold, only after `ready`.

Check: in the "backend does not respond" scenario there are no crashes or frame stalls; with a different exe version the address-based operations refuse rather than crash.

### Stage 6. Paks
Work:
- bring in pak sources from `repack`, `research`, `ue_jester` (big files by link, not in `git`);
- the `Market`, `Quest` and `DebugConfig` tables are built from one place;
- decide the fate of `ru.jester`.

### Stage 7. Documentation
Goes along with each stage, not at the end:
- a `README.md` per mod following the template from the design;
- `mods/README.md`: build and install;
- update `FRAMEWORK.md` and `DS_PIPELINE.md`;
- `FIX_LIST` stays a log, not a description.

### Last: moving into `mods/`
One mechanical pass, when the build already goes through the catalog and there are almost no scripts tied to the old paths.

### What we will not do
- Rename RuClient and RuServer.
- Rewrite logic during a move: moves and edits go in separate commits.
- Change the UE4SS version: that is a separate research.
- Replace Lua with UE4SS C++ mods: no reason, and no sources for this build.

## 5. Relation to other work

- **Move to a new PC.** Stages 0–2 make it simpler: version history and paths from settings. `git` can be started now, it is local only. New secrets do not go into it.
- **Deploy 07.11.** The current edits do not depend on the plan. If the package is built before stage 2, a fresh `dbnative.dll` has to be put into `C:\Games\Dungeonborne`, otherwise players will again get the DLL from 03.11.
- **CI from [CI_CD_PLAN.md](CI_CD_PLAN.md)** later calls the same `xtask mods`.

## 5a. Seasonal and base: waiting for the user's decision

We are building a framework from the base game and pluggable seasons ([FRAMEWORK.md](FRAMEWORK.md)), but some seasonal rules ended up in shared code. **How the base version works is the user's decision.** Until they say so I do not change behaviour, only note it.

| What | Where now | Looks like | Question |
|---|---|---|---|
| Kill message `POST /ds/kill` | RuServer, "Who killed whom" | base event, seasonal reward (currency 1012 Dead Money) | Keep the event in the core and compute the reward only in the season? |
| Red portals: 50% of blue ones on disk 5 become red, plus one guaranteed | RuServer, `Spect.RedTick` | unclear (decision 06.11, works around an unfound cause) | Is this a season rule or a base game rule? |
| The purple portal on the first floor leads to the throne, the second floor has only blue ones | RuServer, `Spect.Shadow`, floor rules | unclear (decision 06.11) | Same |
| Portal appearance message | RuServer, `PortalWatch` | base (native game message) | — |
| Map rules: `MapId`, options, second floor | RuClient, `MapRulesByLevelName` | base (like the official server) | — |
| Lower relic quality | pak `zz_RelicQuality_P` and `relic_lower_quality` in `deadmoney.toml` | seasonal: the pak README already describes moving it into a season | Move it into a season? |
| Jester instead of Reaper | `JesterSwap`, `zz_Jester_P` | unclear: appearance | Is this season or base? |
| Search checks | `match_checks.toml` | base server settings | Should seasons change them? |
| Auto-sale of extracted loot | backend, already only in Dead Money (07.11) | seasonal | — |
| Forge materials on login, system vendors and auction | backend, variables `PLAYTEST`, `TRADE_SYSTEM_*` | unclear | Base, season or playtest only? |

Once there is a decision, the seasonal parts of the mods will get their own plug-in point, like the backend seasons. Right now mods have no such point: [FRAMEWORK.md](FRAMEWORK.md), "What is not seasonal yet".

## 6. What needs deciding

1. **`git`:** start locally now (I recommend) or only on the new PC?
2. **Jester:** do players need it? Then add `ru.jester` to the catalog. If not, keep it for development only.
3. **Cheat and console mods on the server:** remove (I recommend). Need to check against the Robot snapshot.
