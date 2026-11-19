//! Mock of the backend admin API (ADMIN_API.md), so the GM console works before the backend has it.
//! Same checks as the contract (loopback, Host, no Origin, JSON, Bearer for `require = "relay"`) and the same
//! command texts as `player_gm_command_impl`. In `relay` mode it accepts `Bearer mock-admin` and refuses
//! `Bearer mock-gm` with 403.

use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;

use crate::admin::{GmResult, Player};

pub const ADMIN_TOKEN: &str = "mock-admin";
pub const GM_TOKEN: &str = "mock-gm";
const ERR_GM_CMD_INVALID: i32 = -501;
const ERR_GM_CMD_PARAM_INVALID: i32 = -503;
const GOLD: i32 = 1;

struct MockPlayer {
    p: Player,
    account_provider: String,
    online: bool,
    coins: BTreeMap<i32, i64>,
    items: Vec<(i32, i32)>,
    facades: Vec<i32>,
    banned: bool,
    ban_reason: String,
}

pub struct Mock {
    pub require_relay: bool,
    pub port: u16,
    players: Vec<MockPlayer>,
    sink: Arc<dyn Fn(String) + Send + Sync>,
    /// Game server as the backend sees it: `dbserver-status.json` (None = no host → 503) and the last command result.
    pub host: Option<serde_json::Value>,
    host_last: serde_json::Value,
}

pub type Shared = Arc<Mutex<Mock>>;

fn player(open_id: &str, user_id: i64, nick: &str, roles: &[&str], online: bool, since: i64) -> MockPlayer {
    MockPlayer {
        p: Player {
            open_id: open_id.into(),
            user_id,
            nick: nick.into(),
            roles: roles.iter().map(|s| s.to_string()).collect(),
            provider: Some("dev".into()),
            display_name: Some(format!("{nick} (mock)")),
            hall_role_guid: user_id * 100 + 1,
            peer_ip: "127.0.0.1".into(),
            online_since: since,
            in_battle: false,
        },
        account_provider: "dev".into(),
        online,
        coins: BTreeMap::from([(GOLD, 500)]),
        items: vec![],
        facades: vec![],
        banned: false,
        ban_reason: String::new(),
    }
}

impl Mock {
    pub fn new(port: u16, require_relay: bool, sink: Arc<dyn Fn(String) + Send + Sync>) -> Shared {
        let now = dbl::net::unix_now();
        Arc::new(Mutex::new(Self {
            require_relay,
            port,
            players: vec![
                player("76561198000000902", 10000902, "Noob", &["admin"], true, now - 1200),
                player("76561198000000903", 10000903, "DK", &[], true, now - 600),
                player("76561198000000003", 10000003, "Player3", &["gm"], true, now - 60),
                player("76561198000000004", 10000004, "Offline", &[], false, 0),
            ],
            sink,
            host: Some(json!({ "phase": "ready", "map": "/TgfMaps/Crypt/Classic", "mock": true })),
            host_last: serde_json::Value::Null,
        }))
    }
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    let mut body = json!({ "error": code });
    if !message.is_empty() {
        body["message"] = json!(message);
    }
    (status, Json(body)).into_response()
}

/// Contract §2 + §3. Ok(actor open_id) when the request may go on.
fn guard(m: &Mock, peer: SocketAddr, method: &Method, headers: &HeaderMap, needs_auth: bool) -> Result<String, Response> {
    if !peer.ip().is_loopback() {
        return Err(err(StatusCode::FORBIDDEN, "forbidden", "only loopback"));
    }
    let host = headers.get("host").and_then(|h| h.to_str().ok()).unwrap_or("");
    if host != format!("127.0.0.1:{}", m.port) && host != format!("localhost:{}", m.port) {
        return Err(err(StatusCode::FORBIDDEN, "bad_host", ""));
    }
    if headers.contains_key("origin") {
        return Err(err(StatusCode::FORBIDDEN, "browser_forbidden", ""));
    }
    if method == Method::POST {
        let ct = headers.get("content-type").and_then(|h| h.to_str().ok()).unwrap_or("");
        if !ct.starts_with("application/json") {
            return Err(err(StatusCode::UNSUPPORTED_MEDIA_TYPE, "bad_content_type", ""));
        }
    }
    if !needs_auth || !m.require_relay {
        return Ok("admin-api".into());
    }
    let token = headers.get("authorization").and_then(|h| h.to_str().ok()).and_then(|s| s.strip_prefix("Bearer ")).map(str::trim);
    match token {
        Some(ADMIN_TOKEN) => Ok("76561198000000902".into()),
        Some(GM_TOKEN) => Err(err(StatusCode::FORBIDDEN, "forbidden", "admin role required")),
        _ => Err(err(StatusCode::UNAUTHORIZED, "unauthorized", "")),
    }
}

async fn health(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, method: Method, headers: HeaderMap) -> Response {
    let m = s.lock().unwrap();
    if let Err(r) = guard(&m, peer, &method, &headers, false) {
        return r;
    }
    Json(json!({
        "ok": true,
        "server_id": "mock.admin",
        "auth_require": if m.require_relay { "relay" } else { "none" },
        "version": env!("CARGO_PKG_VERSION"),
        "mock": true,
    }))
    .into_response()
}

async fn players(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, method: Method, headers: HeaderMap) -> Response {
    let m = s.lock().unwrap();
    if let Err(r) = guard(&m, peer, &method, &headers, true) {
        return r;
    }
    let mut list: Vec<Player> = m.players.iter().filter(|p| p.online && !p.banned).map(|p| p.p.clone()).collect();
    list.sort_by_key(|p| p.online_since);
    Json(json!(list)).into_response()
}

async fn gm(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, method: Method, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let mut m = s.lock().unwrap();
    let actor = match guard(&m, peer, &method, &headers, true) {
        Ok(a) => a,
        Err(r) => return r,
    };
    if body.len() > 16 * 1024 {
        return err(StatusCode::BAD_REQUEST, "bad_request", "body larger than 16 KiB");
    }
    let req: crate::admin::GmRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return err(StatusCode::BAD_REQUEST, "bad_request", &e.to_string()),
    };
    if req.cmd.chars().count() > 256 {
        return err(StatusCode::BAD_REQUEST, "bad_request", "cmd longer than 256");
    }
    let args: Vec<String> = req.cmd.split_whitespace().map(str::to_string).collect();
    let cmd = args.first().map(|s| s.trim_start_matches(['/', '.']).to_lowercase()).unwrap_or_default();
    (m.sink)(format!("[GM] admin {actor} → {}: {:?}", req.open_id.as_deref().unwrap_or("-"), args));

    // like the backend: every command runs in some player's context, also help/ban/unban
    let open_id = req.open_id.clone().unwrap_or_default();
    let idx = if open_id.is_empty() {
        // no open_id: the only player online (contract §5)
        let online: Vec<usize> = (0..m.players.len()).filter(|&i| m.players[i].online && !m.players[i].banned).collect();
        match online.as_slice() {
            [one] => Some(*one),
            [] => return err(StatusCode::BAD_REQUEST, "bad_request", "nobody online — give an open_id"),
            _ => return err(StatusCode::BAD_REQUEST, "bad_request", "several players online — give an open_id"),
        }
    } else {
        match m.players.iter().position(|p| p.p.open_id == open_id) {
            Some(i) => Some(i),
            None => return err(StatusCode::NOT_FOUND, "player_not_found", &open_id),
        }
    };
    let result = run(&mut m, idx, &cmd, &args, &actor);
    let (target, online) = idx.map_or((None, None), |i| (Some(m.players[i].p.open_id.clone()), Some(m.players[i].online && !m.players[i].banned)));
    let out = match result {
        Ok(list) => GmResult { open_id: target, online, result_code: 0, result_message: "ok".into(), result_list: list },
        Err(msg) => GmResult {
            open_id: target,
            online,
            result_code: if msg.starts_with("unknown command") { ERR_GM_CMD_INVALID } else { ERR_GM_CMD_PARAM_INVALID },
            result_message: msg,
            result_list: vec![],
        },
    };
    Json(out).into_response()
}

fn run(m: &mut Mock, idx: Option<usize>, cmd: &str, args: &[String], _actor: &str) -> Result<Vec<String>, String> {
    let num = |i: usize| args.get(i).and_then(|s| s.parse::<i64>().ok());
    match cmd {
        "" | "help" | "?" => Ok(vec![
            "gold <n>                 — +n gold (n < 0 — take away)".into(),
            "coin <coin_id> <n>       — +n coins".into(),
            "item <type_id> [count]   — item to the stash".into(),
            "facade <id>              — grant a cosmetic (avatar/frame/title/skin)".into(),
            "info                     — profile and roles".into(),
            "ban <open_id|name> [reason] — account ban: sessions are revoked, the player is kicked at once".into(),
            "unban <open_id|name>     — lift a ban".into(),
        ]),
        "gold" | "addgold" | "add_gold" | "money" | "coin" | "addcoin" | "add_coin" => {
            let p = &mut m.players[idx.unwrap()];
            let is_gold = !cmd.contains("coin");
            let (id, n) = if is_gold { (Some(GOLD as i64), num(1)) } else { (num(1), num(2)) };
            let (Some(id), Some(n)) = (id, n) else { return Err(if is_gold { "gold <n>".into() } else { "coin <coin_id> <n>".into() }) };
            let cur = p.coins.get(&(id as i32)).copied().unwrap_or(0);
            if cur + n < 0 {
                return Err(if is_gold { "not enough gold".into() } else { "the balance would go negative".into() });
            }
            p.coins.insert(id as i32, cur + n);
            Ok(vec![if is_gold { format!("gold = {}", cur + n) } else { format!("coin {} = {}", id, cur + n) }])
        }
        "item" | "additem" | "add_item" | "give" => match num(1) {
            Some(t) if t > 0 => {
                let p = &mut m.players[idx.unwrap()];
                let count = num(2).unwrap_or(1).clamp(1, 9999) as i32;
                p.items.push((t as i32, count));
                let n = p.items.len();
                Ok(vec![format!("item {} x{} → stash page 1 ({},{})", t, count, (n * 3) % 12, (n * 5) / 12)])
            }
            Some(t) => Err(format!("no item_type {t}")),
            None => Err("item <type_id> [count]".into()),
        },
        "facade" | "skin" => match num(1) {
            Some(id) if id > 0 => {
                let p = &mut m.players[idx.unwrap()];
                if !p.facades.contains(&(id as i32)) {
                    p.facades.push(id as i32);
                }
                Ok(vec![format!("facade {id} granted")])
            }
            _ => Err("facade <id from facade.json>".into()),
        },
        "info" => {
            let p = &m.players[idx.unwrap()];
            Ok(std::iter::once(format!(
                "user_id={} nick='{}' gold={} hall={}",
                p.p.user_id,
                p.p.nick,
                p.coins.get(&GOLD).copied().unwrap_or(0),
                p.p.user_id * 100 + 1
            ))
            .chain(std::iter::once(format!("role {} '{}' type=400006 lazy_del=0", p.p.user_id * 100 + 1, p.p.nick)))
            .chain(p.items.iter().map(|(t, c)| format!("(mock) stash: {t} x{c}")))
            .collect())
        }
        "ban" | "unban" => {
            let Some(who) = args.get(1) else { return Err(format!("{cmd} <open_id|name> [reason]")) };
            let reason = args[2..].join(" ");
            let Some(p) = m.players.iter_mut().find(|p| &p.p.open_id == who || p.p.nick.eq_ignore_ascii_case(who)) else {
                return Err(format!("account '{who}' not found"));
            };
            p.banned = cmd == "ban";
            p.ban_reason = if p.banned { reason } else { String::new() };
            Ok(vec![format!(
                "{} {} '{}' ({}){}",
                if p.banned { "banned" } else { "unbanned" },
                p.p.open_id,
                p.p.display_name.clone().unwrap_or_default(),
                p.account_provider,
                if p.banned && !p.ban_reason.is_empty() { format!(": {}", p.ban_reason) } else { String::new() }
            )])
        }
        other => Err(format!("unknown command '{other}', see help")),
    }
}

async fn host_status(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, method: Method, headers: HeaderMap) -> Response {
    let m = s.lock().unwrap();
    if let Err(r) = guard(&m, peer, &method, &headers, true) {
        return r;
    }
    Json(json!({
        "root": "(mock)",
        "status": m.host.clone().unwrap_or(serde_json::Value::Null),
        "last_command": m.host_last,
    }))
    .into_response()
}

/// Like the backend: `portal [open_id|all]` / `exits` / `probe`, 503 without a host, `rejected_not_ready` unless ready.
async fn host_command(State(s): State<Shared>, ConnectInfo(peer): ConnectInfo<SocketAddr>, method: Method, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let mut m = s.lock().unwrap();
    let actor = match guard(&m, peer, &method, &headers, true) {
        Ok(a) => a,
        Err(r) => return r,
    };
    let cmd = serde_json::from_slice::<serde_json::Value>(&body).ok().and_then(|v| v["cmd"].as_str().map(|c| c.trim().to_string())).unwrap_or_default();
    if !crate::admin::host_cmd_valid(&cmd) {
        return err(StatusCode::BAD_REQUEST, "bad_request", "host command: portal | exits | probe [argument]");
    }
    let Some(status) = m.host.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "host_unavailable", "host not found (no dbserver-status.json in DB_HOST_ROOT)");
    };
    (m.sink)(format!("[Admin] {actor} -> host: {cmd}"));
    let mut words = cmd.split_whitespace();
    let verb = words.next().unwrap_or("");
    let arg = words.next();
    let online = m.players.iter().filter(|p| p.online && !p.banned).count();
    let result = if verb == "probe" {
        serde_json::Value::Null
    } else {
        let text = if status["phase"] != "ready" {
            "rejected_not_ready".to_string()
        } else {
            match (verb, arg) {
                ("portal", None | Some("all")) => format!("ok: portals {online}"),
                ("portal", Some(oid)) if m.players.iter().any(|p| p.p.open_id == oid && p.online) => "ok: portals 1".into(),
                ("portal", Some(oid)) => format!("no_player {oid}"),
                _ => "ok: exits spawned and activated".into(),
            }
        };
        let r = json!({ "cmd": cmd, "result": text, "at": dbl::net::unix_now() });
        m.host_last = r.clone();
        r
    };
    Json(json!({ "sent": cmd, "host_phase": status["phase"], "map": status["map"], "result": result })).into_response()
}

async fn not_found() -> Response {
    err(StatusCode::NOT_FOUND, "not_found", "")
}

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/admin/v1/health", get(health))
        .route("/admin/v1/players", get(players))
        .route("/admin/v1/gm", post(gm))
        .route("/admin/v1/host/status", get(host_status))
        .route("/admin/v1/host/command", post(host_command))
        .fallback(not_found)
        .with_state(state)
}

/// Binds 127.0.0.1:port and serves until the returned handle is aborted.
pub async fn serve(state: Shared) -> std::io::Result<tokio::task::JoinHandle<()>> {
    let port = state.lock().unwrap().port;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    let port = listener.local_addr()?.port();
    state.lock().unwrap().port = port;
    let app = router(state).into_make_service_with_connect_info::<SocketAddr>();
    Ok(tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    }))
}
