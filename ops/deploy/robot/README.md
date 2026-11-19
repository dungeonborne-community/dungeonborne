# Dungeonborne: game server on Hetzner Robot

Build from 03.11.2026. Robot runs **game servers only**: the game client without a GUI is started as a listen server,
and the `DungeonborneRuServer` mod turns it into a dedicated server. Backend, database and launcher live on Hetzner Cloud (`core`),
connected over a private network (vSwitch). This is the same setup that was tested locally on 03.11 (server from a Steam install + 2 clients).

```
Paks\                      mod paks (16 files): tables, UnLua, BP fixes, server-only zzzzzzz_ServerNet_P
Win64\                     UE4SS + mods (DungeonborneRuServer, DungeonborneRuClient + dbnative.dll, CrashTracer…),
                           Goldberg steam_api64.dll, cacert.pem (trust for the backend certificate)
run\                       server mod folder (-dbroot): watcher steps, status, control, mod logs
install_gameserver.ps1     one-time install into the game (as administrator)
start_headless.ps1         start / status / stop servers
killmutex.ps1              removes the game's mutexes — for the second and further servers on one machine
```

**The paks are for the server only.** Do not install `zzzzzzz_ServerNet_P` on clients: in the lobby and in single player it changes
projectile behaviour. Clients install mods through their own kit/launcher.

## 0. Prerequisites

**Robot (`game-1`)**
- Windows Server on bare metal. ACE (anti-cheat) does not start under virtualization: Hyper-V, VBS and HVCI
  (Memory integrity) must be off, otherwise the game crashes with "can not run under a virtual machine".
- RDP login as administrator. The game runs in the user session: after leaving RDP do not end the session
  (disconnect with the window's X, not "Sign out").
- A copy of the game (~15 GB), e.g. `C:\Dungeonborne`, taken whole from a clean Steam install. The game is not part of the kit.

**Network (vSwitch Robot ↔ Cloud Network)** — addresses as of 03.11 (VM configs: `deploy\vm` in the resources folder):

| Machine | Private | Public |
|---|---|---|
| Cloud `core` (backend) | `10.0.0.2` | — |
| Robot `game-1` | `10.0.1.2` | `203.0.113.20` |

Check from Robot: `ping 10.0.0.2`. If it does not answer, fix the network first (vSwitch, route on Robot, Cloud Network).

**Backend on `core`** — already configured in `deploy\vm\dbserver.env` (verify that it matches):

| Variable | Value | Purpose |
|---|---|---|
| `BIND_IP` | `10.0.0.2` | game login ports 80/443/8085/6601–6604 only on the private address (players go through the relay) |
| `PUBLIC_GATEWAY_IP` | `10.0.0.2` | the gateway that dir hands to the server — over vSwitch |
| `DS_AGENT_BIND` | `10.0.0.2` | server mod HTTP (9091): `/ds/slot`, `/ds/kill`, results, player data |
| `DS_POOL` | `203.0.113.20:7777@*` (several: `…:7777@*;203.0.113.20:7787@*`) | slots: the address the backend gives to players |
| `DS_TRUSTED_IPS` | `10.0.1.2` | server login to the lobby without the launcher (everyone else is refused when the relay is on) |
| `DS_WAITING_ROOM` | `1` | waiting room before the match — as in the local tests on 03.11 |

Firewalls:
- `core` (`deploy\vm\firewall.sh`): from the vSwitch subnet — TCP 80, 443, 8085, 6601–6604, 9091.
- Hetzner Firewall on Robot: inbound **UDP 7777–7857** (10 per server) from anywhere, RDP only from your own IPs.
  The Windows firewall is configured by `install_gameserver.ps1`.

## 1. Install (once, and on every kit update)

1. Unpack the archive, e.g. into `C:\DungeonborneServer` (path without Cyrillic characters or spaces).
2. PowerShell **as administrator**:
   ```
   powershell -ExecutionPolicy Bypass -File C:\DungeonborneServer\install_gameserver.ps1 -GameRoot C:\Dungeonborne -BackendIp 10.0.0.2 -PublicIp 203.0.113.20 -Instances 1
   ```
   - copies the paks (all `.pak/.utoc/.ucas`), UE4SS and mods, Goldberg, `cacert.pem` (originals kept as `*.orig`);
     moves the PVE paks `zz_Solo*_9_P` to `Content\_pve_disabled_paks`;
   - writes the mod's `native_data.txt`: `http://10.0.0.2:9091/...` + `realkey`;
   - writes a block into `hosts` (game login domains → `10.0.0.2`), backup `hosts.dungeonborne-backup`;
   - opens UDP 7777…(7777 + 10·(Instances−1)) in the Windows firewall;
   - saves `server.json` (game folder and addresses), which `start_headless.ps1` reads.
3. Connectivity check:
   ```
   Test-NetConnection 10.0.0.2 -Port 9091
   Test-NetConnection 10.0.0.2 -Port 6601
   ```
   Both should show `TcpTestSucceeded : True`.

## 2. Start

```
powershell -ExecutionPolicy Bypass -File C:\DungeonborneServer\start_headless.ps1
```
- after ~40 s: `start_headless.ps1 -Status` → `"phase":"ready"`; the backend log shows the slot heartbeat;
  debugging on `core`: `curl http://10.0.0.2:9091/ds/slots` — slot `203.0.113.20:7777` is alive (`free`);
- several servers: `-Count 4` (ports 7777, 7787, 7797, 7807; 45 s pause between starts, mutexes are removed by
  `killmutex.ps1`). The backend's `DS_POOL` must list all these addresses, and `-Instances` at install must be at least as large;
- idle map: `-Map /TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01`; with a window for debugging: `-Window`.

From then on the server runs by itself: it takes a room when the backend assigns one (`/ds/slot` → `action=host`), runs the waiting room and the match,
after the match waits for players to leave (up to 30 s, then sends the rest to the lobby, 60 s max), reloads the map
and is `ready` again.

## 3. Stop and update

```
powershell -ExecutionPolicy Bypass -File C:\DungeonborneServer\start_headless.ps1 -Quit -All
```
`taskkill` cannot kill the game process under ACE — it exits by itself on `quit` in `run\dbserver[-port]-control.txt`
(`-Quit -Port 7787` stops one server). Running matches are cut off: update between peaks.

Update: stop all servers → unpack the new kit on top (or into a new folder) → run
`install_gameserver.ps1` again with the same parameters → `start_headless.ps1`.

## 4. Logs

| What | Where |
|---|---|
| server mod (phases, rooms, kills, `[ZONE]`, `[PROJ]`) | `C:\DungeonborneServer\run\dbserver.log` (`dbserver-<port>.log` for other ports) |
| status | `run\dbserver[-port]-status.json` |
| UE (in shipping only errors and the crash stack `[Callstack]`) | `C:\DungeonborneServer\logs\ue-<port>.log` (older ones with a date) |
| UE4SS (verbose, overwritten on start) | `C:\Dungeonborne\Dungeonborne\Binaries\Win64\ue4ss\UE4SS.log` |
| client mods on the server (`[PROJ]`, CrashTracer) | `…\Binaries\Win64\crash_ue4ss.log` |

## 5. Troubleshooting

| Symptom | Cause / what to do |
|---|---|
| game closes immediately, ACE log says "virtual machine" | Hyper-V / VBS / HVCI are on — turn them off, reboot |
| `dbserver.log`: `connect failed 201` repeats | no access to the game login: `hosts`, `BIND_IP`, `core` firewall on 8085/6601–6604, `PUBLIC_GATEWAY_IP` unreachable from Robot; login refused without the launcher — the server IP is missing from `DS_TRUSTED_IPS`/`DS_POOL` |
| `[SLOT] heartbeat … error 12029/12002` (logged in Russian) | 9091 unreachable: `DS_AGENT_BIND`, `core` firewall, the address in `native_data.txt` |
| `[SLOT] … unknown_slot` | `-PublicIp`/port does not match the backend's `DS_POOL` |
| `hosting: map did not load in 180 s` (logged in Russian) | the backend was restarted while the server was starting — `-Quit`, start again |
| second server does not start ("Multiple Game Instances") | `start_headless.ps1` was not run as administrator — mutexes were not removed (`killmutex.log`) |
| players cannot connect to the server | Hetzner Firewall on Robot: UDP 7777+; the address in `DS_POOL` must be Robot's public IP |

## Known limitations (03.11)

- In `Win64\ue4ss\UE4SS-settings.ini` the UE4SS consoles are off (`ConsoleEnabled`/`GuiConsoleEnabled`/`GuiConsoleVisible = 0`):
  on Windows Server without a video driver (Microsoft Basic Display Adapter) the OpenGL GUI console fails
  ("Glfw Error 65542: WGL…") and crashes the game at start. Do not turn them on.

- Memory grows by ~50 MB per match — the process has to be restarted periodically (`-Quit` + start) between peaks;
  there is no automatic graceful restart (drain) yet — see `docs/CI_CD_PLAN.md` §4.4.
- The folder `%LOCALAPPDATA%\Dungeonborne\Saved\RebuildProbe` is shared by all game instances on the machine.
  Since 04.11 the paks have no custom match button and no local Lua (`zz_MatchLocal_P` is the game's original button),
  nobody reads `match-mode.txt`.
- Pinning servers to CPU cores is optional: `run\dbserver-affinity.txt`, lines `<port>=<mask>`
  (e.g. `7777=0x3`, `7787=0xC`). Without the file processes are not pinned.

## ds-agent (03.11) — the server starts and restarts by itself
`ds-agent.exe` + `ds-agent.toml` in the kit folder. Run it as administrator in the RDP session instead of starting
`start_headless.ps1` by hand: the agent starts the server (waiting room), restarts a hung one (heartbeat silent for 90 s,
a transition phase longer than 5 min) and writes `logs\ds-agent.log`. Check without doing anything: `ds-agent.exe --check`.
Details: `ds-agent/README.md` in the backend sources.
