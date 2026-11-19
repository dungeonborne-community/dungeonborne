# Game data: what there is, where it lives, how to get it

Read this before saying "there is no data". Almost everything about the game is in its files: the client paks hold
tables, icons, abilities, effects, curves, Blueprints and Lua. Everyone has their own copy of the game; game data is
built on your machine from it (`docs/PACKAGES.md`), not handed out — except a few artifacts the store carries.

## 1. Your copy of the game: the reference

**Your own copy of the game is required** (since 18.11): `cargo ws doctor` reports an error until there is a
reference, and `cargo ws bootstrap --apply` takes one itself when it can (below). It needs a Steam account that owns
Dungeonborne.

`cargo ws game` says what you have and, when there is no reference, the exact steps. The same with buttons: DevKit,
page "Game data" ("Copy it as the reference", or the Steam console steps). The build numbers are in
`config/game.toml` (today `16716489`, 17 GB); the files of the clean build — `config/game-16716489.files.tsv`.

| Step | Who | How |
|---|---|---|
| **The usual way:** the game installed by Steam | **the human** installs Dungeonborne in Steam (their account that owns it) | `cargo ws bootstrap --apply` (or `cargo ws game original installed --apply`, or DevKit) finds it in any Steam library, checks it is build `16716489` and holds every file of the clean build with its size, and **copies** only those files into `home/game/original/<build>` (17 GB; mods and saves next to them are skipped, the install is not changed). A modded or broken install is refused: Steam → the game → Properties → Installed Files → Verify integrity |
| Or download the clean build | **the human** | open `steam://open/console` and run `download_depot 2448970 2448971 4035623128766163436`; Steam puts it into `<Steam>/steamapps/content/app_2448970/depot_2448971` |
| Register the download as the reference | the human or the agent | `cargo ws game original steam --apply` (or `ws bootstrap --apply`) — finds the download, moves it to `home/game/original/<build>`, writes `original.toml`, sets read-only; `cargo ws game` then checks it against `config/game.toml` |
| Working copies (battle server, clients for the stand) | the agent | `cargo ws game clone original <name> --apply` → `home/game/<name>`: paks are hard links to the reference (about 1 GB per copy instead of 17); list them in `config/local.toml [games]` (`tools/stand/README.md`) |
| How an install differs from the reference | the agent | `cargo ws game diff <name or path>` |

Several workspaces on one machine share one reference: `config/local.toml [game] original = '<other home>/game/original'`
(it is read-only, so sharing is safe). Everything that needs the reference — `ws dump`, `ws game clone`, `ws game diff`
— reads it from there.

The reference is never changed: mods, UE4SS and patched files go only into working copies.

## 2. Dumps: `ws dump`

`cargo ws dump` reads the reference with the pak key and writes files you can read into `home/dumps/<build>/`:

```
home/dumps/16716489/
  export/Content/TGF/...   PNG and JSON, the layout of an FModel export
  index/<set>.tsv          per package: format, package path, result (ok / skip / error)
  manifest.tsv             sha256, size, path of every exported file (compare two dumps by it)
```

| Set (`tools/dumper/sets.toml`) | Format | What |
|---|---|---|
| `icons` | PNG | all UI icons: items and equipment (inventory and tooltip), skills, buffs, classes, markers |
| `monsters` | JSON | monsters and bosses: ability sets, gameplay effects (damage, buffs), ability rules, data assets, blueprints |
| `heroes` | JSON | playable classes: ability sets, abilities, effects |
| `curves` | JSON | curve tables and float curves (numbers by level) |

Run it. The pak key: `config/local.toml` `[re] aes_key` (you find it yourself, it is public — `docs/SECRETS.md`, "The
pak key"), or the vault. With a vault session open — the human typed the master password once
(`cargo ws secrets unlock`, or DevKit, "Game data") — it is just:

```
cargo ws dump --apply              # all sets
cargo ws dump --set=icons --apply  # one set
```

Without a session: `cargo ws secrets run game_aes_key -- cargo ws dump --apply` (asks for the password each time).

Without `--apply` it shows the plan and everything that is missing, at once. Needs the component `research` on
disk (the dumper and its tools; a `web` or `balance` checkout: `cargo ws components add research --apply`, then
`cargo ws bootstrap --apply`), and: the reference (section 1), the usmap in `home/re/ue4ss` (artifact
`game usmap`, or from your copy of the game: `tools\stand\usmap.ps1` after `cargo ws stand --apply`), Oodle (`home/tools/oodle`, from `config/tools.lock`), the .NET SDK (`cargo xtask setup
--apply --tool=dotnet`, or a .NET 10 SDK of your own). `ws dump` builds `tools/dumper` (dbdump, C# over CUE4Parse)
itself. Same reference and same `tools/dumper` → the same `manifest.tsv`.

**Decrypting the paks is done only on a person's word** (`docs/RESEARCH.md`). A person asking for a task that needs
game data (an item, a skill, a wiki page) is that word. An AI agent whose tools refuse to run it (for example
Claude Code in "auto" mode) gives the human the exact command and continues when they say it finished.

## 3. Where each kind of data is

| Data | Where | Notes |
|---|---|---|
| Game tables: items, equipment, rarity, loot, affixes, quests, monsters' spawn, maps, shop | `content/balance/tables` (JSON), CSV views in `content/balance/csv`, seasons override them (`content/seasons/<id>/balance`) | the game keeps them as Lua/protobuf, not UE DataTables; `docs/BALANCE.md` |
| Server copy of the tables | `core/backend/ExcelConfig`, `ExcelConfigjson` (older) | |
| Names and texts (RU, EN) | `content/names/strings.tsv`, `strings_en.tsv` | ids from the tables |
| Item icons by item id | `core/backend/assets/item_icons/` (`icons/<type_id>.png`, `tips/<type_id>.png`, `item_icons.json`) | artifact from the store; rebuild from a dump: `python -I tools/py/item_icons.py` with `[dumps] fmodel_exports = 'home:dumps/<build>/export'` in `config/local.toml` |
| All other icons (skills, buffs, classes, map) | dump set `icons` | |
| Monster and boss abilities, damage effects, rules | dump set `monsters` (`Content/TGF/Character/Monster/<M_Name>/...`) | numbers are in GE (`GE_*`), often through curves |
| Class abilities | dump set `heroes` | `docs/RESEARCH.md`: balance CSV → AbilitySet → GA → GE |
| Curves (numbers by level) | dump set `curves` | |
| Client Lua (UI, tooltips, formulas) | `home/re/lua_all` (artifact) | `luac54 -l -l` |
| Types, fields, enums | `home/re/ue4ss` usmap and CXXHeaderDump (artifacts) | |
| Native client code | `home/re/ghidra/images/db_image.bin` (artifact on request) | `research/ghidra/README.md` |
| Findings and analyses | `research/`, `docs/*.md`, `agents/memory/` | check what is already known before digging |

Dumps and exports are game files: they stay in `home`, never in git. Git gets conclusions, generators and scripts.

## 4. For agents

- A task needs data you do not see — check this file, then `cargo ws components list` (a partial checkout may not have
  `research`), then whether `home/dumps/<build>` exists. Missing → say exactly which command gets it.
- Write "no data" on a page only when the data is not in the game files either, and say where you looked.
