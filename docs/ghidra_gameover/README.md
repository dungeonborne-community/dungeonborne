# Native player result on the server (player_game_over) — Ghidra 02.11

Exe image from the dump (`db_image.bin`, base 0x140000000), Ghidra headless `DecompTree.java`, no analysis.
Decompilation: `go1.c` (death/escape/quit/finish, sending), `go2.c` (GetNetMode, subsystem, PlayerDeadPlayerState).

## Chain

The server itself strips the inventory from the pawn, builds `DSPlayerGameOverReq` and sends `tgf.DsBattleService.player_game_over`.
It is not sent only because the check "dedicated server + DS SDK connected" fails.

| Address | What | How found |
|---|---|---|
| 0x142ea7390 | PlayerGameDeadPlayerState(this, PS) | log string 0x14771da30 "… Begin" |
| 0x142ea7430 | PlayerGameEscapePlayerState | 0x14771dac0 |
| 0x142ea7c20 | PlayerGameQuitPlayerState (SELF_QUIT / DEAD) | 0x14771d910 |
| 0x142ea74d0 | PlayerGameFinish (response/retry: 0x142e7ef90, 0x142f33fa0) | 0x14771e240, 0x14771dc70 |
| 0x142ea71f0 | UBattleDsUtility::PlayerDeadPlayerState (exec 0x142d8e090) → 0x142ec5bc0 DumpSmallBackpackToCache | strings 0x14771dba0, 0x1477270d0.. |
| 0x142eccff0 | PS → OpenId (map key) | |
| this+0x80 | TMap OpenId → BattleUserData (filled by AddBattleUserData 0x142e9b2f0, in our case by dbnative) | "GetBattleUserDataByOpenId Failed" |
| BattleUserData+0x78 | player status; 3 = game over (not processed again); +0x7c reason | |
| 0x142ec68a0 | DumpUserDataToCache(PS, data, …): equipped items, bags, small backpack (SmallBackpackSaveDied), statistics, attributes → DBattleUserData cache | log strings 0x147726a20..0x147727040 |
| 0x142f96b20 / 0x142f96be0 | Item filter in DumpUserDataToCache by c3: c3 = 0 (leaving the raid) — taken if bit 0 of the item type flags (+0x68 of the config) is clear; c3 = 1 (moving to the next floor) — if bit 1 is clear. bring_out_ingame_type: 1 = CAN_NOT_BUT_ALLOW_CROSS_LAYER (bit 0), 3 = CAN_NOT_ALL. With c3 = 0 an extraction record is also written (0x142ecf960). 10.11: dbnative gofloor calls it with c3 = 1 — resurrection stones carry over to the floor | Ghidra 10.11 |
| 0x142ec61b0 | cache → DDsUserBattleResult (ds_user_key, status, wearing/backpack/small_backpack …) in DSPlayerGameOverReq (vtable 0x146d37670) | |
| 0x142e9e8b0 | send check: GetNetMode (0x1455b9fb0) == 1 (NM_DedicatedServer) AND the GameInstance subsystem (0x142e760f0, class 0x142d8f960) exists AND its flag +0x68 | |
| 0x142f69900 | sending: method "player_game_over" (0x147747840), serialization, subsystem transport (0x142f6c4f0) | |

Order in Dead/Escape/Quit/Finish: find BattleUserData by OpenId → if status != 3: DumpUserDataToCache → build
DDsUserBattleResult → (PVE 2001: event DsBattleBringOutItemFlow) → if 0x142e9e8b0: player_game_over.
Game types 2002 and 3001 (this+0x230) — exit without a result.

## Joining a battle (rejoin)

| Address | What |
|---|---|
| 0x142fa17f0 | ALyraGameMode::PreLogin: if 0x142e9e8b0 (dedicated + DS SDK) — `OpenId`, `token` from the URL → validator; error → rejection |
| 0x142ea8f10 | validator(this, OpenId, token, …, ErrorMessage): map this+0x30 OpenId→token; missing — "Invalid Player", wrong — "Invalid Token"; no data — "Player Data Invalid"; data+0x238 and not this+0x1fc — "Player Game Status Invalid" |
| 0x142ea84a0 | PlayerQuit: data+0x238 = 1 (also only with 0x142e9e8b0) |
| 0x142e9ee80 | IsPlayerGameOver(PS): record status == 3 (no record — true) |

On listen the check is disabled entirely. Replacement — DungeonborneRuServer `JoinGuard` (flag `dbserver-rejoin.txt`).

## Consequences

- On listen (including headless listen) NetMode = 2 → native sending never happens; on dedicated — only
  with the DS SDK connected (DSA/GRome).
- The DBattleUserData cache is still updated (Dump runs before the check) — if the record is in the map (dbnative
  AddBattleUserData by OpenId). Hence the importance of the correct OpenId on the PlayerState.
- Not verified in game: that Dead/Escape/Quit are actually called on our server and that the cache is complete after them.
