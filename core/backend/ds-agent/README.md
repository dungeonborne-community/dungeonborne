# ds-agent — game server supervisor (v1, 03.11)

Runs on the server machine (Robot, Windows) next to `start_headless.ps1`. Every `check_secs`, for each port:

| Sees | Does |
|---|---|
| no server (neither port nor heartbeat for two ticks in a row) | start `start_headless.ps1 -Port N -Map <hall>` |
| started, but no heartbeat for `start_grace_secs` | port held — `quit`, free — start again |
| port held, mod heartbeat silent for `silent_secs` | `quit` (hung) |
| phase `hosting` / `resetting` / `entering` longer than `stuck_secs` | `quit` (hung: the hall did not load, a reset got stuck) |
| after `quit` the process did not exit within `quit_wait_secs` | ALERT in the log, waits for a human (under ACE the process is not killed) |
| more than `max_starts` starts per `window_secs` | stop with an ALERT, until the agent is restarted |
| backend not responding | does not judge health, only "no process" |

Health comes from `GET <backend>/ds/slots` (phase and `last_hb_secs` of the mod heartbeat), the port from `netstat -ano -p UDP`.
The agent does not register in the backend (`/ds/register` would route `init_player` to the agent; servers now work
by the hall assignment from `/ds/slot`). Mod updates `*.new` are installed by `start_headless.ps1` on start.

## Per-match process and scaling (04.11)
`standby = true` in `ds-agent.toml` → servers start with `-Standby` (mod `-dbstandby=1`): after login the mod stays in the
**lobby without a map** (phase `standby`; on a map the GameMode would start the game by itself), it loads the hall map only on a `host` assignment,
after the match (players left) it sends `exiting` and exits — the agent starts a new process. No map reset needed.

How many servers to keep is decided by the backend (`DS_AUTOSCALE=1`, spare `DS_MIN_IDLE=1`): the slot in `/ds/slots` has `want`.
Needed: the busy ones (room, reset, exit) + `DS_MIN_IDLE` free ones + one more if players are waiting in the queue. The agent:
not needed and not running — `Off`; needed two ticks in a row — start; extra in `standby` longer than `idle_secs` — quit.
Exit after a match and a "needed" start are planned and do not count toward `max_starts`.

## Running on Robot
As administrator, in an RDP session (the game needs a session; leave RDP with "Disconnect", not "Sign out"):
```
C:\DungeonbornServer\Kit\ds-agent.exe
```
Config — `ds-agent.toml` next to the exe. Log — `Kit\logs\ds-agent.log`. Dry check: `ds-agent.exe --check`.
To stop the agent without touching the servers: `Stop-Process -Name ds-agent` from another window (Ctrl+C or closing the window
of agent v0.1.0 killed the servers it had started; since 03.11 servers start without a console and do not depend on the window).
A second agent instance does not start (lock `127.0.0.1:9095`). Autostart at logon — a scheduler task
(the administrator sets it up):
```
schtasks /Create /TN DbAgent /SC ONLOGON /RL HIGHEST /TR "C:\DungeonbornServer\Kit\ds-agent.exe"
```
10.11: the agent crashed at night (panic, task code 101) and nobody brought it back up for ~3 h. Now the task has a second trigger —
every 5 minutes (an extra instance exits right away because of the lock) and restart on failure; it is set up by `ds-agent/install_task.ps1`
(PowerShell as administrator on Robot). A panic is written to `ds-agent.log` ("AGENT PANIC" + stack), a panic on one
slot does not stop the agent. The backend watches from its side: `[Watchdog] ALERT` in the journal
(`journalctl -u dbserver -p err`) and `GET /admin/v1/ds/health`, if the agent has not polled `/ds/slots` for 3 min or no
slot has sent a heartbeat for 5 min.

## Build
`cargo build --release -p ds-agent` (Windows) → `target\release\ds-agent.exe`. Logic tests — `cargo test -p ds-agent`
(`src/decide.rs`).

## Next (v2)
Registration in the backend as a server pool (instead of the static `DS_POOL`), several machines, alerts to Discord.
