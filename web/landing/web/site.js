// Site: language, file size, news (the server's patch notes), menu on narrow screens, section highlighting.
// No dependencies; without JS the whole page reads in English (except the news — they are in the launcher too).
(() => {
  "use strict";
  const $ = (s, el = document) => el.querySelector(s);
  const $$ = (s, el = document) => [...el.querySelectorAll(s)];

  // ------------------------------------------------------------------ "Animations" over the Windows setting
  // The #motion button in the header turns motion on even if animation effects are off in Windows: <html data-motion="on">
  // (the CSS variants motion-safe / motion-reduce / animated respect it), and matchMedia queries about prefers-reduced-motion
  // here and in atmos.js answer as if motion were on. The choice is remembered (localStorage "motion").
  const motionRoot = document.documentElement;
  try {
    if (localStorage.getItem("motion") === "on") motionRoot.dataset.motion = "on";
  } catch {}
  const realMatchMedia = window.matchMedia.bind(window);
  const motionHooks = new Set();
  window.matchMedia = (q) => {
    if (!/prefers-reduced-motion/.test(q)) return realMatchMedia(q);
    const real = realMatchMedia(q);
    // with animations turned on here: "no-preference" — always yes, "reduce" — always no
    const forcedQ = q
      .replace(/\(\s*prefers-reduced-motion:\s*no-preference\s*\)/g, "(min-width: 0px)")
      .replace(/\(\s*prefers-reduced-motion:\s*reduce\s*\)/g, "(max-width: 0px)");
    return {
      media: q,
      get matches() {
        return motionRoot.dataset.motion === "on" ? realMatchMedia(forcedQ).matches : real.matches;
      },
      addEventListener: (type, fn) => (real.addEventListener(type, fn), motionHooks.add(fn)),
      removeEventListener: (type, fn) => (real.removeEventListener(type, fn), motionHooks.delete(fn)),
    };
  };

  // ------------------------------------------------------------------ language
  // The main language is English: it is in the markup, we remember it on load; Russian comes from i18n.js and classes_ru.js.
  const BASE = new Map();
  const BASE_ATTR = new Map();
  const BASE_TITLE = document.title;
  const textEls = $$("[data-t]");
  const attrEls = $$("[data-t-attr]");
  for (const el of textEls) BASE.set(el, el.textContent.trim().replace(/\s+/g, " "));
  for (const el of attrEls) BASE_ATTR.set(el, el.getAttribute(el.dataset.tAttr.split(":")[0]) ?? "");

  const RU = window.I18N_RU || {};
  const ru = (key, fallback) => {
    if (key in RU) return RU[key];
    console.warn("i18n: no Russian text for", key);
    return fallback;
  };
  const sizeText = (bytes, lang) =>
    (bytes / (1024 * 1024)).toLocaleString(lang === "ru" ? "ru-RU" : "en-US", { maximumFractionDigits: 1 });

  let lang = "en"; // as in the markup
  function apply(next) {
    lang = next;
    document.documentElement.lang = lang;
    for (const el of textEls) el.textContent = lang === "ru" ? ru(el.dataset.t, BASE.get(el)) : BASE.get(el);
    for (const el of attrEls) {
      const [attr, key] = el.dataset.tAttr.split(":");
      el.setAttribute(attr, lang === "ru" ? ru(key, BASE_ATTR.get(el)) : BASE_ATTR.get(el));
    }
    document.title = lang === "ru" ? ru("meta.title", BASE_TITLE) : BASE_TITLE;
    for (const el of $$("[data-release=size][data-bytes]")) el.textContent = sizeText(Number(el.dataset.bytes), lang);
    $("#lang-text").textContent = lang === "en" ? "RU" : "EN"; // the language it will switch to
    renderNews();
    document.dispatchEvent(new Event("langchange"));
  }

  function initialLang() {
    const q = new URLSearchParams(location.search).get("lang");
    if (q === "ru" || q === "en") return q;
    try {
      const saved = localStorage.getItem("lang");
      if (saved === "ru" || saved === "en") return saved;
    } catch {}
    return (navigator.language || "en").toLowerCase().startsWith("ru") ? "ru" : "en"; // Russian browser — Russian, otherwise English
  }

  // ------------------------------------------------------------------ news: news.js from patch_notes.toml
  // Looks like the patch notes in the launcher: sections "New / Fixed / Balance" with their own colours and icons.
  const NEWS = window.NEWS || [];
  const KINDS = {
    new: { icon: "i-star", color: "text-arcane", ru: "Новое", en: "New" },
    fixed: { icon: "i-wrench", color: "text-ok", ru: "Исправлено", en: "Fixed" },
    balance: { icon: "i-scales", color: "text-accent", ru: "Баланс", en: "Balance" },
  };
  const svg = (id, cls) => `<svg class="icon ${cls}"><use href="#${id}"/></svg>`;
  const el = (tag, cls, text) => {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    if (text !== undefined) n.textContent = text;
    return n;
  };

  function renderNews() {
    const list = $("#news-list");
    if (!list || !NEWS.length) return;
    const open = new Set($$("details", list).filter((d) => d.open).map((d) => d.dataset.version));
    list.replaceChildren();
    const fmt = (d) =>
      new Date(d + "T12:00:00").toLocaleDateString(lang === "ru" ? "ru-RU" : "en-GB", { day: "numeric", month: "long", year: "numeric" });
    NEWS.forEach((n, i) => {
      const note = $("#tpl-note").content.firstElementChild.cloneNode(true);
      const t = n[lang];
      note.dataset.version = n.version;
      note.open = open.size ? open.has(n.version) : i === 0; // the first entry is expanded
      $("[data-ref=title]", note).textContent = t.title;
      $("[data-ref=date]", note).textContent = fmt(n.date);
      const counts = $("[data-ref=counts]", note);
      const sections = $("[data-ref=sections]", note);
      for (const [k, kind] of Object.entries(KINDS)) {
        const items = t[k] || [];
        if (!items.length) continue;
        const pill = el("span", `pill ${kind.color}`);
        pill.innerHTML = svg(kind.icon, "size-3.5");
        pill.append(`${kind[lang]} · ${items.length}`);
        counts.append(pill);

        const sec = el("section");
        const h = el("h3", `flex items-center gap-2 text-xs font-bold tracking-[0.15em] uppercase ${kind.color}`);
        h.innerHTML = svg(kind.icon, "size-4");
        h.append(kind[lang]);
        const ul = el("ul", "mt-1.5 list-disc space-y-1 pl-6 text-fg-2 marker:text-accent-strong");
        for (const text of items) ul.append(el("li", "", text));
        sec.append(h, ul);
        sections.append(sec);
      }
      list.append(note);
    });
  }

  // ------------------------------------------------------------------ menu on narrow screens
  const menuBtn = $("#menu-btn");
  const menu = $("#menu");
  function setMenu(open) {
    menu.hidden = !open;
    menuBtn.setAttribute("aria-expanded", String(open));
  }
  menuBtn.addEventListener("click", () => setMenu(menu.hidden));
  // close: picking a section, Esc, a click outside the header, switching to a wide screen
  menu.addEventListener("click", (e) => e.target.closest("a") && setMenu(false));
  addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !menu.hidden) {
      setMenu(false);
      menuBtn.focus();
    }
  });
  document.addEventListener("click", (e) => !menu.hidden && !e.target.closest("header") && setMenu(false));
  matchMedia("(min-width: 64rem)").addEventListener("change", (m) => m.matches && setMenu(false));

  // ------------------------------------------------------------------ section highlighting in the header
  const navLinks = $$("[data-nav]");
  const spy = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (e.isIntersecting) navLinks.forEach((a) => a.setAttribute("aria-current", String(a.dataset.nav === e.target.id)));
      }
    },
    { rootMargin: "-45% 0px -50% 0px" },
  );
  ["game", "start", "launcher", "news", "help"].forEach((id) => spy.observe(document.getElementById(id)));
  // first screen: no section is highlighted
  new IntersectionObserver(([e]) => e.isIntersecting && navLinks.forEach((a) => a.setAttribute("aria-current", "false")), {
    rootMargin: "-45% 0px -50% 0px",
  }).observe($(".hero-pin > section"));

  // ------------------------------------------------------------------ motion
  // Reveal on scroll: the reveal / reveal-stagger utilities in input.css (only with the script and without reduced-motion),
  // here — the children's delay --d and the data-in mark when the block has appeared.
  for (const group of $$(".reveal-stagger")) {
    [...group.children].forEach((child, i) => child.style.setProperty("--d", `${Math.min(i, 8) * 0.08}s`));
  }
  const reveal = new IntersectionObserver(
    (entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        e.target.dataset.in = "";
        reveal.unobserve(e.target);
      }
    },
    { rootMargin: "0px 0px -8% 0px" },
  );
  $$(".reveal, .reveal-stagger").forEach((n) => reveal.observe(n));

  // the header gets denser once scrolling starts (data-scrolled:* classes in index.html)
  const header = $("header");
  const onScroll = () => header.toggleAttribute("data-scrolled", scrollY > 8);
  addEventListener("scroll", onScroll, { passive: true });
  requestAnimationFrame(onScroll);

  // the launcher windows (first screen, the "Launcher" section) turn slightly after the mouse (wide screens only and without reduced-motion)
  const tiltOk = matchMedia("(min-width: 64rem) and (hover: hover) and (prefers-reduced-motion: no-preference)");
  const tilt = (area, win) => {
    if (!area || !win) return;
    area.addEventListener("pointermove", (e) => {
      if (!tiltOk.matches) return;
      const r = area.getBoundingClientRect();
      const x = (e.clientX - r.left) / r.width - 0.5;
      const y = (e.clientY - r.top) / r.height - 0.5;
      win.style.setProperty("--tx", `${(x * 6).toFixed(2)}deg`);
      win.style.setProperty("--ty", `${(-y * 4).toFixed(2)}deg`);
    });
    area.addEventListener("pointerleave", () => {
      win.style.removeProperty("--tx");
      win.style.removeProperty("--ty");
    });
  };
  const hero = $(".hero-pin > section");
  tilt(hero, $(".window-shot"));
  tilt($("#launcher"), $(".demo-window"));

  // image slots: on your own computer we show the file path and size (the slot utility in input.css)
  if (["localhost", "127.0.0.1", "[::1]"].includes(location.hostname)) document.documentElement.dataset.dev = "";
  // the file does not exist yet — hide the img so the browser does not draw a broken-image icon over the placeholder
  // while the image loads, the slot shimmers (skeleton, input.css .slot[data-loading])
  // the shimmer runs only on slots near the screen (data-near): on far lazy images it would spin for nothing
  const nearIO =
    "IntersectionObserver" in window &&
    new IntersectionObserver((list) => list.forEach((e) => e.target.toggleAttribute("data-near", e.isIntersecting)), { rootMargin: "200px 0px" });
  for (const img of $$(".slot > img")) {
    if (img.complete && !img.naturalWidth) img.hidden = true;
    else {
      const slot = img.parentElement;
      if (!img.complete) (slot.dataset.loading = ""), nearIO && nearIO.observe(slot);
      const done = () => {
        delete slot.dataset.loading;
        delete slot.dataset.near;
        if (nearIO) nearIO.unobserve(slot);
      };
      img.addEventListener("load", done, { once: true });
      img.addEventListener("error", () => ((img.hidden = true), done()), { once: true });
    }
  }

  // ------------------------------------------------------------------ section pinning
  // [data-pin]: the scene (.pin-stage) sticks under the header, behind it — empty per-screen steps (.pin-spacers), scrolling them changes
  // the step while the page stays. Steps — [data-step] scenes (the key is their id, it moves onto the step so that #key links work),
  // or states of one scene from data-pin-steps="a b". On a step: data-at on the section, data-active on the scene, data-lit on
  // [data-hl~="key"], diamonds in .pin-marks (data-marks="roman"). Turns on for screens from 1024×720 and only if the scene
  // fits the screen; otherwise the section is a plain feed. The state of each section is in pinState (for the first screen).
  const fits = matchMedia("(min-width: 64rem) and (min-height: 45rem)");
  const roman = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];
  // A scene can have items [data-sub] (chapter items): each is one more step (key "id-N", half a screen of scrolling); the scene
  // does not change, the item gets data-hl = its own key (highlight, a swappable image with the same data-hl).
  const pins = $$("[data-pin]").map((pin) => {
    const scenes = $$("[data-step]", pin);
    const keys = [];
    const keyScene = []; // scene index of each step
    const sub = []; // the step is an item inside a scene
    if (scenes.length) {
      scenes.forEach((sc, i) => {
        keys.push(sc.id), keyScene.push(i), sub.push(false);
        $$("[data-sub]", sc).forEach((el, j) => {
          el.dataset.hl = `${sc.id}-${j + 1}`;
          keys.push(el.dataset.hl), keyScene.push(i), sub.push(true);
        });
      });
    } else {
      for (const k of pin.dataset.pinSteps.split(/\s+/)) keys.push(k), keyScene.push(keys.length - 1), sub.push(false);
    }
    return { pin, scenes, keys, keyScene, sub, stage: $(".pin-stage", pin), spacersBox: $(".pin-spacers", pin), marksBox: $(".pin-marks", pin), spacers: [], marks: [] };
  });
  const pinUpdate = (s) => {
    if (!s.spacers.length) return;
    const line = innerHeight * 0.5;
    let cur = 0;
    s.spacers.forEach((sp, i) => {
      if (sp.getBoundingClientRect().top <= line) cur = i;
    });
    const key = s.keys[cur];
    if (s.pin.dataset.at !== key) {
      s.pin.dataset.at = key;
      const scene = s.keyScene[cur];
      s.scenes.forEach((sc, i) => sc.toggleAttribute("data-active", i === scene));
      s.marks.forEach((m, i) => {
        m.toggleAttribute("data-now", i === scene);
        m.toggleAttribute("data-done", i < scene);
      });
      for (const el of $$("[data-hl]", s.pin)) el.toggleAttribute("data-lit", el.dataset.hl.split(/\s+/).includes(key));
      s.pin.dispatchEvent(new CustomEvent("pinstep", { detail: key }));
    }
    const r = s.pin.getBoundingClientRect();
    s.pin.style.setProperty("--p", Math.min(1, Math.max(0, -r.top / Math.max(1, r.height - innerHeight))).toFixed(3));
  };
  // does the scene fit the screen: the scene's blocks (without decorations with position: absolute) and each step scene
  const overflows = (s) => {
    const box = s.stage.getBoundingClientRect();
    const parts = [...s.stage.children].filter((c) => getComputedStyle(c).position !== "absolute" && c.offsetHeight);
    const outside = parts.some((c) => {
      const r = c.getBoundingClientRect();
      return r.top < box.top - 2 || r.bottom > box.bottom + 2;
    });
    return outside || s.scenes.some((sc) => sc.scrollHeight > sc.clientHeight + 2);
  };
  function pinOn(s) {
    s.pin.dataset.pinned = "";
    if (overflows(s)) return pinOff(s); // does not fit — plain feed
    pinBuild(s);
  }
  // steps and diamonds of a pinned section (already measured)
  function pinBuild(s) {
    s.spacers = s.keys.map((key, i) => {
      const sp = document.createElement("div");
      sp.className = "pin-spacer";
      if (s.sub[i] && i < s.keys.length - 1) sp.dataset.sub = ""; // the last step is a full screen, otherwise the scene leaves too early
      const sc = s.sub[i] ? null : s.scenes[s.keyScene[i]];
      if (sc) {
        sc.dataset.key = key;
        sc.removeAttribute("id");
        sc.addEventListener("click", s.onSceneClick);
      }
      sp.id = key;
      s.spacersBox.append(sp);
      return sp;
    });
    if (s.marksBox) {
      s.marks = s.keys.filter((_, i) => !s.sub[i]).map((key, i) => {
        const a = document.createElement("a");
        a.className = "story-mark";
        a.href = `#${key}`;
        a.tabIndex = -1;
        a.textContent = roman[i];
        s.marksBox.append(a);
        return a;
      });
    }
    delete s.pin.dataset.at;
    pinUpdate(s);
  }
  function pinOff(s) {
    delete s.pin.dataset.pinned;
    delete s.pin.dataset.at;
    for (const sc of s.scenes) {
      if (sc.dataset.key) sc.id = sc.dataset.key; // the anchor goes back onto the scene
      delete sc.dataset.key;
      sc.removeAttribute("data-active");
    }
    for (const el of $$("[data-lit]", s.pin)) el.removeAttribute("data-lit");
    s.spacersBox.replaceChildren();
    s.marksBox?.replaceChildren();
    s.spacers = [];
    s.marks = [];
  }
  for (const s of pins) {
    // a feature scene on click: scroll to its step
    s.onSceneClick = (e) => {
      if (!("pinned" in s.pin.dataset) || e.target.closest("a, button")) return;
      document.getElementById(e.currentTarget.dataset.key)?.scrollIntoView({ behavior: matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth" });
    };
  }
  const pinApply = () => {
    const hash = location.hash;
    for (const s of pins) if ("pinned" in s.pin.dataset) pinOff(s);
    if (fits.matches) {
      // all at once: turn on, measure (one page layout for all sections), then build the steps — otherwise each section
      // made the browser lay out the page again
      for (const s of pins) s.pin.dataset.pinned = "";
      const fit = pins.map((s) => !overflows(s));
      pins.forEach((s, i) => (fit[i] ? pinBuild(s) : pinOff(s)));
    }
    // opened via a link to a step — after rebuilding, lead to its new place
    if (hash && pins.some((s) => s.keys.includes(hash.slice(1)))) document.getElementById(hash.slice(1))?.scrollIntoView({ behavior: "instant" });
  };
  // after the font loads: only re-check whether the scenes fit (without rebuilding all sections — otherwise the anchor gets lost)
  const pinRecheck = () => {
    if (!fits.matches) return;
    let changed = false;
    for (const s of pins) {
      const on = "pinned" in s.pin.dataset;
      const over = on && overflows(s);
      if (over) pinOff(s), (changed = true);
      else if (!on) pinOn(s), (changed = changed || "pinned" in s.pin.dataset);
    }
    const hash = location.hash;
    if (changed && hash && pins.some((s) => s.keys.includes(hash.slice(1)))) document.getElementById(hash.slice(1))?.scrollIntoView({ behavior: "instant" });
  };
  let pinQueued = false;
  addEventListener(
    "scroll",
    () => {
      if (pinQueued) return;
      pinQueued = true;
      requestAnimationFrame(() => {
        pinQueued = false;
        pins.forEach(pinUpdate);
      });
    },
    { passive: true },
  );
  let resizeTimer;
  addEventListener("resize", () => {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(pinApply, 200);
  });
  fits.addEventListener("change", pinApply);
  pinApply();
  document.fonts?.ready.then(pinRecheck); // the font changes the text height — check again whether the scene fits
  addEventListener("pinrecheck", pinRecheck); // the scene changed height (another class in the showcase)

  // ------------------------------------------------------------------ stop at the screen edge
  // Not CSS scroll-snap (it pulled to every step — jerks, removed 13.11) and not a settle after stopping (14.11, "not that kind of
  // snap"). One flick of the wheel / touchpad / drag inertia does not skip past a screen edge: it reaches the edge — stop; further —
  // only by scrolling more (edge resistance, PUSH below). Edges: top, prologue, start and end of each pinned
  // section, "News", "Help", bottom (minus the header). Inside screens and pinned sections scrolling is normal. Only on wide screens, where pinning works.
  const HEADER_H = 56;
  const screenEdges = () => {
    const top = (el) => el.getBoundingClientRect().top + scrollY;
    const p = [0];
    const prologue = $("#game .page > div");
    if (prologue) p.push(top(prologue) - HEADER_H);
    for (const s of pins) {
      if (!("pinned" in s.pin.dataset)) continue;
      const r = s.pin.getBoundingClientRect();
      p.push(r.top + scrollY - HEADER_H, r.bottom + scrollY - innerHeight); // start and end of the pinned section
    }
    for (const id of ["news", "help"]) {
      const el = document.getElementById(id);
      if (el) p.push(top(el) - HEADER_H);
    }
    p.push(document.documentElement.scrollHeight - innerHeight);
    return [...new Set(p.map((v) => Math.round(Math.max(0, v))))].sort((x, y) => x - y);
  };
  // the edge crossed by the path from → to (not counting the one we are already on), or null
  const edgeCrossed = (edges, from, to) => {
    if (to > from) return edges.find((e) => e > from + 2 && e <= to) ?? null;
    if (to < from) return edges.findLast((e) => e < from - 2 && e >= to) ?? null;
    return null;
  };
  {
    const GAP = 220; // pause between wheel events after which it is already a new movement
    const smooth = () => (matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth");
    // inside an element with its own vertical scrolling that can still move — the wheel belongs to it
    const innerScroll = (el, dy) => {
      for (; el && el !== document.body && el !== document.documentElement; el = el.parentElement) {
        if (el.scrollHeight <= el.clientHeight + 1) continue;
        const oy = getComputedStyle(el).overflowY;
        if (oy !== "auto" && oy !== "scroll") continue;
        if (dy > 0 ? el.scrollTop + el.clientHeight < el.scrollHeight - 1 : el.scrollTop > 0) return true;
      }
      return false;
    };
    // Edge resistance: after a stop the page does not move further in the same direction until you scroll another PUSH pixels
    // of wheel (≈ 5 clicks) with new flicks; a pause longer than FORGET resets the accumulated amount. Backwards — immediately.
    const PUSH = 500;
    const FORGET = 1200;
    let hold = null; // { edge, dir, push, last }
    let g = null; // the current wheel movement: { aim, last, stopped, edges }
    addEventListener(
      "wheel",
      (e) => {
        if (!fits.matches || e.ctrlKey || e.defaultPrevented || Math.abs(e.deltaX) > Math.abs(e.deltaY)) return;
        const dy = e.deltaY * (e.deltaMode === 1 ? 40 : e.deltaMode === 2 ? innerHeight : 1);
        if (!dy || innerScroll(e.target, dy)) return;
        const t = e.timeStamp;
        if (!g || t - g.last > GAP) {
          // a new movement; if the previous one has just ended, smooth scrolling is still running — count from its target
          const recent = g && t - g.last < 700;
          g = { aim: recent ? g.aim : scrollY, last: t, stopped: false, edges: screenEdges() };
        }
        g.last = t;
        if (g.stopped) return e.preventDefault(); // the rest of the flick (and touchpad inertia) after a stop — swallow it
        if (hold && Math.abs(scrollY - hold.edge) > 8 && Math.abs(g.aim - hold.edge) > 2) hold = null; // left the edge some other way
        if (hold && Math.sign(dy) !== hold.dir) hold = null;
        if (hold) {
          if (t - hold.last > FORGET) hold.push = 0;
          hold.last = t;
          hold.push += Math.abs(dy);
          if (hold.push < PUSH) return e.preventDefault();
          hold = null; // pushed through — normal scrolling from here
        }
        const max = document.documentElement.scrollHeight - innerHeight;
        const to = Math.max(0, Math.min(max, g.aim + dy));
        const edge = edgeCrossed(g.edges, g.aim, to);
        if (edge === null) return void (g.aim = to);
        e.preventDefault();
        g.stopped = true;
        g.aim = edge;
        hold = { edge, dir: Math.sign(dy), push: 0, last: t };
        scrollTo({ top: edge, behavior: smooth() });
      },
      { passive: false }
    );
  }

  // ------------------------------------------------------------------ launcher themes along the page
  // [data-zone="torch|crypt|blood"] — the section theme (input.css, [data-theme]). The current one belongs to the zone whose top last passed
  // the middle of the screen; for a pinned step scene its scroll step is used (the id moved onto it). Above all zones — "Torch".
  const zones = $$("[data-zone]");
  const zoneTop = (el) => ((el.dataset.key && document.getElementById(el.dataset.key)) || el).getBoundingClientRect().top;
  const zoneUpdate = () => {
    const line = innerHeight * 0.5;
    let best = null;
    let bestTop = -Infinity;
    for (const z of zones) {
      const t = zoneTop(z);
      if (t <= line && t > bestTop) (best = z), (bestTop = t);
    }
    const theme = best?.dataset.zone || "torch";
    if (theme === wantTheme) return;
    wantTheme = theme;
    for (const l of $$(".atmos-layer")) l.toggleAttribute("data-on", l.dataset.theme === theme);
    $(".atmos")?.setAttribute("data-theme", theme);
    themeSwitch(theme);
    dispatchEvent(new CustomEvent("themechange", { detail: theme }));
  };
  let wantTheme = null;

  // The shared parts of the page [data-theme-follow] (header, dots on the right, chapter thread and diamonds, background) change theme with easing: colours
  // data-from → data-to are mixed in OKLCH by the progress --tp (input.css; driven by the JS below, 0.6 s ease-out). A new current chapter is in
  // the new theme at once (otherwise its diamond lights up in the previous one: "Crypt" has an almost white accent). The sections themselves are not recoloured —
  // the theme is written in the markup. The background dissolves in layers, the shader light flows over by itself.
  const followers = $$("[data-theme-follow]");
  let shownTheme = null;
  // The progress --tp is driven by JS every frame, not by a CSS animation: Chrome treats anchor links (#game, #ch1…) as visited, and the colour
  // of a visited link is not recomputed during a CSS animation — the text stayed old (whitish) and then jumped.
  const THEME_MS = 600;
  const easeOut = (t) => 1 - (1 - t) ** 5; // like cubic-bezier(0.22, 1, 0.36, 1): fast start, soft fade-out
  let tpFrame = 0;
  const themeSwitch = (theme) => {
    const from = shownTheme || theme;
    shownTheme = theme;
    cancelAnimationFrame(tpFrame);
    for (const m of $$(".story-mark[data-theme]")) delete m.dataset.theme;
    const cur = $(".story-mark[data-now]");
    if (cur && from !== theme) cur.dataset.theme = theme;
    for (const el of followers) {
      el.dataset.from = from;
      el.dataset.to = theme;
      el.dataset.theme = theme;
      el.dataset.switching = "";
    }
    const instant = from === theme || matchMedia("(prefers-reduced-motion: reduce)").matches;
    const t0 = performance.now();
    const step = (now) => {
      const t = instant ? 1 : Math.min(1, (now - t0) / THEME_MS);
      const tp = easeOut(t).toFixed(4);
      for (const el of followers) el.style.setProperty("--tp", tp);
      if (t < 1) return (tpFrame = requestAnimationFrame(step));
      if (cur) delete cur.dataset.theme;
      for (const el of followers) delete el.dataset.switching;
    };
    step(t0);
  };


  let zoneQueued = false;
  addEventListener(
    "scroll",
    () => {
      if (zoneQueued) return;
      zoneQueued = true;
      requestAnimationFrame(() => {
        zoneQueued = false;
        zoneUpdate();
      });
    },
    { passive: true },
  );
  addEventListener("resize", zoneUpdate);
  zoneUpdate();

  // ------------------------------------------------------------------ screenshot carousel (the "About the game" prologue)
  // Advances by itself every 5.5 s while the carousel is on screen, the mouse is not over it, the window is active (not <html data-calm>) and without
  // "reduce motion". Arrows, diamond dots, arrow keys, sideways drag with the mouse and a finger.
  for (const box of $$("[data-carousel]")) {
    const slides = $$("[data-slide]", box);
    const dots = $$(".shots-dot", box);
    const still = matchMedia("(prefers-reduced-motion: reduce)");
    let cur = 0;
    let timer = 0;
    let hover = false;
    let seen = false;
    const go = (n) => {
      cur = (n + slides.length) % slides.length;
      slides.forEach((sl, k) => {
        sl.toggleAttribute("data-active", k === cur);
        sl.inert = k !== cur;
      });
      dots.forEach((d, k) => d.setAttribute("aria-current", k === cur ? "true" : "false"));
    };
    const schedule = () => {
      clearTimeout(timer);
      if (hover || !seen || document.hidden || still.matches || "calm" in document.documentElement.dataset) return;
      timer = setTimeout(() => (go(cur + 1), schedule()), 5500);
    };
    const step = (d) => (go(cur + d), schedule());
    $('[data-shots="prev"]', box).addEventListener("click", () => step(-1));
    $('[data-shots="next"]', box).addEventListener("click", () => step(1));
    dots.forEach((d, k) => {
      d.setAttribute("aria-label", `${k + 1} / ${dots.length}`);
      d.addEventListener("click", () => (go(k), schedule()));
    });
    box.addEventListener("keydown", (e) => {
      if (e.key === "ArrowLeft") step(-1), e.preventDefault();
      if (e.key === "ArrowRight") step(1), e.preventDefault();
    });
    // sideways with the mouse or a finger: more than 40 px — next / previous
    let down = null;
    box.addEventListener("pointerdown", (e) => {
      if (e.target.closest("button") || (e.pointerType === "mouse" && e.button !== 0)) return;
      down = { x: e.clientX, y: e.clientY };
    });
    box.addEventListener("pointerup", (e) => {
      if (!down) return;
      const dx = e.clientX - down.x;
      const dy = e.clientY - down.y;
      down = null;
      if (Math.abs(dx) > 40 && Math.abs(dx) > Math.abs(dy)) step(dx < 0 ? 1 : -1);
    });
    box.addEventListener("pointercancel", () => (down = null));
    box.addEventListener("pointerenter", (e) => e.pointerType === "mouse" && ((hover = true), schedule()));
    box.addEventListener("pointerleave", () => ((hover = false), schedule()));
    if ("IntersectionObserver" in window) new IntersectionObserver(([e]) => ((seen = e.isIntersecting), schedule())).observe(box);
    else seen = true;
    document.addEventListener("visibilitychange", schedule);
    addEventListener("calmchange", schedule);
    still.addEventListener("change", schedule);
    go(0);
    schedule();
  }

  // ------------------------------------------------------------------ scrollbar (slide-out)
  // Our own instead of the system one — only with a mouse ((hover: hover) and (pointer: fine)); touch screens keep the system one.
  // Visible while the page moves (and 1.2 s more) or when the mouse is near the right edge; the thumb is draggable, a click on the track jumps there.
  // The page height changes (pinning, font, showcase) — recalculated via ResizeObserver. Position — from the scroll event.
  {
    const bar = $(".sbar");
    const fine = matchMedia("(hover: hover) and (pointer: fine)");
    if (bar && fine.matches && "ResizeObserver" in window) {
      const root = document.documentElement;
      const thumb = $(".sbar-thumb", bar);
      bar.hidden = false;
      root.dataset.sbar = "";
      let trackH = 0;
      let thumbH = 0;
      let maxScroll = 1;
      let y = scrollY;
      let hideTimer = 0;
      let near = false;
      let drag = null;
      const place = () => bar.style.setProperty("--sy", `${Math.round((trackH - thumbH) * Math.min(1, y / maxScroll))}px`);
      const measure = () => {
        trackH = bar.clientHeight;
        maxScroll = Math.max(1, root.scrollHeight - innerHeight);
        thumbH = Math.max(36, Math.round((trackH * innerHeight) / root.scrollHeight));
        bar.style.setProperty("--sh", `${thumbH}px`);
        place();
      };
      const show = (stay) => {
        bar.dataset.show = "";
        clearTimeout(hideTimer);
        if (!stay) hideTimer = setTimeout(() => near || drag || delete bar.dataset.show, 1200);
      };
      addEventListener(
        "scroll",
        () => {
          y = scrollY;
          place();
          show(near || !!drag);
        },
        { passive: true },
      );
      new ResizeObserver(measure).observe(document.body);
      addEventListener("resize", measure);
      // mouse near the right edge (below the header) — slide out; it left — hide after a pause
      document.addEventListener(
        "pointermove",
        (e) => {
          if (e.pointerType !== "mouse" || drag) return;
          const n = e.clientX >= innerWidth - 20 && e.clientY > 56;
          if (n === near) return;
          near = n;
          show(n);
        },
        { passive: true },
      );
      document.documentElement.addEventListener("mouseleave", () => near && ((near = false), show(false)));
      // the thumb is dragged with the mouse
      thumb.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        drag = { y0: e.clientY, s0: scrollY };
        show(true);
        thumb.setPointerCapture(e.pointerId);
      });
      thumb.addEventListener("pointermove", (e) => {
        if (!drag) return;
        const k = maxScroll / Math.max(1, trackH - thumbH);
        scrollTo({ top: drag.s0 + (e.clientY - drag.y0) * k, behavior: "instant" });
      });
      const end = () => {
        if (!drag) return;
        drag = null;
        show(near);
      };
      thumb.addEventListener("pointerup", end);
      thumb.addEventListener("pointercancel", end);
      // click on the track — go there (thumb centre under the mouse)
      bar.addEventListener("pointerdown", (e) => {
        if (e.target !== bar || e.button !== 0) return;
        const t = (e.clientY - bar.getBoundingClientRect().top - thumbH / 2) / Math.max(1, trackH - thumbH);
        scrollTo({ top: Math.max(0, Math.min(1, t)) * maxScroll, behavior: "instant" });
      });
      measure();
    }
  }

  // ------------------------------------------------------------------ drag scrolling with the mouse (like a finger)
  // Hold the left button on the page and drag — the page follows the mouse; let go — it coasts on by inertia and settles softly.
  // Mouse only (touch screens have native scrolling). Does not start on links, buttons, fields, video, code and the dots
  // on the right; a click after a drag is swallowed. Without motion (reduced motion) — no inertia. There is no snapping of the scroll to steps
  // (scroll-snap removed: jerks to a step took away control) — the page stays where it was released; inertia
  // only does not skip past a screen edge (the "stop at the screen edge" block).
  {
    const root = document.documentElement;
    const NO_DRAG = "a, button, input, select, textarea, label, video, summary, .code, .scroll-rail, .sbar, [data-carousel], [contenteditable]";
    const still = matchMedia("(prefers-reduced-motion: reduce)");
    let drag = null; // { id, y, moved, samples: [[t, y]] }
    let glide = 0;
    let suppressClick = false;
    const scrollByPx = (dy) => scrollBy({ top: dy, behavior: "instant" });
    const stopGlide = () => {
      cancelAnimationFrame(glide);
      glide = 0;
    };
    document.addEventListener("pointerdown", (e) => {
      stopGlide();
      if (e.pointerType !== "mouse" || e.button !== 0 || e.target.closest(NO_DRAG)) return;
      drag = { id: e.pointerId, y: e.clientY, moved: false, samples: [[e.timeStamp, e.clientY]] };
    });
    document.addEventListener("pointermove", (e) => {
      if (!drag || e.pointerId !== drag.id) return;
      const dy = e.clientY - drag.y;
      if (!drag.moved) {
        if (Math.abs(dy) < 6) return; // hand tremor — still a click
        drag.moved = true;
        root.dataset.dragging = "";
        try {
          e.target.setPointerCapture?.(e.pointerId);
        } catch {}
      }
      drag.y = e.clientY;
      scrollByPx(-dy);
      drag.samples.push([e.timeStamp, e.clientY]);
      if (drag.samples.length > 6) drag.samples.shift();
    });
    const release = (e) => {
      if (!drag || e.pointerId !== drag.id) return;
      const d = drag;
      drag = null;
      delete root.dataset.dragging;
      if (!d.moved) return;
      suppressClick = true;
      setTimeout(() => (suppressClick = false), 0);
      // speed over the last ~100 ms of movement, pixels per millisecond
      const last = d.samples[d.samples.length - 1];
      const first = d.samples.find((s) => last[0] - s[0] <= 100) || d.samples[0];
      let v = last[0] > first[0] ? -(last[1] - first[1]) / (last[0] - first[0]) : 0;
      if (still.matches || Math.abs(v) < 0.05) return;
      v = Math.max(-6, Math.min(6, v));
      let prev = performance.now();
      const edges = fits.matches ? screenEdges() : null; // stop at the screen edge (above)
      const step = (now) => {
        const dt = Math.min(40, now - prev);
        prev = now;
        const edge = edges && edgeCrossed(edges, scrollY, scrollY + v * dt);
        if (edge !== null && edge !== undefined) {
          scrollTo({ top: edge, behavior: "instant" }); // inertia does not skip past a screen edge
          glide = 0;
          return;
        }
        scrollByPx(v * dt);
        v *= Math.pow(0.94, dt / 16); // decay ~ like a finger's
        if (Math.abs(v) > 0.03) glide = requestAnimationFrame(step);
        else glide = 0;
      };
      glide = requestAnimationFrame(step);
    };
    document.addEventListener("pointerup", release);
    document.addEventListener("pointercancel", release);
    // the click that ended a drag is not a click
    document.addEventListener("click", (e) => suppressClick && (e.preventDefault(), e.stopPropagation()), true);
    // wheel or keys during inertia — inertia gives way
    addEventListener("wheel", () => glide && stopGlide(), { passive: true });
    addEventListener("keydown", () => glide && stopGlide());
  }

  // ------------------------------------------------------------------ decoration rests when nobody sees it
  // <html data-calm>: window not focused and the mouse not over the page (DevTools, another window) — sparks, floating and the shader stop
  // (input.css, atmos.js); the mouse came back or the window is focused — they go on. .hero-pin / #launcher [data-offscreen] — the section is off the edge.
  {
    const root = document.documentElement;
    let focused = document.hasFocus();
    let hovered = true;
    const calmUpdate = () => {
      const calm = !focused && !hovered;
      if (calm === "calm" in root.dataset) return;
      root.toggleAttribute("data-calm", calm);
      dispatchEvent(new CustomEvent("calmchange", { detail: calm }));
    };
    addEventListener("focus", () => ((focused = true), calmUpdate()));
    addEventListener("blur", () => ((focused = false), calmUpdate()));
    document.addEventListener("mouseout", (e) => !e.relatedTarget && ((hovered = false), calmUpdate()));
    document.addEventListener("pointermove", () => hovered || ((hovered = true), calmUpdate()), { passive: true });
    // sparks and floating stop while their section is off the screen edge (first screen, "Launcher" with the "Download" button)
    if ("IntersectionObserver" in window) {
      const io = new IntersectionObserver((list) => list.forEach((e) => e.target.toggleAttribute("data-offscreen", !e.isIntersecting)));
      for (const el of $$(".hero-pin, #launcher")) io.observe(el);
    }
  }

  // ------------------------------------------------------------------ page navigation: dots on the right and the anchor in the address
  // The current section is the last one whose top is above 35% of the screen. The anchor in the address changes without a history entry and only after
  // the first scroll (a link like #sha does not disappear right on opening).
  const rail = $(".scroll-rail");
  if (rail) {
    const links = $$("[data-spy]", rail);
    let active = null;
    let scrolled = false;
    let queued = false;
    const update = () => {
      queued = false;
      const line = innerHeight * 0.35;
      const targets = links.map((a) => document.getElementById(a.dataset.spy));
      let cur = 0;
      targets.forEach((t, i) => {
        if (t.getBoundingClientRect().top <= line) cur = i;
      });
      const id = targets[cur].id;
      if (id !== active) {
        active = id;
        links.forEach((a, i) => a.setAttribute("aria-current", String(i === cur)));
        // the group of the current section is expanded (chapters, steps), its heading is outlined if the current one is a sub-item
        const group = links[cur].dataset.group || id;
        for (const a of links) {
          if (a.dataset.group) a.toggleAttribute("data-open", a.dataset.group === group);
          else a.toggleAttribute("data-group-active", a.dataset.spy === group && id !== group);
        }
      }
      if (!scrolled) return;
      const keep = id === "classes" && location.hash.startsWith("#cls-"); // the selected class in the address is more important
      const want = id === "top" ? "" : `#${id}`;
      if (!keep && location.hash !== want) history.replaceState(null, "", want || location.pathname + location.search);
    };
    addEventListener(
      "scroll",
      () => {
        scrolled = true;
        if (queued) return;
        queued = true;
        requestAnimationFrame(update);
      },
      { passive: true },
    );
    update();
  }

  // ------------------------------------------------------------------ classes: crests, skills, video
  // Tabs following WAI-ARIA (arrows, Home/End). The selected skill plays its clip video/skills/<id>, no clip —
  // the class clip video/classes/<class>, none of that either — the cover or a placeholder with a crest. Video without sound, only while the showcase
  // is on screen; with "reduce motion" it does not start by itself — the "Watch" button.
  const picker = $(".class-picker");
  if (picker) {
    const tabs = $$("[role=tab]", picker);
    const panels = tabs.map((t) => document.getElementById(t.getAttribute("aria-controls")));
    const calm = matchMedia("(prefers-reduced-motion: reduce)");
    const missing = new Set(); // addresses without a file: do not ask a second time
    let current = 0;
    // all scenes in one cell (the showcase height follows the longest class, the page does not jump), the selected one is visible
    const panelsBox = panels[0].parentElement;
    panelsBox.dataset.stacked = "";
    // in each class one description is visible from the start (otherwise on a narrow screen the longest scene is the one with all descriptions)
    for (const panel of panels) {
      const first = $(".skill-detail", panel);
      for (const d of $$(".skill-detail", panel)) d.hidden = d !== first;
    }
    // skills and talents are selected only by clicking (page scrolling does not page through them — the user's decision of 13.11);
    // the showcase is pinned for one screen
    let inView = false;

    const parts = (panel) => ({
      v: $("video", panel),
      btn: $(".video-toggle", panel),
      cap: $(".video-caption", panel),
      tiles: $$(".skill-tile, .talent-chip", panel), // skills and talents
    });
    const play = (panel) => {
      const { v } = parts(panel);
      if (!v.dataset.src || "paused" in v.dataset || calm.matches || !inView) return;
      v.play().catch(() => {});
    };
    // caption: the skill name or "Class overview"
    function caption(panel) {
      const { v, cap, tiles } = parts(panel);
      const tile = tiles.find((t) => t.getAttribute("aria-pressed") === "true");
      const skill = v.dataset.src?.includes("/skills/");
      // the skill name is already on the video (the description over its bottom); the caption — only while the class overview plays
      cap.textContent = lang === "ru" ? ru("cls.overview", "Overview") : "Overview";
      cap.hidden = !("ready" in v.dataset) || skill || !tile;
    }
    // set the clip: the preferred one first, on error — the next one from the list
    function setVideo(panel, bases) {
      const { v, btn } = parts(panel);
      const base = bases.find((b) => !missing.has(b));
      if (!base) {
        v.pause();
        v.removeAttribute("data-src");
        delete v.dataset.ready;
        btn.hidden = true;
        videoLoading(v, false);
        caption(panel);
        return;
      }
      if (v.dataset.src === base) return play(panel);
      v.dataset.src = base;
      v.replaceChildren(
        Object.assign(document.createElement("source"), { src: `${base}.webm`, type: "video/webm" }),
        Object.assign(document.createElement("source"), { src: `${base}.mp4`, type: "video/mp4" }),
      );
      v.lastElementChild.addEventListener("error", () => {
        missing.add(base);
        if (v.dataset.src === base) setVideo(panel, bases);
      });
      // the clip loads only when the showcase is on screen (before that the sources are set, but preload="none" and no load())
      if (!inView) return (v.preload = "none");
      v.preload = calm.matches ? "metadata" : "auto";
      videoLoading(v, true);
      v.load();
      play(panel);
    }
    // clip skeleton: the video slot shimmers while the clip loads (like images, input.css .slot[data-loading])
    function videoLoading(v, on) {
      const slot = v.closest(".slot");
      if (!slot) return;
      slot.toggleAttribute("data-loading", on);
      slot.toggleAttribute("data-near", on);
    }
    function selectSkill(panel, tile) {
      const { tiles } = parts(panel);
      for (const t of tiles) t.setAttribute("aria-pressed", String(t === tile));
      for (const d of $$(".skill-detail", panel)) d.hidden = d.dataset.skill !== tile.dataset.skill;
      // a talent (t1…t5) does not change the video: talents have no clips of their own
      if (!tile.dataset.skill.startsWith("t")) setVideo(panel, [`video/skills/${tile.dataset.skill}`, panel.dataset.video]);
      else if (!parts(panel).v.dataset.src) setVideo(panel, [panel.dataset.video]);
      else play(panel);
      caption(panel);
    }
    function select(i, focus, user) {
      tabs.forEach((t, j) => {
        const on = j === i;
        t.setAttribute("aria-selected", String(on));
        t.tabIndex = on ? 0 : -1;
        panels[j].toggleAttribute("data-active", on);
        panels[j].inert = !on; // hidden scenes are not for the keyboard or screen readers
        if (!on) parts(panels[j]).v.pause();
        if (on) delete panels[j].dataset.skip;
        else if (skipReady) setTimeout(() => panels[j].hasAttribute("data-active") || (panels[j].dataset.skip = ""), 800);
      });
      current = i;
      if (user) history.replaceState(null, "", `#${panels[i].id}`); // a link to the class: #cls-rogue
      if (focus) tabs[i].focus();
      const panel = panels[i];
      dispatchEvent(new Event("pinrecheck"));
      const { tiles } = parts(panel);
      selectSkill(panel, tiles.find((t) => t.getAttribute("aria-pressed") === "true") || tiles[0]);
    }

    let skipReady = false;
    const skipHidden = () => {
      skipReady = false;
      panels.forEach((pn) => delete pn.dataset.skip);
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          skipReady = true;
          panels.forEach((pn) => pn.hasAttribute("data-active") || (pn.dataset.skip = ""));
        }),
      );
    };
    (document.fonts?.ready || Promise.resolve()).then(() => (window.requestIdleCallback || setTimeout)(skipHidden));
    addEventListener("langchange", skipHidden);
    for (const panel of panels) {
      const { v, btn, tiles } = parts(panel);
      v.addEventListener("loadedmetadata", () => (btn.hidden = false));
      // there is a frame — the skeleton is no longer needed
      v.addEventListener("loadeddata", () => videoLoading(v, false));
      v.addEventListener("playing", () => {
        videoLoading(v, false);
        v.dataset.ready = "";
        btn.toggleAttribute("data-playing", true);
        caption(panel);
      });
      v.addEventListener("pause", () => btn.toggleAttribute("data-playing", false));
      btn.addEventListener("click", () => {
        if (v.paused) {
          delete v.dataset.paused;
          v.play().catch(() => {});
        } else {
          v.dataset.paused = ""; // stopped by the user — do not start again on scroll
          v.pause();
        }
      });
      for (const t of tiles) t.addEventListener("click", () => selectSkill(panel, t));
    }

    tabs.forEach((t, i) => t.addEventListener("click", () => select(i, false, true)));
    picker.addEventListener("keydown", (e) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
      let next;
      if (step) next = (current + step + tabs.length) % tabs.length;
      else if (e.key === "Home") next = 0;
      else if (e.key === "End") next = tabs.length - 1;
      else return;
      e.preventDefault();
      select(next, true, true);
      tabs[next].scrollIntoView({ block: "nearest", inline: "nearest" });
    });
    new IntersectionObserver(
      ([e]) => {
        inView = e.isIntersecting;
        const { v } = parts(panels[current]);
        if (inView && v.dataset.src && v.preload === "none") {
          v.preload = calm.matches ? "metadata" : "auto";
          videoLoading(v, true);
          v.load();
        }
        if (inView) play(panels[current]);
        else parts(panels[current]).v.pause();
      },
      { threshold: 0.25 },
    ).observe(picker.parentElement);
    document.addEventListener("langchange", () => panels.forEach(caption));

    picker.hidden = false;
    // #cls-<class> in the address: open this class and scroll to the showcase (the scene itself is hidden, the browser will not scroll to it)
    const openFromHash = () => {
      const i = panels.findIndex((p) => `#${p.id}` === location.hash);
      if (i < 0) return false;
      select(i);
      // after the fonts: while they load, the page height above the showcase still changes
      document.fonts.ready.then(() => requestAnimationFrame(() => $("#classes").scrollIntoView({ behavior: "instant" })));
      return true;
    };
    addEventListener("hashchange", openFromHash);
    if (!openFromHash()) select(0);
  }

  // ------------------------------------------------------------------ start
  // the "Animations" button: visible if animation effects are off in Windows (or motion is already turned on here)
  {
    const btn = $("#motion");
    const osReduce = realMatchMedia("(prefers-reduced-motion: reduce)");
    const sync = () => {
      const on = motionRoot.dataset.motion === "on";
      btn.hidden = !osReduce.matches && !on;
      btn.setAttribute("aria-pressed", String(on));
    };
    btn?.addEventListener("click", () => {
      const on = motionRoot.dataset.motion !== "on";
      if (on) motionRoot.dataset.motion = "on";
      else delete motionRoot.dataset.motion;
      try {
        if (on) localStorage.setItem("motion", "on");
        else localStorage.removeItem("motion");
      } catch {}
      sync();
      // to everyone who watches "reduce motion" (shader, carousel…) — as if the setting had changed
      for (const fn of motionHooks) {
        try {
          fn(new Event("change"));
        } catch {}
      }
    });
    osReduce.addEventListener("change", sync);
    if (btn) sync();
  }
  // language change by the button: the text part fades out (input.css, <html data-lang-fade>), the text changes, fades in;
  // the background and the header backdrop stay. Without motion — instantly.
  let langBusy = false;
  $("#lang").addEventListener("click", () => {
    if (langBusy) return;
    const next = lang === "ru" ? "en" : "ru";
    const save = () => {
      try {
        localStorage.setItem("lang", lang);
      } catch {}
    };
    if (matchMedia("(prefers-reduced-motion: reduce)").matches) return apply(next), save();
    const root = document.documentElement;
    langBusy = true;
    root.dataset.langFade = "";
    setTimeout(() => {
      apply(next);
      save();
      requestAnimationFrame(() => {
        delete root.dataset.langFade;
        langBusy = false;
      });
    }, 160);
  });
  apply(initialLang());
  // "Verify the file": expand the answer when following the anchor
  const openSha = () => location.hash === "#sha" && ($("#sha").open = true);
  addEventListener("hashchange", openSha);
  openSha();

  // community links without an address (href="#", addresses are in build_seo.py) — a click does nothing instead of jumping to the top
  for (const l of $$('a[data-link][href="#"]')) {
    l.setAttribute("aria-disabled", "true");
    l.addEventListener("click", (e) => e.preventDefault());
  }

  // ------------------------------------------------------------------ splash screen
  // <html data-boot> (in the markup) shows the splash screen with the "D" tile (input.css .boot). Removed once the font has loaded
  // and the first frame has passed, but no later than 1.2 s; without a script the splash screen leaves by itself (CSS).
  {
    const root = document.documentElement;
    const done = () => {
      if (!("boot" in root.dataset)) return;
      requestAnimationFrame(() => (delete root.dataset.boot, performance.mark("boot-done")));
      // after dissolving — remove from the page: otherwise the invisible pulse and bar spin forever
      setTimeout(() => {
        const boot = $(".boot");
        if (boot) boot.hidden = true;
      }, 700);
    };
    (document.fonts?.ready || Promise.resolve()).then(done);
    setTimeout(done, 1200);
  }
})();
