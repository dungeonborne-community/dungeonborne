# Portal sonar, broken portals, resonance stones, ritual scroll

Analysis from 07.11, static only: blueprints from the paks (retoc → kismet-analyzer), game strings, tables, our loot and mods. Not checked in a match. Working files: `<data>\research\portals` (`cfg\` — BP graphs, `json\` — assets, `filelist.txt` — list of pak files).

## Conclusion
- **The "sonar" is a native UE mechanic; the backend takes no part in it.** Everything is done by battle server blueprints, the marker reaches clients through GameState replication. We have already checked the same links (`BP_TgfGame_MapHint` → client) on the stand with portal messages (`[PORTALMSG]`, FIX_LIST 07.11). So the sonar should work, but it has not been checked live.
- **Broken (grey) portals and stones exist in the game and do appear for us.** The stones drop in our loot on Classic/Hard maps. The stone requirement, consumption, the triple variant and destruction by the zone are native and need no changes.
- **No change from our side is needed.** Check in a match; the list is at the end.

## 1. Sonar: marker when a portal is being opened
The game never uses the word "sonar". It is a map marker visible to all players while someone opens a portal without a scroll. Loading tip `LoadingContent_3`: opening a portal without a ritual scroll reveals your position on the minimap.

**How it works** (`/Game/TGF/Components/PlayerControllerComponents/BP_MapInteractionComponent`, on the server):
1. The player starts interacting with `BP_ExtractDevice_*`, `AddMapHint(pawn, device, location)` is called.
2. If the player has `ID_TeleportScroll` (`LyraCharacter.HasInventoryItem`), there is no marker.
3. Otherwise `BP_TgfGame_MapHint.AddEscapeHintData(type, location, device, player, id)`:

| Device (`PortalType`) | Marker |
|---|---|
| Blue (1) | `PortalBlueHandling` (1) |
| Red (2) | `PortalRedHandling` (3) |
| Grey, broken (4) | `PortalBlueHandling` (1) — same as blue |
| Shadow (3) | None, it has its own permanent marker `PortalShadowSpawn` (7) from `BP_ExtractDevice_Shadow*` |

4. Interaction finishes or is interrupted: `RemoveMapHint` → `RemoveEscapeHintData`, the marker is removed.
5. `ArrEscapeHintData` is replicated (`OnRep_ArrEscapeMapHint`), the client draws the marker and a message (`ETgfDisplayInfoPortalSubType` BlueHandle 5 / RedHandle 6).

The sound and glow on activation are the device's normal effects (`FX_Portal01_Cast`, `Play_Portal_Idle_Loop`); they are not tied to the scroll.

## 2. Ritual scroll (100021)
- Item: `ID_TeleportScroll` (`UeSource_ItemType`: 100021 → `/Game/TGF/Equipment/Item/TeleportScroll/GameAbility/ID_TeleportScroll`).
- **Spawns on the map:** `BP_DeathZone.SpawnOneScroll` places a `BP_CollectableBase` with the scroll. Points — the zone's `PotentialTeleportScrollPlacements`, timing — the disk settings. Our `[DIAG]` on the stand: 47 scroll points.
- **Map marker:** a scroll lying on the ground marks itself with `ScrollsKeyShowing` (2) from `BP_CollectableBase`; the marker is removed on pickup. This is the guide line "scrolls not yet picked up are marked on the map".
- **Consumption:** `BP_ExtractDevice_Base.PerformInteraction on Server`: if `ConsumeScroll` (true in the base class, subclasses do not change it) and the scroll is present — `RemoveItemByDefinitionAuth(ID_TeleportScroll, 1)` and the toast "一个卷轴被消耗了" (a scroll was consumed). It is consumed on any device, including grey.
- The scroll is not in our loot or the shop, only on the map.

## 3. Broken portals (grey, `GreyDevice` = 4)
Devices in `/Game/TGF/Environment/Escapes/`, all spawn the `BP_ExtractPortal_Yellow` portal. Tip: `Interactable_Activate_Portal`, the spawn message — `Game_GreyPortalAppear`, tag `TGF.Environment.PortalGrey.Spawn.Message`.

| BP | Stone (`ItemCost`) | How many | Portals |
|---|---|---|---|
| `BP_ExtractDevice_Grey` | `ID_Loot_100101` | 1 | 1 |
| `BP_ExtractDevice_GreyThree` | `ID_Loot_100101` | 3 | 3 |
| `BP_ExtractDevice_Grey_Classic` | `ID_Loot_2100018` | 1 | 1 |
| `BP_ExtractDevice_GreyTwo_Classic` | `ID_Loot_2100018` | 2 | 2 |
| `BP_ExtractDevice_GreyThree_Classic` | `ID_Loot_2100018` | 3 | 3 |

Values were read from the CDO (unversioned properties, parsed by hand: `PortalType`=4, `PortalClass`=Yellow, `DestroyInDeathZone`=true, `ItemCost`). Check and consumption — `BP_InteractableBase.CheckItemCost` / `ConsumeItems`.

- **Destruction by the zone:** `DestroyInDeathZone`=true → `TgfPortalManagerSubsystem.UpdatePortalsNeedDestroyInDeathZone`. Matches the description: the zone covers it — the device is gone.
- **Which device is on the map** (Hard or Classic) is decided by the map zone's disk settings (`PortalTypeSelections`), not by the backend.
- Our mod: no grey ones on the second floor (user's decision 06.11); `spawnportal grey|greydevice` for testing.

## 4. Resonance stones
| id | Name | UE item | Where it is used |
|---|---|---|---|
| 2100000 | Portal resonance stone (Hard) | `ID_Loot_100101` | Grey Hard devices |
| 2100018 | Portal resonance stone (Standard) | `ID_Loot_2100018` | Grey Classic devices |
| 2100001 | Corrupted stone | `ID_CorruptedRune` | Turns a Hard blue portal into a red one (not in our loot) |
| 100101 | Ley line stone | — | Strings exist, there is no item in `UeSource_ItemType`; its asset is used by stone 2100000 |

- **Loot:** both stones are in treasures on maps 10102/10103/10202/10203 (Classic/Hard; `loot.toml` pools 900085/900086, `loot_ours.toml` pool 900010).
- **Shop:** products 705019/705020 (tab 1019) are in `content/balance/tables/market_product_item.json`. FIX_LIST from 03.11 says they were removed from balance; I did not check what the client shows now.
- A stone brought from the stash is created on the battle server by `ue_path`, so it fits the device the same way as one picked up. Removal from the inventory after the match follows the raid results; not checked.

## 5. Mithril crystal (100071) — portal detector
Not to be confused with the sonar. A consumable "after crushing it you sense the nearest portals": `GA_TGF_PortalDetect` → `TGF.Ability.AbilityTag.StartDetect.Portal` → `BP_TgfGame_MapHint.AddPortalDetectMsg` shows portals within a radius. It is given only in mithril kits (`role_default_equipment`, slot CONSUMABLE3; `mithril_force_item`, tab 1004). It is not in our loot or the shop.

## What to check in a match
1. Open a blue device without a scroll: the second player has a marker on the map, it disappears when opening ends.
2. Same with a scroll: no marker, the scroll is gone, a toast.
3. Grey device: does not open without a stone; opens with a stone, the stone is consumed; GreyThree takes 3.
4. The zone covers a grey device — it disappears.
5. After extraction the consumed stones and scroll did not come back to the stash.

Quick on the stand: `spawnportal greydevice`, `spawnportal bluedevice`; stones — `gm.py` item grant 2100000/2100018.
