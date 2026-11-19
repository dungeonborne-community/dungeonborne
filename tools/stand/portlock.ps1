# Shared lock on the game ports between Claude sessions (agent swarm, research) and the user.
# The client's ports are hardcoded: backend 8085 + 6601-6604 (TCP), game server 7777 (UDP); the folder
# %LOCALAPPDATA%\Dungeonborne\Saved\RebuildProbe is shared too. Only one owner at a time.
#
#   portlock.ps1 -Status
#   portlock.ps1 -Acquire -Owner "research f3f84a" -Purpose "cargo run + 2 clients"
#   portlock.ps1 -Release -Owner "research f3f84a"
#
# Acquire is atomic (CreateNew). Someone else's lock counts as abandoned if it is older than 2 h and the ports are free.
# Exit code: 0 — success / free, 1 — busy (prints by whom), 2 — error.
param([switch]$Acquire, [switch]$Release, [switch]$Status, [string]$Owner = "", [string]$Purpose = "")

. "$PSScriptRoot\paths.ps1"   # $Work, $LockDir, $ProbeLogs, Get-Game — from local.toml (docs/WORKSPACE_PLAN.md)
$LockPath = Join-Path $LockDir "game-ports.lock"
$StaleHours = 2

function Get-BusyPorts {
    $tcp = Get-NetTCPConnection -State Listen -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalPort -in 8085, 6601, 6602, 6603, 6604 } | ForEach-Object { "tcp/$($_.LocalPort)" }
    $udp = Get-NetUDPEndpoint -ErrorAction SilentlyContinue |
        Where-Object { $_.LocalPort -eq 7777 } | ForEach-Object { "udp/$($_.LocalPort)" }
    @($tcp) + @($udp) | Where-Object { $_ } | Sort-Object -Unique
}

function Read-Lock {
    if (-not (Test-Path $LockPath)) { return $null }
    try { Get-Content -Raw $LockPath -Encoding UTF8 | ConvertFrom-Json } catch { [pscustomobject]@{ owner = "?"; since = "?"; purpose = "broken lock" } }
}

function Test-Stale($Lock) {
    try { $age = (Get-Date) - [datetime]::Parse($Lock.since) } catch { return $false }
    return ($age.TotalHours -gt $StaleHours) -and (@(Get-BusyPorts).Count -eq 0)
}

New-Item -ItemType Directory -Force $LockDir | Out-Null
$busy = @(Get-BusyPorts)
$lock = Read-Lock

if ($Status -or (-not $Acquire -and -not $Release)) {
    if ($lock) { Write-Output "LOCKED by '$($lock.owner)' since $($lock.since): $($lock.purpose)" } else { Write-Output "free" }
    Write-Output ("ports in use: " + $(if ($busy.Count) { $busy -join ", " } else { "none" }))
    exit $(if ($lock) { 1 } else { 0 })
}

if (-not $Owner) { Write-Output "-Owner is required"; exit 2 }

if ($Release) {
    if (-not $lock) { Write-Output "no lock"; exit 0 }
    if ($lock.owner -ne $Owner) { Write-Output "the lock belongs to someone else ('$($lock.owner)') — not releasing"; exit 1 }
    Remove-Item $LockPath; Write-Output "released"; exit 0
}

# Acquire
if ($lock) {
    if ($lock.owner -eq $Owner) { Write-Output "already ours"; exit 0 }
    if (-not (Test-Stale $lock)) { Write-Output "BUSY: '$($lock.owner)' since $($lock.since): $($lock.purpose)"; exit 1 }
    Write-Output "abandoned lock '$($lock.owner)' since $($lock.since) — taking it over"
    Remove-Item $LockPath -ErrorAction SilentlyContinue
}
if ($busy.Count) { Write-Output "BUSY: ports are in use without a lock: $($busy -join ', ')"; exit 1 }
$body = [pscustomobject]@{ owner = $Owner; since = (Get-Date).ToString("s"); purpose = $Purpose } | ConvertTo-Json -Compress
try {
    $fs = [IO.File]::Open($LockPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
    $bytes = (New-Object Text.UTF8Encoding $false).GetBytes($body); $fs.Write($bytes, 0, $bytes.Length); $fs.Close()
} catch { Write-Output "BUSY: the lock was taken at the same time"; exit 1 }
Write-Output "acquired"; exit 0
