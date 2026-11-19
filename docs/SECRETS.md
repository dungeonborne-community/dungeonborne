# Secrets: how to work with them

For everyone who has a copy of the workspace. There are no values in git and there will not be: `config/secrets.toml` is only the edition spec (name,
environment variable, who uses it, `visibility` level). The values themselves are in a KeePassXC database (`.kdbx`) outside the
repository, usually `<root>\vault\`. `ws check` (the secrets rule) keeps keys, `.env` and password databases out of git.

## Which database who has

| Who | Database | What is in it |
|---|---|---|
| owner | their own full database | all entries; details — their document in `docs/runbooks` (not in the team copy) |
| team member | `team.kdbx` from the owner | only entries with `visibility = "team"` (and `public`); owner entries are not in it at all |
| public edition | your own empty one (`ws secrets init`) | you fill it with your own values for your own server |

A team member gets the `team.kdbx` file and its password over different channels (`docs/CONTRIBUTING.md`, "Setup from scratch").
In `config/local.toml`:

```toml
[vault]
path = 'ws:../vault'   # folder with the database
db = 'team.kdbx'
tier = 'team'          # entries above this level are not looked up
```

## The pak key

The game's paks are encrypted with an AES-256 key, which is not in this repository. Our own paks do not need it (they
are unencrypted); it is needed only to read the game's paks: the patch paks are built from assets in them
(`cargo ws dump paks`), and `cargo ws dump` extracts data from them. The key is public: community collections of AES
keys for UE4/UE5 games list it. You find it yourself — an AI assistant can help you search. Put it into
`config/local.toml`, which is not in git:

```toml
[re]
aes_key = '0x…'   # 0x + 64 hex digits
```

With a vault (`ws secrets init`), keep it there as `game_aes_key` instead: `ws dump` takes it from the vault session.
`cargo ws dump paks` without `--apply` says whether a key is set. An AI agent never writes the key into a tracked file
and never prints it (`CLAUDE.md`, Secrets).

## Commands

- `cargo ws secrets list` — the spec: which secrets exist, who has which level.
- `cargo ws secrets check` — whether all entries of your level are filled (owner entries in the team copy are an "owner
  only" note, not an error).
- `cargo ws secrets run <name> [<name>…] -- <command>` — run a command with the secret in the environment variable from the spec.
  A person enters the database password; the value lives only in that command's environment. Example: loading data from the store
  `cargo ws secrets run artifact_store -- cargo ws bootstrap --profile=dev --apply`.
- A secret with a `local` field in the spec is written by `ws bootstrap` under `ws secrets run` into `config/local.toml`
  — for programs that read it from settings; local.toml is not in git. No entry uses it now: with a vault the pak key
  comes from the vault session; without one you write `[re] aes_key` yourself (above).
- `cargo ws secrets init` — a new empty database from the spec (public edition, your own machine).
- `cargo ws secrets unlock [--for=4h] [--idle=60m] [--only=<a,b>]`, `status`, `lock` — the vault session, below.

## Vault session

The master password once per working session, not per command. `cargo ws secrets unlock` (or DevKit, page "Game
data") reads the database once and gives the **session secrets** to a small background process, the vault agent.
After that the commands they are meant for get them without asking — also when an AI agent runs them:

| Secret | Given only to |
|---|---|
| `game_aes_key` | `ws dump` |
| `artifact_store` | `ws bootstrap`, `xtask setup` |

`cargo ws dump --apply`, `cargo ws bootstrap --apply` and `cargo xtask setup --apply` ask the session themselves; `ws secrets run <name> -- <command>`
asks it first and falls back to the password when the session is closed or refuses.

- **Which secrets.** A `secrets.toml` entry with `session = true` and `session_for = ["ws dump", …]` (`"ws dump"`
  matches `ws dump …` and `cargo ws dump …`). Only `value` entries, never `owner` ones: prod keys, signing keys and
  tokens always ask for the password (`ws secrets` refuses a `secrets.toml` that marks them).
- **Who may ask.** The agent listens on a named pipe that only the current Windows user can open, local only. It answers
  only `ws.exe` / `DevKit.exe` / `xtask.exe` that live under `DB_HOME` (or `CARGO_TARGET_DIR`) — a binary of this workspace, not a
  script — and gives a secret only for a command in its `session_for`. A value is never printed or returned to
  anything else; `ws secrets run game_aes_key -- powershell …` is refused and asks for the password, so the human sees
  the command.
- **How long.** `--for` (default 4 h, at most 12 h); closes after `--idle` without requests (default 60 min), on
  `cargo ws secrets lock` (DevKit: "Close") and when you sign out of Windows. Unlocking again replaces the session.
- **Journal.** Every request — `home/runs/vault/<UTC date>.log`: time, secret names, result, the asking exe, the
  command. No values.
- **The agent's exe** is a copy in `home/cache/vault` (`ws-vault-agent-<pid>.exe`), so `cargo` can rebuild `ws` while a
  session is open; old copies are removed at the next unlock.
- **What it does not protect against:** anything running as you can still rebuild `ws` with changed code and ask
  the session. The session is a convenience with a narrow door, not a sandbox; close it when you leave the machine.
- **An open session gives the key, not the permission:** decrypting paks still needs the human's word
  (`docs/GAME_DATA.md`).
- Windows only for now; elsewhere `ws secrets run` works as before.

## Rules

- Do not print values, do not write them into workspace files (exception — `config/local.toml` via `ws bootstrap` for secrets
  with a `local` field), do not paste them into chats or merge requests.
- An agent (AI) does not open the database and does not enter the password: only `ws secrets run`, a person enters the password.
  With a vault session open, the agent runs the commands in `session_for` itself and nothing else.
- Need a new secret — add an entry to `config/secrets.toml` (name, `env`, `used_by`, `visibility`) and ask
  the owner to set the value; for the stand a separate test entry is better than the prod key.
