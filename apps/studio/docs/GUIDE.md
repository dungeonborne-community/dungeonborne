# Balance Studio: guide for designers

Balance Studio helps you propose balance changes for Dungeonborne. You change values, and the app saves them to a **patch**
file. You send the patch to the server owner, who reviews it and turns it on.

The app changes nothing in the game or on the server and doesn't use the internet. You don't need Python or Excel.

## What you need
- Windows 10 or 11.
- `BalanceStudio-1.1.0.exe`. No installation.
- **A kit**: a zip archive with the tables, from the server owner. Don't unpack it.

Windows may warn that the app is unknown, because the file isn't signed yet. If you got it from the server owner, choose
"More info" and "Run anyway".

## Quick start
1. Start the app and open the kit: use "Open zip" or drop the archive on the window.
2. Change values in any table, or on the "Items" and "Class curves" screens.
3. Open "My changes" and enter your nickname and why you made the change.
4. Press "Save patch" and send the `patch-….json` file to the server owner the way you agreed.

Changes save themselves, so you can close the app at any time.

## Screens
- **Summary.** Kit version, tables and important notes. Read the notes once: they explain what can be changed.
- **My changes.** All changes as "was → now", errors and warnings, undo, saving the patch.
- **Items.** Everything about an item on one screen: properties, durability, vendor prices, player market, dismantling, crafting,
  affixes. Next to min and max you see all the values the item can get.
- **Class curves.** A class attribute by level. Drag a point or edit it in the table below. The dashed line is the value in
  the game.
- **Loot.** What chests, monsters and vases drop on the chosen map: see "Loot" below.
- **Tables.** Every table of the kit by section: items, economy, experience, loot, reference. Search, filters, click a header to sort.

## Editing a table
| Action | How |
| --- | --- |
| Select a cell | click or arrow keys |
| Replace a value | just start typing |
| Edit the current value | F2 or double click |
| Confirm | Enter (down) or Tab (right) |
| Cancel typing | Esc |
| Back to the kit's value | Delete |
| Undo and redo | Ctrl+Z and Ctrl+Y |
| Paste a block from Excel | Ctrl+V: from the selected cell, locked cells are skipped |

Percent values are typed in percent: for 9%, type `9`.

## Bulk edit
For example, to make all potions 20% cheaper:
1. Open "Vendor: prices" and pick the "Consumables" category.
2. Press "Bulk edit" and check the price field.
3. Choose "Percent", type `-20`, check the "was → now" list and press "Apply".

The edit applies to all rows of the filter. To pick particular rows, click their names in the table: Ctrl adds a row, Shift
selects a range. One Ctrl+Z undoes the whole bulk edit.

## Loot
A loot source (chest, monster, vase) rolls each of its **slots** separately. A slot that drops gives **one** item from its **pool**, by weight.

On the left pick a map and a source type, then a group. "No map of its own" means default loot: a source with no row for the current map
uses it. The group's card shows:
- **On average from one source:** how many items and how much gold one chest or monster gives, and what the kit had.
- A slot's **chance:** in percent, 0 to 100. Chance 0 removes the slot.
- An item's **weight:** its share among the pool's items. Weight 0 removes the item, but at least one item in the pool must keep a weight.
- **Count:** how many pieces. With several numbers, one of them drops, evenly.
- **Pool share** and **per source:** the result, with your changes and the season's rules.

Worth knowing:
- **A pool can be shared.** If the card warns that other groups use the pool, changing an item changes them all.
- **Season rules** can remove categories and multiply chances. The fields keep the template's values and the result is shown beside
  them. The rules themselves are in the "Loot: rules and modes" table.
- **Group names** come from the source type and the game's file names, like "Chests: Gold 01, Gold 02 and 8 more".
- **You can't add an item or a slot,** only change existing ones.
- **Drop per whole raid isn't calculated:** the kit doesn't know how many sources a map has.

## Colors in tables
- **Bluish** cell: editable. No highlight: reference only.
- **Yellow:** differs from the original game. The tooltip shows the value in the game.
- **Green:** the game has no such row.
- **Blue:** your change.
- **Red:** an error, the patch can't be saved with it. Yellow frame: a warning, you can still save.

## Errors and warnings
- **A number is required, a whole number is required.** The field doesn't accept such a value.
- **Minimum is above maximum.** Fix the other bound.
- **Changed more than 3 times, far above any value in the game.** You can save, but check for a typo.
- **The range doesn't divide by the step.** An item's value may miss the bounds: pick min and max as multiples of the step.
- **The value is outside the allowed limits.** For example, a chance above 100 or a negative weight.
- **No item would be left in the pool.** Keep a weight above zero on at least one item.

## What can't be changed here
- **Skill damage:** it's set in effects and curves inside the game, not in these tables.
- **What loot consists of:** new items and slots, which sources belong to a group.
- **Which properties an item has:** that's decided when the item is created.
- **Crafting time and market price limits:** the server doesn't use them for now, so they aren't in the kit.

The app also doesn't compute final damage or final hero stats. Curves show table values, without items. Changing min and max
also changes items already dropped.

## A new kit version
The kit is updated along with the server. Open the new archive and the app offers to move your changes:
- **Will move by themselves:** the value on the server didn't change.
- **Already in the new kit:** the change is no longer needed.
- **Needs a decision:** the value on the server changed. Pick yours or the server's.
- **Lost:** the row or field is gone. These changes stay in the old draft.

A patch is tied to a kit version. If the server was updated, open the new kit, move your changes and save the patch again.

## A patch from an LLM or Excel
You can still edit through `balance.xlsx` or ask ChatGPT or Claude: the instructions are in the kit archive. Open such a patch
with "Open patch" on "My changes". The app checks it and shows what fit.

## If something goes wrong
- **"This is not a balance kit".** Ask the server owner for the whole archive, not unpacked and not rebuilt.
- **"This file doesn't fit"** when opening a patch: it isn't a patch, or it's for another season.
- **The server refused the patch.** Most likely the tables have changed: open the new kit and save the patch again.

The app's data is in `%LOCALAPPDATA%\BalanceStudio`: copies of kits and drafts.
