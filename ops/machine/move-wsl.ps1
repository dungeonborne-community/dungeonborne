# WSL Ubuntu disk (ext4.vhdx, ~10 GB on C:) -> DB_HOME\toolchains\wsl (docs/WORKSPACE_PLAN.md, section 4).
# Run by a human: it stops WSL (backend builds in WSL will be interrupted). Needs WSL 2.3+ (`wsl --version`).
#   powershell -ExecutionPolicy Bypass -File ops\machine\move-wsl.ps1          # show
#   powershell -ExecutionPolicy Bypass -File ops\machine\move-wsl.ps1 -Apply   # move
param([switch]$Apply, [string]$Distro = 'Ubuntu')
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\..\tools\stand\paths.ps1"
$dst = Join-Path (Resolve-Path $DbHome).Path 'toolchains\wsl'
"WSL $Distro -> $dst"
if (-not $Apply) { "Nothing changed. To move: -Apply (WSL will be stopped)"; return }
New-Item -ItemType Directory -Force $dst | Out-Null
wsl --shutdown
wsl --manage $Distro --move $dst
wsl -d $Distro -- echo "WSL $Distro runs from $dst"
