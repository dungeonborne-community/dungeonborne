---
name: dungeonborne-listen-native-data
description: "Working combo (01.11) for \"listen as dedicated\" — dbnative per-player data + hero UCS patch for R14; what was ruled out"
metadata:
  visibility: team
  node_type: memory
  type: project
---

01.11.2026: first fully working run — headless listen server, client DK gets its OWN data natively (Str37 Dex18 Sta15
Int25, HP 1018, 9 items, real ids/OpenId) on server AND client, no crash, connection held until quit.

Combo:
- Server: `game\ue4ss\native\dbnative` DLL (loaded via package.loadlib from DungeonborneRuClient hook on
  InitPlayerGameData; native_data.txt with backend URL `/ds/battle_user_data?open_id=`), native_init.txt=1,
  settings.txt native_data=1 (LocalDev does not re-dress). No exe patching: only calls of game functions
  (AddBattleUserData 0x142e9b2f0, protobuf parse 0x145f93780, DBattleUserData ctor 0x1424a1930, SetOpenId 0x142fe79a0).
- R14 (0x142f602c6, avatar UpdateMesh -> GetEquipBySlot on null equip manager, happens in the hero's creation frame,
  native manager not in first bunch): pak `research\bpfix_pak_hero` (zzzzzzz_BPFix_P, UCS stub
  SetIsReplicated(false)) + UE4SS mod DungeonborneR14 on the CLIENT (removes stub on native OnRep);
  DungeonborneRuClient OFF on the client.

Mobs (01.11): headless observer must NOT DestroyCharacter the host pawn — spawn rooms (BP_ActorSpawnRoom ->
native UPlayerDistanceCheckerComponent) then only check the host; mobs spawned only where the host stood first 5 s.
Fix in DungeonborneRuServer MakeObserver: default steps quit+hide+freeze (pawn kept, hidden, no collision,
DisableMovement); dbserver-observer.txt selects steps. Not the cause: -nullrhi, map option rules, WP streaming.

Charge attacks (01.11): with the host pawn kept alive, DungeonborneRuClient's listen-host release bypass (ReceiveTick)
released CLIENT charges after 0.1 s — fixed by binding it to the ability's avatar (ChargeAvatarAddr).
Tick rate (01.11): listen tick = frame rate; Windows 11 throttles the windowless background server (EcoQoS, ignores
timeBeginPeriod) -> 43-52 fps with 31 ms frames at t.MaxFPS 60. dbnative op=perf (power throttling off + ignore timer
resolution + timeBeginPeriod(1)) called by DungeonborneRuServer in Ready -> steady 60.0; dbserver-tick.txt sets cap.

Ruled out: CrashTracer, DungeonborneRuServer, LocalDev client code, OpenId restore, DK-specific data;
UnLua timer stub is too late (hero+body crash in same frame); UE4SS R14 stubs crash UE4SS 3.0.1.
Related: [[dungeonborne-dedicated-plan]], [[dungeonborne-bp-reverse]], [[dungeonborne-workflow]]
