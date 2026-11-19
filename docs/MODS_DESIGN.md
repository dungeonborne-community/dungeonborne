# Mods: what goes where and how (design for the future)

Status: proposal of 07.11, not yet implemented. Current state and migration plan: [MODS_REFACTOR_PLAN.md](MODS_REFACTOR_PLAN.md).

These are the rules for all new mod code. Old code is brought in line with them by the plan, not all at once.

## 1. Core principles

1. **The server decides, the client shows.** An important decision is made by the backend or the game server. A player can replace a client mod.
2. **Take the safest layer that can do the job** (section 2).
3. **One source.** Everything that goes to players and to the server is built from the repository. A working game install is not a source.
4. **All Lua runs on the game thread.** The reason is in [CRASH_LONG_PLAY.md](CRASH_LONG_PLAY.md), the rules in section 5.
5. **Experiments apart from the product.** Diagnostics and experiments don't ship to players or to the prod server. Every experiment has a removal condition.

## 2. Which layer to pick

Go top to bottom and take the first layer that solves the task.

| # | What is needed | Layer | Why here |
|---|---|---|---|
| 1 | Rule, economy, rewards, login refusal | backend, `src/` | A player can't tamper with it, restart without a client update |
| 2 | Numbers and strings of the game tables | `ExcelConfig` table → balance pak | No code, doesn't crash |
| 3 | Window, button, text | UnLua wrapper in a pak, with the `_Orig` original next to it | The game's native mechanism, client only |
| 4 | Model, texture, icon | cook in UE 5.1 → IoStore pak | No other way |
| 5 | Blueprint logic that can't be bypassed from outside | BP byte patch | No extra hooks, but every patch must be checked against the CFG |
| 6 | Reaction to an in-match event, state polling, server control | UE4SS Lua | Flexible, but the main source of crashes |
| 7 | C++ protobuf, a field without reflection, a call by address, HTTP, a separate thread | `dbnative` | Lua can't do this. The most dangerous layer and depends on the exe version |
| 8 | Launch, restart, process health | `ds-agent` | Lives outside the game and doesn't crash with it |
| 9 | Delivery to players | launcher, `dbmod/1` packages | Hashes, rollback, versions |

Anti-examples we have already had:
- heavy logic in a `NotifyOnNewObject` callback: crashes, use polling or a hook;
- the match button on our own UnLua: replaced by the native one;
- selling and login checks on the client: the backend does this.

## 3. Where things live

Target layout. We move it in one of the last stages, because scripts depend on the current paths.

Now, since 07.11, the catalog and commands already work with the current paths: `mods/catalog.toml`, `cargo xtask mods`, [game/catalog/README.md](../game/catalog/README.md). The sources themselves are still in the old places.

```
backups/mods/
  catalog.toml              single list of everything that is built and installed
  README.md                 how to build and install
  lua/
    dbcore/                 shared Lua code, copied into the finished mod at build time
    DungeonborneRuServer/   README.md, Scripts/main.lua, Scripts/server/*.lua, Scripts/dev/*.lua
    DungeonborneRuClient/   README.md, Scripts/main.lua, Scripts/battle/*.lua, Scripts/client/*.lua, Scripts/dev/*.lua
    DungeonborneR14/
    JesterSwap/
    dev/                    ClientProbe, CrashTracer, NameDump, StatWatch: development only
  native/dbnative/          C sources, build.bat
  paks/<pak name>/          pak sources with files laid out as in Content
  bp/                       byte patches (now bp_patches) and source assets
  assets/<name>/            UE projects for cook or a link to them
  runtime/                  UE4SS.dll, dev and prod settings
  out/                      build output, not in git
```

**We keep the names `DungeonborneRuClient` and `DungeonborneRuServer`.** The launcher, `retired`, `start_headless.ps1`, `mods.txt` and player installs depend on them. Renaming is a risk without benefit; the meaning comes from the split inside.

**`dbcore` is copied into every mod at build time.** The `Mods/shared` folder is part of the runtime embedded in the launcher exe, so an edit there would need a new launcher build. A copy inside the mod is updated by a normal package.

**`dbnative.dll` stays in one place:** `Mods/DungeonborneRuClient/native`. Loading two copies from different folders would give two independent copies of the DLL state in one process. We record the dependency of RuServer on RuClient on the server in the catalog.

## 4. Catalog and profiles

`catalog.toml` answers the question "what is this, where is it built from and who gets it". The launcher packages, the Robot kit and the local install are built from it. Nothing is enabled in installs by hand.

```toml
[[item]]
id = "ru.server-hooks"
kind = "ue4ss"                     # ue4ss | pak-files | pak-iostore | native
source = "mods/lua/DungeonborneRuServer"
profiles = ["server", "dev-server"]
requires = ["ru.client-hooks"]     # dbnative and shared match rules
docs = "mods/lua/DungeonborneRuServer/README.md"
```

Profiles:

| Profile | For whom | What is included |
|---|---|---|
| `client` | players, via the launcher | only what a player needs, no diagnostics |
| `server` | Robot, via the kit | server-only, UE4SS consoles off |
| `dev-client` | local client | `client` plus selected dev mods |
| `dev-server` | local server | `server` plus dev mods, role `-dbclient` |

The build check fails if:
- a server component ended up in `client`, e.g. `zzzzzzz_ServerNet_P`;
- a dev component ended up in `client` or `server`;
- one file is in two paks and the catalog has no explicit `overrides`;
- an IoStore pak is not built from the three files `.pak .ucas .utoc`.

## 5. Threads

All Lua runs on one thread, the game thread. Direct timer calls are allowed only inside `dbcore/sched.lua`.

| UE4SS API | Rule |
|---|---|
| `LoopInGameThreadWithDelay` | The basis of all loops, via `sched.every`. **`return true` does not stop it**, only `CancelDelayedAction(handle)` does |
| `ExecuteInGameThreadWithDelay(ms, fn)` | Deferred one-off call, via `sched.after`. Returns a handle |
| `LoopAsync`, `ExecuteAsync`, `ExecuteWithDelay` | **Forbidden.** This is a worker thread |
| `ExecuteInGameThread` | Forbidden in new code: not needed on the game thread |
| `RegisterHook` | Allowed. Minimal work in the callback |
| `NotifyOnNewObject` | Forbidden on the server. On the client only queue the object; calls on it are forbidden: the object is still being constructed |

```lua
local sched = require("dbcore.sched")
sched.every("pvpleave", 500, PvpLeave.Tick)   -- pcall, error log, timing
sched.after("kick-close", 5000, CloseConn)
```

**A timer may be registered only on the game thread.** `main.lua` is loaded on the UE4SS thread, and `NotifyOnNewObject` callbacks may come from the object-creation thread. On 07.11 the client hung while still loading RuClient (CPU 0, first frame) when loops were registered right at load; the server with the same code started. Likely cause: the game thread runs mod timers and waits for the Lua lock, while the loading thread holds it and waits for the timers. The mechanism inside UE4SS is not verified, no sources. So `sched.every` and `sched.after` first go to the game thread via `ExecuteInGameThread`: that queue has worked from any thread for years.

Checked on the stand on 07.11 (temporary mod, local DS):
- **Stopping a loop:** `return true` does not stop the loop, while `CancelDelayedAction` does, including from inside its own callback. After cancelling, `IsValidDelayedActionHandle` returns `false`.
- **An error in a callback** does not kill the loop: the next call goes on schedule.
- **Map change:** login → lobby → battle map. Loops survive it, a one-off 60 s call fires after two changes.
- **Map loading:** calls are skipped, not accumulated. There were 3 calls in 11 s instead of 11, then exactly on schedule again.
- **Accuracy:** a 1000 ms loop fired 10 times in 10 s, a 100 ms loop — about 88 times in 10 s, because the timer is tied to frames.
- **Call form:** for `ExecuteInGameThreadWithDelay(handle, ms, fn)` the handle must be a number, a table doesn't work.

What `sched` gives:
- a name for every loop;
- `pcall` around every call;
- a warning in the log if a call took longer than a threshold;
- turning a loop off via a setting without editing code.

What a loop must not do:
- wait: no network without a timeout, no file access in a loop more often than once a second;
- call `FindAllOf` every tick without need: keep a cache and refresh it rarely;
- hang around uselessly: if the loop's target is not active, return on the first line.

Forced exit and hang protection live outside Lua, in a separate `dbnative` thread (section 8).

## 6. Safe game calls

These are accumulated rules, now mandatory and collected in `dbcore/safe.lua`:
- check `IsValid` before any call on an object. `GetFName` on a null object gives a native crash that `pcall` doesn't catch;
- don't keep a Lua `UObject` wrapper between frames. Keep the address or name and look it up again;
- inside hooks and object-creation notifications don't call functions of a half-built object;
- post-hooks on native `IsStandalone` and `IsDedicatedServer` return garbage instead of an answer;
- don't count on `ULocalPlayer` and the host controller as a player: filter them separately;
- don't delete the PC, PlayerState and pawn of a player without a connection: the server will crash on reconnect.

## 7. Settings and file exchange

Settings now live in three places and in dozens of separate files. There should be three distinct kinds:

| Kind | Where | Example |
|---|---|---|
| Permanent settings | `Mods/<mod>/settings.ini`, `key=value`, read at start | `herofix=1` |
| Experiment | same place, with the `exp.` prefix; a code comment: why and when to remove | `exp.observer=late` |
| Exchange with the outside world | `Saved/RebuildProbe/`: commands, status, result | `dbserver-control.txt`, `dbserver-status.json` |

Rules:
- every setting has a line in the `Scripts/flags.lua` registry: name, default value, description;
- at start the mod logs in one line all values that differ from the default;
- an experiment is over: remove the flag and the code branch in the same change;
- control commands are registered in a `Commands[name] = function` table. Now a new command is edited in two places, in the `or` chain and in `HostCommand`.

## 8. dbnative

- Only what can't be done in Lua: the game's protobuf, fields without reflection, calls by address, network, a separate thread.
- **All addresses in one table** with the function's first bytes. On mismatch the operation refuses and logs the reason. This rule already exists, we keep it.
- **We don't patch the game's code.** Only calls to its functions and data.
- **No ACE bypass and no reading other processes' memory.**
- **Network doesn't block the frame.** New code sends a request to the `dbnative` worker-thread queue, and Lua picks up the answer on the next tick. Synchronous HTTP stays only where we can't go on without an answer, e.g. player data before `InitPlayerGameData`.
- **Exit watchdog in a separate C thread:** `op=quitwatch sec=10` terminates the process after N seconds if a normal `quit` didn't work. Reason: a hung process under ACE couldn't be killed from outside (30.10), and Lua on a worker thread is no longer allowed for us.
- **Hang watchdog:** the game loop marks itself once a second, the C thread terminates the process if there are no marks for longer than a threshold. Enabled only after the `ready` phase, so it doesn't fire during map loading. Then `ds-agent` brings up a new process.

## 9. Paks

- **Order.** The names `zz_ < zzzzz_ < zzzzzz_ < zzzzzzz_` set the order in the install, and `priority` sets it in the launcher. They must match.
- **One file in one pak.** All game tables go through one balance pak.
- **Season.** Its pak replaces the balance pak, as described in [FRAMEWORK.md](FRAMEWORK.md).
- **UnLua wrapper:** the original `<Name>_Orig.lua` next to it, a minimal edit, its own log.
- **IoStore.** A 347-byte `.pak` is normal: the data is in `.ucas` and `.utoc`. Copy all three files.
- **The server pak is not installed for players:** `zzzzzzz_ServerNet_P` changes `IsDedicatedServer` to `IsServer`, and on the client that breaks the logic.

## 10. Logs

- One log per process and mod: `Saved/RebuildProbe/<mod>-<role><port>.log`.
- Line: `time | [TAG] text`. Tag in upper case, one per subsystem. Tools search by tag, e.g. `tools/py/probe_timeline.py`, so when a tag changes we fix them too.
- Permanent actions and refusals are always logged. Per-tick details are logged only on change, not every tick.

## 11. Build and release

```
catalog.toml + sources → xtask mods build → out/<profile>/ → install
                                              ├─ dev: local installs, game closed
                                              ├─ server: kit for Robot
                                              └─ client: dbmod/1 packages for the launcher
```

- Commands: `cargo xtask mods build`, `mods check`, `mods install --profile dev-client --game <path>`.
- Install paths come from a setting, not from code. Now `F:` and `E:` are hard-coded in `xtask` and `deploy_paks.ps1`.
- Every build writes a manifest file: version, sha256 of every file, exe version, which commit.
- `mods verify --game <path>` checks an install against the manifest. The same check on Robot is read-only.
- Deploy to prod stays a separate step with confirmation and via `maintenance.sh`.

`mods check` checks:
- `luac -p` for all Lua;
- forbidden APIs outside `dbcore`;
- reading a setting with no registry entry;
- the profile rules from section 4;
- the built `dbnative.dll` is newer than its source.

## 12. New feature or mod: checklist

1. Pick the layer per section 2. If it's UE4SS or `dbnative`, write in the description why a higher layer didn't work.
2. Put the feature in a module for its topic. A new module for a new topic, not an addition to `main.lua`.
3. Loops only via `sched`, setting in the registry, log with its own tag.
4. Add or update the entry in `catalog.toml` and the mod's `README.md`.
5. `cargo xtask mods check`.
6. Check in the game: on the stand and by the log, client and server separately. Say plainly what was not checked.
7. Record a workaround in [WORKAROUNDS.md](WORKAROUNDS.md), an error in [FIX_LIST.md](FIX_LIST.md).

## 13. Mod README (template)

```markdown
# <Mod name>
Why: one sentence.
Where it runs: client / server / both. Profiles from the catalog.
Depends on: mods, dbnative, backend addresses.

## Modules
| File | What it does | Loops (ms) | Hooks |

## Settings
Link to flags.lua and what to change in normal operation.

## Commands and exchange files
| File or command | Who writes | Who reads | Format |

## Log
Tags and what to look for on a failure.

## How to check
Stand command and what should be in the log.

## Known limitations
Links to FIX_LIST and WORKAROUNDS.
```
