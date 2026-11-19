---
name: dungeonborne-seasons
description: "Seasons since 07.11 — SEASON=<id>, seasons/base.toml + season.toml flags; current season playtest; Dead Money removed; what is base and what is season"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Since 07.11.2026 the backend enables a season only via the variable `SEASON=<id>` (`core/backend/src/season.rs`, format — `docs/SEASONS_FORMAT.md`). Core flags (`auto_sell_extracted`, `relic_lower_quality`, `kill_reward`, `start_materials`) are taken from `seasons/<id>/season.toml`, otherwise from `content/seasons/base.toml`, where everything is off. The current prod season is `playtest`: forge materials on login and lower relics, no auto-sell and no kill reward. Dead Money is removed entirely; `PLAYTEST`, `DEADMONEY*` and `deadmoney.toml` are no longer read.

**Why:** user decision 07.11: kill reward, lower relics, auto-sell, forge materials and the Jester are not the base game but a season. There is no Jester in playtest: he has no season of his own, in the catalog he is `dev = true`. Still undecided: system vendors and the auction (`TRADE_SYSTEM_*`), `match_checks.toml`, red and purple portals — do not change their behaviour.

**How to apply:** a new seasonal mechanic is a flag in `[features]` with the off value in base.toml, not an environment variable. Whether a pak or mod is seasonal is set only by the `season` field in `mods/catalog.toml` (this is the orchestrator's area). Prod needs the `seasons/` directory next to the backend and `SEASON=playtest`, otherwise forge materials and lower relics disappear. Related: [[dungeonborne-match-checks]], [[dungeonborne-prod-playtest]].
