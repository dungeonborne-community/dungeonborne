// node px.mjs <strip.html> x0 y0 x1 y1 — average colour of the brightest pixels of a region in each frame of a strip
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [file, x0, y0, x1, y1] = process.argv.slice(2);
const port = 9342;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe", ["--headless=new", `--remote-debugging-port=${port}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "cdp-"))}`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {} await sleep(200); }
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0; const p = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await send("Page.enable"); await send("Page.navigate", { url: "file:///" + file.split(String.fromCharCode(92)).join("/") }); await sleep(1500);
const r = await send("Runtime.evaluate", { returnByValue: true, awaitPromise: true, expression: `(async () => {
  const out = [];
  for (const wrap of document.body.children) {
    const label = wrap.firstElementChild.textContent; const img = wrap.querySelector('img');
    await img.decode();
    const c = document.createElement('canvas'); c.width = img.naturalWidth; c.height = img.naturalHeight;
    const g = c.getContext('2d'); g.drawImage(img, 0, 0);
    const d = g.getImageData(${x0}, ${y0}, ${x1 - x0}, ${y1 - y0}).data;
    const px = []; for (let i = 0; i < d.length; i += 4) px.push([d[i], d[i+1], d[i+2], d[i] + d[i+1] + d[i+2]]);
    px.sort((a, b) => b[3] - a[3]); const top = px.slice(0, Math.max(5, px.length / 20 | 0));
    const avg = [0, 1, 2].map(k => Math.round(top.reduce((s, q) => s + q[k], 0) / top.length));
    out.push(label + ' rgb(' + avg.join(',') + ')');
  }
  return out;
})()` });
console.log((r.result.result.value || [JSON.stringify(r.result)]).join("\n"));
ws.close(); proc.kill();
