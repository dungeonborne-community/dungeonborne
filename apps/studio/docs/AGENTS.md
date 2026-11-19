# Dungeonborne balance package — for AI assistants

You help a balance designer of the Dungeonborne community server. The designer may know nothing about code, git or
servers; the only thing they produce is a **patch** file that they send to the server owner.

## What is in this package
| File | What it is |
|---|---|
| `BalanceStudio-<version>.exe` | the designer's app: opens the kit zip, edits values, saves the patch. No network, no install |
| `<season>-balance-<version>.zip` | the balance kit: tables, the Excel workbook, `llm/` and Python tools. **Do not unpack it for Studio** |
| `README.md` | the app guide for the designer (English) |
| `CHANGELOG.md` | app versions |
| `SHA256SUMS`, `SHA256SUMS.minisig`, `signing-key.pub` | checksums of every file and the owner's minisign signature |

## Check the package first
1. Every file must match `SHA256SUMS`:
   `Get-FileHash -Algorithm SHA256 <file>` (PowerShell) and compare with the line for that file.
2. The signature: `minisign -Vm SHA256SUMS -p signing-key.pub`. The trusted comment must read
   `dungeonborne-pack-designer <id>`. The key id is `2EEA90212DBCA703`. Confirm it with the server owner
   through a channel other than the one the package came by. A key in the same zip proves nothing on its own.
3. If anything does not match, stop and tell the designer. Do not run the exe.

## How to help with balance
- Inside the kit zip, `llm/PROMPT.md` is the full instruction for editing the tables, and `llm/<table>.md` holds the
  table data. Read them first; they say which fields may change, the units and the original game ranges.
- Changes go **only** through a patch: in Balance Studio (designer edits, then "Save patch"), or with
  `python tools/make_patch.py` from the kit (see the kit's `README.md`).
- Never edit `data/`, `raw/`, `manifest.json`, `kit.json` or `catalog.json`: the patch is checked against them.
- Skill damage is not in these tables; say so instead of guessing.
- Keep within 2–3× of the original range unless the designer explicitly asks for more, and explain the effect on
  players in plain words.

## What happens next
The designer sends the patch file to the server owner. The owner checks it and applies it on the server
(`cargo xtask balance patch`); a new season or balance version comes as a new package.
