// Screenshot of the launcher sandbox via the Chrome DevTools Protocol: node shot.mjs <url> <out.png> [w] [h] [lang]
import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [url, out, w = "1200", h = "760", lang = "ru", pre = ""] = process.argv.slice(2);
const chrome = "C:/Program Files/Google/Chrome/Application/chrome.exe";
const profile = mkdtempSync(join(tmpdir(), "cdp-"));
const port = 9333;
const proc = spawn(chrome, ["--headless=new", `--remote-debugging-port=${port}`, `--user-data-dir=${profile}`,
  `--window-size=${w},${h}`, "--hide-scrollbars", "--force-device-scale-factor=1", `--lang=${lang}`, "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

let targets;
for (let i = 0; i < 50; i++) {
  try { targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json(); if (targets.length) break; } catch {}
  await sleep(200);
}
const page = targets.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => { const m = JSON.parse(e.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });

await send("Emulation.setDeviceMetricsOverride", { width: +w, height: +h, deviceScaleFactor: 1, mobile: false });
await send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: process.env.RM || "no-preference" }] });
await send("Page.enable");
await send("Page.navigate", { url });
await sleep(2500);
const ev = async (expr) => (await send("Runtime.evaluate", { expression: expr, awaitPromise: true, returnByValue: true })).result?.result?.value;
// close the "What's new" splash if it is there
console.log("splash:", await ev(`(() => { const b = document.getElementById('splash-ok'); if (b && b.offsetParent) { b.click(); return 'closed'; } return 'none'; })()`));
await sleep(1200);
// remove the sandbox inventions: the update banner, the second server, "season 2"
console.log("clean:", await ev(`(() => {
  document.getElementById('update')?.classList.add('hidden');
  const list = document.getElementById('servers'); if (list && list.children.length > 1) [...list.children].slice(1).forEach(n => n.remove());
  const walk = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
  for (let n; (n = walk.nextNode());) {
    n.nodeValue = n.nodeValue.replace('Сезон 2: новые подземелья', 'Сезон «Плейтест»').replace('Season 2: new dungeons', 'Season “Playtest”').replace(/^season-2$/, 'playtest');
  }
  return 'ok';
})()`));
if (pre) { console.log("pre:", await ev(pre)); await sleep(800); }
await sleep(300);
const shot = await send("Page.captureScreenshot", { format: out.endsWith(".webp") ? "webp" : "png", quality: 88 });
writeFileSync(out, Buffer.from(shot.result.data, "base64"));
console.log("saved", out);
ws.close();
proc.kill();
