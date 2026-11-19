// node load.mjs <url> — what loading takes and how long: navigation, paints, LCP, long tasks, resources
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url] = process.argv.slice(2);
const port = 9343;
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
await send("Page.enable");
await send("Page.addScriptToEvaluateOnNewDocument", { source: `window.__lt = []; window.__lcp = 0; new MutationObserver(() => { if (!document.documentElement.hasAttribute('data-boot') && !window.__bootGone) window.__bootGone = Math.round(performance.now()); }).observe(document, { attributes: true, subtree: false, attributeFilter: ['data-boot'] });
  try { new PerformanceObserver(l => l.getEntries().forEach(e => __lt.push({ s: Math.round(e.startTime), d: Math.round(e.duration), scripts: (e.scripts || []).map(x => (x.sourceFunctionName || x.invoker || '?') + '@' + (x.sourceURL || '').split('/').pop() + ':' + Math.round(x.duration)).join(' ') }))).observe({ type: 'long-animation-frame', buffered: true }); } catch {}
  try { new PerformanceObserver(l => l.getEntries().forEach(e => __lcp = Math.round(e.startTime))).observe({ type: 'largest-contentful-paint', buffered: true }); } catch {}` });
await send("Page.navigate", { url });
await sleep(5000);
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
console.log(await ev(`(() => {
  const n = performance.getEntriesByType('navigation')[0];
  const paints = Object.fromEntries(performance.getEntriesByType('paint').map(e => [e.name, Math.round(e.startTime)]));
  const res = performance.getEntriesByType('resource');
  const byType = {}; for (const r of res) { const k = r.initiatorType; byType[k] = byType[k] || { n: 0, ms: 0, kb: 0, fail: 0 }; byType[k].n++; byType[k].ms = Math.max(byType[k].ms, Math.round(r.responseEnd)); byType[k].kb += Math.round((r.transferSize || 0) / 1024); if (!r.transferSize && !r.decodedBodySize) byType[k].fail++; }
  const slow = res.filter(r => r.duration > 150).map(r => r.name.split('/').slice(-2).join('/') + ' ' + Math.round(r.startTime) + '→' + Math.round(r.responseEnd));
  return JSON.stringify({ ttfb: Math.round(n.responseStart), domInteractive: Math.round(n.domInteractive), dcl: Math.round(n.domContentLoadedEventEnd), load: Math.round(n.loadEventEnd), paints, lcp: __lcp, bootGone: Math.round(performance.getEntriesByName('boot-done')[0]?.startTime || -1), nodes: document.getElementsByTagName('*').length, byType, slow: slow.slice(0, 12), longFrames: __lt.slice(0, 12) }, null, 1);
})()`));
ws.close(); proc.kill(); process.exit(0);
