# Agent roles

Roles are Claude Code subagents in `.claude/agents/<role>.md`: each says when to use it, what it may edit, what it
reads, how it checks its work and what it must never do. The rules of `CLAUDE.md` apply to every role.

| Role | Edits | Component | Checks with |
|---|---|---|---|
| `re` | `research/`, `home\re` — what the client does: paks, BP, Lua, CurveTables, native code in Ghidra | research (+ mods) | evidence (asset, address, row) |
| `launcher` | `apps/launcher`, `apps/studio` | launcher, studio | `cargo tauri build`, `cargo test`, mock server |
| `web` | `web/landing` | site | dev server, pages in a browser |
| `mods` | `game/ue4ss`, `dbnative`, `game/catalog`, `game/bp-patches`, `tools/debug` | mods (+ stand) | `cargo xtask mods check`, the stand |
| `balance` | `content/` (tables, seasons, loot, names, match checks) — data only | server (+ studio) | `cargo xtask balance verify` |
| `backend` | `core/backend` | server | `cargo test -p dbserver-rs` |
| `kit` | `tools/ws`, `xtask`, `apps/devkit`, `bootstrap.ps1`, `config/*.toml` | base | `cargo xtask ci` |
| `stand` | `tools/stand`, run logs — checks in the real game | stand | a stand run with logs |
| `design` | `sandbox/<task>/` only — plan, scenarios, mockup, acceptance criteria; reviews the result | — | the human's "yes" on the plan |

Components are the parts of the repository a machine has on disk (`config/components.toml`,
`docs/COMPONENTS_PLAN.md`); a component brings what it needs (studio and site bring server, stand brings server and mods).

## The orchestrator (the main session)

The main session you talk to is the orchestrator. It does not need a role file. Open the session in the
`workspace` folder: only there Claude Code loads the roles (`.claude/agents`) and the environment.

Every role reads before it works (section "Before you start" of each role file): the workspace, its own area in
full where the task touches it, `docs/GAME_DATA.md`. Put "read your area first" into every brief anyway.

### Work modes

Say the mode in the first line of your answer; the human can change it with one word ("quick", "task", "project").

| Mode | When | What you read | The human's "yes" on a plan | Checks |
|---|---|---|---|---|
| **quick** | a text, a style, a number in a table, a small bug in one file | the rules of `CLAUDE.md` and the files you change | no — do it | `cargo ws check` + the member's check |
| **task** | a new feature inside one member | your area in full ("Before you start" of the role) | a plan in words in the chat, no stop | + tests, the browser |
| **project** | several members, or anything people will look at or use (a site, a page, a launcher screen) | discovery by the roles and `design` | **yes** — the plan and the mockup (phases below) | + acceptance by `design` |

A quick edit that touches the core (`ws check` note `core`) or a second member becomes a task: say so and go on in
that mode. For `newcomer` / `designer` (`CLAUDE.md`, "Talking to the human") quick mode is the only one for code-free
edits; anything else goes through a developer.

**A small task in one area** — do it yourself, following that role's file.

**A project** — in phases, and never skip phase 3:

1. **Discovery.** What exists: the members involved (their features and conventions), the data
   (`docs/GAME_DATA.md`: is the reference there, is there a dump — `cargo ws game`, `home/dumps`), what is missing and
   which command gets it. Roles re / balance map the data, `design` the users and similar products.
2. **Plan.** `design` writes the structure, the scenarios, what is reused, a mockup of the key pages and the
   acceptance criteria (`sandbox/<task>/`). You add who builds what, in which order, and the core files that will
   change.
3. **The human's "yes" on the plan.** Show the plan, the mockup screenshots and the open questions; wait for the
   answer. Changes → back to phase 2.
4. **Build.** Give each part to its role (the Agent tool, `subagent_type` = role name) with a brief: the goal, the
   approved plan, the files, what "done" means, "read your area first".
5. **Acceptance.** Every result: the role's check, `cargo ws check`, nothing outside the role's area; then `design`
   reviews against the plan side by side with the existing member; fix, then report.
6. **Report to the human:** what changed, what was verified and how (screenshots), what was not (for example "not
   checked in game").

## Reports to the human

Every report that ends a piece of work (the orchestrator's, and a role's when it talks to the human directly) has
these parts, in this order, in the human's language and level (`CLAUDE.md`, "Talking to the human"):

1. **Result** — one or two sentences: what is now true. For `newcomer` / `designer` in their terms (a page, a number
   in the game), not in files.
2. **See it** — how the human sees it with their own eyes, in one step: the DevKit button ("Work" → "See the result"
   → the name) and the command `cargo ws preview <name> --open`, plus the page to go to. Something without a page
   (a number, a server rule): the one command that shows it, or the place in the game. A report without this part is
   not finished. A new site or app gets its `[preview]` in member.toml as part of the work (`docs/PREVIEW.md`).
3. **What changed** — for `dev` / `owner`: files or members, commits, core files touched and why; for the others:
   what they will see differently.
4. **How it was checked** — the evidence, not the claim: the test or command and its result, a screenshot, a number
   compared with its source. "Checked" without evidence does not count.
5. **Not checked** — what was not run or seen (for example "not tried in the game", "only on desktop width").
6. **Your decision** — only if one is needed: the question, the options, your recommendation and what follows from
   each; one decision at a time for `newcomer`.

A role reporting to the orchestrator uses the same parts in short form, after its "what I read" lines; the
orchestrator checks "See it" itself — runs the preview and opens the page — before it reports to the human.

**Where the final report goes:** also into a file `$DB_HOME/runs/agents/<YYYY-MM-DD-HHMM>-<short-slug>.md` (the
folder `home/runs/agents`; create it). DevKit, page "Work", shows these reports to the human next to the branch,
the changes and `ws check`. A task started from DevKit names its report file in the task file — use that name.

## Core, extensions, sandbox

The existing code is the core (`docs/EXTENDING.md`). Brief a role to put new things into new files and folders —
a new `.rs` file, a new mod folder, a new season folder — with only registration lines in the core, and to say in
the report which core files it changed and why (`ws check` notes them). Throwaway experiments go to `sandbox/`
(`sandbox/README.md`), not into members.

## Partial checkout

A machine may have only some components on disk (`cargo ws components list`; DevKit, "Components" page). Before
handing out work, check that the role's component is there (`cargo ws components which <path>` for a path). Missing —
say which one and why, then `cargo ws components add <name> --apply` (a download from the same repository; `add` never
removes anything), and `cargo ws bootstrap --apply` when the work builds, tests or runs something there. Never recreate a missing folder by hand.
`ws check` does not check what is not on disk, so a change that touches other components is checked on a full
checkout (`cargo ws components set full --apply`) or by the owner.

How a human calls a role directly: "use the mods role to …", or start a session and say "work as the backend role".
Owner-only work (production, deploys, releases, publishing, Discord) has no role here — it stays with the owner.
