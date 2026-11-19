# Trial class 400201 — separate from the main game

**Unreal Engine 5.1 and a separate copy of the game are required** (external programs, installed by a human; `cargo ws doctor` shows whether
UE is there; paths — `config/local.toml [unreal]`).

We test the scheme from `research\mithril_classes\README.md`: a `role_type` row with the order's career and `force` = ADVENTURER plus a ninth slot in the creation window.

## What is where
| Path | What |
|---|---|
| `tables\base` | Export of the current tables (read only) |
| `tables\src` | `role_type`, `role_level_upgrade`, `role_head_icon` with new rows. Built by `build\gen_tables.py` |
| `lua\WBP_CreateRoleInfoUi.lua` | Window wrapper: adds the ninth widget before the original `Construct` |
| `icons\` | Temporary PNGs. Put the real ones under the same names and build with `-Icons` |
| `ue\` | UE 5.1 project only for cooking icons. The first cook takes about 40 minutes (engine shaders) |
| `paks\` | Ready paks: `zzzzzzzz_NewClass_P` (tables, texts, Lua), `zzzzzzzz_NewClassData_P` (icons, `FirstLevelAttr`) |
| `server\` | Backend: own exe, `ExcelConfig`, `assets`, own database `surreal_saves` (created at launch) |
| `F:\DB_NewClass\client`, `\server` | Game copies with our paks |

## Class settings: `class.json`
One file for everything visible before a match. After an edit: `build\build_paks.ps1`, `build\make_sandbox.ps1 -PaksOnly`, restart the backend, the server and the client.

| Field | Where it goes |
|---|---|
| `name` | `CareerName_101` and `CareerName_400201` in `GeneralStringTable.csv`. The creation window takes the name by career |
| `description`, `advantages`, `disadvantages`, `equipments` | New CSV rows, items separated by `\|` |
| `levels_from`, `stats_add` | 20 rows of `role_level_upgrade`: the levels of another class plus an addition to all levels |
| `kit` | New rows of `role_default_equipment`. Only a new character gets the new kit. Without `wear` the item goes into the backpack |
| `show_items_from` | The model look in the creation window: `create_show_item_config_ids` and `revival_default_weapon` of another class |
| `talents` | `FirstLevelAttr` row 101 + Human: `slot` 1–5, `ability_set` from the asset imports, plus text in `SkillStringTable.csv` |

Icons: `icons\*.png`, build with `build_paks.ps1 -Icons`.

Limitations:
- `CareerName_101` is one per career, a second row of the same career shows the same name in the creation window.
- Talents are also per career + race: all classes of one career share them.
- Talent conditions (required stat values) and their icons do not change yet, although they live in the same asset.
- The cross is removed for everyone: `IsMithrilCareer` is always `false`, only the inventory and tooltips call it.

## Separation from the main game
- Nothing is written into `core\backend\ExcelConfig`, `content\balance` and the main installs. There are no new files in the main `Paks`.
- Game copies: paks, exe and dll are hard links to the main installs, everything writable (ue4ss, settings) is a real copy.
  If a main pak is overwritten in place (`copy`), the copy changes too. The copy does not change the main game back.
- `%LOCALAPPDATA%\Dungeonborne\Saved` and `hosts` are shared by all installs.
- The backend and game server ports are hardcoded, so the main and the trial one cannot run at the same time.
- The database is empty: the main game's characters are not here.

## Running the check
The main backend, game server and client must be closed, the port lock free.

```bash
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Acquire -Owner new_class -Purpose "trial class"
```
```bash
powershell -ExecutionPolicy Bypass -File <data>\new_class\run_backend.ps1
```
```bash
powershell -ExecutionPolicy Bypass -File <data>\new_class\start_game.ps1 -Server -Client
```
ACE asks for UAC confirmation. Steps 1–3 do not need the server, `-Client` is enough.

On a new database the server account has no character, and the server gets stuck on the login screen ("no lobby map"). Once: `start_game.ps1 -ServerSetup`, create any character in the window, close the game.
Wait about 2 minutes between closing and a new launch (ACE). The server login sometimes fails with `CURL 7` (log `run\server\dbserver.log`), a restart helps.

After the check:
```bash
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Release -Owner new_class
```

## What we check
1. Nine slots in the window, the layout is intact, the icon is visible. Log: `logs\lua_create_role.log` ("slots in the window: 8 -> 9").
2. Click on the ninth slot: model, talents, description, skills.
3. Character creation, entering the hall, inventory, equipment.
4. A raid on our server: character, Q and E skills, talents, equipment.
5. Leaving the raid, results, return to the hall.

Logs: `logs\ue-client.log`, `logs\ue-server.log`, `server\rpc_dump`.

## Results
- 06.11, steps 1–3 passed, confirmed by the user: 9 slots in the window, character creation and entering the hall work. Log: "slots in the window: 8 -> 9", `role_create` answered without an error.
- 06.11, steps 4–5 passed: the raid works (confirmed by the user). Role `type=400201`, the server showed the pawn `BP_Fighter_HeroSample_C`, the result was accepted: `applied: extraction`, `POST -> 200`.
- Not analysed separately: how each skill and talent works on content foreign to the career, death, custom PVP.

- 06.11, the settings from `class.json` are built, not checked in the game yet. What to look at:
  1. Creation window: the name "Guardian" (Russian "Strazh"), description, pros and cons, icon, stats, the model in warrior equipment, talents 1 and 2.
  2. Inventory: no cross on fighter equipment, a cross on foreign equipment (e.g. cloth).
  3. A new character got the kit from `kit`. The old character `Mifril` keeps the previous kit.
  4. Raid: the new stats and talents 1 and 2 work.

## Known simplifications
- There is no `pvp_default_item_config_ids` (as for the order), so custom PVP is not checked.
- The trial backend since 06.11 14:45 is a debug build from the shared sources (relics, materials and orchestrator fixes as of that moment), `dbserver-rs-0913.exe` is the previous one. The client also has `zz_RelicQuality_P`.
- Loot from bags is sold for gold on exit: this is a general backend rule (`settle_extraction_items`), not a class one.
- `rank_report_setting` and other tables with per-class rows are not extended.
- The icon is temporary.

## Rebuild
```bash
powershell -ExecutionPolicy Bypass -File <data>\new_class\build\build_paks.ps1
```
```bash
powershell -ExecutionPolicy Bypass -File <data>\new_class\build\make_sandbox.ps1 -PaksOnly
```

## How to remove everything
Delete `<data>\new_class` and `F:\DB_NewClass`.
