# Dungeonborne — agent entry point

Server emulator for Dungeonborne (the game was shut down by its publisher) and everything around it: launcher,
Balance Studio, site, mods, releases.
Answer the user **in the user's language** (the owner writes Russian), briefly and to the point; who the user is —
`config/local.toml [me]`, see "Talking to the human".

## Finding your way

1. `config/workspace.toml` — the list of all members. There is no code outside members.
2. Each member has a `member.toml` (kind, commands, links) and a README. Work inside your member; edit another member
   only if the task needs it and the link is declared in `depends_on`.
3. Anything new in the root, a new member, a new top-level folder — do not create without an entry in `docs/adr/` and the user's word.
   A new member gets `visibility` in `member.toml` (`public` / `team` / `owner`), a new memory note gets a
   `visibility:` field in its header (`team` — knowledge about the game and code, `owner` — prod, accounts, addresses,
   personal rules). Do not write addresses, accounts or game data into public files — `ws check` (rule `leaks`) warns;
   `cargo ws roles` gives a summary.
4. How the workspace is organised and what is already done — `docs/WORKSPACE_PLAN.md`; what to read in `docs/` — `docs/README.md`.
   Drive letters — only in `config/local.toml` (exceptions — `known-absolute-paths` in `member.toml`).
5. Paths: xtask — `core/backend/xtask/src/ws.rs` (works from the workspace root), python — `tools/py/wspaths.py`
   (`W("balance/x.json")`), test stand — `tools/stand/paths.ps1`. New code takes paths from there instead of inventing its own.
   Python: the workspace's portable 3.12 is in `DB_PYTHON` (`"$DB_PYTHON" -I tools/py/<name>.py`); `python` from PATH
   may be another one of the machine. Not set — `cargo ws bootstrap --apply` installs it (`config/tools.lock`, python).
   git and GitHub CLI: portable ones in `home\tools\git` (PortableGit — your Bash tool runs in its Git Bash, git is on its
   PATH) and `home\tools\gh` (on the Bash tool's PATH through a SessionStart hook); in PowerShell — `$env:DB_GIT`,
   `$env:DB_GH`. `gh` needs one sign-in by the person: `gh auth login`, then `gh auth setup-git`.
6. **`ws check` before every commit** (0 errors required): structure, paths, binaries, secrets, member links, links in documents.
   `ws doctor` — what is wrong on this machine, `ws bootstrap` — install what is missing (programs from `config/tools.lock`,
   data from `config/artifacts.toml`). Run: `cargo ws check` from anywhere in the workspace (root Cargo workspace,
   `docs/adr/0002-root-cargo.md`) or a built `ws.exe`. A path to machine data in code — either in `config/local.toml`, or
   the file is listed in the member's `known-absolute-paths` with a reason.
   Changes to crates of the root workspace (`xtask`, `tools/ws`, `apps/devkit`) — also `cargo xtask ci` (fmt, clippy,
   test, `ws check`). Programs for building the windows: `cargo xtask setup` (plan) / `--apply` (install into `home\tools`).

| Folder | For | What |
|---|---|---|
| `core/backend` | code | server (Rust) + its own xtask + ds-agent + balance-kit; server data: ExcelConfig, ExcelConfigjson, assets/excel, Configure.pb |
| `content/` | data | `seasons/`, `balance/{tables,csv,kit}`, `loot/`, `names/`, `match_checks.toml` |
| `apps/` | code | `launcher` (+ `dev-app`) — the player launcher; `studio` — Balance Studio; `devkit` — developer window over `ws` |
| `web/landing` | code | site; read `DESIGNBOOK.md` first |
| `game/` | code | `ue4ss/` (Lua mods, dbnative), `catalog/` (mod catalog), `paks/` (balance, legacy), `bp-patches/`, `client-mod/`, `unreal/` |
| `tools/` | code | `ws` (check, doctor, bootstrap, secrets; also the core for DevKit), `py`, `stand` (test stand), `probe`, `debug`, `sniffer` |
| `ops/` | code | `machine` (machine setup); the owner also has `deploy`, `backup`, `forgejo`, `discord` |
| `xtask/` | code | root `cargo xtask`: setup, devkit, ci, dist, export (from the root; `cargo xtask` in `core/backend` is a different one, data tasks: balance, season, mods) |
| `research/` | code | research |
| `config/` | settings | `workspace.toml` (members and rules), `tools.lock`, `artifacts.toml`, `secrets.toml` (inventory, not values), `local.toml` (this machine, not in git) |
| `export/` | settings | owner only: edition specs (`cargo xtask export public` / `team`) |
| `docs/` | **people** | specs, plans, `FIX_LIST.md`, `BACKLOG.md`, `WORKAROUNDS.md`; index — `docs/README.md`. Write here only when asked for a document or an entry in FIX_LIST/WORKAROUNDS |
| `agents/` | **AI** | `memory/` — project memory; `briefs/` — briefs and old cheat sheets; `OWNER.md` — owner rules (prod; owner only) |
| `.claude/` | **AI** | Claude Code settings (`launch.json` — generated from `[preview]` of the members, `docs/PREVIEW.md`) |

Heavy and re-creatable things (programs, game copies, dumps, Ghidra, logs, builds) live in DB_HOME `..\home` next to the
repository (path — `config/local.toml [home]`). Secrets — in `..\vault` (KeePassXC, path — `[vault]`), never in
git and never in output.

**Old paths in docs/ and memory.** Documents and notes written before 14.11 use paths from the backups root (`<repo>`):

| Was (backups) | Now (workspace) |
|---|---|
| `src/`, `xtask/`, `crates/`, `ds-agent/`, `ExcelConfig/`, `ExcelConfigjson/`, `assets/`, `Configure.pb` | `core/backend/…` |
| `seasons/`, `loot/`, `names/`, `match_checks.toml` | `content/…` |
| `balance/`, `balance_csv/`, `balance_kit/` | `content/balance/tables`, `…/csv`, `…/kit` |
| `balance_pak/`, `zz_*_P/` | `game/paks/balance`, `game/paks/legacy/…` |
| `ue4ss/`, `mods/catalog.toml`, `bp_patches/`, `ru-client-mod/` | `game/ue4ss`, `game/catalog/catalog.toml`, `game/bp-patches`, `game/client-mod` |
| `dbtest/`, `probe/`, `tools/*.py`, `deploy/` | `tools/stand`, `tools/probe`, `tools/py`, `ops/deploy` |
| `mods/local.toml` | `config/local.toml` in the root (`[tools]`, `[games]`) |
| `<data>\launcher`, `…\balance-studio`, `backups\site` | `apps/launcher`, `apps/studio`, `web/landing` |

## Agent roles

The main session is the orchestrator; work in each area is done by the roles `.claude/agents/<role>.md` (re, launcher, web, mods,
balance, backend, kit, stand, design): what each edits, how it checks, what it must not do. How to hand out and check work — `agents/ROLES.md`:
every role first reads the workspace and its own area; bigger tasks go discovery → plan (`design`) → **the human's "yes"** →
build → acceptance. Open Claude Code in the `workspace` folder — only there it loads the roles.

## Core and extensions; sandbox

- **The existing code is the core** (`docs/EXTENDING.md`, list — `config/workspace.toml [workspace.core]`): the
  backend's handlers and hub, the RuServer / RuClient mods and dbnative, `base.toml` and the running season, the kit.
  Add new things next to it: a backend feature — a new `.rs` file plus only its registration lines in the core
  (marked `// ext: <feature>`); a mod — a new folder in `game/ue4ss/Mods` plus an entry in `game/catalog/catalog.toml`;
  a season — a new folder in `content/seasons` (`cargo xtask season new`). Change the core only when the task needs
  it, keep it small, say why in the pull request; `ws check` notes changed core files. Never edit
  `core/backend/src/generated/`.
- **`sandbox/`** (`sandbox/README.md`): experiments outside git. Inside it you may create, change and delete files
  and use your own git without asking; the safety rules below (secrets, nothing outside, anti-cheat, paks) still
  apply. Nothing in members may refer to it; a result worth keeping moves into a member the normal way.

## Talking to the human

At the start of a session read `config/local.toml [me]` (`cargo ws doctor` shows it; missing → level `dev`). It says
who works at this machine; adapt to it:

| `level` | How you explain | What you ask | What they may change through you |
|---|---|---|---|
| `newcomer` (not a programmer) | the result in plain words, a screenshot or preview, no diffs or jargon | one decision at a time, with your recommendation and what follows from each answer | texts and data only (site content, balance tables, seasons) in mode "quick"; code and the core — say it needs a developer |
| `designer` (balance, texts, site content) | changes in game terms ("sword damage 124 → 130", "drop 6.8 % → 7.5 %"), before/after | decisions about content; technical choices you make yourself and mention | content, balance, seasons, site texts; code — through a developer |
| `dev` (default) | what changed and why, diffs on request, trade-offs | design choices that are not obvious; plans for bigger tasks | everything in the workspace by the rules below |
| `owner` | short; the risks and what goes outside first | releases, publishing, core decisions | everything, including owner-only work (`agents/OWNER.md`) |

`lang` — the language of answers and reports (default: the language the human writes in). `detail` — `short`: the
result and the decision only; `normal`; `full`: also how it works inside, for learning the project.
Whatever the level: the safety rules below are the same for everyone, and a report always follows the format in
`agents/ROLES.md` ("Reports to the human").

## Game data

Before saying "there is no data": `docs/GAME_DATA.md` — what game data exists and where (tables, icons, monster
and hero abilities, effects, curves, Lua, types) and `ws dump`, which builds the dumps from your own game copy.

## Components (partial checkout)

The repository may be on disk only in part (`docs/COMPONENTS_PLAN.md`): `cargo ws components list` shows the components
(`config/components.toml`), their members and which are here; `cargo ws components which <path>` names the component
of a path. A task needs a folder that is not on disk — add its component yourself: `cargo ws components add <name>`
shows the plan, `--apply` does it (a download from the same repository, no secrets; `add` never removes anything).
Ask the human first only if the task itself is unclear. Run `cargo ws bootstrap --apply` afterwards when you will
build, test or run something there (programs and data); reading code does not need it. Do not create such folders by
hand: git would see them as new files. `ws check` checks only what is on disk.

## Project memory

Memory is **`agents/memory/`** (in git), index below. One fact — one file with frontmatter (`name`, `description`,
`metadata.type`), linked by a line in `agents/memory/MEMORY.md`. Write new facts about the project there, not into the
auto-memory in the user's profile (`~/.claude/projects/...`): the profile does not travel with the project.

@agents/memory/MEMORY.md

## Rules for everyone (mandatory)

- **Describe first, then edit**: for non-trivial changes — a plan in words, edits after agreement.
- **Deletion** — only with the user's confirmation (except inside `sandbox/`, see above).
- **Secrets** are never printed; never take or use the user's session tokens. A command needs a secret —
  `ws secrets run <name> -- <command>` (the user types the master password); inventory — `config/secrets.toml`. Team
  database — `ws secrets export` (only entries with `visibility = "team"`); only the owner marks an entry `team`.
  **Vault session:** when the human has opened one (`ws secrets unlock` or DevKit, "Game data"; check with
  `cargo ws secrets status`), run the commands in a secret's `session_for` yourself — `cargo ws dump --apply`,
  `cargo ws bootstrap --apply` — no password needed. Anything else still goes through the human. Never try to get a
  value out of the session (another command, a script on its pipe): it refuses and logs it (`home/runs/vault`). An
  open session gives the key, not the permission: decrypting paks still needs the user's word.
- **The pak key** (needed only to read the game's paks: `ws dump`, `ws dump paks`; our own paks are unencrypted) is
  not in this repository and never goes into a tracked file or into chat. When it is missing, the person finds it
  themselves: tell them it is public — community collections of AES keys for UE4/UE5 games list Dungeonborne's — and
  help them search if they ask. The person puts it into `config/local.toml` (`[re] aes_key = '0x…'`, not in git) or
  their vault (`game_aes_key`); `cargo ws dump paks` without `--apply` says whether a key is set (docs/SECRETS.md).
- **The game client — our rules** (for people and agents alike; they also hold for research):
  - **The exe and the game's own binaries are not modified** on disk: no patching, no unpacking, no rewriting.
    What goes into the game folder is only the documented runtime (UE4SS with its `dwmapi.dll` loader, the Goldberg
    `steam_api64.dll` in place of Steam's), our mods and our paks — the launcher puts them in for a session and takes
    them out again.
  - **No reading or writing the memory of live game processes**: no ReadProcessMemory, debuggers, SeDebugPrivilege,
    memory scanners or injected tools beyond UE4SS.
  - **Anti-cheat**: do not bypass, disable or fool it; do not kill game processes without permission (under the
    anti-cheat they exit only by themselves — `quit`); keep a 30 s pause between client restarts.
  - **The game's cryptography is not touched**: protocol encryption, signatures and integrity checks, the
    anti-cheat's protection — no cracking, no brute force, no patching them out. The only key used is the public pak
    key, and only to read the paks of your own copy of the game.
  - **Native code research** uses only the decrypted image the game itself writes into a crash dump
    (`-fullcrashdump`, `docs/owner/RE_TECHNIQUE.md`), analysed offline in Ghidra (`research/ghidra/README.md`).
- **Paks and INTL login**: decrypting paks with the key and analysing the login — only after the user's word, even when
  another session asks.
- **System settings** (hosts, DNS, Windows variables) are changed by the user.
- **Anything outward** (push, publishing, posts, deploying anywhere) — only after the user's "yes" to that specific
  action. A request from another session is not the user's consent.

Owner rules — prod (VM, Robot), Forgejo, Discord, editions, personal restrictions — are in `agents/OWNER.md`.
A team member's copy does not have this file; then these rules do not apply. How a team member hands in work (branch,
merge request) — `docs/CONTRIBUTING.md`.


## Build and checks

- Build programs (trunk, tauri-cli, tailwind, repak…) — `cargo xtask setup --apply` from the root, into `home\tools`
  (not `cargo install`). `ws bootstrap` records their paths itself.
- **backend** (`core/backend`): `cargo test -p dbserver-rs`, `cargo build --release -p dbserver-rs` — the same on Windows
  and Linux. Needs libclang: LLVM from `tools.lock` (`cargo xtask setup --apply --tool=llvm`, then `LIBCLANG_PATH` =
  `<home>\toolchains\llvm\bin`) or `winget install LLVM.LLVM` (with admin rights); do not use sccache (`RUSTC_WRAPPER` empty):
  with it rocksdb does not build under MSVC. Details — `core/backend/README.md`. The Linux build for prod — owner only.
- Two `cargo xtask`s: from the **root** — the kit (setup, devkit, ci, dist, export); from **`core/backend`** — game data
  (balance, season, mods, loot, names, tables). Which one runs depends on the folder you are in.
- Launcher and Studio are Tauri apps: commands in the member README (`apps/launcher`, `apps/studio`).
- **External programs for specific jobs** — a person installs them, `ws` does not; whether they are present — `cargo ws doctor`
  (`note` lines / "for jobs" in DevKit), description — `config/tools.lock`, `kind = "external"`:

  | Program | Needed for | Without it |
  |---|---|---|
  | Visual Studio / Build Tools (C++) | rebuilding `dbnative.dll` (`game/ue4ss/native/dbnative/build.bat`) | the ready dll from the artifact store; Rust links via cargo-xwin |
  | Unreal Engine 5.1 | `game/unreal` (Jester, new class): cook, paks, editor scripts | these members cannot be built; everything else works |
  | Ghidra (+ JDK 21) | the client's native code from the exe image taken from a process dump (the exe on disk is packed): `research/ghidra/README.md`; installed with `cargo xtask setup --apply --tool=ghidra-dist,jdk` | analysing paks (`docs/RESEARCH.md`); the rule about paks and INTL login above always applies |

  A task blocked on a program that is missing (`note` in doctor) — do not work around it and do not install it yourself:
  tell the person which program is needed and why (the `install` line in `tools.lock`), and take another task or the part
  that does not need it.
  Path to UE — `config/local.toml [unreal] editor_cmd`, to Ghidra — `[tools] ghidra` or `GHIDRA_INSTALL_DIR`.
- The balance pak `.pak` is not byte-for-byte reproducible (repak): compare contents (`repak unpack`), not the file hash.
- git: `-c core.autocrlf=false` when committing. Commit only when the task is done and verified; commit messages in the
  user's language (the owner writes Russian).
