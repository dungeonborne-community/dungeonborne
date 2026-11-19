//! Mock of the launcher's server side (LAUNCHER_SPEC section 4): manifest, Discord login (a local page
//! stands in for Discord), session refresh/logout, and the TLS relay acceptor.
//!
//! Plain HTTP on loopback only (the launcher allows http for 127.0.0.1); the relay uses its own certificate,
//! pinned in the manifest via `relay.cert_sha256`.
//!
//! Usage: dbl-mock-server [--http 127.0.0.1:8787] [--relay 127.0.0.1:7443] [--exe-sha256 HEX]
//!                        [--upstream HOST [--port-offset N]] [--game-ca FILE.pem] [--demo-mod] [--data DIR]
//! Without --upstream the relay echoes bytes back after the preface (enough to test the launcher).

use std::collections::HashMap;
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use ring::rand::SecureRandom;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const SESSION_TTL: i64 = 7 * 24 * 3600;
const RELAY_PORTS: &[u16] = &[80, 443, 8085, 6601, 6602, 6603, 6604];

struct Cfg {
    http: SocketAddr,
    relay: SocketAddr,
    exe_sha256: String,
    upstream: Option<String>,
    port_offset: u16,
    game_ca: Option<Vec<u8>>,
    demo_mod: Option<(Vec<u8>, String)>,
    relay_pin: String,
}

#[derive(Clone)]
enum Login {
    Pending,
    Denied { reason: String, message: String, link: Option<String> },
    Ok(String),
}

struct Session {
    account: Value,
    expires_at: i64,
}

struct Inner {
    logins: HashMap<String, Login>,
    sessions: HashMap<String, Session>,
    accounts: HashMap<String, u64>,
    banned: std::collections::HashSet<String>,
    bugs: HashMap<String, BugReport>,
}

struct App {
    cfg: Cfg,
    inner: Mutex<Inner>,
    relay_conns: AtomicUsize,
}

type Shared = Arc<App>;

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64
}

fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    ring::rand::SystemRandom::new().fill(&mut b).expect("rng");
    hex::encode(b)
}

fn sha256_hex(d: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(d))
}

fn api_error(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

// ---------------------------------------------------------------------------------------------- http

fn base(app: &App) -> String {
    format!("http://{}", app.cfg.http)
}

async fn manifest(State(app): State<Shared>) -> Json<Value> {
    let b = base(&app);
    let mut connect = json!({ "address": app.cfg.relay.ip().to_string(), "domains": "default" });
    if let Some(ca) = &app.cfg.game_ca {
        connect["ca_cert"] = json!(format!("{b}/dbserver-ca.pem"));
        connect["ca_sha256"] = json!(sha256_hex(ca));
    }
    let mods = match &app.cfg.demo_mod {
        Some((zip, sha)) if !zip.is_empty() => json!([{
            "id": "mock.demo", "version": "0.1.0", "sha256": sha, "required": false,
            "url": "../mods/mock.demo-0.1.0.zip", "size": zip.len()
        }]),
        _ => json!([]),
    };
    Json(json!({
        "schema": "dbserver/1",
        "id": "local.mock",
        "name": "Mock server",
        "description": "Launcher mock server: login without Discord, relay with a pin.",
        "region": "local",
        "languages": ["ru", "en"],
        "game": { "exe_sha256": app.cfg.exe_sha256, "label": "Dungeonborne Win64 final" },
        "connect": connect,
        "mode": { "id": "mock", "tags": ["dev", "mock"] },
        "mods": mods,
        "policy": { "allow_unlisted_client_mods": true, "accounts": "local" },
        "status_url": format!("{b}/dbserver/status"),
        "key": "ed25519:mock-server-key",
        "auth": { "providers": ["discord"], "api": format!("{b}/api/launcher/v1"), "require": true },
        "relay": { "address": app.cfg.relay.ip().to_string(), "port": app.cfg.relay.port(), "cert_sha256": app.cfg.relay_pin }
    }))
}

async fn status(State(app): State<Shared>) -> Json<Value> {
    Json(json!({
        "online": app.relay_conns.load(Ordering::Relaxed), "capacity": 64,
        "backend": "dbl-mock-server", "motd": "This is a mock. Press \"Log in with Discord\"."
    }))
}

async fn serve_game_ca(State(app): State<Shared>) -> Response {
    match &app.cfg.game_ca {
        Some(ca) => ca.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn demo_mod(State(app): State<Shared>) -> Response {
    match &app.cfg.demo_mod {
        Some((zip, _)) => zip.clone().into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn login_start(State(app): State<Shared>) -> Json<Value> {
    let id = random_hex(16);
    app.inner.lock().unwrap().logins.insert(id.clone(), Login::Pending);
    Json(json!({
        "login_id": id,
        "browser_url": format!("{}/auth/discord/start?login_id={id}", base(&app)),
        "poll_interval_ms": 1000,
        "expires_in_s": 300
    }))
}

#[derive(Deserialize)]
struct LoginQ {
    login_id: String,
    #[serde(default)]
    nick: Option<String>,
    #[serde(default)]
    decision: Option<String>,
}

async fn login_poll(State(app): State<Shared>, Query(q): Query<LoginQ>) -> Response {
    let mut inner = app.inner.lock().unwrap();
    let Some(state) = inner.logins.get(&q.login_id).cloned() else {
        return api_error(StatusCode::NOT_FOUND, "expired", "Login expired, start again.");
    };
    match state {
        Login::Pending => Json(json!({ "status": "pending" })).into_response(),
        Login::Denied { reason, message, link } => {
            inner.logins.remove(&q.login_id);
            Json(json!({ "status": "denied", "reason": reason, "message": message, "link": link })).into_response()
        }
        Login::Ok(token) => {
            inner.logins.remove(&q.login_id);
            let s = &inner.sessions[&token];
            Json(json!({ "status": "ok", "session_token": token, "expires_at": s.expires_at, "account": s.account }))
                .into_response()
        }
    }
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"font-family:sans-serif;background:#18141c;color:#eee;display:grid;place-items:center;height:90vh\">\
         <div style=\"max-width:420px\"><h2>{title}</h2>{body}</div>"
    ))
}

async fn discord_start(Query(q): Query<LoginQ>) -> Html<String> {
    let id: String = q.login_id.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    page(
        "Mock Discord",
        &format!(
            "<p>This is a mock instead of Discord. The nick defines the account (same nick = same open_id).</p>\
             <form action=/auth/discord/callback><input type=hidden name=login_id value={id}>\
             <p><input name=nick value=Tester maxlength=32></p>\
             <button name=decision value=ok>Allow</button> <button name=decision value=deny>Deny</button>              <button name=decision value=guild>Not in the guild</button></form>"
        ),
    )
}

async fn discord_callback(State(app): State<Shared>, Query(q): Query<LoginQ>) -> Html<String> {
    let mut inner = app.inner.lock().unwrap();
    if !matches!(inner.logins.get(&q.login_id), Some(Login::Pending)) {
        return page("Login expired", "<p>Start the login in the launcher again.</p>");
    }
    let nick: String = q.nick.unwrap_or_default().chars().filter(|c| !c.is_control()).take(32).collect();
    let nick = if nick.trim().is_empty() { "Tester".to_string() } else { nick };
    let denied = match q.decision.as_deref() {
        Some("ok") if inner.banned.contains(&nick) => Some(("banned", "Access to the server is closed", None)),
        Some("ok") => None,
        Some("guild") => Some(("not_in_guild", "Join the project's Discord server", Some("https://discord.gg/example".to_string()))),
        _ => Some(("denied", "Denied on the mock page", None)),
    };
    if let Some((reason, message, link)) = denied {
        inner.logins.insert(q.login_id, Login::Denied { reason: reason.into(), message: message.into(), link });
        return page("Login failed", "<p>You can return to the launcher.</p>");
    }
    let next = inner.accounts.len() as u64 + 1;
    let n = *inner.accounts.entry(nick.clone()).or_insert(next);
    let token = random_hex(32);
    let account = json!({
        "provider": "discord",
        "subject": format!("{}", 100000000000000000u64 + n),
        "display_name": nick,
        "avatar_url": null,
        "open_id": (76561197960265728u64 + n).to_string(),
        "nick": nick,
        "roles": ["player"]
    });
    inner.sessions.insert(token.clone(), Session { account, expires_at: now() + SESSION_TTL });
    inner.logins.insert(q.login_id, Login::Ok(token));
    page("Done", "<p>You can return to the launcher.</p>")
}

fn bearer(h: &HeaderMap) -> Option<String> {
    h.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ").map(str::to_owned)
}

async fn refresh(State(app): State<Shared>, h: HeaderMap) -> Response {
    let mut inner = app.inner.lock().unwrap();
    let Some(old) = bearer(&h).and_then(|t| inner.sessions.remove(&t)) else {
        return api_error(StatusCode::UNAUTHORIZED, "expired", "Session expired, log in again.");
    };
    let token = random_hex(32);
    let expires_at = now() + SESSION_TTL;
    let account = old.account.clone();
    inner.sessions.insert(token.clone(), Session { account: old.account, expires_at });
    Json(json!({ "session_token": token, "expires_at": expires_at, "account": account })).into_response()
}

async fn me(State(app): State<Shared>, h: HeaderMap) -> Response {
    let inner = app.inner.lock().unwrap();
    match bearer(&h).and_then(|t| inner.sessions.get(&t)).filter(|s| s.expires_at > now()) {
        Some(s) => Json(s.account.clone()).into_response(),
        None => api_error(StatusCode::UNAUTHORIZED, "expired", "Session expired, log in again."),
    }
}

#[derive(Deserialize)]
struct BanQ {
    nick: String,
}

/// Test helper: ban a nick (its sessions die at once, new logins are denied with `banned`).
async fn ban(State(app): State<Shared>, Query(q): Query<BanQ>) -> String {
    let mut inner = app.inner.lock().unwrap();
    inner.sessions.retain(|_, s| s.account["nick"] != q.nick.as_str());
    inner.banned.insert(q.nick.clone());
    format!("banned {}
", q.nick)
}

async fn logout(State(app): State<Shared>, h: HeaderMap) -> StatusCode {
    if let Some(t) = bearer(&h) {
        app.inner.lock().unwrap().sessions.remove(&t);
    }
    StatusCode::NO_CONTENT
}

// ---------------------------------------------------------------------------------------------- bug reports
// Contract docs/specs/bug-report-api.md: multipart `report` JSON + log files, 3 an hour per player; the "bot"
// here posts a report 10 s after it came.

const BUG_FILES: &[(&str, usize)] =
    &[("launcher.log", 2 << 20), ("relay.log", 2 << 20), ("crash_ue4ss.log", 2 << 20), ("Dungeonborne.log", 3 << 20), ("screenshot.png", 1 << 20)];
const BUG_TOTAL: usize = 9 << 20;
const BUG_PER_HOUR: usize = 3;
const BUG_POST_AFTER_S: i64 = 10;

struct BugReport {
    open_id: String,
    created_at: i64,
}

fn session_open_id(app: &App, h: &HeaderMap) -> Option<String> {
    let inner = app.inner.lock().unwrap();
    let s = bearer(h).and_then(|t| inner.sessions.get(&t)).filter(|s| s.expires_at > now())?;
    s.account["open_id"].as_str().map(str::to_owned)
}

async fn bug_report(State(app): State<Shared>, h: HeaderMap, mut form: axum::extract::Multipart) -> Response {
    let Some(open_id) = session_open_id(&app, &h) else {
        return api_error(StatusCode::UNAUTHORIZED, "expired", "Session expired, log in again.");
    };
    {
        let inner = app.inner.lock().unwrap();
        let recent: Vec<i64> =
            inner.bugs.values().filter(|b| b.open_id == open_id && now() - b.created_at < 3600).map(|b| b.created_at).collect();
        if recent.len() >= BUG_PER_HOUR {
            let oldest = recent.iter().min().copied().unwrap_or_else(now);
            return (StatusCode::TOO_MANY_REQUESTS, Json(json!({ "retry_after": (oldest + 3600 - now()).max(1) }))).into_response();
        }
    }
    let mut report: Option<Value> = None;
    let mut files: Vec<(String, usize)> = Vec::new();
    loop {
        let field = match form.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => break,
            Err(e) => return api_error(StatusCode::BAD_REQUEST, "bad_request", &e.to_string()),
        };
        let name = field.name().unwrap_or_default().to_string();
        let data = match field.bytes().await {
            Ok(d) => d,
            Err(e) => return api_error(StatusCode::BAD_REQUEST, "bad_request", &e.to_string()),
        };
        if name == "report" {
            match serde_json::from_slice::<Value>(&data) {
                Ok(v) => report = Some(v),
                Err(e) => return api_error(StatusCode::BAD_REQUEST, "bad_report", &e.to_string()),
            }
        } else if let Some((_, limit)) = BUG_FILES.iter().find(|(n, _)| *n == name) {
            if data.len() > *limit {
                return api_error(StatusCode::PAYLOAD_TOO_LARGE, "too_large", &name);
            }
            files.push((name, data.len()));
        } else {
            return api_error(StatusCode::BAD_REQUEST, "bad_file", &name);
        }
    }
    if files.iter().map(|f| f.1).sum::<usize>() > BUG_TOTAL {
        return api_error(StatusCode::PAYLOAD_TOO_LARGE, "too_large", "total");
    }
    let Some(r) = report else { return api_error(StatusCode::BAD_REQUEST, "bad_report", "no report part") };
    let kind = r["kind"].as_str().unwrap_or_default();
    if !matches!(kind, "crash" | "game" | "launcher") || r["title"].as_str().is_none_or(|t| t.trim().is_empty()) {
        return api_error(StatusCode::BAD_REQUEST, "bad_report", "kind / title");
    }
    let id = format!("r_{}", random_hex(8));
    println!(
        "bug {id} from {open_id}: [{kind}] {:?} game={} mods={} os={} lang={} ua={:?} files={files:?}",
        r["title"].as_str().unwrap_or_default(),
        r["versions"]["game"],
        r["versions"]["mods"],
        r["os"],
        r["lang"],
        h.get("user-agent").and_then(|v| v.to_str().ok()).unwrap_or_default(),
    );
    app.inner.lock().unwrap().bugs.insert(id.clone(), BugReport { open_id, created_at: now() });
    (StatusCode::CREATED, Json(json!({ "id": id }))).into_response()
}

async fn bug_status(State(app): State<Shared>, h: HeaderMap, axum::extract::Path(id): axum::extract::Path<String>) -> Response {
    let Some(open_id) = session_open_id(&app, &h) else {
        return api_error(StatusCode::UNAUTHORIZED, "expired", "Session expired, log in again.");
    };
    let inner = app.inner.lock().unwrap();
    match inner.bugs.get(&id).filter(|b| b.open_id == open_id) {
        Some(b) if now() - b.created_at >= BUG_POST_AFTER_S => {
            Json(json!({ "status": "posted", "url": format!("https://discord.com/channels/0/0/{id}") })).into_response()
        }
        Some(_) => Json(json!({ "status": "new", "url": null })).into_response(),
        None => api_error(StatusCode::NOT_FOUND, "not_found", "No such report."),
    }
}

// ---------------------------------------------------------------------------------------------- relay

#[derive(Deserialize)]
struct Preface {
    v: u8,
    token: String,
    port: u16,
    #[serde(default)]
    modset: String,
    #[serde(default)]
    client: String,
}

async fn relay_conn(app: Shared, tls: tokio_rustls::server::TlsStream<TcpStream>, peer: SocketAddr) {
    let mut s = BufReader::new(tls);
    let mut line = Vec::new();
    let read = tokio::time::timeout(Duration::from_secs(10), (&mut s).take(4096).read_until(b'\n', &mut line)).await;
    if !matches!(read, Ok(Ok(n)) if n > 0) {
        println!("relay {peer}: no preface");
        return;
    }
    let reply = |ok: bool, err: &str| {
        let mut v = if ok { json!({ "ok": true }) } else { json!({ "ok": false, "error": err }) }.to_string();
        v.push('\n');
        v
    };
    let Ok(p) = serde_json::from_slice::<Preface>(&line) else {
        let _ = s.write_all(reply(false, "bad_preface").as_bytes()).await;
        return;
    };
    let who = {
        let inner = app.inner.lock().unwrap();
        inner.sessions.get(&p.token).filter(|x| x.expires_at > now()).map(|x| x.account["nick"].as_str().unwrap_or("?").to_string())
    };
    let err = if p.v != 1 {
        Some("bad_version")
    } else if who.is_none() {
        Some("expired")
    } else if !RELAY_PORTS.contains(&p.port) {
        Some("bad_port")
    } else {
        None
    };
    if let Some(e) = err {
        println!("relay {peer}: port {} rejected: {e}", p.port);
        let _ = s.write_all(reply(false, e).as_bytes()).await;
        let _ = s.shutdown().await;
        return;
    }
    let who = who.unwrap();
    let modset = p.modset.get(..12).unwrap_or(&p.modset);
    println!("relay {peer}: {who} port {} client {} modset {modset}", p.port, p.client);
    if s.write_all(reply(true, "").as_bytes()).await.is_err() {
        return;
    }
    app.relay_conns.fetch_add(1, Ordering::Relaxed);
    let result = match &app.cfg.upstream {
        Some(host) => match TcpStream::connect((host.as_str(), p.port + app.cfg.port_offset)).await {
            Ok(mut up) => tokio::io::copy_bidirectional(&mut s, &mut up).await.map(|(a, b)| format!("up {a} down {b}")),
            Err(e) => Err(e),
        },
        None => {
            let (mut r, mut w) = tokio::io::split(s);
            let n = tokio::io::copy(&mut r, &mut w).await;
            let _ = w.shutdown().await; // close_notify
            n.map(|n| format!("echoed {n}"))
        }
    };
    app.relay_conns.fetch_sub(1, Ordering::Relaxed);
    println!("relay {peer}: {who} port {} closed: {result:?}", p.port);
}

fn relay_cert(data: &PathBuf) -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let (cert_p, key_p) = (data.join("relay-cert.der"), data.join("relay-key.der"));
    if let (Ok(c), Ok(k)) = (std::fs::read(&cert_p), std::fs::read(&key_p)) {
        return (CertificateDer::from(c), PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(k)));
    }
    let key = rcgen::KeyPair::generate().expect("keygen");
    let params = rcgen::CertificateParams::new(vec!["localhost".into(), "127.0.0.1".into()]).expect("params");
    let cert = params.self_signed(&key).expect("self-signed");
    std::fs::create_dir_all(data).expect("data dir");
    std::fs::write(&cert_p, cert.der()).expect("write cert");
    std::fs::write(&key_p, key.serialize_der()).expect("write key");
    (cert.der().clone(), PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der())))
}

async fn run_relay(app: Shared, cert: CertificateDer<'static>, key: PrivateKeyDer<'static>) {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = rustls::ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("tls13")
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .expect("server cert");
    let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(cfg));
    let listener = TcpListener::bind(app.cfg.relay).await.expect("bind relay");
    loop {
        let Ok((sock, peer)) = listener.accept().await else { continue };
        let (acceptor, app) = (acceptor.clone(), app.clone());
        tokio::spawn(async move {
            match acceptor.accept(sock).await {
                Ok(tls) => relay_conn(app, tls, peer).await,
                Err(e) => println!("relay {peer}: tls: {e}"),
            }
        });
    }
}

// ---------------------------------------------------------------------------------------------- demo mod

fn build_demo_mod() -> (Vec<u8>, String) {
    let lua = b"print(\"[DblDemo] loaded by Dungeonborne launcher (mock server demo mod)\\n\")\n".to_vec();
    let mod_json = json!({
        "schema": "dbmod/1", "id": "mock.demo", "version": "0.1.0", "name": "Mock demo UE4SS mod",
        "authors": ["mock"], "license": "MIT", "side": "client",
        "components": [ { "type": "ue4ss", "path": "ue4ss/DblDemo" } ],
        "risk": { "native_code": false, "network": false },
        "files": { "ue4ss/DblDemo/Scripts/main.lua": sha256_hex(&lua) }
    });
    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        let opt = zip::write::SimpleFileOptions::default().last_modified_time(zip::DateTime::default());
        z.start_file("mod.json", opt).unwrap();
        z.write_all(serde_json::to_string_pretty(&mod_json).unwrap().as_bytes()).unwrap();
        z.start_file("ue4ss/DblDemo/Scripts/main.lua", opt).unwrap();
        z.write_all(&lua).unwrap();
        z.finish().unwrap();
    }
    let bytes = buf.into_inner();
    let sha = sha256_hex(&bytes);
    (bytes, sha)
}

// ---------------------------------------------------------------------------------------------- main

#[tokio::main]
async fn main() {
    let mut http: SocketAddr = "127.0.0.1:8787".parse().unwrap();
    let mut relay: SocketAddr = "127.0.0.1:7443".parse().unwrap();
    let mut exe_sha256 = "62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3".to_string();
    let mut upstream = None;
    let mut port_offset = 0u16;
    let mut game_ca = None;
    let mut demo = false;
    let mut data = PathBuf::from("mock-server/data");
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = || args.next().unwrap_or_else(|| panic!("{a} needs a value"));
        match a.as_str() {
            "--http" => http = val().parse().expect("--http addr"),
            "--relay" => relay = val().parse().expect("--relay addr"),
            "--exe-sha256" => exe_sha256 = val().to_ascii_lowercase(),
            "--upstream" => upstream = Some(val()),
            "--port-offset" => port_offset = val().parse().expect("--port-offset"),
            "--game-ca" => game_ca = Some(std::fs::read(val()).expect("--game-ca file")),
            "--demo-mod" => demo = true,
            "--data" => data = PathBuf::from(val()),
            other => panic!("unknown argument {other}"),
        }
    }
    let (cert, key) = relay_cert(&data);
    let relay_pin = sha256_hex(cert.as_ref());
    let cfg = Cfg {
        http,
        relay,
        exe_sha256,
        upstream,
        port_offset,
        game_ca,
        demo_mod: demo.then(build_demo_mod),
        relay_pin: relay_pin.clone(),
    };
    let app: Shared = Arc::new(App {
        cfg,
        inner: Mutex::new(Inner {
            logins: HashMap::new(),
            sessions: HashMap::new(),
            accounts: HashMap::new(),
            banned: Default::default(),
            bugs: HashMap::new(),
        }),
        relay_conns: AtomicUsize::new(0),
    });

    let router = Router::new()
        .route("/.well-known/dbserver.json", get(manifest))
        .route("/dbserver/status", get(status))
        .route("/dbserver-ca.pem", get(serve_game_ca))
        .route("/mods/mock.demo-0.1.0.zip", get(demo_mod))
        .route("/api/launcher/v1/login/start", post(login_start))
        .route("/api/launcher/v1/login/poll", get(login_poll))
        .route("/api/launcher/v1/session/refresh", post(refresh))
        .route("/api/launcher/v1/logout", post(logout))
        .route("/api/launcher/v1/me", get(me))
        .route(
            "/api/launcher/v1/bug_report",
            post(bug_report).layer(axum::extract::DefaultBodyLimit::max(BUG_TOTAL + (1 << 20))),
        )
        .route("/api/launcher/v1/bug_report/{id}", get(bug_status))
        .route("/mock/ban", get(ban))
        .route("/auth/discord/start", get(discord_start))
        .route("/auth/discord/callback", get(discord_callback))
        .with_state(app.clone());

    println!("manifest:  http://{http}/.well-known/dbserver.json");
    println!("relay:     {relay} (TLS 1.3, cert_sha256 {relay_pin})");
    match &app.cfg.upstream {
        Some(h) => println!("upstream:  {h} (port + {})", app.cfg.port_offset),
        None => println!("upstream:  none, relay echoes"),
    }
    tokio::spawn(run_relay(app.clone(), cert, key));
    let listener = TcpListener::bind(http).await.expect("bind http");
    axum::serve(listener, router).await.expect("http server");
}
