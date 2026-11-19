---
name: dungeonborne-backup
description: 14.11 restic backups to Hetzner Storage Box uXXXXXX (FSN1): VM sub1 nightly 05:10 UTC, PC sub2 (home/re, artifacts, unreal, vault); ops/backup
metadata:
  visibility: public
  type: project
---

Since 14.11 restic backups go to Hetzner Storage Box `uXXXXXX` (Falkenstein). Access by SSH keys only, port 23.

**Prod VM** — sub-account `-sub1`, key `/root/.ssh/storagebox_ed25519`.
- Timer `restic-backup` — 05:10 UTC, the VM runs in UTC.
- Copies `/opt/dungeonborne` except the live `db_rocks` and `stage_*`, plus `/opt/discord-bot`, `/var/backups/forgejo`, `/etc/forgejo`, `/etc/nginx`, `/etc/systemd/system`.
- Retention: 7/4/6.
- First backup 14.11: 2.6 GB → 785 MB, restore verified.

**Workstation** — sub-account `-sub2`, the workstation's key.
- Script `ops/backup/pc/backup.ps1`, copies `home\re`, `home\artifacts`, `home\unreal`, `vault\*.kdbx`, `local.toml`.
- Repository password — `vault\restic-pc.password`.

Repository passwords are moved into KeePassXC (`secrets.toml`: `restic_vm_password`, `restic_pc_password`).

**Why:** step 5 of [[dungeonborne-workspace-plan]] — data that cannot be recreated, and the prod database.

**How to apply:**
- The live database is not included in the backup. A checkpoint through the backend admin API is needed (task not done).
- The workstation schedule is a Windows task, set up by the owner.
- Description — `ops/backup/README.md`.
