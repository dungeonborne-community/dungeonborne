//! HTTP for ds-agent. The agent sits next to the game server and calls the backend itself (pull — convenient through NAT
//! Hetzner → GKE). Bodies are JSON, proto messages inside are hex (`payload_hex`).
//!
//!   POST /ds/register {agent_id, ds_address, maps?}   register/update; the server joins the allocator
//!   GET  /ds/next?agent_id=..                          long poll (up to 20 s): backend → DS command
//!        200 {cmd_id, battle_id, method, payload_hex} | 204 — no commands
//!   POST /ds/reply {cmd_id, payload_hex?, error?}      DS reply to a command (DsRpcService: init_player…)
//!   POST /ds/event {agent_id, method, payload_hex}     DS → backend call (DsBattleService) → {type, payload_hex}
//!   GET  /ds/agents                                    agent list (debugging)
//!   POST /ds/slot {port, slot?, phase, epoch, battle_id?, absent?, players?, ws_mb?, private_mb?}  mod heartbeat
//!        → {action: none|reset|host|quit, battle_id?, host?} — reset after the room closes, waiting-hall assignment
//!        (lobby::allocator, docs/WAITING_ROOM.md)
//!   GET  /ds/slots                                     allocator slots (debugging)
//!   POST /ds/kill {killer, victim, session?}           player killed by a player (season flag kill_reward, Sh4)
//!   POST /ds/quest_progress {open_id, monsters, chests, damage}  increments for pass quests (monsters/chests/damage)
//!   POST /ds/arena_result {battle_id, winner, players:[{open_id, team, kills, deaths, damage, heal, taken}], rounds?:[{round, winner}]}
//!        PVP arena match results → battle_settle_sync to all participants (score table in the lobby, docs/PVP_SETTLE.md)
//!
//! Address: DS_AGENT_BIND (127.0.0.1; on GKE — 0.0.0.0), port DS_AGENT_PORT (9091).

use super::{from_hex, to_hex};
use crate::db::GameDb;
use crate::game_data::GameData;
use crate::rpc::handlers::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use prost::Message;
use axum::extract::{ConnectInfo, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{oneshot, Notify};

/// DsBattleService methods the agent may call via /ds/event (anything else is refused)
const EVENT_METHODS: &[&str] = &[
    "player_game_over",
    "ds_end",
    "player_online_heartbeat",
    "player_online_report",
    "ds_alias_status_report",
    "dsc_status_report",
    "create_game_session_result_event",
    "game_session_end_event",
    "hello_world",
];

const POLL_SECS: u64 = 20;
/// An agent that has not polled for longer than this is considered gone: no commands are queued for it
const AGENT_STALE_SECS: u64 = 60;

#[derive(Clone, Debug)]
pub struct Command {
    pub cmd_id: u64,
    pub battle_id: String,
    pub method: String,
    pub payload: Vec<u8>,
}

struct Agent {
    ds_address: String,
    maps: Vec<String>,
    last_seen: Instant,
    queue: VecDeque<Command>,
}

#[derive(Default)]
struct Link {
    agents: HashMap<String, Agent>,
    pending: HashMap<u64, oneshot::Sender<Result<Vec<u8>, String>>>,
    next_cmd: u64,
}

fn link() -> &'static Mutex<Link> {
    static LINK: OnceLock<Mutex<Link>> = OnceLock::new();
    LINK.get_or_init(|| Mutex::new(Link::default()))
}

fn wakeup() -> &'static Notify {
    static N: OnceLock<Notify> = OnceLock::new();
    N.get_or_init(Notify::new)
}

/// Live agent that holds this server address
pub fn agent_for(ds_address: &str) -> Option<String> {
    let l = link().lock().unwrap();
    l.agents
        .iter()
        .find(|(_, a)| a.ds_address == ds_address && a.last_seen.elapsed() < Duration::from_secs(AGENT_STALE_SECS))
        .map(|(id, _)| id.clone())
}

/// Queues a command for the agent; the reply comes into the returned channel (via /ds/reply). None — no agent.
pub fn enqueue(agent_id: &str, battle_id: &str, method: &str, payload: Vec<u8>) -> Option<(u64, oneshot::Receiver<Result<Vec<u8>, String>>)> {
    let mut l = link().lock().unwrap();
    l.next_cmd += 1;
    let cmd_id = l.next_cmd;
    let agent = l.agents.get_mut(agent_id)?;
    agent.queue.push_back(Command { cmd_id, battle_id: battle_id.to_string(), method: method.to_string(), payload });
    let (tx, rx) = oneshot::channel();
    l.pending.insert(cmd_id, tx);
    drop(l);
    wakeup().notify_waiters();
    Some((cmd_id, rx))
}

/// Gave up waiting for the reply — drop the pending entry (a late /ds/reply will simply find nothing)
pub fn forget(cmd_id: u64) {
    link().lock().unwrap().pending.remove(&cmd_id);
}

#[derive(Clone)]
struct AppState {
    db: GameDb,
    game_data: Arc<GameData>,
}

pub async fn run(db: GameDb, game_data: Arc<GameData>) -> std::io::Result<()> {
    let bind = std::env::var("DS_AGENT_BIND").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("DS_AGENT_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(9091);
    let app = Router::new()
        .route("/ds/register", post(register))
        .route("/ds/next", get(next))
        .route("/ds/reply", post(reply))
        .route("/ds/event", post(event))
        .route("/ds/agents", get(agents))
        .route("/ds/battle_user_data", get(battle_user_data))
        .route("/ds/player_result", post(player_result))
        .route("/ds/floor_snapshot", post(floor_snapshot))
        .route("/ds/player_left", post(player_left).get(player_left))
        .route("/ds/team_of", get(team_of))
        .route("/ds/slot", post(slot_heartbeat))
        .route("/ds/kill", post(kill))
        .route("/ds/quest_progress", post(quest_progress))
        .route("/ds/arena_result", post(arena_result))
        .route("/ds/slots", get(slots))
        .with_state(AppState { db, game_data });
    let listener = tokio::net::TcpListener::bind((bind.as_str(), port)).await?;
    println!("[DS-Link] ds-agent HTTP on {}:{}", bind, port);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await
}

fn bad(msg: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": msg }))).into_response()
}

fn str_field<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(|x| x.as_str()).filter(|s| !s.is_empty())
}

async fn register(Json(body): Json<Value>) -> Response {
    let (Some(agent_id), Some(addr)) = (str_field(&body, "agent_id"), str_field(&body, "ds_address")) else {
        return bad("agent_id and ds_address required");
    };
    let maps: Vec<String> = body
        .get("maps")
        .and_then(|m| m.as_array())
        .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    {
        let mut l = link().lock().unwrap();
        let agent = l.agents.entry(agent_id.to_string()).or_insert_with(|| Agent {
            ds_address: addr.to_string(),
            maps: Vec::new(),
            last_seen: Instant::now(),
            queue: VecDeque::new(),
        });
        agent.ds_address = addr.to_string();
        agent.maps = maps.clone();
        agent.last_seen = Instant::now();
    }
    // One slot per agent: the map is the first from the list, empty — any
    crate::lobby::allocator::register(addr, maps.first().cloned());
    println!("[DS-Link] agent {} registered: {} maps {:?}", agent_id, addr, maps);
    Json(json!({ "ok": true })).into_response()
}

async fn next(Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(agent_id) = q.get("agent_id").filter(|s| !s.is_empty()).cloned() else { return bad("agent_id required") };
    let deadline = Instant::now() + Duration::from_secs(POLL_SECS);
    loop {
        // Subscribe before checking the queue: a command queued between the check and the wait is not lost
        let notified = wakeup().notified();
        {
            let mut l = link().lock().unwrap();
            let Some(agent) = l.agents.get_mut(&agent_id) else {
                return (StatusCode::NOT_FOUND, Json(json!({ "error": "agent not registered" }))).into_response();
            };
            agent.last_seen = Instant::now();
            if let Some(cmd) = agent.queue.pop_front() {
                return Json(json!({
                    "cmd_id": cmd.cmd_id,
                    "battle_id": cmd.battle_id,
                    "method": cmd.method,
                    "payload_hex": to_hex(&cmd.payload),
                }))
                .into_response();
            }
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || tokio::time::timeout(left, notified).await.is_err() {
            return StatusCode::NO_CONTENT.into_response();
        }
    }
}

async fn reply(Json(body): Json<Value>) -> Response {
    let Some(cmd_id) = body.get("cmd_id").and_then(|v| v.as_u64()) else { return bad("cmd_id required") };
    let result = match (str_field(&body, "error"), str_field(&body, "payload_hex")) {
        (Some(err), _) => Err(err.to_string()),
        (None, Some(hex)) => from_hex(hex),
        (None, None) => Ok(Vec::new()),
    };
    let tx = link().lock().unwrap().pending.remove(&cmd_id);
    match tx {
        Some(tx) => {
            let _ = tx.send(result);
            Json(json!({ "ok": true })).into_response()
        }
        None => (StatusCode::GONE, Json(json!({ "error": "command is not awaiting a reply (timeout?)" }))).into_response(),
    }
}

async fn event(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let (Some(agent_id), Some(method)) = (str_field(&body, "agent_id"), str_field(&body, "method")) else {
        return bad("agent_id and method required");
    };
    if !EVENT_METHODS.contains(&method) {
        return bad("method is not from DsBattleService");
    }
    let payload = match from_hex(str_field(&body, "payload_hex").unwrap_or("")) {
        Ok(p) => p,
        Err(e) => return bad(&e),
    };
    if let Some(a) = link().lock().unwrap().agents.get_mut(agent_id) {
        a.last_seen = Instant::now();
    }
    // Same router as the gateway: DsBattleService handlers in rpc/handlers/ds_handlers.rs
    let session = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: format!("dsagent{}", agent_id),
        peer_ip: "ds-agent".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    println!("[DS-Link] event {} from agent {} ({} bytes)", method, agent_id, payload.len());
    match crate::game_data::route_rpc(&session, method, &payload).await {
        Some((rsp_type, bytes)) => Json(json!({ "type": rsp_type, "payload_hex": to_hex(&bytes) })).into_response(),
        None => (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": "not parsed / no reply" }))).into_response(),
    }
}

/// One player's match data for a listen server: the native helper dbnative (mod DungeonborneRuClient) puts it into
/// UBattleDsUtility under this player's key before the native InitPlayerGameData. Body — DBattleUserData (prost) with
/// ds_player_key.ds_user_key = open_id; 404 — no such player in the DB.
async fn battle_user_data(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(open_id) = q.get("open_id").filter(|s| !s.is_empty()) else {
        return bad("open_id required");
    };
    let Some(mut player) = st.db.get::<PlayerModel>("players", open_id).await else {
        println!("[DS-Link] battle_user_data: no player {}", open_id);
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no player" }))).into_response();
    };
    // 12.11: marked "did not come to the hall" — the client has already been sent the room end; not let into the raid (the mod kicks on refusal)
    if let Some(battle) = crate::lobby::matchmaking::missed_hall(open_id) {
        println!("[DS-Link] battle_user_data {}: did not come to hall {} before the match started — refused", open_id, battle);
        return (StatusCode::CONFLICT, Json(json!({ "error": "hall_missed" }))).into_response();
    }
    let session = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: open_id.clone(),
        peer_ip: "ds-native".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    // 09.11: first entry into the raid (not a reconnect, not the next floor) — gear over the map's rarity limit goes to
    // the stash: after "Start" in the lobby items can still be moved into the backpack or equipped
    let fresh = player.state["raid_entry"]["applied"] != json!(false) || player.state["raid_entry"]["role_guid"].as_u64() != Some(player.hall_role_guid);
    if fresh {
        if let Some(level) = crate::lobby::matchmaking::room_level(open_id) {
            let moved = session.raid_gear_gate(&mut player, level);
            if !moved.is_empty() {
                println!("[DS-Link] battle_user_data {}: gear over the limit of map {} — to stash: {:?}", open_id, level, moved);
                st.db.save("players", open_id, &player).await;
            }
        }
    }
    let mut u = session.local_battle_user_data(&mut player).await;
    // moving to the next floor: items come from the snapshot the server took before the move (first-floor loot), not from the profile
    if let Some(preview) = crate::lobby::matchmaking::room_of_player(open_id).and_then(|b| floor_snapshot_of(open_id, &b)) {
        println!("[DS-Link] battle_user_data {}: items from the floor snapshot", open_id);
        u.deploy_preview = Some(preview);
    }
    // 11.11 arenas (custom-game DM and "Klotho"): own consumables and backpack do not go into the match — only equipped
    // gear. The profile does not change (the arena result does not touch items, raid_native). ARENA_STRIP_ITEMS=0 — disable
    let arena = crate::lobby::matchmaking::room_of_player(open_id).and_then(|b| crate::lobby::matchmaking::room(&b)).and_then(|r| r.arena_map_id);
    if let (Some(map), true) = (arena, std::env::var("ARENA_STRIP_ITEMS").as_deref() != Ok("0")) {
        if let Some(d) = u.deploy_preview.as_mut().and_then(|p| p.deploy_role.as_mut()) {
            let n = crate::rpc::handlers::arena_strip(d);
            println!("[DS-Link] battle_user_data {}: arena {} — no consumables or backpack, items removed {}", open_id, map, n);
        }
    }
    // "issued for the raid" record for the native result (/ds/player_result, rpc/handlers/raid_native.rs)
    // re-entering the same raid after the result (dbnative passes the server session) — no match data
    let session = q.get("session").cloned().unwrap_or_default();
    // the player's room is closed, there is no new match — no match data given: 03.11 14:46 a server reset to the hall pulled two
    // still-connected players along, the server gave them data again, and leaving the hall produced a second "self-exit" result (−9 items)
    let in_room = matches!(crate::lobby::matchmaking::status(open_id), Some(crate::lobby::matchmaking::PlayerStatus::Matched { .. }));
    if !in_room && crate::lobby::matchmaking::closed_room_of(open_id).is_some() {
        println!("[DS-Link] battle_user_data {}: room closed, no new match — refused", open_id);
        return (StatusCode::CONFLICT, Json(json!({ "error": "room_closed" }))).into_response();
    }
    if GatewaySession::raid_finished_in_session(&player, &session) {
        println!("[DS-Link] battle_user_data {}: result in session {} already applied — refused (re-entry)", open_id, session);
        return (StatusCode::CONFLICT, Json(json!({ "error": "raid_finished" }))).into_response();
    }
    // entry token: the one the player got in the room (ds_init_finish) and passed in the connect address (dbnative &token=).
    // No token (dbtest: dbclient-urlopts.txt without token) — let in while the server is not public
    // ([auth] require = "relay" or DS_REQUIRE_TOKEN=1 — strict).
    let sent = q.get("token").cloned().unwrap_or_default();
    let room_token = crate::lobby::matchmaking::token_of(open_id);
    let strict = crate::auth::require_relay() || std::env::var("DS_REQUIRE_TOKEN").map(|v| v == "1").unwrap_or(false);
    let verified = match join_token_check(&sent, room_token.as_deref(), strict) {
        Ok(v) => v,
        Err(why) => {
            println!("[DS-Link] battle_user_data {}: refused — {}", open_id, why);
            // the server kicks the player (mod RuClient KickRemote), and the client would reconnect into the same room by itself
            // (native reconnect while BATTLING) — refused again, in a loop (LOCAL analysis 03.11). Remove them from the room:
            // do not wait for a result, the client gets "match over".
            if let Some(battle) = crate::lobby::matchmaking::room_of_player(open_id) {
                let (marked, _) = crate::lobby::matchmaking::members_absent(&battle, std::slice::from_ref(open_id), false);
                for o in marked {
                    crate::rpc::handlers::room_finish_push(&o, &battle, None, None);
                }
            }
            return (StatusCode::FORBIDDEN, Json(json!({ "error": why }))).into_response();
        }
    };
    if sent.is_empty() {
        println!("[DS-Link] battle_user_data {}: no token (local mode)", open_id);
    }
    let hall = player.hall_role_guid;
    let token = GatewaySession::raid_entry_issue(&mut player, hall, &session, verified.as_deref());
    st.db.save("players", open_id, &player).await;
    u.ds_player_key = Some(tgf::DBattlePlayerDsKey { ds_user_key: open_id.clone(), ds_token: token, ..Default::default() });
    let bytes = u.encode_to_vec();
    crate::lobby::matchmaking::mark_arrived(open_id);
    println!("[DS-Link] battle_user_data {} → {} bytes (role_guid={})", open_id, bytes.len(), player.hall_role_guid);
    ([(axum::http::header::CONTENT_TYPE, "application/x-protobuf")], bytes).into_response()
}

/// The player's team in the match without issuing match data (the server sets it before choosing the spawn point, ChoosePlayerStart):
/// `GET /ds/team_of?open_id=` → `{"team_id": N}` — same as `DBattleUserParameter.team_id` in `battle_user_data`:
/// the room's team number, outside a room the party or player id.
async fn team_of(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(open_id) = q.get("open_id").filter(|s| !s.is_empty()) else {
        return bad("open_id required");
    };
    let Some(player) = st.db.get::<PlayerModel>("players", open_id).await else {
        return (StatusCode::NOT_FOUND, Json(json!({ "error": "no player" }))).into_response();
    };
    let team = crate::lobby::matchmaking::team_number(open_id).unwrap_or_else(|| crate::lobby::team::battle_team_id(player.user_id));
    Json(json!({ "team_id": team })).into_response()
}

/// A player's raid result from the server (dbnative): a `DBattleUserData` record with status GAMEOVER.
/// Body — raw protobuf (`Content-Type: application/x-protobuf`) with `?open_id=&status=&reason=`, or JSON
/// `{open_id, status, reason, payload_hex}`. status — EnDSPlayerGameStatusType (3), reason — EnDSPlayerActorStatusType.
/// Reply 200 `{ok, applied, …}`; 400 — cannot parse; 404 — no player; 409 — nothing to apply to / already applied.
async fn player_result(State(st): State<AppState>, Query(q): Query<HashMap<String, String>>, headers: axum::http::HeaderMap, body: axum::body::Bytes) -> Response {
    let is_json = headers.get(axum::http::header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).map(|v| v.contains("json")).unwrap_or(false);
    let session = q.get("session").cloned().unwrap_or_default();
    // leaving the waiting hall (mod: Host.Phase waiting/entering) — like native finish_scenes = 1 WAITING_ROOM (B12)
    let mut hall = q.get("scene").map_or(false, |s| s == "waiting_room");
    let (open_id, status, reason, payload) = if is_json {
        let Ok(v) = serde_json::from_slice::<Value>(&body) else { return bad("JSON not parsed") };
        let Some(bytes) = v["payload_hex"].as_str().and_then(|h| from_hex(h).ok()) else { return bad("no payload_hex") };
        hall |= v["scene"].as_str() == Some("waiting_room");
        (v["open_id"].as_str().unwrap_or("").to_string(), v["status"].as_i64().unwrap_or(0) as i32, v["reason"].as_i64().unwrap_or(0) as i32, bytes)
    } else {
        let num = |k: &str| q.get(k).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
        (q.get("open_id").cloned().unwrap_or_default(), num("status"), num("reason"), body.to_vec())
    };
    if open_id.is_empty() {
        return bad("open_id required");
    }
    let Ok(data) = tgf::DBattleUserData::decode(payload.as_slice()) else { return bad("payload is not DBattleUserData") };
    let gs = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: open_id.clone(),
        peer_ip: "ds-native".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    // potion exp for the match (mod: PlayerAddExp hook); on a dedicated server this would be DDsUserBattleInfo.add_exp
    let bonus_exp = q.get("bonus_exp").and_then(|s| s.parse::<i32>().ok()).unwrap_or(0).max(0);
    match gs.raid_apply_native_result_in(&open_id, status, reason, &session, data, hall, bonus_exp).await {
        Ok(mut summary) => {
            summary["ok"] = json!(true);
            Json(summary).into_response()
        }
        Err(crate::rpc::handlers::NativeReject(code, why)) => {
            println!("[DS-Link] player_result {} status={} reason={}: refused {} — {}", open_id, status, reason, code, why);
            let mut body = json!({ "ok": false, "error": why });
            // result repeated for an already closed room whose server has not been reset yet — the mod starts the reset right away
            if let Some(b) = crate::lobby::matchmaking::closed_room_of(&open_id).filter(|b| crate::lobby::allocator::resetting_after(b)) {
                body["room_closed"] = json!(b);
            }
            (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), Json(body)).into_response()
        }
    }
}

/// Item snapshots when moving to the next floor: open_id -> (room, deploy_preview). In process memory: they live within
/// a room and are not applied in another room.
fn floor_snapshots() -> &'static Mutex<HashMap<String, (String, tgf::DBattleDeployPreview)>> {
    static S: OnceLock<Mutex<HashMap<String, (String, tgf::DBattleDeployPreview)>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn floor_snapshot_of(open_id: &str, battle_id: &str) -> Option<tgf::DBattleDeployPreview> {
    floor_snapshots().lock().unwrap().get(open_id).filter(|(b, _)| b == battle_id).map(|(_, p)| p.clone())
}

/// DS mod before moving to the next floor: `POST /ds/floor_snapshot?open_id=&session=`, body — a DBattleUserData record (the same
/// bytes as in the result). The result is not applied: we only remember the items, `battle_user_data` gives them out on the new floor.
async fn floor_snapshot(Query(q): Query<HashMap<String, String>>, body: axum::body::Bytes) -> Response {
    let Some(open_id) = q.get("open_id").filter(|s| !s.is_empty()) else {
        return bad("open_id required");
    };
    let Ok(data) = tgf::DBattleUserData::decode(body.as_ref()) else { return bad("body is not DBattleUserData") };
    let Some(preview) = data.deploy_preview.filter(|p| p.deploy_role.is_some()) else {
        return bad("record has no deploy_role");
    };
    let Some(battle) = crate::lobby::matchmaking::room_of_player(open_id) else {
        println!("[DS-Link] floor_snapshot {}: player not in a room", open_id);
        return (StatusCode::CONFLICT, Json(json!({ "ok": false, "error": "no room" }))).into_response();
    };
    // 10.11: resurrection stones (100014, 100046 — "cannot be extracted, but can go to a floor") — did they reach the snapshot
    let role = preview.deploy_role.as_ref();
    let all: Vec<&tgf::DItem> = role
        .map(|r| r.backpack.iter().flat_map(|b| b.items.iter()).chain(r.wearing.iter().flat_map(|w| w.items.iter())).collect())
        .unwrap_or_default();
    let stones: Vec<String> = all
        .iter()
        .filter_map(|i| i.basic.as_ref())
        .filter(|b| matches!(b.type_id, 100014 | 100046))
        .map(|b| format!("{}x{}", b.type_id, b.count))
        .collect();
    println!("[DS-Link] floor_snapshot {} room {}: {} bytes, items {}, resurrection stones: {}", open_id, battle, body.len(),
        all.len(), if stones.is_empty() { "none".to_string() } else { stones.join(", ") });
    floor_snapshots().lock().unwrap().insert(open_id.clone(), (battle.clone(), preview));
    Json(json!({ "ok": true, "battle_id": battle, "bytes": body.len() })).into_response()
}

/// DS mod: the player's connection closed after their result — the client left the raid map and is loading the lobby (Sh4f, agent 04.11).
/// Deferred result pushes (dirty_role, quests, mail, battle_settle_sync — hub::set_defer) go out now: they arrive before
/// WBP_HallWindow opens (the results screen is checked only on open), but no longer while leaving the raid map, when
/// the client stopped accepting them. If this never comes — player_get_info delivers them (without the results window).
async fn player_left(Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(open_id) = q.get("open_id").filter(|s| !s.is_empty()) else {
        return bad("open_id required");
    };
    // RAID_FLUSH_ON_LEFT=0 — deferred pushes are delivered only by player_get_info in the lobby (experiment Sh4g, 05.11)
    if std::env::var("RAID_FLUSH_ON_LEFT").map(|v| v == "0").unwrap_or(false) {
        println!("[DS-Link] player_left {} — deferred pushes wait for player_get_info (RAID_FLUSH_ON_LEFT=0)", open_id);
        return Json(json!({ "ok": true, "sent": 0, "flush": false })).into_response();
    }
    let pending = crate::lobby::hub::take_deferred(open_id);
    let n = pending.len();
    let mut sent = 0;
    for p in pending {
        sent += crate::lobby::hub::push_raw(open_id, &p.method, &p.type_url, p.body) as usize;
    }
    println!("[DS-Link] player_left {} (room {}): deferred result pushes {} → sent {}", open_id,
        q.get("battle_id").map(String::as_str).unwrap_or("?"), n, sent);
    Json(json!({ "ok": true, "sent": sent })).into_response()
}

/// Server mod heartbeat: state and map number (epoch); reply — what to do with the slot (lobby::allocator::heartbeat).
/// Always 200 (except for an unparseable body), so the mod does not confuse a refusal with the backend being down.
async fn slot_heartbeat(ConnectInfo(peer): ConnectInfo<SocketAddr>, Json(body): Json<Value>) -> Response {
    let Some(port) = body["port"].as_u64().and_then(|p| u16::try_from(p).ok()) else { return bad("port required") };
    let phase = body["phase"].as_str().unwrap_or("");
    let epoch = body["epoch"].as_u64().unwrap_or(0);
    let battle = body["battle_id"].as_str().unwrap_or("");
    // waiting hall: no-shows (absent) or an empty hall (phase=empty) — their result is not awaited, clients get "match over";
    // a room where everyone has finished is closed before the reply — the reply then carries reset right away
    let absent: Vec<String> = body["absent"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
    if !battle.is_empty() && (phase == "empty" || !absent.is_empty()) {
        let (marked, _) = crate::lobby::matchmaking::members_absent(battle, &absent, phase == "empty");
        for open_id in marked {
            crate::rpc::handlers::room_finish_push(&open_id, battle, None, None);
        }
    }
    let mut rsp = crate::lobby::allocator::heartbeat(&peer.ip().to_string(), body["slot"].as_str().unwrap_or(""), port, phase, epoch, battle);
    // R1-stuck: server restarted in the middle of a room — its match is lost, close it like an empty hall (everyone gets "match over")
    if let Some(stale) = rsp["stale_battle"].as_str().map(str::to_string) {
        let (marked, done) = crate::lobby::matchmaking::members_absent(&stale, &[], true);
        for open_id in &marked {
            crate::rpc::handlers::room_finish_push(open_id, &stale, None, None);
        }
        if !done {
            // the matchmaker no longer has the room (or it did not close) — free the slot anyway
            crate::lobby::allocator::release(&stale);
        }
        println!("[DS-Link] room {} closed: server {}:{} restarted (epoch {}), participants get \"match over\" {:?}",
            stale, peer.ip(), port, epoch, marked);
    }
    // 09.11: host command to this room's server (ban — "Hand of Justice"), one per heartbeat
    if !battle.is_empty() {
        if let Some(cmd) = take_slot_cmd(battle) {
            println!("[DS-Link] slot {}:{} room {} ← command \"{}\"", peer.ip(), port, battle, cmd);
            rsp["cmd"] = json!(cmd);
        }
    }
    if rsp["action"] != "none" {
        println!("[DS-Link] slot {}:{} phase={} epoch={} players={} → {}", peer.ip(), port, phase, epoch, body["players"], rsp);
    }
    rsp["ok"] = json!(true);
    Json(rsp).into_response()
}

/// Host commands to battle servers per room (battle_id): handed out in the /ds/slot reply — the RuServer mod executes them
/// (currently only `ban <open_id>`). Commands not picked up within 10 minutes are dropped.
fn slot_cmds() -> &'static Mutex<HashMap<String, Vec<(std::time::Instant, String)>>> {
    static C: std::sync::OnceLock<Mutex<HashMap<String, Vec<(std::time::Instant, String)>>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn queue_slot_cmd(battle_id: &str, cmd: &str) {
    slot_cmds().lock().unwrap().entry(battle_id.to_string()).or_default().push((std::time::Instant::now(), cmd.to_string()));
}

fn take_slot_cmd(battle_id: &str) -> Option<String> {
    let mut m = slot_cmds().lock().unwrap();
    m.retain(|_, v| {
        v.retain(|(t, _)| t.elapsed() < Duration::from_secs(600));
        !v.is_empty()
    });
    let list = m.get_mut(battle_id)?;
    let (_, cmd) = list.remove(0);
    if list.is_empty() {
        m.remove(battle_id);
    }
    Some(cmd)
}

/// Player killed by a player (server mod, at the moment of death): `{killer, victim, session?}` → 200 `{ok, kills}` or refusal
async fn kill(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let (Some(killer), Some(victim)) = (str_field(&body, "killer"), str_field(&body, "victim")) else {
        return bad("killer and victim required");
    };
    let gs = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: killer.to_string(),
        peer_ip: "ds-native".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    match gs.register_kill(killer, victim).await {
        Ok(n) => Json(json!({ "ok": true, "kills": n })).into_response(),
        Err(crate::rpc::handlers::NativeReject(code, why)) => {
            println!("[DS-Link] kill {} → {}: not counted — {}", killer, victim, why);
            (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), Json(json!({ "ok": false, "error": why }))).into_response()
        }
    }
}

/// PVP arena match results (11.11, RuServer mod, PVPGameWinTeam hook): score table in the lobby for all participants
async fn arena_result(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let Some(battle_id) = str_field(&body, "battle_id") else {
        return bad("battle_id required");
    };
    let winner = body["winner"].as_i64().unwrap_or(0);
    let num = |p: &Value, k: &str| p[k].as_f64().unwrap_or(0.0).clamp(0.0, 1e12);
    let stats: Vec<crate::rpc::handlers::ArenaPlayerStat> = body["players"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|p| {
                    let open_id = p["open_id"].as_str().filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))?;
                    Some(crate::rpc::handlers::ArenaPlayerStat {
                        open_id: open_id.to_string(),
                        team: p["team"].as_i64().unwrap_or(0),
                        kills: num(p, "kills").min(1e6) as i32,
                        deaths: num(p, "deaths").min(1e6) as i32,
                        damage: num(p, "damage") as i64,
                        heal: num(p, "heal") as i64,
                        taken: num(p, "taken") as i64,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let gs = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: String::new(),
        peer_ip: "ds-native".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    // BO5 rounds (PVPGameWinTeamRound): [{round, winner}]
    let rounds: Vec<(i32, i64)> = body["rounds"]
        .as_array()
        .map(|a| a.iter().filter_map(|r| Some((r["round"].as_i64()? as i32, r["winner"].as_i64()?))).collect())
        .unwrap_or_default();
    match gs.arena_settle(battle_id, winner, stats, rounds).await {
        Ok(sent) => Json(json!({ "ok": true, "sent": sent })).into_response(),
        Err(crate::rpc::handlers::NativeReject(code, why)) => {
            println!("[DS-Link] arena_result {}: not sent — {}", battle_id, why);
            (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), Json(json!({ "ok": false, "error": why }))).into_response()
        }
    }
}

/// Raid quest progress from the battle server (08.11): {open_id, monsters, chests, damage} — increments since the last send
async fn quest_progress(State(st): State<AppState>, Json(body): Json<Value>) -> Response {
    let Some(open_id) = str_field(&body, "open_id") else {
        return bad("open_id required");
    };
    let n = |k: &str| body[k].as_i64().unwrap_or(0).clamp(0, 1_000_000);
    let gs = GatewaySession {
        db: st.db.clone(),
        game_data: st.game_data.clone(),
        open_id: open_id.to_string(),
        peer_ip: "ds-native".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    };
    match gs.quest_progress_from_ds(open_id, n("monsters"), n("chests"), n("damage")).await {
        Ok(changed) => Json(json!({ "ok": true, "changed": changed })).into_response(),
        Err(crate::rpc::handlers::NativeReject(code, why)) => {
            println!("[DS-Link] quest_progress {}: not counted — {}", open_id, why);
            (StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_REQUEST), Json(json!({ "ok": false, "error": why }))).into_response()
        }
    }
}

/// Slots; with DS_AUTOSCALE=1 each has `want` — whether a server is needed on this port now (ds-agent starts the needed ones and
/// stops extra idle ones), spare — DS_MIN_IDLE (1)
async fn slots() -> Json<Value> {
    super::watchdog::agent_polled();
    let mut list = crate::lobby::allocator::snapshot();
    if std::env::var("DS_AUTOSCALE").map(|v| v == "1").unwrap_or(false) {
        let min_idle = std::env::var("DS_MIN_IDLE").ok().and_then(|v| v.parse().ok()).unwrap_or(1);
        let want = crate::lobby::allocator::want_flags(min_idle, crate::lobby::matchmaking::queued() > 0 || crate::lobby::launcher_lobby::needs_server());
        if let Value::Array(items) = &mut list {
            for it in items.iter_mut() {
                let addr = it["addr"].as_str().unwrap_or_default().to_string();
                it["want"] = json!(want.get(&addr).copied().unwrap_or(true));
            }
        }
    }
    Json(json!({ "slots": list }))
}

/// Entry token decision: Ok(Some(t)) — the token matched the room token (it also becomes the match record token),
/// Ok(None) — let in without checking (local mode), Err — refused.
fn join_token_check(sent: &str, room: Option<&str>, strict: bool) -> Result<Option<String>, &'static str> {
    match (sent.is_empty(), room) {
        (false, Some(t)) if t == sent => Ok(Some(t.to_string())),
        (false, Some(_)) => Err("bad_token"),
        (false, None) if strict => Err("no_room"),
        (true, _) if strict => Err("token_required"),
        _ => Ok(None),
    }
}

async fn agents() -> Json<Value> {
    let l = link().lock().unwrap();
    let list: Vec<Value> = l
        .agents
        .iter()
        .map(|(id, a)| {
            json!({
                "agent_id": id,
                "ds_address": a.ds_address,
                "maps": a.maps,
                "last_seen_secs": a.last_seen.elapsed().as_secs(),
                "queued": a.queue.len(),
            })
        })
        .collect();
    Json(json!({ "agents": list, "pending": l.pending.len() }))
}

#[cfg(test)]
mod join_token_tests {
    use super::join_token_check as c;

    #[test]
    fn join_token_rules() {
        assert_eq!(c("abc", Some("abc"), true), Ok(Some("abc".into())));
        assert_eq!(c("abc", Some("xyz"), false), Err("bad_token"), "foreign token — always refused");
        assert_eq!(c("", Some("xyz"), false), Ok(None), "dbtest without token — let in locally");
        assert_eq!(c("", None, true), Err("token_required"));
        assert_eq!(c("abc", None, true), Err("no_room"));
        assert_eq!(c("abc", None, false), Ok(None));
    }
}

#[cfg(test)]
mod slot_cmd_tests {
    use super::*;

    /// 09.11: host commands per room — one per heartbeat, in order, never handed to another room
    #[test]
    fn slot_cmds_one_per_heartbeat() {
        queue_slot_cmd("b_test_1", "ban 1");
        queue_slot_cmd("b_test_1", "ban 2");
        assert_eq!(take_slot_cmd("b_test_2"), None);
        assert_eq!(take_slot_cmd("b_test_1").as_deref(), Some("ban 1"));
        assert_eq!(take_slot_cmd("b_test_1").as_deref(), Some("ban 2"));
        assert_eq!(take_slot_cmd("b_test_1"), None);
    }
}
