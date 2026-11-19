# Boss at the "secret room" on Dungeon Hard: missing / falls through

Analysis of 08.11 at the orchestrator's request (match on the stand from 14:36, `L_Dungeon_Official_01_Hard`, MapId 10103). Static
only: the level and World Partition cells from the paks (retoc, raw exports), the BP of the Reaper and its abilities
(kismet-analyzer), the JesterSwap mod, the `[DIAG]/[EXITS]` log. I didn't touch the game and didn't change code. Working files:
`<data>\research\leyline` (`boss\`, `cfg\`, `json\L01_Hard.json`).

## Conclusion
- **The first floor of Hard has no door that the boss's death opens.** None of the Reaper spawn points
  (`BP_TGF_H_Monster_Reaper_Special`) references a mechanism (`ControlledMechanism`, `DeathTriggerMechanisms`,
  `Mechanisms to Trigger` — empty both in the raw level and live, `[EXITS] mech=0`). The Reaper BP itself opens nothing
  on death. All the floor's mechanisms open differently (below). The only "boss → door" link in the Dungeon is the Wendigo and
  the stone door of the treasury on the second floor (`L_Dungeon_Official_02` and `_02_Hard`, docs/WENDIGO_DOOR.md).
- **"The icon is there, the boss isn't"** — the second Reaper point `(-105 -4526 -280)` hasn't spawned yet (`notSpawned=true`): its room
  spawns by player proximity. The map marker comes from the spawn point, not from a live monster.
- **"Falls through"** — most likely the Reaper's native attack "go underground and come out behind the target" (Attack3, teleport),
  which on the Jester model looks like falling through the floor. A real fall is possible, but not proven statically; probe below.
- **JesterSwap doesn't affect collision:** it only raises the mesh and `BaseTranslationOffset` (visual model offset),
  the capsule and movement don't change.

## 1. Which mechanisms are on the floor and what opens them
Level `L_Dungeon_Official_01_Hard` (persistent) and cells `_Generated_/MainGrid_*`:

| Mechanism | How many | What opens it |
|---|---|---|
| `BP_EscapeMechanism_MetalDoor_01/02/04`, `_WoodDoor_01` | 17 | Its own `BP_EscapeDeviceSpawnPoint` (one per door): the door opens together with the appearance of the exit device in that room |
| `BP_ChainedDoor_02` (+ `_03` via `BoundMechanisms`) | 1 pair | Two `BP_Handle_02` levers |
| `BP_Mechanism` / `BP_Metal_Door_04` / `BP_ChainedDoor_01` / `BP_D19_Base_01` in cells | several | `BP_Handle_01/02` levers in the same cell |
| `BP_Metal_Door_03`, `BP_Wall_Corner_02` ×2, `BP_PlayerOnlySphereTrigger` | — | Nothing references them: a regular door, decor, a trigger |

Possible confusion: a "secret room" with a metal door is an exit device room. Its door is opened by
the appearance of the device, not by the boss. For us these devices don't appear in rooms "by phase" (FIX_LIST, `[EXITS]
placed=0`), so such doors are always closed for us. To know which room is meant, we need coordinates or
a screenshot from the user.

Reaper points on the floor (both in `BP_ActorSpawnSet_C > BP_ActorSpawnRoom_C`, by proximity):

| Point | State at 14:40 |
|---|---|
| `(-4000 0 -250)` | placed; the live `Reaper_Special` stands at `(-2984 -615 -242)` — went after a player, height normal |
| `(-105 -4526 -280)` | not spawned (`notSpawned=true`, `placed=0`) |

## 2. Why the Reaper "falls through"
1. **Native Attack3 (most likely).** `GA_TGF_H_Reaper_Attack3` is a child of
   `GA_TGF_Teleport_Monster_Base`: montage `AM_H_Reaper_Attack3` (Ready → Loop → End: sinks into the floor, disappears,
   comes out), material states `BP_MaterialState_CommonFade` / `DissipationDeath` / `Stone`. `DoTeleport`:
   `K2_TeleportTo(target + (0, 0, TeleportZOffset) − target facing × DeltaDistance)` — behind the target. The Hard version has
   values 1.4 / 150 / 50 (delay, distance, lift). On the client JesterSwap changes the mesh to `SK_Jester` and does
   `EmptyOverrideMaterials()`: fading via the Reaper's materials may not work on the Jester, and going underground looks like
   falling through the floor. Check: after 1–2 s the Reaper appears behind the player — then it's the attack.
2. **Teleport to a bad point.** The exit point is 150 cm behind the target. If there is a wall or an edge behind, `K2_TeleportTo` either
   won't move it (stays in place), or puts it at the nearest free spot — occasionally below the floor. If the target turns out to be
   the host pawn (hangs above the map), the Reaper appears in the air and falls. The host is hidden and has no collision; whether it becomes a threat
   target is not checked.
3. **The floor is not loaded on the server (World Partition cells).** The floor lies in `MainGrid` cells. If the server doesn't hold the cell at the
   Reaper's spot, the Reaper falls on the server, while the client has the floor. This is related to players falling through at the start of
   the second floor (there heroes fell before the floor loaded, fixed by `HeroFix.DELAY_FLOOR2` for heroes only). Against this
   version: the first Reaper spawned by player proximity and at 14:40 stood at normal height.
4. **JesterSwap — no.** On the server it only changes `Mesh.RelativeLocation.Z` and `BaseTranslationOffset.Z` by +30 cm.
   The capsule, `CharacterMovement` and navmesh don't change; hits on the mesh shift up by 30 cm.

Difference from players falling through: players fell because of late floor loading after moving between floors. The Reaper appears
later, by proximity, and the floor near the player is already loaded; the Reaper's fall, if it is real, comes from the teleport (item 2) or
from the cell (item 3).

### How the exit room door opens (addendum 08.11, "the secret one didn't open" at 14:43)
- `BP_EscapeDeviceSpawnPoint.SpawnActor`: creates the device (`DataActor`, a child of `BP_EscapeDeviceBase`) and passes
  it its `StartMechanisms` and `ActivationMechanisms` (the room doors), activation delays, `MaxEscapeTimes`.
- `BP_EscapeDeviceBase` opens the doors itself: `ActivateStartMechanisms` → `OnTriggered` on `StartMechanisms` (authority
  only), on device activation — `OnTriggered` on `ActivationMechanisms`. Without the device the door won't open by any means.
- The point listens for the `Cheat.EscapeDevice.SpawnAll` message and does `SpawnImmediately` on it. The host command `exits`
  (14:43:29) calls `SpawnAllEscapeDevices` + `ActivateAllEscapeDevices` of the cheat component. `exits_spawned_and_activated`
  only means "no Lua error". At 10:09 the same pair of cheats gave no devices (FIX_LIST, "placed=0"), cheat bodies are cut
  out in shipping in places. There is no `[EXITS]` check in the log after 14:43. Most likely the command placed nothing, and
  the exits state in this match stayed native: no devices, nobody opened the exit room doors. If
  devices had appeared via the points, the doors would have opened with them — the fact that the "secret" one didn't open is consistent with this.

### The treasury from the user's description (addendum 08.11, 15:xx)
I searched by the description "a pile of gold on a raised platform, Valuable Chest, a grate, a Reaper nearby" in `L_Dungeon_Official_01_Hard`
(persistent level + `MainGrid` cells; `_Classic` has the same cells). Coordinates are world ones unless "rel." is said
(points inside rooms have coordinates relative to the room).

"Valuable Chest" is `BP_TreasureChest_Gold_01_Special` (a regular `BP_InteractableChest` with a gold glow, without its own
lock logic). There are exactly two on the floor, both placed in the level, not spawned:

| Where | Chest | Guards | How to get in |
|---|---|---|---|
| **Multi-level treasury**, room cell `(4000, −4000)` (template `…7fba174710cdebdc`, room `BP_ActorSpawnRoom` 2124) | `(4002, −2903, 491)` — on a raised platform | 15 room points: tentacles ×3, skeletons (mage, archers ×2, melee ×5), zombies ×3, a beholder; all have `DataMonsterCombatZone` = this chest (they guard it). No Reaper among them | Mechanism `BP_D19_Base_01` (cell `MainGrid_L1_X1_Y-1`, archangel/griffin/demon statues — moves and opens the passage) and two `BP_Handle_02` levers: `(3665, −1854, 220)` and `(4497, −1791, −86)` |
| **Central room** `(0, 0)` (template `…e7d4837b7453326d`) | ≈ `(5, 2, 398)` (rel.) — on a raised platform in the center | 29 points: insects ×16, skeleton mages ×8, skeletons ×9, wooden chests ×4 | Four `BP_Metal_Door_04` grates in the corners, each with a pair of `BP_Handle_02` levers (≈ `(±830…1100, ±840…1230)`) |

Reapers: two `Reaper_Special` points (rooms `(−4000, 0)` and `(0, −4000)`) and 8 `DT_Monster_Elite_Hard` points (a random
elite monster, including the Reaper: `(−455, 7296)`, `(−8490, 8896)`, `(−9634, 1425)`, `(8300, −9077)`, `(7811, −1513)`,
`(8002, 8170)`, `(−7342, −6919)`, `(17, −6836)`). **None of them references a chest, lever or mechanism.** The closest
to the multi-level treasury is the elite point `(7811, −1513, 250)`.

**What opens it:** only levers. `BP_SwitchBase.TurnOn` → `ControlledMechanism.OnTriggered` (authority) →
`BP_Mechanism` moves `MoveBase`. There are no "monster alive" checks either in the lever (`BP_SwitchBase`/`BP_Switch`/`BP_Handle_02`),
or in the mechanism, or in the spawn room (`BP_ActorSpawnRoom`: only spawn by phase and by proximity), or in the chest
(`bLocked` on the chest is only touched by the `BP_Lock` code lock, which is not on this floor). "Closed while the Reaper is alive" in
the original probably meant "the levers are in the room with the Reaper, you can't get to them".

**Why it might not have opened for us (not checked):**
1. The levers and the mechanism lie in a World Partition cell (`MainGrid_L1_X1_Y-1`), not in the persistent level. The lever
   fires only on the server (`PerformInteraction on Server`). If the listen server doesn't have this cell loaded
   (the server loads cells by its own streaming sources, the host is parked at the spawn point), the client sees the lever, while
   the server doesn't have it — pressing does nothing. The Wendigo door worked on the stand, but it is in the persistent level.
2. A mechanism with two levers may require both (`BP_Mechanism` has a state by the `OnTriggered` parameter); whether both
   were pulled is unknown.
3. The user may have looked for "the Reaper's death" as the condition — there is no such condition in the data.

**Check on the stand (host command, the orchestrator edits):** `levers` — `FindAllOf("BP_Handle_02_C")`,
`BP_D19_Base_01_C`, `BP_Metal_Door_04_C` on the server: coordinates, `bIsOn`, `MoveBase.bIsOn`, whether `MoveBase` ticks.
If the server doesn't have the levers `(3665, −1854)` / `(4497, −1791)` — the cause is cell loading by the server (then look at
`wp.Runtime.EnableServerStreaming` / the host's streaming sources). If it has them — call `TurnOn` on both from the server and
see whether the statue moved. For the center — the same for the four `BP_Metal_Door_04`.

## 3. What to propose
- **First a probe** (the orchestrator edits the mods): every 0.5 s for all `BP_TGF_*Reaper*_C` log position, `MovementMode`,
  `IsFalling`, active ability (`Attack3` or not) and distance to the nearest player; plus `K2_TeleportTo` →
  result. A Z drop below −2000 when not Attack3 — a real fall-through.
- **Host command `boss`:** list of live Reapers (coordinates, Z, movement mode), `boss tp <i>` — put it on its spawn
  point, `boss spawn` — spawn the point `(-105 -4526 -280)` (like `spawnrooms`). No need to kill by command.
- **If the fall-through is due to cells:** keep gravity off for monsters for the first 2–3 s after spawn (like HeroFix for
  heroes) or call `SetMovementMode(Walking)` after checking the floor with a trace.
- **JesterSwap:** no need to turn it off because of the fall. To check version 1 you can temporarily set `ru-jester.txt = off` on one client
  and watch Attack3 on the native model. The decision is the user's.
- **About the "secret room":** if it's the metal door of an exit room — it will open only together with the exit
  device, i.e. after the in-room "by phase" spawn is fixed (FIX_LIST), the boss has nothing to do with it.

## Host commands where / levers (08.11, RuServer)
- `where` — for each player: coordinates, camera angle and cardinal direction, map cell (4000 cm), nearest
  `BP_ActorSpawnRoom`, distance and direction to known floor points (center, treasury chest, both levers;
  on the second floor — the Wendigo door). `[WHERE]` lines.
- `where north` — calibration: the player looks straight up on the minimap, their camera angle is written to `ru-north.txt` next
  to the exe; after that directions are N/NE/E/…. Without calibration the direction is "?".
- `levers [radius|all]` — levers (`BP_SwitchBase/Handle_01/Handle_02`) and mechanisms (statue `BP_D19_Base_01`, grates,
  chain doors, `BP_EscapeMechanism`, stone shutters) on the server: coordinates, `bIsOn`, mechanism, `MoveBase`; by default
  within 3000 cm of the player. `[LEVERS]` lines. `levers on <N>` — press the N-th from the last list from the server (lever — `TurnOn`,
  on error `ControlledMechanism:OnTriggered(1)`; mechanism — `OnTriggered(1)`).
- Not checked in the game (08.11): goes onto the stand together with the Wendigo insurance 8a8177c.
