//! Backend admin API for developers (dev app, `tools/gm.py`): GM commands on a player and the online list.
//! Listens on loopback only (`ADMIN_BIND`, default 127.0.0.1:9092), requests not from loopback are rejected.
//! With `[auth] require = "relay"` it also needs `Authorization: Bearer <launcher session token>` of an account with the admin role.
//!
//! - `POST /admin/v1/db/export` → `{path, bytes, secs}` — consistent DB copy without stopping (`DB_EXPORT_DIR`);
//! - `GET  /admin/v1/players` → `[{open_id, user_id, nick, hall_role_guid}]`;
//! - `POST /admin/v1/gm` `{open_id?, cmd}` → `{result_code, result_message, result_list}`; without `open_id` — the only
//!   player online. Changes (items, coins) reach the player via normal dirty pushes if they are online.
//! - `GET  /admin/v1/host/status` → `{root, status, last_command}` of the game server;
//! - `POST /admin/v1/host/command` `{cmd: "portal [open_id|all]" | "exits" | "probe" | "kill <open_id|all>" | "leave <open_id|all>"}` → `{sent, host_phase, map, result}`
//!   — command to the host (mod DungeonborneRuServer), the reply is awaited for up to 10 s.
//! - `POST /admin/v1/lobby` `{open_id, action: "join"|"vote"|"leave"|"state", instance?, party_size?, level_id?}` →
//!   launcher lobby snapshot as the player sees it (08.11: checking the lobby on the test stand without logging into the launcher via Discord).
//! - `POST /admin/v1/toast` `{text, open_id? | "online"}` → `{online | delivered}` — the system notice at the top of the screen
//!   with any text (rpc/handlers/toast.rs; the client needs the mod DungeonborneToast, otherwise the text is in the system chat).
//! - `GET  /admin/v1/bug-reports?status=new&limit=10`, `GET …/bug-reports/{id}/files/{name}`,
//!   `POST …/bug-reports/{id}/posted {thread_id, url}` → 204 — bug reports for the Discord bot (src/bug_reports.rs,
//!   contract docs/specs/bug-report-api.md §2). The bot calls with `ADMIN_BEARER` = an admin session token.
//!
//! Why: the client has no field for `player_gm_command`, and the client itself has no general chat (ChatChannelConfigDataTable:
//! World has `bShowInPanel = false`) — GM from chat works only in a party or a room.

use crate::db::GameDb;
use crate::game_data::GameData;
use crate::rpc::handlers::GatewaySession;
use crate::structures::PlayerModel;
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::net::SocketAddr;
use std::sync::Arc;

#[derive(Clone)]
struct St {
    db: GameDb,
    game_data: Arc<GameData>,
}

fn deny(status: StatusCode, msg: &str) -> Response {
    (status, Json(json!({ "error": msg }))).into_response()
}

/// Loopback + (with require = relay) an admin Bearer. Ok — who is executing.
async fn check(st: &St, peer: &SocketAddr, h: &HeaderMap) -> Result<String, Response> {
    if !peer.ip().is_loopback() {
        return Err(deny(StatusCode::FORBIDDEN, "only from 127.0.0.1"));
    }
    if !crate::auth::require_relay() {
        return Ok("admin-api".into());
    }
    let token = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| deny(StatusCode::UNAUTHORIZED, "admin Bearer required"))?;
    match crate::auth::sessions::resolve(&st.db, token.trim()).await {
        Some(a) if !a.banned && a.roles.iter().any(|r| r == "admin") => Ok(format!("admin:{}", a.open_id)),
        Some(_) => Err(deny(StatusCode::FORBIDDEN, "no admin role")),
        None => Err(deny(StatusCode::UNAUTHORIZED, "session expired")),
    }
}

async fn players(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let mut out = Vec::new();
    for open_id in crate::lobby::hub::online_ids() {
        let p = st.db.get::<PlayerModel>("players", &open_id).await;
        out.push(json!({
            "open_id": open_id,
            "user_id": p.as_ref().map(|p| p.user_id),
            "nick": p.as_ref().and_then(|p| p.roles.iter().find(|r| r.role_guid == p.hall_role_guid).map(|r| r.role_name.clone())),
            "hall_role_guid": p.as_ref().map(|p| p.hall_role_guid),
        }));
    }
    Json(Value::Array(out)).into_response()
}

async fn gm(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    let actor = match check(&st, &peer, &h).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let Some(cmd) = body["cmd"].as_str().map(str::to_string) else {
        return deny(StatusCode::BAD_REQUEST, "no cmd");
    };
    let open_id = match body["open_id"].as_str() {
        Some(o) => o.to_string(),
        None => {
            let online = crate::lobby::hub::online_ids();
            match online.as_slice() {
                [one] => one.clone(),
                [] => return deny(StatusCode::BAD_REQUEST, "nobody online — specify open_id"),
                _ => return deny(StatusCode::BAD_REQUEST, "several players online — specify open_id"),
            }
        }
    };
    if st.db.get::<PlayerModel>("players", &open_id).await.is_none() {
        return deny(StatusCode::NOT_FOUND, "no player with this open_id");
    }
    let session = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: open_id.clone(),
        peer_ip: "admin-api".into(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    let rsp = session.gm_exec(&[cmd], &actor).await;
    // changes reach the player via normal pushes if they are online
    let pushes = std::mem::take(&mut *session.after_pushes.lock().unwrap());
    let online = crate::lobby::hub::is_online(&open_id);
    for (method, type_url, body) in pushes {
        crate::lobby::hub::push_raw(&open_id, &method, &type_url, body);
    }
    Json(json!({
        "open_id": open_id, "online": online,
        "result_code": rsp.result_code, "result_message": rsp.result_message, "result_list": rsp.result_list,
    }))
    .into_response()
}

// ext: toast — `{text, open_id}`: one player (`online` in the reply — delivered now); `open_id: "online"` or none — everyone online
async fn toast(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let Some(text) = body["text"].as_str().map(str::trim).filter(|t| !t.is_empty()) else {
        return deny(StatusCode::BAD_REQUEST, "no text");
    };
    match body["open_id"].as_str().filter(|o| *o != "online") {
        Some(open_id) => match crate::rpc::handlers::toast_to(&st.db, open_id, text).await {
            Ok(online) => Json(json!({ "open_id": open_id, "online": online })).into_response(),
            Err(e) => deny(StatusCode::NOT_FOUND, &e),
        },
        None => Json(json!({ "delivered": crate::rpc::handlers::toast_online(&st.db, text).await })).into_response(),
    }
}

// ---------------------------------------------------------------- commands to the game server (host)
//
// dev-app -> backend -> host. Host — a headless game instance with the DungeonborneRuServer mod: reads
// `<root>/dbserver-control.txt`, writes `dbserver-status.json` and `dbserver-command-result.json`
// (`<root>` = the server's `-dbroot`, env `DB_HOST_ROOT`, default dbtools\run). This is the local host transport;
// for a remote one (dedicated server) — the same contract via ds-agent (`/ds/next`), once there is an agent.

/// Commands the backend passes to the host (the mod will not execute anything else, or it is dangerous: quit — via dev-app)
const HOST_VERBS: [&str; 5] = ["portal", "exits", "probe", "kill", "leave"];

fn host_root() -> std::path::PathBuf {
    std::env::var("DB_HOST_ROOT").unwrap_or_else(|_| r"dbtools/run".into()).into()
}

fn read_json(path: &std::path::Path) -> Value {
    std::fs::read_to_string(path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null)
}

/// Launcher lobby on behalf of a player (tab sign-up, vote, leave) — like `/api/launcher/v1/lobby*`, but without a session
async fn lobby(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    use crate::lobby::launcher_lobby as ll;
    let Some(open_id) = body["open_id"].as_str().filter(|s| !s.is_empty()) else {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "open_id required" }))).into_response();
    };
    if !ll::enabled() {
        return (StatusCode::CONFLICT, Json(json!({ "error": "LAUNCHER_LOBBY is off" }))).into_response();
    }
    let res = match body["action"].as_str().unwrap_or("state") {
        "join" => ll::join_async(open_id, body["instance"].as_u64().unwrap_or(1) as usize).await,
        "vote" => ll::vote(open_id, body["party_size"].as_u64().map(|n| n as usize), body["level_id"].as_i64()),
        "leave" => {
            ll::leave(open_id);
            Ok(())
        }
        _ => Ok(()),
    };
    match res {
        Ok(()) => Json(ll::snapshot_async(open_id).await).into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e.code(), "state": ll::snapshot(open_id) }))).into_response(),
    }
}

// ---------------------------------------------------------------- bug reports for the Discord bot (src/bug_reports.rs)

use crate::bug_reports as bugs;
use axum::extract::{Path, Query};

/// `?status=new&limit=10` → reports, oldest first (contract bug-report-api.md §2)
async fn bug_reports_list(
    State(st): State<St>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let status = q.get("status").map(String::as_str).unwrap_or("new");
    if !matches!(status, "new" | "posted") {
        return deny(StatusCode::BAD_REQUEST, "status: new | posted");
    }
    let limit = q.get("limit").and_then(|s| s.parse::<usize>().ok()).unwrap_or(10).clamp(1, 50);
    match bugs::list(&st.db, status, limit).await {
        Ok(list) => Json(Value::Array(list.iter().map(bugs::admin_json).collect())).into_response(),
        Err(e) => {
            eprintln!("[BugReport] list: {}", e);
            deny(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable")
        }
    }
}

/// Report file as is
async fn bug_report_file(
    State(st): State<St>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path((id, name)): Path<(String, String)>,
) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let Some(path) = bugs::get(&st.db, &id).await.and_then(|r| bugs::file_path(&bugs::dir(), &r, &name)) else {
        return deny(StatusCode::NOT_FOUND, "no such report or file");
    };
    match tokio::fs::read(&path).await {
        Ok(bytes) => ([(axum::http::header::CONTENT_TYPE, "application/octet-stream")], bytes).into_response(),
        Err(_) => deny(StatusCode::NOT_FOUND, "file already deleted"),
    }
}

/// The bot published a post: `{thread_id, url}` → 204, the report is no longer in `status=new`. A repeat is also 204
async fn bug_report_posted(
    State(st): State<St>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    h: HeaderMap,
    Path(id): Path<String>,
    body: Option<Json<Value>>,
) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let body = body.map(|Json(v)| v).unwrap_or_default();
    let (Some(thread), Some(url)) = (body["thread_id"].as_str(), body["url"].as_str()) else {
        return deny(StatusCode::BAD_REQUEST, "{thread_id, url} required");
    };
    match bugs::mark_posted(&st.db, &id, thread, url, bugs::now()).await {
        Ok(Some(())) => {
            println!("[BugReport] {} published: {}", id, url);
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(None) => deny(StatusCode::NOT_FOUND, "no such report"),
        Err(e) => {
            eprintln!("[BugReport] posted {}: {}", id, e);
            deny(StatusCode::INTERNAL_SERVER_ERROR, "database unavailable")
        }
    }
}

/// 10.11: game server health — the ds::watchdog (agent, slot heartbeats)
async fn ds_health(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    Json(crate::ds::watchdog::health()).into_response()
}

/// DB export folder: `DB_EXPORT_DIR`, default `db_exports` in the working folder
fn export_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("DB_EXPORT_DIR").unwrap_or_else(|_| "db_exports".into()))
}

async fn export_json(db: &GameDb) -> Value {
    match db.export_to(&export_dir()).await {
        Ok((path, bytes, secs)) => json!({ "path": path.display().to_string(), "bytes": bytes, "secs": secs }),
        Err(e) => json!({ "error": e }),
    }
}

/// 16.11: `POST /admin/v1/db/export` — consistent DB copy without stopping (`GameDb::export_to`)
async fn db_export(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let r = export_json(&st.db).await;
    let code = if r.get("error").is_some() { StatusCode::CONFLICT } else { StatusCode::OK };
    (code, Json(r)).into_response()
}

async fn host_status(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap) -> Response {
    if let Err(r) = check(&st, &peer, &h).await {
        return r;
    }
    let root = host_root();
    Json(json!({
        "root": root.display().to_string(),
        "status": read_json(&root.join("dbserver-status.json")),
        "last_command": read_json(&root.join("dbserver-command-result.json")),
    }))
    .into_response()
}

async fn host_command(State(st): State<St>, ConnectInfo(peer): ConnectInfo<SocketAddr>, h: HeaderMap, Json(body): Json<Value>) -> Response {
    let actor = match check(&st, &peer, &h).await {
        Ok(a) => a,
        Err(r) => return r,
    };
    let cmd = body["cmd"].as_str().unwrap_or("").trim().to_string();
    let verb = cmd.split_whitespace().next().unwrap_or("");
    if !HOST_VERBS.contains(&verb) || cmd.len() > 64 || cmd.contains(['\r', '\n']) {
        return deny(StatusCode::BAD_REQUEST, &format!("host command: {} [argument]", HOST_VERBS.join(" | ")));
    }
    let root = host_root();
    let status = read_json(&root.join("dbserver-status.json"));
    if status.is_null() {
        return deny(StatusCode::SERVICE_UNAVAILABLE, "host not found (no dbserver-status.json in DB_HOST_ROOT)");
    }
    let result_path = root.join("dbserver-command-result.json");
    let before = read_json(&result_path);
    if let Err(e) = std::fs::write(root.join("dbserver-control.txt"), &cmd) {
        return deny(StatusCode::INTERNAL_SERVER_ERROR, &format!("control: {}", e));
    }
    println!("[Admin] {} -> host: {}", actor, cmd);
    // the mod reads control every ~1 s and executes in the game thread; probe does not write its result to the file
    let mut result = Value::Null;
    if verb != "probe" {
        for _ in 0..40 {
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            let now = read_json(&result_path);
            if !now.is_null() && now != before {
                result = now;
                break;
            }
        }
    }
    Json(json!({ "sent": cmd, "host_phase": status["phase"], "map": status["map"], "result": result })).into_response()
}

/// Sh4g beacon (05.11, debugging): while a player is in a match room, every RAID_PUSH_BEACON_SECS s — a dirty_role push of their
/// hall role (unchanged). The client's ClientProbe writes a line for each received RoleInfoNoti push — where these lines stop
/// shows from which second the client stops accepting pushes (result, CANCEL, room cleanup do not reach the services).
fn spawn_push_beacon(db: GameDb) {
    let Some(secs) = std::env::var("RAID_PUSH_BEACON_SECS").ok().and_then(|v| v.parse::<u64>().ok()).filter(|s| *s > 0) else {
        return;
    };
    println!("[Beacon] in-match push beacon: every {} s (RAID_PUSH_BEACON_SECS)", secs);
    tokio::spawn(async move {
        let mut n: u64 = 0;
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(secs)).await;
            // RAID_PUSH_BEACON_ALL=1 — to everyone online (also in the lobby): checks "receiving stalls ~100 s after login"
            let all = std::env::var("RAID_PUSH_BEACON_ALL").map(|v| v == "1").unwrap_or(false);
            for open_id in crate::lobby::hub::online_ids() {
                let battle = crate::lobby::matchmaking::room_of_player(&open_id);
                if battle.is_none() && !all {
                    continue;
                }
                let battle = battle.unwrap_or_else(|| "lobby".into());
                let Some(p) = db.get::<PlayerModel>("players", &open_id).await else { continue };
                let Some(r) = p.roles.iter().find(|r| r.role_guid == p.hall_role_guid) else { continue };
                n += 1;
                let msg = crate::tgf::ScPlayerDirtyChgSync { dirty_role: vec![r.data.clone()], ..Default::default() };
                let ok = crate::lobby::hub::push_to(&open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &msg);
                println!("[Beacon] #{} {} (room {}): dirty_role -> online={}", n, open_id, battle, ok);
            }
        }
    });
}

/// 13.11: GM commands via files — for an orchestrator over ssh without an admin session (user: "give him his items back and do
/// the command"). Folder `ADMIN_CMD_DIR` (default `admin_cmd`, on the VM root only): file `<name>.cmd` = a line
/// `<open_id> <command as in /admin/v1/gm>` or `db_export` (DB copy). Executed every 3 s, reply — `<name>.result` (JSON), the file itself →
/// `<name>.done`. Same as the maintenance file: access to the folder = access to the server.
fn spawn_file_commands(db: GameDb, game_data: Arc<GameData>) {
    let dir = std::path::PathBuf::from(std::env::var("ADMIN_CMD_DIR").unwrap_or_else(|_| "admin_cmd".into()));
    if !dir.is_dir() {
        return;
    }
    println!("[AdminFile] commands from {}/*.cmd", dir.display());
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let mut files: Vec<std::path::PathBuf> = std::fs::read_dir(&dir)
                .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|x| x == "cmd")).collect())
                .unwrap_or_default();
            files.sort();
            for path in files {
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                let _ = std::fs::rename(&path, path.with_extension("done"));
                let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
                let (open_id, cmd) = line.split_once(' ').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())).unwrap_or_default();
                let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                // 16.11: line `db_export` — DB copy without stopping (before a deploy), reply — file path
                let result = if line == "db_export" {
                    export_json(&db).await
                } else if open_id.is_empty() || cmd.is_empty() {
                    json!({ "error": "line required: <open_id> <command>" })
                } else if db.get::<PlayerModel>("players", &open_id).await.is_none() {
                    json!({ "error": "no player with this open_id", "open_id": open_id })
                } else {
                    let session = GatewaySession {
                        db: db.clone(),
                        game_data: game_data.clone(),
                        open_id: open_id.clone(),
                        peer_ip: "admin-file".into(),
                        after_pushes: std::sync::Mutex::new(Vec::new()),
                        rpc_error: std::sync::atomic::AtomicI32::new(0),
                    };
                    let rsp = session.gm_exec(&[cmd.clone()], &format!("file:{}", name)).await;
                    for (method, type_url, body) in std::mem::take(&mut *session.after_pushes.lock().unwrap()) {
                        crate::lobby::hub::push_raw(&open_id, &method, &type_url, body);
                    }
                    json!({ "open_id": open_id, "cmd": cmd, "result_code": rsp.result_code, "result_message": rsp.result_message, "result_list": rsp.result_list })
                };
                println!("[AdminFile] {}: {}", name, result);
                let _ = std::fs::write(path.with_extension("result"), serde_json::to_string_pretty(&result).unwrap_or_default());
            }
        }
    });
}

pub async fn run(db: GameDb, game_data: Arc<GameData>) {
    spawn_push_beacon(db.clone());
    spawn_file_commands(db.clone(), game_data.clone());
    let bind = std::env::var("ADMIN_BIND").unwrap_or_else(|_| "127.0.0.1:9092".into());
    let app = Router::new()
        .route("/admin/v1/players", get(players))
        .route("/admin/v1/gm", post(gm))
        .route("/admin/v1/toast", post(toast)) // ext: toast
        .route("/admin/v1/host/status", get(host_status))
        .route("/admin/v1/host/command", post(host_command))
        .route("/admin/v1/lobby", post(lobby))
        .route("/admin/v1/ds/health", get(ds_health))
        .route("/admin/v1/db/export", post(db_export))
        .route("/admin/v1/bug-reports", get(bug_reports_list))
        .route("/admin/v1/bug-reports/:id/files/:name", get(bug_report_file))
        .route("/admin/v1/bug-reports/:id/posted", post(bug_report_posted))
        .with_state(St { db, game_data });
    match tokio::net::TcpListener::bind(&bind).await {
        Ok(l) => {
            println!("[Admin] admin API on http://{} (loopback only)", bind);
            if let Err(e) = axum::serve(l, app.into_make_service_with_connect_info::<SocketAddr>()).await {
                eprintln!("[Admin] stopped: {}", e);
            }
        }
        Err(e) => eprintln!("[Admin] failed to bind {}: {}", bind, e),
    }
}
