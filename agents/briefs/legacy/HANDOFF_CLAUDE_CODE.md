# Context handoff: Dungeonborne emulator (Rust backend) — MINOR affixes

Read this whole file before starting. Answer rules: code changes as BEFORE/AFTER blocks, technical, no filler,
compact logs (line ≤ 300 bytes, no per-tick spam).

## Project
- Rust backend (this folder): `core/backend/src/main.rs`, `core/backend/src/gateway.rs` (TGCP 0x4013 -> RpcFrame -> `game_data::route_rpc`),
  `core/backend/src/rpc/handlers.rs` (handle_* delegates → `src/rpc/handlers/<domain>.rs`, affixes — `handlers/affix.rs`), `core/backend/src/dir.rs` (DirServer 8085), `core/backend/src/tgcp/*`, SurrealDB database.
- The server reads tables from `core/backend/ExcelConfig/*.bytes` (`GameData::load_all("ExcelConfig")`, errors are silent = empty tables!).
  JSON copies of the tables: `ExcelConfigjson/`. Original .bytes: `core/backend/assets/excel/ExcelConfig/`.
- Protobuf schemas: `Configure.pb` (tables), `Network.pb` (network). Cargo already includes `prost-reflect` (serde).
- UE4SS mods: `game/ue4ss/Mods/DungeonborneRuClient/Scripts/main.lua`, `game/ue4ss/Mods/CrashTracer/Scripts/main.lua`.
- Reference: the PVE patch in `DungeonbornePVE/` (SoloDir.exe, SoloLobby.exe in Go, LoginService.exe; save `DungeonbornePVE/存档/gateway.db` sqlite,
  table roles.data = JSON DUserRole). In PVE, affixes (including MINOR) work natively.

## Established facts
1. The new dump of the affix_template/affix_value_template/affix_pool tables is identical to the original 1:1 (694/891/263 rows).
   The client has NO 100xxx (MINOR) pools, and PVE works anyway -> pools are not needed, no need to patch ExcelAffixPool.
2. Native format of DItemData.affix_data (from the PVE gateway.db):
   - BASIC (5): ONLY fixed_rule attributes with `offset` (in practice 1101 on weapons with damage), `{random_level}` without group_index/pool.
     Armor/shields/jewelry have no BASIC entry at all.
   - MAJOR (10): one entry, all attributes of the MAJOR pool; each `{random_level 0..9999, affix_pool_value:true, group_index:1}`;
     gear_set_* -> gear_set_random.
   - MINOR (20): a SEPARATE DBattleAffixRandomData for each random_rule with a non-empty pool_random, 1 attribute each,
     `{random_level 0..9999, affix_pool_value:true, group_index:1}` (group_index = index in pool_random, not the rule number).
   - LEGENDARY: ability_set_random {id: {random_level, legendary_affix_id}}.
   - random_level: value = min + floor(rl*n/10000)*unit, n = (max-min)/unit + 1 (uniform over n values, the maximum is reachable; verified on the DS 05.11, tools/stat_watch.py).
3. `handlers.rs::build_item_affix_data` has already been rewritten for this format (affix_roll, affix_schema_ver=5).
   Not done yet: BASIC only by offset (item 2) and a simpler rebuild condition (schema 6) — see the last answer in the chat.
4. PVE match flow: `UBattleService:StartLocalBattle(map)` -> the client itself sends `battle_fetch_battle_user_data`
   -> the game natively creates items from deploy_role.wearing/backpack. Lua in PVE does NOT touch DItemBuffer/OnRep_DItemBuffer.
   On our side: CrashTracer still calls OnRep_DItemBuffer/AffixDisplayConfigAttached (resets HasAdded) — remove it.
5. User: "it did not work" — the problem is in our backend. PVE and we use the same transport (tgcp/gateway/dir/rpc).

## Current task
Compare which RPCs/pushes PVE SoloLobby sends (especially when the launcher cheat menu grants an item) and what our server sends.
Proposed additions to `main.rs`: `rpc_pool`, `dump_rpc`, `rpc_dump_enabled`, `run_pve_sniffer`, `sniff_tgcp_frames`
(decode any body via Network.pb through prost-reflect into JSON -> `rpc_dump/<PVE|OUR>/NNNNN_DIR_method.json/.bin`),
mode `PVE_SNIFF=127.0.0.1` (our DirServer 8085 -> proxy 7601..7604 -> SoloLobby 6601..6604),
in `config.rs` a `public_gateway_port` field, in `dir.rs` use it instead of the hardcoded 6601,
in `gateway.rs` calls `crate::dump_rpc("OUR", ...)` when `RPC_DUMP=1`.
The code has not been compiled (no access to crates.io) — first `cargo build` and fix the errors.

Running the PVE capture (PowerShell admin): LoginService.exe (-https 127.0.0.1:443 -http 127.0.0.1:80 -certs <login> -out %TEMP%\db_solo_login),
SoloLobby.exe -data "<PVE>\存档", do NOT start SoloDir.exe, then `PVE_SNIFF=127.0.0.1 cargo run --release`, then Dungeonborne.exe directly.
If the proxy sees no connections — the game goes to 6601 via hosts: check `SoloLobby.exe -h` (port flag), run the lobby on 16601,
`PVE_SNIFF_LISTEN=6601 PVE_SNIFF_UPSTREAM=16601`.

## Next step after the dump
Find the PVE packet with MINOR (in the console `| affix ... MIN>0`), compare the JSON of the same method PVE vs OUR, bring our handler to the PVE format.
