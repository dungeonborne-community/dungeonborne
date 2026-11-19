---
name: dungeonborne-skill-formulas
description: "15.11: where skill numbers live — GE (UAssetGUI tojson with the dungeonborne mapping), CareerSkillDamageCurveTable (own Extras decoder), tooltip formulas in Lua Script/Text"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Analysis of the priest's Q "Божественное наставление" (105001), 15.11:
- Description `Skill_Description_105001` → `{4|GE}` a number and `{5|GE}` a percentage; the GE ↔ tooltip formula link is in `content/balance/csv/skill_calculate.csv` (`formula_lua_id`) and `skill_num_exchange.csv`.
- Tooltip formulas — client Lua `research\lua_all\...\Script\Text\<formula_lua_id>.lua` (luac54 -l -l). This is an approximation for the text, the real calculation is native `TgfDamageExecution` / `TgfHealExecution` (the exe is packed).
- GE: `UAssetGUI tojson <in> <out> VER_UE5_1 dungeonborne` (the mapping name from `%LOCALAPPDATA%\UAssetGUI\Mappings`, NOT the path to the usmap — with a path you get RawExport).
- `CareerSkillDamageCurveTable`: UAssetGUI does not parse the rows; Extras = int32 rows, u8 mode, then per row: FName(8) + 3 header bytes + int32 count + count×(time,value float) + 4 (default) + 2 (extrap). Script in the scratchpad of session 96188d8f (pq/curves.txt).
- Q AOE: damage `GE_TGF_Damage_Instant_Priest_Q_AOE` MoveFactorRate 0.75, element 4, base = AttackDPH (weapon attack, type 42 DPH) via `ApplyAOEWithGA`, crit allowed; heal `GE_TGF_Heal_Priest_Q_AOE_Lv1` = (DPH + AttackBonus)×(1+AttackBoostRate)×0.5 (curve Heal.MoveFactor). Charging does not affect the numbers. Related to [[dungeonborne-balance-app]], [[dungeonborne-stat-watch]].
