# Why Leyline Seals do not drop

Analysis from 08.11 at the orchestrator's request (match 13:39–13:47, `L_Dungeon_Official_01_Hard`, GameState.MapId = 10103: two bosses
and many mobs, not a single seal). Static only: Ghidra on the exe image, tables, mod Lua. I did not change code or
run the game. Decompilation: `<data>\research\leyline\ghidra` (`loot3.c` — `Loot`, `loot4.c` — row lookup).
Items, the altar and spawn points — in `docs/LEYLINE.md`.

## Conclusion
**The native loot picks the `loot_ue_source_choose_group` row not by `GameState.MapId` but by the field `UBattleDsUtility+0x1d4`
(loot map). Only `InitGameMap` sets it, and on our listen server it is not called, so the field = 0. All our matches
take the loot of map 0 rows (the default). Map 0 rows have no seals: they are only in rows 10102/10103/10203.**

It also follows: **Classic/Hard loot has never been applied for us at all** — any map gives map 0 loot. This matches the
observation of 30.10 in RuClient (`main.lua`, the `[F8 LOOT]` block): "Loot is called with GameState.MapId=10102, but the drop is like map 0".

There is no filter on the seals themselves: not by item id, not by `is_forbidden_bring_into_battle`, not by the 919xxx slot range,
not by map/season options. `loot_disable_loot_id` and `loot_magic_found` are empty.

## 1. How the battle server gives out loot
One path for chests, monsters, bosses and altars: the BP calls `UBattleUtility::Loot(Owner, SourceName, UserKey,
PlayerStates, LootPlayerState, ClassName, LootType)`. exec `0x142d8d820` → implementation `0x142ed1bf0`:

1. `DsUtility = FUN_142ebba20(GameInstance+0x1b8)` (UBattleDsUtility).
2. If `DsUtility+0x1f4` (scene) == 1 (waiting room) — no loot (`0x142e9e8a0`).
3. `ClassName` → source id via `loot_source_map` (config `+0x29a0`); not found — error, no loot.
4. Source row: `0x14261f800(config+0x2a60, source, DsUtility+0x1d4, LootType)` — hash over the triple
   `(ue_loot_source_id, map_id, loot_type)` (`ExcelLootUeSourceChooseGroup`). `DsUtility+0x1d0` (map_id) is read
   nearby, but is not part of the key.
5. Row `limit` (`+0x2c`) > 0 — check of the per-match grant counter (`0x142ec1f10`); our rows have 0.
6. Then the usual roll of slots (`loot_choose_group_rate`) and pools (`loot`), bad luck (`LootUnluckyValue` rule 1/2
   not found — only a log line, the roll continues).

**Where `+0x1d4` comes from:** `UBattleDsUtility::InitGameMap(map_id, timeout, independent)` (`0x142e9c1e0`): `+0x1d0 = map_id`,
`+0x1c0 = map_detail row` (config `+0x2e20`), `+0x1d4 = row+0x60` (by meaning `loot_map_id`; for our maps
equal to map_id). A subsystem reset zeroes `+0x1d0..+0x1d7` (research\hallleave\l4.c). On listen we do not call `InitGameMap`
(it breaks the transition, WORKAROUNDS K19) — the field stays 0. The native `Loot` does not read `GameState.MapId`, which
RuClient/RuServer set.

**`loot_type`** — an argument of the call from the source BP (monsters and chests have their own), does not depend on the match. Our rows for types
0–5 are identical, so it does not matter for seals. The altar passes `AwardId`: 3 for a seal, 4 for a legendary one.

## 2. Filter on seals
None. The only cause is map 0 in the key. Discrepancy with FIX_LIST R1-loot (05.11, "chests follow GameState.MapId"):
that conclusion was drawn from the composition of arena loot when switching the season pak, not from code; per the code, `Loot` does not read `GameState.MapId`.

## 3. Altar `BP_Altar_Special_Dungeon`
Needs a seal (1900004) or a legendary one (100100) in the inventory; no map option needed. Loot — the same `Loot` with the key
`(300, 0, 3|4)`; we do have map 0 rows for sources 299/300 (g047/g048, `maps = [0, …]`), so the reward
drops even with map zero. It stands on `L_Dungeon_Official_01_Classic` and `_Hard`: 5 points
`BP_ActorSpawnPoint_SpecialAward` in "phase" rooms (`WarmUp`); whether it appears for us — not checked (the same
rooms do not place exit devices, see LEYLINE.md).

## What to change
- **Quick, tables only (playtest):** in `content/seasons/playtest/loot/loot.toml` put the seal slots into map 0 groups
  (any match reads them now). Downside: seals will drop on Casual too, with no difference by difficulty — but there is none
  now for any loot either.
- **Properly (native, user's decision):** after the floor loads set `UBattleDsUtility+0x1d4 = loot_map_id`
  (and `+0x1d0 = map_id`) — in dbnative, before the first `Loot`. There is no reflected field, it cannot be set from Lua. Offsets are for
  the current exe. Then per-map loot (10101…10203) will work in full, including seals; the tables of maps 10102/10103/10203
  will need to be rechecked (they have never been applied).
- GameState options in RuServer do not affect the choice of loot row.
- Check after any change: a hook on `ExcelLoaderConfigGroupWrapper:GetRowExcelLootUeSourceChooseGroup_Of_UeLootSourceIdMapIdLootType`
  already exists in RuClient (`[LOOT lookup] source map type`), but it catches only calls from BP/Lua; the native `Loot` calls C++
  directly. More reliable — kill a boar on Hard and see whether a fragment dropped (20% after c652d8e), or read `+0x1d4`
  from dbnative.

## Done 08.11 (user's decision: "properly")
- **dbnative** `op=lootmap` (`game/ue4ss/native/dbnative/dbnative.c`, `OpLootMap`): `ds=<UBattleDsUtility address> map=<map_id>
  loot=<loot_map_id>` writes `+0x1d0`/`+0x1d4`; without `map`/`loot` it only reads. Before writing — `CheckPrologues` (exe build),
  `VirtualQuery` (the address is writable), old values must be 0…999999.
- **RuServer** `LootMap` (`main.lua`, called from `Ready`): `map_id` from the backend job (`H.MapId`), `loot_map_id` — from
  `map_detail` (otherwise the same). Writes on every new map (hall, floors, new match), rereads after 30 s and on reset
  writes again. Log lines `[LOOTMAP]`. Only maps 10101–10299: there are no loot rows for arenas (30101…).
- Stand 08.11 14:33: dbnative and RuServer are installed in the Steam install (battle server), dbnative — on F: too. **Not checked in a match;
  not deployed to Robot.** After the check — verify Classic/Hard loot (PVE tables, they have never worked before).
