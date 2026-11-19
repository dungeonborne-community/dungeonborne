# Game actions that need administrator rights (the game under ACE raises UAC): starting an instance and
# removing mutexes. Run by the DbTools scheduler task with the highest privileges from C:\ProgramData\DbTools
# (a folder for administrators only, see tools\stand\README_admin.md).
#
# The task takes no arguments: the action comes from the request
#   <stand work folder>\request.json   (local.toml [stand] work)
# and is checked against an allow list — an arbitrary command cannot be run through the request. The request is deleted right
# after reading. The result goes to C:\ProgramData\DbTools\dbtools.log (read-only for everyone else).
# Not in %LOCALAPPDATA%: Claude Desktop is an MSIX app, new folders in AppData are virtualized for it
# (Packages\Claude_…\LocalCache\Local), and a process outside the package cannot see them. The game's UE logs are in the stand work folder.
#
# Request:
#   {"action":"list"}                     — show the game's mutexes (close nothing)
#   {"action":"killmutex"}                — remove the single-instance mutexes
#   {"action":"start","role":"server","install":"steam","port":7777,"map":"/TgfMaps/...","delay":5,
#    "observer":true,"window":false}
#   {"action":"start","role":"client","install":"second","connect":"127.0.0.1:7777","window":false,"probe":false}
#   probe (both roles): true → -dbprobe, enables the client probe in this instance
#   corelimit (both roles): 1..64 → -corelimit=N (fewer engine worker threads), 0/absent — as usual
# install: steam = local.toml [games] steam (battle server), second = [games] client.

$ErrorActionPreference = 'Stop'
$LogPath = Join-Path $PSScriptRoot "dbtools.log"
# Machine paths ($WorkDir, $Installs) — only from dbtools.config.ps1 next to it: it is placed by ops\machine\install-dbtools.ps1
# from the workspace local.toml ([stand] work, [games] steam / client). No paths of its own here (17.11).
$Config = Join-Path $PSScriptRoot "dbtools.config.ps1"
if (-not (Test-Path $Config)) {
    Add-Content -Path $LogPath -Value ((Get-Date -Format "yyyy-MM-dd HH:mm:ss") + " | ERROR no $Config - reinstall the task: ops\machine\install-dbtools.ps1 -Apply") -Encoding UTF8
    exit 1
}
. $Config
# Several workspaces on one machine (18.11: the owner and a second workspace): the config lists them in $Workspaces,
# each with its own work folder and installs; the request is taken from the work folder where it lies (the newest one
# if several). An old config ($WorkDir, $Installs only) works as before.
if ($Workspaces) {
    $pick = $Workspaces | Where-Object { Test-Path (Join-Path $_.WorkDir "request.json") } |
        Sort-Object { (Get-Item (Join-Path $_.WorkDir "request.json")).LastWriteTime } | Select-Object -Last 1
    if (-not $pick) { $pick = $Workspaces[0] }
    $WorkDir = $pick.WorkDir
    $Installs = $pick.Installs
}
$ReqPath = Join-Path $WorkDir "request.json"

function Write-Log([string]$Msg) {
    if ((Test-Path $LogPath) -and (Get-Item $LogPath).Length -gt 1MB) { Remove-Item $LogPath }
    Add-Content -Path $LogPath -Value ((Get-Date -Format "yyyy-MM-dd HH:mm:ss") + " | " + $Msg) -Encoding UTF8
}

function Get-Field($Req, [string]$Name, $Default) {
    if ($Req.PSObject.Properties.Name -contains $Name -and $null -ne $Req.$Name) { return $Req.$Name }
    return $Default
}

function Get-IntField($Req, [string]$Name, [int]$Default, [int]$Min, [int]$Max) {
    $v = Get-Field $Req $Name $Default
    if ($v -isnot [int] -and $v -isnot [long]) { throw "$Name must be a number" }
    if ($v -lt $Min -or $v -gt $Max) { throw "$Name is outside $Min..$Max" }
    return [int]$v
}

function Get-BoolField($Req, [string]$Name, [bool]$Default) {
    $v = Get-Field $Req $Name $Default
    if ($v -isnot [bool]) { throw "$Name must be true/false" }
    return $v
}

function Start-Game($Req) {
    $role = Get-Field $Req "role" ""
    if ($role -notin @("server", "client")) { throw "role: server or client" }
    $install = Get-Field $Req "install" ""
    if (-not $Installs.ContainsKey($install)) { throw "install: steam or second" }
    $port = Get-IntField $Req "port" 7777 1024 65535
    $delay = Get-IntField $Req "delay" 5 0 120
    $window = Get-BoolField $Req "window" $false

    $win64 = Join-Path $Installs[$install] "Dungeonborne\Binaries\Win64"
    $exe = Join-Path $win64 "Dungeonborne.exe"
    if (-not (Test-Path $exe)) { throw "no $exe" }

    $ueLog = Join-Path $WorkDir "logs\ue-$role-$port.log"
    New-Item -ItemType Directory -Force (Split-Path $ueLog) | Out-Null
    # Mod files (<role>.log, -status.json, -control.txt) — also on G:, next to the request
    $runDir = Join-Path $WorkDir "run"
    New-Item -ItemType Directory -Force $runDir | Out-Null
    $gameArgs = @("-ABSLOG=`"$ueLog`"", "-log", "-unattended", "-nosplash", "-dbdelay=$delay", "-dbroot=$($runDir -replace '\\', '/')")
    if ($role -eq "server") {
        $gameArgs = @("-dbserver") + $gameArgs
        # with -port=7777 the gateway login failed (30.10) — the port is passed only when not the default
        if ($port -ne 7777) { $gameArgs += "-port=$port" }
        $map = Get-Field $Req "map" ""
        if ($map -and $map -notmatch '^/TgfMaps/[A-Za-z0-9_/]+$') { throw "map: /TgfMaps/..." }
        if ($map) { $gameArgs += "-dbmap=$map" }
        if (-not (Get-BoolField $Req "observer" $true)) { $gameArgs += "-dbobserver=0" }
        # "dsa": "ip:port" — DSA agent address for the DS SDK (g6 dssdk: -dsaip/-dsaport/-starttoken/-startmode, 31.10)
        $dsa = Get-Field $Req "dsa" ""
        if ($dsa) {
            if ($dsa -notmatch '^(\d{1,3}(\.\d{1,3}){3}):(\d{1,5})$') { throw "dsa: ip:port" }
            $gameArgs += @("-dsaip=$($Matches[1])", "-dsaport=$($Matches[3])", "-starttoken=dbtest", "-startmode=0")
        }
    } else {
        $connect = Get-Field $Req "connect" "127.0.0.1:7777"
        if ($connect -notmatch '^\d{1,3}(\.\d{1,3}){3}:\d{1,5}$') { throw "connect: ip:port" }
        $gameArgs = @("-dbclient", "-dbconnect=$connect") + $gameArgs
    }
    if (-not $window) { $gameArgs += @("-nullrhi", "-nosound") }
    # "corelimit": N (1..64) -> -corelimit=N: the engine thinks there are N cores and creates fewer worker threads
    # (TaskGraph); 03.11, several servers on one PC. The flags -NumberOfWorkerThreads / -useallavailablecores
    # do not exist in this build (checked against the exe), -corelimit= does.
    $corelimit = Get-IntField $Req "corelimit" 0 0 64
    if ($corelimit -gt 0) { $gameArgs += "-corelimit=$corelimit" }
    # "probe": true → -dbprobe: the orchestrator probe (ClientProbe/LuaProbe) works in this instance even though
    # it is off for the user and is normally silent with -dbserver/-dbclient (docs/CLIENT_PROBE.md)
    if (Get-BoolField $Req "probe" $false) { $gameArgs += "-dbprobe" }

    $p = Start-Process -FilePath $exe -WorkingDirectory $win64 -ArgumentList $gameArgs -PassThru
    Write-Log "start $role install=$install pid=$($p.Id) args=$($gameArgs -join ' ')"
}

try {
    if (-not (Test-Path $ReqPath)) { Write-Log "no request $ReqPath"; exit 1 }
    $raw = Get-Content -Raw -Path $ReqPath
    Remove-Item $ReqPath
    $req = $raw | ConvertFrom-Json
    $action = Get-Field $req "action" ""
    switch ($action) {
        "list"      { (& (Join-Path $PSScriptRoot "killmutex.ps1") -List) | ForEach-Object { Write-Log "list: $_" } }
        "killmutex" { (& (Join-Path $PSScriptRoot "killmutex.ps1")) | ForEach-Object { Write-Log "killmutex: $_" } }
        "start"     { Start-Game $req }
        default     { throw "unknown action '$action'" }
    }
    Write-Log "ok $action"
} catch {
    Write-Log "ERROR $($_.Exception.Message)"
    exit 1
}
