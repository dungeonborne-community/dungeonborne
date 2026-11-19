//! Dungeonborne launcher: Tauri commands and the play flow (check -> mods -> profile -> relay -> game -> cleanup).

pub mod auth;
pub mod bootstrap;
pub mod bugreport;
pub mod builtin;
pub mod bundle;
pub mod creds;
pub mod error;
pub mod game;
pub mod hosts;
pub mod locate;
pub mod manifest;
pub mod net;
pub mod netblock;
pub mod netcheck;
pub mod notes;
pub mod profile;
pub mod relay;
pub mod store;
pub mod support;
pub mod update;
pub mod winutil;

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use error::{bail, Error, Result};
use manifest::{Manifest, Status};
use profile::Install;
use store::{Account, ServerEntry, Settings};

const REFRESH_BEFORE_S: i64 = 3600;
/// Restart after a self-update: wait for the old process to release the single-instance mutex.
pub const WAIT_INSTANCE_FLAG: &str = "--wait-instance";
const RELAY_LINGER: Duration = Duration::from_secs(10);
/// Pause between the game exiting and the next start (ACE, see play_flow). 05.11: 30 s, user's choice.
const GAME_RESTART_COOLDOWN_MS: u64 = 30_000;

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Whole seconds still to wait before starting the game (0 = go). A last exit in the future (clock moved back) — no wait.
fn restart_cooldown_left(last_exit_ms: u64, now: u64) -> u64 {
    if last_exit_ms == 0 || last_exit_ms > now {
        return 0;
    }
    GAME_RESTART_COOLDOWN_MS.saturating_sub(now - last_exit_ms).div_ceil(1000)
}

struct RelayRun {
    server_id: String,
    relay: relay::Relay,
    users: usize,
    generation: u64,
}

pub struct App {
    http: reqwest::Client,
    settings: Mutex<Settings>,
    servers: Mutex<Vec<ServerEntry>>,
    exe_hash: Mutex<HashMap<PathBuf, (u64, SystemTime, String)>>,
    logins: Mutex<HashMap<String, tokio::task::AbortHandle>>,
    busy: Mutex<HashSet<usize>>,
    relay: Arc<tokio::sync::Mutex<Option<RelayRun>>>,
    /// Network check (`netblock`): when it last ran per server, and what it found
    net_probe: Mutex<HashMap<String, i64>>,
    net_verdicts: Mutex<HashMap<String, netblock::Verdict>>,
    /// Started in place of an install's bootstrap: that install slot and Steam's arguments for the game.
    boot_install: Option<usize>,
    game_args: Vec<String>,
}

impl App {
    fn load() -> Self {
        let mut servers: Vec<ServerEntry> = store::load(&store::servers_path());
        for s in &mut servers {
            s.account = store::load_profile(&s.manifest.id);
        }
        let mut settings: Settings = store::load(&store::settings_path());
        bootstrap::keep_marker();
        let boot_root = bootstrap::running_as_bootstrap();
        if one_install(&mut settings, boot_root.as_deref()) {
            let _ = store::save(&store::settings_path(), &settings);
        }
        let boot_install = boot_root.map(|_| 0);
        let game_args = if boot_install.is_some() {
            std::env::args().skip(1).filter(|a| a != WAIT_INSTANCE_FLAG).collect()
        } else {
            Vec::new()
        };
        Self {
            http: net::client(),
            settings: Mutex::new(settings),
            servers: Mutex::new(servers),
            exe_hash: Mutex::new(HashMap::new()),
            logins: Mutex::new(HashMap::new()),
            busy: Mutex::new(HashSet::new()),
            relay: Arc::new(tokio::sync::Mutex::new(None)),
            net_probe: Mutex::new(HashMap::new()),
            net_verdicts: Mutex::new(HashMap::new()),
            boot_install,
            game_args,
        }
    }

    fn server(&self, id: &str) -> Result<ServerEntry> {
        self.servers.lock().unwrap().iter().find(|s| s.manifest.id == id).cloned().ok_or_else(|| Error::new("server_unknown", id))
    }

    fn update_server(&self, id: &str, f: impl FnOnce(&mut ServerEntry)) -> Result<()> {
        let mut servers = self.servers.lock().unwrap();
        let Some(e) = servers.iter_mut().find(|s| s.manifest.id == id) else { bail!("server_unknown", "{id}") };
        f(e);
        store::save(&store::servers_path(), &*servers)
    }

    fn set_account(&self, id: &str, account: Option<Account>) -> Result<()> {
        store::save_profile(id, account.as_ref())?;
        let mut servers = self.servers.lock().unwrap();
        if let Some(e) = servers.iter_mut().find(|s| s.manifest.id == id) {
            e.account = account;
        }
        Ok(())
    }

    fn install(&self, index: usize) -> Result<Install> {
        let root = self.settings.lock().unwrap().installs.get(index).cloned().unwrap_or_default();
        Install::open(&root)
    }

    async fn exe_sha256(&self, inst: &Install) -> Result<String> {
        let exe = inst.exe();
        let md = std::fs::metadata(&exe)?;
        let stamp = (md.len(), md.modified()?);
        if let Some((len, mtime, h)) = self.exe_hash.lock().unwrap().get(&exe) {
            if (*len, *mtime) == stamp {
                return Ok(h.clone());
            }
        }
        let exe2 = exe.clone();
        let h = tokio::task::spawn_blocking(move || profile::sha256_file(&exe2)).await.map_err(internal)??;
        self.exe_hash.lock().unwrap().insert(exe, (stamp.0, stamp.1, h.clone()));
        Ok(h)
    }
}

fn internal(e: impl std::fmt::Display) -> Error {
    Error::new("internal", e.to_string())
}

// ---------------------------------------------------------------------------------------------- events & logs

#[derive(Clone, Serialize)]
struct LogEvent<'a> {
    scope: &'a str,
    level: &'a str,
    text: String,
}

fn append_log(file: &str, line: &str) {
    let dir = store::data_dir().join("logs");
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(file)) {
        let _ = writeln!(f, "{} {line}", net::unix_now());
    }
}

fn log(app: &AppHandle, scope: &str, level: &str, text: impl Into<String>) {
    let text = text.into();
    append_log("launcher.log", &format!("[{scope}] {level}: {text}"));
    let _ = app.emit("log", LogEvent { scope, level, text });
}

#[derive(Clone, Serialize)]
struct PlayEvent<'a> {
    install: usize,
    step: &'a str,
    state: &'a str,
    text: String,
    /// The run failed: the error as a code the UI translates (`text` holds the same, untranslated, for the log)
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
}

fn step(app: &AppHandle, install: usize, step: &str, state: &str, text: impl Into<String>) {
    let text = text.into();
    log(app, &format!("play{}", install + 1), "info", format!("{step} {state} {text}"));
    let _ = app.emit("play", PlayEvent { install, step, state, text, error: None });
}

#[derive(Clone, Serialize)]
struct LoginEvent<'a> {
    server_id: &'a str,
    state: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_code: Option<String>,
}

// ---------------------------------------------------------------------------------------------- views

#[derive(Serialize)]
struct ServerView {
    url: String,
    manifest: Manifest,
    key_changed: bool,
    trusted_key: Option<String>,
    account: Option<Account>,
    logged_in: bool,
}

#[derive(Serialize)]
struct StateView {
    version: &'static str,
    settings: Settings,
    servers: Vec<ServerView>,
    hosts_installed: bool,
    /// Lines of older launcher versions are still in hosts (1.7.0 needs none): the settings offer to remove them
    hosts_leftover: bool,
    hosts_path: String,
    /// install slot this launcher was started for (in place of its bootstrap), to preselect "Play"
    boot_install: Option<usize>,
}

fn view(e: &ServerEntry) -> ServerView {
    let logged_in = e.account.as_ref().is_some_and(|a| a.expires_at > net::unix_now())
        && creds::load(&e.manifest.id).ok().flatten().is_some();
    ServerView {
        url: e.url.clone(),
        manifest: e.manifest.clone(),
        key_changed: e.key_changed,
        trusted_key: e.trusted_key.clone(),
        account: e.account.clone(),
        logged_in,
    }
}

// ---------------------------------------------------------------------------------------------- commands: state

#[tauri::command]
fn get_state(st: State<'_, App>) -> StateView {
    StateView {
        version: env!("CARGO_PKG_VERSION"),
        settings: st.settings.lock().unwrap().clone(),
        servers: st.servers.lock().unwrap().iter().map(view).collect(),
        hosts_installed: hosts::is_installed(),
        hosts_leftover: hosts::has_block(),
        hosts_path: hosts::hosts_path().display().to_string(),
        boot_install: st.boot_install,
    }
}

/// What sits in place of the bootstrap in each install slot (`None` — slot empty or not an install).
#[tauri::command]
fn bootstrap_status(st: State<'_, App>) -> Vec<Option<bootstrap::Root>> {
    let installs = st.settings.lock().unwrap().installs.clone();
    installs.iter().map(|r| Install::open(r).ok().map(|i| bootstrap::root_kind(&i))).collect()
}

/// Lays this launcher in place of / takes it back from the bootstrap of every configured install.
fn apply_bootstrap(enable: bool, installs: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    for root in installs.iter().filter(|r| !r.trim().is_empty()) {
        let Ok(inst) = Install::open(root) else { continue };
        bootstrap::cleanup(&inst);
        let r = if enable { bootstrap::install(&inst).map(|w| if w { "installed" } else { "ok" }) } else { bootstrap::uninstall(&inst).map(|_| "restored") };
        out.push(match r {
            Ok(s) => format!("{root}: {s}"),
            Err(e) => format!("{root}: {e}"),
        });
    }
    out
}

#[tauri::command]
async fn set_bootstrap(app: AppHandle, st: State<'_, App>, enable: bool) -> Result<Vec<String>> {
    let installs = {
        let mut s = st.settings.lock().unwrap();
        s.steam_bootstrap = enable;
        store::save(&store::settings_path(), &*s)?;
        s.installs.clone()
    };
    let out = tokio::task::spawn_blocking(move || apply_bootstrap(enable, &installs)).await.map_err(internal)?;
    for line in &out {
        log(&app, "steam", "info", line.clone());
    }
    Ok(out)
}

/// One game install since 1.6.1: slot 0. `installs` keeps two entries so older versions still read the file; the second is
/// always empty. Started as `<folder>\Dungeonborne.exe` (Steam "Play"): that folder is the install. A folder that stops
/// being the install gets its original bootstrap back if this launcher stood in for it. `true` — settings changed.
fn one_install(s: &mut Settings, boot_root: Option<&std::path::Path>) -> bool {
    let norm = |p: &str| p.trim().trim_end_matches(['\\', '/']).to_ascii_lowercase();
    let retire = |root: &str| {
        if let Ok(inst) = Install::open(root) {
            if matches!(bootstrap::root_kind(&inst), bootstrap::Root::Launcher(_)) {
                let _ = bootstrap::uninstall(&inst);
            }
        }
    };
    let before = s.installs.clone();
    s.installs.resize(2, String::new());
    if let Some(root) = boot_root {
        let root = root.display().to_string();
        if norm(&s.installs[0]) != norm(&root) {
            if !s.installs[0].trim().is_empty() {
                retire(&s.installs[0].clone());
            }
            s.installs[0] = root;
        }
    }
    let second = std::mem::take(&mut s.installs[1]);
    if !second.trim().is_empty() && norm(&second) != norm(&s.installs[0]) {
        retire(&second);
        // a player who only filled the second slot keeps that folder
        if s.installs[0].trim().is_empty() {
            s.installs[0] = second;
        }
    }
    s.installs != before
}

#[tauri::command]
fn save_settings(st: State<'_, App>, settings: Settings) -> Result<()> {
    let mut s = settings;
    s.installs.resize(2, String::new());
    s.installs[1].clear();
    if !matches!(s.lang.as_str(), "ru" | "en") {
        s.lang = "ru".into();
    }
    if !matches!(s.theme.as_str(), "torch" | "crypt" | "blood") {
        s.theme = "torch".into();
    }
    store::save(&store::settings_path(), &s)?;
    *st.settings.lock().unwrap() = s;
    Ok(())
}

#[derive(Serialize)]
struct InstallInfo {
    exe_sha256: String,
    running: bool,
    dirty: bool,
}

#[tauri::command]
async fn check_install(st: State<'_, App>, index: usize) -> Result<InstallInfo> {
    let inst = st.install(index)?;
    Ok(InstallInfo {
        exe_sha256: st.exe_sha256(&inst).await?,
        running: game::running_in(&inst.root),
        dirty: profile::has_journal(&inst),
    })
}

/// The game folder on this PC (a running game, else Steam's libraries), or `None`. The UI asks on start when the
/// folder is unset or no longer the game, and on "Find automatically".
#[tauri::command]
async fn find_game(app: AppHandle) -> Result<Option<String>> {
    let found = tokio::task::spawn_blocking(locate::find).await.map_err(internal)?;
    let text = match &found {
        Some(p) => format!("game found: {}", p.display()),
        None => "game not found: not running, not in Steam's libraries".into(),
    };
    log(&app, "install", "info", text);
    Ok(found.map(|p| p.display().to_string()))
}

/// "Browse…": the system folder picker over the launcher window. `None` — closed without a choice. A pick inside the
/// game or its parent `common` folder gives the install root; a folder without the game — `install_invalid`.
#[tauri::command]
async fn pick_game_folder(window: tauri::WebviewWindow, st: State<'_, App>, title: String) -> Result<Option<String>> {
    let current = st.settings.lock().unwrap().installs.first().cloned().unwrap_or_default();
    let mut dialog = rfd::AsyncFileDialog::new().set_title(&title).set_parent(&window);
    let start = PathBuf::from(current.trim());
    if start.is_dir() {
        dialog = dialog.set_directory(&start);
    }
    let Some(picked) = dialog.pick_folder().await else { return Ok(None) };
    match locate::root_of(picked.path()) {
        Some(root) => Ok(Some(root.display().to_string())),
        None => bail!("install_invalid", "{}", picked.path().display()),
    }
}

/// Undo profiles left by a crashed launcher. Called once by the UI on start.
#[tauri::command]
async fn restore_pending(app: AppHandle, st: State<'_, App>) -> Result<Vec<String>> {
    let busy = st.busy.lock().unwrap().clone();
    let installs = st.settings.lock().unwrap().installs.clone();
    let mut out = Vec::new();
    for (root, server_id) in profile::pending_journals() {
        let slot = installs.iter().position(|i| std::path::Path::new(i.trim_end_matches(['\\', '/'])) == root);
        if slot.is_some_and(|s| busy.contains(&s)) || game::running_in(&root) {
            continue;
        }
        let inst = Install { root: root.clone() };
        let r = tokio::task::spawn_blocking(move || profile::restore(&inst)).await.map_err(internal)?;
        let msg = match r {
            Ok(diff) if diff.is_empty() => format!("restored {} ({server_id})", root.display()),
            Ok(diff) => format!("restored {} ({server_id}), differences: {}", root.display(), diff.join(", ")),
            Err(e) => format!("restore {} failed: {e}", root.display()),
        };
        log(&app, "profile", "warn", msg.clone());
        out.push(msg);
    }
    Ok(out)
}

// ---------------------------------------------------------------------------------------------- commands: servers

async fn load_manifest_or_list(st: &App, url: &str) -> Result<std::result::Result<Manifest, Vec<String>>> {
    let dev = st.settings.lock().unwrap().dev_mode;
    let body = if !url.starts_with("http://") && !url.starts_with("https://") && dev {
        std::fs::read(url)?
    } else {
        net::get_bytes(&st.http, &net::check_url(url)?, 1 << 20).await?
    };
    let schema = serde_json::from_slice::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("schema").and_then(|s| s.as_str()).map(str::to_owned))
        .unwrap_or_default();
    if schema.starts_with("dbserverlist/") {
        let urls = manifest::fetch_list(&st.http, url).await?;
        return Ok(Err(urls));
    }
    Ok(Ok(manifest::parse(&body)?))
}

fn upsert(st: &App, url: &str, m: Manifest) -> Result<String> {
    let id = m.id.clone();
    let mut servers = st.servers.lock().unwrap();
    match servers.iter_mut().find(|s| s.manifest.id == id) {
        Some(e) => {
            e.update(m)?;
            e.url = url.to_string();
        }
        None => servers.push(ServerEntry::new(url.to_string(), m)),
    }
    store::save(&store::servers_path(), &*servers)?;
    Ok(id)
}

/// Adds a server by manifest URL, or every server of a `dbserverlist/1`. Returns the ids added or updated.
#[tauri::command]
async fn add_server(app: AppHandle, st: State<'_, App>, url: String) -> Result<Vec<String>> {
    let url = url.trim().to_string();
    match load_manifest_or_list(&st, &url).await? {
        Ok(m) => Ok(vec![upsert(&st, &url, m)?]),
        Err(list) => {
            let mut ids = Vec::new();
            for u in list {
                match manifest::fetch(&st.http, &u).await.and_then(|m| upsert(&st, &u, m)) {
                    Ok(id) => ids.push(id),
                    Err(e) => log(&app, "servers", "warn", format!("{u}: {e}")),
                }
            }
            Ok(ids)
        }
    }
}

async fn refresh_entry(st: &App, id: &str) -> Result<()> {
    let url = st.server(id)?.url;
    let m = match load_manifest_or_list(st, &url).await? {
        Ok(m) => m,
        Err(_) => bail!("manifest_invalid", "server list at a server url"),
    };
    let mut servers = st.servers.lock().unwrap();
    let e = servers.iter_mut().find(|s| s.manifest.id == id).ok_or_else(|| Error::new("server_unknown", id))?;
    e.update(m)?;
    store::save(&store::servers_path(), &*servers)
}

#[tauri::command]
async fn refresh_server(app: AppHandle, st: State<'_, App>, id: String) -> Result<()> {
    refresh_entry(&st, &id).await.inspect_err(|e| suspect(&app, &id, e))
}

/// Every (id, version) any server lists: the mod cache keeps exactly these.
fn referenced_mods(st: &App) -> HashSet<(String, String)> {
    st.servers.lock().unwrap().iter().flat_map(|s| s.manifest.mods.iter().map(|m| (m.id.clone(), m.version.clone()))).collect()
}

#[derive(Serialize)]
struct CacheInfo {
    bytes: u64,
    packages: usize,
}

#[tauri::command]
async fn mod_cache_info() -> Result<CacheInfo> {
    let (bytes, packages) = tokio::task::spawn_blocking(profile::cache_info).await.map_err(internal)?;
    Ok(CacheInfo { bytes, packages })
}

#[tauri::command]
async fn clear_mod_cache(st: State<'_, App>) -> Result<()> {
    if !st.busy.lock().unwrap().is_empty() {
        bail!("install_busy");
    }
    tokio::task::spawn_blocking(profile::clear_cache).await.map_err(internal)?
}

#[tauri::command]
fn remove_server(st: State<'_, App>, id: String) -> Result<()> {
    creds::delete(&id);
    store::save_profile(&id, None)?;
    let mut servers = st.servers.lock().unwrap();
    servers.retain(|s| s.manifest.id != id);
    store::save(&store::servers_path(), &*servers)
}

#[tauri::command]
fn accept_key(st: State<'_, App>, id: String) -> Result<()> {
    st.update_server(&id, |e| e.accept_key())
}

#[tauri::command]
async fn server_status(app: AppHandle, st: State<'_, App>, id: String) -> Result<Status> {
    let m = st.server(&id)?.manifest;
    match manifest::status(&st.http, &m).await {
        Ok(s) => {
            // the server answers: a good moment to deliver what the network check saw while it couldn't
            tauri::async_runtime::spawn(send_net_reports(app, id));
            Ok(s)
        }
        Err(e) => {
            suspect(&app, &id, &e);
            Err(e)
        }
    }
}

/// `null` — the server doesn't publish patch notes
#[tauri::command]
async fn server_notes(st: State<'_, App>, id: String) -> Result<Option<notes::Notes>> {
    let m = st.server(&id)?.manifest;
    let lang = st.settings.lock().unwrap().lang.clone();
    notes::fetch(&st.http, &m, &lang).await
}

/// `null` — nothing to show (no support source, no monthly goal); the UI hides the strip on errors too
#[tauri::command]
async fn server_support(st: State<'_, App>, id: String) -> Result<Option<support::Support>> {
    let m = st.server(&id)?.manifest;
    support::fetch(&st.http, &m).await
}

/// The player opened the notes up to `latest`: the tab stops being "new"
#[tauri::command]
fn notes_seen(st: State<'_, App>, id: String, latest: String) -> Result<()> {
    let mut s = st.settings.lock().unwrap();
    if s.notes_seen.get(&id) == Some(&latest) {
        return Ok(());
    }
    s.notes_seen.insert(id, latest);
    store::save(&store::settings_path(), &*s)
}

#[tauri::command]
fn open_link(url: String) -> Result<()> {
    let u = net::check_url(&url)?;
    winutil::open_url(u.as_str())
}

// ---------------------------------------------------------------------------------------------- commands: login

async fn login_flow(app: &AppHandle, st: &App, id: &str) -> Result<()> {
    let entry = st.server(id)?;
    if entry.key_changed {
        bail!("key_changed");
    }
    let Some(api) = entry.manifest.auth.as_ref().map(|a| a.api.clone()) else { bail!("no_auth") };
    let start = auth::start(&st.http, &api).await?;
    winutil::open_url(&start.browser_url)?;
    let _ = app.emit("login", LoginEvent { server_id: id, state: "browser", error: None, user_code: start.user_code.clone() });
    let deadline = tokio::time::Instant::now() + Duration::from_secs(start.expires_in_s.min(1800));
    let interval = Duration::from_millis(start.poll_interval_ms.clamp(500, 10_000));
    loop {
        tokio::time::sleep(interval).await;
        if tokio::time::Instant::now() > deadline {
            bail!("login_expired");
        }
        let poll = match auth::poll(&st.http, &api, &start.login_id).await {
            Ok(p) => p,
            Err(e) if e.code == "network" => continue,
            Err(e) => return Err(e),
        };
        match poll {
            auth::Poll::Pending => continue,
            auth::Poll::Denied { reason, message, link } => {
                let mut e = Error::new(format!("api.{}", reason.as_deref().unwrap_or("denied")), message.unwrap_or_default());
                e.link = link.filter(|l| net::check_url(l).is_ok());
                return Err(e);
            }
            auth::Poll::Ok { session_token, expires_at, account } => {
                let account = account.into_account(expires_at)?;
                creds::store(id, &session_token)?;
                st.set_account(id, Some(account))?;
                return Ok(());
            }
        }
    }
}

#[tauri::command]
fn login(app: AppHandle, st: State<'_, App>, id: String) -> Result<()> {
    let app2 = app.clone();
    let id2 = id.clone();
    let task = tauri::async_runtime::spawn(async move {
        let st = app2.state::<App>();
        let r = login_flow(&app2, &st, &id2).await;
        st.logins.lock().unwrap().remove(&id2);
        match r {
            Ok(()) => {
                log(&app2, "login", "info", format!("{id2}: ok"));
                let _ = app2.emit("login", LoginEvent { server_id: &id2, state: "ok", error: None, user_code: None });
            }
            Err(e) => {
                suspect(&app2, &id2, &e);
                log(&app2, "login", "warn", format!("{id2}: {e}"));
                let _ = app2.emit("login", LoginEvent { server_id: &id2, state: "error", error: Some(e), user_code: None });
            }
        }
    });
    if let Some(old) = st.logins.lock().unwrap().insert(id, task.inner().abort_handle()) {
        old.abort();
    }
    Ok(())
}

#[tauri::command]
fn cancel_login(st: State<'_, App>, id: String) {
    if let Some(h) = st.logins.lock().unwrap().remove(&id) {
        h.abort();
    }
}

#[tauri::command]
async fn logout(st: State<'_, App>, id: String) -> Result<()> {
    let entry = st.server(&id)?;
    if let (Some(a), Ok(Some(token))) = (&entry.manifest.auth, creds::load(&id)) {
        let _ = auth::logout(&st.http, &a.api, &token).await;
    }
    creds::delete(&id);
    st.set_account(&id, None)
}

/// Token for the relay, refreshed when it expires within an hour.
async fn ensure_session(st: &App, entry: &ServerEntry) -> Result<(String, Account)> {
    let id = &entry.manifest.id;
    let (Some(account), Some(token)) = (entry.account.clone(), creds::load(id)?) else { bail!("login_required") };
    let now = net::unix_now();
    if account.expires_at <= now {
        bail!("login_required");
    }
    if account.expires_at - now > REFRESH_BEFORE_S {
        return Ok((token, account));
    }
    let api = &entry.manifest.auth.as_ref().ok_or_else(|| Error::new("no_auth", ""))?.api;
    let r = auth::refresh(&st.http, api, &token).await?;
    let account = match r.account {
        Some(a) => Account { logged_in_at: account.logged_in_at, ..a.into_account(r.expires_at)? },
        None => Account { expires_at: r.expires_at, ..account },
    };
    creds::store(id, &r.session_token)?;
    st.set_account(id, Some(account.clone()))?;
    Ok((r.session_token, account))
}

// ---------------------------------------------------------------------------------------------- commands: lobby

async fn lobby_session(st: &App, id: &str) -> Result<(String, String)> {
    let entry = st.server(id)?;
    let api = entry.manifest.auth.as_ref().ok_or_else(|| Error::new("no_auth", ""))?.api.clone();
    let (token, _) = ensure_session(st, &entry).await?;
    Ok((api, token))
}

/// Battle tunnel state on this server: `{udp_tunnel, capable}`
#[tauri::command]
async fn net_state(st: State<'_, App>, id: String) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    auth::net_get(&st.http, &api, &token).await
}

/// The player switched the battle tunnel (after the launcher's prompt or in the settings)
#[tauri::command]
async fn set_udp_tunnel(app: AppHandle, st: State<'_, App>, id: String, on: bool, reason: String) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    let v = auth::net_set_udp(&st.http, &api, &token, on, &reason).await?;
    log(&app, "relay", "info", format!("battle tunnel {} ({reason})", if on { "on" } else { "off" }));
    Ok(v)
}

#[tauri::command]
async fn lobby_state(app: AppHandle, st: State<'_, App>, id: String) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    let view = auth::lobby_get(&st.http, &api, &token).await;
    // the server no longer knows this session (database reset, ban): show the login screen now, not on Play
    if let Err(e) = &view {
        if e.code.starts_with("api.") {
            creds::delete(&id);
            log(&app, "relay", "warn", format!("session lost: {e}"));
            let _ = app.emit("session", SessionEvent { server_id: &id, error: e.clone() });
        }
    }
    view
}

#[tauri::command]
async fn lobby_join(st: State<'_, App>, id: String, instance: u32) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    auth::lobby_post(&st.http, &api, &token, "join", serde_json::json!({ "instance": instance })).await
}

#[tauri::command]
async fn lobby_vote(st: State<'_, App>, id: String, party_size: Option<u32>, level_id: Option<i64>) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    auth::lobby_post(&st.http, &api, &token, "vote", serde_json::json!({ "party_size": party_size, "level_id": level_id })).await
}

#[tauri::command]
async fn lobby_leave(st: State<'_, App>, id: String) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    auth::lobby_post(&st.http, &api, &token, "leave", serde_json::json!({})).await
}

// ---------------------------------------------------------------------------------------------- commands: hosts

#[tauri::command]
async fn hosts_install() -> Result<bool> {
    hosts::run_helper("install").await?;
    Ok(hosts::is_installed())
}

#[tauri::command]
async fn hosts_uninstall() -> Result<bool> {
    hosts::run_helper("uninstall").await?;
    Ok(hosts::has_block())
}

// ---------------------------------------------------------------------------------------------- relay sharing

/// One relay at a time (ports are global). A second client of the same server shares it.
async fn relay_acquire(app: &AppHandle, st: &App, server_id: &str, api: &str, cfg: relay::Config) -> Result<()> {
    let mut guard = st.relay.lock().await;
    if let Some(run) = guard.as_mut() {
        if run.server_id == server_id {
            run.users += 1;
            run.generation += 1;
            return Ok(());
        }
        if run.users > 0 {
            bail!("relay_busy", "{}", run.server_id);
        }
        if let Some(old) = guard.take() {
            old.relay.stop().await;
        }
    }
    let app2 = app.clone();
    let (sid, api) = (server_id.to_string(), api.to_string());
    let last_check = AtomicI64::new(0);
    let detector = std::sync::Mutex::new(netcheck::Detector::default());
    let outdated = std::sync::atomic::AtomicBool::new(false);
    let sink: relay::Sink = Arc::new(move |c: relay::ConnLog| {
        append_log(
            "relay.log",
            &format!("port={} up={} down={} ms={} reason={}", c.port, c.up, c.down, c.ms, c.reason),
        );
        // The server takes only newer launchers (backend `[relay] min_client`): say it once, the UI offers the update
        if c.reason.ends_with("launcher_outdated") && !outdated.swap(true, Ordering::Relaxed) {
            log(&app2, "relay", "warn", "the server refuses this launcher version: update it");
            let _ = app2.emit("launcher_outdated", sid.clone());
        }
        // The provider freezes connections to the server: offer the player the battle tunnel
        if let Some(found) = detector.lock().unwrap().push(&c) {
            tauri::async_runtime::spawn(net_hint(app2.clone(), sid.clone(), found));
        }
        // A lobby connection ending or a refused/broken tunnel may mean a ban or a killed session:
        // ask the server instead of letting the game reconnect blindly.
        let suspicious = (6601..=6604).contains(&c.port) || c.reason != "closed";
        let now = net::unix_now();
        if suspicious && c.reason != "relay stopped" && now - last_check.load(Ordering::Relaxed) >= 5 {
            last_check.store(now, Ordering::Relaxed);
            tauri::async_runtime::spawn(check_session(app2.clone(), sid.clone(), api.clone()));
        }
        let _ = app2.emit("relay", c);
    });
    let relay = relay::start(cfg, sink).await?;
    *guard = Some(RelayRun { server_id: server_id.to_string(), relay, users: 1, generation: 0 });
    Ok(())
}

/// Relay connections freeze: the provider banner, with the battle tunnel offered unless it is on already
/// (the server knows) or its state is unknown
async fn net_hint(app: AppHandle, id: String, found: netcheck::Finding) {
    let st = app.state::<App>();
    let tunnel = match lobby_session(&st, &id).await {
        Ok((api, token)) => match auth::net_get(&st.http, &api, &token).await {
            Ok(v) => v["udp_tunnel"].as_bool() != Some(true),
            Err(e) => {
                log(&app, "relay", "warn", format!("tunnel state unknown, not offering it: {e}"));
                false
            }
        },
        Err(_) => false,
    };
    net_blocked(&app, &id, netblock::from_stalls(&found), tunnel);
}

#[derive(Clone, Serialize)]
struct NetBlock {
    server_id: String,
    kind: netblock::Kind,
    /// What the launcher saw; also the reason sent to the server when the player turns the tunnel on
    detail: String,
    /// Offer the battle UDP tunnel too (relay stalls only: it doesn't help when nothing reaches the server)
    tunnel: bool,
}

/// A request to the server failed at the network level: is it the provider? At most one check per server in 2 min.
fn suspect(app: &AppHandle, id: &str, e: &Error) {
    if e.code != "network" {
        return;
    }
    let st = app.state::<App>();
    let now = net::unix_now();
    {
        let mut last = st.net_probe.lock().unwrap();
        if last.get(id).is_some_and(|t| now - t < 120) {
            return;
        }
        last.insert(id.to_string(), now);
    }
    let (app, id) = (app.clone(), id.to_string());
    tauri::async_runtime::spawn(async move {
        let st = app.state::<App>();
        let Ok(entry) = st.server(&id) else { return };
        let Some(target) = netblock::Target::from_manifest(&entry.manifest, &entry.url) else { return };
        let control = reqwest::Url::parse(netblock::CONTROL_URL).expect("control url");
        match netblock::probe(&st.http, &target, &control).await {
            netblock::Outcome::Blocked(v) => net_blocked(&app, &id, v, false),
            o => log(&app, "net", "info", format!("{id}: network check after a failed request: {o:?}")),
        }
    });
}

/// Serialises the pending-reports file between the check and the sender
static NET_REPORTS: Mutex<()> = Mutex::new(());

/// The provider cuts the connection: the player's banner (WARP / VPN) and a report for the server's log
fn net_blocked(app: &AppHandle, id: &str, v: netblock::Verdict, tunnel: bool) {
    log(app, "net", "warn", format!("{id}: the provider cuts the connection ({}): {}", v.kind.as_str(), v.detail));
    app.state::<App>().net_verdicts.lock().unwrap().insert(id.to_string(), v.clone());
    {
        let _g = NET_REPORTS.lock().unwrap();
        let mut p = netblock::Pending::load();
        p.add(netblock::Report { server_id: id.to_string(), kind: v.kind, detail: v.detail.clone(), at: net::unix_now() });
        p.save();
    }
    let _ = app.emit("net_block", NetBlock { server_id: id.to_string(), kind: v.kind, detail: v.detail, tunnel });
}

/// Sends what the network check saw to the server's log once it can be reached and the player is logged in.
/// An older server without the route (404) or a refused report is dropped; a network error keeps it for later.
async fn send_net_reports(app: AppHandle, id: String) {
    let mine = {
        let _g = NET_REPORTS.lock().unwrap();
        let mut p = netblock::Pending::load();
        let mine = p.take(&id);
        if mine.is_empty() {
            return;
        }
        p.save();
        mine
    };
    let st = app.state::<App>();
    let mut keep = Vec::new();
    match lobby_session(&st, &id).await {
        Err(_) => keep = mine,
        Ok((api, token)) => {
            for r in mine {
                match auth::net_report(&st.http, &api, &token, &r).await {
                    Ok(()) => log(&app, "net", "info", format!("{id}: reported {} to the server", r.kind.as_str())),
                    // the server is restarting (nginx 5xx) or the session expired: try again later
                    Err(e) if e.code == "network" || e.code == "api.expired" || (e.code == "http_status" && e.detail.starts_with('5')) => {
                        keep.push(r)
                    }
                    Err(e) => log(&app, "net", "info", format!("{id}: report not taken: {e}")),
                }
            }
        }
    }
    if !keep.is_empty() {
        let _g = NET_REPORTS.lock().unwrap();
        let mut p = netblock::Pending::load();
        for r in keep {
            p.add(r);
        }
        p.save();
    }
}

// ---------------------------------------------------------------------------------------------- commands: bug report

#[derive(Serialize)]
struct BugReportInfo {
    /// Files that would go, sizes after the limits
    files: Vec<bugreport::FileInfo>,
    /// The last game run looks like a crash: the form preselects "crash"
    crash: bool,
    /// Logged in on this server (the report goes with the player's session)
    logged_in: bool,
    title_max: usize,
    description_max: usize,
}

#[derive(Clone, Serialize)]
struct GameCrashed {
    server_id: String,
    exit_code: u32,
}

#[derive(Clone, Serialize)]
struct BugRequest {
    server_id: String,
    #[serde(flatten)]
    request: bugreport::GameRequest,
}

/// The launcher window to the front (`focus`) or only its taskbar button flashing (Windows may refuse the focus to a
/// background process; over a fullscreen game the flash is all there is)
fn to_front(app: &AppHandle, focus: bool) {
    let Some(w) = app.get_webview_window("main") else { return };
    let _ = w.unminimize();
    let _ = w.show();
    if !focus || w.set_focus().is_err() {
        let _ = w.request_user_attention(Some(tauri::UserAttentionType::Critical));
    }
}

/// While the game started by the launcher runs (`done` set when it exits): the pulse file for the game's "Support"
/// button every `PULSE_EVERY_S`, and its requests (checked every second) open the report form here
async fn game_link(app: AppHandle, id: String, done: Arc<std::sync::atomic::AtomicBool>) {
    let dir = bugreport::game_saved_dir();
    // a request left from an earlier run is not the player asking now
    let _ = std::fs::remove_file(dir.join(bugreport::REQUEST_FILE));
    let mut tick = 0u64;
    while !done.load(Ordering::Relaxed) {
        let now = net::unix_now();
        if tick % bugreport::PULSE_EVERY_S == 0 {
            bugreport::write_pulse(&dir, now);
        }
        if let Some(request) = bugreport::take_request(&dir, now) {
            log(&app, "bug", "info", format!("the game asks for the report form ({})", request.scene));
            to_front(&app, true);
            let _ = app.emit("bug_request", BugRequest { server_id: id.clone(), request });
        }
        tick += 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    bugreport::remove_pulse(&dir);
}

/// Log sources for a report and the game folder (`settings.installs[0]`)
fn bug_sources(st: &App) -> (Vec<bugreport::Source>, Option<PathBuf>) {
    let root = st.settings.lock().unwrap().installs.first().cloned().unwrap_or_default();
    let root = (!root.is_empty()).then(|| PathBuf::from(root));
    (bugreport::sources(&store::data_dir().join("logs"), root.as_deref()), root)
}

/// What the "Report a bug" form needs before it opens
#[tauri::command]
async fn bug_report_info(st: State<'_, App>, id: String) -> Result<BugReportInfo> {
    let entry = st.server(&id)?;
    let logged_in = entry.manifest.auth.is_some() && entry.account.as_ref().is_some_and(|a| a.expires_at > net::unix_now());
    let (sources, root) = bug_sources(&st);
    let (files, crash) = tokio::task::spawn_blocking(move || {
        let running = root.as_deref().is_some_and(game::running_in);
        let crash_age = root.as_deref().and_then(|r| bugreport::age_s(&bugreport::crash_log_path(r)));
        let ue_tail = bugreport::tail(&bugreport::ue_log_path(), 4096);
        let crash = bugreport::crash_likely(running, crash_age, &store::last_play(), net::unix_now(), ue_tail.as_deref());
        (bugreport::preview(&sources), crash)
    })
    .await
    .map_err(internal)?;
    Ok(BugReportInfo { files, crash, logged_in, title_max: bugreport::TITLE_MAX, description_max: bugreport::DESC_MAX })
}

/// Sends a bug report to the server with the player's session; returns its id for `bug_report_status`
#[tauri::command]
async fn bug_report(
    app: AppHandle,
    st: State<'_, App>,
    id: String,
    kind: String,
    title: String,
    description: String,
    source: Option<String>,
) -> Result<String> {
    let source = source.unwrap_or_default();
    if !bugreport::KINDS.contains(&kind.as_str()) {
        bail!("bad_kind", "{kind}");
    }
    if bugreport::clean(&title, bugreport::TITLE_MAX, false).is_empty() {
        bail!("bug_title_required");
    }
    let entry = st.server(&id)?;
    let (api, token) = lobby_session(&st, &id).await?;
    let exe_sha256 = match st.install(0) {
        Ok(inst) => st.exe_sha256(&inst).await.unwrap_or_default(),
        Err(_) => String::new(),
    };
    let last = store::last_play();
    let modset =
        if last.server_id == id && !last.modset.is_empty() { last.modset } else { bugreport::manifest_modset(&entry.manifest.mods) };
    let lang = st.settings.lock().unwrap().lang.clone();
    let (sources, _) = bug_sources(&st);
    let (files, os) = tokio::task::spawn_blocking(move || (bugreport::collect(&sources), bugreport::os_name())).await.map_err(internal)?;
    let report = bugreport::report(&bugreport::Fields {
        source: &source,
        kind: &kind,
        title: &title,
        description: &description,
        game_label: entry.manifest.game.label.as_deref(),
        modset: &modset,
        exe_sha256: &exe_sha256,
        os: &os,
        lang: &lang,
    });
    let sizes: Vec<String> = files.iter().map(|(n, b)| format!("{n} {} KB", b.len().div_ceil(1024))).collect();
    let rid = auth::bug_report(&st.http, &api, &token, &report, files).await.inspect_err(|e| {
        log(&app, "bug", "warn", format!("{id}: report not sent: {e}"));
        suspect(&app, &id, e);
    })?;
    log(&app, "bug", "info", format!("{id}: report {rid} sent ({kind}; {})", sizes.join(", ")));
    Ok(rid)
}

/// `{status: "new" | "posted", url}` of a sent report
#[tauri::command]
async fn bug_report_status(st: State<'_, App>, id: String, report: String) -> Result<serde_json::Value> {
    let (api, token) = lobby_session(&st, &id).await?;
    auth::bug_report_status(&st.http, &api, &token, &report).await
}

/// Text for support: what the check saw, the launcher version and the last relay connections. No tokens: the logs
/// never hold them.
#[tauri::command]
fn net_report_text(st: State<'_, App>, id: String) -> String {
    let tail = |file: &str, n: usize, keep: &dyn Fn(&str) -> bool| -> Vec<String> {
        let text = std::fs::read_to_string(store::data_dir().join("logs").join(file)).unwrap_or_default();
        let lines: Vec<&str> = text.lines().filter(|l| keep(l)).collect();
        lines[lines.len().saturating_sub(n)..].iter().map(|s| s.to_string()).collect()
    };
    let verdict = st.net_verdicts.lock().unwrap().get(&id).cloned();
    let mut out = vec![
        format!("Dungeonborne Launcher {} - network report", env!("CARGO_PKG_VERSION")),
        format!("server: {id}, time: {} (unix)", net::unix_now()),
        match verdict {
            Some(v) => format!("seen: {} - {}", v.kind.as_str(), v.detail),
            None => "seen: nothing yet".to_string(),
        },
        String::new(),
        "launcher.log (network):".to_string(),
    ];
    out.extend(tail("launcher.log", 20, &|l| l.contains("[net]") || l.contains("[relay]")));
    out.push(String::new());
    out.push("relay.log:".to_string());
    out.extend(tail("relay.log", 40, &|_| true));
    out.join("\n")
}

#[derive(Clone, Serialize)]
struct SessionEvent<'a> {
    server_id: &'a str,
    error: Error,
}

/// `GET /me`; a dead session (expired, banned) drops the token so the card shows the login screen again.
async fn check_session(app: AppHandle, id: String, api: String) {
    let Ok(Some(token)) = creds::load(&id) else { return };
    let http = app.state::<App>().http.clone();
    match auth::me(&http, &api, &token).await {
        Ok(()) => log(&app, "relay", "info", "session ok"),
        Err(e) if e.code.starts_with("api.") || e.code == "http_status" => {
            creds::delete(&id);
            log(&app, "relay", "warn", format!("session lost: {e}"));
            let _ = app.emit("session", SessionEvent { server_id: &id, error: e });
        }
        Err(e) => log(&app, "relay", "warn", format!("session check failed: {e}")),
    }
}

async fn relay_release(app: &AppHandle, st: &App) {
    let mut guard = st.relay.lock().await;
    let Some(run) = guard.as_mut() else { return };
    run.users = run.users.saturating_sub(1);
    if run.users > 0 {
        return;
    }
    let generation = run.generation;
    let relay = st.relay.clone();
    let app = app.clone();
    tokio::spawn(async move {
        tokio::time::sleep(RELAY_LINGER).await;
        let mut guard = relay.lock().await;
        if guard.as_ref().is_some_and(|r| r.users == 0 && r.generation == generation) {
            if let Some(r) = guard.take() {
                r.relay.stop().await;
            }
            log(&app, "relay", "info", "stopped");
        }
    });
}

// ---------------------------------------------------------------------------------------------- hosts fallback (1.7.2)

/// How long after the INTL login the game may take to come to the directory before the launcher gives up on
/// "without hosts". The original flow asks the directory right after the login, before "Click to start".
const REACH_LIMIT: Duration = Duration::from_secs(90);

/// The server's directory name resolves to this computer (the game can go without hosts). A slow resolver counts as no.
async fn dir_resolves(app: &AppHandle, st: &App) -> bool {
    let r = tokio::time::timeout(Duration::from_secs(5), tokio::task::spawn_blocking(|| hosts::resolves_locally(hosts::DIR_DOMAIN))).await;
    let ok = matches!(r, Ok(Ok(true)));
    if !ok {
        log(app, "hosts", "warn", format!("{} does not resolve to 127.0.0.1 here (router, VPN or antivirus DNS): switching to hosts", hosts::DIR_DOMAIN));
        set_force_hosts(st, true);
    }
    ok
}

/// Remembered in settings.json: the following starts go through hosts until the player turns it off.
fn set_force_hosts(st: &App, on: bool) {
    let mut s = st.settings.lock().unwrap();
    if s.force_hosts == on {
        return;
    }
    s.force_hosts = on;
    if let Err(e) = store::save(&store::settings_path(), &*s) {
        append_log("launcher.log", &format!("force_hosts not saved: {e}"));
    }
}

/// The game logged in through the relay but never opened the directory or lobby: it went to the official servers.
/// The next start goes through hosts; the UI asks to restart the game.
async fn reach_watch(app: AppHandle, id: String, activity: Arc<relay::Activity>, done: Arc<std::sync::atomic::AtomicBool>) {
    let mut w = relay::ReachWatch::new(activity.counts(), REACH_LIMIT);
    while !done.load(Ordering::Relaxed) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        match w.step(activity.counts(), Instant::now()) {
            relay::Reach::Waiting => {}
            relay::Reach::Reached => {
                log(&app, "hosts", "info", "the game came to the server without hosts");
                return;
            }
            relay::Reach::Missed => {
                log(&app, "hosts", "warn", format!("the game logged in but did not come to the server in {} s: next start goes through hosts", REACH_LIMIT.as_secs()));
                set_force_hosts(&app.state::<App>(), true);
                let _ = app.emit("hosts_fallback", id);
                return;
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------- play

#[derive(Serialize)]
struct ModView {
    id: String,
    version: String,
    required: bool,
    cached: bool,
    downloadable: bool,
}

/// Mods of a server as the card shows them.
#[tauri::command]
fn server_mods(st: State<'_, App>, id: String) -> Result<Vec<ModView>> {
    let m = st.server(&id)?.manifest;
    Ok(m.mods
        .iter()
        .map(|r| ModView {
            id: r.id.clone(),
            version: r.version.clone(),
            required: r.required,
            cached: store::data_dir().join("mods").join(&r.id).join(&r.version).join(".dbl_package_sha256").exists(),
            downloadable: r.url.is_some(),
        })
        .collect())
}

#[tauri::command]
async fn play(
    app: AppHandle,
    st: State<'_, App>,
    id: String,
    install: usize,
    optional: Vec<String>,
    confirm_native: Vec<String>,
) -> Result<()> {
    if !st.busy.lock().unwrap().insert(install) {
        bail!("install_busy");
    }
    let r = play_flow(&app, &st, &id, install, &optional, &confirm_native).await;
    st.busy.lock().unwrap().remove(&install);
    match &r {
        Ok(()) => step(&app, install, "done", "done", ""),
        Err(e) => {
            log(&app, &format!("play{}", install + 1), "info", format!("done error {e}"));
            let _ = app.emit("play", PlayEvent { install, step: "done", state: "error", text: e.to_string(), error: Some(e.clone()) });
        }
    }
    r
}

#[derive(Clone, Serialize)]
struct ModProgress {
    install: usize,
    id: String,
    done: u64,
    total: Option<u64>,
}

/// Required + chosen optional mods, up to 3 downloads at a time, progress as `mod_progress` events.
async fn prepare_mods(
    app: &AppHandle,
    st: &App,
    slot: usize,
    entry: &ServerEntry,
    optional: &[String],
    exe: &str,
) -> Result<Vec<profile::Prepared>> {
    let want: Vec<manifest::ModRef> =
        entry.manifest.mods.iter().filter(|r| r.required || optional.contains(&r.id)).cloned().collect();
    let sem = Arc::new(tokio::sync::Semaphore::new(3));
    let mut tasks = tokio::task::JoinSet::new();
    for (i, r) in want.iter().cloned().enumerate() {
        let (http, base, exe, sem, app) = (st.http.clone(), entry.url.clone(), exe.to_string(), sem.clone(), app.clone());
        let last = Mutex::new(Instant::now() - Duration::from_secs(1));
        let id = r.id.clone();
        let progress: profile::Progress = Arc::new(move |done, total| {
            let mut l = last.lock().unwrap();
            if l.elapsed() >= Duration::from_millis(150) || Some(done) == total {
                *l = Instant::now();
                let _ = app.emit("mod_progress", ModProgress { install: slot, id: id.clone(), done, total });
            }
        });
        tasks.spawn(async move {
            let _permit = sem.acquire_owned().await;
            (i, profile::prepare(&http, &r, &base, &exe, progress).await)
        });
    }
    let mut got: Vec<Option<profile::Prepared>> = (0..want.len()).map(|_| None).collect();
    let mut done = 0;
    while let Some(joined) = tasks.join_next().await {
        let (i, r) = joined.map_err(internal)?;
        got[i] = Some(r?); // the first error drops the JoinSet: other downloads are aborted
        done += 1;
        step(app, slot, "mods", "progress", format!("{done}/{}", want.len()));
    }
    Ok(got.into_iter().flatten().collect())
}

async fn play_flow(
    app: &AppHandle,
    st: &App,
    id: &str,
    slot: usize,
    optional: &[String],
    confirm_native: &[String],
) -> Result<()> {
    // 1. check (fresh manifest first: new mod versions, key change; offline - the saved one)
    step(app, slot, "check", "start", "");
    let settings = st.settings.lock().unwrap().clone();
    let dev = settings.dev_mode;
    match refresh_entry(st, id).await {
        Ok(()) => {
            let _ = app.emit("servers_changed", ());
        }
        Err(e) if matches!(e.code.as_str(), "network" | "http_status") => {
            log(app, "servers", "warn", format!("{id}: manifest not refreshed ({e}), using the saved one"));
        }
        Err(e) => return Err(e),
    }
    let entry = st.server(id)?;
    let m = &entry.manifest;
    if entry.key_changed {
        bail!("key_changed");
    }
    if m.game.exe_sha256.is_empty() {
        bail!("exe_unspecified");
    }
    let inst = st.install(slot)?;
    if game::running_in(&inst.root) {
        bail!("game_running");
    }
    let exe = st.exe_sha256(&inst).await?;
    if exe != m.game.exe_sha256 {
        bail!("exe_mismatch", "{exe}");
    }
    // Runtime from the exe (UE4SS, emulator dll, cacert.pem): only changed files are written. Files of older
    // bundles (mod paks, Lua mods — now dbmod/1 packages) go away, unless developer mode keeps the player's own setup.
    {
        let inst = inst.clone();
        let retire = !dev;
        let r = tokio::task::spawn_blocking(move || bundle::install(&inst, retire)).await.map_err(internal)??;
        log(app, "bundle", "info", format!("runtime {}: {} files written, {} old files removed", r.version, r.written, r.removed));
    }
    if !inst.win64().join("steam_api64.dll").is_file() {
        bail!("emulator_missing", "{}", inst.win64().display());
    }
    if profile::has_journal(&inst) {
        let inst2 = inst.clone();
        tokio::task::spawn_blocking(move || profile::restore(&inst2)).await.map_err(internal)??;
    }
    let mut without_hosts = false;
    let session = if dev {
        None
    } else {
        if m.relay.is_none() || m.auth.is_none() {
            bail!("no_relay");
        }
        // the game goes to its real domains unless the server's mod points it at localhost (`relay.no_hosts`).
        // 1.7.2: and even then through hosts when the player (or an earlier failed start) asked for it, or when this
        // computer's DNS does not give the directory name back as 127.0.0.1
        let no_hosts = m.relay.as_ref().is_some_and(|r| r.no_hosts);
        without_hosts = no_hosts && !settings.force_hosts && dir_resolves(app, st).await;
        if !without_hosts && !hosts::is_installed() {
            bail!(if no_hosts { "hosts_fallback" } else { "hosts_not_installed" });
        }
        log(app, "hosts", "info", if without_hosts { "the game goes without hosts" } else { "the game goes through hosts" });
        Some(ensure_session(st, &entry).await?)
    };
    step(app, slot, "check", "done", "");

    // 2. mods + CA
    step(app, slot, "mods", "start", "");
    let set = prepare_mods(app, st, slot, &entry, optional, &exe).await?;
    profile::resolve(&set)?;
    profile::check_runtime(&inst, &set)?;
    // native code: asked once per server and package build, remembered in native_ok/<server>.json
    let allowed = store::native_ok(id);
    let key = |p: &profile::Prepared| format!("{}@{}", p.id, p.pkg_sha256);
    let native: Vec<&profile::Prepared> = set.iter().filter(|p| p.native() && !allowed.contains(&key(p))).collect();
    let unconfirmed: Vec<&str> = native.iter().filter(|p| !confirm_native.contains(&p.id)).map(|p| p.id.as_str()).collect();
    if !unconfirmed.is_empty() {
        bail!("native_confirm", "{}", unconfirmed.join(","));
    }
    if !native.is_empty() {
        store::allow_native(id, &native.iter().map(|p| key(p)).collect::<Vec<_>>())?;
    }
    // only when no other install is mid-start: its downloads live in the same cache
    if st.busy.lock().unwrap().len() == 1 {
        let keep = referenced_mods(st);
        let n = tokio::task::spawn_blocking(move || profile::gc_cache(&keep)).await.map_err(internal)?;
        if n > 0 {
            log(app, "mods", "info", format!("cache: {n} old package versions removed"));
        }
    }
    let modset = profile::modset_sha256(&set);
    let ca = match m.connect.as_ref().and_then(|c| c.ca_cert.clone().zip(c.ca_sha256.clone())) {
        Some((url, want)) => {
            let pem = net::get_bytes(&st.http, &net::check_url(&url)?, 256 * 1024).await?;
            if net::sha256_hex(&pem) != want {
                bail!("ca_hash");
            }
            if !pem.windows(27).any(|w| w == b"-----BEGIN CERTIFICATE-----") {
                bail!("ca_invalid");
            }
            Some(pem)
        }
        None => None,
    };
    step(app, slot, "mods", "done", format!("modset {}", &modset[..12]));
    let mut last_play = store::LastPlay {
        server_id: id.to_string(),
        modset: modset.clone(),
        mods: set.iter().map(|p| format!("{}@{}", p.id, p.version)).collect(),
        ..Default::default()
    };

    // 3. profile
    step(app, slot, "profile", "start", "");
    {
        let inst = inst.clone();
        let server_id = id.to_string();
        let goldberg = session.as_ref().map(|(_, a)| (a.open_id.clone(), a.goldberg_nick()));
        let isolate = m.policy.allow_unlisted_client_mods == Some(false);
        // the open INTL config only with a server that needs no hosts: with hosts the login goes as it always did
        let intl = session.is_some() && m.relay.as_ref().is_some_and(|r| r.no_hosts);
        let client_settings = m.client_settings.clone();
        tokio::task::spawn_blocking(move || {
            profile::apply(
                &inst,
                &profile::Plan {
                    server_id: &server_id,
                    mods: &set,
                    ca_pem: ca.as_deref(),
                    goldberg: goldberg.as_ref().map(|(o, n)| (o.as_str(), n.as_str())),
                    isolate_unlisted: isolate,
                    // through our relay the INTL login needs no hosts line (`relay.no_hosts`)
                    intl_url: intl.then_some("https://localhost"),
                    client_settings: client_settings.as_ref(),
                },
            )
        })
        .await
        .map_err(internal)??;
    }
    step(app, slot, "profile", "done", "");

    // 4-5. relay + game; the profile is undone whatever happens here.
    let mut relay_held = false;
    let run = async {
        if let (Some((token, _)), Some(r)) = (&session, &m.relay) {
            step(app, slot, "relay", "start", format!("{}:{}", r.address, r.port));
            let cfg = relay::Config {
                address: r.address.clone(),
                port: r.port,
                cert_sha256: r.cert_sha256.clone(),
                token: token.clone(),
                modset: modset.clone(),
            };
            let api = m.auth.as_ref().map(|a| a.api.as_str()).unwrap_or_default();
            relay_acquire(app, st, id, api, cfg).await?;
            relay_held = true;
            step(app, slot, "relay", "done", "");
        }
        // ACE: a game started within seconds of the previous one exiting fails with "Anti-cheat system works error
        // (13-131084-2)" (playtests: Alt+F4 and Play at once, 05.11). Wait out the rest of the cooldown, second by second.
        loop {
            let left = restart_cooldown_left(store::last_game_exit_ms(), now_ms());
            if left == 0 {
                break;
            }
            step(app, slot, "game", "cooldown", left.to_string());
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        step(app, slot, "game", "start", "");
        // 1.7.2: without hosts, watch that the game comes to the server after logging in (it falls back to the
        // official directory when the name does not resolve here, and the player only sees its old shutdown notice)
        let watch_done = Arc::new(std::sync::atomic::AtomicBool::new(false));
        if without_hosts && relay_held {
            if let Some(a) = st.relay.lock().await.as_ref().map(|r| r.relay.activity.clone()) {
                tauri::async_runtime::spawn(reach_watch(app.clone(), id.to_string(), a, watch_done.clone()));
            }
        }
        let proc = game::launch(&inst, settings.run_as_admin, &st.game_args).inspect_err(|_| watch_done.store(true, Ordering::Relaxed))?;
        step(app, slot, "game", "running", "");
        last_play.started_at = net::unix_now();
        let _ = store::set_last_play(&last_play);
        // the game's "Support" button: the pulse says the launcher is here, its request opens the report form
        let link = tauri::async_runtime::spawn(game_link(app.clone(), id.to_string(), watch_done.clone()));
        let root = inst.root.clone();
        let waited = tokio::task::spawn_blocking(move || game::wait_exit(proc, &root)).await.map_err(internal);
        watch_done.store(true, Ordering::Relaxed);
        let _ = link.await;
        if let Err(e) = store::set_last_game_exit_ms(now_ms()) {
            log(app, "game", "warn", format!("last exit time not saved: {e}"));
        }
        last_play.exited_at = net::unix_now();
        last_play.exit_code = waited.as_ref().ok().copied();
        let _ = store::set_last_play(&last_play);
        if let Ok(code) = &waited {
            if *code != 0 {
                log(app, "game", "warn", format!("the game exited with code {code:#x}"));
            }
            // the launcher offers a report; it goes only when the player sends it (the logs hold the nick and open_id)
            let (root, last, now) = (inst.root.clone(), last_play.clone(), net::unix_now());
            if tokio::task::spawn_blocking(move || bugreport::crash_after_exit(&root, &last, now)).await.unwrap_or(false) {
                log(app, "game", "warn", "the game seems to have crashed: offering a bug report");
                to_front(app, false);
                let _ = app.emit("game_crashed", GameCrashed { server_id: id.to_string(), exit_code: *code });
            }
        }
        waited?;
        step(app, slot, "game", "done", "");
        Ok::<(), Error>(())
    }
    .await;

    // 6. cleanup
    step(app, slot, "cleanup", "start", "");
    if relay_held {
        relay_release(app, st).await;
    }
    let inst2 = inst.clone();
    let restored = tokio::task::spawn_blocking(move || profile::restore(&inst2)).await.map_err(internal)?;
    match restored {
        Ok(diff) if diff.is_empty() => step(app, slot, "cleanup", "done", ""),
        Ok(diff) => step(app, slot, "cleanup", "warn", diff.join(", ")),
        Err(e) => {
            step(app, slot, "cleanup", "error", e.to_string());
            run?;
            return Err(e);
        }
    }
    run
}

#[derive(Clone, Serialize)]
struct UpdateInfo {
    version: String,
    current: &'static str,
}

/// Newest signed launcher build any server offers.
#[tauri::command]
fn update_check(st: State<'_, App>) -> Option<UpdateInfo> {
    let servers = st.servers.lock().unwrap();
    update::best(servers.iter().filter_map(|s| s.manifest.launcher.as_ref()))
        .map(|r| UpdateInfo { version: r.version, current: update::CURRENT })
}

#[derive(Clone, Serialize)]
struct UpdateProgress {
    done: u64,
    total: Option<u64>,
}

/// Downloads the update, swaps the exe and restarts. Refused while a game runs: the relay lives in this process.
#[tauri::command]
async fn update_apply(app: AppHandle, st: State<'_, App>) -> Result<()> {
    let relay_busy = st.relay.lock().await.as_ref().is_some_and(|r| r.users > 0);
    if relay_busy || !st.busy.lock().unwrap().is_empty() {
        bail!("update_busy");
    }
    let Some(rel) = ({
        let servers = st.servers.lock().unwrap();
        update::best(servers.iter().filter_map(|s| s.manifest.launcher.as_ref()))
    }) else {
        bail!("update_none")
    };
    log(&app, "update", "info", format!("{} -> {}", update::CURRENT, rel.version));
    let app2 = app.clone();
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let exe = update::install(&st.http, &rel, move |done, total| {
        let mut l = last.lock().unwrap();
        if l.elapsed() >= Duration::from_millis(150) || Some(done) == total {
            *l = Instant::now();
            let _ = app2.emit("update_progress", UpdateProgress { done, total });
        }
    })
    .await?;
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg(WAIT_INSTANCE_FLAG).args(&st.game_args);
    cmd.spawn().map_err(|e| Error::new("update_no_write", e.to_string()))?;
    log(&app, "update", "info", format!("started {}", exe.display()));
    app.exit(0);
    Ok(())
}

/// Built-in servers (`builtin::SERVERS`) are added once; while the server is unreachable — retried every minute.
/// Removed by the player — not added again (`builtin_added.json` next to servers.json).
fn builtin_added_path() -> PathBuf {
    store::servers_path().with_file_name("builtin_added.json")
}

async fn add_builtin_servers(app: AppHandle) {
    loop {
        let st = app.state::<App>();
        let added: Vec<String> = store::load(&builtin_added_path());
        let pending: Vec<&str> = {
            let servers = st.servers.lock().unwrap();
            builtin::SERVERS.iter().copied().filter(|u| !added.iter().any(|a| a == u) && !servers.iter().any(|s| s.url == *u)).collect()
        };
        if pending.is_empty() {
            return;
        }
        for url in pending {
            match manifest::fetch(&st.http, url).await.and_then(|m| upsert(&st, url, m)) {
                Ok(id) => {
                    log(&app, "servers", "info", format!("built-in server {id} added"));
                    let mut added: Vec<String> = store::load(&builtin_added_path());
                    added.push(url.to_string());
                    let _ = store::save(&builtin_added_path(), &added);
                    let _ = app.emit("servers_changed", ());
                }
                Err(e) => log(&app, "servers", "warn", format!("{url}: {e}")),
            }
        }
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

// ---------------------------------------------------------------------------------------------- entry

pub fn run() {
    tauri::Builder::default()
        .manage(App::load())
        .setup(|app| {
            let app = app.handle().clone();
            update::cleanup();
            // keep the launcher in place of the bootstrap (Steam's file check puts the original back; older copies)
            let (enabled, installs) = {
                let s = app.state::<App>().settings.lock().unwrap().clone();
                (s.steam_bootstrap, s.installs)
            };
            if enabled {
                let app3 = app.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    for line in apply_bootstrap(true, &installs) {
                        if !line.ends_with(": ok") {
                            log(&app3, "steam", "info", line);
                        }
                    }
                });
            }
            let keep = referenced_mods(&app.state::<App>());
            tauri::async_runtime::spawn_blocking(move || profile::gc_cache(&keep));
            // fresh manifests on start: new mods, launcher updates (offline - the saved ones stay)
            let app2 = app.clone();
            tauri::async_runtime::spawn(async move {
                let st = app2.state::<App>();
                let ids: Vec<String> = st.servers.lock().unwrap().iter().map(|s| s.manifest.id.clone()).collect();
                for id in ids {
                    if let Err(e) = refresh_entry(&st, &id).await {
                        log(&app2, "servers", "warn", format!("{id}: {e}"));
                    }
                }
                let _ = app2.emit("servers_changed", ());
            });
            tauri::async_runtime::spawn(async move { add_builtin_servers(app).await });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            check_install,
            find_game,
            pick_game_folder,
            restore_pending,
            add_server,
            refresh_server,
            remove_server,
            accept_key,
            server_status,
            net_state,
            set_udp_tunnel,
            net_report_text,
            bug_report_info,
            bug_report,
            bug_report_status,
            server_notes,
            server_support,
            notes_seen,
            server_mods,
            mod_cache_info,
            clear_mod_cache,
            update_check,
            update_apply,
            bootstrap_status,
            set_bootstrap,
            open_link,
            login,
            cancel_login,
            logout,
            lobby_state,
            lobby_join,
            lobby_vote,
            lobby_leave,
            hosts_install,
            hosts_uninstall,
            play,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_install_from_old_settings() {
        use super::{one_install, Settings};
        let s = |a: &str, b: &str| Settings { installs: vec![a.into(), b.into()], ..Settings::default() };
        // folders that don't exist: nothing to give back, only the settings change
        let mut x = s("X:/nope/A", "X:/nope/B");
        assert!(one_install(&mut x, None));
        assert_eq!(x.installs, vec!["X:/nope/A".to_string(), String::new()], "the second slot is dropped");
        let mut x = s("", "X:/nope/B");
        assert!(one_install(&mut x, None));
        assert_eq!(x.installs[0], "X:/nope/B", "only the second was set: it becomes the install");
        let mut x = s("X:/nope/A", "");
        assert!(!one_install(&mut x, None), "already one install");
        let mut x = s("X:/nope/A/", "");
        assert!(!one_install(&mut x, Some(std::path::Path::new("x:/nope/a"))), "same folder from Steam");
        let mut x = s("X:/nope/A", "X:/nope/B");
        assert!(one_install(&mut x, Some(std::path::Path::new("X:/nope/B"))));
        assert_eq!(x.installs, vec!["X:/nope/B".to_string(), String::new()], "started from the second: it is the install now");
        let mut x = Settings { installs: vec![], ..Settings::default() };
        assert!(one_install(&mut x, None), "short list is filled to two");
        assert_eq!(x.installs.len(), 2);
    }

    #[test]
    fn restart_cooldown() {
        use super::restart_cooldown_left as left;
        assert_eq!(left(0, 1_000_000), 0, "never exited — no wait");
        assert_eq!(left(1_000_000, 1_000_000), 30);
        assert_eq!(left(1_000_000, 1_000_001), 30, "rounded up");
        assert_eq!(left(1_000_000, 1_029_001), 1);
        assert_eq!(left(1_000_000, 1_030_000), 0);
        assert_eq!(left(1_000_000, 2_000_000), 0);
        assert_eq!(left(2_000_000, 1_000_000), 0, "clock moved back");
    }
}
