# Cloud architecture

How the live server is built, as of November 2026, and how to build the same yourself. Real addresses and accounts
are replaced with placeholders: `<VM_IP>`, `<DS_IP>`, `<DOMAIN>`, `<STORAGE_BOX>`, `<GUILD_ID>`.

## Overview

```
 players (Windows)                                      cloud VM (Linux)                     dedicated box (Windows)
 ┌──────────────────────┐   TLS relay 8443/443   ┌──────────────────────────────┐  private  ┌──────────────────────────┐
 │ launcher (Tauri)     │ ─────────────────────▶ │ nginx :80/:443               │  network  │ ds-agent (task DbAgent)  │
 │  local relay         │                        │ dbserver-rs (backend)        │ ◀───────▶ │ up to 7 battle servers   │
 │  127.0.0.1:80/443/   │                        │  dir 8085, gateway 6601-6604 │ 10.0.0.0/ │  = headless game client  │
 │  8085/6601-6604      │                        │  auth/relay, mods, status    │   16      │  + UE4SS server mods     │
 │ game client + mods   │ ── UDP 7777+10·i ─────────────────────────────────────────────────▶ │  UDP 7777, 7787 … 7837   │
 └──────────────────────┘                        │ Discord bot, Forgejo,        │           └──────────────────────────┘
                                                 │ restic backups ─────────────────▶ storage box (backups, artifacts)
 community site: static pages (Cloudflare Pages) └──────────────────────────────┘
 Discord: OAuth app (login) + bot
```

## 1. Backend VM (cloud, Linux)

A small cloud VM (Ubuntu) with a public address `<VM_IP>` and a private one `10.0.0.2` in a private network
`10.0.0.0/16` shared with the battle-server box.

- **`dbserver-rs`** (`core/backend`), systemd service `dbserver`, folder `/opt/dungeonborne`:
  - directory server `10.0.0.2:8085` and gateway `10.0.0.2:6601–6604` (the game's own protocols);
  - HTTPS for the game's login and the launcher API `10.0.0.2:443` / `:80`;
  - launcher relay on the public address `:8443` (TLS, pinned certificate): each game connection from a player's
    launcher is tunnelled to the internal ports above; UDP tunnel for matches when a player's network needs it;
  - ds-agent API `10.0.0.2:9091` (private network only): battle-server slots, match results;
  - status JSON on `127.0.0.1:8090` (`/dbserver/status`), published by nginx;
  - database: RocksDB in `db_rocks`; season files in `seasons/`, server tables in `ExcelConfig/`, client mod
    packages in `mods/` (`manifest_mods.json`);
  - settings: `server.toml` and `dbserver.env` (secrets — Discord OAuth secret, relay key — come from the vault,
    `config/secrets.toml` lists them).
- **nginx** `:80/:443`: the public status page `status.<DOMAIN>` (only `GET /dbserver/status`), Let's Encrypt via
  webroot (`ops/deploy/vm/nginx-status.conf`).
- **Discord bot** (`apps/discord-bot`), `/opt/discord-bot`: bug reports from the launcher, roles, translations.
- **Forgejo** (`ops/forgejo`): the private git server, only on `127.0.0.1:3000` / `:2222`, reached through an SSH tunnel.
- **Backups** (`ops/backup/vm`): restic to the storage box, daily 05:10 UTC; `/opt/dungeonborne` without the live
  database files, plus Discord bot, Forgejo dumps, nginx and systemd configs. Retention 7 daily / 4 weekly / 6 monthly.
- **Kernel tuning:** the relay opens a backend connection per game request; raise conntrack limits
  (`/etc/sysctl.d/90-dungeonborne-conntrack.conf`: max 262144, TIME_WAIT 30 s), otherwise packets drop under load.
- Firewall: only 22 (keys only, no passwords), 80, 443, 8443 are open to the internet.

## 2. Battle-server box (dedicated, Windows)

Game servers must run on real hardware: the game's anti-cheat refuses virtual machines. A dedicated Windows Server
2022 box with a public address `<DS_IP>` for players' UDP and a private `10.0.1.2` (VLAN into the cloud network,
MTU 1400).

- `C:\DungeonbornServer\Dungeonborne` — a game install; `C:\DungeonbornServer\Kit` — the kit from `ops/deploy/robot`
  (`install_gameserver.ps1`, `start_headless.ps1`, `killmutex.ps1`, `ds-agent.toml`).
- **Battle server** = the game client started headless (`-dbserver`, no GPU) with the server mods
  (`DungeonborneRuServer`, `DungeonborneRuClient` with `dbnative.dll`, the server-only pak `zzzzzzz_ServerNet_P`).
  Mod settings next to the mod: `native_data.txt` (backend URL + `realkey`), `native_init.txt = 1`, `r14_mode.txt`,
  `teams.txt`. UE4SS consoles must stay off (no GPU → the console aborts the game).
- **ds-agent** (`core/backend/ds-agent`), a scheduled task `DbAgent`: keeps one server ready, starts more on demand
  (UDP `7777 + 10·i`, up to 7), restarts them between matches, reports to the backend over the private network.
- Remote access: SSH through the VM (`ssh -J root@<VM_IP> admin@192.0.2.20`); RDP only from the private network.

## 3. Storage box

Network storage with sub-accounts (SSH keys, port 23):

| Sub-account | Use | Access |
|---|---|---|
| sub1 | restic repository of the VM backups | VM, read/write |
| sub2 | restic repository of the owner's workstation | workstation |
| sub3 | artifact store (objects by sha256) | read-only, for the team (`ws bootstrap`) |
| sub4 | the same folder | write, for releases (`ws artifacts push`) |

What is in the artifact store and what to take from it is in `docs/STORAGE_EXPORT.md`.

## 4. Domain, site, Discord

- **DNS** (registrar's DNS): `play.<DOMAIN>` → `<VM_IP>` (backend and relay), `status.<DOMAIN>` → `<VM_IP>`,
  `www.<DOMAIN>` → Cloudflare Pages (the community site, `web/landing`), apex → redirect to `www`. The game's own
  directory host name is resolved by the launcher locally (hosts lines or its relay), not by public DNS.
- **Cloudflare Pages:** the static community site.
- **Discord:** an OAuth application for the launcher login (identity = Discord account; the server issues a technical
  Steam-style `open_id` per account), and a bot application.

## 5. Players' side

The launcher (`apps/launcher`) logs in through Discord, downloads the mod packages listed in the server manifest
(`/.well-known/dbserver.json`, `mods[]`), writes the account into the Goldberg Steam emulator, and runs a local relay
on 127.0.0.1 so the unmodified game client talks to the backend through one TLS connection. Matches go directly
over UDP to the battle-server box, or through the relay's UDP tunnel. Some ISPs block cloud providers' addresses;
`docs/RU_INGRESS_PLAN.md` describes an ingress node as the fix (not built).

## 6. Building your own

### Bring your own

The repository has the code, the tables and the tools. These are not in it, and a running server needs them:

| What | Needed for | How to get it |
|---|---|---|
| A copy of the game | players and the battle-server box | your own, legitimately obtained |
| patch paks `zz_LIPatch_P` (login), `zzzzzzz_BPFix_P` (client crash fix), `zzzzzzz_ChatFix_P` (world chat), `zzzzzzz_ServerNet_P` (server branches) | clients and the battle server | `cargo ws dump paks --apply` — built from your copy of the game (the pak key from your vault, `docs/SECRETS.md`) |
| `dbnative.dll` (player data on the battle server) | client mod on the battle server | `game/ue4ss/native/dbnative/build.bat` — Visual Studio Build Tools if installed, else clang-cl from `cargo xtask setup --apply --tool=llvm` (checked on a battle server 19.11) |
| launcher runtime `client.zip` (Goldberg, UE4SS) | players | `cargo xtask setup --apply --tool=goldberg,ue4ss`, then `python apps/launcher/tools/make_client_bundle.py --upstream --ca <your backend CA>` (releases pinned in `config/tools.lock`) |
| a VM, a Windows box, a domain, a Discord application, your own keys and certificates | everything | sections 1–4 and `docs/SECRETS.md` |

`cargo xtask mods build` stops at the first missing one, names it and says how to get it.

### Steps

1. **VM:** Ubuntu, create user `dungeonborne`, `/opt/dungeonborne`; build the backend for Linux
   (`ops/deploy/scripts/wsl_build_clean.sh`), install the systemd unit, `server.toml`, `dbserver.env`; nginx and
   certificates; open 22/80/443/8443.
2. **Battle-server box:** Windows Server, a game install, the kit (`ops/deploy/robot/README.md`), the private network to
   the VM, the scheduled task for ds-agent.
3. **Season and mods:** `cargo xtask balance build`, `cargo xtask mods build --profile=client|server`,
   `apps/launcher/tools/make_mods.py` → packages into `/opt/dungeonborne/mods`.
4. **Discord:** your own OAuth app and bot (`apps/discord-bot/README.md`), IDs into `server.toml` and the bot config.
5. **Launcher:** set your server's address and certificate pin, sign releases with your own key.
6. **Backups:** `ops/backup/vm/setup.sh` with your storage.

Deploying updates: `docs/runbooks/` and `ops/deploy/scripts/` (soft deploy: maintenance on → wait for battles → copy
the database → swap → maintenance off).
