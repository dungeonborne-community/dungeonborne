//! Match room driver: for rooms in DsInit it sends init_player to the server (via ds-agent) and on the reply moves
//! them to DsReady (matchmaking::set_ds_ready) — after that clients get dsInitFinish with the address and keys.
//!
//! Without ds-agent (static DS_POOL / headless server without an agent) — fallback mode: DsReady right away with keys
//! per the reference (ds_user_key = open_id; token — random per player, the reference had "1001"), as it worked before T1. Agent failure — up to INIT_ATTEMPTS
//! retries, then the same fallback mode (loudly in the log: the server did not get the players).

use super::rpc_client::{self, CallError};
use crate::db::GameDb;
use crate::game_data::GameData;
use crate::lobby::matchmaking::{self, BattleRoom, DsKey};
use crate::rpc::handlers::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

const TICK: Duration = Duration::from_secs(1);
const INIT_TIMEOUT: Duration = Duration::from_secs(15);
const INIT_ATTEMPTS: u32 = 3;
/// How long the server waits for players after init_player, s (reference: startDsInit end_timepoint ≈ +200)
const WAITING_TIMEOUT: i32 = 200;

fn attempts() -> &'static Mutex<HashMap<String, u32>> {
    static A: OnceLock<Mutex<HashMap<String, u32>>> = OnceLock::new();
    A.get_or_init(Default::default)
}

pub async fn run(db: GameDb, game_data: Arc<GameData>) {
    loop {
        tokio::time::sleep(TICK).await;
        for room in matchmaking::take_init_requests() {
            let (db, gd) = (db.clone(), game_data.clone());
            tokio::spawn(async move { init_room(db, gd, room).await });
        }
        pump_room_events(&db, &game_data).await;
    }
}

fn session_for(db: &GameDb, game_data: &Arc<GameData>, open_id: &str, peer: &str) -> GatewaySession {
    GatewaySession {
        db: db.clone(),
        game_data: game_data.clone(),
        open_id: open_id.to_string(),
        peer_ip: peer.to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    }
}

/// Room events — pushed, without a client request: the reference sends battle_room_log_sync by itself right after `matched`
/// (sniffer_log_001), and the native client on 31.10 did not go into battle_room_heartbeat without them. Once per TICK for each
/// online player — the same code as heartbeat (one new event at a time), pushes are forwarded through the hub.
async fn pump_room_events(db: &GameDb, game_data: &Arc<GameData>) {
    for room in matchmaking::active_rooms() {
        for m in &room.members {
            if !crate::lobby::hub::is_online(&m.open_id) {
                continue;
            }
            let session = session_for(db, game_data, &m.open_id, "ds-driver");
            let req = tgf::CsBattleRoomHeartbeatReq { battle_id: room.battle_id.clone(), ..Default::default() };
            session.lobby_battle_room_heartbeat(req).await;
            let pushes = std::mem::take(&mut *session.after_pushes.lock().unwrap());
            for (method, type_url, body) in pushes {
                crate::lobby::hub::push_raw(&m.open_id, &method, &type_url, body);
            }
        }
    }
}

/// Map option_args — like MAP_RULES in the DungeonborneRuClient mod: [5001 crypt | 5002 castle, map_id,
/// 1_000_000 + (map_id − 10000)·100 + 3]. Not checked against the official DS (its init_player is not in the sniffer logs).
pub fn option_args(map_id: i32) -> Vec<i32> {
    match map_id {
        10101..=10199 => vec![5001, map_id, 1_000_000 + (map_id - 10_000) * 100 + 3],
        10201..=10299 => vec![5002, map_id, 1_000_000 + (map_id - 10_000) * 100 + 3],
        _ => vec![map_id],
    }
}

async fn init_room(db: GameDb, game_data: Arc<GameData>, room: BattleRoom) {
    let Some(ds_address) = room.ds_address.clone() else { return };
    let session = session_for(&db, &game_data, "dsdriver", "ds-driver");

    let mut player_data = Vec::new();
    for m in &room.members {
        let player = match db.get::<PlayerModel>("players", &m.open_id).await {
            Some(p) => Some(p),
            None => db.find_by_user_id::<PlayerModel>(m.user_id).await,
        };
        let Some(player) = player else {
            println!("[DS] {}: no player {} in the DB — will not be in init_player", room.battle_id, m.open_id);
            continue;
        };
        let mut u = session.build_battle_user_data(&player);
        u.ds_player_key = Some(tgf::DBattlePlayerDsKey { ds_user_key: m.open_id.clone(), ..Default::default() });
        player_data.push(u);
    }
    let lvl = tgf::DLevelSelect { level_id: room.level_id as i32, ..Default::default() };
    let (level_param, level_map_id) = session.build_level_and_map_params(&lvl);
    // custom game: arena from the room (map_detail 30101…30302) — no waiting hall and no monsters
    let map_id = room.arena_map_id.unwrap_or(level_map_id);
    let req = tgf::DsInitPlayerListReq {
        player_data,
        is_last_one: true,
        init_para: Some(tgf::DsBattleInitParameter {
            map_id,
            user_upper: room.members.len() as i32,
            waiting_timeout: WAITING_TIMEOUT,
            option_args: option_args(map_id),
            level_parameter: Some(level_param),
            ..Default::default()
        }),
    };

    match rpc_client::init_player(&ds_address, &room.battle_id, &req, INIT_TIMEOUT).await {
        Ok(rsp) if rsp.ret_code == 0 => {
            let keys = keys_from_rsp(&room, &rsp);
            println!("[DS] {}: init_player ok, keys {} (server {})", room.battle_id, keys.len(), ds_address);
            attempts().lock().unwrap().remove(&room.battle_id);
            matchmaking::set_ds_ready(&room.battle_id, keys);
        }
        Ok(rsp) => fail(&room, &format!("ret_code={}", rsp.ret_code)),
        Err(CallError::NoAgent) => {
            println!("[DS] {}: {} has no ds-agent — fallback mode without init_player", room.battle_id, ds_address);
            if waiting_room_enabled() && crate::lobby::allocator::has_heartbeat(&ds_address) {
                let host = host_task(&game_data, &room, map_id);
                crate::lobby::allocator::set_host(&ds_address, &room.battle_id, host);
            }
            matchmaking::set_ds_ready(&room.battle_id, fallback_keys(&room));
        }
        Err(e) => fail(&room, &e.to_string()),
    }
}

/// Waiting hall before the match (docs/WAITING_ROOM.md): env DS_WAITING_ROOM=1
fn waiting_room_enabled() -> bool {
    std::env::var("DS_WAITING_ROOM").map(|v| v == "1").unwrap_or(false)
}

/// Server mod assignment for a room: chain of map_detail scenes (hall → match → second floor), hall timeout, option_args
/// by number of players (dynamic_option_args.count), roster with teams (like DBattleUserParameter.team_id)
pub fn host_task(gd: &GameData, room: &BattleRoom, map_id: i32) -> serde_json::Value {
    let detail = gd.map_detail.get(&map_id);
    let mut scenes: Vec<(i32, String)> = detail.map(|d| d.map_config.iter().map(|c| (c.room_type, c.ue_path.clone())).collect()).unwrap_or_default();
    scenes.sort_by_key(|s| s.0);
    // dynamic_option_args.count — group size (1 solo, 2 duo, 3 trio), not the number of players in the room
    let n = room.party_size as i32;
    let opts = detail
        .and_then(|d| d.dynamic_option_args.iter().find(|o| o.count == n).map(|o| o.args.clone()))
        .filter(|a| !a.is_empty())
        .unwrap_or_else(|| option_args(map_id));
    serde_json::json!({
        "battle_id": room.battle_id,
        "level_id": room.level_id,
        "map_id": map_id,
        "scenes": scenes.iter().map(|s| s.1.clone()).collect::<Vec<_>>(),
        // DS_WAITING_TIMEOUT=<sec> — hall timer for debugging (05.11, locally 15 s), otherwise from map_detail
        // the launcher lobby hall has its own timer (LOBBY_WAIT_SECS), it does not touch other rooms
        "waiting_timeout": room.lobby.then(|| crate::lobby::launcher_lobby::wait_secs() as i32)
            .or_else(|| std::env::var("DS_WAITING_TIMEOUT").ok().and_then(|v| v.parse::<i32>().ok()).filter(|t| *t > 0))
            .or(detail.map(|d| d.waiting_room_timeout).filter(|t| *t > 0)).unwrap_or(WAITING_TIMEOUT),
        "battle_timeout": detail.map(|d| d.timeout).unwrap_or(2400),
        // 08.11: the launcher lobby hall does not start the match until min_players have entered (the mod holds the countdown), but no longer than
        // hall_hold_secs beyond the timer; other rooms — 0 (no minimum)
        "min_players": if room.lobby { crate::lobby::launcher_lobby::min_players() } else { 0 },
        "hold_secs": if room.lobby { crate::lobby::launcher_lobby::hall_hold_secs() } else { 0 },
        "option_args": opts,
        // hall capacity (players) — the mod shows it as the "total" so the hall waits the full timeout (WORKAROUNDS K18)
        "capacity": (crate::lobby::matchmaking::max_teams(room.level_id, room.party_size) * room.party_size.max(1)).max(room.members.len()),
        "members": room.members.iter().map(|m| serde_json::json!({
            "open_id": m.open_id,
            "team_id": crate::lobby::team::battle_team_id(m.user_id),
        })).collect::<Vec<_>>(),
    })
}

fn fail(room: &BattleRoom, why: &str) {
    let n = {
        let mut a = attempts().lock().unwrap();
        let n = a.entry(room.battle_id.clone()).or_insert(0);
        *n += 1;
        *n
    };
    if n < INIT_ATTEMPTS {
        println!("[DS] {}: init_player failed ({}), attempt {}/{}", room.battle_id, why, n, INIT_ATTEMPTS);
        matchmaking::retry_init(&room.battle_id);
    } else {
        println!("[DS] {}: init_player failed {} times ({}) — THE SERVER DID NOT GET THE PLAYERS, fallback mode", room.battle_id, n, why);
        attempts().lock().unwrap().remove(&room.battle_id);
        matchmaking::set_ds_ready(&room.battle_id, fallback_keys(room));
    }
}

/// Keys from the server reply; a player without a key in the reply gets a fallback one
pub fn keys_from_rsp(room: &BattleRoom, rsp: &tgf::DsInitPlayerListRsp) -> Vec<DsKey> {
    room.members
        .iter()
        .map(|m| match rsp.key_token.iter().find(|k| k.ds_user_key == m.open_id) {
            Some(k) => DsKey {
                user_id: m.user_id,
                ds_user_key: k.ds_user_key.clone(),
                token: k.token.clone(),
                encryption_key: k.encryption_key.clone(),
            },
            None => fallback_key(m.user_id, &m.open_id),
        })
        .collect()
}

/// Fallback key (no ds-agent): token — random per player and room. The reference gave "1001" to everyone (sniffer_log_001),
/// but the token is now checked: the client gets it in ds_init_finish, connects with `?token=`, dbnative passes it
/// to `battle_user_data` (src/ds/agent_link.rs) — a shared token would let anyone in under someone else's OpenId.
fn fallback_key(user_id: u64, open_id: &str) -> DsKey {
    DsKey { user_id, ds_user_key: open_id.to_string(), token: format!("{:016x}", rand::random::<u64>()), encryption_key: String::new() }
}

fn fallback_keys(room: &BattleRoom) -> Vec<DsKey> {
    room.members.iter().map(|m| fallback_key(m.user_id, &m.open_id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn option_args_like_map_rules() {
        assert_eq!(option_args(10102), vec![5001, 10102, 1010203]);
        assert_eq!(option_args(10201), vec![5002, 10201, 1020103]);
        assert_eq!(option_args(10103), vec![5001, 10103, 1010303]);
    }

    /// Hall assignment from the real map_detail: 10101 — hall L_Dungeon_Matching_01 → Official_01 → Official_02, 120 s
    #[test]
    fn host_task_from_map_detail() {
        let gd = GameData::load_all("ExcelConfig");
        let room = matchmaking::BattleRoom::for_test("bw_1", 40003015, &[("a", 1), ("b", 2)]);
        let h = host_task(&gd, &room, 10101);
        assert_eq!(h["scenes"][0], "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01");
        assert_eq!(h["scenes"][1], "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01");
        assert_eq!(h["waiting_timeout"], 120);
        assert_eq!(h["option_args"], serde_json::json!([1010101]), "two solos — group size 1");
        assert_eq!(h["members"].as_array().unwrap().len(), 2);
        assert_eq!(h["capacity"], 18, "Crypt solo hall capacity");
    }
}
