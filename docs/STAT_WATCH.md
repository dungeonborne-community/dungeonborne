# Match math watcher (StatWatch)

Checks two things: whether we compute hero stats correctly and whether the game applies them correctly in a match.

## Parts

| Part | Where | What it does |
|---|---|---|
| DS mod | `game/ue4ss/Mods/DungeonborneStatWatch` (a copy in the Steam install, a line in `mods.txt`) | Snapshots of all GAS attribute sets of heroes and a record of every hit |
| Script | `tools/py/stat_watch.py` | Expected attributes from `content/balance/tables/*.json` and gear from `rpc_dump/OUR.jsonl`, comparison with the mod log, hit analysis |

The mod is separate because the main block of `DungeonborneRuServer/Scripts/main.lua` already has 200 locals, which is Lua's limit.

## Mod
- Runs with the `-dbserver` flag (set by DbTools) or `-dbstatwatch`. Writes to `<dbroot>/statwatch-<date>-<time>.jsonl`
  (`dbtools/run`).
- Settings — `run/statwatch-config.txt`, `key=value` lines:
  - `enabled=0` turns the mod off;
  - `hooks=ApplyGE,IsCritical` keeps only the listed hooks;
  - `snap_sec` — snapshot period, `snap_force_sec` — forced snapshot, `stats_sec` — counter period;
  - `monsters=0` turns off monster attribute snapshots on hit.
- Records (field `t`):
  - `meta` — start, tokens in the command line are hidden;
  - `hooks` — which hooks got installed;
  - `fields` — where the set's field list came from: walking the class properties or the fallback list from CXXHeaderDump;
  - `flcfg` — the game's `FirstLevelAttrDataAsset`: primary stat → secondary attributes × multiplier;
  - `snap` — hero snapshot: open_id, `MyRoleGuid`, weapon in hand, for each set `{field: [BaseValue, CurrentValue]}`.
    Written when stats change (excluding current HP, mana, shield) and every `snap_force_sec`;
  - `crit` / `hitv` (since v2) — every call of `IsCritical` (random number, result) and `CalcHitValue`;
  - `hit` — a hit: who, whom, GE, `DamageRate`, `WeaponAttack` (the game passes 0, the GE sets the damage itself), target HP and shield before and after, damage, crit (random number
    and `IsCritical` result), `CalcHitValue`/`GetWeaponAttack`/`GetDefendValue`/`GetAttackWeight`, the source's attack attributes
    and the target's defence attributes;
  - `hp` — a health drop that no hook explained (DoT, falling, damage in C++ bypassing the BP library);
  - `calls` — hook hit counters.
- Damage points in the game:
  - calculation — native `UTgfDamageExecution` (GameplayEffect execution) and the meta attributes of `TgfDefendAttributeSet`
    (`DamageValue`, `CriticalDamage`), cannot be intercepted from Lua;
  - hooks sit on the BP functions of `UTgfAbilitySystemBlueprintLibrary`: `ApplyGE`, `ApplyAOEWithGA`,
    `Before/AfterApplyGEWithSpec`, `IsCritical`, `CalcHitValue`, `GetWeaponAttack`, `GetDefendValue`, `GetAttackWeight`,
    `GetVulnerFactorRawValue`. They fire only if abilities call functions from blueprints. The remaining damage
    is caught by polling HP every 200 ms (record `hp`);
  - HP lives in `TgfDefendAttributeSet.CurHealth` + `CurShield`, for Lyra sets — in `LyraHealthSet.Health`.
- Check without the game: an offline run with UE4SS API stubs (Lua 5.4) — syntax, JSON, hooks.

## Script
```
python -X utf8 tools/stat_watch.py expected --role <guid> [--pair 2]   # expected values from the items, all rounding variants
python -X utf8 tools/stat_watch.py report [--log run/statwatch-*.jsonl] [--out x.md]
python -X utf8 tools/stat_watch.py selftest
```
- Expected values:
  - role base — `role_affix`;
  - BASIC — the template's `fixed_rule`: offset by `random_level`, `fixed_value` as is;
  - MAJOR and MINOR — `affix_value_template[(affix_value_id, attribute)].rarity_range[rarity]`, value
    `min + (max − min) × level / 10000`;
  - set points and legendaries are listed: they are abilities, they are not visible in GAS attributes.
- **Roll formula (checked 05.11 on the DS, 252 of 252 attributes):** `min + floor(level × n / 10000) × unit`,
  `n = (max − min)/unit + 1` (`bucket` mode). Linear variants raw/floor/round/ceil/rtot are kept for comparison.
- **Damage formula (reconstructed 05.11, deviation 0.0004%):**
  `damage = K × (1 + AttackBoostRate) × (crit ? CritDamageRate : 1) + D`,
  `D = AdditionalElement*DamageOnCausingHit × (1 − (target res. − ElementResPierceNone))`.
  K (weapon damage × hit coefficient × attack level vs defence level) is not separated yet. Open questions — FIX_LIST BAL2.
- Not confirmed:
  - **field per hand or per slot** — weapon damage and attack level, defence level, block. The mapping is hypothetical,
    marked "hand/slot" in the report;
  - ROLE_HOLD applies only to the active pair's weapons (the pair is taken from the weapon in hand in the snapshot), ROLE_INVENTORY
    is not counted.
- Gear is taken from the last `battle_fetch_battle_user_data` (the one the DS received) no later than the snapshot, otherwise from
  the last `player_get_info` or `battle_room_log_sync`.
- The report `run/statwatch-report-<time>.md` shows for each attribute: expected, primary contribution (by `flcfg`),
  actual Base and Cur, total, sources. For hits:
  - crit: `IsCritical` vs `CriticalRate`, z-score of the frequency;
  - damage models A…E: mean and spread of the damage/model ratio;
  - regression of ln(damage) on ln(parameters);
  - crit multiplier vs `CritDamageRate`.
- Move discrepancies into `docs/FIX_LIST.md`.
