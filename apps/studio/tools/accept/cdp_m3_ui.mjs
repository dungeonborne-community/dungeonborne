// node cdp_m3_ui.mjs <port> <kit.zip> <outdir>: item card, class curves and bulk edit on the real exe, real mouse and keyboard.
import { writeFileSync } from "node:fs";
const [port, kit, out] = process.argv.slice(2);
const page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
const problems = [];
const origins = new Set();
ws.addEventListener("message", (m) => {
  const x = JSON.parse(m.data);
  if (x.id && pending.has(x.id)) { pending.get(x.id)(x); pending.delete(x.id); return; }
  if (x.method === "Runtime.exceptionThrown") problems.push("exception: " + (x.params.exceptionDetails.exception?.description ?? x.params.exceptionDetails.text));
  if (x.method === "Log.entryAdded" && ["error", "warning"].includes(x.params.entry.level)) problems.push(`log ${x.params.entry.level}: ${x.params.entry.text}`);
  if (x.method === "Network.requestWillBeSent") origins.add(new URL(x.params.request.url).origin);
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true, userGesture: true });
  if (r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text));
  return r.result.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const shot = async (name) => { const s = await send("Page.captureScreenshot", { format: "png" }); writeFileSync(`${out}/${name}.png`, Buffer.from(s.result.data, "base64")); };
const VK = { Enter: 13, Tab: 9, Escape: 27 };
async function key(k, mods = 0) {
  const special = VK[k];
  const vk = special ?? (k === "." ? 190 : k === "-" ? 189 : k.toUpperCase().charCodeAt(0));
  const base = { key: k, modifiers: mods, windowsVirtualKeyCode: vk, code: special ? k : "" };
  const text = !special && !(mods & 2) ? k : undefined;
  await send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", ...base, ...(text ? { text } : {}) });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  await sleep(30);
}
const type = async (s) => { for (const c of s) await key(c); };
async function click(selector, count = 1) {
  const r = await ev(`(() => { const el = ${selector}; if (!el) return null; el.scrollIntoView({ block: "center" }); const b = el.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  if (!r) throw new Error(`nothing at ${selector}`);
  for (let i = 1; i <= count; i++) {
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: i });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: i });
  }
  await sleep(80);
}
const status = () => ev(`invoke("list_changes").then(({ changes, ...st }) => st)`);
const changes = () => ev(`invoke("list_changes").then((d) => d.changes.map((c) => [c.table, c.row, c.path, c.old, c.new, c.issue?.code ?? null]))`);
let failed = 0;
const eq = (name, got, want) => {
  const [a, b] = [JSON.stringify(got), JSON.stringify(want)];
  const ok = a === b;
  if (!ok) failed++;
  console.log(`${ok ? "ok " : "BAD"} ${name}${ok ? "" : `: got ${a}, want ${b}`}`);
};
const yes = (name, v) => eq(name, !!v, true);

for (const d of ["Runtime", "Log", "Network", "Page"]) await send(`${d}.enable`);
await ev(`openWith(() => invoke("open_kit", { path: ${JSON.stringify(kit)} }))`);
eq("starts clean", (await status()).count, 0);
eq("screens are on", await ev(`[document.getElementById("nav-items").classList.contains("hidden"), document.getElementById("nav-curves").classList.contains("hidden")]`), [false, false]);

// ---------------------------------------------------------------------------------- item card
await click(`document.getElementById("nav-items")`);
await sleep(500);
console.log("   items:", await ev(`document.getElementById("it-count").textContent`));
yes("list shows icons", await ev(`document.querySelectorAll("#it-list img").length > 100`));
await shot("60_item_first");
await ev(`(IT.search = "Ржавый меч", refreshItems())`);
await sleep(400);
const firstCard = await ev(`({ title: document.querySelector("#it-card h1")?.textContent, sections: [...document.querySelectorAll("#it-card h2")].map((x) => x.textContent), inputs: document.querySelectorAll("#it-card input.num").length })`);
console.log("   first card:", JSON.stringify(firstCard));
eq("item is on one screen", firstCard.sections, ["Базовые свойства", "Торговец", "Аффиксы"]);
yes("durability is there too", await ev(`document.querySelector('#it-card input[data-key*="durability"]')`));

// economy of an item: a sword sold on the market, and a potion that has no properties at all
await ev(`(IT.search = "Меч путешественника", refreshItems())`);
await sleep(400);
const sword = await ev(`({ sections: [...document.querySelectorAll("#it-card h2")].map((x) => x.textContent), sell: IT.data.sell?.cell?.v ?? null, trade: IT.data.trade?.length, dismantle: IT.data.dismantle?.map((f) => [f.label, f.cell?.v, f.item]) })`);
console.log("   sword:", JSON.stringify(sword));
yes("the market is on the card", sword.sections.includes("Рынок игроков"));
yes("dismantling is on the card", sword.sections.includes("Разбор"));
await shot("63_item_economy");
const tradeField = await ev(`IT.data.trade[0].fields.find((f) => f.path === "price_def")`);
await click(`document.querySelector('#it-card input[data-key="${tradeField.table}|${tradeField.row}|${tradeField.path}"]')`);
await type(String(Number(tradeField.cell.v) + 50));
await key("Enter");
await sleep(400);
eq("market price edited on the card", await ev(`IT.data.trade[0].fields.find((f) => f.path === "price_def").cell.v`), String(Number(tradeField.cell.v) + 50));
await ev(`document.activeElement.blur()`);
await click(`document.getElementById("undo")`);
await sleep(300);
await ev(`(IT.search = "Зелье исцеления", refreshItems())`);
await sleep(400);
const potion = await ev(`({ title: document.querySelector("#it-card h1")?.textContent, sections: [...document.querySelectorAll("#it-card h2")].map((x) => x.textContent), craft: IT.data.craft?.map((c) => c.fields.map((f) => [f.path, f.cell?.v, f.item])), sell: IT.data.sell?.cell?.v ?? null })`);
console.log("   potion:", JSON.stringify(potion));
eq("a potion has a card without properties", potion.sections.includes("Базовые свойства"), false);
yes("a potion shows its recipe", potion.sections.includes("Крафт"));
await shot("64_item_potion");

// the tables are grouped in the left panel
const groups = await ev(`[...document.querySelectorAll("#nav-tables > div")].map((x) => x.textContent)`);
eq("tables are grouped", groups, ["Классы", "Предметы", "Экономика", "Опыт", "Лут", "Справка"]);
await ev(`showView("map_exp")`);
await sleep(400);
console.log("   map_exp:", await ev(`[document.querySelector(".name-cell")?.textContent, [...document.querySelectorAll(".head-cell")].map((x) => x.textContent)]`));
await ev(`showView("gear_insurance_item")`);
await sleep(400);
console.log("   insurance:", await ev(`[...document.querySelectorAll(".grid-row")].slice(0, 3).map((x) => x.innerText.replace(/\\n/g, " "))`));
await shot("65_insurance");
await ev(`showView("items")`);
await sleep(300);

// a weapon with a roll range and a price
await ev(`(IT.search = "Простой меч", refreshItems())`);
await sleep(400);
const card = await ev(`({ title: document.querySelector("#it-card h1")?.textContent, roll: document.querySelector("#it-card .card")?.innerText.slice(0, 260) })`);
console.log("   card:", JSON.stringify(card));
await shot("61_item_card");

// edit the maximum: the roll line changes with it, the value is marked, Enter moves on
const baseData = await ev(`IT.data.base.find((p) => p.min)`);
const maxKey = `${baseData.max.table}|${baseData.max.row}|${baseData.max.path}`;
const maxValue = Number(baseData.max.cell.v);
await click(`document.querySelector('#it-card input[data-key="${maxKey}"]')`);
await type(String(maxValue + 4));
await key("Enter");
await sleep(400);
const after = await ev(`IT.data.base.find((p) => p.min)`);
eq("max edited, marked as mine", [after.max.cell.v, after.max.cell.k], [String(maxValue + 4), String(maxValue)]);
eq("roll follows the edit", after.roll.values?.at(-1) ?? null, String(maxValue + 4));
eq("marked in the card", await ev(`document.querySelector('#it-card input[data-key="${maxKey}"]').dataset.mine`), "true");
yes("focus moved to the next value", await ev(`document.activeElement?.classList.contains("num") && document.activeElement.dataset.key !== ${JSON.stringify(maxKey)}`));
yes("item is marked in the list", await ev(`document.querySelector('#it-list [data-item="' + IT.id + '"] .bg-accent')`));

// min above max in the card
await click(`document.querySelector('#it-card input[data-key="${maxKey}"]')`);
await type("1");
await key("Enter");
await sleep(400);
eq("min above max is an error in the card", await ev(`document.querySelector('#it-card input[data-key="${maxKey}"]').dataset.issue`), "error");
await key("z", 2); // focus is in an input: Ctrl+Z belongs to the input, the header button undoes
await click(`document.getElementById("undo")`);
await sleep(400);
eq("undo in the card", await ev(`IT.data.base.find((p) => p.min).max.cell.v`), String(maxValue + 4));

// durability and price
const dur = await ev(`IT.data.durability`);
await click(`document.querySelector('#it-card input[data-key="${dur.table}|${dur.row}|${dur.path}"]')`);
await type("150");
await key("Enter");
await sleep(300);
const price = await ev(`IT.data.market[0].fields.find((f) => f.currency)`);
console.log("   price:", price.label, price.cell.v, price.currency);
await ev(`document.activeElement.blur()`);
await click(`document.querySelector('#it-card input[data-key="${price.table}|${price.row}|${price.path}"]')`);
await type(String(Number(price.cell.v) + 10));
await key("Enter");
await sleep(300);

// affixes: the rarity switch, the warning that it's shared, percent, search
const shared = await ev(`document.querySelector("#it-card .bar")?.textContent`);
console.log("   shared:", shared);
yes("warned that the group is shared", /общая/.test(shared));
eq("major values on screen", await ev(`document.querySelector('#it-card [data-side="major"]').dataset.active`), "true");
await ev(`document.querySelector('#it-card [data-side="minor"]').click()`);
await sleep(100);
eq("minor side switches", await ev(`document.querySelector('#it-card [data-side="minor"]').dataset.active`), "true");
await ev(`document.querySelector('#it-card [data-side="major"]').click()`);
await sleep(100);
const r0 = await ev(`[IT.data.affix.rarity, IT.data.affix.rows.length, IT.data.affix.rows[0].title, IT.data.affix.rows[0].major.min.cell.v, IT.data.affix.rows[0].major.roll.count]`);
console.log("   affix:", JSON.stringify(r0));
await ev(`(() => { const sel = [...document.querySelectorAll("#it-card select")].at(-1); sel.value = "LEGENDARY"; sel.dispatchEvent(new Event("change")); })()`);
await sleep(400);
const r1 = await ev(`[IT.data.affix.rarity, IT.data.affix.rows[0].major.min.cell.v, IT.data.affix.rows[0].major.max.cell.v]`);
console.log("   legendary:", JSON.stringify(r1));
yes("another rarity shows other values", r1[1] !== r0[3]);
const pct = await ev(`(() => { const r = IT.data.affix.rows.find((x) => x.unit === "доля"); return r ? { title: r.title, min: r.major.min, max: r.major.max } : null; })()`);
await click(`document.querySelector('#it-card input[data-key="${pct.max.table}|${pct.max.row}|${pct.max.path}"]')`);
const typedPct = String(Number((Number(pct.max.cell.v) * 100 + 1).toPrecision(6)));
await type(typedPct);
await key("Enter");
await sleep(400);
const stored = (await changes()).find((c) => c[2] === pct.max.path && c[1] === pct.max.row)[4];
eq("percent in the card: typed in %, stored as a share", Number(stored).toFixed(6), (Number(typedPct) / 100).toFixed(6));
await ev(`(() => { const s = document.querySelector('[data-key="affix-search"]'); s.value = "Сила"; s.dispatchEvent(new Event("input")); })()`);
await sleep(100);
console.log("   affix search:", await ev(`[...document.querySelectorAll("#it-card .card .truncate")].filter((x) => x.textContent === "Сила").length`));
await shot("62_item_affix");
await ev(`(() => { IT.affixSearch = ""; })()`);
await ev(`(IT.search = "", IT.rarity = "", refreshItems())`);
await sleep(300);

// ---------------------------------------------------------------------------------- class curves
await click(`document.getElementById("nav-curves")`);
await sleep(500);
await shot("70_curves");
const cv = await ev(`({ attr: document.getElementById("cv-attr").selectedOptions[0]?.textContent, attrs: CV.data.attrs.length, classes: CV.data.classes.map((c) => c.title), shown: [...CV.shown], lines: document.querySelectorAll("#cv-chart polyline").length, hits: document.querySelectorAll("#cv-chart .chart-hit").length, table: document.querySelectorAll("#cv-table tr").length, firstLevels: CV.data.classes[0].levels.length })`);
console.log("   curves:", JSON.stringify(cv));
eq("no service classes, no single-level classes", cv.classes.length, 8);
eq("first three shown", cv.shown.length, 3);
eq("one line per shown class", cv.lines, 3);
eq("20 levels of table rows and a header", cv.table, 21);

// a class keeps its colour when the others go away
const colorOf = `document.querySelector("#cv-chart polyline").style.stroke`;
const before = await ev(`[...document.querySelectorAll("#cv-chart polyline")].map((p) => p.style.stroke)`);
await ev(`document.querySelectorAll("#cv-legend button")[0].click()`);
await sleep(150);
const without = await ev(`[...document.querySelectorAll("#cv-chart polyline")].map((p) => p.style.stroke)`);
eq("colour follows the class", without, before.slice(1));
await ev(`document.querySelectorAll("#cv-legend button")[0].click()`);
await sleep(150);

// drag a point up, the line and the table follow, the original stays dashed
const pt = await ev(`(() => { const hit = document.querySelectorAll("#cv-chart .chart-hit")[9]; hit.scrollIntoView({ block: "center" }); const b = hit.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2, id: hit.dataset.id, row: Number(hit.dataset.row) }; })()`);
const was = await ev(`CV.data.classes.find((c) => c.id === ${JSON.stringify(pt.id)}).levels.find((l) => l.ref.row === ${pt.row}).ref.cell.v`);
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y });
await send("Input.dispatchMouseEvent", { type: "mousePressed", x: pt.x, y: pt.y, button: "left", clickCount: 1 });
for (let i = 1; i <= 8; i++) {
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: pt.x, y: pt.y - i * 6, button: "left", buttons: 1 });
  await sleep(20);
}
await shot("71_curves_dragging");
await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: pt.x, y: pt.y - 48, button: "left", clickCount: 1 });
await sleep(500);
const moved = await ev(`CV.data.classes.find((c) => c.id === ${JSON.stringify(pt.id)}).levels.find((l) => l.ref.row === ${pt.row}).ref.cell`);
console.log("   dragged:", was, "->", JSON.stringify(moved));
yes("drag changed the value", Number(moved.v) > Number(was));
eq("kit value is kept as the base", moved.k, was);
eq("dashed line appears", await ev(`[...document.querySelectorAll("#cv-chart polyline")].filter((p) => p.getAttribute("stroke-dasharray")).length`), 1);
yes("marked in the table", await ev(`[...document.querySelectorAll('#cv-table input[data-mine="true"]')].length === 1`));
yes("whole numbers stay whole", Number.isInteger(Number(moved.v)));

// the table under the chart edits the same value, and a hover reads every class
await click(`document.querySelector('#cv-table input.num:not(:disabled)')`);
const tableBefore = await ev(`document.activeElement.value`);
await type(String(Number(tableBefore) + 3));
await key("Enter");
await sleep(400);
eq("table edits", await ev(`document.querySelector('#cv-table input.num:not(:disabled)').value`), String(Number(tableBefore) + 3));
const over = await ev(`(() => { document.getElementById("cv-chart").scrollIntoView({ block: "start" }); const b = document.querySelectorAll("#cv-chart .chart-hit")[5].getBoundingClientRect(); return { x: b.x + 6, y: b.y + 20 }; })()`);
await sleep(100);
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: over.x, y: over.y });
await sleep(150);
const tip = await ev(`document.getElementById("tip").innerText`);
console.log("   crosshair:", JSON.stringify(tip));
yes("crosshair lists the classes", tip.split("\n").length >= 3);
await shot("72_curves_edited");
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 5 });

// another attribute
await ev(`(() => { const a = document.getElementById("cv-attr"); a.value = CV.data.attrs.find((x) => x.label === "Шанс крит. удара")?.path ?? CV.data.attrs.at(-1).path; a.dispatchEvent(new Event("change")); })()`);
await sleep(400);
console.log("   other attribute:", await ev(`[document.getElementById("cv-attr").selectedOptions[0].textContent, [...document.querySelectorAll("#cv-chart svg text")].slice(0, 3).map((x) => x.textContent), document.querySelector('#cv-table input.num')?.value]`));
await shot("73_curves_other");

// ---------------------------------------------------------------------------------- bulk edit
const n0 = (await status()).count;
await ev(`showView("market_product_item")`);
await sleep(500);
await ev(`setFilter({ kind: "ITEM_CONSUMABLE" })`);
await sleep(400);
const filtered = await ev(`document.getElementById("t-count").textContent`);
console.log("   consumables:", filtered);
await click(`document.getElementById("f-bulk")`);
await sleep(300);
const fields = await ev(`BK.fields.map((f) => [f.label, f.count])`);
console.log("   fields:", JSON.stringify(fields).slice(0, 200));
eq("apply is off until the form is filled", await ev(`document.getElementById("bk-apply").disabled`), true);
await ev(`(() => { for (const f of BK.fields) if (/цена/.test(f.label) && !/скидки/.test(f.label)) BK.checked.add(f.path); renderBulkFields(); })()`);
await click(`document.getElementById("bk-value")`);
await type("-20");
await sleep(500);
const preview = await ev(`({ summary: document.getElementById("bk-summary").textContent, rows: document.querySelectorAll("#bk-preview > div").length, first: document.querySelector("#bk-preview > div")?.innerText, apply: document.getElementById("bk-apply").disabled })`);
console.log("   preview:", JSON.stringify(preview));
eq("preview changes nothing", (await status()).count, n0);
eq("apply is on", preview.apply, false);
await shot("80_bulk_preview");
await click(`document.getElementById("bk-apply")`);
await sleep(500);
const n1 = (await status()).count;
yes("bulk applied", n1 > n0);
console.log("   status:", await ev(`document.getElementById("t-status").textContent`), "count", n1);
const sample = (await changes()).filter((c) => c[0] === "market_product_item" && c[2].endsWith("cost_price")).slice(1, 3);
console.log("   sample:", JSON.stringify(sample));
await click(`document.getElementById("undo")`);
await sleep(400);
eq("bulk undone in one step", (await status()).count, n0);
await click(`document.getElementById("redo")`);
await sleep(400);
eq("bulk redone", (await status()).count, n1);
await click(`document.getElementById("undo")`);
await sleep(300);
await ev(`setFilter({ kind: "" })`);
await sleep(300);

// selected rows only; an error blocks the bulk edit
await ev(`showView("gear_template")`);
await sleep(400);
await click(`G.rendered.get(1).querySelector(".name-cell")`);
await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 5, y: 5 });
await ev(`selectRow(3, { ctrlKey: true, shiftKey: false })`);
await sleep(100);
eq("two rows selected", await ev(`[G.selected.size, document.getElementById("t-status").textContent]`), [2, "Выделено строк: 2"]);
await shot("81_rows_selected");
await click(`document.getElementById("f-bulk")`);
await sleep(300);
console.log("   target:", await ev(`document.getElementById("bk-target").textContent`));
await ev(`(() => { for (const f of BK.fields) BK.checked.add(f.path); renderBulkFields(); })()`);
await ev(`setBulkOp("set")`);
await click(`document.getElementById("bk-value")`);
await type("1.5");
await sleep(500);
console.log("   whole-number field, 1.5:", await ev(`document.getElementById("bk-summary").textContent`), await ev(`document.querySelector("#bk-preview > div")?.innerText`));
await key("Escape");
await sleep(150);
eq("Esc closes the dialog", await ev(`document.getElementById("bulk").classList.contains("hidden")`), true);

// long view
await ev(`(G.selected = new Set(), showView("affix_template"))`);
await sleep(400);
await click(`document.getElementById("f-bulk")`);
await sleep(300);
await ev(`(() => { const f = BK.fields.find((x) => x.path.endsWith("attack_right_weapon_attack.offset.max_value")); BK.checked = new Set([f.path]); renderBulkFields(); })()`);
await ev(`setBulkOp("mul")`);
await click(`document.getElementById("bk-value")`);
await type("2");
await sleep(500);
console.log("   pairs on:", await ev(`document.getElementById("bk-summary").textContent`));
const pairsOn = await ev(`document.querySelector("#bk-preview")?.children.length`);
await ev(`document.getElementById("bk-pairs").click()`);
await sleep(500);
const pairsOff = await ev(`[document.getElementById("bk-summary").textContent, document.querySelector("#bk-preview")?.children.length]`);
console.log("   pairs off:", JSON.stringify(pairsOff));
yes("min goes with max when asked", pairsOn === pairsOff[1] * 2 || pairsOn > pairsOff[1]);
await ev(`document.getElementById("bk-pairs").click()`);
await key("Escape");

// the tables got better names
await ev(`showView("affix_value_template")`);
await sleep(400);
console.log("   heads:", await ev(`[...document.querySelectorAll(".head-cell")].slice(1, 4).map((x) => x.textContent)`));
await ev(`showView("market_product_item")`);
await sleep(400);
await ev(`setFilter({ search: "730001" })`);
await sleep(300);
console.log("   good without an item name:", await ev(`document.querySelector(".name-cell")?.textContent`));
await ev(`setFilter({ search: "" })`);

// English
await ev(`document.getElementById("lang").click()`);
await sleep(300);
await ev(`showView("curves")`);
await sleep(400);
await shot("90_en_curves");
await ev(`showView("items")`);
await sleep(500);
await shot("91_en_item");
console.log("   en:", await ev(`[...document.querySelectorAll("#it-card h2")].map((x) => x.textContent)`));
await ev(`document.getElementById("lang").click()`);
await sleep(300);

eq("no error bar left after all of this", await ev(`document.getElementById("error").classList.contains("hidden")`), true);
console.log("   error bar:", await ev(`document.getElementById("error-detail").textContent`));

// the patch still passes with everything in it
const final = await status();
console.log("final", JSON.stringify(final));
console.log("changes", JSON.stringify((await changes()).map((c) => c.slice(0, 2).concat(c.slice(3)))).slice(0, 700));
console.log("origins", [...origins]);
console.log("problems", problems.length ? problems : "none");
console.log("FAILED", failed);
ws.close();
