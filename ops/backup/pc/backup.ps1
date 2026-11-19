# Backup of this PC to Hetzner Storage Box via restic (ops/backup/README.md).
#   powershell -ExecutionPolicy Bypass -File ops\backup\pc\backup.ps1            # backup
#   powershell -ExecutionPolicy Bypass -File ops\backup\pc\backup.ps1 -Init      # first time: password and repository
#   powershell -ExecutionPolicy Bypass -File ops\backup\pc\backup.ps1 -Snapshots # list snapshots
# What is backed up: home\re (research data), home\artifacts, home\unreal, vault\*.kdbx, local.toml.
# Not backed up (recreated): the reference game copy, dumps, caches, toolchains, runs.
# Repository password: vault\restic-pc.password (created by -Init); the owner moves it into KeePassXC
# (secrets.toml: restic_pc_password). Address and subaccount: local.toml [backup] host, pc_user.
param([switch]$Init, [switch]$Snapshots)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\..\..\..\tools\stand\paths.ps1"     # $DbHome, $WsRoot, Get-LocalValue, Resolve-WsPath
$H = (Resolve-Path $DbHome).Path
$restic = Get-ChildItem (Join-Path $H 'tools\restic') -Filter 'restic*.exe' | Select-Object -First 1 -ExpandProperty FullName
if (-not $restic) { throw 'restic not found in home\tools\restic - run ws bootstrap' }
$sbHost = Get-LocalValue 'backup' 'host'
$sbUser = Get-LocalValue 'backup' 'pc_user'
if (-not $sbHost -or -not $sbUser) { throw 'missing local.toml [backup] host / pc_user' }
$vault = (Resolve-Path (Resolve-WsPath (Get-LocalValue 'vault' 'path'))).Path
$pwFile = Join-Path $vault 'restic-pc.password'

$env:RESTIC_REPOSITORY = "sftp:${sbUser}@${sbHost}:restic"
$env:RESTIC_PASSWORD_FILE = $pwFile
$env:RESTIC_CACHE_DIR = Join-Path $H 'cache\restic'
$sftp = "sftp.command=ssh ${sbUser}@${sbHost} -p 23 -o BatchMode=yes -o ServerAliveInterval=30 -s sftp"

if ($Init) {
    if (-not (Test-Path $pwFile)) {
        $bytes = New-Object byte[] 32
        [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
        [IO.File]::WriteAllText($pwFile, [Convert]::ToBase64String($bytes))
        "new repository password: $pwFile - move it into KeePassXC (Storage Box / restic PC)"
    }
    # no repository yet: restic writes to stderr; with Stop that would abort the script
    $ErrorActionPreference = 'Continue'
    & $restic -o $sftp cat config 2>&1 | Out-Null
    $exists = ($LASTEXITCODE -eq 0)
    $ErrorActionPreference = 'Stop'
    if (-not $exists) { & $restic -o $sftp init } else { 'repository already exists' }
    return
}
if ($Snapshots) { & $restic -o $sftp snapshots; return }

$paths = @((Join-Path $H 're'), (Join-Path $H 'artifacts'), (Join-Path $H 'unreal'), (Join-Path $WsRoot 'config\local.toml')) +
         @(Get-ChildItem $vault -Filter '*.kdbx' | ForEach-Object FullName)
& $restic -o $sftp backup --tag nightly --exclude '*.tmp' @paths
if ($LASTEXITCODE -ne 0) { throw "restic backup: $LASTEXITCODE" }
& $restic -o $sftp forget --tag nightly --keep-daily 7 --keep-weekly 4 --keep-monthly 6 --prune
