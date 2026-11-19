# Native zone portal points (ADeathZone) and the experience potion — Ghidra 06.11

Exe image from the dump, Ghidra headless `DecompTree.java` (method: `docs/ghidra_gameover/README.md`). Addresses — base 0x140000000.

## Addresses

| Address | What |
|---|---|
| 0x142d8b650 → 0x142f44a90 | `DeathZone.Generate(out PrecomputedDiscs)` — precomputes discs, portals and scrolls |
| 0x142d8e720 → 0x142f5df70 | `DeathZone.SpawnPortalAndScrolls(PrecomputedDisc, StateTime)` — places the entries whose time has come (BP calls it with `Precomputed Discs[Current Disc Index]`) |
| 0x142d8ddb0 → 0x142f54560 | `PickRandomPlacementsWithinRange` — see below |
| 0x142d8aa50 | `ShouldPortalSpawn(class)` — check before spawning (BP: always true) |
| 0x142f4f020 | portal entry from the disc setting and the point. **Not analyzed:** class selection by options, possible involvement of ShouldSpawnRedPortal |
| 0x142ff2390 | `LyraPlayerState.PlayerAddExp(int)`: `PS+0xA20 += value` |

## Offsets

- DeathZone: `+0x298` Discs (element 0x78), `+0x2A8` portal points `TArray<FVector>`, `+0x2B8` scroll points.
- Disc: `+0x0` Radius, `+0x8` OuterRadius, `+0xC` InnerRadius, `+0x48` portal settings (element 0x18), `+0x58` scroll settings.
- PrecomputedDisc (0x48): `+0x0` center, `+0x18` Radius, `+0x1C` Outer, `+0x20` Inner, `+0x28` portals (element 0x80), `+0x38` scrolls.
- Portal entry (0x80): `+0x0` time, `+0x4` chance, `+0x10` FTransform, `+0x70` class.

## How the place is chosen

`PickRandomPlacementsWithinRange(Num, OuterCenter, OuterRadius, InnerCenter, InnerRadius, Placements, Occupied) -> TArray<FTransform>`.
The mod called it without the last two arrays, so the call failed, and `pcall` hid the error — the fallback took spawn points.

A point qualifies if (distances in 2D):
`dist(point, OuterCenter) < OuterRadius` and `dist(point, InnerCenter) >= InnerRadius` and the point is not occupied.
Random shuffle, `Num` items are picked, the occupied flag is set; rotation is random in yaw only, scale 1.

Generate for disc `i` (`next = i + 1`, for the last one `next = i`):
`OuterCenter, OuterRadius = Precomputed[i].center, Precomputed[i].Radius`;
`InnerCenter, InnerRadius = Precomputed[next].center, Precomputed[next].InnerRadius`.
Number of portals per disc = number of disc settings. Occupied flags are local to the whole Generate, shared by all discs.

## What this gives the second floor

The floor's discs have no portal settings, so the native Generate computes nothing, but the points exist on the map (162).
Plan: choose the place in Spect.FloorPlace with the same selection over `PotentialPortalPlacements`, without calling the native function.
On the floor `InnerRadius` is 0 for all discs, so take the next disc's `Radius` as the inner radius — this gives a ring, as on the first floor.
The property name and disc centers on the live object have not been checked yet.

## Red portals on the first floor — unresolved

`ShouldSpawnRedPortal` is not called anywhere in BP and was not found as a UFunction pair in the image.
The Classic disc settings have no red portals, so the source remains `BP_EscapeDevice_Red_01` in rooms
(`BP_EscapeDeviceSpawnPoint`, see PVE1 in FIX_LIST). Next step: analyze this BP and 0x142f4f020.

## Changing the device type

`BP_ExtractDevice_Base.ConvertPortalType`: if `ConvertTargetType` is set — disable and hide the device, after 0.2 s place the target
at the same spot, then destroy itself. The entry point is also `EnableConvert` (see `GetConvertEnabled`). The K21 red roll uses this mechanism.
Variants in the paks: `BP_ExtractDevice`, `_BlueTwo`, `_BlueThree`, `_Red`, `_RedOne`, `_Shadow`, `_ShadowOne`, `_ShadowTwo`.
