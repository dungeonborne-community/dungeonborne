# Crashes during long play (analysis of 07.11)

Sources: 119 dumps in `%LOCALAPPDATA%\Dungeonborne\Saved\Crashes` on the PC (31 since 01.11), 7 crashes on Robot over 04–06.11 (logs `Kit\logs\ue-*.log`). Static analysis: stacks are symbolized by the `UE4SS.dll` exports, Lua functions identified by decompilation in Ghidra. The dumps are small (stacks only), so frames below the frames from the UE log were found by scanning the stack and may contain garbage.

## Conclusion

All long-play crashes I analyzed happen in `UE4SS.dll`, i.e. in our mod layer, not in the game code. The game thread crashes while executing mod Lua code. There is enough memory: these crashes show no signs of a leak, see the `[PERF]` check below.

Duration matters as accumulated risk: the longer the game runs, the more Lua calls. The crashes are not tied to a specific minute: 6, 16, 22, 55, 61 and 87 minutes after launch.

## Crashes with a long uptime

| When | Where | Since launch | What crashed |
|---|---|---|---|
| 05.11 18:02 | local DS | 87 min | Lua core, `+c77a33`, table lookup, address `0x35` |
| 06.11 08:23 | Robot 7777 | 16 min | Lua core, `+c78829` (`mainposition`) ← `luaH_newkey` ← `lua_rawset` ← `FindAllOf` |
| 06.11 11:22 | Robot 7807 | 22 min | `+c6a0d8` ← `auto_construct_object` ← `FindAllOf` |
| 06.11 13:13 | Robot 7797 | 6 min | `+2aea33` during a Lua call |
| 06.11 17:45 | local DS | 55 min | `abort` in UE4SS |
| 06.11 21:11 | **client** | 61 min | read of address 0, `get_string` ← `FindAllOf(Lua)` |

`+0x2f3c50` is the Lua function `FindAllOf` (the decompilation shows `is_string`, `get_string`, `FindAllOf`, `prepare_new_table`, `auto_construct_object`).

## Cause: the worker thread and the game thread running Lua at the same time (confirmed by a stress test)

All mod loops are built as `LoopAsync(... ExecuteInGameThread(...))`: the worker thread runs Lua while the game thread runs Lua of the same mod. In total: 6 loops in RuClient, 1 in RuServer, 2 in JesterSwap, plus 8 `ExecuteWithDelay` calls in RuClient.

Signs in the real crashes:
- In the client crash of 06.11 21:11 `FindAllOf` checked `is_string(1)`, and the following `get_string` returned a null pointer. This cannot happen in a single thread.
- On the server, the table key in `lua_rawset` turns out to be garbage.
- In two analyzed dumps, at the moment of the crash 4 more threads were waiting on the Lua lock (`UE4SS+0x1356cf0`).

### Stress test on a local DS (07.11)

A temporary mod, the same load: `FindAllOf("Actor")` and table creation every 5 ms. No clients, the other mods run as usual.

| Mode | Run 1 | Run 2 |
|---|---|---|
| `LoopAsync` + `ExecuteInGameThread` (as now) | crashed in about 10 s: `abort`, the same signature as 05.11 21:30 and 06.11 17:45 | crashed in about 10 s: access violation in a Lua table inside `FindAllOf`, the same chain as 06.11 08:23 on Robot |
| `LoopInGameThreadWithDelay` (game thread only) | 12 minutes without a crash | 12 minutes without a crash |

Why the crashes come over time: there is a risk at every thread overlap. At the normal call rate it takes minutes and hours, under stress — seconds.

Limitations:
- Few runs: two per mode, the load is artificial.
- In the game-thread-only mode the other mods kept running via `LoopAsync` at the normal rate, so they could crash the game too.
- There are no sources or symbols for this UE4SS build, so the corruption mechanism inside Lua was not analyzed.
- The fix has not been checked in real matches.

## Fix (07.11: done in the repository and on the stand, not deployed to prod)

What was done:
- RuClient (6 loops, 8 delays), RuServer (main loop), JesterSwap, StatWatch and ClientProbe run only in the game thread.
- An exit watchdog `op=quitwatch` was added to `dbnative`.
- Timers are registered only from the game thread via `ExecuteInGameThread`: registering at mod load hung the client.
- On the stand, 3 match rounds with a server reset passed 3 of 3, the exit watchdog was checked.

Verification and plan: [MODS_REFACTOR_PLAN.md](MODS_REFACTOR_PLAN.md), stage 1. The original plan is below.

## Proposed fix

1. Replace `LoopAsync(ms, function() ExecuteInGameThread(fn) end)` with `LoopInGameThreadWithDelay(ms, fn)` in RuClient, RuServer and JesterSwap, as well as in StatWatch and ClientProbe. Replace `ExecuteWithDelay` in RuClient with `ExecuteInGameThreadWithDelay`: the function exists in this build, its behaviour needs checking.
2. RuServer: `CheckControlAsync` and the forced exit `QUIT_FORCE_SEC` run from the worker thread. If the server hangs, the game loop will not be able to exit. The watchdog needs to be in a separate C thread inside `dbnative`: killing a hung process under ACE from outside did not work (30.10). Plan: [MODS_REFACTOR_PLAN.md](MODS_REFACTOR_PLAN.md), stage 1.
3. Reduce `FindAllOf` in ticks: cache the object list, poll less often. On its own this does not remove the cause.
4. Verification: a long raid with clients on the stand, then one instance on Robot, compare the number of crashes per day with the baseline of 3 per 136 launches.

Memory is not the cause: on Robot `private` stays within 2–4.3 GB over hours-long runs.

## Other crash groups

- **Stale objects:** `GetFullName`, `GetPathName`, `ToString`, `ProcessEvent` on an invalid object, mostly 01–02.11. Fixed with an `IsValid` check, see the memory note `dungeonborne-ue4ss-null-crash`. Most such places are already fixed.
- **`NotifyOnNewObject`:** signature of 04.11 on Robot, fixed in B13b.
- **StatWatch hooks in a match:** B13c, R1-bo, StatWatch on the DS is disabled.

## No crash: the inventory stops working after 30–40 minutes of play (playtest 07.11)

Players complain that after about 30–40 minutes of play the inventory stops working. The user believes it is the same thread race in the mods as above, just without a game crash.

What was checked (backend journals, 07.11):
- **The network is not to blame.** Gateway 6601 connections through the relay live from a few minutes to 2.7 hours, there is no drop at 30–40 minutes. The short 443 connections from the player's screenshot finish with full volumes (↑524 ↓1494 bytes), their "disconnect without close_notify" is just an abrupt close.
- **The server did serve the inventory.** `Sinni` had dozens of `inventory_item_move`, `role_wear` operations and purchases at 10:50–10:54 UTC.

So the failure is on the client side. The thread race in RuClient fits: Lua in the worker thread running at the same time as the game thread corrupts state, and the client does not crash but stops handling the inventory window.

Not confirmed:
- no client logs at the moment of the failure: `Saved\Logs\Dungeonborne.log` and `crash_ue4ss.log`;
- not checked whether the client still sends inventory requests after the failure.

Verification: the stage 1 fix was deployed on the evening of 07.11. If the inventory complaints disappear along with the crashes, it is the same cause. If they remain, client logs and the failure time are needed.
