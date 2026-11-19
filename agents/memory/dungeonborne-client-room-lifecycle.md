---
name: dungeonborne-client-room-lifecycle
description: "How the client exe forgets a battle room / match (dirty_clear_battle_room), why lobby shows \"Match found\" or reconnects; analysis in research/battleroom"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Exe analysis 04.11 (Ghidra): `<data>\research\battleroom\README.md`. Waiting room on the DS side:
`research\hallleave\README.md`.
- URoomService keeps battle_id (+0x40). `battle_room_heartbeat` runs while battle_id is non-empty, and the client ignores the heartbeat response.
- The room is forgotten ONLY via:
  - `SCPlayerDirtyChgSync.dirty_clear_battle_room` (field 21) — silent;
  - CANCEL (event 18 / snapshot 11) — shows a reason popup.
  Event finish(19), FINISHED and TIMEOUT do nothing.
- A `SCBattleRoomLogSync` that carries a snapshot makes the client ignore its event_logs. Every sync re-writes battle_id.
- Lobby entry (0x145993920):
  - room with status 8 → reconnect to the DS (game «hangs»);
  - any other room → MatchToast Matched + MatchStart.
- Backend fix M10 (orchestrator, 04.11): room_clear_push after room_finish_push.

**How to apply:** for symptoms like "Match found" in the lobby or an endless room heartbeat, first check that clear was sent, before re-reversing. Related: [[dungeonborne-client-probe]], [[dungeonborne-bp-reverse]].
