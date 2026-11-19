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
const mouse = (type, x, y, buttons = 0) => send("Input.dispatchMouseEvent", { type, x, y, button: (type === "mouseMoved" && !buttons) ? "none" : "left", buttons, clickCount: 1 });
await sleep(800);
const st = () => ev(`JSON.stringify({ native: innerWidth - document.documentElement.clientWidth, on: !document.querySelector('.sbar').hidden, show: document.querySelector('.sbar').hasAttribute('data-show'), sh: getComputedStyle(document.querySelector('.sbar')).getPropertyValue('--sh'), sy: getComputedStyle(document.querySelector('.sbar')).getPropertyValue('--sy'), y: Math.round(scrollY) })`);
console.log("start          ", await st());
await mouse("mouseMoved", 700, 400);
await ev("scrollBy({top: 600, behavior: 'instant'})"); await sleep(200);
console.log("after scroll   ", await st());
await sleep(1500);
console.log("1.5 s later    ", await st());
const W = await ev("innerWidth"); await mouse("mouseMoved", W - 6, 400); await sleep(300);
console.log("mouse at edge  ", await st());
const sh1 = await send("Page.captureScreenshot", { format: "png" }); (await import("node:fs")).writeFileSync("sbar.png", Buffer.from(sh1.result.data, "base64"));
const th = await ev("(() => { const r = document.querySelector('.sbar-thumb').getBoundingClientRect(); return [r.x + r.width/2, r.y + r.height/2]; })()");
await mouse("mousePressed", th[0], th[1], 1);
for (let i = 1; i <= 10; i++) { await mouse("mouseMoved", th[0], th[1] + i * 20, 1); await sleep(16); }
await mouse("mouseReleased", th[0], th[1] + 200, 0); await sleep(200);
console.log("thumb dragged  ", await st());
await mouse("mousePressed", W - 6, 850, 1); await mouse("mouseReleased", W - 6, 850, 0); await sleep(200);
console.log("track click low", await st());
await mouse("mouseMoved", 700, 400); await sleep(1600);
console.log("mouse away     ", await st());
ws.close(); proc.kill();
