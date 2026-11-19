# Client analysis: how to find out what the game does

How the game's rules (skills, items, loot, UI, protocol) are researched, with what and in which order. Written from the work of
the 17.11 sessions. A research conclusion that became a decision goes to `research/<topic>`, `docs/*.md` or memory in `agents/memory`.

Where each kind of game data already is, and `ws dump` (icons, monster and hero abilities, effects, curves from
your own game copy in one command) — `docs/GAME_DATA.md`. Look there before unpacking by hand.

## Paks first, then native code

Almost everything is visible in the game files — the client paks: tables, Blueprints, UI and service Lua scripts, skill
assets. Native code (Ghidra) — only when a rule lives in the client's C++ (`research/ghidra/README.md`).

| Step | With | What you get |
|---|---|---|
| Find an asset | `retoc list --path <utoc>` (`home\tools\retoc`) | package names in IoStore |
| Unpack | `retoc to-legacy --version UE5_1 --no-shaders --no-script-objects --filter <name>` (`--aes-key` — the key from `local.toml [re] aes_key`) | regular `.uasset/.uexp`; without `--no-shaders` — shaders too (that is how the invisibility material was analysed) |
| Regular paks | `repak` (`home\tools\repak`) | ini, fonts |
| Assets to JSON | `UAssetGUI tojson <in> <out> VER_UE5_1 dungeonborne` (the third argument is the mapping **name**, not a path to the usmap; otherwise RawExport) | GameplayEffect, DataAsset, AnimMontage with numbers |
| Blueprint logic | `kismet-analyzer gen-cfg` | BP bytecode graph: what each node calls, which native function |
| Client Lua | `luac54 -l -l <file>` (`home\tools\lua`) over the export in `home\re\lua_all` | UI, services, tooltip formulas (Script/Text) |
| Types and fields | usmap and CXXHeaderDump from `home\re\ue4ss` | classes, fields, enums (LyraGame.hpp, TGFGame.hpp, `*_enums.hpp`) |
| Tables | `content/balance` (JSON/CSV), `core/backend/ExcelConfig` | numbers, AbilitySet → GA → GE links |
| CurveTable | UAssetGUI → `python -I tools/py/curvetable.py <json> --grep <skill>` | skill numbers by level (UAssetGUI does not parse CurveTable rows itself) |

The typical path for a skill or an item: balance CSV → AbilitySet → GA / AnimMontage in UAssetGUI → GE → numbers.
If the rule is in packed native code — mark it "not checked in a match" until it is checked in game.

In-game checks — with a mod on your own stand (`game/ue4ss/Mods/ClientProbe`, `DungeonborneStatWatch`; `tools/stand`).

## What comes from where

- **Programs:** retoc, repak, UAssetGUI, kismet-analyzer, FModel, Lua (luac54) — `cargo ws bootstrap` per
  `config/tools.lock` into `home\tools`. Ghidra + JDK — on request (`research/ghidra/README.md`).
- **Analysis data:** usmap, CXXHeaderDump, the client Lua export — artifact store artifacts (`config/artifacts.toml`,
  `home\re`), they come with `ws bootstrap`. With your own copy of the game you can capture them again (UE4SS: Ctrl+Numpad6 — usmap).
- **Pak AES key** — the secret `game_aes_key` in the team database. `ws dump` takes it from the vault session
  (`cargo ws secrets unlock`); for another tool — `cargo ws secrets run game_aes_key -- <command>` (the password each
  time). Since 18.11 it is no longer copied into `config/local.toml [re] aes_key`; `ws doctor` notes an old line there.
- **Scripts:** `research/castle_spawn/tools/pkg.py` (package names and imports), `tools/py/curvetable.py` (CurveTable
  rows from UAssetGUI JSON — skill numbers), `tools/py/mdinfo.py` (crash minidump: exception, registers,
  module+offset), `research/ghidra/scripts` (dump, image, function search), `tools/py` (protocol, loot, stats).

## Rules

- Decrypting paks with the key and analysing the INTL login — **only on a person's word**, even when the key is at hand and even if
  another session asks. A person asking to analyse a skill or an item is that word.
- Do not read live process memory, do not patch the exe or unpack it to disk, do not bypass the anticheat, do not touch the
  protocol cryptography (`CLAUDE.md`).
- Asset exports, JSON and decompilation — in a temp folder or `home\re`, not in git (they are game files); git gets the conclusions
  and the scripts.
