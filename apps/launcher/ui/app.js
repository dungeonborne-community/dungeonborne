"use strict";
// Dungeonborne launcher UI in Windows 11 style. Contract with the launcher: api.d.ts; texts: i18n.js.
// No markup here: index.html holds the layout and <template>s; this file fills texts and sets data-state / data-tone,
// the look of every state is written in index.html with Tailwind data-[…]: variants.

const { invoke } = window.__TAURI__.core;

// Appearance (Settings): "Torch", "Crypt", "Blood" — input.css, [data-theme]. The last choice is also kept in localStorage,
// so the window opens in it right away instead of flashing the default theme until the settings load.
const THEMES = ["torch", "crypt", "blood"];
// Theme change with easing, as on the site: data-from → data-to on <html>, colours are mixed in OKLCH by --tp (input.css). --tp
// is driven by this code every frame for 0.6 s (ease-out), not a CSS animation: during a CSS animation Chrome does not repaint some
// colours (links got stuck on the site). On first show — immediately.
const THEME_MS = 600;
const easeOut = (t) => 1 - (1 - t) ** 5; // like cubic-bezier(0.22, 1, 0.36, 1): fast start, soft fade-out
let themeFrame = 0;
function applyTheme(name) {
  const theme = THEMES.includes(name) ? name : "torch";
  const root = document.documentElement;
  const from = root.dataset.theme;
  if (from === theme) return theme;
  cancelAnimationFrame(themeFrame);
  root.dataset.theme = theme;
  // a hidden window draws no frames (requestAnimationFrame is paused) — there the theme changes at once, otherwise it would stay on the old one.
  // "Reduce motion" does not block it: this is a colour blend, not motion (the user has animation effects off in Windows)
  if (from && !document.hidden) {
    // a change in the middle of another one — from what is visible now to the new one
    root.dataset.from = root.dataset.to && root.dataset.to !== theme ? root.dataset.to : from;
    root.dataset.to = theme;
    root.dataset.switching = "";
    const t0 = performance.now();
    const step = (now) => {
      const t = Math.min(1, (now - t0) / THEME_MS);
      root.style.setProperty("--tp", easeOut(t).toFixed(4));
      if (t < 1) return (themeFrame = requestAnimationFrame(step));
      delete root.dataset.from;
      delete root.dataset.to;
      delete root.dataset.switching;
      root.style.removeProperty("--tp");
    };
    root.style.setProperty("--tp", "0");
    themeFrame = requestAnimationFrame(step);
  }
  try {
    localStorage.setItem("theme", theme);
  } catch {
    // no storage: the setting still comes with the state
  }
  return theme;
}
try {
  applyTheme(localStorage.getItem("theme"));
} catch {
  applyTheme("torch");
}

// <html data-calm>: the window is not in use (not focused — the game, another window; minimised) — the CSS decor stops (sparks, input.css),
// so does the atmos.js background. As on the site: constant CSS animations would otherwise spin the GPU for nothing.
{
  const calm = () => document.documentElement.toggleAttribute("data-calm", document.hidden || !document.hasFocus());
  addEventListener("focus", calm);
  addEventListener("blur", calm);
  document.addEventListener("visibilitychange", calm);
  calm();
}
const { listen } = window.__TAURI__.event;

const STEPS = ["check", "mods", "profile", "relay", "game", "cleanup"];

// Cloudflare WARP: free, fixes the provider cutting connections to the servers (netblock)
const WARP_URL = "https://one.one.one.one/";

const S = {
  state: null,
  selected: null,
  page: "server", // "server" | "settings"
  status: {},
  installs: [null, null],
  mods: {},
  optional: {},
  login: {}, // id -> { code } while the browser login runs
  loginError: {},
  run: {}, // install -> { steps, running, error }
  lastRun: null, // install shown in the progress card
  dl: {},
  log: [],
  tab: "launch",
  update: null,
  pick: null, // install chosen for Play
  open: { mods: false, log: false },
  lobby: null, // { server, view, offset } while the selected server has a lobby
  lobbyTab: 1,
  lobbyTabSize: null, // party size of the fill tab looked at: its tab may go away (battle over), the next one of that size takes over
  lobbyTabLane: 0, // and its solo lane (difficulty): the next one of the same lane takes over
  lobbyMine: null, // instance the player is in, to follow it only when it changes
  lobbySig: "",
  netHint: {}, // server id -> {kind, detail, tunnel}: the provider cuts the connection (event net_block), until closed
  view: "overview", // server page tab: "overview" | "notes"
  notes: {}, // id -> { loading } | { data } (data null = the server publishes none) | { error }
  support: {}, // id -> Support | null (support.rs): this month's server goal; null — nothing to show
  splash: null, // { id, latest } while the full-window "what's new" is open
  splashTried: new Set(), // "id|latest" already offered in this run, so a failed fetch does not loop
  bug: null, // "Report a bug" form: { id, loggedIn, sending, report, posted, url, late }
};

// ------------------------------------------------------------------------------------------------ helpers

const $ = (id) => document.getElementById(id);
const show = (el, on, display = "") => {
  el.classList.toggle("hidden", !on);
  if (display) el.classList.toggle(display, on);
};
/** data-<name>="value"; value null/false removes it (Tailwind: data-[name]: / data-[name=value]:) */
const mark = (el, name, value) => (value === null || value === false || value === undefined ? delete el.dataset[name] : (el.dataset[name] = value === true ? "" : value));

/** clone a <template>: root element + its [data-ref] children */
function clone(id) {
  const root = $(id).content.firstElementChild.cloneNode(true);
  const ref = { root };
  for (const el of root.querySelectorAll("[data-ref]")) ref[el.dataset.ref] = el;
  return ref;
}

const lang = () => (S.state?.settings.lang === "en" ? "en" : "ru");

function t(key, vars) {
  let s = window.I18N[lang()][key] ?? window.I18N.ru[key] ?? key;
  for (const [k, v] of Object.entries(vars || {})) s = s.replaceAll(`{${k}}`, String(v));
  return s;
}

/** A server's own text in the launcher's language: `map` is its `<field>_i18n` ({ru, en}), `text` the main language */
function serverText(text, map) {
  return (map && map[lang()]) || text;
}

function errText(e) {
  if (e && typeof e === "object" && e.code) {
    const known = window.I18N[lang()][`err.${e.code}`];
    // The server's own message is in the server's language: with a text of ours it would only repeat it untranslated.
    // A denied login keeps it: it is the provider's reason, which we have no text for.
    const serverText = e.code.startsWith("api.") && known && e.code !== "api.denied";
    return e.detail && !serverText ? `${known ?? e.code} (${e.detail})` : (known ?? e.code);
  }
  return String(e);
}

function bytes(n) {
  if (n < 1024) return `${n} B`;
  if (n < 1 << 20) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1 << 20)).toFixed(1)} MB`;
}

const server = (id = S.selected) => S.state?.servers.find((s) => s.manifest.id === id);

/** `action`: { label, run } — a button in the toast (it then stays longer) */
function toast(text, tone = "critical", action = null) {
  const r = clone("tpl-toast");
  r.root.dataset.tone = tone;
  r.text.textContent = text;
  if (action) {
    r.action.textContent = action.label;
    r.action.onclick = action.run;
    show(r.action, true);
  }
  $("toasts").append(r.root);
  // slides aside (enter-side, 150 ms), then is removed
  setTimeout(() => {
    r.root.dataset.leaving = "";
    setTimeout(() => r.root.remove(), 200);
  }, action ? 20000 : tone === "critical" ? 9000 : 4000);
}

/** Appear again (Fluent, like .enter): the block is already on screen but shows something else — e.g. another server */
function replayEnter(el) {
  const still = matchMedia("(prefers-reduced-motion: reduce)").matches;
  el.animate(still ? [{ opacity: 0 }, { opacity: 1 }] : [{ opacity: 0, translate: "0 6px" }, { opacity: 1, translate: "0 0" }], {
    duration: 250,
    easing: "cubic-bezier(0.1, 0.9, 0.2, 1)",
  });
}

async function call(cmd, args) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    toast(errText(e));
    throw e;
  }
}

/** `yes` / `no`: button texts instead of "Confirm" / "Cancel" */
function confirmBox(title, text, yes = null, no = null) {
  return new Promise((resolve) => {
    $("confirm-title").textContent = title;
    $("confirm-text").textContent = text;
    $("confirm-yes").textContent = yes ?? t("common.confirm");
    $("confirm-no").textContent = no ?? t("common.cancel");
    show($("dlg-confirm"), true, "flex");
    const done = (v) => {
      show($("dlg-confirm"), false, "flex");
      $("confirm-yes").onclick = $("confirm-no").onclick = null;
      resolve(v);
    };
    $("confirm-yes").onclick = () => done(true);
    $("confirm-no").onclick = () => done(false);
  });
}

function pushLog(entry) {
  S.log.push({ at: Date.now(), ...entry });
  if (S.log.length > 500) S.log.splice(0, S.log.length - 500);
  renderLog();
}

// ------------------------------------------------------------------------------------------------ data

async function reload() {
  S.state = await call("get_state");
  applyTheme(S.state.settings.theme);
  if (!server()) S.selected = S.state.servers[0]?.manifest.id ?? null;
  S.pick = 0; // one game install since 1.6.1
  render();
}

async function loadStatus(id) {
  try {
    S.status[id] = await invoke("server_status", { id });
  } catch (e) {
    S.status[id] = { error: e };
  }
  renderNav();
  if (id === S.selected) renderServer();
}

async function loadInstalls() {
  await Promise.all(
    [0].map(async (i) => {
      try {
        S.installs[i] = await invoke("check_install", { index: i });
      } catch (e) {
        S.installs[i] = { error: e };
      }
    }),
  );
  render();
}

/** Sets the game folder and checks it again */
async function setGameFolder(path) {
  const installs = [...S.state.settings.installs];
  installs[0] = path;
  await saveSettings({ installs });
  await loadInstalls();
}

/** "Browse…": the system folder picker. Closed without a choice — nothing; a folder without the game — an error toast */
async function browseGame() {
  const path = await call("pick_game_folder", { title: t("install.pick_title") }).catch(() => null);
  if (!path) return;
  await setGameFolder(path);
  toast(t("install.set", { path }), "ok");
}

/** Finds the game by itself (a running game, Steam's libraries). `quiet` — on start: no toast when nothing is found */
async function findGame(quiet) {
  const path = await call("find_game").catch(() => null);
  if (!path) {
    if (!quiet) toast(t("install.not_found"));
    return;
  }
  if (path !== S.state.settings.installs[0]) await setGameFolder(path);
  toast(t("install.found", { path }), "ok");
}

async function loadMods(id) {
  try {
    S.mods[id] = await invoke("server_mods", { id });
  } catch {
    S.mods[id] = [];
  }
  if (id === S.selected) renderServer();
}

async function checkUpdate() {
  try {
    S.update = await invoke("update_check");
  } catch {
    S.update = null;
  }
  renderHeader();
}

// ------------------------------------------------------------------------------------------------ render

function render() {
  if (!S.state) return;
  renderTexts();
  renderHeader();
  renderNav();
  show($("page-empty"), S.page === "server" && !server(), "flex");
  show($("page-server"), S.page === "server" && !!server());
  show($("page-settings"), S.page === "settings");
  if (S.page === "server") renderServer();
  else renderSettings();
  renderLog();
}

function renderTexts() {
  document.documentElement.lang = lang();
  for (const el of document.querySelectorAll("[data-t]")) el.textContent = t(el.dataset.t);
  $("lang-text").textContent = lang() === "ru" ? "EN" : "RU";
  $("refresh").title = t("servers.refresh");
  $("add-url").placeholder = t("servers.add_placeholder");
  $("bug-title").placeholder = t("bug.name_placeholder");
  $("bug-desc").placeholder = t("bug.description_placeholder");
  if (S.bug) bugRender();
}

/** Some server still sends the game to its real domains (no `relay.no_hosts` in its manifest), or the player plays
 *  through hosts anyway (`force_hosts`, 1.7.2): hosts lines are needed */
const hostsNeeded = () =>
  !!S.state.settings.force_hosts || S.state.servers.some((s) => s.manifest.relay && !s.manifest.relay.no_hosts);
const someNoHosts = () => S.state.servers.some((s) => s.manifest.relay?.no_hosts);

/** Set up hosts after the player agreed (yellow UAC window explained first); true when it is in place */
async function setupHosts(title, text) {
  if (!(await confirmBox(title, `${text}\n\n${t("hosts.confirm_text")}`))) return false;
  await call("hosts_install");
  await reload();
  return !!S.state.hosts_installed;
}

function renderHeader() {
  const ok = S.state.hosts_installed;
  show($("hosts-pill"), hostsNeeded(), "inline-flex");
  mark($("hosts-pill"), "state", ok ? "ok" : "missing");
  $("hosts-text").textContent = t(ok ? "hosts.ok" : "hosts.missing");
  show($("dev-pill"), S.state.settings.dev_mode, "inline-flex");
  show($("update"), !!S.update, "flex");
  if (S.update) $("update-text").textContent = t("update.available", S.update);
}

function statusOf(id) {
  const st = S.status[id];
  if (!st) return { state: null, text: "…" };
  if (st.error) return { state: "offline", text: t("card.offline") };
  const parts = [];
  if (st.online != null) parts.push(t("card.online", { online: st.online, capacity: st.capacity ?? "?" }));
  if (st.ping_ms != null) parts.push(t("card.ping", { ms: st.ping_ms }));
  return { state: "online", text: parts.join(" · ") };
}

function renderNav() {
  const list = $("servers");
  list.replaceChildren(
    ...(S.state.servers ?? []).map((s) => {
      const r = clone("tpl-server");
      const st = statusOf(s.manifest.id);
      mark(r.root, "active", S.page === "server" && s.manifest.id === S.selected);
      mark(r.dot, "state", st.state);
      r.name.textContent = s.manifest.name;
      r.status.textContent = st.text;
      show(r.warn, s.key_changed);
      r.warn.title = t("card.key_changed");
      r.root.onclick = () => select(s.manifest.id);
      return r.root;
    }),
  );
  mark($("nav-settings"), "active", S.page === "settings");
}

/// why Play is off for install i (null = can play)
function blocked(i, s) {
  const info = S.installs[i];
  if (s.key_changed) return t("err.key_changed");
  if (!S.state.settings.installs[i]) return t("card.install_unset");
  if (!info) return "…";
  if (info.error) return errText(info.error);
  if (!s.manifest.game.exe_sha256) return t("err.exe_unspecified");
  if (info.exe_sha256 !== s.manifest.game.exe_sha256) return t("card.install_bad");
  if (info.running) return t("card.install_running");
  if (S.run[i]?.running) return t("steps.running");
  if (!S.state.settings.dev_mode && !s.logged_in) return t("err.login_required");
  return null;
}

function renderServer() {
  const s = server();
  if (!s || S.page !== "server") return;
  const m = s.manifest;
  const st = S.status[m.id];

  // key warning
  show($("key-bar"), s.key_changed, "flex");
  $("key-old").textContent = t("card.key_old", { key: s.trusted_key || t("card.key_none") });
  $("key-new").textContent = t("card.key_new", { key: m.key || t("card.key_none") });

  // hero
  $("srv-name").textContent = m.name;
  const so = statusOf(m.id);
  mark($("srv-status"), "state", so.state);
  $("srv-status-text").textContent = so.text;
  show($("srv-region"), !!m.region, "inline-flex");
  $("srv-region-text").textContent = m.region ?? "";
  $("srv-tags").replaceChildren(
    ...(m.mode?.tags ?? []).map((tag) => {
      const r = clone("tpl-tag");
      r.root.textContent = tag;
      return r.root;
    }),
  );
  $("srv-desc").textContent = serverText(m.description, m.description_i18n);
  const motd = st && !st.error ? serverText(st.motd, st.motd_i18n) : null;
  show($("srv-motd"), !!motd, "flex");
  $("srv-motd-text").textContent = motd ?? "";
  $("srv-id").textContent = m.id;
  show($("srv-site"), !!m.website, "inline-flex");
  // `contact` of the manifest: a link (Discord invite) — a button; anything else is not shown
  const contact = contactUrl(m);
  show($("srv-contact"), !!contact, "inline-flex");
  $("srv-contact-text").textContent = contact && /(^|\.)discord(app)?\.(gg|com)$/i.test(new URL(contact).hostname) ? "Discord" : t("card.contact");
  show($("srv-bug"), !!m.auth, "inline-flex");
  renderSupport(s);

  // Play: button + why it is off
  const why = S.state.settings.installs[0] ? blocked(S.pick, s) : t("err.install_not_set");
  $("play").disabled = !!why;
  mark($("play"), "boot", S.state.boot_install === S.pick);
  const note = $("play-note");
  if (why) [note.textContent, note.dataset.tone] = [why, "off"];
  else if (S.state.settings.dev_mode) [note.textContent, note.dataset.tone] = [t("card.dev_play"), "dev"];
  else if (S.state.boot_install === S.pick) [note.textContent, note.dataset.tone] = [t("card.from_steam"), "boot"];
  else [note.textContent, note.dataset.tone] = ["", "off"];

  const hint = S.netHint[m.id];
  show($("net-bar"), !!hint, "flex");
  show($("net-on"), !!hint?.tunnel, "inline-flex");
  show($("net-tunnel-text"), !!hint?.tunnel);
  renderTabs(s);
  renderLobby();
  renderRun();
  renderAccount(s);
  renderMods(s);
  renderInstalls(s);
}

// ------------------------------------------------------------------------------------------------ patch notes

const NOTE_ICONS = { new: "\uE734", fixed: "\uE90F", balance: "\uE9D2" };

/** the newest entry the server has, and whether the player has not opened it yet */
function notesNew(s) {
  const latest = s.manifest.patch_notes?.latest;
  return !!latest && S.state.settings.notes_seen?.[s.manifest.id] !== latest;
}

function renderTabs(s) {
  for (const b of document.querySelectorAll(".srv-tab")) mark(b, "active", b.dataset.view === S.view);
  show($("notes-new"), notesNew(s), "flex");
  show($("srv-overview"), S.view === "overview");
  show($("srv-notes"), S.view === "notes");
  if (S.view === "notes") renderNotes(s);
}

// ------------------------------------------------------------------------------------------------ project support
// Owner's decision 16.11: server fundraising — a bar under the server card, in the project's purple. Data — JSON from
// manifest.support.url (one source shared with Discord and the site), through Rust (support.rs): no CORS needed. Polled when a
// server is selected and every 12 minutes. No close button (owner 16.11: the bar is always visible while there is a goal). Under the bar — sub-bars
// per platform (goals[0].sources). Do not duplicate the button anywhere else: do not push it.
const SUPPORT_POLL_MS = 12 * 60 * 1000;

// 1.7.6 had a cross that hid the strip till next month: forget that, the strip is back for everyone
try {
  localStorage.removeItem("support-hidden");
} catch {
  // no storage: nothing was hidden either
}

async function loadSupport(id) {
  let data = null;
  try {
    data = await invoke("server_support", { id });
  } catch {
    // no source, network, a bad JSON: the strip just isn't shown
  }
  S.support[id] = data;
  if (id === S.selected) renderSupport(server(id));
}

/** $90, $12.50: no cents when whole */
function money(n, currency) {
  const whole = Number.isInteger(n);
  const v = whole ? String(n) : n.toFixed(2);
  if (currency === "USD") return `$${v}`;
  try {
    return new Intl.NumberFormat(lang(), { style: "currency", currency, minimumFractionDigits: whole ? 0 : 2, maximumFractionDigits: whole ? 0 : 2 }).format(n);
  } catch {
    return `${v} ${currency}`;
  }
}

function renderSupport(s) {
  const id = s?.manifest.id;
  const sp = id ? S.support[id] : null;
  const on = !!sp;
  show($("support"), on, "flex");
  if (!on) return;
  const { raised, target } = sp.goal;
  const pct = Math.max(0, Math.round((raised / target) * 100));
  // the month is paid (bot 16.11: covered): "✅ Month covered"; the percent may go over 100, the bar stops at full
  const sums = { raised: money(raised, sp.currency), target: money(target, sp.currency) };
  $("support-text").textContent = t(sp.goal.covered ? "support.covered" : "support.month", sums);
  $("support-pct").textContent = `${pct}%`;
  $("support-fill").style.width = `${Math.min(100, pct)}%`;
  // per platform: Patreon, Boosty (in this order; a platform the bot doesn't send is not shown)
  const SOURCES = [["patreon", "Patreon"], ["boosty", "Boosty"]];
  const parts = SOURCES.filter(([k]) => typeof sp.goal.sources?.[k] === "number");
  show($("support-sources"), parts.length > 0, "grid");
  $("support-sources").replaceChildren(
    ...parts.map(([k, name]) => {
      const r = clone("tpl-support-source");
      const v = sp.goal.sources[k];
      r.name.textContent = name;
      r.sum.textContent = money(v, sp.currency);
      r.fill.style.width = `${Math.min(100, Math.max(0, (v / target) * 100))}%`;
      return r.root;
    }),
  );
  // two buttons, Patreon and Boosty (owner 16.11). No link in the data (platform under review) — the button is disabled,
  // "soon"; once the link appears in the bot's data the button works without a new launcher
  for (const k of ["patreon", "boosty"]) {
    const b = $(`support-${k}`);
    const url = sp.links.find(([name]) => name === k)?.[1];
    b.disabled = !url;
    b.title = url ? "" : t("support.soon_hint");
    show(b.querySelector("[data-ref=soon]"), !url, "inline-block");
    b.onclick = url ? () => call("open_link", { url }).catch(() => {}) : null;
  }
}

async function loadNotes(id) {
  S.notes[id] = { loading: true };
  renderServer();
  try {
    const data = await invoke("server_notes", { id });
    S.notes[id] = { data };
    if (data?.latest) {
      // seen once shown: the "new" mark goes away for this entry
      await invoke("notes_seen", { id, latest: data.latest });
      S.state.settings.notes_seen = { ...S.state.settings.notes_seen, [id]: data.latest };
    }
  } catch (e) {
    S.notes[id] = { error: e };
  }
  if (id === S.selected) renderServer();
}

function noteDate(d) {
  const at = new Date(`${d}T00:00:00`);
  return Number.isNaN(at.getTime()) ? d : at.toLocaleDateString(lang(), { day: "numeric", month: "long", year: "numeric" });
}

function noteMessage(text, retry) {
  const box = document.createElement("div");
  box.className = "card flex items-center gap-3 px-6 py-5 text-fg-2";
  const icon = document.createElement("span");
  icon.className = "ico text-xl text-fg-3";
  icon.textContent = retry ? "\uEA39" : "\uE946";
  const p = document.createElement("span");
  p.className = "flex-1";
  p.textContent = text;
  box.append(icon, p);
  if (retry) {
    const b = document.createElement("button");
    b.className = "btn-std";
    b.textContent = t("notes.retry");
    b.onclick = retry;
    box.append(b);
  }
  return box;
}

function renderNotes(s) {
  const id = s.manifest.id;
  const st = S.notes[id];
  const box = $("srv-notes");
  if (!st || st.loading) return box.replaceChildren(noteMessage(t("notes.loading")));
  if (st.error) return box.replaceChildren(noteMessage(`${t("notes.error")}: ${errText(st.error)}`, () => loadNotes(id)));
  if (!st.data || !st.data.updates.length) return box.replaceChildren(noteMessage(t("notes.none")));
  const head = [];
  if (st.data.season?.name) {
    const p = document.createElement("p");
    p.className = "px-1 text-fg-2";
    p.textContent = t("notes.season", { name: st.data.season.name });
    head.push(p);
  }
  box.replaceChildren(...head, ...st.data.updates.map(noteCard));
}

/** one update as a card: title, version, date, sections with their items (plain text) */
function noteCard(u) {
  const r = clone("tpl-note");
  r.title.textContent = u.title;
  r.ver.textContent = u.version;
  show(r.ver, !!u.version && u.version !== u.title, "inline-flex");
  r.date.textContent = noteDate(u.date);
  // the server has no translation of this entry: say so instead of switching languages silently
  r.orig.textContent = u.lang && u.lang !== lang() ? t("notes.untranslated") : "";
  show(r.orig, !!r.orig.textContent, "inline-flex");
  r.sections.replaceChildren(
    ...u.sections.map((sec) => {
      const x = clone("tpl-note-section");
      x.head.dataset.kind = sec.kind;
      x.icon.textContent = NOTE_ICONS[sec.kind] ?? "";
      x.name.textContent = t(`notes.${sec.kind}`);
      x.items.replaceChildren(
        ...sec.items.map((text) => {
          const li = document.createElement("li");
          li.className = "whitespace-pre-line";
          li.textContent = text;
          return li;
        }),
      );
      return x.root;
    }),
  );
  return r.root;
}

/** The first time a server has a new entry (after an update), its notes take the whole window until the player closes
 *  them. Shows what is new since the entry the player saw last; the very first time — only the newest entry. */
async function maybeSplash() {
  const s = server();
  if (!s || S.page !== "server" || S.splash || !notesNew(s)) return;
  const id = s.manifest.id;
  const key = `${id}|${s.manifest.patch_notes.latest}`;
  if (S.splashTried.has(key)) return;
  S.splashTried.add(key);
  let data;
  try {
    data = await invoke("server_notes", { id });
  } catch {
    return; // the tab still says "new"; nothing to show full-window
  }
  if (!data?.updates.length || id !== S.selected || S.splash) return;
  S.notes[id] = { data };
  const seen = S.state.settings.notes_seen?.[id];
  const at = data.updates.findIndex((u) => u.id === seen);
  const fresh = at > 0 ? data.updates.slice(0, at) : at === 0 ? [] : data.updates.slice(0, 1);
  if (!fresh.length) return;
  S.splash = { id, latest: data.latest };
  $("splash-title").textContent = s.manifest.name;
  $("splash-season").textContent = data.season?.name ? t("notes.season", { name: data.season.name }) : "";
  show($("splash-season"), !!data.season?.name);
  $("splash-list").replaceChildren(...fresh.map(noteCard));
  $("splash-scroll").scrollTop = 0;
  show($("notes-splash"), true, "flex");
  $("splash-ok").focus();
}

async function closeSplash(toTab) {
  const sp = S.splash;
  if (!sp) return;
  S.splash = null;
  show($("notes-splash"), false, "flex");
  try {
    await invoke("notes_seen", { id: sp.id, latest: sp.latest });
    S.state.settings.notes_seen = { ...S.state.settings.notes_seen, [sp.id]: sp.latest };
  } catch (e) {
    toast(errText(e));
  }
  if (toTab && sp.id === S.selected) S.view = "notes";
  render();
}

// ------------------------------------------------------------------------------------------------ lobby

const LOBBY_POLL_MS = 2000;
const LOBBY_OFF_POLL_MS = 30000;

const lobbyView = () => (S.lobby && S.lobby.server === S.selected && S.page === "server" ? S.lobby.view : null);

function setLobby(id, view) {
  S.lobby = { server: id, view, offset: view.now ? view.now - Date.now() / 1000 : 0 };
  const mine = view.me?.instance ?? null;
  if (mine !== S.lobbyMine) {
    S.lobbyMine = mine;
    if (mine) S.lobbyTab = mine;
  }
  // fill ids go round (1…99) and a finished battle's tab goes away: stay on the same party size, not on a gone id
  const tabs = view.instances ?? [];
  if (tabs.length && !tabs.some((i) => i.id === S.lobbyTab)) {
    const same =
      tabs.find((i) => i.kind === "fill" && i.party_size === S.lobbyTabSize && (i.lane ?? 0) === S.lobbyTabLane) ??
      tabs.find((i) => i.kind === "fill" && i.party_size === S.lobbyTabSize);
    S.lobbyTab = (same ?? tabs[0]).id;
  }
  renderLobby();
}

async function pollLobby() {
  const s = server();
  let wait = LOBBY_OFF_POLL_MS;
  if (s?.logged_in && s.manifest.auth) {
    const id = s.manifest.id;
    try {
      const view = await invoke("lobby_state", { id });
      setLobby(id, view);
      if (view.enabled) wait = LOBBY_POLL_MS;
    } catch {
      S.lobby = null; // old backend without the lobby, or the network: the card just goes away
      renderLobby();
      wait = LOBBY_POLL_MS * 5;
    }
  } else if (S.lobby) {
    S.lobby = null;
    renderLobby();
  }
  setTimeout(pollLobby, wait);
}

async function lobbyAct(cmd, args = {}) {
  const id = S.selected;
  setLobby(id, await call(cmd, { id, ...args }));
}

function lobbyOption(name, count, { mine, win, enabled, onclick, title }) {
  const r = clone("tpl-lobby-option");
  r.name.textContent = name;
  r.name.title = title || name;
  r.count.textContent = count ? String(count) : "";
  mark(r.root, "mine", !!mine);
  mark(r.root, "win", !!win);
  r.root.disabled = !enabled;
  if (enabled) r.root.onclick = onclick;
  return r.root;
}

function lobbyTimer() {
  const view = lobbyView();
  const inst = view?.instances?.find((i) => i.id === S.lobbyTab);
  const left = inst?.ends_at ? Math.max(0, Math.ceil(inst.ends_at - (Date.now() / 1000 + S.lobby.offset))) : null;
  $("lobby-timer").textContent = left === null ? "" : `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
}

// map matchmaking rules from lobby_state.rules (backend, 08.11): skirmish, rarity limit, "challenge mode"
function lobbyMapRule(view, id) {
  const r = view.rules?.[id];
  if (!r) return "";
  const parts = [t(r.skirmish ? "lobby.rule.skirmish" : "lobby.rule.strict")];
  if (r.gear === "epic") parts.push(t("lobby.rule.gear_epic"));
  if (r.gear === "green") parts.push(t("lobby.rule.gear_green"));
  if (r.challenger) parts.push(t("lobby.rule.challenger"));
  return parts.join(" ");
}

// rules block: a chosen map — its rules; before the choice — maps with identical rules on one line
function lobbyRulesText(view, inst) {
  const min = view.min_players ?? inst.min_players;
  const tail = min > 1 ? `\n${t("lobby.rule.min", { min })}` : "";
  if (inst.level_id) return `${t(`lobby.map.${inst.level_id}`)}: ${lobbyMapRule(view, inst.level_id)}${tail}`;
  const groups = new Map();
  for (const id of view.maps) {
    const text = lobbyMapRule(view, id);
    if (!text) continue;
    if (!groups.has(text)) groups.set(text, []);
    groups.get(text).push(t(`lobby.map.${id}`));
  }
  return [...groups].map(([text, names]) => `${names.join(", ")}: ${text}`).join("\n") + tail;
}

function renderLobby() {
  const view = lobbyView();
  const on = !!view?.enabled;
  show($("maint-bar"), !!view?.maintenance, "flex");
  $("maint-text").textContent = view?.maintenance ? serverText(view.maintenance, view.maintenance_i18n) : "";
  show($("lobby"), on);
  if (!on) {
    S.lobbySig = "";
    return;
  }
  lobbyTimer();
  // a poll that changed nothing must not rebuild the buttons under the cursor (ends_at of the hall moves by a second)
  const sig = JSON.stringify([lang(), S.lobbyTab, view.maintenance, view.me, view.instances.map((i) => ({ ...i, ends_at: 0 }))]);
  if (sig === S.lobbySig) return;
  S.lobbySig = sig;

  const me = view.me;
  // fill instances (backend 11.11): "Solo" / "Duo" / "Trio", numbered only when one size has several ("Solo 1", "Solo 2")
  const bySize = {};
  for (const i of view.instances) if (i.kind === "fill") bySize[i.party_size] = (bySize[i.party_size] ?? 0) + 1;
  const seen = {};
  const tabName = (i) => {
    if (i.kind === "clotho") return t("lobby.clotho.short");
    if (i.kind !== "fill") return t("lobby.tab", { n: i.id });
    // solo lanes (backend 13.11): one difficulty per instance, "Solo · Easy"
    if (i.lane > 0 && i.difficulty) return `${t(`lobby.size.${i.party_size}`)} · ${t(`lobby.diff.${i.difficulty}`)}`;
    seen[i.party_size] = (seen[i.party_size] ?? 0) + 1;
    const size = t(`lobby.size.${i.party_size}`);
    return bySize[i.party_size] > 1 ? `${size} ${seen[i.party_size]}` : size;
  };
  $("lobby-tabs").replaceChildren(
    ...view.instances.map((i) => {
      const r = clone("tpl-lobby-tab");
      const clotho = i.kind === "clotho";
      r.name.textContent = tabName(i);
      r.root.title = r.name.textContent; // many tabs on a narrow window cut the name
      r.state.textContent = clotho && i.phase !== "battle" && i.phase !== "starting"
        ? t("lobby.clotho.count", { n: i.players, min: i.min_players })
        : i.kind === "fill" && i.phase === "gather"
          ? t("lobby.fill.count", { n: i.players, seats: i.seats })
          : t(`lobby.phase.${i.phase}`);
      show(r.mine, me.instance === i.id, "inline-block");
      mark(r.root, "active", i.id === S.lobbyTab);
      // choosing a tab is joining it; a tab in battle can only be looked at. Clotho is only looked at: joining may open a
      // run for a trial stone, so it takes the "Join" button
      r.root.onclick = () => {
        S.lobbyTab = i.id;
        renderLobby();
        // "closed" (backend 13.11: tabs open one after another, every 10–20 min) — also view only, until the timer
        if (!clotho && !view.maintenance && me.instance !== i.id && i.phase !== "battle" && i.phase !== "closed") lobbyAct("lobby_join", { instance: i.id }).catch(() => {});
      };
      return r.root;
    }),
  );

  const inst = view.instances.find((i) => i.id === S.lobbyTab) ?? view.instances[0];
  const mine = me.instance === inst.id;
  const clotho = inst.kind === "clotho";
  const fill = inst.kind === "fill";
  S.lobbyTabSize = fill ? inst.party_size : null;
  // a solo lane has its map and difficulty set: no map vote
  const lane = fill && inst.lane > 0;
  S.lobbyTabLane = lane ? inst.lane : 0;
  const hint =fill ? "lobby.fill.hint" : "lobby.hint";
  $("lobby-hint").dataset.t = hint;
  $("lobby-hint").textContent = t(hint);
  show($("lobby-hint"), !me.instance && !view.maintenance && !clotho && inst.phase !== "closed");
  show($("clotho"), clotho);
  show($("lobby-votes"), !clotho);
  show($("lobby-sizes-box"), !fill, "block");
  show($("lobby-maps-box"), !lane, "block");
  if (clotho) {
    renderClotho(view, inst, mine);
    show($("lobby-start"), false);
    show($("lobby-leave"), mine && (inst.phase === "idle" || inst.phase === "gather"), "inline-flex");
    return;
  }
  // the size is the fill tab's own name: no "Party: Solo." after its text
  const chosen = inst.party_size && !fill ? ` ${t("lobby.chosen", { size: t(`lobby.size.${inst.party_size}`) })}` : "";
  let map = inst.level_id ? ` ${t("lobby.chosen_map", { map: t(`lobby.map.${inst.level_id}`) })}` : "";
  if (lane && inst.difficulty) map += ` ${t("lobby.chosen_diff", { diff: t(`lobby.diff.${inst.difficulty}`) })}`;
  // in a fill gather `seats` is how many start it, said in the text itself; the hall count comes after the start
  const hall = inst.seats && inst.phase !== "closed" && !(fill && (inst.phase === "idle" || inst.phase === "gather")) ? ` ${t("lobby.hall", { n: inst.in_hall, seats: inst.seats })}` : "";
  const min = view.min_players ?? inst.min_players ?? 1;
  // a backend without a minimum (min 1) keeps the old wording: the first player to join starts the vote
  let phaseText = inst.phase === "idle" && min <= 1 ? "lobby.text.idle_first" : `lobby.text.${inst.phase}`;
  if (fill && (inst.phase === "idle" || inst.phase === "gather")) phaseText = `lobby.fill.text.${inst.phase}`;
  const minutes = Math.max(1, Math.round((view.fill_secs ?? 480) / 60));
  $("lobby-status").textContent = `${t(phaseText, { n: inst.players, min, seats: inst.seats, minutes })}${chosen}${map}${hall}`;
  $("lobby-rules").textContent = lobbyRulesText(view, inst);
  // a backend without rules sends none: no empty "Matchmaking rules" heading
  show($("lobby-rules-box"), !!$("lobby-rules").textContent, "block");

  const hallOpen = inst.phase === "starting" || inst.phase === "waitroom";
  show($("lobby-start"), mine && hallOpen, "flex");
  $("lobby-start-text").textContent = t(inst.phase === "starting" ? "lobby.server_starting" : me.game_online ? "lobby.press_start" : "lobby.start_game");
  show($("lobby-play"), mine && hallOpen && !me.game_online, "inline-flex");

  $("lobby-sizes").replaceChildren(
    ...view.sizes.map((n) =>
      lobbyOption(t(`lobby.size.${n}`), inst.votes.sizes[n], {
        mine: mine && me.size_vote === n && inst.phase === "vote_size",
        win: inst.party_size === n,
        enabled: mine && inst.phase === "vote_size",
        onclick: () => lobbyAct("lobby_vote", { partySize: n }),
      }),
    ),
  );
  // the map vote: the old backend's vote_map phase, or a fill instance's whole gather (a vote can be changed). Highlighted:
  // the leading map while gathering, the chosen one after the start
  const mapVote = fill ? inst.phase === "gather" : inst.phase === "vote_map";
  $("lobby-maps").replaceChildren(
    ...view.maps.map((id) =>
      lobbyOption(t(`lobby.map.${id}`), inst.votes.maps[id], {
        title: lobbyMapRule(view, id) && `${t(`lobby.map.${id}`)}: ${lobbyMapRule(view, id)}`,
        mine: mine && me.map_vote === id && mapVote,
        win: fill && inst.phase === "gather" ? inst.leader === id : inst.level_id === id,
        enabled: mine && mapVote,
        onclick: () => lobbyAct("lobby_vote", { levelId: id }),
      }),
    ),
  );

  show($("lobby-leave"), mine, "inline-flex");
}

/** Trial of Clotho tab: recruitment X/min, the player's run (wins of 7, tokens of 3), Join */
function renderClotho(view, inst, mine) {
  const playing = inst.phase === "starting" || inst.phase === "battle";
  const min = inst.min_players ?? 6;
  $("lobby-status").textContent = playing ? t("lobby.clotho.text.battle") : t("lobby.clotho.text.gather", { n: inst.players, min });
  show($("clotho-ingame"), mine && playing);

  const trial = view.me.trial;
  show($("clotho-trial"), !!trial, "grid");
  const pips = (have, of, on) =>
    Array.from({ length: of }, (_, k) => {
      const d = document.createElement("span");
      d.className = `size-3 rotate-45 rounded-[2px] border ${k < have ? `${on} border-transparent shadow-[0_0_8px_color-mix(in_srgb,var(--glow-c)_60%,transparent)]` : "border-line-hi bg-black/30"}`;
      return d;
    });
  if (trial) {
    const wins = Math.min(trial.wins ?? 0, 7);
    const tokens = Math.min(trial.tokens ?? 0, 3);
    $("clotho-wins").replaceChildren(...pips(wins, 7, "bg-accent"));
    $("clotho-wins-text").textContent = t("lobby.clotho.of", { n: wins, max: 7 });
    $("clotho-tokens").replaceChildren(...pips(tokens, 3, "bg-accent-text"));
    $("clotho-tokens-text").textContent = t("lobby.clotho.of", { n: tokens, max: 3 });
  }
  const status = trial?.status ?? null;
  $("clotho-note").textContent = status === null ? "" : t(`lobby.clotho.status.${status}`);
  // a run waiting for its reward can't take a new match: the game's event window gives the reward first
  const canJoin = !mine && !playing && !view.maintenance && status !== 2;
  show($("clotho-join"), canJoin, "inline-flex");
  $("clotho-join").onclick = () => lobbyAct("lobby_join", { instance: inst.id }).catch(() => {});
}

function renderRun() {
  const i = S.lastRun;
  const p = i === null ? null : S.run[i];
  show($("run"), !!p);
  if (!p) return;
  $("run-title").textContent = t("card.run_title");
  // ACE: pause after the previous game (play_flow, state "cooldown", text = seconds)
  const runText = p.cooldown ? t("steps.cooldown", { s: p.cooldown }) : p.steps.game === "running" ? t("steps.running") : "…";
  $("run-state").textContent = p.running ? runText : "";
  $("steps").replaceChildren(
    ...STEPS.map((step, n) => {
      const r = clone("tpl-step");
      mark(r.root, "state", p.steps[step] ?? null);
      r.label.textContent = t(`steps.${step}`);
      if (n === 0) r.line.remove();
      return r.root;
    }),
  );
  const d = Object.values(S.dl[i] || {});
  const downloading = d.length > 0 && p.steps.mods !== "done";
  show($("dl"), downloading, "flex");
  if (downloading) {
    const done = d.reduce((a, x) => a + x.done, 0);
    const total = d.reduce((a, x) => a + (x.total || x.done), 0);
    $("dl-fill").style.width = `${total ? Math.min(100, (done * 100) / total) : 0}%`; // CSSOM, allowed by the CSP
    $("dl-text").textContent = t("card.downloading", { done: bytes(done), total: bytes(total) });
  }
  show($("run-error"), !!p.error, "flex");
  // an error object (code) is translated at render time: it follows a language switch
  $("run-error-text").textContent = p.error && typeof p.error === "object" ? errText(p.error) : (p.error ?? "");
}

function renderAccount(s) {
  const m = s.manifest;
  const ids = ["acc-spin", "acc-code", "acc-join", "acc-login", "acc-cancel", "acc-logout", "acc-avatar"];
  const on = new Set();
  const err = S.loginError[m.id];
  let title = "";
  let sub = "";
  if (!m.auth) {
    title = t("card.account");
    sub = t("card.no_auth");
  } else if (s.logged_in && s.account) {
    const a = s.account;
    title = a.nick !== a.display_name ? `${a.display_name} · ${a.nick}` : a.display_name;
    sub = `${t("card.open_id")}: ${a.open_id}`;
    $("acc-avatar").textContent = (a.display_name || "?").slice(0, 1).toUpperCase();
    on.add("acc-avatar").add("acc-logout");
  } else if (S.login[m.id]) {
    title = t("card.login_wait");
    sub = S.login[m.id].code ? t("card.login_code") : "";
    $("acc-code").textContent = S.login[m.id].code ?? "";
    on.add("acc-spin").add("acc-cancel");
    if (S.login[m.id].code) on.add("acc-code");
  } else {
    title = t("card.account");
    sub = err ? "" : t("err.login_required");
    $("acc-login").textContent = err ? t("card.login_again") : m.auth.providers.includes("discord") ? t("card.login") : t("card.login_generic");
    $("acc-login").disabled = s.key_changed;
    on.add("acc-login");
    if (err?.link) on.add("acc-join");
  }
  for (const id of ids) show($(id), on.has(id), id === "acc-avatar" ? "grid" : id === "acc-spin" ? "inline-block" : "inline-flex");
  // login button is the secondary action when an invite is offered
  $("acc-login").className = err?.link ? "btn-std" : "btn-accent";
  show($("acc-login"), on.has("acc-login"), "inline-flex");
  show($("acc-icon"), !on.has("acc-avatar"), "inline-block");
  $("acc-title").textContent = title;
  $("acc-sub").textContent = sub;
  const showErr = !!err && !(s.logged_in && s.account) && !S.login[m.id];
  show($("acc-error"), showErr, "flex");
  $("acc-error-text").textContent = showErr ? errText(err) : "";
}

function renderMods(s) {
  const id = s.manifest.id;
  const mods = S.mods[id];
  const opt = (S.optional[id] ??= new Set());
  const req = mods?.filter((m) => m.required).length ?? 0;
  $("mods-summary").textContent = !mods ? "…" : mods.length ? t("card.mods_summary", { req, opt: mods.length - req }) : t("card.mods_none");
  mark($("mods-head"), "open", S.open.mods && mods?.length ? "yes" : "no");
  show($("mods"), S.open.mods && !!mods?.length);
  $("mods").replaceChildren(
    ...(mods ?? []).map((md) => {
      const r = clone("tpl-mod");
      r.check.checked = md.required || opt.has(md.id);
      r.check.disabled = md.required;
      r.check.onchange = (e) => (e.target.checked ? opt.add(md.id) : opt.delete(md.id));
      r.text.title = `${md.id}@${md.version}`;
      r.id.textContent = md.id;
      r.ver.textContent = md.version;
      r.req.textContent = t("card.mod_required");
      r.cached.textContent = t("card.mod_cached");
      r.nosrc.textContent = t("card.mod_no_source");
      show(r.req, md.required, "inline-flex");
      show(r.cached, md.cached, "inline-flex");
      show(r.nosrc, !md.cached && !md.downloadable, "inline-flex");
      return r.root;
    }),
  );
}

function renderInstalls(s) {
  $("installs").replaceChildren(
    ...[0].map((i) => {
      const r = clone("tpl-install");
      const root = S.state.settings.installs[i];
      const info = S.installs[i];
      let tone = "none";
      let text;
      if (!root) text = t("card.install_unset");
      else if (!info) text = "…";
      else if (info.error) [tone, text] = ["bad", errText(info.error)];
      else if (info.running) [tone, text] = ["warn", t("card.install_running")];
      else if (!s.manifest.game.exe_sha256) [tone, text] = ["bad", t("err.exe_unspecified")];
      else if (info.exe_sha256 !== s.manifest.game.exe_sha256) [tone, text] = ["bad", t("card.install_bad")];
      else [tone, text] = ["ok", t("card.install_ok")];
      r.root.dataset.tone = tone;
      r.slot.textContent = t("card.folder");
      r.path.textContent = root || "";
      r.path.title = root || "";
      r.text.textContent = text;
      r.browse.title = t("install.browse");
      r.browse.onclick = browseGame;
      show(r.dirty, !!info && !info.error && info.dirty);
      r.dirty.title = t("card.install_dirty");
      return r.root;
    }),
  );
}

function logVisible(e) {
  if (S.tab === "all") return true;
  if (S.tab === "relay") return e.kind === "relay" || e.scope === "relay";
  return e.kind !== "relay";
}

function logText(e) {
  return e.kind === "relay"
    ? `relay ${t("log.relay_line", { port: e.port, up: bytes(e.up), down: bytes(e.down), ms: e.ms, reason: e.reason })}`
    : e.text;
}

function renderLog() {
  const last = S.log[S.log.length - 1];
  $("log-last").textContent = last ? logText(last) : "";
  mark($("log-head"), "open", S.open.log ? "yes" : "no");
  show($("log-body"), S.open.log);
  for (const b of document.querySelectorAll(".log-tab")) mark(b, "active", b.dataset.tab === S.tab);
  if (!S.open.log) return;
  const box = $("log");
  const stick = box.scrollTop + box.clientHeight >= box.scrollHeight - 8;
  box.replaceChildren(
    ...S.log.filter(logVisible).map((e) => {
      const r = clone("tpl-log");
      r.root.dataset.tone = e.kind === "relay" ? (e.reason === "closed" ? "relay" : "warn") : e.level;
      r.time.textContent = new Date(e.at).toLocaleTimeString();
      r.scope.textContent = e.kind === "relay" ? "" : `[${e.scope}]`;
      r.text.textContent = logText(e);
      return r.root;
    }),
  );
  if (stick) box.scrollTop = box.scrollHeight;
}

async function renderSettings() {
  const st = S.state.settings;
  for (const i of [0]) {
    const input = $(`set-install-${i}`);
    if (document.activeElement !== input) input.value = st.installs[i] || "";
    const info = S.installs[i];
    const el = $(`set-install-${i}-info`);
    el.textContent = !st.installs[i] ? "" : !info ? "…" : info.error ? errText(info.error) : `exe sha256 ${info.exe_sha256.slice(0, 16)}…`;
    mark(el, "tone", info?.error ? "bad" : null);
  }
  $("set-admin").checked = st.run_as_admin;
  $("set-dev").checked = st.dev_mode;
  $("set-steam").checked = st.steam_bootstrap;
  $("set-lang").value = lang();
  $("set-theme").value = THEMES.includes(st.theme) ? st.theme : "torch";
  show($("force-hosts-row"), someNoHosts(), "flex");
  $("set-force-hosts").checked = !!st.force_hosts;
  const needed = hostsNeeded();
  show($("hosts-setup-row"), needed, "flex");
  $("set-hosts-state").textContent = t(S.state.hosts_installed ? "hosts.ok" : "hosts.missing");
  $("set-hosts-setup-path").textContent = S.state.hosts_path;
  show($("hosts-row"), !needed && !!S.state.hosts_leftover, "flex");
  $("set-hosts-path").textContent = S.state.hosts_path;
  $("version").textContent = `v${S.state.version}`;
  renderNetSettings();
  try {
    const c = await invoke("mod_cache_info");
    $("set-cache").textContent = t("settings.cache_info", { n: c.packages, size: bytes(c.bytes) });
  } catch (e) {
    $("set-cache").textContent = errText(e);
  }
  try {
    const list = await invoke("bootstrap_status");
    $("set-steam-status").textContent = list
      .slice(0, 1)
      .map((r) => (r ? t(`steam.${r.kind}`, { version: r.version ?? "" }) : null))
      .filter(Boolean)
      .join(" · ");
  } catch (e) {
    $("set-steam-status").textContent = errText(e);
  }
}

// ------------------------------------------------------------------------------------------------ battle tunnel

/** One switch per server the player is logged in to: the state lives on the server, the launcher only asks it */
async function renderNetSettings() {
  // every server with a login: the switch is always there, off and inactive until the player logs in (the choice is
  // stored on the server, per account)
  const servers = (S.state.servers ?? []).filter((s) => s.manifest.auth);
  const box = $("set-net");
  if (!servers.length) return box.replaceChildren(netRow(null));
  box.replaceChildren(...servers.map((s) => netRow(s)));
  for (const s of servers) {
    const row = box.querySelector(`[data-server="${CSS.escape(s.manifest.id)}"]`);
    if (!s.logged_in) {
      row.info.textContent = t("settings.net_need_login");
      continue;
    }
    try {
      const v = await invoke("net_state", { id: s.manifest.id });
      row.toggle.checked = !!v.udp_tunnel;
      row.toggle.disabled = false;
      row.info.textContent = v.udp_tunnel ? t("settings.net_on") : t("settings.net_off");
    } catch (e) {
      row.info.textContent = e?.code === "http_status" ? t("settings.net_unsupported") : errText(e);
    }
  }
}

function netRow(s) {
  const row = document.createElement("label");
  row.className = "row";
  const icon = document.createElement("span");
  icon.className = "ico text-xl text-fg-2";
  icon.textContent = "\uE968";
  const text = document.createElement("div");
  text.className = "min-w-0 flex-1";
  const title = document.createElement("div");
  title.textContent = s ? t("settings.net_tunnel", { name: s.manifest.name }) : t("settings.net_login");
  const info = document.createElement("div");
  info.className = "text-xs text-fg-3";
  info.textContent = s ? "…" : "";
  text.append(title, info);
  row.append(icon, text);
  if (!s) return row;
  const toggle = document.createElement("input");
  toggle.type = "checkbox";
  toggle.className = "toggle";
  toggle.disabled = true;
  toggle.onchange = () => setTunnel(s.manifest.id, toggle.checked, "settings");
  row.append(toggle);
  row.dataset.server = s.manifest.id;
  row.toggle = toggle;
  row.info = info;
  return row;
}

async function setTunnel(id, on, reason) {
  try {
    await call("set_udp_tunnel", { id, on, reason });
    // the tunnel only helps battles: the advice about WARP stays until the player closes it
    if (S.netHint[id]) S.netHint[id].tunnel = false;
    toast(t(on ? "net.done_on" : "net.done_off"), "ok");
  } catch {
    // call() showed the error
  }
  render();
}

// ------------------------------------------------------------------------------------------------ actions

function select(id) {
  const other = S.page === "server" && S.selected !== id;
  S.selected = id;
  S.page = "server";
  render();
  if (other) replayEnter($("page-server")); // the page is already on screen: another server appears as if new
  $("page-server").parentElement.scrollTop = 0;
  loadMods(id);
  loadStatus(id);
  if (!(id in S.support)) loadSupport(id);
  if (S.view === "notes") loadNotes(id);
  maybeSplash();
}

async function saveSettings(patch) {
  const relang = patch.lang && patch.lang !== S.state.settings.lang;
  await call("save_settings", { settings: { ...S.state.settings, ...patch } });
  await reload();
  // patch notes come in the launcher's language: ask again in the new one
  if (relang) {
    S.notes = {};
    if (S.view === "notes" && S.selected) loadNotes(S.selected);
  }
}

async function play(confirmNative = []) {
  const s = server();
  const i = S.pick;
  if (!s) return;
  S.run[i] = { steps: {}, running: true, error: null };
  S.dl[i] = {};
  S.lastRun = i;
  renderServer();
  try {
    await invoke("play", { id: s.manifest.id, install: i, optional: [...(S.optional[s.manifest.id] ?? [])], confirmNative });
  } catch (e) {
    S.run[i].error = e?.code ? e : errText(e); // a code is translated when shown (follows a language switch)
    S.run[i].running = false;
    renderServer();
    if (e?.code === "native_confirm") {
      const ids = e.detail.split(",");
      if (await confirmBox(t("native.title"), t("native.text", { ids: ids.join("\n") }))) return play(ids);
    } else if (e?.code === "hosts_fallback" || e?.code === "hosts_not_installed") {
      // set up hosts right here and start again, instead of sending the player to Settings
      const why = t(e.code === "hosts_fallback" ? "hosts.fallback_text" : "hosts.needed_text");
      try {
        if (await setupHosts(t("hosts.fallback_title"), why)) return play(confirmNative);
      } catch (err) {
        toast(errText(err));
      }
    } else {
      toast(errText(e));
    }
  }
  await reload();
  loadInstalls();
}

async function addServer() {
  const url = $("add-url").value.trim();
  if (!url) return;
  const ids = await call("add_server", { url });
  show($("dlg-add"), false, "flex");
  $("add-url").value = "";
  toast(t("servers.added", { n: ids.length }), "ok");
  await reload();
  if (ids[0]) select(ids[0]);
  ids.forEach(loadStatus);
}

function openAdd() {
  show($("dlg-add"), true, "flex");
  $("add-url").focus();
}

async function refreshAll() {
  for (const s of S.state.servers) {
    try {
      await invoke("refresh_server", { id: s.manifest.id });
    } catch (e) {
      pushLog({ kind: "log", scope: "servers", level: "warn", text: `${s.manifest.id}: ${errText(e)}` });
    }
  }
  await reload();
  S.state.servers.forEach((s) => loadStatus(s.manifest.id));
}

// ------------------------------------------------------------------------------------------------ bug report
// "Report a bug" (contract docs/specs/bug-report-api.md): the launcher sends the text, versions and log tails to
// the server with the player's session; the server's bot posts it to the Discord forum, the launcher then shows the link.

const BUG_POLL_MS = 5000;
const BUG_POLL_FOR_MS = 3 * 60 * 1000; // the bot takes new reports every 20 s

/** `contact` of a manifest when it is an https link, else null */
function contactUrl(m) {
  try {
    const u = new URL(m.contact ?? "");
    return u.protocol === "https:" ? u.href : null;
  } catch {
    return null;
  }
}

/** `opts.kind`: preselected type; `opts.source`: "game" when the game's "Support" button asked; `opts.crashed`: the
 *  launcher saw the game crash just now */
async function openBug(opts = {}) {
  const id = S.selected;
  // already open with the player's text: keep it, only bring it up
  if (S.bug && !S.bug.report && !$("dlg-bug").classList.contains("hidden")) {
    if (opts.kind) $("bug-kind").value = opts.kind;
    if (opts.source) S.bug.source = opts.source;
    $("bug-title").focus();
    return;
  }
  let info;
  try {
    info = await invoke("bug_report_info", { id });
  } catch (e) {
    toast(errText(e));
    return;
  }
  S.bug = { id, source: opts.source ?? "launcher", loggedIn: info.logged_in, sending: false, report: null, posted: false, url: null, late: false };
  const crash = info.crash || !!opts.crashed;
  show($("bug-login"), !info.logged_in, "flex");
  show($("bug-crash"), crash, "flex");
  // a crash is offered once per form; the player's own choice stays otherwise
  if (opts.kind) $("bug-kind").value = opts.kind;
  else if (crash) $("bug-kind").value = "crash";
  else if ($("bug-kind").value === "crash") $("bug-kind").value = "game";
  $("bug-title").maxLength = info.title_max;
  $("bug-desc").maxLength = info.description_max;
  $("bug-files").textContent = info.files.length ? info.files.map((f) => `${f.name} (${bytes(f.size)})`).join(", ") : t("bug.no_files");
  bugRender();
  show($("dlg-bug"), true, "flex");
  $("bug-title").focus();
}

function closeBug() {
  show($("dlg-bug"), false, "flex");
  // a sent report keeps polling in the background and says when it is posted
  if (S.bug && !S.bug.sending) S.bug = null;
}

function bugRender() {
  const b = S.bug;
  if (!b) return;
  const sent = !!b.report;
  show($("bug-form"), !sent);
  show($("bug-sent"), sent);
  show($("bug-send"), !sent);
  show($("bug-open"), sent);
  const title = $("bug-title").value.trim();
  $("bug-send").disabled = b.sending || !b.loggedIn || !title;
  $("bug-send").textContent = t(b.sending ? "bug.sending" : "bug.send");
  $("bug-close").textContent = t(sent ? "bug.close" : "common.cancel");
  $("bug-title-n").textContent = `${$("bug-title").value.length}/${$("bug-title").maxLength}`;
  $("bug-desc-n").textContent = `${$("bug-desc").value.length}/${$("bug-desc").maxLength}`;
  if (sent) {
    $("bug-open").disabled = !b.url;
    $("bug-open").textContent = t(b.url ? "bug.open_post" : "bug.waiting");
    show($("bug-sent-spin"), !b.posted && !b.late);
    show($("bug-sent-ok"), b.posted);
    $("bug-sent-text").textContent = t(b.posted ? (b.url ? "bug.posted" : "bug.posted_nolink") : b.late ? "bug.late" : "bug.wait");
    $("bug-sent-id").textContent = `#${b.report}`;
  }
}

async function sendBug() {
  const b = S.bug;
  if (!b || b.sending || b.report || !$("bug-title").value.trim()) return;
  b.sending = true;
  bugRender();
  try {
    b.report = await invoke("bug_report", { id: b.id, kind: $("bug-kind").value, title: $("bug-title").value, description: $("bug-desc").value, source: b.source });
    $("bug-title").value = "";
    $("bug-desc").value = "";
    pollBug(b);
  } catch (e) {
    if (e?.code === "api.rate_limited") toast(t("bug.rate_limited", { min: Math.max(1, Math.ceil(Number(e.detail) / 60) || 60) }));
    else toast(errText(e));
  } finally {
    b.sending = false;
    // closed while sending: the toast below is all the player sees
    if (S.bug === b && $("dlg-bug").classList.contains("hidden")) S.bug = null;
    bugRender();
  }
}

async function pollBug(b) {
  const until = Date.now() + BUG_POLL_FOR_MS;
  const open = () => b.url && call("open_link", { url: b.url }).catch(() => {});
  while (Date.now() < until) {
    await new Promise((r) => setTimeout(r, BUG_POLL_MS));
    let s;
    try {
      s = await invoke("bug_report_status", { id: b.id, report: b.report });
    } catch {
      continue; // the network or the session: the next round may get through
    }
    if (s.status === "posted") {
      Object.assign(b, { posted: true, url: s.url ?? null });
      if (S.bug === b) bugRender();
      toast(t("bug.posted_toast", { id: b.report }), "ok", b.url ? { label: t("bug.open_post"), run: open } : null);
      return;
    }
  }
  b.late = true;
  if (S.bug === b) bugRender();
}

// ------------------------------------------------------------------------------------------------ wiring

function wire() {
  $("lang").onclick = () => saveSettings({ lang: lang() === "ru" ? "en" : "ru" });
  $("refresh").onclick = refreshAll;
  $("nav-add").onclick = openAdd;
  $("empty-add").onclick = openAdd;
  $("nav-settings").onclick = () => {
    S.page = "settings";
    render();
    $("page-settings").parentElement.scrollTop = 0;
  };
  $("add-ok").onclick = addServer;
  $("add-cancel").onclick = () => show($("dlg-add"), false, "flex");
  // a block body: an on* handler that returns false cancels the key, and then nothing can be typed or pasted
  $("add-url").onkeydown = (e) => {
    if (e.key === "Enter") addServer();
  };
  $("update-btn").onclick = async () => {
    $("update-btn").disabled = true;
    try {
      await invoke("update_apply"); // restarts the launcher on success
    } catch (e) {
      toast(errText(e));
      $("update-btn").disabled = false;
    }
  };

  // full-window "what's new"
  $("splash-ok").onclick = () => closeSplash(false);
  $("splash-all").onclick = () => closeSplash(true);
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && S.splash) closeSplash(false);
    else if (e.key === "Escape" && !$("dlg-bug").classList.contains("hidden")) closeBug();
  });

  // bug report
  $("srv-bug").onclick = () => openBug();
  $("srv-contact").onclick = () => {
    const url = contactUrl(server().manifest);
    if (url) call("open_link", { url }).catch(() => {});
  };
  $("bug-send").onclick = sendBug;
  $("bug-close").onclick = closeBug;
  $("bug-open").onclick = () => S.bug?.url && call("open_link", { url: S.bug.url }).catch(() => {});
  $("bug-title").oninput = $("bug-desc").oninput = () => bugRender();
  $("bug-title").onkeydown = (e) => {
    if (e.key === "Enter") $("bug-desc").focus();
  };

  // own title bar: the window has no system frame
  const win = window.__TAURI__.window?.getCurrentWindow?.();
  $("win-min").onclick = () => win?.minimize();
  $("win-max").onclick = () => win?.toggleMaximize();
  $("win-close").onclick = () => win?.close();
  const maxGlyph = async () => mark($("win-max"), "max", !!(await win?.isMaximized?.()));
  win?.onResized?.(maxGlyph);
  maxGlyph();

  // server page
  $("net-on").onclick = () => setTunnel(S.selected, true, S.netHint[S.selected]?.detail ?? "prompt");
  $("net-warp").onclick = () => call("open_link", { url: WARP_URL }).catch(() => {});
  $("net-copy").onclick = async () => {
    try {
      const text = await invoke("net_report_text", { id: S.selected });
      await navigator.clipboard.writeText(text);
      toast(t("net.copied"), "ok");
    } catch (e) {
      toast(errText(e));
    }
  };
  $("net-later").onclick = () => {
    delete S.netHint[S.selected];
    renderServer();
  };
  for (const b of document.querySelectorAll(".srv-tab")) {
    b.onclick = () => {
      if (S.view === b.dataset.view) return;
      S.view = b.dataset.view;
      if (S.view === "notes" && S.selected) loadNotes(S.selected);
      else renderServer();
    };
  }
  $("play").onclick = () => play();
  $("lobby-play").onclick = () => play();
  $("lobby-leave").onclick = () => lobbyAct("lobby_leave");
  $("key-accept").onclick = async () => {
    await call("accept_key", { id: S.selected });
    await reload();
  };
  $("acc-login").onclick = async () => {
    const id = S.selected;
    S.login[id] = {};
    delete S.loginError[id];
    renderServer();
    try {
      await call("login", { id });
    } catch {
      delete S.login[id];
      renderServer();
    }
  };
  $("acc-cancel").onclick = async () => {
    await call("cancel_login", { id: S.selected });
    delete S.login[S.selected];
    renderServer();
  };
  $("acc-logout").onclick = async () => {
    await call("logout", { id: S.selected });
    await reload();
  };
  $("acc-join").onclick = () => call("open_link", { url: S.loginError[S.selected]?.link });
  $("mods-head").onclick = () => {
    S.open.mods = !S.open.mods;
    renderServer();
  };
  $("srv-site").onclick = () => call("open_link", { url: server().manifest.website });
  $("srv-refresh").onclick = async () => {
    const id = S.selected;
    await call("refresh_server", { id });
    await reload();
    loadMods(id);
    loadStatus(id);
  };
  $("srv-remove").onclick = async () => {
    const s = server();
    if (!(await confirmBox(t("card.remove"), t("card.remove_confirm", { name: s.manifest.name })))) return;
    await call("remove_server", { id: s.manifest.id });
    S.selected = null;
    await reload();
  };
  $("log-head").onclick = () => {
    S.open.log = !S.open.log;
    renderLog();
  };
  for (const b of document.querySelectorAll(".log-tab")) {
    b.onclick = () => {
      S.tab = b.dataset.tab;
      renderLog();
    };
  }
  $("log-clear").onclick = () => {
    S.log = [];
    renderLog();
  };

  // settings page: applies at once, like Windows Settings
  for (const i of [0]) {
    $(`set-install-${i}`).onchange = async (e) => {
      const installs = [...S.state.settings.installs];
      installs[i] = e.target.value.trim();
      await saveSettings({ installs });
      loadInstalls();
    };
  }
  $("install-browse").onclick = browseGame;
  $("install-find").onclick = () => findGame(false);
  $("set-admin").onchange = (e) => saveSettings({ run_as_admin: e.target.checked });
  $("set-force-hosts").onchange = (e) => saveSettings({ force_hosts: e.target.checked });
  $("set-dev").onchange = (e) => saveSettings({ dev_mode: e.target.checked });
  $("set-lang").onchange = (e) => saveSettings({ lang: e.target.value });
  $("set-theme").onchange = (e) => saveSettings({ theme: e.target.value });
  $("set-steam").onchange = async (e) => {
    const lines = await call("set_bootstrap", { enable: e.target.checked });
    for (const text of lines) pushLog({ kind: "log", scope: "steam", level: "info", text });
    toast(t(e.target.checked ? "steam.enabled" : "steam.disabled"), "ok");
    await reload();
  };
  $("hosts-install").onclick = async () => {
    // the yellow UAC window ("unknown publisher") scares people: say what it is first
    if (!(await confirmBox(t("hosts.confirm_title"), t("hosts.confirm_text")))) return;
    await call("hosts_install");
    await reload();
  };
  $("hosts-uninstall").onclick = $("hosts-setup-uninstall").onclick = async () => {
    await call("hosts_uninstall");
    await reload();
  };
  $("cache-clear").onclick = async () => {
    await call("clear_mod_cache");
    toast(t("settings.cache_cleared"), "ok");
    renderSettings();
  };
}

async function init() {
  wire();
  await listen("log", (ev) => pushLog({ kind: "log", ...ev.payload }));
  await listen("relay", (ev) => pushLog({ kind: "relay", ...ev.payload }));
  // the server's relay refuses this version (backend [relay] min_client): the game can't log in until the update
  await listen("launcher_outdated", async () => {
    toast(t("update.required"));
    await checkUpdate();
  });
  // 1.7.2: the game logged in but did not come to the server without hosts; the launcher switched to hosts
  await listen("hosts_fallback", async () => {
    await reload();
    await confirmBox(t("hosts.fallback_title"), t("hosts.fallback_game"));
  });
  // the game's "Support" button (UnLua pack): the report form, about the game
  await listen("bug_request", (ev) => {
    if (server(ev.payload.server_id) && S.selected !== ev.payload.server_id) select(ev.payload.server_id);
    openBug({ kind: "game", source: "game" });
  });
  // the game exited and looks crashed: offer a report, it goes only if the player sends it
  await listen("game_crashed", async (ev) => {
    const id = ev.payload.server_id;
    if (!server(id)) return;
    const yes = await confirmBox(t("bug.crashed_title"), t("bug.crashed_text"), t("bug.crashed_yes"), t("bug.crashed_no"));
    if (!yes) return;
    if (S.selected !== id) select(id);
    openBug({ kind: "crash", crashed: true });
  });
  // shown again on every new finding, even after the player closed it
  await listen("net_block", (ev) => {
    const { server_id, kind, detail, tunnel } = ev.payload;
    S.netHint[server_id] = { kind, detail, tunnel };
    render();
  });
  await listen("play", (ev) => {
    const { install, step, state, text, error } = ev.payload;
    const p = (S.run[install] ??= { steps: {}, running: true, error: null });
    S.lastRun = install;
    if (step === "done") {
      p.running = false;
      if (state === "error") p.error = error ?? text;
    } else if (step === "game" && state === "cooldown") {
      p.steps.game = "start";
      p.cooldown = text;
    } else {
      p.steps[step] = state;
      if (step === "game") p.cooldown = null;
    }
    renderServer();
  });
  await listen("mod_progress", (ev) => {
    const { install, id, done, total } = ev.payload;
    (S.dl[install] ??= {})[id] = { done, total };
    renderRun();
  });
  await listen("login", async (ev) => {
    const { server_id, state, error, user_code } = ev.payload;
    if (state === "browser") {
      S.login[server_id] = { code: user_code };
      renderServer();
      return;
    }
    delete S.login[server_id];
    if (state === "error" && error) {
      S.loginError[server_id] = error;
      toast(errText(error));
    }
    await reload();
  });
  await listen("session", async (ev) => {
    const { server_id, error } = ev.payload;
    S.loginError[server_id] = error;
    toast(`${t("session.lost")}: ${errText(error)}`);
    await reload();
  });
  await listen("servers_changed", async () => {
    await reload();
    if (S.selected) loadMods(S.selected);
    checkUpdate();
    maybeSplash();
  });
  await listen("update_progress", (ev) => {
    const { done, total } = ev.payload;
    $("update-progress").textContent = total ? `${bytes(done)} / ${bytes(total)}` : bytes(done);
  });

  await reload();
  for (const text of await call("restore_pending")) pushLog({ kind: "log", scope: "profile", level: "warn", text });
  // no game folder yet, or the game moved: look for it by itself
  loadInstalls().then(() => {
    const code = S.installs[0]?.error?.code;
    if (!S.state.settings.installs[0] || code === "install_not_set" || code === "install_invalid") findGame(true);
  });
  for (const s of S.state.servers) loadStatus(s.manifest.id);
  if (S.selected) loadMods(S.selected);
  checkUpdate();
  pollLobby();
  setInterval(lobbyTimer, 500);
  if (S.selected) loadSupport(S.selected);
  setInterval(() => S.selected && loadSupport(S.selected), SUPPORT_POLL_MS);
  maybeSplash();
}

init();
