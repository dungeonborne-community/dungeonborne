---
name: dungeonborne-site
description: "Dungeonborne Community site (site/web) — EVERYTHING is in site/DESIGNBOOK.md (design, decisions, rejected, TODO); tools in site/tools; nothing committed"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Site: `<repo>\site\web` — a single page, HTML + Tailwind v4 + plain JS, launcher look and themes.
**Main document — `<repo>\site\DESIGNBOOK.md`**: how to continue, files, design system (theme table),
page and engine structure (pinning, per-section themes, shader), performance, languages, SEO, images, log of
user decisions 11–14.11, rejected ideas ("do not do again"), open tasks, links to other sessions. Read it first.
File details — `web/README.md`, image slots — `web/img/README.md`, check scripts — `web/landing/tools/README.md`
(headless Chrome; check with frames and pixels, not getComputedStyle).

The most important points in brief:
- Preview: `python web/dev_server.py` (no cache!) → :8821; CSS: Tailwind CLI `input.css → styles.css`.
- Name "Dungeonborne Community", domain dungeonborne-community.com (SITE_URL in build_seo.py). Main language English,
  Russian — I18N_RU (i18n.js, classes_ru.js). Shared materials (previews etc.) — English only.
- Section themes in the markup (data-zone+data-theme); shared parts [data-theme-follow] switch theme in 0.6 s OKLCH, --tp is driven by
  JS rAF (CSS colour animation breaks visited links). No scroll-snap. Shader atmos.js (WebGPU, 3 modes).
- Do not touch Discord; do not take Mithril art; do not commit anything without being asked (as of 14.11 nothing is committed);
  `site/fonts-src/` (110 MB of game fonts) is in .gitignore since 14.11 (commit 6ac7dac).
- The old fan site (`site/dungeonborne-site`) is rejected; its `Dungeonborne-Launcher-Setup-1_4_2.exe` is a trojan per
  Defender, do not run or publish it.

**Why:** the site must look like the launcher, be fast and multilingual; many decisions were made by trial — their outcome and
the rejected options are recorded in the design book so they are not repeated.
**How to apply:** before working on the site open DESIGNBOOK.md; after a notable decision — add it (log/rejected).
Related: [[dungeonborne-launcher-ui-style]], [[dungeonborne-launcher-auth]].
