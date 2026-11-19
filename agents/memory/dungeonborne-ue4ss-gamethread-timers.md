---
name: dungeonborne-ue4ss-gamethread-timers
description: "UE4SS LoopInGameThreadWithDelay — return true does not stop it, register only from the game thread (otherwise a hang while the mod loads)"
metadata:
  visibility: team
  node_type: memory
  type: project
---

07.11, experiment on the stand (DS) and a client hang:
- `LoopInGameThreadWithDelay` does not stop on `return true`, only on `CancelDelayedAction(handle)`; an error in the callback does not kill the loop; timers survive a map change, during loading calls are skipped, not queued. `ExecuteInGameThreadWithDelay(ms, fn)` → handle (the second form requires a numeric handle).
- Registering a timer from the mod loading thread (the main.lua body) hung client F: at start (CPU 0, frame 0); the server with the same code started. Rule: register only on the game thread — `ExecuteInGameThread(function() LoopInGameThreadWithDelay(...) end)`. Fix in RuClient (GameLoop/GameDelay), RuServer, JesterSwap, StatWatch, ClientProbe; on the client not yet verified as of 07.11.

**Why:** moving mods to the game thread (crashes, [[dungeonborne-lua-thread-crash]]).

**How to apply:** new code — only through these wrappers; design `docs/MODS_DESIGN.md` §5.
