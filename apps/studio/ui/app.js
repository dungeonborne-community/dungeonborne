"use strict";
// All text from a kit goes through textContent: a kit is someone else's file.

const { invoke, convertFileSrc } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const $ = (id) => document.getElementById(id);
const ROW_H = 36;
const NAME_W = 300;
const COL_W = 104;
const LABEL_W = 420;
const VALUE_W = 120;
const PAGE = 100;
const OVERSCAN_ROWS = 6;
const OVERSCAN_COLS = 2;
const HEAD_H = 64;

const S = {
  lang: "en",
  recent: [],
  kit: null,
  author: "",
  edits: null, // status of my changes: count, errors, warnings, can_undo, can_redo, comment
  view: "summary", // "summary" | "changes" | table name
  filters: {}, // table -> filter state
};

// ------------------------------------------------------------------------------------------------- texts

const t = (key) => window.I18N[S.lang][key] ?? window.I18N.en[key] ?? key;

/** "2 строки", "5 строк", "1 row": the forms are one / few / many, English uses the first two */
function count(n, key) {
  const forms = t(key);
  const [n10, n100] = [n % 10, n % 100];
  let form = 2;
  if (S.lang === "en") form = n === 1 ? 0 : 1;
  else if (n10 === 1 && n100 !== 11) form = 0;
  else if (n10 >= 2 && n10 <= 4 && (n100 < 12 || n100 > 14)) form = 1;
  return `${n.toLocaleString(S.lang)} ${forms[form]}`;
}

/** Text with {name} substituted */
function tf(key, values) {
  return t(key).replace(/\{(\w+)\}/g, (_, name) => values[name] ?? "");
}

function applyTexts() {
  document.documentElement.lang = S.lang;
  for (const el of document.querySelectorAll("[data-t]")) el.textContent = t(el.dataset.t);
  for (const el of document.querySelectorAll("[data-title]")) el.title = t(el.dataset.title);
  $("f-search").placeholder = t("grid.search");
  $("it-search").placeholder = t("grid.search");
  $("bk-search").placeholder = t("bulk.fields_search");
}

function clone(id) {
  const root = document.getElementById(id).content.firstElementChild.cloneNode(true);
  const refs = { root };
  for (const el of root.querySelectorAll("[data-ref]")) refs[el.dataset.ref] = el;
  return refs;
}

/** Small element builder; text always goes through textContent */
function h(tag, className = "", text) {
  const el = document.createElement(tag);
  if (className) el.className = className;
  if (text !== undefined) el.textContent = text;
  return el;
}

function pill(text, tone) {
  const el = document.createElement("span");
  el.className = "pill";
  if (tone) el.dataset.tone = tone;
  el.textContent = text;
  return el;
}

// ------------------------------------------------------------------------------------------------- errors, busy

function showError(e) {
  const code = e?.code ?? "other";
  $("error-text").textContent = window.I18N[S.lang][`err.${code}`] ?? t("err.other");
  $("error-detail").textContent = e?.message ?? String(e);
  $("error").classList.remove("hidden");
}

function clearError() {
  $("error").classList.add("hidden");
}

function setBusy(on) {
  $("busy").classList.toggle("hidden", !on);
  $("busy").classList.toggle("grid", on);
}

// ------------------------------------------------------------------------------------------------- pages

function showPage() {
  const open = !!S.kit;
  $("page-start").classList.toggle("hidden", open);
  $("page-kit").classList.toggle("hidden", !open);
  $("page-kit").classList.toggle("flex", open);
  $("head-kit").textContent = open ? `${S.kit.title} · ${S.kit.version}` : "";
  $("undo").classList.toggle("hidden", !open);
  $("redo").classList.toggle("hidden", !open);
  if (open) {
    updateStatus(S.edits);
    renderNav();
    showView(S.view);
  } else {
    renderRecent();
  }
}

function renderRecent() {
  $("recent-box").classList.toggle("hidden", S.recent.length === 0);
  const list = S.recent.map((r) => {
    const x = clone("tpl-recent");
    x.title.textContent = r.title || r.version;
    x.meta.textContent = r.season ? `${r.version} · ${r.season}` : r.version;
    x.open.addEventListener("click", () => openWith(() => invoke("open_recent", { version: r.version })));
    return x.root;
  });
  $("recent").replaceChildren(...list);
}

function renderNav() {
  $("nav-items").classList.toggle("hidden", !S.kit.features?.items);
  $("nav-curves").classList.toggle("hidden", !S.kit.features?.curves);
  $("nav-loot").classList.toggle("hidden", !S.kit.features?.loot);
  // Tables go together by group, in the order the kit gives; an old kit has none and stays a flat list
  const order = [...new Set(S.kit.tables.map((tb) => tb.group ?? ""))];
  const sorted = order.flatMap((g) => S.kit.tables.filter((tb) => (tb.group ?? "") === g));
  const items = [];
  let group = "";
  for (const tb of sorted) {
    if ((tb.group ?? "") !== group) {
      group = tb.group ?? "";
      if (group) items.push(h("div", "mt-2 px-3 pb-0.5 text-[11px] font-semibold text-fg-4", window.I18N[S.lang][`group.${group}`] ?? group));
    }
    items.push(navItem(tb));
  }
  $("nav-tables").replaceChildren(...items);
}

function navItem(tb) {
    const x = clone("tpl-nav");
    x.root.dataset.table = tb.name;
    x.title.textContent = tb.title;
    x.root.title = tb.title;
    if (tb.changed + tb.new_rows > 0) {
      x.count.textContent = String(tb.changed + tb.new_rows);
      x.count.classList.remove("hidden");
    }
    x.root.addEventListener("click", () => showView(tb.name));
    return x.root;
}

function showView(view) {
  commitEdit();
  S.view = view;
  const isSummary = view === "summary";
  const isChanges = view === "changes";
  const isItems = view === "items";
  const isCurves = view === "curves";
  const isLoot = view === "loot";
  const isTable = !isSummary && !isChanges && !isItems && !isCurves && !isLoot;
  $("nav-summary").dataset.active = String(isSummary);
  $("nav-changes").dataset.active = String(isChanges);
  $("nav-items").dataset.active = String(isItems);
  $("nav-curves").dataset.active = String(isCurves);
  $("nav-loot").dataset.active = String(isLoot);
  for (const el of $("nav-tables").children) el.dataset.active = String(el.dataset.table === view);
  $("view-summary").classList.toggle("hidden", !isSummary);
  $("view-changes").classList.toggle("hidden", !isChanges);
  $("view-items").classList.toggle("hidden", !isItems);
  $("view-items").classList.toggle("flex", isItems);
  $("view-curves").classList.toggle("hidden", !isCurves);
  $("view-loot").classList.toggle("hidden", !isLoot);
  $("view-loot").classList.toggle("flex", isLoot);
  $("view-table").classList.toggle("hidden", !isTable);
  $("view-table").classList.toggle("flex", isTable);
  hideTip();
  if (isSummary) renderSummary();
  else if (isChanges) renderChanges();
  else if (isItems) openItems();
  else if (isCurves) openCurves();
  else if (isLoot) openLoot();
  else openTable(view);
}

/** The status of my changes comes back from every command that changes them */
function updateStatus(st) {
  if (!st) return;
  const { changes: _list, ...status } = st;
  S.edits = status;
  const badge = $("nav-changes-count");
  badge.textContent = String(st.count);
  badge.classList.toggle("hidden", st.count === 0);
  badge.dataset.tone = st.errors > 0 ? "error" : "";
  $("undo").disabled = !st.can_undo;
  $("redo").disabled = !st.can_redo;
  if (st.draft_error) showError({ code: "draft_io", message: st.draft_error });
}

function renderSummary() {
  const k = S.kit;
  $("sum-title").textContent = k.title;
  $("sum-version").textContent = k.version;
  $("sum-season").textContent = k.season ? `${t("summary.season")}: ${k.season}` : t("summary.base");
  $("sum-nocat").classList.toggle("hidden", k.has_catalog);
  const rows = k.tables.map((tb) => {
    const x = clone("tpl-sum-table");
    x.title.textContent = tb.title;
    x.about.textContent = tb.about;
    const tags = [pill(count(tb.rows, "count.rows"))];
    tags.push(pill(tb.editable_values ? `${t("summary.editable")}: ${tb.editable_values.toLocaleString(S.lang)}` : t("summary.readonly")));
    if (tb.source === "season") tags.push(pill(t("summary.from_season")));
    if (!tb.has_orig) tags.push(pill(t("summary.noorig")));
    else if (!tb.changed && !tb.new_rows && !tb.removed_rows) tags.push(pill(t("summary.same")));
    if (tb.changed) tags.push(pill(`${t("summary.changed")}: ${tb.changed.toLocaleString(S.lang)}`, "caution"));
    if (tb.new_rows) tags.push(pill(`${t("summary.new")}: ${tb.new_rows.toLocaleString(S.lang)}`, "ok"));
    if (tb.removed_rows) tags.push(pill(`${t("summary.removed")}: ${tb.removed_rows.toLocaleString(S.lang)}`, "caution"));
    x.tags.replaceChildren(...tags);
    x.root.addEventListener("click", () => showView(tb.name));
    return x.root;
  });
  $("sum-tables").replaceChildren(...rows);
  const notes = (k.notes ?? []).map((n) => {
    const li = document.createElement("li");
    li.textContent = n;
    return li;
  });
  $("sum-notes-box").classList.toggle("hidden", notes.length === 0);
  $("sum-notes").replaceChildren(...notes);
}

// ------------------------------------------------------------------------------------------------- opening a kit

async function openWith(call) {
  clearError();
  setBusy(true);
  try {
    S.kit = await call();
    S.edits = S.kit.edits;
    S.view = "summary";
    S.filters = {};
    IT.id = null;
    IT.rarity = "";
    CV.path = "";
    CV.shown = null;
    await refreshState();
    showPage();
    if (S.kit.draft_dropped > 0) showError({ code: "kit_draft", message: tf("draft.dropped", { n: S.kit.draft_dropped }) });
    if (S.kit.rebase_from) openRebase();
  } catch (e) {
    showError(e);
    await refreshState();
    renderRecent();
  } finally {
    setBusy(false);
  }
}

async function refreshState() {
  const st = await invoke("app_state");
  S.recent = st.recent;
  S.author = st.author;
}

async function pick(kind) {
  try {
    const path = await invoke("pick_kit", { kind });
    if (path) await openWith(() => invoke("open_kit", { path }));
  } catch (e) {
    showError(e);
  }
}

// ------------------------------------------------------------------------------------------------- grid
//
// Vertical and horizontal virtual scrolling: only visible rows and cells exist. Rows come in pages from Rust, which
// keeps the filtered and sorted list. `seq` drops answers to a query that is no longer current.

const G = { table: null, seq: 0, total: 0, long: false, columns: [], pages: new Map(), loading: new Set(), rendered: new Map(), range: "", width: 0, frame: 0, cursor: null, editing: null, selected: new Set(), anchor: null };
const IT = { id: null, rarity: "", search: "", kind: "", mine: false, rarityFilter: "", affixSearch: "", affixSide: "major", data: null };
const CV = { path: "", shown: null, data: null, scale: null, drag: null };

function filtersOf(table) {
  return (S.filters[table] ??= { view: "", search: "", only_diff: false, only_mine: false, all_fields: false, kind: "", rarity: "", sort: "", desc: false, top: 0, left: 0 });
}

function queryOf(f) {
  return { view: f.view, search: f.search, only_diff: f.only_diff, only_mine: f.only_mine, all_fields: f.all_fields, kind: f.kind, rarity: f.rarity, sort: f.sort, desc: f.desc, lang: S.lang };
}

function openTable(name) {
  const tb = S.kit.tables.find((x) => x.name === name);
  if (!tb) return;
  $("t-title").textContent = tb.title;
  $("t-about").textContent = tb.where ? `${tb.about} ${t("grid.where")}: ${tb.where}.` : tb.about;
  const f = filtersOf(name);
  $("f-search").value = f.search;
  G.table = name;
  reload({ keepScroll: true });
}

async function reload({ keepScroll = false } = {}) {
  const f = filtersOf(G.table);
  const seq = ++G.seq;
  cancelEdit();
  G.cursor = null;
  G.selected = new Set();
  G.anchor = null;
  G.pages = new Map();
  G.loading = new Set();
  let first;
  try {
    first = await invoke("query_rows", { table: G.table, query: queryOf(f), offset: 0, limit: PAGE });
  } catch (e) {
    showError(e);
    return;
  }
  if (seq !== G.seq) return;
  G.total = first.total;
  G.long = first.long;
  G.columns = first.columns ?? [];
  G.pages.set(0, first.rows);
  renderToolbar(f, first);
  layoutGrid();
  renderHead(f);
  clearRows();
  const grid = $("grid");
  grid.scrollTop = keepScroll ? f.top : 0;
  grid.scrollLeft = keepScroll ? f.left : 0;
  renderWindow();
}

function renderToolbar(f, page) {
  $("f-wide").dataset.active = String(!page.long);
  $("f-long").dataset.active = String(page.long);
  $("f-diff").dataset.active = String(f.only_diff);
  $("f-diff").classList.toggle("hidden", !hasOriginal(G.table));
  $("f-mine").dataset.active = String(f.only_mine);
  $("f-all").dataset.active = String(f.all_fields);
  $("f-all").classList.toggle("hidden", !page.has_all_fields);
  $("f-bulk").classList.toggle("hidden", !(S.kit.tables.find((x) => x.name === G.table)?.editable_values > 0));
  fillCombo($("f-kind"), t("grid.kind_any"), (page.kinds ?? []).map((k) => ({ id: k, label: window.I18N[S.lang][`kind.${k}`] ?? k })), f.kind);
  fillCombo($("f-rarity"), t("grid.rarity_any"), page.rarities ?? [], f.rarity);
  $("t-count").textContent = count(G.total, G.long ? "count.values" : "count.rows");
  $("t-note").classList.toggle("hidden", !page.columns_matched);
  $("grid-empty").classList.toggle("hidden", G.total > 0);
}

function fillCombo(el, anyLabel, options, value) {
  const make = (id, label) => {
    const o = document.createElement("option");
    o.value = id;
    o.textContent = label;
    return o;
  };
  el.replaceChildren(make("", anyLabel), ...options.map((o) => make(o.id, o.label)));
  el.value = value;
  el.classList.toggle("hidden", options.length === 0);
}

function layoutGrid() {
  const fixed = G.long ? LABEL_W + VALUE_W * 2 : G.columns.length * COL_W;
  G.width = NAME_W + fixed;
  $("grid-head").style.width = `${G.width}px`;
  $("grid-body").style.width = `${G.width}px`;
  $("grid-body").style.height = `${G.total * ROW_H}px`;
}

function clearRows() {
  G.rendered.clear();
  G.range = "";
  for (const el of [...$("grid-body").children]) if (el.id !== "cell-input") el.remove();
}

// long view has three fixed columns: field, value, value in the game
function longColumns() {
  return [
    { kind: "label", left: 0, w: LABEL_W, sort: "label", text: t("grid.label") },
    { kind: "value", left: LABEL_W, w: VALUE_W, sort: "value", text: t("grid.value") },
    { kind: "orig", left: LABEL_W + VALUE_W, w: VALUE_W, sort: "", text: t("grid.orig") },
  ];
}

function headCell(text, sortKey, f, left, w, extra) {
  const el = document.createElement("div");
  el.className = "head-cell";
  el.style.left = `${left}px`;
  el.style.width = `${w}px`;
  const label = document.createElement("span");
  label.className = "line-clamp-3 min-w-0 flex-1";
  label.textContent = text;
  el.append(label);
  if (sortKey) {
    el.dataset.sort = sortKey;
    if (f.sort === sortKey) {
      const arrow = document.createElement("span");
      arrow.className = "ico text-[10px]! text-accent";
      arrow.textContent = f.desc ? "" : "";
      el.append(arrow);
    }
  } else {
    el.dataset.nosort = "true";
  }
  if (extra) Object.assign(el.dataset, extra);
  return el;
}

function renderHead(f) {
  const head = $("grid-head");
  const name = headCell(t("grid.name"), "name", f, 0, NAME_W);
  name.classList.add("head-name");
  head.replaceChildren(name);
  if (G.long) {
    for (const c of longColumns()) head.append(headCell(c.text, c.sort, f, NAME_W + c.left, c.w));
  }
  G.headFor = null;
  renderHeadCells();
}

/** Wide view: only the header cells that are in sight */
function renderHeadCells() {
  if (G.long) return;
  const [from, to] = visibleCols();
  const key = `${from}:${to}`;
  if (G.headFor === key) return;
  G.headFor = key;
  const f = filtersOf(G.table);
  const head = $("grid-head");
  while (head.children.length > 1) head.lastChild.remove();
  for (let i = from; i < to; i++) {
    const c = G.columns[i];
    const el = headCell(c.label, String(c.id), f, NAME_W + i * COL_W, COL_W, { col: String(i) });
    el.dataset.st = c.editable ? "edit" : "locked";
    head.append(el);
  }
}

function visibleCols() {
  const grid = $("grid");
  const left = grid.scrollLeft;
  const from = Math.max(0, Math.floor(left / COL_W) - OVERSCAN_COLS);
  const to = Math.min(G.columns.length, Math.ceil((left + grid.clientWidth - NAME_W) / COL_W) + OVERSCAN_COLS);
  return [from, to];
}

function rowAt(index) {
  const page = G.pages.get(Math.floor(index / PAGE));
  return page ? page[index % PAGE] : undefined;
}

function ensurePage(n) {
  if (G.pages.has(n) || G.loading.has(n)) return;
  G.loading.add(n);
  const seq = G.seq;
  invoke("query_rows", { table: G.table, query: queryOf(filtersOf(G.table)), offset: n * PAGE, limit: PAGE })
    .then((page) => {
      if (seq !== G.seq) return;
      G.pages.set(n, page.rows);
      G.loading.delete(n);
      G.range = "";
      for (const [i, el] of G.rendered) {
        if (Math.floor(i / PAGE) === n) {
          el.remove();
          G.rendered.delete(i);
        }
      }
      renderWindow();
    })
    .catch((e) => {
      G.loading.delete(n);
      showError(e);
    });
}

function isPercent(field, row) {
  return field.value_like && (field.unit === "доля" || row.unit === "доля");
}

function numberText(v) {
  return String(Number(Number(v).toPrecision(7)));
}

/** A share is kept as 0.05 and shown as 5 %; the tooltip shows the stored value */
function shown(value, field, row) {
  if (value === "" || value === undefined) return "";
  if (isPercent(field, row) && value.trim() !== "" && !Number.isNaN(Number(value))) return `${numberText(Number(value) * 100)} %`;
  return value;
}

function makeCell(left, w, cell, field, row) {
  const el = document.createElement("div");
  el.className = "cell";
  el.style.left = `${left}px`;
  el.style.width = `${w}px`;
  el.dataset.st = field.editable ? "edit" : "locked";
  if (cell?.n) el.dataset.diff = "new";
  else if (cell && cell.o !== undefined) el.dataset.diff = "changed";
  if (cell && cell.k !== undefined) el.dataset.mine = "true";
  if (cell?.e) el.dataset.issue = "error";
  else if (cell?.w) el.dataset.issue = "warning";
  el.textContent = cell ? shown(cell.v, field, row) : "";
  return el;
}

function makeRow(index, row) {
  const el = document.createElement("div");
  el.className = "grid-row";
  el.style.top = `${index * ROW_H}px`;
  el.style.width = `${G.width}px`;
  el.dataset.index = String(index);
  const name = document.createElement("div");
  name.className = "name-cell";
  if (row && G.selected.has(row.row)) name.dataset.selected = "true";
  if (row) {
    if (row.icon) {
      const img = document.createElement("img");
      img.className = "size-6 shrink-0 rounded-[3px] object-contain";
      img.alt = "";
      img.loading = "lazy";
      img.src = convertFileSrc(`${S.kit.version}/${row.icon}`, "kit");
      name.append(img);
    }
    const title = document.createElement("span");
    title.className = "min-w-0 flex-1 truncate";
    title.textContent = row.title || row.key;
    name.append(title);
    if (row.service) name.append(rowTag(t("grid.service")));
    if (row.new) name.append(rowTag(t("grid.new")));
    // A row without a name is shown by its key, once
    if (row.title) {
      const key = document.createElement("span");
      key.className = "shrink-0 font-mono text-[11px] text-fg-4 select-text";
      key.textContent = row.key;
      name.append(key);
    }
  } else {
    name.classList.add("text-fg-4");
    name.textContent = t("grid.loading");
  }
  el.append(name);
  return el;
}

function rowTag(text) {
  const s = document.createElement("span");
  s.className = "shrink-0 rounded-[3px] bg-white/[0.08] px-1 text-[10px] text-fg-3";
  s.textContent = text;
  return s;
}

function fillRowCells(el, row, from, to) {
  while (el.children.length > 1) el.lastChild.remove();
  if (!row) return;
  if (G.long) {
    const field = row.field;
    const label = document.createElement("div");
    label.className = "cell justify-start! text-fg-2";
    label.style.left = `${NAME_W}px`;
    label.style.width = `${LABEL_W}px`;
    label.textContent = field.label;
    label.dataset.path = field.path;
    const value = makeCell(NAME_W + LABEL_W, VALUE_W, row.cell, field, row);
    value.dataset.col = "0";
    el.append(label, value);
    const orig = makeCell(NAME_W + LABEL_W + VALUE_W, VALUE_W, row.cell?.o !== undefined ? { v: row.cell.o } : null, { ...field, editable: false }, row);
    if (row.cell?.o === "") orig.textContent = t("tip.orig_default");
    if (row.cell?.n) orig.textContent = "—";
    el.append(orig);
    return;
  }
  for (let i = from; i < to; i++) {
    const cell = makeCell(NAME_W + i * COL_W, COL_W, row.cells?.[i], G.columns[i], row);
    cell.dataset.col = String(i);
    el.append(cell);
  }
}

function renderWindow() {
  const grid = $("grid");
  const f = filtersOf(G.table);
  f.top = grid.scrollTop;
  f.left = grid.scrollLeft;
  const bodyTop = Math.max(0, grid.scrollTop);
  const first = Math.max(0, Math.floor(bodyTop / ROW_H) - OVERSCAN_ROWS);
  const last = Math.min(G.total, Math.ceil((bodyTop + grid.clientHeight) / ROW_H) + OVERSCAN_ROWS);
  const [from, to] = G.long ? [0, 0] : visibleCols();
  const colKey = `${from}:${to}`;
  const colsChanged = G.range !== colKey;
  G.range = colKey;
  renderHeadCells();

  for (let p = Math.floor(first / PAGE); p <= Math.floor(Math.max(first, last - 1) / PAGE); p++) ensurePage(p);

  for (const [i, el] of G.rendered) {
    if (i < first || i >= last) {
      el.remove();
      G.rendered.delete(i);
    }
  }
  const body = $("grid-body");
  for (let i = first; i < last; i++) {
    let el = G.rendered.get(i);
    const row = rowAt(i);
    if (!el) {
      el = makeRow(i, row);
      fillRowCells(el, row, from, to);
      G.rendered.set(i, el);
      body.append(el);
    } else if (colsChanged) {
      fillRowCells(el, row, from, to);
    }
  }
  paintCursor();
}

// ------------------------------------------------------------------------------------------------- cursor and editing
//
// As in a spreadsheet: click or arrows select, typing replaces, F2 or a double click edits, Enter and Tab confirm and
// move, Esc cancels, Delete returns the kit's value. A share is typed and shown in percent and stored as a share.

function cursorData(c = G.cursor) {
  if (!c) return null;
  const row = rowAt(c.index);
  if (!row) return null;
  return G.long ? { row, field: row.field, cell: row.cell } : { row, field: G.columns[c.col], cell: row.cells?.[c.col] ?? null };
}

function paintCursor() {
  for (const el of $("grid-body").querySelectorAll('[data-cur="true"]')) delete el.dataset.cur;
  if (!G.cursor) return;
  const el = G.rendered.get(G.cursor.index)?.querySelector(`[data-col="${G.cursor.col}"]`);
  if (el) el.dataset.cur = "true";
}

function revealCursor() {
  const grid = $("grid");
  const y = HEAD_H + G.cursor.index * ROW_H;
  if (y < grid.scrollTop + HEAD_H) grid.scrollTop = y - HEAD_H;
  else if (y + ROW_H > grid.scrollTop + grid.clientHeight) grid.scrollTop = y + ROW_H - grid.clientHeight;
  if (!G.long) {
    const x = NAME_W + G.cursor.col * COL_W;
    if (x < grid.scrollLeft + NAME_W) grid.scrollLeft = x - NAME_W;
    else if (x + COL_W > grid.scrollLeft + grid.clientWidth) grid.scrollLeft = x + COL_W - grid.clientWidth;
  }
}

function setCursor(index, col) {
  if (G.total === 0) return;
  const maxCol = G.long ? 0 : G.columns.length - 1;
  G.cursor = { index: Math.max(0, Math.min(G.total - 1, index)), col: Math.max(0, Math.min(maxCol, col)) };
  paintCursor();
  revealCursor();
  renderWindow();
}

function moveCursor(rows, cols) {
  if (G.cursor) setCursor(G.cursor.index + rows, G.cursor.col + cols);
  else setCursor(0, 0);
}

function setStatus(text) {
  $("t-status").textContent = text;
}

function startEdit(initial) {
  const data = cursorData();
  if (!data) return;
  if (!data.field.editable) return setStatus(t("tip.locked"));
  hideTip();
  revealCursor();
  const percent = isPercent(data.field, data.row);
  const input = $("cell-input");
  const w = G.long ? VALUE_W : COL_W;
  input.style.left = `${NAME_W + (G.long ? LABEL_W : G.cursor.col * COL_W)}px`;
  input.style.top = `${G.cursor.index * ROW_H}px`;
  input.style.width = `${w}px`;
  const stored = data.cell?.v ?? "";
  const start = percent && stored !== "" ? numberText(Number(stored) * 100) : stored;
  input.value = initial ?? start;
  G.editing = { table: G.table, row: data.row.row, path: data.field.path, percent, start };
  input.classList.remove("hidden");
  input.focus({ preventScroll: true });
  if (initial === undefined) input.select();
}

/** What the person typed -> the stored value. Empty returns the kit's value. */
function parseTyped(text, percent) {
  const v = text.trim();
  if (!percent || v === "") return v;
  const n = Number(v.replace(/\s*%$/, "").replace(",", "."));
  return Number.isNaN(n) ? v : String(Number((n / 100).toPrecision(10)));
}

function closeEditor() {
  G.editing = null;
  $("cell-input").classList.add("hidden");
}

function cancelEdit() {
  if (G.editing) closeEditor();
}

/** Returns a promise only when something was sent */
function commitEdit() {
  const e = G.editing;
  if (!e) return null;
  const typed = $("cell-input").value;
  closeEditor();
  if (typed.trim() === e.start.trim()) return null;
  return sendEdits([{ table: e.table, row: e.row, path: e.path, new: parseTyped(typed, e.percent) }]);
}

async function sendEdits(edits) {
  try {
    const st = await invoke("set_values", { edits });
    updateStatus(st);
    await refreshScreen();
    return st;
  } catch (err) {
    showError(err);
    return null;
  }
}

/** After any change of my edits: whatever is on screen is read again */
async function refreshScreen() {
  if (S.view === "changes") await renderChanges();
  else if (S.view === "items") await refreshItems();
  else if (S.view === "curves") await loadCurves();
  else if (S.view === "loot") await refreshLoot();
  else if (S.view === G.table) await refreshPages();
}

/** The list stays as it is on screen, cells and marks are read again */
async function refreshPages() {
  if (!G.table || S.view !== G.table) return;
  const grid = $("grid");
  const first = Math.floor(Math.max(0, grid.scrollTop) / ROW_H);
  const last = Math.ceil((grid.scrollTop + grid.clientHeight) / ROW_H);
  const pages = new Set();
  for (let p = Math.floor(first / PAGE); p <= Math.floor(last / PAGE); p++) pages.add(p);
  pages.add(0);
  const seq = G.seq;
  const query = queryOf(filtersOf(G.table));
  const answers = await Promise.all([...pages].map((n) => invoke("query_rows", { table: G.table, query, offset: n * PAGE, limit: PAGE }).then((r) => [n, r])));
  if (seq !== G.seq) return;
  G.pages = new Map(answers.map(([n, r]) => [n, r.rows]));
  G.loading = new Set();
  G.total = answers[0][1].total;
  $("t-count").textContent = count(G.total, G.long ? "count.values" : "count.rows");
  $("grid-empty").classList.toggle("hidden", G.total > 0);
  layoutGrid();
  if (G.cursor && G.cursor.index >= G.total) G.cursor = G.total ? { ...G.cursor, index: G.total - 1 } : null;
  clearRows();
  renderWindow();
}

async function pageRows(n) {
  if (!G.pages.has(n)) {
    const r = await invoke("query_rows", { table: G.table, query: queryOf(filtersOf(G.table)), offset: n * PAGE, limit: PAGE });
    G.pages.set(n, r.rows);
  }
  return G.pages.get(n);
}

/** A block from Excel: tabs between cells, new lines between rows, starting at the cursor. Locked cells are skipped. */
async function pasteBlock(text) {
  if (!G.cursor) return;
  const lines = text.replace(/\r\n?/g, "\n").replace(/\n$/, "").split("\n").map((l) => l.split("\t"));
  const edits = [];
  let skipped = 0;
  for (let r = 0; r < lines.length; r++) {
    const index = G.cursor.index + r;
    if (index >= G.total) {
      skipped += lines[r].length;
      continue;
    }
    const rows = await pageRows(Math.floor(index / PAGE));
    const row = rows[index % PAGE];
    for (let c = 0; c < lines[r].length; c++) {
      const field = G.long ? (c === 0 ? row?.field : null) : G.columns[G.cursor.col + c];
      if (!field?.editable) {
        skipped += 1;
        continue;
      }
      edits.push({ table: G.table, row: row.row, path: field.path, new: parseTyped(lines[r][c], isPercent(field, row)) });
    }
  }
  const st = edits.length ? await sendEdits(edits) : null;
  const rejected = st?.rejected?.length ?? 0;
  const parts = [tf("status.pasted", { n: edits.length - rejected })];
  if (skipped + rejected) parts.push(tf("status.skipped", { n: skipped + rejected }));
  setStatus(parts.join(", "));
}

async function history(cmd) {
  commitEdit();
  try {
    updateStatus(await invoke(cmd));
    await refreshScreen();
  } catch (e) {
    showError(e);
  }
}

// ------------------------------------------------------------------------------------------------- value fields
//
// The item card and the table under the curves use plain inputs. Every commit reads the screen again; the input that had
// the focus is found again by its key.

const refKey = (r) => `${r.table}|${r.row}|${r.path}`;

function refPercent(ref) {
  return ref.value_like && (ref.unit === "доля" || ref.row_unit === "доля");
}

function issueText(code) {
  return window.I18N[S.lang][`issue.${code}`] ?? code;
}

/** A table of our own (the loot template) has nothing in the game to compare with */
function hasOriginal(table) {
  return S.kit.tables.find((x) => x.name === table)?.has_orig ?? false;
}

function refTitle(ref) {
  const c = ref.cell;
  const lines = [ref.label];
  if (c?.k !== undefined) lines.push(`${t("tip.kit")}: ${c.k === "" ? t("tip.orig_default") : c.k}`);
  if (c?.n) lines.push(t("tip.not_in_orig"));
  else if (c?.o !== undefined) lines.push(`${t("tip.orig")}: ${c.o === "" ? t("tip.orig_default") : c.o}`);
  if (ref.range) lines.push(`${t(hasOriginal(ref.table) ? "tip.range" : "tip.range_kit")}: ${ref.range}`);
  if (c?.e || c?.w) lines.push(issueText(c.e ?? c.w));
  return lines.join("\n");
}

/** Input for one value. Empty returns the kit's value, as Delete does in the table. */
function numInput(ref) {
  const input = h("input", "num");
  const c = ref.cell;
  const percent = refPercent(ref);
  const stored = c?.v ?? "";
  const start = percent && stored !== "" && !Number.isNaN(Number(stored)) ? numberText(Number(stored) * 100) : stored;
  input.value = start;
  input.dataset.key = refKey(ref);
  input.title = refTitle(ref);
  input.disabled = !ref.editable;
  input.autocomplete = "off";
  input.spellcheck = false;
  if (c?.n) input.dataset.diff = "new";
  else if (c && c.o !== undefined) input.dataset.diff = "changed";
  if (c && c.k !== undefined) input.dataset.mine = "true";
  if (c?.e) input.dataset.issue = "error";
  else if (c?.w) input.dataset.issue = "warning";
  input.addEventListener("focus", () => input.select());
  input.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      input.value = start;
      input.blur();
    } else if (e.key === "Enter") {
      const all = [...input.closest("section").querySelectorAll("input.num:not(:disabled)")];
      const next = all[all.indexOf(input) + (e.shiftKey ? -1 : 1)];
      if (next) next.focus();
      else input.blur();
    }
  });
  input.addEventListener("change", () => {
    if (input.value.trim() === start.trim()) return;
    sendEdits([{ table: ref.table, row: ref.row, path: ref.path, new: parseTyped(input.value, percent) }]);
  });
  if (!percent) return input;
  const box = h("span", "inline-flex items-center gap-1");
  box.append(input, h("span", "text-xs text-fg-3", "%"));
  return box;
}

/** Rebuilds a part of the screen, keeps the scroll and the focus */
function keepingFocus(scroller, build) {
  const key = document.activeElement?.dataset?.key;
  const top = scroller.scrollTop;
  build();
  scroller.scrollTop = top;
  if (key) scroller.querySelector(`input[data-key="${CSS.escape(key)}"]`)?.focus();
}

function rollText(roll, percent) {
  if (roll?.uneven) return t("issue.unit_not_multiple");
  if (!roll || roll.count === null || roll.count === undefined) return t("item.roll_unknown");
  const show = (v) => (percent ? `${numberText(Number(v) * 100)} %` : v);
  if (!roll.more) return roll.values.map(show).join(", ");
  const half = roll.values.length / 2;
  return `${roll.values.slice(0, half).map(show).join(", ")} … ${roll.values.slice(half).map(show).join(", ")} (${count(roll.count, "count.values")})`;
}

// ------------------------------------------------------------------------------------------------- items

function fillSelect(el, anyLabel, options, value) {
  const make = (id, label) => {
    const o = document.createElement("option");
    o.value = id;
    o.textContent = label;
    return o;
  };
  el.replaceChildren(make("", anyLabel), ...options.map((o) => make(o.id, o.label)));
  el.value = value;
}

async function openItems() {
  $("it-search").value = IT.search;
  await loadItemList();
  await loadItemCard();
}

async function loadItemList() {
  try {
    const data = await invoke("list_items", { query: { search: IT.search, kind: IT.kind, rarity: IT.rarityFilter, only_mine: IT.mine, lang: S.lang } });
    fillSelect($("it-kind"), t("grid.kind_any"), data.kinds.map((k) => ({ id: k, label: window.I18N[S.lang][`kind.${k}`] ?? k })), IT.kind);
    fillSelect($("it-rarity"), t("grid.rarity_any"), data.rarities, IT.rarityFilter);
    $("it-mine").dataset.active = String(IT.mine);
    $("it-count").textContent = count(data.items.length, "count.rows");
    if (!data.items.some((x) => x.id === IT.id)) {
      IT.id = data.items[0]?.id ?? null;
      IT.rarity = "";
    }
    const list = $("it-list");
    const top = list.scrollTop;
    list.replaceChildren(...data.items.map(itemRow));
    list.scrollTop = top;
  } catch (e) {
    showError(e);
  }
}

function itemRow(item) {
  const b = h("button", "nav-item h-11!");
  b.dataset.active = String(item.id === IT.id);
  b.dataset.item = item.id;
  if (item.icon) {
    const img = h("img", "size-8 shrink-0 rounded-[3px] object-contain");
    img.alt = "";
    img.loading = "lazy";
    img.src = convertFileSrc(`${S.kit.version}/${item.icon}`, "kit");
    b.append(img);
  } else {
    b.append(h("span", "size-8 shrink-0"));
  }
  b.append(h("span", "min-w-0 flex-1 truncate", item.title || item.id));
  if (item.mine) b.append(h("span", "size-2 shrink-0 rounded-full bg-accent"));
  b.addEventListener("click", () => {
    IT.id = item.id;
    IT.rarity = "";
    for (const el of $("it-list").children) el.dataset.active = String(el.dataset.item === IT.id);
    $("it-card").scrollTop = 0;
    loadItemCard();
  });
  return b;
}

async function loadItemCard() {
  if (!IT.id) {
    $("it-card").replaceChildren(h("div", "py-10 text-center text-fg-3", t("item.none")));
    return;
  }
  try {
    const id = IT.id;
    const data = await invoke("get_item", { id, rarity: IT.rarity });
    if (id !== IT.id) return;
    IT.data = data;
    keepingFocus($("it-card"), () => renderItemCard(data));
  } catch (e) {
    showError(e);
  }
}

async function refreshItems() {
  // The list first: it may pick another item when the filter drops the current one
  await loadItemList();
  await loadItemCard();
}

function cardSection(title, ...children) {
  const box = h("div", "flex flex-col gap-2");
  box.append(h("h2", "font-semibold", title), ...children);
  return box;
}

function renderItemCard(it) {
  const head = h("div", "flex items-center gap-4");
  if (it.icon) {
    const img = h("img", "size-20 shrink-0 rounded-ovl bg-card object-contain p-1");
    img.alt = "";
    img.src = convertFileSrc(`${S.kit.version}/${it.icon}`, "kit");
    head.append(img);
  }
  const names = h("div", "min-w-0");
  names.append(h("h1", "font-display text-2xl leading-8 font-semibold", it.title || it.id));
  const tags = h("div", "mt-1 flex flex-wrap gap-1.5");
  tags.append(pill(it.id));
  if (it.kind) tags.append(pill(window.I18N[S.lang][`kind.${it.kind}`] ?? it.kind));
  if (it.rarity) tags.append(pill(it.rarity_label));
  names.append(tags);
  head.append(names);

  const parts = [head];
  if (it.base.length) parts.push(cardSection(t("item.base"), baseTable(it)));
  for (const [ref, label] of [[it.durability, "item.durability"], [it.sell, "item.sell"]]) {
    if (!ref) continue;
    const row = h("div", "card flex items-center gap-3 px-4 py-2");
    row.append(h("span", "min-w-0 flex-1 font-semibold", t(label)), numInput(ref));
    parts.push(row);
  }
  if (it.market?.length) parts.push(cardSection(t("item.market"), marketTable(it)));
  if (it.trade?.length) parts.push(cardSection(t("item.trade"), fieldRows(it.trade.map((g) => [`${t("item.trade_market")} ${g.market}`, g.fields]))));
  if (it.dismantle?.length) parts.push(cardSection(t("item.dismantle"), fieldRows([["", it.dismantle]])));
  if (it.craft?.length) parts.push(cardSection(t("item.craft"), fieldRows(it.craft.map((g) => [`${t("item.recipe")} ${g.recipe}`, g.fields]))));
  if (it.affix) parts.push(affixSection(it));
  const box = h("div", "mx-auto flex max-w-5xl flex-col gap-5");
  box.append(...parts);
  $("it-card").replaceChildren(box);
}

function baseTable(it) {
  const box = h("div", "card flex flex-col divide-y divide-white/[0.05]");
  if (!it.base.length) {
    box.append(h("div", "px-4 py-3 text-fg-3", t("item.base_empty")));
    return box;
  }
  let hasRoll = false;
  for (const p of it.base) {
    const row = h("div", "flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-1.5");
    const label = h("span", "min-w-40 flex-1 truncate", p.label);
    label.title = p.label;
    row.append(label);
    if (p.fixed) row.append(numInput(p.fixed));
    if (p.min) {
      hasRoll = true;
      const percent = refPercent(p.min);
      const roll = h("span", "min-w-0 max-w-64 truncate text-right text-xs text-fg-3", rollText(p.roll, percent));
      roll.title = roll.textContent;
      row.append(roll);
      row.append(h("span", "text-xs text-fg-3", t("item.min")), numInput(p.min), h("span", "text-xs text-fg-3", t("item.max")), numInput(p.max));
    }
    box.append(row);
    const issue = [p.fixed, p.min, p.max].map((r) => r?.cell?.e ?? r?.cell?.w).find(Boolean);
    if (issue) box.append(h("div", "px-4 py-1 text-right text-xs text-caution", issueText(issue)));
  }
  if (hasRoll) box.append(h("div", "px-4 py-2 text-xs text-fg-3", t("item.roll_note")));
  return box;
}

/** Rows of a card: a caption and the values of one record. A value is named by the item it counts, if any. */
function fieldRows(rows) {
  const box = h("div", "card flex flex-col divide-y divide-white/[0.05]");
  for (const [caption, fields] of rows) {
    const row = h("div", "flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-1.5");
    row.append(h("span", "min-w-20 flex-1 font-mono text-xs text-fg-3", caption));
    for (const f of fields) {
      const label = f.label.replace(/ \([\d,]+\)$/, "");
      row.append(h("span", "text-xs text-fg-3", f.item ? `${label}: ${f.item}` : label), numInput(f));
    }
    box.append(row);
  }
  return box;
}

function marketTable(it) {
  const box = h("div", "card flex flex-col divide-y divide-white/[0.05]");
  for (const g of it.market) {
    const row = h("div", "flex flex-wrap items-center gap-3 px-4 py-1.5");
    row.append(h("span", "min-w-0 flex-1 font-mono text-xs text-fg-3", `${t("item.product")} ${g.product}`));
    for (const f of g.fields) {
      // The numbers in the label only tell list positions apart, a product has one of each here
      const label = f.label.replace(/ \([\d,]+\)$/, "");
      row.append(h("span", "text-xs text-fg-3", f.currency ? `${label}, ${f.currency}` : label), numInput(f));
    }
    box.append(row);
  }
  return box;
}

function affixSection(it) {
  const a = it.affix;
  const box = h("div", "flex flex-col gap-2");
  box.append(h("h2", "font-semibold", t("item.affix")));
  const warn = h("div", "bar");
  warn.dataset.tone = "caution";
  warn.append(h("span", "ico text-caution", "\uE7BA"), h("span", "", tf("item.affix_group", { group: a.group, n: a.shared })));
  box.append(warn);

  const tools = h("div", "flex flex-wrap items-center gap-2");
  const rarity = h("select", "combo w-48!");
  fillSelectPlain(rarity, a.rarities, a.rarity);
  rarity.addEventListener("change", () => {
    IT.rarity = rarity.value;
    loadItemCard();
  });
  const search = h("input", "field w-52!");
  search.type = "search";
  search.placeholder = t("item.affix_search");
  search.value = IT.affixSearch;
  search.dataset.key = "affix-search";
  const sides = h("div", "flex rounded-ctl border border-white/[0.07]");
  for (const [side, label] of [["major", "item.affix_major"], ["minor", "item.affix_minor"]]) {
    const b = h("button", "seg", t(label));
    b.dataset.side = side;
    b.dataset.active = String(IT.affixSide === side);
    b.addEventListener("click", () => {
      IT.affixSide = side;
      keepingFocus($("it-card"), () => renderItemCard(it));
    });
    sides.append(b);
  }
  tools.append(sides, rarity, search);
  box.append(tools);

  const table = h("div", "card");
  const head = h("div", `${AFFIX_GRID} border-b border-white/[0.08] py-2 text-xs text-fg-3`);
  head.append(h("span"), h("span", "text-right", t("item.min")), h("span", "text-right", t("item.max")), h("span", "", t("item.values")));
  table.append(head);
  const rows = h("div");
  const paint = () => {
    const words = IT.affixSearch.toLowerCase().split(/\s+/).filter(Boolean);
    rows.replaceChildren(...a.rows.filter((r) => words.every((w) => r.title.toLowerCase().includes(w))).map(affixRow));
  };
  search.addEventListener("input", () => {
    IT.affixSearch = search.value;
    paint();
  });
  paint();
  table.append(rows);
  box.append(table);
  return box;
}

function fillSelectPlain(el, options, value) {
  for (const o of options) {
    const opt = document.createElement("option");
    opt.value = o.id;
    opt.textContent = o.label;
    el.append(opt);
  }
  el.value = value;
}

const AFFIX_GRID = "grid grid-cols-[minmax(140px,1fr)_104px_104px_minmax(120px,1fr)] items-center gap-2 px-4";

function affixRow(r) {
  const row = h("div", `${AFFIX_GRID} border-b border-white/[0.04] py-1 last:border-b-0`);
  row.append(h("span", "truncate", r.title));
  const side = IT.affixSide === "minor" ? r.minor : r.major;
  if (!side) {
    row.append(h("span"), h("span"), h("span"));
    return row;
  }
  const right = (el) => {
    const box = h("span", "text-right");
    box.append(el);
    return box;
  };
  row.append(right(numInput(side.min)), right(numInput(side.max)));
  const roll = h("span", "truncate text-xs text-fg-3", rollText(side.roll, refPercent(side.min)));
  roll.title = roll.textContent;
  row.append(roll);
  return row;
}

// ------------------------------------------------------------------------------------------------- class curves
//
// One axis, one attribute. A class keeps its colour whatever else is shown. Lines 2 px, points 8 px with a ring in the
// surface colour, hit areas 24 px; a crosshair reads every shown class at a level; the table below repeats every value.

// ------------------------------------------------------------------------------------------------- loot

/** map: null until the first answer, then a map id as text; "" — all maps */
const LT = { key: null, map: null, kind: "", search: "", mine: false, kinds: [], data: null, toggled: new Set(), seq: 0 };
/** A group this big starts with its slots closed: a thousand inputs at once would make every edit slow */
const LOOT_OPEN_ITEMS = 150;

function pct(x) {
  if (x === null || x === undefined) return "—";
  const p = x * 100;
  if (p > 0 && p < 0.01) return "<0.01 %";
  return `${Number(p.toFixed(2))} %`;
}

function dec(x) {
  return String(Number((x ?? 0).toFixed(2)));
}

const lootSame = (a, b) => Math.abs((a ?? 0) - (b ?? 0)) < 1e-6;

async function openLoot() {
  $("lt-search").value = LT.search;
  await refreshLoot();
}

async function refreshLoot() {
  await loadLootList();
  await loadLootCard();
}

async function queryLoot() {
  return invoke("loot_groups", { query: { map: LT.map === null || LT.map === "" ? null : Number(LT.map), kind: LT.kind, search: LT.search, only_mine: LT.mine, lang: S.lang } });
}

async function loadLootList() {
  const seq = ++LT.seq;
  try {
    let data = await queryLoot();
    // The first time: a real map, since one source has many rows, one per map
    if (LT.map === null) {
      LT.map = String(data.maps.find((m) => m.id !== 0)?.id ?? "");
      data = await queryLoot();
    }
    if (LT.kind && !data.kinds.some((k) => k.id === LT.kind)) {
      LT.kind = "";
      data = await queryLoot();
    }
    if (seq !== LT.seq) return;
    LT.kinds = data.kinds;
    fillSelect($("lt-map"), t("loot.all_maps"), data.maps.map((m) => ({ id: String(m.id), label: m.label })), LT.map);
    fillSelect($("lt-kind"), t("loot.all_kinds"), data.kinds.map((k) => ({ id: k.id, label: `${k.label} · ${k.count}` })), LT.kind);
    $("lt-mine").dataset.active = String(LT.mine);
    $("lt-count").textContent = count(data.groups.length, "count.groups");
    if (!data.groups.some((g) => g.key === LT.key)) {
      LT.key = data.groups[0]?.key ?? null;
      LT.toggled.clear();
    }
    const list = $("lt-list");
    const top = list.scrollTop;
    list.replaceChildren(...data.groups.map(lootRow));
    list.scrollTop = top;
  } catch (e) {
    showError(e);
  }
}

function lootRow(g) {
  const b = h("button", "nav-item h-auto! min-h-11 shrink-0 py-1.5!");
  b.dataset.active = String(g.key === LT.key);
  b.dataset.group = g.key;
  const text = h("span", "flex min-w-0 flex-1 flex-col text-left");
  const title = h("span", "truncate", g.title);
  title.title = g.title;
  text.append(title);
  const line = [`${dec(g.items)} ${t("loot.items_short")}`];
  if (g.gold > 0) line.push(`${t("loot.gold_short")} ≈ ${dec(g.gold)}`);
  const sub = h("span", "truncate text-xs text-fg-3 tabular-nums", line.join(" · "));
  text.append(sub);
  b.append(text);
  if (!lootSame(g.items, g.items_was) || !lootSame(g.gold, g.gold_was)) {
    const up = g.items > g.items_was || (lootSame(g.items, g.items_was) && g.gold > g.gold_was);
    const mark = h("span", "ico shrink-0 text-xs text-fg-3", up ? "\uE74A" : "\uE74B");
    mark.title = `${t("loot.was")}: ${dec(g.items_was)} ${t("loot.items_short")}`;
    b.append(mark);
  }
  if (g.mine) b.append(h("span", "size-2 shrink-0 rounded-full bg-accent"));
  b.addEventListener("click", () => {
    LT.key = g.key;
    LT.toggled.clear();
    for (const el of $("lt-list").children) el.dataset.active = String(el.dataset.group === LT.key);
    $("lt-card").scrollTop = 0;
    loadLootCard();
  });
  return b;
}

async function loadLootCard() {
  if (!LT.key) {
    $("lt-card").replaceChildren(h("div", "py-10 text-center text-fg-3", t("loot.none")));
    return;
  }
  try {
    const key = LT.key;
    const data = await invoke("loot_group", { key });
    if (key !== LT.key) return;
    LT.data = data;
    keepingFocus($("lt-card"), () => renderLootCard(data));
  } catch (e) {
    showError(e);
  }
}

function lootStat(label, now, was, text) {
  const box = h("div", "card min-w-36 flex-1 px-4 py-3");
  box.append(h("div", "text-xs text-fg-3", label), h("div", "font-display text-2xl leading-8 font-semibold tabular-nums", text(now)));
  box.append(h("div", "h-4 text-xs text-fg-3 tabular-nums", lootSame(now, was) ? "" : `${t("loot.was")}: ${text(was)}`));
  return box;
}

function lootCats(g) {
  const was = new Map(g.summary_was.cats.map((c) => [c.id, c.value]));
  const rows = g.summary.cats.map((c) => ({ ...c, was: was.get(c.id) ?? 0 }));
  for (const c of g.summary_was.cats) if (!rows.some((r) => r.id === c.id)) rows.push({ ...c, was: c.value, value: 0 });
  if (!rows.length) return null;
  const box = h("div", "card flex flex-col divide-y divide-white/[0.05]");
  for (const c of rows) {
    const row = h("div", "flex items-center gap-3 px-4 py-1.5");
    row.append(h("span", "min-w-0 flex-1 truncate", c.label), h("span", "text-xs text-fg-3 tabular-nums", lootSame(c.value, c.was) ? "" : `${t("loot.was")} ${dec(c.was)}`), h("span", "w-14 text-right tabular-nums", dec(c.value)));
    box.append(row);
  }
  return box;
}

function lootRuleLine(r, mode) {
  const parts = [];
  if (r.remove.length) parts.push(`${t("loot.rule_remove")}: ${r.remove.join(", ")}`);
  for (const [list, key] of [[r.chance, "loot.rule_chance"], [r.weight, "loot.rule_weight"], [r.count, "loot.rule_count"]]) {
    if (list.length) parts.push(`${t(key)}: ${list.map((x) => `${x.label} ×${x.value}`).join(", ")}`);
  }
  if (r.quality !== null && r.quality !== undefined) parts.push(`${t("loot.rule_quality")}: ${r.quality}`);
  const scope = r.scope === "rules" ? t("loot.rule_all") : (mode?.title || r.scope);
  return `${scope}: ${parts.join("; ")}`;
}

function lootIssue(ref) {
  const code = ref?.cell?.e ?? ref?.cell?.w;
  return code ? h("div", "px-4 py-1 text-xs text-caution", issueText(code)) : null;
}

function lootItemRow(item) {
  const row = h("div", "lt-row px-4 py-1");
  const name = h("div", "flex min-w-0 items-center gap-2");
  if (item.icon) {
    const img = h("img", "size-7 shrink-0 rounded-[3px] object-contain");
    img.alt = "";
    img.loading = "lazy";
    img.src = convertFileSrc(`${S.kit.version}/${item.icon}`, "kit");
    name.append(img);
  } else {
    name.append(h("span", "size-7 shrink-0"));
  }
  const label = h("span", "min-w-0 flex-1 truncate", item.title || String(item.id));
  label.title = [item.title, item.rarity_label, item.cat_label, String(item.id)].filter(Boolean).join(" · ");
  name.append(label);
  const w = numInput(item.w);
  w.classList.add("w-20!");
  const cnt = h("div", "flex flex-wrap gap-1");
  for (const c of item.cnt) {
    const input = numInput(c);
    input.classList.add("w-12!");
    cnt.append(input);
  }
  const share = h("div", "text-right text-fg-2 tabular-nums", item.share === null ? "—" : pct(item.share));
  share.title = `${t("loot.in_template")}: ${pct(item.template_share)}`;
  const chance = h("div", "text-right tabular-nums");
  if (item.chance === null) {
    chance.textContent = t("loot.removed");
    chance.classList.add("text-xs", "text-fg-3");
  } else {
    chance.textContent = pct(item.chance);
    if (item.avg > 1) chance.title = `${t("loot.avg_count")}: ${dec(item.avg)}`;
  }
  if (!lootSame(item.chance, item.chance_was)) {
    chance.append(h("div", "text-xs text-fg-3", `${t("loot.was")} ${pct(item.chance_was ?? 0)}`));
  }
  row.append(name, w, cnt, share, chance);
  const box = document.createDocumentFragment();
  box.append(row);
  for (const ref of [item.w, ...item.cnt]) {
    const issue = lootIssue(ref);
    if (issue) box.append(issue);
  }
  return box;
}

function lootPool(pool) {
  const box = h("div", "flex flex-col");
  const head = h("div", "flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-white/[0.05] bg-white/[0.02] px-4 py-1.5");
  head.append(h("span", "min-w-0 flex-1 truncate text-fg-2", pool.label || pool.key), h("span", "font-mono text-xs text-fg-4", pool.key));
  if (pool.pr) {
    head.append(h("span", "text-xs text-fg-3", t("loot.pool_share")), numInput(pool.pr), h("span", "text-xs text-fg-3 tabular-nums", pool.share === null ? "" : pct(pool.share)));
  }
  box.append(head);
  const prIssue = lootIssue(pool.pr);
  if (prIssue) box.append(prIssue);
  if (pool.shared > 0) {
    const warn = h("div", "px-4 pt-1 text-xs text-caution", tf("loot.shared", { n: pool.shared }));
    warn.title = pool.shared_titles.join("\n");
    box.append(warn);
  }
  const heads = h("div", "lt-row px-4 pt-1 text-xs text-fg-3");
  heads.append(h("span", "", t("loot.item")), h("span", "", t("loot.weight")), h("span", "", t("loot.count")), h("span", "text-right", t("loot.share")), h("span", "text-right", t("loot.chance")));
  box.append(heads);
  for (const item of pool.items) box.append(lootItemRow(item));
  return box;
}

function lootSlot(slot, group, opened) {
  const id = `${group.key}|${slot.n}`;
  const isOpen = LT.toggled.has(id) ? !opened : opened;
  const box = h("div", "card flex flex-col overflow-hidden");
  const head = h("div", "flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2");
  const toggle = h("button", "ico flex size-6 shrink-0 items-center justify-center rounded text-xs text-fg-3 hover:bg-white/[0.06]", isOpen ? "\uE70D" : "\uE76C");
  toggle.title = t(isOpen ? "loot.collapse" : "loot.expand");
  toggle.dataset.slot = String(slot.n);
  toggle.addEventListener("click", () => {
    if (LT.toggled.has(id)) LT.toggled.delete(id);
    else LT.toggled.add(id);
    keepingFocus($("lt-card"), () => renderLootCard(group));
  });
  const names = slot.pools.map((p) => p.label || p.key).join(", ");
  const caption = h("div", "flex min-w-40 flex-1 flex-col");
  const nameEl = h("span", "truncate font-semibold", `${t("loot.slot")} ${slot.n + 1}`);
  const poolEl = h("span", "truncate text-xs text-fg-3", names);
  poolEl.title = names;
  caption.append(nameEl, poolEl);
  head.append(toggle, caption, h("span", "text-xs text-fg-3", t("loot.slot_chance")), numInput(slot.chance), h("span", "text-xs text-fg-3", "%"));
  const typed = Number(slot.chance?.cell?.v);
  const note = h("div", "w-28 text-right text-xs text-fg-3 tabular-nums");
  if (slot.rate === null) note.textContent = t("loot.no_drop");
  else if (!lootSame(slot.rate * 100, typed)) note.textContent = `${t("loot.with_rules")} ${pct(slot.rate)}`;
  if (!lootSame(slot.rate, slot.rate_was)) note.append(h("div", "", `${t("loot.was")} ${slot.rate_was === null ? t("loot.no_drop") : pct(slot.rate_was)}`));
  head.append(note);
  box.append(head);
  const issue = lootIssue(slot.chance);
  if (issue) box.append(issue);
  if (isOpen) for (const pool of slot.pools) box.append(lootPool(pool));
  return box;
}

function renderLootCard(g) {
  const parts = [];
  const head = h("div");
  head.append(h("h1", "font-display text-2xl leading-8 font-semibold select-text", g.title));
  const tags = h("div", "mt-1 flex flex-wrap gap-1.5");
  const kind = LT.kinds.find((k) => k.id === g.kind);
  if (kind) tags.append(pill(kind.label));
  for (const m of g.maps) tags.append(pill(m));
  if (g.mode) tags.append(pill(`${t("loot.mode")}: ${g.mode.title || g.mode.id}`, "caution"));
  if (g.types.length && g.types.length < 6) tags.append(pill(`${t("loot.types")} ${g.types.join(", ")}`));
  head.append(tags);
  if (g.sources.length) {
    const more = h("details", "mt-2 text-xs text-fg-3");
    more.append(h("summary", "cursor-default", tf("loot.sources", { n: g.sources.length })));
    more.append(h("div", "mt-1 select-text", g.sources.join(", ")));
    head.append(more);
  }
  parts.push(head);

  const stats = h("div", "flex flex-wrap gap-3");
  stats.append(lootStat(t("loot.stat_items"), g.summary.items, g.summary_was.items, dec), lootStat(t("loot.stat_gold"), g.summary.gold, g.summary_was.gold, dec));
  parts.push(cardSection(t("loot.average"), stats, ...[lootCats(g)].filter(Boolean)));

  if (g.rules.length) {
    const bar = h("div", "bar items-start!");
    bar.dataset.tone = "caution";
    const text = h("div", "flex min-w-0 flex-1 flex-col gap-1");
    text.append(h("div", "font-semibold", t("loot.rules_title")));
    for (const r of g.rules) text.append(h("div", "select-text", lootRuleLine(r, g.mode)));
    text.append(h("div", "text-xs text-fg-3", t("loot.rules_note")));
    const open = h("button", "btn-std shrink-0", t("loot.rules_open"));
    open.addEventListener("click", () => showView("lt_rules"));
    bar.append(h("span", "ico text-caution", "\uE946"), text, open);
    parts.push(bar);
  }

  const total = g.slots.reduce((n, s) => n + s.pools.reduce((m, p) => m + p.items.length, 0), 0);
  const opened = total <= LOOT_OPEN_ITEMS;
  const slots = h("div", "flex flex-col gap-2");
  for (const s of g.slots) slots.append(lootSlot(s, g, opened));
  parts.push(cardSection(tf("loot.slots", { n: g.slots.length }), h("div", "text-xs text-fg-3", t("loot.slots_note")), slots));
  parts.push(h("div", "text-xs text-fg-3", t("loot.pak_note")));

  const box = h("div", "mx-auto flex max-w-5xl flex-col gap-5");
  box.append(...parts);
  $("lt-card").replaceChildren(box);
}

const SVG = "http://www.w3.org/2000/svg";

function s(tag, attrs = {}) {
  const el = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  return el;
}

async function openCurves() {
  await loadCurves();
}

async function loadCurves() {
  try {
    const data = await invoke("class_curves", { path: CV.path });
    CV.data = data;
    CV.path = data.path ?? "";
    data.classes.forEach((c, i) => {
      c.slot = i % 8 + 1;
    });
    CV.shown ??= new Set(data.classes.slice(0, 3).map((c) => c.id));
    const attr = $("cv-attr");
    const options = data.attrs.map((a) => {
      const o = document.createElement("option");
      o.value = a.path;
      o.textContent = a.label;
      return o;
    });
    attr.replaceChildren(...options);
    attr.value = CV.path;
    renderLegend();
    renderCurves();
    keepingFocus($("cv-table"), renderCurveTable);
  } catch (e) {
    showError(e);
  }
}

function renderLegend() {
  const chips = CV.data.classes.map((c) => {
    const b = h("button", "chip");
    b.dataset.active = String(CV.shown.has(c.id));
    const key = h("span", "inline-block h-0.5 w-4 rounded-full");
    key.style.background = `var(--series-${c.slot})`;
    b.append(key, h("span", "", c.title));
    b.addEventListener("click", () => {
      if (CV.shown.has(c.id)) CV.shown.delete(c.id);
      else CV.shown.add(c.id);
      renderLegend();
      renderCurves();
      renderCurveTable();
    });
    return b;
  });
  $("cv-legend").replaceChildren(...chips);
}

/** Value of the point now and in the game. In the game = the original, if the kit differs from it; otherwise the kit. */
function pointValues(level, id) {
  const cell = level.ref.cell;
  const drag = CV.drag;
  const dragged = drag && drag.id === id && drag.row === level.ref.row;
  const now = dragged ? drag.value : Number(cell?.v);
  const inKit = cell?.k !== undefined ? cell.k : cell?.v;
  const game = cell?.n ? NaN : Number(cell?.o !== undefined ? cell.o : inKit);
  return { now, game };
}

function niceTicks(lo, hi, n = 5) {
  if (lo === hi) {
    lo -= 1;
    hi += 1;
  }
  const raw = (hi - lo) / n;
  const mag = 10 ** Math.floor(Math.log10(raw));
  const norm = raw / mag;
  const step = (norm <= 1 ? 1 : norm <= 2 ? 2 : norm <= 5 ? 5 : 10) * mag;
  const from = Math.floor(lo / step) * step;
  const to = Math.ceil(hi / step) * step;
  const ticks = [];
  for (let v = from; v <= to + step / 2; v += step) ticks.push(Number(v.toPrecision(10)));
  return { ticks, from, to };
}

function renderCurves() {
  const box = $("cv-chart");
  const shown = CV.data.classes.filter((c) => CV.shown.has(c.id));
  const field = CV.data.attrs.find((a) => a.path === CV.path);
  const percent = field?.value_like && field?.unit === "доля";
  const fmt = (v) => (percent ? `${numberText(v * 100)} %` : numberText(v));
  const W = Math.max(480, box.clientWidth - 16);
  const H = 320;
  const M = { l: 56, r: shown.length <= 4 ? 110 : 16, t: 12, b: 30 };
  const levels = [...new Set(CV.data.classes.flatMap((c) => c.levels.map((l) => l.level)))].sort((a, b) => a - b);
  const values = shown.flatMap((c) => c.levels.flatMap((l) => {
    const { now, game } = pointValues(l, c.id);
    return [now, game].filter(Number.isFinite);
  }));
  // While a point is dragged the scale stays put, or the point would run away from the mouse
  const scale = CV.drag && CV.scale ? CV.scale : niceTicks(Math.min(...values, Infinity) === Infinity ? 0 : Math.min(...values), values.length ? Math.max(...values) : 1);
  CV.scale = scale;
  const x = (lv) => M.l + ((lv - levels[0]) / Math.max(1, levels[levels.length - 1] - levels[0])) * (W - M.l - M.r);
  const y = (v) => H - M.b - ((v - scale.from) / (scale.to - scale.from)) * (H - M.t - M.b);
  CV.toValue = (py) => scale.from + ((H - M.b - py) / (H - M.t - M.b)) * (scale.to - scale.from);

  const svg = s("svg", { width: W, height: H, viewBox: `0 0 ${W} ${H}`, role: "img", "aria-label": field?.label ?? "" });
  for (const v of scale.ticks) {
    const line = s("line", { x1: M.l, x2: W - M.r, y1: y(v), y2: y(v), "stroke-width": 1 });
    line.style.stroke = "rgb(255 255 255 / 0.07)";
    const label = s("text", { x: M.l - 8, y: y(v) + 4, "text-anchor": "end", "font-size": 11 });
    label.style.fill = "var(--color-fg-3)";
    label.textContent = fmt(v);
    svg.append(line, label);
  }
  for (const lv of levels) {
    const label = s("text", { x: x(lv), y: H - 10, "text-anchor": "middle", "font-size": 11 });
    label.style.fill = "var(--color-fg-3)";
    label.textContent = String(lv);
    svg.append(label);
  }
  const xTitle = s("text", { x: W - M.r, y: H - 10, "text-anchor": "end", "font-size": 11, dx: M.r > 16 ? 60 : 0 });
  xTitle.style.fill = "var(--color-fg-4)";
  xTitle.textContent = t("curves.level");
  svg.append(xTitle);

  const cross = s("line", { y1: M.t, y2: H - M.b, "stroke-width": 1, visibility: "hidden" });
  cross.style.stroke = "rgb(255 255 255 / 0.3)";
  svg.append(cross);

  const line = (points, c, dashed) => {
    const p = s("polyline", { fill: "none", "stroke-width": 2, "stroke-linejoin": "round", "stroke-linecap": "round", points: points.map((q) => q.join(",")).join(" ") });
    p.style.stroke = `var(--series-${c.slot})`;
    if (dashed) {
      p.setAttribute("stroke-dasharray", "2 5");
      p.style.opacity = "0.75";
    }
    svg.append(p);
  };
  for (const c of shown) {
    const pts = c.levels.map((l) => ({ l, ...pointValues(l, c.id) }));
    const differs = pts.some((q) => Number.isFinite(q.game) && Math.abs(q.game - q.now) > 1e-9);
    if (differs) line(pts.filter((q) => Number.isFinite(q.game)).map((q) => [x(q.l.level), y(q.game)]), c, true);
    line(pts.filter((q) => Number.isFinite(q.now)).map((q) => [x(q.l.level), y(q.now)]), c, false);
  }
  const labelled = [];
  for (const c of shown) {
    for (const l of c.levels) {
      const { now } = pointValues(l, c.id);
      if (!Number.isFinite(now)) continue;
      const dot = s("circle", { cx: x(l.level), cy: y(now), r: 4, class: "chart-point" });
      dot.style.fill = `var(--series-${c.slot})`;
      const hit = s("circle", { cx: x(l.level), cy: y(now), r: 12, class: "chart-hit" });
      hit.dataset.id = c.id;
      hit.dataset.row = String(l.ref.row);
      svg.append(dot, hit);
    }
    // Direct label at the end of the line, in text colour; the legend names them too
    const last = c.levels[c.levels.length - 1];
    const endY = last ? y(pointValues(last, c.id).now) : 0;
    // Labels that would collide are left to the legend and the crosshair
    if (M.r > 16 && last && labelled.every((v) => Math.abs(v - endY) >= 13)) {
      labelled.push(endY);
      const label = s("text", { x: W - M.r + 10, y: endY + 4, "font-size": 11 });
      label.style.fill = "var(--color-fg-2)";
      label.textContent = c.title;
      svg.append(label);
    }
  }

  // The crosshair finds the level, one readout lists every shown class
  svg.addEventListener("pointermove", (e) => {
    if (CV.drag) return dragMove(e, svg);
    const r = svg.getBoundingClientRect();
    const px = e.clientX - r.left;
    if (px < M.l - 8 || px > W - M.r + 8) {
      cross.setAttribute("visibility", "hidden");
      return hideTip();
    }
    const lv = levels.reduce((a, b) => (Math.abs(x(b) - px) < Math.abs(x(a) - px) ? b : a));
    cross.setAttribute("x1", x(lv));
    cross.setAttribute("x2", x(lv));
    cross.setAttribute("visibility", "visible");
    showCurveTip(lv, shown, fmt, e.clientX, e.clientY);
  });
  svg.addEventListener("pointerleave", () => {
    cross.setAttribute("visibility", "hidden");
    hideTip();
  });
  svg.addEventListener("pointerdown", (e) => {
    if (!e.target.classList.contains("chart-hit")) return;
    const c = CV.data.classes.find((k) => k.id === e.target.dataset.id);
    const row = Number(e.target.dataset.row);
    const level = c.levels.find((l) => l.ref.row === row);
    if (!level.ref.editable) return;
    const start = Number(level.ref.cell?.v);
    CV.drag = { id: c.id, row, value: start, start, path: level.ref.path, table: level.ref.table, step: dragStep(percent) };
    svg.setPointerCapture(e.pointerId);
    hideTip();
  });
  svg.addEventListener("pointerup", () => dragEnd());
  svg.addEventListener("pointercancel", () => {
    CV.drag = null;
    renderCurves();
  });
  box.replaceChildren(svg);
}

/** Whole numbers stay whole, a share moves by 0.1 %, other fractions by 0.01 */
function dragStep(percent) {
  if (percent) return 0.001;
  const all = CV.data.classes.flatMap((c) => c.levels.map((l) => l.ref.cell?.v)).filter((v) => v !== undefined && v !== "");
  return all.every((v) => Number.isInteger(Number(v))) ? 1 : 0.01;
}

function dragMove(e, svg) {
  const r = svg.getBoundingClientRect();
  const raw = CV.toValue(e.clientY - r.top);
  const v = Number((Math.round(raw / CV.drag.step) * CV.drag.step).toPrecision(10));
  if (v === CV.drag.value) return;
  CV.drag.value = Math.max(CV.scale.from, Math.min(CV.scale.to, v));
  const pointerId = e.pointerId;
  renderCurves();
  $("cv-chart").querySelector("svg").setPointerCapture?.(pointerId);
}

function dragEnd() {
  const d = CV.drag;
  if (!d) return;
  CV.drag = null;
  if (d.value === d.start) return renderCurves();
  sendEdits([{ table: d.table, row: d.row, path: d.path, new: String(d.value) }]);
}

function showCurveTip(level, shown, fmt, x, y) {
  const tip = $("tip");
  const parts = [h("div", "mb-1 text-fg-3", `${t("curves.level")} ${level}`)];
  for (const c of shown) {
    const l = c.levels.find((q) => q.level === level);
    if (!l) continue;
    const { now, game } = pointValues(l, c.id);
    const line = h("div", "flex items-center gap-2");
    const key = h("span", "inline-block h-0.5 w-3 shrink-0 rounded-full");
    key.style.background = `var(--series-${c.slot})`;
    line.append(h("span", "font-semibold tabular-nums", fmt(now)), key, h("span", "text-fg-2", c.title));
    if (Number.isFinite(game) && Math.abs(game - now) > 1e-9) line.append(h("span", "text-fg-3", `${t("curves.in_game")} ${fmt(game)}`));
    parts.push(line);
  }
  tip.replaceChildren(...parts);
  tip.classList.remove("hidden");
  const r = tip.getBoundingClientRect();
  tip.style.left = `${x + 14 + r.width > window.innerWidth ? x - r.width - 14 : x + 14}px`;
  tip.style.top = `${Math.max(8, Math.min(y - 10, window.innerHeight - r.height - 8))}px`;
}

function renderCurveTable() {
  const shown = CV.data.classes.filter((c) => CV.shown.has(c.id));
  const levels = [...new Set(CV.data.classes.flatMap((c) => c.levels.map((l) => l.level)))].sort((a, b) => a - b);
  const table = h("table", "w-full text-sm");
  const head = h("tr", "border-b border-white/[0.08] text-xs text-fg-3");
  head.append(h("th", "px-4 py-2 text-left font-normal", t("curves.level")));
  for (const c of shown) {
    const th = h("th", "px-2 py-2 text-right font-normal");
    const inner = h("span", "inline-flex items-center gap-1.5");
    const key = h("span", "inline-block h-0.5 w-3 rounded-full");
    key.style.background = `var(--series-${c.slot})`;
    inner.append(key, h("span", "", c.title));
    th.append(inner);
    head.append(th);
  }
  const body = levels.map((lv) => {
    const tr = h("tr", "border-b border-white/[0.04] last:border-b-0");
    tr.append(h("td", "px-4 py-0.5 text-fg-3 tabular-nums", String(lv)));
    for (const c of shown) {
      const td = h("td", "px-2 py-0.5 text-right");
      const l = c.levels.find((q) => q.level === lv);
      if (l) td.append(numInput(l.ref));
      tr.append(td);
    }
    return tr;
  });
  table.append(head, ...body);
  // numInput looks for its neighbours inside a <section>: the whole curves view is one
  $("cv-table").replaceChildren(table);
}

// ------------------------------------------------------------------------------------------------- bulk edit

const BK = { op: "pct", fields: [], checked: new Set(), rows: null, seq: 0, timer: 0, spec: null };
const BK_NOTE = { pct: "bulk.note_pct", mul: "bulk.note_mul", add: "bulk.note_add", set: "bulk.note_set" };

async function openBulk() {
  commitEdit();
  const rows = G.selected.size ? [...G.selected] : null;
  BK.rows = rows;
  const query = queryOf(filtersOf(G.table));
  try {
    const f = await invoke("bulk_fields", { table: G.table, query, rows });
    BK.fields = f.fields;
    BK.targetRows = f.rows;
    BK.checked = new Set();
    const at = cursorData();
    if (at?.field.editable && f.fields.some((x) => x.path === at.field.path)) BK.checked.add(at.field.path);
    $("bk-target").textContent = rows ? tf("bulk.target_selected", { n: f.rows }) : tf("bulk.target_filter", { n: f.rows });
    $("bk-search").value = "";
    $("bk-value").value = "";
    renderBulkFields();
    setBulkOp(BK.op);
    $("bulk").classList.remove("hidden");
    $("bulk").classList.add("grid");
    $("bk-value").focus();
  } catch (e) {
    showError(e);
  }
}

function closeBulk() {
  // A preview still on its way belongs to this dialog, not to whatever opens next
  clearTimeout(BK.timer);
  BK.seq++;
  $("bulk").classList.add("hidden");
  $("bulk").classList.remove("grid");
  $("grid").focus({ preventScroll: true });
}

function bulkVisibleFields() {
  const words = $("bk-search").value.toLowerCase().split(/\s+/).filter(Boolean);
  return BK.fields.filter((f) => words.every((w) => `${f.label} ${f.path}`.toLowerCase().includes(w)));
}

function renderBulkFields() {
  const list = bulkVisibleFields().map((f) => {
    const row = h("label", "flex items-center gap-2 border-b border-white/[0.04] px-3 py-1.5 text-sm last:border-b-0 hover:bg-white/[0.04]");
    const box = h("input", "check mt-0!");
    box.type = "checkbox";
    box.checked = BK.checked.has(f.path);
    box.addEventListener("change", () => {
      if (box.checked) BK.checked.add(f.path);
      else BK.checked.delete(f.path);
      updateBulkAll();
      scheduleBulkPreview();
    });
    row.title = f.path;
    row.append(box, h("span", "min-w-0 flex-1 truncate", f.label), h("span", "text-xs text-fg-3 tabular-nums", String(f.count)));
    return row;
  });
  $("bk-fields").replaceChildren(...list);
  updateBulkAll();
}

function updateBulkAll() {
  const visible = bulkVisibleFields();
  $("bk-all").textContent = t(visible.length && visible.every((f) => BK.checked.has(f.path)) ? "bulk.none" : "bulk.all");
}

function setBulkOp(op) {
  BK.op = op;
  for (const b of document.querySelectorAll("#bulk [data-op]")) b.dataset.active = String(b.dataset.op === op);
  $("bk-opnote").textContent = t(BK_NOTE[op]);
  scheduleBulkPreview();
}

/** "−20" with percent is ×0.8. Minus may be typed as a real minus sign. */
function bulkSpec() {
  const value = Number($("bk-value").value.trim().replace("−", "-").replace(",", ".").replace(/\s*%$/, ""));
  if ($("bk-value").value.trim() === "" || Number.isNaN(value) || BK.checked.size === 0) return null;
  const spec = { table: G.table, query: queryOf(filtersOf(G.table)), paths: [...BK.checked], pairs: $("bk-pairs").checked, op: BK.op, value };
  if (BK.rows) spec.rows = BK.rows;
  if (BK.op === "pct") {
    spec.op = "mul";
    spec.value = Number((1 + value / 100).toPrecision(12));
  }
  return spec;
}

function scheduleBulkPreview() {
  clearTimeout(BK.timer);
  BK.timer = setTimeout(previewBulk, 150);
}

async function previewBulk() {
  const spec = bulkSpec();
  BK.spec = null;
  $("bk-apply").disabled = true;
  if (!spec) {
    $("bk-summary").textContent = t("bulk.pick");
    $("bk-preview").replaceChildren();
    return;
  }
  const seq = ++BK.seq;
  try {
    const p = await invoke("preview_bulk", { spec });
    if (seq !== BK.seq) return;
    const parts = [tf("bulk.summary", { n: p.total, e: p.errors, w: p.warnings })];
    if (p.skipped) parts.push(tf("bulk.skipped", { n: p.skipped }));
    if (p.total > p.changes.length) parts.push(tf("bulk.more", { shown: p.changes.length, n: p.total }));
    if (p.errors) parts.push(t("bulk.errors_block"));
    $("bk-summary").textContent = parts.join(" ");
    $("bk-preview").replaceChildren(...p.changes.map(bulkRow));
    BK.spec = p.total > 0 && p.errors === 0 ? spec : null;
    $("bk-apply").disabled = !BK.spec;
  } catch (e) {
    showError(e);
  }
}

function bulkRow(c) {
  const row = h("div", "flex items-center gap-3 border-b border-white/[0.04] px-3 py-1 last:border-b-0");
  const name = h("div", "min-w-0 flex-1");
  name.append(h("div", "truncate text-sm", c.title || c.key), h("div", "truncate text-xs text-fg-3", c.field.label));
  if (c.issue) {
    const line = h("div", c.issue.severity === "error" ? "text-xs text-critical" : "text-xs text-caution", issueText(c.issue.code));
    name.append(line);
  }
  const u = { unit: c.unit };
  row.append(name, h("span", "shrink-0 text-fg-3 tabular-nums", shown(c.old, c.field, u)), h("span", "ico shrink-0 text-xs text-fg-3", "\uE72A"), h("span", "shrink-0 font-semibold text-accent-text tabular-nums", shown(c.new, c.field, u)), h("span", "w-12 shrink-0 text-right text-xs text-fg-3", percentChange(c.old, c.new)));
  return row;
}

async function applyBulk() {
  if (!BK.spec) return;
  try {
    const st = await invoke("apply_bulk", { spec: BK.spec });
    updateStatus(st);
    closeBulk();
    await refreshPages();
    setStatus(tf("status.bulk_done", { n: st.changed }));
  } catch (e) {
    showError(e);
  }
}

// ------------------------------------------------------------------------------------------------- moving changes to a new kit

const RB = { plan: null, decisions: {} };

function rebaseValues(c) {
  const row = { unit: c.unit };
  const show = (v) => (v === "" || v === undefined ? t("tip.orig_default") : c.field ? shown(v, c.field, row) : v);
  return show;
}

function rebaseLabel(c) {
  const parts = [...(c.context ?? []).map((k) => `${k.label} ${k.value}`), c.field?.label ?? c.path];
  return parts.join(" · ");
}

function rebaseName(c) {
  const box = h("div", "min-w-0 flex-1");
  box.append(h("div", "truncate font-semibold", c.title || c.key || c.table), h("div", "truncate text-xs text-fg-3", rebaseLabel(c)));
  return box;
}

function rebaseGroup(title, count, ...rows) {
  const box = h("div", "flex flex-col gap-1.5");
  box.append(h("h3", "font-semibold", `${title}: ${count}`), ...rows);
  return box;
}

function rebaseSimpleRow(c) {
  const row = h("div", "flex items-center gap-3 rounded-ctl border border-white/[0.06] px-3 py-1.5");
  const show = rebaseValues(c);
  row.append(rebaseName(c), h("span", "shrink-0 text-fg-3 tabular-nums", show(c.old)), h("span", "ico shrink-0 text-xs text-fg-3", "\uE72A"), h("span", "shrink-0 font-semibold tabular-nums", show(c.new)));
  return row;
}

function rebaseConflictRow(c) {
  const box = h("div", "flex flex-col gap-2 rounded-ctl border border-caution/40 px-3 py-2");
  const top = h("div", "flex items-start gap-3");
  const show = rebaseValues(c);
  const values = h("div", "flex shrink-0 gap-4 text-right text-xs");
  for (const [label, v] of [["rebase.was", c.old], ["rebase.server", c.server], ["rebase.mine", c.new]]) {
    const col = h("div");
    col.append(h("div", "text-fg-3", t(label)), h("div", "text-sm font-semibold tabular-nums", show(v)));
    values.append(col);
  }
  top.append(rebaseName(c), values);
  box.append(top, h("div", "text-xs text-caution", t(c.partial ? "rebase.partial_note" : "rebase.conflict_note")));
  const choose = h("div", "flex gap-4");
  for (const [value, label] of [["mine", "rebase.keep_mine"], ["server", "rebase.keep_server"]]) {
    const opt = h("label", "flex items-center gap-2 text-sm");
    const radio = h("input", "size-4 accent-[#60cdff]");
    radio.type = "radio";
    radio.name = `rb-${c.id}`;
    radio.value = value;
    radio.checked = RB.decisions[c.id] === value;
    radio.addEventListener("change", () => {
      RB.decisions[c.id] = value;
      updateRebaseApply();
    });
    opt.append(radio, h("span", "", t(label)));
    choose.append(opt);
  }
  box.append(choose);
  return box;
}

function rebaseLostRow(c) {
  const row = h("div", "flex items-center gap-3 rounded-ctl border border-white/[0.06] px-3 py-1.5");
  const name = rebaseName(c);
  name.append(h("div", "text-xs text-critical", t(`rebase.reason.${c.reason}`)));
  row.append(name, h("span", "shrink-0 font-semibold tabular-nums", c.new));
  return row;
}

function updateRebaseApply() {
  const missing = RB.plan.conflicts.some((c) => !RB.decisions[c.id]);
  $("rb-apply").disabled = missing;
  $("rb-hint").textContent = missing ? t("rebase.choose") : RB.plan.lost.length ? t("rebase.lost_note") : "";
}

async function openRebase() {
  try {
    const plan = await invoke("rebase_preview");
    RB.plan = plan;
    RB.decisions = {};
    $("rb-text").textContent = tf("rebase.text", { from: plan.from, to: plan.to });
    const groups = [];
    if (plan.conflicts.length) groups.push(rebaseGroup(t("rebase.conflicts"), plan.conflicts.length, ...plan.conflicts.map(rebaseConflictRow)));
    if (plan.lost.length) groups.push(rebaseGroup(t("rebase.lost"), plan.lost.length, ...plan.lost.map(rebaseLostRow)));
    if (plan.auto.length) groups.push(rebaseGroup(t("rebase.auto"), plan.auto.length, ...plan.auto.map(rebaseSimpleRow)));
    if (plan.already.length) groups.push(rebaseGroup(t("rebase.already"), plan.already.length, ...plan.already.map(rebaseSimpleRow)));
    $("rb-body").replaceChildren(...groups);
    updateRebaseApply();
    $("rebase").classList.remove("hidden");
    $("rebase").classList.add("grid");
  } catch (e) {
    showError(e);
  }
}

function closeRebase() {
  $("rebase").classList.add("hidden");
  $("rebase").classList.remove("grid");
}

async function applyRebase() {
  try {
    const r = await invoke("rebase_apply", { decisions: RB.decisions });
    updateStatus(r);
    closeRebase();
    const parts = [tf("rebase.done", { n: r.applied })];
    if (r.lost.length) parts.push(tf("rebase.done_lost", { n: r.lost.length }));
    showView("changes");
    notice(r.lost.length ? "caution" : "ok", parts.join(" "));
  } catch (e) {
    showError(e);
  }
}

// ------------------------------------------------------------------------------------------------- row selection

function paintSelection() {
  for (const [i, el] of G.rendered) {
    const row = rowAt(i);
    const name = el.querySelector(".name-cell");
    if (row && G.selected.has(row.row)) name.dataset.selected = "true";
    else delete name.dataset.selected;
  }
  setStatus(G.selected.size ? tf("status.selected", { n: G.selected.size }) : "");
}

/** Click selects a row, Ctrl adds, Shift takes everything up to the last one clicked */
async function selectRow(index, e) {
  const row = rowAt(index);
  if (!row) return;
  if (e.shiftKey && G.anchor !== null) {
    const [from, to] = [Math.min(G.anchor, index), Math.max(G.anchor, index)];
    if (!e.ctrlKey) G.selected = new Set();
    for (let i = from; i <= to; i++) {
      const rows = await pageRows(Math.floor(i / PAGE));
      if (rows[i % PAGE]) G.selected.add(rows[i % PAGE].row);
    }
  } else if (e.ctrlKey) {
    if (G.selected.has(row.row)) G.selected.delete(row.row);
    else G.selected.add(row.row);
    G.anchor = index;
  } else {
    G.selected = G.selected.size === 1 && G.selected.has(row.row) ? new Set() : new Set([row.row]);
    G.anchor = index;
  }
  paintSelection();
}

// ------------------------------------------------------------------------------------------------- my changes

let noteTimer = 0;

function saveNote() {
  clearTimeout(noteTimer);
  noteTimer = setTimeout(() => {
    S.author = $("ch-author").value;
    invoke("set_note", { author: S.author, comment: $("ch-comment").value }).catch(showError);
    updateSaveState();
  }, 300);
}

function updateSaveState() {
  const st = S.edits;
  let hint = "";
  if (!st || st.count === 0) hint = t("changes.hint_empty");
  else if (st.errors > 0) hint = t("changes.hint_errors");
  else if (!$("ch-author").value.trim()) hint = t("changes.hint_author");
  $("ch-save").disabled = hint !== "";
  $("ch-hint").textContent = hint;
}

function notice(tone, text) {
  $("ch-notice").dataset.tone = tone;
  $("ch-notice-text").textContent = text;
  $("ch-notice").classList.toggle("hidden", !text);
}

function percentChange(oldText, newText) {
  const [o, n] = [Number(oldText), Number(newText)];
  if (oldText === "" || newText === "" || Number.isNaN(o) || Number.isNaN(n) || o === 0) return "";
  const p = Math.round(((n - o) / o) * 100);
  return `${p > 0 ? "+" : ""}${p}%`;
}

async function renderChanges() {
  let data;
  try {
    data = await invoke("list_changes");
  } catch (e) {
    return showError(e);
  }
  updateStatus(data);
  $("ch-author").value = S.author;
  if (document.activeElement !== $("ch-comment")) $("ch-comment").value = data.comment ?? "";
  $("ch-summary").textContent = tf("changes.summary", { n: data.count, e: data.errors, w: data.warnings });
  $("ch-empty").classList.toggle("hidden", data.count > 0);
  updateSaveState();

  const groups = new Map();
  for (const c of data.changes) {
    if (!groups.has(c.table)) groups.set(c.table, { title: c.table_title, items: [] });
    groups.get(c.table).items.push(c);
  }
  const blocks = [...groups].map(([table, g]) => {
    const box = clone("tpl-change-group");
    box.title.textContent = g.title;
    box.count.textContent = String(g.items.length);
    box.rows.replaceChildren(...g.items.map((c) => changeRow(table, c)));
    return box.root;
  });
  $("ch-list").replaceChildren(...blocks);
}

function changeRow(table, c) {
  const x = clone("tpl-change");
  if (c.icon) {
    x.icon.src = convertFileSrc(`${S.kit.version}/${c.icon}`, "kit");
    x.icon.classList.remove("hidden");
  }
  const row = { unit: c.unit };
  x.title.textContent = c.title || c.key;
  x.key.textContent = c.title ? c.key : "";
  x.label.textContent = [...(c.context ?? []).map((k) => `${k.label} ${k.value}`), c.field.label].join(" · ");
  x.label.title = c.path;
  x.old.textContent = c.old === "" ? t("tip.orig_default") : shown(c.old, c.field, row);
  x.new.textContent = shown(c.new, c.field, row);
  x.pct.textContent = percentChange(c.old, c.new);
  if (c.issue) {
    x.issue.dataset.severity = c.issue.severity;
    x.issue.textContent = window.I18N[S.lang][`issue.${c.issue.code}`] ?? c.issue.message;
    x.issue.classList.remove("hidden");
    x.issue.classList.add("block");
  }
  x.revert.title = t("changes.revert");
  x.revert.addEventListener("click", async () => {
    try {
      updateStatus(await invoke("revert", { cells: [{ table, row: c.row, path: c.path }] }));
      renderChanges();
    } catch (e) {
      showError(e);
    }
  });
  x.open.addEventListener("click", () => {
    commitEdit();
    Object.assign(filtersOf(table), { only_mine: true, search: "", top: 0, left: 0 });
    showView(table);
  });
  return x.root;
}

function localStamp() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return { date: `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}-${p(d.getHours())}${p(d.getMinutes())}`, created: `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}` };
}

async function savePatch() {
  clearTimeout(noteTimer);
  const author = $("ch-author").value;
  const comment = $("ch-comment").value;
  try {
    await invoke("set_note", { author, comment });
    S.author = author;
    const stamp = localStamp();
    const path = await invoke("pick_patch_save", { author, stamp: stamp.date });
    if (!path) return;
    const r = await invoke("export_patch", { path, author, comment, created: stamp.created });
    notice("ok", tf("changes.saved", { path: r.path }));
  } catch (e) {
    showError(e);
  }
}

async function openPatch(path) {
  try {
    const r = await invoke("import_patch", { path });
    updateStatus(r);
    const parts = [tf("changes.imported", { n: r.applied })];
    if (r.rejected.length) {
      parts.push(tf("changes.rejected", { n: r.rejected.length }));
      parts.push(r.rejected.slice(0, 5).map((x) => `#${x.at + 1}: ${window.I18N[S.lang][`issue.${x.code}`] ?? x.message}`).join(" "));
    }
    if (r.patch_warnings.some((w) => w.code === "base_differs")) parts.push(t("changes.other_base"));
    notice(r.rejected.length ? "caution" : "ok", parts.join(" "));
    if (!S.author && r.author) S.author = r.author;
    await renderChanges();
  } catch (e) {
    showError(e);
  }
}

function scheduleRender() {
  if (G.frame) return;
  G.frame = requestAnimationFrame(() => {
    G.frame = 0;
    if (G.table && S.view === G.table) renderWindow();
  });
}

// ------------------------------------------------------------------------------------------------- tooltip

let tipTimer = 0;

function hideTip() {
  clearTimeout(tipTimer);
  $("tip").classList.add("hidden");
}

function tipLine(label, value, mono) {
  const line = document.createElement("div");
  line.className = "flex gap-2";
  const a = document.createElement("span");
  a.className = "text-fg-3";
  a.textContent = label;
  const b = document.createElement("span");
  b.className = mono ? "font-mono" : "";
  b.textContent = value;
  line.append(a, b);
  return line;
}

function cellInfo(target) {
  const rowEl = target.closest(".grid-row");
  const cellEl = target.closest(".cell");
  if (!rowEl || !cellEl) return null;
  const row = rowAt(Number(rowEl.dataset.index));
  if (!row) return null;
  if (G.long) return { row, field: row.field, cell: row.cell };
  const left = parseInt(cellEl.style.left, 10);
  const i = Math.round((left - NAME_W) / COL_W);
  return { row, field: G.columns[i], cell: row.cells?.[i] ?? null };
}

function showTip(target, x, y) {
  const info = cellInfo(target);
  if (!info?.field) return hideTip();
  const { row, field, cell } = info;
  const tip = $("tip");
  const title = document.createElement("div");
  title.className = "font-semibold";
  title.textContent = field.label;
  const path = document.createElement("div");
  path.className = "mb-1 font-mono text-[11px] break-all text-fg-4";
  path.textContent = field.path;
  const parts = [title, path];
  if (cell) {
    parts.push(tipLine(t("tip.raw"), cell.v === "" ? t("tip.orig_default") : cell.v, true));
    if (cell.k !== undefined) parts.push(tipLine(t("tip.kit"), cell.k === "" ? t("tip.orig_default") : cell.k, true));
    const issue = cell.e ?? cell.w;
    if (issue) {
      const line = document.createElement("div");
      line.className = cell.e ? "text-critical" : "text-caution";
      line.textContent = t(`issue.${issue}`);
      parts.push(line);
    }
    if (hasOriginal(G.table)) {
      if (cell.n) parts.push(tipLine(t("tip.orig"), t("tip.not_in_orig")));
      else if (cell.o !== undefined) parts.push(tipLine(t("tip.orig"), cell.o === "" ? t("tip.orig_default") : cell.o, true));
      else parts.push(tipLine(t("tip.orig"), t("tip.same")));
    }
  }
  if (field.range) parts.push(tipLine(t(hasOriginal(G.table) ? "tip.range" : "tip.range_kit"), field.range, true));
  if (!field.editable) {
    const line = document.createElement("div");
    line.className = "text-fg-3";
    line.textContent = t("tip.readonly");
    parts.push(line);
  }
  tip.replaceChildren(...parts);
  tip.classList.remove("hidden");
  const r = tip.getBoundingClientRect();
  tip.style.left = `${Math.min(x + 12, window.innerWidth - r.width - 8)}px`;
  tip.style.top = `${y + r.height + 20 > window.innerHeight ? y - r.height - 10 : y + 16}px`;
}

// ------------------------------------------------------------------------------------------------- events

function setFilter(change, opts) {
  Object.assign(filtersOf(G.table), change);
  reload(opts);
}

function wire() {
  $("lang").addEventListener("click", async () => {
    S.lang = S.lang === "ru" ? "en" : "ru";
    applyTexts();
    await invoke("set_language", { lang: S.lang }).catch(showError);
    if (S.view === "changes") notice("", "");
    if (S.kit) {
      const st = await invoke("app_state");
      S.kit = st.kit;
      S.edits = st.kit?.edits ?? S.edits;
      showPage();
    }
    else renderRecent();
  });
  $("error-close").addEventListener("click", clearError);
  $("open-zip").addEventListener("click", () => pick("zip"));
  $("open-folder").addEventListener("click", () => pick("folder"));
  $("nav-summary").addEventListener("click", () => showView("summary"));
  $("nav-items").addEventListener("click", () => showView("items"));
  $("nav-curves").addEventListener("click", () => showView("curves"));
  $("nav-loot").addEventListener("click", () => showView("loot"));

  let itemTimer = 0;
  $("it-search").addEventListener("input", (e) => {
    clearTimeout(itemTimer);
    const value = e.target.value;
    itemTimer = setTimeout(() => {
      IT.search = value;
      refreshItems();
    }, 120);
  });
  $("it-kind").addEventListener("change", (e) => {
    IT.kind = e.target.value;
    refreshItems();
  });
  $("it-rarity").addEventListener("change", (e) => {
    IT.rarityFilter = e.target.value;
    refreshItems();
  });
  $("it-mine").addEventListener("click", () => {
    IT.mine = !IT.mine;
    refreshItems();
  });

  let lootTimer = 0;
  $("lt-search").addEventListener("input", (e) => {
    clearTimeout(lootTimer);
    const value = e.target.value;
    lootTimer = setTimeout(() => {
      LT.search = value;
      refreshLoot();
    }, 120);
  });
  $("lt-map").addEventListener("change", (e) => {
    LT.map = e.target.value;
    $("lt-card").scrollTop = 0;
    refreshLoot();
  });
  $("lt-kind").addEventListener("change", (e) => {
    LT.kind = e.target.value;
    refreshLoot();
  });
  $("lt-mine").addEventListener("click", () => {
    LT.mine = !LT.mine;
    refreshLoot();
  });

  $("cv-attr").addEventListener("change", (e) => {
    CV.path = e.target.value;
    loadCurves();
  });
  new ResizeObserver(() => {
    if (S.view === "curves" && CV.data && !CV.drag) renderCurves();
  }).observe($("cv-chart"));

  $("rb-later").addEventListener("click", closeRebase);
  $("rb-apply").addEventListener("click", applyRebase);
  $("f-bulk").addEventListener("click", openBulk);
  $("bk-cancel").addEventListener("click", closeBulk);
  $("bk-apply").addEventListener("click", applyBulk);
  $("bk-search").addEventListener("input", renderBulkFields);
  $("bk-value").addEventListener("input", scheduleBulkPreview);
  $("bk-pairs").addEventListener("change", scheduleBulkPreview);
  for (const b of document.querySelectorAll("#bulk [data-op]")) b.addEventListener("click", () => setBulkOp(b.dataset.op));
  $("bk-all").addEventListener("click", () => {
    const visible = bulkVisibleFields();
    const allOn = visible.every((f) => BK.checked.has(f.path));
    for (const f of visible) {
      if (allOn) BK.checked.delete(f.path);
      else BK.checked.add(f.path);
    }
    renderBulkFields();
    scheduleBulkPreview();
  });
  $("bulk").addEventListener("keydown", (e) => {
    if (e.key === "Escape") closeBulk();
  });
  $("nav-changes").addEventListener("click", () => {
    notice("", "");
    showView("changes");
  });
  $("undo").addEventListener("click", () => history("undo"));
  $("redo").addEventListener("click", () => history("redo"));
  $("ch-author").addEventListener("input", saveNote);
  $("ch-comment").addEventListener("input", saveNote);
  $("ch-save").addEventListener("click", savePatch);
  $("ch-open").addEventListener("click", async () => {
    try {
      const path = await invoke("pick_patch_open");
      if (path) await openPatch(path);
    } catch (e) {
      showError(e);
    }
  });
  $("nav-change").addEventListener("click", async () => {
    await commitEdit();
    await invoke("close_kit").catch(showError);
    S.kit = null;
    G.table = null;
    await refreshState();
    showPage();
  });

  let searchTimer = 0;
  $("f-search").addEventListener("input", (e) => {
    clearTimeout(searchTimer);
    const value = e.target.value;
    searchTimer = setTimeout(() => setFilter({ search: value }), 120);
  });
  $("f-wide").addEventListener("click", () => setFilter({ view: "wide", sort: "", desc: false }));
  $("f-long").addEventListener("click", () => setFilter({ view: "long", sort: "", desc: false }));
  $("f-diff").addEventListener("click", () => setFilter({ only_diff: !filtersOf(G.table).only_diff }));
  $("f-mine").addEventListener("click", () => setFilter({ only_mine: !filtersOf(G.table).only_mine }));
  $("f-all").addEventListener("click", () => setFilter({ all_fields: !filtersOf(G.table).all_fields, sort: "", desc: false }));
  $("f-kind").addEventListener("change", (e) => setFilter({ kind: e.target.value }));
  $("f-rarity").addEventListener("change", (e) => setFilter({ rarity: e.target.value }));

  $("grid").addEventListener("scroll", () => {
    hideTip();
    scheduleRender();
  }, { passive: true });
  new ResizeObserver(() => scheduleRender()).observe($("grid"));

  $("grid-head").addEventListener("click", (e) => {
    const key = e.target.closest("[data-sort]")?.dataset.sort;
    if (!key) return;
    const f = filtersOf(G.table);
    // ascending -> descending -> table order
    if (f.sort !== key) setFilter({ sort: key, desc: false }, { keepScroll: true });
    else if (!f.desc) setFilter({ desc: true }, { keepScroll: true });
    else setFilter({ sort: "", desc: false }, { keepScroll: true });
  });

  $("grid-body").addEventListener("mouseover", (e) => {
    clearTimeout(tipTimer);
    const target = e.target;
    const { clientX: x, clientY: y } = e;
    if (!target.closest?.(".cell")) return hideTip();
    tipTimer = setTimeout(() => showTip(target, x, y), 350);
  });
  $("grid").addEventListener("mouseleave", hideTip);

  document.addEventListener("keydown", (e) => {
    if (e.ctrlKey && e.key.toLowerCase() === "f" && S.kit && S.view === G.table) {
      e.preventDefault();
      $("f-search").focus();
      $("f-search").select();
    }
    const typing = ["INPUT", "SELECT", "TEXTAREA"].includes(document.activeElement?.tagName);
    if (e.ctrlKey && S.kit && !typing) {
      const k = e.key.toLowerCase();
      if (k === "z" && !e.shiftKey) {
        e.preventDefault();
        history("undo");
      } else if (k === "y" || (k === "z" && e.shiftKey)) {
        e.preventDefault();
        history("redo");
      }
    }
  });

  const body = $("grid-body");
  body.addEventListener("mousedown", (e) => {
    if (e.target.id === "cell-input") return;
    commitEdit();
    const nameCell = e.target.closest?.(".name-cell");
    if (nameCell) {
      selectRow(Number(nameCell.parentElement.dataset.index), e);
      e.preventDefault();
      $("grid").focus({ preventScroll: true });
      return;
    }
    const cell = e.target.closest?.(".cell[data-col]");
    const rowEl = e.target.closest?.(".grid-row");
    if (cell && rowEl) {
      G.cursor = { index: Number(rowEl.dataset.index), col: Number(cell.dataset.col) };
      paintCursor();
      setStatus(G.selected.size ? tf("status.selected", { n: G.selected.size }) : "");
    }
    e.preventDefault();
    $("grid").focus({ preventScroll: true });
  });
  body.addEventListener("dblclick", (e) => {
    if (e.target.closest?.(".cell[data-col]")) startEdit();
  });

  $("grid").addEventListener("keydown", (e) => {
    if (G.editing || e.target.id === "cell-input" || e.ctrlKey || e.metaKey || e.altKey) return;
    const moves = { ArrowUp: [-1, 0], ArrowDown: [1, 0], ArrowLeft: [0, -1], ArrowRight: [0, 1], Enter: [e.shiftKey ? -1 : 1, 0], Tab: [0, e.shiftKey ? -1 : 1] };
    if (moves[e.key]) {
      e.preventDefault();
      moveCursor(...moves[e.key]);
    } else if (e.key === "PageDown" || e.key === "PageUp") {
      e.preventDefault();
      moveCursor((e.key === "PageDown" ? 1 : -1) * Math.floor($("grid").clientHeight / ROW_H - 2), 0);
    } else if (e.key === "F2") {
      e.preventDefault();
      startEdit();
    } else if (e.key === "Delete") {
      const data = cursorData();
      if (data?.field.editable && data.cell?.k !== undefined) sendEdits([{ table: G.table, row: data.row.row, path: data.field.path, new: "" }]);
    } else if (e.key.length === 1 && G.cursor) {
      e.preventDefault();
      startEdit(e.key);
    }
  });

  $("cell-input").addEventListener("keydown", (e) => {
    e.stopPropagation();
    const finish = async (rows, cols) => {
      e.preventDefault();
      await commitEdit();
      moveCursor(rows, cols);
      $("grid").focus({ preventScroll: true });
    };
    if (e.key === "Enter") finish(e.shiftKey ? -1 : 1, 0);
    else if (e.key === "Tab") finish(0, e.shiftKey ? -1 : 1);
    else if (e.key === "Escape") {
      e.preventDefault();
      cancelEdit();
      $("grid").focus({ preventScroll: true });
    }
  });
  $("cell-input").addEventListener("blur", () => commitEdit());

  document.addEventListener("copy", (e) => {
    if (document.activeElement !== $("grid")) return;
    const data = cursorData();
    if (!data) return;
    e.clipboardData.setData("text/plain", data.cell ? shown(data.cell.v, data.field, data.row) : "");
    e.preventDefault();
  });
  document.addEventListener("paste", (e) => {
    if (document.activeElement !== $("grid") || !G.cursor) return;
    e.preventDefault();
    pasteBlock(e.clipboardData.getData("text/plain"));
  });

  const over = (on) => {
    $("drop").dataset.over = String(on);
  };
  listen("tauri://drag-enter", () => over(true));
  listen("tauri://drag-leave", () => over(false));
  listen("tauri://drag-drop", (e) => {
    over(false);
    const path = e.payload?.paths?.[0];
    if (!path) return;
    // A .json dropped on my changes is a patch; everything else is a kit
    if (S.kit && S.view === "changes" && path.toLowerCase().endsWith(".json")) openPatch(path);
    else openWith(() => invoke("open_kit", { path }));
  });
}

(async function main() {
  wire();
  try {
    const st = await invoke("app_state");
    S.lang = st.lang;
    S.author = st.author;
    S.recent = st.recent;
    S.kit = st.kit;
    S.edits = st.kit?.edits ?? null;
  } catch (e) {
    showError(e);
  }
  applyTexts();
  showPage();
})();
