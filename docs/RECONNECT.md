# Reconnecting to a match: how the client does it (analysis 03.11, Ghidra)

Source: exe (image from a dump, `docs/ghidra_startlocalbattle`), game Lua (`research/lua_all`). Lua takes no part in
reconnecting — everything is in C++ `URoomService` (room/DS) and `UBattleService`.

## Client

`URoomService` (GameInstanceSubsystem) keeps the current room: `+0x50` — room, **`+0xd0` — `EnBattleRoomStatus`**,
`+0x40` — battle_id, `+0x148` — DS address, `+0x178` — reconnect counter, `+0xd5` / `+0x168` — "don't
reconnect" flags (probably the match is settled / the player left on their own — not checked). URL to the DS: `%s?OpenId=%s?token=%s?%s`.

The decision on returning to the hall/lobby — `UBattleService` 0x145992500 (called by 0x1459934b0 and 0x145994420):

| Room status (`+0xd0`) | Flag `+0xd5`/`+0x168` | What the client does |
|---|---|---|
| 8 `BATTLING` | no | attempts < `DSMaxReconnectTimes` → `ReconnectDS` **by itself** (0x145a1bfd0); otherwise the window "Reconnect to the match?" (texts `ReconnectDSMessage/Confirm/Cancel`): Yes → counter+1, `ReconnectDS` (0x145a1c870); No → `CancelReconnectDS` |
| 8 `BATTLING` | yes | close the room (0x1459ee130, internal state 13), regular hall |
| 2–7 (before the match, incl. 7 `LOADING` — our waiting hall) | — | no reconnect: the regular path to the hall (match checks etc.) |
| others (9 `FINISHED`, 10, 11) | — | regular hall |

`DSMaxReconnectTimes` — from `DSConfig.json` (client config, `UTGF…` JSON settings), default **10**
(0x1459e8440: `count(+0x178) < max`).
`ReconnectDS` sends `tgf.GameClientService.network_block_loading` (`CSNetworkBlockLoadingReq {ds_region, ds_address,
battle_id, loading_cnt}`, the response is empty — it's a notification/telemetry) and connects to the DS with the same URL with OpenId and token.
Hence `loading_cnt 1→5` in rpc_dump 03.11: without a "match over" event the client considered the room `BATTLING` and went back.

There is no `IsDsMode` gate in the client part.

## Server (our listen)

- The native re-entry check (PreLogin → validator "Player Game Status Invalid") — only with dedicated + DS SDK;
  for us its role is played by the mod's JoinGuard: entry is allowed while the OpenId has no taken result.
- The game keeps PC/PlayerState for a disconnected player (the reconnected player gets the same ones, without a new `InitPlayerGameData` —
  seen on 02.11 and 03.11). They must not be deleted (03.11: the server crashed). How long they live is not measured.
- Result on no-show: the native one is not taken (the `UBattleDsUtility` record stays in status 1) — check with an experiment
  (client `quit` mid-match → look at `[GAMEOVER]`). Proposal: when the room closes on the offline timeout the mod
  takes the result of those left without a connection (like `leave` — `ServerSelfQuitGame` on their PC), or the backend computes their
  exit itself (SELF_QUIT without items / death) — to decide.

## Waiting hall

`BP_TgfWaiting.OnPlayerLogout` only updates the numbers (`OnUserCountChanged`); it doesn't remove anyone from Total and doesn't
wait for them back. A client that dropped out of the hall (room status 7 `LOADING`) does **not** reconnect by itself — so for the backend it is
`absent` (the mod counts it that way: absent = whoever is not in the hall at the moment of moving to entering).

## What is needed from the backend (proposal)

1. On login (`player_get_info`, `need_user_battle_room`) — `user_battle_room` for a running room without this player's result:
   `DBattlePlayerBattleRoomData { room_key, room_data, participator, acknowledge_event_id, battle_room_status = 8 }`
   with the same data as in the room events (DS address, player key/token from dsInitFinish). On entering the hall the client
   will see `BATTLING` and go by itself. Which exact fields RoomService takes for the address (`+0x148`) — check in the game.
2. `battle_room_heartbeat` of a running room — status `BATTLING` while the player has no result; after the result — events 5/6
   (already done 03.11), after which the client doesn't reconnect.
3. `network_block_loading` — empty response (notification).
4. Keep the room until the reconnect timeout (now offline 300 s) — longer than 10 auto-attempts.

## Backend (03.11, not checked in the game)

- **03.11, user's decision: a "Return to the match" button, no automatic move.** `user_battle_room` on login is NO longer
  returned: with status 8 the client goes to the server by itself. Return is via the match button: `battle_matching_search` of a player
  whose match is running and has no result doesn't put them in the search, but returns their match's `matching_id`, `matched` again (CAN_JOIN_IN)
  and room events 1–4 (the same server and token) → the client goes the native way into its PC/PS (`matchmaking::rejoin`).
  Checked by an agent on 03.11 12:49–12:53 (`dbtestjtest.ps1`): after a restart it doesn't go by itself, via "To Battle" — into the same
  PC/PS. There is no label on the button (no native flag for Lua); instead, on login, a `SCModalMessageSync` window
  "The match is still running… press "To Battle"". Party (teammate in the match, the leader presses "To Battle") not checked. A network drop mid-match — native
  automatic move, can't be turned off by config, left as is.
- (before 03.11, cancelled) `player_get_info.need_user_battle_room` → `user_battle_room` (`lobby_handlers::reconnect_battle_room`). It was returned
  if the player's match was running:
  - they are `Matched` in a `DsReady` room;
  - they have no result;
  - the server is already in the match: without a hall — right away, with a hall — after `phase=battle`.

  Contents: `room_key`, `room_data` (`ds_address`, `map_data`), `participator` — with `ds_player_key` and the same token
  as in dsInitFinish, `acknowledge_event_id: 4`, `battle_room_status: 8` (BATTLING). Otherwise empty, as before.
- `network_block_loading` — empty response, as before.
- The room waits for a disconnected player until the end of the match (03.11, user's requirement: in the original you can return at any
  point of the match). `DS_ROOM_OFFLINE_SECS` defaults to 0 = off; the end — results for everyone (death zone) or `DS_ROOM_TTL`.
- After the result — room events 5–6 and `dirty_clear_matching`: the client stops going to the server and closes the search.
- The result of someone who didn't return — not by offline time, but by the character's state: killed without a connection (death zone, mob) —
  the mod takes the result right away (death); alive — waits for the return. The early "exit on behalf of the disconnected player" after 300 s is cancelled.

## Clarifications 03.11 (requirement: reconnect during the whole match, by button)

- The original has no separate "return to the match" button: `WBP_MatchMainButton.lua` (original, `research/lua_all`) has no such
  state, only `BackToRoomButton` for PVP rooms. The original: silently up to `DSMaxReconnectTimes` attempts, then
  `WBP_AlertWindow` "Reconnect?" (Yes/No).
- `DSConfig.json` is in `pakchunk0-Windows.pak` as `Dungeonborne/Content/AdditionalData/DSConfig.json`, it has no
  `DSMaxReconnectTimes` key (default 10). Our pak `zzzzz_RebuildPrivate_P` can put its own file with
  `"DSMaxReconnectTimes": 0` — then on entering the hall/lobby with a `BATTLING` room the Yes/No window appears right away, without an automatic move
  (0x1459e8440: `counter < max`).
- "No" (`CancelReconnectDS` 0x1459e8550) sends a message with the room key and code 12 (0x145a1d8a0; closing the room
  by the client 0x1459ee130 — code 13); whether it erases the room on the client can't be seen from the code, check in the game. Returning later:
  on the next entry to the hall/restart the client will get the room again (`user_battle_room` / heartbeat) and ask again.
- `ADeathZone` (shrinking zone, `DeathZoneTime`, `EDeathZoneRuntimeState`) — the native end of a raid: whoever didn't extract dies
  in the zone → a regular death → record status 3 reason 2 → the result goes out via the mod (cache dump happens before the DS gate). Whether
  the zone works on listen and whether a disconnected player's abandoned pawn dies — check with an experiment.
- How long a disconnected player's PC/PS live is not measured (seen ≥ 1 min). To check: disconnect a client for 5–10 min.

## Experiments 03.11 12:26–12:39 (`tools\stand\dctest.ps1`, `tools\stand\rctest.ps1`, backend `DS_WAITING_ROOM=1`)

- **Drop mid-match (client `quit`)**: after 60 s the server still has the PC (local=false), a PlayerState with the real
  OpenId and a **live pawn** (hp 1057); the `UBattleDsUtility` record — status 1. The pawn can be killed (death zone GE).
- **Death of a disconnected player** → the record immediately (1 s) has status 3 reason 2 → the `death` result goes out via the mod, without any exit.
- **Death of a connected player (without auto-exit, `killstay`)** → the same: status 3/2 after 2 s, result `death`. So the native
  death takes the result by itself (on 02.11 it seemed an exit was needed) — the rule "disconnected+dead → exit on their behalf" is not needed.
- **`disconnect` on the client mid-match** (with `DSConfig.json` from our pak, `DSMaxReconnectTimes: 1`): both times the client
  reconnected by itself in ~6–7 s, didn't enter the hall, there was no window; it returned into the same PC/PS, data and result fine.
  Whether our `DSConfig.json` applied and whether this is that path (`ReconnectDS` with the counter) can't be seen — we need `loading_cnt` from
  `network_block_loading` in the backend log. Perhaps the counter resets after a successful reconnect — then
  the window appears only on a failed attempt.
- The client doesn't accept the value 0 in `DSMaxReconnectTimes` (only > 0 is taken, otherwise 10).
- Commands added to the mod: `killstay <open_id>` (death without auto-exit), `exec <console>` (for autotests).

## Why there was no window (Ghidra, 03.11 after the experiments)

- **Two branches.** A drop mid-match (`UBattleService +0x38 == 2`, room `BATTLING`) → 0x145993920 → `ReconnectDS`
  right away, **without a limit** (the window never appears). Entering the hall/lobby (client restart) → 0x145992500 → limit.
  The `exec disconnect` test took the first branch (backend: `loading_cnt` 1 and 2 — the counter doesn't reset).
- **`DSMaxReconnectTimes` is not from `DSConfig.json`.** `DSConfig.json` is read by 0x1459f3000 (regions, speedtest, gromelink).
  The limit comes from a subsystem with a "key → string" map (0x14598a290 + Atoi), filled by the remote config
  `ds_config_url` from `Content/AdditionalData/ServerConfig.json` (`https://f.tgf123.com/ds_shipping.json`; ServerConfig
  parsing — 0x14597fad0). The response format and who downloads it — not found (probably a flat JSON object).
  Our `DSConfig.json` was removed from the pak.
- A "window right away" after a restart can't be had by config (at least one auto-attempt: `counter(0) < max(≥1)`).
  Without writing to memory — via backend + Lua: don't return `user_battle_room` on login, and on the match button in the hall
  push the events of the player's own room (dsInitFinish + startBattling) instead of a new search.

## Return via the "To Battle" button — checked (03.11 12:49, `tools\stand\rjtest.ps1`)

The backend doesn't return `user_battle_room` on login; the search of a player with a running match → matched + events of their own room.
Run: match → client `quit` → restart, 40 s in the lobby — doesn't go by itself → "To Battle" → after ~10 s on the server in the same PC/PS
(without a new `InitPlayerGameData`, hero and items from the match) → death → result `death` → reset. Party (teammate in the match, leader
in the lobby) — not checked. There is no "Return to the match" label on the button yet (no field for Lua without a native meaning).
