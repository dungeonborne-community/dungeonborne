# Visual bug 4: gear on the ground looks different from the inventory and on the hero

Analysis from 10.11 of client assets (retoc → kismet-analyzer), the SDK (UHTHeaderDump) and backend code. I did not touch
the mods, the stand or the game. Working files: `<data>\research\ground_item`:
- `base_bc.txt` — `BP_CollectableBase` bytecode;
- `pickup_table.tsv` — table of ground meshes for all 766 item definitions;
- `kdump.py`, `props.py`, `resolve.py` — analysis scripts.

**User's decision 10.11: leave as is (as in the original), bug 4 closed without changes.**

## Conclusion
**This is how the game's own data works; there is no backend or mod bug.** Armor, rings and necklaces have no
item-specific mesh on the ground. Instead there is a generic "loot" model per slot and armor type: `SM_Looting_<Slot>_<C|L|P>`,
one ring model and one necklace model. The inventory icon and the model on the hero are item-specific, so
the look on the ground does not match them.

## How the look is chosen
| Where | Where it comes from | Item-specific? |
|---|---|---|
| Inventory icon | `InventoryFragment_InventoryIcon.Brush` in `ID_<item>` | yes |
| On the hero | `InventoryFragment_EquippableItem.EquipmentDefinition` → `AID_<item>.ChangePartConfigs` (skeletal meshes by race, sex and camera view) | yes |
| On the ground | `BP_CollectableBase`: `FindItemDefinitionFragment(CollectableObjectType, InventoryFragment_PickupMesh)` → `LoadAsset_Blocking(StaticMesh)` → `ObjectNode.SetStaticMesh` | weapons only |

`CollectableObjectType` is the item definition class (`ID_…`). It is determined by `type_id`. `DItem` has no appearance fields:
only `type_id`, affixes, durability, gems etc. There are no skins in a match: `DBattleSkin.skin_ids` is empty
(`battle_handlers.rs:106`, `room.rs:181`). So the server cannot pass the "wrong" look. Our paks (`zz_*`,
`BPFix`, `ChatFix`) touch neither `ID_*` nor `BP_CollectableBase`.

## Data (pickup_table.tsv)
- **Armor 182/183/184 (cloth/leather/plate), rings and necklaces 185.** 227 of 228 definitions have no
  `PickupMesh` of their own, the mesh is inherited from `ID_<Slot>_<Type>_Base`:
  - cloth: `SM_Looting_{Chest,Glove,Helmet,Shoes,Trousers}_C`;
  - leather: `SM_Looting_{…}_L`;
  - plate: `SM_Looting_{…}_P`;
  - `SM_Looting_Ring`, `SM_Looting_Necklace`.
  The exception is leather trousers 183045000 (`SM_S_L015001_T`).
- **Weapons 181.**
  - 58 definitions have their own mesh `SM_WP_<type>_<number>`, the number matches the icon.
  - 13 inherit the mesh of the base model of the same line (for example, `ID_Sword_1002` → `SM_WP_Sword_1001`, icon also 001001).
  - Legendary staves (`Staff_x003`) have their own actor class `BP_CollectableBase_LegendWeapon_*`.
  No discrepancies for weapons.
- **Uniques 186/189** (mithril ring, lunar necklace, holy coffin, helmets `189010000`/`189010006`) — all have
  their own mesh.
- **A bug in the original's data:** leather trousers (`ID_Trousers_Leather_Base`, 10 items 18304xxxx, except
  183045000) lie on the ground as plate (`SM_Looting_Trousers_P`), although `SM_Looting_Trousers_L` exists in the game.

## Options (the user decides)
1. **Keep as in the original (recommended).** Close bug 4 marked "as in the original" and record it in WORKAROUNDS
   or FIX_LIST.
2. **Small fix: leather trousers.** In `ID_Trousers_Leather_Base` replace the path `SM_Looting_Trousers_P` with
   `SM_Looting_Trousers_L`.
   - The string length is the same, so this is a single name edit in the name map.
   - Needs a client pak (IoStore, like `BPFix`). The battle server is not affected: the mesh is set on the client in
     `OnRep`/BeginPlay.
   - Effort — half an hour plus a check in a raid.
3. **A distinct ground look for each armor piece.** There are no ready static meshes for each item. One would have to:
   - make about 230 static meshes from the skeletal ones in the UE editor and set them in every `ID_*`;
   - or rework `BP_CollectableBase` so it uses the skeletal mesh from `AID` (the armor would then stand in a mannequin
     pose and look worse).
   Large effort, not recommended.

If the user meant a different item (a weapon, a unique), its `id` or an "inventory and ground" screenshot is needed:
per the table, weapons have no discrepancies.
