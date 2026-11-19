# PVE reference capture (run in an ADMIN PowerShell): starts PVE LoginService (443/80) and SoloLobby (127.0.0.1:6601-6604)
# on a COPY of the PVE save, so the real PVE save is not changed. SoloDir is NOT started: port 8085 is our DirServer in
# PVE_SNIFF mode (it sends the game to the sniffer on 127.0.0.2:6601-6604 -> SoloLobby). Close this window to stop both.
#   powershell -NoProfile -ExecutionPolicy Bypass -File <repo>\tools\pve_capture_admin.ps1
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$pve = Join-Path $root 'DungeonbornePVE'
$files = Join-Path $pve 'files'
$saveSrc = Get-ChildItem $pve -Directory | Where-Object { Test-Path (Join-Path $_.FullName 'gateway.db') } | Select-Object -First 1
if (-not $saveSrc) { throw "PVE save (gateway.db) not found under $pve" }
$save = Join-Path $root 'probe\pve_save_capture'
if (-not (Test-Path $save)) { New-Item -ItemType Directory -Force $save | Out-Null; Copy-Item (Join-Path $saveSrc.FullName 'gateway.db') $save }
foreach ($p in 443, 80, 6601) {
    # only 127.0.0.1 / 0.0.0.0 conflict; the sniffer itself listens on 127.0.0.2:6601-6604
    $busy = netstat -ano -p TCP | Select-String -Pattern "^\s*TCP\s+(127\.0\.0\.1|0\.0\.0\.0):$p\s+\S+\s+LISTENING"
    if ($busy) { Write-Host "port $p is busy: $busy" -ForegroundColor Red; Read-Host 'Enter to exit'; exit 1 }
}
$out = Join-Path $root 'probe\pve_login_http'
New-Item -ItemType Directory -Force $out | Out-Null
$login = Start-Process (Join-Path $files 'login\LoginService.exe') -WorkingDirectory (Join-Path $files 'login') -PassThru `
    -ArgumentList @('-https', '127.0.0.1:443', '-http', '127.0.0.1:80', '-certs', (Join-Path $files 'login'), '-out', $out)
$lobby = Start-Process (Join-Path $files 'lobby\SoloLobby.exe') -WorkingDirectory (Join-Path $files 'lobby') -PassThru `
    -ArgumentList @('-data', $save)
Write-Host "LoginService pid $($login.Id), SoloLobby pid $($lobby.Id), save copy: $save" -ForegroundColor Green
Write-Host 'Now: Claude starts the sniffer (PVE_SNIFF), then launch Dungeonborne.exe. Press Enter here to STOP both services.'
Read-Host | Out-Null
Stop-Process -Id $login.Id, $lobby.Id -Force -ErrorAction SilentlyContinue
Write-Host 'stopped'
