# tools/ws — workspace tool

`ws-core` (library, also used by DevKit) and `ws` (command line). Run from anywhere inside the workspace:
`cargo ws <command>` (alias from `.cargo/config.toml`).

| Command | What it does |
|---|---|
| `check` | layout, machine paths, binaries, secrets, member links, visibility and leaks — before every commit |
| `doctor [--profile=<p>]` | what is missing on this machine: `config/local.toml`, home folders, tools from `config/tools.lock` |
| `bootstrap [--profile=<p>] [--prebuilt\|--from-source]` | the same, and fixes it (folders, tools by sha256) |
| `members`, `roles [--list=<tier>]` | members and their visibility (`public` ⊂ `team` ⊂ `owner`) |
| `secrets list\|init\|check\|run\|export` | secrets by `config/secrets.toml` in a KeePassXC database; values never printed |
| `game clone\|original\|diff` | working copies of the game in home (paks hard-linked to a reference copy) |
| `runs collect` | move client crash dumps into home |

Build and test: `cargo build --release -p ws`, `cargo test -p ws-core` (from the workspace root).
