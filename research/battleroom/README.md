# Match room on the client (URoomService): exe analysis 04.11

Exe image from the dump, Ghidra (`r1.c`…`r12.c`), read only. `UFunction` strings → class `URoomService`
(`OnGameLogout`, `OnNetworkManagerEnterState/ExitState`, `Get/SetShouldPromptBattleVersionNotCompatible`).

## URoomService fields (offsets)
| Offset | What |
|---|---|
| +0x40 | std::string battle_id (size at +0x50) — **a room exists while the string is not empty** |
| +0x60 | copy of DBattleRoomSnapshotData: +0xc0 last_event_id, +0xd0 room status |
| +0xdc | "send heartbeat" flag: 1 / 0 when NetworkManager enters / exits the right state (0x1459f7c00 / 0x1459f7e40) |
| +0x170 | time of the last heartbeat; interval at +0xd8 |

## heartbeat
- Tick 0x145a227a0 (FTickableGameObject, this+0x38): sends if `+0x50 != 0` (battle_id not empty) and `+0xdc`.
- Send 0x145a1f6d0 / stub 0x145ae71c0: acknowledge_event_id = +0xc0, battle_id = +0x40.
- The response (0x1459829d0) **is ignored**: a response cannot stop the heartbeat.

## SCBattleRoomLogSync — 0x1459f8c80 (registered in 0x1459f2630)
1. **battle_id from the message is always copied to +0x40**: any sync "revives" the room.
2. If there is a snapshot (hasbit 0), **only the snapshot** is processed (0x145a1ad50), `event_logs` are skipped:
   - the snapshot is copied to +0x60;
   - status 8 BATTLING (changed) → connect to the DS;
   - status 11 CANCEL → `0x1459f5ee0` (room cleanup) + `0x1459f3760` (search reset, MatchStop event) +
     the `RoomCancelReason_%d` window with cancel_info.reason;
   - 9 FINISHED and 10 TIMEOUT **close nothing**.
3. Without a snapshot: events one by one (0x145a1a4e0). An event is applied only if last_event_id (+0xc0) != 0 and
   `event_id == last + 1`. On a skipped number the client only speeds up the next heartbeat; old numbers are ignored.
   - 16 start_loading — reply to the server;
   - 17 start_battling — connect to the DS;
   - **18 cancel — cleanup + MatchStop + reason window** (like snapshot 11);
   - **19 finish — not handled**;
   - 108 ds_init_finish — stores the DS data;
   - 201 game_over_settle — only if there is a player_game_over and it is about the own player (0x145a1d8a0).

## Silent cleanup: SCPlayerDirtyChgSync.dirty_clear_battle_room (field 21)
Dirty handler 0x145b27930 → URoomService 0x1459f49b0. If a string from the list matches battle_id (+0x40):
`0x1459f5ee0` (battle_id, snapshot, +0xe0, +0x118 cleared) and `UMatchService 0x1459f3760` (0x1459f3a10 resets
the search state, then the `MatchStop` event). No window.
Field 20 dirty_clear_matching → UMatchService 0x1459f48d0: the same 0x1459f3760 when the search id matches (+0xe0).

Full cleanup (0x1459f5fe0, plus +0xdc = 0) only on `OnGameLogout`.

## Entering the lobby — 0x145993920 (NetworkManager enters a state, mode +0x38 == 2)
1. battle_id not empty and status (+0xd0) == 8 (0x1459f2b00) → 0x145a1bfd0: reconnect to the DS at the stored address.
2. Otherwise battle_id not empty (0x142cf1690) → 0x145a1d0f0: MatchToast(MatchState = 2 Matched) + `MatchStart` event.
3. Otherwise a search is running in UMatchService (+0x63 / +0x188, 0x1459f2b40) → search toast (0x145a1d0d0).
A clean lobby only with an empty battle_id (dirty_clear_battle_room or cancel).
