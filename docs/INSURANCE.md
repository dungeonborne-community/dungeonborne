# Gear insurance: how it works in the game

Analysis of 08.11 for "as in the original". Static only: game strings, client Lua (`research\lua_all`), tables, headers and Ghidra on the exe image. We do not have the official account server, so its part (price, item return, premium sale) is reconstructed from the client and the protocol. Working files: `<data>\research\insurance`.

## Rules (game texts)
`GeneralStrings` `Guide_Description_3_8`, `dice_*`:
- Insurance costs gold ("insurance premium"), the price is based on the item's **trade price**: "Based on the trade price of {Gear_Name}, insurance will cost {Price_Num} golds (Rarity Epic or higher are more recommended)".
- An insured item is **not lost on the next death** ("one chance to avoid losing insured equipment from death during an adventure").
- It can be cancelled **at any time before entering the dungeon**, the gold is refunded in full (`dice_unlock_confirm_text`).
- The killer of an insured player gets the **insurance premium** for that item and can sell it to a vendor after extracting (`dice_loot_tips_1/3`). If the player did not die to a player, the premium also appears (`dice_loot_tips_2`, without a killer).
- The premium cannot be dropped, only put into your own inventory (`dice_loot_cannot_throw`).
- An insured item cannot be changed until the insurance is removed (`dice_locked_gear_operational_lockout_popup`).
- The higher the rarity, the better value the insurance ("higher rarity has higher cost-effectiveness").

## Client (lobby)
- Lobby inventory (`BP_LobbyInventoryMenu`, `BP_InventoryCharacterSection`): the `ButtonDice` button → mode `ETgfInventorySelectMode::Insurance`, pick an item on the character; `ButtonCancelDice` → `UnlockInsuranceItem`. The button is visible only in an inventory of type `Lobby`.
- **One item per character:** `InventoryContext:GetInsuranceItemNum()`; more than 1 — the client logs the error "InsuranceItemNum too much".
- Price: `RequestItemsInsurancePrice` → RPC `gear_insurance_price {item_id[]}` → `ScGearInsurancePriceRsp {data[{item_id, price}]}`, cache `CachedItemsInsurancePrice: TMap<type_id, price>`. **The price depends only on the item type**, not on affixes.
- Insure: `gear_insurance_lock {item, price}`; remove: `gear_insurance_unlock {item}`. On the item: `GetIsInsuranceAndGetValue` → `DItemData.insurance {active, value, cost_item, affix_data}`.

## Battle server (native, `UItemFunctionLibrary`)
| Function | exec | implementation | What it does |
|---|---|---|---|
| `LootInsuranceRewardItem(FromCharacter, FromSlot, TargetInv, …)` | `0x142d93aa0` | fragment `0x142f99c0b` | Looting an insured slot of a corpse: instead of the item a premium is created and given to the looter; the original item gets flag `+0x449` (the premium cannot be taken twice) |
| `CreateFakeInsuranceRewardItemInstance(EquipMgr, InvMgr, Slot)` | `0x142d921c0` | `0x142f8c420` | Premium item for the item in the slot (for display on the corpse and for looting) |
| `CreateInsuranceRewardItemInstance(Owner, OriginItem, BelongTo)` | `0x142d922c0` | `0x142f8c850` | Authority only. The real premium: `DItemCreateInfoInsurance {item_guid, item_id, map_id, player_info, killer_info}`, the killer comes from `BelongTo` |
| `GetInsuranceRewardItemClass(OriginItem)` | `0x142d935d0` | `0x142f929f0` | Premium class: item type → rarity → `gear_insurance_item` → 2100010–2100017 (`0x142ec94c0`) |
| Moving an item out of a slot (general loot) | — | `0x142f8ee20` | If the corpse slot is insured and the premium has not been taken — hands out the premium, not the item |

`UTgfInventoryItemInstance.InsuranceRewardsInfo` {`TimeSeconds`, `KillerName`, `MapId`, `VictimName`, `Tier`, `OriginTypeId`} — the premium tooltip (`GetInsuranceRewardTips`).

What this means: **the battle server already has everything natively.** The item just has to arrive in the match with `DItemData.insurance.active = true` (our `gear_insurance_lock` sets it). What exactly the slot check (`0x142f4ed90`) reads I did not verify — check in a match: kill an insured player and loot the slot.

## Premium price (battle server native)
The premium is built by `0x142ec4130` (called from `CreateInsuranceRewardItemInstance` `0x142f8c850`), authority only:
1. Takes the `DItem` of the original item. If it has `insurance.active = false` or `insurance.value < 1` — no premium (logged).
2. `r = ExcelConstConfig.dice_attenuate_ratio` (const, field 1801; in C++ offset `+0x52c` of the config object `[singleton+0x5f20]`), clamped to `0..1000`. In our `core/backend/ExcelConfig/const.bytes` it is **150**.
3. The premium item (type from `gear_insurance_item` by rarity, quantity 1) gets
   `DItemData.sale_price = value × (1000 − r) / 1000` — 64-bit integers, integer division, i.e. rounded **down**. With r = 150 that is exactly 85% of the payment.
4. The original item gets `insurance.value = 0` (a premium cannot be obtained from it a second time).

The client shows this number in the premium tooltip (`InsuranceRewardsInfo.SalePrice`); without it — `system_repurchase_price`. 08.11 on the stand: payment 108 000 → `sale_price` 91 800.

Evidence: offset `0x52c` = `dice_attenuate_ratio` per the `ExcelConstConfig` field offset table in the image (`0x146cd9ad0`, 241 fields, index 204); `DItemData` — table `0x146ccdec0` (`insurance` +0x38, `sale_price` +0x4c).

Not a trade tax: `trade_commodity.tax_percentage` = 25 in all 310 rows, it does not take part in the premium formula.

What follows from this:
- The premium share is **the same for all rarities** (85%); only the premium item type depends on rarity. Rarity affects the payment, and that was computed by the account server — neither the client nor the battle server has this formula.
- The premium should be sold to a vendor for the `DItemData.sale_price` of the premium itself (set by the battle server). If the field is missing — `payment × (1000 − dice_attenuate_ratio) / 1000`, rounded down.
- Original: premium 166 617 → payment 196 020 or 196 021 (both give 166 617 with rounding down). With the ring's market at 0.8–1.2 million that is 16–25%, about 20% — the hypothesis "payment ≈ market / 5" for a legendary is plausible, but it is one case. The game neither confirms nor refutes percentages for other rarities.

## What remains for the account server (backend)
1. **Price.** There is no formula in the client or the battle server; it was on the official server. Table `gear_insurance_item`: a row per rarity, `id` = premium item of that rarity, `sp_rate` (ROTTED/NORMAL 100, UNCOMMON 300, ENHANCED 1000, EPIC 4000, EXCEPTIONAL 1000, LEGENDARY/UNIQUE 200), `standard_rarity` EPIC, `standard_rate` 1000. A plausible reading (not proven): `price = item trade price × sp_rate / 10000` anchored on epic (`standard_rate` 1000 = 10%), i.e. epic 40%, legendary 2% — which gives "higher rarity — better value". Our `item_insurance_price` takes the vendor buyback price, not the trade price, and divides by `standard_rate × 10` — which is why it comes out at about 2 gold.
2. **Restrictions:** one insured item per character; insure and remove only in the lobby, not in a match; an insured item cannot be moved until the insurance is removed.
3. **Death of an insured player:** the item is not lost — return it to the owner (to the character or the stash) and close the insurance (`active = false`, "one chance"). The battle server does not give it out from the corpse; only the premium is looted.
4. **Premium for the killer:** arrives in the result of their raid as item 2100010–2100017 with `create_info.insurance.item_guid` of the original item. Premium items have no price in `item_type`: selling to a vendor is for the premium's `DItemData.sale_price` = 85% of the victim's payment (section "Premium price"). There is no buyback ("once sold, it cannot be repurchased").
5. **Removing insurance:** full refund of `value`.

`dice_attenuate_ratio` = 150 is the share cut from the premium (section "Premium price"); earlier I mistakenly took it for an old dice constant.

## Our formulas (since 08.11, commit 1e945d6)
We consider the `gear_insurance_item` table (`sp_rate`) a developer mistake and do not use it. The price is a percentage of the item's **market** price,
as in the original: the legendary ring "Lament of the Fallen" at 0.8–1.2 million on the market, insurance 166 617 ≈ 20%.

**Market price of an item** (`insurance_market_prices`, `trade_handlers.rs`), by item type:
1. average deal price over 7 days: `Σ(price × qty) / Σ qty`, first Trade (market 1), if there are no deals — the auction (market 2);
2. no deals in either: `price_def × market_mult` (base trade price from the tables, `market_mult` = 3 — in the original
   the market is about three times more expensive than base prices, a green item ≈ 3 000);
3. the item is not tradable — 0, cannot be insured.

**Insurance price** (`item_insurance_price`, `item_handlers.rs`):
`insurance = market price × rarity percentage / 100` (integer, at least 1). Not gear or percentage 0 — cannot be insured.

| Rarity | Percentage |
|---|---|
| ROTTED, NORMAL (grey) | 0 — not insurable |
| UNCOMMON | 35 |
| ENHANCED | 30 |
| EPIC | 25 |
| EXCEPTIONAL | 22 |
| LEGENDARY | 20 |
| UNIQUE | 15 |

Configured in the season, `[features.insurance]` (`core/backend/src/season.rs`, `Insurance`); without the section — the values from the table and
`market_mult = 3.0`. Example:
```toml
[features.insurance]
market_mult = 3.0
rotted = 0
normal = 0
uncommon = 35
enhanced = 30
epic = 25
exceptional = 22
legendary = 20
unique = 15
```

**Examples** (ring 185015002, legendary, `price_def` 180 000):
- no deals over the week: 180 000 × 3 = 540 000 → insurance 540 000 × 20% = **108 000** (verified in game 08.11);
- average deal 1 000 000 → insurance **200 000**.
- original: premium 166 617 = 196 020 × 85% → payment 196 020 ≈ 20% of the market ~980 000 (the ring cost 0.8–1.2 million).

**Premium for the killer** (`shop_handlers.rs`, `insurance_premium_prices`): sold to a vendor for the `DItemData.sale_price` of the premium itself —
it is set by the battle server (section "Premium price"), and the game shows the same number in the tooltip. If the field is missing, the backend computes the same way:
`payment × (1000 − dice_attenuate_ratio) / 1000`, rounded down (const, 150 in the table → 85%; `insurance_premium_sale`). The payment is the amount
the item owner paid (an `InsurancePaid` record by `item_guid` from the premium's `create_info`); no record — the current insurance price
for that type. The premium is not added to vendor buyback. Stand 08.11: payment 108 000 → premium 91 800 (before the fix the backend
paid the full payment). Looks like anti-RMT: the premium is smaller than the payment. The premium percentage changes only via the const
`dice_attenuate_ratio` in the balance (then both the battle server and the vendor compute it the same way).

**Verified in game 08.11:** owner's insured ring stayed with the owner after death; the killer got the premium.

**Restrictions:** one insured item per character (`EnErrGearInsuranceRepeated`); on death the item stays with the owner
in the same slot, the insurance is removed, and the payment record is kept for selling the premium; removing it before a match — full refund.
