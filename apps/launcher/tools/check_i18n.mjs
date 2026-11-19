// node tools/check_i18n.mjs . (from the launcher folder): keys used by the UI and the Rust side vs ui/i18n.js (ru/en)
import fs from "node:fs";
import path from "node:path";
import vm from "node:vm";

const root = process.argv[2];
const ui = path.join(root, "ui");
const read = (p) => fs.readFileSync(p, "utf8");

const ctx = { window: {} };
vm.runInNewContext(read(path.join(ui, "i18n.js")), ctx);
const I = ctx.window.I18N;
const langs = Object.keys(I);
console.log("languages:", langs.join(", "), langs.map((l) => `${l}=${Object.keys(I[l]).length}`).join(" "));

// 1. keys in one language only
for (const a of langs) for (const b of langs) {
  if (a === b) continue;
  const miss = Object.keys(I[a]).filter((k) => !(k in I[b]));
  if (miss.length) console.log(`\nin ${a}, missing in ${b} (${miss.length}):`, miss.join(", "));
}

// 2. placeholders {x} must match between languages
for (const k of Object.keys(I.ru)) {
  if (!I.en[k]) continue;
  const ph = (s) => [...String(s).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(",");
  if (ph(I.ru[k]) !== ph(I.en[k])) console.log(`placeholders differ: ${k}: ru {${ph(I.ru[k])}} en {${ph(I.en[k])}}`);
}

// 3. keys used in the UI
const html = read(path.join(ui, "index.html"));
const app = read(path.join(ui, "app.js"));
const used = new Set();
for (const m of html.matchAll(/data-t="([^"]+)"/g)) used.add(m[1]);
for (const m of app.matchAll(/\bt\(\s*"([^"]+)"/g)) used.add(m[1]);
for (const m of app.matchAll(/\bt\(\s*'([^']+)'/g)) used.add(m[1]);
// t(cond ? "a" : "b") and similar: every "x.y" literal that looks like a key
for (const m of app.matchAll(/"([a-z_]+(?:\.[a-z0-9_]+)+)"/g)) if (/^(card|err|lobby|net|notes|tabs|settings|nav|play|mods|log|update|login|dlg|hosts|splash|run|status|install|cache|boot|key|account|server|api)\./.test(m[1])) used.add(m[1]);
const missing = [...used].filter((k) => !(k in I.ru) || !(k in I.en));
console.log(`\nused literal keys: ${used.size}; missing: ${missing.length}`, missing.join(", "));

// 4. template keys (`x.${...}`): listed for a manual check of every value
const templ = new Set([...app.matchAll(/t\(\s*`([^`]+)`/g)].map((m) => m[1]));
console.log("\ntemplate keys:", [...templ].join("  |  "));

// 5. Rust error codes -> err.<code>
const src = path.join(root, "src-tauri", "src");
const codes = new Set();
for (const f of fs.readdirSync(src).filter((f) => f.endsWith(".rs"))) {
  const t = read(path.join(src, f)).replace(/#\[cfg\(test\)\][\s\S]*$/, "");
  for (const m of t.matchAll(/bail!\(\s*"([a-z_.]+)"/g)) codes.add(m[1]);
  for (const m of t.matchAll(/Error::new\(\s*"([a-z_.]+)"/g)) codes.add(m[1]);
}
const noErr = [...codes].filter((c) => !(`err.${c}` in I.ru) || !(`err.${c}` in I.en));
console.log(`\nrust error codes: ${codes.size}; without err.* text: ${noErr.length}`, noErr.join(", "));
// backend api.* codes the server sends (from backups/src/auth/web.rs)
const web = path.join(root, "..", "backups", "src", "auth", "web.rs");
if (fs.existsSync(web)) {
  const api = new Set([...read(web).matchAll(/err\(\s*StatusCode::\w+,\s*"([a-z_]+)"/g)].map((m) => m[1]));
  for (const f of ["accounts.rs", "provider.rs", "mod.rs", "sessions.rs"]) {
    const p = path.join(root, "..", "backups", "src", "auth", f);
    if (fs.existsSync(p)) for (const m of read(p).matchAll(/reason:\s*"([a-z_]+)"|Denied::\w+\("([a-z_]+)"/g)) api.add(m[1] ?? m[2]);
  }
  const noApi = [...api].filter((c) => !(`err.api.${c}` in I.ru) || !(`err.api.${c}` in I.en));
  console.log(`backend api codes: ${api.size}; without err.api.* text: ${noApi.length}`, noApi.join(", "));
}

// 6. text written straight into app.js / index.html (Cyrillic outside i18n, or visible Latin words in html text nodes)
const cyr = /[А-Яа-яЁё]/;
const lines = app.split(/\r?\n/);
// a line comment starts at "//" outside a string; Cyrillic before it is text in code
const code = (l) => l.replace(/"(?:[^"\\]|\\.)*"|`(?:[^`\\]|\\.)*`/g, (s) => s.replace(/\/\//g, "")).replace(/\/\/.*$/, "");
const hard = lines.map((l, i) => [i + 1, l]).filter(([, l]) => cyr.test(code(l)));
console.log(`\napp.js lines with Cyrillic outside comments: ${hard.length}`);
for (const [n, l] of hard.slice(0, 40)) console.log(`  ${n}: ${l.trim().slice(0, 140)}`);
const htmlText = [...html.replace(/<!--[\s\S]*?-->/g, "").matchAll(/>([^<>{}]*[A-Za-zА-Яа-яЁё][^<>{}]*)</g)]
  .map((m) => m[1].trim())
  .filter((s) => s && !/^&#x[0-9A-F]+;$/.test(s));
console.log(`\nindex.html text nodes not from data-t: ${htmlText.length}`);
for (const s of htmlText.slice(0, 60)) console.log("  ", s.slice(0, 120));
// attributes a user sees
const attrs = [...html.matchAll(/\b(placeholder|title|aria-label|alt)="([^"]*[A-Za-zА-Яа-яЁё][^"]*)"/g)].map((m) => `${m[1]}="${m[2]}"`);
console.log(`\nvisible attributes: ${attrs.length}`);
for (const a of attrs) console.log("  ", a);
