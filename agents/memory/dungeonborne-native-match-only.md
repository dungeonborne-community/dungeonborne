---
name: dungeonborne-native-match-only
description: "Since 04.11 match is native only: custom match button and all local-match UnLua (LocalDev, Private*) removed from paks"
metadata:
  visibility: team
  node_type: memory
  type: project
---

2026-11-04 (user asked): custom «find game» button and local-match UnLua removed; matchmaking is the game's own
(pre_search/search → room → DS on Robot).
- `zz_MatchLocal_P` now ships the ORIGINAL `WBP_MatchMainButton.lua` (byte-identical to pakchunk0-Windows.pak) — it
  exists only to overwrite the old wrapper still sitting on players' PCs (launcher never deletes files).
- `zzzzz_RebuildPrivate_P` = only `DebugConfig.json` + `affix_pool.bytes`. `match-mode.txt` is read by nobody now.
- Removed sources: `backups\removed\20261104_local_match\` (LocalDev, PrivateRaid/World/Loot/Exit, LocalSettle,
  LuaProbe, AttrIds, both button wrappers). Server mod covers WP streaming itself (`wp.Runtime.EnableServerStreaming 0`).

**Why:** the native flow (dbnative data, native game over, waiting room) replaced the local-battle path; the Lua
wrappers had known bugs (extra MINOR affixes, attribute resets) and made fresh clients default to a local battle.
Backup/rollback: `backups\backup\20261104_before_match_removal\restore.ps1`. Launcher 1.1.1 carries it
(`<data>\dist\DungeonborneLauncher-1.1.1.exe`); 1.1.0 clients still have the old local-battle wrapper.
PrivateLoot was never active in the native flow (hooked the loot UI, host-only); gear doesn't drop anyway.
**How to apply:** don't reintroduce Lua in the match button; LuaProbe only via debug pak `backups\debug\luaprobe`. Related: [[dungeonborne-dedicated-plan]], [[dungeonborne-client-probe]].
