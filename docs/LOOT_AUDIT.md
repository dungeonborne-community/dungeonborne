# PVE loot audit — stage 1 (11.11)

The plan (stages 1–4) and decisions — in the correspondence of the session "Tables: loot rules, chances and bonus sets". How the game rolls loot and
affixes — `<data>\research\loot\GAME_CODE_ANSWERS.md` (analysis of LOCAL from the exe). Detailed tables —
`content/loot/audit_report.md` (generated), census — `content/loot/census_report.md` (after matches on the test stand).

## Tools
```
python -I -X utf8 tools/loot_audit.py [--season playtest] [--out loot/audit_report.md]
python -I -X utf8 tools/loot_census.py <RuServer logs…> [--season playtest] [--out loot/census_report.md]
```
- `loot_audit.py` — tables only: integrity, items, sources per map, average per source by class and
  rarity, the chance of "at least one" item of a rarity, growth by difficulty, `H_` vs ordinary, season vs base.
  The formula is as in the game: `n = min(default_rate, max_rate)`, `n // 10000` rolls plus one more with chance `(n % 10000)/10000`;
  pool by `pr`, item by `rate`.
- `loot_census.py` — RuServer logs (`[CENSUS]`, `[LOOT]`): how many sources of each class are on the map per match and how many of
  them players opened/killed; expected loot per match "everything on the map" and "actual".
- RuServer (`LootLog`, 11.11): `[LOOT]` — hook on `BattleUtility:Loot` (source, type, loot map, instance); the first 3
  calls log the post-hook arguments (checking whether the array of dropped items is visible). `[CENSUS]` — every 30 s `FindAllOf`
  over `BP_Tgf_Monster_C`, `BP_InteractableChestBase_C`, `BP_DestroyableBase_C`, `BP_AltarBase_C`, `BP_CollectableBase_C`,
  unique instances per match, a line only on change. Installed on the test stand (Steam), not deployed to Robot.

## What is in effect on prod (checked 11.11, read-only)
- The balance pak on Robot `490f4b01…` (08.11) — the tables `loot*`, `affix_pool`, `affix_template` are byte-for-byte equal to the seasonal
  `balance_pak` build (pool 100802 is there, 80 items). The audit report describes prod.
- Loot map: Robot logs 10.11 — `[LOOTMAP]` sets 10102/10103/10202/10203, holds after 30 s. Per-map loot on prod
  works since 08.11; before that every match took the rows of map 0.

## Table audit results (playtest season)
1. **Integrity — no errors:** 10 787 source rows, 429 slots (83 without references — `no_source`, tutorial slots),
   169 pools (41 without references), 8 593 item rows; no dangling references, empty pools or unknown items.
   No slots with chance > 100% (several rolls). Guarantees/caps (`condition`, `rare_pool`, `limit`,
   `loot_unlucky_value`) are not used; `loot_magic_found` does not work in our exe.
2. **What equipment can drop at all:** rotten 25 of 25, grey 37/73, green 37/117, blue 37/73,
   purple 43/83, pink 15/51, legendary 16/122, unique 11/21. The rest is not in the loot (vendor, crafting,
   rewards).
3. **Sources:** on all 6 maps the same 256 sources have rows. Without loot, apart from the obviously empty ones (Noloot,
   traps, buff altars, tutorial): `BP_TGF_VirtualMonster_GoblinThiefDrop_C`, `BP_Altar_Lock_01_C`, `BP_TGF_ImmaculateTrial_*`
   — no rows anywhere, same as in PVE.
4. **Loot types:** 1, 2, 5 — copies of type 0 (the game does not pass them); 3/4 differ only for seal altars.
5. **`H_` monsters:** 33 pairs `BP_TGF_H_*` / `BP_TGF_*` — the loot is the same on all 6 maps. The letter distinguishes them only on
   map 0, it can be ignored for our own tables.
6. **Growth by difficulty (normal → Classic → Hard):** rarity grows for all classes except the ordinary monsters of the Crypt
   Classic → Hard (45 sources, rarity points −10%: less green and blue, the same purple).
   **The value of non-equipment loot (treasures) drops as difficulty grows** for chests, barrels, mimics and golden chests:
   Crypt golden chest 2609 → 2270 → 1931, barrel 44 → 31 → 21 (on Hard there are more consumables instead of treasures).
   Gold from monsters: Crypt 4.8 / 9.5 / 16.4, Castle 4.8 / 4.8 / 14.1 — Castle Classic is the same as normal.
7. **The normal Castle is poorer than the normal Crypt** at the same difficulty: boss — purple 0.23 vs 1.17, pink 0 vs
   2.5%; there are no pinks at all in the normal Castle.
8. **Pinks (season ×10, 25 slots of pool 900012):** boss 2.5 / 12.5 / 27.5% (Crypt), 0 / 5 / 20% (Castle); elite
   0.2 / 1.4 / 3.6%; goblin thief 0.7 / 3.2 / 6.9%. Pink and golden treasures — only seal altars.
9. **Set bonus in the raid** (per the game rule — pool merging): ~21–28% for a suitable extra property of a blue/purple
   item (`affix.rs` for the vendor gives 14–17% — they diverge). Monsters from `affix_monster_pool_rule` (conditions 40101…40113)
   give no bonus.

## Our own tables (stage 3) — uniques (11.11, user's decision)
Template `content/seasons/playtest/loot/loot.toml` → `cargo xtask loot build --season=playtest`. Not deployed.
- **Non-working items (11.11):** in raid loot all legendaries/uniques have an effect (`legendary_rule` +
  ability, `research/legendary/mech_table.md`). Items without an effect (11 weapon types, all legendary armour,
  necklaces and rings — 60 on the market) and 6 ice staves are taken off the shelf by `tools/py/season_hide_items.py` → the seasonal
  `trade_commodity.json` (`is_on_shelf = false`, 65 items); backend `trade_seed_system_lots` removes system lots
  without bids for goods not on the shelf. The ice staves (181050002…181054002) are removed from all loot pools; on maps
  10101…10203 the shares and item counts did not change, on map 0 (PVP) the staff pool of the PVP chest became empty — the slot does not drop.
- **Grey and rotten not on the markets (12.11, user's decision "green at minimum"):** they are not in the original `trade_commodity`
  (only UNCOMMON and above); on 06.11 they were added by `tools/py/auction_all.py`. Now `season_hide_items.py` takes
  NORMAL and ROTTED off the shelf: 63 + 24 items on each of the two markets (152 kinds off the shelf in total). The client gets -4205
  `EN_ERR_TRADE_PRODUCT_NOT_ON_SHELF`. Price limits (min/max from the standard price) — a separate backend edit.
- Uniques (11 items, pool 900016) — from the Monarch (`Shadow_King`) and the Wendigo (and their `H_`), groups `g107u`/`g120u`/`g142u`/`g155u`:
  Crypt and Castle Classic 0.5%, Hard 1%, normal — none. The rest of these bosses' loot is unchanged.
- For the other 17 bosses, elites and goblin thieves the unique slots are removed; uniques are struck out of pools 900011 (chests/altars: legendaries
  remain, their chance from this slot became a third higher — 0.01–0.05%) and 9000xx (legendary weapons on the floor: purples
  remain).
- The overall chance of a unique is 1 in 1 000 000 from every source on all maps: slot 0.01% (cannot be written lower in the table) ×
  pools `[900016, 900003]` with weights `[1, 99]` (otherwise — a handful of gold). Checked: all 1771 rows of other sources have exactly 1e-6.
- With ~235 loot calls per match and 100–600 matches per day: ~0.2 uniques per day from bosses, the overall chance — ~0.02–0.14.
- **Legendaries (16 items, pool 900015) — ×5 of uniques:** Monarch and Wendigo Classic 2.5%, Hard 5%, normal — none;
  overall chance 5 in 1 000 000 in the same shared slot (`pools = [900016, 900015, 900003]`, `pr = [1, 5, 94]`). For other bosses,
  goblin thieves, elites, chests/altars and monsters the legendary slots are removed (67), legendaries are struck out of the 12 "legendary weapons on the floor" pools
  (99.8% purples there). The user's target is ~50 per month.
- **Legendaries from others (11.11, second decision)**, normal/Classic/Hard: Monarch and Wendigo 0.5/2.5/5% (on normal — groups
  `g094u`/`g130u`), other bosses 0.05/0.1/0.2%, goblin thief 0.1/0.3/0.5%, elite 0.01/0.02/0.04%, golden/special chests and
  altars 0.01/0.01/0.02%, mimics 0.01/0.02/0.03% (moved out of the shared group with chests: `g…m`); the rest — the shared
  5 in 1 000 000. Map 0 — like Classic (Monarch/Wendigo there are like other bosses). Estimate at 150 matches/day and 25 kills
  of the Monarch and the Wendigo: ~1.9 per day, ~55 per month (more than half — Monarch and Wendigo).
- **Pinks (set armour, 15 items, pool 900012; 11.11)**, normal/Classic/Hard: all bosses, including the Monarch and the Wendigo,
  33/50/100% (one item), goblin thief 1/3/5%, elite 0.1/0.2/0.4%, golden/special chests and altars 0.1/0.1/0.2%, mimics
  0.1/0.2/0.3%, the rest 50 in 1 000 000 (shared slot: `pools = [900016, 900015, 900012, 900003]`, `pr = [10, 50, 500, 440]`).
  The previous seasonal ×10 of the pool (07.11) is replaced. Map 0 — like Classic. Collecting a set (5 needed out of 15, 34 pinks on average):
  solo with 2 bosses and a 50% extraction rate ~34 raids, a pair ~66, a trio ~98 (simulation; without trading). Server-wide — thousands per month.
- **Purples by source type (11.11)** — average per source normal/Classic/Hard: elite 0.5/1/1.3, chests
  0.03/0.06/0.1, monsters 0.005/0.01/0.015, barrels 0/0.005/0.01, mimics 0.05/0.1/0.15, special chests and altars
  ~0.14/0.28/0.37; bosses and goblins — as before. A rarity shift, the number of items per source did not change (checked across all
  classes and maps). Pools `P_noep`/`P_ep` (24 pairs) — copies of a pool without purples / only purples, the share is the slot's `pr`,
  tuned for each map separately. Groups of map 0 only (PVP and fallback loot) are untouched.
- **Elite — purple as a 5/10/15% ladder (12.11, user's decision).** It was on average 0.5/1/1.3 purples per
  kill (at least one — ~42/70/82%). Now the chance of at least one purple per kill: normal 5%, Classic 10%,
  Hard 15% — the share of purple pools (`*_ep` and 900010 — only purples) in each of the 3 rolls is 1.70/3.45/5.27%,
  average 0.051/0.104/0.158. The freed share goes to the other pools of the roll in the previous proportions (more blues/greens and treasures,
  the value of the rest from elites +18…36%). Item count, set bonus 25%, pinks/legendaries — as before; groups
  g088/g101/g114/g125/g136/g149; map 0 and all other sources did not change (checked).
- **Second floor without ley line seals and shards (13.11, user's decision, option A).** In the tables the floor was not
  distinguished: RuServer `LootMap` set the same loot map on the second floor as on the first (`L_Dungeon_Official_02: loot=10102`).
  Now on levels `*_Official_02*` and `*SecondFloor*` LootMap writes loot map +10: 10112/10113/10213 (only for
  10102/10103/10203 — the other maps have no seals). In the template: 36 groups with seal slots (pools 900112–900117) —
  `…f2` copies without these slots, the other 45 groups got the +10 maps added to `maps`. Checked: 1540 source rows for each
  new map = first floor rows without 1494 seal slots, the first floor rows did not change. `affix_monster_pool_rule`:
  the rows of levels 10102/10103/10203 are duplicated to 10112/10113/10213 (+768), the levels are added in `season_set_affixes.py`.
- **"Tipsy Alchemist" (Kleptomancy 11002) drops rarely (13.11, user's decision "give it a small weight").** In pool
  100802 weight 2 vs 10 for the seven other sets (2.8% of set bonuses vs ~13.9%): it has one tier at 8 points,
  and is assembled with two items. Weights are integers, in the base all = 1, so all season pool weights are ×10 — the battle server merges the properties of the
  variant's pools by weight, the proportions of the other properties are unchanged. Battle server (blues): the set bonus share in the extra property
  24.3% → 24.8%, "Alchemist" ~0.7%; purples from bosses — 2.8% of the bonus. Backend (`affix.rs`, items from the backend: reroll on
  Buyout etc.) picks a property inside the set by pool weight (`weighted_pick`; with equal weights — the previous rolls).
- **Blues/greens/greys (11.11, user's decision).** Bosses — without blues, greens, greys and rottens (the slot chance
  reduced by their share, purples/pinks/the rest — as before; normal Castle: a boss now gives ~0.56 equipment). Barrels, crates,
  pots — no equipment at all (and no shared slot of uniques/legendaries/pinks). The others (chests, monsters,
  elites, mimics, goblins, special chests) — among non-purple equipment blues/greens/greys(+rotten): normal 25/25/50,
  Classic 50/25/25, Hard 75/25/0; item count and purples did not change (checked). Pools `blue_/green_/grey_<map>` —
  equipment of that rarity collected from all pools of the map. Per ordinary raid (3 bosses, 140 monsters, 24 chests, 11 special):
  blues ~31 / 57 / 77, greens ~31 / 28 / 26, greys ~62 / 28 / 0.
- **Number of items per drop (11.11, user's decision).** Item slots (equipment, treasures, consumables) are replaced
  by rolls of "one item from a mix of the previous pools in the previous proportions": monster 0.5, chest 2, mimic 2.5, goblin thief 3,
  elite 3, special chests and altars 3.5, boss 5, barrels 0.3 (100% rolls + a fractional one with a chance). Purples — the previous
  average per source; gold, seals, portal stones, pinks, legendaries, uniques — separate slots, as before.
  Goblin thief: expensive treasures (900006, 900008) half as often — the value of its treasures ~2850 → ~1600. Boss of the normal Castle:
  few purples, so the 5 items are mostly treasures (value 4489 → 6801). Checked across classes and maps.
- **Consumables and ingredients ×4 (11.11, user's decision).** On top of the fixed items — slots `consumables`
  (potions, elixirs, flasks from 900004, 1–2 each, expectation per source ×4) and `ingredients` (essence of life and yarrow
  in equal parts, 2–4 each; now also from bosses, elites, goblins; scale — ×4 per Crypt Classic raid). Equipment, treasures,
  gold did not change. Per player per raid (180 mobs, 90 chests, 5 bosses, 9 players): consumables ~7 → ~28–31,
  ingredients ~1.6 → ~6.3–6.5.
- **Test stand 11.11, match Crypt Classic (loot map 10102 confirmed by `[LOOTMAP]`):** the user saw a purple roughly
  from every 3rd chest, per the tables — 0.05–0.07 from a wooden/iron one, 0.3 from a golden/special one. There was no loot log in that match
  (the orchestrator's RuServer without `LootLog`). Found: 100 slots had a `pr` weight sum of up to 100 000 (in the original
  the sum is always ≤ 100, pool weights are normalised to 30 000) — a possible truncation of the random number in the game; the weights were reduced to
  a sum ≤ 10 000. Recheck with recording what dropped.
- **Gold → ingredients (11.11, 12.11):** in all gold slots (except gold piles on the floor and groups of map 0 only) —
  murky essence of life and yarrow in equal parts (`res_s` ×1, `res_m` ×1–2, `res_l` ×2–3); blood and other herbs are removed from the loot
  (12.11, user's decision: of the ingredients only essence and yarrow).
- **Identical items (prod 12.11):** two enemies killed at the same time gave items with identical properties. The battle server's loot
  generator is xoshiro256 in a subsystem (`+0x268`, `0x142e97a30`), the seed comes from the function `(*0x1467c3ee0)(0)`,
  which looks like time in seconds; it is probably re-initialised — then two Loot calls in the same second roll the same.
  Not confirmed. Fix (if needed): before Loot substitute a random state via RuServer + dbnative.
- **Seals (11.11):** bosses — seal 100% (on Classic it was 60%); legendary seal from bosses — 2% (user's decision), the ordinary one — 20% from all its sources.
  The legendary seal altar — one treasure: 50% golden (900119, 70–320k), otherwise from the seal altar pool
  (900118, 1–100k); it used to be two golden ones (~328k).
- **Set bonus on purples — analysis (11.11, no edits).** The property variant with pool 100802 for purples is
  the condition `monster_rule_id` 40111/40112/40113 (for blues — 40101…40103, separately), the rule's property count is always 1
  (`affix_count_random_pool` 101). The bonus share now is 25% (40111), 22% (40112), 21% (40113) from any source.
  `affix_monster_pool_rule` (rule, `monster_id` = id from `loot_source_map`, `level_id`) replaces the variant's pool list
  for a monster on a level; the original has 24 rows (Monarch 212/265, Cyclops 241/249, Wendigo 254/255; levels
  10101/10103/10201/10203, pools 101101/101102/101401/101402). Which `level_id` our battle server passes — not checked.
- **Set bonus on purples — done (11.11, user's decision: bosses 100%, elite 25%, others 0%).**
  `tools/py/season_set_affixes.py` (function `purple_by_source`): for 43 purples pool 100802 is removed from their variant list
  (chests, monsters, vendor — no bonus); variants 40111 with a different pool list (weapons) got their own ids 40121…40125;
  the seasonal `affix_monster_pool_rule.json` (3084 rows): 22 bosses — only `[100802]` (the property is always the set bonus),
  10 elites — their own list + 100802 (as before: 25% / 21.9% / 20.6% per rules 40111 / 40112 / 40113); levels — map_id
  10101…10203 and level ids 40003015…40003020. The original rows for purples (the special pools of Monarch/Cyclops/Wendigo)
  are replaced, those for blues are kept. Test `affix.rs set_affixes_on_blue_and_purple` — now only 37 blues.
  **Check on the test stand:** a purple from a boss — with the bonus, from a chest — without (if from a boss without the bonus, the `level_id` is wrong).
- A guarantee on the 30th kill — an idea in `BACKLOG.md` (the native `unlucky_loot_cnt` does not fit: the counter lives for one match).

## Remaining in stage 1
- [ ] Census on the test stand: 2–3 matches on each of the 6 maps, clear the floors completely (monsters appear room by room);
  logs `dbtools/run\dbserver*.log` → `tools/py/loot_census.py`. In the first match check the lines
  `[LOOT] hook BattleUtility:Loot: ok` and `[LOOT] post-hook arguments`.
- [ ] After the census — loot per match per map and a comparison of maps (entry to stage 2).
