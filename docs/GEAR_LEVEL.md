# Attack level and defence level of gear (gear level) in a match

Analysis from 09.11, static only: Ghidra on the exe image, the `BattleCalculation` asset (retoc → kismet-analyzer).
I did not change code or run the game. Working files: `<data>\research\gear_level` (`g1.c`–`g3.c`, `bc.json`).

## Formula
The native damage calculation (`0x142e30b80`, shared by heroes and monsters, PvE and PvP) multiplies damage by a defence multiplier:

```
Defence    = target's DefGearLevel by hit location (see below)
Pierce     = DefIgnoreBase × (1 + DefIgnorePercent)          of the attacker, 0x142e36c30
Attack     = attacker's AttGearLevel by hit type (see below)

x = max(0, Defence − Pierce) − Attack + 40                     40 = DefendConstructionDenominator
x = max(x, 1)
Multiplier = clamp(20 / x, 0.25, 1.0)                          20 = DefendConstructionNumerator,
                                                               0.25 / 1.0 = DefendMultiplierLower/UpperBound
```

Constants — from the asset `/Game/TGF/Ability/Data/BattleCalculation` (class `UTgfBattleCalculationDataAsset`,
fields `+0x30`…`+0x5c`). Other asset fields: elemental resistance bounds 0.25…1.0, `FriendlyDamageReduction` 0.6,
`SkillCooldownScaleRateMin` 0.6.

**At equal levels the multiplier is 0.5**, not 1. That is why in our monster damage formula (FIX_LIST, "damage = K × …")
this half is hidden in K.

## Examples
| Attack | Defence | x | Multiplier | Vs equal levels |
|---|---|---|---|---|
| 0 | 0 | 40 | 0.500 | — |
| 10 | 5 | 35 | **0.571** | +14% |
| 5 | 10 | 45 | 0.444 | −11% |
| 10 | 10 | 40 | 0.500 | 0 |
| 30 | 10 | 20 | 1.000 (upper bound) | ×2 |
| 0 | 40 | 80 | 0.250 (lower bound) | ×0.5 |

Near equality one unit of difference gives roughly ±2.5% damage. The ceiling is reached when attack is 20 above defence
(after pierce), the floor — when defence is 40 above attack.

## Attack level — `UTgfAttackAttributeSet::GetAttGearLevel(AttackType)` (`0x142e35df0`)
`ExtraAttGearLevel` (+0x1d8) plus the level by hit type:

| Hit type (byte) | Taken from |
|---|---|
| 0 and others | `RightAttGearLevel` (+0x1a8) — right hand |
| 1 | `LeftAttGearLevel` (+0x1b8) — left hand |
| 4 | max(right, left) |
| 11 | `AdditionalAttGearLevel` (+0x1c8) — additional weapon |

Affixes: 1120 `affix_attack_att_gear_level` — level of the weapon in hand; 1121 `…extra_att_gear_level` →
`ExtraAttGearLevel`, added to any hit. Pierce — 1113 `affix_attack_def_ignore_base` (`DefIgnoreBase`).

## Defence level — `UTgfDefendAttributeSet::GetDefGearLevel` (`0x142e36850`)
`ExtraDefGearLevel` (+0x200) plus one value by hit tags:

| Hit tags | Taken from |
|---|---|
| `TGF.Ability.Effect.Damage.Guarded` (hit into a block) | max(`RightDefGearLevel` with `…Guarded.RightGuard`, `LeftDefGearLevel` with `…Guarded.LeftGuard`), at least 0 |
| `TGF.Ability.HurtPart.Chest` | `ChestDefGearLevel` (+0x1d0) |
| `TGF.Ability.HurtPart.Trousers` | `TrousersDefGearLevel` (+0x1e0) |
| `TGF.Ability.HurtPart.Helmet` | `HelmetDefGearLevel` (+0x1f0) |
| no body part tag | torso (`ChestDefGearLevel`) |

That is, **defence comes from the single armor piece the hit landed on, not a sum**. Gloves, shoes, amulet and rings are not
part of the calculation: they have no GAS field, their 2004 is visible only in the tooltip. Through the MINOR affix 2005
`…extra_def_gear_level` (`ExtraDefGearLevel`) they apply to any hit.

## Not checked
- How the hit type (0/1/4/11) is set for specific skills and weapons.
- An observation from FIX_LIST: a skeleton hit a target with defence 2 and 4 for 168.6 and 164.2 (×1.027). The pure formula with attack 0
  would give ×1.048. Most likely part of the damage (the elemental addition D) does not go through the defence multiplier, or "level 2/4" is
  a value from the tooltip, not GAS. Needs a check in a match with a `ChestDefGearLevel` record and a torso hit.
