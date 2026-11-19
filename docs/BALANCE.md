# Balance: how to edit the tables

> **Paths (since 14.11).** The document is written with paths from the backups root. In the workspace: `balance\` → `content/balance/tables`, `balance_csv\` → `content/balance/csv`, `balance_kit\` → `content/balance/kit`, `balance_pak\` → `game/paks/balance`, `seasons\` → `content/seasons`, `loot\` → `content/loot`, `names\` → `content/names`, `ExcelConfig\` → `core/backend/ExcelConfig`; `cargo xtask` is run from `core/backend` and works from the workspace root. Full table — `CLAUDE.md`.


The source of truth is `content\balance\tables\*.json` (113 tables, server base) and the season tables `seasons\<id>\balance\*.json` on top of it.
Everything else is built from them with `cargo xtask`.

## Seasons: original → base → season
| Layer | Where | What |
|---|---|---|
| game original | `core\backend\assets\excel\ExcelConfig\*.bytes` | not edited; the pak and the player kit are compared against it |
| server base | `content\balance\tables\*.json` | all tables; edits shared by any season: `affix_pool` and PVE loot (`loot*` from `content\loot\loot.toml`, without season rules) |
| season | `seasons\<id>\balance\*.json` | only the tables the season changes, in full (Dead Money: loot and vendor) |

The effective table = the seasonal one if present, otherwise the base one. The active season is `content\seasons\active.txt`:
```
cargo xtask season                        # seasons, their tables, which one is active
cargo xtask season use playtest           # active season (none — base only); then balance build
cargo xtask season new <id> [--title="…"] [--loot]  # season skeleton: balance\ (empty), kit.toml, README.md;
                                          #   --loot — own loot\loot.toml (copy of the base template)
```
CSV with a season: `balance csv` / `csv-long` export the effective tables to `seasons\<id>\balance_csv\` (the season's own ones
are marked `[season]`), `balance csv-import` writes to `seasons\<id>\balance\`: a table the season did not have yet
stays in the season only if the CSV actually changes it. Without changes the file is not rewritten (in the base either).
`balance build`, `balance kit`, `loot build/show` take the active season; `--season=<id>` — another one, `--season=none` — the base.
`balance build` with a season also puts the result into the season folder: `seasons\<id>\pak\zzzzzz_Balance_P.pak` and
`seasons\<id>\server\ExcelConfig\` (tables that differ from the original). `balance patch` writes into the patch's season (field `season`):
the first edit of a table in a season — a copy of the base one in `seasons\<id>\balance\`, the base does not change.
For a season with its own `content\loot\loot.toml`, `loot build` takes its template and writes to `seasons\<id>\balance\`.
`balance export` refuses when a season is active (`ExcelConfig` also contains the season — it would end up in the base).

```
cargo xtask balance csv [table…]          # balance\*.json -> balance_csv\*.csv (column = field; no names — all)
cargo xtask balance csv-long [table…]     # balance\*.json -> balance_csv\*.long.csv (row = one value)
cargo xtask balance csv-import [table…]   # balance_csv\*.csv / *.long.csv -> balance\*.json (no names — all)
cargo xtask balance names-import [file] [--lang=en]  # localisation from the game (F6, NameDump) -> names\strings.tsv (en: strings_en.tsv)
cargo xtask balance build                 # balance\*.json (+ season) -> ExcelConfig\*.bytes (server) + balance_pak\zzzzzz_Balance_P.pak (client)
cargo xtask balance build --deploy        # + install the pak into all game installs: Steam, F:, E: (game closed;
                                          #   if it is running — the pak is built but NOT installed: check the output)
cargo xtask balance verify                # self-check: .bytes -> JSON -> .bytes without changes
cargo xtask balance export                # ExcelConfig -> balance (OVERWRITES the edits in balance\!)
cargo xtask balance dump <folder> <out>   # any .bytes (e.g. an unpacked mod pak) -> JSON, for comparison only
```

To unpack a mod pak for `dump`: `DungeonbornePVE\files\Paks\repak.exe unpack -s ../../../ -o <folder> <pak>`.

After `build` restart the server — the tables are read at startup.

## Kit for players (application, Excel, LLM, without our code)
The kit is opened by the Balance Studio application (`<data>\balance-studio`, releases in `balance_studio_release`), it
saves the same kind of patch. Newer kits contain English texts: `title_en`, `about_en`, `where_en`, `notes_en` in `kit.toml`.
`cargo xtask balance kit` — the kit of the active season in `seasons\<id>\kit\<id>-balance-…\` (`--season=none` — the base,
`balance_kit\out\dungeonborne-balance-…\`): `balance.xlsx`, `llm\`, Python scripts. Next to the current values — the game
original: orange cells (original in the cell note), the "Differences" sheet, the section "Differences from the game original" in `llm\<table>.md`.
Players send a patch (`make_patch.py` or an LLM), we run: `cargo xtask balance patch <patch> --dry-run` (a "before → after" list,
warnings; errors stop the patch, rules — `core\backend\crates\balance-kit`), then without `--dry-run` and `balance build`. Tables and what can be edited in them — `content\balance\kit\kit.toml` (shared) + `seasons\<id>\kit.toml`
(season: title, notes, own `[[table]]`); details — `content\balance\kit\README.md`.

## Editing in Excel (CSV)

1. `cargo xtask balance csv role_level_upgrade`
2. Open `balance_csv\role_level_upgrade.csv` in Excel, edit, save **as CSV** (the same file).
3. `cargo xtask balance csv-import role_level_upgrade` → shows how many rows changed.
4. `cargo xtask balance build`

Example (`role_level_upgrade.csv`, class stats by level):

```
role_type_id;role_level;role_attribute.first_level_strength.fixed_value;role_attribute.first_level_dexterity.fixed_value;…
499999;1;0;0;…
400000;1;29;6;…
400000;2;30;6;…
```

- Columns are field paths joined by dots. Nested single messages are expanded into columns; **lists and maps —
  one JSON cell** (`[1,2]`, `{"3":{"min_value":8,"max_value":12}}`). For such tables — the long layout (below).

### Long layout (`csv-long`) — for nested tables

One CSV row — one value. Example `content\balance\csv\affix_value_template.long.csv` (affix ranges by rarity):

```
row;key;title;path;value;name
0;101;Strength;attribute_type;1001;Strength
0;101;Strength;data.affix_data[0].affix_type;EN_BATTLE_AFFIX_TYPE_MAJOR;
0;101;Strength;data.affix_data[0].rarity_range[3].min_value;8;Uncommon
0;101;Strength;data.affix_data[0].rarity_range[3].max_value;12;Uncommon
```

- `row` — the table row number (from 0). `key` (the first field of the row), `title`, `name` — labels for filtering, not read on import.
- `path`: fields joined by `.`, `[i]` — a list element, `[key]` — a map element (here `rarity_range[3]` = rarity 3 = UNCOMMON, "Uncommon").
- In Excel: Data → Filter, filter `path` by "contains max_value" etc. Edit only `value`.
- New value — add a row with `row` and `path`. Delete a value (reset to default) — clear `value`.
- Keep one layout per table: `csv` deletes `.long.csv` and vice versa; if both are present, `csv-import` refuses.
- Empty cell = default value (0 / false / "" / field absent).
- Columns exist only for fields that occur in at least one row. New field — add a column with its path.
- Format for Russian Excel: separator `;`, decimal `,`, UTF-8 with BOM. `--plain` — `,` and `.` (LibreOffice, VS Code).
  On import the separator is detected automatically, any decimal mark is accepted; the Russian Excel words for TRUE/FALSE too.
- Excel corrupts integers longer than 15 digits (turns them into `1,23E+17`) — edit such tables in JSON.
- `csv-import` replaces `rows` in `balance\<table>.json` entirely. Do not edit one table in JSON and CSV at the same time.

## Loot (template `content\loot\loot.toml`)

The four loot tables (`loot`, `loot_rate`, `loot_choose_group_rate`, `loot_ue_source_choose_group`) are easier to edit
not separately, but as one template:

```
cargo xtask loot export [JSON folder] [--force] [--maps=all|0,10101,…]
                                                # tables -> loot\loot.toml (by default from balance\; --force — overwrite;
                                                # by default only maps from our map_detail + 0)
cargo xtask loot show [--map=10103]             # average loot of groups (of this map only) with [rules] applied, writes nothing
cargo xtask loot build                          # loot\loot.toml -> balance\loot*.json
cargo xtask loot build --deploy                 # + server and client pak (only loot tables go into the pak; game closed)
cargo xtask loot verify-view [--patch=file]     # application calculation (balance-kit) vs the table build, writes nothing
```

Designers edit the template through the kit: tables `lt_slots`, `lt_items`, `lt_rules` (`content/balance/kit/kit.toml`, `source = "loot_template"`).
`cargo xtask balance patch` changes the numbers in the template, comments stay, then it runs `loot build` itself.

How loot works: a **source** (chest/monster/vase) → several **slots** → each slot drops independently with chance
`chance` % → picks a **pool** → **one** item drops from the pool by the weights `w`.

```toml
[rules]
remove = ["equipment"]          # remove all equipment (item ids work too: [100013])
chance = { treasure = 2.0 }     # chance of treasure slots ×2 (not above 100%)
weight = { treasure = 3.0 }     # treasures 3 times more often in mixed pools
count  = { gold = 2.0 }         # twice as much gold

[groups.g01]
name = "Barrels, crates, pots"  # label, can be renamed
sources = ["BP_Barrel_01_C", …] # BP name from loot_source_map or a number
slots = [
  { id = 910001, pool = 900001, chance = 35 },   # Gold ×3–12
  { id = 910002, pool = 900008, chance = 12 },   # potions
]

[pools.900001]
name = "Gold ×3–12"
items = [
  { id = 100069, cnt = [3, 5, 6, 8, 10, 12], w = 30000 },  # Gold [gold]
]
```

- Categories for `[rules]`: `gold weapon armor jewelry gem treasure potion throwable supply currency other`, groups
  `equipment` (weapons+armour+jewellery), `consumable` (potions+throwables+supplies), `all`. The category comes from `item_type`
  (the `[treasure]` comment on each pool line). `chance` multiplies the slot chance by the category's share in its pool.
- New pool — `[pools.my_name]` with any name (the id is assigned automatically), in the slot `pool = "my_name"`. New slot — without `id`.
  A slot repeated twice — two rolls. A choice from several pools: `pools = [900001, 900002], pr = [70, 30]`.
- `w` — relative weight, on build the pool is normalised to a sum of 30000 (as in the game). `cnt = [3, 5]` — one of the values
  with equal probability. An item can be given by name: `{ id = "Gold" }` (the localised name; if the name is ambiguous, `build` asks for an id).
- A pool that became empty after `remove`, and the slots with it, simply disappear; `loot show` / `build` report how many.
- **Drop quality** `quality` (in `[rules]` or a mode): the weight of an item that has a price
  (`item_type.system_repurchase_price`, for treasures — from 10 to 320 000) is multiplied by
  `(price / geometric mean price of the pool's items) ^ quality`. 0 — no change, 0.5 — expensive ones noticeably more often,
  1 — proportional to price, < 0 — cheap ones more often. Consumables and others without a price do not change. The slot chance is not affected.
- **Modes** `[modes.<name>]`: `maps = [..]` + the same fields as in `[rules]` (`remove/chance/weight/count/quality`),
  apply only on these maps on top of `[rules]`. Groups are split by map: the part without the mode — as before, the mode's part
  — with its own slots and pools (a pool the mode changes is built separately with a new id; an unchanged one is shared).
  In `loot show` such groups are marked `g099@no_gear`. A source that has no slots left in the mode stays in the
  table with an empty list — nothing drops from it.
  Currently: `[modes.no_gear]` — Crypt 10101/10102/10103 without equipment and gold, `quality = 0.5`.
- **Match loot map** — the field `UBattleDsUtility+0x1d4` (not `GameState.MapId`). On listen RuServer sets it via
  dbnative `op=lootmap` (since 08.11, `[LOOTMAP]` lines; before that always 0 — every match took the loot of map 0). docs/LEYLINE_SEALS.md.
- **Loot per map.** `maps = [10101, 10201]` — the group applies on these maps (`map_detail.loot_map_id`:
  10101/10102/10103 — Crypt normal/Classic/Hard, 10201/10202/10203 — Castle; 1–18 — old ids from `map_detail`,
  neither we nor PVE have loot for them; the full list of maps — `content\loot\maps_report.txt`).
  Without `maps` — map 0 (default loot). `loot_types = [0, 1, …]` — the map's loot variants (in PVE 1–5 ≈ copies of 0,
  3/4 — with altars `BP_Altar_Special_*`). A table row is created for each pair (map, type); one source on
  one pair — in only one group. On export group names start with the maps: "Crypt Hard: …", "all maps: …" (the labels are in Russian in the export).
  `host_from_lobby` always opens `L_Dungeon_Official_01_Classic` = **10102** (Crypt Classic).
- 04.11: `content\loot\loot.toml` — the framework base, PVE loot without season rules (`remove = []`, no `[modes]`); Dead Money rules —
  in `seasons\dead-money\loot\loot.toml`. The loot of `core\backend\assets\excel` (18 `loot` rows) was not taken as the base: for players it is overridden by `zz_Loot_P`.
- Currently (30.10) `content\loot\loot.toml` has the PVE loot (`zz_SoloLoot_9_P`) for our maps: 159 groups, 136 pools, 10916 source
  rows; checked by script — every pair (source, map, type) matches PVE. Our previous loot (without maps) —
  `content\loot\loot_ours.toml`: to restore — copy it over `loot.toml` and `loot build --deploy`.
- PVE sources that are not in our `loot_source_map` (`BP_REA_Tier1–5_chest_wood/iron/gold`, `BP_REA_Tier1–5_goblin_chest`,
  `BP_REA_Probe1–7`, ids 970001–970020 and 990001–990007) are written as numbers. **Checked 08.11: they are not in the game** — not a single
  such asset in the client and server paks, so they cannot be on maps and no loot drops from them. The names were added
  only in `loot_source_map.bytes` of the PVE pak `zz_SoloLoot_9_P` (333 names vs 305 in the game); the PVE build does not bring
  the blueprints themselves. Groups g040–g063 (Tier5 — unique weapons, Tier4 — legendary) are dead.
- The `no_source` group — tutorial slots that no source references (the game may call them by id). Do not delete.
- Without edits, `loot export` → `loot build` gives byte-for-byte the same tables (checked 30.10).
- Importing someone else's loot: unpack the pak, `balance dump <folder> <json>`, then `loot export <json> --force`
  (source names are taken from `loot_source_map` of the same export, on build — from our `balance\`).

## Names instead of numbers (labels)

Next to an id the CSV writes a name — **read-only**, ignored on import (the number goes into the game).

| Layout | Where the label is | Example |
|---|---|---|
| ordinary CSV | the column `<column>#name` to the right of the id | `item_id;item_id#name` → `181010000;Novice Sword` |
| long CSV | `title` — the row name, `name` — the names of keys/values in the path | `Strength … rarity_range[3].max_value;12;Uncommon` |

What gets labelled (rules by field name — `core\backend\xtask\src\names.rs`, `REF_FIELDS` / `MAP_KEY_FIELDS`):

| Field | Name | Source |
|---|---|---|
| `attribute_type`, keys of `attribute_random` | attribute (`first_level_strength` / "Strength") | field number of `DBattleAttributeOffset`; localisation via `battle_attribute_description.name` |
| keys of `rarity_range`, `rarity`, `tiers_rarity`… | rarity ("Uncommon" = 3 = UNCOMMON) | `GeneralStrings Rarity_N`, otherwise the enum `EnItemRarity` |
| `role_type_id`, `career_type_id` | class (`FIGHTER/HUMAN`) | `role_type` |
| `type_id`, `item_id`, `ue_source_type_id`… | item (name from localisation, otherwise the type `EQUIPMENT_WEAPON`) | `item_type` |
| `ability_set_id`, `affix_alias_id`, `legendary_affix_id` | legendary | `battle_ability_set_description` |

### Names from the game (localisation)

Without a dump items are labelled only by type. To get the real names (in the client's language):

1. The UE4SS mod `NameDump` is already installed (`game\ue4ss\Mods\NameDump`, `mods.txt`: `NameDump : 1`).
2. Start the game, in the **lobby** press **F6** → `%LOCALAPPDATA%\Dungeonborne\Saved\names_dump.tsv`
   (in the UE4SS console: `[NameDump] tables N (empty …), strings M, …`).
3. `cargo xtask balance names-import` → `content\names\strings.tsv`.
4. `cargo xtask balance csv` / `csv-long …` — the labels will be taken from the localisation.

**English labels for the balance kit** ("Strength" next to the Russian name): the same with the client in English (language in the game
settings or launch with `-culture=en`), F6 in the lobby, then `cargo xtask balance names-import --lang=en` →
`content\names\strings_en.tsv` (the Russian `strings.tsv` is not touched) and `cargo xtask balance kit`. The import checks the language
of the dump: a Russian dump will not be written as `--lang=en`.

## Format of `balance\<table>.json`

```json
{
  "type": "tgf.config.ExcelAffixLegendaryRule",   // row type from Configure.pb — do not touch
  "header": { ... },                              // xresloader header — do not touch (count/hash are recalculated)
  "rows": [ { "affix_alias_id": 181055003, "offset": { "ability_set_id": 181055003, "min_value": 1, "max_value": 1 } } ]
}
```

- Field names and enum values — as in the schema (`"EN_BATTLE_AFFIX_TYPE_MINOR"`, not a number); fields in schema order,
  map keys sorted (numbers — by value). The export is deterministic — convenient for comparing versions.
- Fields with the default value are not written — they can be added by hand. `float` is written in short form (`0.1`).
- An unknown field or a wrong type → `build` prints `<table>: row #N (id=…): …` and does not build this table
  (the server and the pak keep the old version of the table). UTF-8 with BOM (Notepad) is supported.

## What goes where

| Where | What |
|---|---|
| `core\backend\ExcelConfig\*.bytes` (server) | all tables; a file is rewritten only if its rows changed |
| `zzzzzz_Balance_P.pak` (client, `Content\AdditionalData\ExcelConfig\`) | only tables that differ from the game originals `core\backend\assets\excel\ExcelConfig` |

Unchanged rows are written with the original bytes — without edits the files match the original byte for byte (`verify`).
The pak `zzzzzz_…` (6×z) loads last and overrides `zzzzz_RebuildPrivate_P` (it has its own `affix_pool`/`loot`, see BACKLOG).

## Structure

- `xtask\` — a workspace crate, run as `cargo xtask …` (alias in `.cargo\config.toml`).
  - `tables.rs` — .bytes ↔ JSON via `prost-reflect` using `Configure.pb`
    (`org.xresloader.pb.xresloader_datablocks`: `header` + `data_block[]` + `data_message_type`; `hash_code` = sha256 of the concatenated rows).
  - `csv_view.rs` — CSV ↔ JSON per the schema.
  - `main.rs` — commands, pak build `repak pack` (V8B, `../../../`, no encryption), installation.
- `core\backend\crates\balance-kit\` — the kit and patches without the game schema: patch validation rules shared by the server and the application for
  designers (`docs\BALANCE_APP_SPEC.md`). The same rules — in `content\balance\kit\tools\check.py`; shared examples
  `tests\fixtures\cases.json`, `cargo test -p balance-kit` runs both implementations.
- `XTASK_ROOT=<folder>` — `xtask` works with a copy of the project, not with the working tables (acceptance `balance-studio\tools\accept`).
