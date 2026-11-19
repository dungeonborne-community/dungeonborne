# Loot and affixes: answers from the game code (11.11)

Read only. Sources:
- Ghidra on the exe image;
- BP: retoc → kismet-analyzer;
- levels (JSON from `research/leyline/json`);
- tables `backups/balance` and the season's;
- test stand logs `dbtools/run/*.log`.

Did not touch prod, Robot or the backend code.

The decompilation is in `research/loot/ghidra`:
- `engine.c` — the roll engine;
- `getters.c`;
- `affix.c`, `append.c`, `pick.c` — affixes.

The `Loot` function itself: `research/leyline/ghidra/loot3.c`. Source BPs: `research/loot/bp/*.bc.txt`.

Status: "verified" — seen in code or a log; "conclusion" — follows from the code, not measured in the game; "not verified" — with the reason.

---

## A. Native `UBattleUtility::Loot` (exec `0x142d8d820` → `0x142ed1bf0`)

### Order (verified)
1. Waiting hall (`DsUtility+0x1f4 == 1`): no loot.
2. `ClassName` → `ue_loot_source_id` via `loot_source_map`.
3. The `loot_ue_source_choose_group` row by key `(source, DsUtility+0x1d4, LootType)` (`0x14261f800`).
4. The row's `limit` (`+0x2c`) > 0: a counter of drops per `(source, LootType)` per match (`DsUtility+0x2c8`, `0x142ec1f10`). When reached, loot is empty.
5. For each `loot_id` of the row, if it is not in the set `DsUtility+0x1e0`: the engine `0x14236da40` → `0x142370410`.

### A1. Slot roll formula `loot_choose_group_rate` (verified, `0x142370410`)
```
n = min( (default_rate + B_flat) * (10000 + B_pct) / 10000 , max_rate )
rolls = n / 10000 (integer part) + 1 more with probability (n % 10000) / 10000
```
- `B_pct` and `B_flat` are bonus maps by `loot_id` in the subsystem (`+0x270` and `+0x280`, `0x142b5fab0` / `0x142ea0a30`).
  Did not find who fills them. Our matches have no entries (not verified, conclusion: 0).
- **`max_rate` is the cap after bonuses.** With zero bonuses: `min(default_rate, max_rate)`.
- **`default_rate` > 10000 gives several rolls:** 25000 = 2 guaranteed + 50% for a third.
- Slots are rolled independently, each `loot_id` of the row has its own calculation.

### A2. Choice within a slot (verified, `0x142370410`, row parsing at `0x142680c80`)
1. **Rare pools `rare_pool`** (`pool_id`, `basic_rate`, `min_rate`):
   - pool weight = `max(basic_rate × pool_coefficient, min_rate)` in units of 1 000 000;
   - the remainder up to 1 000 000 is the normal choice;
   - if the sum is ≥ 1 000 000, there is no normal choice;
   - the coefficient (double) is dynamic per-match state (`DsUtility+0x290`).
   In our tables `rare_pool` is empty, always the normal choice.
2. **Normal choice:** `random_ids` weighted by `pr` (`pr / Σpr`), `pr ≤ 0` skipped. Confirmed.
3. **`condition`** (`random_id`, `max_loot_cnt`, `unlucky_loot_cnt`, `priority`) — per-match state by `loot_id` (`DsUtility+0x2d8`):
   - `max_loot_cnt` > 0: after that many drops the `random_id` is excluded until the end of the match;
   - `unlucky_loot_cnt` > 0: a miss counter grows on every normal roll. When it reaches the threshold, this `random_id`
     is forced (if several, the one with the higher `priority`), and the counter resets.
4. **Item from the `loot` pool** (`ExcelItemDropItem`: `item_id`, `cnt`, `level`, `rate`):
   - weight `rate`, rows with `rate ≤ 0` are dropped;
   - rows are grouped by `level`: first the level by summed `rate`, then the item. The resulting probability is the same:
     `rate / Σrate` of the pool (conclusion);
   - `cnt` is the count, split into stacks by the item's max stack (`0x14236c4b0`).
   - **`level` does not affect affixes or rarity:** item creation (`0x14236c4b0`) receives only the id, the count and
     the corruption of the `loot_rate` row (`corruption_id`/`corruption_rate`). Found no other reads of `level` besides grouping
     (conclusion).

### A3. Players, group, mode, options (verified via BP and code)
- **Loot is shared, one call.** The BP calls `Loot` once per chest or monster:
  - `PlayerStates` = `GetPlayerStatesWithSameTeam(opener/killer)`;
  - `LootPlayerState` = the same player.
  The number of players does not enter formulas A1/A2.
- The player list is needed only for bad luck protection (A4) and logs.
- The loot map is `+0x1d4`, not `GameState.MapId` and not the options.
- **Options 5001/5002 and "Magic Corrosion":** they are not in `Loot` or the engine (not verified whether they are in the BP before the call).
  Item corruption comes only from `loot_rate.corruption_*` (A2).

### A4. Empty tables, if filled (conclusion from the code)
- **The source row's `limit`**: cap on drops of that source per match by `(source, loot_type)`.
- **`loot_unlucky_value`** (`rule_id` 1 EPIC / 2 EPIC_WEAPON / 3 LEGENDARY, `reward_score`, `priority`, `only_active_by_other`)
  and `loot_rate.unlucky_value`/`unlucky_group`:
  - "bad luck" accumulates per group for each player in `PlayerStates`;
  - at `reward_score` the choice in the pool is forced (`0x14236f1a0`, `0x1425bcd80`).
  Empty table: the branch does not fire.
- **`loot_magic_found`** (`level`, `coefficient`):
  - the engine can raise item `rate` by `coefficient %` (`0x142370410`, branch `param_4[4]`);
  - `Loot` always passes flag 0: **magic find does not work in our exe**, even if filled.
- **`loot_disable_loot_id`**: by meaning the set `DsUtility+0x1e0`: such `loot_id`s are skipped. Did not find what fills it
  (not verified).

## B. `loot_type` by source class (verified via BP)
| Source | Who calls `Loot` | `LootType` |
|---|---|---|
| Monsters, bosses, thief goblins (`LootBlue`/`LootGreen`/`LootPurple` and `H_`), mimics (`BP_Tgf_Monster_Chest` ← `BP_Tgf_Monster`) | `BP_Tgf_Monster.GenerateLoot` | **0** |
| Chests (`BP_InteractableChestBase` and children) | `BP_InteractableChestBase` | **0** |
| Barrels and pots (`BP_DestroyableBase`) | `BP_DestroyableBase` | **0** |
| Altars `BP_Altar_Award` / `_Special_*` | `CurrentLootType` | **3** — seal, **4** — legendary (analysis 08.11) |

The only difference between a thief goblin and a regular goblin is the **class name** (`ClassName` → its own `ue_loot_source_id`).

Values 1, 2 and 5 do not occur in the checked BPs. Did not do a full pass over all BPs (not verified). The tables
have rows of types 0–5 for every (source, map) pair.

## C. Loot map `+0x1d4` on the battle server from the kit (verified via the test stand log)
Test stand logs `dbtools/run/dbserver.log`, `dbserver-7787.log`, `[LOOTMAP]` lines from RuServer `LootMap`:
- **On every new match the value is 0:** waiting hall `before_map=0 before_loot=0`. Without LootMap all matches would use map 0.
- After the mod writes it, it "holds" after 30 s.
- **On the second floor and on the main map the value persists:** `L_Dungeon_Official_02 … before_loot=10102`,
  `L_GothicCastle_WP_Main … before_loot=10201`. It resets only with a new match.
- Did not look at Robot (forbidden). The same line in its `dbserver.log` is needed.

## D. Census of sources by level — partial
**Verified (`L_Dungeon_Official_01_Classic` / `_Hard`, `PersistentLevel` actors):**

| Actor | Classic | Hard |
|---|---|---|
| `BP_ActorSpawnPoint_C` | 402 | 403 |
| `BP_ActorSpawnSet_C` | 93 | 93 |
| `BP_ActorSpawnRoom_C` | 53 | 53 |
| `BP_ActorSpawnPoint_Destroyable_C` (barrels, pots) | 9 | 9 |
| `BP_ActorSpawnPoint_SpecialAward_C` (altar) | 5 | 5 |
| `BP_TreasureChest_Gold_01_Special_C` (placed directly) | 2 | 2 |
| `BP_TreasureChest_Special_02_C` (placed directly) | 1 | 1 |

Monsters and almost all chests are placed **via spawn points with `DT_ActorSpawn_*` tables** (class and probability per point).
A static census requires parsing the properties of all points and all DTs. **Not done**:
- the volume is large;
- castle levels use World Partition, cells are separate assets.

**`H_` on monsters.** The imports of Classic and Hard have almost the same set: regular and `H_` mixed. The difference:
`BP_TGF_Monster_Reaper_Special` (Classic) vs `BP_TGF_H_Monster_Reaper_Special` (Hard). In `loot_source_map` the `H_` ones have their own
ids, i.e. their own loot rows. What the letter means was not established from the code (not verified).

**Proposal: `[CENSUS]` in RuServer**, as a separate change (no changes made):
1. `RegisterHook("/Script/LyraGame.BattleUtility:Loot")`. The BP calls it as a UFunction, so the hook will fire. Log
   `ClassName`, `LootType`, the map and the number of items in the result.
2. Every 30 s: `FindAllOf` by base classes (`BP_Tgf_Monster_C`, `BP_InteractableChestBase_C`, `BP_DestroyableBase_C`,
   `BP_Altar_*`) counting by `GetClass():GetFName()`. No logging per object: 1800 HLODs caused stalls.
3. Aggregate 5–10 matches per level. This also yields spawn point probabilities.

## E. Affixes of an item from raid loot (battle server, `0x14236e690` → `0x1423731b0` → `0x142371a40`)

### E1. MINOR `random_rule` with several `affix_pool_id` (verified)
1. The `random_rule` has `pool_random` variants, each with a list of `affix_pool_id` and a condition. Among the variants
   with the highest priority **one** is chosen; variant weight = Σ `sum_weight` of its pools (`0x1423731b0`).
2. The number of properties comes from `affix_count_pool_id` (`0x142371730`).
3. **The properties of all pools of the chosen variant are merged into one list weighted by row** and drawn without replacement
   (`0x142371a40`). A property whose attribute is already on the item is excluded from the list.
   **"Pool first, then property" does not happen.** The message of commit 7feefdf ("the backend picks like the game: set first") contradicts
   this. If the backend picks the pool first, its odds differ from the game's: `affix.rs` is worth rechecking.

The chance of a bonus from 100802 per extra property ≈ Σweights of 100802 rows / Σweights of rows of all pools of the variant, adjusted
for drawing without replacement and exclusions.

**Important for 100802.** The variant with 100802 in the season template has the condition `monster_rule_id: 40101`. For type 3
(`monster_rule_id`), with known `level_id` and `monster_id`, the server looks up `affix_monster_pool_rule (40101, monster, level)`.
If the row exists, **the variant's pool list is replaced** by its `affix_pool_ids`; for 40101 that is `[101401]`, without 100802.

So 100802 works:
- on chests and barrels (no monster);
- on monsters that are not in `affix_monster_pool_rule` 40101 for that level.

On monsters from the list (`monster_id` 249/212/255 etc.) it does not work.

### E2. Rule tables (verified from code, values not measured)
- **`affix_count_random_pool`**: how many properties a rule gives (`affix_count_pool_id`). Value −1 means take all.
- **`affix_level_pool_rule`** (`rule_id`, `level_id`, `affix_pool_ids`): the `level_rule_id` condition. With a known
  `level_id` it replaces the variant's pool list.
- **`affix_monster_pool_rule`** (`rule_id`, `monster_id`, `level_id`, `affix_pool_ids`): the `monster_rule_id` condition.
  Replaces the list when all three keys match.
- **Corruption** (`corruption_type`): priority 999: a corrupted item takes only such a variant.
- **`affix_value_template.rarity_range`**: the property value range is chosen by the item's rarity (strings
  `convert_random_attribute_affix_data … affix_value_id {} rarity {}`). Did not analyse the formula (not verified).

### E3. Does the battle server read season `affix_pool`/`affix_template`? — **not on the test stand**
- The server loads tables via `config_set_Excel*::load_file` from `Content/AdditionalData/ExcelConfig` (strings in the exe).
  The balance pak `zzzzzz_Balance_P` overrides them.
- **The pak on the test stand's battle server** (`C:\Steam\…\Paks\zzzzzz_Balance_P.pak`, 08.11 16:58) contains:
  - `affix_pool`, `loot*` (4 tables), `activity_center`, `quest_list`, `trade_commodity`;
  - **no `affix_template`**;
  - **no pool 100802 in its `affix_pool`** (varint id search: 100801 — 1 hit, 100802 — 0).
- The pool was added by commit 7feefdf on 09.11; the server's pak is older. **On the test stand the battle server does not give 100802 bonuses.**
- Not checked on Robot. Needs a season pak after 7feefdf with **both** tables, `affix_pool` and `affix_template`.

## F. Gear set bonuses
- **Thresholds**: `affix_gear_set_rule` (`gear_set_type`, `level`, `unlock_level`). Kleptomancy (11002) Lv1 — 8 points.
  The server looks them up via `get_by_gear_set_type_level` (string in the exe).
- **Point sum**: the set level on the hero `GearSetLevelMap` / `AddGearsetLevel` / `AddDeltaGearsetLevel`
  (`TgfGearsetLevelAbilityItem`, strings in the exe). Whether both items' `gear_set_random` and gems in relics count there
  was not confirmed from the code (not verified).
- **"Tipsy Alchemist" (Kleptomancy):**
  - the game has the attribute `GearSetKleptomancy` (`GetGearSetKleptomancy` in the attribute set), icons
    `T_ItemGearSet_Kleptomancy`, `D_Icon_Buff_Gearset_Kleptomancy`;
  - there is no separate GA or AbilitySet with this name in the pak. The effect is probably in the C++ damage calculation by attribute;
  - whether it fires only against players is not verified from code (only from the description text);
  - in-game check: two players, one with 8+ Kleptomancy points, hit a player and a monster, watch the attribute and the effect.
