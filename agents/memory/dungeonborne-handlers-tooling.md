---
name: dungeonborne-handlers-tooling
description: "RPC handler layout (domain files + delegation) and fact-gathering tools: rpc_evidence.py, sniff_decode, agent brief"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Set up 2026-10-30 for implementing all handlers from `handlers_template.rs.txt` with 9 parallel agents:
- Layout (since 2026-10-31 for ALL 230 RPCs): `core/backend/src/rpc/handlers.rs` = GatewaySession + push/clean_open_id/now_ts +
  `impl RpcHandlers` of one-line delegates `self.<rpc>_impl(req).await`, grouped by `// ---- … (<file>.rs) ----`.
  Logic in `src/rpc/handlers/<domain>_handlers.rs`: account (login, player_get_info, cache), role (create, starter gear,
  base attrs), battle (level_select, matching, room), raid (report_action, settle, exp, p2p host), ds, inventory, lobby,
  item, player, progress, shop, social, guild, room, trade, misc; helpers-only `common.rs`, `affix.rs`. New domain
  files use `use super::*;`. Agent rules: `docs/HANDLERS_AGENTS.md`.
- 20 template methods with `req: Empty` (`*_sync`, player_dirty_change, modal_message…) are server→client pushes, not handlers.
- Fact tools (run from backups root): `python -I tools/rpc_evidence.py <rpc|prefix_>` (template, proto names, stub,
  rpc_dump PVE/OUR, decoded sniffer, tables, UE dump hits); `cargo run --quiet --example sniff_decode -- <method>`
  decodes `sniffer_log*.txt` (official/other server traffic) to JSON via Network.pb; `make_llm_prompt.py <keys> --stdout --ue`.
- Outcome 2026-10-30: all 185 implemented, untested in game; status + test checklist in `docs/HANDLERS_STATUS.md`.
  Errors for RPCs without ret_code go via `set_rpc_error` → RpcHead.error_code. Client's original Lua (bytecode,
  strings readable) unpacks from pakchunk0 with `<data>\repack\repak.exe` → `tools/research/<domain>/lua_orig/`.
  `tools/py/gamedata_dup_keys.py`: 18 GameData tables lose rows (HashMap on non-unique key; role_level_upgrade → lvl-20 stats).
- Backups: `src_before_handlers_20261030.zip` (before agents), `src_before_split_20261031.zip` (before handlers.rs
  split). Folder is not a git repo.

See [[dungeonborne-workflow]], [[dungeonborne-dedicated-plan]].
