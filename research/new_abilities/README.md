# New abilities: note 05.11 (idea, not started)

## How it works
The Lyra ability system (GAS). Example: the rogue (`TGF/Character/Hero/Rogue/GameAbility`) — 9 GA, 18 GE, 7 AbilitySet, 6 GCNL.
- **GA_** — logic, a blueprint on the C++ class `UTgfGameplayAbility` (← `ULyraGameplayAbility`, `LyraGame`).
- **GE_** — effects: damage, buffs, cooldowns, duration. They reference attributes and calculations from the game's C++.
- **AbilitySet_** — what is granted to a class, talent or item (`AID_` on items use the same scheme).
- **GCNL_** — visual and sound cues.
- Also: animation montages, projectiles `B_TGF_Track_Projectile_*`, `DA_TGF_AbilityRule_*` (AI rules), Niagara,
  icons and descriptions in tables (`battle_ability_set_description` etc.).
- Runs both on the server and on clients (prediction). The pak goes everywhere.

## Levels
1. **Recombination and numbers** (possible now): a new AbilitySet from existing GAs, other values in GEs.
   Asset editing (`bp_patches`: UAssetGUI JSON) and tables.
2. **New GE / GCNL**: mostly data, but with references to the game's attributes and classes; needs the mod kit, a light version is enough.
3. **New GAs with their own logic** (dash, zone, custom projectile): the blueprint calls the game's C++ functions; needs the mod kit.
   Animations from the game, Niagara in UE, icons and texts in tables.
- Without the mod kit: simple server logic in a UE4SS script ("apply an existing GE on an event"). A workaround: no animations and no client-side prediction.

Mod kit: `research\modkit\README.md`.

## Analysis 06.11: the wizard's charged Q with the Reaper's effect (from assets only, not verified in the game)
Materials: `<data>\new_class\research\ability` (assets and `cfg\`).

**Charging.** `GA_TGF_Wizard_Q_New` ← `GA_TGF_MultiLevel_Charge_Skill_Base` ← `GA_TGF_Charge_Skill_Base`. All charge-level logic is in the BP bases.
Charge release: `DoReleaseCharge` → `TriggerFireProjectile`. Montages `AM_Classes_Wizard_SkillQ_*` are in `Character/Shared/Animation` (1st and 3rd). Cost `GE_TGF_MagicCost_Wizard_Q*`, cooldown `GE_TGF_Cooldown_Wizard_Q`.

**Charge effect.** `GCNL_TGF_MultiLevelCharge_WizardQ` is a logic-less blueprint on the C++ class `TgfGameplayCueNotify_Looping`, containing the Niagara systems `FX_Classes_Priest_CastingQ`, `Q1`, `Q2`. Torch variants: `TGF/VFX/Niagaras/FX_Torch`, `_01…_04`. Which one is on the DLC torch is not determined.

**Reaper.** The `M_Reaper` assets have nothing with Poison or Venom. The closest: `GA_TGF_H_Reaper_Attack4` (no logic, parent `GA_TGF_Explode_Monster_Base`): an explosion around itself, damage `GE_TGF_H_Reaper_Attack4` and the debuff `GE_TGF_Reaper_Buff_Fear` (fear, cue and camera). Real poison exists in `GE_TGF_M_Zombie_Venom` (zombie) and `GE_TGF_PoisonDamage` (dagger).

**Without the mod kit.** Clone the asset under a new path: `GA_TGF_Wizard_Q_New` + bytecode editing (`bp_patches\tools`), in `DoReleaseCharge` apply the effect instead of the projectile. Plus a clone of the `AbilitySet`, a `role_skill` row and `UeSource_AbilitySet`.

What may get in the way, not verified:
- The fighter career may lack the magic attribute, then `GE_TGF_MagicCost_*` will not let the charge start.
- A new `GCNL` with a new tag may not be found: cues are looked up via the asset registry, and the mod pak does not get into it. Fallback: replace the Niagara at its game path or use a cue with an existing tag.
- The color of an existing Niagara cannot be changed: there are no sources. Either an existing effect or our own in UE 5.1 (CPU simulation, material from the game).

**First cheap test:** give the trial class `role_skill` 109001 (wizard's Q) via `class.json`. It will show whether charging, montages and magic work on the fighter's pawn.

### Addendum 06.11: decisions and findings
User's decision: after charging, an explosion around oneself, damage and fear (`Attack4`, debuff icon: poison), remove the magic cost.

**Color.** The torch skin `Torch_Skin_Moonlight` → `B_Torch_Skin_Moonlight` → `FX_Torch_01`. This may be the grey-white effect; needs visual confirmation. The skin replaces Niagara materials via the component's `User.MI_FX_*` parameters; the same way can change materials without rebuilding Niagara. `FX_Torch*` has `Emitter.ScaleColor.Linear Color Curve` curves: editing the color in the asset bytes may be possible, not tried.

**New UE versions.** UE 5.8 added an experimental MCP plugin. It does not help with the game: cooked data is tied to the engine version, 5.8 output will not load in the game on 5.1. No need to download it for this.

**Explosion.** `GA_TGF_Explode_Monster_Base`: `PickBestCastSkillMontage` → montage → on `FireNotify` a call to `DoEffect` (`SphereOverlapActors` from `FireSocketName`, `Radius`, `Height`, `ApplyMeleeWeaponEffect`, `ApplyMoveSpeedDebuff`), `K2_CommitAbility` at the start. The montage `AM_H_Reaper_Attack4` is on the monster skeleton; the hero needs its own montage with the same notify.

Plan: a clone of `GA_TGF_H_Reaper_Attack4` with a hero montage, launched from the wizard Q clone instead of the projectile (`TriggerFireProjectile`, `TryActivateAbilityByClass`). First the test: the wizard's Q on the class via `class.json` (`skills`).

**Confirmed by the user 06.11:** the charge effect is like the Moonlight skin (`FX_Torch_01`).

### Wizard Q clone: findings 06.11
- UAssetGUI reads all 29 exports of `GA_TGF_Wizard_Q_New` as `RawExport`. Clone: edit names in the `NameMap` and imports (`ug.py`), export bytes as is, bytecode edited as in `bp_patches\tools\patch_relic.py`.
- The fire in the wizard's Q is four cues: `...SkillQ` (`GE_TGF_MultiLevelChargeEffect_WizardQ`), `...SkillQ.CastFire`, `...EnterStage2`, `...LoopStage2`. Their GEs are called from the clone as constants and can be replaced or removed.
- A new `GCNL` with a new tag will not do: the tag must be registered, and cues are looked up via the game's asset registry, which the mod pak does not get into. The working path: find an unused `GCNL` with a tag in the game, replace that asset at its own path with a clone using `FX_Torch_01`, and set its tag in the GE clone. That way the wizard's effect is not affected.
- Cost: `GE_TGF_MagicCost_Wizard_Q`, `Q1`, `Q2` (parent `GE_TGF_MagicCost_Base`, attribute `CurMagic`).
- Hooking up: a clone of `AbilitySet_MithrilFighter_Q` (tag `InputTag.Weapon.Fire.FireQ`), a `UeSource_AbilitySet` row with the path, a `role_skill` row.
- The clone has torch and wizard talent branches (`ID_Torch`, `GE_TGF_FastCharge`, element projectiles): check that they do not fire on another class.

Order: first test the wizard's Q on the fighter's pawn (built, `class.json` → `skills`). Start the clone after that: the test will show whether the montages fit and whether the magic needs a bypass.
