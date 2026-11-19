---
name: dungeonborne-ue4ss-null-crash
description: UE4SS Lua — GetFName() on a null UObject wrapper is a native AV at 0x18 that pcall does not catch; check IsValid first
metadata:
  visibility: team
  node_type: memory
  type: reference
---

In UE4SS 3.0.1 Lua, a property holding a null object still returns a truthy wrapper. Calling `:GetFName()` on it is a
native crash (`EXCEPTION_ACCESS_VIOLATION reading address 0x18`, stack all in UE4SS.dll); `pcall` does not catch it, and
the process dies about 2 s later, which makes the last log line misleading. It killed the DS twice on 06.11 and I first blamed
the wrong array read.

**How to apply:** call `:IsValid()` before `GetFName`/`GetFullName` on anything read from a property, and test new
diagnostics on an empty DS before a real match. See `Spect.ObjName` in DungeonborneRuServer. Related: [[dungeonborne-bp-reverse]].
