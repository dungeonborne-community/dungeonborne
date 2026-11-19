// Link preview: node site/og/render.mjs — captures og.html with headless Chrome at 1200×630 and writes site/web/img/og.jpg.
import { spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const out = join(here, "..", "web", "img", "og.jpg");
const port = 9351;
const chrome = "C:/Program Files/Google/Chrome/Application/chrome.exe";
// --allow-file-access-from-files: the site fonts are loaded from disk (otherwise Chrome does not give them to a file:// page)
const proc = spawn(chrome, ["--headless=new", "--allow-file-access-from-files", `--remote-debugging-port=${port}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), "og-"))}`, "--hide-scrollbars", "about:blank"], { stdio: "ignore" });
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let targets;
for (let i = 0; i < 50; i++) {
  try {
    targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
    if (targets.length) break;
  } catch {}
  await sleep(200);
}
const ws = new WebSocket(targets.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) pending.get(m.id)(m), pending.delete(m.id);
});
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
// node site/og/render.mjs icon — icon for iPhone/Android: favicon.svg → apple-touch-icon.png 180×180 on the site background
if (process.argv[2] === "icon") {
  await send("Emulation.setDeviceMetricsOverride", { width: 180, height: 180, deviceScaleFactor: 1, mobile: false });
  await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 11, g: 9, b: 7, a: 1 } });
  await send("Page.enable");
  await send("Page.navigate", { url: pathToFileURL(join(here, "..", "web", "img", "favicon.svg")).href });
  await sleep(800);
  const png = await send("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: 180, height: 180, scale: 1 } });
  const file = join(here, "..", "web", "img", "apple-touch-icon.png");
  writeFileSync(file, Buffer.from(png.result.data, "base64"));
  console.log(`apple-touch-icon.png: ${Math.round(Buffer.from(png.result.data, "base64").length / 1024)} KB`);
  ws.close();
  proc.kill();
  process.exit(0);
}
await send("Emulation.setDeviceMetricsOverride", { width: 1200, height: 630, deviceScaleFactor: 1, mobile: false });
await send("Page.enable");
await send("Page.navigate", { url: pathToFileURL(join(here, "og.html")).href });
await sleep(1500);
const fonts = await send("Runtime.evaluate", { expression: "document.fonts.ready.then(() => [...document.fonts].filter(f => f.status === 'loaded').length)", awaitPromise: true, returnByValue: true });
const shot = await send("Page.captureScreenshot", { format: "jpeg", quality: 88, clip: { x: 0, y: 0, width: 1200, height: 630, scale: 1 } });
writeFileSync(out, Buffer.from(shot.result.data, "base64"));
console.log(`og.jpg: ${Math.round(Buffer.from(shot.result.data, "base64").length / 1024)} KB, fonts loaded: ${fonts.result.result.value}`);
ws.close();
proc.kill();
process.exit(0);
