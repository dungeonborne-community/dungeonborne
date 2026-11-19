// node cdp_s2_loot.mjs <port> <kit.zip> <outdir>: the Loot screen on the real exe, real mouse and keyboard.
// Leaves <outdir>/drop.json (the average drop of every map, for the comparison with `loot show`) and, for the base kit, <outdir>/patch.json.
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
  await send("Input.dispatchMouseEvent", { type: "mouseMoved", x: r.x, y: r.y });
  for (let i = 1; i <= count; i++) {
    await send("Input.dispatchMouseEvent", { type: "mousePressed", x: r.x, y: r.y, button: "left", clickCount: i });
    await send("Input.dispatchMouseEvent", { type: "mouseReleased", x: r.x, y: r.y, button: "left", clickCount: i });
  }
  await sleep(100);
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
const pickGroup = async (key) => { await click(`document.querySelector('#lt-list [data-group="${key}"]')`); await sleep(400); };
const setMap = async (m) => { await ev(`(LT.map = ${JSON.stringify(m)}, LT.kind = "", LT.search = "", document.getElementById("lt-search").value = "", refreshLoot())`); await sleep(300); };
const card = () => ev(`LT.data`);
// A big group starts with closed slots: opens the slot that holds this input, then clicks it
const clickInput = async (slotN, key) => {
  const sel = `document.querySelector('#lt-card input[data-key="${key}"]')`;
  if (!(await ev(`!!${sel}`))) {
    await click(`document.querySelector('#lt-card button[data-slot="${slotN}"]')`);
    await sleep(300);
  }
  await click(sel);
};
const noWideCard = () => ev(`(() => { const c = document.getElementById("lt-card"); return c.scrollWidth <= c.clientWidth + 1; })()`);

for (const d of ["Runtime", "Log", "Network", "Page"]) await send(`${d}.enable`);
await ev(`openWith(() => invoke("open_kit", { path: ${JSON.stringify(kit)} }))`);
await sleep(300);
const season = await ev(`S.kit.season`);
console.log("   kit", await ev(`S.kit.version`), "season", season);
eq("starts clean", (await status()).count, 0);
eq("the screen is offered", await ev(`document.getElementById("nav-loot").classList.contains("hidden")`), false);
yes("loot tables are in their group", (await ev(`[...document.querySelectorAll("#nav-tables > div")].map((x) => x.textContent)`)).includes("Лут"));

// ---------------------------------------------------------------------------------- the drop of every map, for `loot show`
const drop = await ev(`(async () => {
  const first = await invoke("loot_groups", { query: { map: null, kind: "", search: "", only_mine: false, lang: "ru" } });
  const out = { maps: first.maps.map((m) => m.id), all: {} };
  for (const m of first.maps) {
    const d = await invoke("loot_groups", { query: { map: m.id, kind: "", search: "", only_mine: false, lang: "ru" } });
    out.all[m.id] = d.groups.map((g) => [g.key, g.items, g.gold, g.sources]);
  }
  return out;
})()`);
writeFileSync(`${out}/drop.json`, JSON.stringify(drop));
console.log("   maps", drop.maps.join(","), "groups", Object.values(drop.all).map((x) => x.length).join(","));

// ---------------------------------------------------------------------------------- the list
const started = Date.now();
await click(`document.getElementById("nav-loot")`);
await sleep(600);
console.log("   opened in", Date.now() - started, "ms (with a 600 ms wait)");
eq("a real map is chosen first", await ev(`LT.map`), "10101");
console.log("   list:", await ev(`document.getElementById("lt-count").textContent`), JSON.stringify(await ev(`[...document.querySelectorAll("#lt-list > button")].slice(0, 3).map((b) => b.innerText)`)));
await shot("s2_10_first");

const crypt = await ev(`[...document.querySelectorAll("#lt-list [data-group]")].map((b) => b.dataset.group)`);
await click(`document.getElementById("lt-map")`);
await key("Escape");
await setMap("10102");
const classic = await ev(`[...document.querySelectorAll("#lt-list [data-group]")].map((b) => b.dataset.group)`);
yes("another map has other groups", JSON.stringify(crypt) !== JSON.stringify(classic));
await setMap("10101");
await ev(`(LT.kind = "monster", refreshLoot())`);
await sleep(300);
const kinds = await ev(`[...document.querySelectorAll("#lt-kind option")].map((o) => o.textContent)`);
console.log("   kinds:", JSON.stringify(kinds));
yes("the source kinds are named", kinds.length >= 3);
const monsters = await ev(`[...document.querySelectorAll("#lt-list [data-group]")].length`);
await setMap("10101");
yes("the kind filters the list", monsters > 0 && monsters < crypt.length);
await click(`document.getElementById("lt-search")`);
await type("barrel");
await sleep(400);
const found = await ev(`[...document.querySelectorAll("#lt-list [data-group]")].map((b) => b.dataset.group)`);
yes("search finds a source by name", found.length >= 1 && found.length < crypt.length);
await ev(`(LT.search = "", document.getElementById("lt-search").value = "", refreshLoot())`);
await sleep(300);

// ---------------------------------------------------------------------------------- a group: chance
const barrel = found[0];
await pickGroup(barrel);
let g = await card();
console.log("   group:", g.key, "|", g.title, "| slots", g.slots.length, "| items", g.summary.items, "gold", g.summary.gold);
await shot("s2_11_group");
eq("nothing is marked yet", await ev(`document.querySelectorAll("#lt-list .bg-accent").length`), 0);
yes("the card stays in its width", await noWideCard());
const slotIx = g.slots.findIndex((s) => s.rate !== null);
const slot = g.slots[slotIx];
const was = g.summary.items;
const oldChance = Number(slot.chance.cell.v);
const newChance = oldChance >= 50 ? oldChance / 2 : oldChance * 2;
const chanceKey = `${slot.chance.table}|${slot.chance.row}|${slot.chance.path}`;
await click(`document.querySelector('#lt-card input[data-key="${chanceKey}"]')`);
await type(String(newChance));
await key("Enter");
await sleep(500);
g = await card();
eq("the chance is edited", g.slots[slotIx].chance.cell.v, String(newChance));
eq("the kit's value stays beside it", g.slots[slotIx].chance.cell.k, String(slot.chance.cell.v));
yes("the average drop moves", Math.abs(g.summary.items - was) > 1e-6);
eq("and the old one is shown", g.summary_was.items, was);
yes("the slot says what it was", await ev(`document.getElementById("lt-card").innerText.includes("было")`));
eq("the group is marked in the list", await ev(`document.querySelector('#lt-list [data-group="${barrel}"] .bg-accent') !== null`), true);
await shot("s2_12_edited");
eq("one change", (await changes()).map((c) => [c[0], c[2], c[4]]), [["lt_slots", "chance", String(newChance)]]);

await click(`document.getElementById("undo")`);
await sleep(500);
g = await card();
eq("undo returns the chance", g.slots[slotIx].chance.cell.v, slot.chance.cell.v);
eq("and the drop", g.summary.items, was);
await click(`document.getElementById("redo")`);
await sleep(500);
eq("redo too", (await card()).slots[slotIx].chance.cell.v, String(newChance));

// ---------------------------------------------------------------------------------- a group: weight, count, a shared pool
g = await card();
const pool = g.slots.flatMap((s) => s.pools).find((p) => p.items.filter((i) => i.share > 0).length > 1);
if (pool) {
  const item = pool.items.find((i) => i.share > 0);
  const wKey = `${item.w.table}|${item.w.row}|${item.w.path}`;
  await clickInput(g.slots.find((s) => s.pools.includes(pool)).n, wKey);
  await type(String(Number(item.w.cell.v) * 2));
  await key("Enter");
  await sleep(500);
  const again = (await card()).slots.flatMap((s) => s.pools).find((p) => p.key === pool.key).items.find((i) => i.row === item.row);
  yes("a heavier item takes a bigger share", again.share > item.share);
  console.log(`   item ${item.title}: share ${item.share} -> ${again.share}, shared with ${pool.shared} groups`);
  if (pool.shared > 0) yes("a shared pool warns", await ev(`document.getElementById("lt-card").innerText.includes("Этот пул используют")`));
  await shot("s2_13_weight");
}

// ---------------------------------------------------------------------------------- wrong values
await click(`document.querySelector('#lt-card input[data-key="${chanceKey}"]')`);
await type("150");
await key("Enter");
await sleep(500);
g = await card();
eq("a chance above 100 is marked", g.slots[slotIx].chance.cell.e, "new_out_of_limits");
yes("and explained", await ev(`document.getElementById("lt-card").innerText.includes("вне допустимых границ")`));
eq("the error is counted", (await status()).errors, 1);
await shot("s2_14_error");
await ev(`invoke("revert", { cells: [{ table: ${JSON.stringify(slot.chance.table)}, row: ${slot.chance.row}, path: "chance" }] }).then(updateStatus).then(refreshScreen)`);
await sleep(400);
eq("reverted", (await card()).slots[slotIx].chance.cell.v, slot.chance.cell.v);

// A pool with one item cannot lose it. Gold is such a pool.
const single = g.slots.flatMap((s) => s.pools).find((p) => p.items.length === 1);
if (single) {
  const it = single.items[0];
  const wKey = `${it.w.table}|${it.w.row}|${it.w.path}`;
  await clickInput(g.slots.find((s) => s.pools.includes(single)).n, wKey);
  await type("0");
  await key("Enter");
  await sleep(500);
  const now = (await card()).slots.flatMap((s) => s.pools).find((p) => p.key === single.key).items[0];
  eq("an empty pool is refused", now.w.cell.e, "key_all_zero");
  await ev(`invoke("revert", { cells: [{ table: ${JSON.stringify(it.w.table)}, row: ${it.w.row}, path: "w" }] }).then(updateStatus).then(refreshScreen)`);
  await sleep(400);
}

// ---------------------------------------------------------------------------------- a big group
await ev(`(LT.map = "", LT.search = "ring", document.getElementById("lt-search").value = "ring", refreshLoot())`);
await sleep(500);
const ring = await ev(`document.querySelector("#lt-list [data-group]")?.dataset.group`);
if (ring) {
  await pickGroup(ring);
  const big = await card();
  const total = big.slots.reduce((n, s) => n + s.pools.reduce((m, p) => m + p.items.length, 0), 0);
  console.log(`   big group ${ring}: slots ${big.slots.length}, items ${total}, inputs on screen ${await ev(`document.querySelectorAll("#lt-card input.num").length`)}`);
  if (total > 150) {
    yes("a big group starts closed", (await ev(`document.querySelectorAll("#lt-card input.num").length`)) <= big.slots.length + 1);
    await click(`document.querySelector('#lt-card button[data-slot="0"]')`);
    await sleep(300);
    yes("a slot opens", (await ev(`document.querySelectorAll("#lt-card input.num").length`)) > big.slots.length);
    await shot("s2_15_big");
    const t0 = Date.now();
    await ev(`invoke("loot_group", { key: ${JSON.stringify(ring)} })`);
    console.log("   the big group is read in", Date.now() - t0, "ms");
  }
}
await setMap("10101");

// ---------------------------------------------------------------------------------- rules and a mode
if (season) {
  const list = await ev(`[...document.querySelectorAll("#lt-list [data-group]")].map((b) => b.dataset.group)`);
  const moded = list.find((k) => k.includes("@"));
  yes("a map with a mode shows its groups", moded);
  await pickGroup(moded);
  g = await card();
  console.log("   mode group:", g.key, JSON.stringify(g.mode), JSON.stringify(g.rules).slice(0, 300));
  yes("the card says which mode", g.mode?.title);
  yes("the rules are written out", await ev(`document.getElementById("lt-card").innerText.includes("Поверх шаблона действуют правила")`));
  const gone = g.slots.flatMap((s) => s.pools.flatMap((p) => p.items)).filter((i) => i.chance === null);
  yes("items the rules removed are marked", gone.length > 0 || g.slots.some((s) => s.rate === null));
  await shot("s2_16_mode");
  await click(`[...document.querySelectorAll("#lt-card button")].find((b) => b.textContent === "Открыть правила")`);
  await sleep(500);
  eq("the rules table opens", await ev(`S.view`), "lt_rules");
  await shot("s2_17_rules_table");
  await ev(`showView("loot")`);
  await sleep(400);
}

// ---------------------------------------------------------------------------------- tables, a small window, English
for (const table of ["lt_slots", "lt_items", "lt_rules"]) {
  await ev(`showView(${JSON.stringify(table)})`);
  await sleep(600);
  console.log(`   ${table}:`, JSON.stringify(await ev(`[document.querySelector(".name-cell")?.textContent, [...document.querySelectorAll(".head-cell")].slice(0, 6).map((x) => x.textContent)]`)));
  await shot(`s2_18_${table}`);
}
await ev(`showView("loot")`);
await sleep(400);

await send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 700, deviceScaleFactor: 1, mobile: false });
await sleep(400);
await ev(`LT.key && loadLootCard()`);
await sleep(300);
yes("the card fits a small window", await noWideCard());
await shot("s2_19_small");
await send("Emulation.clearDeviceMetricsOverride");

await click(`document.getElementById("lang")`);
await sleep(800);
const en = await ev(`({ nav: document.getElementById("nav-loot").textContent, title: document.querySelector("#lt-card h1")?.textContent, heads: [...document.querySelectorAll("#lt-card h2")].map((x) => x.textContent), map: document.getElementById("lt-map").selectedOptions[0]?.textContent })`);
console.log("   English:", JSON.stringify(en));
yes("English navigation", en.nav.includes("Loot"));
yes("English map name", en.map && !/[а-яё]/i.test(en.map));
yes("no Russian on the card", !/[а-яё]/i.test(await ev(`[...document.querySelectorAll("#lt-card h1, #lt-card h2")].map((x) => x.textContent).join(" ")`)));
await shot("s2_20_english");
await click(`document.getElementById("lang")`);
await sleep(600);

// ---------------------------------------------------------------------------------- the patch
const final = await status();
console.log("   final", JSON.stringify(final));
const path = `${out}/patch.json`.replace(/\//g, "\\");
await ev(`invoke("export_patch", { path: ${JSON.stringify(path)}, author: "s2", comment: "loot screen", created: "2026-11-06" })`);
console.log("   changes:", JSON.stringify(await changes()).slice(0, 500));
eq("no error bar left", await ev(`document.getElementById("error").classList.contains("hidden")`), true);
console.log("origins", [...origins]);
console.log("problems", problems.length ? problems : "none");
console.log("FAILED", failed);
ws.close();
