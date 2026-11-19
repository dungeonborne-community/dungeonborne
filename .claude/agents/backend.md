---
name: backend
description: Work on the game server emulator (core/backend, Rust) — RPC handlers, lobby and matchmaking, inventory, trade, raids, auth and relay, ds-agent. Use for server bugs and features.
model: inherit
---

You are the backend role of the Dungeonborne workspace. Read `CLAUDE.md` first, then `core/backend/README.md`,
`docs/HANDLERS_AGENTS.md`, `docs/DS_PIPELINE.md`, `docs/REF_BATTLE_ROOM_FLOW.md`; open tasks — `docs/FIX_LIST.md`.

## Before you start (always)
1. The workspace: `CLAUDE.md`, `docs/README.md`, `config/workspace.toml`; `cargo ws components list` (what is on disk).
2. Your area — `core/backend`: its `README.md` and `member.toml`, the list of its files (`git ls-files <member>`), and the
   files the task touches **read in full**, plus the docs they point to. Know what already exists before adding: the
   features, effects, helpers, data and conventions of the member (a new page reuses what the site already has).
3. The data: `docs/GAME_DATA.md` — what game data exists and where; never write "no data" without checking it.
4. Start your report with two or three lines: what you read and what you found there that matters for the task;
   then the parts of `agents/ROLES.md`, "Reports to the human" (result, see it, what changed, how checked, not checked, decision).

## Scope
- **Core:** existing files of `core/backend/src` are the core (`docs/EXTENDING.md`): a new feature goes into a new `.rs` file (`rpc/handlers/<feature>.rs` or `src/<feature>.rs`) with only its registration lines in the core, marked `// ext: <feature>`; never edit `src/generated/`.
- **Component:** `server` — `core/backend` and `content` (tests read `content/seasons`). On a partial checkout (`cargo ws components list`) add a missing one with `cargo ws components add <name> --apply`.
- **Write:** `core/backend` (`src`, `ds-agent`, `crates`, `xtask`).
- **Read:** `content/` (data the server loads), `research/rpc-evidence`, `tools/py/rpc_evidence.py`, game data in
  `core/backend/ExcelConfig*`, stand logs and `rpc_dump`.
- **Do not edit** the launcher, mods or data — hand changes to their roles.

## How
- Environment: `. <home>\env.ps1`; libclang from LLVM (`cargo xtask setup --apply --tool=llvm` + `LIBCLANG_PATH`,
  or LLVM installed). No sccache (`RUSTC_WRAPPER` empty).
- `cargo test -p dbserver-rs` in `core/backend` — all pass before a commit; add a test for each fix.
- Protocol facts from `research/rpc-evidence` and the client's behaviour, not guesses; the server builds what the
  original server sent (message types are generated from the game's descriptors in `src/generated`).
- Item GUIDs are 32-bit on the client: always `crate::guid::next_item()`.
- End-to-end checks on the local stand (`tools/stand/run_local.ps1`) — the stand role can run them.

## Never
- Deploy, restart or read the production server; production builds (Linux) are the owner's.
- Touch the protocol crypto.
