# Mithril Order (Mithril Force): how the mode works and what it takes to make it work

Analysis of 08.11, static only: Ghidra on the exe image, BP bytecode (retoc → kismet-analyzer), client Lua, tables.
I didn't run the game and didn't change code. The previous analysis (06.11, classes, gear, lobby windows):
`<data>\research\mithril_classes\README.md`. Working files of this analysis:
`<data>\research\mithril_order` (decompilation `m1.c`…`m5.c`, assets `legacy\`, CFG `cfg\`).

## Conclusion
- **The mode is half native.** The battle server itself recognises an order player by the role in the data from the backend
  (`meta.force` = 2), gives them team resurrection, a special death camera and a ban on changing gear. This needs
  neither code edits nor mods. Everything else is on the backend:
  - order roles;
  - the hall;
  - gear by reputation;
  - matchmaking;
  - raid result: reputation and a reward instead of loot.

  For us almost all of this is stubs.
- **The maps are the same as for regular players.** The mode runs on Casual: 40003015 (Dungeon, map 10101) and 40003016
  (Castle, map 10201). An order team is matched into a regular adventurers' match — like Scavs in Tarkov. There are no separate
  levels.
- **Enemies are by team, not by faction.** The battle server nowhere compares faction in hostility checks: the order is just
  another team. The "one order team per match" rule is a matchmaking rule on the backend.
- **Reputation grows from damage, not from the number of kills.** Table `mithril_force_settlement`: 500 damage → 5 XP, …,
  5000 → 50 (maximum per raid). The mode description (`Guide_Description_13`): "wound or kill adventurers". So
  damage to players is counted, most likely float statistic 7 `CausedPlayerTotalDamage` (see `docs/QUEST_HOOKS.md`).
  This is not proven: the official server computed the formula.
- **"Resurrection" is native.** It is `GA_TGF_Rebirth_Mithril`, a team-wide pool of respawns, details below.
  **"Portal perception" was not found in the game files.** It is neither in the strings (`GeneralStringTable`, Chinese source),
  nor in the reputation tables, nor in the order's ability sets. The order's talents are only attribute bonuses
  (e.g. `ElementResPierce`). Reputation levels only unlock gear: 1 — white, 2 — green weapons,
  3 — green armor, 4 — green accessories. A fifth level "Income bonus" exists in the strings, but not in the table.

## 1. Entering the mode (lobby, client)
| Step | Client | RPC (`core/backend/src/generated/tgf.rs`) |
|---|---|---|
| Order tile in the map selection window (`WBP_SelectMap_MapEntry7`), ENTER | `ULobbyService.CheckCanChangeHallForce` → `MithrilUtils.EnterMithril` | — |
| No order role yet | window `WBP_Mithril_SelectRole_Window` → `UMithrilService.SelectMithrilRole(career, gender)` | `mithril_force_role_init {career, gender}` → `{role_guid}` |
| Role exists | the same `SelectMithrilRole` | `mithril_force_role_select {role_guid, gender, dress_up}` |
| Entering the hall | `UMithrilService.EnterMithrilHall()` (exec `0x145939080`) | `hall_switch {switch_type: 2}` (`EN_HALL_FORCE_SWITCH_TYPE_TO_MITHRIL_FORCE_HALL`) |
| Leaving the hall | `LeaveMithrilHall()` (exec `0x14593a0b0`) | `hall_switch {switch_type: 1}` |
| Gear in the hall | `WBP_Mithril_Inventory_Window`, `TryPutOnItem` | `mithril_role_wear {type_id, count, goal_position}` → `{worn_item}`, errors −5403/−5404/−5406 |
| Reputation | `player_get_info {need_mithril_force}` | `mithril_force_data {prestige}`, push `dirty_mithril_force_data` |
| Party | only the leader switches the hall; members get `Mithril_TeamMember_SwitchStatus_Tips` | party action `DChatMessageTeamAction::HallSwitch(DHallData {switch_type})`, `DTeam.hall_data`, error −754 |
| Matchmaking | `WBP_MatchMapSelectWindow` shows the order's maps | regular matchmaking; −1030 `EN_ERR_MATCHING_THIS_LEVEL_NOT_ALLOW_MITHRIL_FORCE`, cancel −5025 "too many order players in the queue", `EN_MATCHING_EXIT_TYPE_SEARCH_TIMEOUT_FOR_MITHRIL_FORCE` |
| Result | `WBP_HallWindow`: `GsResult.MithrilForceResult` → rewards window, then reputation window | `DGsUserSettleResult.mithril_force_result = DUserMithrilForceBattleResult {old_prestige, cur_prestige, items, player_backpack}`; collecting — `mithril_force_receive_reward {items_position}` |

`SelectMithrilRole` (`0x145a1d5f0`): if `GetMithrilRoleGuid(career)` = 0 — sends `role_init`, otherwise `role_select`.
One order role per career (101/102/103). The role stores `DBattleDeployRole.last_adventurer_role_guid` — the main
character that receives the rewards.

### Tables
- `role_type` 400101–400103: `meta.force` = MITHRIL_FORCE, starter kit, `role_skill` (Q/E).
- `mithril_force_item`: gear catalog, `unlock_level`.
- `mithril_force_prestige`: 4 levels, XP 0/100/200/300.
- `mithril_force_settlement`: damage → reputation XP.
- `mithril_force_settlement_factor`: damage → reward shares. Currency and ore 100% at any damage; potions and the XP bottle —
  0% at 0 damage, 20/40/60/80/100% at 400/800/1200/1600/2000.
- `const`:

  | Key | Value |
  |---|---|
  | `mithril_force_level_list` | 40003015 40003016 |
  | `mithril_force_default_level_id` | 40003015 |
  | `mithril_force_same_faction_rebirths_count` | **0** |
  | `mithril_force_mail_id` | 501034 |
  | `mithril_force_currency_*` | item 1001 (gold), base multiplier 100, fixed part 0 |
  | `mithril_force_heirloom_*` | 1052 (white cast iron) and `_2` = 1037 (ilmenite), 100 |
  | `mithril_force_exp_bottle_*` | 100070 ×1, chance 10 |
  | `mithril_force_potion_*` | 100013 ×2, chance 100 |
  | `mithril_force_affix_basic_probability` | 0 |
- `competition_level` 40003015/16: `is_allow_mithril_force`. Gear rarity cap: up to green and up to white
  (in season 1). Seasons 1001/1002 have no cap.
- `module_unlock_type` 1200033 `EN_UNLOCK_MODULE_ID_MITHRIL` — unlocking the mode.
- Quests: `quest_progress_condition` 207002 `role_which_force = MITHRIL_FORCE`; statistics
  `EN_STATISTICS_DATA_MITHRIL_FORCE_KILL_PLAYER_COUNT(_NEW)` (5/19).

## 2. Battle server
### How the server recognises an order player
`UBattleDsUtility::AddBattleUserData` (`0x142e9b2f0`) — for us dbnative calls it with data from `/ds/battle_user_data`.
It puts a record into the cache and sets a flag in it (`+0x1ac` of the record):

```
DBattleUserData.deploy_preview.deploy_role.basic.meta.force == 2   (EN_FORCE_TYPE_MITHRIL_FORCE)
```

`LoadUserDataFromCache` (`0x142ecfd20`, log lines "LoadUserDataFromCache …") with this flag does the following:
- `PlayerState.ForceType = 2` (`+0xa16`, setter `0x142fe6df0`, authority only);
- `PlayerState.MithrilRemainingRebirthChances = DBattleUserData.mithril_force_same_faction_rebirths_count`
  (`+0xa18`, setter `0x142fe7850`). The number is taken on the first load of a fresh record (state 1). On a reload
  from the cache (floor transition) — only if scene `+0x1f4` = 2.

Career comes from `meta.career` the usual way. In total: **it is enough for the player's active role in
`battle_user_data` to be an order role.** Our `build_battle_user_data` takes `hall_role_guid`, so the order hall
must switch `hall_role_guid` to the order role.

### What `ForceType = 2` changes
There are only a few native places where `+0xa16` is read; the rest is done by BPs via `IsMithrillCharacter` and
`GetIsMithrilCharacter`.

| Where | What |
|---|---|
| Granting abilities on ASC init (`0x142e42380`) | Besides talents by career and race, an order player gets `FirstLevelAttr.MithrilItems` = `AbilitySet_Mithril` → `GA_TGF_Rebirth_Mithril`. Granted by **force**, not by career (answer to question 3 from the 06.11 README) |
| `GA_TGF_Rebirth_Mithril` | Resurrection, see below |
| `MithrilResurrecting` (`0x142ed43f0`) | The character is dead, but its PlayerState has force 2 and pool > 0 — it is "resurrecting". Such a player counts as alive in the "team eliminated" check (`0x142ecf5a0`), in spectator target selection (`0x142eccc80`/`0x142ecce40`) and on controller death (`0x142ee06c0`: waiting camera, `OnViewMithrilRebirthExhausted`) |
| `ConsumeMithrilRebirthChance` (`0x142ee1d00`) | Minus 1 on **every** PlayerState of the team (the pool is shared by the team) |
| `PlayerState` → message `TgfMithrilInfo {Type, RemainingRebirthChances}` (`0x142ff1a30`) | HUD: `Mithril_Rebirth_RemainingChances` "Team respawns: N", countdown `WBP_TGF_MithrilRebirth_CountDown` |
| Inventory in a match | `UTgfInventoryWrapper.IsMithrillCharacter`, tooltip `MithrilForceEquipLockTip` "an order character can't change gear" |
| Hostility, loot, match result | **Nothing.** The enemy check (`0x14301bf20`) and `DumpUserDataToCache` / `player_game_over` don't read force. The battle server sends an order player's result the same as a regular one |

### Resurrection `GA_TGF_Rebirth_Mithril`
Settings (CDO): `MaxDurationForRebirth` 3600 s, `RebirthDelayTime` 5 s, `RebirthMinDistance` 4000 (40 m),
`AllowActivateOnDeath`, server only.

1. `CanMithrilRebirth`: server time < 3600 s, `GetIsMithrilCharacter`, `GetMithrilRemainingRebirthChances` > 0.
2. Death → `HandleMithrilDeathCamera`, `SpawnGrave` (grave `BP_MithrilGrave`, child of `BP_InteractableGrave` —
   it can probably be looted; not checked).
3. After 5 s — `GetRebirthTransform` (a point no closer than 40 m), teleport, "Full Recover Self", effect
   `GE_TGF_Rebirth_Mithril_NoDamage` (invulnerability), `ConsumeMithrilRebirthChance`, event
   `TGF.GameplayEvent.RebirthSuccess`.

**In the table `mithril_force_same_faction_rebirths_count` = 0**, so in the original (per this table) there were no
respawns, and death was final. If we want resurrection — set a number in const or in the backend.

### Death
The battle server handles death as a regular one, the result is a regular `player_game_over`. "The main character loses nothing"
is ensured by the backend: a separate order role with issued gear goes into the raid. After death the backend restores
the kit: `create_default_item_config_ids` = `game_failed_default_item_config_ids`.

## 3. What we already have
| What | Where | State |
|---|---|---|
| `mithril_force_role_init` | `shop_handlers.rs:1430` | Only finds an existing role; **no creation** |
| `mithril_force_role_select` | `shop_handlers.rs:1446` | Empty response |
| `mithril_force_receive_reward` | `shop_handlers.rs:1451` | Empty response |
| `mithril_role_wear` | `shop_handlers.rs:1457` | Works, but only `unlock_level ≤ 1` (reputation always 0) |
| `player_get_info.mithril_force_data` | `account_handlers.rs:486` | `prestige: 0` |
| `hall_switch` | `player_handlers.rs:809` | Only remembers `switch_type`; doesn't change `hall_role_guid`, no pushes |
| Party action `HallSwitch` | — | None |
| `battle_user_data.mithril_force_same_faction_rebirths_count` | `battle_handlers.rs:117` | 0 |
| Matchmaking: level eligibility, one order team, rarity cap | `lobby/matchmaking.rs` | None, knows nothing about force |
| Result: reputation, reward instead of loot, kit restore, mail 501034 | — | None |
| Official server traffic | `rpc_dump` (PVE/OUR), `sniffer_log*.txt` | **Not a single** `mithril_*` or `hall_switch` call. In OUR only our own `mithril_force_data` field in `player_get_info` |

## 4. What to do, step by step
Estimates are rough, in agent working days. Mods and native code are not needed until we want something of our own.

1. **Order roles (backend, ~0.5 day).**
   - `role_init`: create a `role_type` role with the needed career and `force` = 2. The existing `role_create_impl` already
     accepts any `role_type_id`.
   - Remember `last_adventurer_role_guid` (the current adventurer role). Kit — `create_default_item_config_ids`.
   - Don't show order roles in the adventurer's hero list — check against the client.
   - `role_select`: gender and appearance.
2. **Hall (backend, ~0.5 day).**
   - `hall_switch(2)`: `hall_role_guid` = order role, push `dirty_hall_setting`.
   - `hall_switch(1)`: restore `last_adventurer_role_guid`.
   - In a party: leader only, the `HallSwitch` action to everyone, error −754 to a member.
   - **Unknown** which other pushes the client expects after `hall_switch`: check in the game.
3. **Gear and reputation (backend, ~0.5 day).**
   - Store `prestige` on the player; level — by `mithril_force_prestige.exp`, cumulative (not verified that it is
     cumulative).
   - `mithril_role_wear` — by level.
   - `player_get_info` and push `dirty_mithril_force_data` after the result.
   - The level rarity cap (`battle_gear_rarity_require`) is already in the table.
4. **Matchmaking (backend, ~1 day).**
   - An order role — only levels with `is_allow_mithril_force`, otherwise −1030.
   - No more than one order team in a room. Decide whether to wait for adventurers or let a match run with just the order:
     with low online otherwise it's an endless wait and −5025.
   - `team_id` — the order team's own (as now for any party).
5. **Match data (backend, ~0.1 day).** `mithril_force_same_faction_rebirths_count`: from const (0) or our own number.
   The battle server will do the rest itself.
6. **Raid result (backend, ~1.5–2 days).**
   - Damage to players. In the original it came in `DsPlayerGameOverReq.player_statistics`. Whether we have it on
     listen is not checked. Fallback: RuServer reads `GetFloatStat(7)` (`docs/QUEST_HOOKS.md`) and sends it to the backend.
   - Reputation: the `mithril_force_settlement` row with the largest `damage` ≤ damage → XP; `old/cur_prestige`.
   - Reward instead of loot: items extracted from the raid are removed from the order role. The main character (by
     `last_adventurer_role_guid`) gets gold 1001, cast iron 1052, ilmenite 1037 by multipliers, potions 100013 ×2 and the bottle
     100070 with a chance per `mithril_force_settlement_factor`. **How the currency amount was computed from loot is not in the
     client — we make up the formula ourselves** (e.g. a share of the trade price of what was extracted).
   - `DUserMithrilForceBattleResult` in `DGsUserSettleResult`, `receive_reward` puts in the reward. Stash full — mail
     501034, reasons `EnMithrilForceRewardSendMailReason`.
   - Death or extract: restore the order role's kit from `game_failed_default_item_config_ids`.
7. **Check in the game (2–3 matches).**
   - `PlayerState.ForceType` = 2 for an order player;
   - `GA_TGF_Rebirth_Mithril` is granted;
   - Q/E and the order classes' talents work;
   - gear can't be changed in a match;
   - with pool > 0 resurrection fires;
   - results screen and rewards windows.

In total about **4–5 days** of backend plus checking. No table changes are needed unless we decide to enable resurrection or
blue and purple gear: there are no `item_type` and `mithril_force_item` rows for them, the assets exist.

## Risks and unknowns
- **Order on listen.** Order abilities are granted on ASC init if `ForceType = 2` is already set by then.
  Force is set in `LoadUserDataFromCache` before character init, the order looks
  right, but it is not checked in a match.
- **Hall client.** Which pushes and fields the order hall windows need after `hall_switch` is not checked, there is no official
  traffic.
- **Reward formulas** (converting loot to currency, "income bonus") — ours only.
- **Online.** The mode is designed for joining adventurers' matches. With low online matchmaking must be simplified.
- **Mithril crystal** from the starter kit — what it does was not analysed.
- The order classes are not checked in a match on our server. Q/E and talents are native assets, they should work.
