# Crafting relics of a lower quality

If a higher quality is unlocked, on the blacksmith panel you can pick a lower one with the arrows next to the "Craft" button (players' suggestion, 06.11). The panel shows the name, stats and price of the selected quality.

`WBP_Heirloom_InstantiatePanel` is left alone: the game does not open this window, crafting goes through `WBP_Heirloom_Panel`. Log: `%LOCALAPPDATA%\Dungeonborne\Saved\Logs\RelicQuality.log`.

## What it consists of
| Part | Where | Without it |
|---|---|---|
| Quality selection buttons | This pak: `WBP_Heirloom_Panel.lua`, the original `_Orig` next to it | The panel as in the original |
| Permission on the server | Season flag `relic_lower_quality` (`seasons/<id>/season.toml`), code `gear_heirloom_instantiation_impl` | The server rejects a lower quality (-3834) |
| Gems by number of sockets | `relic_gems_for_slots`, always works | — |

## Season
User's decision 07.11: this is not the base game but a season flag. Currently enabled in the `playtest` season (`seasons/playtest/season.toml`, `[features] relic_lower_quality = true`); disabled in the base (`seasons/base.toml`). The pak in `mods/catalog.toml` is marked `season = "playtest"` and is installed only with this season (docs/SEASONS_FORMAT.md).

To disable in a season: `relic_lower_quality = false` in its `season.toml`, restart the backend, remove the pak from the season's catalog.

The comment at the top of `WBP_Heirloom_Panel.lua` still refers to `deadmoney.toml`: the Lua is not edited so that the pak matches what players have installed.

## Build
The folder contents without this README:
```bash
powershell -ExecutionPolicy Bypass -File build.ps1
```

## Game rules that remain
- In the bag and on the character — one relic per type (`Heirloom_Prompt_Equipped`, client check `CheckHeirloomCanInstantiate`).
- One copy of each quality: the server rejects a second instance (-3837).
- Different qualities may lie together in the storage.

## In-game check (06.11, trial stand)
Checked by the user: the arrows are in place and do not get in the way of the help button, quality switching on the panel, crafting a lower quality.

Not checked:
- A lower quality with gems: only the gems in its sockets remain. Socket numbers from 1 were determined from the client code, the values in the traffic were not looked at.
- With the flag off and the pak installed: the server response.
- Gem sockets on a lower quality are shown by the group's `MaxGemSlotCount`, i.e. as for the higher one: extra ones may be visible as closed.
- On an error the panel works like the original, the reason is in the log.
