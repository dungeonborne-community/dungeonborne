---
name: dungeonborne-robot-server
description: "Hetzner Robot game server — paths, SSH jump, IPs, kit, consoles off; 10.11 agent crash + watchdog"
metadata:
  visibility: public
  node_type: memory
  type: reference
---

- Robot (Windows Server 2022, bare metal): private 10.0.1.2, public 203.0.113.20. Game `C:\DungeonbornServer\Dungeonborne`,
  kit `C:\DungeonbornServer\Kit`. SSH: `ssh -J root@203.0.113.10 admin@192.0.2.20` (as of 2026-11-03).
- Backend VM (Cloud) private 10.0.0.2; its env/firewall in `<data>\deploy\vm` (dbserver.env: BIND_IP,
  PUBLIC_GATEWAY_IP, DS_AGENT_BIND = 10.0.0.2, DS_POOL, DS_TRUSTED_IPS=10.0.1.2).
- Server kit: `backups\dist\dungeonborne-robot-20261103` (+ .zip): install_gameserver.ps1 -BackendIp -PublicIp,
  start_headless.ps1 (-Count/-Status/-Quit -All), killmutex.ps1, README. Server-only pak zzzzzzz_ServerNet_P lives here,
  never in client bundles (clients get mods from launcher 1.1, launcher/tools/make_client_bundle.py).
- UE4SS consoles (ConsoleEnabled/GuiConsoleEnabled/GuiConsoleVisible) must be 0 on Robot: no GPU driver →
  OpenGL GUI console aborts the game at start (Glfw 65542).

**How to apply:** when rebuilding the Robot kit keep consoles off; deploy/coordination with the orchestrator session.
Related: [[dungeonborne-dedicated-plan]], [[dungeonborne-workflow]].
- 2026-11-03: ds-agent (crate `backups/ds-agent`, Windows exe in Kit) supervises DS: start/quit via start_headless.ps1,
  health from backend `GET 10.0.0.2:9091/ds/slots`; does NOT /ds/register. `ds-agent.exe --check` is read-only (safe via SSH).
  start_headless.ps1 applies `*.new` mod files before start (running DS locks dbnative.dll — stage updates as .new).
  Remote PS via `powershell -NoProfile -Command -` < script: keep each loop on one line, ASCII only.
- 2026-11-04: narrow allow rules in backups/.claude/settings.local.json (approved list): VM binary swap/restart,
  scp to /opt/dungeonborne/dbserver-rs.new; Robot start_headless -Quit, schtasks /Run|/End /TN DbAgent,
  Stop-Process -Name ds-agent, scp into ue4ss/Mods, Content/Paks, Kit. Use these exact command forms; anything else asks.
  VM deploy gotcha (04.11 07:44, ~30 s outage): a fresh scp'd dbserver-rs.new lands root:root 644 → systemd 203/EXEC
  restart loop. Before the swap run `chown dungeonborne:dungeonborne dbserver-rs.new && chmod 755 dbserver-rs.new`.
  Agent runs in the RDP session via the manually created task DbAgent (ONLOGON, /IT) — start it with `schtasks /Run /TN DbAgent`.
- 2026-11-04 crash signature: AV in UE4SS.dll (7 frames …20e4/9e2e/a0c3/b956/8d7c/d065/6aa8) ← exe 0x1450e0023
  (UWorld::SpawnActor → object construct, detoured by UE4SS NotifyOnNewObject) ← 0x142fa6b91 SpawnDefaultPawnAtTransform =
  a mod's NotifyOnNewObject(Pawn) callback; seen 31.10 too, NOT mod logic changes. Prime suspect CrashTracer (the orchestrator
  disables it on Robot with approval). UE4SS.log name is hardcoded in UE4SS.dll and shared by all ports (overwritten
  each start) — start_headless.ps1 to copy it to Kit\logs before launch. Analysis: research\crash7777.
- 2026-11-07: RDP closed to the internet (firewall rules RemoteDesktop-* = 10.0.0.0/16, NLA on, lockout 10/30 min). RDP only through tunnel: `ssh -L 13389:10.0.1.2:3389 root@203.0.113.10`, client 127.0.0.1:13389. Log in as the same Administrator user to keep the session DbAgent needs.
- 2026-11-10: at night ds-agent crashed with a panic (task exit code 101, the text went to the console); the DbAgent task was ONLOGON only
  (and with the default 72 h limit) — 3 h without servers. Robot local time = UTC−8. Then Robot stopped returning the SSH banner
  (Win32-OpenSSH spawns a process per connection — it looks like Windows could not create processes). Fix 3627cfb:
  agent + `core/backend/ds-agent/install_task.ps1`, backend `[Watchdog] ALARM` (the word is Russian in the log) / `GET /admin/v1/ds/health`.
