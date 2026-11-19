# The Wendigo treasury door (second floor) does not open

Analysis of 08.11 on the user's request. Static only: assets from the paks (retoc → kismet-analyzer; the level — UAssetGUI with an enlarged stack), native — Ghidra on the exe image, the mod and the git history. I did not run the game or the server and did not touch prod. Working files: `<data>\research\wendigo`.

## Conclusion
- **The door is opened by the game itself, it is not in our code.** The "Wendigo death → door" link has been read in full and looks intact: all links are server-side (authority), the "open" signal = 1.
- **This is not a regression of the mod refactoring.** Stage 1 (`3e188cb`, 07.11) moved only the RuServer main loop to the game thread (44 lines), it did not touch spawning, monsters or doors. The second floor appeared on prod on the evening of 06.11; before that players did not reach the Wendigo. There is no evidence that the door ever opened on our servers.
- **Where exactly it breaks is not proven statically.** Below are the suspect links and a probe that will show the exact place in one match.

## 1. How the door opens in the game
Map `L_Dungeon_Official_02` (persistent level, always loaded):

| Actor | What |
|---|---|
| `BP_ActorSpawnPoint_C_UAID_0068EBC94D99F2CD01_1841134598` (export 1349) | Spawns `BP_Tgf_Monster_Wendigo_C`. `ControlledMechanism` = the right door leaf, `DataMonsterCombatZone` = `BP_TGF_CombatZone` (1953). `TriggerParam` is not set, defaults to **1** |
| `BP_Mechanism_StoneDoor_Right_C` (1906) | Right leaf, `BoundMechanisms` = the left one |
| `BP_Mechanism_StoneDoor_Left_C` (1905) | Left leaf |

The doors have no other links to the boss: the doors in the `MainGridAlt_*` cells (`BP_Metal_Door_04_DistantLoad`, `BP_FantasyDungeon_Door_*`) are ordinary doors with handles.

Chain:
1. **Spawn** (`BP_ActorSpawnPoint.SpawnActor`): `GameMode.SpawnMonster` → `K2_GetPawn` → cast to `BP_Tgf_Monster` → `DeathTriggerMechanism = ControlledMechanism`, `DeathTriggerParam = TriggerParam`, `DeathTriggerMechanisms` are copied into the monster. If the controller has no pawn at that moment, the copy is silently skipped.
2. **Death** (`BP_Tgf_Monster`): `TgfAttributeComponent.DeathEvent` → `HasAuthority` → `TriggerMechanismOrTrap` → `DeathTriggerMechanism.OnTriggered(1)` (interface `BPI_Triggerable`), then the same for `DeathTriggerTrap` and the `DeathTriggerMechanisms` array.
3. **Door** (`BP_Mechanism.OnTriggered`): forwards the call to `BoundMechanisms` (the left leaf); parameter < 10 → `ChangeIsOn(1)`: `FlushNetDormancy`, `bIsOn = true` (replicated, push model) → `StartMovement` → `MoveBase.SetIsOn(true)`.
4. **Movement** (`UMechanismMoveNode`, native): `SetIsOn` (`0x142f5c680`) works only with `ROLE_Authority`: it sets the replicated `bIsOn` and `StopTime` and enables the component tick. The leaves move in `TickComponent`. On the client `OnRep_bIsOn` (`0x142f53c00`) also only enables the tick. The `OnRep_bIsOn` of `BP_Mechanism` itself only plays a sound on the client.

## 2. How our server differs from the official one
Our battle server is a listen server with a parked host (the host pawn is +300 m above the spawn point, `IsDedicatedServer` = false). Native "player 0" logic sees the host, not the players:
- `UPlayerDistanceCheckerComponent` on spawn rooms does not fire without a pawn nearby — we replaced it with our own check (`Spect.ProxTick`, flag `prox`).
- `UActorMapComponent` zeroed the heroes' gravity — HeroFix disables it on heroes (`DisableActorMap`, `ActorMapComponent:Deactivate()`).
- `UActorMapSubsystem` keeps the hero list `HeroList` and, by distance to them, enables or disables monster AI and tick (`MonsterActorMapAIBound`, `MonsterActorMapTickBound`, pause reason `TgfPauseActorTickReasonActorMap`).

## 3. Suspect links (in order of probability)
1. **Leaf tick on the server.** Doors move only through the component tick. If the `MoveBase` tick does not run on the server (pause by distance to the host, headless optimizations), on the client the leaves will open (its tick), but the server will keep the collision closed — the player will bump into an "open" door. Ask the players: does the door not move at all, or does it open but not let them through.
2. **The door reference was not copied into the monster.** Step 1 of the chain is skipped if the controller has no pawn yet or the monster was spawned not by the point (our command, room restart). Then `DeathTriggerMechanism` is empty and the door gets no signal.
3. **`HeroList` of `UActorMapSubsystem`.** HeroFix disables `ActorMapComponent` on heroes. If heroes get into `HeroList` through this component, the list holds only the host; monsters far from the host follow different rules. This does not directly affect the Wendigo's death (the death event does not depend on the tick), but it affects everything the monster needs to do in its tick.
4. **Door replication to the client** (`bIsOn` does not arrive): unlikely — `FlushNetDormancy` and push model are there, the door is in the persistent level.

## 4. Probe (finds the place in one match)
A server log is needed; the mods are frozen, the probe is added by the orchestrator (or a separate temporary mod on the stand only).

Server, every 2 s on floor `L_Dungeon_Official_02` and right on the Wendigo's `MulticastDeathEvent` (the hook already exists in RuServer for the kill feed):
- on the live `BP_Tgf_Monster_Wendigo_C`: `DeathTriggerMechanism` (name or nil), `DeathTriggerParam`, `#DeathTriggerMechanisms`;
- on both leaves `BP_Mechanism_StoneDoor_*`: `bIsOn`, `MoveBase.bIsOn`, `MoveBase:IsComponentTickEnabled()`, world position of the root (moving or not), `NetDormancy`;
- `UActorMapSubsystem`: `GetCurrentHeroNum`, `MonsterActorMapTickBound`, `MonsterTickEnableNum/TotalNum`;
- distance from the door to the host and to the nearest player.

Client (RuClient or ClientProbe): the leaves' `MoveBase.bIsOn` and root position after the boss dies.

Reading the result:

| Server | Client | Where it breaks |
|---|---|---|
| Wendigo has `DeathTriggerMechanism` = nil | — | Step 1, spawn |
| Present, but after death `bIsOn` = false | — | Step 2, the death did not get through |
| `bIsOn` = true, leaves in place, tick disabled | Leaves moved | Tick on the server: collision closed |
| `bIsOn` = true, leaves moved | `bIsOn` = false | Replication |

Quick stand without a long match: `spawnrooms` (spawns the Wendigo by its point), the probe command `hp` for the hero, and fight; the boss cannot be killed by a command — there are no native cheats for that, a probe command is needed (e.g. `damage` to a monster via a native GE).

## 5. Fix options (after the probe)
- **Tick on the server:** in RuServer, after `bIsOn = true` on mechanisms, enable `MoveBase:SetComponentTickEnabled(true)` and remove the tick pause from the actor; or exclude mechanisms from pauses when the floor starts.
- **Reference not copied:** on the Wendigo's `MulticastDeathEvent`, call `OnTriggered(1)` on the leaves linked to its spawn point (`ControlledMechanism`, `DeathTriggerMechanisms`).
- **Replication:** `ForceNetUpdate` on the door after opening.

Link to falling through the floor at the start of the second floor: the common cause is that native systems orient on the host (`UActorMapComponent`), not on the players. Sublevel streaming has nothing to do with it: the door and the Wendigo spawn point are in the persistent level, not in World Partition cells.
