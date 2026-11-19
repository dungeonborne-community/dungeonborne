# Benchmark of several headless servers on one PC (03.11): 1, 2, 4, 8 listen -nullrhi sessions from the Steam install.
#   powershell -ExecutionPolicy Bypass -File tools\stand\bench.ps1 [-Counts 1,2,4,8] [-Label base] [-Warmup 60] [-Measure 180]
#   -CoreLimit N — -corelimit=N for each server (fewer engine worker threads; needs an updated dbtools.ps1)
#   -Lite "widgets textures niagara" — safe RuServer set (run\dbserver-lite.txt); without -Lite the file is empty (baseline)
# Servers are started one by one and are not stopped between steps: 1 -> add up to 2 -> up to 4 -> up to 8.
# Instance i: port 7777 + 10*i, its own physical core (logical CPUs 2i and 2i+1, file dbserver-affinity.txt —
# the process sets the mask itself via dbnative, no administrator rights needed). Each has its own mod files
# (dbserver-<port>.log, for 7777 — dbserver.log).
# Before starting the next instance free memory is checked (-MinFreeGB) — otherwise the run stops.
# Measurement: after all reach ready — warmup, then a -Measure s window; [TICK] and [PERF] lines are taken from the logs
# (RuServer, every 10 s), system numbers — CIM every 5 s. Result: docs\bench\bench-<label>-<time>.md and .csv.
# Needs: the DbTools task (README_admin.md), backend on 6601, no Dungeonborne running.
param([int[]]$Counts = @(1, 2, 4, 8), [string]$Label = "base", [int]$Warmup = 60, [int]$Measure = 180,
    [int]$MinFreeGB = 6, [int]$ReadyTimeout = 300, [string]$Lite = "", [int]$CoreLimit = 0)

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$Run = Join-Path $Work "run"
$Req = Join-Path $Work "request.json"
$OutDir = Join-Path $WsRoot "docs\bench"
$Stamp = Get-Date -Format "yyyyMMdd-HHmm"

function Say([string]$M) { Write-Host ((Get-Date -Format "HH:mm:ss") + " " + $M) }
function PortOf([int]$I) { 7777 + 10 * $I }
function FileOf([int]$Port, [string]$Suffix) {
    if ($Port -eq 7777) { Join-Path $Run "dbserver$Suffix" } else { Join-Path $Run "dbserver-$Port$Suffix" }
}

function Send-Request([string]$Json) {
    Set-Content -Path $Req -Value $Json -Encoding ASCII
    Start-ScheduledTask -TaskName DbTools
    $deadline = (Get-Date).AddSeconds(20)
    while ((Get-Date) -lt $deadline -and (Test-Path $Req)) { Start-Sleep -Milliseconds 300 }
    Start-Sleep -Milliseconds 1500
}

function Get-System {
    $cpu = (Get-CimInstance Win32_PerfFormattedData_PerfOS_Processor -Filter "Name='_Total'").PercentProcessorTime
    $os = Get-CimInstance Win32_OperatingSystem
    [pscustomobject]@{ Cpu = [double]$cpu; FreeMB = [math]::Round($os.FreePhysicalMemory / 1KB) }
}

function Phase-Of([int]$Port) {
    $f = FileOf $Port "-status.json"
    if (-not (Test-Path $f)) { return "" }
    try { return ((Get-Content $f -Raw) | ConvertFrom-Json).phase } catch { return "" }
}

function Stop-All([int[]]$Ports) {
    foreach ($p in $Ports) { Set-Content (FileOf $p "-control.txt") -Value "quit" -Encoding ASCII }
    $deadline = (Get-Date).AddSeconds(90)
    while ((Get-Date) -lt $deadline -and (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue)) { Start-Sleep 2 }
    $left = @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue).Count
    Say "servers stopped (processes left: $left)"
}

# [TICK]/[PERF] lines of the instance log in the window [From, To] (time in the log — HH:mm:ss of the same day); the regex accepts the mod's Russian or English text
function Read-Window([int]$Port, [datetime]$From, [datetime]$To) {
    $f = FileOf $Port ".log"
    $ticks = @(); $cpu = @(); $ws = @(); $priv = @(); $thr = @()
    if (-not (Test-Path $f)) { return $null }
    foreach ($line in Get-Content $f -Encoding UTF8) {
        if ($line -notmatch '^(\d\d:\d\d:\d\d) \| (.*)$') { continue }
        $t = [datetime]::ParseExact($Matches[1], "HH:mm:ss", $null)
        if ($t -lt $From -or $t -gt $To) { continue }
        $body = $Matches[2]
        if ($body -match '^\[TICK\] ([\d.]+) fps .*(?:мин|min) ([\d.]+)') { $ticks += [pscustomobject]@{ Avg = [double]$Matches[1]; Min = [double]$Matches[2] } }
        elseif ($body -match '^\[PERF\] cpu=(\d+)% ws=(\d+)MB private=(\d+)MB .*threads=(\d+)') {
            $cpu += [double]$Matches[1]; $ws += [double]$Matches[2]; $priv += [double]$Matches[3]; $thr += [double]$Matches[4]
        }
    }
    if ($ticks.Count -eq 0) { return $null }
    [pscustomobject]@{
        Port = $Port
        Fps = [math]::Round(($ticks | Measure-Object Avg -Average).Average, 1)
        FpsMin = [math]::Round(($ticks | Measure-Object Min -Minimum).Minimum, 1)
        Cpu = if ($cpu.Count) { [math]::Round(($cpu | Measure-Object -Average).Average) } else { -1 }
        WsMB = if ($ws.Count) { [math]::Round(($ws | Measure-Object -Average).Average) } else { -1 }
        PrivMB = if ($priv.Count) { [math]::Round(($priv | Measure-Object -Average).Average) } else { -1 }
        Threads = if ($thr.Count) { [math]::Round(($thr | Measure-Object -Average).Average) } else { -1 }
    }
}

# --- checks ---
if (Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue) { Say "the game is already running — close it"; exit 1 }
if (-not (Get-NetTCPConnection -State Listen -LocalPort 6601 -ErrorAction SilentlyContinue)) { Say "backend is not listening on 6601"; exit 1 }
New-Item -ItemType Directory -Force $Run, $OutDir | Out-Null
$Max = ($Counts | Measure-Object -Maximum).Maximum
if ($Max -gt 8) { Say "more than 8 sessions: not enough cores for all (i9-11900K: 8 cores)"; exit 1 }

# Masks: instance i — logical CPUs 2i and 2i+1 (one physical core with HT)
$aff = for ($i = 0; $i -lt 8; $i++) { "{0}=0x{1:X}" -f (PortOf $i), ([uint64]3 -shl (2 * $i)) }
Set-Content (Join-Path $Run "dbserver-affinity.txt") -Value $aff -Encoding ASCII
foreach ($f in "dbserver-control.txt") { Set-Content (Join-Path $Run $f) -Value "" -Encoding ASCII }
Set-Content (Join-Path $Run "dbserver-fakeds.txt") -Value "0" -Encoding ASCII
Set-Content (Join-Path $Run "dbserver-lite.txt") -Value $Lite -Encoding ASCII
$LiteText = if ($Lite) { $Lite } else { "empty (baseline)" }

$idle = Get-System
Say ("idle: CPU {0}%, free {1} MB; dbserver-lite.txt: {2}" -f $idle.Cpu, $idle.FreeMB, $LiteText)

$Ports = @()
$Rows = @()
$Stopped = $false
foreach ($N in ($Counts | Sort-Object)) {
    while ($Ports.Count -lt $N) {
        $free = (Get-System).FreeMB
        if ($free -lt $MinFreeGB * 1024) { Say "free $free MB < $MinFreeGB GB — not starting more"; $Stopped = $true; break }
        $port = PortOf $Ports.Count
        foreach ($s in "-control.txt", "-status.json") { $f = FileOf $port $s; if (Test-Path $f) { Remove-Item $f } }
        Send-Request '{"action":"killmutex"}'
        Send-Request ('{"action":"start","role":"server","install":"steam","port":' + $port + ',"delay":5,"observer":true,"window":false,"corelimit":' + $CoreLimit + '}')
        Say "starting server $port (free $free MB)"
        $t0 = Get-Date; $phase = ""
        while (((Get-Date) - $t0).TotalSeconds -lt $ReadyTimeout) {
            $phase = Phase-Of $port
            if ($phase -eq "ready") { break }
            Start-Sleep 3
        }
        if ($phase -ne "ready") { Say "server $port did not reach ready (phase '$phase') — see $(FileOf $port '.log')"; $Stopped = $true; break }
        Say ("server $port ready in {0:N0} s" -f ((Get-Date) - $t0).TotalSeconds)
        $Ports += $port
        # the new instance's mutexes must have time to appear before the next killmutex (play.ps1: 45 s)
        $gap = 45 - ((Get-Date) - $t0).TotalSeconds
        if ($gap -gt 0) { Start-Sleep -Seconds ([int]$gap) }
    }
    if ($Stopped -or $Ports.Count -lt $N) { break }

    Say "N=${N}: warmup $Warmup s"
    Start-Sleep $Warmup
    $from = Get-Date
    $sys = @()
    $deadline = $from.AddSeconds($Measure)
    while ((Get-Date) -lt $deadline) { $sys += Get-System; Start-Sleep 5 }
    $to = Get-Date
    $inst = foreach ($p in $Ports) { Read-Window $p $from $to }
    $inst = @($inst | Where-Object { $_ })
    $row = [pscustomobject]@{
        N = $N
        Measured = $inst.Count
        FpsAvg = [math]::Round(($inst | Measure-Object Fps -Average).Average, 1)
        FpsWorst = [math]::Round(($inst | Measure-Object FpsMin -Minimum).Minimum, 1)
        CpuPerInst = [math]::Round(($inst | Measure-Object Cpu -Average).Average)
        WsPerInstMB = [math]::Round(($inst | Measure-Object WsMB -Average).Average)
        PrivPerInstMB = [math]::Round(($inst | Measure-Object PrivMB -Average).Average)
        Threads = [math]::Round(($inst | Measure-Object Threads -Average).Average)
        SysCpu = [math]::Round(($sys | Measure-Object Cpu -Average).Average, 1)
        SysFreeMB = [math]::Round(($sys | Measure-Object FreeMB -Minimum).Minimum)
    }
    $Rows += $row
    Say ("N={0}: fps {1} (worst {2}), CPU per server {3}% (100% = 1 logical CPU), WS {4} MB, private {5} MB, system CPU {6}%, free {7} MB" -f
        $row.N, $row.FpsAvg, $row.FpsWorst, $row.CpuPerInst, $row.WsPerInstMB, $row.PrivPerInstMB, $row.SysCpu, $row.SysFreeMB)
    foreach ($i in $inst) { Say ("   {0}: fps {1} (min {2}) cpu {3}% ws {4} MB private {5} MB threads {6}" -f $i.Port, $i.Fps, $i.FpsMin, $i.Cpu, $i.WsMB, $i.PrivMB, $i.Threads) }
}

Stop-All $Ports

$csv = Join-Path $OutDir "bench-$Label-$Stamp.csv"
$md = Join-Path $OutDir "bench-$Label-$Stamp.md"
$Rows | Export-Csv $csv -NoTypeInformation -Encoding UTF8
$lines = @("# Server benchmark: $Label ($Stamp)", "",
    "i9-11900K, 48 GB; listen -nullrhi -nosound, observer; one core per server (2 logical CPUs); warmup $Warmup s, window $Measure s.",
    "dbserver-lite.txt: $LiteText; corelimit: $(if ($CoreLimit) { $CoreLimit } else { 'none' }). Idle before start: CPU $($idle.Cpu)%, free $($idle.FreeMB) MB.", "",
    "| Sessions | fps avg | fps worst | CPU per server | WS per server, MB | Private per server, MB | Threads | System CPU | Free min, MB |",
    "|---|---|---|---|---|---|---|---|---|")
foreach ($r in $Rows) {
    $lines += "| $($r.N) | $($r.FpsAvg) | $($r.FpsWorst) | $($r.CpuPerInst)% | $($r.WsPerInstMB) | $($r.PrivPerInstMB) | $($r.Threads) | $($r.SysCpu)% | $($r.SysFreeMB) |"
}
if ($Stopped) { $lines += ""; $lines += "The run stopped early: see the script output." }
Set-Content $md -Value $lines -Encoding UTF8
Say "result: $md"
