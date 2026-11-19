---
name: dungeonborne-match-checks
description: "Search checks as in the original since 08.11 (rarity limit EPIC+ on Hard, rooms without mixing); auto-sell — a season flag; require_guild kept"
metadata:
  visibility: team
  node_type: memory
  type: project
---

**08.11.2026 user decision — "as in the original":** in `backups\match_checks.toml` the checks `gear_rarity`, `gear_score`, `level_cost` are `enabled = true` again (commit 6d60448). Of these, the original `competition_level` has only the rarity limit: Hard 40003019/40003020 — all slots EPIC/EXCEPTIONAL/LEGENDARY/UNIQUE; the original has no minimum gear score and no entry price. Season playtest: `mixed_party_rooms = false` — group size equals room size (solo — solo, duo — duo, trio — trio). Exceptions as in the original (commit 0cf1935): Hard 40003019/40003020 — "skirmish mode", all groups into rooms of three; Challenger Mode ("challenge mode", `level_challenge`) — solo and duo into rooms of three (`matchmaking::queue_size`).

On 07.11 it was the other way round: checks off (the rarity limit blocked search on lobby maps), mixed rooms on. Auto-sell of loot on exit — season flag `auto_sell_extracted` (off in playtest) [[dungeonborne-seasons]]; the whitelist role is off (`login_roles = []`), `require_guild = true` kept by the user — do not change without their word.

**Why:** the user wants the original behaviour; downside — with grey starting gear you cannot enter Hard until epic gear is worn in every slot.

**How to apply:** client checks come from `competition_level` (after an edit: `cargo xtask balance build` and the `ru.balance` mod); the server section and the season are read at backend start. Check the "Start" button with a live client (the `native` autoclient bypasses the checks). Deploy status — `docs/FIX_LIST.md`. Do not touch Discord [[dungeonborne-no-discord]].
