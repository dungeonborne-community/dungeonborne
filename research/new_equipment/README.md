# New equipment from scratch: note 05.11 (idea, not started)

User's decision: write it down next to "new characters" (`research\mithril_classes`), do it later.

## What an item consists of (using "Hood of Faith" 182014000 and the sword 181014000 as examples)

### Tables (`content\balance\tables\*.json`, to the client as a table pak via `cargo xtask balance … --deploy`)
- `item_type`: `type_id`, `name` (localization key `Item_Name_<id>`), `level_require`, prices, `ue_source_type_id`.
- `UeSource_ItemType`:
  - `item_type` (`EQUIP_ARMOR` / `EQUIP_WEAPON`), `slot_type` (`EN_SKIN_SLOT_TYPE_CLOTH_HELMET`, `…_SWORD`);
  - `brush_data` — icon;
  - `position_cfg` — size in the grid, `row_size` along x and `column_size` along y; helmet 2×2, sword 3×1;
  - `ue_path` → `…/ID_Helmet_C004001.ID_Helmet_C004001_C`;
  - `ue_source_equipment_equip_id`.
- `UeSource_Equipment`:
  - `equip_subtype`, `equipment_type`;
  - `gear_configs.available_equip_slot`;
  - `career_restrict` — classes (cloth: priest, wizard, lich, mithril priest);
  - `ue_path` → `…/AID_Helmet_C004001…` — what the item grants when equipped.

### Assets
- `TGF/Equipment/Armor/<Slot>/GameAbility/S_C004001_<X>/ID_*.uasset`, `AID_*.uasset` — blueprints on the game's C++ classes.
- `TGF/Character/Suit/S_C004001/`:
  - `Mesh/C004001_{A,C,G,H,S,T}` — models; `X`: A, C — torso, G — gloves, H — helmet, S — shoes, T — pants;
  - per-body variants: `_Human_Female`, `_Undead`; helmet and shoes have a single model;
  - `C004_{Man,Female,Undead}_AnimBlueprint` + `*_Skirt_*_PhysicsAsset` — physics for long cloth;
  - `Texture/T_C004001_{D,N,ORM,MicroNormalMask}`, `MI_C004001_*`.
- Weapons: `TGF/Equipment/Weapon/<Type>/GameAbility/WP_<Type>_4001/{ID_…, B_…}`, where `B_` is the weapon actor.
  Animations and attacks come from the weapon type.

## Levels
1. **Tables only**: a new item on an existing model — name, stats, affixes, classes. Easy.
2. **New look**:
   - models for each body type weighted to the hero skeleton (Blender), textures, icon;
   - a clone of `ID_`/`AID_` with replaced references: asset editing as in `bp_patches`, or via the mod kit;
   - pak build: the Jester pipeline `<data>\ue_jester`, materials are children of the game's ones, no new shaders;
   - prototype: a helmet (single model); the model can be made in Meshy, like the Jester.
3. **New mechanic** (a custom ability on the item): mod kit, stubs of C++ classes from `UHTHeaderDump`.

See also `research\mithril_classes\README.md`: mithril suits use the same "item grants abilities" scheme.
