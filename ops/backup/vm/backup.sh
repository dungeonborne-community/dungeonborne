#!/bin/bash
# Nightly backup of the production VM to Storage Box (restic). Installed by ops/backup/vm/setup.sh as /usr/local/sbin/restic-backup.
# The live database db_rocks is not backed up (copying RocksDB while it runs gives an inconsistent copy): the backup gets the
# consistent backup_* made before deploys. A nightly database snapshot comes once the backend can checkpoint (ops/backup/README.md).
set -euo pipefail
set -a; . /root/.config/restic/env; set +a
restic backup --one-file-system --tag nightly \
  --exclude /opt/dungeonborne/db_rocks \
  --exclude '/opt/dungeonborne/stage_*' \
  --exclude '*.log' --exclude '/opt/discord-bot/target' \
  /opt/dungeonborne /opt/discord-bot /var/backups/forgejo \
  /etc/forgejo /etc/nginx /etc/systemd/system /root/.config/restic/env
restic forget --tag nightly --keep-daily 7 --keep-weekly 4 --keep-monthly 6 --prune
restic check --read-data-subset=1%
