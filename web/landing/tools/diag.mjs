// node diag.mjs <url> [from] [to] — smooth scrolling across a theme boundary: elements with intermediate colours and long frames
import { spawn } from "node:child_process";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
const [url, from = "ch4", to = "ch5"] = process.argv.slice(2);
const port = 9340;
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
await ev(`document.getElementById(${JSON.stringify(from)}).scrollIntoView({behavior:"instant"}); window.STEP = ${process.env.STEP || 40}; document.documentElement.style.scrollSnapType = "none";`);
await sleep(2500);
const out = await ev(`new Promise(res => {
  const loaf = [];
  try { new PerformanceObserver(l => loaf.push(...l.getEntries())).observe({ type: 'long-animation-frame' }); } catch {}
  const els = [...document.querySelectorAll('#game *, header *')].filter(e => e.getClientRects().length);
  const props = ['color', 'borderTopColor', 'backgroundColor', 'fill', 'stroke', 'boxShadow'];
  const seen = els.map(() => props.map(() => []));
  const sample = () => els.forEach((e, i) => { const cs = getComputedStyle(e); props.forEach((pr, j) => { const v = cs[pr]; const a = seen[i][j]; if (a[a.length - 1] !== v) a.push(v); }); });
  sample();
  const startTheme = document.querySelector('header').dataset.theme;
  const frames = []; let last = performance.now(); let switchedAt = null; const t0 = last;
  const target = document.getElementById(${JSON.stringify(to)}).getBoundingClientRect().top + scrollY;
  const f = (t) => {
    const spc = document.getElementById(${JSON.stringify(to)}); frames.push({ t: Math.round(t - t0), d: +(t - last).toFixed(1), theme: document.querySelector('header').dataset.theme, y: Math.round(scrollY), past: spc.getBoundingClientRect().top <= innerHeight * 0.5 }); last = t;
    if (switchedAt === null && document.querySelector('header').dataset.theme !== startTheme) switchedAt = Math.round(t - t0);
    if (scrollY < target + innerHeight) scrollBy({ top: +(window.STEP || 40), behavior: "instant" });
    if (t - t0 < 4000) requestAnimationFrame(f);
  };
  requestAnimationFrame(f);
  const iv = setInterval(sample, 40);
  setTimeout(() => {
    clearInterval(iv); sample();
    const bad = [];
    els.forEach((e, i) => props.forEach((pr, j) => { if (seen[i][j].length > 2) bad.push({ el: e.tagName.toLowerCase() + '.' + [...e.classList].slice(0, 3).join('.'), prop: pr, n: seen[i][j].length, vals: seen[i][j].slice(0, 4).map(v => v.slice(0, 40)) }); }));
    const groups = {}; for (const b of bad) { const k = b.el + ' ' + b.prop; groups[k] = groups[k] || { ...b, count: 0 }; groups[k].count++; }
    res({ startTheme, endTheme: document.querySelector('header').dataset.theme, switchedAt, bad: Object.values(groups).sort((a, b) => b.count - a.count).slice(0, 15),
      long: frames.filter(x => x.d > 25), frames, loaf: loaf.map(l => ({ start: Math.round(l.startTime - (t0 - performance.timeOrigin + performance.timeOrigin)), dur: Math.round(l.duration), render: Math.round(l.renderStart ? l.startTime + l.duration - l.renderStart : 0), style: Math.round(l.styleAndLayoutStart ? l.startTime + l.duration - l.styleAndLayoutStart : 0), scripts: l.scripts.map(s => (s.sourceFunctionName || s.invoker) + ':' + Math.round(s.duration)).join(' ') })) });
  }, 4300);
})`);
console.log("theme", out.startTheme, "->", out.endTheme, "switched at", out.switchedAt, "ms");
console.log("elements with intermediate values:", out.bad.length, "y from", out.frames[0]?.y, "to", out.frames[out.frames.length-1]?.y, "frames", out.frames.length, "themes seen", [...new Set(out.frames.map(f => f.theme))].join(","));
for (const b of out.bad) console.log(" ", b.count + "x", b.el, b.prop, "steps:", b.n, JSON.stringify(b.vals));
console.log("long frames (>25ms):", JSON.stringify(out.long));
const fp = out.frames.find(x => x.past); console.log("crossed boundary at", fp && fp.t, "ms, y", fp && fp.y, "; scroll ended", out.frames.filter(x=>x.y<out.frames[out.frames.length-1].y).slice(-1)[0]?.t);
console.log("LoAF:", JSON.stringify(out.loaf, null, 0));
ws.close(); proc.kill();
