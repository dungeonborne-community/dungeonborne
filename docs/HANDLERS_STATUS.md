# RPC handler status (30.10, after the parallel implementation)

All 185 stubs from `handlers_template.rs.txt` are implemented in `src/rpc/handlers/<domain>_handlers.rs`.
20 template methods with `req: Empty` (`*_sync`, `player_dirty_change`, `modal_message`…) are server→client pushes, sent by
the handlers. `cargo check` is clean, `cargo test` 25/25. **Nothing has been checked in game.** Backup before starting:
`src_before_handlers_20261030.zip`. Rules and sources for the agents — `docs/HANDLERS_AGENTS.md`.

Errors: `ret_code`/`result_code` in the response if the field exists; otherwise `set_rpc_error(code)` → `RpcHead.error_code`
(the gateway logs `error_code=`). How the client shows an error from the header — **check this first**.

| Domain (file) | RPC | Format source | Not done / TODO |
|---|---|---|---|
| misc | 24 | rpc_dump PVE, sniffer | payments — refused (0 diamonds); DLC skins are not granted |
| player | 21 | proto, tables, client Lua | `icon_type_id` order, GM `cmd_args` format; role deletion restrictions (-707/-708/-709) disabled on purpose |
| item | 19 | proto, tables, UE/Lua | the insurance formula is made up; `gear_durability` is empty |
| progress | 21 | sniffer (mail), Lua | lottery — stub (no pool); rating `start_no` |
| shop | 21 | rpc_dump PVE, sniffer, Lua | pass cycle rewards, trial (expired), mithril — empty |
| social | 15 | rpc_dump, proto, Lua | gifts: own box `friend_gift`, no UI in the client — `gift_type_id` = item ×1 (a guess) |
| guild | 19 | proto, const.json | **the client has no guild UI** — server side built ahead of time |
| room | 28 | proto, UE, Lua | room map ids ↔ levels (`level_for_map`) |
| trade | 17 | rpc_dump PVE, Lua | prices by set; "body part" groups in search; `sell_delay` |

## Structure (31.10)
`core/backend/src/rpc/handlers.rs` (~890 lines) — only `GatewaySession`, `push`/`clean_open_id`/`now_ts` and 230 delegates
`self.<rpc>_impl(req).await`, grouped by domain file. The logic of the former handlers.rs was split out:
`account_handlers` (login, player_get_info, cache), `role_handlers` (character creation, starting gear, attributes),
`affix` (affix generation), `battle_handlers` (level_select, matchmaking, battle room), `raid_handlers`
(report_action, raid results, deadmoney, experience, p2p host), `ds_handlers` (DS service, including hello_world/ds_end from misc);
pay_get_all → misc, team_match_exit → room. Function bodies were moved unchanged. Backup before the move:
`src_before_split_20261031.zip`.

## Wiring in handlers.rs (done by the orchestrator; the code now lives in the domain files)
- `player_get_info`: profile/nickname/icons/birthday/privacy options, lazy role deletion, quests, appearance,
  crafting, relics, soul binding, activities, merchant tabs, sold items, video popups, all coins from `state["coins"]`.
- `cache_batch_get`: real profile and online status from `lobby::hub` (offline: `logout_time > login_time`).
- The control scheme (`battle_update_setting`) goes to `deploy_preview` and `dirty_level_current`.
- `team_match_exit` cancels the party search (`team::match_cancel`).
- DLC grants moved to the shared `inv_new_item`/`inv_place_item` (the stash axes were swapped).
- Starting items in the backpack: width 7 along x and the item shape (was up to x=10, 1 cell each). Only for new roles.

## Second pass (30.10, by the orchestrator's decision)
- **GameData**: every map table has `<table>_rows: Vec<T>` with all rows (generator `build123.rs`
  and `core/backend/src/generated/generated_server_code.rs`). The HashMap is kept for compatibility — with a non-unique key use `_rows`.
- **Role base attributes by its level** (`role_level_upgrade_rows`; previously everyone got the level 20 row). Experience is not
  granted anywhere, so right now all characters have level 1 attributes — **weaker than before**. To restore the old behaviour:
  env `ROLE_ATTR_LEVEL=20`. Attributes are recalculated on `player_get_info`.
- Soul binding: gear parts by any group from `gear_soul_bound_wear_part_random_rows`.
- The item shape is the same in all domains (trade took the size without `ue_source_type_id` — 153 of 885 items
  would have been 1×1 and items could overlap).
- Friend gifts: `friend_send_gift` (friends only, 1 per day to each, `friend_daily_send_gift_limit`, expiry
  `friend_gift_expire`), `friend_receive_gift` (empty — all; into the stash via `inv_give`, `friend_daily_receive_gift_limit`).
- Room chat: channel `room_<room_id>` (type PrivateRoom = 7) — members of both teams and spectators.
- The party follows the captain: after `room_create`/`room_enter`/`room_accept_invite` by the captain, the other online party
  members enter the room (without a password).

## In-game check list (by decreasing importance)
1. Login/lobby/match are not broken (login, `player_get_info`, matchmaking, match, inventory).
2. Error via `RpcHead.error_code`: for example, renaming to a taken nickname — what the client shows.
3. Merchant shop: tabs are visible, buy/sell, gold changes.
4. Mail: a letter with attachments (GM: `gold`, `item` → or `mail_send`), receiving into the stash.
5. Friends/chat between two clients: request, online status, message to world/private.
6. Room: create, join with a second client, ready, match start.
7. Trade between two clients: list, buy, collect.
8. Crafting, repair, dismantling, insurance; activity pass.

## Known problems outside the handlers
- 18 of 86 tables have a non-unique key (`python -I tools/gamedata_dup_keys.py`) — the HashMap is incomplete for them,
  use `<table>_rows` (see "Second pass").
- Item placement is still in several places (shop `shop_put`, progress mail, trade, player GM, inventory
  `inv_give`): shape and axes are the same, but the code is duplicated — merge into `inv_give` once checked in game.
- Experience and levels (since 30.10): LocalSettle/PrivateRaid — `map_exp` by time in the raid (from the first match snapshot to extraction
  or death; up to 160 experience per 600 s), DS flow — `batle_info.add_exp`. Level up by `role_level_exp` up to the class's top
  level, attributes are recalculated, the player gets a `dirty_role` push. TODO(verify): experience on death, `team_bonus`.
- `player_game_over`: the `battle_settle_sync` results screen used to go to the DS server (`self.push`), now to the player via the hub.
