# 0002. Root Cargo workspace: rustup → xtask → DevKit

- **Date:** 2026-11-14.
- **Decided by:** the user ("the idiomatic Rust way: first Rust → xtask → DevKit build → everything else").
- **Decision:** the workspace root holds the Cargo workspace of the developer kit:
  - `Cargo.toml` (members `xtask`, `tools/ws/core`, `tools/ws/cli`, `apps/devkit/src-tauri`) and a single `Cargo.lock`;
  - `rust-toolchain.toml` — the Rust version for the whole tree (1.98.1, rustfmt, clippy);
  - `.cargo/config.toml` — the `cargo xtask` and `cargo ws` aliases;
  - `xtask/` — a new member: build tasks (setup, devkit, ci — item 2 of section 13.4).

  `core/backend`, `apps/launcher`, `apps/studio`, `tools/sniffer` stay separate Cargo workspaces (`exclude`), with
  their own `Cargo.lock`; inside `core/backend` `cargo xtask` still means the backend's xtask.
- **Why:** setup from scratch should start with a single trusted external tool (rustup), and everything of our own
  should be built from sources the AI can read (`docs/WORKSPACE_PLAN.md`, section 13).
- **What changes:**
  - `ws` is run as `cargo ws <command>` from anywhere in the workspace, not via `--manifest-path tools/ws/Cargo.toml`;
  - `tools/ws` and `apps/devkit` no longer have their own top-level `Cargo.toml` / `Cargo.lock`;
  - the `ws check` "depends" rule takes `x.workspace = true` dependencies into account (path from the root `Cargo.toml`);
  - the prod backend build (`ops/deploy/scripts/wsl_build_clean.sh`) takes only `core/backend` and `content` — root
    files do not get into it.
