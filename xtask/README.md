# xtask — workspace tasks (`cargo xtask` from the root)

The root `cargo xtask` builds and checks the developer kit itself. `cargo xtask help` lists everything; the main ones:

| Command | What |
|---|---|
| `setup [--apply]` | tools that build the windows (trunk, tauri-cli, tailwindcss) into `home\tools`, then `ws` and DevKit (signed release or source) |
| `devkit` | build DevKit.exe here |
| `ci` | fmt, clippy, tests, `ws check` — run before committing changes to `xtask`, `tools/ws`, `apps/devkit` |
| `dist [--sign]`, `dist push` | release of `ws` and DevKit with SHA256SUMS (signed by the owner), upload to the artifact store |
| `pack designer` | the designer's package (Balance Studio + kit) |
| `export <edition>` | one-way copy of the workspace for an edition (owner) |

Not to be confused with `cargo xtask` inside `core/backend`: that one works on game data (balance, season, mods, loot,
names, tables). Which one runs depends on the folder you are in. `dist.pub` is the public key that checks signed releases.
