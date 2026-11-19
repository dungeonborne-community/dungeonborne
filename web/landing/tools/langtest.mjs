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
const warns = [];
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.method === "Runtime.consoleAPICalled" && (m.params.type === "warning" || m.params.type === "error")) warns.push(m.params.args.map(x => x.value).join(" ")); });
await send("Runtime.enable");
for (const q of ["", "?lang=ru", "?lang=en"]) {
  await send("Page.navigate", { url: "http://localhost:8821/" + q }); await sleep(2500);
  console.log((q || "default") + ":", await ev(`JSON.stringify({ lang: document.documentElement.lang, title: document.title, h1sub: document.querySelector('[data-t="hero.sub"]').textContent, lead: document.querySelector('[data-t="g.title"]').textContent, cls: document.querySelector('.class-panel h4').textContent, pros: document.querySelector('[data-t="cls.pros"]').textContent, btn: document.querySelector('#lang-text').textContent, news: document.querySelector('#news-list details summary')?.textContent.trim().slice(0, 30) })`));
}
console.log("warnings:", warns.length ? warns.slice(0, 5) : "none");
ws.close(); proc.kill();
