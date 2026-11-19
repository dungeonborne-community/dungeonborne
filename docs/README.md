# docs — index

Documents for people: specs, plans, write-ups. Code and its READMEs live with the members (`config/workspace.toml`), AI
memory lives in `agents/`. A new document gets a line in the matching section below.

## Where to start

| You are… | Read |
|---|---|
| new to the project | root `README.md` → [WORKSPACE_PLAN.md](WORKSPACE_PLAN.md) (how it is laid out) → [PACKAGES.md](PACKAGES.md) (who gets what) |
| a team member: setup, submitting work, tasks | [CONTRIBUTING.md](CONTRIBUTING.md) (setup from scratch, branch, merge request, where to start) |
| looking for secrets | [SECRETS.md](SECRETS.md) (which database who has, `ws secrets`) |
| reverse-engineering the client (skills, items, protocol) | [RESEARCH.md](RESEARCH.md) (paks → BP/Lua → tables; native — `research/ghidra/README.md`) |
| looking for what is broken and what to do | [FIX_LIST.md](FIX_LIST.md), [BACKLOG.md](BACKLOG.md), [WORKAROUNDS.md](WORKAROUNDS.md) |
| editing balance | [BALANCE.md](BALANCE.md), then [BALANCE_APP_SPEC.md](BALANCE_APP_SPEC.md) (Balance Studio) |
| writing backend code | [DS_PIPELINE.md](DS_PIPELINE.md), [REF_BATTLE_ROOM_FLOW.md](REF_BATTLE_ROOM_FLOW.md), [SEASONS_FORMAT.md](SEASONS_FORMAT.md) |
| writing mods | [MODS_DESIGN.md](MODS_DESIGN.md), [MOD_SETTINGS.md](MOD_SETTINGS.md) |
| working on the launcher | [LAUNCHER_SPEC.md](LAUNCHER_SPEC.md) |

## Living lists

- [FIX_LIST.md](FIX_LIST.md) — what is broken and what is fixed, by section.
- [BACKLOG.md](BACKLOG.md) — plans and tasks.
- [WORKAROUNDS.md](WORKAROUNDS.md) — deliberate simplifications ("not as in the original") and why.

## Project layout

- [WORKSPACE_PLAN.md](WORKSPACE_PLAN.md) — the workspace: layout, paths, `ws`, setup from scratch, public edition.
- [PACKAGES.md](PACKAGES.md) — packages by role: what a developer, a designer, an agent gets.
- [COMPONENTS_PLAN.md](COMPONENTS_PLAN.md) — components: install only the parts you work on (`config/components.toml`).
- [GAME_DATA.md](GAME_DATA.md) — game data: your game reference, `ws dump` (icons, monsters, heroes, curves), where each kind of data is.
- [EXTENDING.md](EXTENDING.md) — the core and where new things go (a new `.rs` file, a new mod, a new season); `sandbox/`.
- [PREVIEW.md](PREVIEW.md) — seeing the result: `[preview]` in member.toml, `ws preview`, DevKit "See the result", `.claude/launch.json`.
- [adr/](adr/README.md) — layout decisions (new things in the root, root Cargo).
- [CI_CD_PLAN.md](CI_CD_PLAN.md) — CI/CD draft.

## Backend and protocol

- [DS_PIPELINE.md](DS_PIPELINE.md) — the match: backend ↔ game server ↔ clients.
- [REF_BATTLE_ROOM_FLOW.md](REF_BATTLE_ROOM_FLOW.md) — reference match and battle room flow from a sniff of the original.
- [HANDLERS_STATUS.md](HANDLERS_STATUS.md) — RPC handler status; [LEGACY_AUDIT.md](LEGACY_AUDIT.md) — audit of the old layer.
- [WAITING_ROOM.md](WAITING_ROOM.md), [RECONNECT.md](RECONNECT.md), [PVP_SETTLE.md](PVP_SETTLE.md) — waiting room, reconnect, PVP scoring.
- [SEASONS_FORMAT.md](SEASONS_FORMAT.md) — season format.
- [SERVER_MANIFEST_DRAFT.md](SERVER_MANIFEST_DRAFT.md) ([EN](SERVER_MANIFEST_DRAFT_EN.md)), [BACKEND_FRAMEWORK_PLAN.md](BACKEND_FRAMEWORK_PLAN.md) — server manifest and the "server + mods" model.
- [specs/bug-report-api.md](specs/bug-report-api.md) — bug report API (launcher → backend → bot).

## Launcher and network

- [LAUNCHER_SPEC.md](LAUNCHER_SPEC.md) — launcher spec (login, lobby, mods, updates).
- [RU_INGRESS_PLAN.md](RU_INGRESS_PLAN.md) — ingress node for players behind ISP restrictions (draft).

## Balance, loot, items

- [BALANCE.md](BALANCE.md) — how to edit the tables.
- [BALANCE_APP_SPEC.md](BALANCE_APP_SPEC.md), [BALANCE_APP_ROADMAP.md](BALANCE_APP_ROADMAP.md) — Balance Studio: spec and plan.
- [LOOT_AUDIT.md](LOOT_AUDIT.md) — PVE loot audit.
- [GEAR_LEVEL.md](GEAR_LEVEL.md), [LEGENDARY_ITEMS.md](LEGENDARY_ITEMS.md), [INSURANCE.md](INSURANCE.md) — gear level, legendaries, insurance.

## Mods and client

- [MODS_DESIGN.md](MODS_DESIGN.md) — how mods are built (the target); [MODS_REFACTOR_PLAN.md](MODS_REFACTOR_PLAN.md) — the move towards it.
- [MOD_SETTINGS.md](MOD_SETTINGS.md) — mod settings (INI).
- [CLIENT_PROBE.md](CLIENT_PROBE.md), [STAT_WATCH.md](STAT_WATCH.md) — client probe and match math watcher.
- [CRASH_LONG_PLAY.md](CRASH_LONG_PLAY.md) — crashes during long play sessions.
- [CN_PATCH_IMPORTS.md](CN_PATCH_IMPORTS.md) — what was taken from community patches.

## Game mechanics write-ups

Each one is "how it works in the original and what we did":

- Portals and dungeons: [PORTALS_SONAR.md](PORTALS_SONAR.md), [LEYLINE.md](LEYLINE.md), [LEYLINE_SEALS.md](LEYLINE_SEALS.md),
  [WENDIGO_DOOR.md](WENDIGO_DOOR.md), [SECRET_ROOM_BOSS.md](SECRET_ROOM_BOSS.md), [CASTLE_SPAWN.md](CASTLE_SPAWN.md).
- Modes and quests: [MITHRIL_ORDER.md](MITHRIL_ORDER.md), [QUEST_HOOKS.md](QUEST_HOOKS.md), [CLOTHO_TRIAL_PLAN.md](CLOTHO_TRIAL_PLAN.md).
- UI and visibility: [ITEM_FLAG.md](ITEM_FLAG.md), [GROUND_ITEM_VISUAL.md](GROUND_ITEM_VISUAL.md),
  [ROGUE_STEALTH.md](ROGUE_STEALTH.md), [BATTLE_ID_IN_PING.md](BATTLE_ID_IN_PING.md).

## Outdated (kept for history)

- [FRAMEWORK.md](FRAMEWORK.md) — replaced by [SEASONS_FORMAT.md](SEASONS_FORMAT.md).
- [HANDLERS_AGENTS.md](HANDLERS_AGENTS.md) — brief for parallel agents from 30.10, paths from the old root.
- [EVENT_DEADMONEY.md](EVENT_DEADMONEY.md) — the Dead Money event was removed.
- [fix_listen.md](fix_listen.md) — old note on the listen server crash.

## Owner only

`runbooks/` (prod: domain move, secrets), `discord/`, `ghidra_*` (decompilation), `bench/` (measurements) — visibility
`owner` or working material; team members do not need them.
