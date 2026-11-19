# Leaving the waiting hall: native vs ours (analysis 04.11, B12)

Exe image from the dump, Ghidra headless (`DecompTree.java`), read only. Decompilation: `l1.c` (ALyraGameMode::Logout),
`l2.c` (PlayerGameFinish, InitGameMap, PlayerQuit, QuitPlayerState), `l3.c` (building the result, DumpUserDataToCache),
`l4.c` (scene resets to 0).

## Scene: `UBattleDsUtility+0x1f4`
The value is `EnDsBattleScenesType` (1 WAITING_ROOM, 2 LEVEL_ONE, 3 LEVEL_TWO, 4 PURPLE_DOOR).
Only `InitGameMap(map_id, timeout, IsIndependentMapMode)` 0x142e9c1e0 sets it: with IsIndependentMapMode it takes
`scenes[0].type` from map_detail, without it it moves to the next scene. When leaving scene 1, it broadcasts
`WaitingRoomLeaveDelegate` (+0x308). Server travel from this function happens only in DS mode. The other writes store 0
(0x142ec10d0, 0x142ec2570, 0x142ebf510).
**Ours (listen, mod without InitGameMap): the scene is always 0.**

## PlayerGameFinish(PS, reason) 0x142ea74d0 — the common finish for Dead / Escape / Quit / Ban
The record is taken by OpenId from the map at +0x80. If its status != 3:

1. **If scene == 1 (hall):**
   - the OpenId is removed from +0xd0, **+0x30** (OpenId→token, checked by PreLogin), **+0x80** (BattleUserData) and
     +0x170 (joined players);
   - `Current = +0x178 − +0x1a4`, `Total = +0x240` (with flag +0x245: `Current, Current`);
   - **`PlayerWaitingRoomLogoutDelegate` (+0x328)** is broadcast. BP_TgfWaiting.OnPlayerLogout calls
     `OnUserCountChanged`.
2. `DumpUserDataToCache(PS, data, 0, scene != 1, &reason)` (0x142ec68a0): status 3 and the reason are always written.
   **In the hall, removing equipped gear, bags, backpack and stats is skipped**: the record keeps the data the
   player joined with.
3. Building `DDsUserBattleResult` (0x142ec61b0): `batle_info.finish_scenes` = scene (1 = WAITING_ROOM).
4. In DS mode `player_game_over` is sent, **including from the hall**: game_type 3, actor_type 1 SELF_QUIT,
   finish_scenes 1.

PlayerQuit 0x142ea84a0 (record flag +0x238 forbids rejoining) works only in DS mode.

## ALyraGameMode::Logout 0x142f99b30
- Removes the player from the GameMode map at +0x360 by the string PS+0xa00.
- Calls Super::Logout (the normal UE path: controller, pawn and PlayerState are destroyed).
- Calls the object at +0x2f0 through vtable +0x718.
