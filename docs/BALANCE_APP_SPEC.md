# Spec: application for balance designers (Tauri 2 + Rust) — v1

Status: 2026-11-06, **M0–M4 done, release 1.0.0**. The build has not been shown to designers yet. Working title — **Balance Studio**. Basis: `BALANCE.md`, `balance_kit/`,
`core/backend/xtask/src/kit.rs`, `core/backend/crates/balance-kit/`, stack and techniques — the launcher (`<data>\launcher`).

## 0. Why, and what is in v1

External designers edit balance through the kit: `balance.xlsx` or an LLM, then a Python script builds a patch. The server
owner checks the patch and applies it. What gets in the way:

- Python and `openpyxl` are needed, the patch is built in a console;
- one item is spread over several sheets, the consequences of an edit are not visible;
- after a new kit, old edits have to be redone.

Designer complaints have not been collected yet, this is our analysis. So after milestone M2 (section 10) we will show the build to 1–2 designers and
change the order if needed.

**Principle:** the application only prepares a patch. It does not touch the game, the server or the project sources. The only input is
the kit, the only output is a `dbbalance-patch/1` patch file. The server chain stays: `cargo xtask balance patch`,
then `balance build`.

**In v1:**

1. Open a kit from a zip or a folder, without Python and Excel.
2. Tables with search, filters, names, icons and comparison with the game original.
3. Item card: everything about an item on one screen.
4. Class attribute curves by level.
5. Bulk edits ("potions −20%").
6. Edit validation right while typing.
7. Draft autosave and carrying edits over to a new kit.
8. Saving a patch and opening a finished patch (from an LLM or `make_patch.py`) for review and further work.
9. Russian and English, Windows 11 theme like the launcher.

**Not in v1:** loot, server owner mode, sending patches through the application, auto-update, damage calculator,
editing skill damage, launching the game or the server, macOS and Linux. See section 11.

Excel and LLM stay: `balance.xlsx` and `llm/` are still in the kit.

## 1. Facts about the kit (checked against the code and the kit `20261104-542ba6f5`, server base)

| Table | Rows | Values | Editable | Layout in the workbook | Row key | Icon |
| --- | --- | --- | --- | --- | --- | --- |
| `role_level_upgrade` | 184 | 4 009 | 3 641 | wide | `role_type_id` | `role_type_id` |
| `affix_template` | 694 | 14 017 | 2 150 | long | `item_id` | `item_id` |
| `affix_value_template` | 891 | 21 542 | 14 572 | wide | `affix_value_id` (not unique) | none |
| `affix_legendary_rule` | 27 | 108 | 54 | wide | `affix_alias_id` | `affix_alias_id` |
| `market_product_item` | 906 | 14 313 | 2 723 | wide | `product_id` | `product.reward_items[0].type_id` |
| `gear_template` | 550 | 654 | 63 | wide | `item_id` | `item_id` |
| `affix_count_random_pool` | 20 | 59 | 0 (reference) | wide | `affix_count_pool_id` | none |
| `affix_level_pool_rule` | 2 | 6 | 0 (reference) | wide | `rule_id` | none |

Total **23 203 editable values**. A seasonal kit may contain other tables and a different number of rows, so the
application code has **no list of tables**: everything comes from `kit.json`. Dedicated screens (card, curves) are enabled only
if the required tables are in the kit.

**Kit contents:**

- `manifest.json`: `format = dbbalance-kit/1`, `version = <date>-<8 hex sha256 of tables>`, `title`, `season`,
  `patch_format`, per table `sha256/rows/values/type/source/original`. The date is part of the version, so a new version
  does not necessarily mean new data.
- `kit.json`: `notes`, `table[]` (`name/title/about/where/editable/layout/icon`), `fields`, `fields_en`, `attr_units`
  (fraction / points; the stored unit values are the Russian words for these), `attr_ids`.
- `data/<table>.csv`: long layout `row,key,title,path,value,name`; separator `,`, decimal `.`, UTF-8 with BOM.
- `data/<table>.orig.csv` — the game original (if present); `data/<table>.en.csv` — English labels (if present).
- `raw/<table>.json`: the source JSON, it shows the number type; `icons/<key>.png`: 767 files, 11 MB.
- `tools/`, `llm/`, `balance.xlsx`: the application does not read them and **never runs them**.

**What the kit lacks but the application needs:**

- item category and rarity (`item_type`, `UeSource_ItemType`);
- readable class names: the label from `role_type` has a technical form `FIGHTER/HUMAN`. `400000` and `400002` are labelled
  the same, because `400000` is a service class (`EN_ENVIRONMENT_TYPE_ONLY_EDIT`), like `499999`;
- vendor row names: `title` of `market_product_item` is empty, the name is taken from `name` of `reward_items[0].type_id`;
- the field type when the row has no value.

Solution — section 3.

**Table links (checked):**

- `gear_template.item_id` — all 550 are in `affix_template.item_id` (694).
- `affix_template … data.affix_value_id` → `affix_value_template.affix_value_id`: 10 references, 11 groups, all resolve.
- `market_product_item … reward_items[0].type_id` — 218 distinct items, of which 143 are in `affix_template`.
- How a legendary is linked to an item has not been checked against the kit tables. For now `affix_legendary_rule` is a separate screen.

## 2. Patch format: unchanged

```json
{
  "format": "dbbalance-patch/1",
  "base": "<kit version>",
  "season": null,
  "author": "Nickname",
  "comment": "why the edit",
  "created": "2026-11-06 12:00",
  "app": "balance-studio/1.0.0",
  "changes": [
    {"table": "role_level_upgrade", "row": 12, "key": "400001", "title": "…",
     "path": "role_attribute.first_level_strength.fixed_value", "old": "10", "new": "12"}
  ]
}
```

- `app` — a new optional field. `balance patch` and `check.py` do not read extra fields.
- `season` is copied from the manifest: `null` — server base. The `old` and `new` values are text, decimal with a dot.
- File name as in `make_patch.py`: `patch-<author>-<YYYYMMDD-HHMM>.json`, but with the author cleaned of characters forbidden
  in file names.
- A value cannot be cleared: the server and `check.py` reject an empty `new`. In v1 a field can only be changed.

## 3. Additions to the kit (our side)

The format stays `dbbalance-kit/1`: only a new file is added, so old kits still open (with
reduced filters), and the workbook and `make_patch.py` do not read it. `check.py` takes field types from it if it is present.

`catalog.json` (`cargo xtask balance kit` writes it from the tables and the schema):

```json
{
  "format": "dbbalance-catalog/1",
  "items": {"181010000": {"ru": "…", "en": "…", "major": "EQUIPMENT", "minor": "…", "rarity": "NORMAL", "slot": "SWORD", "kind": "EQUIP_WEAPON"}},
  "classes": {"400002": {"order": 2, "ru": "…", "en": "Fighter", "career": "FIGHTER", "race": "HUMAN"},
              "400000": {"order": 0, "ru": "…", "en": "Fighter", "career": "FIGHTER", "race": "HUMAN", "hidden": true}},
  "rarities": {"UNCOMMON": {"value": 3, "ru": "…", "en": "Uncommon"}},
  "field_types": {"gear_template": {"durability": "int32"}}
}
```

- `items` — from `item_type`, `UeSource_ItemType` and localisation. Fields without data are not written. The kit `20261106-542ba6f5` has
  977 items, 804 have names, 768 have a rarity.
- `classes` — from `role_type` and localisation (`str_name`). `hidden` — a service class, not shown in lists. The optional
  section `[classes."<id>"]` in `kit.toml` overrides any field.
- `rarities` — from `EnItemRarity` and localisation; the keys match the values of `items[…].rarity`.
- `field_types` — from `Configure.pb`, paths with `[*]`, so the application knows the type even for empty fields.

In `kit.json` a table got the optional field `context`: non-editable fields shown next to the editable ones (the level in
class curves).

A separate file, not reference tables in `kit.toml`, because a table from `kit.toml` would become one more sheet
of the workbook.

## 4. Structure

```
backups/crates/balance-kit/   shared library (new workspace member): reading the kit, wildcard, patch validation,
                              carrying edits over, writing the patch, accepting a kit, table layout. Dependencies: serde_json, csv, zip. No prost and no schema.
backups/xtask/                uses balance-kit; apply_changes and .bytes stay here
<data>\balance-studio\
  src-tauri/                  Tauri 2; thin commands, logic in balance-kit
  ui/                         index.html, app.js, i18n.js, input.css, api.d.ts, dev/mock-tauri.js, Trunk.toml
```

- **Stack as in the launcher:** Tauri 2, HTML + Tailwind v4 standalone + plain JS, `trunk` build, no npm and no frameworks.
  Windows 11 design: `launcher/ui`, the `<template>` + `data-state` technique. Charts — inline SVG.
- **Delivery:** one portable exe, like the launcher. WebView2 — the system one. Windows 10/11.
- **No network at all in v1.** Tauri permissions: only our own commands, the file picker plugin and drag-and-drop.
  CSP as in the launcher, but `img-src 'self' data:` and the kit icon scheme, without `https:`.
- **Rust holds the data:** on open the kit is indexed in memory. The UI requests pages of rows, filtering and sorting
  are computed by Rust. The table in the UI uses virtual scrolling, only visible rows are drawn. Targets: the kit opens in under
  3 s, search and scrolling without noticeable delay on the 21 000 values of `affix_value_template`.
- **Icons:** a custom Tauri scheme that serves only files from `icons/` of the open kit.
- **Sandbox:** `ui/dev/mock-tauri.js` with a small sample kit, so the UI runs in a browser without Rust.
- **Storage:** `%LOCALAPPDATA%\BalanceStudio\`: `settings.json` (language, author, recent), `kits/<version>/` (unpacked
  kits), `drafts/<id>.json` (drafts). Checking from a Claude session: new AppData folders are virtualised there, see
  the MSIX memory note; files have to be viewed through the application's own directory.

### Commands (contract `ui/api.d.ts`)

| Command | What it does |
| --- | --- |
| `open_kit(path)` | Validates and unpacks the kit, returns the description and the list of tables |
| `list_recent()` | Recent kits and drafts |
| `query_rows(table, filter, sort, offset, limit)` | A page of rows with current values, original and edits |
| `get_item(id)` / `get_class_curves(ids)` | Data for the item card / curves |
| `set_values(edits)` | Writes edits into the draft, returns the validation result |
| `preview_bulk(spec)` / `apply_bulk(spec)` | Bulk edit: preview and apply |
| `list_changes()` / `revert(ids)` | List of edits / revert |
| `rebase_preview(kit)` / `rebase_apply(decisions)` | Carry-over to a new kit |
| `import_patch(path)` | Opens a patch: validation and display as edits |
| `export_patch(path, author, comment)` | Validation and saving of the patch |

Each command returns `{ok}` or `{code, message}`, user-facing texts are in `i18n.js`.

## 5. Screens

Window 1280×800, minimum 1100×680. Navigation on the left, like the launcher.

1. **Start.** A button and a drop zone for a zip or a folder, recent kits, short instructions. The nickname
   is asked only when saving a patch, on the "My edits" screen.
2. **Kit summary.** Title, version, season, `notes` from `kit.json`, tables with an edit counter, how many values already
   differ from the game original.
3. **Table.**
   - `wide`: a row = a table row; `long`: a row = a value. The layout comes from `layout`, can be switched.
   - The name and the icon are pinned. Column header: `fields` + unit (the fraction unit is shown as %, stored as a fraction).
   - Editable cells are highlighted, the rest is read-only. Colours as in the workbook: differs from original, own edit, error.
   - Cell tooltip: original, kit value, field description, original range.
   - Search by name, id and field. Filters: "changed by me", "differs from original", category and rarity (from
     `catalog.json`).
   - A single value is edited right in the cell; Enter, Tab and arrows work, Ctrl+Z / Ctrl+Y — undo and redo;
     pasting a block from the clipboard — as in Excel.
4. **Items and the item card.** A list with icons and filters. The card collects and edits together:
   base properties (`affix_template`), durability (`gear_template`), prices (`market_product_item`, if the item is sold) and
   the affixes of the group `data.affix_value_id`. Next to them — all possible roll values (section 6).
5. **Class curves.** Attribute lines over levels 1–20 for the selected classes, the original dashed. Points can be dragged
   or edited in the table under the chart. Caption: "table values, without items". `400101`–`400103` contain only
   level 1, so they are not in the curves.
6. **Bulk edit.** Invoked on selected rows or the current filter: `×`, `+`, set, both directions for min/max
   pairs. Integer fields are rounded. Before applying — a "before → after" table with errors and warnings.
7. **My edits.** All edits "table → row → field: before → after (±%)", grouping, revert, warnings, author and
   comment fields, a "Save patch" button.
8. **Settings.** Language, nickname, kits folder, application version.

## 6. Edit validation

The same rules as in `check.py`, plus new ones. The rules live in `balance-kit`; `check.py` stays for the LLM path without
the application. Shared sample files `core/backend/crates/balance-kit/tests/fixtures/` run through both implementations so they do not
diverge.

**Errors** (the patch cannot be saved):

- no table or row, `key` does not match, the field is not in `editable`;
- `old` is not equal to the kit value, a different season, a wrong `format`;
- empty `new`, a non-number in a numeric field, a fraction in an integer field;
- min greater than max (taking into account other edits of the same patch).

**Warnings** (saving is allowed):

- a change of more than 3 times, a sign change, a value greater than 3× the largest absolute value of the original for this
  field;
- **new:** `max − min` is not a multiple of the `unit` step, because then the roll may fall outside the bounds (how the game computes such values has not been checked). Checked only for
  changed pairs: in the current data 4 of 7 286 `affix_value_template` pairs are like this;
- **new:** `unit` is 0 or less than 0;
- a patch from a different kit version.

**Roll (checked on the server 05.11, `FIX_LIST` BAL2):** `value = min + floor(roll × n / 10000) × unit`, where
`n = (max − min) / unit + 1`, `roll` from 0 to 9999. The application shows all `n` values. Editing min/max also changes already
dropped items: a tooltip on such fields reminds about this.

**What is computed exactly and what is not:**

| We show | Basis |
| --- | --- |
| Table values, roll | The formula above |
| Final hero attributes | Not in v1. 252 of 252 checked, but the role base and the full list of `FirstLevelAttrDataAsset` coefficients are needed, the kit does not have them |
| Damage, DPS | Not shown. The formula is partly reconstructed (BAL2), the multiplier K is not separated |
| Skill damage | Not editable: it is in the effects and curves of the game pak |

### On the server (done in M0)

`balance patch` validates the patch by the same `balance-kit` rules, comparing it with the server tables: current values,
`editable` from its own `kit.toml`, game original ranges, types per schema. Min greater than max, `NaN` and the other errors above
stop the patch; warnings and the "before → after" list are always printed. The schema is still checked on apply.

## 7. Drafts and carrying edits over

- **Draft** `dbbalance-draft/1` is saved automatically after each edit: kit version, season, author, comment, edits.
  Each edit stores a row identifier.
- **Row identifier:** `key`, the ordinal among rows with this `key` and a hash of the row's non-editable values. A `key` alone
  is not enough: `affix_value_template` has 891 rows for 11 keys, and inserting a row in the middle of a group would shift the numbers. The hash
  catches such a shift, including when values coincide by chance. In the kit `20261104-542ba6f5` the pair "`key` + non-editable
  values" is unique for all rows of all 8 tables.

**Carry-over** starts when a new kit of the same season is opened and there is a draft from the old one:

| Case | Result |
| --- | --- |
| Row identified, the value in the new kit equals `old` | Carried over automatically, `row` is updated |
| The value in the new kit equals `new` | The edit is no longer needed, removed, shown in the report |
| The value is different | Conflict: "was in the draft / now on the server / mine", manual choice |
| The identifier matched only partly, the row was found only by `key` and ordinal | Conflict, not applied automatically |
| The row is gone, or the field can no longer be changed | Lost, shown in the report and stays in the old draft |

The old draft is not deleted until the user confirms the result. A different season is not carried over automatically.

If the server moved ahead and a designer sent a patch from an old kit, `balance patch` refuses on the mismatched `old`.
The order is: the owner releases a new kit, the designer opens it and saves the patch again.

## 8. Security

The kit and the patch are foreign files, and the application runs on outsiders' machines.

- **Zip:** protection against escaping the folder (`..`, absolute paths, links), limits: 200 MB total, 20 000 files, compression
  ratio. Only `manifest.json`, `kit.json`, `catalog.json`, `data/`, `raw/`, `icons/` are extracted.
- **No execution:** `tools/*.py` and the rest are not executed, not a single shell command.
- **Icons:** only PNG, JPEG and WebP with content-based checking; SVG is not accepted.
- **Text output:** names from the kit and `author`, `comment`, `title` from the patch — only via `textContent`.
- **Kit signatures are not checked:** in v1 a kit contains only data, and a forged kit can only harm a patch,
  which the owner checks anyway. Signing comes back into the plan together with automatic kit distribution.
- **No secrets:** the application stores no tokens and does not log into accounts.
- **Server side:** the patch is treated as a foreign file; allowed fields are taken only from the server's `kit.toml`.

## 9. Acceptance

**Compatibility (automated tests):**

1. On each of the 6 editable tables an application patch passes `check.py` and `cargo xtask balance patch --dry-run`.
2. The same edit via the workbook and `make_patch.py` and via the application gives identical `changes`, except `title`:
   the workbook appends the second language and the unit to it, the server does not read it.
3. The patch applies, `balance build` builds, and the table JSON has exactly the expected values.
4. A kit without `catalog.json`, without `.orig.csv` and without `.en.csv` opens, the corresponding filters are hidden.
5. Seasonal kit: the patch has the correct `season`, `balance patch` writes into the season tables.

**Carry-over:** sample kits for each row of the table in section 7, including inserting a row in the middle of an
`affix_value_template` group with coinciding values.

**Security:** a zip with `..`, an absolute path, a bomb, a forged icon; a patch with HTML in `comment`; a patch with a field outside
`editable`.

All of this is run by `balance-studio\tools\accept\run_all.ps1`. Patch application runs on a copy of the project (`XTASK_ROOT`), not on
the working server. Dragging a file into the window is not checked automatically: it is checked by hand.

**Manually, the real exe (CDP WebView2, like the launcher):**

- the full path: open a kit → edit an item in the card → bulk edit → close and reopen, the draft is in place →
  save the patch → apply via xtask on the local server;
- reviewing an LLM patch with and without errors; carry-over to a new kit;
- window 1100×680 and 1280×800, RU and EN, no CSP or console errors, no network requests.

## 10. Stages

| Milestone | What it includes | Done when |
| --- | --- | --- |
| **M0. Server and kit** (done 06.11) | The `balance-kit` crate, moving `wildcard` and the validation rules; `balance patch` checks (section 6); `catalog.json`, `[classes]`, `field_types`; new kit build | `xtask` works as before, there are tests, a bad patch is rejected by the server |
| **M1. Skeleton and tables** (done 06.11) | Application, sandbox, opening a kit, zip security, tables, search, filters, comparison with original, icons | All kit tables are visible and work fast |
| **M2. Editing and patch** (done 06.11; showing to designers postponed, the build has not been shown to them yet) | In-cell editing, validation, undo, draft, "Changes" screen, saving and opening a patch | Compatibility acceptance. **Show to 1–2 designers** |
| **M3. Meaning** (done 06.11) | Item card, roll, class curves, bulk edits | Editing a whole item on one screen |
| **M4. Release 1.0** (done 06.11) | Carrying edits over, EN, instructions for designers, exe build | All acceptance of section 9 |

## 11. After v1

- **Auto-update:** the launcher mechanism (`launcher/src-tauri/src/update.rs`, `tools/release.py`). Its own key pair and its own
  signature string `dbbs-update/1`, so that the launcher signature does not fit here. Hosting is needed: prod is off since 05.11.
- **Kit distribution and patch intake through the server:** a separate API, authorisation, limits. Together with this — kit signing.
- **Loot (done in 1.2, `docs/BALANCE_APP_ROADMAP.md`):** the template is shown as `lt_*` tables, a new patch format was not needed. New
  slots and items did not make it into the first version.
- **Owner mode:** list of patches, authors and reasons, conflicts between patches, applying. For now this is `balance patch`.
- **Final hero attributes:** after exporting the role base and all coefficients.
- **Damage:** only once BAL2 is closed.

## 12. Open questions

| Question | Who decides | By which milestone | Current assumption |
| --- | --- | --- | --- |
| Application name | Owner | M4 | Balance Studio |
| What actually bothers designers | Owner, designers | M2 | The analysis from section 0 |
| Loot earlier | Owner | After the M2 showing | After v1 |
| How a designer sends a patch | Owner | M4 | As a file, as now |
| Exe signing | Owner | M4 | Unsigned; a SmartScreen warning is possible |
