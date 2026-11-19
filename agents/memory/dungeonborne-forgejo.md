---
name: dungeonborne-forgejo
description: 14.11 Forgejo 17.0.1 on the prod VM, local-only (127.0.0.1:3000 http, :2222 ssh), SSH tunnel, nightly dump; ops/forgejo in X:\DBDev\workspace
metadata:
  visibility: public
  type: project
---

On 14.11, after approval, Forgejo 17.0.1 was installed on the prod VM 203.0.113.10.

- **Network:** nothing is exposed externally. HTTP `127.0.0.1:3000`, built-in SSH `127.0.0.1:2222`.
- **Tunnel:** `ssh -N -L 3300:127.0.0.1:3000 -L 2222:127.0.0.1:2222 root@203.0.113.10`; in the browser `http://localhost:3300` (this is ROOT_URL).
- **Files on the VM:** binary `/usr/local/bin/forgejo`, config `/etc/forgejo/app.ini` (640 root:git, secrets generated in place, do not print), data `/var/lib/forgejo`, user `git`.
- **forgejo service:** MemoryMax=600M, CPUWeight/IOWeight=20, Nice=10 — yields to dbserver.
- **Backups:** timer `forgejo-backup` at 04:30, `forgejo dump` into `/var/backups/forgejo`, kept for 7 days.
- **Install and update:** `ops/forgejo/install.sh <ver>` checks sha256 and the GPG signature (key EB11 4F5E … C592 3710).

14.11: administrator account `<owner>`, repository `<owner>/dungeonborne` (private), X:\DBDev\workspace pushed (origin = ssh://git@localhost:2222/<owner>/dungeonborne.git, requires the tunnel).

**Why:** step 5 of [[dungeonborne-workspace-plan]] — our own git server without new open ports on prod.

**How to apply:**
- Changes on the VM — only after the owner's explicit "yes".
- Do not use `pkill -f` inside ssh.
- Log: `journalctl -u forgejo --since -10min`.
