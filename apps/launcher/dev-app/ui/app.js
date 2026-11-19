// Dungeonborne Dev UI: plain JS over Tauri commands (src/lib.rs). No build step.
"use strict";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);

const ERR = {
  ports_busy: "Ports busy",
  port_lock: "Port lock",
  uac_cancelled: "UAC cancelled",
  backend_running: "Backend already running",
  install_invalid: "No game exe",
  "admin.unauthorized": "401: admin Bearer needed (Log in / paste a token)",
  "admin.forbidden": "403: no rights (admin role) or the request is not from loopback",
  "admin.player_not_found": "404: no player with this open_id",
  "admin.bad_request": "400",
  network: "No connection",
};
function errText(e) {
  if (e && typeof e === "object" && e.code) return (ERR[e.code] ? tr(ERR[e.code]) : e.code) + (e.detail ? ":\n" + e.detail : "");
  return String(e);
}
let toastTimer;
function toast(msg, info) {
  const t = $("toast");
  t.textContent = msg;
  t.className = "toast" + (info ? " info" : "");
  t.style.display = "block";
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.style.display = "none"), info ? 3000 : 9000);
}
async function call(cmd, args, quiet) {
  try {
    return await invoke(cmd, args);
  } catch (e) {
    if (!quiet) toast(errText(e));
    throw e;
  }
}
const hhmmss = (ms) => new Date(ms).toTimeString().slice(0, 8);

// ------------------------------------------------------------------------------------------- state
let S = null; // last get_state
let editIdx = 0; // profile in the form
let formDirty = false;

async function refresh() {
  try {
    S = await invoke("get_state");
  } catch (e) {
    return;
  }
  $("ver").textContent = "v" + S.version;
  renderChips();
  renderProfileSelect();
  renderInstalls();
  renderHost(S.host);
  if (!formDirty) fillForm();
}

function chip(text, cls, title) {
  return `<span class="chip ${cls || ""}" title="${esc(title || "")}">${esc(text)}</span>`;
}
function renderChips() {
  const b = S.backend;
  const lock = S.lock;
  const out = [];
  out.push(b.running ? chip(tr("backend: {profile} (pid {pid})", { profile: b.profile, pid: b.pid }), "ok") : chip(tr("backend: stopped"), ""));
  if (lock) out.push(chip(tr("lock: {owner}", { owner: lock.owner }), lock.owner === "dev-app" ? "ok" : "warn", `${lock.since}: ${lock.purpose}`));
  else out.push(chip(tr("lock: free"), ""));
  out.push(S.elevated ? chip(tr("admin"), "ok", tr("dev-app runs as administrator: the game starts without UAC")) : chip("UAC", "warn", tr("dev-app without administrator rights: launching the game asks for UAC")));
  out.push(S.dbtools_task ? chip("DbTools", "ok", tr("mutexes are killed by the DbTools task without UAC")) : chip(tr("no DbTools"), "", tr("mutexes — killmutex.ps1 via UAC (see dbtest\\README_admin.md)")));
  out.push(chip(tr("game: {n}", { n: S.game_processes }), S.game_processes ? "ok" : ""));
  if (S.host.phase) out.push(chip(tr("server: {phase} ({age} s)", { phase: S.host.phase, age: S.host.age_s }), S.host.phase === "ready" ? "ok" : "", S.host.raw));
  if (S.mock) out.push(chip(tr("mock :{port}{relay}", { port: S.mock.port, relay: S.mock.require_relay ? " relay" : "" }), "warn"));
  $("chips").innerHTML = out.join("");
  $("stopBtn").disabled = !b.running;
}

function renderProfileSelect() {
  const sel = $("profileSel");
  const names = S.settings.profiles.map((p) => p.name).join("\n");
  if (sel.dataset.names !== names) {
    sel.innerHTML = S.settings.profiles.map((p, i) => `<option value="${i}">${esc(p.name)}</option>`).join("");
    sel.dataset.names = names;
  }
  sel.value = String(S.settings.selected);
}

$("profileSel").onchange = async () => {
  const i = Number($("profileSel").value);
  S.settings.selected = i;
  editIdx = i;
  formDirty = false;
  await call("save_settings", { settings: S.settings });
  fillForm();
};

// ------------------------------------------------------------------------------------------- profile form
const P_TEXT = ["name", "server_toml", "db_path", "bind_ip", "host_map"];
const P_BOOL = ["rpc_dump", "pve_sniff", "release", "host_observer", "host_window"];

function installName(i) {
  const r = S.settings.installs[i] || "";
  return `${i + 1}: ${r.split("\\").slice(-2).join("\\") || "—"}`;
}
function installOptions(sel, value) {
  sel.innerHTML = S.settings.installs.map((_, i) => `<option value="${i}">${esc(installName(i))}</option>`).join("");
  sel.value = String(value ?? 0);
}

function fillForm() {
  editIdx = S.settings.selected;
  const p = S.settings.profiles[editIdx];
  if (!p) return;
  for (const k of P_TEXT) $("p_" + k).value = p[k] || "";
  for (const k of P_BOOL) $("p_" + k).checked = !!p[k];
  $("p_client_mode").value = p.client_mode;
  $("p_host").value = p.host;
  installOptions($("p_host_install"), p.host_install);
  $("p_clients").innerHTML = S.settings.installs
    .map((_, i) => `<label class="chk"><input type="checkbox" data-ci="${i}" ${p.clients.includes(i) ? "checked" : ""}> ${esc(installName(i))}</label>`)
    .join("");
  $("p_cmdline").textContent = S.commands[editIdx] || "";
  if (!$("h_install").dataset.init) {
    installOptions($("h_install"), p.host_install);
    $("h_install").dataset.init = "1";
  }
}

function readForm() {
  const p = structuredClone(S.settings.profiles[editIdx]);
  for (const k of P_TEXT) p[k] = $("p_" + k).value.trim();
  for (const k of P_BOOL) p[k] = $("p_" + k).checked;
  p.client_mode = $("p_client_mode").value;
  p.host = $("p_host").value;
  p.host_install = Number($("p_host_install").value);
  p.clients = [...document.querySelectorAll("#p_clients input[data-ci]")].filter((c) => c.checked).map((c) => Number(c.dataset.ci));
  return p;
}

$("profileForm").addEventListener("input", () => (formDirty = true));
$("profileForm").addEventListener("change", () => (formDirty = true));

async function saveProfile() {
  S.settings.profiles[editIdx] = readForm();
  await call("save_settings", { settings: S.settings });
  formDirty = false;
  await refresh();
}
$("saveProfile").onclick = () => saveProfile().then(() => toast(tr("profile saved"), true));
$("dupProfile").onclick = async () => {
  const p = readForm();
  p.name += tr(" (copy)");
  S.settings.profiles.push(p);
  S.settings.selected = S.settings.profiles.length - 1;
  formDirty = false;
  await call("save_settings", { settings: S.settings });
  await refresh();
};
$("delProfile").onclick = async () => {
  if (S.settings.profiles.length <= 1) return toast(tr("the last profile cannot be deleted"));
  if (!confirm(tr("Delete profile \"{name}\"?", { name: S.settings.profiles[editIdx].name }))) return;
  S.settings.profiles.splice(editIdx, 1);
  S.settings.selected = 0;
  formDirty = false;
  await call("save_settings", { settings: S.settings });
  await refresh();
};
$("copyCmd").onclick = () => navigator.clipboard.writeText($("p_cmdline").textContent).then(() => toast(tr("copied"), true));

async function saveIfDirty() {
  if (formDirty) await saveProfile();
}
$("runBtn").onclick = async () => {
  await saveIfDirty();
  $("flow").innerHTML = "";
  await call("run_profile", { profile: S.settings.selected });
};
$("cancelFlowBtn").onclick = () => call("cancel_flow");
$("startBackendOnly").onclick = async () => {
  await saveIfDirty();
  await call("backend_start", { profile: S.settings.selected });
  refresh();
};
$("stopBtn").onclick = () => call("backend_stop").then(refresh);

// ------------------------------------------------------------------------------------------- flow
listen("flow", (e) => {
  const { step, state, text } = e.payload;
  if (step === "start") $("flow").innerHTML = "";
  const d = document.createElement("div");
  d.className = state;
  d.textContent = `${hhmmss(Date.now())} ${step}: ${text}`;
  $("flow").appendChild(d);
  $("flow").scrollTop = 1e9;
  if (step === "done" && state === "error") toast(text);
  refresh();
});

// ------------------------------------------------------------------------------------------- ports
async function checkPorts() {
  const v = await call("check_ports");
  $("lockInfo").textContent = v.lock ? tr("Port lock: {owner} since {since} — {purpose}", v.lock) : tr("Port lock is free (portlock.ps1)");
  $("portsBody").innerHTML = v.ports
    .map((p) => {
      const cls = !p.busy ? "" : p.ours ? "ours" : p.blocking ? "busy" : "info";
      const who = p.busy ? `${esc(p.who)}${p.path ? `<br><span class="muted">${esc(p.path)}</span>` : ""}` : `<span class="muted">${tr("free")}</span>`;
      return `<tr class="${cls}"><td class="mono">${p.port}</td><td>${esc(p.role)}</td><td>${who}</td></tr>`;
    })
    .join("");
}
$("portsBtn").onclick = checkPorts;

// ------------------------------------------------------------------------------------------- backend log
const TAGS = ["[RPC]", "[Auth]", "[Relay]", "[GM]", "[Admin]", "[DS]", "[Room]", "[Matchmaking]"];
const activeTags = new Set();
const LOG_KEEP = 20000, DOM_KEEP = 4000;
let lines = [];

$("logFilters").innerHTML = `<button data-tag="" class="on">${tr("all")}</button>` + TAGS.map((t) => `<button data-tag="${t}">${t}</button>`).join("");
$("logFilters").onclick = (e) => {
  const t = e.target.dataset.tag;
  if (t === undefined) return;
  if (t === "") activeTags.clear();
  else activeTags.has(t) ? activeTags.delete(t) : activeTags.add(t);
  for (const b of $("logFilters").children) b.classList.toggle("on", b.dataset.tag === "" ? activeTags.size === 0 : activeTags.has(b.dataset.tag));
  renderLog();
};
$("logSearch").oninput = () => renderLog();
$("logErrOnly").onchange = () => renderLog();
$("logClear").onclick = async () => {
  await call("backend_log_clear");
  lines = [];
  renderLog();
};

function match(l) {
  if ($("logErrOnly").checked && !l.err) return false;
  if (activeTags.size) {
    let ok = false;
    for (const t of activeTags) if (l.tag.startsWith(t.slice(0, -1)) || l.text.includes(t)) ok = true;
    if (!ok) return false;
  }
  const q = $("logSearch").value.trim().toLowerCase();
  return !q || l.text.toLowerCase().includes(q);
}
function lineNode(l) {
  const d = document.createElement("div");
  if (l.err) d.className = "e";
  const tag = l.tag ? l.text.indexOf(l.tag) : -1;
  d.innerHTML = `<span class="ts">${hhmmss(l.ts)}</span> ` + (tag >= 0 ? esc(l.text.slice(0, tag)) + `<span class="t">${esc(l.tag)}</span>` + esc(l.text.slice(tag + l.tag.length)) : esc(l.text));
  return d;
}
function renderLog() {
  const box = $("log");
  const shown = lines.filter(match).slice(-DOM_KEEP);
  box.replaceChildren(...shown.map(lineNode));
  $("logCount").textContent = `${shown.length} / ${lines.length}`;
  if ($("logAuto").checked) box.scrollTop = 1e9;
}
function appendLog(batch) {
  lines.push(...batch);
  if (lines.length > LOG_KEEP) lines = lines.slice(-LOG_KEEP);
  const box = $("log");
  const frag = document.createDocumentFragment();
  for (const l of batch) if (match(l)) frag.appendChild(lineNode(l));
  box.appendChild(frag);
  while (box.childElementCount > DOM_KEEP) box.firstChild.remove();
  $("logCount").textContent = `${box.childElementCount} / ${lines.length}`;
  if ($("logAuto").checked) box.scrollTop = 1e9;
}
listen("backend-log", (e) => appendLog(e.payload));
listen("backend-state", () => {
  refresh();
  checkPorts();
});

// ------------------------------------------------------------------------------------------- game
function renderInstalls() {
  $("elevNote").textContent = S.elevated
    ? tr("dev-app runs as administrator — the game starts without UAC.")
    : tr("dev-app has no administrator rights — Windows asks for UAC on every game launch (the game under ACE needs the rights). Run dev-app as administrator to avoid the prompt.");
  const rows = S.installs.map((i) => {
    const st = !i.ok ? `<span class="muted">${esc(i.error)}</span>` : (i.running ? tr("running (UE4SS.log locked)") : tr("not running")) + (i.ue4ss ? "" : tr(", no UE4SS"));
    const dis = i.ok ? "" : "disabled";
    return `<tr><td>${i.index + 1}</td><td class="mono">${esc(i.root)}</td><td>${st}</td>
      <td><button class="primary sm" data-launch="${i.index}" ${dis}>${tr("Launch")}</button></td></tr>`;
  });
  const html = rows.join("");
  if ($("installsBody").dataset.html !== html) {
    $("installsBody").innerHTML = html;
    $("installsBody").dataset.html = html;
  }
}
$("installsBody").onclick = async (e) => {
  const i = e.target.dataset.launch;
  if (i === undefined) return;
  const slot = S.game_processes > 0 ? 1 : 0;
  $("gameOut").textContent = tr("launching install {n}…", { n: Number(i) + 1 });
  try {
    const log = await call("launch_client", { install: Number(i), slot, mode: $("g_mode").value });
    $("gameOut").textContent = (log ? tr("mutexes:") + "\n" + log + "\n" : "") + tr("launched");
  } catch (err) {
    $("gameOut").textContent = errText(err);
  }
  setTimeout(refresh, 1500);
};
$("killMutexBtn").onclick = async () => {
  $("gameOut").textContent = tr("killing mutexes…");
  $("gameOut").textContent = await call("killmutex").catch(errText);
};
$("listMutexBtn").onclick = async () => {
  $("gameOut").textContent = "…";
  $("gameOut").textContent = await call("list_mutexes").catch(errText);
};
document.addEventListener("click", async (e) => {
  const c = e.target.dataset?.ctl;
  if (!c) return;
  const [role, word] = c.split(":");
  await call("game_control", { role, word });
  toast(`${role}-control.txt = ${word}`, true);
});

// ------------------------------------------------------------------------------------------- host
function renderHost(h) {
  $("hostStatus").innerHTML = [
    [tr("phase"), h.phase || "—"],
    [tr("status written"), h.age_s >= 0 ? tr("{s} s ago", { s: h.age_s }) : tr("no file")],
    ["match-mode.txt", h.match_mode || tr("<none>")],
    ["status.json", h.raw || "—"],
  ]
    .map(([k, v]) => `<b>${esc(k)}</b><span>${esc(v)}</span>`)
    .join("");
}
$("hostStart").onclick = async () => {
  const opts = { map: $("h_map").value.trim(), window: $("h_window").checked, observer: $("h_observer").checked, probe: $("h_probe").checked, dsa: $("h_dsa").value.trim() };
  await call("host_start", { install: Number($("h_install").value), opts });
  toast(tr("server is starting — the phase is in \"status\""), true);
};
$("mmNative").onclick = () => call("host_match_mode", { native: true }).then((m) => toast("match-mode.txt = " + m, true)).then(refresh);
$("mmRestore").onclick = () => call("host_match_mode", { native: false }).then((m) => toast("match-mode.txt: " + m, true)).then(refresh);
// Commands to the game server go through the backend admin API, never into dbserver-control.txt directly.
const fmtAt = (at) => (typeof at === "number" ? hhmmss(at * 1000) : String(at ?? ""));
function renderHostLast(lc) {
  const rows = lc && typeof lc === "object"
    ? [[tr("last command"), lc.cmd], [tr("result"), typeof lc.result === "string" ? lc.result : JSON.stringify(lc.result)], [tr("when"), fmtAt(lc.at)]]
    : [[tr("last command"), "—"]];
  $("hcLast").innerHTML = rows.map(([k, v]) => `<b>${esc(k)}</b><span>${esc(v)}</span>`).join("");
}
async function loadHostStatus() {
  try {
    const s = await invoke("admin_host_status");
    const st = s.status && typeof s.status === "object" ? s.status : null;
    $("hcState").textContent = st ? tr("backend sees the server: {phase}{map}", { phase: st.phase || "?", map: st.map ? " · " + st.map : "" }) : tr("backend does not see the server ({root})", { root: s.root || "?" });
    renderHostLast(s.last_command);
  } catch (e) {
    $("hcState").textContent = tr("admin API: ") + (e?.code || e);
  }
}
async function loadHostPlayers() {
  try {
    const list = await invoke("admin_players");
    const keep = $("hcPlayer").value;
    $("hcPlayer").innerHTML = `<option value="">${tr("— online player ({n}) —", { n: list.length })}</option>` +
      list.map((p) => `<option value="${esc(p.open_id)}">${esc(p.nick || "?")} · ${esc(p.open_id)}</option>`).join("");
    $("hcPlayer").value = list.some((p) => p.open_id === keep) ? keep : "";
  } catch (e) {
    $("hcPlayer").innerHTML = `<option value="">${esc(errText(e))}</option>`;
  }
}
async function hostCommand(cmd) {
  $("hcOut").textContent = "> " + tr("{cmd} (waiting up to 10 s for the mod's answer)…", { cmd });
  try {
    const r = await invoke("admin_host_command", { cmd });
    const res = r.result;
    const text = res && typeof res === "object" ? `${res.result}` : cmd === "probe" ? tr("sent (probe writes no result)") : tr("the mod did not answer within 10 s");
    $("hcOut").textContent = `> ${r.sent}\n${tr("phase: {phase} · map: {map}", { phase: r.host_phase ?? "?", map: r.map ?? "?" })}\n${text}`;
    if (res && res.result === "rejected_not_ready") toast(tr("the server is not in phase ready — command rejected"));
  } catch (e) {
    $("hcOut").textContent = `> ${cmd}\n` + (e?.code === "admin.host_unavailable" ? tr("503: the backend did not find the server (no dbserver-status.json in DB_HOST_ROOT)") : errText(e));
  }
  loadHostStatus();
}
document.querySelectorAll("[data-hc]").forEach((b) => (b.onclick = () => hostCommand(b.dataset.hc)));
$("hcPortalOne").onclick = () => {
  const oid = $("hcPlayer").value;
  if (!oid) return toast(tr("pick a player"));
  hostCommand(`portal ${oid}`);
};
$("hcPlayersBtn").onclick = loadHostPlayers;

$("agentsBtn").onclick = async () => {
  try {
    $("agentsOut").textContent = JSON.stringify(await invoke("ds_agents"), null, 2);
  } catch (e) {
    $("agentsOut").textContent = errText(e);
  }
};

// ------------------------------------------------------------------------------------------- GM
let gmTarget = null; // {open_id, nick}
let players = [];
const gmHistory = [];
let gmHistPos = 0;

function gmLine(text, cls) {
  const d = document.createElement("div");
  if (cls) d.className = cls;
  d.textContent = text;
  $("gmOut").appendChild(d);
  $("gmOut").scrollTop = 1e9;
}

async function adminHealth() {
  try {
    const h = await invoke("admin_health");
    $("adminState").textContent = `${h.mock ? tr("mock") : tr("backend")} ${h.server_id || ""} · require = ${h.auth_require || "?"}${S?.admin_token_set ? tr(" · token set") : ""}`;
    $("adminState").className = "tag";
  } catch (e) {
    const c = e?.code || "";
    $("adminState").textContent = c === "admin.not_found" || c === "admin.player_not_found" || c === "http_status" ? tr("backend (no /health: require shows up on a 401)") : tr("no answer: ") + (e?.detail || c);
  }
}

async function loadPlayers() {
  try {
    players = await invoke("admin_players");
  } catch (e) {
    $("playersBody").innerHTML = `<tr><td colspan="5" class="muted">${esc(errText(e))}</td></tr>`;
    return;
  }
  const now = Date.now() / 1000;
  $("playersBody").innerHTML = players.length
    ? players
        .map((p) => {
          const since = p.online_since ? tr("{n} min", { n: Math.max(0, Math.round((now - p.online_since) / 60)) }) : "";
          return `<tr data-oid="${esc(p.open_id)}" class="${gmTarget?.open_id === p.open_id ? "sel" : ""}">
            <td>${esc(p.nick || "—")}</td><td class="mono">${esc(p.open_id)}</td><td class="mono">${p.user_id || ""}</td>
            <td>${esc((p.roles || []).join(", "))}</td><td>${since}${p.in_battle ? tr(" · match") : ""}</td></tr>`;
        })
        .join("")
    : `<tr><td colspan="5" class="muted">${tr("nobody online")}</td></tr>`;
}
$("playersBody").onclick = (e) => {
  const tr = e.target.closest("tr[data-oid]");
  if (!tr) return;
  const p = players.find((x) => x.open_id === tr.dataset.oid);
  gmTarget = { open_id: p.open_id, nick: p.nick };
  $("gmTarget").textContent = `${p.nick || "?"} (${p.open_id})`;
  for (const r of $("playersBody").children) r.classList.toggle("sel", r === tr);
  $("gmCmd").focus();
};
$("playersBtn").onclick = loadPlayers;

async function gmRun(cmd) {
  cmd = cmd.trim();
  if (!cmd) return;
  gmHistory.push(cmd);
  gmHistPos = gmHistory.length;
  const targetless = /^[/.]?(help|\?|ban|unban)\b/i.test(cmd);
  // help/ban/unban need no target, but the backend runs every command in some player's context
  const oid = gmTarget?.open_id || (targetless && players[0]?.open_id) || "";
  if (!oid && players.length !== 1) return toast(players.length ? tr("pick a player in the online list") : tr("nobody online — the backend has no player to run the command for"));
  gmLine(`> [${gmTarget ? gmTarget.nick || oid : players.length === 1 ? tr("the only one online") : "-"}] ${cmd}`, "cmd");
  try {
    const r = await invoke("admin_gm", { openId: oid, cmd });
    const where = r.open_id && r.open_id !== oid ? ` → ${r.open_id}` : "";
    const off = r.online === false ? tr(" (offline: DB only)") : "";
    if (r.result_code === 0) gmLine(`ok${where}${off}`, "good");
    else gmLine(tr("code {code}: {message}", { code: r.result_code, message: r.result_message }) + where, "bad");
    for (const l of r.result_list) gmLine("  " + l);
  } catch (e) {
    gmLine(errText(e), "bad");
  }
  if (/^[/.]?(ban|unban)\b/i.test(cmd)) loadPlayers();
}
$("gmForm").onsubmit = (e) => {
  e.preventDefault();
  gmRun($("gmCmd").value);
  $("gmCmd").value = "";
};
$("gmCmd").onkeydown = (e) => {
  if (e.key === "ArrowUp" && gmHistPos > 0) $("gmCmd").value = gmHistory[--gmHistPos];
  else if (e.key === "ArrowDown") $("gmCmd").value = gmHistPos < gmHistory.length - 1 ? gmHistory[++gmHistPos] : ((gmHistPos = gmHistory.length), "");
  else return;
  e.preventDefault();
};
$("gmQuick").onclick = (e) => {
  const b = e.target;
  if (b.dataset.gm) gmRun(b.dataset.gm);
  else if (b.dataset.fill) {
    $("gmCmd").value = b.dataset.fill;
    $("gmCmd").focus();
  }
};
$("mockBtn").onclick = async () => {
  const port = await call("mock_start", { requireRelay: $("mockRelay").checked });
  gmLine(tr("mock admin API on 127.0.0.1:{port}{relay}", { port, relay: $("mockRelay").checked ? " (relay: Bearer mock-admin / mock-gm)" : "" }), "good");
  await refresh();
  adminHealth();
  loadPlayers();
};
$("mockStopBtn").onclick = () => call("mock_stop").then(refresh).then(adminHealth);
$("tokenBtn").onclick = async () => {
  await call("admin_set_token", { token: $("tokenIn").value });
  $("tokenIn").value = "";
  $("loginInfo").textContent = tr("token set by hand");
  await refresh();
  adminHealth();
  loadPlayers();
};
$("loginBtn").onclick = () => call("admin_login").then(() => ($("loginInfo").textContent = tr("login: opening the browser…")));
$("logoutBtn").onclick = async () => {
  await call("admin_logout");
  $("loginInfo").textContent = tr("no token");
  await refresh();
  adminHealth();
};
listen("admin-login", (e) => {
  const { state, text, user_code } = e.payload;
  $("loginInfo").textContent = (state === "error" ? tr("error: ") : "") + text + (user_code ? tr(" · code: {code}", { code: user_code }) : "");
  if (state === "ok") {
    refresh().then(adminHealth);
    loadPlayers();
  }
});
listen("mock-log", (e) => gmLine(e.payload, "muted"));

// ------------------------------------------------------------------------------------------- tools
function toolLine(text) {
  const d = document.createElement("div");
  d.textContent = text;
  $("toolOut").appendChild(d);
  while ($("toolOut").childElementCount > 20000) $("toolOut").firstChild.remove();
  $("toolOut").scrollTop = 1e9;
}
document.querySelectorAll("[data-tool]").forEach((b) => {
  b.onclick = async () => {
    $("toolOut").innerHTML = "";
    await call("tool_run", { name: b.dataset.tool, args: $("arg_" + b.dataset.tool).value });
  };
});
$("toolStop").onclick = () => call("tool_stop");
listen("tool-log", (e) => toolLine(e.payload.text));
document.querySelectorAll("[data-open]").forEach((b) => (b.onclick = () => call("open_path", { what: b.dataset.open })));

// ------------------------------------------------------------------------------------------- settings
const S_KEYS = ["backups_dir", "dbtools_dir", "admin_url", "web_url", "ds_agent_url", "python"];
function fillSettings() {
  for (const k of S_KEYS) $("s_" + k).value = S.settings[k] || "";
  for (let i = 0; i < 3; i++) $("s_install" + i).value = S.settings.installs[i] || "";
}
$("saveSettings").onclick = async () => {
  for (const k of S_KEYS) S.settings[k] = $("s_" + k).value.trim();
  const inst = [0, 1, 2].map((i) => $("s_install" + i).value.trim());
  while (inst.length > 1 && !inst[inst.length - 1]) inst.pop();
  S.settings.installs = inst;
  await call("save_settings", { settings: S.settings });
  toast(tr("saved"), true);
  formDirty = false;
  await refresh();
};
// UI language (i18n.js): kept in localStorage, applied on reload
$("uiLang").value = LANG;
$("uiLang").onchange = () => setLang($("uiLang").value);

// ------------------------------------------------------------------------------------------- tabs, timers
let tab = "backend";
$("tabs").onclick = (e) => {
  const t = e.target.dataset.tab;
  if (!t) return;
  tab = t;
  for (const b of $("tabs").children) b.classList.toggle("on", b.dataset.tab === t);
  for (const s of document.querySelectorAll(".tab")) s.classList.toggle("on", s.id === "tab-" + t);
  if (t === "settings") fillSettings();
  if (t === "gm") {
    adminHealth();
    loadPlayers();
  }
  if (t === "host") {
    $("agentsBtn").onclick();
    loadHostStatus();
    loadHostPlayers();
  }
};

(async function init() {
  await refresh();
  fillSettings();
  invoke("list_tomls").then((l) => ($("tomlList").innerHTML = l.map((t) => `<option value="${esc(t)}">`).join("")));
  appendLog(await invoke("backend_log_tail", { n: 5000 }));
  checkPorts();
  setInterval(refresh, 3000);
  setInterval(() => {
    if (tab === "gm" && $("playersAuto").checked) loadPlayers();
    if (tab === "host") loadHostStatus();
  }, 5000);
})();
