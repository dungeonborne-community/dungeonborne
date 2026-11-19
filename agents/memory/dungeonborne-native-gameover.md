---
name: dungeonborne-native-gameover
description: Server natively dumps player inventory and builds player_game_over; blocked only by dedicated+DS SDK gate (addresses in docs/ghidra_gameover)
metadata:
  visibility: team
  node_type: memory
  type: project
---

02.11.2026 Ghidra: on death/escape/quit/finish the server (UBattleDsUtility internals 0x142ea7390/7430/7c20/74d0)
looks up BattleUserData by OpenId (map this+0x80, filled by AddBattleUserData — our dbnative), runs
DumpUserDataToCache 0x142ec68a0 (wearing, bags, small backpack rule), builds DDsUserBattleResult 0x142ec61b0, and
sends player_game_over 0x142f69900 only if gate 0x142e9e8b0 passes (NetMode==Dedicated AND subsystem flag +0x68).
Listen/headless listen never sends. Full table: `docs\ghidra_gameover\README.md`.

Rejoin (02.11, tested OK): native PreLogin validator 0x142ea8f10 is gated off on listen too. A returning player
gets the OLD PlayerState and OLD PlayerController — only the UNetConnection (PC.Player) is new; ALyraGameMode is
AGameModeBase (no InactivePlayerArray). DungeonborneRuServer JoinGuard polls remote PCs, OpenId from RequestURL
(dbnative op=connurl), sends repeat/anonymous joins to lobby via ClientTravel. Client UE4SS crashes after disconnect
came from RuClient timers holding UObject wrappers — use path + StaticFindObject (PathOf/LiveObject).

Death/leave (02.11, tested OK): a dead player's result (status 3 reason 2) appears only when he exits; the exit
button is server RPC ALyraPlayerController::ServerSelfQuitGame — calling it server-side runs the native quit chain
(ClientTravel alone does not). Kill: DamageSelfDestruct does nothing in shipping; apply
GE_TGF_Damage_DeathZone_InstantDeath via ASC BP_ApplyGameplayEffectToSelf. Host verbs kill/leave in RuServer.

**Why:** user wants server-authoritative raid results instead of client report_action snapshots
(LocalSettle.lua is dead since 27.10; PrivateRaid.lua covers only the host's own player).
**How to apply:** build the settle path on the native cache/result, not on Lua inventory snapshots. Not yet verified
in-game that these functions fire on our server. Related: [[dungeonborne-listen-native-data]], [[dungeonborne-dedicated-plan]]
