# Rogue stealth: why the enemy was not visible and how it was fixed

12.11.2026. Complaint: an invisible rogue is "literally not visible", even point-blank. Analysis — this session and the LOCAL session
(`LOCAL_GAME_MODE_OPENID` filling, static BP analysis), verification on the stand with two clients.

## How the game renders stealth

- Skill E (`GA_TGF_Rogue_E`) applies `GE_TGF_Rogue_EnterStealth`: tags `TGF.Ability.BuffState.Stealth`
  (+ `.FullCloak`), `HideAllFx`.
- The tag enables the material state `BP_MaterialState_Stealth_C` (`UTgfMaterialState`) on the hero in
  `TgfCharacterMaterialManagerComponent`. All of the hero's meshes (body, armor `AvatarSubMesh_*`, weapon, torch) get
  a MID with parent `MI_Character_CharacterInvisibility_Albedo_Distance` — 13 meshes on a typical rogue.
- The manager's `CloakType` is set by native code: 0 none, 1 self/ally, 2 enemy, 3 enemy-highlight.
- The BP's `OnTickState` with CloakType 2/3: `LyraTeamSubsystem.GetOwnPlayer()` → own pawn → every frame writes into the material
  `Distance` (to own pawn, cm) and `Velocity`.

## What the stand showed

`ClientProbe` log (lines `STL`, `stealth = 1`), three runs:

| What | Value |
|---|---|
| State on the enemy | `active=true`, `tick=true`, `cloak=2`, Stealth/FullCloak tags present |
| Own player | `GetOwnPlayer` is local, the pawn exists, teams differ |
| `Distance` in the MID | matches the distance computed by the mod (1844 → 80 → 1977 cm) |
| `StealthVisibleDistance` | **always 0** |
| Other parameters | `FarDistanceVisibility` 0.4, `CloseDistanceVisible` 0.2, `Distance_ToClose_InVisibilityBegin` 500, `…Ended` 0, `VelocityMax` 0.05, `LightOpacity` 8 |
| `CharacterEnemyCloakingMatDynamic`, `CharacterFadeOutMatDynamic` | null |

Conclusion: replication and the BP logic work (the listen server is not to blame). The `StealthVisibleDistance` parameter is set by
**native code** (the name is in the exe as a single ASCII string among the hero code; it is not in assets, ini, ExcelConfig or the protocol),
and on our side it stays 0. Where the native code takes the number from and under which condition is unknown: the exe code on disk is packed,
the image from the 01.11 dump was deleted, we do not read live memory.

## Experiments (ClientProbe on the observing client, only writing parameters into the MID)

| Experiment | What the user saw |
|---|---|
| `StealthVisibleDistance` = 500 (first 10 MIDs — the body was not included) | only the sheathed swords are visible |
| 500 on all MIDs | visible up close, but does not disappear when moving away |
| 500 + `FarDistanceVisibility` = 0 | visible (semi-transparent); "not beyond 5 m" — in fact, see the formula, the threshold has nothing to do with it |
| 1000 + `CloseDistanceVisible` 1 + `Distance_ToClose_*` 1000→0 | constant semi-transparency, no transition |
| threshold 1 + `FarDistanceVisibility` = 1 − d/1000 every 100 ms | first "works", then "always visible the same", "did not work from the start" |

## Material formula (LOCAL, 12.11: shader from the paks)

Preshaders (`FUniformExpressionSet` in the frozen data of `MI_Character_CharacterInvisibility_Albedo_Distance.uexp`)
and the base pass pixel shader (PCD3D_SM5, `ShaderArchive-…Chunk1004`, d3dcompiler disasm), format — the UE 5.1
sources. Files — the LOCAL session's scratchpad (`stealth/sm5/ps32.asm`, `presh.py`).

- **`StealthVisibleDistance` is not a distance but a strength S = saturate(SVD), 0..1.** Any value ≥ 1 is 1.
- **`Distance` and `Velocity` are not in the shader** — stripped at build time; the BP writes them for nothing. `Distance_ToClose_*` are not there either.
- Opacity: `Base = R · Fade · lerp(FarDistanceVisibility, 1, S)`, `Opacity = sat(lerp(Base, noise·max(Fade,0.4),
  Visibility))`; R — Fresnel ring (silhouette edges), Fade — DepthFade (`FadeBias/Divide/Add`); `CloseDistanceVisible`
  only enters as `clamp(SVD, 0, Close)` in the ring base. S also amplifies refraction (`IOR + S/20`).
- This explains all the experiments: with S=0 visibility = Far·R·Fade (almost zero) — "never visible"; with S=1 Far stops mattering —
  "constant semi-transparency", a ramp on Far is useless.
- `Visibility` 0..1 — a blend towards the almost opaque noise look (how your own rogue sees itself); the BP resets it to 0 when
  meshes change.

## Fix: RuClient StealthReveal

A module at the end of `game/ue4ss/Mods/DungeonborneRuClient/Scripts/main.lua` (5b1aaed → by the formula 3a6f116), client only
(not on `-dbserver`). A temporary solution — WORKAROUNDS K25. **Stand 12.11 18:45, two clients: "it works"** — the rogue
fades in smoothly up close (log: d=224 cm → strength 0.776, d=155 → 0.845, d=1560 → 0; written into all MIDs, reading back
matches).

- Every `scan_ms`: `FindAllOf("BP_MaterialState_Stealth_C")`, the active state (the manager's `CurrentMaterialState`)
  with CloakType 2/3 → the rogue's pawn → the stealth MIDs on the meshes (walk `ACharacter.Mesh` → `AttachChildren`, materials
  `GetMaterial(i)` + `OverlayMaterial`, filtered by `Parent`). They get `FarDistanceVisibility` = 0.
- Every `update_ms`: distance from own pawn (`LyraTeamSubsystem.GetOwnPlayer().Pawn`) to the rogue → `StealthVisibleDistance`
  = strength from the curve; with `solid_distance` > 0 also `Visibility`.
- Not possible in UE4SS 3.0.1: an array return value of a function (`K2_GetComponentsByClass`), TMap (`MaterialDynamicMap`),
  `GetAbilitySystemComponent()` (not a UFUNCTION; tags — via `AbilitySystemBlueprintLibrary`).

### Settings: from the server, the season's `[client.DungeonborneRuClient.stealth]` section

Set in `content/seasons/base.toml` (a season can replace any key with its own section). The backend serves them in the manifest
(`client_settings`), the launcher writes `Mods/DungeonborneRuClient/server_settings.ini` before launch (`docs/MOD_SETTINGS.md`).
Distances in cm, strength 0..1 (1 — semi-transparent silhouette). `update_ms`/`scan_ms` — in the code (100/1000), if needed — `settings.ini`.

| Key | Default | What |
|---|---|---|
| `enabled` | 1 | 0 — as without the mod (the enemy rogue is not visible) |
| `distance` | 1500 | beyond it — not visible (strength 0); 17.11: 15 m (was 10) |
| `full_distance` | 0 | closer than this — strength `opacity_near` |
| `opacity_near` | 1 | strength point-blank |
| `opacity_far` | 0 | strength at the `distance` boundary |
| `curve` | 1 | 1 — linear; 2 — stays transparent longer, appears sharply up close; 0.5 — the opposite |
| `solid_distance` | 0 | closer than this — also denser (`Visibility`, almost opaque); 0 — off |
| `solid_max` | 1 | density point-blank (0..1) |

Strength: `d ≥ distance` → 0; `d ≤ full_distance` → `opacity_near`; otherwise
`opacity_far + (opacity_near − opacity_far) · ((distance − d) / (distance − full_distance))^curve`.

In the client log (`Win64\crash_ue4ss.log`): `[STEALTH] settings: …` on start, `[STEALTH] enemy rogue … in stealth: materials N [meshes]`
on the first encounter, `[STEALTH] d=… strength=… solid=… written …, …` every 2 s (up to 300 lines).

## Not done / not verified

- Not deployed to players: the `ru.client-hooks` package, the backend (`client_settings`) and the launcher writing `server_settings.ini` — only on the user's "yes". With the old launcher the code defaults apply (the same 10 m).
- `solid_distance` (Visibility) was not tried in game.
- Own rogue and allies (CloakType 1) are not touched. The SM6 shader and the second permutation (38) were not compared line by line.
- What strength/range the original had is unknown; 10 m was chosen by the user.
- The experiments remain in ClientProbe (`stl_vis`, `stl_far`, `stl_set`, `stl_ramp`, off by default).
- On game exit the launcher removes the stand mods from install E: (12.11 18:31) — after playing through the launcher, run
  `dbtools\sync_mods_E.ps1` and reinstall RuClient/ClientProbe from the repository.
