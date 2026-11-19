//! Dungeonborne Dev: developer tool for the server emulator (not for players). Flow: backend → game server
//! (Host Game) → clients, plus the GM console over the admin API and quick tools of the backups repo.

pub mod admin;
pub mod backend;
pub mod clients;
pub mod config;
pub mod host;
pub mod mock;
pub mod ports;
pub mod tools;
pub mod winx;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use dbl::error::{Error, Result};

use config::{ClientMode, HostMode, Profile, Settings};

const CLIENT_GAP: Duration = Duration::from_secs(25);
const BACKEND_READY_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const SERVER_READY_TIMEOUT: Duration = Duration::from_secs(180);

pub struct App {
    http: reqwest::Client,
    settings: Mutex<Settings>,
    backend: Mutex<Option<backend::Running>>,
    log: Arc<Mutex<backend::LogBuf>>,
    lock_ours: AtomicBool,
    admin_token: Mutex<Option<String>>,
    admin_account: Mutex<Option<admin::AdminAccount>>,
    login_task: Mutex<Option<tokio::task::AbortHandle>>,
    mock: Mutex<Option<(mock::Shared, tokio::task::JoinHandle<()>)>>,
    flow: Mutex<Option<tokio::task::AbortHandle>>,
    tool: Mutex<Option<tokio::task::AbortHandle>>,
}

impl App {
    fn load() -> Self {
        Self {
            http: dbl::net::client(),
            settings: Mutex::new(Settings::load()),
            backend: Mutex::new(None),
            log: Arc::new(Mutex::new(backend::LogBuf::default())),
            lock_ours: AtomicBool::new(false),
            admin_token: Mutex::new(None),
            admin_account: Mutex::new(None),
            login_task: Mutex::new(None),
            mock: Mutex::new(None),
            flow: Mutex::new(None),
            tool: Mutex::new(None),
        }
    }

    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn profile(&self, index: usize) -> Result<Profile> {
        self.settings.lock().unwrap().profiles.get(index).cloned().ok_or_else(|| Error::new("profile_unknown", index.to_string()))
    }

    fn install_root(&self, index: usize) -> Result<String> {
        self.settings.lock().unwrap().installs.get(index).cloned().ok_or_else(|| Error::new("install_unknown", (index + 1).to_string()))
    }
}

fn internal(e: impl std::fmt::Display) -> Error {
    Error::new("internal", e.to_string())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f).await.map_err(internal)?
}

// ---------------------------------------------------------------------------------------------- events

#[derive(Clone, Serialize)]
struct FlowEvent<'a> {
    step: &'a str,
    state: &'a str,
    text: String,
}

fn flow(app: &AppHandle, step: &str, state: &str, text: impl Into<String>) {
    let _ = app.emit("flow", FlowEvent { step, state, text: text.into() });
}

fn emit_backend_state(app: &AppHandle) {
    let _ = app.emit("backend-state", backend_view(&app.state::<App>()));
}

// ---------------------------------------------------------------------------------------------- state

#[derive(Clone, Serialize)]
struct BackendView {
    running: bool,
    pid: u32,
    profile: String,
    started_ms: i64,
    lock_ours: bool,
}

fn backend_view(st: &App) -> BackendView {
    let b = st.backend.lock().unwrap();
    BackendView {
        running: b.is_some(),
        pid: b.as_ref().map_or(0, |r| r.pid),
        profile: b.as_ref().map(|r| r.profile.clone()).unwrap_or_default(),
        started_ms: b.as_ref().map_or(0, |r| r.started_ms),
        lock_ours: st.lock_ours.load(Ordering::SeqCst),
    }
}

#[derive(Serialize)]
struct StateView {
    version: &'static str,
    elevated: bool,
    dbtools_task: bool,
    settings: Settings,
    backend: BackendView,
    lock: Option<ports::LockInfo>,
    installs: Vec<clients::InstallState>,
    game_processes: usize,
    host: host::ServerStatus,
    admin_account: Option<admin::AdminAccount>,
    admin_token_set: bool,
    mock: Option<MockView>,
    commands: Vec<String>,
}

#[derive(Serialize)]
struct MockView {
    port: u16,
    require_relay: bool,
}

#[tauri::command]
async fn get_state(app: AppHandle) -> Result<StateView> {
    let app2 = app.clone();
    blocking(move || {
        let st = app2.state::<App>();
        let settings = st.settings();
        let installs = settings.installs.iter().enumerate().map(|(i, r)| clients::install_state(i, r)).collect();
        let mock = st.mock.lock().unwrap().as_ref().map(|(m, _)| {
            let m = m.lock().unwrap();
            MockView { port: m.port, require_relay: m.require_relay }
        });
        let view = StateView {
            version: env!("CARGO_PKG_VERSION"),
            elevated: winx::is_elevated(),
            dbtools_task: clients::dbtools_registered(),
            backend: backend_view(&st),
            lock: ports::lock_status(),
            installs,
            game_processes: clients::game_processes(),
            host: host::status(&settings.dbtools()),
            admin_account: st.admin_account.lock().unwrap().clone(),
            admin_token_set: st.admin_token.lock().unwrap().is_some(),
            mock,
            commands: settings.profiles.iter().map(backend::command_line).collect(),
            settings,
        };
        Ok(view)
    })
    .await
}

#[tauri::command]
fn save_settings(st: State<'_, App>, settings: Settings) -> Result<()> {
    settings.save()?;
    *st.settings.lock().unwrap() = settings;
    Ok(())
}

/// `SERVER_TOML` candidates relative to backups: `probe/auth_test/*.toml` and `server*.toml` in the root.
#[tauri::command]
fn list_tomls(st: State<'_, App>) -> Vec<String> {
    let b = st.settings().backups();
    let mut out = Vec::new();
    for (dir, prefix) in [(b.join("probe").join("auth_test"), "probe/auth_test/"), (b.clone(), "")] {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".toml") && (!prefix.is_empty() || (n.starts_with("server") && !n.contains("example"))))
            .collect();
        names.sort();
        out.extend(names.into_iter().map(|n| format!("{prefix}{n}")));
    }
    out
}

#[derive(Serialize)]
struct PortsView {
    ports: Vec<ports::PortState>,
    lock: Option<ports::LockInfo>,
}

#[tauri::command]
async fn check_ports(app: AppHandle) -> Result<PortsView> {
    blocking(move || {
        let st = app.state::<App>();
        let job = st.backend.lock().unwrap().as_ref().map(|r| r.job.clone());
        Ok(PortsView { ports: ports::check(job.as_deref()), lock: ports::lock_status() })
    })
    .await
}

// ---------------------------------------------------------------------------------------------- backend

/// Ports this profile's backend will bind: web API and relay only with a server.toml.
fn needed_ports(s: &Settings, p: &Profile) -> Vec<u16> {
    let toml = !p.server_toml.trim().is_empty() || s.backups().join("server.toml").is_file();
    ports::BACKEND_PORTS.iter().map(|&(port, _)| port).filter(|&port| toml || !matches!(port, 8090 | 7443)).collect()
}

fn start_backend(app: &AppHandle, index: usize) -> Result<()> {
    let st = app.state::<App>();
    if st.backend.lock().unwrap().is_some() {
        return Err(Error::new("backend_running", "the backend is already running — Stop it first"));
    }
    let s = st.settings();
    let p = st.profile(index)?;
    let needed = needed_ports(&s, &p);
    let busy: Vec<String> = ports::check(None)
        .into_iter()
        .filter(|ps| ps.busy && needed.contains(&ps.port))
        // PVE sniffer runs next to SoloLobby on 6601-6604 by design
        .filter(|ps| !(p.pve_sniff && (6601..=6604).contains(&ps.port) && ps.process.eq_ignore_ascii_case("SoloLobby.exe")))
        .map(|ps| format!("{} ({}): {}", ps.port, ps.role, ps.who))
        .collect();
    if !busy.is_empty() {
        return Err(Error::new("ports_busy", busy.join("\n")));
    }
    let lock = ports::acquire(&s.backups(), &format!("dev-app: {}", p.name))?;
    st.lock_ours.store(true, Ordering::SeqCst);
    {
        let mut buf = st.log.lock().unwrap();
        buf.push(false, format!("[dev-app] {} — {}", lock, backend::command_line(&p)));
    }
    let sink_app = app.clone();
    let sink: Arc<dyn Fn(Vec<backend::LogLine>) + Send + Sync> = Arc::new(move |lines| {
        let _ = sink_app.emit("backend-log", lines);
    });
    let spawned = match backend::spawn(&s.backups(), &p, st.log.clone(), sink) {
        Ok(sp) => sp,
        Err(e) => {
            let _ = ports::release(&s.backups());
            st.lock_ours.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    *st.backend.lock().unwrap() = Some(spawned.running);
    emit_backend_state(app);
    let app2 = app.clone();
    let backups = s.backups();
    tokio::spawn(async move {
        let code = spawned.exited.await.ok().flatten();
        let st = app2.state::<App>();
        st.backend.lock().unwrap().take();
        let mut note = format!("[dev-app] backend exited (code {})", code.map_or("?".into(), |c| c.to_string()));
        if st.lock_ours.swap(false, Ordering::SeqCst) {
            let r = tokio::task::spawn_blocking(move || ports::release(&backups)).await;
            note.push_str(&format!("; port lock: {}", r.ok().and_then(|r| r.ok()).unwrap_or_else(|| "release failed".into())));
        }
        let line = st.log.lock().unwrap().push(false, note);
        let _ = app2.emit("backend-log", vec![line]);
        emit_backend_state(&app2);
    });
    Ok(())
}

#[tauri::command]
async fn backend_start(app: AppHandle, profile: usize) -> Result<()> {
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || start_backend(&app2, profile)).await.map_err(internal)?
}

fn stop_backend(st: &App) -> bool {
    match st.backend.lock().unwrap().as_ref() {
        Some(r) => {
            r.job.terminate();
            true
        }
        None => false,
    }
}

#[tauri::command]
fn backend_stop(st: State<'_, App>) -> bool {
    stop_backend(&st)
}

#[tauri::command]
fn backend_log_tail(st: State<'_, App>, n: usize) -> Vec<backend::LogLine> {
    st.log.lock().unwrap().tail(n.min(20_000))
}

#[tauri::command]
fn backend_log_clear(st: State<'_, App>) {
    st.log.lock().unwrap().clear();
}

/// Ready = the Dir port (8085) is held by a process of our backend job.
async fn wait_backend_ready(app: &AppHandle) -> Result<()> {
    let deadline = Instant::now() + BACKEND_READY_TIMEOUT;
    loop {
        let job = app.state::<App>().backend.lock().unwrap().as_ref().map(|r| r.job.clone());
        let Some(job) = job else { return Err(Error::new("backend_exited", "the backend exited before it was ready — see the output")) };
        let ready = tokio::task::spawn_blocking(move || dbl::winutil::port_owner(8085).is_some_and(|(pid, _)| job.contains(pid)))
            .await
            .unwrap_or(false);
        if ready {
            tokio::time::sleep(Duration::from_secs(1)).await;
            return Ok(());
        }
        if Instant::now() > deadline {
            return Err(Error::new("backend_timeout", "the backend did not open 8085 within 15 min"));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

// ---------------------------------------------------------------------------------------------- game

fn launch_client_blocking(app: &AppHandle, install: usize, slot: usize, mode: &ClientMode) -> Result<String> {
    let st = app.state::<App>();
    let s = st.settings();
    let root = st.install_root(install)?;
    let mut log = String::new();
    if clients::game_processes() > 0 {
        log = clients::killmutex(&s.backups(), &s.dbtools())?;
    }
    let connect = matches!(mode, ClientMode::Connect).then_some("127.0.0.1:7777");
    clients::launch(&root, &clients::client_args(&s.dbtools(), slot, connect))?;
    Ok(log)
}

#[tauri::command]
async fn launch_client(app: AppHandle, install: usize, slot: usize, mode: ClientMode) -> Result<String> {
    blocking(move || launch_client_blocking(&app, install, slot, &mode)).await
}

#[tauri::command]
async fn killmutex(app: AppHandle) -> Result<String> {
    blocking(move || {
        let s = app.state::<App>().settings();
        clients::killmutex(&s.backups(), &s.dbtools())
    })
    .await
}

#[tauri::command]
async fn list_mutexes(app: AppHandle) -> Result<String> {
    blocking(move || {
        let s = app.state::<App>().settings();
        if !clients::dbtools_registered() {
            return Err(Error::new("dbtools_missing", "the mutex list is only available via the DbTools task (dbtest\\README_admin.md)"));
        }
        clients::dbtools_request(&s.dbtools(), r#"{"action":"list"}"#)
    })
    .await
}

#[tauri::command]
fn game_control(st: State<'_, App>, role: String, word: String) -> Result<()> {
    host::control(&st.settings().dbtools(), &role, &word)
}

// ---------------------------------------------------------------------------------------------- host game

fn start_server_blocking(app: &AppHandle, install: usize, opts: &clients::ServerOpts) -> Result<()> {
    let st = app.state::<App>();
    let s = st.settings();
    let root = st.install_root(install)?;
    host::reset_control(&s.dbtools())?;
    if clients::game_processes() > 0 {
        clients::killmutex(&s.backups(), &s.dbtools())?;
    }
    clients::launch(&root, &clients::server_args(&s.dbtools(), opts)?)
}

#[tauri::command]
async fn host_start(app: AppHandle, install: usize, opts: clients::ServerOpts) -> Result<()> {
    blocking(move || start_server_blocking(&app, install, &opts)).await
}

#[tauri::command]
fn host_status(st: State<'_, App>) -> host::ServerStatus {
    host::status(&st.settings().dbtools())
}

#[tauri::command]
fn host_match_mode(native: bool) -> Result<String> {
    if native {
        host::set_native()?;
    } else {
        host::restore_match_mode()?;
    }
    Ok(host::match_mode().unwrap_or_else(|| "<no file>".into()))
}

#[tauri::command]
async fn ds_agents(st: State<'_, App>) -> Result<serde_json::Value> {
    let base = st.settings().ds_agent_url;
    host::ds_agents(&st.http, &base).await
}

async fn wait_server_ready(app: &AppHandle, since: std::time::SystemTime) -> Result<()> {
    let dbtools = app.state::<App>().settings().dbtools();
    let status = dbtools.join("run").join("dbserver-status.json");
    let deadline = Instant::now() + SERVER_READY_TIMEOUT;
    let mut last = String::new();
    while Instant::now() < deadline {
        let fresh = std::fs::metadata(&status).and_then(|m| m.modified()).is_ok_and(|t| t > since);
        if fresh {
            let s = host::status(&dbtools);
            if s.raw != last {
                flow(app, "server", "progress", s.raw.clone());
                last = s.raw.clone();
            }
            match s.phase.as_str() {
                "ready" => return Ok(()),
                "ended" => return Err(Error::new("server_ended", "the server exited — see dbtools\\run\\dbserver.log")),
                _ => {}
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    Err(Error::new("server_timeout", "the server did not reach ready within 180 s"))
}

// ---------------------------------------------------------------------------------------------- one-button profile

async fn run_flow(app: AppHandle, index: usize) -> Result<()> {
    let st = app.state::<App>();
    let p = st.profile(index)?;
    flow(&app, "start", "progress", p.name.clone());

    if st.backend.lock().unwrap().is_some() {
        flow(&app, "backend", "ok", "the backend is already running — using it");
    } else {
        flow(&app, "backend", "progress", "ports, lock, cargo run…");
        let app2 = app.clone();
        tokio::task::spawn_blocking(move || start_backend(&app2, index)).await.map_err(internal)??;
    }
    flow(&app, "backend", "progress", "waiting until ready (8085)…");
    wait_backend_ready(&app).await?;
    flow(&app, "backend", "ok", "backend ready");

    if p.host != HostMode::None {
        if p.clients.contains(&p.host_install) {
            flow(&app, "server", "warn", "the client is from the same install as the server: they share one Goldberg ID");
        }
        let since = std::time::SystemTime::now();
        flow(&app, "server", "progress", "starting the server (UAC unless dev-app runs as administrator)");
        let opts = clients::ServerOpts { map: p.host_map.clone(), window: p.host_window, observer: p.host_observer, ..Default::default() };
        let (app2, install) = (app.clone(), p.host_install);
        blocking(move || start_server_blocking(&app2, install, &opts)).await?;
        wait_server_ready(&app, since).await?;
        flow(&app, "server", "ok", "server ready (127.0.0.1:7777)");
        if p.host == HostMode::Ds {
            host::set_native()?;
            flow(&app, "server", "ok", "match-mode.txt = native (to restore: Host Game → \"Restore match-mode\")");
        }
    }

    for (slot, &install) in p.clients.iter().enumerate() {
        if slot > 0 || p.host != HostMode::None {
            flow(&app, &format!("client{}", slot + 1), "progress", format!("pause {} s: the previous instance is creating its mutexes", CLIENT_GAP.as_secs()));
            tokio::time::sleep(CLIENT_GAP).await;
        }
        let step = format!("client{}", slot + 1);
        flow(&app, &step, "progress", format!("install {} ({:?})", install + 1, p.client_mode));
        let (app2, mode) = (app.clone(), p.client_mode.clone());
        let log = blocking(move || launch_client_blocking(&app2, install, slot, &mode)).await?;
        flow(&app, &step, "ok", if log.is_empty() { "launched".to_string() } else { format!("mutexes:\n{log}\nlaunched") });
    }
    flow(&app, "done", "ok", "done");
    Ok(())
}

#[tauri::command]
fn run_profile(app: AppHandle, st: State<'_, App>, profile: usize) -> Result<()> {
    let mut slot = st.flow.lock().unwrap();
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Err(Error::new("flow_running", "a scenario is already running"));
    }
    let app2 = app.clone();
    let h = tokio::spawn(async move {
        if let Err(e) = run_flow(app2.clone(), profile).await {
            flow(&app2, "done", "error", e.to_string());
        }
    });
    *slot = Some(h.abort_handle());
    Ok(())
}

#[tauri::command]
fn cancel_flow(app: AppHandle, st: State<'_, App>) {
    if let Some(h) = st.flow.lock().unwrap().take() {
        h.abort();
        flow(&app, "done", "error", "scenario stopped (whatever was launched keeps running)");
    }
}

// ---------------------------------------------------------------------------------------------- GM console

fn admin_token(st: &App) -> Option<String> {
    st.admin_token.lock().unwrap().clone()
}

#[tauri::command]
async fn admin_health(st: State<'_, App>) -> Result<admin::Health> {
    let base = st.settings().admin_url;
    admin::Client { http: &st.http, base: &base, token: None }.health().await
}

#[tauri::command]
async fn admin_players(st: State<'_, App>) -> Result<Vec<admin::Player>> {
    let (base, token) = (st.settings().admin_url, admin_token(&st));
    admin::Client { http: &st.http, base: &base, token: token.as_deref() }.players().await
}

#[tauri::command]
async fn admin_gm(st: State<'_, App>, open_id: String, cmd: String) -> Result<admin::GmResult> {
    let (base, token) = (st.settings().admin_url, admin_token(&st));
    admin::Client { http: &st.http, base: &base, token: token.as_deref() }.gm(&open_id, &cmd).await
}

#[tauri::command]
async fn admin_host_status(st: State<'_, App>) -> Result<admin::HostStatus> {
    let (base, token) = (st.settings().admin_url, admin_token(&st));
    admin::Client { http: &st.http, base: &base, token: token.as_deref() }.host_status().await
}

/// Host commands go dev-app → backend → game server; dev-app never writes them into dbserver-control.txt.
#[tauri::command]
async fn admin_host_command(st: State<'_, App>, cmd: String) -> Result<admin::HostCommandResult> {
    let (base, token) = (st.settings().admin_url, admin_token(&st));
    admin::Client { http: &st.http, base: &base, token: token.as_deref() }.host_command(&cmd).await
}

#[tauri::command]
fn admin_set_token(st: State<'_, App>, token: String) {
    let token = token.trim().to_string();
    *st.admin_token.lock().unwrap() = (!token.is_empty()).then_some(token);
    *st.admin_account.lock().unwrap() = None;
}

#[derive(Clone, Serialize)]
struct LoginEvent {
    state: &'static str,
    text: String,
    user_code: Option<String>,
}

#[tauri::command]
fn admin_login(app: AppHandle, st: State<'_, App>) -> Result<()> {
    if let Some(h) = st.login_task.lock().unwrap().take() {
        h.abort();
    }
    let web = st.settings().web_url;
    let app2 = app.clone();
    let h = tokio::spawn(async move {
        let st = app2.state::<App>();
        let res: Result<()> = async {
            let l = admin::login_start(&st.http, &web).await?;
            // Saved token of this server first: no browser if it is still alive.
            if let Ok(Some(saved)) = dbl::creds::load(&admin::cred_key(&l.server_id)) {
                if dbl::auth::me(&st.http, &l.api, &saved).await.is_ok() {
                    *st.admin_token.lock().unwrap() = Some(saved);
                    let _ = app2.emit("admin-login", LoginEvent { state: "ok", text: format!("saved token {}", l.server_id), user_code: None });
                    return Ok(());
                }
            }
            dbl::winutil::open_url(&l.browser_url)?;
            let _ = app2.emit("admin-login", LoginEvent { state: "pending", text: "log in in the browser".into(), user_code: l.user_code.clone() });
            let (token, acc) = admin::login_wait(&st.http, &l).await?;
            let _ = dbl::creds::store(&admin::cred_key(&l.server_id), &token);
            let is_admin = acc.roles.iter().any(|r| r == "admin");
        let text = format!("{} ({}) roles: {:?}{}", acc.display_name, acc.open_id, acc.roles, if is_admin { "" } else { " — no admin role, the API will answer 403" });
            *st.admin_token.lock().unwrap() = Some(token);
            *st.admin_account.lock().unwrap() = Some(acc);
            let _ = app2.emit("admin-login", LoginEvent { state: "ok", text, user_code: None });
            Ok(())
        }
        .await;
        if let Err(e) = res {
            let _ = app2.emit("admin-login", LoginEvent { state: "error", text: e.to_string(), user_code: None });
        }
    });
    *st.login_task.lock().unwrap() = Some(h.abort_handle());
    Ok(())
}

#[tauri::command]
fn admin_logout(st: State<'_, App>) {
    if let Some(acc) = st.admin_account.lock().unwrap().take() {
        dbl::creds::delete(&admin::cred_key(&acc.server_id));
    }
    *st.admin_token.lock().unwrap() = None;
}

#[tauri::command]
async fn mock_start(app: AppHandle, st: State<'_, App>, require_relay: bool) -> Result<u16> {
    if let Some((_, h)) = st.mock.lock().unwrap().take() {
        h.abort();
    }
    let port = dbl::net::check_url(&st.settings().admin_url)?.port().unwrap_or(9092);
    let app2 = app.clone();
    let shared = mock::Mock::new(port, require_relay, Arc::new(move |line| {
        let _ = app2.emit("mock-log", line);
    }));
    let h = mock::serve(shared.clone()).await.map_err(|e| dbl::winutil::port_busy_error(port, e))?;
    *st.mock.lock().unwrap() = Some((shared, h));
    Ok(port)
}

#[tauri::command]
fn mock_stop(st: State<'_, App>) -> bool {
    st.mock.lock().unwrap().take().map(|(_, h)| h.abort()).is_some()
}

// ---------------------------------------------------------------------------------------------- tools

#[derive(Clone, Serialize)]
struct ToolEvent {
    text: String,
    done: bool,
}

#[tauri::command]
fn tool_run(app: AppHandle, st: State<'_, App>, name: String, args: String) -> Result<()> {
    let mut slot = st.tool.lock().unwrap();
    if slot.as_ref().is_some_and(|h| !h.is_finished()) {
        return Err(Error::new("tool_running", "a tool is already running — Stop"));
    }
    let s = st.settings();
    let mut child = tools::spawn(&s.python, &s.backups(), &name, &args)?;
    let app2 = app.clone();
    let _ = app.emit("tool-log", ToolEvent { text: format!("> {} -I tools/{name}.py {args}", s.python), done: false });
    let h = tokio::spawn(async move {
        let sink_app = app2.clone();
        tools::pump(&mut child, move |text| {
            let _ = sink_app.emit("tool-log", ToolEvent { text, done: false });
        })
        .await;
        let code = child.wait().await.ok().and_then(|s| s.code());
        let _ = app2.emit("tool-log", ToolEvent { text: format!("[exit code {}]", code.map_or("?".into(), |c| c.to_string())), done: true });
    });
    *slot = Some(h.abort_handle());
    Ok(())
}

#[tauri::command]
fn tool_stop(app: AppHandle, st: State<'_, App>) {
    // Aborting the task drops the child (kill_on_drop).
    if let Some(h) = st.tool.lock().unwrap().take() {
        h.abort();
        let _ = app.emit("tool-log", ToolEvent { text: "[stopped]".into(), done: true });
    }
}

#[tauri::command]
fn open_path(st: State<'_, App>, what: String) -> Result<()> {
    let s = st.settings();
    let path = match what.as_str() {
        "our_jsonl" => s.backups().join("rpc_dump").join("OUR.jsonl"),
        "rpc_dump" => s.backups().join("rpc_dump"),
        "backups" => s.backups(),
        "dbtools_logs" => s.dbtools().join("logs"),
        "dbtools_run" => s.dbtools().join("run"),
        "probe_logs" => s.backups().join("probe").join("logs"),
        "settings" => config::data_dir(),
        _ => return Err(Error::new("bad_path", what)),
    };
    winx::open_path(&path)
}

// ---------------------------------------------------------------------------------------------- run

pub fn run() {
    tauri::Builder::default()
        .manage(App::load())
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            list_tomls,
            check_ports,
            backend_start,
            backend_stop,
            backend_log_tail,
            backend_log_clear,
            launch_client,
            killmutex,
            list_mutexes,
            game_control,
            host_start,
            host_status,
            host_match_mode,
            ds_agents,
            run_profile,
            cancel_flow,
            admin_health,
            admin_players,
            admin_gm,
            admin_host_status,
            admin_host_command,
            admin_set_token,
            admin_login,
            admin_logout,
            mock_start,
            mock_stop,
            tool_run,
            tool_stop,
            open_path,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                let st = app.state::<App>();
                stop_backend(&st);
                if st.lock_ours.swap(false, Ordering::SeqCst) {
                    let _ = ports::release(&st.settings().backups());
                }
            }
        });
}

/// `--mock-admin [--port N] [--require relay]`: only the admin API mock, until Ctrl+C (for tests and the
/// backend session that implements ADMIN_API.md).
pub fn mock_main(args: &[String]) -> i32 {
    let get = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let port = get("--port").and_then(|p| p.parse().ok()).unwrap_or(9092);
    let require_relay = get("--require").is_some_and(|r| r == "relay");
    let rt = tokio::runtime::Runtime::new().expect("tokio");
    rt.block_on(async move {
        let shared = mock::Mock::new(port, require_relay, Arc::new(|line| println!("{line}")));
        match mock::serve(shared).await {
            Ok(h) => {
                println!("admin API mock: http://127.0.0.1:{port}/admin/v1 (require = {})", if require_relay { "relay" } else { "none" });
                let _ = tokio::signal::ctrl_c().await;
                h.abort();
                0
            }
            Err(e) => {
                eprintln!("bind 127.0.0.1:{port}: {e}");
                1
            }
        }
    })
}
