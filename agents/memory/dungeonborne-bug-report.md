---
name: dungeonborne-bug-report
description: 14.11 bug reports launcher->backend->Discord bot: contract docs/specs/bug-report-api.md, backend 18ef2c1, launcher f109fc0, game button pak draft; nothing deployed
metadata:
  visibility: team
  type: project
---

On 14.11 the bug report was built. Chain: player → launcher → backend → Discord bot posts to 🐛・bug-reports.

**Contract:** source of truth — `docs/specs/bug-report-api.md` (maintained by the "Discord server via MCP" session), copy — `docs/specs/bug-report-api.md`.

**Backend** (`core/backend/src/bug_reports.rs`, commit 18ef2c1):
- `POST`/`GET /api/launcher/v1/bug_report`;
- limit `PER_HOUR = 3` per hour per player;
- admin API `/admin/v1/bug-reports*` for the bot.

**Launcher** (`apps/launcher/src-tauri/src/bugreport.rs`, commit f109fc0):
- report form and logs up to 9 MB;
- heartbeat `Saved\launcher_alive` and pickup of `Saved\bug_report_request.json`;
- window after a game crash.

**"SUPPORT" button in the game:** draft pak `zz_SupportButton_P` in `<data>\research\support_button`. It wraps `CustomerService:Open` (analysed by LOCAL). Without the launcher it opens `dungeonborne-community.com/#start`.

**Status:** nothing deployed. Checklist — FIX_LIST, "To check: bug report and the "SUPPORT" button".

**Why:** the user wants player reports in Discord without collecting logs by hand.

**How to apply:**
- Deploying the backend, launcher and pak, and enabling the bot on the live server — only on the user's "yes".
- Test on the test Discord Sirius.ai first.
