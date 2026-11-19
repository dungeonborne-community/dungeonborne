//! Launcher HTTP API and login pages (docs/LAUNCHER_SPEC.md §4). Plain HTTP on `[web] bind`; outside, TLS is terminated by
//! a reverse proxy with a public certificate (`[web] public_url`).
//!
//! - `GET  /.well-known/dbserver.json`: the `dbserver/1` manifest (+ `auth`, `relay`);
//! - `GET  /dbserver/status`: online, capacity, version, `modset_sha256` of required mods;
//! - `GET  /dbserver/patch_notes[?lang=en]`: the `dbnotes/1` changelog of the active season or the common one, with a translation if
//!   there is one (src/auth/notes.rs); no file → 404;
//! - `GET  /launcher/<file>`: launcher build for self-update (`[launcher] dir`, src/auth/release.rs);
//! - `GET  /mods/<file>`: zip package of a mod from `[mods] dir` (only packages from `manifest_mods.json`, src/auth/mods.rs);
//! - `POST /api/launcher/v1/login/start` `{provider?}` → `{login_id, user_code, browser_url, poll_interval_ms, expires_in_s}`;
//! - `GET  /api/launcher/v1/login/poll?login_id=` → pending | ok (token, account) | denied;
//! - `POST /api/launcher/v1/session/refresh`, `POST …/logout`, `GET …/me`: `Authorization: Bearer <token>`;
//! - `GET  /api/launcher/v1/lobby`, `POST …/lobby/join|vote|leave`: instance lobby (docs/LAUNCHER_SPEC.md §12);
//! - `GET  /api/launcher/v1/net`, `POST …/net/udp_tunnel {on, reason}`: battle UDP tunnel by the player's choice (§14);
//! - `POST /api/launcher/v1/net/report {kind, detail, at, version}`: the launcher saw the ISP cutting the connection (§14.1):
//!   only a `[Net] report` line in the log, no DB;
//! - `POST /api/launcher/v1/bug_report` (multipart: JSON `report` + logs) → 201 `{id}` | 429 `{retry_after}`;
//!   `GET …/bug_report/{id}` → `{status, url}`: bug report for the Discord bot (src/bug_reports.rs);
//! - `GET  /auth/<provider>/start?login_id=`, `GET /auth/<provider>/callback`: the player's browser.

use super::accounts::{self, Account};
use super::provider::{Denied, Start};
use super::sessions::{self, LoginResult, LOGIN_TTL};
use super::AuthCore;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

type Core = State<Arc<AuthCore>>;

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

fn page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        r#"<!doctype html><html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{t}</title><style>body{{font-family:system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 1rem;background:#16161a;color:#e8e6e3}}
code{{font-size:1.6rem;letter-spacing:.15rem;background:#2a2a30;padding:.2rem .5rem;border-radius:.3rem}}input,button{{font-size:1rem;padding:.4rem}}</style>
</head><body><h2>{t}</h2>{b}</body></html>"#,
        t = html_escape(title),
        b = body
    ))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

/// Account for the launcher, not tied to a particular provider
pub fn account_json(a: &Account) -> Value {
    json!({
        "provider": a.provider, "subject": a.subject, "display_name": a.display_name, "avatar_url": a.avatar_url,
        "open_id": a.open_id, "nick": a.display_name, "roles": a.roles,
    })
}

fn bearer(h: &HeaderMap) -> Option<String> {
    h.get("authorization")?.to_str().ok()?.strip_prefix("Bearer ").map(|s| s.trim().to_string())
}

/// sha256 of the DER of the first PEM certificate: the relay pin for the launcher
pub fn cert_sha256(path: &str) -> Option<String> {
    let pem = std::fs::read(path).ok()?;
    let der = rustls_pemfile::certs(&mut pem.as_slice()).next()?.ok()?;
    Some(Sha256::digest(der.as_ref()).iter().map(|b| format!("{:02x}", b)).collect())
}

// ---------------------------------------------------------------- manifest and status

async fn manifest(State(c): Core) -> Json<Value> {
    let s = &c.cfg;
    let api = format!("{}/api/launcher/v1", s.web.public_url.trim_end_matches('/'));
    let relay = (!s.relay.bind.is_empty()).then(|| {
        let mut r = json!({ "address": s.relay.public_address, "port": s.relay.public_port, "cert_sha256": cert_sha256(&s.relay.cert) });
        // 12.11: game without hosts (launcher 1.7.1+ reads relay.no_hosts); without the flag there is no field, as before
        if s.relay.no_hosts {
            r["no_hosts"] = json!(true);
        }
        r
    });
    let providers: Vec<&str> = s.auth.providers.iter().map(String::as_str).filter(|p| c.providers.contains_key(*p)).collect();
    let mut m = json!({
        "schema": "dbserver/1",
        "id": s.server.id, "name": s.server.name, "description": s.server.description.base(),
        "description_i18n": s.server.description.i18n_json(), "region": s.server.region,
        "languages": s.server.languages, "website": s.server.website, "contact": s.server.contact,
        "game": { "exe_sha256": s.game.exe_sha256, "label": s.game.label },
        "connect": { "address": s.relay.public_address, "domains": "default",
                     "ports": { "dir": 8085, "lobby": [6601, 6602, 6603, 6604], "https": 443 } },
        "mods": mods_json(&c),
        "launcher": launcher_json(&c),
        "patch_notes": notes_json(&c),
        // 12.11: client mod settings from the season ([client.<mod>.<section>]); the launcher writes server_settings.ini
        "client_settings": crate::season::active().client.clone(),
        // 13.11: `[mods] allow_unlisted = false`: for the game session the launcher removes foreign paks (and UE4SS mods since 1.7.4)
        "policy": { "allow_unlisted_client_mods": s.mods.allow_unlisted, "accounts": "local" },
        "status_url": format!("{}/dbserver/status", s.web.public_url.trim_end_matches('/')),
        "auth": { "providers": providers, "api": api, "require": s.auth.require },
        "relay": relay,
    });
    // 16.11 fundraising bar (launcher: manifest.rs SupportRef): without `[support] url` there is no field
    if let Some(url) = support_url(s) {
        m["support"] = json!({ "url": url });
    }
    Json(m)
}

fn support_url(s: &crate::server_toml::ServerToml) -> Option<&str> {
    Some(s.support.url.trim()).filter(|u| !u.is_empty())
}

/// manifest `mods[]`: absolute url, from `public_url` (like `status_url` and `auth.api`)
fn mods_json(c: &AuthCore) -> Value {
    let base = c.cfg.web.public_url.trim_end_matches('/');
    let list: Vec<Value> = c
        .mods
        .list()
        .iter()
        .map(|p| {
            let e = &p.entry;
            json!({ "id": e.id, "version": e.version, "sha256": e.sha256, "size": e.size, "required": e.required,
                    "url": format!("{}/mods/{}", base, p.file) })
        })
        .collect();
    Value::Array(list)
}

/// manifest `launcher`: a build with the release signature (checked by the launcher), url from `public_url`
fn launcher_json(c: &AuthCore) -> Value {
    match c.releases.current() {
        Some(r) => json!({
            "version": r.version, "size": r.size, "sha256": r.sha256, "signature": r.signature,
            "url": format!("{}/launcher/{}", c.cfg.web.public_url.trim_end_matches('/'), r.file),
        }),
        None => Value::Null,
    }
}

/// manifest `patch_notes`: where to get the changelog and the key of its latest entry (the launcher marks "new").
/// The field is optional: old launchers do not read it; no file → `null`.
fn notes_json(c: &AuthCore) -> Value {
    notes_ref(c.notes.current().as_deref(), &c.cfg.web.public_url)
}

fn notes_ref(notes: Option<&super::notes::Notes>, public_url: &str) -> Value {
    match notes {
        Some(n) => json!({ "url": format!("{}/dbserver/patch_notes", public_url.trim_end_matches('/')), "latest": n.latest() }),
        None => Value::Null,
    }
}

async fn patch_notes(State(c): Core, Query(q): Query<HashMap<String, String>>) -> Response {
    notes_reply(c.notes.current().as_deref(), q.get("lang").map(String::as_str))
}

fn notes_reply(notes: Option<&super::notes::Notes>, lang: Option<&str>) -> Response {
    match notes {
        Some(n) => Json(n.json(lang)).into_response(),
        None => err(StatusCode::NOT_FOUND, "no_patch_notes", "Сервер не публикует список изменений"),
    }
}

async fn launcher_file(State(c): Core, Path(file): Path<String>) -> Response {
    let Some(path) = c.releases.file(&file) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, "application/vnd.microsoft.portable-executable")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn mod_file(State(c): Core, Path(file): Path<String>) -> Response {
    let Some(path) = c.mods.file(&file) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, "application/zip")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn status(State(c): Core, h: HeaderMap) -> Response {
    // the maintenance text also goes into motd: the old launcher shows it too, without its own banner
    let maintenance = crate::lobby::maintenance::text();
    let motd = maintenance.as_ref().unwrap_or(&c.cfg.server.motd);
    let (rooms, players_in_rooms) = crate::lobby::matchmaking::rooms_summary();
    let body = Json(json!({
        "online": crate::lobby::hub::online(), "capacity": c.cfg.server.capacity,
        "backend": concat!("dbserver-rs ", env!("CARGO_PKG_VERSION")), "modset_sha256": c.mods.required_modset(),
        "motd": motd.base(), "motd_i18n": motd.i18n_json(),
        "maintenance": maintenance.is_some(), "rooms": rooms, "players_in_rooms": players_in_rooms,
        // 15.11: lobby tabs for the Discord bot, the site and the launcher (`launcher_lobby::public_instances`)
        "instances": crate::lobby::launcher_lobby::public_instances(), "updated": sessions::now_secs(),
    }));
    // the response is public: the community site reads it from the browser (`[web] status_cors`)
    let origin = h.get(axum::http::header::ORIGIN).and_then(|v| v.to_str().ok()).unwrap_or("");
    match c.cfg.web.status_cors_for(origin).and_then(|o| axum::http::HeaderValue::from_str(&o).ok()) {
        Some(o) => ([(axum::http::header::ACCESS_CONTROL_ALLOW_ORIGIN, o), (axum::http::header::VARY, "Origin".parse().unwrap())], body).into_response(),
        None => body.into_response(),
    }
}

// ---------------------------------------------------------------- launcher API

/// 11.11: launcher older than `[relay] min_client` (by User-Agent `dbl/<version>`, set by the launcher): denied before the game starts.
/// Old launchers do not update themselves (only the "Update and restart" banner), and they show an unknown code as
/// "code (server text)", so the text tells the player what to press. 409, not 401/403: the launcher does not reset the session.
fn launcher_outdated(c: &AuthCore, h: &HeaderMap) -> Option<Response> {
    let min = c.cfg.relay.min_client.trim();
    if min.is_empty() {
        return None;
    }
    let ua = h.get(axum::http::header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let client = ua.split_whitespace().find(|t| t.starts_with("dbl/")).unwrap_or("");
    super::relay::client_outdated(client, min).then(|| {
        err(
            StatusCode::CONFLICT,
            "launcher_outdated",
            "Сервер принимает только новую версию лаунчера. Нажмите «Обновить и перезапустить» вверху окна.",
        )
    })
}

async fn login_start(State(c): Core, h: HeaderMap, body: Option<Json<Value>>) -> Response {
    if let Some(r) = launcher_outdated(&c, &h) {
        return r;
    }
    let wanted = body.as_ref().and_then(|Json(v)| v["provider"].as_str()).map(str::to_string);
    let Some(provider) = wanted.or_else(|| c.default_provider().map(str::to_string)) else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "no_provider", "На сервере не включён ни один способ входа");
    };
    let Some(p) = c.provider(&provider).cloned() else {
        return err(StatusCode::BAD_REQUEST, "bad_provider", "Такой способ входа на сервере не включён");
    };
    let Some((login_id, user_code)) = c.pending.start(&provider) else {
        return err(StatusCode::TOO_MANY_REQUESTS, "busy", "Слишком много входов, попробуйте через минуту");
    };
    // external OAuth (https): the browser goes straight there, bypassing our page: the launcher opens only https,
    // while our pages without a domain may be on http (`[web] browser_url`)
    let browser_url = match p.start(&login_id) {
        Start::Redirect(url) if url.starts_with("https://") => url,
        _ => format!("{}/auth/{}/start?login_id={}", c.cfg.web.browser_base(), provider, login_id),
    };
    Json(json!({
        "login_id": login_id, "user_code": user_code, "provider": provider, "browser_url": browser_url,
        "poll_interval_ms": 1000, "expires_in_s": LOGIN_TTL.as_secs(),
    }))
    .into_response()
}

async fn login_poll(State(c): Core, Query(q): Query<HashMap<String, String>>) -> Response {
    let id = q.get("login_id").map(String::as_str).unwrap_or("");
    match c.pending.poll(id) {
        None => err(StatusCode::NOT_FOUND, "expired", "Вход устарел, начните заново"),
        Some(LoginResult::Waiting) => Json(json!({ "status": "pending" })).into_response(),
        Some(LoginResult::Denied(d)) => {
            Json(json!({ "status": "denied", "reason": d.code, "message": d.message, "link": d.link })).into_response()
        }
        Some(LoginResult::Ok { token, expires_at, account }) => Json(json!({
            "status": "ok", "session_token": token, "expires_at": expires_at, "account": account_json(&account),
        }))
        .into_response(),
    }
}

/// Account by Bearer; ban → 403
async fn authed(c: &AuthCore, h: &HeaderMap) -> Result<(String, Account), Response> {
    if let Some(r) = launcher_outdated(c, h) {
        return Err(r);
    }
    let token = bearer(h).ok_or_else(|| err(StatusCode::UNAUTHORIZED, "expired", "Нет токена сессии"))?;
    let acc = sessions::resolve(&c.db, &token).await.ok_or_else(|| err(StatusCode::UNAUTHORIZED, "expired", "Сессия истекла, войдите заново"))?;
    if acc.banned {
        return Err(err(StatusCode::FORBIDDEN, "banned", "Доступ к серверу закрыт"));
    }
    if !accounts::login_allowed(&acc) {
        return Err(err(StatusCode::FORBIDDEN, "not_whitelisted", super::login_policy().1));
    }
    crate::lobby::maintenance::note_roles(&acc.open_id, &acc.roles, acc.banned);
    Ok((token, acc))
}

async fn session_refresh(State(c): Core, h: HeaderMap) -> Response {
    match authed(&c, &h).await {
        Err(r) => r,
        Ok((old, acc)) => {
            let (token, expires_at) = sessions::create(&c.db, &accounts::key(&acc.provider, &acc.subject), c.cfg.auth.session_ttl_days).await;
            sessions::revoke(&c.db, &old).await;
            Json(json!({ "session_token": token, "expires_at": expires_at, "account": account_json(&acc) })).into_response()
        }
    }
}

async fn logout(State(c): Core, h: HeaderMap) -> Response {
    if let Some(t) = bearer(&h) {
        sessions::revoke(&c.db, &t).await;
    }
    Json(json!({ "ok": true })).into_response()
}

async fn me(State(c): Core, h: HeaderMap) -> Response {
    match authed(&c, &h).await {
        Err(r) => r,
        Ok((_, acc)) => Json(account_json(&acc)).into_response(),
    }
}

// ---------------------------------------------------------------- network: battle UDP tunnel

use super::udp_tunnel;

fn net_json(open_id: &str) -> Value {
    json!({ "udp_tunnel": udp_tunnel::is_on(open_id), "capable": udp_tunnel::capable(open_id) })
}

async fn net_get(State(c): Core, h: HeaderMap) -> Response {
    match authed(&c, &h).await {
        Err(r) => r,
        Ok((_, acc)) => {
            // after a server restart the choice has not yet been loaded by a game login: take the stored one
            udp_tunnel::restore(&c.db, &acc.open_id, udp_tunnel::is_on(&acc.open_id)).await;
            Json(net_json(&acc.open_id)).into_response()
        }
    }
}

/// The player agreed to the bypass (or turned it off in settings). Takes effect from the next search: the battle server address
/// is issued at matchmaking (`udp_tunnel::client_ds`)
async fn net_udp_tunnel(State(c): Core, h: HeaderMap, body: Option<Json<Value>>) -> Response {
    let (_, acc) = match authed(&c, &h).await {
        Err(r) => return r,
        Ok(x) => x,
    };
    let Some(on) = body.as_ref().and_then(|Json(v)| v["on"].as_bool()) else {
        return err(StatusCode::BAD_REQUEST, "bad_request", "need {\"on\": true|false}");
    };
    let reason = body.as_ref().and_then(|Json(v)| v["reason"].as_str()).unwrap_or_default();
    let why = format!("{} \"{}\": {}", accounts::key(&acc.provider, &acc.subject), acc.display_name, reason);
    udp_tunnel::choose(&c.db, &acc.open_id, on, "launcher", &why).await;
    Json(net_json(&acc.open_id)).into_response()
}

/// How many ISP reports we accept from one account per hour: the launcher sends one per kind, this is with margin
const REPORTS_PER_HOUR: u32 = 20;

/// Log line for a launcher report; `Err`: what is wrong with the request body
fn report_line(who: &str, body: &Value, now: i64) -> Result<String, &'static str> {
    let kind = match body["kind"].as_str() {
        Some(k @ ("reset" | "freeze" | "stall")) => k,
        _ => return Err("kind: reset | freeze | stall"),
    };
    let detail = udp_tunnel::clean_reason(body["detail"].as_str().unwrap_or_default());
    let version: String = body["version"].as_str().unwrap_or("?").chars().filter(|c| c.is_ascii_graphic()).take(20).collect();
    // `at`: when the launcher saw it; the report may have waited until the player turned on a VPN
    let ago = body["at"].as_i64().map(|at| (now - at).clamp(0, 30 * 86400));
    let ago = ago.map_or(String::new(), |s| format!(" {} min ago", s / 60));
    Ok(format!("[Net] report {who} kind={kind} v={version}{ago}: {detail}"))
}

/// The launcher saw the ISP cutting the connection to the server (TLS reset, freeze, relay drops): to gauge the scale
async fn net_report(State(c): Core, h: HeaderMap, body: Option<Json<Value>>) -> Response {
    static SEEN: std::sync::LazyLock<std::sync::Mutex<HashMap<String, (i64, u32)>>> = std::sync::LazyLock::new(Default::default);
    let (_, acc) = match authed(&c, &h).await {
        Err(r) => return r,
        Ok(x) => x,
    };
    let body = body.map(|Json(v)| v).unwrap_or_default();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    {
        let mut seen = SEEN.lock().unwrap();
        let e = seen.entry(acc.open_id.clone()).or_insert((now, 0));
        if now - e.0 >= 3600 {
            *e = (now, 0);
        }
        if e.1 >= REPORTS_PER_HOUR {
            return err(StatusCode::TOO_MANY_REQUESTS, "busy", "Слишком много отчётов");
        }
        e.1 += 1;
    }
    let who = format!("{} {} \"{}\"", acc.open_id, accounts::key(&acc.provider, &acc.subject), acc.display_name);
    match report_line(&who, &body, now) {
        Ok(line) => {
            println!("{line}");
            Json(json!({ "ok": true })).into_response()
        }
        Err(why) => err(StatusCode::BAD_REQUEST, "bad_request", why),
    }
}

// ---------------------------------------------------------------- bug reports (src/bug_reports.rs)

use crate::bug_reports as bugs;
use axum::extract::multipart::{MultipartError, MultipartRejection};
use axum::extract::{DefaultBodyLimit, Multipart};

/// The `report` part is JSON with text: this is plenty for 100 + 1500 characters and versions
const REPORT_JSON_MAX: usize = 64 * 1024;

fn reject(r: bugs::Reject) -> Response {
    err(r.status, r.code, &r.message)
}

fn multipart_err(e: MultipartError) -> bugs::Reject {
    let status = e.status();
    let code = if status == StatusCode::PAYLOAD_TOO_LARGE { "too_large" } else { "bad_multipart" };
    bugs::Reject { status, code, message: e.body_text() }
}

fn rate_limited(secs: i64) -> Response {
    let body = json!({ "error": "rate_limited", "message": "Не больше 3 репортов в час", "retry_after": secs });
    (StatusCode::TOO_MANY_REQUESTS, [(axum::http::header::RETRY_AFTER, secs.to_string())], Json(body)).into_response()
}

/// Multipart parts: JSON `report` and files. Each part's limit is checked while reading, not afterwards
async fn read_parts(mut mp: Multipart) -> Result<(Value, Vec<(String, Vec<u8>)>), bugs::Reject> {
    let mut report = None;
    let mut files = Vec::new();
    while let Some(mut field) = mp.next_field().await.map_err(multipart_err)? {
        let name = field.name().unwrap_or_default().to_string();
        let max = match name.as_str() {
            "report" => REPORT_JSON_MAX,
            _ => bugs::file_limit(&name).ok_or_else(|| bugs::check_file(&name, &[]).unwrap_err())?,
        };
        let mut data = Vec::new();
        while let Some(chunk) = field.chunk().await.map_err(multipart_err)? {
            data.extend_from_slice(&chunk);
            if data.len() > max {
                return Err(bugs::too_large(format!("{name}: more than {} KB", max / 1024)));
            }
        }
        if name != "report" {
            files.push((name, data));
        } else if report.is_some() {
            return Err(bugs::bad("bad_report", "report part twice"));
        } else {
            report = Some(serde_json::from_slice::<Value>(&data).map_err(|e| bugs::bad("bad_report", format!("report: {e}")))?);
        }
    }
    let report = report.ok_or_else(|| bugs::bad("bad_report", "no report part"))?;
    Ok((report, files))
}

/// The player pressed "Report a bug" in the launcher: the report waits for the bot with status `new`
async fn bug_report_post(State(c): Core, h: HeaderMap, mp: Result<Multipart, MultipartRejection>) -> Response {
    // limit check and write one at a time, so two concurrent requests do not both pass the limit
    static GATE: std::sync::LazyLock<tokio::sync::Mutex<()>> = std::sync::LazyLock::new(Default::default);
    let (_, acc) = match authed(&c, &h).await {
        Err(r) => return r,
        Ok(x) => x,
    };
    // before reading the body: we do not accept an extra 9 MB
    if let Err(secs) = bugs::rate_check(&c.db, &acc.open_id, bugs::now()).await {
        return rate_limited(secs);
    }
    let mp = match mp {
        Ok(m) => m,
        Err(e) => return err(StatusCode::BAD_REQUEST, "bad_multipart", &e.body_text()),
    };
    let (body, files) = match read_parts(mp).await {
        Ok(x) => x,
        Err(r) => return reject(r),
    };
    let f = match bugs::parse_report(&body) {
        Ok(f) => f,
        Err(r) => return reject(r),
    };
    if let Err(r) = bugs::check_files(&files) {
        return reject(r);
    }
    let ua = h.get(axum::http::header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let battle_id = bugs::battle_of(&c.db, &acc.open_id).await;
    let _gate = GATE.lock().await;
    let now = bugs::now();
    if let Err(secs) = bugs::rate_check(&c.db, &acc.open_id, now).await {
        return rate_limited(secs);
    }
    let report = bugs::BugReport {
        rid: bugs::new_id(),
        open_id: acc.open_id.clone(),
        discord_id: if acc.provider == "discord" { acc.subject.clone() } else { String::new() },
        created_at: now,
        source: f.source,
        kind: f.kind,
        title: f.title,
        description: f.description,
        battle_id,
        game_version: f.game_version,
        launcher_version: bugs::launcher_version(ua),
        mods_version: f.mods_version,
        exe_sha256: f.exe_sha256,
        os: f.os,
        lang: f.lang,
        files: files.iter().map(|(name, data)| bugs::FileMeta { name: name.clone(), size: data.len() as u64 }).collect(),
        status: "new".into(),
        thread_id: String::new(),
        url: String::new(),
        posted_at: 0,
    };
    if let Err(e) = bugs::store(&c.db, &bugs::dir(), &report, &files).await {
        eprintln!("[BugReport] {} not saved: {}", acc.open_id, e);
        return err(StatusCode::INTERNAL_SERVER_ERROR, "store_failed", "Не удалось сохранить репорт, попробуйте позже");
    }
    let size: usize = files.iter().map(|(_, d)| d.len()).sum();
    println!(
        "[BugReport] {} from {} {} \"{}\": {} \"{}\", match {}, files {} ({} KB)",
        report.rid,
        acc.open_id,
        accounts::key(&acc.provider, &acc.subject),
        acc.display_name,
        report.kind,
        report.title,
        report.battle_id.as_deref().unwrap_or("-"),
        files.len(),
        size / 1024
    );
    (StatusCode::CREATED, Json(json!({ "id": report.rid }))).into_response()
}

/// Status of one's own report: `new` or `posted` with a link to the post. Someone else's is treated as nonexistent
async fn bug_report_get(State(c): Core, h: HeaderMap, Path(id): Path<String>) -> Response {
    let (_, acc) = match authed(&c, &h).await {
        Err(r) => return r,
        Ok(x) => x,
    };
    match bugs::get(&c.db, &id).await.filter(|r| r.open_id == acc.open_id) {
        Some(r) => Json(bugs::status_json(&r)).into_response(),
        None => err(StatusCode::NOT_FOUND, "not_found", "Репорт не найден"),
    }
}

// ---------------------------------------------------------------- instance lobby

use crate::lobby::launcher_lobby as lobby;

/// Account for the lobby; a disabled lobby is the `lobby_off` error
async fn lobby_account(c: &AuthCore, h: &HeaderMap) -> Result<Account, Response> {
    let (_, acc) = authed(c, h).await?;
    if !lobby::enabled() {
        return Err(err(StatusCode::NOT_FOUND, "lobby_off", "Лобби на сервере выключено"));
    }
    Ok(acc)
}

/// Lobby denial: the snapshot for the screen does not have to change, the launcher shows text by code
fn lobby_result(open_id: &str, r: Result<(), lobby::Refused>) -> Response {
    match r {
        Ok(()) => Json(lobby::snapshot(open_id)).into_response(),
        Err(e) => err(StatusCode::CONFLICT, e.code(), e.text()),
    }
}

async fn lobby_get(State(c): Core, h: HeaderMap) -> Response {
    let (_, acc) = match authed(&c, &h).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    if !lobby::enabled() {
        let m = &acc.open_id;
        let (text, i18n) = (crate::lobby::maintenance::message_for(m), crate::lobby::maintenance::i18n_for(m));
        return Json(json!({ "enabled": false, "maintenance": text, "maintenance_i18n": i18n })).into_response();
    }
    Json(lobby::snapshot_async(&acc.open_id).await).into_response()
}

async fn lobby_join(State(c): Core, h: HeaderMap, Json(body): Json<Value>) -> Response {
    match lobby_account(&c, &h).await {
        Err(r) => r,
        Ok(acc) => {
            let instance = body["instance"].as_u64().unwrap_or(0) as usize;
            lobby_result(&acc.open_id, lobby::join_async(&acc.open_id, instance).await)
        }
    }
}

async fn lobby_vote(State(c): Core, h: HeaderMap, Json(body): Json<Value>) -> Response {
    match lobby_account(&c, &h).await {
        Err(r) => r,
        Ok(acc) => {
            let party_size = body["party_size"].as_u64().map(|n| n as usize);
            lobby_result(&acc.open_id, lobby::vote(&acc.open_id, party_size, body["level_id"].as_i64()))
        }
    }
}

async fn lobby_leave(State(c): Core, h: HeaderMap) -> Response {
    match lobby_account(&c, &h).await {
        Err(r) => r,
        Ok(acc) => {
            lobby::leave(&acc.open_id);
            Json(lobby::snapshot(&acc.open_id)).into_response()
        }
    }
}

// ---------------------------------------------------------------- provider pages

async fn auth_start(State(c): Core, Path(provider): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let login_id = q.get("login_id").cloned().unwrap_or_default();
    let (Some(p), Some(pending)) = (c.provider(&provider), c.pending.get(&login_id)) else {
        return page("Вход устарел", "<p>Начните вход заново в лаунчере.</p>").into_response();
    };
    if pending.provider != provider {
        return page("Вход устарел", "<p>Начните вход заново в лаунчере.</p>").into_response();
    }
    match p.start(&login_id) {
        Start::Redirect(url) => Redirect::to(&url).into_response(),
        Start::Page(body) => page("Вход на сервер", &body).into_response(),
    }
}

async fn auth_callback(State(c): Core, Path(provider): Path<String>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(p) = c.provider(&provider).cloned() else {
        return page("Ошибка входа", "<p>Неизвестный способ входа.</p>").into_response();
    };
    let deny = |state: Option<String>, d: Denied| {
        if let Some(s) = state {
            c.pending.finish(&s, LoginResult::Denied(d.clone()));
        }
        let link = d
            .link
            .as_deref()
            .map(|l| format!(r#"<p><a href="{0}" style="color:#8ab4ff">{0}</a></p>"#, html_escape(l)))
            .unwrap_or_default();
        page("Вход не выполнен", &format!("<p>{}</p>{}<p>Вернитесь в лаунчер.</p>", html_escape(&d.message), link)).into_response()
    };
    let (state, identity) = match p.callback(&q).await {
        Ok(v) => v,
        Err((state, d)) => return deny(state, d),
    };
    let Some(pending) = c.pending.get(&state).filter(|x| x.provider == provider && matches!(x.result, LoginResult::Waiting)) else {
        return page("Вход устарел", "<p>Начните вход заново в лаунчере.</p>").into_response();
    };
    let acc = accounts::upsert(&c.db, &provider, &identity).await;
    if acc.banned {
        return deny(Some(state), Denied::new("banned", "Доступ к серверу закрыт"));
    }
    if !accounts::login_allowed(&acc) {
        println!("[Auth] login: denied {} '{}' — no role from login_roles", accounts::key(&acc.provider, &acc.subject), acc.display_name);
        let d = Denied::new("not_whitelisted", super::login_policy().1);
        return deny(Some(state), d.with_link(&c.cfg.auth.discord.invite_url));
    }
    let (token, expires_at) = sessions::create(&c.db, &accounts::key(&acc.provider, &acc.subject), c.cfg.auth.session_ttl_days).await;
    c.pending.finish(&state, LoginResult::Ok { token, expires_at, account: acc.clone() });
    println!("[Auth] login {} '{}' (open_id {})", accounts::key(&acc.provider, &acc.subject), acc.display_name, acc.open_id);
    page(
        "Готово",
        &format!(
            "<p>Вы вошли как <b>{}</b>.</p><p>Код в лаунчере должен быть таким же:</p><p><code>{}</code></p><p>Если коды разные — вы входите не в свой лаунчер, закройте страницу.</p><p>Можно вернуться в лаунчер.</p>",
            html_escape(&acc.display_name),
            html_escape(&pending.user_code)
        ),
    )
    .into_response()
}

pub fn router(core: Arc<AuthCore>) -> Router {
    Router::new()
        .route("/.well-known/dbserver.json", get(manifest))
        .route("/dbserver/status", get(status))
        .route("/dbserver/patch_notes", get(patch_notes))
        .route("/mods/:file", get(mod_file))
        .route("/launcher/:file", get(launcher_file))
        .route("/api/launcher/v1/login/start", post(login_start))
        .route("/api/launcher/v1/login/poll", get(login_poll))
        .route("/api/launcher/v1/session/refresh", post(session_refresh))
        .route("/api/launcher/v1/logout", post(logout))
        .route("/api/launcher/v1/me", get(me))
        .route("/api/launcher/v1/net", get(net_get))
        .route("/api/launcher/v1/net/udp_tunnel", post(net_udp_tunnel))
        .route("/api/launcher/v1/net/report", post(net_report))
        .route("/api/launcher/v1/bug_report", post(bug_report_post).layer(DefaultBodyLimit::max(bugs::MAX_BODY)))
        .route("/api/launcher/v1/bug_report/:id", get(bug_report_get))
        .route("/api/launcher/v1/lobby", get(lobby_get))
        .route("/api/launcher/v1/lobby/join", post(lobby_join))
        .route("/api/launcher/v1/lobby/vote", post(lobby_vote))
        .route("/api/launcher/v1/lobby/leave", post(lobby_leave))
        .route("/auth/:provider/start", get(auth_start))
        .route("/auth/:provider/callback", get(auth_callback))
        .with_state(core)
}

pub async fn run(core: Arc<AuthCore>) {
    let bind = core.cfg.web.bind.clone();
    let listener = match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => l,
        Err(e) => return eprintln!("[Auth] web API: could not bind {}: {}", bind, e),
    };
    println!("[Auth] launcher web API on http://{} (outside {})", bind, core.cfg.web.public_url);
    if let Err(e) = axum::serve(listener, router(core)).await {
        eprintln!("[Auth] web API stopped: {}", e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_only_when_configured() {
        let mut cfg = crate::server_toml::ServerToml::default();
        assert_eq!(support_url(&cfg), None, "no [support] url: no field");
        cfg.support.url = "  ".into();
        assert_eq!(support_url(&cfg), None);
        cfg.support.url = "https://status.example.org/dbserver/support".into();
        assert_eq!(support_url(&cfg), Some("https://status.example.org/dbserver/support"));
    }

    async fn body(r: Response) -> (StatusCode, Value) {
        let status = r.status();
        let bytes = axum::body::to_bytes(r.into_body(), 1 << 20).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[test]
    fn provider_reports() {
        let b = json!({ "kind": "reset", "detail": "tls to x: reset\n after hello", "at": 1000, "version": "1.5.2" });
        assert_eq!(
            report_line("o1 discord:1 «Player»", &b, 1000 + 600).unwrap(),
            "[Net] report o1 discord:1 «Player» kind=reset v=1.5.2 10 min ago: tls to x: reset after hello"
        );
        assert!(report_line("o", &json!({ "kind": "drop table" }), 0).is_err());
        assert!(report_line("o", &json!({}), 0).is_err());
        let long = json!({ "kind": "stall", "detail": "я".repeat(1000), "version": "1.5.2\n[Net] fake" });
        let line = report_line("o", &long, 0).unwrap();
        assert!(!line.contains('\n') && line.chars().count() < 400, "{line}");
    }

    /// Multipart body as the launcher (reqwest) sends it: parts with a name and a file name
    async fn bug_parts(parts: &[(&str, &[u8])]) -> Result<(Value, Vec<(String, Vec<u8>)>), bugs::Reject> {
        use axum::extract::FromRequest;
        let mut body = Vec::new();
        for (name, data) in parts {
            body.extend(format!("--XX\r\nContent-Disposition: form-data; name=\"{name}\"; filename=\"{name}\"\r\n\r\n").into_bytes());
            body.extend_from_slice(data);
            body.extend(b"\r\n");
        }
        body.extend(b"--XX--\r\n");
        let req = axum::http::Request::builder()
            .method("POST")
            .header("content-type", "multipart/form-data; boundary=XX")
            .body(axum::body::Body::from(body))
            .unwrap();
        read_parts(Multipart::from_request(req, &()).await.unwrap()).await
    }

    #[tokio::test]
    async fn bug_report_multipart() {
        let report = br#"{"kind":"game","title":"t"}"#;
        let (v, files) = bug_parts(&[("report", report), ("relay.log", b"a\nb\n")]).await.unwrap();
        assert_eq!(v["kind"], "game");
        assert_eq!(files, vec![("relay.log".to_string(), b"a\nb\n".to_vec())]);

        let code = |r: Result<_, bugs::Reject>| r.map(|_| ()).unwrap_err().code;
        assert_eq!(code(bug_parts(&[("relay.log", b"x")]).await), "bad_report", "no report");
        assert_eq!(code(bug_parts(&[("report", b"{")]).await), "bad_report");
        assert_eq!(code(bug_parts(&[("report", report), ("report", report)]).await), "bad_report");
        assert_eq!(code(bug_parts(&[("report", report), ("cmd.exe", b"MZ")]).await), "bad_file");
        let big = vec![0u8; bugs::file_limit("screenshot.png").unwrap() + 1];
        let r = bug_parts(&[("report", report), ("screenshot.png", &big)]).await.map(|_| ()).unwrap_err();
        assert_eq!((r.status, r.code), (StatusCode::PAYLOAD_TOO_LARGE, "too_large"));
    }

    #[tokio::test]
    async fn patch_notes_with_and_without_a_file() {
        let (status, v) = body(notes_reply(None, None)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(v["error"], "no_patch_notes");
        assert!(notes_ref(None, "https://x.org").is_null(), "no file: no field in the manifest");

        let n = super::super::notes::parse("[[update]]\nversion = \"v1\"\ndate = \"2026-11-07\"\ntitle = \"Выкладка\"\nnew = [\"a\"]\n").unwrap();
        let (status, v) = body(notes_reply(Some(&n), Some("en"))).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["schema"], "dbnotes/1");
        assert_eq!(v["updates"][0]["sections"][0]["items"][0], "a");
        let m = notes_ref(Some(&n), "https://x.org/");
        assert_eq!(m, json!({ "url": "https://x.org/dbserver/patch_notes", "latest": "v1" }));
    }
}
