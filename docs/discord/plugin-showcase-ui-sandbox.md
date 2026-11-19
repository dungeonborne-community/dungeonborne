<!-- Post in the ✨・plugin-showcase forum. Heading — the "Title" field, tag — UI. Message 1 — post body
     (attachments: ui-sandbox-cover.webp first — gallery cover, then the screenshot and the zip). Message 2 — first reply in the thread.
     The server owner publishes it personally (agents do not touch Discord).
     The original post is bilingual (RU + EN); this copy gives the English text only. -->

## Title (≤100)
🧪 UI Sandbox — the launcher in your browser for your own design

## Tag
UI

## Attachments
1. ui-sandbox-cover.webp (cover)
2. ui-sandbox-screenshot.png (the sandbox in action: the "Windows 11" example, the "Ready to play" scenario, the panel at the bottom left)
3. dungeonborne-ui-sandbox-20261104-dc26cebe.zip (77 KB, next to this file; rebuilt from `launcher/dist/ui-sandbox/`)

## Message 1 (post body)
**Author:** <author>

**UI Sandbox** — the launcher running in a plain browser, so you can design your own launcher UI without Rust, the game or a server. The sandbox fakes the launcher with mock data: servers, Discord login, mod downloads, launch steps, errors, updates — everything answers like the real one.
**Scenarios:** ready to play · first launch · login · not in the guild · native code · port in use · game running · developer mode — switch from the panel or `?scenario=…`
**Panel:** end game, ban mid-session, offer update, log line. Collapsible, movable to another corner, `?panel=hidden` for screenshots.
**Inside:** current UI · from-scratch Windows 11 example (in the screenshot) · `api.d.ts` contract · README with rules
**Download:** zip below 👇

## Message 2 (first reply in the thread)
**🚀 Quick start**
1. Unzip, open the `ui/` folder (or `examples/windows11/`)
2. `tailwindcss -i input.css -o styles.css --watch` — [Tailwind CLI v4](https://tailwindcss.com/blog/standalone-cli), a single exe
3. `python -m http.server 8811` → http://127.0.0.1:8811/index.html?scenario=ready
Not updating — Ctrl+F5

**🧩 Any stack:** the sandbox replaces `window.__TAURI__`, so it works with any page — include `dev/mock-tauri.js` as the first script. React/Vue are possible, but ask before you start: the launcher is built without Node.js.

**📏 Rules**
• nothing external: no CDN and no Google Fonts
• no `style="…"` or `onclick="…"` in HTML
• all states from the README stay; texts go in `i18n.js` in RU and EN

**📤 Submit:** a zip of the folder + screenshots of all scenarios in RU/EN as a reply in this thread. By submitting, you agree to the MIT license.
