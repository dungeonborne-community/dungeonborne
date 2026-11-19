# Packages by role: who gets what

The from-scratch build plan is `docs/WORKSPACE_PLAN.md`, section 13. This is the inventory: what from the workspace, `home` and `vault` goes
into which package. Compiled on 14.11 as of workspace `19a9567`.

## Roles

| Role | How they get it | git | Secrets | Agent prompts and memory |
|---|---|---|---|---|
| **Owner** | `bootstrap.ps1 -Root <folder> -Apply` (clone from Forgejo) → `cargo ws bootstrap --profile=owner --apply` + Storage Box | yes | the whole database (12 entries) | everything: `CLAUDE.md`, `agents/memory`, briefs |
| **Team** (developer) | team edition (`cargo xtask export team`) in the team repository: `bootstrap.ps1 -Root <folder> -Apply` → `cargo ws bootstrap --profile=dev --apply`; submitting work — `docs/CONTRIBUTING.md` | yes | a separate subset database, without prod | `CLAUDE.md` + memory marked `team` |
| **Designer** | a ready package from the owner (zip with a signature), no git and no script | no | none — Studio works offline | the "balance assistant" prompt + `BALANCE.md` |
| **Community** (public edition) | `bootstrap.ps1 -Root <folder> -Apply` → public repository | yes | their own empty database (`ws secrets init`) | the framework's shared `CLAUDE.md` + `AGENTS.md` |
| **Agent / builder** | `ws.exe` in a ready workspace | yes | `ws secrets run` (env) for the duration of the command | `CLAUDE.md` |

Only the owner gets the prod secrets. The designer package and the team package do not have them: a designer's laptop must not give
access to the VM, Discord and the launcher signing. Prompts and data are given to the designer ready to use — they do not need to configure anything.

## Workspace inventory

Markup in the repository (since 15.11): `member.toml [member] visibility`, `[visibility] team/owner` for parts of a member,
`config/workspace.toml [workspace.visibility]` for files outside members, the memory note header; `ws roles` — summary.

Legend: **P** — public edition, **T** — team, **O** — owner only, **D** — designer package,
**game** — derived from game files: not published, in the public edition it is built on the person's machine from their copy of the game.

| What | Roles | Note |
|---|---|---|
| `config/workspace.toml`, `config/tools.lock`, `config/local.example.toml` | P T O | in P — without O members (the package builds its own copy) |
| `config/artifacts.toml` | T O | in P — its own list without game data |
| `config/secrets.toml` (inventory) | P T O | in P — the framework's shared inventory, without prod addresses and paths; entries have `visibility` (currently all `owner`) |
| `CLAUDE.md` | T O | in P — a separate framework `CLAUDE.md` |
| `core/backend`: `src`, `xtask`, `ds-agent`, `crates`, `server.example.toml`, `Dockerfile` | P T O | framework core |
| `core/backend`: `core/backend/assets/excel`, `ExcelConfig`, `ExcelConfigjson`, `Configure.pb`, `Network.pb` | T O, **game** | in P — `xtask` builds them from one's own copy of the game (open question: the pak key is needed) |
| `content/balance` (`tables`, `csv`, `kit`) | T O, D (only `kit`) | tables — **game** |
| `content/seasons`, `loot`, `names`, `match_checks.toml` | T O | in P — one season as a format example |
| `apps/devkit`, `apps/studio` | P T O | Studio — also in D (exe) |
| `apps/launcher` | P T O | for P, move the addresses, the Discord application id and the relay key into settings |
| `web/landing` | T O | our community's site; does not go into P |
| `game/ue4ss` (Lua, `dbnative` sources), `game/catalog`, `game/client-mod` | P T O | |
| `game/bp-patches`, `game/paks/legacy`, `game/unreal/*` | T O, **game** | modified game assets |
| `game/paks/balance` | T O | built by `xtask balance` |
| `tools/ws`, `tools/stand`, `tools/py` | P T O | `py` — check file by file |
| `tools/probe`, `tools/debug`, `tools/sniffer` | T O | research tools |
| `ops/machine` | P T O | Windows variables, WSL |
| `ops/backup`, `ops/forgejo` | O | in P — later, as a sample without our credentials |
| `ops/deploy`, `ops/discord` | O | prod addresses, our Discord |
| `research/` (incl. `rpc-evidence`) | T O | protocol recordings and analyses |
| `docs/ghidra_*` | O | decompilation of the game exe — never goes out |
| `docs/discord`, `docs/bench` | O / T | |
| `docs/FIX_LIST.md`, `BACKLOG.md`, plans | T O | |
| `docs/FRAMEWORK.md`, `MODS_DESIGN.md`, `BALANCE.md`, `LAUNCHER_SPEC.md`, `adr/`, `specs/` | P T O | in P — after proofreading for addresses and names |
| `agents/memory` | O, T (subset) | notes have a `visibility` field in the header (`owner` or `team`, missing — `owner`); does not go into P |
| `agents/briefs`, `.claude/` | T O | |

## home and vault inventory

| What | Roles | Where it comes from on a new machine |
|---|---|---|
| `home/toolchains`, `tools` | P T O (per `config/tools.lock`) | official sites, sha256 |
| `home/artifacts` | T O | Storage Box (restic); in P — built locally |
| `home/re`, `home/unreal` | O, T (by decision) | Storage Box |
| `home/game/original` | everyone who has the game | their own copy from Steam (`download_depot`) |
| `home/dumps`, `cache`, `runs`, game copies | — | created locally |
| `vault/dungeonborne.kdbx` | O | Storage Box + emergency kit (section 13) |
| subset database `team.kdbx` | T | `ws secrets export` (entries with `visibility = "team"`), password via a separate channel; on the developer's side `config/local.toml [vault] db`, `tier = 'team'` |

## Designer package

Built by `cargo xtask pack designer --sign` (key — `cargo ws secrets run dist_signing_key -- …`) into
`home\out\pack\DungeonborneBalance-<Studio version>-<commit>\` with a `.zip` next to it:

```
BalanceStudio-<version>.exe       the application (offline, no secrets)
<season>-balance-<version>.zip    balance kit of the active season (cargo xtask balance kit); inside, llm/PROMPT.md —
                                  a prompt about the tables for an AI
README.md                         designer instructions (apps/studio/docs/GUIDE.md)
CHANGELOG.md                      Studio versions
AGENTS.md                         for the designer's AI: what is in the package, how to check the signature, where the prompt is, what is not allowed
signing-key.pub                   public key (2EEA90212DBCA703); verify with the owner via another channel
SHA256SUMS, SHA256SUMS.minisig    hashes of all files, signature "dungeonborne-pack-designer <commit>"
```

File names are in Latin characters: Windows PowerShell 5.1 reads `SHA256SUMS` as ANSI and breaks Cyrillic.

The way back is a patch file `dbbalance-patch/1`, the owner applies it with `cargo xtask balance patch`. A new season — a new
zip; later Studio will check for a new kit version itself.

## Public edition: what to add

Written for an AI agent without context (section 13.0): the person only gives their AI a link.

`README.md` (one screen, for a human), `AGENTS.md` (verification and install order), `GLOSSARY.md` (backend,
battle server, UE4SS, pak, season, balance), a `README` for each member for the agent, `LICENSE`, `THIRD_PARTY.md`,
`SECURITY.md`, `DISCLAIMER.md` (fan project, not affiliated with the developer or publisher; the package contains no game files),
`CHANGELOG.md`, `rust-toolchain.toml`, root `Cargo.toml` + `Cargo.lock`, `xtask/`, `bootstrap.ps1` +
`config/bootstrap.manifest.toml`. There are no prebuilt exes — `ws` and DevKit are built locally (section 13.1).

Exported by `cargo xtask export public` from the private repository, one way, without history (section 13.3).
Edition spec — `export/public.toml` (exclusions, cutting entries from `config/tools.lock`, string replacements), files on top —
`export/public/`; the result — `home\out\export\public-<commit>` (a git repository without a commit). What is excluded out of
caution and what the owner decides before publishing — `docs/WORKSPACE_PLAN.md`, items 7 and 13.5.
