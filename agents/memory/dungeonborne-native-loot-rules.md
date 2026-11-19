---
name: dungeonborne-native-loot-rules
description: Native DS loot roll + raid affix roll rules (LOCAL Ghidra 11.11) — research\loot\GAME_CODE_ANSWERS.md; backend affix.rs diverges
metadata:
  visibility: team
  node_type: memory
  type: reference
---

LOCAL's answers from the game code (11.11): `<data>\research\loot\GAME_CODE_ANSWERS.md` (function addresses).
- Slot: n = min(default_rate, max_rate) (+bonuses, 0 for us); rolls = n/10000 whole + the fraction as a chance → >10000 = several rolls.
- There are unused mechanics: `condition` (max_loot_cnt — cap per match, unlucky_loot_cnt — guarantee), `rare_pool`,
  source `limit`, `loot_unlucky_value` (per player). `loot_magic_found` does not work in our exe (flag 0).
- loot_type: all monsters/chests/barrels = 0, altars 3/4; 1, 2, 5 do not occur. Loot is shared, 1 call per source.
- Raid affixes: the properties of all pools of the chosen pool_random variant are merged by weight (NOT "set first", as in
  backend affix.rs). Seasonal 100802: ~21–28% per item in a raid vs ~14–17% in the backend. The variant condition
  monster_rule_id 40101…40113 replaces the pools with affix_monster_pool_rule for the listed monsters (without 100802).
- The source census is not done statically (spawn points + DT_ActorSpawn, the castle is World Partition) — [CENSUS] in RuServer is needed.

Related: [[dungeonborne-stat-watch]], [[dungeonborne-seasons]].
