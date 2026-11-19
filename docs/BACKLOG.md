# Backlog — Dungeonborne emulator (Rust backend + client mods)

Format: `[ ]` open, `[x]` done. Each task says where to look and why. Date — when found/closed.

## Open

### Dedicated (headless listen server, GKE + Hetzner)
- [x] **Double projectiles (DK E hook, sword wave) — server pak `zzzzzzz_ServerNet_P`, verified in a match 03.11:**
  the server has one `B_Hook` per cast without `B_Hook_Predict`; the wave (`B_TGF_Normal_Projectile_SwordRelicIce`) — one
  copy each on the server and clients; no visual duplicates (user). For Hetzner — the full Robot kit `dist\dungeonborne-robot-20261103.zip`
  (README inside; paks, mods, install/start/killmutex), a separate `gameserver-servernet-20261103.zip` — pak only
  (`install_server_paks.ps1 -GameRoot …` with the game closed); not yet installed on Robot.
  Cause (static analysis): BP has `IsDedicatedServer` branches, a headless listen answers false and takes the client branch —
  `GA_TGF_DeathKnight_E` also spawns `B_Hook_Predict` on the server, `B_TGF_Monster_Track_Bullet_Base` does not
  home on the server. The pak (`game\bp-patches\pak_server`, `tools\patch_isserver.py`) swaps the import to `IsServer` in 5 assets:
  DK E, `B_Hook`, `B_TGF_Normal_ProjectileBase`, `B_TGF_Monster_Track_Bullet_Base`, `B_TGF_Track_Projectile_WizardFireEye`.
  Installed only in the Steam install (server); clients (C:\Games\Dungeonborne, E:) — without it. The server with the pak starts
  (03.11), not verified in a match. Sword wave: no `IsDedicatedServer` branches (`GA_TGF_SwordMaster_E` — only
  `K2_HasAuthority`) — look at the `[PROJ]` lines of the client mod (`crash_ue4ss.log` of the server and clients).
  For Hetzner: add the pak to the server build (dist not touched).
- [ ] **Match pipeline backend ↔ server ↔ clients — design `docs/DS_PIPELINE.md` (31.10), tasks T1–T8.** Main point:
  for `tgf.DsRpcService` (`init_player`, `heartbeat`, `query_player_game_status`, `player_self_quit`, `player_kickout*`)
  the direction is backend → DS, but `ds_handlers.rs` implements them as incoming; `DsBattleService` (`player_game_over`,
  `ds_end`, `player_online_*`) — correct. The DS SDK is alive in the server process but does not connect to DSA → bridge via ds-agent
  with the same proto messages. Raid results in DS mode — only from the server; a single `DBattleUserData` builder.
  - [x] T1 (`core/backend/src/ds/`: HTTP ds-agent 9091, outgoing init_player, driver) and T3 (room stages, room events
    per the reference `docs/REF_BATTLE_ROOM_FLOW.md`) — 31.10, build/test ok, **not verified in game**, ds-agent not written.
  - [ ] Match: `notify_matched` sends `match_status=4` (not in the enum), reference — `CAN_JOIN_IN=8` + event `matched`
    with roomData/dsAddress, then `FINISHED`. Change together with `WBP_MatchMainButton.lua`.
  - [ ] DS service formats are not checked against anything but proto: they are absent from the sniffer logs (DS ↔ their backend traffic).
  - [x] **31.10: native match passed LIVE** (user, 2 windows F + E, `tools\stand\play.ps1`): native button →
    search → room → both on the headless server. Abilities work as they should (listen-host bugs are gone), each has
    their own class and gear with affixes. The button was blocked by two `player_get_info` fields (account_handlers.rs, per
    reference sniffer_log_001): `user_matching {matching_heartbeat_interval 1, battle_heartbeat_interval 5}` and
    **`user_team` (empty for a solo player)** — without them search was not sent after pre_search ("Cannot be used during
    match search"). The commented-out block "DO NOT UNCOMMENT" was left untouched. Window E — third Goldberg account
    (`C:\Games\Dungeonborne\…\Save_Profile_3`, 76561198000000003, backup `C:\Games\Dungeonborne\_backup_20261031`).
    Found in the live match — open:
    - [~] **31.10 done: base role attributes.** backend writes `role_affix.attribute` as the 11th loadout field
      (`raid_handlers::encode_player_loadout`), LocalDev applies it via `AttrIds.lua` (id → set/property, generated
      from battle_attribute_description + CXXHeaderDump) after InitFirstLevel. Server probe: Str 23 Dex 8 Sta 9 Int 19
      Wil 6 Fai 6 MaxHPBase 557 (was 30/100). Reference for the same DK in a local match: Str 37 Dex 18 Sta 15 Int 25, HP 1018 —
      **gear bonuses are missing**. Cause — the R14 stub on the server: LocalDev equips items onto it
      (GetComponentByClass), the native `TgfEquipmentManagerComponent` is empty; with the stub the native equip is rejected,
      without the stub on the server (r14_mode=off) the native one accepts (WornSlots=9), but the client crashes in UE4SS.dll+0x3e881b
      (4 of 4). → a real R14 fix is needed (no stubs, the avatar waits for the native manager) — it will also give gear stats.
    - [ ] **R14 "defer" (31.10, not ready, r14_mode rolled back to all)**: no stubs; on the client the UCS component
      `ChildActorComponent_<n>` gets ChildActorClass=nil, restored via `SetChildActorClass(BP_Avatar_Default_C)` when
      the manager arrives (`PendingAvatars`, LoopAsync). Result: the server is stable, but the client still crashes at
      0x142f602c6 (nulling this component does not stop the avatar — the body is created another way; the probe of the hero's
      component list has not fired yet: K2_GetComponentsByClass returned a table, fixed, not verified), and 3× a new
      crash UE4SS.dll+0x2aee17 (AV 0x18). Lessons: in NotifyOnNewObject you must not read properties of a half-built object
      (reading ChildActorClass crashed the server) or call FindAllOf — IsServerWorld now uses the GameMode map cache.
    - **31.10 body creation timeline (BODY_TRACE in DungeonborneRuClient):** for the pawn owner (local match,
      server) the native manager `…TgfEquipmentManagerComponent` (name without a number) is created 2 ms after the pawn,
      BEFORE UCS → no crash. The client has none at all: the server's manager arrives over the network in the same packet (`_<n>`),
      is created before the avatar, but is not yet registered on the pawn → the avatar (which creates the UCS component
      `ChildActorComponent_<n>`) gets null. `InitGameMap` does not open the map (only), with StartLocalBattle (before)
      changes nothing (IsOptionArgsInit=false). r14 mode `client` with cached IsServerWorld — crash UE4SS.dll+0x3ecee5
      at body creation. Conclusion: Lua/UE4SS is fragile at this boundary — look at a BP patch (retoc) or another tool.
    - **31.10 BP_AvatarActor patch did not help — the crash path is native.** The patch (`game\bp-patches\tools\patch_avatar.py`:
      `UpdateMesh` in ReceiveBeginPlay only when `IsServer`) is built and CFG-checked, but with r14=off the client crashes at
      the same 0x142f602c6 (pak removed, lives in `<data>\dbtools\disabled_paks`). UE writes to
      `ue-client-7777.log` an unwound stack: main loop → UE4SS tick hook → `UWorld::Tick` → ~11 engine frames
      (network packet receive) → `0x14538eaa4` (looks like UFunction::Invoke) → LyraGame `0x142eed770` →
      `0x142eeecc0` → `0x142ef0ac0` → `0x142f17ec0` → `0x142f62be0` → `0x142f601a0` (loop over 5 armor slots). No
      BP VM frames: the body is updated by a native RepNotify/client RPC, a BP patch cannot fix this. The exe code on disk is packed,
      decrypted in memory on demand; `ReadProcessMemory` on the game — ACCESS_DENIED (SeDebugPrivilege from the
      Claude session is forbidden by auto mode). The function table (.pdata) and "name → exec-thunk" pairs are read from disk
      (`scratchpad\mdstack.py`, `refs.py`).
    - **31.10 client GameFeature is fine (hypothesis "no own manager" dropped).** The manager is added by
      `GameFeatureAction_AddComponents` from `LAS_TgfGame_StandardComponents` (`BP_Tgf_Hero_Default_C →
      TgfEquipmentManagerComponent`, bClientComponent=1, bServerComponent=1). Client probe: Experience
      `B_Tgf_Game_Experience` loaded, client components of the action set are in place with stable names
      (`TgfInGameServiceComponent`, `PhysicalAnimationComponent`, `B_TgfGameFollowUIManagerComponent_C`, on the controller
      `BP_TeammateMarkerManagerComponent_C`), while REPLICATED components (equipment/inventory managers,
      `TgfLyraInteractComponent`) — only `_<n>`: GameFrameworkComponentManager creates replicated components
      only on authority, the client receives them over the network. This is normal in the official game too. So R14 is about ordering: the native
      body RepNotify/RPC arrives before the manager is ready.
    - **31.10 "LocalDev equipping" is not to blame (verified).** LocalDev got `settings.txt equip_delay_sec=N`
      (default 0 — as before; the hero is equipped N s after appearing on the server). With r14=off and a 10 s delay
      the client crashed at 0x142f602c6 4 s after `open`, while the server had not equipped anything by then (no
      entries in `equip-debug.log`). The crash comes from the native hero arrival itself. Next — find which event (OnRep/RPC)
      updates the body: UE4SS pre-hook loggers on the candidates (`UTgfAvatarHelperComponent:OnRep_*`,
      `UTgfCharacterAvatarComponent:OnRep_Avatar`, `LyraPawnExtensionComponent:OnRep_PawnData`, `Pawn:OnRep_PlayerState`)
      writing line by line to a file, r14=off.
    - **31.10 OnRep trace (DungeonborneRuServer, `dbclient-reptrace.txt = 1` → `dbclient-reptrace.log`, 27 hooks):**
      on hero arrival only `LyraPlayerState:OnRep_PawnData/OnRep_MyTeamID` fired (entered and exited); not a single
      hero/body/equipment OnRep before the crash. Crash — ~2 s after the hero appears on the client in 5 runs of 6
      (11:36:46→48.5, 16:21:04→06.5, 17:35:20→22.4, 18:01:46→48.8, 18:09:52→54.5) — looks like a timer, not a packet.
      In the defer runs the mod looked for the manager on the hero every 100 ms for 2 s — never found it. The main question now:
      **does the server's native manager (GameFeature, 9 items) reach the client at all** (the probe writes `rep=` —
      GetIsReplicated). Mode `client` (stub only on the client) crashed in UE4SS.dll+0x3e881b (a mod Lua callback read
      an object) 5 s after the hero, before the probe — gave no answer.
    - **31.10 CONFIRMED: the server does not deliver the hero's equipment and inventory managers to the client.**
      Server (probe `dbserver-hero-probe.txt` on the client's hero appearance): the hero has native `TgfEquipmentManagerComponent`
      and `TgfInventoryManagerComponent` (GameFeature, stable names), the manager holds 9 items. Client (object creation log
      in `dbclient-reptrace.log`): the hero and its CAC are created, but manager objects do NOT EXIST AT ALL — neither 2 s before
      the crash nor after the "wake-up". The managers' flags are the same as those of the controller's `TgfLyraInteractComponent`
      that did reach the client and of the attribute component: bReplicates=true, bReplicatesRate=1 (a Dungeonborne-specific field in
      UActorComponent), bNetAddressable=false, bReplicateUsingRegisteredSubObjectList=true. Hero: NetDormancy=2
      (DORM_DormantAll), NetUpdateFrequency=30, ForceAlwaysRelevent=true, DisableDistanceSlowReplicateRate=true,
      PS.HasInitedPlayerData=true. Did NOT help (server, `dbserver-fixrep.txt`): 1 — SetIsReplicated(false/true) +
      FlushNetDormancy + ForceNetUpdate + the old subobject path on the instance; 2 — RegisteredSubObjectList=false in the
      CDO of the hero and managers before the client joins; 3 — setting the hero/player "filters". Open: why exactly these
      hero components are not sent (a native filter in the hero's ReplicateSubobjects/AllowActorComponentToReplicate?
      a component replication condition? DormantAll?). Check: does a stub created on the server via
      AddComponentByClass (mode all) reach the client — if yes, the difference is in how the component is created.
    - **31.10 mode all, client probe +10 s:** the client's hero has 3 managers (Dungeonborne names go downward): `_832`
      (0 items, the client's own stub right after the hero `_833`), `_825` (**9 items — the server stub from
      AddComponentByClass arrived**), `_816` (0 items — **the server's native manager arrived too**, last). So the native
      manager is replicated, but ~2 s LATER — after the body timer, which is what crashes in mode off. The question now:
      why the native (GameFeature) one arrives late while the one created by AddComponentByClass arrives fast. Fix options: on the
      client — keep the stub only until the native one arrives (and don't let it intercept GetComponentByClass);
      on the server — speed up sending the native one. The first all run crashed in UE4SS.dll+0xba20e4 right after
      AddComponentByClass in NotifyOnNewObject (known stub instability).
    - **31.10 r14 mode `temp` (DungeonborneRuClient, code kept, off by default):** stub only on the
      client, removed 1 s after the native manager arrives (`TempStubs`, log `[R14_TEMP]`). Server without
      a stub — the native manager got 9 items (what is needed for stats). But the client crashed in UE4SS 2 of 2
      (+0x3ecee5 after 3 s, then +0xba20e4 after 2 s, after disabling `[RES +3s]`) — before the stub was removed.
      Conclusion: a stub created by UE4SS on the client is unreliable (in all it crashes rarely, in client/temp — almost always).
      Next step: the same client stub without UE4SS — patch the UCS of BP_Tgf_Hero_Default (`if not HasAuthority:
      AddComponentByClass(TgfEquipmentManagerComponent)`) via bp_patches; server without a stub.
    - **31.10 hero UCS patch (`game\bp-patches\tools\patch_hero.py`, pak `research\bpfix_pak_hero`, removed from installs to
      `dbtools\disabled_paks\hero_*`):** `if not HasAuthority(): AddComponentByClass(TgfEquipmentManagerComponent)`.
      Works: on the client the manager already exists when the body is created, **the 0x142f602c6 crash is gone** (r14=off, no
      UE4SS stubs). Server without a stub — native manager with 9 items. New problems:
      1) with DungeonborneRuClient enabled the client crashes 2/2 in UE4SS.dll+0x3e881b after ~5 s (when the
      real gear arrives; moving the weapon UseHitValidation write to the next tick did not help) — without the mod it does not crash;
      2) the client sends its stub to the server (the class is replicated) → server: "ReadContentBlockHeader: Client
      attempted to create sub-object" → connection drop after ~4 s;
      3) GetComponentByClass on the client returns the empty stub → "GetCurrentWeaponSlot … Invalid, EquipWeapons: empty".
      Next: make the stub non-replicated (bReplicates=false on the CDO on the client or SetIsReplicated(false) in the UCS patch) and
      remove it when the native manager arrives (hook on the native OnRep_EquipmentList/WeaponSlotList → destroy
      the stub); find what in DungeonborneRuClient crashes on real gear (or don't load it on clients).
    - [ ] **02.11 the "create sub-object" drop is back — fix awaits verification.** 08:35–08:52 all clients dropped
      ~2 s after the hero appeared (01.11 — not once). In the second install the native manager arrived (`r14-stub.log`
      08:35:32), but in the same frame — "GetCurrentWeaponSlot … Owner:[]" (stub) and the drop: R14 removed the stub only
      on the next tick, and the native OnRep managed to call listeners. Now removal happens right in the hook (before the OnRep body).
      What changed since 01.11 is not proven (candidate — 32-bit item guids since 02.11). Steam install — server:
      R14 is off there and the Goldberg ID is the same as the host's — do not run it as a client.
    - **31.10 follow-up (did not help, all removed):** patch_hero.py now does `Context(AddComponentByClass(...))
      .SetIsReplicated(false)` (SetIsReplicated import added); mini-mod `game\ue4ss\Mods\DungeonborneR14` (hook on
      the native OnRep_EquipmentList/WeaponSlotList → remove the hero's other managers; disabled in installs,
      `enabled.txt.off`; log `RebuildProbe\r14-stub.log` shared by server and client). The client without DungeonborneRuClient
      does not crash, but the server still drops the connection after ~4 s: "Client attempted to create sub-object" — the client calls a
      server RPC on the stub (preceded by "GetCurrentWeaponSlot … Invalid"), and the server does not know such a component.
      The native manager has not yet reached the client by then (there is no removal line from the client). So the stub on the
      client is a problem in principle: its RPCs go to the server. Pak — `research\bpfix_pak_hero`, removed copies —
      `dbtools\disabled_paks\hero2_*`.
      Further ideas: (a) understand why the native manager takes >4 s to reach the client (the server sends the hero DormantAll; compare
      with the AddComponentByClass stub, which arrives fast) — then the stub is not needed at all; (b) on the server
      create the hero manager via AddComponentByClass instead of GameFeature (since that one arrives fast).
    - [x] **"Floating" crash UE4SS.dll+0x2aee17 (AV 0x18) — it is the DungeonborneRuServer probe**: reading
      `ChildActorComponent.ChildActorClass` (TSubclassOf). Found by line-by-line probe writes (`<role>-probe-live.txt`),
      section removed; `K2_GetComponentsByClass` also replaced with targeted `GetComponentByClass`.
    - [x] RMB: the charge-release workaround only for the listen host's own pawn (`IsListenHostPawn`); souls on the server
      are no longer hidden (the hiding was replicated); hero hooks (UCS/ReceiveTick) are installed once. Not verified in game.
    - [ ] Stats 30 / no substats (health, damage, speed at defaults): LocalDev on the server
      (`InitCharacterAttributesAndResources`) takes `FirstLevelConfig`/`CurveLevel`/`CopyAttrValueFromASC` from the
      HOST's pawn, and the host is a spectator without a pawn → `InitFirstLevel` does not compute, 30 and HP 650 are set. Fix: load
      `/Game/TGF/Ability/Data/FirstLevelAttr.FirstLevelAttr` (the only UeSource_FirstLevelAttrDataAsset row),
      CurveLevel — role level; the right way — player data from the backend (S4, `InitPlayerGameData`).
    - [ ] RMB (charged attacks) is cancelled immediately — the charge-release fallback of `DungeonborneRuClient` (watches LMB)
      also runs on a clean client; listen workarounds (charge, Relic, souls) are no longer needed on the client — turn them off.
    - [ ] Knockbacks — tick/FPS of the headless server: measure (probe), set t.MaxFPS / net tick rate, check
      Windows background process throttling.
    - [ ] The death zone deals no damage: `IsDedicatedServer` in BP_DeathZone only in BroadCastDeathZoneMessage, damage
      is behind HasAuthority — look at the mod changes (`AliveFighterNumber`, MAP_RULES) and GameMode.
    - [ ] Button/search state in the UI (animation, cancel) — compare match pushes with the reference from the probe log.
    - [ ] "Could not reach the other player" — clarify (PS teams: Team=1 and Team=2; COOP_SAME_TEAM).
    - [ ] The server is held by the room until TTL (no ds_end) — for backend sessions with DS_ROOM_TTL=90.
  - [x] **31.10: native match passed without a human** (`dbtest\run.ps1 -Match native`): `MatchService:StartMatch` →
    pre_search/search → room → CAN_JOIN_IN → the driver pushes room events 1–4 → FINISHED → the client goes to the DS itself
    (`connected` in 24 s). What was needed: pushing room events without a client request (`ds::driver::pump_room_events`):
    the reference sends them on its own, without them the client does not go to `battle_room_heartbeat`. The button in normal mode (`-Match local`,
    StartLocalBattle) — works. Clicking a UMG button from UE4SS is impossible (`OnBtnMatchClicked` — UnLua Lua), the mod calls
    what it ends up calling. Mod `DSTest` (log "Кто-то проверяет IsRoomReady!") is off — enabled.txt removed.
    Floating crash UE4SS.dll+0xba20e4 on pawn creation/removal (twice a day, server and client) — investigate.
  - 31.10 automated run with T1/T3 and the orchestrator's match changes: login, match, client connection — no regressions.
    The run does NOT check the battle room: the client joins with a console `open`, nobody sends `battle_room_heartbeat`.
    Needed: a client mode "native match" (`match-mode.txt = native`, the mod clicks the match button, the client travels on its own
    on `dsInitFinish`). The ClientProbe/LuaProbe probes broke login (heap, NotifyOnNewObject) — turned off, the orchestrator is fixing it.
- [x] **Headless `DungeonborneRuServer` works: from launch to `ready` ~18 s without a human (headless, -nullrhi).** (30.10)
  Mod `game/ue4ss/Mods/DungeonborneRuServer` (only with the `-dbserver` flag), launch `dbserver\start_headless.ps1`
  (`-Map`, `-Delay`, `-NoObserver`, `-Window`, `-Quit`). Chain: hooks `WBP_LoginWindow_C:OnEnterState*` →
  in `WaitingStartGame` +3 s click `BndEvt__…StartGameButton…` (on error — OK in `AlertWindow` and retry) → `IsLogin`
  → lobby → `StartLocalBattle(FName(map?listen))` → spectator (`bOnlySpectator`, `IsSelfQuit`, `DestroyCharacter`)
  → `ready`, UDP 7777. Mod log: `RebuildProbe\dbserver.log`, status `dbserver-status.json`. Memory ~3 GB per instance.
  `-Quit` verified at login and in a match (a normal quit, the process exits on its own).
  Facts 30.10:
  - the exe restarts itself under ACE: arguments get through, environment variables do NOT → settings only via flags.
  - a process under ACE cannot be killed (`taskkill /F` → Access denied) → exit on its own: `-Quit` (console quit, `os.exit` after 10 s).
    Console quit once hung the process (52332, after connect failed) — the `os.exit` fallback has not been needed yet.
  - `StartSpectatingOnly` cannot be called from UE4SS (not a UFUNCTION) — `bOnlySpectator` + `DestroyCharacter` is enough.
  - second instance: "Not Support Starting Multiple Game Instances"; the user worked around it by closing the mutex.
  - UE4SS.log is overwritten at startup (shared between instances) — the script archives it to `%LOCALAPPDATA%\DungeonborneServers`.
    The UE log in shipping — errors only; `-LogCmds="… …"` (quotes with spaces) hangs the launcher.
  - `StartLocalBattle` without login: sends `battle_fetch_battle_user_data`, timeout -101 → gateway login is needed.
  - UE4SS: an `FName` parameter must be passed as `FName(str)` (a string crashes UE4SS); `FindFirstOf` may return `Default__`.
  - `ULoginWindow::StartGame()` called directly crashes (AV 0x60) in any state; the button click handler works.
  - `connect failed 201 ext:-6` (before TCP, the gateway sees no attempts) happened in all launches after crashed/hung instances,
    with and without a window, with and without `-port`; the first launch after a clean `-Quit` logged in on the first click. Looks like
    ACE/GCloud state after an abnormal exit; manual login also failed on the first attempt and succeeded on the second.
  - Second client for tests: `C:\Games\Dungeonborne` (30.10 the same paks applied, PVE paks `zz_Solo*_9_P` moved to
    `Content\_pve_disabled_paks`; its own Goldberg Steam ID 76561198000000903). RebuildProbe is shared by the server and client.
  - [x] The client from `C:\Games\Dungeonborne` (mutex removed manually) connected to the headless server — works (30.10, user;
    minor bugs unrelated to dedicated).
  - [ ] Not verified separately: gear handout
    to other players without LocalDev (it starts only from the lobby — its `EquipRemoteClients` logic does not run on the server?).
- [ ] **Lobby in the backend (`core/backend/src/lobby/`, handlers `core/backend/src/rpc/handlers/lobby_handlers.rs`) — built, not verified in game.** (30.10)
  - `lobby::hub` — session registry open_id → channel; the gateway now uses `select!` (socket + pushes from other players).
  - `lobby::matchmaking` — tickets (solo/whole party) on a `BinaryHeap` per level_id, 500 ms tick; a room starts at
    `MATCH_ROOM_SIZE` (3) or when the oldest ticket has waited `MATCH_WAIT_SECS` (15). Match for everyone — push `battle_matching_log_sync`
    (status 4 + room_key); `battle_room_heartbeat` returns the members and `DsInitFinish` with the assigned server's address.
  - `lobby::allocator` — static pool `DS_POOL="ip:port@/TgfMaps/...;ip:port@*"` (default `127.0.0.1:7777@*`),
    the server is held for `DS_ROOM_TTL` (45 min). Rooms not from the matchmaker — `DS_DEFAULT_ADDR`.
  - `lobby::team` — party: invite/approve/reject/exit/remove/transfer_captain/ready, snapshot `dirty_team_snapshot` to everyone.
  - Fakes removed: `p2p_room` in search, "match after 2 heartbeats", push `create_game_session_result_event`.
  - [ ] Check: does the client reach the server via `DsInitFinish` (native button mode); party UI from the snapshot.
    **On hold (30.10):** the native button (original bytecode `WBP_MatchMainButton_Orig`) sends only `pre_search`,
    `search` is not sent ("NativeStartPreMatch already in matching"). Reference — `sniffer_log_001.txt` (server 198.51.100.1):
    there an empty `pre_search` reply and right away `search`. In native mode the wrapper does not start LocalDev/Private* on the client.
    31.10: match pushes aligned with the reference (startMatching → matched CAN_JOIN_IN=8 with dsAddress → finished; it used to be
    the nonexistent match_status=4) — repeat the native check (docs/REF_BATTLE_ROOM_FLOW.md).
  - [x] Archive for Hetzner (Windows Server): `dist\dungeonborne-hetzner-20261030.zip` — release backend + game
    server kit + scripts `start_backend.ps1`, `install_gameserver.ps1`, `start_headless.ps1`, README. (30.10)
  - [ ] Native rooms `room_*` (create/enter/ready/start_battle, `dirty_private_room_snapshot`) — built 30.10,
    not verified (`core/backend/src/lobby/room.rs`, `room_handlers.rs`; see docs/HANDLERS_STATUS.md).
  - [ ] ds-agent: launching servers, mutex, registration in the allocator, release when the match ends.
- [x] `make_llm_prompt.py` outputs only context: `context/<keys>.md`, flags `--ue` (CXXHeaderDump), `--gamedata`, `--stdout`. (30.10)

### Match / listen host (`host_from_lobby`, `?listen`)
- [ ] **Heavy/charge in listen: verify the `TellServerRelease` fix.** (30.10)
  Diagnosis: the ASC receives the release (`Spec.InputPressed=false`), `ServerSetInputReleased` does not help
  (`ChargeReleased` stays false) → in listen `GA_TGF_MeleeWeapon_Charge_Base` has no `WaitInputRelease`:
  the BP on the server waits for `TellServerRelease` from the client (as on the official dedicated), and on the listen host there is no client branch.
  Fix in `game/ue4ss/Mods/DungeonborneRuClient` (`CHARGE_RELEASE_MODE="rpc"`): 0.15 s after LMB up, if it has not
  released on its own, we call `ability:TellServerRelease(held)`. In standalone the mod does nothing.
  Verified 30.10 (listen, Relic+sword): the hit goes through (`ChargeReleased=true`), but there is NO post-effect — the frost wave
  (`B_TGF_Normal_Projectile_SwordRelicIce`, spawned by `Charge_Enchant_Base.ProcessProjectile`). Apparently it is launched by the
  client release branch. Trace local vs listen (`CHARGE_TRACE`): natively `OnRelease_EA7A…` → `ProcessRelease` +
  `TellServerRelease`, then `Enchant.DealCastCharge` receives `EventReceived_F72DFA85` → `ProcessProjectile`; in listen
  this event does not happen. With the `OnRelease_EA7A…(held)` callback the release in listen is identical to standalone, but there is still no wave —
  the event is sent by the "client" branch of `DealCastCharge`, which does not run in listen. Fix: if we did the release and
  `Enchant.DealCastCharge` passed without `ProcessProjectile` — call `ProcessProjectile()` (awaits verification).
  More native: capture the event tag `EventReceived_F72DFA85` in localbattle (the trace writes `tag=`) and send this event.
  Afterwards — turn off `CHARGE_TRACE`.
- [ ] **Other abilities on `WaitInputRelease` in listen.** (30.10)
  The same break may affect `GA_TGF_Staff_Laser`, `GA_TGF_ADS` (aim), `GA_TGF_MeleeWeapon_Shield_Base` (block),
  `GA_TGF_Charge_Skill_Base` (chargeable skills), `GA_TGF_MeleeWeapon_AttackWithDefence_Base`, `GA_TGF_BrightTorch_Base`,
  `GA_TGF_InteractWithMap`, `GA_Hero_Jump`, `GA_TGF_AdditionalRecoverAmmo` (all declare `OnRelease_*`, see DumpGame).
  Check in listen; add to `ChargeAbilityClasses` if needed (the button may not be LMB).
- [ ] **Dual daggers (rogue): after a heavy in listen the character "breaks"** — stops attacking and switching weapons. (30.10)
  Logs 30.10: in localbattle the dual dagger heavy is native (after ~1 s `ExceedMaxChargeTime`). In listen —
  NOT A SINGLE `CHARGE_START`: the heavy does not even activate, our fallback did not fire → another ability breaks it.
  F7 reworked (active abilities only, by string) — needs a repeat in listen after the "breakage".
- [ ] **Items in the lobby: role_wear / role_take_off / role_weapon_set_swap / inventory_item_move /
  inventory_item_swap — await verification.** (30.10) They were not implemented (the server replied with a stub — actions were not
  saved). `core/backend/src/rpc/handlers/inventory_handlers.rs`: moving by guid between the stash (new field
  `PlayerModel.stash`, 8 pages 17×10 in `player_get_info`), backpacks and worn gear; occupied spot — swap; error —
  rollback; push `dirty_postion_action` as in PVE; tests `cargo test inventory_handlers` (4). Not done: two-handed
  weapons (`secondary_weapon_position`), splitting/merging stacks, checking cell and slot sizes (we trust the client).
- [ ] **The net mode flag breaks abilities on listen — building a map.** (30.10) Root of the relic wave,
  souls, charge release: BPs do `if HasAuthority and not IsStandalone: return` (or `K2_HasAuthority`,
  `GetNetMode`), designed for dedicated; the listen host for its own pawn is both server and local player, the client
  branch does not run. `DungeonborneRuClient` `NET_TRACE=true`: hook on `KismetSystemLibrary:IsStandalone` and
  `GameplayAbility:K2_HasAuthority` (BP calls through UFUNCTION — UE4SS catches it; native C++ calls are not caught), log
  `[NET …] netmode=… caller=<class> locallyCtrl=… hasAuth=… -> <answer>` deduplicated by class+function+mode. Capture in
  localbattle (standalone) and host_from_lobby (listen), the answer difference = cut branches. Then: one
  IsStandalone hook for the own pawn vs targeted branch calls (an asset patch broke charging with a 2nd player).
  **Decision 30.10: instead, "listen as dedicated"** — a headless server on the host PC (`-dbserver`, spectator),
  the host connects to it as a client. Static map: `<data>\research\NETMODE_MAP.md` —
  `IsStandalone` in 24 assets (incl. the base `GA_TGF_Base/Active_Base/Skill_Base/Combo_Base/Charge_Base`),
  `IsDedicatedServer` in 17 (`BP_TGF_PlayerState`, `BP_Tgf_Monster`, projectiles, `GA_TGF_Rebirth`…) — a headless
  listen server answers false to it → a hook = true is needed in the `-dbserver` process. No `GetNetMode`/`IsServer` in BP.
- [ ] **Automated runs without the user (phase 0).** (30.10) Scheduler task `DbTools` (installed by the user,
  `backups\dbtest\README_admin.md`): launching server/client and removing mutexes on request
  `<data>\dbtools\request.json`. The first launch did not see the request: Claude Desktop is MSIX, new folders
  in `%LOCALAPPDATA%` are virtual for it → the request, UE logs and mod files (`-dbroot`) moved to `G:\…\dbtools`.
  **Needed from the user:** copy `dbtools.ps1` to `C:\ProgramData\DbTools` again (admin console).
  Mod `DungeonborneRuServer`: client role `-dbclient -dbconnect=ip:port` (login → lobby → `open` → `connected`),
  files `<role>.log/-status.json/-control.txt` — not verified in game. Port lock between sessions:
  `backups\dbtest\portlock.ps1` (the orchestrator agreed). Backend copy for research: `research\backend`
  (snapshot of `src` with 185 handlers, its own target `research\target`, DB copy).
  **31.10: works.** The Claude session runs as administrator; hosts switched to 127.0.0.1 (copy
  `<data>\dbtools\hosts.backup`, Custom Server block commented out). `dbtest\run.ps1 [-Hold N] [-FakeDs]`:
  server ready ~20 s, client connected ~40 s, probe, quit, logs and minidumps in `dbtools/runs\<time>`.
  A hung instance is removed with `taskkill` (works as administrator). R14 run results (`r14_mode.txt` in
  `Mods\DungeonborneRuClient`: all | client | off):
  - `off`: client crash confirmed by a minidump — `0x142f602c6`, read of 0x0, R14=0, RDX=0xB (UpperArmor), right
    after a dynamic `ChildActorComponent`.
  - `all` (2 of 2 without a crash): on the server the player's pawn has the native `TgfEquipmentManagerComponent` (stable name) +
    a stub, and `GetComponentByClass` returns the STUB — server logic sees an empty manager (candidate for
    "disappearing damage"). The client has 3 managers on its own pawn (its own stub + the server one, replicated + the native one?).
  - `client` (stub only on the client): 3 of 3 client crashes on avatar creation (2× in UE4SS.dll, 1× in the game
    0x1431d569f). Looks like client stability depends on the server stub that replicates to it. Investigate.
  - Overriding `IsDedicatedServer = true` in UE4SS works (probe), but the server crashed in UE4SS.dll when the client joined (1 run).
  - `NET_TRACE` is off: the UE4SS 3.0.1 post-callback does not give a reliable function result.
  - The DS layer is alive in both processes: `BattleDsUtility` (`IsOptionArgsInit=false`, server `GetCurrentUserCount=1`),
    `DSSDK`/`DssdkSubsystem` are created. There were no calls to the backend DS service (`hello_world`).
  - The server calls `GetDefaultPawnClassForController` once a second (log `[GM_PLAYER:PreGetDefaultPawnClass]`) —
    apparently the host spectator's controller is trying to respawn.
  - Comparison with the Lyra source (`<data>\Lyra`, 31.10): the equipment manager replicates by default
    (the server stub reaches the client — explains "all is stable, client crashes"); the pawn is spawned deferred,
    GameFeature components are added after UCS; the spam is the loop `FailedToRestartPlayer` → `RequestPlayerRestartNextFrame`
    while `PlayerCanRestart` is true. Next R14 fix: on the client don't let the avatar be created before the native
    manager arrives (clear `ChildActorClass` before UCS, restore `SetChildActorClass` on the manager's `NotifyOnNewObject`) and without
    the server stub.
- [ ] **Damage disappears or ×4 — data needed.** (30.10) F7 now writes `[F7 ATK]` (weapon damage, bonuses, crit,
  primary stats) for each hero. Suspicion: LocalDev `InitCharacterAttributesAndResources` for joined players —
  `CopyAttrValueFromASC(HostASC)` (host attributes) and stats = 30 if 0.
- [ ] **Joined players got the same class (all DK, no souls) — fix awaits verification.** (30.10) LocalDev on
  the host took the loadout of all clients from a single `client-loadout.txt` (the server rewrote it on `player_get_info` /
  `hall_set_role` of any player; it contained the DK of account 10000903). Now the server also writes `client-loadout-<user_id>.txt`,
  LocalDev picks the file by `PlayerState.GetUserId()` (otherwise from `GetOpenId()`: 10 000 000 + last 6 digits), log
  `equip-debug.log`: `Loadout Char=… <- user N` / `<- общий …`. If the user_id of another PS is empty — the mapping
  will not work (check). LocalDev does not set class attributes for joined players → `RefillHero` sets the base
  class resource maximum if it is 0 (mage/priest 300 mana, DK/lich 100 souls, druid 120 energy).
  Pak installation — `deploy_paks.ps1` with the game closed.
- [ ] **deadmoney mode — awaits verification.** (30.10) See `DEADMONEY.md`: stages from `deadmoney.toml` (menu map + kit
  tier, threshold by earnings), kit on open/death/stage change, replacing closed maps in the menu,
  LocalDev launches the selected map, loot without gear globally. Not done: showing progress to the player in the lobby
  (server log only); a joined client's kit follows their stage, but the map is the host's.
- [ ] **Extracted loot → gold — awaits verification.** (30.10) `handlers.rs` `settle_extraction` (both exit branches:
  LocalSettle status 2/3 and PrivateRaid extracted/left): items in bags (backpack + small) whose guids the role did not have
  before the raid are sold at
  `item_type.system_repurchase_price` (otherwise `trade_commodity.price_def`), × count; worn items (even found ones) and own
  items stay, loot without a price too (in the log). Gold → `state.gold`, result → `state.raid_last_sale` + history `state.raid_sales` (last 20).
  After settlement `raid_last_snapshot` is cleared — a repeated exit status will not sell twice. Log `[RaidSale]`.
- [ ] **A new client on the listen host appeared with 100 HP — fix awaits verification.** (30.10) `DungeonborneRuClient`
  `RefillWhenReady`: on the host (Role=3) once per 1 s after the hero spawns; when max HP (`RoleDisplayAttributeUtility.
  GetMaxHealth`) is unchanged for two polls in a row and the gear is on (or ≥12 s) — once Cur = Max for HP, mana,
  soul energy, primal energy (Max = `…Base × (1 + …Percent)`, Percent — a fraction). Log `[REFILL] … HP 100->…`.
- [ ] **BP patch pilot (`zzzzzzz_BPFix_P`) removed 30.10**: with a second player charge attacks were cancelled. Cause not
  investigated (the patch or something else in listen with two players). The pak and tools are kept in `bp_patches\`, in game —
  the Lua workaround for the sword again. Check charge cancellation with two players and without the pak.
- [x] **Relic wave (sword/mace/dagger) in listen — fixed, verified in game 30.10.** (30.10)
  `<Relic>_Enchant.DealCastCharge`: `if HasAuthority and not IsStandalone: return`, otherwise the "client" branch computes
  the position from the camera and calls `Hero.SpawnProjectileWithClientParam` → `SpawnParam` + `SpawnParamReady` → the server
  (`Charge_Enchant_Base`) checks the position and `ProcessProjectile` moves the pre-spawned hidden projectile.
  The listen host for its own hero is the server and not standalone → nobody runs the branch. The previous workaround (empty event)
  could not work: `SpawnParam` is not filled. Fix in `DungeonborneRuClient`: after `DealCastCharge` of the own hero on
  listen we call the same branch `ExecuteUbergraph_<class>(entry)`: sword 104, mace 89, dagger `_03` 89 (`_03_6002` — a subclass).
  Log: `RELIC listen: ExecuteUbergraph_…(104) ok=true -> ProcessProjectile вызван`. Analysis: `scratchpad\re\cfg\*.txt`.
- [ ] **Dual daggers in listen break already on a normal LMB** (not only heavy). F7 in listen right after LMB is needed. (30.10)
- [ ] **Items without a UE source**: some `…001` variants (e.g. 181112001 "Merchant's Dagger", 181122001) have no entry in
  `UeSource_ItemType`. `upgrade_starter_item_id` iterates suffixes 0/1/2 — exclude ids without UeSource. (30.10)
- [ ] **Two-handed sword / other weapons without a heavy class in the list.** (30.10)
  The two-hander's charge class was not found in DumpGame. If the heavy of some weapon does not release — find the class and add it.

### Loot / resources
- [x] **No loot in chests or from enemies — fixed (loot is in the game, 30.10).** (30.10) Cause: `zzzzz_RebuildPrivate_P` contained
  the ORIGINAL `loot.bytes` (605 B, 18 rows, random_id 100001–100016) and, loading after `zz_Loot_P`, overrode its
  `loot.bytes` (18784 B, 1060 rows). Chain: `loot_ue_source_choose_group` (source → loot_id 910xxx) →
  `loot_choose_group_rate` (910xxx → random_id 900xxx) → `loot` (900xxx → items); the overriding table has no 900xxx.
  `loot.bytes` removed from the folder and pak `zzzzz_RebuildPrivate_P` (it was identical to the game's original). Old pak — in scratchpad.
- [x] **Souls (`B_TGF_Soul_C`) visible from enemies for any class (listen) — fixed, verified in game 30.10.** (30.10) Resources are correct
  (mage: `Magic=300/300`, no SoulEnergy set); the server too (`role_affix` 5001=300 without 5101). The soul spawns with
  `VisibleTag=TGF.Character.ActorTag.CareerSpec.SoulEnergy`, `VisibleToLocalPlayer=false`, but is visible — hiding it
  is apparently the job of the client BP branch, which does not exist on the listen host for its own hero. Fix in `DungeonborneRuClient`:
  50 ms after spawn `SetActorHiddenInGame(true)` if `VisibleToLocalPlayer=false`. Log: `*** SOUL … hidden=`.
- [ ] **PVE pak `zz_SoloLoot_9_P`** — analysis 30.10, report `content\loot\pve_loot_report.txt` (script `content\loot\pve_loot_report.py`).
  Loot is defined by `(map_id, loot_type)` in `loot_ue_source_choose_group`: 246 keys, 92 distinct sets. Relevant for us:
  6 maps 10101–10103 (Crypt normal/Classic/Hard) and 10201–10203 (Castle) = `map_detail.loot_map_id`, plus `0` (default).
  `10111…10213` and `11101…16213` are not in our `map_detail` — unused. `loot_type` 1–5 = type 0 without 7 `BP_REA_Probe*`,
  3/4 also have `BP_Altar_Special_*`. 136 pools (we have 60), all 265 items exist in our `item_type`.
  Differences from ours: about twice as many treasures almost everywhere; on Classic/Hard more chests (`BP_REA_Tier*` — 28 sources
  that are not in our `loot_source_map` and, it seems, not in the game).
  **Imported 30.10**: the template got `maps`/`loot_types`, `content\loot\loot.toml` = PVE for the maps from our `map_detail` + 0,
  checked 10916/10916, installed (`zzzzzz_Balance_P`, 4 loot tables; server `ExcelConfig`). Our previous one — `loot_ours.toml`.
  [ ] Check in game that the host takes rows by `map_id` (10102 for host_from_lobby), not only map 0.
- [ ] **`no_gear` mode on the Crypt (10101–10103) — awaits verification.** (30.10) `content\loot\loot.toml` `[modes.no_gear]`: no
  gear and gold, `quality = 0.5`; checked: Crypt pools have 0 gear/gold items, other maps = PVE.
  Also a check of selection by `map_id`: if gear drops on the Crypt — the host takes map 0 (or substitutes it for
  sources with an empty list), then the mode has to go through map 0.
  **Verified 30.10: does NOT work** — gear and gold drop. Log `[LOOT]` (hook `UBattleUtility::Loot`): call with
  `GameState.MapId=10102 loot_type=0`, but the drop (e.g. Goblin_CrossBow 129: staff 181052001 + gold + 100107) exists
  only in the rows of map 0 / Castle → native loot does not look up the row by `GameState.MapId` (probably 0; the DS battle map
  is set by `UBattleDsUtility::InitGameMap(map_id,…)`, not called in host_from_lobby). The table is indexed by
  `(UeLootSourceId, MapId, LootType)` (`ExcelLoaderConfigGroupWrapper`), but native code calls it bypassing UFUNCTION
  (the hook only caught our calls); F8 on the wrapper CDO — 0 rows (data is in another instance: `FindFirstOf`).
  It is also possible that `zzzzzz_Balance_P` is not loaded (map 0 and `zz_Loot_P` have the same set). Next: F8 via
  the wrapper instance → row count (10916 / 262); then `InitGameMap` or the mode via map 0.
  All maps — `content\loot\maps_report.txt` (30.10): `map_detail`/`map_select` of the PVE server are byte-for-byte the same as ours. Loot exists
  only for 10101–10103/10201–10203 (+0). The lobby menu (`map_select`) refers to old map_id 1–5 — they have no loot either
  for us or for PVE; PVP arenas 30101–30302/40001 and test Gyms have no loot. 62 extra PVE map_ids are unknown to any of our tables.

### Tables / balance
- [ ] **Client and server tables diverge**: `zzzzz_RebuildPrivate_P` holds `affix_pool.bytes` (104830 B), the server has
  106826 B (`loot.bytes` removed from there 30.10, see "Loot"). Server `ExcelConfig\loot*` = `zz_Loot_P`. Decide whether to install
  `zzzzzz_Balance_P.pak` (it will override all 5 client tables with the server ones) — `cargo xtask balance build --deploy`. (30.10)
- [ ] **Client tables also override other mod paks**: `zz_Loot_P` (loot, loot_choose_group_rate, loot_rate,
  loot_ue_source_choose_group), `zz_Market_P` (market_product_item), `zz_Quest_P` (activity_center, quest_list),
  `zzzzz_RebuildPrivate_P` (affix_pool). `zzzzzz_Balance_P` loads last and will override them with its own versions —
  compare with these paks before `--deploy`. (30.10)
- [ ] Hot reload of tables on the server without a restart (now `GameData::load_all` only at startup). (30.10)
- [ ] The `ExcelConfigjson/` tables are outdated (they duplicate `balance/`) — delete once `balance/` settles. (30.10)

### Affixes
- [ ] **MINOR pools 100xxx are missing from the client tables** → `build_item_affix_data` takes the attribute from
  `FALLBACK_MINOR_ATTRS`; PVE attributes like 6212 (ice) do not drop for us. The format is not affected. (30.10)
- [ ] **The raid snapshot overwrites gear** (`handle_report_action`, LocalSettle/PrivateRaid status 2/3).
  If the client corrupted the buffer — bad affixes are saved; rebuilding in the lobby only on a schema/id change or empty
  `affix_data` (schema 6). Decide: validate the snapshot or rebuild affixes on receipt. (30.10)

### Client pak `zzzzz_RebuildPrivate_P` (UnLua)
- [x] **04.11: the custom match button and all local Lua removed** — match is native only (pre_search/search → room →
  server on Robot). Only `DebugConfig.json` and `affix_pool.bytes` remain in `zzzzz_RebuildPrivate_P`;
  `zz_MatchLocal_P` now ships the ORIGINAL `WBP_MatchMainButton.lua` (byte-for-byte from `pakchunk0-Windows.pak`) —
  to override the old copy players have (the launcher does not delete files). Nobody reads `match-mode.txt` anymore.
  Removed (sources — `removed\20261104_local_match\`): button wrappers (zz_MatchLocal_P v9, RebuildPrivate),
  `LocalDev`, `PrivateRaid`, `PrivateWorld` (the server does it itself: `wp.Runtime.EnableServerStreaming 0` in RuServer),
  `PrivateLoot`/`PrivateLootRules`, `PrivateExit`, `LocalSettle`, `LuaProbe`/`LuaProbeTargets`, `AttrIds`.
  Locally since 01.11 16:36 (match-mode = native) the server and clients already lived without them. Loot affixes: PrivateLoot hooked
  the loot window (`BP_InventoryLootSection.Setup`, only if the client is itself the server) — in the native process it never
  worked; MINOR pools 100xxx exist in `affix_pool` from `zzzzzz_Balance_P`; gear does not drop right now anyway
  (`loot.toml` `remove = ["equipment"]`). Backup before removal and rollback — `backup\20261104_before_match_removal\`;
  Lua probe — debug pak `debug\luaprobe` (not in the launcher). Launcher 1.1.1 (`dist\DungeonborneLauncher-1.1.1.exe`).
  The items below about LocalDev/PrivateLoot are closed together with the removal.
- [x] **`PrivateLoot.InstallLootHook` processes WORN items in a match on the host** and calls `OnRep_DItemBuffer`.
  `PrivateLootRules` for MINOR expects `group_index=2, affix_pool_value=false` (our/PVE format: 1/true) → `Fill()`
  appends an extra MINOR. Proposal: remove gear from `Extra`, process only loot. (30.10)
- [x] **`LocalDev` command `init_server`** calls `InitCharacterAttributesAndResources` on the host too
  (`InitDefault`/`InitFirstLevel` reset attributes). Limit to other players' pawns. (30.10)
- [x] **`LocalDev` `unlua-affix-dump.txt`** is overwritten in the lobby without the match section — write the match dump to a separate file. (30.10)
- [x] Two `WBP_MatchMainButton.lua` (`zz_MatchLocal_P` and `zzzzz_RebuildPrivate_P`) — the last one wins; the second is dead code. (30.10)

### UE4SS mods
- [ ] **`DungeonborneRuClient` R14_FIX adds `TgfEquipmentManagerComponent` to the pawn** on `PawnCreated`
  (the native one does not exist yet). Check whether it is needed (crash 0x142f602c6) and whether it duplicates the native component. (30.10)
  Comparison 30.10 (`data010.md`, `docs\fix_listen.md`, logs `C:\Games\Dungeonborne\…\crash_ue4ss*.log`, 20 `UEMinidump.dmp`):
  - confirmed: 0x142f602c6 — loop over 5 armor slots (dl = 11/13/15/14/12), `vcall [r14]+0x6A8(slot, -1)` =
    signature `UTgfEquipmentManagerComponent::GetEquipBySlot(ETgfEquipSlot, int32 IgnoreIndex)`, then `test rax; je`.
  - NOT confirmed: ".ecxr from UEMinidump, r14=0". `data010.md` is a live process (thread in NtWaitForSingleObject,
    r14=0xf7c9b8); none of the 20 minidumps has an exception at 0x142f602c6, and none contains code at 0x142f60246.
  - there is a duplicate: every other player's pawn has our manager (`…956`) + the native one (`…949`), the native one is created dynamically right
    before the dynamic `ChildActorComponent` (name counter 949→947, 770→769, 677→676). Then `GetComponentByClass`
    returned the NATIVE one in all 3 cases — the "takes the empty one" hypothesis is not confirmed by the logs so far.
  - `OnRep_PawnData` arrives after UCS → cannot prevent the crash; usable only as a point to clean up the dummy.
- [ ] **Two game installs**: `C:\Steam\steamapps\common\Dungeonborne` (main, logs there) and `C:\Games\Dungeonborne`.
  Mods are edited in `backups\ue4ss\Mods` and copied into both. Decide which one to keep. (30.10)
- [ ] **Clean UE4SS in the installs, dumps — to research (14.11).** `ws game diff` against reference 16716489:
  `Binaries\Win64\ue4ss` of the installs holds dumps, not only the runtime. Steam (stand server): 77 665 files,
  243 MB — `UE4SS_SDK` (14 204), `UHTHeaderDump` (23 965), `CXXHeaderDump` (402), `ULTRADUNP` (38 574, game dump
  with `CreateTreeCatalog.ps1`), `liveview`, `watches`, `UE4SS_SDK_Backends`; client `C:\Games\Dungeonborne`: 78 949 files,
  159 MB — the same plus `DumpGame` instead of `ULTRADUNP`; `C:\Games\Dungeonborne` — already clean (UE4SS.dll, settings, Mods)
  + 2 `crash_*.dmp` of 39 MB each. To do:
  - dumps to `home\dumps\ue4ss\<build 16716489>\` (UHT, CXX, SDK, DumpGame): they are not in `home\re` and `home\dumps`
    now; a copy exists only in `<repo>\ue4ss\` (same file counts: UHT 23 965, CXX 402, SDK
    14 204, DumpGame 38 574) — compare with the installs, take the full set, compare `ULTRADUNP` with `DumpGame`;
  - switch the tools that read dumps to `home\dumps\ue4ss`: `tools/py/wspaths.py` (`W("ue4ss/…Dump")`),
    `gen_probe_targets.py`, `rpc_evidence.py`; remove the `.gitignore` for `game/ue4ss/*Dump*`;
  - leave only the UE4SS runtime in the installs (as on E:): `UE4SS.dll`, `UE4SS-settings.ini`, `dwmapi.dll`, Mods
    from `cargo xtask mods install`; delete dumps from the installs only after copying and comparing, on confirmation;
  - crashes `crash_*.dmp` from the installs' `ue4ss\` — to `home\runs\crashes` (teach `ws runs collect` this folder too).

### Backend
- [ ] **Client probe ClientProbe (UE4SS) + LuaProbe (pak) — enabled in both installs, verified by automated runs.** (31.10)
  Logs `probe/logs`, merging with `rpc_dump/OUR.jsonl` — `tools/py/probe_timeline.py`; description `docs/CLIENT_PROBE.md`.
  UE4SS hooks on service functions (`services`) crashed the game at login — turned off for good; events, windows and the Lua layer
  (LCALL/FIELD) got through login → lobby → match. Automated instances (-dbserver/-dbclient) are skipped by the probe, `-dbprobe` enables it.
  Next: capture logs of manual play (mail, friends, shop, rooms) and analyse `probe_timeline.py --fields`.
- [ ] **All RPCs from `handlers_template.rs.txt` are implemented (185, 9 domains) — built, not verified in game.** (30.10)
  Status per domain, verification checklist and known problems — `docs/HANDLERS_STATUS.md`. Errors without `ret_code` —
  `RpcHead.error_code` (`set_rpc_error`). Native rooms `room_*` are included here.
- [x] **GameData lost rows in 18 tables** (HashMap by a non-unique key; `tools/py/gamedata_dup_keys.py`) —
  `<table>_rows` added (generator + generated). (30.10)
- [ ] **Base role attributes now follow its level** (was: level 20 for everyone) + **raid experience** (`map_exp`, leveling via
  `role_level_exp`). New and old characters start with level-1 attributes — weaker than before;
  `ROLE_ATTR_LEVEL=20` restores the old behaviour. Check: level growth after a raid, balance in a match. (30.10)
- [ ] 2 build warnings (`AsyncWriteExt` in main.rs:25, `gateway_ports` is never read). (30.10)

### Graphics
- [x] **"Blurry" textures on castle (World Partition) — checked, everything is fine on the client.** (05.11)
  The host has no graphics (`-nullrhi`), the client renders the image. The user checked the client on castle — textures are fine, no changes needed.
  If the "blur" comes back, in order:
  1. `r.Streaming.PoolSize` (current pool size);
  2. `r.Streaming.LimitPoolSizeToVRAM 0` + `r.Streaming.PoolSize 3000` — is the pool large enough;
  3. `r.Streaming.FullyLoadUsedTextures 1` — does streaming request full textures;
  4. compare with single-player and check how many game copies share video memory.
  In game on Epic the pool is 1000 MB (engine default; `DefaultScalability` does not change it).

### Idea: "dynamic classes" based on the Mithril Order (later, 05.11)
- [ ] **New classes via mithril costumes, without C++.** User's decision: "write it down, we'll implement and develop the topic later".
  A new class in the pure sense is impossible: the class list is hardcoded in the exe (`ETgfActorCareer` uint8) and in the protocol. Paladin (9) exists
  only in C++, there are no assets. Mithril classes 101–103 already exist in both.
  **Corrected 06.11:** Order items do not grant abilities, the digit in the id is quality. A class is a `role_type` row.
  **Main scheme (06.11):** a `role_type` row with the Order's `career` (101–103) and `force` = ADVENTURER: normal lobby, stash and raids,
  its own class in a match. No need to make an Order mode for this; we do not substitute `career`. Skills — the `role_skill` pool + `role_select_skill` choice.
  At most 3 fully distinct classes: talents are tied to `career` + race. Details — `research\mithril_classes\README.md`. Plan:
  1. First check: one `role_type` row, the backend creates such a character, joining a raid on our server.
  2. Check: the creation window, `ForceType` in a match, `MithrilItems`, skill choice, another class's skill on this `career`.
  3. Own talents (`FirstLevelAttr`), gear by armor type, texts.
  4. Later — fully custom abilities. A mod kit is needed: stubs of the game's C++ classes from `UHTHeaderDump` in a UE project.

  **State 06.11: trial class 400201, paused at the user's request.** Everything is separate from the main game: `<data>\new_class`
  (README, `class.json`, paks, backend with its own DB) and game copies `F:\DB_NewClass`. Do not install into the main installs or `ExcelConfig`.
  - [x] Verified by the user in game: ninth slot in the creation window, character creation, hall, raid with extraction, class skills.
  - [ ] Built, not verified in game: `class.json` (name, description, stats, kit, talents 1–2, icons), removing the cross on gear, the mage's Q on the class (`skills`, test).
  - [ ] **New Q:** charging like the mage's without the magic cost, effect `FX_Torch_01` (Moonlight skin, confirmed by the user), then an explosion around self, damage and the reaper's fear (`Attack4`, the debuff has a poison icon). Clone not started, plan — `research\new_abilities\README.md`. First the mage's Q test. New UE versions won't help: cooked assets won't load in 5.1.
  - Launch: one stand for everyone, ports are hardcoded. Before a test stop the main backend and server only with the user's consent, then remove the `new_class` lock.
  - Loot from bags on exit is sold for gold only with `enabled = true` in `deadmoney.toml` (user's decision 07.11).
  The Order mode itself (Wild) — a separate task, optional: analysis in the same place.

### Idea: new equipment from scratch (later, 05.11)
- [ ] **Own look, name and stats on existing mechanics.** User's decision — "write it down next to the new characters".
  Details — `research\new_equipment\README.md`. Levels:
  1. A new item on an old model (name, stats, affixes, classes) — tables only: `item_type`, `UeSource_ItemType`, `UeSource_Equipment`.
  2. A new look — models (for each body type: base, `_Human_Female`, `_Undead`), textures, icon + a clone of `ID_`/`AID_` with new references.
     Build — the Jester pipeline (`ue_jester`). Prototype — a helmet: one model, no per-body variants.
  3. An item with a new mechanic (its own ability) — needs a mod kit, as in "dynamic classes".

### Idea: new abilities from scratch (later, 05.11)
- [ ] **Lyra GAS abilities: GA (logic), GE (effects), AbilitySet (granting), GCNL (cues) + montages, projectiles, Niagara, description tables.**
  Details — `research\new_abilities\README.md`. Levels:
  1. Recombining and tuning numbers of existing ones — possible right now: editing assets and tables.
  2. New GE/GCNL — a light mod kit.
  3. New GA with their own logic — a mod kit.
  The abilities pak is installed both on the server and on all clients.
- [ ] **The basis for everything "new" (classes, gear with mechanics, abilities) — a mod kit.** Description and plan — `research\modkit\README.md`.
  Build Tools for Visual Studio 2022 (MSVC v143 14.3x) are needed — the user installs them.
  Right now only VS "18" (MSVC 14.44/14.51) is present, UE 5.1 may not see it.

### Idea: guaranteed unique from the Monarch and Wendigo (later, 11.11)
- [ ] **A unique item on the 30th kill of the Monarch (`Shadow_King`) / Wendigo without a unique.** User's decision — "write it down
  as an idea". The tables' native guarantee (`condition.unlucky_loot_cnt`) does not fit: the counter lives for one match (`DsUtility+0x2d8`)
  and counts successful slot rolls, not kills; the boss is killed once per match. Own scheme:
  1. RuServer (hook `BattleUtility:Loot`, block `LootLog`) sees loot from `Shadow_King`/`Wendigo` (and `H_`) → tells the backend
     who killed it (a new `/ds/…` modelled on `/ds/kill`, `/ds/quest_progress`).
  2. Backend: a per-player counter in the DB across matches; on the 30th — a random unique (11 items, pool 900016) by mail, like
     `kill_reward_mail`; counter to 0.
  3. Reset on a normal drop — requires reading the dropped array in the `Loot` post-hook (argument 2 `TArray`, visible on
     the stand, contents not read). Simpler option — "every 30th kill" without checking the drop.
  Decide: counter per killer or per whole team; shared between the two bosses or separate; always by mail or only when leaving
  the raid alive. Tables "uniques only from the Monarch and Wendigo" — `docs/LOOT_AUDIT.md`, stage 3.

### Content: Jester instead of Reaper (Dead Money season)
- [ ] **Jester attack effect (Niagara) — the user does it themselves, later.** (05.11)
  The ribbon weapon is hidden by the `JesterSwap` mod (`HideBoneByName(Weapon_Bone_01)`). A VFX "bloody ribbon with coins" is needed
  for the duration of the attack. The hook in the mod already exists: start on the `*Attack*` montage, stop by timer, `SpawnSystemAttached` on the hand.
  Own asset — in `ue_jester\Dungeonborne\Content\Mods\Jester`, wiring — `FX_PATHS` + `fx=` in `ru-jester.txt`.
  The game's `FX_common_DaggerTrail` launched but is not visible (default `fx=none`).
  Details — `<data>\ue_jester\README.md`, section "Эффект атаки" (attack effect).
- [ ] **Jester on only one spot + behaviour after "death"** (invulnerability, loot, blue portal). (02.11)
  Right now `JesterSwap` replaces all Reapers. Pak `zz_Jester_P` and the mod — in the client `C:\Games\Dungeonborne`.
  On the local server — the server part (lift by 30 cm).

## Done (30.10)
- [x] Loot template `content\loot\loot.toml` + `cargo xtask loot export/show/build [--deploy]` (BALANCE.md, "Loot"): source
  groups, slots, pools, `[rules]` (remove/chance/weight/count by category). A round trip without edits — byte-for-byte.- [x] CSV labels by names (attributes, rarity, classes, items, legendaries) from the client's Russian localization:
  UE4SS mod `NameDump` (F6 in the lobby) → `cargo xtask balance names-import` → `content\names\strings.tsv` (5627 rows, 6 tables).
  Items with a name 804/885, the rest have no name key in `item_type` — labelled by type.
- [x] Balance editing: `content\balance\tables\*.json` (or CSV for Excel) → `cargo xtask balance build` → server `core\backend\ExcelConfig\*.bytes` + client
  `zzzzzz_Balance_P.pak` (only differences from the originals). verify: 103/103 originals byte-for-byte. See BALANCE.md.
- [x] RPC trace: `PVE_SNIFF=127.0.0.1` (sniffer 127.0.0.2:6601-6604 → SoloLobby) and `RPC_DUMP=1` → `rpc_dump/<PVE|OUR>/`;
  PVE pushes without routing are decoded as `tgf.SCPlayerDirtyChgSync`. `examples/find_push.rs` — find a type by .bin.
- [x] `build_item_affix_data` in PVE format (schema 6): BASIC only offset attributes, MAJOR + `gear_set_random`, LEGENDARY.
- [x] `battle_fetch_battle_user_data` — a minimal reply as in PVE (without `core_fighters` and DS fields).
- [x] CrashTracer v15: gear tooltips in a match (`AffixDisplayConfigAttached` → `OnRep_DItemBuffer` when the display is empty).
- [x] DungeonborneRuClient: `HookSpecificClasses` disabled (empty hooks on all hero functions); GA hooks narrowed to
  `K2_ActivateAbility`/`K2_OnEndAbility` of 13 charge classes (including Relic Enchant); GA distance flags — via NotifyOnNewObject.
- [x] Established: heavy breaks only in listen (`host_from_lobby` with `?listen`); in localbattle (standalone) it works natively.

### Proposal from Discord: crafting relics of lower quality (new season, 06.11)
- [ ] Unlocked the purple sword — you can craft the blue and green ones. **Done 06.11, verified by the user in game on the trial stand:** arrows at the "Изготовить" (Craft) button on the blacksmith panel, quality switch on the panel, crafting a lower quality (server: 210140130 with 210140180 unlocked). Pak `zz_RelicQuality_P` (README in the same place), not installed into the main installs. Flag `relic_lower_quality` in `deadmoney.toml`. Not verified in game: trimming gems to sockets, the flag turned off. The main build and Robot were not updated. For new seasons — see the pak README.
  - **Cannot be done with tables.** The qualities of one relic are `gear_heirloom_guide` rows with a shared `group_id` and an `unlock_condition` chain. The craft window takes only the current quality: `WBP_Heirloom_InstantiatePanel` → `group:GetCurHeirloomId()`. Splitting `group_id` by quality is possible, but that means 4 times more cards and breaks upgrading.
  - **Our game build (tables from 11.12.2024) has no such feature.** Nothing can be concluded about other patches from the files.
  - **Along the way 06.11, blacksmith materials showed 0.** The client reads as coins only ids 1000–1030 and 2000–29999, materials 1037–1039, 1052, 1053 are virtual items for it (`DItem` with `position.x < 0`). Fixed in `common::split_virtual_coins` and `player_get_info`, the display was confirmed by the user. Pushes via `hub::push_to` that bypass `push_dirty` are not converted. Not yet rolled out to the main build.
  - **The backend already accepts it:** `gear_heirloom_instantiation_impl` only checks that `heirloom_id` is unlocked, and lower qualities stay in `unlock_heirloom_id`. Each quality has its own price.
  - **Needed:** (1) a Lua wrapper of the panel with a quality switch: `CurHeirloomId`, `SetHeirloomUpgradeItem`, `SetMaterialCostList` work with any id; (2) in the backend trim gems to the quality's socket count: right now the group's `heirloom_gem_data` is copied whole, while lower qualities have fewer sockets (`display_slot`).
  - **Rules (native `CheckHeirloomCanInstantiate`):** crafting into the bag is not allowed if the character already has a relic of the same `type_id`; the stash has no such restriction. The server forbids only a copy of the same quality (-3837). The check does not depend on quality, so the window opens and does not block the lower one.
