"use strict";
// Sandbox: stands in for window.__TAURI__ in a plain browser, so the UI can be worked on without Rust. Inside the app
// window.__TAURI__ already exists and this file does nothing. The contract it imitates: ../api.d.ts.
//   ?scenario=start     start screen with recent kits (default)
//   ?scenario=kit       a kit is open
//   ?scenario=old       an old kit without catalog.json
//   ?scenario=error     opening a kit fails
//   ?scenario=rebase    opening a kit offers to move changes from an older draft

(() => {
  if (window.__TAURI__) return;
  const scenario = new URLSearchParams(location.search).get("scenario") ?? "start";
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const icon = (c) => `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect width="24" height="24" rx="4" fill="${c}"/></svg>`)}`;
  const ATTRS = [["Сила", "first_level_strength"], ["Ловкость", "first_level_dexterity"], ["Выносливость", "first_level_stamina"], ["Интеллект", "first_level_intelligence"], ["Воля", "first_level_will"], ["Вера", "first_level_faith"]];
  const CLASSES = [["Мастер меча", 400001], ["Боец", 400002], ["Жрец", 400003], ["Разбойник", 400004], ["Пиромант", 400005], ["Рыцарь смерти", 400006], ["Криомант", 400007], ["Друид", 400008]];
  const KINDS = ["EQUIP_WEAPON", "EQUIP_ARMOR", "EQUIP_ACCESSORY"];
  const RARITIES = [["ROTTED", "Низкое качество"], ["NORMAL", "Обычное"], ["UNCOMMON", "Редкое"], ["EPIC", "Эпическое"], ["LEGENDARY", "Легендарное"]];

  const col = (id, path, label, editable, extra = {}) => ({ id, path, label, editable, value_like: /(min|max|fixed)_value$/.test(path), ...extra });

  function makeClasses() {
    const columns = [col(0, "role_level", "Уровень", false)];
    ATTRS.forEach(([n, a], i) => columns.push(col(i + 1, `role_attribute.${a}.fixed_value`, n, true, { unit: "очки", range: "0…60" })));
    const rows = [];
    for (const [title, key] of CLASSES) {
      for (let lvl = 1; lvl <= 20; lvl++) {
        const cells = columns.map((c, i) => {
          if (i === 0) return { v: String(lvl) };
          const v = 5 + i * 2 + Math.floor(lvl * (1 + i / 3));
          return key === 400005 && i === 4 && lvl > 10 ? { v: String(v + 5), o: String(v) } : { v: String(v) };
        });
        rows.push({ row: rows.length, key: String(key), title, icon: "x", cells });
      }
    }
    return { columns, rows, kinds: [], rarities: [] };
  }

  function makeItems() {
    const columns = [col(0, "durability", "прочность", true, { range: "50…200" }), col(1, "display.as_show", "as_show", false)];
    const rows = [];
    const longRows = [];
    for (let i = 0; i < 550; i++) {
      const kind = KINDS[i % KINDS.length];
      const rarity = RARITIES[i % RARITIES.length];
      const title = `${["Меч", "Щит", "Кольцо"][i % 3]} ${i}`;
      const key = String(181010000 + i * 1000);
      const durability = i % 40 === 0 ? { v: "150", o: "100" } : i % 7 === 0 ? null : { v: "100" };
      const r = { row: i, key, title, icon: "x", kind, rarity: rarity[0], cells: [durability, { v: "1" }] };
      if (i === 549) r.new = true;
      rows.push(r);
      for (const [j, [n, a]] of ATTRS.entries()) {
        const f = col(j, `data.fixed_rule[0].attribute.${a}.offset.min_value`, `${n} мин.`, true, { unit: j === 5 ? "доля" : "очки", range: "0…100" });
        const c = j === 5 ? { v: "0.05" } : i % 50 === 0 && j === 0 ? { v: "12", o: "10" } : { v: String(10 + j) };
        longRows.push({ ...r, cells: undefined, field: f, cell: c });
      }
    }
    return { columns, rows, longRows, kinds: KINDS, rarities: RARITIES.map(([id, label]) => ({ id, label })), hasAll: true };
  }

  const TABLES = {
    role_level_upgrade: { data: makeClasses(), info: { title: "Классы: атрибуты по уровням", about: "Базовые атрибуты персонажа каждого класса на каждом уровне.", where: "игровой сервер + клиент", layout: "wide", changed: 10 } },
    affix_template: { data: makeItems(), info: { title: "Предметы: базовые характеристики", about: "Базовые свойства каждого предмета.", where: "игровой сервер и клиент", layout: "long", changed: 11, new_rows: 1 } },
    affix_level_pool_rule: { data: { columns: [col(0, "rule_id", "rule_id", false), col(1, "level_id", "level_id", false)], rows: [{ row: 0, key: "1", title: "", cells: [{ v: "1" }, { v: "10101" }] }, { row: 1, key: "2", title: "", cells: [{ v: "2" }, { v: "10102" }] }], kinds: [], rarities: [] }, info: { title: "Справка: пулы аффиксов", about: "Только для чтения.", where: "бэкенд", layout: "wide", changed: 0 } },
  };

  // my changes: "table|row|path" -> new value; history holds whole snapshots, it's a sandbox
  const edits = new Map();
  const undoStack = [];
  const redoStack = [];
  let comment = "";
  const editKey = (table, row, path) => `${table}|${row}|${path}`;
  const issueOf = (v) => (Number.isNaN(Number(v)) ? "new_not_number" : null);
  const overlay = (table, row, field, cell) => {
    const e = edits.get(editKey(table, row, field.path));
    if (e === undefined) return cell;
    const out = { ...(cell ?? {}), v: e, k: cell?.v ?? "" };
    const issue = issueOf(e);
    if (issue) out.e = issue;
    return out;
  };
  const status = () => {
    const errors = [...edits.values()].filter(issueOf).length;
    return { count: edits.size, errors, warnings: 0, can_undo: undoStack.length > 0, can_redo: redoStack.length > 0, comment };
  };

  const ref = (table, row, f, cell, row_unit = "") => ({ ...f, table, row, row_unit, cell: overlay(table, row, f, cell) });
  const roll = (lo, hi, step) => {
    const n = Math.floor((hi - lo) / step) + 1;
    return { count: n, values: Array.from({ length: Math.min(n, 12) }, (_, i) => String(lo + i * step)), more: false };
  };
  function itemCard(id, rarity) {
    const it = TABLES.affix_template.data.rows.find((r) => r.key === id);
    const n = (a, m) => col(0, `data.fixed_rule[0].attribute.${a}.${m}`, a === "attack" ? `Урон ${m}` : "Шанс крит. удара", true, { unit: a === "crit" ? "доля" : "очки", range: "0…100" });
    const t = "affix_template";
    const side = (r, kind, lo) => ({
      min: ref("affix_value_template", r, col(0, `data.affix_data[${kind}].rarity_range[3].min_value`, "мин.", true, { unit: "очки" }), { v: String(lo) }),
      max: ref("affix_value_template", r, col(0, `data.affix_data[${kind}].rarity_range[3].max_value`, "макс.", true, { unit: "очки" }), { v: String(lo + 4) }),
      roll: roll(lo, lo + 4, 1),
    });
    return {
      id, title: it.title, icon: "x", kind: it.kind, rarity: it.rarity, rarity_label: it.rarity,
      base: [
        { attr: "attack", label: "Урон за удар", min: ref(t, it.row, n("attack", "offset.min_value"), { v: "10" }), max: ref(t, it.row, n("attack", "offset.max_value"), { v: "14" }), roll: roll(10, 14, 2) },
        { attr: "crit", label: "Шанс крит. удара", fixed: ref(t, it.row, n("crit", "fixed_value"), { v: "0.05" }, "доля") },
      ],
      durability: ref("gear_template", it.row, col(0, "durability", "прочность", true), { v: "100" }),
      sell: ref("item_type", it.row, col(0, "system_repurchase_price", "цена выкупа", true), { v: "40" }),
      trade: [{ market: "2", fields: [ref("trade_commodity", it.row, col(0, "price_def", "базовая цена", true), { v: "900" }), ref("trade_commodity", it.row, col(1, "tax_percentage", "налог, %", true), { v: "25" })] }],
      dismantle: [{ ...ref("gear_dismantle", it.row, col(0, "reward[0].count", "материал кол-во (0)", true), { v: "10" }), item: "Железо" }],
      craft: [],
      market: [{ product: "700001", fields: [{ ...ref("market", it.row, col(0, "cost_price", "цена", true), { v: "50" }), currency: "Золото" }] }],
      affix: { group: "102", shared: 52, rarity: rarity || "UNCOMMON", rarities: RARITIES.map(([k, label]) => ({ id: k, label })), rows: ATTRS.map(([title], i) => ({ row: i, title, unit: "очки", attr: title, major: side(i, 0, 4 + i), minor: side(i, 1, 2 + i) })) },
    };
  }
  function classCurves(path) {
    const { columns, rows } = TABLES.role_level_upgrade.data;
    const attrs = columns.filter((c) => c.editable).map((c) => ({ ...c, varies: true }));
    const picked = attrs.find((a) => a.path === path) ?? attrs[0];
    const idx = columns.findIndex((c) => c.path === picked.path);
    const classes = CLASSES.map(([title, id]) => ({
      id: String(id), title,
      levels: rows.filter((r) => r.key === String(id)).map((r) => ({ level: Number(r.cells[0].v), ref: ref("role_level_upgrade", r.row, picked, r.cells[idx]) })),
    }));
    return { attrs, path: picked.path, classes };
  }
  function bulkPreview(spec) {
    const { data } = TABLES[spec.table];
    const rows = data.rows.filter((r) => !spec.rows || spec.rows.includes(r.row));
    const changes = [];
    let skipped = 0;
    for (const r of rows) {
      data.columns.forEach((c, i) => {
        if (!spec.paths.includes(c.path)) return;
        const now = edits.get(editKey(spec.table, r.row, c.path)) ?? r.cells[i]?.v;
        if (now === undefined) return (skipped += 1);
        const v = spec.op === "mul" ? Number(now) * spec.value : spec.op === "add" ? Number(now) + spec.value : spec.value;
        changes.push({ row: r.row, key: r.key, title: r.title, unit: "", field: c, old: now, new: String(Number(v.toPrecision(10))) });
      });
    }
    return { total: changes.length, skipped: 0, empty: skipped, errors: 0, warnings: 0, changes: changes.slice(0, 300) };
  }

  const overview = (old) => ({
    features: { items: true, curves: true },
    edits: status(),
    version: "20261106-542ba6f5",
    title: "Dungeonborne — баланс",
    season: null,
    has_catalog: !old,
    notes: ["Урон умений здесь НЕ меняется: его считают эффекты умений и кривые урона в паке игры.", "Доля: 0.05 = 5 %. Очки: целые единицы атрибута."],
    tables: Object.entries(TABLES).map(([name, t]) => ({
      name, title: t.info.title, about: t.info.about, where: t.info.where, layout: t.info.layout,
      rows: t.data.rows.length, values: t.data.rows.length * t.data.columns.length,
      editable_values: t.data.columns.some((c) => c.editable) ? t.data.rows.length * t.data.columns.filter((c) => c.editable).length : 0,
      changed: t.info.changed, new_rows: t.info.new_rows ?? 0, removed_rows: 0, has_orig: name !== "affix_level_pool_rule", source: "base",
    })),
  });

  const state = { lang: "en", kit: scenario === "kit" ? overview(false) : scenario === "old" ? overview(true) : null, recent: [{ version: "20261104-bc119176", title: "Dungeonborne — баланс сезона Dead Money", season: "dead-money", source: "C:\\kits\\dead-money.zip" }, { version: "20261106-542ba6f5", title: "Dungeonborne — баланс", season: null, source: "C:\\kits\\base.zip" }] };

  function compare(a, b) {
    if (a === "" || b === "") return a === b ? 0 : a === "" ? 2 : -2;
    const [x, y] = [Number(a), Number(b)];
    return Number.isNaN(x) || Number.isNaN(y) ? String(a).localeCompare(String(b)) : x - y;
  }

  function query({ table, query: q, offset, limit }) {
    const { data } = TABLES[table];
    const long = q.view === "long" || (q.view === "" && TABLES[table].info.layout === "long" && !!data.longRows);
    const words = q.search.toLowerCase().split(/\s+/).filter(Boolean);
    let columns = data.columns.filter((c) => q.all_fields || c.editable || c.path === "role_level" || !data.columns.some((x) => x.editable));
    const rowHasEdit = (r) => [...edits.keys()].some((k) => k.startsWith(`${table}|${r.row}|`));
    const passes = (r) => (!q.kind || r.kind === q.kind) && (!q.rarity || r.rarity === q.rarity) && (!q.only_mine || rowHasEdit(r));
    let list;
    let matched = false;
    if (long && data.longRows) {
      list = data.longRows
        .filter((r) => passes(r) && (!q.only_mine || edits.has(editKey(table, r.row, r.field.path))) && (!q.only_diff || r.cell.o !== undefined) && words.every((w) => `${r.title} ${r.key} ${r.field.label} ${r.field.path}`.toLowerCase().includes(w)))
        .map((r) => ({ ...r, cell: overlay(table, r.row, r.field, r.cell) }));
    } else {
      const base = data.rows.filter((r) => passes(r) && (!q.only_diff || r.new || r.cells.some((c) => c?.o !== undefined)));
      list = base.filter((r) => words.every((w) => `${r.title} ${r.key}`.toLowerCase().includes(w)));
      if (!list.length && words.length) {
        const cols = columns.filter((c) => words.every((w) => `${c.label} ${c.path}`.toLowerCase().includes(w)));
        if (cols.length) {
          columns = cols;
          list = base;
          matched = true;
        }
      }
      const idx = columns.map((c) => data.columns.indexOf(c));
      list = list.map((r) => ({ ...r, cells: idx.map((i, n) => overlay(table, r.row, columns[n], r.cells[i])) }));
    }
    if (q.sort) {
      const value = (r) => {
        if (q.sort === "name") return r.title;
        if (q.sort === "key") return r.key;
        if (q.sort === "value") return r.cell?.v ?? "";
        if (q.sort === "label") return r.field?.label ?? "";
        const i = columns.findIndex((c) => String(c.id) === q.sort);
        return r.cells?.[i]?.v ?? "";
      };
      list = [...list].sort((a, b) => {
        const o = compare(value(a), value(b));
        return Math.abs(o) === 2 ? o : q.desc ? -o : o;
      });
    }
    const page = { total: list.length, offset, long: long && !!data.longRows, rows: list.slice(offset, offset + limit) };
    if (offset === 0) Object.assign(page, { columns, columns_matched: matched, kinds: data.kinds, rarities: data.rarities, has_all_fields: !!data.hasAll || table === "role_level_upgrade" });
    return page;
  }

  const commands = {
    app_state: () => ({ lang: state.lang, author: state.author ?? "", recent: state.recent, kit: state.kit, version: "sandbox" }),
    set_language: ({ lang }) => { state.lang = lang; },
    pick_kit: () => "C:\\kits\\base.zip",
    open_kit: async () => {
      await sleep(400);
      if (scenario === "error") throw { code: "kit_unsafe", message: "опасный путь в архиве: ../evil.txt" };
      state.kit = overview(false);
      return scenario === "rebase" ? { ...state.kit, rebase_from: "20261104-aaaaaaaa" } : state.kit;
    },
    open_recent: async () => { await sleep(200); return (state.kit = overview(false)); },
    close_kit: () => { state.kit = null; },
    query_rows: query,
    set_values: ({ edits: list }) => {
      undoStack.push(new Map(edits));
      redoStack.length = 0;
      for (const e of list) {
        const key = editKey(e.table, e.row, e.path);
        if (e.new === "") edits.delete(key);
        else edits.set(key, e.new.replace(",", "."));
      }
      return { ...status(), changed: list.length, rejected: [] };
    },
    revert: ({ cells }) => commands.set_values({ edits: cells.map((c) => ({ ...c, new: "" })) }),
    undo: () => {
      if (undoStack.length) {
        redoStack.push(new Map(edits));
        const prev = undoStack.pop();
        edits.clear();
        prev.forEach((v, k) => edits.set(k, v));
      }
      return status();
    },
    redo: () => {
      if (redoStack.length) {
        undoStack.push(new Map(edits));
        const next = redoStack.pop();
        edits.clear();
        next.forEach((v, k) => edits.set(k, v));
      }
      return status();
    },
    list_changes: () => ({
      ...status(),
      changes: [...edits].map(([key, v]) => {
        const [table, row, path] = key.split("|");
        const info = TABLES[table];
        const r = info.data.rows.find((x) => x.row === Number(row));
        const pool = info.data.longRows?.map((x) => x.field) ?? [];
        const field = info.data.columns.find((c) => c.path === path) ?? pool.find((c) => c.path === path);
        const issue = issueOf(v);
        return { table, table_title: info.info.title, row: Number(row), path, key: r?.key, title: r?.title, icon: r?.icon, field, old: "10", new: v, ...(issue ? { issue: { severity: "error", code: issue, message: issue } } : {}) };
      }),
    }),
    set_note: (a) => { comment = a.comment; state.author = a.author; },
    pick_patch_open: () => "C:\\kits\\patch-llm.json",
    pick_patch_save: ({ author, stamp }) => `C:\\kits\\patch-${author}-${stamp}.json`,
    import_patch: () => ({ ...status(), applied: 0, rejected: [{ at: 0, code: "old_mismatch", message: "#1 gear_template row 0 durability: old 123, а в наборе 100" }], patch_warnings: [], author: "llm" }),
    export_patch: ({ path }) => ({ path, count: edits.size }),
    rebase_preview: () => {
      const f = TABLES.role_level_upgrade.data.columns[1];
      const base = { table: "role_level_upgrade", path: f.path, key: "400005", title: "Пиромант", field: f, unit: "", table_title: "Классы" };
      return {
        from: "20261104-aaaaaaaa", to: "20261106-542ba6f5", comment: "",
        auto: [{ ...base, id: 0, row: 3, old: "30", new: "35", context: [{ label: "Уровень", value: "3" }] }],
        already: [{ ...base, id: 1, row: 4, old: "30", new: "31", context: [{ label: "Уровень", value: "4" }] }],
        conflicts: [{ ...base, id: 2, row: 5, old: "30", new: "35", server: "33", partial: false, context: [{ label: "Уровень", value: "5" }] }, { ...base, id: 3, row: 6, old: "31", new: "36", server: "31", partial: true, context: [{ label: "Уровень", value: "6" }] }],
        lost: [{ id: 4, table: "role_level_upgrade", path: "role_attribute.old_field.fixed_value", key: "400009", title: "Старый класс", old: "1", new: "2", reason: "row_gone" }],
      };
    },
    rebase_apply: () => ({ ...status(), applied: 2, lost: [{ id: 4 }] }),
    list_items: ({ query: q }) => ({
      items: TABLES.affix_template.data.rows
        .filter((r) => (!q.kind || r.kind === q.kind) && (!q.rarity || r.rarity === q.rarity) && `${r.title} ${r.key}`.toLowerCase().includes(q.search.toLowerCase()))
        .map((r) => ({ id: r.key, title: r.title, icon: "x", kind: r.kind, rarity: r.rarity, mine: false })),
      kinds: KINDS,
      rarities: RARITIES.map(([id, label]) => ({ id, label })),
    }),
    get_item: ({ id, rarity }) => itemCard(id, rarity),
    class_curves: ({ path }) => classCurves(path),
    bulk_fields: ({ table, rows }) => {
      const { data } = TABLES[table];
      return { rows: rows?.length ?? data.rows.length, fields: data.columns.filter((c) => c.editable).map((c) => ({ ...c, count: data.rows.length })) };
    },
    preview_bulk: ({ spec }) => bulkPreview(spec),
    apply_bulk: ({ spec }) => {
      const p = bulkPreview(spec);
      undoStack.push(new Map(edits));
      redoStack.length = 0;
      const { data } = TABLES[spec.table];
      for (const c of p.changes) edits.set(editKey(spec.table, c.row, c.field.path), c.new);
      void data;
      return { ...status(), changed: p.total };
    },
  };

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        if (!commands[cmd]) throw { code: "bad_request", message: cmd };
        return commands[cmd](args ?? {});
      },
      convertFileSrc: (p) => icon(`hsl(${[...p].reduce((a, c) => a + c.charCodeAt(0), 0) % 360} 45% 45%)`),
    },
    event: { listen: async () => () => {} },
  };
})();
