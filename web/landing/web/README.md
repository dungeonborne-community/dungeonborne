# Dungeonborne site

> Overview, design system, decision log, rejected ideas and open tasks — [../DESIGNBOOK.md](../DESIGNBOOK.md)
> (read first). Check scripts in headless Chrome — [../tools/README.md](../tools/README.md).

Landing page of the community server in the launcher's style; the palettes are the launcher's three themes ("Torch", "Crypt", "Blood") and change along the page: HTML + Tailwind CSS v4 + plain JS, no frameworks or
bundlers — the same stack as `launcher/ui`. The background, the "art" and the launcher window on the first screen are built from the same components
and gradients as the launcher itself. Image slots are marked up — the list is in [img/README.md](img/README.md); until the files exist,
they show placeholders with class crests (sprite in `index.html`). The earlier own SVG drawings are in `../svg-drafts/`.

| File | What it is |
| --- | --- |
| `index.html` | all markup; the English text (main language) is right in it, works without JS too |
| `i18n.js` | Russian texts (`window.I18N_RU`), keys = `data-t` and `data-t-attr="attribute:key"`; new text in the markup (English) — right away here too (Russian) |
| `i18n_swap.py` | text replacement by `data-t` keys: puts the main language into the markup, returns the previous text as a dictionary (used by `build_classes.py`) |
| `news.js` | news — built by `build_news.py` from `seasons/<active>/patch_notes.toml`, do not edit by hand |
| `site.js` | language (`?lang=ru`/`?lang=en`, remembered in localStorage, browser language: Russian — Russian, otherwise English), file size, news, menu on narrow screens |
| `input.css` | theme and controls — a copy of `launcher/ui/input.css` + page tokens and utilities (see "Tailwind" below) |
| `fonts.css` | the game font "Dungeonborne Serif", as in launcher 1.6.1: Libre Baskerville (Latin) + Noto Serif (Cyrillic), OFL in `fonts/` |
| `styles.css` | built from `input.css`, do not edit by hand |
| `_headers` | CSP, caching and `Content-Disposition` for Cloudflare Pages / Netlify |
| `update_release.py` | puts a new launcher build on the site (after it — `build_seo.py`: version and size in JSON-LD) |
| `build_seo.py` | search engines and social networks: in `<head>` the `seo:start…end` block (og:site_name "Dungeonborne Community", card, JSON-LD: site, community, the launcher as a free Windows app, the "Help" questions), community link addresses in the footer (`LINKS`), `robots.txt`; when `SITE_URL` is set — also canonical, og:url, og:image (`img/og.jpg` 1200×630 — JPG, not WebP: that is how Discord, Telegram, VK see previews), hreflang (ru via `?lang=ru`) and `sitemap.xml`. Run after changes to "Help", links, domain |
| `robots.txt` | built by `build_seo.py` |
| `build_news.py` | news from the season's patch notes |
| `build_classes.py` | class showcase from the game tables and strings: writes the `classes:start…end` block into `index.html` (English) and `classes_ru.js` |
| `classes_ru.js` | Russian texts of the class showcase — built by `build_classes.py`, do not edit by hand |
| `downloads/` | the launcher exe, not stored in git |

## The page

1. First screen: the title, "Download the launcher" (`btn-play` with sparks) and the launcher window on the right — markup, not a screenshot:
   window title, server, server card with "Play", tabs, instance lobby. The labels are the launcher's strings word for word.
2. "About the game" — the story of one raid in chapters (`story`, `chapter`): prologue, I Gearing up, II The descent, III The dark, IV Portals,
   V Deeper (maps and difficulties), VI The outcome; then the class showcase and the epilogue "This server". A golden thread on the left fills as
   you scroll (animation-timeline: view()). Rules — as short notes (`notes`) under the chapter text. Names of classes,
   races and maps — from the game strings (`names/strings*.tsv`), rules — from `docs/` and `patch_notes.toml`.
3. "Getting started" — settings rows with diamond numbers and a "What you will need" card.
4. "Launcher" — six cards with icon tiles.
5. "News" — the server's patch notes in the launcher's style: sections "New / Fixed / Balance", the first entry is expanded.
6. "Help" — expanders (Expander).

Images: 14 marked-up slots (the `slot` utility) — four "About the game" chapters, two maps, eight class covers.
Class showcase (like a hero page): selection by crests (tabs, arrow keys); each class has a description, strengths and
weaknesses, weapons, Q/E skills and talents — all from the game tables (`build_classes.py`). The selected skill plays its clip
`video/skills/<id>`, otherwise the class clip `video/classes/<class>` (no sound, looped, only while on screen; with
"reduce motion" — via the "Watch" button). Skill ids and clip requirements — in [img/README.md](img/README.md). Which file goes where
and at what size — [img/README.md](img/README.md). Until a file exists, a placeholder with a crest is shown.

Page navigation: on the right (from width xl) a column of dots — the sections and the "About the game" chapters (`scroll-rail`). The current section
is highlighted, the chapters expand inside "About the game", labels — on hover. The anchor in the address changes as you scroll
(`#ch1`…`#ch6`, `#classes`, `#epilogue`, `#start`…) without adding history entries; `#cls-<class>` opens the class in the showcase.

Section pinning: a section settles on the screen and stays, and scrolling changes its content. One engine
(site.js, "section pinning"; styles — the `pin` utility) for all sections:

| Section | Steps | What changes |
|---|---|---|
| About the game, chapters | 6 chapters + 14 items (`#ch1`, `#ch1-1`…) | chapter I–VI: text and image, the thread and diamonds on the left; chapter items (`data-sub`: notes, portals, outcomes) — their own half-screen steps (the last step of a section is a full screen): the current one is highlighted, the chapter image switches to the item's image (`.pin-swap` with `data-hl="chN-K"`) |
| Classes | 1 (`cls`) | showcase on the full screen; class — by clicking a crest, skills and talents — by clicking (scrolling does not page through them) |
| Epilogue | 2 (`ep`, `ep-hold`) | stays on the screen for one scroll step |
| Getting started | 4 (`#st1`…`#st4`) | the current step is expanded, its image on the right (`img/start/`) |
| Launcher | 6 (`#lf1`…`#lf6`) | the current feature is expanded, its part of the launcher window is highlighted (`data-hl`) |
| News, Help | — | normal scrolling: long entries and expanders |

The first screen and the "About the game" prologue are exactly the window height. There is no scroll snapping (scroll-snap) on the page: the user
on 13.11 — jerks to a step took away control and fought with the pinned sections; do not bring it back. The scrollbar is our own,
slide-out (`.sbar`, site.js "scrollbar"; mouse only, touch screens get the system one): the system one is hidden
(`html[data-sbar]`), ours slides out while the page moves (and 1.2 s more) or when the mouse is near the right edge; the thumb is draggable, a click on
the track jumps there, the colour follows the section theme ([data-theme-follow]). Changing the class in the showcase works like changing a chapter: the old scene dissolves (opacity and visibility), the new one's text
rises, the video fades in from a slight zoom. The launcher windows on the first screen and in the "Launcher" section turn
after the mouse (site.js `tilt`). Step and feature descriptions, chapter notes — `text-fg-2` (fg-3 only for small captions).

The "About the game" prologue — screenshot carousel (`[data-carousel]`, site.js "carousel"; images `img/shots/`, list in
img/README.md): advances by itself every 5.5 s while visible, the mouse is not over it and the window is active; arrows, diamond dots, ←/→ keys,
sideways drag with the mouse and a finger.

How to mark up: `<div class="pin" data-pin>` → `.pin-stage` (scene) + `.pin-spacers` (empty, site.js creates the steps).
Steps — `[data-step]` elements with an id (the scenes `.pin-scenes > [data-step]` lie on top of each other and take turns; without `pin-scenes`
a step only gets `data-active`), or states of one scene `data-pin-steps="a b"`. On a step: `data-at` on the section,
`data-active` on the step, `data-lit` on `[data-hl~="key"]` (`.pin-show` — visible only on its step, `.pin-swap` — swappable
images). Anchors move from the scenes onto the steps, so `#key` links, the diamonds and the dot column lead to the step. Pinning
turns on for screens from 1024×720 and only if the scene fits (otherwise the section is a plain feed). There is no snapping to steps — a step changes when its marker passes the middle of the screen.

Launcher themes along the page (draft 13.11): the palettes "Torch" (default, `@theme`), "Crypt" and "Blood"
(`[data-theme]` in input.css) — values from `launcher/ui/input.css`; when they change there, carry them over here. The section theme is written
in the markup: `data-zone="X" data-theme="X"` — a section always has its own colours and is not recoloured on scroll (at the boundary
both are visible). By the zone whose top last passed the middle of the screen, site.js changes the theme only on the shared parts `[data-theme-follow]`
(header, dots on the right, chapter thread, `.atmos` background) — with easing 0.6 s, ease-out (the user's decision on 13.11 after trying a wave with
a spark and flashes and an instant change): colours `data-from` → `data-to` are mixed in OKLCH by the progress `--tp`, the glow too
(`--glow-c`), the text on the accent button flips in the middle (otherwise it merges with the background). `--tp` is driven by site.js every frame
(`requestAnimationFrame`), not by a CSS animation: Chrome treats anchor links as visited and does not repaint their colour during a CSS animation —
the "About the game" text stayed whitish and then jumped (that was the "transition to white"). A new current chapter is in the new
theme at once. During the change the shared parts change only opacity and motion smoothly, their own colour transitions are off
(`[data-switching]`). The background — three `.atmos-layer` layers, switched by opacity over 1 s, the shader light
blends by itself. Do not do again: theme on `<html>` (style recalculation of the whole page 50–80 ms); mixing colours via a CSS animation
or `transition` (visited links are not repainted, in RGB blue → red goes through grey); View
Transitions (a ghost of the old text stays while scrolling); multicoloured diamonds per chapter theme; a wave along the thread with a spark and
flashes (tried — removed). Check frame by frame
(a strip of frames), not by numbers from the styles. Now: first screen, chapters I–II —
"Torch"; III–IV — "Crypt"; V–VI — "Blood"; classes — "Crypt"; epilogue ("The torches still burn") and everything below — "Torch".

Atmosphere: `<div class="atmos">` first in `<body>` — a fixed layer behind the page (the launcher window background and the halo of the light
source), the page moves over it, nothing is repainted on scroll. The theme light is a WebGPU shader in `atmos.js`, each
theme has its own mode: "Torch" — rays and dust in them; "Crypt" — fireflies and a faint cold light from above; "Blood" — low fog and
light pillars from items, as in the game (blue/purple/gold by rarity, tied to the page, moving slower than it).
The modes blend by weights on a theme change (`<canvas class="atmos-gpu">`): one full-screen pass, half resolution, at most 30 frames per second, stops on a hidden
tab, colours — the theme's `--glow` and `--ember`. Without WebGPU (old driver, no adapter) — static CSS rays `.rays`
without dust. With "reduce motion" — one still frame. The shader itself — under 0.15 ms per frame in any mode (960×540, RTX 3080 Ti). Measured 13.11 (1920×1080, 60 Hz): it was 12.4 %
of the GPU at rest (of which ~6 % — the rotating rings behind the launcher window, they were removed), now 2.2 %. Do not bring back
SVG rotation, `blur`/`mix-blend` on animated layers, `backdrop-blur` over the background (header, dot labels — under them
the shader changes every frame) and `background-attachment: fixed`: all of these repaint every frame. Decoration rests when
nobody sees it: `<html data-calm>` (window not focused and the mouse not over the page — DevTools, another window) — sparks, floating and
the shader stop; `.hero-pin[data-offscreen]` — the first screen is off the edge, its sparks and floating stop (site.js, input.css).
Preview — `python dev_server.py` (no cache).

Loading (measured 13.11, local, warm cache): the document is ready in ~0.3 s instead of 1.1, full load ~0.4 s instead of 2.1,
LCP ~1.5 s instead of 2.7. What was done for it and what not to bring back:
- the splash screen `<html data-boot>` + `.boot` (a "D" tile, pulse, bar) — until the font loads, no longer than 1.2 s; the page
  fades in already in its own font; without a script the splash screen leaves by itself after 2.5 s;
- pinned sections are measured at once (pinApply: `data-pinned` on all, then measurements, then steps) — one by one, each caused
  a full page layout;
- do not read `scrollY` in a frame or in the middle of startup: the shader takes the position from the scroll event, the header checks in the first frame;
- the shader (atmos.js) starts after `load`, in idle time; a class clip loads only when the showcase is on screen;
- hidden class scenes after fading — `content-visibility: hidden` (`data-skip`), sizes remembered
  (`contain-intrinsic-size: auto none`), re-remembered after the font loads and on a language change;
- image slots shimmer while the image loads (`.slot[data-loading]`, skeleton); images `loading="lazy"`.

Scrolling by dragging with the mouse (like a finger; site.js, "drag scrolling"): hold the left button on the page and drag,
let go — it coasts on by inertia. Mouse only; does not start on links, buttons, video, code and the dots on the right; a click after
a drag is swallowed; during the drag and inertia scroll snapping is off. Text on the page is not selectable
(`user-select: none` on body), code and paths (`code`) are selectable, people copy them; images and links do not follow the cursor.

Motion: reveal on scroll (`reveal`, `reveal-stagger` — children one after another), card lift (`lift`),
floating and tilting of the window after the mouse, the scroll progress bar under
the header (`scroll-progress`), the header gets denser after scrolling (`data-scrolled:*`). All of it is off with
`prefers-reduced-motion`, and without JS the content is visible at once (the `animated:` variant = a script is present and motion is not reduced).

## Tailwind

All styling is Tailwind classes; there is no bare CSS in `input.css`, only `@theme`, `@utility` and `@custom-variant`.

- Repeated values are tokens in `@theme`, the markup uses ordinary classes: text `text-copy` (descriptions, 15 px),
  `text-label` (13), `text-2xs` (11), `text-hero`; letter spacing `tracking-caps` / `tracking-brand`; shadows `shadow-glow`,
  `shadow-lift`, `shadow-header`, `drop-shadow-ink`; portal colours `text-portal-blue|red|violet`; `aspect-art`;
  animations `animate-float|ember|progress`; easing `ease-glide`.
- Parts used many times — `@utility`: `page` (width and margins), `grid-cols-feature`, `grid-cols-aside-<N>` (side
  panel N×4 px), `led` (a light in the `text-*` colour), `crest`, `lift`, `reveal`, `reveal-stagger`, plus the launcher controls.
- Values in square brackets — only one-off ones (the size of the circle on the first screen etc.). If a value repeats —
  first a token in `@theme`.

Components change in the launcher — carry them over here (font, `btn-*`, `card`, `framed`, `row`, `tab`, `title-rule`, sparks). Palettes — the launcher themes: "Torch" in `@theme`, "Crypt" and "Blood" in `[data-theme]` (`input.css`); which one is where — below, "Launcher themes along the page".

## Build and view

```
tailwindcss -i input.css -o styles.css --minify      # --watch while editing
python dev_server.py                                 # http://127.0.0.1:8821/  (?lang=en — English); no cache, otherwise the browser takes the old styles.css/site.js
```

New text: English — in the markup with `data-t="key"`, Russian — in `i18n.js` under the same key. If the key is missing,
the English text stays and the console shows a warning. An element with `data-t` has its whole text replaced, so keep icons and `code`
next to it, not inside.

## News

```
python build_news.py          # after every edit of the season's patch_notes.toml
```

## New launcher version

```
python update_release.py <data>/launcher/dist/launcher-1.5.3
```

Checks the exe against the release's `launcher.json`, puts it as `downloads/DungeonborneLauncher.exe` (the link on the site does not change)
and writes the version, size and SHA-256 into `index.html`. Deploy only signed builds from `launcher/dist/`.

## What's next

- Take the version and the link from the server manifest once the public address has a proper certificate and CORS.
- Live server status and online count in the launcher window on the first screen.
- Open: domain and hosting, a contact for help.
