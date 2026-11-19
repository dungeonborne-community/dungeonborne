# Score table after a PVP match (Deathmatch, custom games)

Analysis from 11.11. Sources:
- BP: retoc → kismet-analyzer;
- client Lua: `luac54 -l`;
- the `map_detail` table from the original's dump;
- backend and mod code.

I did not touch the stand or the game, no changes. Working files: `<data>\research\pvp_settle`:
- `*.bc.txt` — BP bytecode;
- `HallWindow.dis.txt` — lobby Lua.

Earlier analysis of match start and end — `tools/research/room/PVP_START_END.md`.

## Conclusion
**In the original the score table is a lobby window, not a match window.** It is opened by the `battle_settle_sync` push with the results of all
participants. For arenas the backend does not send this push: the end of the match reaches it only as an "empty hall" (`phase=empty` →
`room_finish_push`, "match over"). So there is no window in the lobby.

Our auto-leave `PvpLeave` (RuClient) is not to blame. It takes players to the lobby at the same second when the real DS would itself close
the session. There is no final table in the match in the original either.

## How it works in the original
1. **Match, server.** `BP_Tgf_PVPPhaseHint.ServerSetIsPlaying(false)`:
   - mode is not single (`IsPVPSingleMatchType` = false): `PlayingEndTime = 1`, `Server Notify Single Settle(true)`;
   - then for all modes: `GenerateWinner` → `ServerQuit(Winner)` (native `PVPGameWinTeam`) → `StopCharatersMovement` →
     `ShowGameOver`.
2. **`Server Notify Single Settle(IgnoreSingleSettle)`.**
   - With `false` (start of round 2+, from `Server Set Warm Up End Time`) it computes the statistics increment and sends the RPC
     `ClientSingleSettle`. The client opens the intermediate round table `WBP_TGF_PVPMulti_SingleSettle`.
   - With `true` (end of match) it does **not show** the table in the match: only the native `PVPGameWinTeamRound`.
3. **Match, client.** `WBP_TGF_PVP_GameOver` — only "Victory/Defeat/Draw", a countdown and the "Exit" button.
   There is no table in it.
4. **Native DS** (`PVPGameWinTeam` / `GameOver`, games 2002/3001):
   - collects the results of all players and sends `player_game_over`;
   - works only with dedicated + DS SDK; for us this check does not pass (`docs/ghidra_gameover`).
   The game server sends `SCBattleSettleSync` out based on these results.
5. **Lobby, client.**
   - `WBP_HallWindow` on open: if `UMatchService.ShouldShowBattleResult` and
     `ResultData.LevelParameter.LevelType` = `kCompetitionSeasonPvp` (2002) or `kPrivateBattle` (3001) — opens
     `/Game/TGF/UI/Game/PVP/WBP_TGF_PVP_Settle` (Lua `WBP_PvpSettle`).
   - The window builds the table from `Result.ParticipateData[]`:
     - `FactionId` 1 — blue, 2 — red;
     - `RoleGuid`, `UserKey` (name via RoleInfoCtl);
     - `Statistics`: `KillPlayerCount`, `DeathCount`, `CausedTotalDamage`, `GotTotalDamage`, `CausedTotalHeal`;
     - `ActorType`: ESCAPE for the winning team, ALIVE — draw;
     - for BO5 also `DsResult.PlayerPvpRoundData[]` (WIN/LOSE per round).
   - The window type — `GsResult.SettleInterfaceType`. In `map_detail` arenas 30101/30201/30301 and 40001 have `PVP`,
     30102/30202/30302 — `BO5`.

## What we have
- RuServer: on the `BattleDsUtility:GameOver` hook for a `pr…` match sets `Host.Phase = "empty"`
  (`main.lua`, the `GoHooks` block).
- backend `agent_link.rs` `slot_heartbeat`: `phase=empty` → `members_absent` + `room_finish_push`. No results.
- `battle_settle_sync` is sent only by `raid_native::raid_push_settle`, and only the PVE raid window.

## Fix plan (after the user's "yes")
1. **RuServer.** Hook `/Script/LyraGame.BattleDsUtility:PVPGameWinTeam(TeamId)`. The BP calls it at the end of the match, ~5 s
   before `GameOver`. For each `BP_TGF_PlayerState_C` collect:
   - `MyOpenId`, `GetTeamId()`;
   - from its controller's `TgfActorStatisticComponent`: `GetIntStat(1,1)` — kills, `GetIntStat(51,0)` — deaths,
     `GetFloatStat(3)` — damage, `GetFloatStat(5)` — damage taken, `GetFloatStat(4)` — healing. This is exactly what
     the original's `MakeStatisticDataFromPlayer` reads.
   Plus the winning team. Send `POST /ds/arena_result {battle_id, winner_team, players[]}`.
   UFunction calls only, we do not write FStrings.
2. **backend.** New `/ds/arena_result`: `ScBattleSettleSync` to each participant via `hub::push_to` while they are still in the match:
   - `level_parameter`: `level_type 3001`, the room's `level_id`;
   - `result.participate_data` — all players: `user_key`, `role_guid`, `role_name`, `faction_id` from the match team,
     `statistics`, `actor_type` ESCAPE for winners / ALIVE on a draw;
   - `gs_result.settle_interface_type` from `map_detail` (PVP or BO5).
   No experience, loot or raid records. Room end (`phase=empty`) — as now.
3. **Check on the stand** (1v1 match, two clients or a client and a bot):
   - the table opened in the lobby, teams and numbers are correct;
   - victory and defeat for both sides;
   - `[ARENA]` in the logs.
   For BO5 (30x02) separately: `PlayerPvpRoundData` — in a second pass.

The client mod does not need changes. Keep `PvpLeave`.

I do not suggest opening the DS SDK check so that the native code collects the results: it needs the original's SDK server, the risk is high.

## Done 11.11 (user's "yes")
- **backend** `POST /ds/arena_result {battle_id, winner, players:[{open_id, team, kills, deaths, damage, heal, taken}]}`
  (`core/backend/src/ds/agent_link.rs`). Building the result — `core/backend/src/rpc/handlers/arena_settle.rs`, tests `arena_settle::tests` pass.
  - `battle_settle_sync` to each room participant:
    - `level_type` 3001, the room's `level_id`;
    - `map_data` = the arena map;
    - `participate_data` of all players: faction 1/2 by match team, statistics, `actor_type` ESCAPE for winners /
      DEAD for losers / ALIVE on a draw;
    - `settle_interface_type` PVP or BO5.
  - The last result — in GM `settle last`.
  - Experience, loot and raid records are not touched. Log `[Arena]`.
- **RuServer** (`main.lua`, inside `GoHooks`): hook `BattleDsUtility:PVPGameWinTeam`.
  - Only a `pr…` match with a single scene, once per match.
  - Statistics from `TgfActorStatisticComponent` → POST. Log `[ARENA]`.
  - Switch `dbserver-arenaresult.txt` = 0.
- **Not checked in game:**
  - whether the hook fires;
  - whether the numbers arrive;
  - whether the window opens and whether the rows have names.
  - BO5 without rounds (`PlayerPvpRoundData`).
