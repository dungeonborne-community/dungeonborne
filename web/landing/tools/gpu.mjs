// node gpu.mjs <url> — load of the real GPU (Windows GPU Engine counter, 3D) by the processes of this Chrome, per variant
import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url] = process.argv.slice(2);
const chrome = "C:/Program Files/Google/Chrome/Application/chrome.exe";
const port = 9337;
const proc = spawn(chrome, ["--headless=new", "--enable-gpu", "--use-angle=d3d11", "--ignore-gpu-blocklist", `--remote-debugging-port=${port}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "cdp-"))}`, "--window-size=1920,1080", "--hide-scrollbars", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) { try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {} await sleep(200); }
const ver = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json();
const bws = new WebSocket(ver.webSocketDebuggerUrl); await new Promise((r) => bws.addEventListener("open", r, { once: true }));
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl); await new Promise((r) => ws.addEventListener("open", r, { once: true }));
const mk = (sock) => { let id = 0; const p = new Map(); sock.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && p.has(m.id)) { p.get(m.id)(m); p.delete(m.id); } });
  return (method, params = {}) => new Promise((r) => { const i = ++id; p.set(i, r); sock.send(JSON.stringify({ id: i, method, params })); }); };
const send = mk(ws), bsend = mk(bws);
await send("Emulation.setDeviceMetricsOverride", { width: 1920, height: 1080, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: "no-preference" }] });
await send("Page.enable"); await send("Page.navigate", { url }); await sleep(3000);
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
const gpuInfo = (await bsend("SystemInfo.getInfo")).result?.gpu?.devices?.map((d) => d.deviceString).join(" | ");
console.log("gpu:", gpuInfo);
const pids = new Set((await bsend("SystemInfo.getProcessInfo")).result.processInfo.map((p) => p.id));
const gpuLoad = () => {
  const out = execFileSync("powershell", ["-NoProfile", "-Command",
    "1..5 | ForEach-Object { Start-Sleep -Milliseconds 1000; Get-CimInstance Win32_PerfFormattedData_GPUPerformanceCounters_GPUEngine | Where-Object { $_.Name -like '*engtype_3D' } | ForEach-Object { $_.Name + '=' + $_.UtilizationPercentage } }"], { encoding: "utf8" });
  let sum = 0;
  for (const line of out.split(/\r?\n/)) { const m = line.match(/pid_(\d+)_.*=([\d.,]+)/); if (m && pids.has(+m[1])) sum += parseFloat(m[2].replace(",", ".")); }
  return sum / 5;
};
const setup = {
  "now-all": "",
  "now-no-atmos": ".atmos{display:none}",
  "now-no-anim": ".atmos{display:none} *,*::before,*::after{animation:none!important}",
};
for (const v of (process.env.VARIANTS || Object.keys(setup).join(",")).split(",")) {
  await ev(`(() => { let s = document.getElementById('perf-css'); if (!s) { s = document.createElement('style'); s.id='perf-css'; document.head.append(s); } s.textContent = ${JSON.stringify(setup[v] ?? "")}; scrollTo(0, 0); })()`);
  await sleep(1000);
  const idle = gpuLoad();
  const scrolling = ev(`new Promise(r => { const t0 = performance.now(); const f = () => { scrollBy(0, 6); performance.now() - t0 < 7000 ? requestAnimationFrame(f) : r(); }; requestAnimationFrame(f); })`);
  const sc = gpuLoad(); await scrolling;
  console.log(v.padEnd(10), "idle GPU", idle.toFixed(1) + "%", " scroll GPU", sc.toFixed(1) + "%");
}
ws.close(); bws.close(); proc.kill();
