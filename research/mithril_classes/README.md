# The Mithril Order: how it works in the game — analysis 06.11

Sources: tables (`content\balance\tables\*.json`), protocol (`src\generated\tgf.rs`), client Lua (`research\lua_all`, disassembly in `lua\`),
`UMithrilService` headers, item assets (retoc). We have not seen real traffic for this mode; `rpc_dump` has only the `need_mithril_force` field and `mithril_force_data`.

**Correction 06.11.** The note from 05.11 said: "the class determines the suit, each item has its own abilities". That is wrong.
- `AID_Mithril_*` contain only per-body models, a physical material and the armor base class `AID_Chest_Plate_Base` (checked on white and purple quality torsos).
- The digit `Y` in the id `19?0XY000` is the quality, not a separate suit.
- Skills are set by `role_type.role_skill`.

## What it is
A no-risk mode, like Scav in Tarkov. A separate force `EN_FORCE_TYPE_MITHRIL_FORCE` with its own hall.

### Classes — `role_type` 400101–400103 (all race Human, gender of choice, 4 skins)
| type_id | career | Name | Skills (`role_skill`) |
|---|---|---|---|
| 400101 | 101 | Mithril Adept | 1101001 Q, 1101002 E |
| 400103 | 102 | Mithril Shadow | 1102001 Q, 1102002 E |
| 400102 | 103 | Mithril Righteous | 1103001 Q, 1103002 E |

- Regular classes have a larger skill pool: the fighter has 5 (`107001…`), mithril exactly 2, with no choice.
- Chain: `role_skill` → `ExcelRoleSkill.ability_set_id` → `UeSource_AbilitySet.ue_path` → `AbilitySet_MithrilFighter_Q`.
- Talents by career + race (`FirstLevelAttr`, `AbilitySet_TalentMithril*`).
- Starting kit: `role_default_equipment`, `create_default_item_config_ids` = `game_failed_default_item_config_ids`,
  so the kit is restored after death. Contents: two pairs of weapons, 5 armor pieces, 2 rings, a necklace, torches ×3, potions ×3, bandages ×3, a mithril crystal; bow/crossbow users get arrows ×15.

### Equipment — `mithril_force_item` (free, by reputation)
- Tabs: 1001 weapons, 1002 armor, 1003 accessories, 1004 consumables.
- id: `191…` weapons, `192…` cloth, `193…` leather, `194…` plate, `195…` rings/necklaces.
- `19?0XY000`: X — slot (1 helmet, 2 torso, 3 gloves, 4 pants, 5 shoes), Y — quality (1 white, 2 green, 3 blue, 4 purple).
- The tables have only white and green. Blue and purple: assets and `UeSource_ItemType` exist, there are no rows in `item_type` and `mithril_force_item`: unused content. Folders `P071…P074` are their looks.
- Equipping: `TryPutOnItem` → `mithril_role_wear(type_id, count, goal_position)`; a locked item gives error -5404.

### Reputation — `mithril_force_prestige`, `mithril_force_settlement`
| Level | XP | Unlocks |
|---|---|---|
| 1 | 0 | white, mithril crystal |
| 2 | 100 | green weapons |
| 3 | 200 | green armor |
| 4 | 300 | green accessories |

- There are 4 levels, not 3. The string `MithrilForceAbilityName_005` "Income bonus" exists in the localization but not in the table.
- XP per raid depends on damage dealt: 500 damage → 5 XP, 1000 → 10 … 5000 → 50 (up to 50 per raid).
- Not verified: whether level XP is cumulative. `GetMithrilPrestigeLevel` is in C++.

### Extraction reward — `const`, `mithril_force_settlement_factor`
- Items from the raid do not reach the player: in the results window the backpack dissolves, sound `Play_UI_Blacksmith_Desynth`.
- Granted: gold 1001, white cast iron 1052, ilmenite ore 1037 (the window also shows 1038, 1039), ascension potion 100070 (chance), health potions 100013 ×2 (chance).
- Factors by damage: 0 → 100% currency/ore, potions 0%; 400 → 20%; 800 → 40%; 1200 → 60%; 1600 → 80%; 2000 → 100%.
  Base: `mithril_force_currency_basic_factor` 100, `heirloom_basic_factor` 100, `expbottle_basic_probability` 10, `potion_basic_probability` 100.
- The formula converting loot to currency is not visible on the client: it is on the server, so we have to invent it (`currency_active_coefficient` etc.).
- "Rewards sent to the main character"; backpack full: mail `mithril_force_mail_id` 501034, reasons `EnMithrilForceRewardSendMailReason`.

### Maps and matchmaking
- `mithril_force_level_list` = 40003015 (Dungeon Casual, pool 20029) and 40003016 (pool 20030), default 40003015. Both levels are `is_allow_mithril_force`, type 2001, PVE results.
- Equipment limit: 40003015 up to green, 40003016 white only (`battle_gear_rarity_require`).
- "One Order team per match" is not visible in the client: it is a server-side matchmaking rule; our `matchmaking.rs` knows nothing about the force.
- `mithril_force_same_faction_rebirths_count` = 0, the number of revivals of own players.
- Mode unlock: module `EN_UNLOCK_MODULE_ID_MITHRIL` 1200033. String `MithrilForce_Lobby_EnterLimit` "the number of team members exceeds the limit".

## Client flow
1. Map selection window → the Order tile (`WBP_SelectMap_MapEntry7`, shows the reputation level) → ENTER button → `ULobbyService.CheckCanChangeHallForce` → `MithrilUtils.EnterMithril`.
2. `UMithrilService.CheckHasMithrilRole`:
   - yes: `EnterMithrilHall()` and the `MithrilEnterMithrilHallEvent` event;
   - no: the `WBP_Mithril_SelectRole_Window` window: class, gender, pros and cons, weapons, skills, talents. "Select" → `SelectMithrilRole(career, gender)`.
3. Hall: `SwitcherForce` to the Order view, button `ButtonLeaveMithril` → `LeaveMithrilHall()`. Inventory: `WBP_Mithril_Inventory_Window` (Tab), catalog `WBP_Mithril_EquipmentList` on the left.
4. Matchmaking: `WBP_MatchMapSelectWindow` switches the map list to the Order variant.
5. Results (`WBP_HallWindow`): if `GsResult.MithrilForceResult` has `items` → `WBP_Mithril_SettleReward_Window`, on close `SendMithrilForceReceiveRewardReq`; otherwise, if reputation changed → `WBP_Mithril_SettleReputation_Window`.

## Protocol
- `mithril_force_role_init(career, gender)` → `role_guid`.
- `mithril_force_role_select(role_guid, gender, dress_up)`.
- `mithril_role_wear(type_id, count, goal_position)` → `worn_item`.
- `mithril_force_receive_reward(items_position)`.
- `player_get_info`: `need_mithril_force` → `mithril_force_data.prestige`; push `dirty_mithril_force_data`.
- Results: `DGsUserSettleResult.mithril_force_result` = `DUserMithrilForceBattleResult { old_prestige, cur_prestige, items, player_backpack }`.

## Our backend now (`shop_handlers.rs`)
| What | State |
|---|---|
| `mithril_force_role_init` | only returns an existing role, no creation |
| `mithril_force_role_select` | empty response |
| `mithril_force_receive_reward` | empty response |
| `mithril_role_wear` | works, but only `unlock_level ≤ 1` (reputation is always 0) |
| `prestige` | always 0, reputation tables unused |
| Entering/leaving the hall | TODO, no pushes (`player_handlers.rs`) |
| Raid results, loot conversion, kit restore | none |
| Matchmaking: "one Order team" rule | none |

## What this means for the "new classes" idea
The scheme is better than we thought on 05.11: an Order class is **a table row**.
- Without C++ and assets you can change: skills (`role_type.role_skill`), starting kit, descriptions, allowed equipment (`career_restrict`).
  For example, the adept can be given another class's `role_skill`. **Not verified**: whether the skill is granted from the table and whether another class's skill works on a different career.
- Hard limit: 3 slots. Selection goes by career: `SelectMithrilRole(career, gender)`, `mithril_force_role_init(career, …)`, `GetMithrilRoleGuid(career)`.
  A second `role_type` row with the same career is indistinguishable in the selection window.
- Talents are bound to career + race in `FirstLevelAttr`; only an asset edit changes them.
- Equipment looks can be replaced, they are regular items (see `research\new_equipment`).

## Main scheme for a new class (user's decision 06.11)
**A `role_type` row with an Order `career` and `force` = ADVENTURER.** No need to build the Order mode: such a character lives like a regular one.

```
type_id: 400201
career:  MITHRIL_FORCE_FIGHTER   (101)
force:   ADVENTURER
role_skill, starting kit, name, description: our own
```

### Why not swap the career
In the client these are different fields. The hall (`WBP_HallWindow`, `WBP_AdventureWindow`), ranking, social panels and the role background (`RoleInfoCtl`) look at `force`
(`GetHallRoleForceType`, `Meta.Force`). The Order windows open only from its hall. A swap "one career in the lobby, another in the match" makes client and server diverge: a fallback only.

### What is configurable
- Tables: `role_skill` (a pool for Q and E, the player picks via `role_select_skill`, a regular fighter has 3 + 2), starting kit, texts, equipment `career_restrict`.
- The `FirstLevelAttr` asset: talents, one set per slot.
- Backend: economy, progression.

### Limitations
- There are three Order `career`s (101–103), so at most 3 classes fully distinct by talents. Stats are set by the `role_type` / `role_level_upgrade` row. Within that: variations by skills and equipment.
- `career` still matters: equipment and tooltips (`IsMithrilCareer`, `MatchCareer`), talents, the name in the character window (`CareerName_101`).

### What to check (not verified)
1. ~~Whether the new row appears in the character creation window~~: per the code yes, but the window has 8 places and all are taken. See "A new slot in the creation window". Not verified in the game.
2. ~~Whether the match accepts `ForceType` = Adventurer with `career` 101~~: yes, a raid with extraction was completed on 06.11 with class 400201 (`<data>\new_class`).
3. What `MithrilItems` from `TgfFirstLevelAttrDataAsset` (`FTgfMithrilAbilityItem.AbilitySet`) are granted by: `career` or `force`.
4. Whether such a character gets skill selection in the regular inventory and whether the battle server grants exactly the selected ones.
5. Whether another class's skill works on this `career`: its own animations, weapons, checks.

## A new slot in the creation window (analysis 06.11, from code only, not run in the game)
Materials: `create_role\bp\cfg\` (`BPFL_Window`, `BPFL_CreateRole`, `WBP_CreateRoleInfoUi`, `WBP_CreateRoleCareerItem`), `create_role\role_native.c`.

### How the window builds the list
`WBP_CreateRoleInfoUi.SetCareerList(SelectRace)` → `BPFL_Window.GetCareerList(RaceId)`:
```
for each role_type row:
    environment == 1 (ALL_AVAILABLE)
    and force == 1 (ADVENTURER)
    and (RaceId == 0 or meta.race == RaceId)   // Set Data always sets SelectRace = 0
→ SortCareerConfigList (native sort, by the order field; check on the row)
```
The list is not hard-coded in C++: a row with `force` = ADVENTURER gets into it by itself. Order rows (`force` = 2) do not get here, which is why they are not in the window.

### Limitation: 8 places, all taken
- `WBP_CreateRoleInfoUi` has eight ready widgets `CareerItem1…8` in `CareerWrap` (**WrapBox**).
- `SetCareerList` walks the `CareerItems` array, not the list of rows. A ninth row after sorting is simply not shown, with no error.
- Right now there are exactly 8 rows with `force` = 1 and `environment` = 1 (400001–400008), `order` 1–8.
- `InitSubItemUi` builds `CareerItems` from `CareerWrap.GetAllChildren()` and sets `WBP = self` on each. A click goes as `WBP.OnCareerSelect(RoleTypeId)`, i.e. selection by `type_id`, not by career. A second row with the same career does not get in the way here.

### How to add a ninth place
Create a `WBP_CreateRoleCareerItem_C` at runtime, add it to `CareerWrap` and to `CareerItems`, set `WBP`. All the other window logic works with the array and will pick it up by itself.

| Option | How | Assessment |
|---|---|---|
| **UnLua** (main) | The window has the module `Game.System.CreateRole.WBP_CreateRoleInfoUi` (with `Construct`, `InitUI`). Override it in the pak and add the code there | No UE4SS, ships in the same pak as the tables and icons |
| UE4SS | A pre-hook on `InitSubItemUi` in RuClient: `WidgetBlueprintLibrary.Create` + `AddChild` | Faster for a first check, edits only in the mod |
| Asset edit | New widget and slot exports in `WBP_CreateRoleInfoUi` | Harder and no benefit |
| No ninth place | Give the new row a smaller `order`: it joins the list, the last class (currently the druid) drops out | Test only, nothing but the table |

Not verified: whether a third row fits the layout (`WrapBox`, parent size). If not, move the slots or the parent with the same code.

### What the new row needs
| What | Where | Why |
|---|---|---|
| `force`, `meta.force` = ADVENTURER, `environment` = ALL_AVAILABLE, `order` | `role_type` | To get into the list |
| `create_show_item_config_ids`, `skin_ids`, `available_gender`, `rotation` | `role_type` | The model on the creation screen |
| `str_desc`, `str_advantages`, `str_disadvantages`, `str_equipments` | `role_type` + `GeneralStrings` | Description |
| `role_skill`, `role_initial_skill` | `role_type` | Skills |
| Name `CareerName_{career}` | `GeneralStrings` | The window takes the name **by career, not by `str_name`**: rename `CareerName_101`. The Order mode will not coexist with that name |
| `RoleIcon_{type_id}`, `RoleIcon_{type_id}_1` | New textures in the pak, `/Game/TGF/UI/Textures/Icon/CareerIcon/` | Icon by `type_id`. A regular `Texture2D`, no mod kit needed |
| 20 level rows | `role_level_upgrade` | The Order has only 1 row, regular classes have 20. The backend computes attributes strictly by `role_type_id` |
| Portrait rows | `role_head_icon` (`type_id` + gender + face + skin) | Portrait. Copy from 400002, it has 64 rows |
| Talents | `FirstLevelAttr` by career + race | The window shows them via `GetTalentsWithOrder`. Already present for 101 + Human |
| Faces | `role_face_pinching_limit` by race + gender | Nothing to add for Human |

Check on the spot: `rank_report_setting`, `quest_progress_condition`, `market_poor_man`, `immaculate_trial_random_rewards` have 400002 but not the Order.

### Client and backend
- `SendRoleCreateRequest(TypeId, RoleName, Gender)` on the client does not check `TypeId` (0x1459d6b00, `force` in the request is the constant 1).
- `role_create_impl` accepts any `role_type_id` from the table: race, career, force, kit and attributes come from the row. No code change is needed if the row is in the backend's table set. Starting kit: `create_default_item_config_ids` if there is no `starter_loadout` entry.
- The character limit `role_count_limit` = 10 does not affect the new class. The heroes screen is paged; the backend does not check the limit now.
- Tables are needed on the client and on the battle server (`cargo xtask balance build`). Adding a row, rather than changing a value, has not been tried this way yet.

## Plan
1. First check: one `role_type` row (client table pak and backend), creation via the window (ninth place or the `order` test), entering a raid on our server.
2. Close the "What to check" items.
3. Own talents in `FirstLevelAttr`, equipment by armor type, texts.
4. Own skill logic: the mod kit (`research\modkit`).

Separately, if the Order mode itself (Scav) is needed: role creation, hall, equipment by reputation, results with loot conversion, reputation, kit restore, the matchmaking rule.
