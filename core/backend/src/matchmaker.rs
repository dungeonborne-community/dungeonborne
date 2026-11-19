use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Information about one game map
pub struct MapConfig {
    pub map_id: i32,
    pub level_type: i32,
    pub ue_path: &'static str,
    pub timeout: u32,
    pub competition_rule_id: i32,
    pub matching_pool_id: i32,
}

const fn mc(map_id: i32, level_type: i32, ue_path: &'static str, timeout: u32, rule: i32, pool: i32) -> MapConfig {
    MapConfig { map_id, level_type, ue_path, timeout, competition_rule_id: rule, matching_pool_id: pool }
}

pub const DEFAULT_LEVEL: i64 = 40003016; // Castle

pub fn is_pvp(level_id: i64) -> bool {
    get_map_config(level_id).level_type == 2002
}

pub fn get_map_config(level_id: i64) -> MapConfig {
    const MGM1: &str = "/TgfMaps/Scenes/Common/Level/L_Common_PVP01_MultiGameMode";
    match level_id {
        50000006 => mc(30102, 2002, MGM1, 1800, 90030, 20027), // Klotho (BO5_TRIAL)
        50000004 => mc(30102, 2002, MGM1, 1800, 90028, 20019), // BO5
        50000005 => mc(30202, 2002, "/TgfMaps/Scenes/Common/Level/L_Common_PVP02_MultiGameMode", 1800, 90029, 20020),
        50000003 => mc(30101, 2002, "/TgfMaps/Scenes/Common/Level/L_Common_PVP01_DeathMatch", 600, 90021, 20021),
        50000007 => mc(30201, 2002, "/TgfMaps/Scenes/Common/Level/L_Common_PVP02_DeathMatch", 600, 90031, 20028),
        40003015 => mc(10101, 2001, "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01", 2400, 90020, 20029),
        40003017 => mc(10102, 2001, "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic", 2400, 90024, 20031),
        40003019 => mc(10103, 2001, "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Hard", 2400, 90032, 20033),
        40003016 => mc(10201, 2001, "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main", 2400, 90023, 20030),
        40003018 => mc(10202, 2001, "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Classic", 2400, 90025, 20032),
        40003020 => mc(10203, 2001, "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Hard", 2400, 90033, 20034),
        _ => mc(10201, 2001, "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main", 2400, 90023, 20030),
    }
}

#[derive(Clone, Debug)]
pub struct QueuedPlayer {
    pub open_id: String,
    pub peer_ip: String,
    pub team_id: i64,
}

#[derive(Clone, Debug)]
pub struct MatchRoom {
    pub room_id: u64,
    pub level_id: i64,
    pub host: QueuedPlayer,
    pub clients: Vec<QueuedPlayer>,
    pub created_at: Instant,
}

/// Matchmaker decision for a specific player
#[derive(Clone, Debug)]
pub enum MatchDecision {
    /// Still waiting for other players in the queue
    Waiting {
        seconds_elapsed: u64,
        players_count: usize,
    },
    /// The player is assigned as HOST (Listen Server, Team 1)
    StartAsHost {
        map_path: String,
        map_id: i32,
        level_id: i64,
        room_id: u64,
        participants: Vec<QueuedPlayer>,
    },
    /// The player is assigned as CLIENT (Team 2, connects to the Host)
    JoinAsClient {
        host_open_id: String,
        host_ip: String,
        map_path: String,
        map_id: i32,
        level_id: i64,
        room_id: u64,
        participants: Vec<QueuedPlayer>,
    },
}

#[derive(Clone)]
pub struct Matchmaker {
    inner: Arc<Mutex<MatchmakerState>>,
}

struct MatchmakerState {
    next_room_id: u64,
    /// Selected level_id for each player (open_id -> level_id)
    selected_levels: HashMap<String, i64>,
    /// The player's assigned team in the current/last match (open_id -> team_id)
    player_teams: HashMap<String, i64>,
    /// Active waiting rooms by level_id
    rooms: HashMap<i64, MatchRoom>,
    /// Ready decisions for players (open_id -> MatchDecision)
    decisions: HashMap<String, MatchDecision>,
    /// Minimum players for an instant start (2 for the 1v1 duel test in Klotho)
    min_players: usize,
    /// Wait timeout
    wait_timeout: Duration,
}

impl Matchmaker {
    pub fn new(min_players: usize, wait_timeout_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(MatchmakerState {
                next_room_id: 1000,
                selected_levels: HashMap::new(),
                player_teams: HashMap::new(),
                rooms: HashMap::new(),
                decisions: HashMap::new(),
                min_players,
                wait_timeout: Duration::from_secs(wait_timeout_secs),
            })),
        }
    }

    pub async fn get_player_level(&self, open_id: &str) -> i64 {
    let state = self.inner.lock().await;
    *state.selected_levels.get(open_id).unwrap_or(&DEFAULT_LEVEL)
    }

    /// Remembers the map the player chose in the Hall (RPC level_select)
    pub async fn set_player_level(&self, open_id: &str, level_id: i64) {
        let mut state = self.inner.lock().await;
        state.selected_levels.insert(open_id.to_string(), level_id);
    }

    /// Returns the player's assigned team_id (1 for the Host/Team A, 2 for a Client/Team B)
    pub async fn get_player_team(&self, open_id: &str) -> i64 {
        let state = self.inner.lock().await;
        *state.player_teams.get(open_id).unwrap_or(&1)
    }

    /// Puts the player into the match search queue (default 50000006 — Klotho Trial)
    pub async fn enqueue_player(&self, open_id: &str, peer_ip: &str) {
        let mut guard = self.inner.lock().await;
        let state = &mut *guard; // <-- Explicitly get &mut MatchmakerState

        state.decisions.remove(open_id);

        // By default queue for the Klotho Trial (50000006)
        let level_id = *state.selected_levels.get(open_id).unwrap_or(&DEFAULT_LEVEL);

        if let Some(room) = state.rooms.get_mut(&level_id) {
            if room.host.open_id != open_id && !room.clients.iter().any(|c| c.open_id == open_id) {
                // Alternate teams: Host = Team 1, 1st client = Team 2, 2nd client = Team 1, etc.
                let team_id = if room.clients.len() % 2 == 0 { 2 } else { 1 };
                state.player_teams.insert(open_id.to_string(), team_id);

                let player = QueuedPlayer {
                    open_id: open_id.to_string(),
                    peer_ip: peer_ip.to_string(),
                    team_id,
                };
                println!(
                    "[Matchmaker] Player {} ({}) joined Room #{} as TEAM {} (Host: {})",
                    open_id, peer_ip, room.room_id, team_id, room.host.open_id
                );
                room.clients.push(player);
            }
        } else {
            state.next_room_id += 1;
            let room_id = state.next_room_id;
            state.player_teams.insert(open_id.to_string(), 1);

            let player = QueuedPlayer {
                open_id: open_id.to_string(),
                peer_ip: peer_ip.to_string(),
                team_id: 1,
            };
            println!(
                "[Matchmaker] Player {} ({}) created Room #{} as HOST / TEAM 1 (level_id={})",
                open_id, peer_ip, room_id, level_id
            );
            state.rooms.insert(
                level_id,
                MatchRoom {
                    room_id,
                    level_id,
                    host: player,
                    clients: Vec::new(),
                    created_at: Instant::now(),
                },
            );
        }
    }

    /// Cancels the match search for the player
    pub async fn cancel_player(&self, open_id: &str) {
        let mut state = self.inner.lock().await;
        state.decisions.remove(open_id);
        state.rooms.retain(|_, room| room.host.open_id != open_id);
        for room in state.rooms.values_mut() {
            room.clients.retain(|c| c.open_id != open_id);
        }
    }

    /// Checks the search status (called on every client poll)
    pub async fn poll_status(&self, open_id: &str) -> MatchDecision {
        let mut state = self.inner.lock().await;

        if let Some(dec) = state.decisions.remove(open_id) {
            return dec;
        }

        let level_id = *state.selected_levels.get(open_id).unwrap_or(&DEFAULT_LEVEL);
        let min_players = state.min_players;
        let timeout = state.wait_timeout;

        let should_launch = if let Some(room) = state.rooms.get(&level_id) {
            let total = 1 + room.clients.len();
            total >= min_players || room.created_at.elapsed() >= timeout
        } else {
            false
        };

        if should_launch {
            if let Some(room) = state.rooms.remove(&level_id) {
                let cfg = get_map_config(room.level_id);
                let mut all_participants = vec![room.host.clone()];
                all_participants.extend(room.clients.clone());

                let host_dec = MatchDecision::StartAsHost {
                    map_path: cfg.ue_path.to_string(),
                    map_id: cfg.map_id,
                    level_id: room.level_id,
                    room_id: room.room_id,
                    participants: all_participants.clone(),
                };
                state
                    .decisions
                    .insert(room.host.open_id.clone(), host_dec);

                for client in &room.clients {
                    let client_dec = MatchDecision::JoinAsClient {
                        host_open_id: room.host.open_id.clone(),
                        host_ip: room.host.peer_ip.clone(),
                        map_path: cfg.ue_path.to_string(),
                        map_id: cfg.map_id,
                        level_id: room.level_id,
                        room_id: room.room_id,
                        participants: all_participants.clone(),
                    };
                    state.decisions.insert(client.open_id.clone(), client_dec);
                }

                println!(
                    "[Matchmaker] Room #{} LAUNCHED (Clotho map_id={})! Host={} (Team 1), Clients={}",
                    room.room_id,
                    cfg.map_id,
                    room.host.open_id,
                    room.clients.len()
                );

                if let Some(dec) = state.decisions.remove(open_id) {
                    return dec;
                }
            }
        }

        if let Some(room) = state.rooms.get(&level_id) {
            MatchDecision::Waiting {
                seconds_elapsed: room.created_at.elapsed().as_secs(),
                players_count: 1 + room.clients.len(),
            }
        } else {
            MatchDecision::Waiting {
                seconds_elapsed: 0,
                players_count: 1,
            }
        }
    }
}