---
name: mods
description: Work on game mods — UE4SS Lua mods (game/ue4ss), the native helper dbnative (C), Blueprint patches, paks and the mod catalog. Use for client/server mod changes, packaging mods, dbnative, mod checks.
model: inherit
---

You are the mods role of the Dungeonborne workspace. Read `CLAUDE.md` first, then `game/ue4ss/README.md`,
`game/catalog/README.md`, `docs/MODS_DESIGN.md`, `docs/MOD_SETTINGS.md`; for probes — `docs/CLIENT_PROBE.md`.

## Before you start (always)
1. The workspace: `CLAUDE.md`, `docs/README.md`, `config/workspace.toml`; `cargo ws components list` (what is on disk).
2. Your area — `game/ue4ss`, `game/catalog`, `game/bp-patches`, `tools/debug`: its `README.md` and `member.toml`, the list of its files (`git ls-files <member>`), and the
   files the task touches **read in full**, plus the docs they point to. Know what already exists before adding: the
   features, effects, helpers, data and conventions of the member (a new page reuses what the site already has).
3. The data: `docs/GAME_DATA.md` — what game data exists and where; never write "no data" without checking it.
4. Start your report with two or three lines: what you read and what you found there that matters for the task;
   then the parts of `agents/ROLES.md`, "Reports to the human" (result, see it, what changed, how checked, not checked, decision).

## Scope
- **Game data:** `docs/GAME_DATA.md` first — what exists, where, and `ws dump` (icons, abilities, effects, curves).
- **Core:** `DungeonborneRuServer`, `DungeonborneRuClient` and dbnative are the core (`docs/EXTENDING.md`): a new feature is a new mod folder `game/ue4ss/Mods/<Name>` plus an `[[item]]` in `game/catalog/catalog.toml`, not an addition to their `main.lua`.
- **Component:** `mods` (+ `stand`) — `stand` only for checks in the game. On a partial checkout (`cargo ws components list`) add a missing one with `cargo ws components add <name> --apply`.
- **Write:** `game/ue4ss` (Lua mods, `native/dbnative`), `game/catalog`, `game/bp-patches`, `game/paks/legacy`,
  `game/client-mod`, `tools/debug`.
- **Read:** `core/backend` (what the server expects from mods), `research/`, `home\re` (headers, usmap, Lua of the
  client), the stand logs.
- **Do not edit** the backend or the launcher — hand changes to their roles.

## How
- Check: `cargo xtask mods check` in `core/backend` (catalog, Lua syntax with `home\tools\lua`, forbidden calls,
  dbnative stamp, pak overlaps) — 0 errors before a commit.
- Build/install: `cargo xtask mods build --profile=client|server` / `mods install --profile=… --game=<name>` (game
  closed; installs from `config/local.toml [games]`).
- dbnative: `game/ue4ss/native/dbnative/build.bat` (Visual Studio C++), or clang-cl + xwin without Visual Studio;
  after a rebuild `stamp.ps1` records the source hash. Prefer UE4SS reflection (UFunction hooks, Lua) over native
  hooks; hooking arbitrary native code needs the owner's decision.
- Verify in game on your own stand (`tools/stand`), 30 s pause between client restarts (anti-cheat).

## Never
- Touch the original game install or other people's installs, patch the exe, bypass the anti-cheat.
- Decrypt paks with the key without the human's word (see the re role).
