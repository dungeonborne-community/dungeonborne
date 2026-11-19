---
name: dungeonborne-shadow-arena
description: "Purple portal = Shadow scene, the throne arena inside L_Dungeon_Official_02 (120000,0,-80); RuServer moves players there; second floor without purple ones"
metadata:
  visibility: team
  node_type: memory
  type: project
---

The purple portal (Shadow) goes only from the first floor and leads to the Shadow King boss in the throne arena. The arena is part of the second-floor map `L_Dungeon_Official_02`, the throne at (120000, 0, -80). The separate map `L_Common_Throne_01` has no mode and no boss: do not send players there. Verified in game 06.11, user: "everything is as in native".

**Why:** the native `TgfPlayerGameFlow.ServerTeleport` takes both red and purple to a hall under the map, the Shadow scene is passed on via `scene_tag`, on listen this hand-over does not exist. RuClient used to erase the scene on every pass, which made the mechanic look broken.
**How to apply:** user rules: the second floor has neither grey nor purple portals, only blue; purple never becomes red. Code: `Spect.ArenaTick`, `Spect.ToArena` in RuServer. Red and purple are compatible in one match, one map. Details: docs/FIX_LIST.md PVE1, docs/ghidra_portals. Related: [[dungeonborne-listen-native-data]].
