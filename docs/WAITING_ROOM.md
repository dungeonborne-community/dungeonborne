# Waiting room → match: how it works in the game and what to do on listen (analysis of 03.11)

Sources: assets (retoc + kismet-analyzer: `BP_TgfWaiting`, `B_Tgf_Waiting_Experience`, `L_Dungeon_Matching_01.umap`,
`B_LyraGameMode`), exe (Ghidra on the image from the dump, `docs/ghidra_startlocalbattle`), Lyra source.

## Map and Experience

| Map | WorldSettings |
|---|---|
| `/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01` | `LyraWorldSettings`: GameMode `B_LyraGameMode`, Experience **`B_Tgf_Waiting_Experience`** |
| `L_Dungeon_Official_01_Classic` (match) | `B_LyraGameMode`, Experience `B_Tgf_Game_Experience` |

The waiting room is configured: Lyra takes the Experience from WorldSettings (`ALyraGameMode::InitGame`: URL `?Experience=` → command line →
WorldSettings → default). Nothing needs to be set.
`B_Tgf_Waiting_Experience`: hero `BP_Tgf_Hero_Default` / `HeroData_HeroSample`, components `BP_TgfWaiting`,
`BP_Tgf_MultiTeams`, `BP_TgfGame_MapHint` (on GameState), HUD `LAS_TgfWaiting_StandardHUD` (`WBP_TgfWaitingGameWidget`).
`B_LyraGameMode` does not change `bUseSeamlessTravel` → engine default `false` (unless TGF C++ enables it itself —
not verified).

## Chain in the game (dedicated)

`BP_TgfWaiting` (GameState component, logic on the server):
1. After `WaitForExperienceReady` it subscribes to `UBattleDsUtility` delegates: `RoomReadyDelegate`,
   `PlayerLoginSuccessDelegate`, `PlayerWaitingRoomLogoutDelegate`, `WaitingRoomLeaveDelegate`;
   if `IsRoomReady()` is already true — `OnRoomReady` right away.
2. `OnPlayerLoginSuccess(Character, Current, Total)`: the hero gets tag `TGF.Procedure.WaitingRoom` and "InvSee",
   `OnUserCountChanged`; when `Total > 0 && Current >= Total` → `FinishWaitingUser`.
3. `OnRoomReady` → `StartWaitingUserCountdown`: `WaitingUserRemainTime = GetWaitingRoomTimeout()`, ticks down to zero →
   `FinishWaitingUser` → `StartEnteringGameCountdown` (`IsEnteringGame`) → `FinishEnteringGame` → **`EnterGame` =
   `UBattleDsUtility::OnMapFinish()`**.
4. Client UI — driven by the replicated `BP_TgfWaiting` fields: `CurrentUserCount`, `TotalUserCount`,
   `WaitingUserRemainTime`, `IsWaitingUser`, `IsEnteringGame`, `EnteringGameRemainTime` (OnRep_*).

Native part (`UBattleDsUtility`, addresses of this build):

| What | Address | Essence |
|---|---|---|
| `IsRoomReady` | 0x142e9efa0 | byte `this+0x244` |
| `GetTotalUserCount` | 0x142e99460 | `this+0x240` |
| `GetCurrentUserCount` | 0x142e92000 | `this+0x178 − this+0x1a4` (number of logged-in players) |
| "room ready" | **0x142e9b7c0** | **only with `IsDsMode` (0x142e9e8b0: NetMode = dedicated + DS SDK)**: parses the message with the list of players and teams, takes the map_detail row by `map_id` (scene chain `this+0x1c0`, timeouts), fires the delegates, sets `+0x244 = 1` |
| `OnMapFinish` (exec 0x142d8dbf0) | — | if a scene chain exists and index `this+0x1f8` < number of scenes: "MapTravel" to the next scene; index++ |

`OnMapFinish` → MapTravel: without DS mode — **`OpenLevel(<scene map>, absolute, "")`** (a hard host transition without
`?listen`: the listen server becomes a single-player game, clients get kicked); in DS mode — server travel
(0x1455bfbf0(world, url, true, false)); with `-EnableMultiWorld` in DS mode — its own MultiWorld path (0x1429c26d0).
The error is logged as `[BattleDsModule] UBattleDsUtility::MapTravel, Failed`.

**Result for listen:** the room never becomes "ready" (0x142e9b7c0 exits immediately without DS mode), `RoomReady`
does not arrive, the waiting room countdown does not start; if a scene chain were loaded, `OnMapFinish` would do `OpenLevel` and
destroy the listen server. Right now (without `InitGameMap`) there is no chain — `OnMapFinish` does nothing.

## How to do it on listen (proposal, not verified)

The mod runs the waiting room itself, the native BP and UI stay:
1. Server: `StartLocalBattle(L_Dungeon_Matching_01?listen)` (same match data path, the swap to the player works).
   Do not call `InitGameMap` (otherwise `OnMapFinish` will do `OpenLevel`).
2. When the backend has sent the roster (members) — the mod calls on `BP_TgfWaiting`: `OnRoomReady()` and then sets
   `WaitingUserRemainTime` = room timeout (the native `GetWaitingRoomTimeout` on listen is most likely 0 — check);
   as players enter — `OnUserCountChanged(Current, Total)` with its own numbers (the native `PlayerLoginSuccess` on listen
   may not arrive — check). Then the native BP does the rest: everyone entered or timeout → entering countdown → `EnterGame`.
3. Post-hook on `BP_TgfWaiting:EnterGame` (or `BattleDsUtility:OnMapFinish`, which is empty on listen without a chain):
   the mod does `ServerTravel <match map>?listen` (console `servertravel`). Without seamless, clients reconnect on their own,
   they get new PC/PS → `InitPlayerGameData` → data swap with the backend (same session) → teams by `teamof`.
4. After the transition — same as after a reset: spectator, `ready`, JoinGuard, report to the backend (`phase=battle`).
5. Reset after the match (M3) — return to the waiting room (`StartLocalBattle(<waiting room>?listen)`), not to the match.

Check with a prototype: do heroes appear in the waiting room on listen, does `PlayerLoginSuccess` arrive, what does
`GetWaitingRoomTimeout` return, does the countdown run on clients, do clients move over on `servertravel` and get their data,
does the transition break JoinGuard / `UBattleDsUtility` records (the result must not be sent when leaving the waiting room).

Second floor (`BATTLE_ROOM_LEVEL_TWO`, `…_PURPLE_DOOR`) — the native transition also goes through MapTravel; same hook, separate task.

### Prototype 03.11 10:54 — works (`tools\stand\wrproto.ps1`, backend without the flag)

Mod (RuServer): commands `wr_ready <sec>`, `wr_count <cur> <total>`, `wr_travel [map]` in `dbserver-control.txt`;
match map — `dbserver-waiting.txt`; post-hook `BP_TgfWaiting_C:EnterGame` → `servertravel <match>?listen`; after the transition,
on `ready` the mod's match state is taken anew (without a new session — the raid is the same).
Run: server in the waiting room (ready ~60 s after start) → client via a native match into the waiting room (swap ok, hero with data) →
`wr_ready 15` → native 15 s countdown → `FinishWaitingUser` → 10 s entering countdown → `EnterGame` → servertravel →
match ready after 19 s, the client reconnected on its own (same OpenId from the address, swap ok, team, hero with data) →
`leave all` → result 200 + `room_closed` → reset 23 s. No result was sent when leaving the waiting room (the record stayed in status 1).
Facts:
- `OnPlayerLoginSuccess` on listen does arrive (native delegate), but `CurrentUserCount` also counts the host (1 before the
  client enters), `TotalUserCount` = 0 — so the mod counts "everyone entered" itself (members) and passes `wr_count`;
- BP defaults: `WaitingUserRemainTime` 180, `EnteringGameRemainTime` 10 s;
- without seamless: clients reconnect, PC/PS are new, JoinGuard sees a new connection of the same OpenId — no kick.
Remaining: tie it to the `host` task from `/ds/slot`, phase reporting, reset to the waiting room (the reset currently reloads the match map).

## Backend (03.11, behind the `DS_WAITING_ROOM=1` flag, not verified in game)

Without the flag — as before: the server sits on the match map, clients get all 4 room events at once.

1. The matchmaker assembled a room and assigned a slot. The room driver (`ds/driver.rs`, path without ds-agent) gives the slot a task
   if the server sends a heartbeat (`ds::driver::host_task`):
   `{battle_id, level_id, map_id, scenes:[waiting room, match, second floor], waiting_timeout, battle_timeout, option_args, members:[{open_id, team_id}]}`.
   Scenes — `map_detail.map_config` by `room_type`, timeout — `waiting_room_timeout`. `option_args` is taken from
   `dynamic_option_args` by the number of players, otherwise from the old formula. `team_id` — same as `DBattleUserParameter.team_id`.
2. The response of `POST /ds/slot` is `{action:"host", battle_id, host:{…}}`. It repeats until the mod reports the phase of this
   room: in the heartbeat body `phase` = `waiting` / `entering` / `battle` and `battle_id`.
3. Room events to clients (`allocator::room_events_allowed`): before the phase — only 1 `startDsInit`, without an address;
   with phase `waiting` / `entering` / `battle` — all 4. The client goes to the server only on 4 `startBattling`: hostflow
   03.11 11:04 — it got `dsInitFinish` and `startLoading`, acknowledged and waited until it gave up. For the client the waiting room is already the match.
3a. Did not arrive: in the heartbeat `absent: [open_id…]` (by match entry) — we do not wait for their result, they get "match over" (events 5–6).
   `phase: "empty"` + `battle_id` — nobody arrived by the waiting room timeout: the room is closed right away, the slot goes to
   resetting, the same response has `action: "reset"`. Do not move to the match in that case.
4. End and reset — as in M3 (`room_closed`, `reset`). When the room is closed, the slot's task and phase are erased.

Debugging: `GET /ds/slots` — `host` (whether there is a task) and `room_phase`. Tests: `waiting_room_host_and_room_events`,
`host_task_from_map_detail`.

## Mod ↔ backend link verified (03.11 11:17, `dbtest\hostflow.ps1 -Rounds 2`, backend `DS_WAITING_ROOM=1`)

Mod: heartbeat every 2 s when idle and before the match, 10 s during the match, an extra one on a phase change; the body has `battle_id`, `absent`,
`phase` = `waiting` / `entering` / `empty` / `battle`. `action=host` → waiting room (`scenes[0]`; already there — immediately
`OnRoomReady` + `WaitingUserRemainTime = waiting_timeout`) → arrivals per `members` (live connections, without the host) →
`OnUserCountChanged(N, total)` → everyone entered or timeout: `entering` (+ `absent`), nobody — `empty` without a transition →
`EnterGame` → `servertravel scenes[1]?listen` → `battle`. A reset erases the task and returns to the waiting room map.
Result 2/2: task → client in the waiting room after ~5 s → 10 s entering countdown → match 34–36 s after the task (transition ~20 s)
→ result 200 + `room_closed` → reset back to the waiting room 18–19 s → next task on the same server. Not verified: `empty`
(nobody arrived), `absent` with several players, the waiting room of another location (Gothic), the second floor.

## Open hall (03.11, env `MATCH_OPEN_HALL=1`, together with `DS_WAITING_ROOM=1`)

The first "Start" opens a room on a free server right away (without `MATCH_WAIT_SECS`). The next tickets of the same pool
(level, group size — stacks are still separate) go to its waiting room (`matchmaking::join_open_halls`) while the room
has space (`max_teams`, `MATCH_ROOM_TEAMS`) and the waiting room accepts (`allocator::hall_open`: the task is issued, the mod is loading the waiting room
or waiting in it, more than 25 s left until the end of `waiting_timeout`). Late joiners get `matched` and the room events, like everyone; the mod gets
`action: members` with the full roster in the heartbeat response (the mod appends the new ones to `Host.Members`). No free
server and no open hall — the ticket waits in the queue: there are no rooms without a server. The waiting room always waits the full timeout
(WORKAROUNDS K18). Test: `open_hall_takes_new_tickets`. Not verified in game.

## Launcher lobby (06.11, `LAUNCHER_LOBBY=1`, docs/LAUNCHER_SPEC.md §12)

The room is created from the vote result with an empty roster and gets a server the usual way; the `host` task goes out with `members: []` and the timer
`LOBBY_WAIT_SECS`. Players are added by the "Start" button (`matchmaking::join_room`, then `action: members`). Verified on the Castle waiting room and the
Dungeon waiting room: the mod starts the countdown without players; with no players by the end of the timer the room is closed. Since 06.11 the `DS_ARRIVE_SECS` countdown does not run in the waiting room.
