---
name: dungeonborne-lua-thread-crash
description: "Crashes in long play — a race between the worker and game threads in UE4SS Lua (LoopAsync + ExecuteInGameThread); a stress test reproduces the signatures, LoopInGameThreadWithDelay does not crash"
metadata:
  visibility: team
  node_type: memory
  type: project
---

07.11.2026: DS and client crashes in long play land in `UE4SS.dll`, in Lua or `FindAllOf` (`+0x2f3c50`), or `abort`. Stress test on a local DS: `LoopAsync` + `ExecuteInGameThread` crashed within seconds with the same signatures, `LoopInGameThreadWithDelay` ran 12 minutes twice. The mod fix is not done yet and waits for the user's decision.

Baseline on Robot (snapshot 07.11, `Critical error` in `Kit\logs\ue-*.log`, time from the log timestamps, not from file dates — file dates lie): since 06.11 3 crashes in 193 launches and 61 hours of uptime; over the whole period 7 in 311 launches and 90.5 hours. Table in `docs/MODS_REFACTOR_PLAN.md`, section 3. The same UE4SS build has `ExecuteInGameThreadWithDelay`, `CancelDelayedAction` and `RegisterEngineTickPreCallback`.

07.11 playtest: "the inventory stops working after 30–40 min" — the server and network are fine (6601 sessions up to 2.7 h, inventory operations go through), the user believes it is the same thread race on the client; check — whether it goes away after deploying stage 1 (`docs/CRASH_LONG_PLAY.md`, last section).

**Why:** mod loops run Lua on a worker thread at the same time as the game thread; few runs, the mechanism inside Lua is not analysed.

**How to apply:** in new mod code do not use `LoopAsync` with Lua work, use `LoopInGameThreadWithDelay`. Details and plan: `docs/CRASH_LONG_PLAY.md`. Symbolize stacks by `UE4SS.dll` exports. Related: [[dungeonborne-ue4ss-null-crash]].
