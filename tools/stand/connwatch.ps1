# L1 (login error 201 ext:-6): logs all TCP connections and UDP endpoints of the game processes every 150 ms — on change.
# Shows where the client knocks during "Не удалось подключиться" (Could not connect), if gateway 6601 sees no attempts.
#   powershell -ExecutionPolicy Bypass -File tools\stand\connwatch.ps1 [-Minutes 10]
# Log: probe\logs\connwatch_<time>.log
param([int]$Minutes = 10)
$Log = Join-Path $PSScriptRoot ("..\probe\logs\connwatch_{0}.log" -f (Get-Date -Format "yyyyMMdd-HHmmss"))
$prev = @{}
$deadline = (Get-Date).AddMinutes($Minutes)
"connwatch until $($deadline.ToString('HH:mm:ss')) -> $Log"
while ((Get-Date) -lt $deadline) {
    $pids = @(Get-Process -Name Dungeonborne -ErrorAction SilentlyContinue | ForEach-Object Id)
    $now = @{}
    if ($pids.Count) {
        Get-NetTCPConnection -OwningProcess $pids -ErrorAction SilentlyContinue | ForEach-Object {
            $now["tcp $($_.OwningProcess) $($_.LocalAddress):$($_.LocalPort) -> $($_.RemoteAddress):$($_.RemotePort)"] = $_.State
        }
        Get-NetUDPEndpoint -OwningProcess $pids -ErrorAction SilentlyContinue | ForEach-Object {
            $now["udp $($_.OwningProcess) $($_.LocalAddress):$($_.LocalPort)"] = "bound"
        }
    }
    $t = Get-Date -Format "HH:mm:ss.fff"
    $lines = @()
    foreach ($k in $now.Keys) { if (-not $prev.ContainsKey($k) -or $prev[$k] -ne $now[$k]) { $lines += "$t + $k [$($now[$k])]" } }
    foreach ($k in $prev.Keys) { if (-not $now.ContainsKey($k)) { $lines += "$t - $k" } }
    if ($lines.Count) { Add-Content -Path $Log -Value $lines -Encoding UTF8 }
    $prev = $now
    Start-Sleep -Milliseconds 150
}
"connwatch: end"
