# Components: install only what you work on

Status: done 18.11 — step 1 (inventory `config/components.toml` + the `components` rule of `ws check`),
step 2 (`ws components`, the `components` field of programs and data, presets), step 3 (checks on a partial tree),
step 4 (`bootstrap.ps1 -Components`), step 5 (DevKit "Components" page), steps 6–7 (roles, CLAUDE.md, install docs), step 8 (trial).

## Goal

The team repository stays one repository with one history, one `main` and pull requests as today
(`docs/CONTRIBUTING.md`). What changes is what lands on a machine: a person or an AI picks components in DevKit (or
on the command line), and only their folders, programs and data appear on disk. Someone who works on the site does not
see the mods, the research and the Unreal projects.

Not a goal: access control. A partial checkout hides folders, it does not protect them — everyone with access to the
repository can fetch everything. If code and game data are ever split into separate repositories, the same
inventory is reused; only the way a component is fetched changes (clone instead of sparse checkout).

## How it works

- A **component** is a set of members (`config/workspace.toml`) with a title, a description, the components it needs
  and later the programs and data it needs. Inventory — `config/components.toml`.
- **Base** is always installed: everything outside members (root files, `config`, `docs`, `agents`, `.claude`) plus
  `tools/ws`, `xtask`, `apps/devkit`.
- The repository is cloned partially: `git clone --filter=blob:none --sparse`, then `git sparse-checkout set` with the
  folders of the chosen components and of everything they need. Adding a component later fetches only its files.
- The choice is kept in `config/local.toml [components]`. `ws bootstrap` installs the programs and data of the chosen
  components only.

## Components (step 1)

| Component | Members | Needs |
|---|---|---|
| base (always) | `tools/ws`, `xtask`, `apps/devkit` + everything outside members | — |
| server | `core/backend`, `content` | — |
| launcher | `apps/launcher` | — |
| studio | `apps/studio` | server |
| site | `web/landing` | server |
| mods | `game/ue4ss`, `game/client-mod`, `game/catalog`, `game/paks`, `game/bp-patches` | server |
| stand | `tools/stand`, `ops/machine`, `tools/probe`, `tools/debug` | server, mods |
| unreal | `game/unreal/jester`, `game/unreal/new-class` | mods, stand |
| research | `research`, `tools/py`, `tools/sniffer` | server |
| owner (owner only) | `ops/deploy`, `ops/backup`, `ops/forgejo`, `ops/discord` | server, stand |

Why these edges (checked 18.11 against the code, not only `depends_on`):

- `core/backend` and `content` are one component: backend tests read `content/seasons`, and the backend's
  `cargo xtask` (balance, season, loot, names, match checks) reads and writes `content/`; `content` in turn is checked
  with that xtask. Split apart, neither works.
- studio → server: the `balance-kit` crate is a path dependency.
- site → server: `build_news.py` and `build_classes.py` read seasons, names and `ExcelConfigjson`.
- mods → server: mods are built and checked by the backend's `cargo xtask mods`.
- stand → server, mods: the stand runs the backend and installs the mods into the game.
- unreal → stand: `build_pak.ps1` and `start_game.ps1` load `tools/stand/paths.ps1`.
- research → server: the scripts (`tools/py` too) read backend data and call its xtask; `tools/py` depends on
  `core/backend`, so it cannot be in the base (the rule caught it).
- launcher's release scripts `tools/make_mods.py` and `make_client_bundle.py` import `tools/py/wspaths.py`: they
  work only with research installed (they also need mods built by the server's xtask). See open questions.

## Steps

1. **Inventory and rule** — done. `config/components.toml`; `ws check` rule `components`: every member belongs to
   exactly one component, needed components exist, no cycles, a member's `depends_on` lies inside its component's
   closure, a component's `visibility` equals its strictest member. Team export cuts owner components (`filter`);
   the public edition does not get the file yet.
2. **`ws components`** — done. `list` (components, file counts, what is on disk, presets); `add`, `remove`, `set`
   with component or preset names or `all` (plan without `--apply`). Sparse checkout in cone mode: the top-level
   folders without members (docs, agents, config, .claude …) and the members of the chosen components with
   everything they need. `remove` refuses when another installed component needs it or when its folders have
   uncommitted changes. The choice — `config/local.toml [components] installed`; `set full` (or `all`) goes back
   to a full checkout. Programs (`tools.lock`) and data (`artifacts.toml`) have a `components` field; an artifact
   at `ws:<path>` follows the component of its member. `ws doctor` / `ws bootstrap` handle only what the installed
   components need. Profiles (owner, dev, designer, agent) stay as they are — they say *who* the machine is for;
   presets (`[[preset]]` in `components.toml`: full, balance, web, modding, re) say *what* is on it.
   Checked 18.11 on a partial clone: `set web` → base, server, site (54 MB of files); `add modding`; `remove`
   refusals; `set balance`; `set full` → full checkout, `ws check` clean.
3. **Checks on a partial tree** — done. `ws check` reads the `member.toml` of a member that is not on disk from git
   (visibility and links stay known); files and folders of components that are not installed are not checked; a
   document link into a path that is in git but not on disk is not checked (one note with the count); an installed
   component, or one it needs, missing from disk is an error with the fix (`ws components add <name> --apply`).
   `cargo xtask ci` builds only the root Cargo workspace (base members), so it runs on any partial tree.
   Checked 18.11: presets web, balance, modding, re — `ws check` 0 errors, 0 warnings; `cargo xtask ci` inside a
   `web` clone — all passed.
4. **`bootstrap.ps1`** — done. `-Components <preset or names>`: the clone step makes a partial clone
   (`--filter=blob:none --sparse`) with the base only (top-level folders without members + the base members, read
   from `config/workspace.toml` and `config/components.toml`), and a new last step `components` runs
   `ws components set <names> --apply` (creates `config/local.toml` from the example when there is none).
   Without `-Components` — a full clone as before (kept as the default, see step 7). Checked 18.11: `-Components web` on a fresh root → base + server + site on disk, `-Status` all done,
   `ws check` 0/0, `ws doctor` lists only the programs of those components. GitHub (team repository, 18.11): partial
   clone 6 s, base on disk 2.4 MB, + server + site 52 MB; full clone 6 s, 71 MB — the repository is small, the gain
   is what lands on disk, not the download.
5. **DevKit, "Components" page** — done. A row per component: checkbox, title, file count, what it needs, the
   external programs its work needs (Visual Studio, Unreal Engine, Ghidra — from `tools.lock`), the state (on disk /
   will be added / will be removed / not on disk); the base and whatever a chosen component needs are checked and
   locked ("needed by …"). Presets as buttons; "Show plan" and "Apply" call `ws components set` (ws-core
   `components::status` and `run`). English and Russian. Checked 18.11 in the browser sandbox (mock data): preset
   balance → studio added, site removed, server "needed by studio"; plan shown; Russian texts. Not checked: the
   built DevKit.exe against a real partial clone (the commands are thin, ws-core is tested).
6. **AI** — done. Every role file names its component (`- **Component:**` under Scope); `agents/ROLES.md` has a
   Component column and a "Partial checkout" section for the orchestrator; `CLAUDE.md` has "Components (partial
   checkout)": say which component a missing folder is in, `cargo ws components add <name> --apply` (only adds) or ask
   the human, never recreate folders by hand.
7. **Documents** — done. `docs/CONTRIBUTING.md` step 2: `-Components <preset>` with the preset table, how to change
   it later; the owner's guide has a section on components. Decided: `bootstrap.ps1` without `-Components` stays a
   full clone — the install doc always names a preset, and a full clone stays the safe default for the owner and
   for old instructions.
8. **Trial** — done 18.11, from the team export of `581ddfc` with its own `bootstrap.ps1` (Rust and programs taken
   from an existing install; clone and components steps as a newcomer runs them):
   - preset `web`: on disk base + server + site; `ws check` 0/0; `ws doctor` 0 errors (warnings only for what step 4
     of CONTRIBUTING brings: home folders, store data, agent env); site build — news, 8 class pages, SEO;
   - preset `balance`: base + server + studio; `ws check` 0/0; Balance Studio builds (`cargo tauri build
     --no-bundle`, 177 s, the balance-kit crate from `core/backend`);
   - `set full` on the `web` install: 2549 files on disk = 2549 in the export, not sparse, `ws check` 0/0;
   - AI newcomer (a fresh agent on the `web` install) asked about a line in a mod that was not on disk: read
     `CLAUDE.md`, found the component, ran `ws components add mods` (plan), then `--apply`, answered with file and
     line; tree clean. From its notes: `ws components which <path>` and the members in `list` were added, and
     `CLAUDE.md` / `ROLES.md` now say to add a component yourself (ask only when the task is unclear) and that
     `ws bootstrap` is needed only to build, test or run.
   Found on the way, not about components: the committed `web/landing/web/news.js` is older than
   `content/seasons/playtest/patch_notes.toml` (no entries for 13–16.11) — rebuild it with `build_news.py`.

Steps 1–3 are the foundation (everything works from the command line), DevKit (5) is a shell on top.

## Open questions

- Is `docs/` whole in the base, or do component documents move next to their members? Today all of `docs/` is
  about 1 MB — kept whole.
- `tools/py/wspaths.py` is the shared python path helper (like `tools/stand/paths.ps1`); moving it into the base
  would let the launcher's release scripts run without research.
- Member `depends_on` is incomplete (for example `core/backend` reads `content`). Inside one component it does not
  matter; it is not fixed in step 1.
