# Framework: base game + pluggable seasons

> **Obsolete since 07.11.** The current season format is [docs/SEASONS_FORMAT.md](SEASONS_FORMAT.md) (`core/backend/src/season.rs`,
> `SEASON=<id>`, flags in `seasons/<id>/season.toml`). Dead Money was removed. Below is the description of the separate copy
> `<data>\framework` with season modules: a sample of hooks for the future, not current code.

The base code (backend `src/`, UE4SS mods, LocalDev/BPFix paks) is the **core**: a listen server acting as dedicated, per-player
data, raids, loot extraction, experience. The core knows nothing about specific events. A **season** is a folder in `seasons/`
plus a backend module that the core calls at several points of a player's life. Without a season the base game runs.

Working copy: `<data>\framework` (separate from `backups`, no git). Build as usual: `cargo build`,
`cargo xtask …` from that folder.

## Layers

| Layer | Where | Whose |
|---|---|---|
| backend: protocol, lobby, inventory, raids, DS | `src/` except `src/season/` | core |
| season API: `Season` trait, loading, table overrides | `src/season/mod.rs` | core |
| season rules | `src/season/<module>.rs` | season |
| season settings, tables, paks, loot | `seasons/<id>/` | season |
| game hooks (UE4SS): DungeonborneRuClient / RuServer / R14 | `game/ue4ss/Mods` | core (the same for all seasons) |
| game scripts (UnLua) and the R14 patch | paks `zzzzz_RebuildPrivate_P`, `zzzzzzz_BPFix_P` | core |
| season game tables | season pak `seasons/<id>/pak/` | season |

## Season folder

```
seasons/<id>/
  season.toml            manifest (required)
  <config>               module settings (name in the manifest, format decided by the module)
  server/ExcelConfig/    backend tables instead of the base ExcelConfig/ (only changed files, names as in ExcelConfig/)
  pak/                   game paks (.pak/.ucas/.utoc): installed on the server and on all clients
  loot/loot.toml         season loot source (pak/ and server/ExcelConfig/ are built from it)
  <id>.md                season description
```

`season.toml`:

```toml
id = "dead-money"
name = "Dead Money"
plugin = "dead_money"        # backend module from the registry src/season/mod.rs → create()
config = "deadmoney.toml"    # module settings file in the season folder
```

## Enabling

```
cargo xtask season list                    # which seasons exist
cargo xtask season deploy <id>             # season paks into all game installs (game closed)
SEASON=<id>  target\debug\dbserver-rs.exe  # backend with a season (SEASONS_DIR — another root instead of seasons/)
```

Without `SEASON` (or if the manifest/config cannot be read) the backend logs the reason and starts the base game.
To return the base balance pak to the installs: `cargo xtask balance build --deploy`.
Clients get the same files from `seasons/<id>/pak/` into `Content\Paks` (the installer `dist\…\install.ps1` copies
all `.pak/.ucas/.utoc` from its `Paks` folder).

Startup journal:
```
[Season] active "Dead Money" (dead-money, module dead_money, folder seasons\dead-money)
[Season] table ExcelConfig/loot.bytes -> seasons\dead-money\server\ExcelConfig\loot.bytes
```

## Hooks (`trait Season`)

All hooks have an empty default implementation — a season overrides only the ones it needs. `s` is the request session
(`GatewaySession`: database, tables `s.game_data`, the services below), `p` is the player.

| Hook | When the core calls it | Where in the core | Example (Dead Money) |
|---|---|---|---|
| `on_level_select(s, p, &mut lvl)` | the player picked a map in the lobby, before saving | `battle_handlers.rs` `level_select_impl` | closed stage → the highest open one |
| `on_raid_prepare(s, p) -> bool` | before a raid, before the match data is assembled: `local_battle_user_data` (own and `/ds/battle_user_data` for each player of the listen server), `FETCH_LOADOUT` of a connecting player. `true` — the player was changed, the core saves and writes the loadout | `battle_handlers.rs`, `raid_handlers.rs` | stage kit for the selected map |
| `extracted_item_price(s, type_id) -> Option<i64>` | selling extracted loot (`settle_extraction`), for each foreign item in the bags. `None` — normal price, `Some(≤0)` — goes away without gold | `raid_handlers.rs` | gear = 0 |
| `on_extraction(s, p, role_guid, gold)` | after selling the loot on raid extraction | `raid_handlers.rs` `settle_extraction` | earnings → unlocking stages |
| `on_death(s, p, role_guid) -> bool` | death in a raid (LocalSettle status=1, PrivateRaid dead). `true` — save | `raid_handlers.rs` | a new stage kit |

A season keeps its own progress in `p.state["<season key>"]` (Dead Money — `state.deadmoney`); the core does not touch this key.

## Core services for seasons

| What | Call |
|---|---|
| class kit up to a tier with random affixes (equipped items are replaced) | `s.issue_class_kit(p, role_guid, tier) -> Option<Vec<i32>>` |
| give an item into a bag (stacks are topped up) | `s.inv_give(p, type_id, count, &pos, &mut ItemChanges::default())`, positions `BACKPACK`, `SMALL_BACKPACK` (`crate::rpc::handlers`) |
| is the item gear? | `s.is_equipment(type_id)` |
| normal sell price | `s.item_sell_price(type_id)` |
| item of the same slot of another tier | `s.stage_kit_item_id(base_item, tier)` |
| item affixes with a given seed | `s.build_item_affix_data_seeded(item_id, wear_part, seed)` |
| map selected in the lobby | `GatewaySession::get_saved_level_select(p)` |
| game tables (with season overrides) | `s.game_data.<table>` |

## New season

1. `src/season/<module>.rs`:
   ```rust
   use super::{Manifest, Season};
   pub struct MySeason { id: String, name: String /* , cfg … */ }
   impl MySeason {
       pub fn load(m: &Manifest, dir: &std::path::Path) -> Result<Self, String> {
           // read dir.join(m.config…) — an error = the season will not be enabled, the backend starts the base game
           Ok(Self { id: m.id.clone(), name: m.name.clone() })
       }
   }
   impl Season for MySeason {
       fn id(&self) -> &str { &self.id }
       fn name(&self) -> &str { &self.name }
       // the hooks you need
   }
   ```
2. In `src/season/mod.rs`: `pub mod <module>;` and a line in `create()`: `"<plugin>" => <module>::MySeason::load(m, dir).map(…)`.
3. `seasons/<id>/season.toml` (+ config). Own loot: `content/loot/loot.toml` → copy into the root `content\loot\loot.toml`,
   `cargo xtask loot build` → `game\paks\balance\zzzzzz_Balance_P.pak` into `seasons/<id>/pak/`, the changed
   `ExcelConfig\loot*.bytes` into `seasons/<id>/server/ExcelConfig/`.
4. `cargo build`, `cargo xtask season deploy <id>`, `SEASON=<id> …`.

A module that is not needed (e.g. a season with only different loot) is optional: any existing `plugin`
with its rules disabled will do, but it is simpler to create an empty module with the two methods `id`/`name`.

## What is not seasonal yet

- **The headless server map** is set at launch (`dbtest\gui2.ps1 -Map …`), not by the season: each season map gets
  its own server (`DS_POOL`).
- **UE4SS and LocalDev mods** are shared by all seasons; if a season needs its own game hook — for now via a core change.
- **One active season** per backend; progress of different seasons in `state` does not overlap (different keys).
- The seasonal loot pak carries the name of the base balance pak (`zzzzzz_Balance_P`) and replaces it in the install.

## Differences from `backups`

- `src/deadmoney.rs` → `src/season/dead_money.rs` (the config is no longer global, it lives in the season).
- The variables `DEADMONEY`, `DEADMONEY_CONFIG` and the root `deadmoney.toml` are not read — only `SEASON=dead-money`
  (disable in the config: `enabled = false`).
- Core: hooks instead of direct deadmoney calls in `battle_handlers.rs` / `raid_handlers.rs`; `issue_stage_kit`
  split into `issue_class_kit` (core) and consumables/progress (season); `inv_give`, `ItemChanges`, `BACKPACK`,
  `SMALL_BACKPACK`, `is_equipment`, `item_sell_price` are exposed to seasons; `game_data.rs` reads tables via
  `season::table_override`.
- `cargo xtask season list|deploy`.
- Dead Money behaviour did not change; it has not been checked in game through the framework yet (backend start was checked: the season and
  4 loot tables are picked up, without `SEASON` — the base game).
