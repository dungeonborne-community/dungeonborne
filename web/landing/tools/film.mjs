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
import("node:fs").then(() => {});
const { writeFileSync } = await import("node:fs");
const W = 1440, H = 900;
await send("Emulation.setDeviceMetricsOverride", { width: W, height: H, deviceScaleFactor: 1, mobile: false });
await ev("document.documentElement.style.scrollSnapType = 'none'; document.getElementById(" + JSON.stringify(process.env.FROM || 'ch4') + ").scrollIntoView({behavior:'instant'})");
await sleep(2000);
const box = await ev("(() => { const rs = [...document.querySelectorAll('.story-mark')].map(m => m.getBoundingClientRect()); const l = Math.min(...rs.map(r => r.left)), t = Math.min(...rs.map(r => r.top)), rr = Math.max(...rs.map(r => r.right)), b = Math.max(...rs.map(r => r.bottom)); return { x: Math.max(0, l - 30), y: Math.max(0, t - 20), w: rr - l + 60, h: b - t + 40, n: rs.length }; })()");
const frames = [];
const shot = async () => { const r = await send("Page.captureScreenshot", { format: "jpeg", quality: 85 }); frames.push(r.result.data); };
await shot();
const t0 = Date.now();
await ev("document.getElementById(" + JSON.stringify(process.env.TO || 'ch5') + ").scrollIntoView({behavior:'instant'})");
const times = [0];
while (Date.now() - t0 < 1400) { await shot(); times.push(Date.now() - t0); }
console.log("box", JSON.stringify(box), "frames", frames.length, "times", times.join(","));
const html = '<body style="margin:0;background:#000;display:flex;gap:4px;font:12px sans-serif;color:#fff">' + frames.map((f, i) => '<div><div>' + times[i] + 'ms</div><div style="width:' + box.w + 'px;height:' + box.h + 'px;overflow:hidden"><img style="margin:-' + box.y + 'px 0 0 -' + box.x + 'px" src="data:image/jpeg;base64,' + f + '"></div></div>').join("") + "</body>";
writeFileSync("film/strip.html", html);
ws.close(); proc.kill();
