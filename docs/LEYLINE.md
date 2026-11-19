# Leyline Seals and Altars

Analysis from 08.11. Static only: game strings, tables, assets from the paks (retoc → kismet-analyzer / UAssetGUI), our
loot template, the stand server log. I did not run the game and did not touch prod or the mods. Working files: `<data>\research\leyline`.

## Conclusion
- **The mechanic is fully present in the game files and in our tables:** items, drops, combining, the altar, the altar reward.
  Nothing is "cut", and it needs no changes in our code or mods.
- **Seals do not drop everywhere:** only on Classic and Hard difficulties (loot maps 10102, 10103, 10203). On Casual (Map_x_1)
  and in the Classic Castle there are none at all — that is how the PVE loot tables we took as the base are.
- **The altar stands on only three maps:** Dungeon Classic and Hard (first floor, 5 points), Castle Hard. The Classic Castle
  and all Casual maps have no altar.
- **Main suspect for why the altar is missing even where it should be:** its spawn points lie in "phase" rooms
  (`bSpawnWhenPhaseStarts`, phase `TGF.GamePhase.WarmUp`). From the same kind of rooms on the first floor our blue
  and red exit devices do not appear (FIX_LIST, "placed=0, rooms ran, cause not found"). Today's match on
  Classic (stand log 08.11 12:56): `[DIAG] points (phase): … BP_Altar_Special_Dungeon_C×5`, all points in these rooms have
  0 placed. The log does not show directly that the altar did not appear (the probe does not look for it), but the cause is most likely the same.

## Items (`item_type`, `UeSource_ItemType`)
| id | Name (EN) | Rarity | Stack | Notes |
|---|---|---|---|---|
| 1900003 | Leyline Seal Fragment | ENHANCED | 10 | Ctrl+LMB: 10 fragments → 1 seal |
| 1900004 | Leyline Seal | EPIC | 7 | Hand in at the altar; Ctrl+LMB: 7 seals → 1 legendary |
| 100100 | Legendary Leyline Seal | LEGENDARY | 1 | Hand in at the altar |

All three: `is_forbidden_bring_into_battle`, extraction `CAN_NOT_BUT_ALLOW_CROSS_LAYER` — they live only inside a raid (carried
between floors). Combining is the native fragment `InventoryFragment_SynthesizableItem`, on the client and the battle server; the backend takes no part.
Quests EA_QuestGroup4 1651685–1651687 ("Leyline Alter" on Hard, crafting a legendary seal) also belong to this mechanic.

## Drops (our `content\loot\loot.toml`, matches PVE loot)
Pools 900112 (seal), 900113 (legendary), 900114–900117 (fragments ×1–4). Slots by map (chance, %):

| Source | Classic 10102 | Hard 10103 / 10203 |
|---|---|---|
| wooden/regular chests | fragment 2.5 | fragment 5 |
| armored hammer, beholder, treant | fragments ×1–3 — 10 | 20 |
| small monsters (boar, goblins, ghosts…) | fragment 3 | 6 |
| mimics | fragments ×1–2 — 7.5 | 15 |
| golden chests | fragments ×2–4 — 15; seal 2.5 | 30; 5 |
| bosses (Shadow King, Cyclops, Executioner…) | seal 20; legendary 1 | 40; 2 |
| goblin thief | seal 10 | 20 |

The stand server loads exactly these tables (`[LOOTCHECK]` 919020→990022 etc. = `content\balance\tables\loot_choose_group_rate.json`
from 08.11, 11 seal rows in `loot.json`). **Correction 08.11:** in practice they do not drop — the native `Loot` takes the map rows
from `UBattleDsUtility+0x1d4`, and on listen it is 0 there, i.e. always the loot of map 0 without seals. Analysis — `docs/LEYLINE_SEALS.md`.

## Altar
- Classes: `BP_Altar_Special_Dungeon_C` (loot source 300), `BP_Altar_Special_Castle_C` (299); parent
  `BP_Altar_Award` → `BP_AltarBase`. Tip `Interact_Altar_Special` "Convert Leyline Seals", refusal
  `Interact_Fail_Altar_Special` "Lack of Leyline Seal".
- Rewards (`AvailableAwards`, struct `FAltarAward {AwardId, ItemCost: Map<item class, count>}`), the same
  for both altars:
  - 1 × `ID_Loot_Gold_100100` (legendary seal) → `AwardId` 4;
  - 1 × `ID_ItemFragment_1900004` (seal) → `AwardId` 3.
- `CanPlayerInteract`: walks the rewards in order; the first one the player has enough items for sets
  `CurrentLootType = AwardId`. The legendary one is checked first, then the regular one.
- `SpawnItems`: the native `BattleUtility::Loot(self, …, altar class name, CurrentLootType)` — regular loot by source and type.
- Altar loot in the tables (`loot_ue_source_choose_group`, sources 299/300, all maps): type 3 → slot 910571, pool 900118
  "treasures 8", 100%; type 4 → slots 910572 + 910573, pools 900119/900120 "treasures 4", 100% each.
- `IsDedicatedServer` in `BP_AltarBase` turns off only sound and cooldown effects on the server — does not affect how it works.
- "25% and guaranteed on the 4th try" from player descriptions is not visible in our tables: the altar always gives a treasure, the
  bad-luck table `loot_unlucky_value` is empty (in the original 2.19.2 export too).

### Where the altar stands
`BP_ActorSpawnPoint_SpecialAward` (subclass of `BP_ActorSpawnPoint`): `Spawn` → native altar spawn, then looks for
`BP_DeathZone`; if there is exactly one zone — sets its `bHasSpecialAward = true` and `SpecialAwardPosition` (the "rich zone" marker on
the minimap). If there is not exactly one zone — writes `[Warning] The number of death zones if not 1` and sets no marker.

| Map | Points |
|---|---|
| `L_Dungeon_Official_01_Classic` | 5 (UAID …AD1902_1588156472, _1747755746, _1896112981, _2059423994, …AE1902_1081777262) |
| `L_Dungeon_Official_01_Hard` | the same 5 |
| `L_GothicCastle_WP_Main_Hard` | present, cell `MainGrid_L6_X-1_Y0` (did not count them) |
| Casual, Castle Classic, second floor | none |

All 5 Dungeon points are in "phase" rooms `WarmUp` (`[DIAG]` log).

## What to check in game (one match on Dungeon Classic)
1. Whether `BP_Altar_Special_Dungeon_C` is on the map (FindAllOf in the server probe) and the zone marker on the minimap. If not —
   it is the same breakage as with the exit devices in phase rooms; fix them together.
2. Whether fragments/seals drop (kill a couple of monsters/open a golden chest; a GM grant of 1900004 also works to test the altar).
3. Altar: hand in a seal → a treasure from pool 900118 drops, the seal is consumed.

If the altar does not spawn, a temporary bypass for the server mod (the decision is up to the user and the orchestrator): after the
floor starts, call `Spawn` on one random `BP_ActorSpawnPoint_SpecialAward_C`. How many altars the original placed (one point
of five or all of them) cannot be seen statically: the choice is made by the `BP_ActorSpawnSet` in the room.
