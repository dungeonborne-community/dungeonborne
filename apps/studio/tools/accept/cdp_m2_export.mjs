// node cdp_m2_export.mjs <port> <kit.zip> <edits.json> <patch out> : same edits as the book, saved by the real exe.
import { readFileSync } from "node:fs";
const [port, kit, editsFile, out] = process.argv.slice(2);
const page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (m) => { const x = JSON.parse(m.data); if (x.id && pending.has(x.id)) { pending.get(x.id)(x); pending.delete(x.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const ev = async (expression) => {
  const r = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails.exception?.description ?? r.result.exceptionDetails.text));
  return r.result.result.value;
};
const edits = JSON.parse(readFileSync(editsFile, "utf-8")).map((e) => ({ table: e.table, row: e.row, path: e.path, new: e.new }));
console.log("open", await ev(`invoke("open_kit", { path: ${JSON.stringify(kit)} }).then((k) => [k.version, k.season, k.edits.count])`));
const st = await ev(`invoke("set_values", { edits: ${JSON.stringify(edits)} })`);
console.log("status", JSON.stringify({ count: st.count, errors: st.errors, warnings: st.warnings, rejected: st.rejected }));
if (st.errors) console.log("issues", JSON.stringify(await ev(`invoke("list_changes").then((d) => d.changes.filter((c) => c.issue).map((c) => [c.table, c.row, c.path, c.new, c.issue.code]))`)));
console.log("export", JSON.stringify(await ev(`invoke("export_patch", { path: ${JSON.stringify(out)}, author: "m2", comment: "acceptance", created: "2026-11-06 12:00" })`)));
ws.close();
