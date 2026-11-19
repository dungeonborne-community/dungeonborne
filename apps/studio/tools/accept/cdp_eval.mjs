// node cdp_eval.mjs <port> <js>: prints the value or the rejection.
// Windows can run out of free local ports for a moment when something else opens many connections, so connecting is retried.
const [port, js] = process.argv.slice(2);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function connect() {
  for (let i = 0; ; i++) {
    try {
      const page = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === "page");
      const ws = new WebSocket(page.webSocketDebuggerUrl);
      await new Promise((ok, fail) => {
        ws.addEventListener("open", ok, { once: true });
        ws.addEventListener("error", fail, { once: true });
      });
      return ws;
    } catch (e) {
      if (i >= 20) throw e;
      await sleep(500);
    }
  }
}

const ws = await connect();
let id = 0;
const pending = new Map();
ws.addEventListener("message", (m) => { const x = JSON.parse(m.data); if (x.id && pending.has(x.id)) { pending.get(x.id)(x); pending.delete(x.id); } });
const send = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const r = await send("Runtime.evaluate", { expression: `(async () => { try { return await (${js}); } catch (e) { return { rejected: e }; } })()`, awaitPromise: true, returnByValue: true });
console.log(JSON.stringify(r.result.result?.value ?? r.result));
ws.close();
