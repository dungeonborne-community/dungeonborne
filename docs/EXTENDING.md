# Core and extensions: where new things go

The server, the mods and the data that players run today are **the core**. Many things depend on it, it is checked
against the live game, and a change in it lands in every season at once. So: **add new things next to the core, not
inside it.** Almost everything new fits into a new file or folder plus one or two "registration lines" in the core.

`cargo ws check` prints a note when files of the core are changed (the list — `config/workspace.toml`
`[workspace.core]`). A note is not an error: changing the core is allowed when the task really needs it — keep the
change small and say in the pull request why it was needed.

## What is the core

| Area | Core | Never edit |
|---|---|---|
| Backend (`core/backend/src`) | the hub `rpc/handlers.rs` and `rpc/handlers/common.rs`, the existing domain files in `rpc/handlers/`, `gateway.rs`, `game_data.rs`, `structures.rs`, `db.rs`, `season.rs`, `main.rs`, `lobby/`, `auth/`, `ds/` | `generated/` — the game protocol (generated code) |
| Mods (`game/ue4ss`) | `Mods/DungeonborneRuServer`, `Mods/DungeonborneRuClient` (one big `main.lua` each), `native/dbnative` | stock UE4SS mods (`Mods/shared`, `BPModLoaderMod` …) |
| Data (`content`) | `seasons/base.toml` (flags of the base game), the running season `seasons/playtest`, `balance/tables` | — |
| Kit | `tools/ws`, `xtask`, `apps/devkit`, `config/`, `bootstrap.ps1` | — |

## How to add

### A backend feature

1. New file: `core/backend/src/rpc/handlers/<feature>.rs` for game RPC logic, or `core/backend/src/<feature>.rs` for
   anything else (a background task, an HTTP API, a table). Inside an RPC file: `use super::*;` and
   `impl GatewaySession { pub(super) async fn <rpc>_impl(...) }` — the pattern of the existing domain files
   (`docs/HANDLERS_AGENTS.md`); shared helpers — `rpc/handlers/common.rs` (`load_me`, `save_me`, `push_dirty` …).
2. Registration lines in the core — only these, each with a comment `// ext: <feature>`:

   | What you add | The line(s) in the core |
   |---|---|
   | an RPC handler | `mod <feature>;` and the `handle_<rpc>` forwarder in `rpc/handlers.rs` |
   | a module or background task | `mod <feature>;` (and `tokio::spawn`) in `main.rs` |
   | an HTTP endpoint | one `.route(...)` in the router of `auth/web.rs` or `admin_api.rs` |
   | a GM command (gm/admin accounts) | `mod <feature>;` in `rpc/handlers.rs`, one arm in the `match cmd.as_str()` of `gm_exec` in `rpc/handlers/player_handlers.rs` and its line in the `help` list there |
   | a chat command for every player | `mod <feature>;` in `rpc/handlers.rs` and one arm in the `match` of `chat_command` in `rpc/handlers/social_handlers.rs` (next to `help`, `online`) |
   | a season flag | a field in `Features` / `FeaturesToml` of `season.rs` (+ the line in `over`, `From`) |

   Chat input goes to `chat_command`; what it does not know goes on to `gm_exec`, which answers only gm/admin
   accounts. Before adding a command check it does not exist already (`/help`, `/info`).
3. Tests next to your code (`#[cfg(test)] mod tests` in your file); `cargo test -p dbserver-rs` (`cargo check` does not
   compile tests).
4. Format only your own files: `rustfmt core/backend/src/rpc/handlers/<feature>.rs`. Do not run `cargo fmt` over the
   backend — the existing code is not rustfmt-formatted, and it would rewrite the whole core.

An RPC the game protocol does not have cannot be added (the protocol is generated from the client). A new behaviour
inside an existing flow (login, extraction, death) still needs a call in that domain file — keep it to one line
that calls into your file.

### A mod

1. New folder `game/ue4ss/Mods/<NewMod>/Scripts/main.lua` (+ more `.lua` files loaded with `require`); do not add the
   feature to `DungeonborneRuServer` / `DungeonborneRuClient` — they are frozen (`docs/MODS_DESIGN.md`, §12.2).
2. One `[[item]]` in `game/catalog/catalog.toml` (`id`, `kind = "ue4ss"`, `name`, `source`, `profiles`, and
   `season = "<id>"` when it belongs to one season).
3. `cargo xtask mods check` in `core/backend` (Lua compiles, no banned calls, catalog is consistent).

There is no API between mods yet: a new mod cannot call into RuServer / RuClient.

### A season

1. `cargo xtask season new <id> [--title=…] [--loot]` in `core/backend`: a new folder `content/seasons/<id>/` with
   `season.toml` (empty `[features]` — everything from `base.toml`), an empty `balance/`, `kit.toml`, `README.md`.
2. Put into that folder only what the season changes: flags in `[features]` (`docs/SEASONS_FORMAT.md`), the tables it
   changes in `balance/` (whole tables), its loot. Do not edit `base.toml` or another season.
3. Try it: `cargo xtask season use <id>`, then the balance and loot commands; the backend runs it with `SEASON=<id>`.

A season that needs new behaviour needs a new flag in `season.rs` (a core change — see the backend table above).
Season modules (`plugin = "…"`) are not supported yet.

## Experiments

Trying ideas, throwaway scripts, copies of code to play with — `sandbox/` in the root (`sandbox/README.md`): not in
git, not checked, nothing else may depend on it. What should stay moves into a member the normal way.
