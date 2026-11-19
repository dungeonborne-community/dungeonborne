# Fix list by section (from live tests)

Deliberate simplifications ("workarounds") are kept separately: `docs/WORKAROUNDS.md`.

Section order follows `docs/BACKEND_FRAMEWORK_PLAN.md` (E4): login → items → match and results → party and match → mail →
shop → friends → rooms → trade → guilds. Cross-cutting core problems are listed separately and are fixed first.
The source of each item is a session and its logs; analysis: `tools/py/probe_timeline.py`, `tools/py/compare_flow.py`.

## ⚠ CRITICAL — K0 (05.11): the game "died" after ~130 s of activity — TGCP frame number
- [x] **Fixed and verified locally 05.11 11:42. Not deployed to prod.** The server wrote the S2C frame number in the
  TGCP header (0x4013) as **u32 LE at [12..16]**, while the client reads **u32 BE at [9..13]** (and sends its own C2S the same way). Up to 255 the bytes
  match — everything works; **from frame 256 on the client sees number 0 and silently drops all pushes** (sync/notify), while replies
  to requests still arrive — the game looks alive, but the server can no longer tell the client anything until the game restarts.
  The number grows with every frame we send (replies + pushes): slowly in the lobby, but search/room/hall send a heartbeat once per 1 s
  (≈2 frames/s) — the 256 threshold is hit about 1.5–2 min after "Start" (≈130 s), i.e. almost always in the hall or in
  the raid. Prod uses the same `encode_tgcp_s2c` — all players and all sessions are affected.
  Proof: beacon `RAID_PUSH_BEACON_SECS` (dirty_role every 5 s) + ClientProbe RoleInfoNoti — at 11:20 the last
  accepted push = frame 0xFA, after 0xFF→0x100 none were accepted; in the lobby without search all were accepted over 211 s; after the fix
  the frame reached 548, results/CANCEL/room end were accepted. Fix: `core/backend/src/tgcp/mod.rs` `encode_tgcp_s2c`, test
  `s2c_seq_big_endian_at_9`. Analysis — Sh4g.
- **What was probably this bug (recheck after deploy; conclusions "client did not accept / the client is the cause" before
  05.11 are unreliable):** Sh4f/Sh4g (no results window, dead "Start"); M9/M10 (search/room did not close on the client,
  "Match found" in the lobby) — CANCEL/clear arrived after the threshold; M11 (client "did not go" to the hall); B13 (client hung in
  "connecting", did not accept "match over"); B7 (after death it re-equips lost items — dirty after results was dropped);
  P2/P3 (character disappears from the lobby, "Enter lobby" does not work on the first try — dirty/cache_expired pushes dropped);
  "character disappears from the lobby after a long raid"; mail did not refresh (item "the client's mail list did not
  refresh"); F1 (friends online).
- **Workarounds added because of it (review, possibly remove):** search CANCEL resend
  (`matchmaking::match_cancel_resend`), deferring results/room end (`hub::defer_*`, RAID_DEFER_SETTLE/ROOM — already
  disabled), M11 "snapshot = current state", room event repeats.
- **Rule:** if the client "stopped reacting" to pushes after some time — check the transport first:
  `rpc_dump/OUR_tgcp.jsonl` field `hdr`, S2C numbers against the client's C2S encoding.
- **Confirmed 12:28–12:41 (user: "everything works"):** hall 2 min 15 s + raid 10 min (S2C frame reached 2074),
  extraction — results window, CANCEL/room/clear taken, "Start" works, **the character is in place in the lobby** (the bug "character
  disappears after a long raid" — also K0).

## Session 2026-11-02 03:30–03:37 (one client, Steam, clicking through menus by hand)
Dump `rpc_dump/OUR.jsonl` (376 records), probe `probe/logs/steam_20261102-033038_ue4ss.log` + `…033309_lua.log`.
Covered: login, lobby, merchant (5 purchases), equipment, trade (search, listing), leaderboards, character
creation/deletion, skill selection, party search, quests, relics/alchemy/crafting (tabs only). No match, mail or friends.

## ACE on prod
- [ ] **A1 05.11 — under investigation. On prod ACE "crashes" the game with random messages (for the user and players),
  locally — never.** Ruled out: backend (player_ace_packet is only acknowledged everywhere, ScPlayerAcePacketSync is not
  sent in either place); client files (launcher bundle = local C:\Games\Dungeonborne byte for byte: steam_api64 emulator,
  UE4SS dwmapi, cacert; the only difference is the UE4SS console); launch method (the standalone patch also starts
  Win64\Dungeonborne.exe directly). **A/B experiment 13:07–13:24:** game via launcher 1.3.0 (parent, process
  handle, TLS relay 127.0.0.1:7443) + all 11 prod mods (modset 4766c90c3045) against the local backend
  (`tools/stand/run_launcher_dev.ps1`, dev login, no Discord) and local DS 7777: 16.5 min, search, raid, lobby, trade
  — no crash. So it is not the launcher/relay/mods as such. Remaining: the network to Hetzner (with RU ISPs the relay
  hangs — see memory ru-isp-freeze; ACE may not have waited for replies to its packets), DS on Robot (raid over the internet),
  the prod backend before K0. Need the ACE message texts and times; check whether players with WARP/VPN had crashes.
  Earlier launches via the launcher 04.11 21:20–21:55 (per launcher.log): 3.5 / 13 / 9 / 3.5 / 5 min.
  User after the experiment: "no ace" — the main suspect is now the network to Hetzner.
  **13:35 locally — first ACE window:** "Anti-cheat system works error… (13-131084-2)" in the game SERVER process
  (pid 42464, DbTools launch at 13:35:34, 5 s after the previous one quit at 13:35:29); after OK the process closed. Launch at
  13:38:39 (3 min later) — no window. Hypothesis: ACE had not released itself after the previous process — keep a pause between quit and
  a new launch. For prod players — check whether they restarted the game right after closing/crashing.
  **User confirmation (05.11):** at playtests people often closed the game with Alt+F4 and launched it again right away — matches.
  Two more server launches with a 2 min pause (13:45, 13:56) — no window. Conclusion: not our code, but a fast restart under ACE.
  What we can do on our side (without touching ACE): after the game exits the launcher keeps "Play" disabled for ~1–2 min with
  an explanation; ds-agent/start_headless on Robot — a pause between a DS exit and the next launch.
  **Launcher 1.3.1 (05.11, built locally, not signed or released):** 30 s pause (user's choice) between game exit
  and the next launch — `GAME_RESTART_COOLDOWN_MS`, exit time in `%LOCALAPPDATA%\DungeonborneLauncher\
  last_game_exit.json` (survives a launcher restart), the "Game" step in cooldown state with a countdown ("anti-cheat pause
  after the previous game: N s"). Test `restart_cooldown`. Pause for the DS on Robot — not done.

## Cross-cutting (core)
- [x] **C1 fixed 02.11, not verified in game.** `core/backend/src/guid.rs`: shared item guid counter
  [100 000 000, i32::MAX], mark in the DB `counters:item_guid` (+10 000 reserve), all issuing places switched to it
  (login/starter items, roles, inventory, shop, mail, raids/deadmoney, trade, GM). One-off migration at startup
  (`counters:item_guid_migration_v1`): on the 02.11 saves — 6 players, 702 old guids replaced; trade,
  mail, gift tables — no replacements. Backup before migration: `surreal_saves_before_guid_20261102`. Verify in game: equip, unequip,
  move, sell on the market; also compare client memory before/after (suspected accumulation of item copies).
  `GUID_MIGRATE_ONLY=1 cargo run` — migration only, then exit.
- **C1 (before). Item guids do not fit in 32 bits — critical.** The server issues `user_id × 10000 + n` (≈1.09·10¹¹), the client
  stores the guid in 32 bits: 109 490 200 139 mod 2³² = 2 116 017 739 — that is what it sends back. Consequences in the session:
  `role_wear`/`role_take_off`/`inventory_item_move` — "item guid=2116017739 not owned by the player", toast
  "Invalid item class or slot."; `trade_sales_order_publish` → `-4212 EnErrTradeSaleItemInvaild`. On the official
  server guid ≈ 10⁸ (`106598477`). Fix: a shared guid counter < 2³¹ (in the DB, atomic), migration of all existing
  player items + references (trade orders, mail, relics). Also check `role_guid` (`user_id × 100 + n` ≈ 1.09·10⁹ —
  fits for now, but overflows when `user_id > 21 474 836`).
- [ ] **C2. Player save without locking** (plan E3.2) — has not shown up in a single-player session yet, check with two.
- [ ] **C4. `user_id` = 10 000 000 + last 6 digits of SteamID** — two players with the same tail get the same `user_id`
  (UNIQUE index in the DB → the second will not be created). Needs a shared counter, like guids.
- [ ] **C5. Database is 190 MB for 6 players** (`surreal_saves`) — check whether it is the SurrealKV log or real data volume.
- [ ] **C3. Empty dirty push on failure.** `inventory_item_move` on failure sends `SCPlayerDirtyChgSync` with 0 bytes — do not send it.

- [x] **C6. RPC header not per schema — fixed 02.11, verified in game 04:58** (character switch ×3, creation, -726). Audit of the rest of the inherited code — `docs/LEGACY_AUDIT.md`. Our `RpcHead` was guessed;
  the real one is `atframework.CSMsgHead` (schema_dump.proto): `error_code`=1 (sint32), `client_sequence`=3,
  `server_sequence`=4, `op_type`=6. We wrote our own S2C counter (replies + pushes) into 3, the request number into 4, the error into 6.
  The client matches a reply by field 3: until the first push the counters matched, then they diverged. Proof —
  `hall_set_role` (expert: the window closes only on the reply callback with error==0): both successful switches — with divergence
  0, all failed ones — 3/6/9. Now 3 = echo of the request number, 4 = our counter, the error goes in 1. Consequences: callbacks of all RPCs
  after pushes (role_create P4 and others), and `set_rpc_error` codes reach the client for real for the first time — error toasts
  may pop up where things used to pass "silently". Full headers are written to `rpc_dump/OUR_head.jsonl` and
  `PVE_head.jsonl` (sniffer) — compare fields 5/7/11/12 with PVE.

- [x] **C7. Transport check against PVE 02.11 05:05** (`docs/LEGACY_AUDIT.md`): pushes in `rpc_stream`, extra
  header fields removed, `0x2002` with the client's open_id, `login.heartbeat_interval` 30 (was 30000 — the client did not ping),
  `login.version` = package_version. Not verified in game.

- [x] **C8. GM commands were available to everyone** (`player_gm_command`: gold, items). 02.11: only the account role `gm`/`admin`
  (Discord roles → `[auth.discord.roles]`); a player without an account — only with `[auth] require = "none"`.
- [x] **C9. Instant ban** (02.11, `core/backend/src/auth/ban.rs`): a `DEFINE EVENT` in SurrealDB revokes the account's sessions when
  `banned = true`, `LIVE SELECT` cuts its relay connections; GM commands `ban <open_id|name> [reason]` / `unban`.
  Test on a real SurrealKV — `ban_revokes_sessions_and_kicks`. The Discord ban role — still checked at login.

## Login
- [x] **L1 02.11 18:20 — cause found: the `gtcp://` address in 0x4002.** Series of auto-logins (`tools\stand\loginseries.ps1`,
  env `DIR_URL_MODE`): both addresses `gtcp://…;tcp://…` (as in the dump) — 201 in 1 login out of 6; `tcp://` only — 0 out of 10;
  `gtcp://` only — never logged in (3 out of 3, ~200 errors per login). The default is now `tcp://` (`core/backend/src/dir.rs`).
- [ ] **L1 (history). `connect failed 201, ext:-6` before TCP, cured by retries.** The client gets `gtcp://127.0.0.1:6601` from DirServer,
  but the gateway sees no connection at all. In this session with connection recording the login worked on the first try — need
  a recording during a failure (script: game process connections every 150 ms). In BACKLOG since 30.10 (after game crashes).
  02.11: `tools\stand\connwatch.ps1` — connections and UDP of game processes every 150 ms (probe/logs/connwatch_*.log).
  Caught at 17:43 (connwatch_20261102-174212.log): dir 8085 17:43:18–20 → ~16 s without TCP (only UDP sockets 17:43:17–23)
  → 201 ×4 at 17:43:36 → 6601 connected at 17:43:38. The client does not try to connect to the gateway until something
  inside GCloud expires (DNS/LBS/probe). Analysis of ext -6 — agent (Ghidra). The gateway accepted one connection during this login (successful,
  17:43:38). DirServer: 0x1001 → 0x1002, 0x4001 → 0x4002 (`gtcp://127.0.0.1:6601;tcp://127.0.0.1:6601`), 0x9001 — close.
  dbclient logs: the failure comes exactly 5 s after the click, most often 2 times in a row, the third attempt goes through
  (12:18, 12:46, 19:59 31.10); sometimes it works on the first try.
- [ ] **L6. No character selection window after login** — agent (LuaProbe): the hall WBP_HallWindow opens now too
  (the window name was not read in the ue4ss log). There is no separate selection window in the game: hero selection/creation is inside the hall.
  In the morning account 9020 had a choice (5–6 roles), F:/E: had one role each — nothing to choose. Login replies are the same.
  Waiting for user confirmation (2+ roles or a screenshot).
- [ ] **L2. Foreign lines in `hosts`.** Three lines `198.51.100.1` (dir/tconnd/t01) after our block sent the login to
  someone else's server. `game/client-mod/install.ps1` must remove foreign entries for these domains; in the manifest model — the launcher relay.
- [ ] **L3. The client hammers HTTPS 443** with short connections every 0.5–2 s for the whole session. Find out which path (`auth_http`
  does not log requests) and what the client is waiting for — probably an SDK request that fails and is retried.
- [ ] **L4.** `pre_login`: no `info.await_queue_index`/`total_index` (login queue) — low priority.
- [ ] **L5.** `cache_batch_get`: `player_status_cache` lacks `is_in_battle`/`last_battle_start_time` (status for friends/party).

## Items
- [ ] **BAL1 05.11 — combat balance audit (agent); MINOR pools built 05.11 (WORKAROUNDS K20), balance built, not deployed to installs and not verified in game.** Combat tables balance\*.json = the original game
  (assets\excel\ExcelConfig, data 2.19.2.20241211); the fandom wiki confirmed 43 of 55 weapon types, the other
  differences look like patches after July 2024. **The error is our 23 MINOR pools affix_pool 100101…101301** (the original
  has none): all identical — 15 attributes = `FALLBACK_MINOR_ATTRS` (affix.rs); per the game's strings
  affix_random_role_season1_* are different categories (stats, Offensive/Defensive Ω/Φ/Δ, weapon/armor
  modifiers, Midas Touch, resource). Result: green slots never roll resistances, penetration,
  elemental damage, lifesteal, cooldown, 6xxx effects, set points. Minor: the formula in balance_kit\kit.toml
  (× roll / 10000), kit.toml wrongly says the backend reads affix_value_template/affix_count_random_pool/
  gear_template; roll salts collide in affix.rs; outdated balance_csv\loot*.csv. Report and files —
  scratchpad of session 3af43e95 `balance_audit\REPORT.md`.
- [ ] **BAL3 05.11 — ids 6211/6212 "added cold/lightning damage" are swapped in the backend: "Blade of the Thunder
  Knight" adds cold instead of lightning.** In proto `DBattleAttributeOffset` the field `…ice…` = 6211, `…thunder…` = 6212.
  The DS applies by number (StatWatch 05.11: Orb with id 6211 → `AdditionalElementIceDamageOnCausingHit`), the client's tooltip
  headers agree (6211 "Added Cold Damage", 6212 "Added Lightning Damage"). In `battle_attribute_description`
  the name keys of these two ids are swapped (as in the original, the texts compensate), and `affix.rs::map_attribute_offsets`
  repeated the swap: the pool field `…thunder…` is issued as 6211 (cold), `…ice…` — as 6212 (lightning).
  - Affected: MAJOR pools 110260000 (Blade of the Thunder Knight 181025000, Bloodmane 181026000: lightning → cold),
    110460000 (Scorpion Sting 181046000: cold → lightning) and our MINOR pools 100503 and 100901.
  - Fix: in `map_attribute_offsets` swap 6211 ↔ 6212 (as in proto). Items already issued
    keep the old id, i.e. the element the player sees in the tooltip — do not touch them.
  - The other 93 ids match proto.
  - **Fixed 05.11** in the sources (`affix.rs`: 6211 = `…ice…`, 6212 = `…thunder…`), `cargo check --release`
    passed. Binary not rebuilt — the running backend holds `target\release\dbserver-rs.exe`. Takes effect after
    a build and backend restart, only for new items. Not verified in game: need a new Blade of the Thunder
    Knight and `AdditionalElementThunderDamageOnCausingHit` in the StatWatch report.
- [ ] **BAL2 05.11 — combat math watcher (StatWatch): hero stats are computed correctly, the damage formula is partially
  recovered.** DS mod `game/ue4ss/Mods/DungeonborneStatWatch` + `tools/py/stat_watch.py`, description `docs/STAT_WATCH.md`.
  Raid 16:17–16:25: log `dbtools\run\statwatch-20261105-161648.jsonl`, report `…\statwatch-report-20261105-161648.md`.
  - [x] **Hero attributes: 252 of 252 matched exactly** (6 hero objects: bot Char5 and the user's hero fdsfsdfsd,
    all GAS sets). Checked: role base (`role_affix`), BASIC (template offset and fixed_value), MAJOR/MINOR
    (`affix_value_template`, rarity `UeSource_ItemType`), secondary from primary (the game's `FirstLevelAttrDataAsset`:
    Strength → phys. damage ×0.005, Agility → speed ×0.3 and CritDamageRate ×0.004, Vigor → MaxHealthPercent ×0.007
    and resistances ×0.001, Intellect → elemental damage ×0.005, Will → SkillBoostRate ×0.0055, Faith → HealBoost and
    ShieldBoost ×0.007). The game writes everything to BaseValue (Base = Current), ROLE_HOLD applies only to the weapon in hand.
  - [x] **The roll formula is discrete, not linear:** value = min + floor(level × n / 10000) × unit,
    n = (max − min)/unit + 1. This is a uniform choice among n values, the maximum is reachable (the audit question "top 6~7
    unreachable" is closed). The linear min + (max − min) × level / 10000 with any rounding does not fit (no more than 192 of
    252). The backend does not compute values. Text fixed in `HANDOFF_CLAUDE_CODE.md:27` and `content/balance/kit/kit.toml`;
    the prompts in `balance_kit/out/*/llm/PROMPT.md` come from the old kit.toml and will update on the next kit build.
  - [x] **Observation (not a backend bug):** defense level 2004 from gloves, boots, amulet and rings (template
    fixed_value) does not reach GAS — there are no fields for these slots, `ExtraDefGearLevel` = only MINOR 2005. Apparently
    the value is only for the UI tooltip.
  - [x] **Hero damage (longsword vs monsters) matches to 0.0004%:** damage = K × (1 + AttackBoostRate) ×
    (crit ? CritDamageRate : 1) + D, where D = AdditionalElement*DamageOnCausingHit × (1 − (target res. −
    ElementResPierceNone)). CritDamageRate is the full crit multiplier (role base 1105 = 1.0). A +0.3 buff to
    AttackBoostRate gave exactly 1.606/1.306. Combo hit coefficients: Quickside2 = 1, Quickside3 = 1.3.
  - [ ] **Open:**
    - Quickside1 gave 5 of 5 identical hits ×1.3932 = 1.2 × 1.161 — either all crits (p ≈ 0.04 at CriticalRate 0.523)
      or a coefficient of 1.3932. Check via the `crit` records.
    - Multiplier K is not split up: weapon damage × GE coefficient × elemental boost × attack level vs defense
      level. A skeleton hit a target with defense level 2 for 168.6, with level 4 — for 164.2 (×1.027).
    - Skeleton hits on the hero: 64 (res. 0.291) and 175.9 (res. 0.591, apparently while blocking) — not explained.
    - 5 HP drops bypassing the hooks (617 damage, 601 of it to the hero): damage is computed in C++ bypassing the BP library (DoT,
      Reaper).
    - Need a raid with mod v2 and different weapons and armor.
  - [x] **Check "equipment counted twice in combat" (user complaint 05.11, raid 16:39–16:43,
    `statwatch-20261105-163543.jsonl`): no doubling on the DS.** `role_affix` from the backend for all roles equals exactly the
    `role_level_upgrade` row (class + level, without items), so the expected value is not doubled either. Hero BaseValue =
    base + items exactly once: 249 of 249 attributes. Health confirmed: MaxHealth = MaxHealthBase ×
    (1 + MaxHealthPercent) + MaxHealthExtra (456 × 1.687 + 15 = 784.3). Additions above base (CurrentValue −
    BaseValue) are GameplayEffects, the report now has a separate "Buffs" section:
    - `CriticalRateWithFullHealth` from items gives +0.245 crit chance, only while HP is full;
    - Flask of Cruelty — +0.15 crit and +0.15 crit damage;
    - Flask of Protective — +0.05 to resistances and `DamageReduceRate`;
    - `MaxHealthPercent` +0.025 and `HealOverTime` +1.

    Why it may seem higher in combat than in the lobby:
    1. Conditional effects and potions — in combat.
    2. Weapon MAJOR/MINOR of **both** pairs always apply: location `ROLE_EQUIP` per the game template, with no weapon in hand
       only BASIC applies (`ROLE_HOLD`). For mage Mage1 (guid 1060933106) the sword in the second pair gives +0.2 crit.
    3. The mage's base crit chance is 1.008 (100.8%): boots MAJOR 0.391 + gloves and amulet MINOR 0.417 (our K20 pools) +
       sword 0.2. Crit on MINOR in 3 slots is a consequence of the pool reconstruction, in the original it might have been rarer.

    What the lobby shows, the watcher cannot see (it is the client UI) — need numbers from the user. Open.
  - [x] **Mod v1 did not tie crits to the hero's hit:** the hero's ASC lives on PlayerState. Fixed in v2 (pawn via
    `PlayerState:GetPawn`) and separate `crit` and `hitv` records added. v2 is in the Steam install and will be enabled on the
    next DS start. In `ApplyGE` the game passes DamageRate = WeaponAttack = 0 — the GE itself sets the damage.
- [ ] **I3 05.11 — done locally, not verified in game. After a raid the character's inventory "breaks": when
  dragging, items (1×1) appear/disappear.** Consumables picked up in the raid come in the DS results without a guid
  (guid 0) and without a role in the position, `raid_native` stored them as is (…9331 has 12 such things across two roles: potions in
  slots 41–43, 100018/100050/100060/100066/100106 in the backpack). The client sends `inventory_item_move` with guid 0, and
  `inventory_handlers::take` took the first item with guid 0 from any role: at 12:40:53 the client moved 100047 from backpack …103,
  the server moved 100018 ×3 from backpack …101 — client and backend diverged. Dead Money is not involved (set with guids).
  Done: `inventory_handlers::repair_items` — guids from the shared counter for items with 0/>2^31 and role type/guid in the position
  of bags/equipped; called on raid results (`raid_native`, before saving) and at login (one-off repair of old ones);
  `move_item` with guid 0 — rejected. Test `repair_gives_guid_and_role_position`.
- [ ] **I4 05.11 — done locally, not verified in game. Equipping a two-hander does not remove the off-hand** (mace + relic
  → staff: the relic stays in the slot and gives stats). For a two-hander the client sends `secondary_weapon_position` in `role_wear`
  (where to put the off-hand; for a one-hander there is no field), we ignored it. Done:
  `take_off_off_hand` — the item from the set's off-hand (1→2, 3→4) to the given cell (if occupied — the first free one of the same
  place), `taken_off_secondary_weapon` + dirty in the reply; same in `role_take_off`. Test `two_hander_takes_off_off_hand`.
  Slot 14 (a single case on 01.11 with secondary) is left alone — unknown which pair it is.
- [ ] **I5 05.11 — done locally, not verified. Stacks of the same type did not merge/split** (user:
  "different guids on items of the same type"). The client sends `inventory_item_move` with a count: 13:23:25 — "2 from stack 054
  (×3) onto the cell with stack 070 (×1)" + "the remaining 1 of 054 onto empty y8"; we moved the whole stack with a swap —
  server and client had different stacks, items "appeared". Done: `move_count` — onto a stack of the same kind we add up to
  the limit (`UeSource position_cfg.accumulation_limit`), part of a stack onto an empty cell — a new stack with a new guid,
  otherwise the whole stack as before; on action error — rollback. Test `stacks_merge_and_split_like_client`.
- [ ] **I6 05.11 — done locally, not verified. Crossbow does 0 damage; selling a crossbow/bolts — error -608.**
  (1) Starter weapons that the class puts into the backpack (crossbow 181040000) were issued with empty `data` — without BASIC
  `ROLE_HOLD 1101` (weapon damage). Now `build_starter_items` gives affixes to backpack equipment too
  (`inv_gear_affixes`), at login — repair of equipment without affixes (`inv_repair_gear_affixes`). (2) Grey
  starter weapons and bolts 100045 have `system_repurchase_price` 0 and are not in trade_commodity — `market_sell` kept the item and
  replied -608 (EnErrItemGuidInvalid). Now they sell for 0.
- [ ] **I7 05.11 — done locally, not verified. After a raid the gold in the main menu and in the stash differ.** The results
  (auto-sale of loot 907 775 → 915 662, deadmoney) pushed only dirty_role, without dirty_coin_data — the menu kept the old
  gold, the stash took the new value from player_get_info. `raid_push_role` now sends gold too.
- [ ] **I1.** Everything from C1: equip/unequip/move/sell — repeat the scenario after C1.
- [x] Toast "Invalid item class or slot" — correct (user, 02.11).
- [x] **I2 (analysis 02.11).** Crafting sent no RPC because there were no materials: recipes 101–103 — `100060` ×3 (sold,
  merchant tab `MarketTabName_1004`), 10000–10004 — `100066` ×3 (loot/quests only). There are no references for crafting and relics
  (PVE and sniffer). For tests — GM commands from chat (`/item 100066 15`, reply — `SCModalMessageSync`).
- [x] **I4. Crafting — per the PVE reference (recording 02.11 08:18), accepted as workaround K1/K2 (`docs/WORKAROUNDS.md`).** PVE: crafting is instant — the
  `crafting` reply immediately has `completed_count += count`, `uncompleted 0`, `start_time` of the first order, `time_cost` 1; no
  readiness pushes, dirty — only the consumed materials; `crafting_recived` → `item [{basic{type_id,count}}]` + dirty
  with the delivered item. We did the same (before: a timed queue, -4110 on a repeated order, "Completed 0/4").
  The in-game animation is apparently client UI only. Cancellation (`crafting_cancel`) was not captured in the reference.

- [x] **I3. Dismantling works:** `gear_dismantle` gives `1037` ×15 per the table — this is currency (`EN_ITEM_MAJOR_TYPE_CURRENCY`,
  material for repairs and relics), credited to coins + `dirty_coin_data`, not present in the inventory.
- [x] **S2. There is no general (world) chat in the client itself** (02.11, `ChatChannelConfigDataTable` from the pak, `.uexp` analysis):
  `Chat_Channel_World` has `bShowInPanel = false`; the "General" tab = `Chat_Channel_Public` — an aggregate (World, Team, Private,
  System, Room), you cannot send from it except to the available channels (team/DM/room). Enabling world chat — only
  with a client mod (table patch: World `bShowInPanel = true`). GM for tests — admin API `127.0.0.1:9092`
  (`core/backend/src/admin_api.rs`, `tools/py/gm.py`), GM from chat — in a party or room.
- [x] **S1. General chat did not work: "Failed to send message, you are not in a team".** We sent all
  `module_unlock_type` modules as unlocked, including `CHAT_SILENCE` (1200002) and `CHAT_ONLY_ENGLISH` (1200016);
  the official server and PVE do not send `user_unlock` — now neither do we. Not verified in game.
- [ ] **I2 (before).** Crafting, alchemy, relics: the tabs opened (`client_action_log`: Craft_Supply/Collect/Compound,
  Alchemy, Heirloom), but not a single RPC went out — check after C1 what the client waits for (recipes in `user_crafting_data`?).

## Characters (profile)
- [x] **P2–P4 fixed and verified 02.11 04:58** (root cause — C6). Earlier: PVE reference `rpc_dump/PVE.jsonl` 04:21.
  After `role_create` and `hall_set_role` PVE sends: push `dirty_role` + `dirty_hall_setting`, then two `SCCacheExpiredSync`:
  `{EN_ROT_ROLE_BASIC=4, role}` and `{EN_ROT_PLAYER=1, player}`; the client re-requests `player_get_info` and the cache. We had:
  `hall_set_role` sent only `dirty_hall_setting`; `role_create` — cache reset with wrong types (2/3/5);
  `cache_batch_get` took the object type from `keys_with_status[].type` (that is `EnGetCacheType` Subscribe/HotData) and numbering
  2=RoleBasic — a role cache request with Subscribe returned the player profile → the character disappeared from the hall. Fixed:
  `common::push_role_cache_expired`, `hall_set_role` as in PVE, `role_create` — correct types, `cache_batch_get` —
  type from `req.cache_type` per `EnRouterObjectType` (1 Player, 4 RoleBasic, 5 RoleDetail, 6 UserGlobal, 7 UserMatch),
  cache constants in `player_handlers` (rename/appearance) — also 4/5.
- [x] **P3 (second cause) fixed 02.11 04:40, not verified in game.** Session 04:35: switching to role 001
  "dasdasdas" worked (hall opened, `player_get_info`), to 004 "Char3" — did not: after the pushes the client sends only
  `cache_batch_get` Player. Difference from PVE: in PVE the player nickname in the profile always = the name of the current hall role
  ("Adventurer" with no roles), ours is constant (the first role's name). Only switching to a role with the same name as the nickname worked.
  `player_handlers::player_nick` now takes the name of the `hall_role_guid` role.
- [ ] **P2. Your own character disappears in the lobby — dangerous (breaks the game).** Session 02.11 03:55: when switching role from the list
  (`ULoginCreateRoleWindow::EnterHallWithGuid`) the client does `cache_unwatch` `EN_ROT_ROLE_BASIC` for the old role, but does not subscribe
  the new one; at 03:57:55 it unsubscribed the current one too (…001) → the hall has nothing to show. After `hall_set_role` the server sends only
  `dirty_hall_setting`. No reference — capture from PVE SoloLobby (`PVE_SNIFF`): switch role from the list, enter the lobby.
- [ ] **P3. "Enter lobby" from the character list does not work on the first try / does not work.** Works at startup. 03:56:59:
  `hall_set_role` → reply + `dirty_hall_setting`, the window did not close; from the 2nd press — hall. 03:57:30: the current
  role selected — the client sent nothing and the window did not close. Hypothesis: the order (reply, then push) or a different push is needed.
- [ ] **P4. Character creation does not leave the screen** (`OnRoleCreateSuccess` does not fire → the player presses again → duplicate,
  P1). After `role_create` the client does not send `hall_set_role`. Reference — also PVE.
- [ ] **P3 after the nickname fix (session 04:43) — did not help, the cause is in the client.** After login (hall 004) switching to 001 —
  hall opened; then 006, 005 — no (after pushes only `cache_batch_get` Player); then 004 (the hall role at login) —
  opened. Replies and pushes are identical in all cases → need analysis of `ULoginCreateRoleWindow::EnterHallWithGuid` /
  `OnHallSetRoleRspSuccess` (C++ expert). Nickname = role name kept (as in PVE).
- [x] **P1 fixed 02.11:** `role_create` rejects a name taken by any role in the DB (case-insensitive) —
  `EnErrRoleCreateFailedRoleNameAlreadyToken` (-726), empty — -724. DB reset, the old one —
  `surreal_saves_before_reset_20261102`.
- [ ] **P1 (before). Character "Char2" created twice within 8 s** (two `role_create` with the same name, both accepted). No name
  uniqueness check; probably the creation screen did not close after the reply — clarify with the user, compare with a reference
  (the PVE/sniffer dumps have no `role_create`).

## Match and results
Native path (headless listen server, dbnative): raid results come from the server (`POST /ds/player_result`), docs/DS_PIPELINE.md.
- [x] **B1 02.11.** Results of an exit through the portal are applied exactly once (sale of extracted loot, gold, XP);
  a repeat → 409. Verified in game 4 times.
- [x] **B2 02.11.** Re-entering the raid after results (same OpenId, the game hands out the old PC/PS, only the
  connection is new) and entering without an OpenId → JoinGuard moves the player to the lobby. Verified 11:21–11:28.
- [ ] **B2a.** JoinGuard took a player leaving after the portal for a re-entry (12:13:46, their own connection was alive
  11 s after results). Fix is with the patch agent: a re-entry is only a connection that did not exist at results time.
- [x] **B3 02.11.** Results screen: on native results the backend sends `battle_settle_sync` (`raid_native::raid_push_settle`:
  what the server removed, level/XP before and after, `settle_interface_type` PVE, `gain_gold_out_of_portal` = gold for loot).
  The client (WBP_HallWindow `ShowBattleResult`) for PVE shows gold/XP toasts and the level window — the toasts are visible (12:13).
- [ ] **B3a.** XP and results map are taken from the last level selection in the lobby (`raid_finish_exp` → 10101), while the dbtest server
  is up on Classic (10102). In a match via a room the level matches; for dbtest — pass the map from the server.
- [x] **B4 02.11.** Death (from mobs, 12:25): the record status 3 / reason 2 appears only when the dead player leaves
  to the lobby (until then they spectate); `applied: death`, 11 items lost, deadmoney set "Crypt", +100 XP
  (level 2→3), level-up window in the hall.
- [x] **B4a 02.11.** Host commands for automated runs (DungeonborneRuServer + admin API, `tools/gm.py "host kill …"`):
  `kill <open_id|all>` — native GEs `GE_TGF_Damage_DeathZone_InstantDeath` / `GE_TGF_Kill_Self` (`DamageSelfDestruct`
  deals no damage in shipping), after 5 s — quit on behalf of the player `ServerSelfQuitGame` (without it there are no results: the native
  PlayerGameDeadPlayerState calls the quit RPC, not a disconnect), then ClientTravel; `leave <open_id|all>` — the same without
  the kill (reason 1 SELF_QUIT = death). Verified 12:39–12:40: kill → status 3/2 → `applied: death` within 6 s.
- [ ] **B4b.** Raid XP after a raid without results (kill v3 12:32) — 463 s instead of 16 (old `raid_started_at`).
  Fixed: a new server session resets the time when the record is reissued; not verified in game.
- [x] **B5 02.11.** Room token in a native match (`dbtest\run.ps1 -Match native`, 12:46–12:48): room
  b40003015_2 → the client travels to the server itself → `battle_user_data` with the token from the address is accepted; a foreign token → 403
  `bad_token`. Without a token in local mode (not `require = "relay"`) the request is let through and overwrites the issue
  session — the results of that raid then get 409 (that is what happened during the check); in strict mode without a token — 403.
  `leave` checked along the way: status 3 / reason 1 within 0 s.

## Party and match
- `team_match_search` / `team_match_exit` / `team_change_team_finder` — no errors (session 03:30).
- [x] Solo match via a room (native, B5 02.11) — works.
- [x] **M1 02.11 13:33.** Party of 2 (quick party search, windows F: and E:, `tools\stand\play.ps1`) → `team_id` 1 for both →
  the leader presses match → both in room b40003016_2 and on the same server.
- [ ] **M1a.** The party member is shown incorrectly (for both): `cache_batch_get` ROLE_BASIC/DETAIL by someone else's role_guid
  returned your own role. Fixed (role owner by guid = user_id·100+n), not verified in game.
- [x] **M1c/friends 02.11 16:29.** Party invitation (via a TeamMember chat message) and friends (request from the menu on
  another player) — work (user). There is no "Add friend" button in the menu on yourself — by design.
- [x] **S4. Cyrillic in chat is erased while typing** — works 03.11 (user). — the client (WBP_Chat_DetailPanel) filters "English only" if
  module EN_UNLOCK_MODULE_ID_CHAT_ONLY_ENGLISH (1200016) is not forbidden (IsModuleAllowed = "not in the forbidden list"; we did not send
  user_unlock, K5). 02.11: `global_ban = [1200016, global_forbid]` — did not help (03.11, both clients): a
  global_ban record without `ban_datas` does not forbid. 03.11: `user_unlock.unlock_modules = [{1200016, global_forbid: true}]`
  (agent, UUnlockService 0x145bb7210) — and only that.
  Not verified. Latin-only without a condition in friend search, search in the invitation window and report text (Lua) — the backend cannot remove it.
- [ ] **S3.** DM chat "same name everywhere" — not a data error (agent, LuaProbe 16:28–16:37): the line always shows the name
  of the OTHER PARTY — your own message "<Chat_Message_SendTo> other party: …", incoming "other party: …". The game's Russian locale
  translates Chat_Message_SendTo as "Send...", and your own message reads as incoming. Options: a pak with the ru string
  for "To" (GeneralStrings/Chat_Message_SendTo); remove the duplicate "Name(Name)" (nickname = role name, K8). Awaiting decision.
- [ ] **M1c (history).** Invitation (13:55, by nickname and from "Add teammate"): `team_send_invitation` goes through, the invitee
  gets `dirty_team_snapshot.pending_invitation`, but sees no invitation window/list → both invited each other
  and became leaders of one-person parties. What the client waits for to show it (WBP_TeamInviteListPage ← event
  TeamReceiveInvitation from UTeamService) — not found out.
  Cause (agent, Ghidra): the client creates an invitation ONLY from the chat message `DChatMessageDetail.team_member.invite`
  (UChatService 0x145997490 → UTeamService 0x145b8c5d0 → 0x145b8ca20), it does not read the snapshot. Fixed:
  `chat_push_team_member` — invite to the invitee, reject to the inviter, in the system channel `sys_<user_id>`; not
  verified in game. The client also silently drops an invitation if the recipient is in match matchmaking/a battle room, in a PVP room,
  in the character creation window or Mithril role selection.
  14:25: with "Team invitations" enabled — also nothing (the client does not send key 101, the setting is local). A friend
  request is not sent by the client at all (no `friend_invite` in the whole dump). No reference. With the agent: a client probe.
- [x] **M1d 02.11 14:27.** Quick search did not pair two leaders of one-person parties. Fixed (one in a party = solo,
  `team::try_match_locked`, test `lone_captains_match`), works in game.
- [ ] **M7 03.11 — done, not verified.** Cannot leave the party after a match: `team_exit` goes out, the backend replies with an empty
  snapshot, the party UI does not change. After the match both clients endlessly sent `battle_matching_heartbeat` (ack 3 = FINISHED,
  ~1/s) — the search did not close on the client. Then a search heartbeat without a live search (room closed, backend
  restarted) → RpcHead `EnErrMatchingNotFound` (-1001) — worse: the client hung in an endless "search" (11:58). Now
  instead of an error, the push `dirty_clear_matching [matching_id]` (as on search cancel): together with "match over" and on a heartbeat
  of a nonexistent search. Not verified. If it does not help — the agent will analyse UMatchService/TeamService.
- [ ] **M8. Reconnect** (left the waiting hall, crash mid-match): question to the agent 03.11 — how it works natively
  (user_battle_room at login, network_block_loading, how long the server keeps PC/PS, the native hall BP on exit).
  Agent's analysis — docs/RECONNECT.md: the client travels back by itself up to 10 times while the room is BATTLING, then the window
  "Reconnect to the match?"; there is no reconnect from the waiting hall (whoever left is absent). 03.11 backend:
  `player_get_info.user_battle_room` with the running match (status 8, address, token) — for a restarted client. Results of a player
  who did not return — not by being offline, but by the character's state. Agent's experiments 03.11 (RECONNECT.md): after a disconnect the PC/PS and
  the live pawn remain; death (of both a disconnected and a connected player) itself records results 3/2 within 1–2 s — "quit on behalf of the dead"
  is not needed (we were wrong on 02.11); a disconnected live player holds the room until returning/end of match (`DS_ROOM_OFFLINE_SECS` = 0).
  `exec disconnect` in a match: the client returns by itself within 6–7 s into the same PC/PS. UX (user): the native window
  "Reconnect to the match?" right away. DSConfig.json with DSMaxReconnectTimes=1 in the pak apparently did not apply (loading_cnt 1→2,
  no window). A window right after a restart is impossible without writing to memory (at least one auto-attempt) — the user
  chose a button: at login we do not send `user_battle_room`, "To battle" from a player with a running match → `matchmaking::rejoin` into their
  room; at login a hint window "The match is still on". Verified by the agent (rjtest 12:49–12:53): the same PC/PS. Not verified:
  the hint window in game, party (teammate in the match, leader presses "To battle").
- [ ] **M3c 03.11 — done.** The backend was restarted mid-match: the server is in a match, the backend has no room — the slot was considered
  free and could go to a new room. Now such a slot is "orphan": it gets no rooms, after `DS_ROOM_TTL` — reset.
  Test `orphan_battle_after_backend_restart`.
- [ ] **M8a 03.11 (user, not critical).** "Return to battle" for a teammate (not the party leader): they have a
  "ready / not ready" button, not "To battle" — they do not send `battle_matching_search`, rejoin does not fire. Worked around by leaving the party.
  To do: rejoin on a teammate's `team_send_battle_confirm` with a running match too (or on any button press).
- [ ] **M9. Room limits: players, teams, spawn points** (user question 03.11). Currently the matchmaker counts
  players (`MATCH_ROOM_SIZE` 3, `MATCH_MIN_PLAYERS` 1, `MATCH_WAIT_SECS` 15), a party as one ticket. There are no matchmaking pools or
  official-server mode rules in the client tables; map_detail.dynamic_option_args count 1/2/3. The agent
  counts the start sets (ALyraPlayerStartSet) on the maps; then the limit "teams per room = start sets".
  03.11 user decision: solo, duo and trio have their own queues. Done: pools (level_id, group size), a room holds
  only groups of one size, the limit is `MATCH_ROOM_TEAMS` groups (3, was MATCH_ROOM_SIZE players), `MATCH_MIN_TEAMS`;
  the map's dynamic_option_args — by group size (`BattleRoom.party_size`), not by player count. Test
  `queues_split_by_party_size`. Agent's census (DS_PIPELINE "Spawn points by map"): set = team (3 starts),
  MaxPlayers 20; dungeon 16 sets + 4 solo-only, hall 18; castle 8 + 6 (Hard 8 + 5), hall 20. Group limits
  (`matchmaking::max_teams`): dungeon solo 18 / duo 10 / trio 6, castle solo 14 (Hard 13) / duo 8 / trio 6.
  Not verified: what the game does when sets run out (need ≥ 17 teams), whether the hall seats people by sets.
- [ ] **M1e.** The match buttons of the leader and the teammate look different — clarify what is wrong (the teammate sends
  `team_send_battle_confirm/unconfirm`, the leader — `battle_matching_search`).
- [x] **M2a 02.11 14:45.** The party is in one team (`[TEAMS] party … -> MyTeamID 1`, 14:28), but the spawn points differ: the team
  was set after the start was chosen (an ALyraPlayerStartSet set is bound to a team). Agent: ChoosePlayerStart hook +
  `GET /ds/team_of?open_id=` (new, no side effects, `ds/agent_link.rs`). User: "players together".
- [ ] **M1b.** The E: install had no mods and no BPFix (client crash after `role_create` 13:25) — synced with F:,
  old paks in `E:\…\Content\_paks_backup_20261102`.
- [ ] **M2.** Team in a match: the backend sends `DBattleUserParameter.team_id` = party id, a solo player gets their own (`team::battle_team_id`;
  it was 1 for everyone — raid solo players were allies). Server: our mod (COOP_SAME_TEAM) gave everyone the "host" team; the native
  assignment (`teams.txt = native`, 13:34) gives a party different teams 1 and 2 — it does not look at team_id. With the agent: party mode
  (MyTeamID/MySquadID from the record's team_id).
- [x] **M3 03.11 — verified:** `tools\stand\resetcycle.ps1` 3/3 (10:30–10:37, agent): one server process, three matches
  in a row by the same player, reset 23–26 s, no "ghosts" or crashes. Contract — docs/DS_PIPELINE.md "Server reset between
  matches". Next: scheduled process restart after N matches (memory +50 MB, handles +150 per match) — with the agent.
- **M3 (history).** The room lived until TTL (45 min): in the native path the server does not send ds_end.
  Now native results (`raid_native` → `matchmaking::member_result`) are marked in the player's room; when everyone has results —
  the room is closed, match statuses are cleared, the server is free in the allocator (the `/ds/player_result` reply has `room_closed`).
  A member without a gateway connection for longer than `DS_ROOM_OFFLINE_SECS` is not waited for — off by default since 03.11 (0), we wait until the end of the match (M8) (matchmaker tick; in a raid the client
  keeps the gateway — ping every 30 s, dump 02.11). Test `room_closes_after_all_results`.
  Server reset (contract with the patch agent 03.11, `lobby/allocator.rs`): slot free → busy → after the room closes
  "reset", not free. Every 10 s the mod sends `POST /ds/slot {port, slot?, phase, epoch, players…}` → `{action:
  none|reset|quit, battle_id?}`; reset — while the slot is in reset and the mod has not reset (covers closing by timeout).
  Free — a heartbeat with phase=ready and a different epoch (map reload or process restart). Heartbeat silent for
  60 s — the slot is dead, gets no rooms. Without heartbeat (old mod, dbtest) — as before, free immediately. On a repeated
  result (409) for a closed room whose server is still in reset — also `room_closed`. Debugging — `GET /ds/slots`.
  Test `slot_reset_cycle`. The mod (ClientTravel of those remaining, JoinGuard cleanup, map reload) — with the agent; the reset
  cycle takes 24–28 s (agent, `tools\stand\resetcycle.ps1`).
- [x] **M3a 03.11 — verified by an agent run.** The client of a closed room travelled back to the server by itself after ~20 s
  (`network_block_loading`, loading_cnt 1→5): the backend never reported the end of the match — the last room event was
  BATTLING up to +2400 s. Now after results the player gets pushes of room events 5 `gameOverSettle` (results) and 6 `finish`, status
  FINISHED (`lobby_handlers::room_finish_push`); a heartbeat of a closed room is answered only with that and no longer gives the server
  address. battle_ids are unique across backend restarts (numbers derive from the start time). There is no reference for the match end:
  which of the two events the client uses — check with a run. Test `finish_pushed_once_and_stops_room_events`.
  Agent run 03.11: after room_closed the client no longer reconnects.
- [x] **M3b 03.11 — verified by an agent run.** A room without results (409 for the player) hung busy: the same open_id back in the lobby and in a new
  search — the "not in game" timer did not run, the server did not go to the next room. Now a new search or a direct room
  marks the member in old rooms as finished (`matchmaking::abandon_locked`); everyone finished — the room is
  closed, the server goes to reset.
- [ ] **M6. Waiting hall before the match** (docs/WAITING_ROOM.md). The agent's prototype 03.11 10:54–10:58 ran end to end: hall
  `L_Dungeon_Matching_01` → native countdown → servertravel into the match → the client reconnected with data → results → reset.
  Backend — a `host` task behind the flag `DS_WAITING_ROOM=1`. The chain is verified: `tools\stand\hostflow.ps1` 2/2 (11:17–11:23):
  task → hall → client after 4–5 s → entry countdown 10 s → match 34–36 s after the task → results → reset to the hall 18–19 s.
  The client travels to the server only on startBattling — we send all 4 events as soon as the hall is loaded (round 1 11:04).
  Not run: `absent`/`empty` (needs a no-show and a lineup of 2+), the Gothic hall (hall reload), the second floor.
  Later: set `dynamic_option_args` by player count (difficulty 1/2/3) on the server instead of the client's MAP_RULES;
  prefer a slot where the needed hall is already loaded (if a hall reload turns out to be expensive).
- [ ] **M4.** XP and results map — by the room's level, not by the last selection in the lobby (= B3a).
- [ ] **M5 03.11 — done, not verified in game.** Match access by roles: `[auth] battle_roles` (empty — everyone, as
  before; `gm`/`admin` — always), Discord role → our role in `[auth.discord.roles]`. Without the role, `battle_matching_search`
  (for a leader — for the whole party) and `room_start_battle` (the whole room team) are refused: code -1028 in the header and a
  `SCModalMessageSync` window with `battle_denied_message`. Roles — from the last login (K9). Test `battle_roles`.

## Mail / quests
- [ ] **Q1 03.11 — done, not verified in game.** Quests were empty: daily quests of passport 16518 were issued only in
  `activity_center` (PassportData.DailyQuests), while the client takes quests and progress from `user_quest` (TaskService).
  Now `act_sync_quests` (shop_handlers, on `player_get_info.need_user_quest`) puts them into `progress_quests`, as PVE
  02.11: 3 daily from pool 16518 + 10 weekly (`progress_reset_period`). Daily — anew at the start of the day (00:00 UTC),
  weekly — at the start of the period (start_day_reset_timepoint 2024-05-27 + k·7 days); replacing a daily
  (`passport_refresh_quest`) changes the quest too. Progress (`quest_event`, conditions `quest_progress_condition`):
  - leaving a raid — BATTLE_VICTORY (+ BATTLE_PARTICIPATE for any result), level type — from the lobby selection (like B3a);
  - dismantling gear of uncommon and above — ITEM_CONSUME;
  - receiving something bought on the marketplace — TRADE_BUY_ITEM (coins do not count).
  Rewards — already existed (`quest_rewards_receive`). Test `passport_quests_sync_and_events`.
- [ ] **Q1a. Kills, loot, damage (KILL_TYPE, LOOT, PLAYER_CAUSED_TOTAL_DAMAGE) — need server data.** The native results
  record (`DBattleUserData`) carries no match statistics; the official DS has `DDsUserBattleResult.player_statistics`
  (`DRoleStatisticsData`). Without them 2 of 3 daily and 3 weekly do not progress. With the patch agent: pass statistics in
  `/ds/player_result`. Quest 1601000 (CONSECUTIVE_DEAD, set 1710002) is not issued — deaths are already handled by deadmoney.
- [ ] **Q2a 03.11 — fixed, not verified.** Mail (user): deletion did not remove the letter, after taking the attachment
  the letter was not "completed" and the attachment could be "taken" again, then the inventory broke entirely.
  (1) Attachment "to the backpack": the client sends a place without x/y (0,0), `grant_items` put the item there without checking the cells →
  overlap with an occupied cell → client and server inventories diverged. Now the cell is checked (`inventory_handlers::
  area_free`), otherwise the first free one in the backpack, otherwise the stash; `limit_option` from the client's place is not stored. Test
  `bag_cell_must_be_free`. (2) The server did not issue again (-915), but the client's mail list did not refresh: now after
  every operation a push `mail_change_sync.mails` (DMailRecord: status, received_items, remove_time).
- [ ] **Q2. Mail** — implemented in full (mailbox, reading, attachments, "take all", placement into the stash), not
  checked in game: nobody sent letters. To check — GM `/mail [type_id] [count]` (a letter to yourself, with an item attached).

## Shop
- [x] **Sh0 03.11.** Merchant — works normally (user): tabs, buying for gold, equip/unequip/move purchases,
  selling. Buying with guids after C1 is fine.
- [ ] **Sh1. Buying back sold items does not work** (user 03.11, "just write it down"). The log at 13:24–13:25 has two
  sales with `dirty_sold_items_list`, but not a single `market_buy_sold_item` — the client did not send a buyback request. Check:
  whether the client sees the buyback list (format DUserMarketSoldItem, market_sold_ver) and what the button waits for.
- [ ] **Sh2. Skin shop:** all skins are unlocked (by design, `player_facade_owned` + everything from facade). Equip/templates —
  replies exist (`facade_update_role_skin`, `facade_save_skin_template`). The inventory breakage after that is not because of skins but
  because of a mail attachment (Q2a).
- [ ] **Sh4. Moonstone for kills (deadmoney, proposal 03.11).** In the game the moonstone is currency 1012
  (MoonStone_Pay/Bonus in localization); the shop (tabs 3001–3005, market_type 102) sells skins, gold,
  the pass for it (3004 — the stones themselves for real money, 1015). Idea: in deadmoney mode a player kill → N stones by mail
  after the raid; skins — bought in the native shop (remove K11), prices — in balance (market_product_item.cost_price).
  03.11 user decision: 1 kill = 1 stone, no multipliers, prices / 10, nothing for newcomers, DLC — for stones.
  Done (backend): `POST /ds/kill {killer, victim}` → `state.dm_kills`, on the killer's results a letter "Kill reward"
  with 1012 (`raid_native::dm_register_kill`, `deadmoney.toml [kill_reward]`); buying an appearance unlocks it
  (`facade_own` + dirty_facade_data), "all skins unlocked" off (K11 removed, starter ones — `DEFAULT_FACADES`); balance:
  66 products + 4 special prices for 1012 / 10, +40 appearances (DLC 25, event rewards 15) at 30 stones (790001–790040).
  Test `kills_counted_for_killer`. Waiting for: from the agent — POST /ds/kill from the mod; `balance build --deploy` with the game closed.
- [ ] **Sh4a 03.11 — live test with two players (14:39–14:47):**
  (1) 36 stones charged instead of the 40 shown: the backend took cost_price (2024 discount), the client — origin_price; now outside
  the discount_start/end window the price = origin_price (`market_cost`). (2) the backend sends the 40 new 790xxx products, the client does not
  show them — WBP_Store_FacadePanel filters by buy_start/end_timepoint, IsSkinItem, ExcelFacade sort; added
  buy_start_timepoint, not verified. (3) CRITICAL: the server reset dragged two still-connected players into the hall, they were again
  given match data → a second self_quit result (−9 items) — with the agent; backend: battle_user_data 409 room_closed if
  the room is closed and there is no new match. (4) no letter with a stone: the server did not see the killer (statistics empty) — with the agent;
  backend: killer outside the raid — letter immediately. (5) "every second player fills the party in the match", (6) host pawn — with the agent.
  Agent 03.11 15:06–15:19 (hostflow 2/2 ×2): reset without kicking (we wait for them to leave, 15 s — "to main menu", ~30 s total);
  the host gets team 254 (was 2 → a player in the same team as the host); the host becomes a spectator 5 s after the first player
  (`dbtools
un\dbserver-observer.txt` = `late spec destroy`, mobs in place). In (4) the killer apparently was correctly not found —
  both were killed by the zone; need clean PvP. Not verified live: room population by mobs while running, a PvP kill.
- [ ] **Sh4b 03.11 15:22–15:40 — live test 2:**
  (1) not all products in the shop — a note, not critical. (2) on F: (609331) the purchase says "not enough": shop tabs without
  a period (3001–3005) were never rebuilt — the player had the old prices (250 instead of 25) and no 790xxx.
  Now a tab config signature (`tab_signature`): products/prices changed — rebuild keeping purchases; tabs
  without a signature are rebuilt once. (3) skins are issued and equipped — ok. (4) Portal resonance stones
  (705019/705020, tab 1019) removed from balance; which tabs to remove — clarify. (5) crit chance 72.4% for DK 609331:
  base 1104 = 0.2 from role_level_upgrade, only one 1104 on items — analyse the final calculation. (6) the spectator host breaks
  spawn → below the map (agent). (7) "any exit breaks the character": after the room closed, each search heartbeat
  (once per second) triggered dirty_clear_matching — now once per (player, matching_id); what exactly is "broken" —
  clarify. (8) no letter: the killer is again "not a player (statistics empty)" — the agent is looking for another source.
- [ ] **Sh4c 03.11 16:06–16:13 — live test 3:**
  (1) the added DLC appearances (790xxx) are broken in the shop — probably the tables lack data (unfinished
  by the developers); a note, do not remove. (2) deadmoney anti-abuse: after a raid all equipment in the stash is deleted
  (`raid_native::dm_wipe_stash_gear`, `deadmoney.toml stash_no_gear`). (3) merchant: tabs with random equipment
  (1009/1010/1011/1014/1015) and the empty 4444 removed from market_info 101 — 1016, 1017, 1019 remain. (4) "after an exit
  characters are broken" — role data is fine, no client errors; clarify with the user what exactly. (5) BUG: you can
  spectate the host (the spectator camera switches to the hidden host) — with the agent.
  (4) CRITICAL, clarified: after any exit your own character is missing from the lobby, tabs and inventory break. The client
  does not disconnect from the backend. After player_get_info the client requests only cache EN_ROT_PLAYER and does not subscribe
  ROLE_BASIC of the hall hero (like P2). Differences of the reply from the working exit at 11:58: non-empty user_sold_items_list, bigger stash.
  Suspects: equipped unbought skins (K11 removed), the sold list, clear_matching. Analysis — agent (LuaProbe). (6) the letter with a stone ARRIVED:
  [KILL] … killer 76561198000000903 (DeathEvent) → /ds/kill → letter "Kill reward: 1" at 16:12:48.
- [x] **Sh4d 03.11 16:27–16:40 — live test 4 (user):** kill stones arrive by mail; stash equipment
  is deleted after the raid; merchant — consumables only; characters are fine after an exit (the reset again does ClientTravel to the
  lobby after 30 s, agent).
- [ ] **Sh4f. CRITICAL, it came back: after a raid your own character is missing from the lobby, tabs and menus break** (the third time
  after Sh4b(7) and Sh4c(4)). Prod: 03.11 18:08:50 (…738, death, lvl 1→2), 03.11 18:40:10 (…739, lvl 1→2, restart),
  04.11 07:18:08 (…739, portal exit, lvl 2→3). Successful exits (04.11 06:04, 03.11 17:03, locally 10:35) — short
  runs (~1.5 min) without a level-up. ClientProbe (E:): after the results pushes the client has not a single event
  (no RoleInfoNoti for the first `dirty_role`, no reply to `player_get_info`, no trade) — all receiving stops, sending goes on.
  All bytes arrived (relay down=206073 = backend). Ruled out: a frame >100 KB (the whole stream is 206 KB, ~160 KB of it heartbeat),
  Lua errors (none), item guids (32 bit), server kick (left via portal). Agent (exe): the `dirty_role` handler
  has no waits or requests. 04.11 GM `/exp n` in the lobby (level-up + `dirty_role`) — everything works. Suspects:
  a level-up while the client is on the DS map; a long raid (XP accrues over time — the level is just a consequence); the push
  `dirty_clear_matching` in `room_finish_push` while the client is in the match (Sh4b(7) already broke the lobby with that push). Workaround — restart the game.
  04.11 08:34 leaving the hall after 20 s with a level-up 8→9 on the DS map — the stream did NOT stop (RoleInfoNoti, GetInfo present):
  level-up on the DS is cleared of suspicion, the long raid and clear_matching remain.
- [x] **B10 04.11 — deployed 08:47.** While the backend was down (deploy 07:44, ~30 s) ds-agent considered the live standby 7777
  "missing" (no port without a map, heartbeat unavailable) and started a second process on the same port → two servers on 7777:
  one got the room, the client went to the other ("did not arrive in the hall" 08:25). Now without the backend and for 60 s after it returns
  the agent decides nothing (ds-agent/src/main.rs, BACKEND_GRACE). Leftover: process 5344 (since 23:44 Robot time) hangs without the mod.
- [x] **B11 04.11 — deployed 08:47.** Raid XP counted from the time of the previous issue without results: the room at 08:25 was closed without
  results → leaving the hall at 08:34 after 20 s was counted as 559 s (+150, lvl 8→9). A new raid record now sets
  `raid_started_at` = issue time (`raid_native::raid_entry_issue`).
- [ ] **Sh4f check 04.11 — deployed 13:18 UTC; 13:47 result: the stream did not stop** (…729, raid exit after
  157 s: CANCEL/room/clear immediately, deferred dirty_role+quests+settle after player_get_info at 13:47:26, search heartbeat
  stopped after ~6 s). So it stops when the results pushes arrive while leaving the raid map. Side effect:
  no results screen (settle arrives after HallWindow opens) — question to the agent about when to send so that the lobby shows the results.
  Agent: results are shown only when the lobby opens (CheckAutoPopupSystems in OnEnterHall), there is no subscription to a late settle.
  Window: the client has left the raid map, the lobby is not open yet (~4 s). Done (deployed 14:03 UTC backend, mod on
  Robot 14:05, 7777 restarted; not verified in game): the RuServer mod sends
  `POST /ds/player_left?open_id&battle_id` when the player's connection drops after their results; the backend
  (`agent_link::player_left`) immediately delivers what was deferred. Safety net — player_get_info (without the results window). 12:50 (no level growth, 9→9): after the raid the client stopped
  receiving (all 90985 bytes arrived, not a single network event after ReturnToLobbyConfirm), so search CANCEL/clear did not
  take effect — "Start" is dead. It stops when results pushes arrive on the raid map; in the hall — no (06:04, 08:34, 10:09,
  10:15). Agent: the SCBattleSettleSync and dirty_role handlers wait for nothing. Check: while results are applied
  `hub::set_defer` — dirty_role/quests/letters/battle_settle_sync accumulate and go out after the reply to the first player_get_info
  in the lobby (`account_handlers`); search CANCEL + room 5–6 + clear — immediately. env RAID_DEFER_SETTLE=0 — turn off.
  Analysis: the reply to this player_get_info not handled → it stops from leaving the raid map (look at NetworkManager); handled
  and stopped after the batch → bisect the pushes; all passed → keep it. Raid for the check — exit 30–60 s after loading.
- [ ] **M12 04.11 — deployed 12:46 UTC, not verified in game. After a match "Start" did not work.** 12:39: after the raid
  `dirty_clear_battle_room` removed the room (room heartbeat stopped, the character is in the lobby), but the search heartbeat
  went on — clearing the room does not close the search, and CANCEL was suppressed (mark_match_closed). Now
  `room_finish_push` sends the search CANCEL event FIRST (hub, same channel), then events 5–6 and
  `dirty_clear_battle_room`. CANCEL after the clear (10:48) broke the button — the order matters.
- [ ] **G1 04.11 — HEROFIX deployed 13:30 UTC, not verified in game.** The agent moved FixHeroPawn from CrashTracer into
  DungeonborneRuServer (`HeroFix.Tick` every 1 s in Ready, `[HEROFIX]` lines, switch dbserver-herofix.txt=0) and
  RuClient (LoopAsync → ExecuteInGameThread, `ru-herofix.txt`=0). Gravity cause (agent): UActorMapComponent on raid maps
  (World Partition) zeroes gravity for actors far from the "local player" — for us that is the spectator host, i.e.
  everyone; the hall has no such component — fine there. Robot: both main.lua, 7777 restarted 05:31 Robot time. Clients:
  `ru.client-hooks@2026.1104.1930` (built from C:\Games\Dungeonborne, the other 10 packages unchanged; the old F: main.lua —
  in scratchpad RuClient_main_F_20261103.lua). Before: the gravity and second-player combat fix was disabled on the server. It lived in CrashTracer (FixHeroPawn:
  DisableActorMap, GravityScale 1.28, UseHitValidation=false, ShellMesh/AnimLayer, managers, tooltips) — RuClient/RuServer
  do not have it, although docs/fix_listen.md says otherwise. CrashTracer disabled on Robot 09:46 UTC (B13, suspected of the crash);
  user: keep it disabled. PrivateWorld.lua (GuardNpc) went away with local match. The agent moves what is needed into
  DungeonborneRuServer/RuClient with deferred handling; check whether there is a client part in ru.client-hooks 1.3.0.
- [ ] **M11 04.11 — revised, deployed 12:33 UTC, not verified in game: a player who left the hall does not return
  to the same room.** Agent (exe): the same in the original — exit reason -5017 BATTLE_WAITING_ROOM_SELF_QUIT, error -1025
  SELF_QUIT_WAITING_ROOM_BAN_MATCH + `self_quit_ban_creat_matching_time_duration` (a search ban after leaving the
  hall, not implemented — by the user's decision), the DS erases the OpenId from the token table.
  11:33 (after the snapshot fix): return to the same hall — events 1–4 with a BATTLING snapshot were received, but the client
  did not send `battle_fetch_battle_user_data` and did not travel to the server (on the first entry it did). The client does not return to a room with the same
  battle_id after leaving — a game peculiarity, not fixed (user: "maybe don't fix it"). Now
  `BattleRoom.left`: a player who left is not taken into this room (`join_open_halls`), an empty room is closed immediately; a new search —
  into a new room (another hall or a spare server). The other players in the hall stay in it. Before (11:09–11:3x):
- [ ] **M11 (before) The hall lives until the timer ends.** User: "while the timer runs, people can
  leave and come in without problems". Before: a solo player leaving closed the room → reset/process exit → every search on
  a new server. Now `leave_room` does not close an empty room while `allocator::hall_open` (the tick does not count it as
  "everyone has results" either); a new search (including your own) gets into it via `join_open_halls`, the returning player gets room events and FINISHED
  again (`room_sent_reset`, `finished_sent`). An empty hall is closed by the mod (phase=empty on EnterGame) or by the tick when
  < 25 s remain before the match. 11:24 verified: return to the same hall worked, but events 1–4 went out within a second over two channels
  (client heartbeat and the driver), the LOADING snapshot arrived last — the client did not travel, "Start" sent only pre_search.
  Fixed (deployed 11:30 UTC): the snapshot of each event is the current room state (the latest available), not
  the event's own. Open: at 10:59:19 the client itself sent `battle_matching_exit` 1 s after CAN_JOIN_IN, and in the
  next room (10:59:38 BATTLING) it did not travel to the server ("did not arrive in the hall" 11:01:37) — cause not found.
- [ ] **M10 04.11 — deployed 10:41 UTC, not verified in game. The client never closed the battle room** (root of "Match found, connecting"
  when returning to the lobby, the dead "Start", B8). Agent's analysis (exe, research\battleroom\README.md): URoomService sends
  `battle_room_heartbeat` while its battle_id is not empty, does not read the reply; it clears the room ONLY on
  `SCPlayerDirtyChgSync.dirty_clear_battle_room` (silently, also resetting the search — MatchStop) or cancel/CANCEL snapshot (with
  a reason window). Finish, FINISHED/TIMEOUT, game_over_settle — do not close it; with a snapshot in SCBattleRoomLogSync
  events are not read at all; any sync by battle_id brings the room back up. Verified 10:09/10:15: after our 5–6 the
  room heartbeat always continued (after a normal raid too). Done: `lobby_handlers::room_clear_push` — after events
  5–6 (`room_finish_push`) and on a heartbeat of a closed/unknown (with DS_WAITING_ROOM) room, once
  `dirty_clear_battle_room [battle_id]` + `dirty_clear_matching`; after it we do not send sync for this room. The timed search CANCEL
  after FINISHED was removed (in the hall it showed "search cancelled"). 10:47 first exit from the hall — search works again
  right away. 10:48 second — a late search heartbeat 1 s after the clear got CANCEL, after which "Start" sent
  pre_search without search; since 10:55 UTC a search closed together with the room does not get CANCEL
  (`matchmaking::mark_match_closed`). Check: after a raid/hall there is no
  `battle_room_heartbeat` from the player, no "Match found" toast, "Start" works.
- [ ] **M9 04.11 — deployed 10:05 UTC, partially verified. The search never closed on the client.**
  10:09/10:15: CANCEL accepted, the search heartbeat stopped; "Match found" remained — because of the room (M10). After a raid
  "Start" did not work (09:53: `pre_search` went out, `search` did not — `already in matching`), after the match the client endlessly
  sent `battle_matching_heartbeat` of the old search; the same caused "Match found" after leaving the hall, the hang after
  a cancel (09:27), M7 (cannot leave the party). Reference (sniffer_log_001 lines 124 → 171): after FINISHED (event 3), after
  ~10 heartbeats the server sends `SCBattleMatchingLogSync` **event 4 CANCEL** (`cancel {reason: -5012 MATCHING_DESTROY}`,
  CANCEL snapshot, last_event_id 4) — only after it does the client stop sending search heartbeats. We did not send it, and
  search cancellation went as a snapshot with last_event_id 4 without an event (the client did not accept it); `dirty_clear_matching` and FINISHED do not close
  the search. Done: `matchmaking::note_match_event` (number of the last search event per player),
  `match_cancel_msg` (CANCEL with the next number, once; unknown — 4); search heartbeat — CANCEL 10 s after
  FINISHED or immediately if there is no search; `battle_matching_exit` — CANCEL SELF_QUIT (-5014); `room_finish_push` — CANCEL
  MATCHING_DESTROY. Test `match_cancel_follows_last_event`. Check: after a raid and after leaving the hall "Start"
  works, the search heartbeat in the log stops after the line "event N CANCEL".
- [x] **B13 04.11 — deployed 09:43 UTC.** 7777 crashed while loading the hall (09:24:08, AV in UE4SS.dll from
  NotifyOnNewObject during SpawnDefaultPawnAtTransform — the same stack as 31.10; not B12, agent's analysis research\crash7777).
  The room waited ~2.5 min for the same port to restart, the client hung in "connecting" (did not accept "match over" or the cancel,
  sent room/search heartbeats — only a game restart helped), although 7787 stood free. Done: (1) room
  failover — the server is silent >15 s while the hall is not loaded (clients have not connected yet), the task goes to a free
  server, the old one to reset (`allocator::failover`, test `failover_moves_room_while_hall_loading`); (2) CrashTracer
  disabled on Robot (mods.txt, main suspect); (3) start_headless copies the shared `UE4SS.log` to
  `Kit\logs\ue4ss-<time>-before-<port>.log` before each launch (keeps 40).
- [ ] **B13b 05.11 — done in the mod, being checked locally, not deployed on Robot.** Without CrashTracer the crashes remained:
  04.11 on Robot 3 times the old signature (01:24, 02:14, 06:42 Robot time — AV 0xffff… in UE4SS.dll ← UWorld::SpawnActor
  0x1450e0023 ← SpawnDefaultPawnAtTransform 0x142fa6b91, i.e. a NotifyOnNewObject callback while creating the hero) and 1 new one
  (7777, 15:45:54 UTC, a 9-player raid ~25 s after the map loaded: AV 0x15b in UE4SS.dll, Lua from the engine tick —
  ExecuteInGameThread/ExecuteWithDelay). The culprit of the old signature is DungeonborneRuClient, which also runs on the server:
  NotifyOnNewObject(Pawn) (GetFullName on every pawn created, incl. mobs) and NotifyOnNewObject(ChildActorComponent)
  (GetFullName/GetOwner/.Role on a half-built component). The new one is probably deferred calls on stale UObject wrappers
  (weapon: ExecuteInGameThread with a captured `Weapon`). Done: `IsServerProc` (-dbserver) + `ClientNotify` — on the server
  no notifications are registered for Pawn, ChildActorComponent, B_WeaponBase_C, TgfProjectileBase (and with Pawn also the charge/soul/
  loot hooks, they are only for the listen host's hero); full HP — by polling heroes once per 1 s (`ServerHeroPoll` → RefillWhenReady);
  UseHitValidation for weapons is set by HeroFix in RuServer. Remaining on the server: notifications for GameModeBase/LyraGameState/
  LyraGameMode (once per map) and GA_TGF_Base_C (distance flags — the server needs them). Locally (05.11): the server recognised itself,
  four notifications not registered. Check in raids: no crashes, full HP, hits, charges, souls.
- [ ] **Sh4g 05.11 — done locally, being checked.** Self-exit from a raid (locally 09:51, server: exit=true, status 3
  reason 3): no results window and "Start" is dead. rpc_dump: the search CANCEL (event 4) went out at 09:51:18.010 while the client
  was leaving the raid map — the client did not apply it: the search heartbeat in the lobby too acknowledges `acknowlodge_event_id: 3`
  (it did take the room clear — the room heartbeat stopped). Result: `/ds/player_left` did not go out — the last
  player's results close the room, the server goes straight to resetting, and the leave check lived only in GameOverProbe (match phase);
  the deferred results were delivered after player_get_info (25.6), while the lobby opened at 24.7 — no results window. Done:
  (1) backend `matchmaking::match_cancel_resend` — a search heartbeat with an acknowledgement below the number of the sent CANCEL
  → repeat the same CANCEL (no more often than every 3 s, up to 20 times), test `match_cancel_resent_until_acknowledged`; (2) RuServer
  `GoProbe.LeftStep` — player_left in ResetStep too (OpenIds of the living from PlayerState; not read — skip the tick).
  Clarification: it was a portal exit (extraction), not a self-exit. User: results worked before the hall-exit changes.
  rpc_dump comparison: 03.11 15:36 (the window was there) — at results time dirty_role ×2 + **battle_settle_sync** + clear_matching
  + room 5–6, the lobby opened 3 s later; 05.11 — settle deferred (Sh4f check, 04.11 13:18) and arrived after
  the lobby opened. (3) Deferral is off by default (`hub::defer_enabled`, enable with RAID_DEFER_SETTLE=1): settle is again
  immediate, before CANCEL/room/clear. If Sh4f comes back (receiving stops after a raid) — look for another solution.
  10:12 (portal extraction, results immediately): no results window. Client probe: after returning — not a single network event
  (the player_get_info reply arrived at 12.218, no TGFGamePlayerGetInfoEvent, WBP_HallWindow_C did not open), CANCEL repeats
  in the lobby not applied (acknowledgement 3) — this is Sh4f, receiving stopped. Comparison: both 05.11 raids hung, where on the raid map
  search CANCEL and dirty_clear_battle_room arrived (both added 04.11); 03.11 15:36 (without them) — normal.
  Experiment (locally, run_local.ps1): RAID_DEFER_ROOM=1 — results (dirty_role, settle) immediately, room end (CANCEL,
  5–6, clear) accumulates and goes out after player_get_info; while accumulating, all hub pushes to the player are queued, the search heartbeat
  sends nothing; RAID_FLUSH_ON_LEFT=0 — player_left does not deliver what was deferred. 10:23 — did not help (no window).
  Client analysis (research/battleroom s2–s5.c, WBP_HallWindow.lua): the results window is `UMatchService`, not the room.
  battle_settle_sync → 0x1459f4630 (subscribed to the message type unconditionally at 0x1459f1da4) puts the results into +0x1a8 and the flag
  +0x1b0; the lobby in CheckAutoPopupSystems calls ShouldShowBattleResult 0x145a21c20: flag AND results AND NOT (an "empty" gs_result:
  SettleInterfaceType==1, OldExp==CurrExp, OldLevel==CurrLevel, !ValChanged, no GiveBackItems/ShowExchangeItems/
  MithrilForceResult). Search reset (0x1459f3a10) and room clear do not touch the flag; SetShowBattleResult is called only by
  the lobby. Our gs_result passes (XP grows, val_changed). RuClient probe [RESULTPROBE] (10:38, everything immediately): the results arrived
  at 37.399 on the raid map, ShouldShowBattleResult=false the whole time, the lobby opened at 44 (WBP_HallWindow on screen) —
  the client did NOT accept the results. The search CANCEL was not applied in the lobby either (repeats 45–55). Next check: GM `settle` (minimal),
  `settle gs`, `settle last` via the admin API in the lobby — will the client accept results outside a raid.
  10:46 GM `settle` (minimal) in the lobby — accepted (show=true). 10:58 (hall 15 s — DS_WAITING_TIMEOUT=15, client
  restarted at 10:56, raid 34 s): THE SAME pushes as at 10:38 — the flag was set at 33.377 on the raid map, search and
  room heartbeats stopped immediately (CANCEL/clear taken), the lobby showed the results window. Difference from 10:38 — hall ~140 s vs
  ~30 s (and a fresh client). Hypothesis: after a long stay in the hall/raid the client stops processing incoming
  pushes (matches Sh4f: short runs went through). Check: repeat with 15 s, then with 120 s.
  11:02 (hall 15 s) — again no: the hall hypothesis is dropped (user: it worked with 5 s too). In total 1 of 6 raids with
  results under identical traffic. Checked and ruled out: server_sequence numbers (consecutive, one channel), TGCP frame length
  (frames that are multiples of 16 occurred before working pushes too). A failed client does not take CANCEL repeats in the lobby either (11:02:55–11:03:02),
  a fresh client in the lobby takes a GM settle. Beacon: RAID_PUSH_BEACON_SECS=5 — dirty_role every 5 s while the player is in a room;
  ClientProbe writes RoleInfoNoti for each accepted push — where it breaks off shows the second receiving stops.
  **CAUSE (11:35):** in the lobby all beacons were accepted for 211 s — it is not time. At 11:20 the last accepted beacon was an S2C frame
  with TGCP number 0xFA, at 11:21:50 the number went 0xFF→0x100, the next beacon (0x105) was no longer accepted. The frame number in
  the TGCP header is u32 **BE at [9..13]** (client C2S: `00 00 00 ff` → `00 00 01 00`), while `tgcp::encode_tgcp_s2c` wrote
  LE at [12..16]: up to 255 the bytes match, from frame 256 on the client read number 0 and dropped pushes (replies to requests
  got through). Search/hall send ~2 heartbeats per second — 256 frames accumulate in ~1.5 min after search starts; the short
  raid at 10:58 made it before the threshold. This is Sh4f too. Fixed in `encode_tgcp_s2c`, test `s2c_seq_big_endian_at_9`.
  **Verified locally 11:38–11:42:** hall 2 min 15 s, raid, extraction — settle accepted on the raid map (show=true), results window in the lobby;
  search CANCEL and room end taken (search/room heartbeats stopped at 11:41:57); the S2C frame number
  reached 548, beacons were accepted to the end. Remaining: remove the debugging (beacon, RAID_DEFER_ROOM/FLUSH_ON_LEFT) and
  deploy the backend to prod (with permission).
- [ ] **B13d 05.11 — fixed in the mod, not verified. The server hung in reset after a raid, search "does not find a game".**
  Happened again at 13:23 (2 of 4 resets that day). Likely cause — in the mod: `Hosting` took the first live GameMode
  (`FindLive`); after a reset the old GameMode lives until GC, and if it kept coming up first (`ResetOldGM`), the new map
  was considered not loaded. Done (RuServer, put into the Steam install): iterate over all GameModes — take one that is not old and on
  the right map; on timeout, log the list of GameModes found, one StartLocalBattle retry, after the second failure —
  quit via the control file (on Robot the process will be restarted). The counter is in `Standby.HostingRetries` — the main block of
  RuServer already has 200 locals (Lua limit: a new local breaks mod loading — "too many local variables", 13:38).
  Originally: Locally 11:42:
  after a raid (extraction, room 857000002) reset: `StartLocalBattle(L_Dungeon_Matching_01?listen) ok=true`, but the map did not
  load — every 3 min the mod logs "hosting: map did not load in 180 s (GameMode map=)" and does nothing,
  heartbeat `phase=resetting` for 18 min. Meanwhile the backend gave the slot to a new room (11:58, "server ready, keys 1",
  action=host every 20 s) — the client hung in search until the player left. What helped: quit via the control file + launch
  (DbTools killmutex + start; without killmutex a second instance does not start — "Not Support Starting Multiple Game
  Instances"). Needed: (1) mod — after a hosting timeout retry StartLocalBattle, after the 2nd failure quit (on Robot
  start_headless will restart it); (2) backend — do not give a room to a slot in phase=resetting longer than N s / consider the slot
  dead and move the room (like `allocator::failover`). Why the map did not load was not found (the UE log has only errors:
  `TGFNetworkMessageHandler::SendRequest Send msg failed` once a minute — this happened before too).
- [ ] **B13c 05.11 — not done.** Happened again at 18:02 (DS crash in UE4SS mid-raid on Hard, probably the StatWatch hooks): after login the window "The match is still on", `room_create` → -4637 EnErrRoomUserInBattle (matchmaking holds Matched + DsReady). Workaround — restart the backend (rooms are in memory). The backend does not see a server crash mid-match: the process is restarted (heartbeat
  standby with a new epoch), while for 45 min (DS_ROOM_TTL) the room is listed as "match running", and players are sent to reconnect to an empty
  server (04.11 15:46–16:28, 9 players; ~15 requests/s from their clients). Needed: on a heartbeat ready/standby with an epoch different
  from that of a room in the battle phase — close the room (room_finish_push + clear) and free the slot. Not done.
- [ ] **B12 04.11 — backend and mod deployed 09:09–09:13 UTC, not verified in game.** Leaving the waiting hall counted as leaving the raid (`self_quit`,
  −10 items, deadmoney set, XP). In the original — no losses (user). Now results while the room's match has not
  started (`allocator::battle_started` = false) — `hall_leave` (`raid_native::raid_hall_leave`): items/XP/quests are not
  touched, the raid record is closed, events 5–6 + clear_matching, without `battle_settle_sync`, the player is removed from the room (an empty one
  is closed → `room_closed`). The DS side (removing PS/pawn and the record by OpenId, the hall counter, whether to send results) — agent.
- [ ] **Sh4e.** The host can still be spectated (after SetReplicates(false)) — with the agent.
- [ ] **B6. The death zone deals no damage** (user 03.11) — with the agent.
- [ ] **PVE1 06.11 — PVPVE diagnostics (RuServer: `[DIAG]`, `[ZONEDMG]`, `[MOBFALL]`; commands `diag`, `zonetest [R]`,
  `spawnrooms`; switch dbserver-pvediag.txt=0). Measurements on an empty server, Crypt Classic, 08:04:**
  (1) Mobs: 129 rooms, 123 by proximity (30 m threshold for all), 6 by phase start (they hold chests, altars, EscapeDevice Blue/Red).
  Proximity rooms hold mobs, and also barrels and chests: without a working proximity check the loot environment disappears too.
  Options cut only 9 sets (`need[101]`) — the theory "normal mobs disappeared because of options" is dropped for this map.
  A real spectator without a pawn breaks the native UPlayerDistanceCheckerComponent (native code not read).
  "Everything at once" option (`spawnrooms`, the native proximity event): 112 rooms in 151 ms, 372 mobs, fell >3 m within 20 s = 0,
  disappeared = 0; but 23 mobs in MovementMode=3 (falling) without a Z change — gravity is apparently disabled by ActorMap, near
  a player they may start falling: check. Cost: 60 -> 38–47 fps (min 33) with no players. Preferable: our own proximity
  check in the mod (the same 30 m, the same native event), load like the native one.
  (2) Zone: all discs have a GE set (GE_TGF_Damage_DeathZone_Hard_C, the last — _Last_C). Match 08:29: damage IS there
  (hp 1108 -> 438, accelerating, GE=1), B6 did not reproduce in this match — confirmed by the user. A bug was found there: the zone computes
  distance in the plane, the raised host pawn took its damage and died (08:29:58, LOCAL death=true, a false death in
  the kill feed). Done, not verified: the host gets scene tag Shadow=1 in EnsureHost (the damage loop skips it).
  `exits` (08:30) placed only GreyDevice=8, gave no red devices; commands `redportal`/`shadowportal` added,
  a DS restart is needed.
  (3) Portals: HasNextFloor=false, IsOptionArgsInit=false, OptionArgs=[], IsRoomReady=false — the native match initialization
  was not run. In BP_DeathZone: ShouldSpawnRedPortal = HasNextFloor || ForceSpawnRedPortal; EndCurrentLevel: a next
  floor and someone in the hall -> TravelToNextFloor -> OnMapFinish, otherwise GameOver (empty on listen). The second floor is in
  map_detail (LEVEL_TWO) and in the server task. Confirm live (`[DIAG] portals`).
  (4) 06.11 08:50 portals depend on the group size option: discs set PortalTypeSelections with LevelArgs (1010201 solo,
  1010202 duo, no option — trio). RuClient always set trio (1010203), so solo got few exits (4 blue in the match).
  Done, not verified: RuServer writes the task's option_args to ru-map-options.txt, RuClient on the server takes them for this map.
  (5) Command `spawnportal <blue|red|shadow|grey|bluedevice|reddevice|shadowdevice|greydevice|fixedblue|fixedred>`; logs [PORTAL].
  (6) Host mode `late spec destroy prox` (dbserver-observer.txt; rollback — `quit hide freeze park`), our own proximity check
  Spect.ProxTick; on an empty server the native component is disabled, no falls, not checked with a player.
  (7) PVE results window: in WBP_HallWindow.lua (by strings, not decompilation) ShowBattleResult opens only the level-up window
  (OldLevel≠CurrLevel), mithril rewards and hints; PVP has WBP_TGF_PVP_Settle. Match 08:41: level 20→20, the window has nowhere to come from.
  (8) 09:03 `spawnportal red`, entry: OnEnterPortal -> after 5 s IsTeleportToWaitingRoom=true -> Alive=0 -> EndCurrentLevel ->
  BroadCastGameOverEvent -> GameOver (empty), Playing=false, the player stays in the hall. Cause — HasNextFloor=false. The native
  "everyone in the hall" check exists and fired immediately; the 16 min on screen is apparently the time until the zone ends. Needed: HasNextFloor=true when
  Host.Scenes[3] exists, on TravelToNextFloor — servertravel to the floor (the native OnMapFinish does not travel on listen),
  carry the inventory over: currently on entry the data is taken from the backend, raid loot would be lost (DBattleUserData snapshot -> backend).
  **09:14 floor travel written, not checked with a player:** RuServer: HasNextFloor hook (true if Host.Scenes[3] exists and the map =
  Scenes[2]); on TravelToNextFloor — Spect.FloorTravel: dbnative `gofloor` (native DumpUserDataToCache with reason=NULL, as on
  a portal exit, does not change the status) -> POST /ds/floor_snapshot -> servertravel; command `nextfloor`, lines [FLOOR].
  Backend: deploy_preview snapshot by (open_id, room), battle_user_data returns it on the new floor. RuClient: map rules for the second
  floor (MapId from ru-map-options.txt, line map=). Option 101 = only BP_Tgf_Monster_Virus_C ×8, unrelated to portals.
  **09:19 verified in a match: red portal -> L_Dungeon_Official_02 in 15 s** (the HasNextFloor override worked, snapshot 2448 bytes,
  record status unchanged, options and MapId correct). Bug: there are no portals on the second floor: all 7 zone discs have 0 portals and GE=[]
  (no zone damage, death only on the last disc, MaxZoneTime=60); the map references only BP_ExtractDevice_Short, plus
  Reaper. Carrying inventory and equipment to the floor confirmed by the user. User decision: portals and zone damage on the second
  floor as on a normal map. Done, not verified in a match: Spect.FloorCapture remembers the GEs and portal settings of the discs on the first floor
  (type by map options), Spect.FloorApply/FloorRemove apply the GEs in the native zone hooks, Spect.FloorSpawn places portals on a
  disc change (PickRandomPlacementsWithinRange, otherwise a spawn point inside the radius); command `floorspawn <disc>`.
  User clarification: on the second floor there are no grey portals, only blue and purple — grey are excluded from the floor rules.
  10:18 first run: the rules were overwritten by `diag`, the portal class did not load — fixed.
  10:49 second run: `floorspawn 3` and `5` placed 4 and 2 BP_ExtractDevice (location: spawn point; the native placement choice did not work —
  the ring is computed, cause not found). 10:50 `zonetest`: the DS hung (the game thread spins, heartbeat and logs stopped, the control file
  is not read — Lua is busy) right after the ApplyCurrentDamageGEs hook. The cause is most likely LoadAsset or applying a GE inside a BP hook.
  Fixed: the hook only queues, loading and GE — in Spect.FloorTick.
  **11:01–11:15 third run, without floorspawn and zonetest — verified in a match:** no hangs; portals appear by themselves on a disc change: disc 3 (11:08:35,
  7:17 after the floor start) 4 BP_ExtractDevice, disc 5 (11:13:36) 2 more, no grey ones; no purple rolled (25% chance), its look and entry not checked.
  Zone damage applies and is removed on return: about 10–12 HP/s on discs 2–5 (on the first floor on the same discs 7–9, a different measurement; spikes of
  65–145 HP/s — probably mob hits, not the zone). `GE_TGF_Damage_DeathZone_Last` on the floor's last disc not checked. Portal entry 11:14:47,
  extraction result (status 3, reason 3, XP +160, 2 items sold for 134 gold), the room closed, the slot reset started.
  Open: on the first floor the red portal does not appear by itself, the floor is reachable only by command for now (`nextfloor`, `spawnportal red`);
  portal placement is the fallback (spawn point in the ring), the native PickRandomPlacementsWithinRange did not work.
  Auto-closing of the room after a DS restart (stale_battle) verified 10:55: b40003017_934805008 closed, the player got "match over".
  **Written 11:29, not deployed and not verified in game:** (a) ascension potion: the PlayerAddExp hook accumulates XP by OpenId, bonus_exp in the results URL,
  raid_finish_exp adds it on exit and on death (test native_result_applies_once passed; release not built); (b) red portal (user decision): a blue device of disc 5 on the first floor
  turns red with a 50% chance, plus one guaranteed red slot on this disc via the native ConvertPortalType (solo -> RedOne, duo and trio -> Red), Spect.RedTick, command `redroll`, lines [RED];
  at least one red per match if the map has disc 5, device capacity by group not checked; (c) second-floor portal placement — native points
  PotentialPortalPlacements (Spect.NativePlace, docs/ghidra_portals), if absent the previous spawn point. Check: `spawnportal bluedevice` and `redroll`,
  entry and floor travel; the natural roll and the guaranteed red on disc 5; the line "native point" in [FLOOR]; [EXP] and XP in the results after the potion.
  **Rule (user, 06.11):** purple never becomes red. Already so: only blue ones roll (Spect.RedTargets), there are no purple ones there.
  **12:30–12:34 verified in a match:** `redroll` replaced a blue one with BP_ExtractDevice_RedOne (the replaced blue one is still listed at (0,0,0), removal not checked); native points on the second
  floor: `floorspawn 3` placed 4 devices on them (whether they stood on the floor is not confirmed). Entering the red one after replacement, the roll on disc 5, the guaranteed red and the potion not checked.
  **Purple leads to the wrong place (user: it should lead to a closed arena with a boss):** currently entry -> Shadow scene -> at 0 alive -> next floor.
  In the game files: scene type EN_DS_BATTLE_SCENES_TYPE_BATTLE_ROOM_LEVEL_TWO_PURPLE_DOOR = 4, map /TgfMaps/Scenes/Common/Level/L_Common_Throne_01 (BP_Throne_ShadowKing, exit
  BP_ExtractDevice_Short, found no player starts in the map), boss BP_TGF_Monster_Shadow_King and variant _H, loot 212 and 265. In map_detail scene 4 is not set for any map.
  **13:00 verified:** the arena is part of L_Dungeon_Official_02: throne (120000, 0, -80), the boss appears via native spawn when a player approaches; 3 exits BP_ExtractDevice_Short (121061 0 -65, 119250 ±750 -65).
  The separate L_Common_Throne_01 has no mode and no boss. Currently entering purple on the floor: the player is taken under the map (Z ≈ -50000, falling), 0 alive on the floor -> Phase_TgfGame_GameOver, the boss then stands still;
  in the normal phase the boss moves. On the arena the zone hits (the player is far outside the circle). To do: on entering purple the mod moves the player to the throne and sets the Shadow scene, preventing the move to the hall and the end of the match.
  **Rule (user, 06.11):** on the second floor there is no purple at all, only blue; purple only on the first floor and it leads to the throne on the second.
  **13:53 verified in a match, user: "everything as in native".** Cause of the earlier failures: RuClient reset the player scene SetPlayerSceneTag(0) on every pass, now once per PlayerState.
  Native: RedDoor and Shadow go through TgfPlayerGameFlow.ServerTeleport into the hall under the map, Shadow sets the scene, on the next floor such players are at the throne (no native scene_tag handoff on listen).
  Done in RuServer (Spect.ArenaTick, Spect.ToArena): entering purple on the first floor is remembered, on the second floor the player is moved to the throne in the Shadow scene. Red and purple are compatible:
  it is one map.
  Ascension potion: the GA calls PlayerState.PlayerAddExp, the value does not reach the backend (the DBattleUserData record has no add_exp), XP is only
  time-based. To do after portals: a PlayerAddExp hook + adding it in the raid results.
  First floor: `[EXITS]` on an empty map — 11 BP_EscapeDevice_Blue_01 and 6 BP_EscapeDevice_Red_01 (BP_EscapeDeviceSpawnPoint in rooms
  by phase), 0 live devices: check in a match whether they spawn, this is the main candidate for "no red portals".
  10:09: the native SpawnImmediately on all 17 points (`escapedevices`) and the cheat SpawnAllEscapeDevices (`exits`) gave no devices; placed=0,
  the rooms ran (WarmUp phase). Cause not found; SpawnedActors of this point may not get filled, but there are no live actors either.
  09:57:37 client F: crashed (AV 0x0, ucrtbase -> UE4SS, fight with crossbowmen), the hero stayed on the server and died (35 items lost).
  Cause not proven; suspicion on the [PROJ] diagnostics (reads the arrow after 0.2 s) — disabled, file ru-projdiag.txt=1 enables it.
  (9) With the host pawn removed our proximity check works (09:03: 23 rooms started, 100 characters). Open: `heroes=2`
  in the log after DestroyCharacter.
  Important for mods: `GetFName()` on a null object in UE4SS = a native crash (AV 0x18), pcall does not catch it — the DS crashed
  twice at 07:58 and 08:01, fixed in Spect.ObjName.
- [ ] **B7. After death the client re-equips lost items** (prod 03.11 17:14, …732): the "death" result removed 15 items and gave a deadmoney set, 8 s later `role_wear` with the guids of the old items (100030230, 100030224) — "item not owned by the player". Apparently the client's role inventory cache is not reset after battle_settle_sync (compare with a reference: which dirty/cache_expired PVE sends after death).
- [ ] **B8. The client reconnects to a match the backend does not know after a restart** (prod 03.11 17:31, …741: match b40003015_725415033 was running during the backend restart). Every time battle_user_data → 403 no_room → kick (KickRemote, closeconn ok), the client comes back again. Needed: on battle_room_heartbeat / user_battle_room with an unknown battle_id reply "match over" (room_finish_push) so the client leaves the old match. For now — restart the game.
- [ ] **B9. A slot without a heartbeat was given to a room** (prod 03.11 17:30, after a backend restart): a room without a hall task held the slot. Fixed in allocator::free_for (with DS_WAITING_ROOM — only after the first heartbeat), built, not deployed.
- [ ] **Sh3. Battle pass 16518:** in the tables, per-level rewards and `exp_item_type` exist only for the old pass 16515;
  our pass XP is always 0 (TODO in shop_handlers) — the level does not grow. Investigate.

## Friends
- [ ] **F1 03.11 — done, not verified.** Friends' online status was refreshed only at login: the status comes from the profile cache
  (EN_ROT_PLAYER, login_data_cache); the client subscribes to it at login and never re-requests it (the friends tab has no
  RPC). Now, when a player logs in/out, their online friends get SCCacheExpiredSync {EN_ROT_PLAYER, user_id}
  (`social_handlers::presence_changed`; logout — from the gateway when the connection closes).
- `friend_get_all`/`friend_get_suggest` — empty responses, same as the reference. There were no two players to test with.

## Rooms / Trade / Guilds
- [ ] **R1 05.11 — custom games (private rooms up to 3×3 on arenas), done locally, partially verified.**
  **NOT VERIFIED IN GAME (as of 05.11 21:00):**
  - room list refresh (room::suggest modulo page count) — build only, the button was not pressed;
  - arena MapId rollback (20:4x) — that the mode is again as at 19:30/20:03 (no zone, no "modulo 0", players are not kicked);
  - "First to 4 wins" (PVP0X_MultiGameMode) — never played to the end without StatWatch on the DS; the first run crashed the DS (R1-bo);
  - 60 s connection timeout in RuClient (LOCAL, ConnTimeout, ru-connect-timeout.txt);
    **06.11 user: the connection timeout does not work.** Conditions and logs not analysed yet;
    **06.11 prod, player B:** the client loses the connection to the DS (UDP without WARP, `Connection TIMED OUT` every ~65 s), the room stays `battle`, no result; in the menu the client looks like it is in a long search,
    a new search returns to the same room (`returns to the battle`, logged in Russian). It does not let go until the player returns. Not fixed;
  - R1-stuck backend: closing the room when the DS restarts (stale_battle) and removing those who did not arrive within DS_ARRIVE_SECS=75;
  - PVP result window (battle_settle_sync) — not done;
  - "Leave room" during a match (everyone leaves) — not re-checked after phase=empty;
  - arena chest loot — R1-loot (below): without Dead Money the chests have no equipment.
  Recorded with two clients (rpc_dump 18:23–19:41). Room window: `map_id` = PVPMapConfigDataTable.MapId (1 "Sanctuary
  Ruins", 2 "Hall of the Departed", 3 "Ruined Fortress"), `battle_type` = PVPModeConfigDataTable.ModeId (1 "Team
  Deathmatch", 2 "First to 4 wins"); tables extracted with retoc. Done: `room::pvp_arena` -> map_detail 30000+100·map+mode
  (arenas `L_Common_PVP0X_DeathMatch` / `_MultiGameMode`, one scene without a hall), level 50000003 / 50000006;
  `BattleRoom.arena_map_id` -> server task per arena; teams by seat (0–2 / 3–5) — `team::set_room_battle_team`
  on top of the party for the duration of the match (removed by `room::tick`); match result "pr…" without losses (raid_native — the hall_leave path);
  RuServer: task with one scene (was "no two scenes — skipping"), arena loaded — match starts right away, the mode's GameOver
  on the arena -> phase=empty (backend broadcasts "match over", closes the room); arena loot — the game's base, not Dead Money
  (`[keep_base]` in seasons/dead-money/loot/loot.toml, xtask loot). Verified 19:30: start, arena, teams, match runs.
  LOCAL's analysis (tools/research/room/PVP_START_END.md): the start window WBP_PVP_MatchWindow is opened only by the room action
  `create_battle_action = true` — we now send it before start_battle_action; the 5 s auto-exit does not work on listen (the native
  GameOver() is dedicated-only) — LOCAL did it in RuClient (PvpLeave block, deployed to F: and E:). Open:
  a room whose server did not start the match hangs "in match" until the timeout (needs a return to preparation); there is no PVP result window;
  "Leave room" while the room is in a match — everyone leaves (because of the unclosed match; check after phase=empty).
  20:10 — two fixes after the user's check: (1) the room list refresh was empty — the client sends a random number
  in `room_suggest.index` (17542, 12718 …), and `room::suggest` paged at index·PAGE; now page = index mod the number of
  pages. (2) arena chests still had Dead Money loot: `[keep_base]` puts rows under 30101…30302, but
  nobody set GameState.MapId on the arena (RuClient `MapRulesByLevelName` knew only dungeons) -> map 0 loot = seasonal.
  Now RuServer sets MapId on all GameStates at arena start (log `[HOST] arena (...): ... MapId`, in Russian), RuClient does too
  (`MapOnly`, without options and MatchType). Awaiting verification.
  - [ ] **R1-loot (20:25, NOT FIXED): arena chests still have Dead Money loot.** MapId is now set (DS log:
    `B_TGF_MatchGameState_C MapId 0 -> 30101`, `-> 30302`), the `[keep_base]` rows under 30101…30302 are in the pak — the loot
    is still seasonal. So the chest does not pick its table by GameState.MapId (or reads it earlier, before we set it
    on "arena without hall — match"; or the key is the drop source without a map: LOOTCHECK `919020->[900004]` — the seasonal pool on
    the source). Needed: find what the chest's loot component selects the row by (map_detail.loot_map_id from the DS task /
    InitGameMap / its own key), and either set the map earlier, or have the season not touch the sources used on arenas.
    **20:4x MapId setting ROLLED BACK** (RuServer "arena without hall" + arenas in RuClient MapRulesByLevelName): with MapId=30x0y
    the arena mode broke — the server enabled the shrinking zone (`[ZONE] Dynamic Radius=4300 Playing=true` in "First to 4 wins";
    in the working matches 19:30/20:03 with MapId=0 there was no zone), BP logs "Attempted modulo 0", players left after 15–30 s.
    MapId must not be set on an arena — arena loot has to be solved differently (chest key / the season does not touch arena sources).
    **20:5x — chests follow the map tables (by GameState.MapId):** the 20:38 match (Dead Money already off, but the arena MapId
    still 30101 — the rollback was at 20:43) gave chests without equipment: the base loot_ue_source_choose_group has rows
    only for map 0 (default, ~1550) and 10101…10203, none for 30101…30302; `[keep_base]` put them only into
    the season. Now the arena MapId = 0 -> map 0 rows, which do contain equipment (`cargo xtask loot show --season=none --map=0`:
    BP_ChestMidrangeLarge ≈0.63 armour, 0.48 jewellery, 0.44 weapons). Check one arena match after the rollback.
    **21:1x — PVP chests by equipment type (user's hint).** Arenas have their own chests
    BP_TreasureChest_PVP_{Cloth,Leather,Plate,Ring,Staff,Sword,CrossBow}_C (loot_source_map 165–171). The game's original
    loot_ue_source_choose_group has 11 rows, these are not there; our template loot/loot.toml (xtask) put them into the common group
    g073 (mimics/iron chests: gold, all equipment mixed, treasures). Now each has its own pvp_* group
    (map 0): gold 100 %, equipment of its type 100 % + 75 % (pools 900021/900020 filtered by
    UeSource_Equipment.equip_subtype, weights unchanged), potions 60 %. Ring = rings+amulets, Staff = staves+relics,
    Sword = swords, longswords, daggers, maces, shields, CrossBow = heavy crossbows. `loot build` + `balance build`,
    pak to all three installs. Copy of the template before the change — scratchpad loot_before_pvp_chests.toml. Not verified in game.
    **21:3x at the user's request:** equipment only (no gold or potions), purple only (6th digit of the id = 4:
    0 rusty, 1 plain, 2 green, 3 blue, 4 purple, 5 legendary, 6 unique), maces moved from Sword to Staff,
    all rolls 100 %. Pools pvp_*_epic — all items of the subtype with quality 4 (that have item_type and gear_template):
    cloth 10, leather 10, plate 10, rings+amulets 12, staff/relic/mace 6, sword/longsword/dagger/shield 8, crossbow 2.
    Rolls per chest — 6 (temporarily): "all cells are full" — LOCAL is analysing chest capacity and item sizes.
    **21:4x:** the user saw both purple and pink — the id digit is not the rarity: armour …4001 is
    EN_ITEM_RARITY_EXCEPTIONAL (pink, tier 5). Colour = UeSource_ItemType.rarity (ROTTED, NORMAL, UNCOMMON,
    ENHANCED, EPIC, EXCEPTIONAL, LEGENDARY, UNIQUE). Pools are now by rarity = EPIC: cloth 5, leather 5, plate 5,
    rings+amulets 25, staff/relic/mace 10, sword/longsword/dagger/shield 13, crossbow 3. Built, not rolled out —
    waiting for LOCAL's answer on capacity (roll out together).
    - [x] **21:5x done (not verified in game):** the PVP chest grid is 9×10 = 90 cells (LOCAL,
      tools/research/chest/PVP_CHEST_CAPACITY.md; sizes — UeSource_ItemType.position_cfg). A pool per kind, slots
      100 % in an order where "first free spot" placement fills all 90 cells (simulated traversal by
      rows and by columns, scratchpad chest/): cloth/leather/plate 19 (chest 4, legs 3, helmet 4, gloves 4, boots 4),
      rings 90 (45 rings + 45 amulets alternating), staves 22 (staff 4, relic 4, mace 14), swords 21 (longsword 3,
      shield 6, sword 6, dagger 6), crossbows 15. Only ids 18xxxxxxx (2201502xx "legacy" EPICs removed). Risk: the game may
      place loot not in slot order or traverse the grid differently — then a couple of cells stay empty (extras are not placed).
    - [x] **22:0x — generator `tools/py/pvp_chests.py`** (the PVP chest section in loot/loot.toml is now generated by it;
      then `cargo xtask loot build` + `balance build`). Checks the 90-cell layout by simulation. Weapons (crossbow,
      mace, staff, relic, shield, dagger, sword, longsword) — variants alternate A, B, A, B… (staves have 3 EPIC
      variants, the relic 1, the rest 2). Pak deployed 22:0x, DS not restarted yet (a match was running) — it will pick it up on restart.
    - [x] **AFTER LOCAL'S ANSWER — even distribution (user's request 21:4x):** a chest must not be packed with
      one kind (e.g. only boots). Currently N rolls from one pool — repeats are possible. To do: a pool per
      subtype (armour: helmet, chest, gloves, legs, boots; rings / amulets; staff / relic / mace; sword /
      longsword / dagger / shield; crossbow) and slots round-robin over subtypes (one of each, then again), number of slots
      = chest capacity from LOCAL accounting for item sizes.
    Conclusion for the season: Dead Money rewrites the map 0 rows — arenas (MapId 0) get seasonal loot; for arenas
    to keep the base, we need either a dedicated arena MapId + rows for it without breaking the mode, or the season not touching map 0.
    Current workaround — the season is off (`cargo xtask season use none`, deadmoney.toml enabled=false, 05.11 20:28;
    base zzzzzz_Balance_P.pak to all three installs). To restore: `season use dead-money`, `balance build --deploy`, enabled=true.
  - [ ] **R1-stuck (20:47): room "in match" after a DS restart — room_create -4637, entering shows "match in progress — reconnecting".**
    The DS was restarted (quit) in the middle of match pr50000003_888917001; the new process sends heartbeat ready with a different epoch, while the slot
    on the backend is still "busy, room_phase battle" — nobody closed the room. Manually: POST /ds/slot {phase:"empty",
    battle_id, epoch} (like an arena GameOver) -> members_absent -> "match over" + dirty_clear_battle_room, slot reset.
    Needed in the backend: heartbeat ready/standby with epoch ≠ the match start epoch while room_phase=battle -> close the room the same way.
    Plus from LOCAL (connection timeout in RuClient, ConnTimeout, 60 s, ru-connect-timeout.txt): if a player has not appeared
    on the DS within ~60 s after start_battling — stop sending them room syncs, dirty_clear_battle_room + clear_matching, remove
    them from the room (otherwise the next SCBattleRoomLogSync returns battle_id and the client goes to reconnect again).
    **Done 20:58 (not verified in game):** (1) allocator: `Slot.room_epoch` — the epoch at which the mod reported the room phase;
    heartbeat ready/standby with a different epoch -> `stale_battle`, agent_link closes the room (members_absent all + "match
    over" + dirty_clear), slot reset; test `restarted_server_reports_stale_battle`. (2) matchmaking: FINISHED time
    after startBattling (`battling_at`) and server arrival (`mark_arrived` in /ds/battle_user_data); not arrived within
    DS_ARRIVE_SECS (default 75, 0 — off) — `take_not_arrived` -> members_absent + room_finish_push (lobby tick).
  - [ ] **R1-floor (21:5x, user): falling through the floor on arenas — fix it the same way as on the main maps.**
    On raid maps this is cured by HeroFix (G1: RuServer `HeroFix.Pawn` — DisableActorMap, ActorMapComponent off,
    GravityScale 1.28, MovementMode walking; RuClient — the same, ru-herofix.txt) plus the observer host "park/hide/freeze"
    (RuServer ~612: otherwise it falls through the floor). Check whether HeroFix fires on L_Common_PVP0X_* (on the arena
    `[HEROFIX]` is in the DS log — so the cause may be different: World Partition cell collision around players,
    PVP start points below the floor, KillZ) — first the DS/client log at the moment of the fall and where it happened (start point / match).
  - [ ] **R1-bo (20:27): "First to 4 wins" (pr50000006, L_Common_PVP03_MultiGameMode, 30302) — the DS crashed** ~30 s after
    Playing=true: EXCEPTION_ACCESS_VIOLATION 0xa, the whole stack in UE4SS.dll (Lua callback). The server had the
    StatWatch hooks on (CalcHitValue, ApplyGE …) — the second such crash in a match. StatWatch on the DS is off (mods.txt = 0), re-check
    the mode; if it crashes again — look at the RuServer hooks (GameOver/PlayerDead on round change).
- Trade: search and recommendations respond; listing for sale — C1 (`-4212`).
- Rooms, guilds — not tested.

## Network for players from Russia and load (06.11)

Prod: domain `dbserver.example.org`, relay on `8443` (for `player B`, port `7443` and TLS without SNI froze), launcher 1.3.2.

- [x] **Login and lobby without VPN:** domain in `public_url`/`public_address`, relay `8443`. Old launchers only need the manifest.
- [x] **Match without VPN (UDP tunnel):** direct UDP to Robot is cut after ~11 packets on any port. Tunnel through the relay only for affected players (`core/backend/src/auth/udp_tunnel.rs`, launcher `127.0.0.1:27777`, GM `udp on|off`, `/to <open_id> udp on`). Verified in a match: `player B` without WARP.
- [ ] **Automatic tunnel enabling** on `network_block_loading` is written, but has never fired in a real match.
- [ ] **Load for 100 tunnels.** VM: 2 cores, 3.8 GB. Measured 06.11 over 30 s with one tunnel: dbserver 10% of a core (together with the lobby and the log), network ≈22 KB/s per player each way. For 100: network ≈2.2 MB/s per leg and file handles suffice; CPU estimate 50–100% of a core, not measured, needs a load test. If it grows — a 4-core VM and less logging.
- [ ] **Server slots:** pool of 4 servers × 20 = 80 players; this hits the limit before the VM does.
- [ ] Leftovers on the VM after moving to `8443` — with the owner (`docs/owner/DEPLOY_LOG.md`).

## Playtest 06.11: backend flags

- `PLAYTEST=1`: forge materials once at login (`PLAYTEST_MATERIALS`).
- `TRADE_SYSTEM_BUYOUT=1`: system sellers on Buyout Trades, unidentified items (WORKAROUNDS K22).
- `TRADE_SYSTEM_AUCTION=1`: system lots on the auction with buyout. The user liked it; enabled by a flag (K22).
- `tools/py/auction_all.py`: all equipment with rarity into `trade_commodity` (82 items without data skipped), then `cargo xtask balance build`.

## Instance lobby in the launcher 06.11 (LAUNCHER_SPEC §12, flag LAUNCHER_LOBBY)

- Implemented: `core/backend/src/lobby/launcher_lobby.rs`, `join_room`/`open_lobby_room` in the matchmaker, API `lobby/*`, launcher 1.4.0 screen, GM `lobby`.
- **Prod bug, found by a run:** `take_not_arrived` kicked a player who stayed in the hall longer than `DS_ARRIVE_SECS` (75 s): the server requests match data
  only on the match map, and the hall on prod is 120 s. Now the countdown does not run in the hall and starts over after it; non-arrivals are reported by the mod (`absent`).
  Did not show up locally: the hall there is 15 s. Needed on prod regardless of the lobby.
- `allocator::hall_open`: the `hosting` phase was treated as a closed hall although the hall is still loading. An open hall now accepts players while loading too.
- Matchmaker tests run one at a time (`serial()`): the shared `tick` grabbed other tests' tickets, causing rare false failures.
- Launcher 1.4.1: a dead session (DB reset, ban) shows the login screen immediately; before, the lobby card simply disappeared until "Play" was pressed. Not verified on a real dead session.

## Playtest 06.11 results (logs 19:39–07:30 local time)

Done 07.11:
- **Returning to a match:** a player returning after `DS_ARRIVE_SECS` was counted as not arrived and got "match over" (7 times; the `player D` case analysed). Now `arrived` is kept.
- **Maintenance mode:** `core/backend/src/lobby/maintenance.rs`, `deploy/vm/maintenance.sh`, banner in the launcher. On prod since 07.11 08:15 (launcher 1.4.3); the script was tested on the VM with an empty server; not yet applied with matches running.

Open:
Order: DS crashes first — only they cut people's matches short.
- **DS crashes in `UE4SS.dll`** (cause and plan: `docs/CRASH_LONG_PLAY.md`, fix not done)**:** 3 of 136 runs, `EXCEPTION_ACCESS_VIOLATION` (addresses 0xe, 0x24, 0x0). `CrashTracer` on Robot is off. Logs `Kit\logs\ue-7777-20261106-082343.log`, `ue-7807-20261106-112230.log`, `ue-7797_2-backup-2026.11.06-21.13.18.log`. The backend notices a crash after about 2 minutes; what happened to players' items is not checked.
- **"Start" refusal text:** the player sees the native "this event is not available yet", 287 refusals. Whether `modal_message` is shown was not checked by eye.
- **Empty halls:** 46 of 107 rooms without players held an instance for about 5 minutes (3.7 hours in total); all 4 instances busy for 136 minutes; because of this private rooms got 121 `-4628` refusals.
- **A player who left the hall** cannot come back until the next round (37 refusals).
- **DB growth:** `surreal_saves` 4.8 GB in 12 hours; server `pid 11308` on Robot hung at startup and is still hanging.
- **Antivirus hits on the launcher (checked 07.11):** VirusTotal for 1.4.1 (`ae731b94…`, not uploaded by us): 1 of 68, only `Trapmine: Malicious.high.ml.score`; Microsoft, Google, Kaspersky and ESET clean. In the CAPE and Zenbox sandboxes the only network traffic is `150.171.28.11:443`, an address that looks like Microsoft (WebView2). The exe structure of 1.3.2, 1.4.1 and 1.4.3 is the same: same sections, entry point in `.text`, no overlay. The file on the server matches the build. Defender found no threats in the project folders and game installs, but exclusions cannot be read without admin rights and there was no full scan (`D:` is off limits). Conclusion: no signs of infection, probably a false positive, not proven. Needed: code signing and a submission to Trapmine.

## Security audit 07.11 and move to a new PC

Moved on 17.11 to the owner document `docs/owner/SECURITY.md` (not in the team copy).

### Changes 07.11: gear, search checks, login (built and tested, not deployed to prod, not verified in game)

- Loot auto-sale moved into Dead Money: without the mode, extracted items stay with the player. Playtest: gear from enemies was sold on exit (`[RaidSale]`); the icon has nothing to do with it.
- Return to a teammate: `raid_team_returns`, `raid_send_returns`, flag `RAID_RETURN_TEAM_GEAR` (default 1). Match record in `state.raid_gear`, last 5 matches.
- Search checks: `match_checks.toml`. Rarity limit, minimum gear score and entry cost are off (`gear_rarity`, `gear_score`, `level_cost`: `enabled = false`); on prod put the file in `/opt/dungeonborne`. The table is built by `cargo xtask balance build`; players need the new `ru.balance` mod.
- Whitelist: the role was removed back on 04.11 (`login_roles = []`, last startup line with the role — 04.11 13:18). `require_guild = true` is active, the user's decision 07.11: keep it. The `not_in_guild` refusal was not logged; logging added. This log will show which refusal players saw.
- To check in game: entering a map with a limit via the "Start" button, extracting enemy gear, the mail to the teammate and the return window, the off-hand, the icon. The auto-client (`native`) bypasses the button checks; live clients are needed.

### Portal message in the kill feed (07.11, not deployed to prod)

Native chain: `BP_DeathZone` "Broadcast SpawnPortal Message" → `BP_TgfGame_MapHint` → `WBP_TgfGame_DisplayMessage_Portal` on clients. Added `Spect.PortalWatch` to RuServer: new `BP_ExtractDevice_*` are announced after 2 s if the game did not announce them itself (hook); devices already on the map at the first pass are not announced. Test command `portalmsg [Blue|Red|Shadow|Grey]`.
- Verified on the stand: the command produces lines on the client (window screenshot), `spawnportal reddevice` and `shadowdevice` produce `[PORTALMSG]` on the server.
- Not verified: blue-to-red conversion (`redroll`, the test run has no second floor), guaranteed red, second floor, no duplicate when the native message fires.
- Translation defect: `Game_RedPortalAppear` in the Russian Game.locres contains `{Number} {Number} {Number}` and shows "1 1 1". The text is the same in the game's pak; not edited.
- On conversion the game may already have announced the blue portal of that device; the red line will come in addition.

### Flag icon on items (analysis by the LOCAL session, `docs/ITEM_FLAG.md`, not fixed)

The backend sent `faction_id = 1` to everyone, and the icon is shown when `MyTeamID == Item.FactionId`, so team 1 saw it on other players' gear, while teammates from team 2+ did not. Static analysis, not checked against prod.
- Done (not deployed): a team number 1…253 is pinned to the player on entering a room (`assign_team_number`), goes into `team_id`, `faction_id` and `/ds/team_of`; the RuClient mod puts it into `MyTeamID`. Test command `itemteams`.
- Verified on the stand with one client and a temporarily set number 2: `MyTeamID=2`, 10 items have `FactionId=2`.
- Not verified: the icon itself on a teammate's gear (needs two clients; the `E:` client hung before login), custom games (`PVPGameWinTeam` now gets different numbers).
- Deploy: backend and `DungeonborneRuClient` on Robot. An old backend with the new mod and vice versa are incompatible only in the icon number; the match does not break.

### Returning gear to a teammate (analysis 07.11)

Game rule (`Guide_Description_3_6`): a player extracted with a teammate's gear, alive or dead, and it reaches the teammate by mail. Gear found in a team raid is marked by all team members, and in later raids the return between them does not trigger.
- The protocol and the client have everything: `DItemCreateInfo.ownership`, `UTgfInventoryItemInstance.Ownership`, owner list in the tooltip (`GetOwners`), `DGsUserSettleResult.give_back_items` and the `WBP_Teammate_ItemReturnWindow` window, mails `mail_501035` (`TEAMMATE_NAME`) and `mail_501036` (off-hand when the main weapon is returned), channel `EN_MAIL_CHANNEL_BATTLE_BRING_IN_RECOVER`.
- The teammate's `raid_entry` is not usable for the check: they may start a new raid before the extracting player exits, so the gear record is kept for the whole match.
- Not verified: whether the DS keeps `create_info` from entry to the result, whether the client draws the icon from `Ownership`.

### The "Change your inventory arrangement" message (analysis 07.11; per the user, it was blocked by the rarity check)

Text of `SelectMap_EquipmentLevel_Tips`: "Not enough gear score" (in the Russian client). The English original asks to check your gear; the Russian translation is wrong. Shown in `WBP_MatchMainButton.CheckEquipScoreIsOk` when `battle_affix_score_require > 0` and the score is too low. The client's `competition_level.bytes` has no such field in any of its 59 rows, and no mod overrides it, so by the files the window should not appear. A screenshot and the map are needed.
Another candidate: `MatchEnter_RarityNotMatch`, rarity limit `battle_gear_rarity_require` on levels 40003015–40003020. Where the client shows it was not found.

## Playtest 07.11: inventory stops working after 30–40 minutes
- [ ] **Inventory fails after 30–40 minutes of play.** The user's decision: it is a thread race in client mods (Lua on a UE4SS worker thread). Server and network checked and fine: gateway 6601 sessions live up to 2.7 h, inventory operations go through, and the 443 "disconnects" are abrupt closes of connections that had already finished. Fix — stage 1 (`docs/MODS_REFACTOR_PLAN.md`), deployed 07.11. Close if the complaints stop; if not, collect the player's `Saved\Logs\Dungeonborne.log` and `crash_ue4ss.log` with the time of the failure. Analysis: `docs/CRASH_LONG_PLAY.md`, last section.

## Prod deploy records

Moved on 17.11 to the owner document `docs/owner/DEPLOY_LOG.md` (not in the team copy). What is deployed can also be seen from the
"on prod since …" notes in the sections above.

## Playtest season: stats, resistance cap, damage reduction stacks (07.11)
- [x] Stats (the user's decision): level 20, all classes — base HP +300, 50% resistance to none/fire/ice/thunder/dark (light unchanged): `seasons/playtest/balance/role_level_upgrade.json` (00f3b28). Stand 07.11: hero in a match `MaxHPBase=756` (456+300), resistances 0.5 + gear (None 0.691). **Base stats are sent into the match by the backend** (player data): with the old pak on the server it is the same — for prod the tables on the VM and a backend restart are enough, the Robot pak is not needed; clients need the pak for display — via the launcher.
- [x] 80% resistance cap, hardcoded: RuServer `HeroFix.ResistCap`, once a second on the game thread, all 6 elements. Stand: `resist 0.95` → after ~1 s `[RESISTCAP] … 0.950 -> 0.80`. Finding: gear adds/subtracts its bonus to the value (not a recompute from zero) — so the cap remembers what it cut and gives it back when the value drops. The fixed version (accounting for the cut amount) is not verified in game yet. Host test command `resist <fraction>`; the hero probe logs resistances and `DamageReduceRate`.
- [ ] **For the future — damage reduction stacks** (the user's rule): each hit on a hero — a stack of −5% damage taken, up to 10 stacks (−50%), effect 2 s, each hit refreshes the timer, 2 s without hits — all stacks are removed.
  - Found 07.11: `TgfAttributeComponent.ClientDamageDirectionEvent` on the server **does not fire** either from mobs or from players (Robot logs: 191 deaths, 113 from players, not a single hit entry; stand — goblin hits not visible). Binding to it does not work; the stacks code was removed.
  - Path 1 — server mod by HP: every 0.1 s look at `CurHealth` of player heroes, a drop = a hit → a stack; add to `DamageReduceRate` (BaseValue and CurrentValue by the same delta, remove exactly that). Downsides: the damage source is not distinguished (death zone, falling, poisons also give stacks), hits within 0.1 s merge.
  - Path 2 — a real buff with an icon: a GameplayEffect with stacks (up to 10), 2 s duration with refresh, damage reduction modifier; build in the UE project and cook, like the Jester model (`<data>\ue_jester`). The game applies and removes it itself; slower, but honest and visible to the player.
  - Not verified for either path: whether the game reduces damage by `DamageReduceRate` (attribute `TgfDefendAttributeSet`). First step — the stand: set `DamageReduceRate` and compare HP between mob hits.

## Trade: purchase cancellation, deposit refund, auction (07.11)
- [x] **"I can't cancel the order."** Purchase cancellation worked the first time, but the server returned the deposit (gold) as a `refund_data` record, which the client does not show. The player saw the order as active and pressed "Cancel" again → -4237 (prod 06–07.11: order `#1001546` — 26 attempts; on the stand refunds were never collected). Sales were not affected: the client shows and collects an item return. Fixed: the purchase deposit is a confirmation with gold of type FailedReturn (like the seller's proceeds), the remaining purchase amount is zeroed; old closed purchases are converted on the fly while walking the book (`migrate_purchase_refunds`). Test `purchase_cancel_refunds_escrow_as_confirmation`. Stand 07.11: works (user).
- [x] **Auction locked.** At login we sent `unlock_trade_auction_privilege_timestamp: 0`; now 10 years ahead — the auction is always open (the user's decision). Stand 07.11: works (user).
- [ ] **The price in a buy order is probably the total, not the unit price.** Prod log: "3×24000 — immediately 3 pcs for 24000" (item at 8000), and without sellers "3×24000 — 72000 charged"; `#1001546` 5×62500 at a price of 12500 — 312500 charged. The server treats `price` as a unit price: for an order without an immediate deal the deposit is N times larger, and a seller who sells into such an order gets the price for the whole batch for one item. Not confirmed by a client recording with count > 1 (the stand only had orders for 1 pc). Check on the stand: a buy order for 2–3 pcs below market, look at `price` in rpc_dump and the numbers in the window.
- [ ] "Buy now" −4254 (129 times on prod): the shown min. price (`trade_basic_info`) includes one's own and empty sales, while the instant purchase (`sales_for`) skips them — check and align.
- Other trade refusals on prod 06–07.11: `market_sell_items` −608 (35), `trade_sales_order_publish` −4209 sales limit (8), −4265 selling an equipped item (expected), `trade_bid_order_publish` −4237 bid on a finished lot (5).

## Inventory: items stack on top of each other (07.11)
- [x] **Swords are placed on top of each other** (user, stand 07.11 19:30). Swords 181010000 are 3×1 along x. On a swap (`inventory_item_swap`) the client places sword A with an offset — goal x1, while sword B lies at x0 — and itself says where B goes (`specify_swapped_item_new_position`, x4). The server did not read this field and looked for B only exactly in the goal cell: not found — A was placed on top of B, B stayed in place, and the push to the client had only A. The client trusts the push and drew A over B. On purchase (market_buy_product) swords were placed without overlaps — it was the swap that broke things. rpc_dump has 11 such swaps in one evening (seq 2390…397). Fixed: `swap_items` takes B by guid from the request and puts it on the given cell (if not given — at A's old place), the push contains both items. If something is missing — rollback, response −620 (`EnErrItemSwapPositionCheckFailed`). Test `swap_with_offset_goal_moves_other_to_specified_cell`. Whether the cell is free is checked by the client; a server-side overlap check was added and removed by the user's decision (cdb4923 → the next commit). Items already overlapping do not separate by themselves. **Not verified in game.**

## Merchant and playtest settings (07.11, stand; not deployed to prod)
- [x] **Crafted items sell to the merchant for 0** (the user's decision). Relics (Heirloom, 22xxxxxxx: results of `gear_heirloom_guide` and UE resources from the Heirloom folder) have no `system_repurchase_price`, and the merchant took the trade price `price_def`: pink EXCEPTIONAL — 40000, EPIC — 9000, ENHANCED — 2700, UNCOMMON — 900. Now `merchant_sell_price` = 0 for all crafted items; insurance is still based on `item_sell_price`. Test `crafted_items_sell_for_zero` on real data (d0a7ed3). **Not verified:** what price the sell window will show — if the client computes it itself, it may still display 40000 while paying 0.
- [x] **Playtest (the user's decisions 07.11, d7450d6):** starting gold 100 000 000, later reduced to 10 000 000 (user 07.11) — season flag `start_gold`, gold of players created earlier does not change; the merchant refreshes once an hour — `merchant_refresh_secs = 3600`, rotation 1009–1011 also changes goods every hour; 25 of each item on Buyout and the auction — `TRADE_SYSTEM_BUYOUT_STOCK=25`, `TRADE_SYSTEM_AUCTION_LOTS=25` in the environment (stand — `tools/stand/run_local.ps1`). The stand DB was wiped: the old one is in `surreal_saves_before_playtest100m_20261107`.
- [x] **Crafted items are not listed by system sellers** (the user's decision 07.11, eaa56f6): neither on Buyout nor on the auction; old system lots without bids are removed while walking the book. Stand: 3848 removed, the book has 8710 orders, 3368 KB. Players are also forbidden to list relics (ae07228): `trade_escrow`, shared by Buyout and the auction, answers −4212 (`E_SALE_ITEM_INVALID`), test `crafted_items_cannot_be_listed`. **Not verified in game:** what message the client shows.
- [x] **Mixed rooms** (the user's decision 07.11, 77e50fe): season flag `mixed_party_rooms` (playtest — on). Launcher lobby: the vote on size stays (user: do not remove) and sets the map options and room seats, while "Start" lets in a group of any size 1–3. Queue without the lobby — one pool per map, the room as for solo. Limits: seats (players) and spawn point sets (groups by match team). **Not verified in a match:** how a group spawns in a solo spawn point set (a trio in a solo room) and what else map options count 1/2/3 change.
- [ ] **Trade book ~4 MB:** at 25/25 system sellers listed 12 558 orders (was ~966 = 240 KB); the book is a single document and is saved in full on every deal. Watch trade latency; if it slows down — reduce the numbers or store system lots separately. Prod needs the same variables in dbserver.env and a DB reset — only with a separate confirmation.

## Gear insurance "as in the original" (08.11, stand; not deployed to prod)
- [x] **Insurance price from the market** (1e945d6): average deal over a week, without deals `price_def × 3`; percentage by rarity
  (legendary 20%). Formulas — `docs/INSURANCE.md`, "Our formulas". Stand: ring 185015002 — 108 000.
- [x] **An insured item is not lost** — verified in game 08.11: owner died, the ring stayed.
- [x] **Merchant premium — 85% of the fee**: the game showed 91 800 in the premium tooltip, while the backend paid the full fee 108 000.
  LOCAL found the native code: the battle server sets the premium's `sale_price` = fee × (1000 − const `dice_attenuate_ratio` 150) / 1000, rounded down.
  The backend sells for the premium's `sale_price`, without it — by the same formula. Matches the original: 166 617 = 196 020 × 85%.
  **Not verified:** a sale after the change in game.

## Ley line seals and pink treasures (08.11, stand; not deployed to prod)
- [x] **Why there were no pink/gold treasures:** pink 2100002–2100005 (24–100 k) and gold 2100006–2100009
  (70–320 k) in the template drop **only from the altar** `BP_Altar_Special_Dungeon/Castle` (g047 type 3 — pool 900118, pink 50%;
  g048 type 4 — two gold rolls), and the altar works only for a seal. Seals dropped rarely and from 7 sources.
- [x] **Seal chances raised** (the user's decision, `content/seasons/playtest/loot/loot.toml`, file header): fragments from all
  chests and monsters of the Crypt Classic/Hard and the Castle Hard, seals from the gold chest, the thief goblin and bosses (Hard ×2).
  Pak `content/seasons/playtest/pak/zzzzzz_Balance_P.pak` (sha256 cab7a0c1…) is on the stand: Steam, F:, E:; the old ones are in the scratchpad.
- [ ] **Not verified in game:** whether there are altars on our map (loot variant 3/4 is chosen by the battle server), exchanging a seal for
  a treasure, combining fragments. The pak is not deployed on Robot.
- [x] **Why seals did not drop even at 100%** (LOCAL's analysis, docs/LEYLINE_SEALS.md): the native Loot looks up the row by
  `UBattleDsUtility+0x1d4` (loot_map_id), which only InitGameMap sets — on listen it is 0, so **every match took map 0 loot**;
  the Classic/Hard tables never worked. The R1-loot conclusion "by GameState.MapId" is wrong. Fix: dbnative `op=lootmap` +
  RuServer `LootMap` (`[LOOTMAP]` lines). **Not verified in a match; not deployed on Robot.** The PVE tables Classic/Hard will kick in —
  check the drops.

## Leaderboard (08.11, stand; not deployed to prod, not verified in game)
- [x] **The ranking window was always empty:** `rank_get_specify_rank` / `rank_get_self_rank` answered from `rank_board`, but
  nobody wrote scores there. Now — `core/backend/src/rpc/handlers/rank.rs`: tables "average player kills per match" in
  the dungeon (rank_type 1001, data_type 3 — solo search, 4 — team; per level 40003015..20, "all levels",
  per class and "all classes" — as in `rank_report_setting`). Eligibility — 5 matches of the role in 7 days, the average — over the last 50
  matches (const report_pve_kill_player_rank_min_batlle_count / pve_kill_player_rank_max_save_battle_count, tooltip
  RankHelpTips_3); score = average × 100 (the client divides by 100, verified via the WBP_Rank_ListItem_MyEntry bytecode).
- [x] **Kills for the ranking are counted without the `kill_reward` season flag** (`/ds/kill` → `raid_entry.kills`); the reward
  by mail is still only with the flag. Solo/team — `raid_entry.team_size` (party in the room when sent to the raid).
- [x] Tables per season: id `<season>_1001_<instance>_0_0`. GM: `rank`, `rank add <kills> [team]`, `rank reset`.
- [ ] **Not verified in game:** the ranking window with entries (names via the role cache, "999+" place outside the table), `/ds/kill`
  in a raid without kill_reward. Arena (data_type 1/2, level 50000003) not done — we have no such mode.

## Search checks and rooms — as in the original (08.11, the user's decision; stand, not deployed to prod)
- [x] **Gear rarity limit re-enabled** (`match_checks.toml`: `gear_rarity`, `gear_score`, `level_cost` —
  `enabled = true`). There was only one difference from the original: Hard 40003019/40003020 — all slots EPIC and above
  (EPIC/EXCEPTIONAL/LEGENDARY/UNIQUE). The original `competition_level` has no minimum gear score and no entry cost.
  `competition_level` now matches the original and is not included in the balance pak. Downside: with grey starting gear one cannot
  enter Hard until an epic is equipped in every slot.
- [x] **Mixed rooms turned off** (`content/seasons/playtest/season.toml`: `mixed_party_rooms = false`): group size equals
  room size (solo — solo, duo — duo, trio — trio), as in the original. Since 07.11 it was: solo and duo could join trio rooms.
- [ ] Not verified in game; for prod — the `ru.balance` pak (Robot and players) and `seasons/` on the VM with a backend restart.
- [x] **Hard — "skirmish mode" and Challenger Mode** (08.11, the user's decision; game tooltips `MeleeMode_Description`,
  `SelectMap_Match_description`). `matchmaking::queue_size`: on Hard (40003019/40003020, `skirmish`) any group, and with
  "challenge mode" on (`level_challenge`, `state.progress_level_challenger`) solo/duo — into the pool of three-player rooms;
  otherwise strictly by group size. Groups in a room — no more than spawn point sets, players — no more than seats.
  The launcher lobby on Hard does not require an exact group size. Toggle: `level_challenge` now pushes
  `dirty_level_current.current_select_challenge`, `player_get_info.user_level.current_select_challenge` is filled in too.
  **Verified on the stand 08.11:** the checkbox toggles (off → on), solo on Classic went into a three-player room.
  Party: the mode is the captain's choice; `DTeamStorage.challenger_model` in the team snapshot, when the captain toggles it the snapshot
  is sent to everyone, search takes the captain's mode. **Party not verified in game** (we do not send the chat action `ChallengerModelSwitch`).
- [ ] **Pass quests, stand 08.11:** monster kills are counted (quest 1651809 completed, reward received);
  chests — 0 (the native counter was not found?): added diagnostics `[QUEST] native counter: …` and a fallback hook
  `AddActorIntStat` (5, 41). The "claim" button did not disappear even with RECEIVE status in the snapshot — now also the event
  `dirty_quest_events: Received`. Neither verified in game.
- [ ] **Launcher lobby: minimum 8 players and matchmaking rules** (08.11; backend f1fa15d, launcher 4dd1fd5,
  LAUNCHER_SPEC §12.3a). Tests pass; not verified in game, needs a launcher release (the UI is in the exe) and prod.

## Stackable items when moved (08.11 evening, reported by the user; fixed, on prod since 13:54 UTC)
- [x] Items that stack (e.g. potions) "behave strangely" when moved in the inventory/stash.
  Reproduced by the user on prod 08.11 13:14–13:20 UTC (open_id 76561198000000906). Backend log:
  - `inventory_item_move guid=100000096: cell for part of the stack guid=100000096 is occupied` (logged in Russian) (and the same for 100000110) — moving
    part of a stack into an occupied cell is rejected; probably the client adds part of a stack onto an identical stack (should
    merge up to the stack limit), while the backend considers the cell occupied;
  - then `the player has no item guid=100000110` (logged in Russian) — the client refers to a guid the server does not have: the client's and
    server's views diverged (after the refusal the client apparently had already shown the move locally).
  Needed: in the `inventory_item_move` code (src/rpc/handlers/inventory_handlers.rs) — moving part of a stack onto a stack of the same
  type (merge); on refusal — push the original state so the client rolls back; a recording of the exchange (RPC_DUMP on the stand) with the move
  request bodies.
  - **Cause (stand, rpc_dump 08.11 seq 368–388):** (1) for a stack onto a partial stack the client sends one request with two
    actions on the same guid: "1 pc → onto the stack" + "2 pcs → neighbouring cell". The actions were applied separately: the first
    was rejected ("full 3/3"), the second executed — the stack was split in half, while the client considered it whole. (2) The
    `dirty_postion_action` push contained all intermediate steps of one guid ("removed / 2 pcs in the backpack / removed / 2 pcs in
    the inventory") — the client lost track of where the stack was and later took it from the old place.
  - **Fixed (inventory_handlers.rs):** `apply_moves` — the request is "all or nothing": if one action fails —
    the player is as before the request, all actions go to failed. `compact_dirty` (in `inventory_action`, for all moves/equips) —
    per guid one "removed" entry from the old place and one final entry; no change — nothing. `move_count`: a whole
    stack onto a full stack of the same type — swap places; part of a stack onto a full one — still a refusal.
    Tests `stack_request_is_all_or_nothing`, `stacks_merge_and_split_like_client`.
  - **Verified on the stand 08.11 19:45:** 75 actions (adding to stacks, splitting, double requests, moving back and forth) —
    not a single position/count mismatch between client and server; the only refusal — part of a stack onto a full one
    (seq 90), the client stayed in sync. Swapping a whole stack with a full one did not occur in game (not verified).
  - **Prod 08.11 13:54 UTC** (on "Yes, deploy"): backend b0081ae (Linux, from `git archive` in WSL, 126 tests,
    sha256 2e4a6434…), `maintenance.sh restart` — the old one in `dbserver-rs.prev`. Maintenance lifted at 14:02 UTC on the user's
    word, timer `dbmaint-off-20261108` stopped.
    Not verified in game on prod.

## Lobby: vote reset when a player leaves, minimum 4 (08.11 evening, reported by the user; on prod since 15:25 UTC)
- [x] On prod 14:37–14:38 UTC tab 1 gathered 8, passed the vote on size (1) and on the map, but by the end of the vote
  there were 7 players — per rule 12.3a the votes were cleared and the tab went back to gathering. The log did not show who left and why.
  Fixed (`core/backend/src/lobby/launcher_lobby.rs`): a started vote is carried through to the end, reset only when everyone has left;
  log entries "left during the vote" / "dropped out of the vote — launcher silent for N s" / "removed — maintenance".
  Default minimum `LOBBY_MIN_PLAYERS` 8 → 4 (the variable is not set on prod — after the deploy it will be 4).
  Test `gather_until_min_players`, LAUNCHER_SPEC §12.3a.
  - **Prod 08.11 15:25 UTC** (on "yes", gracefully): backend 77eaae6 (WSL from `git archive`, 126 tests, sha256 7d336c5a…),
    `maintenance.sh restart` waited for the end of match l40003016_146079001 (8 of 8 results), backup `backup_20261108c`
    (b0081ae), the old one also in `dbserver-rs.prev`; maintenance lifted right after. Not verified in game on prod.

## Rarity limit: Castle normal — grey and green (08.11 evening, the user's decision; on prod since 17:18 UTC)
- [x] `match_checks.toml` `[gear_rarity] levels`: 40003016 = NONE/ROTTED/NORMAL/UNCOMMON in all three rows of the map
  (seasons 1, 1001, 1002 of the `competition_level` table). `balance build --season=playtest`: competition_level is now in the
  balance pak (9 tables) and in `content/seasons/playtest/server/ExcelConfig`.
- Finding: in the original `competition_level` the season 1 rows have limits on all maps (Crypt normal — up to green,
  Castle normal — grey only, Crypt Classic — blue and above, Castle Classic — green and blue, Hard — epic and above), while
  the rows of seasons 1001/1002 (only 40003015–18) have no limits. Which row the client takes was not checked — hence the earlier
  entry "limit only on Hard". The new rule is in all rows, so it does not depend on which row is chosen.
- [x] Tooltip: backend 58e579a — `map_rules` takes its level limits from `[gear_rarity] levels` (`gear: "green"` —
  grey/green only). Launcher 1.5.1 only knows `epic` — the text "Grey and green gear only." is in the launcher
  sources (8ff136b), will appear with the 1.5.2 release (not released).
- **Prod 08.11 ~17:18 UTC, hard restart on the user's word** (the last player IVO lost connection at 16:49 and did not return, match
  l40003015_151528061 was hanging): the graceful `maintenance.sh restart` was stopped, backend 58e579a (sha256 8b09fc0f…) installed
  manually, `match_checks.toml` and `core/backend/ExcelConfig/competition_level.bytes` on the VM, package `ru.balance-2026.1108.2243`
  (modset e671ecdf…). Robot: agent and all DS stopped (`start_headless -Quit -All`, IVO's match cut off), balance pak
  e0e82c93… (the old b399c499… in `Kitackup_before_20261108c`), agent started, DS came up. VM backup
  `backup_20261108d`. Maintenance lifted. IVO's raid result was most likely not recorded.
- [x] **The limit did not work in the launcher lobby** (reported by players 08.11 evening): the client checks rarity against the map selected
  locally (`level_select` in `battle_matching_search`), while the match map comes from the vote (prod 17:23: tab 2 voted for
  40003016, …753 pressed "Start" with 40003015 selected). "Epic and above" on Hard was bypassed the same way. Fixed: a check on
  the server before entering the hall (`lobby_handlers::lobby_gear_refusal`) — the map from the vote result (`launcher_lobby::voted_level`),
  rule `match_checks::gear_rule` (the level's own list, or Hard — epic and above), what is equipped on the current character of each
  group member (gear only, rarity 0 is not counted); refusal — a "Gear" window listing the rarities and who.
  Tests `gear_rule_by_level`, `gear_violation_words`. **Prod 08.11 18:08 UTC** (on "Straight to prod", gracefully):
  backend 3c7dcd9 (sha256 872ad97f…), backup `backup_20261108e` (58e579a), maintenance lifted. Not verified in game.
- [ ] Not verified in game: the "All slots…"/"Gear" window with a blue item on Castle normal.

## AFK kick by time (08.11 evening, task from the user; NOT DONE)
- [ ] A player who does nothing in a match for N minutes (or whose game dropped off the backend and did not return) should be kicked
  from the match by the server. Case 08.11: IVO (…749) lost connection to the backend at 16:49:19 (relay 6601 closed, no reconnects),
  and match l40003015_151528061 kept him until the match timer ran out and prevented a graceful backend restart.
  To think through: the AFK criterion (no input/pawn movement on the DS — RuServer mod; or no client connection to the backend — hub),
  the time (e.g. 5 min in a match, 2 min without connection), what happens to the character (death with loss as when quitting the game, or
  extraction), a warning to the player before the kick, an exception for the waiting hall. Write it up for the user before implementing.

## Set bonuses on blue and purple gear (09.11, the user's decision; on prod since ~19:50 UTC 08.11)
- Before (original): the set bonus (`gear_set_*`) — only on pink set items and 5 legendaries (always), Midas
  (coin_master) — on 45 green items (weapons, armour, rings, amulets 181–185) as one of the extra property sets (~1 in 5),
  fixed 3 — on 36 pink relics. Points for rarities 4/5 (blue/purple) exist in `affix_value_template`.
- [x] Playtest season (`tools/py/season_set_affixes.py` → `content/seasons/playtest/balance/affix_pool.json`, `affix_template.json`):
  pool 100802 — 7 sets (without Midas and Kleptomancy — it was not in the original), equal weights; 80 items (37 blue,
  43 purple, 181–185) are added as a variant to one extra property — the one with the most sets. Chance ~14–17% per item,
  at most one bonus. Set collection — as is (the set is random out of 7).
- [x] Backend (`affix.rs`, merchant/shop/reward items): extra property — first the set, then a property within it (before —
  from all properties of all sets in a row: Midas on green weapons dropped ~1 in 12 instead of 1 in 5). Test
  `set_affixes_on_blue_and_purple` (80 items × 400 rolls: 12–19%, ≤1 bonus, no Midas/Kleptomancy; Midas ~1/5).
- [ ] Not verified: that the battle server (raid drops) picks the extra property the same way (set → property) and shows the bonus on
  blue/purple; deploy (ru.balance pak on Robot and to players, ExcelConfig affix_pool/affix_template on the VM,
  backend restart — on "yes").

## Lobby: 6 instances and percentage voting (09.11, the user's decision; on prod since ~19:50 UTC 08.11)
- [x] The launcher shows 6 instances instead of 4 (`LAUNCHER_VISIBLE`, 9a5ff9a); on prod LOBBY_INSTANCES=8 and 8 DS slots — unchanged.
- [x] The vote result is by percentage (`launcher_lobby::winner`): an option's chance = its share of the votes, an option without votes never wins,
  no votes at all — any option equally. Test `winner_counts_votes_and_breaks_ties` (3 vs 1 — exactly 3000/1000 over 4000 rolls).
- [ ] Launcher: show the chance next to options ("3 · 75%") — with the 1.5.2 release. Backend deploy — on "yes".

## Returning items to a trio in the 3rd instance (08.11 20:11 UTC, on the user's word)
- A trio of players (…757, …731, …778) in match l40003017_167252012 (DS 7817) together with owner and hipach (who were given hp
  99999). No results had arrived yet — items in the DB as before the match. Decision: server 7817 was closed (`quit` in
  `Kit\run\dbserver-7817-control.txt`) — the raid result does not arrive (as with the hard deploy at 19:50), the backend closed the room
  ("did not arrive in the hall"). All five have items as before the match; the match loot is lost. RaidNative results for them after 20:11 — 0.
- [ ] There is no proper way: after death items are not kept (only the guid in `raid_gear`). A possible improvement —
  an admin list "keep raid items" (the result is not applied); the started change was reverted, not done.

## Relic resources 5000 each at login (09.11, the user's decision; ready, NOT DEPLOYED — enable later)
- [ ] `content/seasons/playtest/season.toml`: `start_materials` = 1037 Ilmenite Ore, 1038 Noble Steel, 1039 Star
  Shard, 1052 Piece of White Cast Iron, 1053 Mithril Token — 5000 each. Granted at login once per player (marker
  `playtest_materials`): since the evening of 08.11 the grant was off; on prod only owner has the marker (…730, 13:05) — all
  other current players will get it at their next login, owner will not get it again (has 5000/5000/5000/20000/500).
- To enable: `content/seasons/playtest/season.toml` on the VM (`/opt/dungeonborne/seasons/playtest/`) + a graceful backend restart.
  There is no mass mailing to everyone (GM `mail` — only to oneself).

## Portals: inside the circle on all maps, no red or purple in the Castle (09.11, the user's decision; mod ready, not deployed)
- Before: portals appeared in the zone. On the second floor the mod placed them in the ring between the current and the next circle ("like
  the native Generate" — the part that will close soon); native ones (first floor) — the same. In the Castle the mod added red ones
  (roll of a blue device on the 5th disc 50% + a guaranteed red: decision of 06.11 for the Crypt; the Castle got it as a map with
  a second floor); purple ones led to the "hall under the map" (stuck players on the evening of 08.11).
- [x] RuServer mod: hook on the native `BP_DeathZone:SpawnOnePortalByType(Type, Transform)` (before the BP body) — a spot beyond the next
  circle (`Next Center/Next Radius`, without it — the current one; margin 0.85) is replaced with a free native point
  (`PotentialPortalPlacements`) inside; in the Castle (`GothicCastle`) the type: red/shadow → blue (2,3→1; 6,7→5; 10→9).
  Second floor (`Spect.FloorPlace`/`NativePlace`) — also inside the next circle; `RedTick`/`RedSure` are off in the Castle;
  the floor rules in the Castle place red/shadow ones as the blue native type. Log: `[PORTALPLACE]`.
- Stand 09.11: the SpawnOnePortalByType hook is registered, but native first-floor portals are placed bypassing it (by classes from
  the disc settings) and are already inside the circle — Castle Classic (24) and Crypt Classic (18, up to the 5th disc via
  `nextdisc`): "inside the circle" (user). The hook is kept just in case (commands and the second floor). For the Castle,
  conversion of spawned red/purple devices and portals into blue on tick was added (`Spect.CastleTick`, native
  ConvertPortalType, log `[CASTLE]`); per the user, there are none in the Castle anyway — not checked in game.
- Mod: command `nextdisc [sec]` — shifts the end time of the current zone state, one step per command (verified).
- [ ] Deploy the mod on Robot (with "Hand of Justice" and nextdisc) and the backend (status 16 as death) — between matches, on "yes".

## Next deploy (list as of 09.11, by the user's decision)
**Deployed 10.11 07:34 UTC** (gracefully, `maintenance.sh restart 45`, then `off`, on the user's word "everything from HEAD"): backend 709b147 (sha 4c8c84ec…) — server watchdog 3627cfb, auction eb460fc (+ `core/backend/ExcelConfig/trade_commodity.bytes` 26a87416…), UDP tunnel 251ba7d. VM backup `backup_20261110a` (dbserver-rs.33c15ca, trade_commodity.bytes). Packages for players (ru.balance / ru.client-hooks 2026.1109.2337) and Robot mods (afc1110) — not deployed.
**Deployed 09.11 ~11:33 UTC** (gracefully, `maintenance.sh restart`, on the user's word): backend 33c15ca (sha 1e07efd2…)
— everything marked [x] below; seasons/ from git (relic resources 5000, granted at login); `dbserver.env`:
`LOBBY_MIN_PLAYERS=2`, `LOBBY_WAIT_SECS=180` (hall 3 minutes — both in game and in the launcher via `ends_at`); RuServer main.lua on
Robot (865a2cba…, the old one — `Kitackup_before_20261109c`), old battle servers 7777/7787 closed normally (`quit`).
VM backup `backup_20261109c`. Not deployed (not done): empty hall after a cancel, map options per port, duplicate members.
Ready (in commits, not on prod):
- [ ] 10.11 ascension potion: XP was granted on death too, and a potion drunk on the 1st floor was lost on exit from the 2nd ("of which potions 0" for the user on prod). Commit (see the git log entry for the ascension potion; its message is in Russian): backend — potion XP only on exiting alive; RuServer — potion XP carries over through the floor transition. Stand: +3010 (of which potions 3000) after exiting from the 2nd floor. To deploy: backend (VM) + RuServer on Robot.
- [ ] 10.11 match ID in the native ping widget (commit 7e5092a; our own label 9380833 removed): RuServer writes LyraGameState.BattleId; no client mod or player package needed. Stand: the number in the ping line, also after the 2nd floor.
- [ ] 10.11 the guaranteed red portal was always in the centre — now anywhere in the current circle (7e5092a, stand: 1007 from the centre with radius 2000).
- [ ] 10.11 resurrection stones were lost on the transition to the 2nd floor — dbnative gofloor c3 = 1 (272fbc1; stand: stones on the 2nd floor, taken out after leaving the raid — verified by the user). Deploy on Robot: RuServer main.lua from HEAD + `dbnative.dll` (build with build.bat; the DLL is locked by the game — put `dbnative.dll.new`, start_headless installs it at a start without the game running).
- [ ] 10.11 the "deal damage" quest (1651603/1651620 and 1651803/1651820, damage to players) was not counted: the stats counter has an empty PlayerState field — on prod "components 3; no players", damage always 0. Commit 231d4f4 (RuServer mod): player by the component owner, reason in the log, monsters = the greater of hook and counter. Stand: monsters 3 / chests 1 = sent 3 / 1; damage to players — check on prod via `[QUEST] … damage N`. Deploy on Robot: RuServer + RuClient from HEAD (together with map options per port afc1110).
- [ ] 10.11 at night ~3 h without game servers: ds-agent on Robot crashed with a panic (code 101) at 03:37 UTC, the slots exited, nobody brought them back; then Robot stopped serving SSH (no banner), 7777 went silent after start. Commit 3627cfb: the agent does not crash on output, panic goes to ds-agent.log, a slot panic does not bring down the loop; `core/backend/ds-agent/install_task.ps1` (DbAgent task every 5 min + restart on failure, no 72 h limit); backend `ds::watchdog` — ALARM in the log and `GET /admin/v1/ds/health`. Robot rebooted by the user via the Hetzner panel (~07:00 UTC); deployed on Robot 10.11 ~07:12 UTC: `ds-agent.exe` 3a2fdc0 (+ protection against a duplicate after an agent restart, `Kit\run\ds-agent-starts.json`; the old one — `ds-agent.exe.bak_20261110`), the DbAgent task from `install_task.ps1` (auto-restart verified: 34 s). Backend deployed 10.11 07:34 UTC (see above).
- [x] RuServer mod (Robot): "Hand of Justice" (`ban`), command channel in the `/ds/slot` response, second-floor portals inside
  the next circle, SpawnOnePortalByType hook (safety net), Castle without red/purple (`Spect.CastleTick`), `nextdisc`.
- [x] Backend: a ban in a match → "Hand" to the battle server; status 16 (`ban_kick`) — as death.
- [x] Season: relic resources 5000 each at login (`start_materials`, 26f6121).
To do before the deploy:
- [ ] **Empty lobby hall after a cancel — as a new room** (prod 09.11 09:06, tab 1, …797: "Start" while the server was loading
  → after 6 s `battle_matching_exit` → `left` → 16 refusals "You left this hall"; the empty hall held the tab
  for up to 10 min). Returning to the same room is impossible (M11: the client does not go to the same battle_id). The user's decision: if
  the player who left presses "Start" while the tab's hall is empty and the match has not started — the old room is closed, a new one is opened for the tab
  (same map and group size), the player enters it; a hall with people in it is not touched.
- [x] **Map options file per port** (`ru-map-options-<port>.txt` in RuServer/RuClient, docs/CASTLE_SPAWN.md by LOCAL):
  the shared file on Robot gave the Castle server another server's options → 8 start sets instead of 14 → spawning close together in solo at 9+.
  Confirmed by the Robot 7797 log, match l40003016_167252001 (08.11): task [1020101], the map had [5002,10201,1020103] —
  6 solo sets disabled. Fixed 09.11: each server has its own file (7777 keeps the old name) + safety net `Spect.OptionGuard`
  in RuServer — every tick of the match map the group options are as in the task from memory; on mismatch it corrects them and logs `[MAP_GUARD]`.
  Stand 09.11, server 7787, a trio decoy in the shared file: with the old RuClient — `[MAP_GUARD]` removed 1020103, set
  1020101 while the map was loading (before the player entered), once; with the new RuClient — `[DIAG]` [5002,10201,1020101], the safety net was not
  needed. The effect with 9+ players cannot be checked on the stand.
  **Not deployed** (09.11 user: "note it for the deploy"): on Robot RuServer + RuClient main.lua (db476e55… / 3f205045…),
  backup in `Kitackup_before_20261109d`; do not touch running matches — their servers will finish with the old files.
- [x] **Auction: items without a set bonus, and filters** (09.11, user). (1) In `trade_commodity`, 396 market-2 goods had
  `item_data_require_id = 2` (EN_TRADE_ITEM_REQUIRE_TYPE_GEAR_SET) — the client did not allow listing an item without a set bonus;
  the requirement was removed in `content/balance/tables/trade_commodity.json` (the item set is unchanged). (2) The class filter let everything through: the class
  tag came only from `special_career_require` (empty for ordinary equipment) — now classes and slots come from
  UeSource_Equipment/SubType (`gear_fit_table`): weapons by weapon_configs, armour by cloth/leather/plate, rings/amulets —
  for everyone. (3) Base properties (affix_type BASIC) — tag 1008 (the "fixed affix" filter used to let everything through).
  (4) The main page collections "slot × rarity" were empty — they are filled by the slot tag (1004). Lot tags
  are recomputed when the book is loaded. Stand 09.11: item 181043000 without a set was listed (#1063860); class 6 — 170 lots,
  0 foreign ones (plate, maces, shields, crossbows, jewellery); slot collections have lots.
  **Not deployed:** backend + `core/backend/ExcelConfig/trade_commodity.bytes` on the VM; the `ru.balance` package for players (make_mods.py, update
  via the launcher — without it the client still will not allow listing); the pak on Robot for consistency.
- [x] **The server no longer enables the UDP tunnel by itself** (09.11, user: "it turned on automatically, let's remove that").
  Before: `network_block_loading` (the client did not load into a live server) → tunnel on (for owner 08.11 00:11 and 20:11 UTC).
  Now on `network_block_loading` the player is only released from the room; it is enabled by the player in the launcher and by GM `udp on|off`.
  Tests 134 ok. **Not deployed:** backend.
- [ ] Duplicate members in the host task (…771 twice in l40003018_167252051) — backend.
- [x] Player statistics (19f1236, `stats.rs`): games, extractions, deaths/failures, player kills, time in raid (result),
  monsters, chests, damage to players (`/ds/quest_progress`); `DUserRole.statistics` (per map) and `user_statistics`.
  Stand 09.11: raid Crypt Classic (death) → the character has games 1, deaths 1, failures 1, monsters 3, chests 1, 276 s;
  the statistics window in game shows it (user). The client did not set `need_user_statistics` in these requests —
  the window takes the character's statistics. Counting starts from the deploy; past raids are not included.
- [x] **Backpack width by race** (`inventory_handlers::fit_bags`, `apply_bag_sizes`): per const.json 6×10, humans
  +1 (7×10); before, everyone had 7×10. At login, character creation and raid result; items from the 7th column of non-humans
  move to a free spot in the backpack, otherwise the stash, otherwise they stay (not lost). The small backpack is not touched
  (the user's decision). Along the way: the class in statistics — from role_type.meta.career (was type−400000, priest and rogue
  were swapped). Stand 09.11: an undead character created (400006) — backpack 6 columns, works (user).
- [x] **Rarity limit — for the backpack too** (`lobby_gear_refusal`, `carried_violations`): on "Start" in the launcher lobby
  gear in the backpack and the small backpack must not exceed the map's highest allowed rarity (Castle normal — blue and above
  not allowed in bags; Hard "epic and above" is a lower bound, only for equipped gear). Before, only equipped gear was checked —
  blue items were carried out in the backpack. Stand 09.11: blue item 181125000 in the backpack → 3 "Gear" refusals, in the stash → passed.
- [x] **Rarity limit when entering the battle server** (`agent_link::battle_user_data` → `raid_gear_gate`,
  `stash_over_limit`): after "Start" items can still be moved around, so on the first delivery of match data (not
  a reconnect, not the next floor) gear outside the map limit goes to the stash: equipped items not on the list, in bags — above
  the upper bound. No room in the stash — it stays. Stand 09.11 (the "Start" check temporarily off): blue
  181125000 and 181043000 went from the backpack to the stash on entering the Castle hall.
Deploy: mod on Robot with a DS restart between matches, backend + seasons/ on the VM, gracefully, on "yes".

## For deploy (since 12.11, "note it for the deploy")

- [x] **DEPLOYED 14.11 06:58 UTC (b976aae).** **Lobby: initial delay of 10–20 minutes between instances** (13.11, user: "add an initial delay of 10-20 minutes between
  instances so that all instances are not endlessly in a match"; choice "Offset at startup"). DONE 4df69c1
  (WSL: tests 165/165, build df4f56f9): at backend startup the tabs open one by one — solo·Easy immediately, then
  duo, trio, solo·Classic, duo, solo·Hard, each a random 10–20 min after the previous one (the last after ~50–100 min);
  after a match the tab starts gathering again immediately. A closed tab: phase "closed", ends_at = opening time, joining — instance_not_open.
  LOBBY_STAGGER=0 — all at once, LOBBY_STAGGER_MIN_SECS/MAX_SECS. Launcher: the "Opens at" and error texts — 1.7.5 (handed over
  to the launcher session); before it the launcher shows raw "lobby.phase.closed"/"instance_not_open". **Deploy:** backend 4df69c1
  (it also includes the item return c2aa021) + launcher 1.7.5, on "yes". Keep in mind: after every backend restart the tabs again
  open one by one over ~1–1.5 h.

- [x] **DEPLOYED 13.11 21:04 UTC** (user: "deploy the new launcher 1.7.5"): exe on the VM (sha 8d190884 checked), the old
  manifest — `launcher.json.1.7.4`; the manifest /.well-known/dbserver.json serves 1.7.5, the exe downloads (200, 20020224 bytes).
  **Launcher 1.7.5** (14.11, user: "build it for prod", then "note it for the deploy"; built by
  the launcher session, commit 5e0415f launcher main). File `launcher/dist/launcher-1.7.5`, sha256 8d190884…3a20 (8d1908847b45b23659127ecfe7f44bf5011c072f9b60ce24143cb9aebfa43a20), 20020224
  bytes; signed, the window of the signed copy checked. Contents: texts for closed tabs (for the offset 4df69c1); live theme background on
  WebGPU; header and secondary text as on the site; smooth theme switching; Fluent animations — the user looked at them in the test
  launcher. **Deploy (on "yes"):** exe to /opt/dungeonborne/launcher/, launcher.json → launcher.json.1.7.4, the new one in
  its place, chown dungeonborne; rollback — launcher.json.1.7.4 → launcher.json. The launcher before backend 9374452 or together with it
  (otherwise 1.7.4 shows raw "lobby.phase.closed"). Afterwards — tell the launcher session, it will check the manifest.

- [x] **DEPLOYED 14.11 06:58 UTC (b976aae).** **"Tipsy Alchemist" (Kleptomancy 11002) in set bonuses — READY FOR DEPLOY** (13.11, user: "the
  Tipsy Alchemist set bonus affix never drops"; tables session, commit 9374452, tests 166/166). Pool 100802: weight 2 versus
  10 for the other seven sets (all season pool weights x10, other ratios unchanged): blue ~0.7% of items with an extra property,
  purple from bosses/elites — 2.8% of set bonuses. **Deploy (everything together, on "yes", gracefully via maintenance.sh):**
  - backend from **9374452** (it contains 4df69c1 and c2aa021 — take it instead of 4df69c1): affix.rs picks the set by
    the pool weight. **Do NOT deploy the tables without this backend** — otherwise items from the backend get the "Alchemist" in 1/8 of bonuses;
  - VM: `core/backend/ExcelConfig/affix_pool.bytes` and `content/seasons/playtest/server/ExcelConfig/affix_pool.bytes` (the same file, sha256
    ebc2ddfc…);
  - pak zzzzzz_Balance_P **d8d44788** (on top of 97846f3) — on Robot and in ru.balance for players; **replaces pak 58378cf7**.
    trade_commodity (b6ecacf5) and item_type (f7fd8835) unchanged.

- [x] **DEPLOYED 14.11 06:58 UTC (b976aae).** **Returning items from a match + return player B's items (13.11, commits b3e289c, c2aa021; user: "game 583076034 player
  player B give him back his items and make a command", "note it for the deploy").** GM `raidrestore <gameover-…bin> [apply]`: the battle
  server's record at the end of the raid (Robot `…\DungeonborneRuClient\native\gameover-<open_id>-3-HHMMSS.bin`, ~6000 of them since 03.11)
  is put into `restore/` on the VM; the return — equipped items and backpack (without insured ones and without those the player already has) by mail in batches of 10, new
  guids, each record once (state.raid_restored). GM file channel: `admin_cmd/<name>.cmd` = "<open_id> <command>" →
  `<name>.result` (folders 700, owner dungeonborne). Tests 164/164 (WSL), build edc226ca ready.
  player B (76561198000000905): match l40003018_583076034, PvP death 16:10 UTC (killer …783), result 16:26, 13
  items lost; record `gameover-76561198000000905-3-082640.bin` (1620 bytes) taken from Robot. **Deploy:** backend c2aa021
  (restart, script scratchpad deploy_20261113/swap5_vm.sh: creates admin_cmd/restore and puts the record), then
  `raidrestore … ` (list) → `… apply`; on "yes". The killer keeps what he carried away — the items reappear for player B.
  **player B'S ITEMS — RETURN** (from the match record, decoded with scratchpad decode_gameover.py; 13 in the record, 11 to return):
  | equipped/backpack | type_id | qty | guid in record | what | return |
  |---|---|---|---|---|---|
  | equipped | 181124000 | 1 | 100080766 | weapon, epic, 4 properties | yes |
  | equipped | 181084000 | 1 | 100085918 | weapon, epic, 3 properties | yes |
  | equipped | 182044000 | 1 | 100079250 | armour, epic, 2 properties | yes |
  | equipped | 220110541 | 1 | 100116970 | relic, weapon | yes |
  | equipped | 220190151 | 1 | 100150702 | relic, armour | yes |
  | equipped | 100012 | 3 | 100078025 | throwable | yes |
  | equipped | 100048 | 2 | 100083418 | throwable | yes |
  | equipped | 100018 | 1 | 100152896 | potion | yes |
  | backpack | 100018 | 3 | 100181843 | potion | yes |
  | backpack | 100018 | 3 | 100181959 | potion | yes |
  | backpack | 100048 | 3 | 100142726 | throwable | yes |
  | equipped | 182024001 | 1 | 100083514 | armour, 2 properties, insured | no — stayed with him (insurance) |
  | backpack | 100060 | 1 | — (guid 0) | consumable | no — no guid, raidrestore skips it |
  Return: open_id 76561198000000905, file restore/gameover-76561198000000905-3-082640.bin; `raidrestore` must show
  exactly these 11 (if the player already has some guid — it drops out of the list; cross-check).

- [ ] **Rogue: the mod "does not work" — solid visibility up close and missed materials (13.11, commit 8700f7d; user:
  "the mod doesn't work at all!!!", "Yes, fix it", "note it for the deploy").** Probe logs stand 12.11 / prod 13.11: the values in
  the materials are the same (2 m → strength 0.81), but strength S is only the silhouette outline (not visible in the dark castle); for a rogue in armour with
  a skin, the materials recreated on a new stealth entry stayed native for up to 1 s (in the second game 6 of 12 MIDs).
  Done: RuClient StealthReveal — solid_distance 400 (Visibility, almost opaque closer than 4 m), scan_ms 250, traversal
  of up to 200 components and 32 materials; seasons/base.toml solid_distance = 400 (the server overrides the code). **Deploy:**
  package ru.client-hooks (RuClient from 8700f7d + dbnative 3bef3859 + settings.ini; no restart) and base.toml on the VM
  (picked up at backend startup); before that — the stand with two clients (the solid mode has not been seen in game); on "yes".

- [x] **13.11 16:27 UTC: backend 2250bea (sha 2f70a1e6), gracefully; launcher 1.7.4** (copy `backup_20261113d/`, also containing
  server.toml). Isolation of third-party mods: server.toml `[mods] allow_unlisted = false` → manifest
  policy.allow_unlisted_client_mods = false; launcher 1.7.4 (e23cee8, sha 973ea7fe, rollback launcher.json.1.7.3) for the duration of the game
  removes third-party paks and UE4SS mods (except server mods and those built into UE4SS). The same backend has the market without old out-of-range
  prices (5ac0822): "lowest price" and price history ignore deals and orders at 1 gold. User: "Yes, enable it".
- [x] **13.11 15:18 UTC: backend abf5e7e (sha 50fa61c3), gracefully** (copy `backup_20261113c/`): in the permanent set
  all 6 tabs are visible in any phase — in 8dd35af a started solo tab was hidden from players not signed up (filter from 51ab88b),
  user: "5 instances in the launcher instead of 6", "all 6 instances must always be visible!".
- [x] **13.11 14:46 UTC: backend 8dd35af (sha 90b2f118), gracefully** (maintenance since 14:11, waited for 5 matches; copy
  `backup_20261113b/`). Lobby — a permanent set: 3 solo (Easy/Classic/Hard, the map alternates), 2 duo, 1 trio;
  tabs do not open or disappear, after a match the same one gathers again; start only by the timer, no seats — refusal.
  User: "3 max!", "all instances open, remove the mechanics of filling up and opening new ones", "let's do it gently".
  Before that b631277 accumulated solo tabs (5–8: started halls and matches next to new gatherings); 51ab88b ("one tab
  per difficulty", but without a match limit) did not reach prod — replaced before the restart.


- [x] **13.11 ~04:30 UTC (20:30 VM) deployed "part 1 only": packages ru.rebuild-private and ru.client-hooks 2026.1112.2100**
  (the same as on the stand 12.11, probe/auth_test/mods_nohosts). Reason: since 04.11 prod had ru.rebuild-private
  2026.1104.1341 (pak 5b457385) **without** the directory address — the pak with it (fc771d6e, 2693e2c) was built 11.11 but not deployed;
  the entry below "players already have it in 2026.1111.2022" was wrong. With `no_hosts`, login worked only for those who still had
  hosts lines from previous launchers; for the rest the game went to the official directory and showed the shutdown message of
  29.09.2024 (yian, Viperstar 12.11). ru.client-hooks: RuClient from HEAD (StealthReveal, settings.ini), dbnative 3bef3859
  the same. No restart, `manifest_mods.json.bak_20261113` — rollback. New modset ed678d46…. Part 2 (seasons/ +
  ExcelConfig, ru.balance 7253c9db, pak and SeedBones on Robot, DB copy, maintenance) and launcher 1.7.2 — waiting for "yes".

- [ ] **DEPLOY, PART 2 (13.11, "note the rest for the deploy"; everything on the user's "yes", with maintenance).**
  Cross-checked 13.11 with the VM and Robot — only these files differ:
  - VM `ExcelConfig/`: `loot_choose_group_rate.bytes` (cbbfde6e → ac79880e), `trade_commodity.bytes` (c8bd4afe → 636c436b) — 85d537e.
  - VM `seasons/`: `base.toml` (c4987556 → d9e725e9: `[client.DungeonborneRuClient.stealth]` → the manifest's `client_settings`),
    `playtest/balance/loot_choose_group_rate.json`, `playtest/balance/trade_commodity.json`, `playtest/loot/loot.toml` (85d537e).
    The season and tables are read only at backend startup — a restart is needed.
  - 13.11 UPDATED (tables session 718ddda, merchant prices 50%): pak zzzzzz_Balance_P **b37e4c36** (instead of 7253c9db;
    loot, market, properties the same + item_type) and VM `core/backend/ExcelConfig/item_type.bytes` f7fd8835… together with backend edb250f/df7b71c
    (restart). ru.balance must be rebuilt with this pak (2026.1112.2100 still has 7253c9db). Ley line seals on
    the second floor — the tables session is working on it, the pak may change once more; take the latest before the deploy.
  - **SUMMARY AS OF 13.11 (take this; the lines below are history):** pak zzzzzz_Balance_P **58378cf7** (97846f3: second floor,
    merchant prices, limits x0.5–x4) — on Robot and in ru.balance for players; VM: ExcelConfig + seasons/ from **97846f3** or
    newer (trade_commodity b6ecacf5…, loot_ue_source_choose_group ae7f4878…, affix_monster_pool_rule 81ca3e86…,
    item_type f7fd8835…, loot_choose_group_rate ac79880e…) and backend from HEAD (daf8dc2/0fa3eae: static price, merchant,
    dupe via the hall; cfcffc9: a refused instance join — logged as "[Lobby] … joining instance N rejected — reason");
    Robot RuServer `main.lua` from **0fa3eae** or newer (MobBones + second floor + HallGuard).
  - 13.11 UPDATED AGAIN (3fb63a2, second floor without seals and fragments; user: "Option A", "Seals and fragments"):
    pak **3b6fd533** (instead of b37e4c36, merchant prices are included) — to players (rebuild ru.balance) and on Robot; VM
    ExcelConfig + seasons/ from 3fb63a2 (new `loot_ue_source_choose_group.bytes` ae7f4878…, `affix_monster_pool_rule.bytes`
    81ca3e86…, item_type f7fd8835…); RuServer `main.lua` on Robot — from **0fa3eae** (= 3fb63a2 + HallGuard against the dupe
    via the hall; backend on the VM also from 0fa3eae or newer) (MobBones 67dea54 + LootMap.Want:
    on *_Official_02*/*SecondFloor* the loot map = loot_map_id + 10 for 10102/10103/10203). Pak and mod — only together
    (mod without the pak → no loot on the second floor; pak without the mod → second floor with seals, as now). Not verified on the stand:
    "[LOOTMAP] L_Dungeon_Official_02_…: loot=10112", no fragments on the second floor, with them on the first; MobBones there as well.
  - To players: `ru.balance` 2026.1112.2100 (pak zzzzzz_Balance_P 7253c9db) from probe/auth_test/mods_nohosts, entry in
    `mods/manifest_mods.json` (currently 2026.1112.616, pak 6a456af6).
  - Robot: `Content/Paks/zzzzzz_Balance_P.pak` 6a456af6 → 7253c9db (`game/paks/balance/zzzzzz_Balance_P.pak`); RuServer
    `Scripts/main.lua` 9919f23 → 67dea54 (MobBones only — mob bones within 30 m of players, replaced SeedBones 1d9774c; stand check — at deploy; take it from the commit, not from the working folder; + `Kit\run\dbserver-mobbones.txt` is not needed — on by default). The pak is locked
    by running battle servers — replace it with the DS and the DbAgent agent stopped; the old one — in `Kit\backup_before_<date>`.
  Order: maintenance (`maintenance.sh on` + wait) → stop the backend, copy `db_rocks` (82 MB) → VM files → Robot:
  `schtasks /End /TN DbAgent`, stop the DS, pak + main.lua, `schtasks /Run /TN DbAgent` → start the backend; check
  `client_settings` in the manifest, login, a match (elites, market: grey items cannot be listed, price limits; rogue; druid tree against
  skeletons) → `maintenance.sh off` → entry in the changelog (rogue, druid, elites 5/10/15%, grey off the markets).
  Rollback: `.bak` next to the files on the VM, `Kit\backup_before_<date>` on Robot, `manifest_mods.json.bak`.

- [ ] **Three solo instances always open: Easy / Classic / Hard, the map alternates** (13.11, user: "all 3 solo
  instances open, the maps in them alternate, the difficulty is always the same per instance (1 - easy, 2 - classic, 3 - hard)").
  Done: backend a29cfcb — SOLO_LANES (easy 40003015/16, classic 40003017/18, hard 40003019/20, Crypt → Castle →…),
  once started — immediately the next one of the same difficulty on the other map, the LOBBY_SOLO limit does not apply to solo (matches are limited
  by free Robot servers), voting in solo has no effect; snapshot: lane, difficulty, level_id in advance; test.
  6092b8f (user: "always 3 open, no filling up", "Refuse the join"): solo starts only by the timer, without
  starting on fill-up; the map's seats are taken — the join is rejected (hall_full "No seats in the hall").
  0f953de (user: "match timer in the launcher"): a running match has ends_at = start + map_detail.timeout (40 min for both
  floors; LOBBY_BATTLE_SECS — custom) — launcher 1.7.x shows "Match in progress 23:14" without updating; the match may end
  earlier (the tab is removed).
  `LOBBY_SOLO_LANES=0` — rollback without a rebuild. Launcher 1.7.3 (names "Solo · Easy/Classic/Hard", the map instead of
  voting) — handed over to the "Tauri launcher in Rust" session; it works without it, but the tabs are "Solo 1/2/3" with voting.
  The launcher is ready: launcher 7e6888a (1.7.3), not built/not signed/not deployed — the build and deploy are done by the launcher session on
  the user's word. Maps in it are named by the existing lobby.map.* (Dungeon / Castle), not "Crypt".
  **Deploy:** backend (restart, maintenance), with part 2; the launcher — right after; on "yes". Not verified on the stand.

- [ ] **Trade without a dynamic market price, limits x0.5–x4 on both markets** (13.11, user: "abolish
  the dynamic market price system… you can't sell more expensive than x4 and cheaper than x0.5"; "Yes, x0.5–x4 everywhere"). Before: the standard
  price (SP) = average of deals over a week (the client shows it, recomputed every 2 h), Buyout limits 50–200/500/2000% of
  it, auction 10–500…1 000 000%; insurance — from the average or price_def × 3. Done: backend daf8dc2 — SP = price_def
  always, limits and insurance (price_def × market_mult) from it; the deal history chart is kept. Tables — by the tables session:
  trade_commodity of both markets price_lower/upper_limit 50/400, is_calculate_sp false, new pak (the client takes the limits from
  its own table). **Deploy:** backend + ExcelConfig on the VM + ru.balance to players and the pak on Robot, with part 2, on "yes".

- [ ] **DUPE via the waiting hall** (13.11, user: "disconnects the internet during the waitroom (at the end of the timer), the backend
  saves the character as if he left, but the battle server spawns the pawn… dupes the inventory"). Cause: the battle server sent
  the result "left the hall" (`?scene=waiting_room`) → backend `raid_hall_leave` closed the raid without losses (items at home), while the
  RuServer mod (HallLeave: "JoinGuard will let him back into the same hall") let the reconnecting player in — a pawn, a raid with the same
  items; the backend rejected the raid result with 409 "result already applied" → items at home + what was given away in the raid. Done 0fa3eae:
  RuServer HallGuard (`GoProbe.HallLeft`, JoinGuard — in the lobby "You left the waiting hall — the raid goes on without you");
  backend `raid_late_after_hall` — a late result after hall_leave: nothing found is granted, given-away items that are not in the result
  are removed from home (death — except the small backpack and insured items), "ALARM … dupe?" in the log; test
  late_result_after_hall_leave_removes_given_away, backend 160/160. Not verified on the stand (disconnect at the end of the timer and
  reconnect — should kick to the lobby). **Deploy:** RuServer on Robot + backend on the VM, with part 2, on "yes".
  The prod case was not found in the log (nickname/time needed — to analyse the match and revert the dupe).
  Two addresses (user: "we have 2 IPs, backend and battle"): cutting only the connection to Robot (firewall on 203.0.113.20) gives
  the same "left the hall" result while the lobby works — HallGuard sits on the battle server and the backend safety net acts on the fact of
  a late result, so both cases are covered. Cutting only the backend does not close the raid (removal from search and the private room).
  Stand: check both — the whole internet and only the battle server address at the end of the hall timer.

- [ ] **Remove ley line seals (1900004, legendary 100100) from second-floor loot** (13.11, user: "remove them, also
  the leyline seals from the floor 2 tables"). Handed over to the tables session (seasons/playtest/loot/loot.toml; how the floor is distinguished — see
  docs/LEYLINE_SEALS.md: the native loot takes the row by the loot map UBattleDsUtility+0x1d4). Deploy — with part 2, on "yes".

- [ ] **The merchant buys equipment back for 50% of the Buyout Trades standard price** (13.11, user: "let's do 50 %"). Before:
  `item_type.system_repurchase_price` — a constant per type, median green 68 / blue 84 / purple 102 / legendary 108 /
  unique 90 gold (the client shows the same value in the tooltip); at 0 the backend took the price_def of the first trade row —
  189010000/189010001 sold for 400 000 at the auction price. Done: backend edb250f — only system_repurchase_price (no
  fallback trade price). New prices (483 types, market price_def 1 // 2: green ~450, blue ~1 200, purple ~5 000,
  legendary ~100 000, unique ~500 000) — research/merchant_prices/merchant_prices_50.json, handed over to the tables session: the seasonal
  item_type (ExcelConfig on the VM + ru.balance pak to players and Robot). **Deploy:** backend + tables together (otherwise the tooltip
  and the payout diverge), with part 2; on "yes".

- [ ] **Launcher 1.7.2 — fallback path via hosts (13.11, launcher 18b0907).** The directory domain in the hosts block; before
  launch a check that it resolves to 127.0.0.1; during the game, INTL login without the directory/lobby within 90 s → the next launch
  goes via hosts; setting "Connect via hosts". Tests 44/44, UI sandbox. Build
  `launcher/dist/dungeonborne-launcher-1.7.2-built.exe` (EF119C93…, copied right after `cargo tauri build`). **Before
  the deploy:** a check by the user on the stand (`http://127.0.0.1:8090/.well-known/dbserver.json`): without hosts — the log shows
  "came to the server without hosts"; with the toggle — the "hosts file needed" window, UAC, login. Then `release.py sign --exe
  <copy>`, look at the window of the signed copy, launcher/ on the VM. Those who need hosts will be asked by Windows once more
  (the 1.7.1 block without the directory domain counts as incomplete).

- [ ] **INCIDENT 12.11 16:12 UTC: the one-off relic reset ran a second time.** Backend c061f40a (the "without hosts" deploy) failed to
  read the marker `migrations:relic_reset_20261112` (`GameDb::get::<serde_json::Value>` — "invalid type: enum", the id field) and
  took away from 28 of 62 players what they had collected since 00:39: resources 4262, relics 60, stones 24, relic items 22. There is no
  DB backup between 00:39 and 16:12 (only the binary and server.toml were copied before the deploy — a deploy mistake). Fixed
  553139a (the call removed from main, the marker is read by type, a read error = do not reset, a second-run test), on prod since
  16:27 UTC, no repeats. Hot DB copy after the incident: backup_20261112d/db_rocks_hot_after_2nd_reset. Log of what was
  taken per player: scratchpad deploy_20261112d/relic_reset_2nd_run.log. Compensation — the user's decision.
  Lesson: before any backend deploy — a DB copy (systemctl stop + cp, or maintenance with a stop).

- [x] **Playing without hosts — ENABLED ON PROD 12.11 16:12 UTC (backend 55d63f5→553139a, `[relay] no_hosts = true`), a match without hosts verified 16:39–16:43 UTC (owner, launcher 1.7.1, l40003016_500866001: login, lobby, match, result; "works for me"). Maintenance was on at the time of writing.** (12.11, user: "to the launcher without hosts"). Verified on the stand
  12.11 21:20: hosts lines removed (0 active lines for the game domains), dev backend `tools/probe/auth_test/server_launcher_nohosts.toml`
  (`[relay] no_hosts = true`), launcher 1.7.1 candidate: INTL login via the open INTLConfig.ini → relay (`session ok`,
  `Login request`), directory `dir.228918463-2-1.dbserver.example.org` (DNS → 127.0.0.1) → DirServer, lobby
  (user: "removed it and logged in"). A match without hosts was not verified on the stand (the stand's battle server itself goes through hosts); on
  prod the match runs on Robot by IP. Done: backend 55d63f5 (`relay.no_hosts` in the manifest), ServerConfig 2693e2c — players
  did NOT have it until 13.11 (prod had 2026.1104.1341, see the 13.11 entry above). **Deploy:** backend on the VM + `no_hosts = true` in `[relay]`
  of /opt/dungeonborne/server.toml (rollback — remove the line); the launcher is not needed (everyone has 1.7.1). Risk: routers with DNS
  rebinding protection will not return 127.0.0.1 for the directory domain — such players will get no server list; the global rollback is the flag.

- [x] **DEPLOYED 13.11 13:53 UTC with part 2: RuServer 0fa3eae, MobBones 67dea54 instead of SeedBones (bones for all mobs within 30 m
  of living players). Robot 14.11: [MOBBONES] works on 7 battle servers, no errors; tree hits on skeletons in players'
  matches were not checked separately.** Before: **Druid trees do not hit skeletons — READY FOR DEPLOY: RuServer SeedBones 1d9774c (WORKAROUNDS K26); stand 12.11 20:53 with the mod (DruidProbe experiment off) — "works". Deploy: RuServer main.lua on Robot (from HEAD, not the working folder — it has other unfinished changes), battle servers pick it up on restart between matches; on "yes"** (12.11, user: "100% miss on skeletons", "it hits, but literally does not
  connect"; NOT DONE). The tree in a match is the pawn BP_TGF_Summoned_Seed_C (AIC_TGF_DruidSeed_C), hit
  GE_TGF_Damage_Instant_Seed_Attack1_C; the tree's team = the druid's team (1), the skeleton — 200, CompareTeams = enemy. Stand
  12.11 20:22, DruidProbe: the skeleton's bones are not updated on the server — pelvis = foot_l = (−4899, 4756, 0) with the actor at
  (…, 90), `VisibilityBasedAnimTickOption` = 1 (AlwaysTickPose — the pose ticks, bones are not updated while the mesh is not
  rendered), `bRecentlyRendered` = false; the skeleton's hitbox is the bones' physics bodies (LOCAL's analysis), the zombie's — a capsule.
  There was one hit on a skeleton nevertheless (392 → 0, 20:22). Hypothesis: on the headless server the skeleton's hitbox is collapsed into a point
  at its feet. DruidProbe experiment fix_bones (b931a4d): from 20:35 for skeletons within 6 m of a tree, once per 1 s `Mesh.VisibilityBasedAnimTickOption = 0` — the tree hits all skeletons (user: "works on all skeletons"; StatWatch: 7 kills 471/392 → 0). Caveats: reading the pelvis/foot_l bones is apparently wrong (no names — the mesh point); there were rare hits before the experiment too; why exactly this way — asked LOCAL. The fix goes into the server mod (RuServer), option: for all skeletons or only near players/trees; set it repeatedly (the monster BP rolls back the cache); WORKAROUNDS.

- [ ] **Auction and Buyout Trades: grey and white items cannot be listed, price limits on both markets** (12.11, user:
  "Yes, do it"). From the tables (tables session): the original markets have no grey items — ROTTED/NORMAL were added by our
  tools/auction_all.py on 06.11; market 1 = Buyout Trades, market 2 = auction (per trade_market.json). Price:
  trade_commodity.price_lower/upper_limit — % of SP (Buyout 50–200%, auction from 10%). Done: backend 3710906 —
  `price_bounds` (SP — weekly average for is_calculate_sp, not lower than price_def x lower; otherwise price_def): Buyout sale
  and buy order — -4210, auction start/buyout — -4261. Grey/white — is_on_shelf=false in the seasonal trade_commodity
  (tables session 85d537e: 63 NORMAL + 24 ROTTED on each market) → -4205. Deploy: backend + trade_commodity.bytes/seasons on the VM, ru.balance to players; on "yes".
- [ ] **Elites: purple chance per kill — a 5/10/15% ladder** (normal/Classic/Hard; user 12.11; was
  ~42/70/82%). Done by the tables session 85d537e: elite groups g088/g101/g114/g125/g136/g149, the share
  of purple per roll 1.70/3.45/5.27%; cross-check 0.051/0.104/0.158 purple per kill; pak zzzzzz_Balance_P 7253c9db. Deploy: pak on
  Robot, loot.bytes + seasons on the VM, ru.balance to players; on "yes".
- [ ] **`/ban` finds a player by open_id, user_id, character name, Discord name and login** (12.11, user: "ban from
  chat does not work either by in-game nickname or by Discord nickname"; 243d822). Before — only open_id and display_name (global_name
  or the server nickname): `/ban Legend`, `/ban izao4ik` → "not found". The Discord login (`Account.login`) is recorded at login —
  for old players, search by login will work after their next login; by character name and open_id — right away. Several
  matches — a list. Tests WSL 156/156 (ban_revokes_sessions_and_kicks extended). **Deploy:** backend on the VM, gracefully, on "yes".

- [ ] **Rogue stealth: the enemy is visible up close — READY FOR DEPLOY** (12.11, complaint "the rogue is invisible even point-blank"; analysis
  docs/ROGUE_STEALTH.md, temporary solution WORKAROUNDS K25). From the shader (LOCAL): `StealthVisibleDistance` is a strength 0..1, not
  a distance; the native code does not set it (0) — an enemy rogue is never visible. RuClient StealthReveal (3a6f116): strength by
  distance every 100 ms (default 0 at 10 m → 1 point-blank), `FarDistanceVisibility` 0, option `solid_distance`.
  Stand 12.11 18:45, two clients: "it works". Settings — from the server (6fed8ab, docs/MOD_SETTINGS.md):
  `[client.DungeonborneRuClient.stealth]` in `content/seasons/base.toml` → the manifest's `client_settings` → the launcher writes
  `Mods/DungeonborneRuClient/server_settings.ini` (launcher afe8b35). Stand 12.11 19:30: the settings came from server_settings.ini (all keys `<server` in the log), the rogue becomes visible — "works". **Deploy (everything on the user's "yes"):**
  1) backend + `seasons/` on the VM; 2) package `ru.client-hooks` to players (RuClient main.lua + settings.ini; make_mods.py,
  manifest `mods/manifest_mods.json`); 3) launcher with `client_settings` (before it players have the default values from the code,
  the same 10 m). RuClient is not needed on Robot for this (the module is off with `-dbserver`).
- [ ] **RuClient settings — `settings.ini` instead of 10 flag files** (12.11, user: "let's do it properly";
  docs/MOD_SETTINGS.md). `ModSetting`/`ModFlag`: server_settings.ini → settings.ini → the old file (transition period) →
  default. `stealth_reveal.txt`, `r14_mode.txt` removed from the repository (`r14.mode = off` in the package's settings.ini).
  Deploy — with the same `ru.client-hooks` package. Afterwards — remove the old files on Robot and the stand, `native_data.txt` →
  `[native] data_url`.

## Hand of Justice — verified on prod (12.11 12:20 UTC)

- The user (owner, …907) in a match on Robot 7807, trio, Castle Hard. The command `ban 76561198000000907` was written directly into
  `C:\DungeonbornServer\Kit\run\dbserver-7807-control.txt` (chat is unavailable in a match, Claude has no admin Bearer) → mod:
  `ban_sent=1`; backend: result `ban_kick` (as death), match over, room cleared; `lost_items 0`, raid XP
  granted (+160). User: "works". The account was not banned.
- [ ] Not verified: the chain `/ban` from chat → LIVE SELECT (RocksDB) → `/ds/slot` cmd → Hand.
- [ ] **`/ban` from chat does not find a player either by in-game nickname or by Discord nickname** (user 12.11; prod: `/ban Legend`,
  `/ban izao4ik …` → "account not found"). Cause: `auth::ban::find` searches only `open_id` or `accounts.display_name`, and
  display_name is the Discord `global_name` or the server nickname (`discord.rs:155`), not the username: for izao4ik it is "Izao"
  (open_id …789, user_id 11000061). The character name is not searched at all. Fix: search by open_id, user_id, character
  name (like `/to`, and not only online), display_name and Discord username; with several — a list. By open_id
  it should already work now (not tried). Implement search
  by character name too (like `/to`), and an admin login without a token on the VM (root-only command) — on "yes".
- [ ] Chat is unavailable in a match — a player in a match can be banned only by a command to the battle server (as above) or from the lobby
  by another GM.

## To verify: bug report and the "SUPPORT" button (14.11, user: "note it for verification")

Verify after DevKit (workspace). None of this is deployed.

- [ ] **Backend** `18ef2c1` (backups, `core/backend/src/bug_reports.rs`): POST/GET `/api/launcher/v1/bug_report`, admin API for the bot
  (`/admin/v1/bug-reports…`), limit 3 per hour. Tests 175/175 in WSL. Not deployed; not tested live with the bot.
- [ ] **Launcher** `f109fc0` (`<data>\launcher`): "Report a bug" button, form, log upload
  (up to 9 MB), waiting for the post, retry after a 401/429 failure; heartbeat `Saved\launcher_alive` and pickup of
  `Saved\bug_report_request.json`; window after a game crash. e2e on the mock and a run against the local backend — green.
  Version 1.7.5 unchanged, not deployed to players.
- [ ] **Pak `zz_SupportButton_P`** (LOCAL's draft: `<data>\research\support_button\zz_SupportButton_P`):
  a wrapper over `CustomerService:Open` (the native `CustomerService_Orig.lua` next to it, byte for byte). Move to
  `backups\zz_SupportButton_P`, add to the mod catalog (first the `dev-client` profile), build with `build.ps1`,
  `cargo xtask mods check`.
- [ ] **Stand** (only with the user's consent):
  - "SUPPORT" in the ESC menu and on the login screen → the form in the launcher (source=game, scene);
  - the launcher window comes on top of the fullscreen game;
  - daily quests open (they call `GetUserCareer` from the original);
  - without the launcher — the toast "Open the launcher…" and a browser at `https://dungeonborne-community.com/#start`;
  - in game `os.rename`, `GetCurrentLanguage`, the path to RuClient's `server_settings.ini` work (log
    `Saved\Logs\SupportButton.log`);
  - the game exited with an error → the launcher offers a kind=crash report.
- [ ] **Test Discord Sirius.ai** with the bot (`apps/discord-bot`, section `[bug_reports]` in bot.toml): post in
  🐛・bug-reports, header "game (Setting)" — check that the backend returns `scene` in the admin API.
- [ ] The user's decision: whether to disable Tencent CrashSight (`CrashSight64.dll` sends crashes to
  pc.crashsight.wetest.net).
- [ ] Deploying the backend, the launcher, the pak and enabling the bot on the live Discord — only on the user's "yes".
