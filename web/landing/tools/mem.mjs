import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url] = process.argv.slice(2);
const port = 9344;
const proc = spawn("C:/Program Files/Google/Chrome/Application/chrome.exe", ["--headless=new", "--enable-gpu", "--use-angle=d3d11", "--ignore-gpu-blocklist",
  `--remote-debugging-port=${port}`, `--user-data-dir=${mkdtempSync(join(tmpdir(), "cdp-"))}`, "--window-size=1920,1080", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {} await sleep(200); }
const ver = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const mk = async (u) => { const ws = new WebSocket(u); await new Promise((r) => ws.addEventListener("open", r, { once: true })); let id = 0; const p = new Map(); ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } }); return [ws, (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); })]; };
const [bws, bsend] = await mk(ver.webSocketDebuggerUrl);
const [ws, send] = await mk(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await send("Emulation.setDeviceMetricsOverride", { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await send("Page.enable"); await send("Performance.enable");
await send("Page.navigate", { url }); await sleep(4000);
const ev = async (e) => (await send("Runtime.evaluate", { expression: e, awaitPromise: true, returnByValue: true })).result?.result?.value;
const cpu = async () => (await bsend("SystemInfo.getProcessInfo")).result.processInfo.reduce((a, p) => a + p.cpuTime, 0);
const heap = (await send("Runtime.getHeapUsage")).result;
const m = Object.fromEntries((await send("Performance.getMetrics")).result.metrics.map((x) => [x.name, x.value]));
console.log("JS heap used", (heap.usedSize / 1048576).toFixed(1), "MB of", (heap.totalSize / 1048576).toFixed(1), "| DOM nodes", m.Nodes, "| layouts", m.LayoutCount, "| listeners", m.JSEventListeners);
for (const [label, setup] of [["idle top (decor running)", ""], ["idle, calm (unfocused)", "document.documentElement.setAttribute('data-calm',''); dispatchEvent(new Event('calmchange'))"], ["idle mid-page", "document.documentElement.removeAttribute('data-calm'); dispatchEvent(new Event('calmchange')); document.getElementById('ch3').scrollIntoView({behavior:'instant'})"]]) {
  await ev(setup || "0"); await sleep(1500);
  const c0 = await cpu(); await sleep(5000); const c1 = await cpu();
  console.log(label.padEnd(26), "CPU all Chrome processes:", ((c1 - c0) / 5 * 100).toFixed(1) + "% of one core");
}
ws.close(); bws.close(); proc.kill(); process.exit(0);
