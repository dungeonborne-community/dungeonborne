---
name: dungeonborne-dedicated-plan
description: "Dedicated-server goal (started 2026-10-30): Rust backend on GKE, headless UE game servers on Hetzner, native match flow, new UE4SS mod DungeonborneRuServer"
metadata:
  visibility: public
  node_type: memory
  type: project
---

**Update 2026-11-03 (decision): GCloud/GKE dropped — Hetzner only** (Robot bare metal + Hetzner Cloud), no k8s.
Game servers stay on Robot bare metal (ACE refuses VMs, so never on Cloud VMs); backend/DB/Gitea/CI on Hetzner Cloud.
CI/CD direction: Gitea (+ act_runner) discussed; the old Dockerfile/k8s-deployment.yaml in backups are GKE-era.
Private net (set up 2026-11-03): Robot vSwitch VLAN 4000 ↔ Cloud network #12801033 (10.0.0.0/16; vSwitch subnet
10.0.1.0/24, gw 10.0.1.1). DS box = Windows Server 2022, Intel I210 "Ethernet 2": LBFO team `T0` (public, untagged) +
team NIC `vlan4000` (10.0.1.2/24, MTU 1400, route 10.0.0.0/16 via 10.0.1.1). Works: DS → cloud VM ping OK; gw 10.0.1.1 doesn't answer ping (normal).
Clients get the DS public IP; DS↔backend goes over 10.0.x.
Backend VM (deployed 2026-11-03): `Dung-Backend` 203.0.113.10, hel1, Ubuntu 26.04, private 10.0.0.2 (enp7s0); Robot DS
public 203.0.113.20. Root SSH by key works from the dev workstation (`ssh root@203.0.113.10`). /opt/dungeonborne, systemd `dbserver`,
nginx :80/:443 on public IP, ufw via deploy/vm/firewall.sh; configs/README in <data>\deploy\vm (launcher
relay mode). Linux build: `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.35 --target-dir target_linux`
(not musl). dbserver-rs has no --version flag — running it with any args starts the real server.

Goal set 2026-10-30: replace listen-host (`host_from_lobby`) with dedicated servers.
- Backend (Rust, `backups/src`) runs on GCloud GKE; a network between GKE and Hetzner already exists.
- Game servers = the client exe started headless as listen server on Hetzner bare metal; host's own local player
  must be switched to observer/spectator.
- Client uses `match-mode.txt = native` (`WBP_MatchMainButton.lua` in `zzzzz_RebuildPrivate_P`) → original
  `battle_matching_*` / `battle_room_heartbeat` RPCs; DS address goes to clients via `DBattleRoomEventLog.DsInitFinish`.
- Protocol already has the native DS service (`hello_world`, `init_player`, `heartbeat`, `player_game_over`, `ds_end`)
  in `core/backend/src/generated/generated_server_code.rs` — emulate it instead of inventing a custom one.
- New UE4SS mod `DungeonborneRuServer` (server-only, enabled by cmdline flag).
- Open questions as of 2026-10-30: Hetzner OS (Windows vs Linux+Wine), exact headless command, match size.

Facts from first headless run (2026-10-30):
- `Dungeonborne.exe` relaunches itself under ACE: child gets the command line but NOT custom env vars → mod config
  via `-dbserver -dbopenid= -dbmap=` (read with KismetSystemLibrary.GetCommandLine).
- The ACE child can't be killed (`taskkill /F` → Access denied); server must exit itself (mod: `dbserver-control.txt` = quit).
- Second instance on the same Windows session refuses: "Not Support Starting Multiple Game Instances"; closing the
  mutex reportedly allows multiple instances.
- Working headless chain (2026-10-30): login via BP click handler `BndEvt__WBP_LoginWindow_StartGameButton_…`
  (direct `ULoginWindow::StartGame()` crashes), state from hooks on `/Game/TGF/UI/WBP/WBP_LoginWindow.WBP_LoginWindow_C:OnEnterState*`,
  then `StartLocalBattle(FName("map?listen"))`. UE4SS.log is the detailed log (overwritten each start); UE log = errors only.
- ACE refuses to start under a VM ("can not run under a virtual machine", 13-65544-2, seen 2026-10-30 on the
  Hetzner box): game servers need bare metal with no hypervisor (Hyper-V/VBS/HVCI off). Not helping bypass ACE detection.
- The agent can't kill game processes (auto-mode classifier denies Stop-Process) — ask the owner, or use `start_headless.ps1 -Quit`.

Direction 2026-10-30: make listen behave like dedicated — the host's PC runs the headless server instance and the
host's own client connects to it like everyone else; TGF custom logic (loadout, map, game over) closed via backend.
Client exe has the DS layer: `UBattleDsUtility` (GameInstanceSubsystem: InitGameMap, InitPlayerGameData, PlayerSetLogin,
PlayerDeadPlayerState, GameOver, IsRoomReady…) and `UDSSDK : UDssdkSubsystem` (Tencent DS SDK). Unknown: whether DSSDK
starts in a non-dedicated process and where it connects.

**Why:** big multi-part effort; parallel work on separate code areas is wanted.
**How to apply:** keep tracks in disjoint folders (src/ matchmaker, ds-agent crate, ue4ss mod, deploy/); see [[dungeonborne-workflow]], [[dungeonborne-bp-reverse]].

**Deployed 03.11 (evening):** VM `Dung-Backend` 203.0.113.10 / 10.0.0.2 (Ubuntu 26.04, root SSH by key): /opt/dungeonborne
(dbserver, launcher mode per `<data>\deploy\vm`), nginx 443/80, discord-bot /opt/discord-bot. Robot
`<DS_HOST>` 203.0.113.20 / 10.0.1.2 (Windows Server 2022): SSH only via jump `ssh -J root@203.0.113.10
admin@192.0.2.20`; game C:\DungeonbornServer\Dungeonborne, kit C:\DungeonbornServer\Kit. The game server must be
started from an interactive RDP session (start_headless.ps1); creating a logon scheduled task is denied for the agent
(persistence) — ask the owner. Remote PS scripts: ASCII only (PS 5.1 reads BOM-less UTF-8 as ANSI).

**Prod DB reset 2026-11-03 17:48 UTC (full wipe, on request):** old DB at /opt/dungeonborne/surreal_saves_before_legendary_20261103,
old binary dbserver-rs.before_legendary. New binary seeds `starter_loadout` (legendary starter gear, src/starter_loadout.rs).
Second prod reset 2026-11-03 18:10 UTC: Dead Money stays on, all stages tier 5 / need 0 (deadmoney.toml), stages with reached
need open on read (deadmoney::progress calls unlock). Dead Money re-dresses players into the stage kit on raid entry, so
battle gear = stage tier, not starter_loadout. Backups: surreal_saves_legendary_try1_20261103, deadmoney.toml.before_allopen.
Third prod reset 2026-11-04 13:12 UTC: normal Dead Money (needs 0/5000/20000/50000/120000/300000), kit tier by map
difficulty casual 2 / Classic 3 / Hard 4; kits + starter = class template (starter_loadout::class_template: Excel gear +
JEWELRY rings/necklace, jewelry exists only in tiers 1-2). New char: level 20, purple (STARTER_TIER 4), gold START_GOLD
1 000 000, kit_stage = 0 so first raid keeps the starter set. Deploys to the VM are coordinated with the "Agent orchestrator" session.
