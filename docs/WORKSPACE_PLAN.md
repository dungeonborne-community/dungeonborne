# Unified Dungeonborne workspace: plan

Status: 2026-11-14, **steps 1–6 done** (section 12; from step 4 the user still has the Windows variables and WSL), steps 7–8 not started. User's decisions: layout like a Cargo
workspace (idioms, not the language), people and AI are not mixed, everything in one directory `X:\DBDev`, one repository together with
the launcher, secrets in KeePassXC, git server — the owner's Forgejo (14.11); the team has its own repository of the team edition
(`docs/CONTRIBUTING.md`); Discord may be touched.

## 0. Why

Right now the work is scattered: `<data>` (about 30 folders, 5 of them backend copies), the client on F:, the battle server in
Steam, the agent's knowledge in `C:\Users\...\.claude`, deploy scripts in a temp folder, languages and caches on C:. Agents decide
on their own where to put files. Moving the work to another disk or PC is impossible: the agent's memory would be lost and paths would break
(G: is hardcoded in 67 files, F: in 30, E: in 9).

**Goal:** one directory. Copy it, run `ws bootstrap`, open the agent, and it continues the work.

## 1. Root: everything in one directory

```
X:\DBDev\                      moved as a whole (caches need not be copied)
  workspace\                   git repository: what people and agents write
  home\                        DB_HOME: programs, game, dumps, caches, logs
  vault\                       secrets; not copied together with the rest (section 6)
```

The principle is like Cargo's: the project (`workspace\`) holds only what is written by hand, and dependency lists with versions.
What is downloaded, unpacked and built lives in `home\` (like `CARGO_HOME` and `target\`) and is recreated by a command.

## 2. Repository `workspace\`

| Cargo | Ours |
|---|---|
| `Cargo.toml` `[workspace] members` | `workspace.toml`: all members. Whatever is not in the list is not in the project |
| `Cargo.toml` `[package]` | `member.toml` for each member: name, kind, language, version, commands, links |
| path dependencies | `depends_on` in `member.toml` |
| `[workspace.dependencies]` | shared tool versions: Rust, Python, Tailwind, UE4SS, repak |
| `[workspace.lints]` | shared rules: naming, encoding, no absolute paths, no secrets |
| `Cargo.lock` | `tools.lock` (programs) and `artifacts.toml` (heavy data) with versions and hashes |
| `target\` | `home\out\`: all builds and releases |
| `cargo xtask`, `cargo new` | `ws build|test|release|check|new|bootstrap|dump|game` |

```
workspace\
  workspace.toml  tools.lock  artifacts.toml
  local.toml               [not in git] machine paths: DB_HOME, Robot, VM
  core\
    backend\               server (Rust, its own Cargo workspace inside)
    ds-agent\              agent on Robot
    protocol\              contracts: Network.pb, Configure.pb, dbserver/1, dbmod/1, dbbalance-patch/1
  apps\
    launcher\              player launcher
    studio\                Balance Studio, the designer's app
    devtool\               backend profiles, clients, GM console (currently launcher/dev-app)
  web\
    landing\               site
  game\
    ue4ss\  paks\  unreal\  client-mod\
  content\
    seasons\  balance\  loot\  names\
  ops\
    deploy\  vm\  robot\  docker\     deploy; the scripts from the temp folder go here too
  tools\
    ws\                    ws itself: build, structure check, templates
    py\                    stat_watch, rpc_evidence, loot_audit etc.
    sniffer\  stand\       (stand = dbtest)
  research\                research notes and scripts, without heavy data
  docs\                    FOR PEOPLE: architecture, specs, runbooks, adr, FIX_LIST, BACKLOG
  agents\                  FOR AI: memory\, <role>\, rules
  .claude\                 skills\, agents\ (roles), settings.json
  CLAUDE.md                agent entry point: find the member in workspace.toml, work inside it
```

Example `member.toml`:

```toml
[member]
name = "launcher"
kind = "app"               # service | app | site | mod | content | tool | contract
lang = "rust+html"
version = "1.7.5"
depends_on = ["core/protocol"]

[commands]
build   = "cargo tauri build"
test    = "cargo test"
release = "ws release launcher"   # puts it into home\out\release\launcher\
```

**`ws check`** (before a commit) fails if: a new file or folder in the root; a member not from `workspace.toml`; a link not
listed in `depends_on`; an absolute path outside `local.toml`; a binary, archive or secret in git. A new member — only via
`ws new <kind> <name>` and an entry in `docs\adr\`.

Splitting `core\backend` into crates is a separate task, unrelated to the structure.

## 3. Working base `home\` (DB_HOME)

```
home\
  toolchains\
    rustup\  cargo\        RUSTUP_HOME, CARGO_HOME
    python\  uv-cache\     UV_PYTHON_INSTALL_DIR, UV_CACHE_DIR
    go\                    GOPATH, GOCACHE (PVE in Go)
    wsl\                   Ubuntu disk (ext4.vhdx)
  tools\                   FModel, Ghidra, UE4SS SDK, repak, retoc, kismet-analyzer (per tools.lock)
  game\
    original\<build>\      game reference: read-only, hashes
    client\  server\  test\  working copies, made by `ws game clone`
  dumps\<build>\           unpacked paks, ExcelConfig, UHT, icons; made by `ws dump`
  re\                      Ghidra projects, annotations, BP analyses: valuable
  runs\                    logs, rpc_dump, probes, client crashes, stand DB snapshots; rotated
  worktrees\               git working copies for parallel agents
  cache\
    target\                shared CARGO_TARGET_DIR for all members and worktrees
    sccache\  ue-ddc\  tmp\
  out\                     builds and releases
```

### Budget 200 GB

| Folder | Size |
|---|---|
| `game\`: reference and 3 copies of 16 GB each | 64 GB (~20 GB if unchanged paks are linked with hard links) |
| `dumps\`: no more than 2 builds | ~20 GB |
| `re\` + research | ~15 GB |
| `toolchains\`: rustup 9.3, cargo 2.9, WSL 10, Python ~1 | ~25 GB |
| `tools\` | ~6 GB |
| `cache\` | ~25 GB |
| `runs\` (with rotation), `worktrees\`, `out\` | ~30 GB |
| **Total** | **~185 GB, with links ~140 GB** |

Rules: dumps for no more than two builds; one shared `target` (otherwise each working copy accumulates 3–7 GB); do not accumulate
prod DB snapshots. Check hard links to paks first: ACE must not complain, the game must not write into paks.
Unreal Engine (for `ue_jester`) and Visual Studio Build Tools stay on C:, they are not in the budget.

## 4. System disk: what ends up there and what to do about it

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 5. Specialised agents

Role = `.claude\agents\<role>.md` + `agents\<role>\` for its notes. Parallel agents get their own working copies
in `home\worktrees\` and hand back a branch.

| Role | Writes | Reads | Not allowed |
|---|---|---|---|
| backend | `core\` | `dumps`, `runs` | ops, prod |
| launcher, web | `apps\launcher`, `web\landing` | `core\protocol` | backend |
| balance | `content\balance`, `content\seasons` | `dumps` | code |
| mods | `game\` | `dumps`, `re`, `home\game\test` | `home\game\original` |
| re | `research\`, `home\re` | `dumps`, `home\game\original` | the rest of the repository; pak and INTL login analysis only after the user's word |
| ops | `ops\` | everything | deploy only after the user's "yes" |

Shared memory — `agents\memory\`; the user's rules (Discord, disk D:, deploy, deletion) — in `CLAUDE.md` for all roles.

## 6. Secrets

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 7. Backups (3-2-1 by value)

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 8. Machines

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 9. DevKit: workstation installer

`DevKit.exe` (Tauri) on a clean PC creates `X:\DBDev` and downloads everything needed: repository, programs, game, data,
secrets, documents. Models: rustup-init, Visual Studio Installer, JetBrains Toolbox.

**Logic lives in a library, not in the exe.**

| Part | What it is | Who uses it |
|---|---|---|
| `tools\ws\core` (`ws-core`) | Rust library: all the logic | both shells below |
| `tools\ws\cli` (`ws.exe`) | command line | agents, Robot, CI, a person in a terminal |
| `apps\devkit` (`DevKit.exe`) | Tauri window, HTML + Tailwind, Windows 11 look, like the launcher | a person |

The launcher is built the same way (library `dbl` + window). `launcher\dev-app` (backend profiles, clients, Host Game,
GM console) eventually moves into DevKit.

**Profiles** — like component sets in the Visual Studio installer:

| Profile | Installs |
|---|---|
| Owner | everything: code, programs, game, dumps, ops, secrets |
| Developer | code, languages, programs, game, dumps; no ops and secrets |
| Designer | a ready package without git: Balance Studio, balance kit, assistant prompt (section 13) |
| Agent or builder | code, languages, `ws`; no window |

**Where things come from:**

| What | Source |
|---|---|
| Repository | owner — Forgejo (14.11); team — the team edition repository (address in `config/bootstrap.manifest.toml`) |
| Languages and programs | official sites per `tools.lock`, with hash verification |
| Game reference | a Steam install or private storage; not distributed publicly |
| Dumps, game working copies | built locally (`ws dump`, `ws game clone`) |
| `home\re\` | private storage (Storage Box) |
| Secrets | password manager (KeePassXC or Bitwarden): DevKit asks to open it and lays them out into `vault\`; only the "Owner" profile; never downloaded in plain form |

**Requirements:** installation can be repeated without harm (what is already done is skipped); large downloads resume; every file
is checked against its hash; `doctor` finds and fixes deviations; DevKit updates itself.

## 10. Order of work (G: and prod are not stopped)

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 11. Open questions

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 12. Migration state

The section about the machine, prod, backups and the owner's secrets is in the owner's document `docs/owner/WORKSPACE_OWNER.md` (not present in the team copy).

## 13. Building from scratch the Rust-idiomatic way: rustup → xtask → DevKit → the rest (plan 14.11)

Edition spec "what goes to whom" — `docs/PACKAGES.md`.

### 13.0 Who it is for and what we split

- **The work is done by an AI agent, the human gives consent.** A developer, "Joe", does not know what a backend, a battle server,
  UE4SS, a pak or Ghidra is, and should not have to. His entry point is "give this link to your AI". So the main documents
  of the public edition are written for an agent without context: `AGENTS.md` (order of checking and installing), the `README` of each
  member (what it does, how to build, how to check), a glossary. For a human — a one-screen `README`.
- **Our labour and the public are kept apart.** Research — Ghidra, protocol captures (`research/rpc-evidence`), analyses, probes,
  agent memory, FIX_LIST, our server and its ops — stays in the private repository. What goes out is the result: the framework
  code and a description of how to use it, without how it was obtained.

### 13.1 Order the Rust-idiomatic way

Like rustup → cargo → `cargo xtask` (the cargo-xtask pattern): from outside we trust only the official rustup, git and
downloads with a pinned sha256; everything of ours is built from sources the AI can read.

| # | Stage | What it does | What comes from outside |
|---|---|---|---|
| 0 | `bootstrap.ps1` | checks Windows and free space; linker — Visual Studio Build Tools if installed, otherwise without them: cargo-xwin + LLVM in `home` (14.11, "Building without Visual Studio" below; no administrator rights needed); `rustup-init` (version + sha256) → `home\toolchains` (`RUSTUP_HOME`, `CARGO_HOME`, `--no-modify-path`); system git or MinGit (sha256) → `home\tools\git`; repository clone (tag + commit hash); then — `cargo xtask setup` | rustup-init, MinGit, Build Tools |
| 1 | Rust | `rust-toolchain.toml` in the root pins the version, rustup installs it itself; `Cargo.lock` in git, all builds `--locked` | Rust components |
| 2 | xtask | root Cargo workspace: `xtask`, `tools/ws/core`, `tools/ws/cli`, `apps/devkit/src-tauri`; alias `cargo xtask` in `.cargo/config.toml`. `cargo xtask setup` shows the plan, `--apply` installs trunk and tauri-cli (prebuilt by sha256 or `cargo install --locked` — choice below) and tailwindcss (sha256) into `home\tools`, then builds `ws` and DevKit; PATH is changed only for its own builds | crates.io, tailwindcss |
| 3 | DevKit | `cargo xtask devkit` → `home\cache\target\release\DevKit.exe`; `ws.exe` next to it — for agents; the shortcut is made by `bootstrap.ps1` (writing outside the root — with the human's consent) | — |
| 4 | The rest | `ws bootstrap` = the "Fix" button in DevKit: `local.toml`, `home` folders, programs, data, game, secrets, agent environment; checks — `ws doctor`, `ws check`, `cargo xtask ci` | Storage Box, Steam |

**Choice: prebuilt or from source** (user's decision 14.11) — flags `--prebuilt` / `--from-source` for
`cargo xtask setup` and `ws bootstrap`, otherwise `local.toml [setup] build`, otherwise `prebuilt`:

| | `prebuilt` | `source` |
|---|---|---|
| third-party programs (trunk, tauri-cli) | official release: url, sha256 of the archive and exe — in `tools.lock` (`[tool.prebuilt]`) | `cargo install --locked` of the version from `tools.lock` |
| ours (`ws`, DevKit) | release `cargo xtask dist --sign` for the current commit: minisign signature of `SHA256SUMS` (public key — `xtask/dist.pub`, release id in the signature), hash of each file → `home\tools\{ws,devkit}` | `cargo build` / `cargo tauri build` here |
| time on a clean machine | ~2.5 min | ~30 min (trunk 9, tauri-cli 8 min) |

No key, no release for this commit, or uncommitted changes — ours are built here. A release that fails
the signature or hash check — setup stops and installs nothing. Where releases live — `local.toml [dist] source`
(default `home:out/dist`); distribution via Forgejo or Storage Box is the next step. The designer package is also prebuilt
exes with `SHA256SUMS` and the owner's signature: the designer has no Rust.

The own Cargo workspaces of `core/backend` (its own xtask: balance, mods, season…), `apps/launcher`, `apps/studio` stay
as they are — `exclude` in the root `Cargo.toml`. Later the root xtask calls them (`cargo xtask backend …`), so that there is
a single entry point.

### 13.2 Profiles by stage

| Profile | 0–1 Rust | 2–3 xtask, DevKit | 4 the rest | Secrets |
|---|---|---|---|---|
| `owner` | yes | yes | everything: Storage Box, Forgejo, ops | the whole database — from the emergency kit |
| `team` | yes | yes | without ops and owner data | `team.kdbx` (a subset), password via a separate channel |
| `public` | yes | yes | framework; game data — from their own copy | their own empty database (`ws secrets init`) |
| `agent` | yes | only `ws` | per task | `ws secrets run` for the duration of a command |
| `designer` | no | no | zip: Studio + balance kit + the assistant's `AGENTS.md` | no |

Common rules for all stages:
- without `-Apply` — only a plan;
- re-running is safe;
- no administrator rights (Build Tools are optional: without them — cargo-xwin + LLVM in `home`, 14.11);
- no `iex (irm …)`, base64, `-EncodedCommand`, Defender exclusions, autostart, scheduler tasks, downloads without
  a hash, bare IPs or telemetry.

`AGENTS.md` sets the order: read the script → compare with the manifest → run without `-Apply` → the human's "yes" →
`-Apply` → `ws doctor`, `ws check`.

**The owner's emergency kit** (offline, in case of a new PC) — in the owner's document `docs/owner/SECURITY.md`.

### 13.3 Private repository and public edition

- The source of truth is the private `owner/dungeonborne`. The public edition is a separate repository that receives
  a one-way export `cargo xtask export public` (like Copybara): only members with `visibility = "public"`, without
  the owner's history.
- Before the export — a leak check (the `ws check` rule for `public`): no machine paths, IPs, accounts, prod secret
  entries, Ghidra and decompilation, game files (`.pak`, `.utoc`, `.ucas`, `.uasset`, `.pb`, `core/backend/assets/excel`, tables).
- Community contributions come as pull requests to the public repository; they are carried over to the private one as a patch (later — automatically).
- `cargo xtask pack <profile>` — the designer package and the team kit; `ws secrets export` — a subset database (`team` entries).

### 13.4 Order of work

| # | What | Done when |
|---|---|---|
| 1 | Rust: `rust-toolchain.toml`; root `Cargo.toml` (xtask, ws, devkit), `tools/ws` and `apps/devkit` move into it, a single `Cargo.lock` | `cargo build --locked` from the root builds ws and DevKit |
| 2 | xtask: `setup`, `devkit`, `ci` (fmt, clippy, test, `ws check`) | on a machine with only Rust, `cargo xtask setup` gets as far as DevKit.exe |
| 3 | `bootstrap.ps1` + `bootstrap.manifest.toml`; check from an empty folder (a different root) | from the script to `ws doctor` with 0 errors without manual steps, except Build Tools |
| 4 | `visibility` marking (members, files outside members, memory notes, secrets) + the leaks rule | `ws check` shows what is in which role |
| 5 | Designer package (`cargo xtask pack designer`) | the designer works from a zip without git |
| 6 | Team subset database (`ws secrets export`) | a second person sets up a workstation without prod secrets |
| 7 | Public edition: `export public`, `AGENTS.md`, glossary, member READMEs for an agent, `LICENSE` / `THIRD_PARTY` / `SECURITY` / `DISCLAIMER`. Going out — only after the user's "yes" | an outside AI checks the script and installs the framework following `AGENTS.md` |
| 8 | New PC: `bootstrap.ps1 -Profile owner` + emergency kit, rotation of all secrets (`rotate = true`) | the old PC is not needed |

**Item 1 done 14.11** (`docs/adr/0002-root-cargo.md`):
- root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` (1.98.1), `.cargo/config.toml` (`cargo xtask`, `cargo ws`);
- `xtask/` only prints help for now;
- `tools/ws` and `apps/devkit` moved into the root workspace, all crate versions — from their previous lock files;
- `cargo build --locked --release --workspace` and `cargo test --locked --workspace` pass, `cargo tauri build`
  builds DevKit.exe;
- backend, launcher, Studio, sniffer resolve in their own workspaces, their lock files did not change;
- the `depends` rule takes `x.workspace = true` into account.

For item 3: rustup does not install the version from `rust-toolchain.toml` by itself (warning rust-lang/rustup#4836), so
`bootstrap.ps1` calls `rustup toolchain install` explicitly.

**Item 2 done 14.11:**
- `cargo xtask setup [--apply] [--prebuilt|--from-source]`, `devkit`, `ci` (DevKit page, fmt, clippy `-D warnings`,
  test, `ws check`), `dist [--sign]`, `dist keygen`;
- `tools.lock`: trunk, tauri-cli — `kind = "cargo"` + `[tool.prebuilt]`; tailwindcss — `download` by sha256; everything in
  `home\tools`, PATH is changed only for xtask builds;
- verified: setup from source (30 min) and prebuilt (2.5 min) on empty `home\tools\{trunk,tauri-cli}`; a signed
  release passes, a tampered file — refusal (exit 1);
- the root workspace crates are formatted with rustfmt (defaults), clippy without warnings;
- `secrets.toml` has `dist_signing_key` (optional). The real key has not been created yet: `cargo xtask dist keygen
  --secret-out <file outside the workspace>` — after the user's word.
- on a clean clone `cargo build --workspace` requires the built DevKit page (`ui/dist`) — it is built by `setup`, `devkit`
  and `ci`.

**Building without Visual Studio (cargo-xwin check, 14.11, start of item 3).** Environment as on a clean machine:
- PATH has only `home\toolchains\cargo\bin`, programs from `home\tools` and System32;
- no VS, LLVM or Git;
- the `LIB` / `INCLUDE` variables removed.

| What is built | Needed | Result |
|---|---|---|
| `ws`, `xtask` (pure Rust) | cargo-xwin 0.23.1 (release by sha256) + CRT and Windows SDK from Microsoft (cargo-xwin downloads them itself, 1.1 GB in `home\cache\xwin`) + linker: `rust-lld.exe` from the toolchain, copied as `lld-link.exe` (cargo-xwin requires this name) | built in 36 s, `ws check` — 0 errors |
| DevKit (Tauri; likewise the launcher and Studio) | plus the C/C++ compiler `clang-cl` (dependency `vswhom-sys` of embed-resource) and `llvm-rc` — from LLVM | without LLVM — error; with LLVM (without VS) built in 24 s, the window works |

Conclusion:
- Administrator rights and Visual Studio are not needed.
- The `prebuilt` path (default): DevKit is not built on the machine, only cargo-xwin is needed — for `xtask` and `ws`.
- The `source` path: DevKit additionally needs LLVM — the official archive `clang+llvm-<version>-x86_64-pc-windows-msvc.tar.zst`
  (264 MB) in `home\toolchains\llvm`.
- If Build Tools are already installed, the normal build stays as is.

CRT and SDK are downloaded under the Microsoft license (the same as for Build Tools).

**Item 3 done 15.11** — `bootstrap.ps1` + `bootstrap.manifest.toml` in the root:

```
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root <empty folder>            # plan
powershell -ExecutionPolicy Bypass -File bootstrap.ps1 -Root <empty folder> -Apply     # do it
   [-Repo <url or path>] [-Ref <branch|tag|commit>] [-Build prebuilt|source] [-Linker auto|msvc|xwin]
```

Script steps:
1. rustup-init (sha256) with `--no-modify-path`; rustup self-update is turned off.
2. git — system or MinGit (sha256).
3. Clone, commit check.
4. `rustup toolchain install`.
5. Linker:
   - Visual Studio, if `vswhere` found it;
   - otherwise cargo-xwin: CRT and SDK in `home\cache\xwin`, `rust-lld` under the name `lld-link`. Settings (`LIB`,
     `INCLUDE`, `clang-cl`, `llvm-rc`, `DBDEV_XWIN`) are written to CARGO_HOME's `config.toml` — that is outside git. After that
     plain `cargo xtask`, `cargo ws`, `cargo tauri build` work without wrappers.
6. `cargo xtask setup --apply`. Without VS it installs LLVM itself (`tools.lock`, `manual = true`, `.tar.xz` 860 MB:
   Windows tar.exe does not unpack `.tar.zst`).

All output is in English; the comments in the script are for AI review.

Check (15.11), from an empty `X:\DBDevTest` with `-Linker xwin`, i.e. without Visual Studio even though it is present:
1. The first run got as far as LLVM. Found and fixed:
   - `.tar.zst` did not unpack;
   - rustup updated itself;
   - empty version string;
   - programs' stderr could stop the script when output was redirected.
2. A repeat run on the same root skipped the finished steps and built `ws` and DevKit (27 min, mostly LLVM).
4. A full run from scratch into a new empty `X:\DBDevTest2` with the fixes: 14 min, exit code 0.
   - rustup stayed at 1.29.1, `ws check` — 0 errors, DevKit opens and finds its workspace.
   - Found: after `cargo tauri build` a clean clone showed `apps/devkit/src-tauri/Cargo.toml` as modified — tauri
     writes LF, Git for Windows outputs CRLF by default. Fixed with `.gitattributes` (`eol=lf` for these files).
3. The build scripts' output contains no Visual Studio and Windows SDK paths, except a search path that `vswhom-sys`
   adds because VS is installed. No libraries are taken from there.

Leftovers of the check:
- `home\cache\target-xwin` (1.5 GB), `X:\DBDevTest` and `X:\DBDevTest2` (4–5 GB each) — can be deleted;
- `home\tools\cargo-xwin` and `home\cache\xwin` are not needed on the main root: it has Visual Studio.

Not done in item 3:
- clone from Forgejo through the tunnel (verified with a local path);
- MinGit (this PC has git, the path is visible only in the plan);
- DevKit shortcut.

**Item 4 done 15.11.** One word `visibility` for everything, levels `public` ⊂ `team` ⊂ `owner`:

| Where | How |
|---|---|
| member | `member.toml`: `[member] visibility`; `[visibility] team = [...]`, `owner = [...]` — stricter parts of the member |
| outside members | `workspace.toml [workspace.visibility]`: `public` / `team` / `owner` (path patterns), `default = "team"` |
| `agents/memory` notes | `visibility:` in the header; no field — `owner` |
| secrets | `visibility` on a `secrets.toml` entry; no field — `owner` |

Patterns: `*` — within a folder, `**` — any folders; a matched folder — with all its contents; several matched — the strictest
level.

`ws check` rules (warnings for now, refusal — at export, item 7):
- `visibility` — a member is not marked; a public member depends on a private one;
- `leaks` — in a `public`-level file:
  - a public IP (except loopback, private, documentation ranges and `public-ips-ok`);
  - an owner marker (`owner-markers`: accounts, domain, Ghidra);
  - game data (`game-files`).

`ws roles` (and `--list=<level>`) — a summary. Currently:
- `public` — 462 files; `team` — 2060 (43 notes); `owner` — 71 (15 notes);
- members: 11 public, 11 team, 4 owner (`ops/deploy`, `backup`, `forgejo`, `discord`);
- in DevKit ("Structure") members show a level badge.

17 `leaks` / `visibility` warnings — work for item 7:
- launcher: the prod address and `example.org` are hardcoded in `builtin.rs`, `hosts.rs`, mocks, README;
- backend: the Robot IP in the `auth/registry.rs` test, the domain in `server_toml.rs`, mentions of Ghidra;
- `bootstrap.manifest.toml` points to the owner's repository; in the team edition — to the team repository
  (replacement in `export/team.toml`);
- `local.example.toml` contains the prod IP;
- `game/catalog` depends on the team-level `game/paks`, `game/bp-patches`;
- a stray file `core/backend/src/rpc/handlers — Copy.rs` (a Windows copy, the word "Copy" in Russian).

**Item 5 done 15.11** — `cargo xtask pack designer [--sign] [--allow-dirty]`, package contents — `docs/PACKAGES.md`.
- Studio is built in its own Cargo workspace with tauri-cli from `home\tools`.
- Balance kit — `cargo xtask balance kit` in `core/backend`; the fresh zip is taken from the season folder.
- The signature is shared with `dist`; the trusted comment `dungeonborne-pack-designer <commit>` does not match `ws`/DevKit releases.

Check: the package was unpacked into a clean folder, hashes were verified with PowerShell tools, Studio from the package opened the kit
(`20261115-df305331`, 19 tables).

Found along the way:
- Cyrillic in a file name breaks the `SHA256SUMS` check in PowerShell 5.1 → only Latin names in the package
  (`README.md` instead of a Cyrillic name, the Russian for `INSTRUCTIONS.md`).
- In `home\toolchains\cargo\registry\src` the crates lacked nested `target` folders (for example,
  `cc-1.11.0/src/target/`): when moving the cache in step 4 the copy skipped all folders named `target`. The cache
  was set aside in `registry\src.broken-20261115` (2.8 GB, can be deleted), cargo unpacked everything again. When moving caches
  and `home`, do not exclude `target` folders.

**Item 6 done 15.11** — `cargo ws secrets export [--tier=team] [--out=<file>]`, procedure — `docs/SECRETS.md` (for everyone) and
the owner's document `docs/runbooks/secrets.md` ("Team database"):
- a new database `vault\team.kdbx` with its own password: only entries with `visibility` no higher than `team`; there are no `owner` entries
  in it at all; the source database is not changed, an existing file is not overwritten;
- after writing, the database is reopened and every value is compared with the source (mismatch — the file is deleted);
- a developer has `local.toml [vault] db = 'team.kdbx'`, `tier = 'team'`: `ws secrets check` and DevKit do not look for
  `owner` entries, `ws secrets run <owner entry>` refuses.

Verified: a `ws-core` test (export, refusal on repeat and on `owner`, value comparison, check/run with tier `team`) and
the command on temporary databases. Right now all 13 entries are `owner`, the export is empty: what to give the team is the owner's decision
(see 13.5).

**Item 7 done 15.11** — export, private GitHub (`073aa66`), an outside agent installed the framework following `AGENTS.md`
with an interrupted download, `ws doctor`/`check` 0/0; installation straight from the GitHub address — 4 min, 0/0; update —
`export public --update` (details below and in 13.5). All that is left is to open the repository (the owner's decision).
(`cargo xtask export public [--allow-dirty] [--update=<clone> --message=<text>]`):
- `ws_core::export` + `export/public.toml` (team level — it contains the replaced values themselves):
  - only public-level files tracked by git are exported;
  - `exclude` — cautious exclusions, see below;
  - `filter` — entries with `visibility = "team"/"owner"` are cut out of `tools.lock` (analysis tools, restic);
  - `[[replace]]` — prod addresses and domain, players' Steam IDs (consistently with user_id), machine paths, Robot/Hetzner,
    traces of analysis in comments (`ghidra`, `0x14…` addresses, `research/…`);
  - `export/public/` is laid on top: README, AGENTS, CLAUDE, GLOSSARY, DISCLAIMER, SECURITY, THIRD_PARTY,
    CHANGELOG, LICENSE-MIT/APACHE, its own `workspace.toml` and `secrets.toml`.
- After writing, every line is checked by the `[workspace.rules]` rules as errors (owner markers, IPs, game files,
  machine paths); then in the copy `git init` + `git add` (no commit: the owner chooses the author and the first commit) and
  `ws check` with its own rules. The result — `home\out\export\public-<id>`.
- `ws check` in the private repository checks leaks after the replacements: of 17 warnings, 0 remain.
- Found by four reviews (backend, launcher, mods, tools) beyond the automatic check: real Steam IDs
  (official server capture, Goldberg machine, a player in mocks), a Discord ID in a test, `G:\`/`E:\` paths, Robot/Hetzner,
  exe addresses and "Ghidra" in comments, `dbnative` on addresses from the exe analysis.
- Along the way: the `ws` and launcher tests assemble a "machine path" from parts / use the disk `X:` — otherwise the export check sees
  them as a leak; the cycle `game/ue4ss` ↔ `game/catalog` is broken, `game/catalog` — team (refers to private paks);
  the stray `handlers — Copy.rs` (named in Russian) is deleted.

Verified: in the export copy (367 files) `cargo xtask ci` passes entirely (page, fmt, clippy, tests, `ws check`).
A third-party AI without our context read the export following `AGENTS.md` and ran `bootstrap.ps1` without `-Apply`:
the script is safe (no red flags; only the Microsoft CRT/SDK that cargo-xwin downloads is not pinned by hash).
Per its remarks: in `AGENTS.md` / `README` — `-Repo`, what is needed in advance, scripts with administrator rights,
a contradiction about the signing key; READMEs for `core/backend`, `tools/ws`, `game/ue4ss`, `game/client-mod`; `bootstrap.ps1`
hints without our Forgejo and `ops/machine`; replaced the prod certificate fingerprint (the server can be found by it
via Censys) and the host `Administrator@…`. Left: ~40 references to private `docs/…` in comments (noted in
`AGENTS.md`), a full installation with `-Apply` — after publication (the repository address is needed).

Excluded out of caution (the owner's decision — lift the exclusion in `export/public.toml`):
- `tools/stand`, `ops/machine` — the owner's stand: disk paths, own Steam ID, a task with administrator rights
  that closes the game mutex;
- `game/ue4ss/native` (dbnative) — exe function addresses from the Ghidra analysis;
- probe mods (ClientProbe, DruidProbe, StatWatch, ArenaScan, NameDump, DSTest), JesterSwap (private pak), stock
  UE4SS mods (their license belongs to UE4SS);
- launcher: `dev-app` (a console for our machine), `tools/nettest` (prod test), `PATCH_NOTES.md`, `experiments`;
- Studio `tools/accept` (acceptance on game tables); the old backend `Dockerfile`.

### 13.5 Open questions

**Decided by the owner 15.11** (public edition):
- license — MIT OR Apache-2.0; third-party programs — `THIRD_PARTY.md`;
- where — GitHub, a community organisation: `https://github.com/dungeonborne-community/dungeonborne` (created by
  the owner 15.11, private for now); this address is in the export's `bootstrap.manifest.toml`, README and AGENTS; push — only
  on a "yes" to that action;
- author — neutral (`[export] author` in `export/public.toml`): xtask makes the first commit in the copy as this author with
  a UTC time and writes it into the copy's `git config`;
- `core/backend/src/generated` stays in the export;
- game data: the backend README says only from your own game copy, we do not provide the tool or the pak key, `dump_*.bin` is not
  published;
- the INTL login bypass, the publisher's hosts and the hosts edit in `client-mod` stay;
- secrets for the team — none for now; later — separate test stand keys, not prod ones;
- `bundle/client.zip` is not in git: the launcher is built after `make_client_bundle.py` from your own install, Goldberg
  and UE4SS are not distributed in the repository.

Left:
- ~~MSVC Build Tools require administrator rights~~ — verified 14.11, see "Building without Visual Studio" below.
- ~~First push to GitHub~~ — 15.11 on the owner's "yes": export `public-5301911ce7`, commit `1d73f33` in `main`
  of the private repository. Subsequent exports — as a new commit on top (not force), also on a "yes".
  15.11 second push on a "yes": `073aa66` = export `aac17be` (step-by-step installation). An update is now a single command:
  `cargo xtask export public --update=<clone> --message=<text>` — the clone linked to GitHub:
  `home\out\export\public-5301911ce7`; the export becomes a commit on top (tree compared, neutral author, UTC),
  push — `git -C <clone> push` only on a "yes". A repeated run without changes — "nothing to update".
- ~~Installation with `-Apply` from this address~~ — 15.11: a clean `F:\DBGithubTest`, clone from GitHub (`073aa66`), 4 min,
  `ws bootstrap`/`doctor`/`check` 0/0 (private repository — login via the saved GitHub login).
- ~~References to private `docs/…`~~ — 15.11: in the export they are replaced with "internal notes" (`export/public.toml`).
- 15.11 decided: the installation is driven by the agent step by step, the whole script is the fallback. Done (3c833d9): `bootstrap.ps1
  -Step <machine|rustup|git|clone|toolchain|linker|tools|devkit>`, `-Status` (done/todo and the next command),
  `-Tool`; `cargo xtask setup --only=tools|ours`, `--tool=`; downloads via `.part` and `--retry-all-errors`.
  Verified on a clean folder from the export (a machine with Visual Studio): steps one by one and refusal of an out-of-order step; everything
  at once ~4 min; 3.2 GB on disk, of which 2.1 GB is the build cache `home\cache\target` (deletable), toolchains 0.95 GB,
  tools 0.15 GB; downloaded ~0.3 GB (Rust ~0.12, tailwindcss 0.11, crates 0.04). Next: an installation section in
  `AGENTS.md` for the agent and a check by an outside agent with a network interruption.
- The owner's public keys (`xtask/dist.pub`, the launcher release key in `builtin.rs`) — in the export as is; forks
  install their own.
- DevKit buttons for Forgejo/backups (`localhost:3300`, `ops/backup`) lead nowhere in the public edition.
- Delivering updates to the designer: a new zip or a version check in Studio.
- ~~Artifact store~~ — 17.11 (step 5): Storage Box, subaccounts `sub3` (read-only, team) and `sub4` (write,
  owner), folder `artifacts`, WebDAV. `local.toml [store]`, passwords — `ws secrets run artifact_store` /
  `artifact_store_push`. `ws artifacts push --apply` — 15 artifacts (~62 MB, `store_sha256` in `artifacts.toml`);
  `cargo xtask dist push` — signed ws + DevKit releases and the index `dist-index.txt`; setup takes a release of an earlier
  commit if `tools/ws`, `apps/devkit`, `Cargo.*` have not changed since. Analysis tools (repak, retoc,
  kismet-analyzer, UAssetGUI, FModel) — official GitHub releases, the exes matched the previous copies byte for byte.
  `ws bootstrap --profile=…` remembers the profile in `local.toml [setup] profile`. Verified on `X:\DBDevTest2` as
  a team member (profile dev, team database `team.kdbx` with only `artifact_store`): ws and DevKit — a signed
  release from the store, 15 artifacts from the store, `bootstrap`/`doctor`/`check` 0/0, without `G:` and the owner's home.
