---
name: dungeonborne-rogue-stealth
description: "12.11: rogue stealth — StealthVisibleDistance is a strength 0..1, the native code does not set it; RuClient StealthReveal using the shader formula works (stand 18:45), not deployed"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Complaint 12.11: an invisible enemy rogue cannot be seen at any distance. Analysis, experiments, formula and settings — `docs/ROGUE_STEALTH.md`; temporary solution — WORKAROUNDS K25; for deploy — FIX_LIST "To deploy (since 12.11)".

- **Formula from the shader (LOCAL):** `StealthVisibleDistance` is a strength S 0..1 (saturate), not a distance; `Distance`/`Velocity` are dropped in the shader (the BP writes them for nothing). `Base = R·Fade·lerp(Far,1,S)`, `Opacity = sat(lerp(Base, noise·max(Fade,0.4), Visibility))`. The native code does not set S for us (0) → always invisible. The source of S in the native code was not found (the exe is packed). How to analyse the shader: retoc to-legacy .utoc without --no-shaders → ushaderbytecode, DXBC → D3DDisassemble; preshaders — FUniformExpressionSet in uexp.
- **Mistake along the way:** experiments with a "500/1000 cm threshold" and a ramp on FarDistanceVisibility gave false conclusions (S≥1 = 1, Far stops mattering); the first "it works" was wrong. Conclusion: without the material formula, eyeball experiments are misleading — get the shader earlier.
- **Fix (3a6f116):** RuClient StealthReveal — Far=0, S by distance every 100 ms (0 at 10 m → 1 up close), option `solid_distance` (Visibility). Settings `Mods/DungeonborneRuClient/stealth_reveal.txt`. Stand 12.11 18:45: "it works". Not deployed — the ru.client-hooks package only on "yes".
- UE4SS 3.0.1 lessons: ASC via `Default__AbilitySystemBlueprintLibrary:GetAbilitySystemComponent(pawn)`; a function's array result and TMap cannot be read → meshes via `pawn.Mesh` + `AttachChildren:ForEach(e:get())`.
- Stand: on game exit the launcher removes the stand's mods from E: → `dbtools\sync_mods_E.ps1` + RuClient/ClientProbe from the repository.

- **Prod 13.11 "does not work":** writes to the MID did happen. In the match the rogue was at 12–157 m (strength 0), closer than 10 m — one line (5.9 m, strength 0.41). The stand "worked" at 1.5–2 m (0.8). Risks: the Mids() walk is limited to 40 components, MIDs get recreated (count 4→16), diagnostics only for the first target every 2 s. There is one stealth parent in the paks. Question to the user — at what distance they did not see it; maybe these are balance settings (distance, curve, solid_distance).

**Why:** the rogue was invisible to enemies and unbeatable — the key playtest complaint.
**How to apply:** change the look only via stealth_reveal.txt; when deploying ru.client-hooks make sure stealth_reveal.txt is in the package. Stand: [[dungeonborne-local-dev]], ClientProbe experiments `stl_*` (off by default). Related to [[dungeonborne-bp-reverse]].
