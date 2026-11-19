// node trans.mjs <url> — the "Crypt" → "Blood" transition: element colours every ~40 ms and frame durations
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url] = process.argv.slice(2);
const port = 9339;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe", ["--headless=new", "--enable-gpu", "--use-angle=d3d11", "--ignore-gpu-blocklist",
  `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "cdp-"))}`, "--window-size=1920,1080", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {} await sleep(200); }
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0; const p = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await send("Emulation.setDeviceMetricsOverride", { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await send("Page.enable"); await send("Page.navigate", { url }); await sleep(2500);
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
for (const [from, to] of [["ch4", "ch5"], ["ch6", "classes"], ["classes", "start"]]) {
await ev(`document.documentElement.style.scrollSnapType = 'none'; document.getElementById('${from}').scrollIntoView({behavior:'instant'})`);
await sleep(1800);
const out = await ev(`new Promise(res => {
  const tile = document.querySelector('header a[href="#top"] .rounded-ctl');
  const rows = []; const frames = []; let last = performance.now(); const t0 = last;
  const f = (t) => { frames.push(t - last); last = t; if (t - t0 < 1500) requestAnimationFrame(f); };
  requestAnimationFrame(f);
  document.getElementById('${to}').scrollIntoView({behavior:'instant'});
  const iv = setInterval(() => rows.push(getComputedStyle(tile).color), 100);
  setTimeout(() => { clearInterval(iv); res({ rows, avg: +(frames.reduce((a,b)=>a+b,0)/frames.length).toFixed(1), max: +Math.max(...frames.slice(2)).toFixed(1), long: frames.slice(2).filter(x => x > 25).length, theme: document.querySelector('header').dataset.theme }); }, 1600);
})`);
console.log(from, "->", to, out.theme, "| frames avg", out.avg, "ms, max (after jump)", out.max, "ms, >25ms:", out.long);
console.log("  logo D color:", out.rows.join(" | "));
}
ws.close(); proc.kill();
