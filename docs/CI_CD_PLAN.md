# CI/CD: plan (Hetzner, no k8s)

Status: 2026-11-03, draft for discussion. None of this has been done yet.

> **17.11: the draft is outdated in details.** The DB is RocksDB (since 11.11, not SurrealDB), git is the owner's Forgejo, build and
> deploy are done by the owner (`docs/CONTRIBUTING.md` — how a member hands in work). The CI ideas below are not done yet.

## 0. Decisions and boundaries

- **Hetzner only.** We no longer use GCloud/GKE, k8s is not needed: the backend is one binary + SurrealDB, the game
  servers won't go into a container/pod anyway (Windows on bare metal, ACE doesn't start under a hypervisor).
- **Hetzner Cloud** — Linux VM: backend, DB, Gitea, HTTPS. **Hetzner Robot** — Windows bare metal: only game
  servers and everything that needs the game files.
- **Gitea + act_runner** — repository and CI (workflows in GitHub Actions syntax, `.gitea/workflows/*.yaml`).
  Forgejo is an equivalent replacement if wanted.
- **Hyper-V must not be enabled on Robot**: no WSL2, no Docker Desktop, no VBS/HVCI — with a hypervisor ACE refuses
  to start ("can not run under a virtual machine", 30.10). So Linux builds run only on the Cloud VM.
- Game files (exe, original paks, `core\backend\ExcelConfig\*.bytes` from the game) and `aes_key.txt` never get into the
  repository. The runner on Robot takes them from the local game install.
- The `Dockerfile` from the backups stays (rework for Compose), `k8s-deployment.yaml` is no longer needed.

## 1. Target infrastructure

| Machine | What runs | How |
|---|---|---|
| **Cloud VM `core`** (Linux x86, 8 vCPU / 16 GB — the surrealdb build is hungry) | `dbserver-rs`, SurrealDB, Gitea, Caddy, act_runner (Linux) | Docker Compose, files in `/srv/dungeonborne` |
| **Robot `game-1`** (Windows Server, bare metal) | game servers (`-port=7777+10·i`), DbTools task, deploy service, act_runner (host mode) | Windows services / scheduler with highest privileges |
| **Storage** (Storage Box or Object Storage) | nightly SurrealDB and Gitea dumps, release archive | restic/rclone from `core` on a schedule |
| **Developer PC** (optional) | act_runner (host mode) for manual runs: `tools\stand\run.ps1`, `bench.ps1` | label `dev-pc`, jobs only manually |

Network:
- **vSwitch Robot ↔ Cloud Network**: the backend and game servers talk over a private network (ds-agent `DS_AGENT_PORT`
  9091, `/ds/player_result`, admin API, SurrealDB). These ports are not opened to the outside.
- One location for all machines; for players from Russia HEL1 is more convenient (it has both Robot and Cloud).
- Open to the outside from `core`: 443 (Caddy: Gitea, launcher web API, manifest and mod packages), the launcher relay port
  (`[relay]` in `server.toml`). The direct 80/8085/6601–6604 are no longer needed by players — everything goes via the relay.
- Open to the outside from `game-1`: game server UDP (7777–7857 with 8 instances), RDP only from an IP allowlist.
- Hetzner Firewall on both machines + OS firewall.

## 2. Repositories

Two repositories in Gitea:

| Repository | From | What's inside |
|---|---|---|
| `dungeonborne` | `<repo>` as is | backend (`src`, `build.rs`, `assets`), `xtask`, `balance`, mods (`game\ue4ss\Mods`, `game\ue4ss\native\dbnative`), the `zzzzz_RebuildPrivate_P` pak source, `dbtest`, `tools`, `docs`, new `deploy\` |
| `launcher` | `<data>\launcher` | Tauri launcher, mock-server |

The first import — without rearranging folders: scripts (`deploy_paks.ps1`, `tools\stand\*.ps1`) and sessions working in
parallel depend on the current paths. Reorganisation, if needed, is a separate step later.

**`.gitignore` (draft):**
```
target/  target_*/  **/target/
surreal_saves*/  rpc_dump/  probe/logs/  dbtest/runs/  docs/bench/*.csv
*.rar  *.zip  *.pak  *.stackdump
dist/
ExcelConfig/  ExcelConfigjson/  from_game/  DungeonbornePVE/
cert.pem  key.pem  *_key.pem  *.pfx  server.toml  .env  aes_key.txt
dump_*.bin  sniffer_log*.txt  data*.md  llm_context_*.md
```
Before the first commit — review the final file list by eye: the `backups` root has many one-off dumps and
archives, better to weed them out right away than to clean the history later. Separately: `src\rpc\handlers — Copy.rs` (a Windows copy, named in Russian) is junk.

**Large binaries** (UE4SS, Goldberg, `repak.exe`, generated paks) — not in git, but in the Gitea package registry
(generic packages) with versions; workflows download them by version.

**Branches:** `main` always builds and passes CI; work happens in `feat/*`, `fix/*` branches via PRs. Parallel
Claude sessions each work in their own branch.

## 3. Versions and artifacts

Each component is versioned separately by tag:

| Component | Tag | Artifact | Where it goes |
|---|---|---|---|
| Backend | `backend-v0.N.M` | Docker image `dbserver-rs:<version>` | Gitea container registry |
| Client mods | `mods-v0.N.M` | `dbmod/1` packages (zip + sha256) per mod | Gitea generic packages → published on `core` (`/mods/…`) |
| Game server kit | built from the same `mods-v…` | `gameserver-<version>.zip` | Gitea generic packages |
| Launcher | `launcher-v0.N.M` | `dungeonborne-launcher.exe` + sha256 | generic packages → link in the manifest |

The version and commit hash are embedded into the backend binary at build time (`env!` from build.rs), returned in
`/dbserver/status` and in the log at start — so you can see what is really running.

## 4. Pipelines

### 4.1. CI on every push and PR (fast, no game)

| Job | Runner | What it does |
|---|---|---|
| `backend-check` | linux | `cargo fmt --check`, `cargo clippy`, `cargo test`, Linux build |
| `auth-e2e` | linux | backend with `tools/probe/auth_test/server.toml` + `tools/py/auth_e2e.py` (now 20/20) |
| `lua-check` | windows (`game-1`) | `luac -p` of all `game\ue4ss\Mods\**\*.lua` and the Lua inside `zzzzz_RebuildPrivate_P` |
| `ps-check` | windows | parse all `*.ps1` with the PowerShell parser (catches errors like the one with bench.ps1) |
| `dbnative-build` | windows | build `dbnative.dll` (MSVC) |
| `balance-check` | windows | `cargo xtask balance build` without `--deploy` (the tables build) |
| `launcher-check` | windows | `cargo test` + `mock_e2e` in the `launcher` repository |

Rule: a PR into `main` is merged only with green CI.

### 4.2. Backend → `core`

Trigger: tag `backend-v*` (staging — every merge into `main`, see §8).
1. Build the image (cargo cache in the runner volume), push to the Gitea registry.
2. On `core`: SurrealDB dump to storage (snapshot before rollout).
3. Maintenance announcement: the backend is switched to maintenance mode via the admin API — new logins get
   `EN_ERR_MAINTENANCE` (the code already exists in the protocol), running matches finish on the game servers.
4. `docker compose pull && docker compose up -d dbserver`.
5. Check: `/dbserver/status` returns the new version, `online=true`; a test account login (`tools/py/auth_login.py`).
6. Check failed → roll back to the previous image tag (and, if the schema changed, restore the dump).

Now the backend keeps connections in memory, so a rollout = a short disconnect for players in the lobby. That's fine
for our scale; do it between peaks.

### 4.3. Mods → packages for the launcher

Trigger: tag `mods-v*`. Runner `game-1` (needs repak, MSVC, game tables).
1. `cargo xtask balance build` → `zzzzzz_Balance_P.pak`; `repak pack` → `zzzzz_RebuildPrivate_P.pak`.
2. Build `dbnative.dll`; UE4SS mods (`DungeonborneRuClient`, …) — as is.
3. Pack into `dbmod/1` packages per `SERVER_MANIFEST_DRAFT.md`: sha256 of the package and files, `depends`/`conflicts`.
4. Compute `modset_sha256` per the `LAUNCHER_SPEC.md` §10 contract.
5. Publish the packages on `core` (`/mods/<id>/<version>/…`) — **without** switching the manifest.
6. Build `gameserver-<version>.zip`: the same paks and mods + `DungeonborneRuServer`, `dbtools`, launch scripts +
   **server-only** paks `game\bp-patches\pak_server\*` (`zzzzzzz_ServerNet_P`, 03.11; not for clients — in
   standalone `IsServer` = true). IoStore paks — three files each (`.pak .utoc .ucas`): `install_gameserver.ps1` from
   the 30.10 kit copies only `*.pak`, the new kit must copy all three (like `pak_server\install_server_paks.ps1`).
   Before CI — the manual kit `dist\gameserver-servernet-20261103.zip` (paks + `install_server_paks.ps1 -GameRoot …`).

### 4.4. Game server → `game-1` (smooth replacement)

The game process under ACE can't be killed from outside, so only via the mod itself:
1. The deploy service on `game-1` (PowerShell under SYSTEM or a task with highest privileges, like DbTools) downloads
   `gameserver-<version>.zip`, checks sha256, unpacks into `releases\<version>`.
2. Asks the backend to put the `game-1` instances into **drain**: no new rooms are handed out to them.
3. Waits for current matches to end (timeout, e.g. 40 min; then — forced, with a warning in the match chat).
4. Writes `quit` into `dbserver[-<port>]-control.txt` of each instance, waits for the processes to exit.
5. Switches mods/paks in the game install to the new version (the old one stays in `releases\` for rollback).
6. Starts the instances, waits for `phase -> ready` in `dbserver[-<port>]-status.json`, removes the drain.
7. No `ready` → restore the previous version and restart; in CI — red status.

Needs work:
- **backend:** admin API `drain/undrain` for a pool instance (`DS_POOL`) and a drain flag in room assignment;
- **DbTools/deploy service:** `deploy`, `stop-all`, `start-all` actions in the allowlist; for the future — removing
  hung processes (on 03.11 two servers didn't exit after `os.exit`, only the user could remove them).

### 4.5. Release for players: client + server order

`modset` in the relay preamble makes the rollout atomic:
1. The new version's packages are already on `core` (§4.3, step 5).
2. The game servers are updated (§4.4).
3. Switching the manifest (`mods[]`, `modset_sha256`) to the new version — one file edit on `core`.
4. Old clients get `modset` from the relay → the launcher downloads the new version.

Rollback — restore the previous manifest and the previous `gameserver`. While the `modset` check in the relay is not enabled (stage E5
of `LAUNCHER_SPEC.md`), the order is the same, but old clients are not cut off — enable it before the first public release.

### 4.6. Launcher

Trigger: tag `launcher-v*`. Runner `game-1` (Windows, MSVC, `trunk`, `tailwindcss`; don't set `NO_COLOR` — it breaks trunk).
1. `cargo tauri build` → exe, sha256.
2. Publish on `core` and a link/version in the server manifest; updating — via the built-in Tauri updater (later) or
   a "download the new version" message.
3. Code signing — optionally later (without it SmartScreen complains about a fresh exe).

### 4.7. Smoke test with the game

Runner `game-1` (or `dev-pc`), nightly and on demand, against the **staging** backend:
- `dbtest\run.ps1 -Match native`: headless server + client, login, match, game over, `player_result` in the backend;
- `tools\stand\loginseries.ps1`: a series of logins (now 10/10);
- once a week `bench.ps1` → `docs\bench` (watch for tick degradation).

Result: logs into job artifacts, red status on failure. The smoke test doesn't block PRs (long and
needs a machine with the game), but blocks mod rollout to prod.

## 5. Secrets

| Secret | Where it is stored | Who uses it |
|---|---|---|
| `DISCORD_CLIENT_SECRET` | `/srv/dungeonborne/.env` on `core` (mode 600) | backend |
| relay/TLS certificates | Caddy gets the public one itself (Let's Encrypt); `relay_*.pem`, `cert.pem` — files on `core` | backend, Caddy |
| SurrealDB password | `.env` on `core` (now `secret_pass` in plain text in the k8s manifest — change it) | backend, DB |
| deploy SSH key for `core` | Gitea Actions secret | workflow §4.2 |
| package download token | Gitea secret + a file on `game-1` | deploy service |
| `aes_key.txt` | local only, not passed to CI and not printed | — |

Keys are not committed to the repository (see `.gitignore`); on the first import check that `cert.pem`/`key.pem` from
the `backups` root did not get into the history.

## 6. Backups

- SurrealDB: nightly dump + before every backend rollout; keep 14 nightly and 8 weekly.
- Gitea: `gitea dump` nightly (repositories, issues, packages).
- Cloud VM: Hetzner snapshots (Backups) as the last line.
- Once a month — a test restore on a temporary VM (otherwise the backup is not verified).

## 7. Observability (minimum)

- Backend logs: `docker compose logs`, with rotation; game server logs — on `game-1`, archived on rollout.
- Uptime check of `/dbserver/status` and game server `ready` → notification to a Discord channel (webhook).
- The same webhook gets CI/rollout statuses.

## 8. Environments

| Environment | Backend | Game servers | Rollout |
|---|---|---|---|
| dev | developer PC (`cargo run`) | PC (DbTools) | by hand, as now |
| staging | a second Compose project on `core` (own ports and DB) | 1 instance on `game-1` (separate port) or PC | automatically from `main` |
| prod | `core` | `game-1` | by tag + manual confirmation |

## 9. Rollout stages

| Stage | What | Done when |
|---|---|---|
| **1. Repository** | Cloud VM `core`, Gitea behind Caddy, import of `dungeonborne` and `launcher` with `.gitignore`, Gitea backups | history in Gitea, no secrets in it |
| **2. CI** | act_runner on `core` and `game-1`, workflow §4.1 | a PR shows green/red status |
| **3. Backend on Hetzner** | Linux build, Compose (backend + SurrealDB + Caddy), DB migration, §4.2 | the backend runs on `core`, rollout by tag with rollback |
| **4. Network** | vSwitch Robot ↔ Cloud Network, firewall | the game server on `game-1` reaches the backend at a private address |
| **5. Mods and manifest** | §4.3 + §4.5, `modset` check in the relay | the launcher installs mods from the manifest, an old modset is cut off |
| **6. Game server rollout** | drain in the backend, deploy service, §4.4 | updating `game-1` without manual actions and without dropping matches |
| **7. Smoke test** | §4.7 nightly | red status when login/match/result breaks |

Stages 1–3 are independent of the game and pay off right away; 5–6 depend on the launcher and the DS pipeline.

## 10. Open questions

- The backend hasn't been built under Linux with the current code yet (Dockerfile from the GKE era) — check at stage 3;
  `LOCALAPPDATA` in `raid_handlers.rs` is a Windows/local-mode-only path.
- `core` plan: one VM for everything or a separate build VM (a backend build may interfere with the live backend; at least —
  limit the runner to one job and a CPU quota).
- Where to keep the staging game server instance: on `game-1` (takes a slot from prod) or on a PC.
- How long to wait for matches to end on drain and what to do with ones that drag on.
- Backup storage: Storage Box (cheaper, SFTP/restic) or Object Storage (S3, handier for mod packages).
