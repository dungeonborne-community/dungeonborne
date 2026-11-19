---
name: dungeonborne-workspace-plan
description: "14.11 plan for one portable workspace X:\\DBDev (workspace git + home DB_HOME + vault), Cargo-workspace idioms; docs/WORKSPACE_PLAN.md"
metadata:
  visibility: public
  node_type: memory
  type: project
---

Decided on 14.11: everything in one directory `X:\DBDev` (200 GB on F:):
- `workspace\` (git, members with member.toml, workspace.toml, ws check);
- `home\` (DB_HOME: toolchains, game, dumps, re, runs, cache);
- `vault\` (secrets).

Cargo workspace idioms, but not tied to the language. People (`docs\`, Studio) and AI (`agents\`, `.claude\`) are not mixed. Languages and caches move off C: via `CARGO_HOME`/`RUSTUP_HOME`/`UV_*` etc.

Full plan: docs/WORKSPACE_PLAN.md. 14.11 step 1 done: `X:\DBDev\workspace` — a clone of backups with history + the launcher (subtree), layout by members, `workspace.toml`, `member.toml`, `CLAUDE.md`, memory in `agents/memory`. backend: tests 167/167 and build in WSL; Studio and the launcher build. Prod and other sessions still work in G:; carry new commits over like this: `git fetch g-backups && git merge g-backups/main`, launcher — `git subtree pull --prefix=apps/launcher g-launcher main`. 14.11 step 2 also done: people's data in content/, mods in game/; paths — xtask/src/ws.rs, tools/py/wspaths.py, tools/stand/paths.ps1; drive letters only in local.toml. Table of old paths backups -> workspace — in the workspace CLAUDE.md. 14.11 step 3 done: tools/ws (ws-core + ws.exe): check (before commit, 0 errors), doctor, bootstrap (tools.lock, artifacts.toml, legacy: = G:), game clone, runs collect. 14.11 step 4 (without system settings): Rust stable + cargo copied to home/toolchains (workspace agents already use them via settings.local.json), research data in home/re, FModel in home/dumps, UE projects in home/unreal, crashes in home/runs. The owner runs ops/machine/set-user-env.ps1, move-wsl.ps1, install-dbtools.ps1 (at the switch-over). Reference game copy ready 14.11: home/game/original/16716489 (download_depot 2448970 2448971 4035623128766163436, 17 GB, read-only). ws game diff: all installs differ only in cacert.pem, steam_api64.dll and added ue4ss/logs (0.2–1.3 GB).

**Why:** work was scattered across G:, F:, C:, Temp; agents put files wherever they like; moving to a new PC ([[dungeonborne-build-pc-risk]]) loses agent memory.

**How to apply:** plan new files and tools for this structure. Windows variables are set by the owner (script). Deletion on C:/G: only with confirmation. Related: [[dungeonborne-msix-appdata]], [[dungeonborne-local-dev]].

14.11 step 5 (secrets): secrets.toml (inventory without values) + ws secrets list/init/check/run (crate keepass 0.17 — 0.8 lost attachments; kdbx and env adapters). ws does not overwrite an existing .kdbx. Waiting on the owner: KeePassXC, ws secrets init, filling in, ws secrets check. Gitea and Storage Box — next.

14.11 steps 5 and 6 done: Forgejo (origin `<owner>/dungeonborne` via tunnel), Storage Box backups of the VM and the workstation, KeePassXC + ws secrets; clean clone + bootstrap + clean agent session passed, remarks closed. Next step 7 (DevKit.exe) and 8 (new PC); tailwindcss/uv — in tools.lock.

14.11 step 7 done (d97e8b4): apps/devkit — DevKit.exe (Tauri 2 over ws-core: doctor/bootstrap, check, secrets, restic backups, launch). Build `cargo tauri build --no-bundle` → home\cache\target\release\DevKit.exe; it also looks for the workspace in a sibling folder `workspace`. Checking the real window: WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333 + CDP (invoking commands). Next step 8 (new PC); tailwindcss/uv as download — not done.

14.11 step 8 plan rewritten (plan section 13 + docs/PACKAGES.md): build from scratch with bootstrap.ps1 (dry-run by default, everything by sha256, verifiable by AI), roles owner/team/designer/public/agent; prod secrets owner-only, a designer gets a zip without git and without secrets (Studio + kit + balance assistant prompt); ws pack, ws secrets export, public edition as a separate repository; offline emergency kit for the owner.

14.11 (later), decision: following Rust idioms — "rust → xtask → DevKit build → everything else"; the audience is AI agents and newcomers who have not heard of Ghidra or the backend; our work (research, memory, ops) and the public part kept separate. Plan section 13 rewritten: bootstrap.ps1 installs only rustup+git and clones; root Cargo workspace (xtask, ws, devkit), cargo xtask setup/devkit/ci; no prebuilt exes (except the designer zip); the public edition is a one-way export via cargo xtask export public.

14.11 section 13.4 item 1 done (adr/0002): root Cargo workspace (xtask, ws-core, ws, devkit), rust-toolchain.toml 1.98.1, aliases `cargo ws <cmd>` (instead of --manifest-path tools/ws/Cargo.toml) and `cargo xtask` (help only for now); backend/launcher/studio/sniffer — their own workspaces (exclude), in core/backend `cargo xtask` = xtask backend. Builds in the workspace — with RUSTUP_HOME/CARGO_HOME from home\toolchains (settings.local.json). Next — item 2: xtask setup/devkit/ci.

14.11 item 2 done: cargo xtask setup/devkit/ci/dist. Decision: "the choice is prebuilt artifacts or a build from scratch, also for our own builds". --prebuilt (default; local.toml [setup] build) — trunk/tauri-cli from official releases by sha256 ([tool.prebuilt] in tools.lock), ws/DevKit from a minisign-signed release from cargo xtask dist (xtask/dist.pub, id in the signature); --from-source — everything here. Clean machine: 2.5 min versus 30. The real dist key has not been created yet (keygen — only with the owner's explicit go-ahead). Do not run downloaded exes before the hash check (the classifier refused).

14.11 item 3 started: cargo-xwin checked in a "clean" environment (no VS/LLVM in PATH). ws/xtask build via cargo-xwin 0.23.1 + Microsoft CRT/SDK (home\cache\xwin, 1.1 GB) + rust-lld.exe copied as lld-link.exe; DevKit (Tauri) additionally needs clang-cl and llvm-rc (vswhom-sys) — LLVM without VS works; portable LLVM — a .tar.zst archive of 264 MB. Admin rights are needed neither for prebuilt nor for source. Trap: in PowerShell `| Select-Object -First N` cuts the build short — write the log to a file.

15.11 item 3 done: bootstrap.ps1 + bootstrap.manifest.toml in the root. Clean run without VS (-Linker xwin) in X:\DBDevTest2: 14 min, DevKit works. Without VS, settings (LIB/INCLUDE/clang-cl/llvm-rc/DBDEV_XWIN) are written to config.toml in CARGO_HOME; LLVM — .tar.xz (tar.exe does not take zst). rustup: auto-self-update disable. .gitattributes: apps/*/src-tauri/Cargo.toml eol=lf (tauri writes LF). Test folders X:\DBDevTest, X:\DBDevTest2 — the owner can delete them (Remove-Item there is blocked for the agent). Not verified: clone from Forgejo via tunnel, MinGit, shortcut.

15.11 item 4 done: one word, visibility (public ⊂ team ⊂ owner) — member.toml, [workspace.visibility], memory note header (absent — owner), secrets.toml. ws check: visibility, leaks (IP, owner-markers, game-files) — warnings; cargo ws roles. Set visibility in metadata for every new memory note. 17 real leaks recorded in plan 13.4 (item 7 work).

15.11 item 5 done: cargo xtask pack designer [--sign] — Studio + season kit (xtask backend balance kit) + README/CHANGELOG/AGENTS.md + signing-key.pub + SHA256SUMS(.minisig) → home/out/pack. Names in the package in Latin script (PS 5.1 + SHA256SUMS). Trap: when moving cargo registry/src in step 4 all nested `target` folders were lost (cc-1.11.0/src/target) — the cache is set aside in registry/src.broken-20261115; do not exclude `target` when copying caches.
