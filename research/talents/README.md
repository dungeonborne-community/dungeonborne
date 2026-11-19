# Stat breakpoints (talents): how it works, 04.11

- Data: `/Game/TGF/Ability/Data/FirstLevelAttr` (UTgfFirstLevelAttrDataAsset). retoc export → scratchpad `talents/`.
  The UAssetGUI CLI did not parse the properties: the export stayed raw.
- `CareerRaces[]` (class + race) → `Talents[]`. Each `FTgfCareerTalentCfgItem` contains:
  - `FirstLevelAttrConditions[]` — {attribute, RequiredValue};
  - `RequiredTags`;
  - `AbilitySets[]` — the passive itself;
  - TalentId, Name/desc — localization keys `Talent_<Class>_NameN`.
- The asset has 57 `AbilitySet_Talent*` sets, 4–6 per class. Mithril classes have separate sets.
  - The NameMap has **no tags at all**: `RequiredTags` is empty for every talent.
  - Rogue (heuristic from floats next to the attribute name): Dex 93, Stamina 36, Strength 60, **Dex 62**, Strength 29.
- Granting on the server is C++: UTgfFirstLevelAttributeSet (`Talents`, `GrantedTalentHandles`, `IsSatisfiedAttribute`,
  `InitTalents`/`ClearTalentGranted` on the attribute component). It grants AbilitySets when a stat reaches the threshold.
- **Not verified**: whether native `IsSatisfiedAttribute` takes the `RequiredTags` field into account. Ghidra did not fully
  show Exec 0x142e60a80 with its inlined implementation.
