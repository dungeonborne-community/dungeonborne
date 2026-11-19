---
name: dungeonborne-stat-watch
description: "StatWatch: DS mod logs hero GAS attributes + hits, tools/stat_watch.py checks them; confirmed discrete affix roll formula and partial damage formula (05.11)"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Combat-math observer built 05.11.2026 (doc `docs/STAT_WATCH.md`, findings FIX_LIST BAL2):
- UE4SS mod `game/ue4ss/Mods/DungeonborneStatWatch` (separate mod: RuServer main.lua is at the 200-local limit), on the
  Steam-install DS via `mods.txt`; writes `dbtools/run\statwatch-*.jsonl` (snap/hit/crit/hp).
- `python -X utf8 tools/stat_watch.py report` → `run/statwatch-report-*.md`; `expected --role GUID`; `selftest`.
- Affix roll in game = `min + floor(level*n/10000)*unit`, n=(max-min)/unit+1 (NOT linear); 252/252 attrs matched.
- Hero ASC lives on PlayerState (owner ≠ pawn) — map ASC → pawn via `PlayerState:GetPawn()`.
- ApplyGE gets DamageRate=WeaponAttack=0; damage = K·(1+AttackBoostRate)·(crit?CritDamageRate:1) + flat on-hit.
Related: [[dungeonborne-bp-reverse]], [[dungeonborne-local-dev]].
