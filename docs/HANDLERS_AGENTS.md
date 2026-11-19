# Brief: implementing RPC handlers (parallel agents, 30.10)

A Rust emulator of the Dungeonborne game backend (UE 5.1). Root — `<repo>`. Not a git repository;
backup before starting work — `src_before_handlers_20261030.zip`. Run commands from the root.

## How it works
- `core/backend/src/gateway.rs` → `game_data::route_rpc` (`core/backend/src/generated/generated_server_code.rs`) → `impl RpcHandlers for GatewaySession`
  in `core/backend/src/rpc/handlers.rs`. A handler returns `Option<Rsp>`; `None` = no response is sent at all (the client waits for a timeout) —
  almost always answer with `Some(...)`. Return errors through the response's `ret_code` (codes — `core/backend/ExcelConfigjson/error_code.json`);
  if there is no `ret_code` field — `self.set_rpc_error(code)` (goes into the response's `RpcHead.error_code`) + an empty response.
- All handlers live in domain files `src/rpc/handlers/<domain>.rs`: `handle_<rpc>` in handlers.rs only
  delegates to `<rpc>_impl` (since 31.10 — all 230, including login, profile, match and DS). Besides the delegates handlers.rs holds
  only `GatewaySession` and `push`/`clean_open_id`/`now_ts`. A new RPC: `<rpc>_impl` in the domain file + a delegate in
  that file's section in handlers.rs. Files without the `_handlers` suffix (`common.rs`, `affix.rs`) are helpers only.
- Style and format example: `core/backend/src/rpc/handlers/inventory_handlers.rs` (module doc comment with facts about the format and
  sources, comments in Russian, no filler), `lobby_handlers.rs` (pushes to other players through `lobby::hub`).
- Shared helpers — `core/backend/src/rpc/handlers/common.rs` (read only): `load_me()`, `save_me(&p)`, `load_by_user_id(uid)`,
  `push_dirty(ScPlayerDirtyChgSync)`, `common::push_to(open_id, method, type_url, &msg)`, coins `coin/add_coin/coin_dirty`
  (1001 = gold, stored in `state["gold"]`). Also `self.push(method, type_url, msg)` (push to yourself after the response),
  `self.clean_open_id()`, `GatewaySession::now_ts()`, `self.game_data.<table>` (tables from `core/backend/ExcelConfig/*.bytes`,
  JSON copies in `ExcelConfigjson/`), `self.db.get/save/find_by_user_id` (SurrealDB, schemaless).
- Player model `src/structures.rs::PlayerModel`: `roles` (`tgf::DUserRole` in `data`), `stash`, `state` (JSON for
  everything else). New player data — in `state["<domain>_..."]`; shared data (guilds, orders, mail) — their own DB tables
  with the domain prefix (`guild_*`, `trade_*`, `mail_*`...).
- Push: `method = "tgf.GameClientService.<rpc>"`, `type_url` = full proto name (`tgf.SCPlayerDirtyChgSync`, case as in
  `route_rpc`/`Network.pb`, NOT the Rust type name). 20 methods with `req: Empty` (`*_sync`, `player_dirty_change`, `modal_message`,
  `login_notify_sync`...) are server→client pushes; they are not implemented as handlers but sent via push.
- After changing player data send `ScPlayerDirtyChgSync` with the needed `dirty_*` (as inventory_handlers does).

## Sources of facts (do not invent the format — look first)
- `python -I tools/rpc_evidence.py <rpc|prefix_>` — summary: template, proto names, our stub, `rpc_dump/` dumps
  (PVE_* — the reference PVE server SoloLobby, OUR* — ours), sniffer of the official server (decoded JSON), tables,
  mentions in UE dumps. `--no-sniff` is faster, `--max N` block length.
- `python -I make_llm_prompt.py <keys> --stdout [--ue] [--gamedata]` — all related protobuf structs/enums recursively.
- `cargo run --quiet --example sniff_decode -- <method substring>` — sniffer (`sniffer_log*.txt`) to JSON.
- `rpc_dump/<tag>/*.json` — full PVE/OUR streams (look at the order of pushes around a method).
- `core/backend/src/generated/tgf.rs` (types), `schema_dump.proto` (proto with comments/enums), `Configure_decoded.txt`.
- Client UE dumps: `ue4ss/CXXHeaderDump/*.hpp`, `ue4ss/UHTHeaderDump/` (TGFGame, TGFBattleProtocol — client UI/logic,
  which response fields it reads), `ue4ss/DumpGame/`. BP bytecode — `retoc` + `kismet-analyzer` (`<data>\tools_re`;
  AES key in `<data>\aes_key.txt` — read it into a variable, never print or save it).
- The client's original Lua scripts (bytecode, strings are readable): `pakchunk0-Windows.pak`, unpack with
  `repak.exe --aes-key <key from aes_key.txt> unpack -i Dungeonborne/Content/Script/<Folder> pakchunk0-Windows.pak`
  (`repak.exe` — `<data>\repack\repak.exe`, the pak — in the game install's Paks). Chat/Social are already unpacked:
  `tools/research/social/lua_orig/`. There you can see which response fields the client actually reads.
- `state_and_data_cheatsheet.md`, `db_json_cheat_sheet.txt`, `llm_context_*.md`, `BACKLOG.md` — notes from earlier work.
- You may write your own fact-gathering scripts: `tools/research/<domain>/` (not in `src/`). If one is generally useful, describe it in the report.

## Rules for parallel work
- Edit ONLY your own domain file (plus those explicitly allowed in the task). Do not touch `handlers.rs`, `common.rs`, other agents'
  domain files, `core/backend/src/generated/*`, `Cargo.toml`. Need a helper — write it in your file: free module functions or
  `impl GatewaySession` methods with the domain prefix (method names are shared across the whole crate — otherwise a conflict).
- Build: `cargo check --message-format short` from the root. Ignore errors in other files (others fix them), retry
  in a minute. Keep your own file compiling — change it in portions.
- Do not run the server/game, do not kill processes, do not install crates.
- No facts about the format — minimal plausible logic + `// TODO(verify): what to check in game`. Purely internal
  RPCs — a short `ret_code: 0` with nothing extra.
- Do not break working flows (login, lobby, matchmaking, match, inventory).

## Report (last message, ≤ 400 words)
A table for each RPC: `done (format source)` / `stub (why)` / `needs an in-game check (what)`;
new DB tables and `state` keys; which pushes you send; open questions; scripts created.
