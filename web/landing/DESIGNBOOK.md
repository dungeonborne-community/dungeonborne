# Dungeonborne Community site — design book and work log

A single document for continuing the work without the chat history: what the site is, how it is built, the design system,
all of the user's decisions (with dates), what was tried and rejected, performance rules, open tasks.
Per-file details — `web/README.md`, image slots — `web/img/README.md`, tools — `tools/README.md`.
Update this file on every notable decision.

---

## 0. How to continue (quick)

- Site code: `<repo>\site\web` (deployed as a whole, except `downloads/` in git).
- Preview: `python web/dev_server.py` → http://127.0.0.1:8821/ (no cache — with `python -m http.server` the tab took
  old `styles.css`/`site.js` from the cache, and the user saw the old version). In the Claude app: `.claude/launch.json` → `site`.
- Styles: edit `web/input.css` → build:
  `"/c/Program Files (x86)/Tailwind/tailwindcss.exe" -i input.css -o styles.css --minify` (from `web/`).
- Builds (from `web/`): `python -X utf8 build_classes.py` (class showcase), `python build_news.py` (news),
  `python update_release.py <dist/launcher-X>` (new exe) → then `python build_seo.py` (JSON-LD, links, robots, sitemap),
  `python -X utf8 build_local_zip.py` (archive for viewing from disk → `site/dungeonborne-site-local.zip`).
- Link previews and icon: `node site/og/render.mjs` (og.jpg), `python site/og/favicon.py` + `node site/og/render.mjs icon`.
- Checks: `site/tools/` (see its README). Always look at frames/pixels, not only at styles.
- Do not commit or push anything unless the user asks. As of 14.11 **nothing is committed**.
- General project rules (memory): do not touch Discord (bot/server/posts), do not touch disk D:, do not take Mithril
  art/logo/code, do not send the user's personal data outside, pak decryption — only after the user's "yes".

---

## 1. What it is and principles

- Landing page of the **unofficial community server** of Dungeonborne (the game was officially shut down): download the launcher, sign in
  with Discord, play again. Name as the Discord server's: **Dungeonborne Community**. Domain (chosen 13.11, ~$10):
  **dungeonborne-community.com** (`SITE_URL` in `build_seo.py`; not yet bought/connected at the time of writing — to be confirmed).
- **Looks like the launcher** (`<data>\launcher\ui`): the same components, themes, font. The launcher windows on the page
  are built from HTML components (not screenshots), the labels are the launcher's strings word for word.
- **Stack:** one page, HTML + Tailwind v4 (CLI) + plain JS, no frameworks (the user's decision for the launcher and the site;
  Leptos was discussed on 14.11 — not needed: static text, WASM would add weight and SEO problems). Python — builds only.
- **Languages:** English is the main one (in the markup), Russian — via a dictionary. The community is multilingual: shared materials (previews,
  banners, images with text) — **English only**.
- **Nothing borrowed:** no Mithril art/logo; images are placeholder slots, the user fills them in.
- Without scripts the whole page reads fine (in English); everything animated is an enhancement on top.

---

## 2. Files

| Path | What |
|---|---|
| `web/index.html` | all markup (English), blocks `classes:start…end` (built by build_classes.py) and `seo:start…end` (build_seo.py) |
| `web/input.css` → `styles.css` | `@theme` tokens, `[data-theme]` themes, all utilities; almost no bare CSS (themes, @property, reset) |
| `web/fonts.css` | "Dungeonborne Serif": Libre Baskerville (Latin, renamed per the OFL) + Noto Serif (Cyrillic) |
| `web/site.js` | all behaviour (sections below), one IIFE |
| `web/atmos.js` | WebGPU background shader (3 modes by theme) |
| `web/i18n.js`, `web/classes_ru.js` | Russian texts (`window.I18N_RU`) |
| `web/i18n_swap.py` | text replacement by `data-t` keys (main language ↔ dictionary) |
| `web/build_classes.py` | class showcase from the game tables (`ExcelConfigjson`, `names/strings*.tsv`) |
| `web/build_news.py`, `news.js` | news from `seasons/<active>/patch_notes.toml` |
| `web/build_seo.py`, `robots.txt`, `sitemap.xml` | search engines and social networks |
| `web/update_release.py` | new launcher build: version, size, sha256 in the markup |
| `web/build_local_zip.py` | archive for viewing from disk (fonts embedded in the CSS, since file:// does not load woff2) |
| `web/dev_server.py` | preview server without cache |
| `web/_headers` | CSP (`script-src 'self'; style-src 'self'` — no inline styles or scripts), caching, for Cloudflare Pages |
| `web/img/README.md` | all image and video slots with sizes |
| `og/` | `og.html` + `render.mjs` (og.jpg preview, icons), `favicon.py` |
| `tools/` | check scripts in headless Chrome |
| `fonts-src/` | game fonts for all languages (110 MB, unpacked by the LOCAL session on 13.11; in .gitignore since 14.11, commit 6ac7dac) |

---

## 3. Design system

### Themes (from the launcher, `launcher/ui/input.css`; "Torch" is the default in `@theme` and also a `[data-theme="torch"]` block)

| Token | Torch | Crypt | Blood |
|---|---|---|---|
| mica | #0b0907 | #07090c | #0a0606 |
| layer | #14110d | #0d1116 | #130c0c |
| card | #1c1813 | #131920 | #1b1111 |
| card-hi | #27211a | #1b232c | #251717 |
| stone | #322a21 | #232d38 | #301d1d |
| line | #3b3127 | #2a3542 | #3d2424 |
| line-hi | #6b5638 | #4d6276 | #7a3a35 |
| fg | #efe6d6 | #e4ebf2 | #f1e4dc |
| fg-2 | #d4c9b6 | #c1cbd6 | #d9c5bc |
| fg-3 | #afa392 | #9ba7b2 | #b39f97 |
| fg-4 | #7c7264 | #6a7580 | #7f6e68 |
| accent | #e0a84a | #8cc8dc | #c9413a |
| accent-strong | #9b6624 | #3f7f96 | #7e211d |
| accent-text | #f6cd7a | #c3ebf7 | #ef7466 |
| accent-ink | #1d1206 | #061218 | #fff3ee |
| arcane | #72b8cc | #b49ae6 | #d6ad5c |
| arcane-strong | #3f7f96 | #6b55a0 | #8a6a2a |
| glow (triplet, atmos.js only) | 224 168 74 | 140 200 220 | 201 65 58 |
| glow-c (colour, for CSS) | rgb(224 168 74) | rgb(140 200 220) | rgb(201 65 58) |
| art-a / art-b | #3a2412 / #120c08 | #10202c / #06090c | #3a1010 / #0d0505 |
| ember | #ff9a3c | #9fd8ff | #ff5a3c |

- **fg-2/3/4 are lighter than the launcher's** (14.11, "make the fonts lighter": 40/35/20 % of the main fg mixed in). The launcher took
  the same values (its session, commit 04f6ba1).
- **Glow in CSS — only `--glow-c`** via `color-mix(in srgb, var(--glow-c) X%, transparent)`, not `rgb(var(--glow)/a)`:
  otherwise on a theme change the glow went through RGB via whitish grey.
- Descriptions and body text — `text-fg-2` (not fg-3; fg-3 is for small captions).
- Portal colours (blue/red/purple) and states (ok/caution/critical) do not depend on the theme.

### Font and sizes
- "Dungeonborne Serif" everywhere (like the launcher). On 14.11 we tried "as in the game" (headings Libre Baskerville + Source Han Serif CN
  Bold for Cyrillic, text Noto Sans) — the user chose **"as it is now"**; the test was removed. Breakdown of the game fonts by language —
  `fonts-src/README.md` (LOCAL session): game headings = Libre Baskerville → digits Noto Serif → Hangul Noto Serif KR →
  fallback Source Han Serif CN Bold; text = Noto Sans (+KR/SC/TC/JP/Arabic/Emoji); timers — Orbitron.
- Licences (checked against the name table): OFL 1.1 — all Noto, Libre Baskerville, Source Han Serif, Zen Old Mincho, Orbitron,
  Lato; Apache 2.0 — Roboto, Droid Sans Fallback. Reserved names: Libre Baskerville, Source, Orbitron, Lato
  (rename when subsetting). Commercial FZDBSK-GBK, STZHONGS, DroidSansMono — do not use.
- Text tokens: `text-3xs/2xs/label/copy/hero`, letter spacing `tracking-caps/brand`, shadows `shadow-glow/lift/header`,
  `aspect-art` 16/10, `ease-glide`.

### Components (`@utility` utilities in input.css)
From the launcher (change there → carry over here): `btn`, `btn-accent` (gold metal), `btn-play` (large, with
`embers` sparks), `btn-std` (stone), `btn-flat`, `card`, `framed` (gold corners), `row`, `bar`, `pill`, `title-rule`, `tab`,
`scene`, `hero-art`. Site differences: `btn-flat` with `aria-current`, `row` items-start, no :enabled.
Own: `slot` (image slot + skeleton), `crest`/`class-crest` (diamond crest), `step-no`, `led`, `code`, `expander`,
`window-shot` (launcher window on the first screen), `demo-window` (in the "Launcher" section), `story/chapter*/notes`,
`pin*`, `story-stage/story-mark/chapter-subs`, `feature/step-row/feature-text`, `class-*/skill-*/talent-chip`,
`shots*` (carousel), `scroll-rail/rail-*`, `sbar*`, `boot*`, `atmos*`, `rays`, `brand-sub`, `scroll-progress`, `reveal*`, `lift`.

### Logo and icon
- In the header: a "D" tile + "UNGEONBORNE" (the tile is the first letter), for screen readers — "Dungeonborne" (sr-only). Next to it
  `brand-sub` "Community server" in small caps in the accent colour with a glow.
- Icon (favicon.svg, apple-touch-icon.png 180) = the "D" tile as in the splash screen; the letter is an outline from the site font.
- Link preview `img/og.jpg` 1200×630: "DUNGEONBORNE COMMUNITY", "Community server — download the launcher and play
  again", the address. English only (decision of 14.11). JPG, not WebP (Discord/Telegram/VK).

---

## 4. The page (top to bottom)

| Section | Theme (data-zone) | How it works |
|---|---|---|
| Header | follows the section's theme | logo, menu, language, "Download"; scroll progress bar at the bottom |
| First screen | torch | exactly the window height; heading, button with sparks, launcher window (floats, tilts after the mouse). The second step "This is the launcher" was **removed** (14.11) |
| About the game: prologue | torch | exactly the window height (on lg); text + **screenshot carousel** (6 slots `img/shots/`) |
| About the game: chapters I–VI | I–II torch, III–IV crypt, V–VI blood | pinned scene; chapters have **sub-steps** (`data-sub`: notes, 3 portals, 2 outcomes) — each its own half-screen step, the item is highlighted, the chapter image switches to the item's image (`img/story/N-K-*.webp`) |
| Classes | crypt | pinned for 1 step; crest — class, skills/talents — **click only** (14.11); skill video on top, the description fades on hovering the clip |
| Epilogue "The torches still burn" | torch | pinned (2 steps: `ep`, `ep-hold`) |
| Getting started | torch | 4 steps, pinned, images `img/start/` |
| Launcher | torch | 6 features, launcher window with a highlighted part (data-hl), the window turns after the mouse, **"Download" button** under the list |
| News | torch | plain feed (news.js) |
| Help | torch | expanders; #sha opens from the "Verify the file" link |
| Footer | — | "Community" row: Discord · Dungeonborne Community, YouTube, Telegram, VK, Reddit (placeholders `href="#"`, addresses — `LINKS` in build_seo.py) |

On the right: a column of dots (only the 6 sections, no sub-items; from xl). Own slide-out scrollbar. "D" splash screen until the font loads.

### Pinning engine (site.js "section pinning")
- `[data-pin]` → `.pin-stage` (sticky scene) + `.pin-spacers` (steps are created by JS, one per screen; a `data-sub` item — 50svh,
  the last step of a section is always a full screen). Steps — `[data-step]` (the id moves onto the step) or `data-pin-steps="a b"`.
- On a step: `data-at` on the section, `data-active` on the scene, `data-lit` on `[data-hl~=key]`; `.pin-swap` — swappable images;
  `.story-mark` diamonds — per scene (not per item).
- Turns on from 1024×720 and if the scene fits (otherwise a plain feed). All sections are measured **at once** (pinApply), not one by one.
- **No CSS scroll-snap** (removed 13.11: it pulled to every step). Instead — **stop at the screen edge** (site.js, block
  "stop at the screen edge", 14.11): one flick of the wheel / touchpad or the inertia of a drag does not skip past a screen edge —
  it reaches the edge, the rest of that flick is swallowed (flick = wheel events without a pause > 220 ms). **Edge resistance**:
  further in the same direction — only after scrolling another `PUSH` = 500 px of wheel with new flicks (≈ 5 clicks); a pause > 1.2 s
  (`FORGET`) resets the accumulated amount; backwards — immediately, no resistance. Drag inertia simply stops at the edge.
  Edges (`screenEdges`): top, prologue, start and end of each pinned section, "News", "Help", bottom (minus the header
  56). Nothing pulls while the scroll is at rest. Inside screens and pins — normal scrolling. Keys and the scrollbar thumb — no
  stops. Only while pinning works (`fits`). Check: `tools/edgestop.mjs`.

### Themes along the page
- The section theme is **written in the markup** (`data-zone="X" data-theme="X"`): sections are not recoloured.
- Only the shared parts `[data-theme-follow]` change theme: header, dots on the right, chapter thread and diamonds, `.atmos` background, scrollbar.
- Change: **0.6 s ease-out**, colours `data-from → data-to` are mixed with `color-mix(in oklch, …)` by the progress `--tp`;
  `--tp` is driven **by JS via requestAnimationFrame** (not a CSS animation!). The text on the accent button (`accent-ink`) flips
  in the middle. A new current chapter gets the new theme at once. During the change the shared parts have colour transitions off
  (`[data-switching]`), but `opacity, visibility, scale, translate, rotate, transform` stay smooth (otherwise chapters at the theme boundary
  vanished with a jerk).
- Background — three `.atmos-layer` layers (one per theme), switched by opacity over 1 s; the shader blends the modes itself.

### Other behaviour
- Chapter change and class change: the old scene dissolves (opacity+visibility 700 ms), the new one's text rises (translate 24→0),
  the image/video fades in from a slight zoom (scale 1.03→1).
- Carousel: advances by itself every 5.5 s (if visible, the mouse is not over it, the window is active, no reduced motion), arrows, diamond dots, ←/→,
  sideways drag.
- Scrolling by dragging with the mouse (like a finger) with inertia; not on links/buttons/video/code/carousel/scrollbar; a click after
  a drag is swallowed. Text is not selectable (`user-select: none`), code and paths are.
- Slide-out scrollbar (`.sbar`): mouse only; visible while scrolling (+1.2 s) and near the right edge; the thumb is always thin
  and muted (14.11 "always in the unhover style"); draggable, a click on the track jumps. The system one is hidden (`html[data-sbar]`).
- Splash screen `<html data-boot>`: a "D" tile with a pulse and a bar until the font loads (≤1.2 s), then it dissolves and
  is switched off (hidden — otherwise the animations would keep running invisibly). Without JS it leaves by itself after 2.5 s.
- Decoration pause: `<html data-calm>` (window not focused and the mouse outside the page — DevTools, another window) — sparks, floating, shader
  stop; `[data-offscreen]` on the first screen and "Launcher" — their sparks and floating stop while not visible.
- Language change by the button: the text part (`main`, `footer`, `header > .page`, dots) fades out in 0.15 s (`<html data-lang-fade>`),
  the text changes while invisible, fades in over 0.25 s; the background and the header backdrop stay. ~0.55 s in total. On load — instantly.
- "Reduce motion" (Windows: Animation effects off): a still frame of the shader, no theme transitions, no inertia,
  the language changes without dissolving.
- The "▶ Animations / Анимации" button in the header (14.11) — visible only if animation effects are off in Windows (or it is already
  turned on on the site): `<html data-motion="on">` + localStorage `motion`. The Tailwind variants `motion-safe`/`motion-reduce`/
  `animated` are overridden via `@custom-variant` (they respect data-motion), and `window.matchMedia` is wrapped in site.js: queries
  about prefers-reduced-motion answer as if motion were on (atmos.js and every check use it). On click
  the "change" listeners get an event — shader, carousel etc. turn on at once. New motion-related code: only via
  matchMedia and these variants, not via a bare `@media (prefers-reduced-motion)` (or with `:not([data-motion="on"])`).

---

## 5. Background shader (web/atmos.js, WebGPU)

- One fragment shader for the whole screen, **half resolution** (SCALE 0.5), 30 fps at rest, every monitor frame while
  the page is moving or the modes are blending. Starts after `load` in idle time (requestIdleCallback).
- Modes by theme (weights are blended): **Torch** — rays (a fan from a point above the screen on the left, swaying ±2.5°, "breathing") and dust in
  the rays; **Crypt** — fireflies (wander, blink) + cold light from above; **Blood** — low fog (fbm) + light pillars from
  items as in the game (blue/purple/gold by rarity, a spot on the floor, shimmer; tied to the page, parallax 0.45).
- The colours of the three themes are read **once** from `.atmos-layer[data-theme=X]`; the mode — from `.atmos[data-theme]`; the scroll position —
  from the scroll event (curY), **do not read scrollY in a frame**.
- Cost: < 0.15 ms per frame (960×540, RTX 3080 Ti). No WebGPU → static CSS rays `.rays`.
- The user likes the "sharpness" (core + soft halo, half resolution, background grain) (14.11) — do not blur it.
- The launcher keeps its own copy (theme on `<html>`, scrolling of the main panel); when the shader changes — tell the launcher session.

---

## 6. Performance (measurements and rules)

- Page weight ~162 KB gzip in total (HTML 26 KB gzip / 20 KB brotli; fonts 81 KB). JS heap ~1.3 MB. No need to optimise the weight.
- Loading (local, warm cache): DCL ~0.3 s, load ~0.4 s, LCP ~1.5 s (was 1.1 / 2.1 / 2.7).
- Idle: unfocused ~10 % of a core (was 90 %), middle of the page ~8 % without the shader (headless Chrome overstates).
- We do not follow the 14 KB rule (it would mean moving the class showcase out of the HTML — losing SEO). Instead: Brotli, HTTP/3,
  Early Hints on Cloudflare, stripping comments on deploy (−2 KB).
- **Do not:** rotate SVG; `blur`/`mix-blend` on animated layers; `backdrop-blur` over the shader (header, captions);
  `background-attachment: fixed`; theme on `<html>` (style recalculation of the whole page 50–80 ms); read `scrollY` in a frame or
  in the middle of startup; measure pinned sections one by one; infinite CSS animations on invisible things (skeleton of far images —
  only with `data-near`, the splash screen — switch off); CSS animation of colours (visited links are not repainted).
- Hidden class scenes after fading — `content-visibility: hidden` (`data-skip`), sizes remembered
  (`contain-intrinsic-size: auto none`), re-remembered after the font and after a language change.
- Class clips load only when the showcase is on screen; images `loading="lazy"`; skeleton — a shimmer on slots near the screen.

---

## 7. Languages

- The markup is English; `site.js` remembers it as BASE; Russian — `I18N_RU` (`i18n.js` + `classes_ru.js`).
- Choice: `?lang=ru|en` → saved in localStorage → browser language (ru* → Russian, otherwise English).
- New text: in the markup in English with `data-t="key"` (or `data-t-attr="attribute:key"`), in `i18n.js` — in Russian.
  Inside `data-t` — text only (site.js sets textContent).
- Class showcase: build_classes.py builds it in Russian and runs it through `i18n_swap.swap` → English markup + classes_ru.js.
- For Yandex (does not always run JS) later — a static `/ru/` from the same dictionaries (not done).
- News — both languages in news.js (from patch_notes.toml).

---

## 8. SEO and social networks (build_seo.py)

- `<title>`: "Dungeonborne — community server: play again" / Russian "Dungeonborne — сервер сообщества: играть снова".
- In `<head>` (seo block): og:site_name "Dungeonborne Community", locales en_US/ru_RU, twitter card, canonical, hreflang
  (en / ru via ?lang=ru / x-default), og:url, og:image (img/og.jpg), JSON-LD: WebSite, Organization (sameAs = LINKS),
  SoftwareApplication (launcher, free, Windows 10/11, version, size, downloadUrl), FAQPage (9 questions of "Help").
- `robots.txt` (Disallow /downloads/, Sitemap), `sitemap.xml`. Re-run after changes to "Help", links, domain, version.
- After the domain is connected: Yandex.Webmaster + Google Search Console, submit the sitemap.

---

## 9. Images and video

- All slots — `web/img/README.md` (story, start, maps, classes, skills, shots, og, favicon). Until the files exist — placeholders
  (`slot` with a crest and the path; on localhost the path and size are shown).
- Formats: WebP q75–80 (a 1920 screenshot ≈ 150–250 KB); later `srcset` 640/1280/1920 via a script. Video: skills 3–6 s,
  class overview 10–20 s, 1280×720, 30 fps, no sound, WebM VP9 + MP4 H.264 (`.webm`, then `.mp4` — that is how site.js looks them up).
  ffmpeg: `-an -vf "scale=1280:-2,fps=30" -c:v libvpx-vp9 -b:v 0 -crf 36` and `-c:v libx264 -crf 26 -preset slow -movflags +faststart`.
- Storage: short ones — with the site on Cloudflare Pages (≤25 MB/file); many — Cloudflare R2; long ones — YouTube. Big
  clips are not in git.

---

## 10. Log of the user's decisions

- **11.11** an enthusiast's site rejected; own landing "in the launcher's style", no borrowed art; Tailwind all the way; palette
  "Torch + Crypt" (later replaced by themes); the "About the game" story in chapters; class showcase like Dota 2's, from the game tables;
  section pinning ("you scroll — the content changes, the page does not move down"), classes and epilogue pinned.
- **11–12.11** talents — tags, description on top of the video; the description fades on hovering the clip; launcher 1.7.0 on the site.
- **13.11** launcher themes along the page; rays and dust; rings behind the launcher window — remove ("remove the rings altogether");
  WebGPU shader (the user's suggestion); three shader modes (torch/crypt-fireflies/blood-fog+loot-pillars);
  theme change: tried a wave with a spark and flashes → "no pulsing" → "and no spark or wave either" → "let's add easing"
  (result — OKLCH 0.6 s via JS); multicoloured diamonds per chapter — "awful" (removed); hide text selection, touch-like scrolling with the mouse;
  remove the second screen of the first section; sections per screen; screenshot carousel; chapter items into the scroll; lighter descriptions;
  the "Launcher" window follows the mouse; Download button in "Launcher"; remove scroll-snap ("I lose control"); own scrollbar — slide-out.
- **13.11** loading: splash screen, lazy loading, skeletons; English is the main language; name Dungeonborne Community; domain
  dungeonborne-community.com; JSON-LD, robots, placeholder links; fonts "as in the game" — a test, chosen "as it is now".
- **14.11** skills — click only; lighter fonts; previews English only; icon = the "D" tile from the splash screen;
  scrollbar always thin (no hover style); the shader's "sharpness" — liked; no framework needed; 14 KB — do not chase it;
  simple language change animation (text dissolve); `site/fonts-src/` in .gitignore — asked the "Agent orchestrator" session;
  "Animations" button — turn motion on over the Windows setting.
- **14.11** "need to add snapping to the ends of screens" → first a settle-after-stop (glide to the nearest edge) —
  "not that kind of snap, remove it"; needed: a flick does not skip the next screen (prologue → chapters) → stop at the screen edge; "scrolling should take
  longer" → edge resistance 500 px.

## 11. Rejected (do not do again)

- Theme on `<html>` repainting the whole page; `@property`/CSS transition/CSS animation of colours on theme change
  (RGB gives a grey-white middle; Chrome does not repaint the colour of visited anchor links during a CSS animation —
  while `getComputedStyle` is correct!); View Transitions (ghost text on scroll); wave/spark/flashes on theme change;
  multicoloured chapter diamonds per theme; content-visibility on hidden classes without fading.
- CSS scroll-snap (any, on the page); settle after the scroll stops (pulls to the nearest edge) — "not that kind of snap";
  stopping at the screen edge is something else, it stays; the second step of the first screen; rings behind the launcher window; scrolling that pages through class skills.
- Fonts "as in the game" (Source Han Serif + Noto Sans) — test rejected 13.11.
- Leptos/a framework for the landing page; a strict 14 KB rule.
- Russian labels in shared materials (previews etc.).

## 12. Open / next

- Images and clips (the user): shots, story (incl. items), start, maps, classes, skills, class/skill videos.
- Buy/connect the domain, Cloudflare Pages (Brotli, HTTP/3, Early Hints), Webmaster/Search Console.
- Community addresses in `LINKS` (build_seo.py) → re-run.
- ~~`site/fonts-src/` in `.gitignore`~~ — done 14.11 by the orchestrator, commit 6ac7dac (.gitignore only).
- Maybe: a static `/ru/` for Yandex; a srcset script for images; an ffmpeg script for clips; stripping comments
  from the HTML on deploy; other languages (the fonts are in fonts-src, subset by unicode-range).
- Commit — only when the user asks.

## 13. Interaction with other sessions

- "Tauri launcher in Rust": the launcher takes the shader (atmos.js, adapted to the window) and fg-2/3/4 from the site. Does not take:
  --glow-c, the torch block, the splash screen, the skeleton, per-page things. Changing the shader/theme tokens → tell it.
- "LOCAL_GAME_MODE_OPENID filling": unpacked the game fonts into `fonts-src/` (with the user's permission), README there.
