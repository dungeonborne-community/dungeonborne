# One-time administrator setup: launching the game and removing mutexes without UAC

The agent works without administrator rights, while the game under anti-cheat raises UAC at launch, and its mutexes can be removed
only with administrator rights. The `DbTools` scheduler task with the highest privileges runs ONLY
`C:\ProgramData\DbTools\dbtools.ps1`. The folder is writable only by administrators, so what runs elevated
cannot be replaced without an administrator. The script takes what to do from the request
`<stand work folder>\request.json` (`config/local.toml [stand] work`) and checks it against an allow list (header of
`dbtools.ps1`): launching the server or client of one of the installs with validated parameters, `list`, `killmutex`.
Paths of the installs and the work folder are in `C:\ProgramData\DbTools\dbtools.config.ps1`, written by the installer from `local.toml`.

## Install and update (PowerShell "Run as administrator", from the workspace root)

```powershell
powershell -ExecutionPolicy Bypass -File ops\machine\install-dbtools.ps1          # show what will be done
powershell -ExecutionPolicy Bypass -File ops\machine\install-dbtools.ps1 -Apply   # install or update
```

Requires `[stand] work` and `[games] steam`, `client` in `config/local.toml`. The script copies `dbtools.ps1` and
`killmutex.ps1`, writes `dbtools.config.ps1`, sets permissions (`icacls`: administrators and SYSTEM — full access,
`Users` — read-only access to the logs `dbtools.log`, `killmutex.log`) and registers the `DbTools` task. After editing
`dbtools.ps1` or the paths in `local.toml` — run it again.

**Several workspaces on one PC** (since 18.11): run it once from each. The config keeps a list (`$Workspaces`): a run
adds or updates its own workspace (by its work folder) and keeps the others; `-Remove` takes it out. The task takes
the request from the work folder where it lies. `cargo ws stand` says whether the task knows this workspace.

## Usage (without administrator rights)

The stand scripts (`run.ps1`, `play.ps1`, `gui2.ps1` and others) write the request and start the task themselves. Manually:

```powershell
'{"action":"list"}' | Set-Content <stand work folder>\request.json -Encoding ASCII
Start-ScheduledTask -TaskName DbTools
Get-Content C:\ProgramData\DbTools\dbtools.log -Tail 5
```

The request and UE logs are in the stand work folder, not in `%LOCALAPPDATA%`: Claude Desktop is an MSIX app, new folders in
AppData are virtualized for it, and the task (a process outside the package) cannot see them (verified 30.10).

## Remove

```powershell
Unregister-ScheduledTask -TaskName DbTools -Confirm:$false
Remove-Item C:\ProgramData\DbTools -Recurse -Force
```
