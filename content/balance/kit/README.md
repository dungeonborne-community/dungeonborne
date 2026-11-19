# Dungeonborne balance kit

You can propose balance changes without our code: edit a table in Excel (or ask an LLM), and a script makes a
**patch** — a file listing the changes "was → now". You send the patch to us, we check it and enable it on the server.

The easiest way to edit is the **Balance Studio** app: it opens this whole archive and needs neither Python nor Excel.
Below is the way without the app: through Excel or an LLM.

## What is in the kit
| File / folder | What for |
|---|---|
| `balance.xlsx` | tables for people: a sheet per table, **yellow** cells may be edited, grey ones are reference, **orange** ones no longer match the game's original (the original is in the cell note), the "Differences" sheet lists all such values |
| `llm/` | the same for LLMs: `PROMPT.md` (instructions) + `<table>.md` (description and data) |
| `tools/` | Python scripts: `make_patch.py` (workbook → patch), `check.py` (patch check), `build_excel.py` |
| `lt_slots`, `lt_items`, `lt_rules` | loot sheets: slot chances, item weights and counts, rules. Easier to edit on the "Loot" screen of the app |
| `data/`, `raw/` | source data (long-form CSV and JSON) — do not edit, the patch is checked against them; `data/*.orig.csv` — the game's original |
| `icons/` | item pictures `<id>.png` (the "Picture" column in the workbook) |
| `manifest.json`, `kit.json` | kit version, which tables and fields may be changed |
| `catalog.json` | items, classes, rarities and field types: read by `check.py` and the designers' app, no need to edit |

## Needed once
Python 3.9+ and `pip install openpyxl` (for pictures in the workbook also `pip install pillow`).

## Through Excel
1. Open `balance.xlsx`, change the yellow cells. Do not touch sheet rows 1–2 or the `row` column; do not delete or
   add rows (sorting and filtering are fine).
2. Save and run in the kit folder:
   ```
   python tools/make_patch.py balance.xlsx --author YourNick --comment "why"
   ```
   The script shows all changes and writes `patch-YourNick-<time>.json`.
3. Send the patch to the playtest organisers.

## Through an LLM (ChatGPT, Claude…)
1. Upload `llm/PROMPT.md` and the `llm/<table>.md` files you need into the chat.
2. Ask in your own words: "the knight is weak at levels 1–10, raise strength", "vendor potions 20% cheaper".
3. Save the JSON from the answer as `patch.json` and check it:
   ```
   python tools/check.py patch.json
   ```
   Errors — give their texts to the LLM so it fixes them. No errors — send it.

## Pictures (the "Picture" column)
The picture key is the value of the table's `icon` field from `kit.toml`: `item_id` (items), `role_type_id` (classes),
`affix_alias_id` (legendaries), `product.reward_items[0].type_id` (product). The file is `icons/<key>.png` (or .jpg/.webp)
in the kit folder: `build_excel.py` inserts it into the cell (needs `pillow`), otherwise the cell keeps the key itself.
Put pictures into `balance_kit\icons\` — `cargo xtask balance kit` copies them into the kit.

## What happens to the patch next (our side)
`cargo xtask balance patch <patch> --dry-run` — check; without `--dry-run` — write into the tables of the patch's season
(`season` in the patch: `seasons\<id>\balance\*.json`; `null` — the base `balance\*.json`), then
`cargo xtask balance build` (server + client pak). If another patch has already changed the table, "was" will not
match — such edits are not applied (a patch from a new kit is needed). The server checks the patch by the same rules as
`check.py`, but against the server tables, so a designer's `check.py` result guarantees nothing: errors
(including min greater than max) stop the whole patch, warnings are only shown. Rules and shared examples:
`crates/balance-kit` (`cargo test -p balance-kit` checks both Rust and `check.py`).

## Building the kit (our side)
`cargo xtask balance kit` — the active season's kit → `seasons\<id>\kit\<id>-balance-<date>-<hash>\`;
`--season=none` — the server base → `balance_kit\out\dungeonborne-balance-<date>-<hash>\`. Next to the folder — `<folder>.zip`
for handing out to players (without the `.thumbs` thumbnail cache; `--no-zip` — do not pack).
Tables and rules — `balance_kit\kit.toml` (shared by the framework) + `seasons\<id>\kit.toml` (season: title, notes,
own `[[table]]`). A season's own README/PROMPT — `seasons\<id>\kit\README.md`, `PROMPT.md` (otherwise — from here).
Data — season tables on top of the base; the game's original — `data\<table>.orig.csv` (differences highlighted in the workbook and for the LLM).
