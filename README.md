# Dungeonborne community framework

Everything needed to run **your own community server** for **Dungeonborne**, an online PvPvE dungeon game whose
official servers were shut down in 2025: a game-server emulator, a players' launcher, server- and client-side game
mods, a balance editor, a Discord bot, a website, and the developer tooling that builds and tests all of it on one
Windows PC.

This is the public edition of a working project: the same code ran a live community server with real players
in November 2026. Real addresses, accounts and keys are replaced with placeholders; the publisher's code, assets
and binaries are not here — you rebuild what you need from your own copy of the game (see
[Legal](#legal-and-licenses)).

## Contents

- [What it does](#what-it-does)
- [How it fits together](#how-it-fits-together)
- [Repository layout](#repository-layout)
- [Requirements](#requirements)
- [Install](#install)
- [Your copy of the game](#your-copy-of-the-game)
- [Building the parts](#building-the-parts)
- [Local test stand](#local-test-stand)
- [Running a real server](#running-a-real-server)
- [Discord bot](#discord-bot)
- [Balance, loot and seasons](#balance-loot-and-seasons)
- [Research and reverse engineering](#research-and-reverse-engineering)
- [Command reference](#command-reference)
- [Working with AI assistants](#working-with-ai-assistants)
- [Contributing](#contributing)
- [Secrets and security](#secrets-and-security)
- [Troubleshooting](#troubleshooting)
- [Documentation map](#documentation-map) · [FAQ](docs/FAQ.md)
- [Legal and licenses](#legal-and-licenses)

## What it does

- **Game server emulator** (`core/backend`, Rust): the game's directory server and TGCP gateway, login, characters,
  inventory and stash, trade and auction with system sellers, quests, guild and social features, lobby and
  matchmaking with the game's **native match flow** (search → room → the game travels to the battle server by
  itself), a waiting hall, match results and rewards, seasons, an admin API and a link to the battle servers.
- **Battle servers**: the game's own client started headless (`-dbserver`, no GPU) with UE4SS server mods
  (`game/ue4ss`), managed by `ds-agent` on a Windows box. Matches are real game sessions, not a re-implementation.
- **Launcher** (`apps/launcher`, Tauri): Discord sign-in, server list, an encrypted relay so players need no
  open ports or `hosts` edits for the backend, per-server mods delivered on demand (`dbmod/1` packages), network
  checks, bug reports with logs, signed self-updates, an optional "Play in Steam opens the launcher" mode.
- **Game mods**: server branches and fixes as UE4SS Lua mods and small patch paks (login, a client crash fix, world
  chat, server-only branches), a native helper DLL (`dbnative`) for player data on the battle server.
- **Balance Studio** (`apps/studio`): a desktop editor for designers; produces a checked patch file the server
  applies. **Balance kit** for spreadsheets and scripts.
- **Discord bot** (`apps/discord-bot`, Rust): moderation log, tickets, giveaways, translations, server status, patch
  notes, bug reports from the launcher, fundraising card; plus scripts that build the whole Discord server from YAML.
- **Website** (`web/landing`): a static community site with news built from the server's patch notes.
- **Workspace tooling** (`tools/ws`, `xtask`, `apps/devkit`): pinned installs, health checks, a one-command local
  test stand, an export pipeline with a leak scanner, and roles for AI assistants.

## How it fits together

```
 players (Windows)                                 cloud VM (Linux)                    battle-server box (Windows)
 ┌─────────────────────┐  TLS relay :8443/:443  ┌───────────────────────────┐  private  ┌────────────────────────────┐
 │ launcher (Tauri)    │ ─────────────────────▶ │ nginx                     │  network  │ ds-agent (scheduled task)  │
 │  local relay        │                        │ dbserver-rs (backend)     │ ◀───────▶ │ battle servers = headless  │
 │ game client + mods  │ ── UDP 7777+10·i ──────┼───────────────────────────┼─────────▶ │ game clients + server mods │
 └─────────────────────┘                        │ Discord bot, backups      │           └────────────────────────────┘
                                                └───────────────────────────┘
```

The battle servers must run on real hardware: the game's anti-cheat refuses virtual machines. Everything else is an
ordinary Linux VM. The full picture — ports, services, what lives where, backups — is in
[`docs/CLOUD_ARCHITECTURE.md`](docs/CLOUD_ARCHITECTURE.md). For development, all of it runs on one Windows PC
(the [local test stand](#local-test-stand)).

## Repository layout

| Path | What it is |
|---|---|
| `core/backend` | game server emulator (`dbserver-rs`), `ds-agent`, the balance-kit crate, data tasks (`cargo xtask` there) |
| `core/backend/ExcelConfig`, `Configure.pb`, `Network.pb`, `assets/packets` | the game's server tables, protocol descriptors and reference packets the backend loads |
| `apps/launcher` | players' launcher; `dev-app` is a developer UI over the backend's admin API |
| `apps/studio` | Balance Studio |
| `apps/devkit` | DevKit — a window over `ws`: what is missing on this PC, fix it, secrets, checks |
| `apps/discord-bot` | community Discord bot and the Discord server setup scripts (`server/`) |
| `web/landing` | community website |
| `game/ue4ss` | UE4SS mods for the client and the battle server; `native/dbnative` — the helper DLL |
| `game/bp-patches` | tools that build the patch paks from your game copy |
| `game/catalog` | which mods and paks each profile (client, server, dev) gets |
| `game/paks`, `game/client-mod`, `game/unreal` | balance pak output, an older manual client installer, Unreal Engine projects |
| `content` | game data edited by people: balance tables, loot templates, seasons, names, matchmaking rules |
| `tools/ws` | the workspace tool (`cargo ws …`) |
| `tools/stand` | the local test stand scripts |
| `tools/dumper`, `tools/py`, `tools/probe`, `tools/sniffer`, `tools/debug` | game data export, helper scripts, protocol probes |
| `ops` | deployment (VM, battle-server box), backups, a private git server, machine setup |
| `research` | protocol and client research notes and evidence |
| `docs` | specs, plans, write-ups ([index](docs/README.md)) |
| `agents`, `.claude` | memory notes and role definitions for AI assistants |
| `config` | workspace settings: members, pinned tools, artifacts, secrets inventory, game build |
| `xtask` | `cargo xtask` at the root: setup, DevKit build, CI, releases, exports |
| `sandbox` | a place for experiments that are not members yet |
| `bootstrap.ps1` | the installer |

Each member folder has a `member.toml` (commands, links) and a `README.md`.

## Requirements

- **Windows 10/11 (64-bit)** and an internet connection. Everything installs under one folder; no administrator
  rights for the install, the user PATH and profile are not changed.
- **Disk:** the base install is ~3.5 GB (~5.5 GB without Visual Studio Build Tools); building the backend,
  launcher and Studio adds several GB of build cache. The installer warns below 20 GB free.
- **Optional:** Visual Studio Build Tools with C++ (otherwise the Microsoft CRT/SDK is fetched by `cargo-xwin` and
  LLVM is used), KeePassXC for secrets.
- **For anything that touches the game** (patch paks, the test stand, a real server): **your own copy of
  Dungeonborne** from Steam, build `16716489` (`config/game.toml`). The game was delisted, so it has to be in your
  Steam library.
- **For a real server:** a Linux VM, a physical Windows machine for the battle servers, a domain, your own TLS
  keys and launcher signing key, and a Discord application.

## Install

### With an AI assistant (recommended)

Give this repository's link to an AI assistant that can run commands (Claude Code, Codex and the like) and ask it
to "check and set it up". It follows [`AGENTS.md`](AGENTS.md): audits `bootstrap.ps1` against the pinned hashes,
shows you the plan, installs step by step after your "yes" and handles failures. About 5 minutes and ~0.3 GB of
downloads with Visual Studio Build Tools present.

### By hand

PowerShell, in a folder that holds `bootstrap.ps1` and `config/bootstrap.manifest.toml` (or a clone of this
repository). The script clones `https://github.com/dungeonborne-community/dungeonborne` into `<root>\workspace`;
add `-Repo <url>` for a fork.

```powershell
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev          # plan only, changes nothing
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev -Apply   # install
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev -Status  # what is done, what is next
```

After a failure run the same command again: finished steps are skipped. Steps: `machine`, `rustup`, `git`, `clone`,
`toolchain`, `linker`, `tools`, `devkit`, `components` — details and error meanings in `AGENTS.md`. Useful switches:
`-Linker xwin` (build without Visual Studio), `-Step <name>` (one step), `-Tool <name>` (repeat one download),
`-Components <preset>` (a partial checkout: `web`, `balance`, `modding`, `re`, `full`).

Every download comes from a pinned URL with a pinned sha256 (`config/bootstrap.manifest.toml`,
`config/tools.lock`). Third-party tools are always their official releases; `ws` and DevKit are built from source
unless a release signed with `xtask/dist.pub` exists for the commit.

### After the install

The layout is `<root>\workspace` (this repository), `<root>\home` (toolchains, tools, caches, game copies, build
output — never in git) and `<root>\vault` (your secrets database, optional). In a new PowerShell window load the
workspace's Rust and tools for that window, then finish the setup:

```powershell
. 'C:\DBDev\home\env.ps1'                      # this window only: Rust, cargo, the build tools, Python
cargo ws bootstrap --profile=dev --apply       # config/local.toml, home folders, the rest of the tools
cargo ws doctor                                # what is wrong on this PC
cargo ws check                                 # repository layout and leak rules — 0 errors expected
```

Profiles: `dev` (developer: everything to build and test), `designer` (Balance Studio and the kit), `agent`
(an AI assistant's minimum). DevKit (`<root>\home\tools\devkit\DevKit.exe`) shows the same checks in a window.

All machine-specific settings go into `config/local.toml` (created from `config/local.example.toml`, not in git).

## Your copy of the game

Everything game-specific is rebuilt from the copy you own. The order below is what a fresh workspace needs; each
command without `--apply` only shows what it would do.

1. **Game reference** — a clean copy of the game files the tools build from (`home\game\original\<build>`):
   ```powershell
   cargo ws game                          # state, or the exact steps
   cargo ws game original installed --apply   # copy the clean files of your Steam install
   ```
   If your Steam install is modified, download the exact build with Steam's console
   (`download_depot 2448970 2448971 4035623128766163436`) and adopt it with `cargo ws game original steam --apply`.
   Another workspace on the same PC can share its reference: `config/local.toml` `[game] original = '…'`.
2. **The pak key.** The game's paks are encrypted with an AES-256 key, which is not in this repository. It is public:
   community collections of AES keys for UE4/UE5 games list it. Put it into `config/local.toml` (not in git):
   ```toml
   [re]
   aes_key = '0x…'   # 0x + 64 hex digits
   ```
   or keep it in your secrets vault as `game_aes_key` ([`docs/SECRETS.md`](docs/SECRETS.md)).
3. **Patch paks** — login fix (`zz_LIPatch_P`), client crash fix (`zzzzzzz_BPFix_P`), world chat
   (`zzzzzzz_ChatFix_P`), server branches (`zzzzzzz_ServerNet_P`), built from your reference into
   `game/bp-patches/build`:
   ```powershell
   cargo ws dump paks --apply
   ```
   Checked: the result has the same content as the paks the live server shipped.
4. **Game mappings (usmap)**, needed only for the research dumps below. After the stand exists
   ([Local test stand](#local-test-stand)), `tools\stand\usmap.ps1` starts the stand client headless, dumps the
   mappings with UE4SS and copies them to `home\re\ue4ss` (about a minute, no backend needed).
5. **Research dumps** (optional): icons, monsters, heroes, curves as PNG/JSON into `home\dumps\<build>`:
   ```powershell
   cargo ws dump --apply                  # or --set=icons,curves
   ```
   Details: [`docs/GAME_DATA.md`](docs/GAME_DATA.md). Decrypting your copy's paks is your decision under the
   game's terms.

## Building the parts

Run these in a window where `env.ps1` is loaded.

| Part | Command | Result / notes |
|---|---|---|
| Backend | `cd core\backend; cargo build --release -p dbserver-rs` | `home\cache\target\release\dbserver-rs.exe`. RocksDB needs `libclang`: `cargo xtask setup --apply --tool=llvm` and `LIBCLANG_PATH=<home>\toolchains\llvm\bin`. Do not build through sccache. Tests: `cargo test -p dbserver-rs` |
| Balance pak and server tables | `cd core\backend; cargo xtask balance build` | `game\paks\balance\zzzzzz_Balance_P.pak` and the season's server tables |
| Mods | `cd core\backend; cargo xtask mods build --profile=client` (or `server`) | `home\out\mods\<profile>`; `mods check` validates the catalog and Lua, `mods install --game=<name>` lays them into a game copy |
| `dbnative.dll` | `game\ue4ss\native\dbnative\build.bat` | MSVC when Visual Studio is present, otherwise clang-cl with the cargo-xwin SDK (`build.bat clang`) |
| Launcher client runtime | `cargo xtask setup --apply --tool=goldberg,ue4ss`, then `python apps\launcher\tools\make_client_bundle.py --upstream [--ca <your backend CA .pem>]` | `apps\launcher\bundle\client.zip` (Goldberg Steam emulator and UE4SS from pinned releases); the launcher embeds it |
| Launcher | `cd apps\launcher\src-tauri; cargo tauri build` | a single `dungeonborne-launcher.exe`; see [`apps/launcher/README.md`](apps/launcher/README.md) for your server address and certificate pin |
| Balance Studio | `cd apps\studio\src-tauri; cargo tauri build --no-bundle` | `balance-studio.exe` (set `NO_COLOR=true` if trunk complains) |
| DevKit | `cargo xtask devkit` | `DevKit.exe` |
| Website | `cd web\landing; python web/build_news.py; python web/build_classes.py; python web/build_seo.py` | static files; preview with `python web/dev_server.py 8821` |
| Discord bot | `cd apps\discord-bot; cargo build --release` | see [Discord bot](#discord-bot) |
| Everything, as CI does | `cargo xtask ci` | fmt, clippy, tests, `ws check` |

What is not in git and how each piece is rebuilt: [`docs/STORAGE_EXPORT.md`](docs/STORAGE_EXPORT.md).

## Local test stand

One Windows PC runs the backend, a headless battle server and one or two game clients — the way to check a change
from login to the end of a match. Details: [`tools/stand/README.md`](tools/stand/README.md).

```powershell
cargo ws stand                 # what is there and what is missing (changes nothing)
cargo ws stand --apply         # build it; --clients=2 for a second client
```

`ws stand --apply` makes the copies `home\game\stand-server` and `stand-client` from your reference (paks are hard
links, ~1 GB each), installs the launcher runtime, gives each copy its own Goldberg Steam account, installs the mods
of its profile, adds the autotest driver to the clients, makes a TLS certificate of this PC for the local login and
writes `[games]` / `[stand]` into `config/local.toml`. Two things only a human does, and the command prints both
with the exact text:

- **The DbTools task**, once, in PowerShell *as administrator* in the workspace folder:
  `powershell -ExecutionPolicy Bypass -File ops\machine\install-dbtools.ps1 -Apply`. The game runs under an
  anti-cheat that raises UAC; the task starts game instances without the AI or the scripts holding admin rights.
- **The `hosts` block** pointing the game's former service names to `127.0.0.1`.

Running it:

```powershell
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Acquire -Owner me -Purpose test
$env:LAUNCHER_LOBBY = '0'      # plain matchmaking for automated runs (the launcher lobby needs a human)
powershell -ExecutionPolicy Bypass -File tools\stand\run_local.ps1          # backend; keep the window open
# in another window:
powershell -ExecutionPolicy Bypass -File tools\stand\run.ps1 -Match native  # automated: login, match, connect
powershell -ExecutionPolicy Bypass -File tools\stand\play.ps1               # or play yourself with windows
powershell -ExecutionPolicy Bypass -File tools\stand\portlock.ps1 -Release -Owner me
```

The game's ports are fixed (backend 8085 and 6601–6604, battle server 7777), so the port lock lets only one person
or agent use the stand at a time. Under the anti-cheat a game process can only exit by itself; keep ~30 s between
client restarts. On a fresh database the stand's accounts get a character automatically.

## Running a real server

[`docs/CLOUD_ARCHITECTURE.md`](docs/CLOUD_ARCHITECTURE.md) describes the live setup and the steps; in short:

1. **VM (Linux):** build the backend for Linux (`ops/deploy/scripts/wsl_build_clean.sh`), install it as a systemd
   service with your `server.toml` (from `core/backend/server.example.toml`) and secrets in an env file; nginx and
   certificates; open 22, 80, 443 and 8443; raise the conntrack limits (the relay opens many short connections).
2. **Battle-server box (physical Windows):** a game install, the kit from `ops/deploy/robot`, the private network to
   the VM, the `ds-agent` scheduled task (`core/backend/ds-agent`), the server mods and `zzzzzzz_ServerNet_P`.
3. **Season and mods:** `cargo xtask balance build`, `cargo xtask mods build --profile=client|server`,
   `apps/launcher/tools/make_mods.py` → packages into the server's `mods` folder; the launcher delivers them.
4. **Discord:** your own OAuth app (players sign in with Discord) and bot; IDs into `server.toml` and the bot config.
5. **Launcher:** set your server address and certificate pin, sign releases with your own key, host them.
6. **Backups:** `ops/backup/vm/setup.sh` with your own storage.

Updates without kicking players: [`docs/runbooks/`](docs/runbooks/) and `ops/deploy/scripts/` (maintenance on →
wait for running matches → copy the database → swap → maintenance off).

## Discord bot

`apps/discord-bot` — Rust (poise + serenity), state in an embedded SurrealDB; modules are switched on by sections
of `bot.toml` (an example with placeholders). Secrets only in `.env` (`DISCORD_TOKEN`, optional `DEEPL_KEY`,
Twitch keys, `PATREON_TOKEN`). `cargo run --release` locally; deploying on a Linux server with a systemd unit —
[`apps/discord-bot/README.md`](apps/discord-bot/README.md). `server/setup_server.py` creates the Discord server's
roles, channels, forums, permissions and posted texts from `server/server.yaml` and is safe to re-run
([`apps/discord-bot/server/README.md`](apps/discord-bot/server/README.md)).

## Balance, loot and seasons

- **Tables** live in `content/balance/tables/*.json` (the server base). Edit through CSV
  (`cargo xtask balance csv <table>` → edit in Excel → `cargo xtask balance csv-import <table>`), through Balance
  Studio, or through a players' patch (`cargo xtask balance patch <file>`). Guide: [`docs/BALANCE.md`](docs/BALANCE.md).
- **Loot** is a readable template per map (`content/loot/loot.toml`): `cargo xtask loot show` prints the average
  drop of each source, `cargo xtask loot build` turns it into tables.
- **Seasons** (`content/seasons/<id>`) override the base: their own tables, loot, flags and patch notes; the active
  one is `content/seasons/active.txt` or the `SEASON` variable. `cargo xtask season new <id>`;
  format: [`docs/SEASONS_FORMAT.md`](docs/SEASONS_FORMAT.md).
- **Matchmaking rules** (gear rarity limits per map level and the like): `content/match_checks.toml`.

## Research and reverse engineering

How the client was studied, so you can continue where we stopped:

- **Paks: Blueprints, Lua, tables.** Most game rules live in the paks — Blueprints (UAssetGUI, FModel), UnLua
  bytecode, data tables. `cargo ws dump` exports icons, monsters, heroes and curves from your copy; the whole path is
  in [`docs/RESEARCH.md`](docs/RESEARCH.md) and [`docs/GAME_DATA.md`](docs/GAME_DATA.md).
- **Native code: the exe dump and Ghidra.** `Dungeonborne.exe` on disk is packed (its code sections are zeros). The
  engine's own `-fullcrashdump` switch makes the game write a full memory dump when it crashes, with the code already
  decrypted; the scripts in `research/ghidra/scripts` carve the exe image out of it, and Ghidra decompiles functions
  offline. Method: [`docs/owner/RE_TECHNIQUE.md`](docs/owner/RE_TECHNIQUE.md); analysis:
  [`research/ghidra/README.md`](research/ghidra/README.md); Ghidra 12.2 and JDK 21 without administrator rights:
  `cargo xtask setup --apply --tool=ghidra-dist,jdk`.
- **Protocol.** The backend's message types are generated from the game's protobuf descriptors; evidence per area is
  in `research/`, live probes in `tools/probe` and `tools/sniffer`.
- **Checking a finding** happens in the game: a UE4SS hook and a log line on your own test stand.

### The game client — our rules

They hold for people, for AI assistants and for every piece of research here (full text: [`AGENTS.md`](AGENTS.md)):

- **The exe and the game's own binaries are not modified** on disk — no patching, no unpacking. The game folder
  only gets the documented runtime (UE4SS and its loader, the Goldberg Steam emulator), our mods and our paks.
- **No reading or writing the memory of a running game** — no debuggers, memory scanners or injected tools beyond
  UE4SS.
- **The anti-cheat is not bypassed, disabled or fooled.**
- **The game's cryptography is not touched** — protocol encryption, signatures, integrity checks; no cracking, no
  brute force. The only key used is the public pak key, only to read your own copy's paks.
- **Native code** is studied only through the crash dump the game writes itself, offline.

## Command reference

`cargo ws` (the workspace tool; `cargo ws help` for everything):

| Command | What it does |
|---|---|
| `ws check` | layout, machine paths, binaries, secrets, member links — before every commit |
| `ws doctor` / `ws bootstrap --apply` | what is missing on this PC / install it |
| `ws game …` | the game reference: state, adopt a Steam download or install, clone working copies, diff |
| `ws dump [--set=…]`, `ws dump paks` | research dumps / patch paks from your game copy |
| `ws stand [--apply]` | the local test stand |
| `ws components list\|add\|remove\|set` | keep only some parts of the repository on disk |
| `ws secrets init\|list\|check\|run\|unlock` | the KeePassXC vault and commands that need secrets |
| `ws preview [<member>]` | run and show a member's result (sites, apps) |
| `ws members`, `ws roles` | members and what is public |

`cargo xtask` at the root: `setup [--apply] [--prebuilt|--from-source] [--tool=…]`, `devkit`, `ci`,
`dist [--sign]`, `dist keygen`, `pack designer`, `export <edition>`.

`cargo xtask` in `core/backend`: `balance build|verify|csv|csv-long|csv-import|names-import|kit|patch`,
`loot export|show|build`, `season [use|new]`, `mods build|check|install|verify`.

## Working with AI assistants

The repository is set up for AI-assisted work:

- [`AGENTS.md`](AGENTS.md) — the install procedure an assistant follows, and its rules (no secrets in git or chat,
  pinned downloads only, no changes to the system without the person's yes).
- [`CLAUDE.md`](CLAUDE.md) — working rules for Claude Code inside the workspace.
- `.claude/agents/*.md` — roles (backend, launcher, mods, balance, stand, web, design, kit, re for reverse
  engineering): "do this as
  the backend role". `cargo ws bootstrap` writes the agent's environment into `.claude/settings.local.json`,
  including portable git and `gh`.
- `agents/memory/` — project notes the assistants keep: decisions, pitfalls, what was verified and when.

## Contributing

Branch from `main` (`fix/<short>` or `feat/<short>`), keep the change small, run `cargo xtask ci` (fmt, clippy,
tests, `ws check` with 0 errors), open a pull request that says what changed and how you checked it. Where new
things go: [`docs/EXTENDING.md`](docs/EXTENDING.md); setup and the workflow in detail:
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md). Open problems and plans: [`docs/FIX_LIST.md`](docs/FIX_LIST.md),
[`docs/BACKLOG.md`](docs/BACKLOG.md); deliberate simplifications: [`docs/WORKAROUNDS.md`](docs/WORKAROUNDS.md).

## Secrets and security

No secret values are in this repository. `config/secrets.toml` lists what exists (names and purposes); values live
in your KeePassXC database (`cargo ws secrets init`), the pak key in `config/local.toml`, the bot's tokens in its
`.env`. `ws check` refuses keys and password databases in git. Report vulnerabilities as described in
[`.github/SECURITY.md`](.github/SECURITY.md).

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `bootstrap.ps1` fails to download | network; run the same command again (it retries). Never switch to another source or edit a hash |
| `sha256 … does not match` twice | the file on the server changed; stop and report it |
| linker errors / no Visual Studio | `bootstrap.ps1 … -Linker xwin -Apply` (Microsoft CRT/SDK via cargo-xwin, LLVM) |
| backend build fails in `librocksdb-sys` | `LIBCLANG_PATH` is not set, or sccache is the rustc wrapper |
| `ws doctor`: no game reference | `cargo ws game original installed --apply`; if your install is modified, use `download_depot` (above) |
| `ws dump paks`: no pak key | `config/local.toml [re] aes_key` |
| `ws stand`: DbTools does not know this workspace | run `ops\machine\install-dbtools.ps1 -Apply` as administrator |
| `run.ps1 -Match`: the match button is rejected (`-1028`) | start the backend with `$env:LAUNCHER_LOBBY = '0'` |
| a game process cannot be killed | the anti-cheat protects it; it exits by itself on `quit`, or by signing out |
| trunk: `invalid value '1' for '--no-color'` | set `NO_COLOR=true` |

## Documentation map

- [`docs/FAQ.md`](docs/FAQ.md) — questions people ask: where the code comes from, legal, the anti-cheat, the pak key.
- [`docs/README.md`](docs/README.md) — the index of all documents, by role.
- [`docs/HISTORY.md`](docs/HISTORY.md) — how the project came to be.
- [`docs/GLOSSARY.md`](docs/GLOSSARY.md) — the words used everywhere.
- [`docs/CLOUD_ARCHITECTURE.md`](docs/CLOUD_ARCHITECTURE.md), [`docs/STORAGE_EXPORT.md`](docs/STORAGE_EXPORT.md) —
  the live setup, and what lives outside git.
- [`docs/WORKSPACE_PLAN.md`](docs/WORKSPACE_PLAN.md), [`docs/COMPONENTS_PLAN.md`](docs/COMPONENTS_PLAN.md) — how the
  workspace is laid out.
- [`docs/DS_PIPELINE.md`](docs/DS_PIPELINE.md), [`docs/REF_BATTLE_ROOM_FLOW.md`](docs/REF_BATTLE_ROOM_FLOW.md) — the
  match pipeline between backend, battle server and clients.
- [`docs/LAUNCHER_SPEC.md`](docs/LAUNCHER_SPEC.md), [`docs/MODS_DESIGN.md`](docs/MODS_DESIGN.md),
  [`docs/BALANCE_APP_SPEC.md`](docs/BALANCE_APP_SPEC.md) — launcher, mods, Balance Studio.
- [`docs/RESEARCH.md`](docs/RESEARCH.md), [`docs/owner/RE_TECHNIQUE.md`](docs/owner/RE_TECHNIQUE.md) — how the client
  was studied.

## Legal and licenses

Dungeonborne and its assets belong to their owners; this project is not affiliated with them. The repository holds
our own code, the game's server tables and protocol descriptors the emulator needs, and research notes — not the
publisher's client code, assets or paks ([`docs/DISCLAIMER.md`](docs/DISCLAIMER.md)). You need your own copy of the
game, and what you build from it is under the game's terms.

Our code is licensed under **MIT or Apache-2.0** at your option ([`LICENSE-MIT`](LICENSE-MIT),
[`LICENSE-APACHE`](LICENSE-APACHE)). Third-party programs are downloaded at setup from their official releases and
keep their own licenses ([`docs/THIRD_PARTY.md`](docs/THIRD_PARTY.md)).
