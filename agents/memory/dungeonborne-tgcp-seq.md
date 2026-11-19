---
name: dungeonborne-tgcp-seq
description: "CRITICAL — TGCP S2C seq is u32 BE at [9..13]; LE bug silently dropped ALL pushes after frame 256 (~130 s after Start) — game \"died\""
metadata:
  visibility: team
  node_type: memory
  type: project
---

**CRITICAL (user 05.11: "after 130 seconds the game was basically dying"; this fix closed many bugs).**
TGCP 0x4013 header: magic(2) 000b 000c cmd(2) | [8]=0 | seq u32 **BE at [9..13]** | [13..16]=0 | [16]=0x19 tag | body len BE [17..21] | flag [21..25].
Until 05.11 the server wrote seq LE at [12..16]: identical bytes up to 255, so from the 256th S2C frame the client read seq 0 and silently dropped every push (responses still arrived). Matching/room heartbeats (~2 frames/s) hit 256 about 1.5–2 min after "Start".

**Why:** cost a full day chasing hall timers/push order/defer. Many earlier conclusions "client ignores push / cause is in the client" (Sh4f, M9–M11, B7, B13 hang, P2/P3 character vanishes from lobby, mail list, friends online) are unreliable — recheck them. Workarounds added for it (CANCEL resend, defer settle/room) may be removable. Full list: K0 at top of docs/FIX_LIST.md.
**How to apply:** if pushes "stop" after some time, first check transport numbering in rpc_dump/OUR_tgcp.jsonl `hdr` vs the client's C2S encoding. Fixed in src/tgcp/mod.rs encode_tgcp_s2c (test s2c_seq_big_endian_at_9), verified locally 05.11 11:42; prod deploy needs user approval. Related: [[dungeonborne-client-room-lifecycle]], [[dungeonborne-matching-cancel]].
