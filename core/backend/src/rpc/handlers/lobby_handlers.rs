//! Matchmaking, battle room and party RPCs on top of crate::lobby. The handlers in handlers.rs only delegate here.

use super::GatewaySession;
use crate::lobby::matchmaking::{self, Member, RoomStage};
use crate::lobby::{hub, launcher_lobby, team};
use crate::structures::PlayerModel;
use crate::tgf;

/// Default server address for rooms that are not in the matchmaker (local LocalDev listen scenarios)
fn default_ds_address() -> String {
    std::env::var("DS_DEFAULT_ADDR").unwrap_or_else(|_| "127.0.0.1:7777".to_string())
}

/// How long the server gets for init_player, s (reference: end_timepoint startDsInit ≈ +200 s)
const DS_INIT_TIMEOUT_SECS: i64 = 200;

/// Which room event was last given to the player: (battle_id, open_id) -> event_id
fn room_sent() -> &'static std::sync::Mutex<std::collections::HashMap<(String, String), i64>> {
    static SENT: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<(String, String), i64>>> =
        std::sync::OnceLock::new();
    SENT.get_or_init(Default::default)
}

/// The player returned to the hall of the same room (open hall): room events for them — from the start
pub(crate) fn room_sent_reset(battle_id: &str, open_id: &str) {
    room_sent().lock().unwrap().remove(&(battle_id.to_string(), open_id.to_string()));
}

fn room_sent_get(battle_id: &str, open_id: &str) -> i64 {
    room_sent().lock().unwrap().get(&(battle_id.to_string(), open_id.to_string())).copied().unwrap_or(0)
}

/// Take an event number for the player: true — if event_id − 1 or less was given before
fn room_sent_claim(battle_id: &str, open_id: &str, event_id: i64) -> bool {
    let mut sent = room_sent().lock().unwrap();
    if sent.len() > 10_000 {
        sent.clear(); // rooms live ≤ 45 min; old records are not needed
    }
    let cur = sent.entry((battle_id.to_string(), open_id.to_string())).or_insert(0);
    if *cur >= event_id {
        return false;
    }
    *cur = event_id;
    true
}

/// dirty_clear_matching — once per (player, matching_id): after it the client still sends a search heartbeat every
/// second, and a push on each (player data change every second) broke the lobby — "any exit breaks the character" (03.11)
pub(crate) fn clear_matching_once(open_id: &str, matching_id: &str) -> bool {
    static SENT: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<(String, String)>>> = std::sync::OnceLock::new();
    let mut set = SENT.get_or_init(Default::default).lock().unwrap();
    if set.len() > 10_000 {
        set.clear();
    }
    set.insert((open_id.to_string(), matching_id.to_string()))
}

/// End-of-match events after 4 startBattling (M3): the player's result and closing the room, status FINISHED
const EV_GAME_OVER: i64 = 5;
const EV_FINISH: i64 = 6;

/// Match over — push room events 5 gameOverSettle (result) and 6 finish to the player, status FINISHED. Without them the client
/// considers the room alive (the last it knew was BATTLING until +2400 s), sends battle_room_heartbeat and travels to the server
/// again by itself (network_block_loading, 03.11 after room_closed). Once per player and room. No reference:
/// the official end of a match was not recorded. TODO(verify): which of the two events the client uses to forget the room.
pub(crate) fn room_finish_push(open_id: &str, battle_id: &str, game_over: Option<tgf::DBattlePlayerGameOver>, level: Option<tgf::DBattleLevelParameter>) -> bool {
    if !room_sent_claim(battle_id, open_id, EV_FINISH) {
        return false;
    }
    // close this match's search with the FIRST event, CANCEL (as the reference: CANCEL after FINISHED), then the room events and
    // dirty_clear_battle_room. Clearing the room does not close the search: 04.11 12:39 after the match the room heartbeat stopped,
    // but the search one went on, "Start" sent only pre_search. CANCEL after the clearing (10:48) broke the button — hence before it
    // and over the same channel (hub), so that the order is kept.
    let matching_id = match matchmaking::status(open_id) {
        Some(matchmaking::PlayerStatus::Matched { matching_id, battle_id: b }) if b == battle_id => Some(matching_id),
        _ => None,
    };
    if let Some(m) = matching_id.as_deref() {
        let uid = matchmaking::room(battle_id).and_then(|r| r.members.iter().find(|x| x.open_id == open_id).map(|x| x.user_id));
        if let Some(msg) = uid.and_then(|u| matchmaking::match_cancel_msg(open_id, u, m, matchmaking::MATCH_EXIT_DESTROY)) {
            let id = msg.snapshot.as_ref().map(|s| s.last_event_id).unwrap_or(0);
            hub::push_to(open_id, hub::MATCHING_LOG_METHOD, hub::MATCHING_LOG_TYPE, &msg);
            println!("[Matchmaking] search {} for {}: event {} CANCEL before the end of room {}", m, open_id, id, battle_id);
        }
    }
    let now = GatewaySession::now_ts();
    let ts = |t: i64| Some(crate::google::protobuf::Timestamp { seconds: t, nanos: 0 });
    let room_key = tgf::DBattleRoomKey { battle_id: battle_id.to_string() };
    let start = matchmaking::room(battle_id).map(|r| r.ds_ready_ts).unwrap_or(0);
    use tgf::d_battle_room_event_log::Event;
    let finished = tgf::EnBattleRoomStatus::Finished as i32;
    let msg = tgf::ScBattleRoomLogSync {
        battle_id: battle_id.to_string(),
        event_logs: vec![
            tgf::DBattleRoomEventLog {
                event_id: EV_GAME_OVER,
                timepoint: ts(now),
                battle_room_status: finished,
                event: Some(Event::GameOverSettle(tgf::DBattleGameOverSettle {
                    room_key: Some(room_key.clone()),
                    level_parameter: level,
                    battle_finish_timepoint: now,
                    battle_start_timepoint: start,
                    player_game_over: game_over,
                })),
            },
            tgf::DBattleRoomEventLog {
                event_id: EV_FINISH,
                timepoint: ts(now),
                battle_room_status: finished,
                event: Some(Event::Finish(tgf::DBattleBanPickStepInfo { end_timepoint: now })),
            },
        ],
        snapshot: Some(tgf::DBattleRoomSnapshotData {
            last_event_id: EV_FINISH,
            battle_room_status: finished,
            end_timepoint: now,
            room_key: Some(room_key),
            ..Default::default()
        }),
    };
    let online = hub::push_to(open_id, "tgf.GameClientService.battle_room_log_sync", "tgf.SCBattleRoomLogSync", &msg);
    println!("[BattleRoom] {} -> {}: match over (events 5–6, FINISHED, online={})", battle_id, open_id, online);
    room_clear_push(open_id, battle_id, matching_id);
    true
}

/// The "event" number in room_sent marking that the room is already cleared on the client
const EV_CLEARED: i64 = 7;

/// End of the room for the client (FIX_LIST M10, agent analysis research/battleroom): URoomService forgets the room (stops
/// sending battle_room_heartbeat, resets the search — MatchStop, no window) ONLY on `dirty_clear_battle_room` with its
/// battle_id or on a cancel/CANCEL snapshot (with a reason window). Finish, FINISHED/TIMEOUT and the heartbeat response do not;
/// with a snapshot in SCBattleRoomLogSync the events are not read at all. Without this: "Match found, connecting" on return to the
/// lobby, "Start" is dead, the client traveled to the server by itself (B8). After clearing, do not send SCBattleRoomLogSync of this room —
/// it "raises" the room again. Once per (player, room).
pub(crate) fn room_clear_push(open_id: &str, battle_id: &str, matching_id: Option<String>) -> bool {
    if battle_id.is_empty() || !room_sent_claim(battle_id, open_id, EV_CLEARED) {
        return false;
    }
    if let Some(m) = matching_id.as_deref() {
        matchmaking::mark_match_closed(open_id, m);
    }
    let clear_matching: Vec<String> = matching_id.filter(|m| clear_matching_once(open_id, m)).into_iter().collect();
    let online = hub::push_to(open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &tgf::ScPlayerDirtyChgSync {
        dirty_clear_battle_room: vec![battle_id.to_string()],
        dirty_clear_matching: clear_matching.clone(),
        ..Default::default()
    });
    println!("[BattleRoom] {} -> {}: dirty_clear_battle_room{} (online={})", battle_id, open_id,
        if clear_matching.is_empty() { String::new() } else { format!(" + clear_matching {}", clear_matching[0]) }, online);
    true
}

fn key(user_id: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id, zone_id: 1 }
}

/// Role for the party member card: type, race/class/gender, level of the active character
pub(super) fn team_deploy_role(player: &PlayerModel) -> Option<tgf::DBattleDeployRoleTeam> {
    let role = player
        .roles
        .iter()
        .find(|r| r.role_guid == player.hall_role_guid)
        .or_else(|| player.roles.first())?;
    let deploy = role.data.deploy_role.as_ref();
    let basic = deploy.and_then(|d| d.basic.as_ref());
    Some(tgf::DBattleDeployRoleTeam {
        role_type_id: basic.map(|b| b.type_id).unwrap_or(role.type_id as i32),
        meta: basic.and_then(|b| b.meta.clone()),
        role_level: deploy.map(|d| d.role_level).filter(|l| *l > 0).unwrap_or(role.role_level as i32),
    })
}

impl GatewaySession {
    async fn player_by_open_id(&self, open_id: &str) -> Option<PlayerModel> {
        self.db.get::<PlayerModel>("players", open_id).await
    }

    async fn current_player(&self) -> PlayerModel {
        let open_id = self.clean_open_id();
        self.db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001))
    }

    // ---------------------------------------------------------------- matchmaking

    pub(super) async fn lobby_matching_search(
        &self,
        req: tgf::CsBattleMatchingSearchReq,
    ) -> tgf::ScBattleMatchingSearchRsp {
        let open_id = self.clean_open_id();
        let mut player = self.current_player().await;
        // the player has a match running (client crash/restart) — the match button = "Return to match": not into the search, but into their room
        // (the same server, token, their own PC/PS); the room events are given again
        if self.reconnect_battle_room(&player).await.is_some() {
            if let Some((matching_id, battle_id)) = matchmaking::rejoin(&open_id) {
                room_sent().lock().unwrap().remove(&(battle_id, open_id.clone()));
                return tgf::ScBattleMatchingSearchRsp {
                    matching_id,
                    matching_heartbeat_interval: 1,
                    battle_heartbeat_interval: 1,
                    level_parameter: None,
                };
            }
        }
        if let Some(ref req_lvl) = req.level_select {
            if req_lvl.level_id != 0 {
                player.state["level_select"] = serde_json::json!({
                    "level_type": if req_lvl.level_type != 0 { req_lvl.level_type } else { 2001 },
                    "level_id": req_lvl.level_id,
                    "region": if req_lvl.region.is_empty() { "eu" } else { &req_lvl.region },
                });
                self.db.save("players", &open_id, &player).await;
            }
        }
        let lvl = Self::get_saved_level_select(&player);
        team::touch_open_id(player.user_id, &open_id);

        // The leader queues the whole party with one ticket; the others — solo
        let members: Vec<Member> = match team::team_of(player.user_id) {
            Some(t) if t.captain == player.user_id => {
                t.members.iter().map(|m| Member { open_id: m.open_id.clone(), user_id: m.user_id }).collect()
            }
            _ => vec![Member { open_id: open_id.clone(), user_id: player.user_id }],
        };
        let ids: Vec<String> = members.iter().map(|m| m.open_id.clone()).collect();
        if self.battle_access_denied(&ids).await {
            return tgf::ScBattleMatchingSearchRsp::default();
        }
        // maintenance: no new matches, returning to a running one is possible (the branch above); a group of own people (gm/admin) — passes
        if let Some(content) = ids.iter().find_map(|o| crate::lobby::maintenance::blocks(o).then(crate::lobby::maintenance::message).flatten()) {
            println!("[Lobby] {}: \"Start\" rejected, maintenance", open_id);
            self.set_rpc_error(tgf::EnErrorCode::EnErrMatchingThisActiveLevelNotOpen as i32);
            self.push(
                "tgf.GameClientService.modal_message",
                "tgf.SCModalMessageSync",
                tgf::ScModalMessageSync { title: "Технические работы".into(), content },
            );
            return tgf::ScBattleMatchingSearchRsp::default();
        }
        // Challenger Mode ("challenge mode", level_challenge): solo and duo — into rooms of three; in a party — the leader's choice
        let challenger = match team::team_of(player.user_id).filter(|t| t.captain != player.user_id) {
            Some(t) => self.db.find_by_user_id::<PlayerModel>(t.captain).await.map_or(false, |c| Self::challenger_on(&c)),
            None => Self::challenger_on(&player),
        };
        // 08.11 evening: gear rarity limit by the map FROM THE VOTE. The client checks it itself, but by the map
        // selected locally — in the launcher lobby that is not the same map, and the limit was bypassed.
        if launcher_lobby::enabled() {
            if let Some(content) = self.lobby_gear_refusal(&open_id, &members).await {
                println!("[Lobby] {}: \"Start\" rejected, gear: {}", open_id, content);
                self.set_rpc_error(tgf::EnErrorCode::EnErrMatchingThisActiveLevelNotOpen as i32);
                self.push(
                    "tgf.GameClientService.modal_message",
                    "tgf.SCModalMessageSync",
                    tgf::ScModalMessageSync { title: "Снаряжение".into(), content },
                );
                return tgf::ScBattleMatchingSearchRsp::default();
            }
        }
        // launcher lobby: "Start" leads into the tab's hall, the map — from the vote, not from the selection in the client
        let (matching_id, level_id) = if launcher_lobby::enabled() {
            match launcher_lobby::enter(&open_id, &members, challenger) {
                Ok(v) => v,
                Err(content) => {
                    println!("[Lobby] {}: \"Start\" rejected: {}", open_id, content);
                    self.set_rpc_error(tgf::EnErrorCode::EnErrMatchingThisActiveLevelNotOpen as i32);
                    self.push(
                        "tgf.GameClientService.modal_message",
                        "tgf.SCModalMessageSync",
                        tgf::ScModalMessageSync { title: "Лобби".into(), content },
                    );
                    return tgf::ScBattleMatchingSearchRsp::default();
                }
            }
        } else {
            (matchmaking::enqueue(members.clone(), lvl.level_id as i64, challenger), lvl.level_id as i64)
        };
        if launcher_lobby::enabled() {
            // the server and the results take the map from the player's saved selection (build_level_and_map_params)
            for m in &members {
                if let Some(mut p) = self.db.get::<PlayerModel>("players", &m.open_id).await {
                    p.state["level_select"]["level_id"] = serde_json::json!(level_id);
                    if p.state["level_select"]["level_type"].is_null() {
                        p.state["level_select"]["level_type"] = serde_json::json!(2001);
                    }
                    self.db.save("players", &m.open_id, &p).await;
                }
            }
        }

        // Match event #1 per the reference: MATCHING startMatching (then matched/finished — lobby::matchmaking)
        use tgf::EnMatchStatus as S;
        let mut snapshot = matchmaking::match_snapshot(level_id, &members, S::Matching, matchmaking::MATCH_EVENT_START);
        snapshot.estimate_waiting_time = std::env::var("MATCH_WAIT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(15);
        let searching = tgf::ScBattleMatchingLogSync {
            matching_id: matching_id.clone(),
            event_logs: vec![matchmaking::match_event(
                matchmaking::MATCH_EVENT_START,
                S::Matching,
                tgf::d_match_event_log::Event::StartMatching(tgf::DMatchStartNotify { start_timepoint: Self::now_ts() }),
            )],
            snapshot: Some(snapshot),
            is_switch: false,
        };
        for m in &members {
            matchmaking::note_match_event(&m.open_id, &matching_id, matchmaking::MATCH_EVENT_START);
            if m.open_id == open_id {
                self.push(hub::MATCHING_LOG_METHOD, hub::MATCHING_LOG_TYPE, searching.clone());
            } else {
                hub::push_to(&m.open_id, hub::MATCHING_LOG_METHOD, hub::MATCHING_LOG_TYPE, &searching);
            }
        }

        tgf::ScBattleMatchingSearchRsp {
            matching_id,
            matching_heartbeat_interval: 1,
            battle_heartbeat_interval: 1,
            level_parameter: None,
        }
    }

    /// Search heartbeat — only "alive"; a found match comes as a push from the matchmaker tick. false — no such search
    /// (room closed, backend restarted): the caller sends dirty_clear_matching. Without it, after a match the client sent
    /// the search heartbeat forever (FINISHED does not stop it) and kept the party "in search" — leaving the party was
    /// impossible (03.11 11:32, play.ps1 with the hall).
    pub(super) fn lobby_matching_heartbeat(&self, matching_id: &str) -> bool {
        let live = match matchmaking::status(&self.clean_open_id()) {
            Some(matchmaking::PlayerStatus::Searching { matching_id: m }) | Some(matchmaking::PlayerStatus::Matched { matching_id: m, .. }) => {
                matching_id.is_empty() || m == matching_id
            }
            None => false,
        };
        if !live {
            println!("[Matchmaking] heartbeat {} from {}: no search — clear_matching", matching_id, self.clean_open_id());
        }
        live
    }

    pub(super) fn lobby_matching_cancel(&self, why: &str) {
        matchmaking::cancel(&self.clean_open_id(), why);
    }

    // ---------------------------------------------------------------- battle room

    /// Room events per the reference (docs/REF_BATTLE_ROOM_FLOW.md, sniffer_log_001):
    ///   1 DS_INIT startDsInit → 2 DS_INIT dsInitFinish (address + player keys) → 3 LOADING startLoading → 4 BATTLING startBattling.
    /// They arrive as battle_room_log_sync pushes, one per heartbeat; the heartbeat response is empty. The client sends
    /// only battle_id in the request (acknowledge_event_id = 0), so what was already given to the player is remembered here (ROOM_SENT).
    /// Only what the room has passed in the matchmaking::RoomStage state machine is given: until the server responds to init_player
    /// the client gets only startDsInit and waits.
    ///
    /// Also called from crate::ds::driver (31.10): the reference pushes room events itself, right after `matched`, and the client
    /// does not go into the room heartbeat without them — the driver runs this function once a second for every player
    /// of the room and forwards the pushes via the hub. Nothing new — exit before loading players from the DB.
    pub(crate) async fn lobby_battle_room_heartbeat(&self, req: tgf::CsBattleRoomHeartbeatReq) {
        let now = Self::now_ts();
        let me_key = self.clean_open_id();

        // The room is closed (everyone has results, timeout) — only "match over", the server address is no longer given
        let room = matchmaking::room(&req.battle_id);
        // an unknown room with dedicated servers — a match from before the backend restart (B8): the client traveled into it again by itself
        let dedicated = std::env::var("DS_WAITING_ROOM").map(|v| v == "1").unwrap_or(false);
        if room.is_none() && (matchmaking::is_closed(&req.battle_id) || dedicated) {
            if matchmaking::is_closed(&req.battle_id) {
                room_finish_push(&me_key, &req.battle_id, None, None);
            }
            room_clear_push(&me_key, &req.battle_id, None);
            return;
        }
        // Participants, server and stage — from the matchmaker; otherwise (LocalDev/listen) — only us, the default address, everything ready
        let stage = room.as_ref().map(|r| r.stage).unwrap_or(RoomStage::DsReady);
        let sent = room_sent_get(&req.battle_id, &me_key).max(req.acknowledge_event_id);
        let available: i64 = match stage {
            RoomStage::WaitingServer => 0,
            RoomStage::DsInit => 1,
            // waiting hall: the address — when the mod loaded the hall, startBattling — after the transition into the match
            RoomStage::DsReady => crate::lobby::allocator::room_events_allowed(&req.battle_id),
        };
        let next_id = sent + 1;
        if next_id > available {
            return; // nothing new: as in the reference, no pushes, the response is empty
        }
        // The driver and the client's own heartbeat can arrive at the same time — whoever takes the number first gives the event
        if !room_sent_claim(&req.battle_id, &me_key, next_id) {
            return;
        }

        let me = self.current_player().await;
        let (players, ds_address, level_id, created_ts, ds_init_ts, ds_ready_ts) = match &room {
            Some(room) => {
                let mut players = Vec::new();
                for m in &room.members {
                    let found = match self.player_by_open_id(&m.open_id).await {
                        Some(p) => Some(p),
                        None => self.db.find_by_user_id::<PlayerModel>(m.user_id).await,
                    };
                    match found {
                        Some(p) => players.push(p),
                        None => println!("[BattleRoom] {}: no player {} in DB", room.battle_id, m.open_id),
                    }
                }
                let ds = room.ds_address.clone().unwrap_or_else(default_ds_address);
                (players, ds, room.level_id as i32, room.created_ts, room.ds_init_ts, room.ds_ready_ts)
            }
            None => {
                let lvl = Self::get_saved_level_select(&me);
                (vec![me.clone()], default_ds_address(), lvl.level_id, now, now, now)
            }
        };
        // players with the UDP tunnel have their own server address (auth::udp_tunnel)
        let ds_address = crate::auth::udp_tunnel::client_ds(&me.open_id, &ds_address);

        let mut lvl = Self::get_saved_level_select(&me);
        lvl.level_id = level_id;
        let (level_param, map_id) = self.build_level_and_map_params(&lvl);
        let ds_key_of = |user_id: u64, open_id: &str| -> tgf::DBattlePlayerDsKey {
            let key = room.as_ref().and_then(|r| r.ds_key_of(user_id));
            tgf::DBattlePlayerDsKey {
                ds_user_key: key.as_ref().map(|k| k.ds_user_key.clone()).unwrap_or_else(|| open_id.to_string()),
                ds_token: key.as_ref().map(|k| k.token.clone()).unwrap_or_default(),
                ds_encryption_key: key.map(|k| k.encryption_key).unwrap_or_default(),
            }
        };
        let participators: Vec<tgf::DBattleUserData> = players
            .iter()
            .map(|p| {
                let mut u = self.build_battle_user_data(p);
                u.ds_player_key = Some(ds_key_of(p.user_id, &p.open_id));
                u
            })
            .collect();
        let own = self.build_battle_user_data(&me);

        let room_key = tgf::DBattleRoomKey { battle_id: req.battle_id.clone() };
        let room_data = tgf::DBattleRoomData {
            battle_server_id: 1,
            create_timepoint: created_ts,
            expired_timepoint: created_ts + 2400,
            battle_id: req.battle_id.clone(),
            ds_address: ds_address.clone(),
            map_data: Some(tgf::DBattleMapData { map_id }),
            battle_data: Some(tgf::d_battle_room_data::BattleData::CompetitionSeasonPvpve(
                tgf::DBattleDataCompetition {},
            )),
            ..Default::default()
        };
        // reference: end_timepoint startBattling = start + 2400 s (40 min raid; matchmaker.rs with map timeouts is not built)
        let battle_secs: i64 = 2400;
        let ts = |t: i64| Some(crate::google::protobuf::Timestamp { seconds: t, nanos: 0 });

        use tgf::d_battle_room_event_log::Event;
        use tgf::EnBattleRoomStatus as St;
        let (status, timepoint, _end_timepoint, event) = match next_id {
            1 => (St::DsInit, ds_init_ts, ds_init_ts + DS_INIT_TIMEOUT_SECS,
                  Event::StartDsInit(tgf::DBattleDsInitInfo { end_timepoint: ds_init_ts + DS_INIT_TIMEOUT_SECS })),
            2 => (St::DsInit, ds_ready_ts, 0, Event::DsInitFinish(tgf::DBattleDsInfo {
                room_key: Some(room_key.clone()),
                ds_address: ds_address.clone(),
                ds_player_keys: participators
                    .iter()
                    .map(|u| tgf::DBattlePlayerDsPlayerKey { user_key: u.user_key.clone(), ds_key: u.ds_player_key.clone() })
                    .collect(),
                ..Default::default()
            })),
            3 => (St::Loading, ds_ready_ts + 1, ds_ready_ts + 61, Event::StartLoading(tgf::DBattleLoadData {
                room_key: Some(room_key.clone()),
                room_data: Some(room_data.clone()),
                participator: participators.clone(),
                loading_end_timepoint: ds_ready_ts + 61,
            })),
            _ => (St::Battling, ds_ready_ts + 2, ds_ready_ts + 2 + battle_secs,
                  Event::StartBattling(tgf::DBattleBanPickStepInfo { end_timepoint: ds_ready_ts + 2 + battle_secs })),
        };
        // Snapshot — the current room state (the last available event), not of this event: with a snapshot the client
        // does not read the events and takes the status from it (agent analysis research/battleroom). Events go over two channels
        // (the client heartbeat and the driver) and can arrive out of order: 04.11 11:24 return to the hall — 1–4 within a second,
        // the LOADING snapshot arrived last, the client did not travel to the server, "Start" afterwards sent only pre_search.
        let (snap_status, snap_end) = match available.max(next_id) {
            1 | 2 => (St::DsInit, if available <= 1 { ds_init_ts + DS_INIT_TIMEOUT_SECS } else { 0 }),
            3 => (St::Loading, ds_ready_ts + 61),
            _ => (St::Battling, ds_ready_ts + 2 + battle_secs),
        };
        println!(
            "[BattleRoom] {} -> {}: event {} {:?} (snapshot {:?}, ds={} participants={})",
            req.battle_id, me.open_id, next_id, status, snap_status, ds_address, participators.len()
        );

        self.push(
            "tgf.GameClientService.battle_room_log_sync",
            "tgf.SCBattleRoomLogSync",
            tgf::ScBattleRoomLogSync {
                battle_id: req.battle_id.clone(),
                event_logs: vec![tgf::DBattleRoomEventLog {
                    event_id: next_id,
                    timepoint: ts(timepoint),
                    battle_room_status: status as i32,
                    event: Some(event),
                }],
                snapshot: Some(tgf::DBattleRoomSnapshotData {
                    last_event_id: available.max(next_id),
                    battle_room_status: snap_status as i32,
                    end_timepoint: snap_end,
                    room_key: Some(room_key),
                    room_data: Some(room_data.clone()),
                    participator: participators,
                    ..Default::default()
                }),
            },
        );
        // the match has started — match event #3 FINISHED (in the reference it comes after startBattling)
        if next_id == 4 && matchmaking::notify_finished(&req.battle_id, &me.open_id) {
            println!("[Matchmaking] {} FINISHED of the match of room {}", me.open_id, req.battle_id);
        }

        // The current level in DLevelData (for the room=map mode) — once, with the first event
        if next_id == 1 {
            self.push(
                hub::DIRTY_METHOD,
                hub::DIRTY_TYPE,
                tgf::ScPlayerDirtyChgSync {
                    dirty_level_current: Some(tgf::DLevelData {
                        level_parameter: Some(level_param),
                        status: 1,
                        deploy_request: None,
                        deploy_response: Some(tgf::DLevelDeployResponse {
                            deploy_role: own.deploy_preview.and_then(|p| p.deploy_role),
                            battle_setting: Some(super::progress_handlers::progress_battle_options(&me)),
                        }),
                        battle_data: Some(room_data),
                        current_select_challenge: None,
                    }),
                    ..Default::default()
                },
            );
        }
    }

    /// The player's running match (docs/RECONNECT.md) in the user_battle_room format. NOT given to the client on login: with status
    /// 8 BATTLING URoomService takes the player to the server by itself, while the user needs a "Return to match" button (03.11) —
    /// the return goes via the match button (lobby_matching_search → matchmaking::rejoin). Here — the "match is running" check.
    /// None — no match: the result already exists, the room is still in the waiting hall (the client does not reconnect from the hall) or closed.
    pub(super) async fn reconnect_battle_room(&self, me: &PlayerModel) -> Option<tgf::DBattlePlayerBattleRoomData> {
        let Some(matchmaking::PlayerStatus::Matched { battle_id, .. }) = matchmaking::status(&me.open_id) else { return None };
        let room = matchmaking::room(&battle_id).filter(|r| r.stage == RoomStage::DsReady)?;
        if matchmaking::has_result(&battle_id, &me.open_id) || !crate::lobby::allocator::battle_started(&battle_id) {
            return None;
        }
        let mut lvl = Self::get_saved_level_select(me);
        lvl.level_id = room.level_id as i32;
        let (_, map_id) = self.build_level_and_map_params(&lvl);
        let mut participator = Vec::new();
        for m in &room.members {
            let found = match self.player_by_open_id(&m.open_id).await {
                Some(p) => Some(p),
                None => self.db.find_by_user_id::<PlayerModel>(m.user_id).await,
            };
            let Some(p) = found else { continue };
            let mut u = self.build_battle_user_data(&p);
            u.ds_player_key = room.ds_key_of(p.user_id).map(|k| tgf::DBattlePlayerDsKey {
                ds_user_key: k.ds_user_key,
                ds_token: k.token,
                ds_encryption_key: k.encryption_key,
            });
            participator.push(u);
        }
        let room_data = tgf::DBattleRoomData {
            battle_server_id: 1,
            create_timepoint: room.created_ts,
            expired_timepoint: room.created_ts + 2400,
            battle_id: battle_id.clone(),
            ds_address: crate::auth::udp_tunnel::client_ds(&me.open_id, &room.ds_address.clone().unwrap_or_else(default_ds_address)),
            map_data: Some(tgf::DBattleMapData { map_id }),
            battle_data: Some(tgf::d_battle_room_data::BattleData::CompetitionSeasonPvpve(tgf::DBattleDataCompetition {})),
            ..Default::default()
        };
        println!("[BattleRoom] {} -> {}: match running — user_battle_room for reconnecting", battle_id, me.open_id);
        Some(tgf::DBattlePlayerBattleRoomData {
            room_key: Some(tgf::DBattleRoomKey { battle_id }),
            room_data: Some(room_data),
            participator,
            acknowledge_event_id: 4,
            battle_room_status: tgf::EnBattleRoomStatus::Battling as i32,
            ..Default::default()
        })
    }

    // ---------------------------------------------------------------- party

    /// DUserTeamData snapshot for the player: the current party + incoming invitations.
    pub(super) async fn team_snapshot(&self, user_id: u64) -> tgf::DUserTeamData {
        let now = Self::now_ts();
        let mut data = tgf::DUserTeamData::default();
        if let Some(t) = team::team_of(user_id) {
            let mut members = Vec::new();
            let mut level_select = None;
            let mut challenger_model = None;
            for m in &t.members {
                let player = self.player_by_open_id(&m.open_id).await;
                if m.user_id == t.captain {
                    level_select = player.as_ref().map(Self::get_saved_level_select);
                    // 08.11: party Challenger Mode — the leader's choice (only the leader toggles it in the map window)
                    challenger_model = player.as_ref().map(Self::challenger_model);
                }
                members.push(tgf::DTeamMember {
                    user_key: Some(key(m.user_id)),
                    joined_team_timepoint: m.joined_ts,
                    deploy_role: player.as_ref().and_then(team_deploy_role),
                    ready: Some(tgf::DTeamReady { is_ready: m.ready }),
                    offline: Some(tgf::DTeamOnline { is_offline: !hub::is_online(&m.open_id) }),
                    battle: Some(tgf::DTeamBattle { is_in_battle: false }),
                    gamesvr_id: 1,
                    join_team_type: m.join_type,
                    ..Default::default()
                });
            }
            // Join applications (team finder) — visible to the whole party, the leader decides
            let mut pending_applies = Vec::new();
            for a in team::applies_of(t.team_id) {
                let applicant = self.player_by_open_id(&a.open_id).await;
                pending_applies.push(tgf::DTeamApply {
                    team_key: Some(tgf::DTeamKey { team_id: t.team_id }),
                    user_key: Some(key(a.user_id)),
                    deploy_role: applicant.as_ref().and_then(team_deploy_role),
                    gamesvr_id: 1,
                    apply_type: a.apply_type,
                    expired_timepoint: a.expired_ts.max(now),
                    ..Default::default()
                });
            }
            data.current_team = Some(tgf::DTeamStorage {
                team_key: Some(tgf::DTeamKey { team_id: t.team_id }),
                captain_user_key: Some(key(t.captain)),
                level_select,
                challenger_model,
                members,
                pending_applies,
                team_match_data: team::match_status(t.captain).map(|s| tgf::DTeamMatchData {
                    matching_id: s.matching_id,
                    team_matchsvr_id: 1,
                    start_matching_timepoint: s.start_ts,
                }),
                ..Default::default()
            });
        }
        data.finder_status = team::finder_status(user_id);
        data.team_match_data = team::match_status(user_id).map(|s| tgf::DTeamMatchData {
            matching_id: s.matching_id,
            team_matchsvr_id: 1,
            start_matching_timepoint: s.start_ts,
        });
        for inv in team::invitations_for(user_id) {
            data.pending_invitation.push(self.team_invitation_proto(&inv, now).await);
        }
        data
    }

    async fn team_invitation_proto(&self, inv: &team::Invitation, now: i64) -> tgf::DTeamInvitation {
        let inviter = self.db.find_by_user_id::<PlayerModel>(inv.inviter).await;
        tgf::DTeamInvitation {
            inviter: Some(key(inv.inviter)),
            invitee: Some(key(inv.invitee)),
            team_key: Some(tgf::DTeamKey { team_id: inv.team_id }),
            level_select: inviter.as_ref().map(Self::get_saved_level_select),
            deploy_role: inviter.as_ref().and_then(team_deploy_role),
            gamesvr_id: 1,
            start_timepoint: inv.start_ts,
            expired_timepoint: inv.expired_ts.max(now),
            ..Default::default()
        }
    }

    /// Sends party snapshots to players (by user_id); offline players get the state on login.
    pub(super) async fn push_team_snapshots(&self, user_ids: &[u64]) {
        let mut seen = std::collections::HashSet::new();
        for &uid in user_ids {
            if !seen.insert(uid) {
                continue;
            }
            let Some(player) = self.db.find_by_user_id::<PlayerModel>(uid).await else {
                println!("[Team] no player user_id={} in DB", uid);
                continue;
            };
            let snapshot = self.team_snapshot(uid).await;
            let msg = tgf::ScPlayerDirtyChgSync { dirty_team_snapshot: Some(snapshot), ..Default::default() };
            let online = hub::push_to(&player.open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
            println!("[Team] snapshot -> {} (user_id={}, online={})", player.open_id, uid, online);
        }
    }

    pub(super) fn team_user_ids(t: &team::Team) -> Vec<u64> {
        t.members.iter().map(|m| m.user_id).collect()
    }

    pub(super) async fn lobby_team_invite(&self, req: tgf::CsTeamSendInvitationReq) {
        let me = self.current_player().await;
        let Some(invitee) = req.invitee.map(|k| k.user_id) else { return };
        match team::invite(me.user_id, &self.clean_open_id(), invitee) {
            Ok(t) => {
                println!("[Team] {} invites {} to party {}", me.user_id, invitee, t.team_id);
                let mut ids = Self::team_user_ids(&t);
                ids.push(invitee);
                self.push_team_snapshots(&ids).await;
                // the client opens the invitation window only on the chat message TeamMember.invite
                // (UChatService → UTeamService), the snapshot's pending_invitation does not create it
                if let Some(inv) = team::invitations_for(invitee).into_iter().find(|i| i.team_id == t.team_id && i.inviter == me.user_id) {
                    let p = self.team_invitation_proto(&inv, team_now()).await;
                    self.chat_push_team_member(invitee, &me, tgf::d_chat_message_team_member::Action::Invite(p)).await;
                }
            }
            Err(e) => println!("[Team] invitation {} -> {} rejected: {}", me.user_id, invitee, e),
        }
    }

    pub(super) async fn lobby_team_accept(&self, req: tgf::CsTeamApproveInvitationReq) {
        let me = self.current_player().await;
        let team_id = req.team_key.map(|k| k.team_id).unwrap_or_default();
        match team::accept(me.user_id, &self.clean_open_id(), team_id) {
            Ok((t, old)) => {
                println!("[Team] {} joined party {}", me.user_id, team_id);
                let mut ids = Self::team_user_ids(&t);
                if let Some(old) = old {
                    ids.extend(Self::team_user_ids(&old));
                }
                self.push_team_snapshots(&ids).await;
            }
            Err(e) => {
                println!("[Team] {} did not join party {}: {}", me.user_id, team_id, e);
                self.push_team_snapshots(&[me.user_id]).await;
            }
        }
    }

    pub(super) async fn lobby_team_reject(&self, req: tgf::CsTeamRejectInvitationReq) {
        let me = self.current_player().await;
        let team_id = req.team_key.map(|k| k.team_id).unwrap_or_default();
        let mut ids = vec![me.user_id];
        let rejected = team::reject(me.user_id, team_id);
        if let Some(inv) = &rejected {
            ids.push(inv.inviter);
        }
        self.push_team_snapshots(&ids).await;
        if let Some(inv) = rejected {
            let p = self.team_invitation_proto(&inv, team_now()).await;
            self.chat_push_team_member(inv.inviter, &me, tgf::d_chat_message_team_member::Action::Reject(p)).await;
        }
    }

    pub(super) async fn lobby_team_exit(&self) {
        let me = self.current_player().await;
        self.lobby_matching_cancel("team_exit");
        let mut ids = vec![me.user_id];
        if let Some(t) = team::leave(me.user_id) {
            ids.extend(Self::team_user_ids(&t));
        }
        self.push_team_snapshots(&ids).await;
    }

    pub(super) async fn lobby_team_remove_member(&self, req: tgf::CsTeamRemoveMemberReq) {
        let me = self.current_player().await;
        let Some(target) = req.user_key.map(|k| k.user_id) else { return };
        match team::remove_member(me.user_id, target) {
            Ok(after) => {
                let mut ids = vec![target];
                if let Some(t) = after {
                    ids.extend(Self::team_user_ids(&t));
                }
                self.push_team_snapshots(&ids).await;
            }
            Err(e) => println!("[Team] {} cannot kick {}: {}", me.user_id, target, e),
        }
    }

    pub(super) async fn lobby_team_transfer_captain(&self, req: tgf::CsTeamTransferCaptainReq) {
        let me = self.current_player().await;
        let Some(target) = req.user_key.map(|k| k.user_id) else { return };
        match team::transfer_captain(me.user_id, target) {
            Ok(t) => self.push_team_snapshots(&Self::team_user_ids(&t)).await,
            Err(e) => println!("[Team] {} did not transfer leadership to {}: {}", me.user_id, target, e),
        }
    }

    pub(super) async fn lobby_team_ready(&self, ready: bool) {
        let me = self.current_player().await;
        if let Some(t) = team::set_ready(me.user_id, ready) {
            self.push_team_snapshots(&Self::team_user_ids(&t)).await;
        }
    }
}

// ---- RPC wrappers (handle_<rpc> in handlers.rs delegates here) ----
impl GatewaySession {
    pub(super) async fn team_send_invitation_impl(&self, req: crate::tgf::CsTeamSendInvitationReq) -> Option<crate::tgf::ScTeamSendInvitationRsp> {
        self.lobby_team_invite(req).await;
        Some(crate::tgf::ScTeamSendInvitationRsp {})
    }

    pub(super) async fn team_approve_invitation_impl(&self, req: crate::tgf::CsTeamApproveInvitationReq) -> Option<crate::tgf::ScTeamApproveInvitationRsp> {
        self.lobby_team_accept(req).await;
        Some(crate::tgf::ScTeamApproveInvitationRsp {})
    }

    pub(super) async fn team_reject_invitation_impl(&self, req: crate::tgf::CsTeamRejectInvitationReq) -> Option<crate::tgf::ScTeamRejectInvitationRsp> {
        self.lobby_team_reject(req).await;
        Some(crate::tgf::ScTeamRejectInvitationRsp {})
    }

    pub(super) async fn team_exit_impl(&self, _req: crate::tgf::CsTeamExitReq) -> Option<crate::tgf::ScTeamExitRsp> {
        self.lobby_team_exit().await;
        Some(Default::default())
    }

    pub(super) async fn team_remove_member_impl(&self, req: crate::tgf::CsTeamRemoveMemberReq) -> Option<crate::tgf::ScTeamRemoveMemberRsp> {
        self.lobby_team_remove_member(req).await;
        Some(crate::tgf::ScTeamRemoveMemberRsp {})
    }

    pub(super) async fn team_transfer_captain_impl(&self, req: crate::tgf::CsTeamTransferCaptainReq) -> Option<crate::tgf::ScTeamTransferCaptainRsp> {
        self.lobby_team_transfer_captain(req).await;
        Some(Default::default())
    }

    pub(super) async fn team_send_battle_confirm_impl(&self, _req: crate::tgf::CsTeamSendBattleConfirmReq) -> Option<crate::tgf::ScTeamSendBattleConfirmRsp> {
        self.lobby_team_ready(true).await;
        Some(crate::tgf::ScTeamSendBattleConfirmRsp {})
    }

    pub(super) async fn team_send_battle_unconfirm_impl(&self, _req: crate::tgf::CsTeamSendBattleUnconfirmReq) -> Option<crate::tgf::ScTeamSendBattleUnconfirmRsp> {
        self.lobby_team_ready(false).await;
        Some(Default::default())
    }
}

fn team_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or_default()
}

impl GatewaySession {
    /// Rejection text if someone in the group wears gear outside the limit of the tab's voted map (`match_checks::gear_rule`)
    async fn lobby_gear_refusal(&self, me: &str, members: &[Member]) -> Option<String> {
        let level = launcher_lobby::voted_level(me)?;
        let allowed = crate::lobby::match_checks::gear_rule(level)?;
        let mut lines = Vec::new();
        for m in members {
            let Some(p) = self.db.get::<PlayerModel>("players", &m.open_id).await else { continue };
            let worn = worn_gear(&p).into_iter().filter(|t| self.item_is_gear(*t)).map(|t| self.item_rarity(t));
            let bad = gear_violations(worn, &allowed);
            // 09.11: gear in the backpack and the small backpack — not above the map limit (people carried blue gear onto the "grey-green"
            // Castle in the backpack). The lower bound (Hard: epic and above) — only for worn gear.
            // 14.11: swords in the bags are not checked ("the check for swords must be removed in the inventory too")
            let carried = carried_gear(&p).into_iter().filter(|t| self.item_is_gear(*t) && !self.item_is_sword(*t)).map(|t| self.item_rarity(t));
            let bad_bag = carried_violations(carried, &allowed);
            if bad.is_empty() && bad_bag.is_empty() {
                continue;
            }
            let who = if m.open_id == me { "У тебя".to_string() } else { format!("У {}", active_role_name(&p)) };
            if !bad.is_empty() {
                lines.push(format!("{} надето не по правилам: {}.", who, bad.join(", ")));
            }
            if !bad_bag.is_empty() {
                lines.push(format!("{} в рюкзаке не по правилам: {}.", who, bad_bag.join(", ")));
            }
        }
        if lines.is_empty() {
            return None;
        }
        Some(format!("{}: {}\n{}\nСними эти вещи или убери их из рюкзака на склад и нажми «Старт» снова.", map_name(level), rule_text(&allowed), lines.join("\n")))
    }
}

/// type_ids worn by the current character (hall_role_guid), except the sword master's class slots (14.11: not checked,
/// see `in_class_weapon_slot`)
fn worn_gear(p: &PlayerModel) -> Vec<i32> {
    p.roles
        .iter()
        .find(|r| r.role_guid == p.hall_role_guid)
        .and_then(|r| r.data.deploy_role.as_ref())
        .and_then(|d| d.wearing.as_ref())
        .map(|w| {
            w.items.iter().filter(|i| !super::inventory_handlers::in_class_weapon_slot(i)).filter_map(|i| i.basic.as_ref()).map(|b| b.type_id).filter(|t| *t != 0).collect()
        })
        .unwrap_or_default()
}

/// type_ids of gear in the backpack and the small backpack of the current character (hall_role_guid)
fn carried_gear(p: &PlayerModel) -> Vec<i32> {
    let Some(d) = p.roles.iter().find(|r| r.role_guid == p.hall_role_guid).and_then(|r| r.data.deploy_role.as_ref()) else {
        return vec![];
    };
    [d.backpack.as_ref(), d.small_backpack.as_ref()]
        .into_iter()
        .flatten()
        .flat_map(|b| b.items.iter())
        .filter_map(|i| i.basic.as_ref())
        .map(|b| b.type_id)
        .filter(|t| *t != 0)
        .collect()
}

/// Gear in the bags: a violation — rarity above the highest allowed one (on Hard the "epic and above" limit is a lower one,
/// anything is allowed in the bags)
fn carried_violations(carried: impl Iterator<Item = i32>, allowed: &[i32]) -> Vec<String> {
    let top = allowed.iter().copied().max().unwrap_or(i32::MAX);
    gear_violations(carried.filter(|r| *r > top), allowed)
}

fn active_role_name(p: &PlayerModel) -> String {
    p.roles.iter().find(|r| r.role_guid == p.hall_role_guid).map_or_else(|| p.open_id.clone(), |r| r.role_name.clone())
}

/// Rarities missing from the list — in words (player text, e.g. "2 blue"); rarity 0 (not in the tables) is not counted
fn gear_violations(worn: impl Iterator<Item = i32>, allowed: &[i32]) -> Vec<String> {
    let mut count: std::collections::BTreeMap<i32, usize> = std::collections::BTreeMap::new();
    for r in worn.filter(|r| *r != 0 && !allowed.contains(r)) {
        *count.entry(r).or_default() += 1;
    }
    count.into_iter().map(|(r, n)| format!("{} × {}", rarity_word(r), n)).collect()
}

fn rarity_word(r: i32) -> &'static str {
    use tgf::EnItemRarity as R;
    match R::try_from(r).unwrap_or(R::None) {
        R::Rotted => "гнилое",
        R::Normal => "серое",
        R::Uncommon => "зелёное",
        R::Enhanced => "синее",
        R::Epic => "эпическое",
        R::Legendary => "легендарное",
        R::Unique => "уникальное",
        R::Exceptional => "исключительное",
        R::None => "без редкости",
    }
}

fn rule_text(allowed: &[i32]) -> String {
    use tgf::EnItemRarity as R;
    if !allowed.contains(&(R::Enhanced as i32)) && !allowed.contains(&(R::Epic as i32)) {
        "только серое и зелёное снаряжение".into()
    } else if !allowed.contains(&(R::Uncommon as i32)) && !allowed.contains(&(R::Enhanced as i32)) && allowed.contains(&(R::Epic as i32)) {
        "всё снаряжение — эпическое и выше".into()
    } else {
        let mut words: Vec<&str> = allowed.iter().filter(|r| **r != 0).map(|r| rarity_word(*r)).collect();
        words.dedup();
        format!("разрешено снаряжение: {}", words.join(", "))
    }
}

fn map_name(level: i64) -> &'static str {
    crate::lobby::launcher_lobby::map_title(level).unwrap_or("Эта карта")
}

#[cfg(test)]
mod gear_tests {
    use super::*;

    #[test]
    fn gear_violation_words() {
        use tgf::EnItemRarity as R;
        let green = [R::None as i32, R::Rotted as i32, R::Normal as i32, R::Uncommon as i32];
        let worn = [R::Normal, R::Uncommon, R::Enhanced, R::Enhanced, R::Epic, R::None].map(|r| r as i32);
        assert_eq!(gear_violations(worn.into_iter(), &green), vec!["синее × 2", "эпическое × 1"]);
        assert!(gear_violations([R::Normal as i32, 0].into_iter(), &green).is_empty());
        assert_eq!(rule_text(&green), "только серое и зелёное снаряжение");
        let epic = [R::Epic, R::Exceptional, R::Legendary, R::Unique].map(|r| r as i32);
        assert_eq!(rule_text(&epic), "всё снаряжение — эпическое и выше");
        assert_eq!(gear_violations([R::Uncommon as i32].into_iter(), &epic), vec!["зелёное × 1"]);
        // 09.11: in the backpack — only the upper bound
        let bag = [R::Normal as i32, R::Uncommon as i32, R::Enhanced as i32, R::Legendary as i32, 0];
        assert_eq!(carried_violations(bag.into_iter(), &green), vec!["синее × 1", "легендарное × 1"]);
        assert!(carried_violations(bag.into_iter(), &epic).is_empty(), "Hard: anything goes in the backpack");
    }
}

#[cfg(test)]
mod room_finish_tests {
    use super::*;

    #[test]
    fn finish_pushed_once_and_stops_room_events() {
        // the player is offline — the push is not delivered, but events 5–6 are taken: no repeat, startBattling and earlier — too
        assert!(room_finish_push("test_finish_a", "b_test_finish_1", None, None));
        assert!(!room_finish_push("test_finish_a", "b_test_finish_1", None, None));
        // after events 5–6 — clearing the room on the client (dirty_clear_battle_room), also once
        assert_eq!(room_sent_get("b_test_finish_1", "test_finish_a"), EV_CLEARED);
        assert!(!room_clear_push("test_finish_a", "b_test_finish_1", None));
        assert!(!room_sent_claim("b_test_finish_1", "test_finish_a", 4));
        assert!(room_finish_push("test_finish_b", "b_test_finish_1", None, None), "another participant gets their own");
    }
}
