---
name: dungeonborne-matching-cancel
description: Client closes a match search ONLY on SCBattleMatchingLogSync event CANCEL (next event id); FINISHED and dirty_clear_matching do not close it
metadata:
  visibility: team
  node_type: memory
  type: project
---

04.11: the client's UMatchService keeps a search open (battle_matching_heartbeat every 1 s, "Start" dead with
`already in matching`, "Match found" toast after hall exit) until it gets `SCBattleMatchingLogSync` with an
**event** CANCEL whose event_id = last sent + 1. Reference (sniffer_log_001 lines 124→171): 1 startMatching,
2 matched (CAN_JOIN_IN), 3 finished, then ~10 s later 4 CANCEL `reason -5012` (MATCHING_DESTROY), snapshot status
CANCEL, last_event_id 4. A snapshot without event_logs, FINISHED, or `dirty_clear_matching` do NOT close it.
Reasons: EnMatchingExitReasonType (-5014 self quit, -5017 waiting-room self quit, -5016 join hall failed).

The **battle room** is separate (agent's exe analysis, research\battleroom\README.md, FIX_LIST M10): URoomService
sends battle_room_heartbeat while its battle_id is set and ignores the reply; it forgets the room ONLY on
`SCPlayerDirtyChgSync.dirty_clear_battle_room [battle_id]` (silent, also MatchStop = search reset) or a CANCEL
event/snapshot (shows a reason window). Finish/FINISHED/TIMEOUT/game_over_settle do nothing; if a SCBattleRoomLogSync
has a snapshot its event_logs are ignored; any later sync for that battle_id re-opens the room. Backend:
`lobby_handlers::room_clear_push` after room_finish_push and on heartbeat of closed/unknown room. The timed CANCEL
after FINISHED was dropped (it showed "search cancelled" inside the hall).

Backend since 04.11 10:05 UTC: `lobby::matchmaking::note_match_event` / `match_cancel_msg`, sent from the matching
heartbeat (10 s after FINISHED, or at once if the search is unknown), `battle_matching_exit`, `room_finish_push`.
FIX_LIST M9.

**Why:** months of "search stuck / Start does not work / Match found" bugs (M7, Sh4b(7), LOCAL's room-numbering theory)
were this one missing event.
**How to apply:** any new path that ends a match (party leave, kick, timeout) must send this CANCEL event; when a
client "won't leave matching", check that a numbered CANCEL went out before touching anything else.
Related: [[dungeonborne-native-match-only]], [[dungeonborne-handlers-tooling]].
