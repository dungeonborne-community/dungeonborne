# Balance Studio: changes

## 1.2.0 — 06.11.2026

### For designers
- **"Loot" screen:** map and source type, a group with its slots and pools, chance, weight and count right in the card, the item's share,
  the average drop per source and what the kit had.
- **Season rules:** you see what they remove and multiply. The value in the field stays the template's value, the result is shown beside it.
- **Shared pools:** the card warns that changing an item changes other groups too.
- **New tables** in the "Loot" section: slots, pool items, rules and modes. Bulk edit and pasting from Excel work here too.
- **New checks:** chance from 0 to 100, weight not below 0, count not below 1, at least one item stays in the pool.

### For the server owner
- The kit exports the loot template: `data/lt_*.csv` and `raw/loot_template.json`. The `lt_*` tables are described in `kit.toml` with the
  field `source = "loot_template"`.
- `cargo xtask balance patch` edits `loot.toml` (comments are kept), first does a trial build of the tables, then writes the template and runs
  `loot build`. A season without its own template gets one on the first edit.
- `cargo xtask loot verify-view [--patch=file]` compares the app's calculation with the table build.
- `kit.toml`: fields `limits` and `positive_in_key`, check codes `new_out_of_limits` and `key_all_zero`.
- Loot selection by map in a control raid is not verified yet: see `docs/BALANCE_APP_ROADMAP.md`, step 2.0.

Kit and app are compatible both ways: an old kit opens in 1.2 without the "Loot" screen, a new one in 1.1 shows the loot tables, but
a patch with them is accepted only by a server with this version of `xtask`.

## 1.1.0 — 06.11.2026

### For designers
- **Economy and experience:** new tables: sell price, player market (base price, tax, listing fee), dismantling, crafting recipes,
  special goods, insurance, experience to level and experience per raid.
- **Item card:** sell price, market, dismantling and recipes. Every item with a name has a card, potions for example.
- **Left panel:** tables are grouped by section.
- **Names:** map and rarity rows are labelled.

### For the server owner
- `kit.toml`: field `group`, sections `[maps]` and `[fields_en]`.
- Kits got bigger: workbook 14 MB, archive 21 MB.
- Edits of the new tables were not tested in the game: only checked by reading the server code; the list of manual checks is in
  `docs/BALANCE_APP_ROADMAP.md`.

Kit and app are compatible both ways: an old kit opens in 1.1, a new one in 1.0, only without sections and the new parts of the card.

## 1.0.0 — 06.11.2026

First release. An app for external balance designers: opens a kit, edits values, saves a
`dbbalance-patch/1` patch. The server owner applies the patch with `cargo xtask balance patch`. The app changes neither the game nor the server,
uses no network and runs no programs.

### For designers
- **Kit:** opens from a zip or a folder, no Python or Excel needed. Old kits without `catalog.json` open too, but without
  category and rarity filters.
- **Tables:** every table of the kit, search, filters, sorting, names and icons, comparison with the original game.
- **Editing like in Excel:** typing in a cell, keyboard, undo and redo, pasting a block from the clipboard.
- **Items:** properties, durability, prices and affixes on one screen, with every possible roll value beside them.
- **Class curves:** an attribute by level, points can be dragged with the mouse, the dashed line shows the value in the game.
- **Bulk edit:** percent, multiply, add, set, on the selected rows or the current filter.
- **Checks while typing:** the same rules as on the server. With errors the patch is not saved.
- **Draft:** saved after every edit and survives a restart.
- **Moving to a new kit:** what can move moves by itself, you decide the disputed ones, the lost ones stay in the old draft.
- **Patch from an LLM or Excel:** opens and shows what fit.
- **Languages:** Russian and English.

### For the server owner
- `balance patch` now checks the patch by the same rules: min above max, `NaN`, type bounds, the same value twice
  stop the patch. Warnings and the "was → now" list are always printed.
- Kits contain `catalog.json` and English texts. The workbook and `make_patch.py` work as before.
- The new library `crates/balance-kit` is shared by the server and the app. The same rules are in `check.py`, a test checks both versions.

### Limitations
- Doesn't change loot or skill damage, doesn't compute final damage or final hero stats.
- No self-update: the server owner hands out new versions.
- The exe is not signed, Windows may show a SmartScreen warning.
