# Reference: match and battle room (sniffer_log_001.txt, 26.10, live server)

Decoding: `cargo run --example sniff_decode -- battle_` (requests/responses) and `cargo run --example push_try`
(pushes without routing: `[S2C] [PUSH] UNKNOWN_TYPE_0` — the type is picked from `*Sync` candidates). One player
(user_id <id>, zone 2200), map 10201 (Castle), level 40003016, DS `198.51.100.2:7867`.

## Sequence

| t | What | Contents |
|---|---|---|
| 23:41:50 | `battle_matching_pre_search` → `search` | pre_search response is empty, search right away |
| :50 | push `SCBattleMatchingLogSync` #1 `MATCHING` `startMatching` | snapshot: matchingLevel {2001, 40003016, region "de"}, participators |
| :50 | push `SCBattleMatchingLogSync` #2 **`CAN_JOIN_IN` (8)** `matched` | roomKey, roomData {create/expired (+2400), battleId, **dsAddress**, mapData}, participators (deployPreview, dsPlayerKey) |
| :51 | push `SCBattleRoomLogSync` #1 `DS_INIT` `startDsInit` | end_timepoint ≈ +200 s |
| :52 | push `SCBattleRoomLogSync` #2 `DS_INIT` `dsInitFinish` | dsAddress, dsPlayerKeys [{userKey, dsKey {dsUserKey = Steam ID, dsToken "1001"}}] |
| :53 | push `SCBattleRoomLogSync` #3 `LOADING` `startLoading` | roomKey, roomData, participator |
| :54 | push `SCBattleRoomLogSync` #4 `BATTLING` `startBattling` | end_timepoint = +2400 s |
| :56 | push `SCBattleMatchingLogSync` #3 `FINISHED` (20) `finished` | |

- Each push is **one new event** + a snapshot (status and last_event_id of that event, participator with dsPlayerKey
  from the very first event).
- `battle_room_heartbeat`: the request holds only `battleId` (**acknowledge_event_id is not sent**), the response is **empty**;
  `battle_matching_heartbeat` — empty responses too. Events arrive only as pushes.

## Differences between our backend and the reference (31.10)

- Battle room — fixed (T3): `lobby_handlers::lobby_battle_room_heartbeat` sends events per the reference, one
  per heartbeat, remembers what was sent per (battle_id, open_id), and until the server answers init_player sends only startDsInit.
- Match — fixed (31.10): events per the reference, one per push — `startMatching` (MATCHING, snapshot with
  matchingLevel and participants; `lobby_handlers::lobby_matching_search`), `matched` (CAN_JOIN_IN = 8: roomKey,
  roomData with dsAddress and mapData, participants with dsPlayerKey; `matchmaking::notify_matched`), `finished` (FINISHED = 20,
  after startBattling was sent; `matchmaking::notify_finished`). `is_switch` = false, as in the reference.
  Difference: in `matched` the participants have no deployPreview (tick without the DB) — full participants come in startLoading.
  Our Lua wrappers do not check the match status (it used to be `match_status = 4`); in native mode the button is driven by the original
  client logic — check in game that after CAN_JOIN_IN it moves on to battle_room_heartbeat.
