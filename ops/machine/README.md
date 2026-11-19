# ops/machine — developer machine setup

The scripts are run by a human (not the agent: these are system settings). Without `-Apply` each one only shows what it would do.

| Script | What | Rights |
|---|---|---|
| `set-user-env.ps1 [-Apply\|-Revert]` | Windows user variables: `DB_HOME`, `RUSTUP_HOME`, `CARGO_HOME`, `CARGO_TARGET_DIR` and caches — in `home` | normal |
| `move-wsl.ps1 [-Apply]` | moves the WSL Ubuntu disk to `home\toolchains\wsl` | normal |
| `install-dbtools.ps1 [-Apply] [-MoveFrom <folder>]` | the DbTools task for the stand (launching the game and removing mutexes without UAC), paths from `local.toml` — `tools/stand/README_admin.md` | administrator |

Paths are taken from `config/local.toml` (`tools/stand/paths.ps1`).
