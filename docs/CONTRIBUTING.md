# Team member: setup and submitting work

For a developer with the team copy. The owner works in their own private repository; the team copy is produced from it
by the export `cargo xtask export team` (without history and without the owner's files: prod, deploy, Discord, backups).

## Where the code comes from

**The team repository** — its address is in `config/bootstrap.manifest.toml` (`[repo] url`). It has a `main` branch: each owner
export is one "Update team edition" commit from a neutral author. The history of `main` is not rewritten by the owner
or by you.

## Setup from scratch

This is the only order; the README and `docs/WORKSPACE_PLAN.md` (section 13) explain how it works inside.

0. **Yours:** Windows 10/11, Dungeonborne in your Steam library, installed (your own copy of the game is required —
   step 5), and about 35 GB free on the disk for the workspace (programs, caches, the 17 GB reference of the game).
1. **From the owner** (over a separate channel, not in git): access to the team repository; the team secrets database file
   `team.kdbx` and its password (the password separately from the file); the artifact store address and login (a read-only
   sub-account) — two lines for `[store]`.
   **Repository only** (a tester, a first look): just the access to the repository. Skip `team.kdbx` and `[store]` in
   steps 3–4: `ws` and DevKit are built from source, `cargo ws doctor` shows one note "no artifact store on this
   machine" instead of errors, and what needs the store or the pak key (game paks and mods, `ws dump`) is not
   available. The site, balance tables, seasons and the kit itself work.
2. **Programs and the repository** — without administrator rights. Download `bootstrap.ps1` and `config/bootstrap.manifest.toml`
   from the team repository (on GitHub: file → Download) into an empty folder, keeping the layout `<folder>\bootstrap.ps1` and
   `<folder>\config\bootstrap.manifest.toml` — put nothing else there. With an AI assistant: it reads the script first and
   gives you a short conclusion (what it downloads, where, what it does not touch); if its tools refuse to run the script
   (for example Claude Code in "auto" mode), it gives you the exact commands and you run them yourself — it then checks
   the result with `-Status`. Run it first without `-Apply` (the plan),
   then with it: `powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root C:\DBDev -Components <preset> -Apply`. The
   script installs rustup, git, Rust, a linker, clones the repository into `C:\DBDev\workspace` (GitHub login — a Git
   Credential Manager window), calls `cargo xtask setup` (trunk, tauri-cli, tailwind, ws, DevKit into `home\tools`) and
   puts the chosen components on disk.
   - **Components** — only the parts you work on are on disk (`config/components.toml`, `docs/COMPONENTS_PLAN.md`).
     `-Components` takes a preset or component names, comma separated; what they need comes with them:

     | Preset | On disk (besides the base: ws, xtask, DevKit, docs) | For |
     |---|---|---|
     | `web` | server, site | the community site |
     | `balance` | server, studio | balance tables, seasons, Balance Studio |
     | `modding` | server, mods, stand | game mods and checking them in the game |
     | `re` | server, mods, research | client research |
     | `full` (or no `-Components`) | everything | backend, launcher, several areas |

     Backend work is the component `server` (`-Components server`), the launcher — `launcher`. Change it any time: DevKit,
     "Components" page, or `cargo ws components list` / `add <name> --apply` / `set <preset> --apply`; then step 4 again
     for the programs and data of the new components. Branches, commits and pull requests work the same on a partial
     checkout.
   - Downloads: Rust ~110 MB, programs ~200 MB, then data ~100 MB. On a slow connection Rust takes long to download
     (17.11 for the owner ~10 minutes) — it is not hanging. The kit downloads large files over several connections by itself.
   - `ws` and DevKit are built from source at this step (2–10 minutes): the store with a ready release is connected
     only at step 4. This is normal.
   - At the end the script writes `C:\DBDev\home\env.ps1` — **this workspace's environment**. Our own Rust is installed into `home` and does
     not go into the user's PATH, so in every new PowerShell window, before `cargo …` commands, first run
     `. C:\DBDev\home\env.ps1` (a dot and a space at the start). Without it `cargo` will not be found or a different Rust
     from the machine will run. AI agent sessions in the workspace get the environment by themselves (`.claude/settings.local.json`).
3. **Machine settings:** put `team.kdbx` into `C:\DBDev\vault\`. Copy `config/local.example.toml` to
   `config/local.toml` (if it does not exist yet) and fill in `[vault] db = 'team.kdbx'`, `tier = 'team'` and `[store] url`, `user`.
4. **Data** (paks, `dbnative.dll`, the launcher kit, analysis tools) from the store:
   `. C:\DBDev\home\env.ps1`, then
   `cargo ws secrets unlock` — the `team.kdbx` password once for the working session (4 h by default; DevKit, "Game data",
   does the same), then `cargo ws bootstrap --profile=dev --apply`. Later `cargo ws dump --apply` and the AI agents use the
   same session without asking; `cargo ws secrets lock` closes it (`docs/SECRETS.md`, "Vault session").
   Enter the `team.kdbx` password only in response to the `Master password for …` line — typed after the command finishes
   it will run as a command and stay in the PowerShell history (then ask the owner to change the database password).
5. **Check:** `cargo ws doctor` and `cargo ws check` — 0 errors. `ws doctor` counts **your copy of the game** too: it
   is required (`docs/GAME_DATA.md`). Have Dungeonborne installed in Steam (your account that owns it; ~17 GB more
   free on the disk of `C:\DBDev`) — `cargo ws bootstrap --profile=dev --apply` copies the clean files of the
   install into `home\game\original` and leaves the install as it is; the same button in DevKit, "Game data". Once: `git config user.name` and `user.email`
   (your own) in `C:\DBDev\workspace`. Python 3.12 for `tools/py` and the site scripts: `ws bootstrap --apply` puts a
   portable one into `home\toolchains\python` (through `uv`, nothing goes into the system; a Python 3.12+ of your own is
   fine too). In a new window `env.ps1` puts it first in `PATH` (`env.ps1` from an older `bootstrap.ps1` — run it
   again); AI agents get it as `DB_PYTHON`. git and the GitHub CLI are portable too: `ws bootstrap --apply` puts
   PortableGit (with Git Bash, which Claude Code uses for its Bash tool) into `home\tools\git` and `gh` into
   `home\tools\gh`, and tells the AI agents where they are (`.claude/settings.local.json`). Sign `gh` in once yourself:
   `"$env:DB_GH" auth login` (browser), then `"$env:DB_GH" auth setup-git` — git then pushes to GitHub through it.
6. **For the backend** — libclang (LLVM), one of two ways:
   - without administrator rights: `cargo xtask setup --apply --tool=llvm` (LLVM from `tools.lock` into `home\toolchains\llvm`:
     ~860 MB download, ~4 GB on disk) and the variable `LIBCLANG_PATH` = `<home>\toolchains\llvm\bin` in the window where
     you build (checked 17.11: 185 backend tests);
   - or `winget install LLVM.LLVM` (an installer, needs administrator rights; the backend finds it by itself).

   Then `cargo test -p dbserver-rs` in `core/backend` (`core/backend/README.md`).
   Needed for the components `server` and everything that brings it (all presets but the base).
7. **Game data:** `cargo ws dump --apply` for icons, abilities, effects and curves from your copy of the game (needs the
   pak key from `team.kdbx`) — `docs/GAME_DATA.md`.
8. **Stand** (component `stand`; to try a change in the game on this PC): `cargo ws stand --apply` builds the stand
   copies of the game from your reference with the launcher's runtime and mods, then names the two steps only you can
   do — the DbTools task as administrator and the `hosts` block (exact text printed). Details — `tools/stand/README.md`.

Open the AI assistant (Claude Code: the CLI, or the Claude app, tab Code) in the `workspace` folder, not in its
parent: only there it loads the roles
(`.claude/agents`) and this workspace's environment. Working with AI is easiest through roles: "do this with the mods role", "work as backend" — the list of roles and what each does is
`agents/ROLES.md` (role files — `.claude/agents/`). Each role names its component; the agent says when a task needs a
component that is not on disk and adds it (`cargo ws components add`) or asks you.

## How to make a change

1. Fresh `main`: `git pull`. A branch from it: `fix/<short>` (a bug) or `feat/<short>` (something new).
2. The edit — inside your member (the member's `member.toml` and README say what is where). An edit in another member — if the task
   requires it and the link is in `depends_on`. **New things go next to the core, not into it** (`docs/EXTENDING.md`):
   a backend feature — a new `.rs` file and only its registration lines; a mod — a new folder; a season — a new
   folder. Experiments — `sandbox/` (not in git).
3. Before every commit — `cargo ws check` (0 errors). Edits in `xtask`, `tools/ws`, `apps/devkit` — also
   `cargo xtask ci`. Backend — `cargo test -p dbserver-rs` in `core/backend` (how to build — `core/backend/README.md`).
4. Commit: `git -c core.autocrlf=false commit`, message in English or Russian — what and why. Name and email — your own
   (`git config user.name` / `user.email` in the copy once after setup).
5. `git push -u origin <branch>` and a merge request (pull request) into `main` of the team repository. In the description: what
   was checked and how (tests, stand, in game), what was not checked.

An AI agent does not push or open requests by itself: only on a person's "yes" to a specific action (`CLAUDE.md`).

## What the owner does next

1. Reviews the request. Remarks — as comments on it; you make the fixes in the same branch.
2. Moves what is accepted into the private repository: `git format-patch main..<branch>` from the team copy and
   `git am -3` on their side. Lines that the export replaces (prod addresses, accounts, IDs) are different on the owner's side: if the edit
   touched them, `git am -3` stops on a conflict, and the owner resolves it by hand.
3. The next export (`cargo xtask export team --update=<clone of the team repository>`) brings the change into `main`
   of the team repository as a new commit. The request is closed with a link to that commit, not by merging the branch.
4. After that: `git switch main && git pull`, the old branch can be deleted.

## What the copy does not have

- Prod (VM, battle servers), deploy, backups, Forgejo, the Discord server — owner only. Need a deploy or
  data from prod — ask the owner.
- The repository holds no more game files than the server needs; your own copy of the game for the stand — from your own Steam
  (`tools/stand/run_local.ps1`, what it needs — in the script header and in `config/local.example.toml`).

## Where to start: tasks

`docs/FIX_LIST.md` and `docs/BACKLOG.md` have about 200 open items. About half need the game (a check on the stand),
a quarter — prod or the owner's decision. The tasks below are done with code and tests, without the game and without prod (analysis 17.11; find the item
by its label or words in the list). Before starting, tell the owner which one you take.

| Task | Where | Done when |
|---|---|---|
| FIX_LIST **C3** — empty dirty push on refusal | `core/backend`, `rpc/handlers/inventory_handlers.rs` | on a refused move there is neither a save nor `SCPlayerDirtyChgSync`; a test |
| FIX_LIST **C4** — `user_id` from the Steam ID tail | `core/backend`, `account_handlers.rs` | players without a launcher account get `user_id` from a counter in the DB; a test for the same tail |
| FIX_LIST **L5** — "in match" status for friends and party | `core/backend`, `account_handlers.rs` | `is_in_battle`, `last_battle_start_time` are taken from matchmaking; a test |
| FIX_LIST **B3a** — results experience from the last lobby choice | `core/backend`, `raid_handlers.rs` | experience and the results map — by the match level, not by the lobby choice; a test |
| FIX_LIST **M8a** — "Return to match" for a teammate | `core/backend`, `lobby_handlers.rs` | confirming returns a teammate with an ongoing match to their room; a test |
| FIX_LIST **Sh3** — battle pass experience 16518 | `core/backend`, `shop_handlers.rs` | the experience source is found in the tables, the level grows in a test |
| FIX_LIST "Buy now" −4254 — min. price | `core/backend`, `trade_handlers.rs` | `trade_basic_info` shows the same min. price as selling; a test |
| FIX_LIST — chance on vote options | `apps/launcher`, `ui/app.js` | an option shows "3 · 75%", the share as in `launcher_lobby` |
| BACKLOG — 2 warnings during build | `core/backend` | `cargo build` without warnings |
| BACKLOG — outdated `ExcelConfigjson/` | `core/backend`, `tools/py` | the directory is gone, scripts read `content/balance`, tests are green |
| BACKLOG — hot reload of tables | `core/backend` | an admin command rereads tables without a restart; a test |

With a stand (your own copy of the game) — the "done, not checked in game" items in `FIX_LIST`: inventory I3–I7 after a raid, mail
via GM `/mail`, order price for 2–3 pcs., pass quests and chests.
