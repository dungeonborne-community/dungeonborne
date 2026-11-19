// node themes.mjs <url> <prefix> [w] [h] — go through the anchors, at each: theme, screenshot; console errors
import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url, prefix, W = "1440", H = "900"] = process.argv.slice(2);
const chrome = "C:/Program Files/Google/Chrome/Application/chrome.exe";
const port = 9335;
const proc = spawn(chrome, ["--headless=new", "--enable-gpu", "--use-angle=d3d11", "--ignore-gpu-blocklist", ...(process.env.GPUFLAGS ? process.env.GPUFLAGS.split(" ") : []), `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "cdp-"))}`,
  `--window-size=${W},${H}`, "--hide-scrollbars", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {} await sleep(200); }
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0; const pending = new Map(); const errs = [];
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); }
  if (m.method === "Runtime.exceptionThrown") errs.push("exc: " + m.params.exceptionDetails.exception?.description);
  if (m.method === "Runtime.consoleAPICalled" && /gpu|wgsl|shader/i.test(JSON.stringify(m.params.args))) errs.push("gpu: " + JSON.stringify(m.params.args.map(a => a.value ?? a.description)).slice(0, 600));
  if (m.method === "Log.entryAdded" && /gpu|wgsl|shader/i.test(m.params.entry.text)) errs.push("gpulog: " + m.params.entry.text.slice(0, 600));
  if (m.method === "Runtime.consoleAPICalled" && m.params.type === "error") errs.push("console: " + JSON.stringify(m.params.args.map(a => a.value ?? a.description)));
  if (m.method === "Log.entryAdded" && m.params.entry.level === "error" && !/video\//.test(m.params.entry.url || "") && !/\.webp/.test(m.params.entry.url || "")) errs.push("log: " + m.params.entry.text + " " + (m.params.entry.url || ""));
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
await send("Emulation.setDeviceMetricsOverride", { width: +W, height: +H, deviceScaleFactor: 1, mobile: +W < 768 });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: process.env.RM || "no-preference" }] });
await send("Runtime.enable"); await send("Log.enable"); await send("Page.enable");
if (process.env.NOGPU) await send("Page.addScriptToEvaluateOnNewDocument", { source: "Object.defineProperty(Navigator.prototype, \"gpu\", { get: () => undefined })" });
await send("Page.navigate", { url });
await sleep(2500);
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
const stops = (process.env.STOPS || "top,ch1,ch3,ch5,classes,ep,st1,lf1,news").split(",");
for (const s of stops) {
  const r = await ev(`(() => { const el = document.getElementById(${JSON.stringify(s)}); if (!el) return 'missing'; el.scrollIntoView({behavior:'instant'}); dispatchEvent(new Event('scroll')); return 'ok'; })()`);
  await sleep(+(process.env.WAIT || 1700));
  const th = await ev(`(document.documentElement.dataset.atmos || "no-gpu") + " " + document.querySelector('header').dataset.theme + ' accent=' + getComputedStyle(document.documentElement).getPropertyValue('--color-accent') + ' y=' + scrollY`);
  console.log(s, r, th);
  const shot = await send("Page.captureScreenshot", { format: "jpeg", quality: 80 });
  writeFileSync(`${prefix}-${s}-${Date.now() % 100000}.jpg`, Buffer.from(shot.result.data, "base64"));
}
console.log("errors:", errs.length ? errs.join("\n") : "none");
ws.close(); proc.kill();
