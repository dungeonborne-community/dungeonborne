# 0003. `sandbox/` in the root: experiments outside git

- **Date:** 2026-11-18.
- **Decided by:** the owner ("a sandbox folder where AI agents do not restrict themselves with git and the rest").
- **Decision:** a top-level folder `sandbox/` in the workspace:
  - only `sandbox/README.md` is tracked; everything else is ignored (`/sandbox/*`, `!/sandbox/README.md` in
    `.gitignore`);
  - it is a shared folder in `config/workspace.toml [workspace.layout] shared`, so `ws check` accepts it in the root;
    ignored files are not tracked, so the other rules do not look at them;
  - inside it an agent may create, change and delete files and keep its own git repositories without asking; the
    safety rules of `CLAUDE.md` (secrets, nothing outside, anti-cheat, paks and the INTL login) still apply;
  - member code must not refer to `sandbox/` — `ws check` rule `sandbox` warns.
- **Why:** the workspace rules (members, `ws check` before a commit, deletion only with the human's word) are right for
  code that others use, but slow down trying ideas. Experiments in `home` are far from the code an agent works on; a
  folder in the root is next to it and invisible to git.
- **What changes:** the root gets one more entry; `docs/EXTENDING.md` and `CLAUDE.md` point to it; a result worth
  keeping moves into a member the normal way (branch, `ws check`, pull request). In a partial checkout
  (`docs/COMPONENTS_PLAN.md`) it is part of the base (a top-level folder without members).
