# web/landing — community site

Static site (HTML, Tailwind, JS without frameworks). **Read first: `DESIGNBOOK.md`** — structure, style,
editing and deploy rules.

| Folder | What |
|---|---|
| `web/` | page sources and builders (`build_news.py`, `build_classes.py`, `build_seo.py`), `dev_server.py` |
| `og/` | link preview images (Open Graph) |
| `tools/` | page checks and measurements in the browser (`.mjs` scripts) |
| `svg-drafts/` | illustration drafts |

Locally: `python web/landing/web/dev_server.py 8821` (or the `site` configuration in `.claude/launch.json`).
Build: `python web/build_news.py && python web/build_classes.py && python web/build_seo.py` in `web/landing`.
Publishing the site — owner only.
