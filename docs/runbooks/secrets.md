# Secrets: inventory and KeePassXC

What is common to everyone (which database belongs to whom, `ws secrets` commands, rules) — `docs/SECRETS.md`.
This page covers the owner's database.

Inventory of 14.11 (docs/WORKSPACE_PLAN.md, step 5). **There are no values here and never will be** — only what it
is, where it lives, who uses it. Values are in the KeePassXC database `X:\DBDev\vault\dungeonborne.kdbx`. There are no
secrets in git or in `home` (`ws check`, secrets rule).

## How it works

- Database: `X:\DBDev\vault\dungeonborne.kdbx`; only the owner knows the master password. A key file (optional) is
  kept not in `vault\` but on a separate medium.
- File secrets (TLS key, signing key, .env) are stored in entries as attachments; on disk they remain only where a
  running service reads them (VM, Robot) — listed below.
- The agent does not enter passwords or keys and does not open the database. If a file is needed for work (for
  example, deploying a key), the owner exports the attachment to a temporary folder and gives the path; the file is
  deleted after the work.
- SSH keys: can be kept in KeePassXC and served to the Windows OpenSSH agent through KeePassXC's built-in SSH Agent —
  then no key file in `~\.ssh` is needed.
- Secrets marked "rotate" in the tables below are reissued, not copied, when the workspace moves to a new machine
  (FIX_LIST, 07.11).

## What goes where (the database is created by `ws secrets init`, names come from `config/secrets.toml`)

| # | Where it is now | Group in the database | Entry (Title) | Where | Required |
|---|---|---|---|---|---|
| 1 | `<data>\deploy\tls\backend.key` | Prod VM | Relay TLS key | attachment **`backend.key`** (Advanced → Attachments → Add) | yes |
| 2 | `<data>\deploy\vm\dbserver.env` | Prod VM | dbserver.env | attachment **`dbserver.env`** (Advanced → Attachments → Add) | yes |
| 3 | Windows variable `DISCORD_CLIENT_SECRET` (Machine) or the line `DISCORD_CLIENT_SECRET=` in `dbserver.env` | Discord | OAuth client secret | **Password** field | yes |
| 4 | `<data>\deploy\release\release_key.secret` | Launcher | Release signing key | attachment **`release_key.secret`** (Advanced → Attachments → Add) | yes |
| 5 | `<data>\Discord\.env` | Discord | Bot | attachment **`.env`** (Advanced → Attachments → Add) | yes |
| 6 | `<data>\aes_key.txt` (file contents) | Game | Pak AES key | **Password** field | yes |
| 7 | `%USERPROFILE%\.ssh\id_ed25519` | Access | SSH id_ed25519 | attachment **`id_ed25519`** (Advanced → Attachments → Add) | yes |
| 8 | `%USERPROFILE%\.ssh\id_rsa` | Access | SSH id_rsa | attachment **`id_rsa`** (Advanced → Attachments → Add) | no |
| 9 | `%USERPROFILE%\.ssh\ruvds_ingress` | Access | SSH ruvds_ingress | attachment **`ruvds_ingress`** (Advanced → Attachments → Add) | no |

Attachment names must match the "Where" column letter for letter (`ws secrets check` looks them up by name).

## Project secrets

| Database entry | What it is | Where now (copies) | Who uses it | Rotate on a new machine |
|---|---|---|---|---|
| Prod VM / relay TLS | private TLS key of the launcher relay (`[relay] key`) | `<data>\deploy\tls\backend.key`; on the VM — the file from `server.toml` `[relay] key` | backend on the VM; the launcher remembers the public key (TOFU) | yes — with launchers moving to the new key |
| Prod VM / dbserver.env | prod backend variables; the secret among them is `DISCORD_CLIENT_SECRET` | `<data>\deploy\vm\dbserver.env`, `G:\…\backups\dist\backend-linux-20261103\dbserver.env`; on the VM — `/opt/dungeonborne` | systemd `dbserver` | yes (Discord secret) |
| Discord / OAuth client secret | secret of the Discord-login OAuth application | Windows variable (Machine) `DISCORD_CLIENT_SECRET`; `dbserver.env` | backend (`[auth.discord] client_secret_env`), `tools/probe/auth_test/run_launcher.ps1` | yes |
| Launcher / signing key | 32-byte ed25519 seed for signing launcher releases | `<data>\deploy\release\release_key.secret` | `apps/launcher/tools/release.py sign`; the public key is built into the launcher (`builtin.rs`) | yes — new key = new public key in the launcher |
| Discord / bot | bot `DISCORD_TOKEN`, `DISCORD_GUILD_ID`, `DEEPL_KEY` | `<data>\Discord\.env`; on the VM — the `discord-bot` service | Discord bot, `ops/discord` (setup, audit) | yes (bot token, DeepL) |
| Game / pak AES | the game's pak decryption key (third-party, the publisher's) | `<data>\aes_key.txt`, `C:\Games\Dungeonborne-res_20261104\aes_key.txt` | FModel, retoc — pak analysis (only with the owner's explicit go-ahead) | no (not our key) — but keep it only in the database |
| Access / SSH id_ed25519 | this machine's key | `~\.ssh\id_ed25519` (+ `.pub`) | ssh to the VM `root@203.0.113.10` and through it to Robot | yes |
| Access / SSH id_rsa | old key | `~\.ssh\id_rsa` (+ `.pub`) | to be determined; if not needed — remove from `authorized_keys` on the servers | yes or delete |
| Access / SSH ruvds_ingress | key for the ruvds ingress node (RU_INGRESS_PLAN) | `~\.ssh\ruvds_ingress` (+ `.pub`) | ssh to the ruvds node | yes |
| Robot / ds-agent | ds-agent ↔ backend link (`DS_REQUIRE_TOKEN`, session tokens) | on Robot `C:\DungeonbornServer\Kit` (ds-agent.toml); the repository has a sample without the secret | ds-agent | check when moving |

The `cert.pem`/`key.pem` pairs of 1391/1675 bytes in `DungeonbornePVE`, `china-*`, `Single-player game`, `framework`,
`dbserver-rs`, `trash`, `research\backend`, `new_class\server` (and a copy in `X:\DBDev\home\unreal\new-class\server`)
are the PVE patch certificate (third-party, a public set), not our key. Not needed in the database; copies in `home`
need not be kept.

## Procedure

The machine-readable inventory is `config/secrets.toml` in the root (names, groups, entries, field or attachment,
environment variable). Working with the database is done via `ws secrets` (no custom cryptography: the database is
read by the `keepass` crate; ws never overwrites an existing database):

| Command | What it does |
|---|---|
| `ws secrets list` | inventory: what is needed, where in the database, who uses it |
| `ws secrets init` | new database `X:\DBDev\vault\dungeonborne.kdbx` with groups and empty entries per the inventory; asks for the master password twice; refuses if the file exists |
| `ws secrets check` | what is not filled in (does not print values) |
| `ws secrets export [--tier=team] [--out=<file>]` | team subset database (below) |
| `ws secrets run <names\|all> -- <command>` | secrets into the command's environment: a value goes into a variable, a file into the temporary folder `home\cache\tmp` with its path in a variable; the folder is deleted after the command |

`--vault=env` — without a database: values and file paths are taken from environment variables (for CI and servers).
Example: `ws secrets run release_signing_key -- powershell -c "python tools/release.py sign --key $env:RELEASE_SIGNING_KEY"`.

1. Install KeePassXC (`winget install KeePassXCTeam.KeePassXC` or keepassxc.org).
2. `ws secrets init` (in a terminal, from `X:\DBDev\workspace`) — creates the database with the structure. The owner
   enters the master password; it is not shared with the agent.
3. Open the database in KeePassXC and fill it in: for entries with `kind = "value"` — the Password field; for
   `kind = "file"` — Advanced → Attachments → Add, with the attachment name exactly as in the inventory
   (`backend.key`, `release_key.secret`, `.env`, …). Save; put a copy of the `.kdbx` on E: (until Storage Box).
4. `ws secrets check` — 0 errors (optional SSH keys are warnings).
5. Deleting copies from disks (G:, E:, F:) is a separate step, as a list and with confirmation: files on the VM and Robot stay.
6. New machine: reissue everything with `rotate = true`, remove old keys from the servers.

If KeePassXC cannot open a database created by `ws secrets init`, create the database in KeePassXC itself with the
same groups and entry names (`ws secrets list`): ws reads any KDBX4.

## Team database (`ws secrets export`, 15.11)

A team developer gets not the owner's database but an export from it: only entries with `visibility = "team"` (and
`public`) in `config/secrets.toml`. `owner` entries are absent from the export entirely — neither the value nor an
empty entry.

1. Owner: `cargo ws secrets export` → `X:\DBDev\vault\team.kdbx` (`--out=<file>`, `--tier=team` by default).
   Asks for the owner database password, then the new team database password twice. The source database is not
   changed, an existing file is not overwritten. After writing, the database is opened with the new password and every
   value is compared with the source. Prints names: what was copied, what is empty in the owner database, how many
   remain owner-only.
2. The file goes to the developer, the password through another channel.
3. In the developer's `config/local.toml`:
   ```toml
   [vault]
   path = 'X:/DBDev/vault'
   db = 'team.kdbx'
   tier = 'team'
   ```
   `ws secrets check` and DevKit then do not look for `owner` entries (the "owner only" line is informational, not an
   error), and `ws secrets run <owner name>` refuses with an explanation.

Currently all entries are `owner`, so the team database comes out empty. What can be given to the team is the owner's
call: mark the entry `visibility = "team"` in `config/secrets.toml` (preferably a separate entry for the test stand,
not the prod key).

## Cleaning up copies

- **14.11, database filled** (`X:\DBDev\vault\dungeonborne.kdbx`, a copy on another drive).
- **14.11, old copies deleted (group A, with the owner's confirmation):**
  `<repo>\dist\backend-linux-20261103\dbserver.env`;
  in `C:\Games\Dungeonborne-res_20261104\`: `aes_key.txt`, `deploy\release\release_key.secret`, `deploy\tls\backend.key`,
  `deploy\vm\dbserver.env`, `Discord\.env`, `emulator\builds\backend-linux-20261103\dbserver.env`.
- **Group B — working copies, kept until the move to `X:\DBDev\workspace`** (deployment from G: reads them):
  `<data>\deploy\tls\backend.key`, `deploy\vm\dbserver.env`, `deploy\release\release_key.secret`,
  `<data>\Discord\.env`, `<data>\aes_key.txt`, Windows variable `DISCORD_CLIENT_SECRET`.
  After the move, deployment takes them via `ws secrets run`, and the files are deleted with confirmation.
- **Group C — SSH keys in `%USERPROFILE%\.ssh`** — until the new machine (they are reissued there).
