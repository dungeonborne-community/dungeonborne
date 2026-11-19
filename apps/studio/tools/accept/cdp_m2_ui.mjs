// node cdp_m2_ui.mjs <port> <kit.zip> <outdir>: real mouse and keyboard on the real exe.
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
const KEYS = { Enter: 13, Tab: 9, Escape: 27, Delete: 46, F2: 113, ArrowDown: 40, ArrowUp: 38, ArrowLeft: 37, ArrowRight: 39 };
async function key(k, mods = 0) {
  const special = KEYS[k];
  // '.' has the code of Delete in charCodeAt terms: real keyboards send 190 for it
  const vk = special ?? (k === "." ? 190 : k.toUpperCase().charCodeAt(0));
  const base = { key: k, modifiers: mods, windowsVirtualKeyCode: vk, code: special ? k : k === "." ? "Period" : `Key${k.toUpperCase()}` };
  const text = !special && !(mods & 2) ? k : undefined;
  await send("Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", ...base, ...(text ? { text } : {}) });
  await send("Input.dispatchKeyEvent", { type: "keyUp", ...base });
  await sleep(40);
}
const type = async (s) => { for (const c of s) await key(c); };
async function clickCell(index, col, count = 1) {
  const r = await ev(`(() => { const el = G.rendered.get(${index})?.querySelector('[data-col="${col}"]'); if (!el) return null; const b = el.getBoundingClientRect(); return { x: b.x + b.width / 2, y: b.y + b.height / 2 }; })()`);
  if (!r) throw new Error(`no cell ${index}:${col}`);
  for (let i = 1; i <= count; i++) {
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: i });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: i });
  }
  await sleep(60);
}
const cell = (index, col) => ev(`(() => { const el = G.rendered.get(${index})?.querySelector('[data-col="${col}"]'); return el ? { text: el.textContent, mine: el.dataset.mine ?? null, issue: el.dataset.issue ?? null, cur: el.dataset.cur ?? null } : null; })()`);
const status = () => ev(`({ ...S.edits, cursor: G.cursor, editing: !!G.editing, focus: document.activeElement?.id })`);
let failed = 0;
const eq = (name, got, want) => {
  const [a, b] = [JSON.stringify(got), JSON.stringify(want)];
  const ok = a === b;
  if (!ok) failed++;
  console.log(`${ok ? "ok " : "BAD"} ${name}${ok ? "" : `: got ${a}, want ${b}`}`);
};

for (const d of ["Runtime", "Log", "Network", "Page"]) await send(`${d}.enable`);
await ev(`openWith(() => invoke("open_kit", { path: ${JSON.stringify(kit)} }))`);
await ev(`showView("role_level_upgrade")`);
await sleep(500);
eq("nothing edited at start", (await status()).count, 0);

// type over a value, Enter moves down; row 1 = "Боец 400000 level 1" (Fighter), column 1 = Сила (Strength)
const before = await cell(1, 1);
await clickCell(1, 1);
eq("click selects", (await cell(1, 1)).cur, "true");
await type("35");
eq("typing opens the editor", (await status()).editing, true);
await key("Enter");
await sleep(150);
let c = await cell(1, 1);
eq("value replaced and marked", [c.text, c.mine], ["35", "true"]);
let st = await status();
eq("cursor moved down, focus back on the grid", [st.cursor, st.focus, st.count], [{ index: 2, col: 1 }, "grid", 1]);

// Esc cancels, Tab confirms and moves right
await type("99");
await key("Escape");
await sleep(80);
eq("Esc cancels", (await cell(2, 1)).mine, null);
await type("40");
await key("Tab");
await sleep(150);
st = await status();
eq("Tab confirms and moves right", [st.cursor, st.count], [{ index: 2, col: 2 }, 2]);

// arrows
await key("ArrowLeft");
await key("ArrowUp");
eq("arrows move", (await status()).cursor, { index: 1, col: 1 });

// undo, redo with the real keys
await key("z", 2);
await sleep(150);
eq("Ctrl+Z undoes the last edit", (await status()).count, 1);
await key("z", 2);
await sleep(150);
eq("Ctrl+Z again", [(await status()).count, (await cell(1, 1)).text], [0, before.text]);
await key("y", 2);
await sleep(150);
eq("Ctrl+Y redoes", [(await status()).count, (await cell(1, 1)).text], [1, "35"]);

// Delete returns the kit's value, F2 and double click edit the current text
await key("Delete");
await sleep(150);
eq("Delete returns the kit's value", [(await status()).count, (await cell(1, 1)).text], [0, before.text]);
await key("F2");
eq("F2 keeps the text", await ev(`document.getElementById("cell-input").value`), before.text);
await key("Escape");
await clickCell(3, 1, 2);
eq("double click opens the editor", (await status()).editing, true);
await key("Escape");

// locked column: selectable, not editable
await clickCell(3, 0);
await type("7");
eq("locked cell stays closed", [(await status()).editing, await ev(`document.getElementById("t-status").textContent`)], [false, "Эту ячейку менять нельзя"]);

// an error is marked at once and blocks saving, fixing it clears it
await clickCell(3, 1);
await type("abc");
await key("Enter");
await sleep(150);
c = await cell(3, 1);
eq("text in a number field is an error", [c.text, c.issue, (await status()).errors], ["abc", "error", 1]);
await shot("40_error_cell");
await ev(`showView("gear_template")`);
await sleep(400);
await clickCell(0, 0);
await type("10.5");
await key("Enter");
await sleep(150);
eq("fraction in a whole-number field is an error", (await cell(0, 0)).issue, "error");
await key("z", 2);
await sleep(150);
await ev(`showView("role_level_upgrade")`);
await sleep(400);
await clickCell(3, 1);
await type("41");
await key("Enter");
await sleep(150);
c = await cell(3, 1);
eq("fixing clears the error", [c.issue, (await status()).errors], [null, 0]);

// percent: typed as 9, stored as 0.09
await ev(`showView("affix_value_template")`);
await sleep(500);
const pctRow = await ev(`(async () => { for (let i = 0; i < 20; i++) { const r = rowAt(i); if (r?.unit === "доля") return i; } return -1; })()`);
const was = (await cell(pctRow, 0)).text;
await clickCell(pctRow, 1);
await type("9");
await key("Enter");
await sleep(200);
const changes = await ev(`invoke("list_changes").then((d) => d.changes.filter((x) => x.table === "affix_value_template").map((x) => [x.path, x.old, x.new]))`);
eq("9 is stored as 0.09", changes.map((x) => x[2]), ["0.09"]);
console.log("   percent cell", was, "->", (await cell(pctRow, 1)).text, changes);
await ev(`(() => { const c = cursorData({ index: ${pctRow}, col: 0 }); return c.field.path; })()`);

// min above max in the same row
const minCol = 0; // data.affix_data[0].rarity_range[1].min_value
await clickCell(pctRow, minCol);
await type("500");
await key("Enter");
await sleep(200);
eq("min above max is an error", (await cell(pctRow, minCol)).issue, "error");
eq("both cells of the pair are marked", (await cell(pctRow, 1)).issue, "error");
await key("z", 2);
await sleep(200);
eq("undone", (await cell(pctRow, minCol)).issue, null);

// paste a block from Excel with a locked column in it
await ev(`showView("role_level_upgrade")`);
await sleep(400);
await clickCell(5, 0);
await ev(`(() => { const d = new DataTransfer(); d.setData("text/plain", "1\\t31\\t32\\r\\n2\\t33\\t34\\r\\n"); document.dispatchEvent(new ClipboardEvent("paste", { clipboardData: d, bubbles: true, cancelable: true })); })()`);
await sleep(400);
eq("paste: locked column skipped", await ev(`document.getElementById("t-status").textContent`), "Вставлено: 4, пропущено: 2");
eq("pasted values", [(await cell(5, 1)).text, (await cell(5, 2)).text, (await cell(6, 1)).text, (await cell(6, 2)).text], ["31", "32", "33", "34"]);
await key("z", 2);
await sleep(200);
eq("paste undone in one step", (await cell(6, 2)).mine, null);
await key("y", 2);
await sleep(200);
await shot("41_edited_table");

// the filter of my changes
await ev(`setFilter({ only_mine: true })`);
await sleep(300);
console.log("   only mine:", await ev(`document.getElementById("t-count").textContent`));
await shot("42_only_mine");
await ev(`setFilter({ only_mine: false })`);

// long view
await ev(`showView("affix_template")`);
await sleep(400);
eq("items open as a list", await ev(`G.long`), true);
await clickCell(0, 0);
const oldLong = (await cell(0, 0)).text;
await type("777");
await key("Enter");
await sleep(200);
c = await cell(0, 0);
console.log("   long edit", oldLong, "->", c.text, c.mine, c.issue, "next", (await status()).cursor);
eq("long view edits", [c.text, c.mine], ["777", "true"]);

// tooltip of an edited cell
await ev(`showTip(G.rendered.get(0).querySelector('[data-col="0"]'), 700, 300)`);
await sleep(80);
console.log("   tip:", JSON.stringify(await ev(`document.getElementById("tip").innerText`)));
await ev(`hideTip()`);

// my changes
await ev(`document.getElementById("nav-changes").click()`);
await sleep(400);
console.log("   summary:", await ev(`document.getElementById("ch-summary").textContent`));
console.log("   hint:", await ev(`[document.getElementById("ch-hint").textContent, document.getElementById("ch-save").disabled]`));
await shot("43_changes");
eq("groups by table", await ev(`[...document.querySelectorAll("#ch-list h2")].length`), 3);
await ev(`(() => { const a = document.getElementById("ch-author"); a.value = "m2 тест"; a.dispatchEvent(new Event("input")); const c = document.getElementById("ch-comment"); c.value = "проверка"; c.dispatchEvent(new Event("input")); })()`);
await sleep(500);
eq("errors still block saving", await ev(`document.getElementById("ch-hint").textContent`), "Сначала исправьте ошибки.");
const n0 = (await status()).count;
await ev(`[...document.querySelectorAll("#ch-list [data-ref=issue]")].find((x) => x.dataset.severity === "error").closest("div.flex").querySelector("[data-ref=revert]").click()`);
await sleep(400);
eq("revert removes one", (await status()).count, n0 - 1);
eq("save allowed with a nickname and no errors", await ev(`document.getElementById("ch-save").disabled`), false);
console.log("   row label:", await ev(`document.querySelector("#ch-list [data-ref=label]").textContent`));

// speed of an edit on the largest table, with a large patch already in place
const speed = await ev(`(async () => {
  const q = { view: "long", search: "", only_diff: false, only_mine: false, all_fields: false, kind: "", rarity: "", sort: "", desc: false, lang: "ru" };
  const rows = [];
  for (let o = 0; o < 3000; o += 500) rows.push(...(await invoke("query_rows", { table: "affix_value_template", query: q, offset: o, limit: 500 })).rows);
  const edits = rows.map((r) => ({ table: "affix_value_template", row: r.row, path: r.field.path, new: String(Number(r.cell.v) * 1.1) }));
  let a = performance.now();
  await invoke("set_values", { edits });
  const bulk = performance.now() - a;
  a = performance.now();
  await invoke("set_values", { edits: [{ ...edits[0], new: String(Number(edits[0].new) + 1) }] });
  const single = performance.now() - a;
  a = performance.now();
  const list = await invoke("list_changes");
  const listed = performance.now() - a;
  a = performance.now();
  renderChanges();
  await new Promise((r) => setTimeout(r, 50));
  const shown = performance.now() - a;
  const st = await invoke("undo");
  await invoke("undo");
  return { edits: edits.length, bulk: Math.round(bulk), single: Math.round(single), listed: Math.round(listed), count: list.count, errors: list.errors, warnings: list.warnings, shown: Math.round(shown), after: st.count };
})()`);
console.log("   speed", JSON.stringify(speed));
await sleep(600);

await ev(`refreshState()`);
console.log("final", JSON.stringify(await ev(`invoke("list_changes").then(({ changes, ...st }) => st)`)));
console.log("origins", [...origins]);
console.log("problems", problems.length ? problems : "none");
console.log("FAILED", failed);
ws.close();
