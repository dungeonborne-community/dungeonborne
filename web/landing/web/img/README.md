# Site images

The image slots are already marked up in `index.html`. It is enough to drop a file with the right name — the page picks it up
by itself, no markup edits needed. Until the file exists, a stone placeholder with a crest is shown in its place. On your own computer
(`localhost`) the placeholder shows the path and size.

| File | Where on the page | Aspect | Size, px |
|---|---|---|---|
| `story/1-gear.webp` | "About the game", chapter I "Gearing up": town, trader, gear | 16:10 | 1600×1000 |
| `story/2-descent.webp` | chapter II "The descent": waiting hall, gates | 16:10 | 1600×1000 |
| `story/3-dark.webp` | chapter III "The dark": corridors, monsters, the circle | 16:10 | 1600×1000 |
| `story/4-portals.webp` | chapter IV "Portals": blue, red, purple | 16:10 | 1600×1000 |
| `shots/1-town.webp` | "About the game" prologue, carousel: Town: trader, auction and forge | 16:9 | 1920×1080 |
| `shots/2-descent.webp` | "About the game" prologue, carousel: The descent: first corridors | 16:9 | 1920×1080 |
| `shots/3-fight.webp` | "About the game" prologue, carousel: Fighting monsters and other parties | 16:9 | 1920×1080 |
| `shots/4-portals.webp` | "About the game" prologue, carousel: Portals: blue, red, violet | 16:9 | 1920×1080 |
| `shots/5-boss.webp` | "About the game" prologue, carousel: A boss and its loot | 16:9 | 1920×1080 |
| `shots/6-extract.webp` | "About the game" prologue, carousel: Made it out — the loot is yours | 16:9 | 1920×1080 |
| `story/1-1-insurance.webp` | chapter I, item 1 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/1-2-trader.webp` | chapter I, item 2 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/2-1-hall.webp` | chapter II, item 1 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/2-2-groups.webp` | chapter II, item 2 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/3-1-loot.webp` | chapter III, item 1 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/3-2-search.webp` | chapter III, item 2 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/4-1-blue.webp` | chapter IV, item 1 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/4-2-red.webp` | chapter IV, item 2 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `story/4-3-violet.webp` | chapter IV, item 3 — replaces the chapter image while the item is highlighted | 16:10 | 1600×1000 |
| `favicon.svg`, `apple-touch-icon.png` | site icon — the "D" tile as in the splash screen (the dark "Torch" tile, a golden "D" with a glow; the letter is an outline from the site font). Built with: `python site/og/favicon.py`, then `node site/og/render.mjs icon` | 1:1 | 64 (SVG), 180×180 |
| `og.jpg` | link preview in Discord, Telegram, VK and search (og:image, build_seo.py). Now — a simple one, no art: built from `site/og/og.html` with `node site/og/render.mjs` (the site's fonts and colours, 70 KB); can be replaced with your own: the title "Dungeonborne Community" large + art; keep the important part in the centre, the edges may be cropped. JPG (not WebP — not everyone understands it), up to ~300 KB | 1.91:1 | 1200×630 |
| `start/1-download.webp` | "Getting started", step 1: the downloaded exe in a folder | 16:10 | 1600×1000 |
| `start/2-folder.webp` | step 2: picking the game folder in the launcher | 16:10 | 1600×1000 |
| `start/3-discord.webp` | step 3: signing in with Discord in the browser | 16:10 | 1600×1000 |
| `start/4-play.webp` | step 4: the instance lobby and "Start" in the game | 16:10 | 1600×1000 |
| `maps/sinners-end.webp` | chapter V "Deeper" → "Sinner's End" | 16:9 | 1280×720 |
| `maps/clouseau-castle.webp` | chapter V "Deeper" → "Clouseau Castle" | 16:9 | 1280×720 |
| `classes/fighter.webp` | class showcase → "Fighter", video cover | 16:9 | 1280×720 |
| `classes/rogue.webp` | "Rogue" | 16:9 | 1280×720 |
| `classes/priest.webp` | "Priest" | 16:9 | 1280×720 |
| `classes/pyromancer.webp` | "Pyromancer" | 16:9 | 1280×720 |
| `classes/swordmaster.webp` | "Swordmaster" | 16:9 | 1280×720 |
| `classes/druid.webp` | "Druid" | 16:9 | 1280×720 |
| `classes/death-knight.webp` | "Death Knight" | 16:9 | 1280×720 |
| `classes/cryomancer.webp` | "Cryomancer" | 16:9 | 1280×720 |

- **Format** — WebP, quality around 80, up to 200 KB per file. The sizes leave headroom for double-density screens.
- **Cropping.** The image fills the frame completely, the excess is cut off at the edges, and the centre stays. If the aspect ratio differs,
  keep the main thing in the centre.
- **Whose images.** Your own screenshots from the game or your own drawings. We do not take art or the logo from the official Mithril site
  (decision of 11.11).
- **A different name or extension** (e.g. `.jpg`) — change the `src` of the `<img>` and the `data-slot` of its frame in `index.html`.
- **A slot for a new image** — follow the example in the comment to the `slot` utility in `input.css`. The aspect ratio is set by an
  `aspect-*` class. If the ratio is new, first add a token to `@theme`.

The earlier own SVG drawings (portals, map diagrams) are in `../../svg-drafts/`.

## Class and skill videos

The class showcase ("About the game" → "Who you'll be next time"): you pick a crest — the class description is on the right, below it a row of
Q and E skills. The selected skill plays its clip; no skill clip — the class clip; none of that either — the cover or a placeholder.
Videos go into `video/` (next to `index.html`, not into `img/`). The general class clip — `video/classes/`:

| Class | Files |
|---|---|
| Fighter | `fighter.webm`, `fighter.mp4` |
| Rogue | `rogue.webm`, `rogue.mp4` |
| Priest | `priest.webm`, `priest.mp4` |
| Pyromancer | `pyromancer.webm`, `pyromancer.mp4` |
| Swordmaster | `swordmaster.webm`, `swordmaster.mp4` |
| Druid | `druid.webm`, `druid.mp4` |
| Death Knight | `death-knight.webm`, `death-knight.mp4` |
| Cryomancer | `cryomancer.webm`, `cryomancer.mp4` |

- **Clip:** 6–12 seconds, looped (the end joins the start), no sound — the sound is muted anyway.
  Show 1–2 moves of the class, without the game UI and nicknames.
- **Format:** 1280×720, 30 fps. WebM (VP9) — the main one, MP4 (H.264) — for Safari; either one alone is enough.
  Size — up to 2–3 MB per file. Example from a source `src.mp4`:
  `ffmpeg -i src.mp4 -an -vf scale=1280:720 -r 30 -c:v libvpx-vp9 -b:v 1200k fighter.webm` and
  `ffmpeg -i src.mp4 -an -vf scale=1280:720 -r 30 -c:v libx264 -crf 26 -movflags +faststart fighter.mp4`.
- **Cover** (`img/classes/<class>.webp`, 1280×720) is visible while the video loads, with "reduce motion" and if there is no
  video. Handy to take a frame from the same clip: `ffmpeg -ss 2 -i src.mp4 -frames:v 1 -vf scale=1280:720 fighter.webp`.
- **Skill clip** — `video/skills/<id>.webm` / `.mp4`, the same requirements, 4–8 seconds: one skill close up.
- **Skill icon** — `img/skills/<id>.webp`, 128×128 (shown at 64×64). No icon — the letter Q or E.
- Neither video nor cover — a placeholder with a crest is shown. A video plays only while the showcase is on screen, and only for the selected
  class; it loads at the moment of selection, not with the page.

### Skill ids

The skill id is the file name of the clip and the icon (`video/skills/107001.webm`, `img/skills/107001.webp`).

| Class | Skills |
|---|---|
| Fighter | `107001` Q Whirlwind, `107002` E Charge, `107004` Q Battle Cry, `107005` E Inspire, `107006` Q Rupture |
| Rogue | `103001` Q Petrifying Poison, `103002` E Vanish, `103003` Q Quick Blades, `103004` E Swift Concealment, `103005` E Shadow Veil |
| Priest | `105001` Q Divine Guidance, `105002` E Divine Protection, `105004` Q Cleanse, `105005` E Guard |
| Pyromancer | `109001` Q Pyroblast, `109002` E Fire Blast |
| Swordmaster | `101001` Q Psionic Blades, `101002` E Whirling Blade |
| Druid | `106001` Q Primal Awakening, `106002` E Force of Nature, `106004` E Shadow Assault, `106006` E Regrowth |
| Death Knight | `108001` Q Soul Shroud, `108002` E Grasp of the Grave, `108005` Q Soulstorm |
| Cryomancer | `110001` Q Ice Storm, `110002` E Frost Barrier, `110004` Q Frostbite Curse, `110005` E Ice Armor |

The showcase texts are built by `build_classes.py` from the game tables and strings — after editing it run `python build_classes.py`.
