# Layout decisions (ADR)

This is where decisions that change the workspace layout are recorded: a new member in `workspace.toml`,
a new entry in the root, a new shared folder, a tool change. `CLAUDE.md` requires a record here before such a change.

Format: a file `NNNN-short-name.md` — date, decision, why, what changes, who decided.

| № | Decision |
|---|---|
| [0001](0001-workspace.md) | A single workspace `X:\DBDev` (workspace + home + vault), Cargo workspace idioms |
| [0002](0002-root-cargo.md) | Root Cargo workspace: `rust-toolchain.toml`, `cargo xtask`, `cargo ws`, DevKit |
| [0003](0003-sandbox.md) | `sandbox/` in the root: experiments outside git |
| [0004](0004-dumper.md) | `tools/dumper` + `ws dump`: game data from your own game copy, without a window |
