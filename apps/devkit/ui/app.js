// DevKit page: calls Rust commands (src-tauri/src/lib.rs), renders results. No direct file or network access.
"use strict";
const invoke = (cmd, args) => window.__TAURI__.core.invoke(cmd, args);
const $ = (s) => document.querySelector(s);
const $$ = (s) => [...document.querySelectorAll(s)];
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

// ------------------------------------------------------------------------------------------- language
// English by default; the choice is remembered in localStorage (i18n.js holds the texts)
let LANG = "en";
try {
  const saved = localStorage.getItem("devkit.lang");
  if (saved === "en" || saved === "ru") LANG = saved;
} catch {}
const t = (key, vars) => {
  let s = window.I18N[LANG][key] ?? window.I18N.en[key] ?? key;
  if (vars) s = s.replace(/\{(\w+)\}/g, (m, k) => (k in vars ? vars[k] : m));
  return s;
};
function applyI18n() {
  document.documentElement.lang = LANG;
  $$("[data-t]").forEach((el) => (el.textContent = t(el.dataset.t)));
  $$("[data-t-html]").forEach((el) => (el.innerHTML = t(el.dataset.tHtml)));
  $$("[data-t-placeholder]").forEach((el) => (el.placeholder = t(el.dataset.tPlaceholder)));
}

// note — an external program (Visual Studio, Unreal Engine, Ghidra) needed only for specific jobs, not an error
const LEVELS = ["error", "warn", "done", "ok", "note"];
// member visibility (docs/WORKSPACE_PLAN.md, 13.3): badge color; the label comes from vis.<level>
const VIS = {
  public: "bg-ok-bg text-ok",
  team: "bg-accent/15 text-accent-text",
  owner: "bg-caution-bg text-caution",
};
// last data of each view: re-rendered when the language changes
const view = {};

function status(text, busy = false) {
  $("#status").innerHTML = (busy ? '<span class="spin"></span>' : "") + esc(text);
}

function findings(list, el, emptyKey) {
  if (!list.length) {
    el.innerHTML = `<div class="px-4 py-3 text-fg-3">${esc(t(emptyKey))}</div>`;
    return;
  }
  const order = { error: 0, warn: 1, done: 2, note: 3, ok: 4 };
  el.innerHTML = [...list]
    .sort((a, b) => order[a.level] - order[b.level])
    .map(
      (f) => `<div class="finding" data-level="${f.level}"><span class="lvl">${esc(t("lvl." + f.level))}</span>
        <div class="min-w-0"><div class="text-xs text-fg-3">[${esc(f.rule)}] ${esc(f.path)}</div>
        <div class="break-words select-text">${esc(f.msg)}</div></div></div>`,
    )
    .join("");
}

function counts(list) {
  const c = { error: 0, warn: 0, done: 0, ok: 0, note: 0 };
  list.forEach((f) => c[f.level]++);
  return c;
}

async function guard(label, fn) {
  status(label + "…", true);
  $$("button").forEach((b) => (b.disabled = true));
  try {
    await fn();
  } catch (e) {
    status(t("err") + e);
    return;
  } finally {
    $$("button").forEach((b) => (b.disabled = false));
  }
}

// ------------------------------------------------------------------------------------------- pages
function show(page) {
  $$("[data-page]").forEach((b) => b.toggleAttribute("data-active", b.dataset.page === page));
  $$("[data-page-body]").forEach((s) => s.classList.toggle("hidden", s.id !== "page-" + page));
  $$("[data-page-body]").forEach((s) => s.classList.toggle("flex", s.id === "page-" + page));
  try { localStorage.setItem("devkit.page", page); } catch {}
  if (page === "layout" && !$("#members").innerHTML) loadMembers();
  if (page === "components" && !view.cmp) loadComponents();
  if (page === "work") loadWork();
  if (page === "game") loadGame();
  if (page === "secrets" && !$("#secrets-list").innerHTML) loadSecrets();
}

function renderOverview() {
  const list = view.overview;
  if (!list) return;
  findings(list, $("#overview-list"), "ov.nodata");
  const c = counts(list);
  $("#overview-summary").innerHTML = LEVELS.map((l) => [l, t("sum." + l), c[l]])
    .map(([l, label, n]) => `<div class="finding card !border px-4 py-2" data-level="${l}"><span class="lvl">${n}</span><span class="text-fg-2">${esc(label)}</span></div>`)
    .join("");
}

async function doctor(apply) {
  const profile = $("#profile").value;
  await guard(t(apply ? "ov.fixing" : "ov.checking"), async () => {
    const list = await invoke("doctor", { profile, apply });
    view.overview = list;
    renderOverview();
    const c = counts(list);
    status(t("ov.status", { mode: apply ? "bootstrap" : "doctor", profile, error: c.error, warn: c.warn }) + (c.done ? t("ov.status_done", { done: c.done }) : ""));
    loadInfo();
  });
}

function renderCheck() {
  if (view.check) findings(view.check, $("#check-list"), "lay.clean");
}

async function check() {
  await guard("ws check", async () => {
    view.check = await invoke("check");
    renderCheck();
    const c = counts(view.check);
    status(t("lay.status", { error: c.error, warn: c.warn }));
  });
}

function renderMembers() {
  const ms = view.members;
  if (!ms) return;
  $("#members").innerHTML = ms
    .map(
      (m) => `<div class="finding"><span class="lvl bg-white/[0.06] text-fg-2">${esc(m.kind)}</span>
      <div class="min-w-0"><div><span class="font-semibold">${esc(m.path)}</span>
        <span class="ml-2 rounded-full px-2 text-xs ${VIS[m.visibility] ?? "bg-critical-bg text-critical"}">${esc(VIS[m.visibility] ? t("vis." + m.visibility) : t("vis.none"))}</span></div>
      <div class="text-xs text-fg-3">${m.depends_on.length ? esc(t("mem.depends")) + esc(m.depends_on.join(", ")) : esc(t("mem.nodeps"))}</div></div></div>`,
    )
    .join("");
}

async function loadMembers() {
  view.members = await invoke("members");
  renderMembers();
}

// ------------------------------------------------------------------------------------------- start: me + agent
let agMode = "task";
function renderStart() {
  const me = view.me;
  if (!me) return;
  $("#me-level").value = me.level || "dev";
  $("#me-lang").value = me.lang || LANG;
  $("#me-detail").value = me.detail || "normal";
  $("#me-machine").textContent = t("me.machine", { p: me.machine });
  $$("#ag-modes [data-mode]").forEach((b) => b.classList.toggle("!bg-accent/20", b.dataset.mode === agMode));
  $("#ag-mode-hint").textContent = t("ag.h." + agMode);
  // no Claude Code CLI: the form stays visible but does nothing (inert survives guard(), which re-enables buttons)
  const noCli = !me.claude;
  $("#ag-form").inert = noCli;
  $("#ag-form").classList.toggle("opacity-40", noCli);
  $("#ag-nocli").classList.toggle("hidden", !noCli);
  $("#ag-nocli").textContent = noCli ? t("ag.nocli", { ws: view.info?.root || "workspace" }) : "";
}

async function loadMe() {
  try {
    view.me = await invoke("me");
  } catch {
    return;
  }
  // the interface language: the person's own choice in DevKit first, else [me] lang
  let saved = null;
  try { saved = localStorage.getItem("devkit.lang"); } catch {}
  if (!saved && (view.me.lang === "ru" || view.me.lang === "en") && view.me.lang !== LANG) {
    LANG = view.me.lang;
    applyI18n();
  }
  renderStart();
}

async function saveMe() {
  await guard(t("me.saving"), async () => {
    await invoke("set_me", { level: $("#me-level").value, lang: $("#me-lang").value, detail: $("#me-detail").value });
    await loadMe();
    status(t("me.saved"));
  });
}

async function startAgent() {
  const task = $("#ag-task").value.trim();
  if (!task) {
    status(t("ag.empty"));
    return;
  }
  await guard(t("ag.starting"), async () => {
    const file = await invoke("start_agent", { mode: agMode, role: $("#ag-role").value, task });
    $("#ag-result").textContent = t("ag.started", { f: file });
    status(t("ag.ok"));
  });
}

// ------------------------------------------------------------------------------------------- game data
function renderGame() {
  const g = view.game;
  const v = view.vault;
  if (v) {
    $("#gm-vault-state").innerHTML = v.unlocked
      ? `<span class="text-ok">${esc(t("gm.open", { m: v.minutes_left, i: v.idle_minutes_left }))}</span> <span class="text-xs text-fg-3">${esc(v.names.join(", "))}</span>`
      : `<span class="text-fg-3">${esc(t("gm.closed"))}</span>`;
  }
  if (!g) return;
  $("#gm-ref").innerHTML = g.references.length
    ? g.references.map((r) => `<div class="mb-2"><span class="font-semibold">${esc(r[0])}</span> <span class="text-xs text-fg-3 select-text">${esc(r[1])}</span><div class="text-xs text-ok">${esc(r[2])}</div></div>`).join("")
    : `<div class="mb-3 text-fg-2">${esc(t("gm.none"))}</div>
       ${g.installed
         ? `<div class="mb-3 rounded-md border border-white/[0.06] p-3 text-sm">
              <div>${esc(t("gm.installed", { p: g.installed }))}</div>
              ${g.installed_ok
                ? `<button id="gm-take" class="btn-accent mt-2">${esc(t("gm.take"))}</button>`
                : `<div class="mt-1 text-xs text-caution select-text">${esc(g.reference_note)}</div>`}
            </div>
            <div class="mb-2 text-xs text-fg-3">${esc(t("gm.or_download"))}</div>`
         : ""}
       <ol class="mb-3 list-decimal space-y-2 pl-5 text-sm">
         <li>${esc(t("gm.s1"))} <button id="gm-steam" class="btn-flat !h-7">${esc(t("gm.open_console"))}</button></li>
         <li>${esc(t("gm.s2"))} <code class="select-text rounded bg-white/[0.06] px-1">${esc(g.steam_command)}</code> <button id="gm-copy" class="btn-flat !h-7">${esc(t("gm.copy"))}</button></li>
         <li>${esc(t("gm.s3", { p: g.steam_download }))}</li>
         <li>${esc(t("gm.s4"))} <button id="gm-register" class="btn-std !h-7" ${g.steam_download_ready ? "" : "disabled"}>${esc(t("gm.register"))}</button></li>
       </ol>
       <div class="text-xs ${g.steam_download_ready ? "text-ok" : "text-fg-4"}">${esc(t(g.steam_download_ready ? "gm.ready" : "gm.notyet"))}</div>`;
  $("#gm-dump-state").textContent = g.dump_files
    ? t("gm.have", { n: g.dump_files, h: g.dump_when, p: g.dump_dir })
    : t("gm.nodump", { p: g.dump_dir });
  $("#gm-sets").innerHTML = g.sets
    .map((s) => `<div class="finding" data-level="${s.error ? "error" : s.ok ? "ok" : "note"}"><span class="lvl">${s.ok}</span><div class="min-w-0"><span class="font-semibold">${esc(s.name)}</span> <span class="text-xs text-fg-3">${esc(t("gm.counts", { s: s.skip, e: s.error }))}</span></div></div>`)
    .join("");
  $("#gm-steam")?.addEventListener("click", () => gameAction("steam_console"));
  $("#gm-register")?.addEventListener("click", () => gameAction("register"));
  $("#gm-take")?.addEventListener("click", () => gameAction("take"));
  $("#gm-copy")?.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(g.steam_command);
      status(t("gm.copied"));
    } catch {
      status(g.steam_command);
    }
  });
  if (view.gameResult) findings(view.gameResult, $("#gm-result"), "cmp.nothing");
}

async function loadGame() {
  try {
    [view.game, view.vault] = await Promise.all([invoke("game"), invoke("vault_state")]);
  } catch (e) {
    status(String(e));
  }
  renderGame();
}

async function gameAction(action) {
  const label = { steam_console: "gm.opening", register: "gm.registering", dump_plan: "cmp.planning", dump: "gm.dumping" }[action];
  await guard(t(label), async () => {
    const list = await invoke("game_action", { action });
    if (list.length) {
      view.gameResult = list;
      $("#gm-result").classList.remove("hidden");
    }
    await loadGame();
    const c = counts(list);
    status(t("gm.status", { error: c.error, warn: c.warn }));
  });
  // guard() re-enables every button at the end: draw again so "Register" stays off without a download
  renderGame();
}

async function vault(action) {
  if (action === "unlock") {
    const password = $("#gm-pass").value;
    if (!password) {
      status(t("sec.enter"));
      return;
    }
    await guard(t("gm.unlocking"), async () => {
      view.vault = await invoke("vault_unlock", { password, minutes: +$("#gm-min").value });
      $("#gm-pass").value = "";
      renderGame();
      status(t("gm.open", { m: view.vault.minutes_left, i: view.vault.idle_minutes_left }));
    });
    renderGame();
  } else {
    view.vault = await invoke("vault_lock");
    renderGame();
  }
}

// ------------------------------------------------------------------------------------------- work
function renderWork() {
  const w = view.work;
  if (!w) return;
  $("#wk-branch").innerHTML =
    `<div class="font-semibold">${esc(w.branch)}</div><div class="text-xs text-fg-3">${esc(w.head)}</div>` +
    (w.changes.length
      ? `<div class="mt-3 text-xs text-caution">${esc(t("wk.changes", { n: w.changes.length }))}</div><pre class="mt-1 font-mono text-xs text-fg-3 select-text">${esc(w.changes.join("\n"))}</pre>`
      : `<div class="mt-3 text-xs text-ok">${esc(t("wk.clean"))}</div>`);
  $("#wk-branches").innerHTML = w.branches
    .map((b) => `<div class="finding"><div class="min-w-0"><div class="font-semibold">${esc(b.name)}</div><div class="text-xs text-fg-3">${esc(b.when)} · ${esc(b.subject)}</div></div></div>`)
    .join("");
  $("#wk-reports").innerHTML = w.reports.length
    ? w.reports
        .map(
          (r, i) => `<button class="finding w-full text-left hover:bg-card-hi" data-level="${r.pending ? "warn" : "ok"}" data-report="${i}">
            <span class="lvl">${esc(t(r.pending ? "wk.pending" : "wk.done"))}</span>
            <div class="min-w-0"><div class="truncate">${esc(r.title)}</div><div class="text-xs text-fg-4">${esc(r.file)}</div></div></button>`,
        )
        .join("")
    : `<div class="px-4 py-3 text-fg-3">${esc(t("wk.none"))}</div>`;
  $("#wk-previews").innerHTML = (w.previews || []).length
    ? w.previews
        .map(
          (p) => `<div class="finding"><div class="min-w-0 flex-1"><div class="font-semibold">${esc(p.name)} <span class="text-xs font-normal text-fg-4">${esc(p.member)}</span></div>
            <div class="text-xs text-fg-3">${esc(p.note)}</div><div class="text-xs text-fg-4 select-text">${esc(p.url)}</div></div>
            <button class="btn-std shrink-0" data-preview="${esc(p.name)}"><span class="ico">&#xE8A7;</span><span>${esc(t("wk.open"))}</span></button></div>`,
        )
        .join("")
    : `<div class="px-4 py-3 text-fg-3">${esc(t("wk.no_previews"))}</div>`;
  $$("#wk-previews [data-preview]").forEach((b) =>
    b.addEventListener("click", async () => {
      try {
        status(t("wk.opening", { n: b.dataset.preview }));
        await invoke("open_preview", { name: b.dataset.preview });
      } catch (e) {
        status(String(e));
      }
    }),
  );
  $$("#wk-reports [data-report]").forEach((b) =>
    b.addEventListener("click", () => {
      const r = view.work.reports[+b.dataset.report];
      $("#wk-report").classList.remove("hidden");
      $("#wk-report").textContent = r.text;
    }),
  );
}

async function loadWork() {
  try {
    view.work = await invoke("work");
    renderWork();
  } catch (e) {
    status(String(e));
  }
}

// ------------------------------------------------------------------------------------------- components
// Chosen by the person (besides the base); what they need comes with them (closure over `needs`).
let cmpChosen = new Set();
const cmpOf = (name) => view.cmp.components.find((c) => c.name === name);
function cmpNeeds(name, acc = new Set()) {
  (cmpOf(name)?.needs ?? []).forEach((n) => {
    if (!acc.has(n)) {
      acc.add(n);
      cmpNeeds(n, acc);
    }
  });
  return acc;
}
// everything that will be on disk with the current choice
function cmpTarget() {
  const all = new Set(view.cmp.components.filter((c) => c.base).map((c) => c.name));
  cmpChosen.forEach((n) => {
    all.add(n);
    cmpNeeds(n).forEach((x) => all.add(x));
  });
  return all;
}

function renderComponents() {
  const st = view.cmp;
  if (!st) return;
  const target = cmpTarget();
  const on = st.components.filter((c) => c.installed).length;
  $("#cmp-state").textContent = st.sparse ? t("cmp.partial", { n: on, m: st.components.length }) : t("cmp.full");
  $("#cmp-presets").innerHTML =
    `<span class="text-xs text-fg-3">${esc(t("cmp.presets"))}</span>` +
    st.presets
      .map((p) => `<button class="btn-std" data-preset="${esc(p.name)}" title="${esc(p.components.join(", "))}">${esc(p.title || p.name)}</button>`)
      .join("");
  $$("#cmp-presets [data-preset]").forEach((b) => b.addEventListener("click", () => cmpPreset(b.dataset.preset)));
  $("#cmp-list").innerHTML = st.components
    .map((c) => {
      const inTarget = target.has(c.name);
      const by = [...cmpChosen].filter((x) => x !== c.name && cmpNeeds(x).has(c.name));
      const locked = c.base || (!cmpChosen.has(c.name) && by.length > 0);
      const why = c.base ? t("cmp.always") : !cmpChosen.has(c.name) && by.length ? t("cmp.neededby", { by: by.join(", ") }) : "";
      const [lvl, state] = c.installed
        ? inTarget ? ["ok", "cmp.on"] : ["warn", "cmp.remove"]
        : inTarget ? ["done", "cmp.add"] : ["note", "cmp.off"];
      const needs = c.needs.length ? `${esc(t("cmp.needs"))}${esc(c.needs.join(", "))}` : "";
      const ext = c.external.length ? ` <span class="text-caution">${esc(t("cmp.programs"))}${esc(c.external.join(", "))}</span>` : "";
      return `<label class="finding cursor-default items-center" data-level="${lvl}">
        <input type="checkbox" class="size-4 shrink-0 accent-accent" data-cmp="${esc(c.name)}" ${inTarget ? "checked" : ""} ${locked ? "disabled" : ""}>
        <div class="min-w-0 flex-1"><div><span class="font-semibold">${esc(c.title)}</span>
          <span class="text-fg-3"> · ${esc(c.name)} · ${esc(t("cmp.files", { n: c.files }))}</span>${why ? ` <span class="text-xs text-fg-3">(${esc(why)})</span>` : ""}</div>
          <div class="text-xs text-fg-3 select-text">${esc(c.about)}</div>
          <div class="text-xs text-fg-3">${needs}${ext}</div></div>
        <span class="lvl">${esc(t(state))}</span></label>`;
    })
    .join("");
  $$("#cmp-list [data-cmp]").forEach((i) =>
    i.addEventListener("change", () => {
      if (i.checked) cmpChosen.add(i.dataset.cmp);
      else cmpChosen.delete(i.dataset.cmp);
      renderComponents();
    }),
  );
  if (view.cmpResult) findings(view.cmpResult, $("#cmp-result"), "cmp.nothing");
}

function cmpPreset(name) {
  const p = view.cmp.presets.find((x) => x.name === name);
  if (!p) return;
  const nonBase = view.cmp.components.filter((c) => !c.base).map((c) => c.name);
  cmpChosen = new Set(p.components.includes("all") ? nonBase : p.components);
  renderComponents();
}

async function loadComponents() {
  try {
    view.cmp = await invoke("components");
  } catch (e) {
    $("#cmp-list").innerHTML = `<div class="px-4 py-3 text-fg-3">${esc(String(e))}</div>`;
    return;
  }
  // start from what is installed now; a component another installed one needs is shown as "needed by", not chosen
  const inst = view.cmp.components.filter((c) => c.installed && !c.base).map((c) => c.name);
  const needed = new Set(inst.flatMap((n) => [...cmpNeeds(n)]));
  cmpChosen = new Set(inst.filter((n) => !needed.has(n)));
  renderComponents();
}

async function componentsSet(apply) {
  if (!view.cmp) return;
  const target = cmpTarget();
  // everything chosen: back to a full checkout
  const names = target.size === view.cmp.components.length ? ["all"] : cmpChosen.size ? [...cmpChosen] : ["base"];
  await guard(t(apply ? "cmp.applying" : "cmp.planning"), async () => {
    const list = await invoke("components_set", { names, apply });
    view.cmpResult = list;
    $("#cmp-result").classList.remove("hidden");
    const c = counts(list);
    if (apply && !c.error) {
      await loadComponents();
      loadInfo();
      status(t("cmp.applied"));
    } else {
      renderComponents();
      status(t("cmp.status", { error: c.error, warn: c.warn }));
    }
  });
}

function renderSecrets() {
  if (!view.secrets) return;
  const [db, list] = view.secrets;
  $("#secrets-db").textContent = t("sec.db") + db;
  $("#secrets-list").innerHTML = list
    .map(
      (s) => `<div class="finding"><span class="lvl ${s.required ? "bg-accent/15 text-accent-text" : "bg-white/[0.06] text-fg-3"}">${esc(t(s.required ? "sec.required" : "sec.optional"))}</span>
      <div class="min-w-0"><div><span class="font-semibold">${esc(s.place)}</span>
        <span class="text-fg-3"> — ${esc(t(s.kind === "file" ? "sec.attachment" : "sec.field"))} ${esc(s.target)} → ${esc(s.env)}</span>${s.rotate ? ` <span class="text-caution">${esc(t("sec.rotate"))}</span>` : ""}</div>
      <div class="text-xs text-fg-3">${esc(s.description)} [${esc(s.used_by.join(", "))}]</div></div></div>`,
    )
    .join("");
}

async function loadSecrets() {
  view.secrets = await invoke("secrets_list");
  renderSecrets();
}

function renderSecretsResult() {
  if (view.secretsResult) findings(view.secretsResult, $("#secrets-result"), "sec.empty");
}

async function secretsCheck() {
  const password = $("#master").value;
  if (!password) {
    status(t("sec.enter"));
    return;
  }
  await guard(t("sec.checking"), async () => {
    const list = await invoke("secrets_check", { password });
    $("#master").value = "";
    $("#secrets-result").classList.remove("hidden");
    view.secretsResult = list;
    renderSecretsResult();
    const c = counts(list);
    status(t("sec.status", { error: c.error, warn: c.warn, ok: c.ok }));
  });
}

async function backup(action) {
  await guard(t(action === "run" ? "bk.running" : "bk.reading"), async () => {
    // the output replaces the hint: it is no longer a translated text
    $("#backup-out").removeAttribute("data-t");
    try {
      $("#backup-out").textContent = await invoke("backup", { action });
      status(t(action === "run" ? "bk.done" : "bk.read"));
    } catch (e) {
      $("#backup-out").textContent = String(e);
      throw t("bk.failed");
    }
  });
}

async function loadInfo() {
  try {
    const i = await invoke("info");
    view.info = i;
    renderInfo();
    // machine profile comes from local.toml [setup] profile; "Fix" with another profile remembers the new one
    if (i.profile) $("#profile").value = i.profile;
    return true;
  } catch (e) {
    return false;
  }
}

function renderInfo() {
  const i = view.info;
  if (!i) return;
  $("#ws-root").textContent = i.root;
  $("#ws-head").textContent = `${i.branch} · ${i.head}${i.dirty ? t("info.changed", { n: i.dirty }) : ""}`;
  $("#ws-root").title = `home: ${i.home}\nvault: ${i.vault}\norigin: ${i.origin}\n${t("info.members")}${i.members}`;
}

function switchLanguage() {
  LANG = LANG === "en" ? "ru" : "en";
  try { localStorage.setItem("devkit.lang", LANG); } catch {}
  applyI18n();
  renderOverview();
  renderCheck();
  renderMembers();
  renderComponents();
  renderStart();
  renderWork();
  renderGame();
  renderSecrets();
  renderSecretsResult();
  renderInfo();
  status("");
}

async function start() {
  applyI18n();
  $("#lang").addEventListener("click", switchLanguage);
  $$("[data-page]").forEach((b) => b.addEventListener("click", () => show(b.dataset.page)));
  $("#doctor").addEventListener("click", () => doctor(false));
  $("#bootstrap").addEventListener("click", () => doctor(true));
  $("#check").addEventListener("click", check);
  $("#me-save").addEventListener("click", saveMe);
  $("#ag-start").addEventListener("click", startAgent);
  $$("#ag-modes [data-mode]").forEach((b) =>
    b.addEventListener("click", () => {
      agMode = b.dataset.mode;
      renderStart();
    }),
  );
  $("#wk-refresh").addEventListener("click", loadWork);
  $("#gm-refresh").addEventListener("click", loadGame);
  $("#gm-plan").addEventListener("click", () => gameAction("dump_plan"));
  $("#gm-dump").addEventListener("click", () => gameAction("dump"));
  $("#gm-unlock").addEventListener("click", () => vault("unlock"));
  $("#gm-lock").addEventListener("click", () => vault("lock"));
  $("#gm-pass").addEventListener("keydown", (e) => e.key === "Enter" && vault("unlock"));
  $("#cmp-plan").addEventListener("click", () => componentsSet(false));
  $("#cmp-apply").addEventListener("click", () => componentsSet(true));
  $("#secrets-check").addEventListener("click", secretsCheck);
  $("#master").addEventListener("keydown", (e) => e.key === "Enter" && secretsCheck());
  $("#snapshots").addEventListener("click", () => backup("snapshots"));
  $("#backup-run").addEventListener("click", () => backup("run"));
  $$("[data-open]").forEach((b) =>
    b.addEventListener("click", async () => {
      try {
        await invoke("open", { what: b.dataset.open });
        status(t("ln.opened") + b.querySelector(".font-semibold").textContent);
      } catch (e) {
        status(t("ln.failed") + e);
      }
    }),
  );
  $("#pick-root").addEventListener("click", async () => {
    try {
      await invoke("pick_root");
      location.reload();
    } catch (e) {
      status(String(e));
    }
  });
  if (!(await loadInfo())) {
    $$("[data-page-body]").forEach((s) => s.classList.add("hidden"));
    $("#page-none").classList.remove("hidden");
    status(t("ws.none"));
    return;
  }
  await loadMe();
  let page = "start";
  try { page = localStorage.getItem("devkit.page") || page; } catch {}
  show(page);
  doctor(false);
}

start();
