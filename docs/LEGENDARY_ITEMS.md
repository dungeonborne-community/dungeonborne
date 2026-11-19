# Legendary and unique items: which ones have their own mechanic

Analysis of 09.11, from files only:
- game tables (`ExcelConfigjson`: `UeSource_ItemType`, `affix_template`, `affix_legendary_rule`,
  `battle_ability_set_description`, `UeSource_AbilitySet`), plus the playtest season;
- strings (`AffixStringTable.csv` and the Russian `Game.locres` from `pakchunk0`);
- item blueprints `ID_*` / `WID_*` (retoc);
- the PVE reference save.

Not checked in the game. Working files: `<data>\research\legendary` (`table.py`, `table.json`, `locres.py`).

## Conclusion
**Items without a mechanic simply don't have one in the game.** It's not a tooltip issue.

- There are 93 legendary and unique gear items. **27** have their own mechanic: 25 by the tables, plus 2 items
  `189010xxx`.
- The other 66 are "legendary" only by color: boosted numbers and regular properties. They have no
  `legendary_rule` row in `affix_template`, no ability set and no description. The `ID_*` / `WID_*` blueprints themselves don't
  grant anything either: only the base weapon (Endor's Frost Staff has the ice `WAID_Staff_Ice`, i.e. just the element).
- For all 27 the tooltip chain is complete:
  1. `affix_legendary_rule`;
  2. description in `battle_ability_set_description`;
  3. Russian string in `Game.locres`;
  4. ability set in `UeSource_AbilitySet`.

  Our backend writes the mechanic into the item the same way as PVE:
  `ability_set_random {id: {legendary_affix_id, random_level}}`, type LEGENDARY (`affix.rs`). The playtest season doesn't change these
  rows.
- **On prod only these 27 drop** (playtest season loot: 11 legendary and 9 unique weapon types, 6 rings and
  the "Initiator" helmet). None of the 66 items occurs in loot, shop, lotteries or rewards. The exception is
  "Sword of the Gods" (189010005): it is in `quest_list` as a reward.

If a player sees an item from the 27 without a mechanic line, it's a bug: we need its id (or a tooltip screenshot) and where the item
came from. If the item is from the 66, there is no mechanic in the game data.

## 27 items with a mechanic
| id | Item (name translated from ru) | Rar. | Mechanic (tooltip text, translated from ru) | Ability |
|---|---|---|---|---|
| 181016000 | Relic of the Sacred Grove | uniq. | When you hit a target with a melee attack, the target is slowed by 80%, and you by 60%. | `AbilitySet_Affix_BattleKing` |
| 181015001 | High Elf Guardian's Sword | leg. | When you hit a target with a melee attack, movement speed increases by 150 units. The effect fades over 3 sec. Cooldown: 8 sec. | `AbilitySet_Affix_SpeedUpOnHit` |
| 181015000 | White Star Knight's Sword | leg. | When you hit a target with a melee attack, you briefly gain lifesteal. The effect stacks up to 10 times. | `AbilitySet_Affix_LifeStealWhileHit` |
| 181056003 | Lightning Staff of Immortality | uniq. | Activates an enhanced "Storm Call". | `AbilitySet_Staff_Thunder_181056003` |
| 181055003 | Stormcaller | leg. | Stores 3 charges of the staff skill "Storm Call", which can be released by holding LMB. | `AbilitySet_Staff_Thunder_181055003` |
| 181055001 | Endor's Scorching Tree | leg. | The last explosion of the fire beam deals area damage and briefly sets the target on fire. | `AbilitySet_Staff_Fire_181055001` |
| 181036001 | Servant of Darkness | uniq. | While guarding you can press LMB to throw a homing shield that bounces 3 times, dealing damage and slowing targets. The effect can be used every 5 sec. | `AbilitySet_Affix_ShieldProjectile` |
| 181035001 | Silver Defender | leg. | A successful guard slows the attacker for 1.5 sec. | `AbilitySet_Affix_BounceSpeedDown` |
| 181085000 | Endor's Orb | leg. | A successful guard restores a little orb energy. | `AbilitySet_Affix_SucceedGuardRecoverAmmo` |
| 181126001 | Beast Hunter's Hammer | uniq. | The second stage of the mace-and-shield charged attack additionally deals area damage. | `AbilitySet_LegendaryAffix_181126001` |
| 181125001 | Heavenly Hammer | leg. | After a successful guard movement speed increases greatly. | `AbilitySet_Mace_MoveSpeedBurstWhenGuardSucceed` |
| 181125000 | Sanctum Breaker | leg. | Has enormous attack power. | `AbilitySet_Affix_MaceHighDPH` |
| 181026001 | Soulslayer | uniq. | Has an elongated blade and deals more damage. Normal attacks can break through guard. | `AbilitySet_Affix_LongSword_6002` |
| 181025001 | The 17th Key | leg. | Has a unique charged attack. After the attack is fully charged, keep holding the stance to increase movement speed for a short time. | `AbilitySet_LegendaryAffix_181025001` |
| 181025000 | Thunderer's Blade | uniq. | A counterattack after a perfect block summons chain lightning that bounces up to 5 times, dealing significant lightning damage to the target. | `AbilitySet_LegendaryAffix_181025000` |
| 189010001 | Error | uniq. | Killing enemy players fully restores life. | `AbilitySet_Affix_LongSword_189010001_KillRecoverFullHP` |
| 181046000 | Scorpion's Sting | uniq. | Fires ice bolts, freezing the target for 3 sec. | `AbilitySet_HeavyCrossbow_181046000` |
| 181045000 | Storm Wind | leg. | Aiming and reloading no longer reduce movement speed. | `AbilitySet_HeavyCrossbow_181045000` |
| 181116001 | Sorcerer's Thorn | uniq. | If the dagger is in the main hand, charged attacks absorb all of the target's shields and give the same amount of shields to the wielder. The effect lasts 10 sec. | `AbilitySet_LegendaryAffix_181116001` |
| 181115001 | Rogue's Dagger | leg. | If the dagger is in the main hand, backstabs and charged attacks have increased range. | `AbilitySet_Affix_ManaBreak` |
| 185016002 | Fairy's Blessing | leg. | The number of fairies of "Lake Fairies" increases to 2. | `AbilitySet_Affix_AeryEnhanced` |
| 185016001 | Dispiriting Gaze | leg. | The bonus levels of the "Thunder Fury" set increase greatly. | `AbilitySet_Affix_FleetFootworkEx` |
| 185016000 | Chaos Entanglement | leg. | The hit radius of "Tangled Chain" increases to 15 m. | `AbilitySet_Affix_UndyingGraspEnhanced` |
| 185015002 | Lament of the Fallen | leg. | The cooldown of "Dragon Scales" is reduced to 30 sec. | `AbilitySet_Affix_BonePlatingEnhanced` |
| 185015001 | Aegis Protection | uniq. | When a shield activates, life is restored by 50% of its durability. | `AbilitySet_Affix_Ring005002_HealWhenGettingShield` |
| 185015000 | Death's Gaze | leg. | When "Death's Mercy" triggers, invulnerability activates for 1.5 sec. | `AbilitySet_Affix_Ring005001_AvoidDeathEnhanced` |
| 189010000 | Initiator | uniq. | Killing enemy players resets the cooldown of all active skills. | `AbilitySet_Ex_Helmet_189010000` |

Quirks of the original data (that's how the game is, not our bug):
- **Lament of the Fallen** (185015002) and **Dispiriting Gaze** (185016001) take their text from someone else's key (`affix_legendary_181035000`,
  `…181115000`). The text, however, is about their own mechanic: the enhanced sets "Dragon Scales" (`BonePlating`) and
  "Thunder Fury" (`FleetFootwork`).
- **White Star Knight's Sword** and **High Elf Guardian's Sword** (181015000/001) swapped mechanics: the first has lifesteal,
  the second speed. The tooltip matches what the item gives.
- The unique sword 189010001 is called **"Error"** in Russian and Chinese (`错误`).

## 66 items without a mechanic
- **Weapons (11):**
  - Frost Fang 181016001;
  - Sword of the Gods 189010005;
  - Fire Staff of Immortality 181056001;
  - Endor's Frost Staff 181055002;
  - Dragonscale Bulwark 181036000;
  - Cloud Veil 181035000;
  - Monarch's Skull 181086000;
  - Avil's Blessing 181126000;
  - Blood Echo 181026000;
  - Avil's Favor 181116000;
  - Thunder Dagger 181115000.

  Cloud Veil and Thunder Dagger still have old descriptions in the strings (`affix_legend_*`: "a successful block
  restores mana", "hits poison"), but there is no ability behind them: the mechanic was cut in the game itself.
- **Armor (25):** the Endor, Resolve, Shadows, Mythic and Glory sets — 5 pieces each.
- **Necklaces (9):** 185025000–006, "Dragon Heart", "Reaper's Mark".
- **Class necklaces (15):** 185020xxx. These are the class starter necklaces. Their abilities (Q/E and `NecklaceAttr`) are granted by
  the `AID_*` blueprint, not by a legendary property.
- **Other (6):**
  - test necklaces 18999999x (4);
  - "Devotee's Ring" and "Moon Pendant" (186015001/002, Mithril Order) — they don't even have an `affix_template`.

## Fire Staff of Immortality (181056001), analysis 09.11
- **The original's table dump** (`<data>\pb\dumped_json`) matches our `ExcelConfigjson` one to one:
  `affix_template` (694), `affix_legendary_rule` (27), `battle_ability_set_description` (27), `UeSource_AbilitySet`
  (83). The staff's row is the same, there is no `legendary_rule` in the original either. In `rpc_dump` the staff appears only in our
  server's trade responses, there are no reference packets with it.
- **The staff has no ability of its own in the pak.** `WP_Staff_6001` contains only `ID_`, `WID_` and `B_Staff_6001`. For
  comparison, the Lightning Staff of Immortality (6003) has `GA_TGF_Staff_Thunder_6003`, `AbilitySet_…` and `WAID_…` next to it.
- **The staff doesn't even have the fire beam.** `WID_Staff_6001` inherits from `WID_Staff_Base`
  (→ `WAID_Staff_Base`: `AbilitySet_Staff_Common` — Combo1/2 hits, `ReCharge_Fire` and `AutoRecover`). All other
  fire staves (0001…5001, inherited) inherit from `WID_Staff_Fire` (→ `WAID_Staff_Fire`), which has
  `AbilitySet_Staff_Fire` = `GA_TGF_Staff_Laser` (beam on hold) and the weapon tooltip `WBP_WeaponTips_StaffFire`.
  Only the model and the `B_Staff_Fire_Base` projectiles make the staff a fire one.
- **Numbers:**

  | Staff | Level | Attack | Beam | Mechanic |
  |---|---|---|---|---|
  | Fire Staff of Immortality | 6 | 156 | no | no |
  | Endor's Scorching Tree (legendary, 5001) | 5 | 156 | yes | area beam explosion |

  The unique staff is weaker than the legendary one: it has 1 more attack level, but no staff skill.
- **Check by BP (properties and bytecode, kismet-analyzer → `research\legendary\json`).**
  - There is no bytecode in `WID_Staff_6001`, `B_Staff_6001` or `ID_Staff_6001`.
  - `Default__WID_Staff_6001_C` has only `EquipDefId` = 181056001, `bVisibleWhenUnequip` and `ActorsToSpawn` (model).
    No ability list of its own, so the base one (`WAID_Staff_Base`) is used. The same for 5001: only `EquipDefId` and
    the model, and the beam comes from the parent `WID_Staff_Fire`.
  - `B_Staff_6001` is a model on top of `B_Staff_Fire_Base`, with no logic of its own.
- **Check by Ghidra (`research\legendary\l1.c`).**
  - Not a single id of the legendaries or uniques (181056001, 181055001, 181025000, 185015000, 189010000 and others) is in the code
    as a constant: native code grants nothing by item id.
  - An ability set by id is loaded by one function, `0x142f45570` (log `GetAbilitySetClass Load %d`). It is called by:
    - `0x142ed0610`, `0x142ed7710`, `0x1459c1040` — an id from the table at offset `+0x46e0`, field `+0x2c` (class
      skills);
    - `0x142f979a8` — iterating the `{set id: …}` dictionary from the item data, i.e. granting legendary properties by
      `ability_set_random`;
    - `0x142d8b740` — a BP wrapper (`GetAbilitySetClass`, a helper class next to `GetItemClass`).
  - **Nobody calls `GetAbilitySetClass` as a BP function**: none of the 6398 assets
    `TGF/Equipment|Ability|Character/Hero|UI/Game/HUD`, none of the game's 472 Lua scripts.
  - The `LegendaryAffix_Awake_*` sets (101–106) are mentioned only in `UeSource_AbilitySet`; nothing in tables or scripts
    references them.
  - Result: an item's mechanic comes only from `legendary_rule` in the item data. 181056001 doesn't have it.
- By the files this is how the original is (a bug or an unfinished item). Not checked in the game: whether the staff has a beam on
  holding LMB.
- **How to fix (user's decision):**
  - minimum — `WID_Staff_6001` with parent `WID_Staff_Fire` (blueprint edit, pak);
  - mechanic — a `legendary_rule` row, e.g. `181055001` (AoE beam explosion) or a new ability set.

## What can be done (user's decision)
1. Change nothing: that's how the original was.
2. Give some of the 66 a mechanic. For example, assign them an existing set from the 27 with a `legendary_rule` row
   in the season's `affix_template` (the backend reads it; whether the battle server does the same is not checked). New mechanics need their own
   abilities (see `research\new_abilities`).
