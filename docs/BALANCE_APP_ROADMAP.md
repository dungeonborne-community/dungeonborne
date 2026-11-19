# Balance Studio roadmap after 1.0

Status: 2026-11-06, stages 1 and 2 are done (the stage 2 control raid has not been done), the rest not started. Basis: [BALANCE_APP_SPEC.md](BALANCE_APP_SPEC.md), sections 11–12. Work size is given
relative to each other: **S**, **M**, **L**.

| Stage | What the designer gets | Size | Release | Depends on |
| --- | --- | --- | --- | --- |
| 1. Economy and XP (done) | Sell prices, market, dismantling, crafting, insurance, XP | S | 1.1 | Nothing |
| 2. Loot (done, raid not checked) | Chances and weights per map, average drop per source | L | 1.2 | In-game check (step 2.0) |
| 3. Affixes and final stats | Pool contents, final hero attributes | M | 1.3 | Decision on weights (step 3.1) |
| 4. Skills and monsters | At first only the answer whether this can be changed | M, research | None | Nothing, can run in parallel |

Releases are independent: the kit format stays `dbbalance-kit/1`, the patch format `dbbalance-patch/1`, new things are only added.

**Recommendation:** show release 1.0 to the designers before stage 2. Stage 2 is the largest, and it depends most on how they
actually work.

## What was checked before the plan

- **Whether the backend reads the tables.** `item_type`, `gear_dismantle`, `craft_table`, `trade_commodity`, `market_special_cost`,
  `gear_insurance_item`, `map_exp`, `role_level_exp`, `affix_pool` are used in the handlers in `core/backend/src/rpc/handlers/`.
- **Sell price.** `item_sell_price` (`raid_handlers.rs:211`) takes `system_repurchase_price`, and if it's missing, `price_def` from
  `trade_commodity`. Insurance is computed from the same value (`item_handlers.rs:268`). In Dead Money gear with `gear_worthless` costs 0.
- **Crafting is instant** (WORKAROUNDS K1), so editing `time_cost` is pointless.
- **Affix weights don't work now.** `affix.rs` only looks at whether `weight` is set and picks uniformly (`:115`, `:275`).
  The weight value doesn't affect the result.
- **The number of affixes is hard-coded.** The backend doesn't read `affix_count_random_pool` and `affix_level_pool_rule`, and `affix.rs:237,257` sets
  the number by affix kind. In `kit.toml` these tables say "backend — on item creation": this is wrong, the text must be fixed.
- **Loot is read by the game, not the backend.** Chests follow `GameState.MapId` (FIX_LIST, 05.11, "20:5x"), arenas use map 0. Earlier,
  in listen mode, per-map selection didn't work, so the current mode needs a separate control raid.
- **Loot template.** `content/loot/loot.toml` has 166 groups and 169 pools. The average-drop calculation is in `core/backend/xtask/src/loot.rs`, it computes per
  source. How many sources stand on a map is not in the data.
- **Stat coefficients.** The StatWatch mod writes `FirstLevelAttrDataAsset` in full (`dbtools/run/statwatch-*.jsonl`). Hero attributes
  matched 252 of 252 (FIX_LIST BAL2), the damage multiplier K is not split out.
- **Skills and monsters.** The tables have only names, number formats and a link to `CareerSkillDamageCurveTable`. Unpacked curves and
  monster tables are not in the project.

## Stage 1. Economy and XP (done 06.11, release 1.1)

New tables appear in the app without code. Every field was checked by reading the server code, not checked in the game: below is a list
of actions for a manual check.

| Table | We edit | Who reads | Check in game |
| --- | --- | --- | --- |
| `item_type` | `system_repurchase_price` | `raid_handlers.rs:212`, `item_sell_price` | Sell an item, extract from a raid with loot |
| `trade_commodity` | `price_def`, `tax_percentage`, `publish_cost` | `raid_handlers.rs:219`, `trade_handlers.rs:1060,1323,1526` | An item without a buyback price, a deal and a lot on the market |
| `gear_dismantle` | `reward[*].count` | `item_handlers.rs:617` | Dismantle an item |
| `craft_table` | `item_cost[*].count`, `product[*].count` | `item_handlers.rs:401,454` | Craft a potion |
| `market_special_cost` | Prices by purchase number | `shop_handlers.rs:511` | Buy a special item twice |
| `gear_insurance_item` | `sp_rate` | `item_handlers.rs:276` | Insure an item |
| `role_level_exp` | `max_exp` | `progress_handlers.rs:1418` | Level up |
| `map_exp` | `exp` | `progress_handlers.rs:1390` | XP after a raid |

**Fields deliberately left out because they affect nothing on the server:**

- `great_success_multiple`: every recipe has an empty `great_success_rate`.
- `time_cost`: crafting is instant (WORKAROUNDS K1).
- `price_upper_limit`, `price_lower_limit`: the server doesn't check them (`trade_handlers.rs:1397`, TODO).
- `data.stock` in `market_special_cost`: the server doesn't read it.
- `standard_rate`: a divisor that is the same in all rows, `sp_rate` is enough.

`role_default_equipment` was left out: it is mostly the kit composition, and a new row can't be added by a patch.

**What else was done:** table groups in the left panel, map names, price, market, dismantling and recipes in the item card, every item
with a name has a card, not only those with properties. The reference affix tables now honestly say the server doesn't read them.

**Cost of the change:** the workbook grew from 8 to 14 MB, the kit archive from 16 to 21 MB, mostly because of item thumbnails in the new sheets.

## Stage 2. Loot (done 06.11, release 1.2)

**What turned out differently from the plan below:**

- **Step 2.0 was not done before development.** A control raid needs a player and a closed game. The logs only show that the tables loaded:
  `[LOOTCHECK]` reads map 10101 rows via the API and doesn't prove per-map selection. The difference between two real maps in the game is **not
  confirmed**. The FIX_LIST entry "20:5x" only confirms the difference between map 0, the Crypt and the arenas.
- **Table names:** `lt_slots`, `lt_items`, `lt_rules`, so as not to confuse them with the game's `loot*` tables.
- **Step 2.4 without manual work.** Names are built from the source type and the blueprint name: "Chests: Gold 01, Gold 02 and 8 more". Sources have no names
  from the game. Hand-written names for 166 groups would give little: most groups repeat across maps.
- **Kit beyond the plan:** `raw/loot_template.json`, a template with item categories and prices. Without it the app can't compute the drop with edits.

**How it was checked:**

| What | How | Result |
| --- | --- | --- |
| App calculation and table build | `loot verify-view`: every group, slot and pool row | Matched: base 166 groups, season 217 groups |
| Edit in memory (app) and in text (server) | `loot verify-view --patch` | Matched |
| Average drop vs `loot show` | `s2_show_compare.py`, 7 maps, base and season | Matched: 473 and 466 group rows |
| Patch on a project copy | `s2_loot_accept.py`: only numbers in the template, only loot tables, the same patch via the workbook | Matched |
| Bad patches | Chance 150, empty pool, wrong "was", closed field | Refused, nothing written |
| Screen | `cdp_s2_loot.mjs` on the real exe, base and season, together with `run_all.ps1` | No errors |

**Not checked in the game:** drops after the new pak. To check: on a copy or in a season set different gold chances in groups of the Crypt and Crypt
Classic, build the pak, do one raid on each map with the game closed during the build.

**Found along the way.** The Dead Money season source table lagged behind the base: the base was rebuilt on 05.11 at 22:01, the season at 19:43. 252 arena rows
(`keep_base`, maps 30101…30302) have old slot numbers. The very first season loot edit or `loot build --season=dead-money` will update them, and the season's arena
loot will become as in the current base. This is expected given what `keep_base` means, but on the copy we only checked that other rows don't change.

**Cost of the change:** the workbook and archive barely grew: pictures in the loot items sheet are turned off, otherwise the workbook came out at 46 MB instead of 15 MB.

Below is the original plan.

**2.0 Control check before any development (S).** Change the chance in one group differently on two maps, do one raid on each
and compare with the `[LOOTCHECK]` log. If per-map selection isn't confirmed, fix that first, otherwise the screen will show chances that don't exist in the game.

**Design decision (recommended).** The `loot.toml` template is shown as three service tables:

| Service table | Row | We edit |
| --- | --- | --- |
| `loot_slots` | Group slot | `chance` |
| `loot_items` | Item in a pool | `w`, `cnt` |
| `loot_rules` | `[rules]` or mode rule | Multipliers `chance`, `weight`, `count`, `quality` |

This way cell edits, undo, draft, validation, transfer and the patch format work right away, no new patch format is needed. Downside: such a patch can't add or
remove an item. For the first version this is enough: an item can be disabled with weight 0.

1. **2.1 Kit.** `balance kit` exports the service tables of the active template: the season one if it exists, otherwise the base one. The row
   identifier is built from the pool id and the item id.
2. **2.2 Server.** `balance patch` routes the `loot_*` tables to the template editor preserving comments (`toml_edit`), then
   `loot build` is needed. New rules: chance from 0 to 100, weight not below 0, at least one item with weight above 0 remains in the pool.
3. **2.3 Calculation.** Move the average-drop calculation from `core/backend/xtask/src/loot.rs` into `balance-kit`. Both programs compute with one code, a test
   compares the result with `loot show` output on all maps.
4. **2.4 Group names.** Now it's "all maps: type 0,1,2 — BP_…". We need clear names and source types (chest, monster, vase,
   altar) in two languages. This is separate manual work on 166 groups.
5. **2.5 "Loot" screen.** Choice of map and source type, a group with slots and pools, each item's share in percent, the average drop per
   source and its change. A pool shared by several groups warns about it, like an affix group.
6. **2.6 Seasons.** Show the effect of `[rules]` and modes: the value in the template and the value after the rules. Dead Money with `no_gear` matters here.
7. **2.7 Release.** Acceptance: the patch is applied on a copy, the tables are built and match the calculation; a control raid confirms the change.

**Outside the first version:**

- new slots and items;
- drop for a whole raid: it needs to know how many sources stand on each map, a separate task on level data.

**Risk:** loot lives in the pak, so an edit needs a new pak build for the game server and the clients, not only the backend.

## Stage 3. Affixes and final stats (M, release 1.3)

**3.1 Decision: what to do with weights.** Options:

- **Leave as is (recommended to start).** The kit only edits whether an attribute is in a pool. The weight number must not be shown:
  the designer will think it works.
- **Take weights into account in `affix.rs`.** This will change affix drops on the server, so the owner decides this separately.

The same applies to the number of affixes: while it is hard-coded, the tables stay reference-only or are removed from the kit.

1. **3.2 Pools.** A pool and its attributes in the kit and in the item card: which affixes this item can get at all. Turning an
   attribute on and off — per decision 3.1.
2. **3.3 Coefficients.** Export `FirstLevelAttrDataAsset` from StatWatch once and put it in the kit together with the rules from BAL2: template,
   affixes, secondaries from primaries, `ROLE_HOLD` only for a weapon in hand.
3. **3.4 Calculator.** Screen: class, level, equipped items and roll (minimum, average, maximum). Result: final attributes and what
   changes after an edit.
4. **3.5 Check.** The calculator reproduces the StatWatch report on a recorded raid: the same 252 values. Without this the screen is not released.

**Outside the stage:** damage and DPS, until BAL2 is closed.

## Stage 4. Skills and monsters (research, no release)

The goal is a written answer, not an app feature. Same limits as before: only the needed game files, no reading process memory, the AES key is not
printed or written down.

1. **4.1 Skills: what exists.** Extract `CareerSkillDamageCurveTable` and the `GE_TGF_Damage_*` effects via `retoc`, read them in FModel. Make
   a table: skill, effect, curve, what each number means.
2. **4.2 Monsters: where it lives.** Find the source of health and damage via `BP_Tgf_Monster`. StatWatch already captures monster attributes on hit:
   compare with what is found.
3. **4.3 Probe on one value.** Change one coefficient of one skill in a test pak, install it on the game server and the client.
   Confirm the damage change with a StatWatch recording and check the skill tooltip in the client.
4. **4.4 Distribution.** Find out whether both clients and the server must have the pak, what happens on a mismatch, whether launcher mods fit.
5. **4.5 Report.** For skills and for monsters: whether they can be changed, with what (file edit or a build via `ue_jester`), how to distribute, what the risks are.
   Only after this is it decided whether this goes into the app.

**Stop if:** the probe value doesn't affect damage on the server, or the edit needs a protection bypass or an exe change. This is out of scope.

## What the owner needs to decide

| Question | When | Recommendation |
| --- | --- | --- |
| Show 1.0 to the designers before stage 2 | Now | Yes |
| Loot in the first version without new items and slots | Before 2.1 | Yes |
| Affix weights: leave or take into account on the server | Before 3.2 | Leave for now |
| Whether to start the stage 4 research in parallel | Now | Yes, it changes nothing in working files |
