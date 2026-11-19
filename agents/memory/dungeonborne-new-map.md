---
name: dungeonborne-new-map
description: "11.11: own map — what is placed in the Castle level and what the game creates by itself; idea: only geometry in the umap, the server mod places the game markers"
metadata:
  visibility: team
  node_type: memory
  type: project
---

On 11.11 the user wants a new scene ("a proper replacement, most objects are created dynamically"). Nothing started, waits for a "yes" on a prototype.

Census of the Castle level (`research\castle_spawn\legacy`, 875 World Partition cells, script census.py on top of `tools\pkg.py`):
- geometry and light: ~89 thousand StaticMeshActor, HLOD, PointLight, BlockingVolume, Ak sound;
- game markers (on them the game later creates mobs, chests, portals): BP_ActorSpawnPoint 1054 in BP_ActorSpawnRoom 104 / BP_ActorSpawnSet 120, BP_PatrolPoint 206, LyraPlayerStart 168 in BP_LyraPlayerStartSet 56, BP_EscapeDeviceSpawnPoint 15, BP_DeathZone 5, BP_TGF_CombatZone, door/floor mechanisms, NavMesh, MinimapReferencer.
- The markers' parent is the native `TgfModularActor` (/Script/LyraGame, not from public Lyra) → they cannot be placed in our own editor without stubs.
- Navmesh: `RuntimeGeneration=DynamicModifiersOnly` → the navmesh must be baked in our map. Copy the Lyra/Tgf collision channels (GameTraceChannel1–18) from the game's DefaultEngine.ini.
- Map defaults: GameMode `B_LyraGameMode`, match experience `B_Tgf_Game_Experience` (Lyra also takes the experience from `?Experience=` in the URL).

Plan idea: a umap from our UE 5.1 (geometry + navmesh + light, ordinary WorldSettings), the server mod places the markers from a layout file before the match starts, marker settings are carried over "by rooms" from the Castle level. The mod is the orchestrator's area. Related to [[dungeonborne-jester]], [[dungeonborne-warn-before-pak-re]].
