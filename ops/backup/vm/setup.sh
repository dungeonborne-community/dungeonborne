#!/bin/bash
# Backups of the production VM to Hetzner Storage Box via restic (ops/backup/README.md).
# Run as root on the VM:  bash setup.sh <storage-box-host> <subaccount>
#   for example: bash setup.sh uXXXXXX.your-storagebox.de uXXXXXX-sub1
# The SSH key /root/.ssh/storagebox_ed25519 must already be in the subaccount's .ssh (the owner does this via the main
# account). The repository password is created here (/root/.config/restic/password, 600) and is not printed —
# the owner moves it into KeePassXC (secrets.toml: restic_vm_password). Safe to re-run.
set -euo pipefail
HOST=${1:?"usage: setup.sh <host> <subaccount>"}
SUB=${2:?"usage: setup.sh <host> <subaccount>"}
HERE=$(cd "$(dirname "$0")" && pwd)
command -v restic >/dev/null || apt-get install -y -qq restic

install -d -m 700 /root/.config/restic
if ! grep -q "^Host storagebox$" /root/.ssh/config 2>/dev/null; then
  cat >> /root/.ssh/config <<EOF

Host storagebox
  HostName $HOST
  User $SUB
  Port 23
  IdentityFile /root/.ssh/storagebox_ed25519
  IdentitiesOnly yes
  ServerAliveInterval 30
EOF
  chmod 600 /root/.ssh/config
fi
if [ ! -s /root/.config/restic/password ]; then
  (umask 077; head -c 32 /dev/urandom | base64 > /root/.config/restic/password)
  echo "== new repository password: /root/.config/restic/password (copy it into KeePassXC)"
fi
cat > /root/.config/restic/env <<'EOF'
RESTIC_REPOSITORY=sftp:storagebox:restic
RESTIC_PASSWORD_FILE=/root/.config/restic/password
RESTIC_CACHE_DIR=/var/cache/restic
EOF
chmod 600 /root/.config/restic/env
set -a; . /root/.config/restic/env; set +a
restic cat config >/dev/null 2>&1 || restic init

install -m 755 "$HERE/backup.sh" /usr/local/sbin/restic-backup
install -m 644 "$HERE/restic-backup.service" /etc/systemd/system/restic-backup.service
install -m 644 "$HERE/restic-backup.timer" /etc/systemd/system/restic-backup.timer
systemctl daemon-reload
systemctl enable --now restic-backup.timer
systemctl list-timers restic-backup --no-pager | sed -n 2p
echo "== done"
