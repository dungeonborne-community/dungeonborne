# "Clotho Trial" as a permanent mode — analysis and plan

> **12.11 (user's decision "remove Clotho"): Clotho is closed in the playtest season** — the seasonal `content/seasons/playtest/balance/activity_center.json` and `immaculate_trial_rule.json` restore the game's dates and `open_time` for event 16600 (pass 16518 is still extended); to enable it again — delete these two files and rebuild `cargo xtask balance build --season=playtest`.

Analysis of 11.11 based on the client Lua (`luac -l`), tables (`backups/balance`), localisation, the exe and the backend code.
No edits, not checked on the test stand. Working files: `<data>\research\clotho`:
- `dis/*.dis.txt` — Lua;
- `loc_ru.txt` — strings;
- `lsum.py`.

## 1. How it works in the original

### Rules (string `Act_Bo5_R`, tables)
- **Entry.** You buy a "Clotho Rune Stone" (1033, shop: product 704016, tab 2000, 30 000 gold). The stone
  opens a run: `immaculate_trial_rule.cost_itmes`.
- **Run.** 3 vs 3 matches (`map_select.join_members = 3`), each match up to 4 wins: up to 7 rounds, plus an extra one
  on a tie. The map is fixed — 30102 `L_Common_PVP01_MultiGameMode`.
  - A run starts with 3 tokens. Losing a match costs a token, 0 tokens — the end.
  - 7 wins — end of the run and a reward (`win_count 7`, `failed_count 3`).
  - Matchmaking solo or as a group. Opponents and teammates — by the current number of wins.
- **Grade** — `immaculate_trial_grade`, 10 rows:
  - 101 "Flawless" — 7 wins without losses, first rewards 100098 and 631103 + a random one;
  - 102–103 — 7 wins with 1–2 losses;
  - 104–110 — 6…0 wins;
  - random reward — a pool per class (`immaculate_trial_random_rewards`, `random_reward_id` × `career_type_id`);
  - mails `first_reward_mail_id 501030`, `random_reward_mail_id 501031`;
  - leaderboard of flawless runs `rank_id 20001`.
- **Schedule** — `open_time [5, 6, 0]`: Friday, Saturday, Sunday. A warning and closing of matchmaking before the end of
  the event: `activity_end_*_time`.
- **Map modifiers** (`map_select.model_feature_ids [1, 3, 6]`):
  - 1 "Magic Corrosion" — a zone;
  - 3 **"Equipment Issue"**: equipment is issued that can be used only in the match and cannot be taken out. Per the
    rules text — "basic equipment of epic quality", your own can also be taken if it is equipped;
  - 6 **"Loot Protection"**: after death players do not lose the items they brought.
- **Level** 50000006 (`competition_level`):
  - type 2002;
  - `settle_interface_type BO5_TRIAL`;
  - `is_only_active_open: true`;
  - matchmaking pool 20027, rule 90030.
  The table for rule 90030 is not in the client (server-side).
- **Event** `activity_center` 16600 "Act_Bo5_T": 16.08–08.09.2024, `bind_levels [50000006]`.

### Client (Lua)
- **Map in the menu** — `WBP_SelectMap_MapEntry6` (Map_6_1). A click opens the event window `WBP_PVPChallenge_Window`
  (`PVPChallengeService.OpenUI`). It has no matchmaking button of its own.
- **Event window** `WBP_PVPChallenge_MainPanel`:
  - "Open" — `FlawlessTrialsService.SendTrialOpenReq(ActivityId)` (`activity_center_trial_open`, consumes 1033);
  - "To battle" — checks the team: not in a group, or the captain. Then `UMatchService.SelectMapLevel(50000006)` and closes
    the window. After that the usual matchmaking button;
  - "Reward" — `SendTrialReceiveRewardReq`; "Surrender" — `SendTrialSelfQuitReq`.
- **Matchmaking button** `WBP_MatchMainButton`:
  - if `IsTrialsLevel(level)` (the level is in the event's `bind_levels`), then `CheckAllowImmaculate`;
  - `PVPChallengeService.GetImmaculateState()` gives one of the states: `NotInActivityTime` ("Event ended") /
    `NotInLevelTime` (not Fri–Sun) / `NotItemActivate` (run not opened; if a stone is present it sends `trial_open` itself) /
    `Award` / `CanPlay`;
  - the time is taken from the **client** tables `ExcelActivityCenter` (16600, 2024) and `ExcelImmaculateTrialRule.open_time`;
  - the run state — from the event data from the backend: `TrialData.CurrentGame` (`DUserActivityTrialData`):
    `status`, `win_count`, `failed_count`, `challenges_data[]`.
  - Then the usual `battle_matching` with level 50000006.
- **After a match** the client waits for:
  - `activity_center_trial_record_sync` (`DTrialRoundData`) — the new run state;
  - `battle_settle_sync` with the BO5_TRIAL window.

**Client summary:** right now the mode is closed by the **client** tables. Event 16600 ended in 2024, and the matchmaking button
will say "Event ended" whatever the backend answers.

### What the original server does
- 3 vs 3 matchmaking by wins, run results, rewards, match records — the original game server, which we do not have.
- **Equipment issue.** I did not find in the client data who issues it. The exe has only the item field
  `DItemCreateInfo.temporary_item_guid` (temporary item). Conclusion: the game server issued it in the match data, it is not in BP or
  in the mode tables. **Not checked.**
- **Loot protection** — the match result is not applied to equipment. Our arenas already work this way: the result of a custom game is not
  applied (`phase=empty`, no `player_result`).

## 2. What we already have
- Level 50000006 is open to everyone (`account_handlers.rs:25`). Map and rule — `battle_handlers.rs:44` (30102, 90030, 20027),
  map path — `matchmaking.rs:302`.
- Custom games: the "Up to 4 wins" mode on 30102/30202 (`room.rs pvp_arena`). The BO5 arena already starts for us.
- Arena result (commit 7c77a80, `docs/PVP_SETTLE.md`):
  - winner and statistics → `battle_settle_sync`;
  - there are no rounds (`PlayerPvpRoundData`) yet, and the BO5_TRIAL window shows them.
- `activity_center_trial_*` — stubs (`shop_handlers.rs:1403`), event 16600 is not returned in the list.
- `matchmaking` is built for raids: a room of many groups, team = the player's group. It does not assemble two teams of 3 from
  solo players.

## 3. Options

### A. As in the original, but always open (I recommend)
Event window, stone, 7/3 run, grades and rewards — without dates.
1. **Balance pak (client and server), about 0.5 day:**
   - `activity_center` 16600: `start_time` ≈ now, `end_time`/`expired_time` — 2099;
   - `immaculate_trial_rule.open_time` → all days (or keep Fri–Sun, per the user's decision);
   - check `competition_level 50000006.time_limit`.
   The end of the event with top-50 rewards then never comes — that is exactly the "permanent mode".
2. **Backend: event and run, 1.5–2 days:**
   - `activity_center_list` returns 16600 with the player's `DUserActivityTrialData`;
   - `trial_open` — consume 1033 (or `items` from the request), `status` open, `open_times + 1`, 3 tokens;
   - `self_quit`; `receive_reward` — grade per `immaculate_trial_grade`: first rewards once
     (`received_first_reward_ids`), a random one from the class pool, mails 501030/501031;
   - `history_record`;
   - after each match: win or loss in `challenges_data`, `win_count`/`failed_count`, push `trial_record_sync`,
     `immaculate_count` for a flawless run.
3. **3 vs 3 matchmaking, about 1 day.** A separate queue for level 50000006:
   - a group of up to 3, filled up with solo players to 3;
   - 2 teams per room (6 players);
   - match teams by composition, not by group (`team_no`);
   - server — map 30102;
   - buckets by `win_count`, optionally.
   **Test:** 6 players are needed. For the test stand — a "1 vs 1" switch.
4. **Equipment issue, 0.5–1 day, depends on the kit composition decision.** In the match data (`battle_user_data`) — fill empty slots
   with the class's basic epic kit:
   - temporary items with `temporary_item_guid`;
   - not returned in the result or to the inventory;
   - own equipped items stay;
   - the kit composition per class has to be defined: it is not in the client data.
5. **Match result, about 0.5 day.**
   - RuServer: detect the arena by the map, not by the `pr` prefix of a custom game;
   - hook `PVPGameWinTeamRound(round, faction)` — rounds into `PlayerPvpRoundData` for the BO5_TRIAL window;
   - the winner from `/ds/arena_result` goes into the run (item 2);
   - equipment is not taken away (loot protection), experience and loot — as the user decides.

Total about **4–5 days** of work plus testing.

### B. Match only, no event
A "Clotho 3v3, up to 4 wins" mode in the launcher lobby tab: 3 vs 3 matchmaking + equipment issue + BO5 result.
- No stone, tokens or grades, rewards — our own (for example, a mail for a win).
- The client pak is not needed if entry goes through the launcher lobby: the matchmaking button with the event check is not involved.
  **Not checked:** whether the client lets you into a match of the event level without an active event when the match came via a push.
- The event window in the client will show "Event ended".
- About **2–2.5 days**.

## 4. What to check on the test stand
1. After the pak edit the matchmaking button with the Clotho map does not say "Event ended", and the event window opens.
2. `trial_open`: the stone is consumed, state "Open", the card shows 0/7 and 3 tokens.
3. Match: map 30102, rounds up to 4 wins, issued equipment on empty slots, items in place after death.
4. After the match:
   - the BO5_TRIAL results window with rounds;
   - `trial_record_sync` — win or loss in the card;
   - 3 losses — run closed; 7 wins — "Reward", mails.
5. 3 vs 3 from solo players: teams of 3, on the test stand — 1 vs 1 mode.

## User decisions
- Option A or B.
- Days of the week: always or Fri–Sun.
- The composition of the issued kit per class.
- Rewards: original `immaculate_trial_grade` or our own.
- Whether to grant experience and loot for a match.
- Whether the leaderboard of flawless runs (`rank_id 20001`) is needed.

---

## 5. Decision of 11.11 and the working plan
The user's decision: **"fully as in the original, but adjusted for our instances in the launcher, for example a Clotho
instance"**. We take option A as a whole:
- event and stone 1033;
- 3 tokens, 7 wins, `immaculate_trial_grade` grades, rewards by mail;
- equipment issue, "Loot Protection";
- BO5 rounds and the BO5_TRIAL window, `trial_record_sync`.

**Entering a match — via the "Clotho" tab in the launcher lobby** instead of an in-game queue.

### 5.1 How it is played
1. **Event window in the game** (as in the original, after the pak edit):
   - the stone is bought in the shop; "Open" consumes it, and the run is open (0/7, 3 tokens);
   - "Reward", "Surrender", history — also there.
2. **Launcher, "Clotho" tab:**
   - a separate card next to the 6 instances, **without voting**: map 30102 and the 3 vs 3 up-to-4-wins format are fixed;
   - you can sign up only with an open run. No run but a stone — the backend opens the run itself (like
     `IsAutoUse` in the original), otherwise a refusal "Trial not activated…" (original string `ImmaculateTrial_Prompt_State_3`);
   - solo or as a group of up to 3 (the game group, all members in the tab).
3. **Gathering** (`gather`): we wait for 6 players. For the test stand — `LOBBY_CLOTHO_MIN=2`, a 1 vs 1 match.
4. **Teams:**
   - groups are not split, solo players fill teams up to 3;
   - when choosing the split, teams are balanced by the current run wins (in the original, opponents were matched
     "by number of wins");
   - the match team number is set explicitly (1 and 2), not by the game group.
5. **Start.**
   - There is no separate hall: the arena has its own warm-up (`Phase_PVP_WarmUp`).
   - Once 6 have gathered — the room is created immediately, like the start of a custom game (`start_direct_on`, arena 30102). Players
     get "match found", the client travels by itself.
   - Whoever is not in the game by this moment does not get into the gathering (`game_online`).
   - Alternative — "Start" in the game as a readiness confirmation. Decided on the test stand.
6. **Match and result** (BO5 arena, rounds):
   - each player gets the BO5_TRIAL results window with rounds;
   - the backend records the win or loss into the run and sends `trial_record_sync`;
   - equipment is not taken away ("Loot Protection"), issued items are not returned;
   - the tab starts gathering again.

### 5.2 Who does what
| Part | Who | What |
|---|---|---|
| Balance pak (client and server) | me, build `cargo xtask balance build`; deploy — the orchestrator on "yes" | `activity_center` 16600: `start_time` now, `end_time`/`expired_time` 2099. The rest as in the original (`open_time` Fri–Sat–Sun, see 5.4) |
| Backend: event and run | me | 16600 in `activity_center_list` with `DUserActivityTrialData`; `trial_open` (1033, tokens), `self_quit`, `receive_reward` (grades, first rewards once, a random one per class, mails 501030/501031), `history_record`; match recording and `trial_record_sync` |
| Backend: "Clotho" tab | me | In `launcher_lobby`: a mode tab without voting, gathering up to `LOBBY_CLOTHO_MIN` (6), run check on sign-up, 3/3 split, a room on arena 30102 with explicit teams; fields for the launcher in `GET lobby` |
| Backend: equipment issue | me | In `build_battle_user_data` for 50000006 empty slots — the class's basic epic kit (starter set `role_default_equipment` at tier 4, `upgrade_starter_item_id`), temporary items (`temporary_item_guid`); not returned in the result or to the inventory |
| Backend: match result | me | `/ds/arena_result` + rounds → BO5_TRIAL; winner → run. No experience or loot for the match (see 5.4) |
| RuServer | me, the file has other people's uncommitted edits — commit together with the orchestrator | Arena by map (`#Scenes == 1`), not by the `pr` prefix; hook `PVPGameWinTeamRound(round, faction)` → rounds into `/ds/arena_result` |
| Launcher (UI) | the "Tauri launcher in Rust" session | "Clotho" card: players X/6, run state (wins, tokens), no voting buttons, refusal hints. Contract: `instances[].kind = "clotho"`, `me.trial = {status, wins, fails, tokens}` in `GET lobby` |

### 5.3 Order
1. Backend: run and event plus the balance pak. Checked in game without a match: event window, stone, "Open".
2. Backend and RuServer: "Clotho" tab, teams, equipment issue, result with rounds. Checked with the GM command
   `lobby join clotho` and a 1 vs 1 match on the test stand.
3. Launcher: the "Clotho" card per the contract from step 2.
4. End-to-end check, then deploy on the user's "yes": backend, balance pak, RuServer, launcher.

Estimate: steps 1–2 — 3.5–4.5 days (mine), step 3 — the launcher session.

### 5.4 Where the original is silent — proposals (the user decides)
- **Days of the week:** in the original Fri–Sat–Sun (`open_time`). I propose keeping it as in the original.
- **Equipment kit:** in the original "basic equipment of epic quality", the composition is not in the data. I propose
  the class starter set at tier 4 (epic) without extra properties, only on empty slots.
- **Experience and loot for a match:** the trial had no per-match rewards, only grades. I propose not granting them.
- **Flawless leaderboard** (`rank_id 20001`, top-50 at the end of the event). A permanent mode has no end. I propose
  a weekly table without rewards, or not doing it.

### 5.5 What to check on the test stand
1. After the pak: in the event window "Clotho Trial" is open, the stone can be bought, "Open" — 0/7 and 3 tokens.
2. "Clotho" tab: without a run and a stone — refusal; with a stone — the run opens automatically; gathering X/2 (test stand).
3. 1 vs 1 match:
   - arena 30102, rounds up to 4 wins;
   - epic equipment issued on empty slots, own equipped items in place;
   - after death items are not lost.
4. Result:
   - the BO5_TRIAL window with rounds;
   - win or loss in the run card;
   - 3 losses — run closed; 7 wins — "Reward", mails with first and random rewards;
   - issued equipment did not appear in the inventory.
5. 3 vs 3 (6 clients or bots) — after the test stand, on the playtest.

---

## 6. User decisions 11.11 (amendments to §5)
1. **No restrictions by day or time.** In the balance pak:
   - `immaculate_trial_rule.open_time` — all days, `[0, 1, 2, 3, 4, 5, 6]`;
   - event 16600 — until 2099;
   - warnings about the event end (`activity_end_*`) will not fire.
2. **No equipment issue.** Players play in their own equipped gear. The "Equipment Issue" modifier in the map card
   stays as a label from the client table. Removing it — an edit of `map_select.model_feature_ids` in the pak, optional.
3. **No leaderboard of flawless runs** (`rank_id 20001`, mail 501032 not needed).

From §5.2 the "Equipment issue" rows are removed; in §5.4 the days, the kit and the leaderboard are decided; experience and loot for a match — not granted
(the §5.4 proposal, to be confirmed).

## 7. The reward for 7 wins in the original (checked via Lua, BP and exe)
**The "Reward" button in the event window** (`PVPChallengeService`, function on lines 427–459):
- **7 wins without losses ("Flawless"):**
  - if this is the first flawless run (`GetImmaculateTimes() == 1`) — immediately
    `StartLocalFlawlessBattle()`, a move to the platform;
  - otherwise a window "Do you want to go to the trial platform and collect your reward?" (`ImmaculateTrial_Prompt_Again`) with
    the buttons "Go to the trial" (platform) and "Receive" (`SendTrialReceiveRewardReq`, without the platform).
- **Any other result** (7 wins with 1–2 losses, 0–6 wins) — immediately `SendTrialReceiveRewardReq`, reward without the platform.

**Platform** — `BattleService:StartLocalFlawlessBattle("L_Common_PVP_end", Flawless, WinCount, RewardAvailable)`:
- **a local single-player map without a battle server** (like the "egg" `StartLocalBattle`). The native `0x14599f430` stores
  the arguments and loads the map. The mode `B_LyraGameMode_FlawlessTrial` reads them via
  `BattleDsUtility.GetFlawlessTrialLevelArgs` (`0x142e93ea0`: Flawless, WinningStreak, RewardsAvailable, Initialized);
- on the map:
  - the throne `BP_FlawlessTrial_Throne` — enabled only with `RewardAvailable && Flawless`;
  - 12 bowing "servants" `BP_TGF_ImmaculateTrial_Armor_Hammer`, braziers, the gate `BP_Door_FlawlessTrial`;
  - the chest `BP_TGF_FlawlessTrial_Chest`, portals;
- **the player sits on the throne** (`GA_TGF_InteractWithThrone` from `AbilitySet_FlawlessTrail`): throne camera, events
  `FlawlessTrial.ServantSpawn`, `Kowtow`, `BrazierLightOn`, `PortalSpawn`;
- **the chest** waits for `FlawlessTrial.ShowChest` (appear) and `FlawlessTrial.OpenChest` (contents ready), then
  opens by itself (`AutoOpenChest`). Both events are sent by native code, BP and Lua have no senders for them;
- **the reward is a request to the server right from the platform:**
  - `UFlawlessTrialsService.InSceneSendFlawlessRewardReq` (`0x14591a780`) finds an activity of type 10 (trial) and
    sends `activity_center_trial_receive_reward`;
  - the response is parsed by `OnTrialReceiveRewardResponse` (`0x145bbc680`). Event `FlawlessTrialsReceiveRewardEvent` for the UI.
    If the game is in a match (including on a local map), the items `first_item_rewards` and `random_item_rewards` from
    `DTrialRewardsData` are put into the platform chest — they are visible in the chest;
  - who calls `InSceneSend…` — there is no call in the platform BP or in Lua, so it is native (on mode readiness or on the throne).
    **Not checked**, we will check via the RPC log on the test stand;
- **the server issues the rewards themselves by mail** (`mail_template`):
  - 501030 "[Clotho Trial] Flawless Victory" (first rewards; text "Your flawlessness opens the sealed
    gates") — this is the platform gate;
  - 501031 "[Clotho Trial] Rating Reward" (a random reward by grade).
  The chest on the local map is for show, nothing can be taken out of the local map.

**What this means for us:**
- **The client part is all native.** The button, platform, throne and chest work by themselves if the backend:
  - returns in `TrialData` `status = REWARD` (`EN_ACTIVITY_TRIAL_CHALLENGE_REWARD`), `win_count`, `failed_count`,
    `immaculate_count`;
  - answers `trial_receive_reward` with `DTrialRewardsData` with `first_*` (only for the first "Flawless",
    `received_first_reward_ids`) and `random_*` (by grade and class from `immaculate_trial_random_rewards`);
  - puts the same items into mails 501030/501031 and closes the run (`status` → finished).
- A repeated "Flawless": only the random reward, the platform — at the player's choice.
- **Check on the test stand:**
  - whether the local map `L_Common_PVP_end` loads for us: a local match without a server, like the "egg";
  - whether `trial_receive_reward` is sent from the platform;
  - whether items appear in the chest;
  - whether the mails arrive.
