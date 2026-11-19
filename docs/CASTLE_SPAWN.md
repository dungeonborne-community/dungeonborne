# Player spawns in the Castle: why different teams appear next to each other

Analysis of 09.11 for the complaint "players appear in the same room a meter apart" (Castle). Static only:
- Ghidra on the exe image;
- coordinates of the start sets from the Castle levels (retoc → own header parsing, `research\castle_spawn\tools`);
- Robot logs for 04.11 (`G:\logs`);
- mod and backend code.

I did not run the game and did not change code. Working files: `<data>\research\castle_spawn`.

## Conclusion
**Updated after the prod logs (section "Addendum 09.11"): the main cause is on the first floor.** All Robot servers
share one options file `ru-map-options.txt`. The Castle server picks up someone else's options, the solo start sets are disabled, and
8 sets remain. In solo rooms of 9–12 players the extra players are placed into other teams' sets. The second floor is a separate, smaller
problem.

- **The Castle first floor is laid out fine.** On Casual, Classic and Hard the combat start sets are more than 15 m apart
  from each other. There are enough sets for the matchmaking limits, as long as the map options for the group size are set (and the mod sets them).
- **The main suspect is the Castle second floor.** `L_GothicCastle_WP_Main_SecondFloor` (and `_Hard`) has only
  **3 start sets (9 points), all in one room**. Coordinates: (120775, −1557), (120895, 1542), (121382, −129);
  15–31 m between sets. In our setup the floor transition is a `servertravel` of the whole server (RuServer `Spect.FloorTravel`):
  everyone who went into the red portals arrives there at once.
  - The first three teams get a set each — already "in the same room".
  - The fourth and later: there is no free set. The native code places the player on a random free point of **any**
    set, i.e. a meter away from another team.

  In the original the red portal opens once per three passes (`Guide_Description_3_2`), and the second floor never had more
  than three teams. We can have several red devices (roll K21), and everyone who went into them moves
  at once.
- Secondary causes of the same kind on the first floor: the team was not set before the start was chosen, or the map options were not set
  in time (see below). The 04.11 logs do not show this. I have no fresh prod logs with a complaint.

## Addendum 09.11: prod logs (backend journal since 08.11, 38 Castle matches)
Data from the orchestrator: `research\castle_spawn\castle_rooms.txt`, `castle_members.txt`, `early_deaths.txt`,
`prod_castle_journal.txt`. I did not read the battle server logs on Robot: prod access is closed by the rules.

- **Early deaths are on the first floor**, before any transition: player kills at +52…+118 s after the start, deaths without
  a player killer at +34…+41 s (Classic).
- **All 5 early kills are in solo rooms of 9–10 players** (`l40003016_151528001`, `_151528019`, `_167252001` ×2,
  `_167252019`). Rooms of up to 8 players have no early kills. Castle solo rooms with 9–12 players — 14 of 38.
- **8 is exactly the number of Castle sets without the solo sets.** With option `1020101` / `1020201` there are 14 sets, without it 8.
  The ninth and later players are placed on a free point of another team's set (step 4 below) — a meter away from an opponent.

### Why there are no solo sets: one shared options file for all Robot servers
1. Solo sets (`LevelArgs`) are filtered **once**, on the first start choice in the world (flag `+0x17c` in `0x142fdaaa0`).
   Whatever options the GameState has at that moment determine the sets for the whole match.
2. The options are set by RuClient (`ApplyStandardMapRulesToGameState`): it takes its map's options from the file `ru-map-options.txt`.
   If they are not in the file — it sets the **trio variant** (`1020103`), with which the solo sets are filtered out.
3. The file is written by RuServer (`ParseHost`, on every `/ds/slot` response, i.e. every ~2 s) **by a relative path**. All
   servers on Robot start from one folder (`start_headless.ps1`: `-WorkingDirectory …\Binaries\Win64`, ports 7777,
   7787 … 7817). The other RuServer files are already split by port (`Inst` = `-<port>`), but this one is not.
4. Result: the Castle server often reads a file that the Crypt server or another room has just overwritten. Its own
   options are not there → trio variant → 8 sets instead of 14 → players 9–12 appear next to other teams.
   The Crypt suffers less: without solo sets it has 16, and rooms rarely exceed 16 players (for comparison: 2 early kills
   in 28 matches).

The second floor reads the same file (`map=` for the MapId and floor options), so there too it can take a neighbouring server's
map. Not checked in the prod logs.

### Also noticed
The host task sometimes has a **duplicate member** (`l40003018_167252051`: `…265771` twice). Hence the "group of 2" in
solo rooms. There are no real duos there, it does not affect the points, but the member list should be cleaned up.

### What to fix (step by step, the user decides before any changes)
1. **RuServer + RuClient:** each server gets its own options file (`ru-map-options-<port>.txt`, like the other
   RuServer files). RuClient takes the port from the command line (`-port=`). This is the main fix.
2. **RuClient, safety net:** in the `ChoosePlayerStart` pre-hook (it already exists), set the map options again before the first start choice.
   Then the sets are filtered by the correct options even if the file was late.
3. **Check in a match:** a Castle solo room with 9+ players, lines `[MAP_RULES]` (options `1020101`) and `[STARTS]`
   (sets with `args=[1020101]` got teams).
4. Separately — the second floor (3 sets, see above) and duplicate members in the host task.

## How the game chooses a point (native)
`ALyraGameMode::ChoosePlayerStart_Implementation` (vtable `0x147764fa8`, slot `0x700` → `0x142f8aeb0`) →
`ULyraPlayerSpawningManagerComponent::ChoosePlayerStart` (`0x142fcc120`):

1. The player's team — `PlayerState.MyTeamID` (`+0x879`); no team (255) → −1.
2. A set already bound to this team (`ALyraPlayerStartSet.TeamId` `+0x2b0` == team) → a free point in it
   (`0x142fd4350`). This is how teammates spawn together.
3. Otherwise — free sets (`TeamId == −1`, not the waiting room and not death) by group (`0x142fe5950`). The Castle has no group actors,
   and each set is its own group. The choice is random, the set is bound to the team.
4. **No free sets** → any set with `TeamId == −1`. None of those either → **a random free point among all
   starts of the scene** (`0x142fd4350` over the common list). Hence "a meter away from others".

Special cases:
- **If the player has no team at the time of the choice (−1),** step 2 finds the first unbound set and does not bind it. All such
  players spawn in the same set.
- **Sets with `LevelArgs`** (solo sets) are enabled only if the GameState has **all** options from the list (`0x142f95df0`).
  For Hard sets `[1020201, 1020101]` this never holds.

## Castle sets (coordinates from the levels)
| Map | Combat, always | Solo (`LevelArgs`) | Waiting room | Nearest combat sets |
|---|---|---|---|---|
| `L_GothicCastle_WP_Main` | 8 | 6 (`1020101`) | 3 | > 15 m |
| `L_GothicCastle_WP_Main_Classic` | 8 | 6 (`1020201`) | 3 | > 15 m |
| `L_GothicCastle_WP_Main_Hard` | 8 | 5 (`1020201,1020101` — never enabled) | 3 | > 15 m |
| `L_GothicCastle_WP_Main_SecondFloor` / `_Hard` | **3, in one room** | — | ? | 15–31 m |

The matchmaking limits (`matchmaking::spawn_sets`) match the first floor: solo 14 (Hard 8), duo and trio 8. There is no limit
for the second floor. The dungeon second floor has 12 sets, per WORKAROUNDS K21.

## What to change (the user decides)
1. **Castle second floor — no more than three teams,** as in the original. For example, count the teams that entered the red portals
   of the first floor and after the third one stop letting them in (the portal goes out) or send them further as on extraction. A RuServer change
   (roll K21 and `FloorTravel`).
2. **Or add start sets on the second floor.** The server mod places additional
   `BP_LyraPlayerStartSet_C` with three `LyraPlayerStart` in other rooms of the floor before the transition. The coordinates have to be found in game.
   The native manager picks up newly appeared starts, but this is **not verified**.
3. **Diagnostics on the first floor** to rule out the secondary causes. For each entry, log `MyTeamID`,
   the chosen set and its coordinates (the `ChoosePlayerStart` hook already exists in RuClient) and the GameState options at the moment
   of the first choice.

## What to check on prod (logs of the match with the complaint)
- Whether there was a transition to the second floor (`[FLOOR] transition`) and how many teams arrived there.
- `[TEAMS] ChoosePlayerStart … team_id=` for each player: whether it is empty, whether it matches across different groups.
- `[MAP_RULES]` on the match map: whether `1020101` / `1020201` is set in a solo room.
