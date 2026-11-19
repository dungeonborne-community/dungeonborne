# Arena PVP chests: capacity and layout — analysis 05.11

## Capacity
- `BP_TreasureChest_PVP_*` → parents `BP_InteractableChest` → `BP_InteractableChestBase`.
  The base class has a `TgfInventoryManagerComponent` component ("Loot Inventory Manager").
- Initialisation (`ExecuteUbergraph_BP_InteractableChestBase`):
  `InitInventoryWithSize(GameState.IsPVPMatchType ? GetDefaultInventorySizeInPVP() : GetDefaultInventorySize())`.
- Both are static C++ functions: `AssetManager.Get()` → `GetGameData()` (exec `0x142de30b0` / `0x142de3070` → `0x142fd27b0` / `0x142fd2780`).
  Source: the asset `/Game/TGF/Ability/Data/Inventory` (`TgfInventoryDataAsset`, `DefaultGame.ini [TgfAssetManager] InventoryData`).
  Properties in order (unversioned, parsed by hand):

  | Property | Value |
  |---|---|
  | `PlayerInventorySize` | (8, 10) |
  | **`PVPInventorySize`** | **(9, 10)** |
  | `PlayerStorageSize` | (20, 12) |
  | `PackageList` | array |

- **In a PVP match the chest is a 9 × 10 = 90 cell grid.** If `IsPVPMatchType` = false, it is 8 × 10 = 80.
- Axes: occupancy is stored as `index = Y * x + y` (`0x142fdeb10`), an item takes `row_size` cells along x and `column_size` along y.
  The backend counts the same way (`progress_handlers::footprint`, `stash_grid`). x = 9, y = 10.

## Item sizes
- `GetItemSize(ItemDef)` (`0x142fd4bd0`): object ItemDef+0x110, fields +0x54 / +0x58; if not > 0, 1×1.
- The game tables say the same: `item_type.ue_source_type_id` → `UeSource_ItemType.position_cfg` (`row_size`, `column_size`).

| Type | row_size × column_size |
|---|---|
| helmet, gloves, shoes (cloth / leather / plate) | 2×2 |
| torso, pants | 3×2 |
| ring, necklace | 1×1 |
| staff | 4×2 |
| orb relic | 2×2 |
| mace / hammer | 3×1 |
| sword | 3×1 |
| longsword | 4×2 |
| shield | 3×2 |
| dagger | 2×1 |
| crossbow | 3×2 |

## If it does not fit
- BP: `BattleUtility::Loot` (native) returns an array of items. A loop calls `AddItemInstanceAuth` for each.
  The result is not checked; the chest has no "drop on the ground" branch (`DropItemToWorld`).
- Native `AddItemInstanceAuth` is virtual (vtable+0x4a8 of `TgfInventoryManagerComponent`), not traced to the end.
  There is no "no space" message among the `UTgfInventoryManagerComponent` strings.
  Most likely an extra item is simply not added. **Not verified**: whether `BattleUtility::Loot` accounts for space in advance.

## How much is needed to fill 9×10 (90 cells) completely
It is convenient to count in strips 2 wide along y: there are 5 of them, each 9 long along x.

| Chest | How to fill completely | Items |
|---|---|---|
| Ring | 1×1 only | 90 |
| Cloth / Leather / Plate | strip = 3 large (3×2) **or** 3 small (2×2) + 1 large (2+2+2+3 = 9) | 15 (all large) … 20 (15 small + 5 large); small only: at most 20, leaving 10 empty cells |
| Staff | strip = staff 4 + orb 2 + 2 maces stacked (3×1×2) | 20; maces only: 30 |
| Sword | strip = longsword 4 + shield 3 + 2 daggers (2×1×2) | 20; swords only (3×1): 30; daggers only: 40, 10 cells empty |
| CrossBow | 3×2 only | exactly 15 |

Currently the `pvp_*` groups in `loot/loot.toml` give 6 slots at 100%: 6 items per chest.
With a random mix and automatic placement ("first free spot") 100% is not guaranteed. Only a uniform mix fills reliably: rings 90, crossbows 15, torso/pants 15, swords/maces 30.
