# Audit of the legacy layer (what existed before the 30.10–02.11 rework)

Reason: on 02.11 we found that the RPC header (`core/backend/src/rpc/mod.rs` `RpcHead`) had been guessed — the field numbers did not match
`atframework.CSMsgHead`, and after the first push the client stopped matching responses to requests (FIX_LIST C6).
Everything below was written from traffic observations without the schema, and every part has to be checked against a reference.

References: `schema_dump.proto` / `Network.pb` (the client schema), PVE SoloLobby via the sniffer (`PVE_SNIFF`), the dump of the
official server `sniffer_log*.txt` (bodies only, no headers), a client C++ expert.
Tools: `rpc_dump/<TAG>_head.jsonl` (full `CSMsg` headers), `rpc_dump/<TAG>_tgcp.jsonl` (TGCP frames),
`python -I tools/compare_heads.py [--since HH:MM]` — PVE vs ours summary.

Status: ✅ verified, 🔧 fixed, ❓ not verified (a guess), ⚠️ known problem.

## 1. RPC header (`core/backend/src/rpc/mod.rs`) — verified against PVE 02.11 05:05 (`PVE_head.jsonl`, `compare_heads.py`)
- 🔧 `CSMsgHead`: error_code=1 (sint32), client_sequence=3 (echo of the request number), server_sequence=4 (counter of responses
  and pushes, strictly increasing — same in PVE). Checked in game 04:58: character switch ×3, creation, refusal -726.
- ✅ Client request: only `client_sequence` + `rpc_request{version "0.0.0", rpc_name, type_url}`.
- 🔧 PVE response: only client_sequence, server_sequence, timestamp, `rpc_response{version "0.0.0", …}`. Removed our
  session_sequence/sequence_ack/session_node_id/session_id=10002 (10002 — a user_id from someone else's PVE session), version "0.0.0".
- 🔧 PVE push: routed in `rpc_stream` (33), without client_sequence. We sent it in `rpc_response` (32) — fixed.
- ⚠️ Consequence of C6: `set_rpc_error` codes used to not reach the client, now they do — toasts may pop up where
  things used to pass "silently". Note them during tests. PVE does not check the character name — the -726 refusal is ours.

## 2. TGCP transport (`core/backend/src/tgcp/`, `core/backend/src/gateway.rs`) — verified against PVE (`PVE_tgcp.jsonl`)
- ✅ The client does not send the TGCP ping `0x6001` at all; the heartbeat is the RPC `ping` (`CSPingReq`/`SCPongRsp`) every
  `heartbeat_interval` seconds. 🔧 We returned `heartbeat_interval: 30000` (official and PVE — `30`) → the client never
  pinged us. Fixed in `login` together with `version` (= the client's package_version, was protocol_version),
  removed `reconnect_uid`/`max_session_sequence` (the official one does not have them).
- 🔧 `0x2002` — now a PVE frame with the client's open_id (`tgcp::auth_rsp_2002`), tokens `LOCAL_ATK`/`LOCAL_RTK`. It used to be
  someone else's capture (`dump_0008_GATEWAY_S2C.bin`, open_id `76561198000000901`, binary tokens, the glued `0x6002` had
  an extra 48-byte tail) — one for all clients.
- ✅ S2C numbering in [12..16] starts at 4 (after 0x1002/0x2002/0x6002 = 1/2/3), C2S is encrypted (0x300), S2C is plain — as in PVE.
- ⚠️ `0x1002`: one AES key for all sessions (PVE has one per session) — tolerable for local, for a server — generate it.
- ❓ `open_id` from `0x1001` is taken from [37] dropping non-digits (at [37] is the length byte 0x12) — works, but fragile;
  the gateway's fallback open_id is the same foreign `76561198000000901`.
- ❓ `0x5001` eats the whole buffer; an unknown cmd → buffer reset.

## 3. DirServer (`core/backend/src/dir.rs`), HTTPS login (`core/backend/src/auth_http.rs`)
- ❓ The `0x4002` response is a 281-byte template (`dump_0001/0003_DIR_S2C.bin`), the url is substituted.
- ⚠️ L3: the client hammers HTTPS 443 every 0.5–2 s; `auth_http` does not log requests. There is a reference:
  `tools/probe/pve_login_http` (PVE LoginService with `-out`) — compare paths and responses.

## 4. Identifiers and data
- 🔧 item guids < 2³¹ (C1, `core/backend/src/guid.rs`).
- ⚠️ C4: `user_id` = 10 000 000 + the last 6 digits of the SteamID — collisions; `role_guid` = user_id·100 + n — will overflow
  32 bits when user_id > 21 474 836.
- ❓ `zone_id` = 1 everywhere (PVE has 2200) — the client apparently accepts any; keep it the same in all keys.
- ⚠️ GameData: 18 tables lose rows (HashMap on a non-unique key; `tools/py/gamedata_dup_keys.py`), incl.
  `role_level_upgrade` → level 20 stats.
- 🔧 Player nickname = the hall role name (as in PVE). 🔧 Character name is unique across the DB (-726).
- ⚠️ C5: DB is 190 MB for 6 players; C2: player save without locking.

## How to recheck
1. Capture PVE with headers (captured 02.11 05:05): `tools/py/pve_capture_admin.ps1` (admin), backend `PVE_SNIFF=127.0.0.1`, in game — login,
   character switch and creation, stay in the lobby ≥ 1 min (pings), exit. You get `PVE_head.jsonl`, `PVE_tgcp.jsonl`.
2. The same scenario on our backend (`RPC_DUMP=1`).
3. `python -I tools/compare_heads.py` — discrepancies per field/frame → fixes per the items above.
