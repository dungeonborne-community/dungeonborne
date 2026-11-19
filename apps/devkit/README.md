# DevKit

A window over `tools/ws/core` (ws-core) — the same as `ws doctor / bootstrap / check / secrets check`, but with buttons.
Plan and scope of the first version: `docs/WORKSPACE_PLAN.md`, "Step 7".

| Page      | What it does |
|-----------|------------|
| Start     | who you are (`config/local.toml [me]`: level, language, detail — the AI talks and reports to it, CLAUDE.md) and what you want to do: mode (quick / task / project), area (role), the task in your words → a task file in `home/runs/agents` and a terminal with Claude Code that reads it |
| Work      | what the agents did: their reports (`home/runs/agents/*.md`, format — agents/ROLES.md "Reports to the human"), tasks still in work, the branch, uncommitted changes, recent branches |
| Game data | the game reference (`ws game`): state, or the steps — open the Steam console, copy the `download_depot` line, register the download; the dump (`ws dump`): sets and counts, plan, Export; **vault session** — the master password once for 1–12 h: the same session as `ws secrets unlock` (a background agent, a copy of DevKit.exe started with `--vault-agent`), shared with the console and the AI agents; the pak key only for `ws dump`, the store password only for `ws bootstrap`; never written or shown, every use logged in `home/runs/vault` (docs/SECRETS.md) |
| Overview  | `ws doctor` (check) and `ws bootstrap` (fix) for what is installed on the machine (profile owner / dev / designer / agent — programs and data, not the person) |
| Components | what is on this machine (`ws components`, `docs/COMPONENTS_PLAN.md`): a checkbox per component, presets, "Show plan" and "Apply" (git partial checkout); then Overview → Fix installs their programs and data |
| Structure | `ws check` and the member list |
| Secrets   | the `config/secrets.toml` spec; KeePassXC database check with the master password. Secret values never reach the window |
| Backups   | `ops/backup/pc/backup.ps1`: snapshots and a backup now (restic → Storage Box) |
| Launch    | workspace / home / vault folders, Balance Studio, KeePassXC, Forgejo (needs the tunnel), a terminal with the agent |

There is no logic in the app: the commands in `src-tauri/src/lib.rs` are thin, ws-core does everything. The page passes no paths
or commands — only action names from a fixed set.

The interface is in English by default; the button at the bottom of the navigation switches to Russian (the choice is remembered).
Texts are in `ui/i18n.js`; findings from ws-core are shown as they come.

The workspace is looked up like this: `DBDEV_WORKSPACE`, then from `DevKit.exe` upwards, then from the current folder upwards; at every level —
also the sibling folder `workspace` (the `DBDev\{workspace,home,vault}` layout). If not found — the "Choose folder" button.

## Build
From anywhere in the workspace:

```powershell
cargo xtask setup            # plan: which build programs are missing (changes nothing)
cargo xtask setup --apply    # install trunk, tauri-cli, tailwindcss into home\tools (versions from config/tools.lock), ws and DevKit
cargo xtask devkit           # rebuild DevKit.exe
```

`--prebuilt` (default) — official trunk and tauri-cli releases by sha256 and the signed `cargo xtask dist` release
for ws and DevKit; `--from-source` — everything is built here. A permanent choice — `config/local.toml [setup] build`. Details —
`docs/WORKSPACE_PLAN.md`, section 13.1.

The system PATH is not changed: xtask itself puts the program folders first in the PATH of its builds. The shared target is
`home\cache\target` (or `CARGO_TARGET_DIR`); the result is `release\DevKit.exe`, a single file.

The `devkit` crate is part of the root Cargo workspace (`docs/adr/0002-root-cargo.md`); `cargo xtask ci` checks it
together with the rest (fmt, clippy, test, `ws check`).

## Sandbox
`cd ui; trunk serve` → http://127.0.0.1:1440 — the page with fake answers (`ui/dev/mock-tauri.js`), without Rust.
