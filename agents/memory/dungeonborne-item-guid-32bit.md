---
name: dungeonborne-item-guid-32bit
description: Client keeps item guids in 32 bits — server must issue item guids < 2^31 via src/guid.rs; old user_id*10000 scheme broke all item RPCs
metadata:
  visibility: team
  node_type: memory
  type: project
---

Found 2026-11-02 in a live session: server issued item guid = user_id*10000+n (~1.09e11); the client truncates to
32 bits (109490200139 mod 2^32 = 2116017739) and sends that back → role_wear/role_take_off/inventory_item_move
"item not found", toast "Invalid item class or slot", trade_sales_order_publish -4212. Official server guids ≈ 1e8.

**Why:** any new code that invents its own guid range (user_id-based, base + id*100…) silently breaks the client.
**How to apply:** always allocate item guids with `crate::guid::next_item()` (range [1e8, i32::MAX], high-water mark in
DB `counters:item_guid`); treat `guid::too_big(g)` guids from old data/clients as needing a new guid. One-time migration
`counters:item_guid_migration_v1` already ran on the saves (backup `surreal_saves_before_guid_20261102`).
Role guids (user_id*100+n ≤ ~1.1e9) still fit. Status/next checks: `docs/FIX_LIST.md`. Related: [[dungeonborne-handlers-tooling]].
