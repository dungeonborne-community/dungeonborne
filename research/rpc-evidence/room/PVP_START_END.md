# Custom games (PVP rooms): start indicator and auto-leave after the match — client analysis 05.11

Sources:
- Lua `lua_orig/` and `<data>\research\lua_all`;
- BP — `bp/cfg/*.txt` (retoc + kismet-analyzer);
- C++ — the exe image (`db_image.bin`), Ghidra: `pvp_gameover.c`, `pvp_winteam.c`, `pvp_matchwindow.c`.

## 1. The "Start" button and the indicator

- `WBP_PVP_RoomWIndow.OnBtnStartClicked` checks three conditions, each with its own toast:
  - both teams are not empty — `PVPRoom_Room_TeamNotReady`;
  - everyone is ready — `PlayerNotReady`;
  - everyone is online — `PlayerNotOnline`.
  Then it sends `PVPService:SendRoomStartBattleReq(VelocityService:GetCurSelectRegionData().Id)`. The room window has no indicator of its own.
- The indicator is a separate window **`/Game/TGF/UI/WBP/PVP/WBP_PVP_MatchWindow`**. It is opened by C++, function `0x145b3cf60`,
  the handler of the `DRoomMessageAction` list (dirty private room action, `switch` on the `oneof` field number):
  - fields 51–65 (`map_action` … `invite_notify_action`) update the room data; 64 `status_action` → RoomInfo+0x98;
  - **112 `create_battle_action` (bool) = true** → after parsing the packet, `OpenWindowBySoftPath(WBP_PVP_MatchWindow)`;
  - if 112 is absent, a TGF event (string 0x148331990) is sent instead of the window: the usual "room updated";
  - 110 `start_battle_action`, 111, 113 `finish_battle_action`, 114 are not touched by this handler.
- `WBP_PVP_MatchWindow` listens only to the TGF event `RoomCancel`: toast `RoomCancelReason_<reason>` and closing the window.
  There are no other closes in the BP; the window goes away when the match map loads (level change).
- Result: to get the "creating match / looking for a server" indicator, `dirty_private_room_action` needs the action
  `create_battle_action = true` **before** `start_battle_action` and the `battle_room/matching` events.
  On cancel: room CANCEL, the client sends `RoomCancel` and closes the window.

## 2. End of the match and "leaving in 5 s"

- Server (authority), `BP_Tgf_PVPPhaseHint` (a GameState component):
  - victory → `ServerQuit(Winner)` → native `UBattleDsUtility::PVPGameWinTeam(TeamId)` (exec `0x142d8dc50` → `0x142ea6950`);
  - then `StopCharatersMovement`, `ShowGameOver` and the start of the Lyra phase **`Phase_PVP_GameOver`**.
- `Phase_PVP_GameOver.K2_ActivateAbility`:
  - `PhaseHint.ServerSetIsPlaying(false, ServerWorldTime + DelayTime)` → `GameOverEndTime` replicates;
  - `Delay(DelayTime)` → **`UBattleDsUtility::GameOver()`** (exec `0x142d8b630`) → `K2_EndAbility`.
- Client, `WBP_TGF_PVP_GameOver` (the "Victory/Defeat" window):
  - `SetLeavingTime` — a timer every 1 s, `CountDown`;
  - `CountDown` reads `Get Phase End Time` (phase 3) from `PhaseHint`; while `ceil(Rest-1) > 0` it writes `PVP_Leaving_Game_CountDown`; on reaching 0 it **only clears the timer**;
  - the `ButtonLeave` button → `TgfSystemBlueprintLibrary::OnQuitGame` + `GameplayStatics::OpenLevel(<empty>, true)`, i.e. back to the lobby.
- Native `GameOver` (`0x142d8b630`) is **entirely behind the check `0x142e9e8b0`** (NetMode == Dedicated AND DS SDK flag +0x68).
  On a real DS:
  - games of type 2002/3001 → `0x142ea6950`;
  - otherwise `PlayerQuit` + `PlayerGameFinish(1|2)` (player_game_over) for each player;
  - then `0x142f6caf0` / `0x142f44750` — DS shutdown.
  Clients go to the lobby because the DS ends the session, not because of a backend push.
- `PVPGameWinTeam` → `0x142ea6950`: for 2002/3001 it collects results of all players (`DumpUserDataToCache`, `DDsUserBattleResult`),
  sending `player_game_over` (type 3) only when NetMode == 1 (dedicated). On listen nothing is sent.
- **Conclusion:** on our listen server `GameOver()` is empty, so there is no auto-leave. No backend push will enable it.
  Options:
  1. client mod: when phase 3 of `BP_Tgf_PVPPhaseHint` has ended (or `WBP_TGF_PVP_GameOver` has counted down to 0),
     do what the button does: `OnQuitGame()` + `OpenLevel(world, "", true, "")`;
  2. server mod: on `GameOverEndTime`, `ClientTravel` all remote PCs to the lobby or shut down the server instance, like a DS.

**Done 05.11 (option 1):** `DungeonborneRuClient`, the `PvpLeave` block at the end of `main.lua`.
- Polling every 0.5 s, only on the client (not with `-dbserver`).
- Conditions: `WBP_TGF_PVP_GameOver_C` is shown, `BP_Tgf_PVPPhaseHint.GameOverEndTime > 0`,
  `GameState:GetServerWorldTimeSeconds() >= GameOverEndTime + 1`.
- Then `BndEvt__WBP_TGF_PVP_GameOver_ButtonLeave_…` is called — the "Leave" button handler, once per window.
- Log `[PVPLEAVE]`. Switch: `ru-pvpleave.txt` = 0.
- Installed in the client `C:\Games\Dungeonborne`. Not verified in the game.

## 3. The PVP results window in the lobby

`Lua/Game/System/PvpResult/WBP_PvpSettle.lua`. HallWindow opens it (BattleResult auto-popup); closing sends `PVPResultCloseEvent`.
Data: `UMatchService:GetResultData()` from **`SCBattleSettleSync`**:
- `GsResult.SettleInterfaceType` (BO5 / BO5_TRIAL);
- `Result.ParticipateData[]`: `RoleGuid`, `FactionId`, `Statistics.DeathCount`, `ActorType` (ALIVE/ESCAPE);
- `Result.DsResult.PlayerPvpRoundData[]`: `Result` WIN/LOSE.

Without `battle_settle_sync` there is no PVP results screen in the lobby. It does not affect auto-leave.
