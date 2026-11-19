---
name: dungeonborne-balance-app
description: "Tauri app for external balance designers (Balance Studio, working title); spec in docs/BALANCE_APP_SPEC.md, decisions from 2026-11-06"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Balance app for **external** designers (user decision 06.11). Spec: `docs/BALANCE_APP_SPEC.md`. Release 1.2.0 (stages 1–2 of the `docs/BALANCE_APP_ROADMAP.md` plan; loot: `lt_*` tables from `loot.toml`; control raid not done) was built 06.11 in `<data>\balance_studio_release`, acceptance `balance-studio/tools/accept/run_all.ps1`. The build has not been shown to designers yet: the user decided to finish M3 and M4 first; their feedback may change the plans. Code: `<data>alance-studio`, logic in `backups/crates/balance-kit`; build and sandbox — see its README, from PowerShell with `NO_COLOR=true` (otherwise trunk fails).

**Why:** the Excel + Python + LLM kit is not enough: designers need entry without Python, to see what an edit means, and edits must not get lost with a new kit. No concrete complaints from designers yet.

**How to apply:** the app only prepares a `dbbalance-patch/1` patch from the kit; the owner applies it via `cargo xtask balance patch`. v1 has no network, owner mode later. Same stack as the launcher, see [[dungeonborne-launcher-ui-style]]. Do not suggest Discord as a way to send the patch, see [[dungeonborne-no-discord]].
