# Stand paths from the workspace config/local.toml (docs/WORKSPACE_PLAN.md). Dot-source at the start of a script:
#   . "$PSScriptRoot\paths.ps1"
# Provides: $WsRoot (workspace root), $Work (DbTools work folder: request.json, run\, logs\, runs\),
#       $LockDir (port locks), $ProbeLogs (probe logs), $Games (name -> game install from [games]).
# dbtools.ps1 does not look here: it is installed into C:\ProgramData\DbTools (README_admin.md) and has its own paths.

$WsRoot = (Resolve-Path "$PSScriptRoot\..\..").Path
$__local = Join-Path $WsRoot 'config\local.toml'
$__text = if (Test-Path $__local) { Get-Content $__local -Raw -Encoding UTF8 } else { '' }

function Get-LocalValue([string]$Section, [string]$Key) {
    # simple TOML parsing: key = 'string' or "string" inside [Section]
    $m = [regex]::Match($__text, '(?ms)^\[' + [regex]::Escape($Section) + '\]\s*$(.*?)(?=^\[|\z)')
    if (-not $m.Success) { return $null }
    $v = [regex]::Match($m.Groups[1].Value, '(?m)^\s*' + [regex]::Escape($Key) + '\s*=\s*[''"]([^''"]*)[''"]')
    if ($v.Success) { return $v.Groups[1].Value } else { return $null }
}

$DbHome = if ($env:DB_HOME) { $env:DB_HOME } elseif (Get-LocalValue 'home' 'path') { Get-LocalValue 'home' 'path' } else { Join-Path $WsRoot '..\home' }
# no "..": the DbTools config and ws stand compare these paths as text (19.11: an administrator window without DB_HOME
# registered 'X:\ws\..\home\runs\stand', which ws stand did not recognize)
$DbHome = [IO.Path]::GetFullPath($DbHome)
function Resolve-WsPath([string]$p) { if ($p -like 'home:*') { Join-Path $DbHome $p.Substring(5) } else { $p } }

# DbTools work folder. While the DbTools task (C:\ProgramData\DbTools) looks into <data>\dbtools,
# local.toml [stand] work points there too; after the task is reinstalled — home:runs/stand.
$Work = Resolve-WsPath $(if (Get-LocalValue 'stand' 'work') { Get-LocalValue 'stand' 'work' } else { 'home:runs/stand' })
# Port locks: one folder for the whole machine by default (18.11) - the game's ports are the machine's, and two workspaces
# on one PC must see each other's locks. [stand] locks overrides it (then every workspace here must name the same folder).
$LockDir = Resolve-WsPath $(if (Get-LocalValue 'stand' 'locks') { Get-LocalValue 'stand' 'locks' } else { Join-Path $env:ProgramData 'DungeonborneStand\locks' })
$ProbeLogs = Join-Path $WsRoot 'tools\probe\logs'
# The DbTools task (ops\machine\install-dbtools.ps1) lives in a system folder that is the same on all machines
$DbToolsDir = 'C:\ProgramData\DbTools'
$AdminLog = Join-Path $DbToolsDir 'dbtools.log'

$Games = @{}
$__g = [regex]::Match($__text, '(?ms)^\[games\]\s*$(.*?)(?=^\[|\z)')
if ($__g.Success) {
    foreach ($l in [regex]::Matches($__g.Groups[1].Value, '(?m)^\s*([\w-]+)\s*=\s*[''"]([^''"]*)[''"]')) {
        $Games[$l.Groups[1].Value] = (Resolve-WsPath $l.Groups[2].Value) -replace '/', '\'
    }
}
function Get-Game([string]$Name) {
    if (-not $Games.ContainsKey($Name)) { throw "no install '$Name' in local.toml [games] (available: $($Games.Keys -join ', '))" }
    return $Games[$Name]
}

# Starting the backend from the stand: working folder — core/backend (ExcelConfig, Network.pb, db_rocks, rpc_dump),
# seasons and matchmaking checks — from content/, host folder — $Work/run. Returns the path to dbserver-rs.exe.
function Enter-Backend {
    Set-Location (Join-Path $WsRoot 'core/backend')
    $env:SEASONS_DIR = Join-Path $WsRoot 'content/seasons'
    $env:MATCH_CHECKS_FILE = Join-Path $WsRoot 'content/match_checks.toml'
    $env:DB_HOST_ROOT = Join-Path $Work 'run'
    $target = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $WsRoot 'core/backend/target' }
    return (Join-Path $target 'release/dbserver-rs.exe')
}
