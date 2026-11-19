// Moves a draft between two kit versions in the real
// exe, checks the small window and English on every screen. Args: <port> <old.zip> <new.zip> <bad_patch.json> <good_patch path> <outdir>
import { writeFileSync } from "node:fs";
const [port, oldZip, newZip, badPatch, goodPatch, out] = process.argv.slice(2);
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
let failed = 0;
const eq = (name, got, want) => {
  const [a, b] = [JSON.stringify(got), JSON.stringify(want)];
  const ok = a === b;
  if (!ok) failed++;
  console.log(`${ok ? "ok " : "BAD"} ${name}${ok ? "" : `: got ${a}, want ${b}`}`);
};
const j = (s) => JSON.stringify(s);
for (const d of ["Runtime", "Log", "Network", "Page"]) await send(`${d}.enable`);

// ------------------------------------------------------------ the old kit with four kinds of changes
await ev(`openWith(() => invoke("open_kit", { path: ${j(oldZip)} }))`);
eq("no offer on the first kit", await ev(`[S.kit.rebase_from ?? null, document.getElementById("rebase").classList.contains("hidden")]`), [null, true]);
const STR = "role_attribute.first_level_strength.fixed_value";
const rows = await ev(`(async () => { const q = { view: "wide", search: "", only_diff: false, only_mine: false, all_fields: true, kind: "", rarity: "", sort: "", desc: false, lang: "ru" }; const r = await invoke("query_rows", { table: "gear_template", query: q, offset: 0, limit: 100 }); const col = r.columns.findIndex((c) => c.path === "durability"); return r.rows.filter((x) => x.cells[col]).slice(0, 6).map((x) => [x.row, x.cells[col].v]); })()`);
console.log("   gear rows:", j(rows));
const edit = (table, row, path, value) => ({ table, row, path, new: String(value) });
const edits = [
  edit("gear_template", rows[0][0], "durability", 150), // moves by itself
  edit("gear_template", rows[1][0], "durability", 151), // the server gets the same value: already there
  edit("gear_template", rows[2][0], "durability", 152), // the server gets another value: conflict
  edit("gear_template", rows[3][0], "durability", 153), // conflict too, keep the server's
  edit("role_level_upgrade", 5, STR, 99),               // this row disappears: lost (see make_m4_kit.py)
];
const st0 = await ev(`invoke("set_values", { edits: ${j(edits)} })`);
eq("draft in the old kit", [st0.count, st0.rejected.length], [5, 0]);
await ev(`invoke("set_note", { author: "m4", comment: "перенос" })`);

// ------------------------------------------------------------ a newer kit offers the move
await ev(`invoke("close_kit").then(() => { S.kit = null; })`);
await ev(`openWith(() => invoke("open_kit", { path: ${j(newZip)} }))`);
await sleep(300);
const offered = await ev(`[S.kit.version, S.kit.rebase_from ?? null, document.getElementById("rebase").classList.contains("hidden"), S.edits.count]`);
console.log("   offered:", j(offered));
eq("offered on opening", offered[2], false);
eq("nothing moved yet", offered[3], 0);
const plan = await ev(`RB.plan`);
console.log("   plan:", j({ auto: plan.auto.length, already: plan.already.length, conflicts: plan.conflicts.map((c) => [c.old, c.server, c.new, c.partial]), lost: plan.lost.map((c) => [c.title, c.reason]) }));
eq("sorted into the four outcomes", [plan.auto.length, plan.already.length, plan.conflicts.length, plan.lost.length], [1, 1, 2, 1]);
await shot("100_rebase");
eq("can't move until every conflict is decided", await ev(`[document.getElementById("rb-apply").disabled, document.getElementById("rb-hint").textContent]`), [true, "Выберите вариант в каждом конфликте."]);

// "not now" keeps everything, opening again offers again
await ev(`document.getElementById("rb-later").click()`);
await sleep(100);
eq("not now closes the dialog", await ev(`document.getElementById("rebase").classList.contains("hidden")`), true);
eq("still nothing moved", (await ev(`invoke("list_changes")`)).count, 0);
await ev(`openRebase()`);
await sleep(200);

const [c1, c2] = plan.conflicts;
await ev(`document.querySelector('input[name="rb-${c1.id}"][value="mine"]').click()`);
await ev(`document.querySelector('input[name="rb-${c2.id}"][value="server"]').click()`);
await sleep(50);
eq("hint speaks of the lost ones", await ev(`[document.getElementById("rb-apply").disabled, document.getElementById("rb-hint").textContent]`), [false, "Эти правки останутся в старом черновике."]);
await ev(`document.getElementById("rb-apply").click()`);
await sleep(500);
const after = await ev(`invoke("list_changes").then((d) => ({ count: d.count, comment: d.comment, list: d.changes.map((c) => [c.row, c.old, c.new]) }))`);
console.log("   moved:", j(after));
eq("one automatic and one kept by choice", after.count, 2);
eq("comment moved too", after.comment, "перенос");
console.log("   notice:", await ev(`document.getElementById("ch-notice-text").textContent`));
eq("report on my changes", await ev(`S.view`), "changes");
await shot("101_rebased");
const undoCount = (await ev(`invoke("undo")`)).count;
eq("the move is one undo step", undoCount, 0);
await ev(`invoke("redo")`);

// the new kit has its own draft, the old one holds only the lost change and doesn't ask again
await ev(`invoke("close_kit").then(() => { S.kit = null; })`);
await ev(`openWith(() => invoke("open_kit", { path: ${j(newZip)} }))`);
await sleep(300);
eq("no second offer", await ev(`[S.kit.edits.count, S.kit.rebase_from ?? null, document.getElementById("rebase").classList.contains("hidden")]`), [2, null, true]);

// the old kit opens with just what was lost
await ev(`invoke("close_kit").then(() => { S.kit = null; })`);
await ev(`openWith(() => invoke("open_kit", { path: ${j(oldZip)} }))`);
await sleep(300);
const oldNow = await ev(`invoke("list_changes").then((d) => d.changes.map((c) => [c.table, c.new]))`);
eq("the old draft keeps the lost change", oldNow, [["role_level_upgrade", "99"]]);
eq("an older kit is not offered the newer draft", await ev(`S.kit.rebase_from ?? null`), null);

// ------------------------------------------------------------ the patch from a moved draft
await ev(`invoke("close_kit").then(() => { S.kit = null; })`);
await ev(`openWith(() => invoke("open_kit", { path: ${j(newZip)} }))`);
await sleep(200);
console.log("   export:", j(await ev(`invoke("export_patch", { path: ${j(goodPatch)}, author: "m4", comment: "перенос", created: "2026-11-06 12:00" })`)));

// ------------------------------------------------------------ a patch from an LLM with errors, HTML in its texts
await ev(`showView("changes")`);
await sleep(300);
await ev(`openPatch(${j(badPatch)})`);
await sleep(400);
const imported = await ev(`({ notice: document.getElementById("ch-notice-text").textContent, summary: document.getElementById("ch-summary").textContent, injected: document.querySelectorAll("#ch-list img[src=x], #ch-notice img, #ch-list b").length, comment: document.getElementById("ch-comment").value })`);
console.log("   llm patch:", j(imported));
eq("HTML from a patch stays text", imported.injected, 0);
await shot("102_llm_patch");

// ------------------------------------------------------------ every screen at the minimum window size, both languages
await send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 680, deviceScaleFactor: 1, mobile: false });
await sleep(200);
const VIEWS = ["summary", "changes", "items", "curves", "role_level_upgrade", "affix_template", "affix_value_template", "market_product_item"];
const overflow = `(() => { const d = document.documentElement; const out = []; if (d.scrollWidth > d.clientWidth) out.push("page x " + d.scrollWidth); if (d.scrollHeight > d.clientHeight) out.push("page y " + d.scrollHeight);
  for (const el of document.querySelectorAll("main button, main select, main input, main h1, main h2")) { if (!el.offsetParent) continue; const r = el.getBoundingClientRect(); if (r.right > innerWidth + 1) out.push("right of the window: " + (el.id || el.textContent).slice(0, 30)); }
  return out; })()`;
for (const lang of ["ru", "en"]) {
  if ((await ev(`S.lang`)) !== lang) {
    await ev(`document.getElementById("lang").click()`);
    await sleep(400);
  }
  for (const v of VIEWS) {
    await ev(`showView(${j(v)})`);
    await sleep(350);
    const o = await ev(overflow);
    eq(`${lang} ${v}: fits 1100×680`, o, []);
    await shot(`11_${lang}_${v}`);
  }
}
const hasRu = /[А-Яа-яЁё]/;
const texts = await ev(`(async () => { showView("summary"); await new Promise((r) => setTimeout(r, 300)); return { nav: [...document.querySelectorAll("nav button")].map((x) => x.textContent), notes: S.kit.notes, title: S.kit.title, about: S.kit.tables.map((x) => x.about + " " + x.where) }; })()`);
const ruLeft = [...texts.nav, ...texts.notes, texts.title, ...texts.about].filter((x) => hasRu.test(x));
eq("no Russian left in the kit's texts in English", ruLeft, []);
await shot("110_en_summary");
// the bulk and move dialogs in the small window
await ev(`showView("market_product_item")`);
await sleep(300);
await ev(`openBulk()`);
await sleep(300);
const box = await ev(`(() => { const b = document.getElementById("bk-apply").getBoundingClientRect(); return [b.bottom <= innerHeight, b.right <= innerWidth]; })()`);
eq("bulk dialog fits", box, [true, true]);
await shot("111_en_bulk_small");
await ev(`closeBulk()`);
await ev(`document.getElementById("lang").click()`);
await sleep(300);
await send("Emulation.clearDeviceMetricsOverride");

console.log("origins", [...origins]);
console.log("problems", problems.length ? problems : "none");
console.log("error bar:", await ev(`document.getElementById("error").classList.contains("hidden") ? "none" : document.getElementById("error-detail").textContent`));
console.log("FAILED", failed);
ws.close();
