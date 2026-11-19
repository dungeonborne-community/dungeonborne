# 0001. A single workspace

- **Date:** 2026-11-14.
- **Decided by:** the user (project owner).
- **Decision:** all work lives in one directory `X:\DBDev`:
  - `workspace\` — git, members per `workspace.toml`;
  - `home\` — DB_HOME: programs, the game, dumps, caches, logs;
  - `vault\` — secrets, KeePassXC.

  The layout mirrors a Cargo workspace: a list of members, `member.toml`, `tools.lock`/`artifacts.toml` as lock files,
  a single `ws` utility. People (`docs/`, Balance Studio) and AI (`agents/`, `.claude/`) are kept apart.
- **Why:** work was scattered across G:, F:, C:, Temp; agents put files wherever they liked; moving to another PC lost
  the agent's memory and broke paths.
- **What changes:**
  - machine data paths — only in `local.toml`;
  - `ws check` before every commit;
  - git server — Forgejo on the VM;
  - backups — restic to a Storage Box.

  Details — `docs/WORKSPACE_PLAN.md`.
