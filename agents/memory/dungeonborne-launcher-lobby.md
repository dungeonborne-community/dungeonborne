---
name: dungeonborne-launcher-lobby
description: "FaceIt-style instance lobby in the launcher — user decisions 06.11, spec in LAUNCHER_SPEC §12, flag LAUNCHER_LOBBY"
metadata:
  visibility: team
  node_type: memory
  type: project
---

User decisions 06.11: 4 instance tabs, the player picks one; first a vote on group size, then on all 6 maps; a tab can be joined even while the hall is open; on a timer a waiting hall opens for 5 minutes; entry only with the native "Start" button in the game; whoever did not press it is simply not in the hall; the old search is replaced via the flag `LAUNCHER_LOBBY=1`.

**Why:** matchmaking moves into the launcher, but the user does not want the client auto-thrown into a match: the player presses "Start" himself.

Implemented locally 06.11 (backend, launcher 1.4.0 not published), the full loop verified on DS; solo is not let in when trio won. Test without the launcher: `$env:LAUNCHER_LOBBY='1'` + `run_local.ps1` + GM `lobby join|size|map|leave`; autoclient only with `window:true` (without graphics it closes).

**How to apply:** spec, what is verified and what is not: `docs/LAUNCHER_SPEC.md` §12. A "match found" push without search is not needed. An instance is a per-match process under ds-agent (`core/backend/ds-agent/README.md`): it loads the hall only on a `host` job, it does not need the roster in advance. My conclusion that an empty hall gets in the way was wrong: the user corrected it. Basis — the open hall [[dungeonborne-native-match-only]]. UI per [[dungeonborne-launcher-ui-style]].

**08.11 evening (user):** minimum per tab 4 (`LOBBY_MIN_PLAYERS`, default 4); a started vote is NOT reset when players leave — it runs to the end, the minimum holds the hall (HallHold). Do not bring back the rule "below the minimum — recruit again".

**09.11 (user):** minimum 2 players and hall timer 3 minutes — on prod via `dbserver.env` (`LOBBY_MIN_PLAYERS=2`, `LOBBY_WAIT_SECS=180`); the launcher shows a countdown from `ends_at` sent by the server, there is no timer in the launcher code.
