# Backups (restic → Hetzner Storage Box)

Storage Box `uXXXXXX` (Falkenstein) is the current owner's; the address and per-machine subaccounts live in `config/local.toml [backup]`
(`config/local.example.toml` has `uXXXXXX` placeholders: a new owner fills in their own Storage Box). Each machine has its own subaccount
and its own folder: `-sub1` = production VM (`vm/`), `-sub2` = this PC (`pc/`). Login is SSH key only (port 23); the keys
were placed via the main account (the PC's key) into `vm/.ssh/authorized_keys` and `pc/.ssh/authorized_keys`.
The restic repositories are encrypted with their own passwords, kept in KeePassXC (`config/secrets.toml`: `restic_vm_password`,
`restic_pc_password`). Without the password a backup cannot be restored.

## Production VM

- Install: `bash /root/backup/setup.sh uXXXXXX.your-storagebox.de uXXXXXX-sub1` (key `/root/.ssh/storagebox_ed25519`,
  Host `storagebox` in `/root/.ssh/config`, password `/root/.config/restic/password`, env `/root/.config/restic/env`).
- Timer `restic-backup` runs every night at 05:10 UTC (the VM runs in UTC; after `forgejo-backup` at 04:30 UTC): `/opt/dungeonborne` (without the live database
  `db_rocks`, `stage_*`, logs), `/opt/discord-bot`, `/var/backups/forgejo`, `/etc/forgejo`, `/etc/nginx`,
  `/etc/systemd/system`. Keeps 7 daily, 4 weekly, 6 monthly; afterwards `restic check` on 1 % of the data.
- The live database is not backed up: copying RocksDB while it runs gives an inconsistent copy. The backup contains the consistent `backup_*` made before
  deploys. A nightly database snapshot is a backend task: a checkpoint through the admin API, then add it to `backup.sh`.
- Log: `journalctl -u restic-backup --since today`. Snapshots: `set -a; . /root/.config/restic/env; set +a; restic snapshots`.

## This PC

- `backup.ps1 -Init` — password `vault\restic-pc.password` (move it into KeePassXC) and the repository.
- `backup.ps1` — backup: `home\re`, `home\artifacts`, `home\unreal`, `vault\*.kdbx`, `config/local.toml`. The reference game copy,
  dumps, caches and toolchains are not backed up — they are recreated.
- restic lives in `home\tools\restic` (installed by `ws bootstrap` from `config/tools.lock`).

## Restore

- VM: `restic restore latest --target /tmp/r --include <path>`; full restore on a new VM: key for the subaccount via
  the main account, password from KeePassXC, `restic restore latest --target /`.
- PC: `restic -r sftp:uXXXXXX-sub2@uXXXXXX.your-storagebox.de:restic -o "sftp.command=ssh uXXXXXX-sub2@uXXXXXX.your-storagebox.de -p 23 -s sftp" restore latest --target <folder>`.
- Storage Box snapshots (Hetzner, daily) protect against corruption of the repository itself.
