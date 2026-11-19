# Site check tools

Node scripts (22+, built-in WebSocket) that drive **headless Chrome** over the DevTools protocol.
Run them from this folder (`site/tools`), the site must be up: `python ../web/dev_server.py` (port 8821, no cache).
Chrome — `C:/Program Files/Google/Chrome/Application/chrome.exe`. Each script takes its own debugging port (93xx) and
a temporary profile, and closes after it is done. Frame strips are written to `film/` (create it: `mkdir film`).

Why this way and not "by eye": in this project the numbers from the styles (`getComputedStyle`) were correct several times while the screen
was not (the colour of visited links during a CSS animation, see DESIGNBOOK "Rejected"). Check **by pixels and frames**.

| Script | What it does | Example |
|---|---|---|
| `themes.mjs` | goes through the anchors, at each — header theme, screenshot, console errors. Env: `STOPS=top,ch3,…`, `WAIT=ms`, `RM=reduce` (reduce motion), `NOGPU=1` (no WebGPU), arguments — width and height | `STOPS=top,ch3,classes WAIT=1200 node themes.mjs http://localhost:8821/ shot 1440 900` |
| `shot2.mjs` | screenshot of any page (file:// included) — for building "strips" and comparison grids | `node shot2.mjs file:///…/film/grid.html out.png 1450 900` |
| `film.mjs` | frame strip of the chapter diamonds on the ch4→ch5 theme change (full frames + crop; a region capture comes out empty in GPU mode) → `film/strip.html` | `node film.mjs http://localhost:8821/` |
| `film_h.mjs` | the same for the header | |
| `px.mjs` | average colour of the brightest pixels of a region in each frame of a strip — the "no white" check | `node px.mjs <path to film/strip.html> x0 y0 x1 y1` |
| `fade.mjs` | template: open the page at 1920×1080 with GPU, `ev()` in the page; the others are built on it | |
| `scenefade.mjs` | opacity and visibility of the leaving chapter at the boundaries (smoothness of chapter changes) | |
| `diag.mjs` | smooth scrolling across a theme boundary: elements with intermediate colours + long frames (LoAF). Env `STEP` — pixels per frame | `STEP=25 node diag.mjs http://localhost:8821/ ch4 ch5` |
| `load.mjs` | loading: TTFB, DCL, load, FCP, LCP, long frames with scripts, resources | `node load.mjs http://localhost:8821/` |
| `prof.mjs` | CPU profile during loading — time per function (looks for forced layouts) | |
| `mem.mjs` | JS heap, node count, CPU at rest (top, unfocused, middle) — compare with `about:blank` (~1 %) | |
| `anims.mjs` | which animations are running at rest (catches infinite animations on invisible things) | |
| `gpu.mjs` | GPU load from the WMI counter — **noise of 2–10 %**, trust only big differences | |
| `mkbench.mjs` + `runbench.mjs` | frame time of the shader itself per mode (40 passes per submit) | `node mkbench.mjs ../web/atmos.js bench.html && node runbench.mjs file:///…/bench.html` |
| `sbartest.mjs` | own scrollbar: show/hide, near the edge, dragging the thumb, clicking the track | |
| `langtest.mjs` | both languages: title, texts, classes, language button, warnings about missing translations | |
| `vidtest.mjs` | clips do not load before the class showcase, skeleton, placeholder links, JSON-LD | |
| `nosnaptest.mjs` | inside pinned sections the page stays where it was released | |
| `edgestop.mjs` | stop at the screen edge: a wheel flick does not skip the edge; further — only after another 500 px | |
| `check_local.mjs` | open the unpacked site archive from disk (file://) — fonts, pinning, errors | |

Headless Chrome composites frames on the CPU — CPU percentages in it are overstated; compare variants with each other.
If `chrome.exe` processes with `--remote-debugging-port=93xx` are left hanging after a run — they are ours, they can be closed.
