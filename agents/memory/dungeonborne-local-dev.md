---
name: dungeonborne-local-dev
description: "Since 05.11 development is local: backend dbtest/run_local.ps1 on 127.0.0.1, game server = Steam install (Robot kit), client = C:\\Games\\Dungeonborne; prod VM switched off"
metadata:
  visibility: public
  node_type: memory
  type: project
---

On 05.11.2026 development switched to local, and the prod backend VM was turned off (203.0.113.10 unreachable;
Robot reachable only over RDP — SSH to Robot needs the VM as jump host).

Setup (made 05.11):
- Backend: `tools\stand\run_local.ps1` (legacy, no server.toml; DS_POOL=127.0.0.1:7777@*, DS_WAITING_ROOM=1,
  MATCH_OPEN_HALL=1, RPC_DUMP=1) from `target\release\dbserver-rs.exe`, local DB `surreal_saves`, Dead Money on
  (`deadmoney.toml enabled = true`). Port lock `tools\stand\portlock.ps1` first.
- Game server: Steam install `C:\Steam\steamapps\common\Dungeonborne` laid out like Robot (kit Paks + Win64,
  CrashTracer 0, native_data.txt → http://127.0.0.1:9091). Previous files saved in the session scratchpad
  `steam_install_before_20261105`. Start via DbTools `{"action":"start","role":"server","install":"steam",...}`;
  mod files in `dbtools/run` (old per-port overrides moved to `run\_old_20261103`), UE log
  `<data>\dbtools\logs\ue-server-7777.log`, UE4SS log in the Steam install `ue4ss\UE4SS.log`,
  RuClient log `Win64\crash_ue4ss.log`.
- Client: `C:\Games\Dungeonborne`, started manually after DbTools `killmutex`. hosts: the launcher block
  `# >>> dungeonborne-launcher` already maps all 10 game domains to 127.0.0.1.

**Why:** prod crashes (B13b) need reproduction, and changes are to be tested before any deploy.
**How to apply:** test mod/backend changes here first; deploy to Robot/VM only when they are turned back on and the
owner approves. Related: [[dungeonborne-workflow]], [[dungeonborne-robot-server]].
