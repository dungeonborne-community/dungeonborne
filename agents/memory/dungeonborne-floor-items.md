---
name: dungeonborne-floor-items
description: "Floor transition: the native DumpUserDataToCache c3=1 keeps items \"cannot be taken out, but allowed to the next floor\" (resurrection stones); game ID — LyraGameState.BattleId"
metadata:
  visibility: team
  type: project
---

10.11 (stand, confirmed by the user): our transition to the second floor is our own — dbnative `gofloor` takes off the items with the native
`DumpUserDataToCache(PS, data, c3, c4, reason, c6)` (0x142ec68a0). c3 = 0 — "leaving the raid": filter 0x142f96b20
throws away everything with bit 0 of the type flags (bring_out CAN_NOT_BUT_ALLOW_CROSS_LAYER = 1, e.g. resurrection stones 100014/100046);
c3 = 1 — "to the floor": filter 0x142f96be0 (bit 1, CAN_NOT_ALL = 3), without an extraction record. Since 272fbc1 gofloor calls c3 = 1.
The backend logs the stones from the snapshot (`floor_snapshot … resurrection stones:`).

Game ID: the native WBP_TgfGame_Ping shows LyraGameState.BattleId (purple ID block); the server writes it with
KismetSystemLibrary.SetStringPropertyByName + ForceNetUpdate — it replicates, no client mod is needed (7e5092a,
docs/BATTLE_ID_IN_PING.md).

**Why:** stones disappeared on the second floor; our own ID label was redundant.
**How to apply:** new "to the floor" items need no changes; if something else is lost in the transition — first the
floor_snapshot line in the backend log. Related: [[dungeonborne-native-gameover]], [[dungeonborne-bp-reverse]].
