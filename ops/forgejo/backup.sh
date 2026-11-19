#!/bin/bash
# Nightly Forgejo backup: forgejo dump (repositories, database, settings) into /var/backups/forgejo, kept for 7 days.
# Later: a copy to Storage Box (docs/WORKSPACE_PLAN.md, step 5).
set -euo pipefail
OUT=/var/backups/forgejo
cd "$OUT"
sudo -u git /usr/local/bin/forgejo dump --config /etc/forgejo/app.ini --type tar.gz --file "$OUT/forgejo-$(date +%F).tar.gz"
find "$OUT" -name 'forgejo-*.tar.gz' -mtime +7 -delete
ls -l "$OUT"
