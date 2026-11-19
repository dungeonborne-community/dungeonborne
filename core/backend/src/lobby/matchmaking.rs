//! Matchmaker: a heap-based ticket queue (oldest first) per pool (level_id, group size): solo, duo and trio have
//! their own queues (user decision 03.11) — a room holds only groups of one size.
//! A ticket is a solo player or a whole party. A tick every 500 ms assembles match rooms and asks the allocator for a server.
//! Match events (battle_matching_log_sync) — as in the reference, one per push: startMatching (MATCHING) on
//! queueing, matched (CAN_JOIN_IN = 8: room_key, room_data with ds_address, participants) when the room is assembled
//! and a server is assigned, finished (FINISHED = 20) after startBattling of the match room. After that the client goes to
//! battle_room_heartbeat (room events: DsInitFinish with the address and keys).
//!
//! Group limit per room — by map and group size ([`max_teams`]: start sets, the agent's census 03.11,
//! docs/DS_PIPELINE.md "Spawn points by map"). env settings: MATCH_ROOM_TEAMS — a group cap on top of the map
//! (for tests), MATCH_MIN_TEAMS (1), MATCH_WAIT_SECS (15) — a room starts when fully assembled or when the oldest
//! ticket has been waiting longer than MATCH_WAIT_SECS.
//!
//! Match room state machine (docs/DS_PIPELINE.md, T3): WaitingServer → DsInit (server assigned, the backend sends it
//! init_player — crate::ds) → DsReady (the server replied with player keys) → then clients get room
//! events as in the reference (docs/REF_BATTLE_ROOM_FLOW.md): startDsInit, dsInitFinish, startLoading, startBattling.

use super::{allocator, hub};
use crate::tgf;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug)]
pub struct Member {
    pub open_id: String,
    pub user_id: u64,
}

struct Ticket {
    members: Vec<Member>,
    level_id: i64,
    enqueued: Instant,
    /// group size of the queue pool: usually the number of members; Hard (skirmish mode) and Challenger Mode — 3 (`queue_size`)
    queue: usize,
}

#[derive(Clone, Debug)]
pub enum PlayerStatus {
    Searching { matching_id: String },
    /// Room assembled; ds_address = None — waiting for a free server
    Matched { matching_id: String, battle_id: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RoomStage {
    /// Room assembled, no free server
    WaitingServer,
    /// Server assigned, waiting for the init_player reply
    DsInit,
    /// The server accepted the players: keys are there, the address can be given to clients
    DsReady,
}

/// A player's key on the server (DBattlePlayerDsKey): per the reference ds_user_key = Steam ID (open_id), ds_token — from the server
#[derive(Clone, Debug)]
pub struct DsKey {
    pub user_id: u64,
    pub ds_user_key: String,
    pub token: String,
    pub encryption_key: String,
}

#[derive(Clone, Debug)]
pub struct BattleRoom {
    pub battle_id: String,
    pub level_id: i64,
    pub map_path: String,
    pub members: Vec<Member>,
    pub ds_address: Option<String>,
    pub created_ts: i64,
    pub stage: RoomStage,
    /// Time of entering DsInit / DsReady (s), for the timepoint of room events
    pub ds_init_ts: i64,
    pub ds_ready_ts: i64,
    pub ds_keys: Vec<DsKey>,
    /// room group size (1 solo, 2 duo, 3 trio) — the map's dynamic_option_args follow it (ds::driver::host_task)
    pub party_size: usize,
    /// custom game (05.11): map_detail map_id of the arena (30101…30302) — the server task is based on it, not on the level
    pub arena_map_id: Option<i32>,
    created: Instant,
    notified: bool,
    /// who has already been sent the match FINISHED (notify_finished)
    finished_sent: Vec<String>,
    /// left the hall of this room — we do not take them back into it: the client does not return to a room with the same battle_id
    /// (04.11 11:33: it got events 1–4 with a BATTLING snapshot, did not send battle_fetch_battle_user_data, did not travel to
    /// the server), a new search goes to another room
    left: Vec<String>,
    /// init_player has already been handed to the crate::ds driver (reset for a retry)
    init_requested: bool,
    /// who has finished: the raid result was applied (member_result) or they went to a new search/room (abandon_locked);
    /// when everyone has, the room closes (M3)
    results: Vec<String>,
    /// a member without a gateway connection (game closed) — since when; longer than offline_grace — counted as left
    offline_since: HashMap<String, Instant>,
    /// R1-stuck (05.11): when the member was sent the match FINISHED (after startBattling — the client travelled to the server)
    battling_at: HashMap<String, Instant>,
    /// the server requested the member's match data (/ds/battle_user_data) — they reached the server
    arrived: Vec<String>,
    /// launcher lobby room (launcher_lobby): created without members, the hall waits for them for the whole timer
    pub lobby: bool,
    /// the hall is open during maintenance (only our own people, gm/admin, can do that): it waits for players instead of closing immediately
    pub staff_hall: bool,
    /// the player's team number in the match (1…253): one for `faction_id` and `MyTeamID`, fixed on entering the room
    team_no: HashMap<String, i64>,
    team_index: HashMap<i64, i64>,
}

impl BattleRoom {
    /// A member's team number: the room's first team is 1, the next is 2. An already assigned one does not change
    fn assign_team_number(&mut self, m: &Member) {
        if self.team_no.contains_key(&m.open_id) {
            return;
        }
        let next = (self.team_index.len() % 253) as i64 + 1;
        let n = *self.team_index.entry(crate::lobby::team::battle_team_id(m.user_id)).or_insert(next);
        self.team_no.insert(m.open_id.clone(), n);
    }

    fn new(battle_id: String, level_id: i64, members: Vec<Member>, ds_address: Option<String>) -> Self {
        let now = now_ts();
        let stage = if ds_address.is_some() { RoomStage::DsInit } else { RoomStage::WaitingServer };
        // group size: the largest match team among the members (a party is one team, solo — their own user_id)
        let mut sizes: HashMap<i64, usize> = HashMap::new();
        for m in &members {
            *sizes.entry(crate::lobby::team::battle_team_id(m.user_id)).or_default() += 1;
        }
        let party_size = sizes.values().copied().max().unwrap_or(1);
        let mut room = BattleRoom {
            party_size,
            arena_map_id: None,
            map_path: map_path(level_id).to_string(),
            battle_id,
            level_id,
            members,
            ds_address,
            created_ts: now,
            stage,
            ds_init_ts: if stage == RoomStage::DsInit { now } else { 0 },
            ds_ready_ts: 0,
            ds_keys: Vec::new(),
            created: Instant::now(),
            notified: false,
            finished_sent: Vec::new(),
            left: Vec::new(),
            init_requested: false,
            results: Vec::new(),
            offline_since: HashMap::new(),
            battling_at: HashMap::new(),
            arrived: Vec::new(),
            lobby: false,
            staff_hall: false,
            team_no: HashMap::new(),
            team_index: HashMap::new(),
        };
        for m in room.members.clone() {
            room.assign_team_number(&m);
        }
        room
    }

    #[cfg(test)]
    pub fn for_test(battle_id: &str, level_id: i64, members: &[(&str, u64)]) -> Self {
        let members = members.iter().map(|(o, u)| Member { open_id: o.to_string(), user_id: *u }).collect();
        BattleRoom::new(battle_id.to_string(), level_id, members, None)
    }

    /// Time to close the room: a launcher lobby hall waits for members for the whole timer, including while there are none or all left
    /// (an empty one is closed by phase=empty), the others — when everyone has finished
    fn finished(&self, grace: Duration) -> bool {
        let wait_hall = self.lobby && allocator::hall_open(&self.battle_id) && (!super::maintenance::is_on() || self.staff_hall);
        !wait_hall && self.all_done(grace)
    }

    /// All members have finished: result applied or the game has been closed longer than grace
    fn all_done(&self, grace: Duration) -> bool {
        self.members.iter().all(|m| {
            self.results.contains(&m.open_id) || self.offline_since.get(&m.open_id).map_or(false, |t| t.elapsed() >= grace)
        })
    }

    /// A player's key on the server; until the server replies — open_id as ds_user_key (as in the reference) without a token
    pub fn ds_key_of(&self, user_id: u64) -> Option<DsKey> {
        self.ds_keys.iter().find(|k| k.user_id == user_id).cloned().or_else(|| {
            self.members.iter().find(|m| m.user_id == user_id).map(|m| DsKey {
                user_id,
                ds_user_key: m.open_id.clone(),
                token: String::new(),
                encryption_key: String::new(),
            })
        })
    }
}

#[derive(Default)]
struct State {
    next_id: u64,
    /// level_id -> heap (queueing time in ms since start, ticket id); cancelled tickets are removed lazily
    /// (level_id, group size) -> heap (queueing time in ms since start, ticket id)
    pools: HashMap<(i64, usize), BinaryHeap<Reverse<(u128, u64)>>>,
    tickets: HashMap<u64, Ticket>,
    players: HashMap<String, PlayerStatus>,
    rooms: HashMap<String, BattleRoom>,
    /// the player's last closed room (open_id → battle_id) — for `room_closed` in the reply to a repeated result
    closed: HashMap<String, String>,
    /// closed rooms: the client heartbeat for them gets "match over" (lobby_handlers::room_finish_push)
    closed_battles: std::collections::HashSet<String>,
    /// entered a launcher lobby hall (join_room): the next tick sends them "match found", and the mod the new roster
    lobby_joined: Vec<(String, Vec<Member>)>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();
static EPOCH: OnceLock<Instant> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    // ticket/room numbers start from the launch timestamp: a battle_id after a backend restart will not match an old one
    // for which the client still sends battle_room_heartbeat
    STATE.get_or_init(|| Mutex::new(State { next_id: (now_ts() as u64 % 1_000_000) * 1000, ..Default::default() }))
}

fn env_num(key: &str, default: u64) -> u64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

/// How many groups of size `party` fit into a match of level `level_id`: an ALyraPlayerStartSet = a team (3 starts);
/// sets with LevelArgs are enabled only for solo (dynamic_option_args count 1); GameSession.MaxPlayers = 20.
/// Census 03.11: dungeon — 16 sets + 4 solo, castle — 8 + 6 (Hard 8 + 5). Also no more than the waiting hall's
/// sets (L_Dungeon_Matching_01 — 18, L_GothicCastle_Matching — 20): not verified whether the hall seats by sets.
pub fn max_teams(level_id: i64, party: usize) -> usize {
    const MAX_PLAYERS: usize = 20;
    spawn_sets(level_id, party).min(MAX_PLAYERS / party.max(1)).max(1)
}

/// Spawn point sets (groups) on the map for the options of size `party`, no more than the hall's seats
fn spawn_sets(level_id: i64, party: usize) -> usize {
    let (always, solo_only, hall) = match level_id {
        40003015 | 40003017 | 40003019 => (16, 4, 18),
        40003016 | 40003018 => (8, 6, 20),
        40003020 => (8, 5, 20),
        _ => (8, 0, 18),
    };
    let sets = if party.max(1) == 1 { always + solo_only } else { always };
    sets.min(hall).max(1)
}

/// Season (playtest 07.11): rooms are not strictly by solo/duo/trio — the room size (launcher vote / queue pool)
/// is a cap: a solo room — only solo, duo — duo and solo, trio — trio, duo and solo (user clarification).
/// Map options and seats — by room size; the limit is seats (players) and spawn point sets (groups).
pub fn mixed() -> bool {
    crate::season::features().mixed_party_rooms
}

/// Hard (40003019 Crypt, 40003020 Castle) — the original's "skirmish mode" (hint MeleeMode_Description): groups of any
/// size are mixed when entering the instance
pub fn skirmish(level_id: i64) -> bool {
    matches!(level_id, 40003019 | 40003020)
}

/// Whether groups of different sizes are mixed at a level: season or skirmish mode
pub fn mixed_level(level_id: i64) -> bool {
    mixed() || skirmish(level_id)
}

/// Queue pool size for a group: skirmish mode (Hard) and Challenger Mode ("challenge mode": solo and duo against trio,
/// hint SelectMap_Match_description) — into rooms of three; otherwise — as many as there are players in the group
pub fn queue_size(level_id: i64, group: usize, challenger: bool) -> usize {
    if skirmish(level_id) || challenger { 3 } else { group.max(1) }
}

/// Whether a room of size `room_party` lets in a group of `group` players
fn room_accepts_for(room_party: usize, group: usize, mixed: bool) -> bool {
    let group = group.max(1);
    if mixed { group <= room_party.max(1) } else { group == room_party.max(1) }
}

/// Groups (match teams) in the roster
fn groups_of<'a>(members: impl Iterator<Item = &'a Member>) -> usize {
    members.map(|m| crate::lobby::team::battle_team_id(m.user_id)).collect::<std::collections::HashSet<_>>().len()
}

/// Map path for StartLocalBattle/the server by level_id (like SelectedMaps in LocalDev + PVP arenas).
/// Custom game arena map by map_detail map_id (30101…30302, see room::pvp_arena)
pub fn arena_path(arena_map_id: i32) -> String {
    let n = (arena_map_id / 100) % 10; // 301xx -> 1
    let mode = if arena_map_id % 100 == 2 { "MultiGameMode" } else { "DeathMatch" };
    format!("/TgfMaps/Scenes/Common/Level/L_Common_PVP0{}_{}", n.clamp(1, 3), mode)
}

pub fn map_path(level_id: i64) -> &'static str {
    match level_id {
        40003015 => "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01",
        40003017 => "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic",
        40003019 => "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Hard",
        40003016 => "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main",
        40003018 => "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Classic",
        40003020 => "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Hard",
        50000004 | 50000006 => "/TgfMaps/Scenes/Common/Level/L_Common_PVP01_MultiGameMode",
        50000005 => "/TgfMaps/Scenes/Common/Level/L_Common_PVP02_MultiGameMode",
        50000003 => "/TgfMaps/Scenes/Common/Level/L_Common_PVP01_DeathMatch",
        50000007 => "/TgfMaps/Scenes/Common/Level/L_Common_PVP02_DeathMatch",
        _ => "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic",
    }
}

/// Members went to a new search/room: the old rooms no longer wait for them (the same open_id is back in the lobby — the
/// "not in game" timer will not run, and no result will come from the old room: 03.11 resetcycle run, the slot hung busy).
/// Returns the rooms where everyone has now finished — close them after releasing the lock.
fn abandon_locked(st: &mut State, members: &[Member], grace: Duration) -> Vec<String> {
    // A room still without a server: just strike out those who left, remove an empty one. Otherwise it waits for a server and gives it
    // to "ghosts": on 03.11 16:02 a room assembled at 15:56 got the only slot, its events went to a player
    // who was already searching again — the server refused them the data (status not Matched), the queue stalled for 120 s.
    let mut empty = Vec::new();
    for room in st.rooms.values_mut().filter(|r| r.stage == RoomStage::WaitingServer) {
        let before = room.members.len();
        room.members.retain(|x| !members.iter().any(|m| m.open_id == x.open_id));
        if room.members.len() != before {
            println!("[Matchmaking] room {} (no server): went to a new search {}, remaining {}", room.battle_id,
                before - room.members.len(), room.members.len());
            if room.members.is_empty() {
                empty.push(room.battle_id.clone());
            }
        }
    }
    for b in empty {
        st.rooms.remove(&b);
        println!("[Matchmaking] room {} (no server) removed: no members", b);
    }
    let mut done = Vec::new();
    for room in st.rooms.values_mut().filter(|r| r.stage >= RoomStage::DsInit) {
        let mut touched = false;
        for m in members {
            if room.members.iter().any(|x| x.open_id == m.open_id) && !room.results.contains(&m.open_id) {
                room.results.push(m.open_id.clone());
                touched = true;
                println!("[Matchmaking] room {}: {} went to a new search — not waiting", room.battle_id, m.open_id);
            }
        }
        if touched && room.all_done(grace) {
            done.push(room.battle_id.clone());
        }
    }
    done
}

fn remove_members_locked(st: &mut State, members: &[Member]) {
    for m in members {
        if let Some(PlayerStatus::Searching { matching_id }) = st.players.remove(&m.open_id) {
            if let Ok(id) = matching_id.parse::<u64>() {
                st.tickets.remove(&id);
            }
        }
    }
}

/// Puts a ticket (solo or party) into the level_id queue. Returns matching_id.
/// The members' previous searches are cancelled.
pub fn enqueue(members: Vec<Member>, level_id: i64, challenger: bool) -> String {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    remove_members_locked(st, &members);
    let finished = abandon_locked(st, &members, offline_grace());
    st.next_id += 1;
    let id = st.next_id;
    let matching_id = id.to_string();
    let enqueued = Instant::now();
    let order = enqueued.duration_since(*EPOCH.get_or_init(Instant::now)).as_millis();
    for m in &members {
        st.players.insert(m.open_id.clone(), PlayerStatus::Searching { matching_id: matching_id.clone() });
    }
    println!(
        "[Matchmaking] ticket {} level={} players={}{} ({})",
        id,
        level_id,
        members.len(),
        if queue_size(level_id, members.len(), challenger) != members.len().max(1) { " → into rooms of three" } else { "" },
        members.iter().map(|m| m.open_id.as_str()).collect::<Vec<_>>().join(",")
    );
    let size = queue_size(level_id, members.len(), challenger);
    st.tickets.insert(id, Ticket { members, level_id, enqueued, queue: size });
    st.pools.entry((level_id, size)).or_default().push(Reverse((order, id)));
    drop(guard);
    for b in finished {
        close_room(&b);
    }
    matching_id
}

/// Takes a player off the search (a party ticket is removed entirely). Does not touch an assembled room.
pub fn cancel(open_id: &str, why: &str) {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let Some(PlayerStatus::Searching { matching_id }) = st.players.get(open_id).cloned() else { return };
    if let Some(ticket) = matching_id.parse::<u64>().ok().and_then(|id| st.tickets.remove(&id)) {
        for m in &ticket.members {
            st.players.remove(&m.open_id);
        }
        println!("[Matchmaking] ticket {} removed ({}, {})", matching_id, open_id, why);
    }
}

/// A match room without a queue (private room start): the server is assigned right away, None — no free ones.
/// After that everything is as with a queued match: the tick broadcasts battle_matching_log_sync (status 4 + room_key)
/// with matching_id = battle_id, battle_room_heartbeat takes the members and address from this room.
pub fn start_direct(members: Vec<Member>, level_id: i64, tag: &str) -> Option<BattleRoom> {
    start_direct_on(members, level_id, tag, None)
}

/// Like start_direct; `arena` — map_detail map_id of the custom game arena (the server map and task are based on it)
pub fn start_direct_on(members: Vec<Member>, level_id: i64, tag: &str, arena: Option<i32>) -> Option<BattleRoom> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let finished = abandon_locked(st, &members, offline_grace());
    if !finished.is_empty() {
        drop(guard);
        for b in finished {
            close_room(&b); // the old room's server goes into reset or is free — it may go to this one
        }
        guard = state().lock().unwrap();
    }
    let st = &mut *guard;
    st.next_id += 1;
    let battle_id = format!("{}{}_{}", tag, level_id, st.next_id);
    let map = arena.map(arena_path).unwrap_or_else(|| map_path(level_id).to_string());
    let ds_address = allocator::allocate(&battle_id, &map)?;
    remove_members_locked(st, &members);
    for m in &members {
        st.players.insert(
            m.open_id.clone(),
            PlayerStatus::Matched { matching_id: battle_id.clone(), battle_id: battle_id.clone() },
        );
    }
    println!("[Matchmaking] direct room {} ({} players, ds={})", battle_id, members.len(), ds_address);
    let mut room = BattleRoom::new(battle_id.clone(), level_id, members, Some(ds_address));
    if arena.is_some() {
        room.arena_map_id = arena;
        room.map_path = map;
    }
    st.rooms.insert(battle_id, room.clone());
    Some(room)
}

/// Rooms in DsInit that have not been sent init_player yet; marks them as sent (the crate::ds driver).
pub fn take_init_requests() -> Vec<BattleRoom> {
    let mut guard = state().lock().unwrap();
    let mut out = Vec::new();
    for room in guard.rooms.values_mut() {
        if room.stage == RoomStage::DsInit && !room.init_requested {
            room.init_requested = true;
            out.push(room.clone());
        }
    }
    out
}

/// How many rooms and players without a result: a graceful restart waits until it is 0 (deploy/vm/maintenance.sh)
pub fn rooms_summary() -> (usize, usize) {
    let st = state().lock().unwrap();
    let players = st.rooms.values().map(|r| r.members.iter().filter(|m| !r.results.contains(&m.open_id)).count()).sum();
    (st.rooms.len(), players)
}

/// Rooms with an assigned server (DsInit/DsReady) — the driver broadcasts room events to them (crate::ds::driver).
pub fn active_rooms() -> Vec<BattleRoom> {
    state().lock().unwrap().rooms.values().filter(|r| r.stage >= RoomStage::DsInit).cloned().collect()
}

/// init_player did not arrive/did not reply — the driver will retry on the next pass.
pub fn retry_init(battle_id: &str) {
    if let Some(room) = state().lock().unwrap().rooms.get_mut(battle_id) {
        room.init_requested = false;
    }
}

/// The server accepted the players (init_player reply or the fallback mode without ds-agent): keys and transition to DsReady.
pub fn set_ds_ready(battle_id: &str, keys: Vec<DsKey>) -> bool {
    let mut guard = state().lock().unwrap();
    let Some(room) = guard.rooms.get_mut(battle_id) else { return false };
    if room.stage == RoomStage::DsReady {
        return true;
    }
    room.stage = RoomStage::DsReady;
    room.ds_ready_ts = now_ts();
    room.ds_keys = keys;
    // those added to an open hall while the driver was waiting for the server (keys were read for the old roster) get their own token
    for m in room.members.clone() {
        add_join_key(room, &m);
    }
    println!("[Matchmaking] room {}: server ready, keys {}", battle_id, room.ds_keys.len());
    true
}

/// "Return to match" (match button, docs/RECONNECT.md): for a player whose match is running and has no result, again — matched of their
/// room; FINISHED will be sent again after startBattling. Returns (matching_id, battle_id). Room events are re-sent
/// by lobby_handlers (reset of what was sent).
pub fn rejoin(open_id: &str) -> Option<(String, String)> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let Some(PlayerStatus::Matched { matching_id, battle_id }) = st.players.get(open_id).cloned() else { return None };
    let room = st.rooms.get_mut(&battle_id).filter(|r| r.stage == RoomStage::DsReady && !r.results.iter().any(|o| o == open_id))?;
    room.finished_sent.retain(|o| o != open_id);
    notify_matched(room, &st.players, Some(open_id));
    println!("[Matchmaking] {} returns to match {}", open_id, battle_id);
    Some((matching_id, battle_id))
}

/// The player's result in the room already exists (or they went to a new search / did not come to the hall)
pub fn has_result(battle_id: &str, open_id: &str) -> bool {
    state().lock().unwrap().rooms.get(battle_id).map_or(false, |r| r.results.iter().any(|o| o == open_id))
}

/// Tickets in the queue (scaling: players are waiting and there is no free server)
pub fn queued() -> usize {
    state().lock().unwrap().tickets.len()
}

pub fn status(open_id: &str) -> Option<PlayerStatus> {
    state().lock().unwrap().players.get(open_id).cloned()
}

/// The player's token in their current match room (ds_player_key, issued in ds_init_finish). None — no room or no token yet.
pub fn token_of(open_id: &str) -> Option<String> {
    // current room — by Matched status; otherwise — the room where the player has no result yet. Someone who went to a new search
    // stays in the old room's members (with a result), and with several servers its token was taken — bad_token
    // (prod 03.11 18:33, 4 servers)
    let current = match status(open_id) {
        Some(PlayerStatus::Matched { battle_id, .. }) => Some(battle_id),
        _ => None,
    };
    let rooms = active_rooms();
    let token = |r: &BattleRoom| {
        let m = r.members.iter().find(|m| m.open_id == open_id)?;
        r.ds_key_of(m.user_id).map(|k| k.token).filter(|t| !t.is_empty())
    };
    if let Some(b) = current {
        if let Some(t) = rooms.iter().find(|r| r.battle_id == b).and_then(token) {
            return Some(t);
        }
    }
    rooms.iter().filter(|r| !r.results.iter().any(|o| o == open_id)).find_map(token)
}

pub fn room(battle_id: &str) -> Option<BattleRoom> {
    state().lock().unwrap().rooms.get(battle_id).cloned()
}

/// Room members without a result (still in the match) — for the public status (`launcher_lobby::public_instances`)
pub fn room_in_battle(battle_id: &str) -> usize {
    let st = state().lock().unwrap();
    st.rooms.get(battle_id).map_or(0, |r| r.members.iter().filter(|m| !r.results.contains(&m.open_id)).count())
}

/// The server requested a player's match data (/ds/battle_user_data) — the player reached their room's server.
/// 12.11: the player is marked "did not come to the hall" (`members_absent`) in a live room and never reached the server — the client was already
/// sent "match over" and the search cancel. We do not give them match data: otherwise the server let them into the raid, and after death or leaving
/// the client stayed in the match with no room end (k1ko 11.11 18:59). Reply — the battle_id of such a room.
pub fn missed_hall(open_id: &str) -> Option<String> {
    let st = state().lock().unwrap();
    let missed = |r: &BattleRoom| {
        r.members.iter().any(|m| m.open_id == open_id) && r.results.iter().any(|o| o == open_id) && !r.arrived.iter().any(|o| o == open_id)
    };
    if let Some(PlayerStatus::Matched { battle_id, .. }) = st.players.get(open_id) {
        return st.rooms.get(battle_id).filter(|r| missed(r)).map(|r| r.battle_id.clone());
    }
    // there is a live room where the player is still in the game — so they are travelling there
    let active = st.rooms.values().any(|r| r.stage >= RoomStage::DsInit && r.members.iter().any(|m| m.open_id == open_id) && !r.results.iter().any(|o| o == open_id));
    if active {
        return None;
    }
    st.rooms.values().find(|r| missed(r)).map(|r| r.battle_id.clone())
}

pub fn mark_arrived(open_id: &str) {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let Some(PlayerStatus::Matched { battle_id, .. }) = st.players.get(open_id) else { return };
    if let Some(room) = st.rooms.get_mut(battle_id) {
        if !room.arrived.iter().any(|o| o == open_id) {
            room.arrived.push(open_id.to_string());
        }
        room.battling_at.remove(open_id);
    }
}

/// How long to wait for entering the server after startBattling: env DS_ARRIVE_SECS, default 75 (the client's
/// RuClient ConnTimeout connect timeout is 60 s, with a margin), 0 — do not wait (off).
fn arrive_timeout() -> Option<Duration> {
    match env_num("DS_ARRIVE_SECS", 75) {
        0 => None,
        s => Some(Duration::from_secs(s)),
    }
}

/// R1-stuck: members who did not reach the server within arrive_timeout after startBattling (the client did not connect and
/// went to the lobby on timeout). Returns (battle_id, open_id) once; the caller removes them from the room (members_absent)
/// and sends "match over" + dirty_clear_battle_room, otherwise the next room sync gives the client the battle_id back and it goes
/// to reconnect again.
pub fn take_not_arrived() -> Vec<(String, String)> {
    let Some(limit) = arrive_timeout() else { return Vec::new() };
    let mut guard = state().lock().unwrap();
    let mut out = Vec::new();
    for room in guard.rooms.values_mut() {
        // in the hall the server does not request match data: it is needed only on the match map (06.11: a hall longer than DS_ARRIVE_SECS kicked
        // a player standing in it). Whoever did not come to the hall, the mod reports itself (`absent`); the countdown restarts from the end of the hall.
        if matches!(allocator::room_phase(&room.battle_id).as_deref(), Some("hosting" | "waiting" | "entering")) {
            room.battling_at.values_mut().for_each(|at| *at = Instant::now());
            continue;
        }
        let late: Vec<String> = room
            .battling_at
            .iter()
            .filter(|(o, at)| at.elapsed() >= limit && !room.arrived.contains(o) && !room.results.contains(o))
            .map(|(o, _)| o.clone())
            .collect();
        for o in late {
            room.battling_at.remove(&o);
            out.push((room.battle_id.clone(), o));
        }
    }
    out
}

/// How long a member may be without a gateway connection while the room waits for them. env DS_ROOM_OFFLINE_SECS,
/// default 0 = waits until the end of the match: as in the original, one can return at any moment of the match, the character stays
/// on the server (docs/RECONNECT.md). End of the raid — the death zone, results from everyone, DS_ROOM_TTL.
fn offline_grace() -> Duration {
    match env_num("DS_ROOM_OFFLINE_SECS", 0) {
        0 => Duration::MAX,
        s => Duration::from_secs(s),
    }
}

/// The player's raid result was applied (native result, rpc/handlers/raid_native.rs). We mark it in their room; results from everyone
/// (or the others have long been out of the game) — the room closes and the server is free (M3). Returns the closed room.
/// No ds_end: on the native path the server does not send it.
pub fn member_result(open_id: &str) -> Option<String> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let by_status = match st.players.get(open_id) {
        Some(PlayerStatus::Matched { battle_id, .. }) => Some(battle_id.clone()),
        _ => None,
    };
    let battle_id = by_status.filter(|b| st.rooms.contains_key(b)).or_else(|| {
        st.rooms.values().find(|r| r.stage >= RoomStage::DsInit && r.members.iter().any(|m| m.open_id == open_id)).map(|r| r.battle_id.clone())
    })?;
    let room = st.rooms.get_mut(&battle_id)?;
    if !room.results.iter().any(|o| o == open_id) {
        room.results.push(open_id.to_string());
    }
    println!("[Matchmaking] room {}: result {} ({} of {})", battle_id, open_id, room.results.len(), room.members.len());
    let done = room.all_done(offline_grace());
    drop(guard);
    if done {
        println!("[Matchmaking] room {}: results from all members", battle_id);
        close_room(&battle_id);
        return Some(battle_id);
    }
    None
}

/// Match found but the match has not started yet (waiting hall, connecting) — battle_id. For a search cancel at that moment.
pub fn matched_before_battle(open_id: &str) -> Option<String> {
    match status(open_id) {
        Some(PlayerStatus::Matched { battle_id, .. }) if !allocator::battle_started(&battle_id) => Some(battle_id),
        _ => None,
    }
}

/// The player cancelled the search with a match found before the match (04.11 06:04: battle_matching_exit a second after
/// CAN_JOIN_IN — the player stayed in the room, connection events kept coming to them, the client hung on "Match found,
/// connecting"). We remove them from the room: no result awaited, the token is removed; an empty room closes. "Match over"
/// is sent by the caller BEFORE this (room_finish_push takes matching_id from the Matched status).
pub fn leave_room(open_id: &str, battle_id: &str) {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let mut empty = false;
    if let Some(room) = st.rooms.get_mut(battle_id) {
        if let Some(uid) = room.members.iter().find(|m| m.open_id == open_id).map(|m| m.user_id) {
            room.ds_keys.retain(|k| k.user_id != uid);
        }
        room.members.retain(|m| m.open_id != open_id);
        room.results.retain(|o| o != open_id);
        if !room.left.iter().any(|o| o == open_id) {
            room.left.push(open_id.to_string());
        }
        // a lobby hall waits for those signed up until the end of the timer, even if the only one who entered left
        empty = room.members.is_empty() && !(room.lobby && allocator::hall_open(battle_id));
        println!("[Matchmaking] room {}: {} left before the match, remaining {}", battle_id, open_id, room.members.len());
    }
    if matches!(st.players.get(open_id), Some(PlayerStatus::Matched { battle_id: b, .. }) if b == battle_id) {
        st.players.remove(open_id);
    }
    st.closed.insert(open_id.to_string(), battle_id.to_string());
    drop(guard);
    // close an empty one: whoever left cannot return to this same room (M11, `left`), a new search goes to a new room
    if empty {
        close_room(battle_id);
    }
}

/// The player's last closed room
pub fn closed_room_of(open_id: &str) -> Option<String> {
    state().lock().unwrap().closed.get(open_id).cloned()
}

pub fn is_closed(battle_id: &str) -> bool {
    state().lock().unwrap().closed_battles.contains(battle_id)
}

/// A match room (with an assigned server) where the player is a member
/// The player's room and their teammates: members with the same match team, excluding the player
pub fn battle_mates(open_id: &str) -> Option<(String, Vec<String>)> {
    let battle = room_of_player(open_id)?;
    let st = state().lock().unwrap();
    let room = st.rooms.get(&battle)?;
    let me = room.members.iter().find(|m| m.open_id == open_id)?;
    let team = crate::lobby::team::battle_team_id(me.user_id);
    let mates = room
        .members
        .iter()
        .filter(|m| m.open_id != open_id && crate::lobby::team::battle_team_id(m.user_id) == team)
        .map(|m| m.open_id.clone())
        .collect();
    Some((battle, mates))
}

/// The player's team number in the current room (1…253)
pub fn team_number(open_id: &str) -> Option<i64> {
    let battle = room_of_player(open_id)?;
    state().lock().unwrap().rooms.get(&battle)?.team_no.get(open_id).copied()
}

pub fn room_of_player(open_id: &str) -> Option<String> {
    let st = state().lock().unwrap();
    // like token_of: the current one by Matched status, otherwise — where the player has no result yet
    if let Some(PlayerStatus::Matched { battle_id, .. }) = st.players.get(open_id) {
        if st.rooms.get(battle_id).map_or(false, |r| r.stage >= RoomStage::DsInit) {
            return Some(battle_id.clone());
        }
    }
    st.rooms
        .values()
        .find(|r| r.stage >= RoomStage::DsInit && r.members.iter().any(|m| m.open_id == open_id) && !r.results.iter().any(|o| o == open_id))
        .map(|r| r.battle_id.clone())
}

/// Level (map) of the player's room
pub fn room_level(open_id: &str) -> Option<i64> {
    let battle = room_of_player(open_id)?;
    state().lock().unwrap().rooms.get(&battle).map(|r| r.level_id)
}

/// Address of the game server of the player's room (the real one, for the relay)
pub fn ds_of_player(open_id: &str) -> Option<String> {
    let battle = room_of_player(open_id)?;
    state().lock().unwrap().rooms.get(&battle)?.ds_address.clone()
}

/// Waiting hall: the server reported who did not come (`absent`), or that nobody came (`all`, phase=empty). We no longer
/// wait for them; returns (who was marked, whether the room is closed). The caller sends "match over" to the no-show clients.
pub fn members_absent(battle_id: &str, ids: &[String], all: bool) -> (Vec<String>, bool) {
    let mut guard = state().lock().unwrap();
    let Some(room) = guard.rooms.get_mut(battle_id) else { return (vec![], false) };
    let mut marked = Vec::new();
    for m in &room.members {
        if (all || ids.contains(&m.open_id)) && !room.results.contains(&m.open_id) {
            marked.push(m.open_id.clone());
        }
    }
    room.results.extend(marked.iter().cloned());
    let done = room.all_done(offline_grace());
    drop(guard);
    if !marked.is_empty() {
        println!("[Matchmaking] room {}: did not come to the hall {:?}{}", battle_id, marked, if done { " — closing" } else { "" });
    }
    if done {
        close_room(battle_id);
    }
    (marked, done)
}

/// Room finished (everyone left / the server finished): we free the server and the players' statuses.
pub fn close_room(battle_id: &str) {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    if let Some(room) = st.rooms.remove(battle_id) {
        if st.closed_battles.len() > 10_000 {
            st.closed_battles.clear();
        }
        st.closed_battles.insert(battle_id.to_string());
        for m in &room.members {
            st.closed.insert(m.open_id.clone(), battle_id.to_string());
            if matches!(st.players.get(&m.open_id), Some(PlayerStatus::Matched { battle_id: b, .. }) if b == battle_id) {
                st.players.remove(&m.open_id);
            }
        }
        allocator::release(battle_id);
        println!("[Matchmaking] room {} closed", battle_id);
    }
}

// ---- SCBattleMatchingLogSync pushes per the reference (docs/REF_BATTLE_ROOM_FLOW.md): one event per push —
// #1 MATCHING startMatching (lobby_handlers::lobby_matching_search) → #2 CAN_JOIN_IN matched → #3 FINISHED finished
// (after startBattling of the match room).

/// Match event numbers (as in the reference)
pub const MATCH_EVENT_START: i64 = 1;
const MATCH_EVENT_MATCHED: i64 = 2;
const MATCH_EVENT_FINISHED: i64 = 3;

/// Search close reasons (EnMatchingExitReasonType)
pub const MATCH_EXIT_DESTROY: i32 = -5012;
pub const MATCH_EXIT_SELF_QUIT: i32 = -5014;
/// Reference (sniffer_log_001 lines 124 → 171): after FINISHED the server sends event 4 CANCEL (MATCHING_DESTROY). For us
/// at the end of a room the search is closed by `dirty_clear_battle_room` (MatchStop, no window); CANCEL — cancel by button and
/// heartbeat of an unknown search (M9). We do not send CANCEL on a timer after FINISHED: the client in the hall showed "search cancelled".

#[derive(Default)]
struct MatchLog {
    last_id: i64,
    cancelled: bool,
    /// the sent CANCEL: (event number, reason, user_id) — for a resend until the client acknowledges it
    cancel: Option<(i64, i32, u64)>,
    resent: u32,
    last_resend: Option<std::time::Instant>,
}

/// CANCEL search resend: at most once per CANCEL_RESEND_SECS and no more than CANCEL_RESEND_MAX times
const CANCEL_RESEND_SECS: u64 = 3;
const CANCEL_RESEND_MAX: u32 = 20;

fn match_log() -> &'static Mutex<HashMap<(String, String), MatchLog>> {
    static LOG: OnceLock<Mutex<HashMap<(String, String), MatchLog>>> = OnceLock::new();
    LOG.get_or_init(Default::default)
}

/// Search event `id` sent to the player (1 startMatching, 2 matched, 3 finished)
pub fn note_match_event(open_id: &str, matching_id: &str, id: i64) {
    let mut log = match_log().lock().unwrap();
    if log.len() > 20_000 {
        log.clear();
    }
    let e = log.entry((open_id.to_string(), matching_id.to_string())).or_default();
    e.last_id = e.last_id.max(id);
}

/// The search was closed without CANCEL (dirty_clear_battle_room / dirty_clear_matching — MatchStop on the client): no more CANCEL
/// for it. 04.11 10:48:54: a search heartbeat came a second after the room was cleared, CANCEL went out after
/// it — after that the client sent pre_search without search ("Start" did not work from the second exit from the hall).
pub fn mark_match_closed(open_id: &str, matching_id: &str) {
    if matching_id.is_empty() {
        return;
    }
    match_log().lock().unwrap().entry((open_id.to_string(), matching_id.to_string())).or_default().cancelled = true;
}

/// A search CANCEL event as in the reference: the number is the next after the last sent one (unknown — the backend
/// was restarted — 4, as after FINISHED), a snapshot with the member and CANCEL status. Once per (player, search).
pub fn match_cancel_msg(open_id: &str, user_id: u64, matching_id: &str, reason: i32) -> Option<tgf::ScBattleMatchingLogSync> {
    if matching_id.is_empty() {
        return None;
    }
    let id = {
        let mut log = match_log().lock().unwrap();
        let e = log.entry((open_id.to_string(), matching_id.to_string())).or_default();
        if e.cancelled {
            return None;
        }
        e.cancelled = true;
        let id = if e.last_id > 0 { e.last_id + 1 } else { MATCH_EVENT_FINISHED + 1 };
        e.cancel = Some((id, reason, user_id));
        id
    };
    Some(cancel_msg(user_id, matching_id, id, reason))
}

/// Resend of a sent CANCEL if the client did not apply it: in the search heartbeat `acknowlodge_event_id` is below the CANCEL
/// number. 05.11 (locally, leaving a raid): CANCEL arrived while the client was leaving the raid map — the client did not take it
/// (the heartbeat in the lobby also acknowledged 3), "Start" was dead. In the lobby/hall the client applies CANCEL (04.11, leaving the hall).
pub fn match_cancel_resend(open_id: &str, matching_id: &str, ack: i64) -> Option<tgf::ScBattleMatchingLogSync> {
    let (id, reason, user_id) = {
        let mut log = match_log().lock().unwrap();
        let e = log.get_mut(&(open_id.to_string(), matching_id.to_string()))?;
        let (id, reason, user_id) = e.cancel?;
        if ack >= id || e.resent >= CANCEL_RESEND_MAX {
            return None;
        }
        if e.last_resend.map_or(false, |t| t.elapsed() < std::time::Duration::from_secs(CANCEL_RESEND_SECS)) {
            return None;
        }
        e.resent += 1;
        e.last_resend = Some(std::time::Instant::now());
        (id, reason, user_id)
    };
    Some(cancel_msg(user_id, matching_id, id, reason))
}

fn cancel_msg(user_id: u64, matching_id: &str, id: i64, reason: i32) -> tgf::ScBattleMatchingLogSync {
    use tgf::EnMatchStatus as S;
    let now = now_ts();
    tgf::ScBattleMatchingLogSync {
        matching_id: matching_id.to_string(),
        event_logs: vec![match_event(id, S::Cancel, tgf::d_match_event_log::Event::Cancel(tgf::DMatchCancleData { timepoint: now, reason }))],
        snapshot: Some(tgf::DMatchSnapshotData {
            last_event_id: id,
            participators: vec![tgf::DMatchUserData { user_key: Some(key(user_id)), ..Default::default() }],
            match_status: S::Cancel as i32,
            ..Default::default()
        }),
        is_switch: false,
    }
}

fn key(user_id: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id, zone_id: 1 }
}

/// Search level (matching_level of the snapshot) and map id by level_id
fn level_and_map(level_id: i64) -> (tgf::DBattleLevelParameter, i32) {
    let lvl = tgf::DLevelSelect { level_type: 2001, level_id: level_id as i32, region: "eu".to_string() };
    crate::rpc::handlers::GatewaySession::level_map_params(&lvl)
}

/// Match snapshot: level, last event number, members, status
pub fn match_snapshot(level_id: i64, members: &[Member], status: tgf::EnMatchStatus, last_event_id: i64) -> tgf::DMatchSnapshotData {
    tgf::DMatchSnapshotData {
        matching_level: Some(level_and_map(level_id).0),
        last_event_id,
        participators: members.iter().map(|m| tgf::DMatchUserData { user_key: Some(key(m.user_id)), ..Default::default() }).collect(),
        is_matching: status == tgf::EnMatchStatus::Matching,
        match_status: status as i32,
        ..Default::default()
    }
}

pub fn match_event(event_id: i64, status: tgf::EnMatchStatus, event: tgf::d_match_event_log::Event) -> tgf::DMatchEventLog {
    tgf::DMatchEventLog {
        event_id,
        timepoint: Some(crate::google::protobuf::Timestamp { seconds: now_ts(), nanos: 0 }),
        match_status: status as i32,
        event: Some(event),
    }
}

/// #2: the room is assembled and a server assigned — CAN_JOIN_IN + matched {room_key, room_data, participators with ds_player_key}.
/// We do not fill the members' deploy_preview here (no DB access from the tick): full members come in
/// startLoading of the match room. TODO(verify): whether the client needs deploy_preview already in matched (the reference has it).
fn notify_matched(room: &BattleRoom, players: &HashMap<String, PlayerStatus>, only: Option<&str>) {
    use tgf::EnMatchStatus as S;
    let (_, map_id) = level_and_map(room.level_id);
    let room_key = tgf::DBattleRoomKey { battle_id: room.battle_id.clone() };
    let room_data = tgf::DBattleRoomData {
        battle_server_id: 1,
        create_timepoint: room.created_ts,
        expired_timepoint: room.created_ts + 2400,
        battle_id: room.battle_id.clone(),
        ds_address: room.ds_address.clone().unwrap_or_default(),
        map_data: Some(tgf::DBattleMapData { map_id }),
        battle_data: Some(tgf::d_battle_room_data::BattleData::CompetitionSeasonPvpve(tgf::DBattleDataCompetition {})),
        ..Default::default()
    };
    let participators: Vec<tgf::DBattleUserData> = room
        .members
        .iter()
        .map(|m| {
            let k = room.ds_key_of(m.user_id);
            tgf::DBattleUserData {
                user_key: Some(key(m.user_id)),
                user_parameter: Some(tgf::DBattleUserParameter {
                    team_id: room.team_no.get(&m.open_id).copied().unwrap_or_else(|| crate::lobby::team::battle_team_id(m.user_id)),
                    faction_id: room.team_no.get(&m.open_id).copied().unwrap_or(1) as i32,
                    ..Default::default()
                }),
                ds_player_key: k.map(|k| tgf::DBattlePlayerDsKey {
                    ds_user_key: k.ds_user_key,
                    ds_token: k.token,
                    ds_encryption_key: k.encryption_key,
                }),
                ..Default::default()
            }
        })
        .collect();
    for m in room.members.iter().filter(|m| only.map_or(true, |o| o == m.open_id)) {
        // players with a UDP tunnel have their own server address (auth::udp_tunnel)
        let mut room_data = room_data.clone();
        room_data.ds_address = crate::auth::udp_tunnel::client_ds(&m.open_id, &room_data.ds_address);
        let matching_id = match players.get(&m.open_id) {
            Some(PlayerStatus::Matched { matching_id, .. }) => matching_id.clone(),
            _ => continue,
        };
        let mut snapshot = match_snapshot(room.level_id, &room.members, S::CanJoinIn, MATCH_EVENT_MATCHED);
        snapshot.room_key = Some(room_key.clone());
        snapshot.room_data = Some(room_data.clone());
        let msg = tgf::ScBattleMatchingLogSync {
            matching_id,
            event_logs: vec![match_event(
                MATCH_EVENT_MATCHED,
                S::CanJoinIn,
                tgf::d_match_event_log::Event::Matched(tgf::DMatchBattleData {
                    room_key: Some(room_key.clone()),
                    room_data: Some(room_data.clone()),
                    participators: participators.clone(),
                    ..Default::default()
                }),
            )],
            snapshot: Some(snapshot),
            is_switch: false,
        };
        note_match_event(&m.open_id, &msg.matching_id, MATCH_EVENT_MATCHED);
        let online = hub::push_to(&m.open_id, hub::MATCHING_LOG_METHOD, hub::MATCHING_LOG_TYPE, &msg);
        println!("[Matchmaking] match {} -> {} CAN_JOIN_IN ds={:?} (online={})", room.battle_id, m.open_id, room.ds_address, online);
    }
}

/// #3: the match began (the match room gave startBattling) — FINISHED. Once per player; true — sent.
pub fn notify_finished(battle_id: &str, open_id: &str) -> bool {
    use tgf::EnMatchStatus as S;
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let matching_id = match st.players.get(open_id) {
        Some(PlayerStatus::Matched { matching_id, battle_id: b }) if b == battle_id => matching_id.clone(),
        _ => return false,
    };
    let Some(room) = st.rooms.get_mut(battle_id) else { return false };
    if room.finished_sent.iter().any(|o| o == open_id) {
        return false;
    }
    room.finished_sent.push(open_id.to_string());
    // the client travels to the server: countdown until entry (take_not_arrived). On returning to the match `arrived` stays: the server already knows
    // the player and does not request the data again, otherwise someone returning was kicked after DS_ARRIVE_SECS (playtest 06.11, 7 times)
    room.battling_at.insert(open_id.to_string(), Instant::now());
    let msg = tgf::ScBattleMatchingLogSync {
        matching_id,
        event_logs: vec![match_event(MATCH_EVENT_FINISHED, S::Finished, tgf::d_match_event_log::Event::Finished(now_ts()))],
        snapshot: Some(match_snapshot(room.level_id, &room.members, S::Finished, MATCH_EVENT_FINISHED)),
        is_switch: false,
    };
    drop(guard);
    note_match_event(open_id, &msg.matching_id, MATCH_EVENT_FINISHED);
    hub::push_to(open_id, hub::MATCHING_LOG_METHOD, hub::MATCHING_LOG_TYPE, &msg)
}

/// Open hall (env MATCH_OPEN_HALL=1, together with DS_WAITING_ROOM=1, decision 03.11): the first ticket immediately opens
/// a room on a free server, the next ones of the same pool (level, group size) go into its hall while it has seats
/// and there is time before the match (allocator::hall_open). No free server and no open hall — the ticket waits in the queue.
fn open_hall() -> bool {
    std::env::var("MATCH_OPEN_HALL").map(|v| v == "1").unwrap_or(false)
}

/// Entry key for someone added to an open hall (like ds::driver::fallback_key: a random token per player and room).
/// Without it the client connected without `?token=`, the server got a `token_required` refusal for the match data and put in a default
/// hero (03.11 16:45: everyone added was a Sword Master, only the one who opened the hall had a token).
fn add_join_key(room: &mut BattleRoom, m: &Member) {
    if room.ds_keys.iter().any(|k| k.user_id == m.user_id) {
        return;
    }
    room.ds_keys.push(DsKey {
        user_id: m.user_id,
        ds_user_key: m.open_id.clone(),
        token: format!("{:016x}", rand::random::<u64>()),
        encryption_key: String::new(),
    });
}

/// Members (status Searching) — into room `battle_id`, whose hall is accepting
fn add_members_locked(st: &mut State, battle_id: &str, members: &[Member]) {
    let Some(room) = st.rooms.get_mut(battle_id) else { return };
    for m in members {
        room.results.retain(|o| o != &m.open_id); // back in their own hall (went to search and came back)
        // the returning player gets the room events from the start (the client cleared it with dirty_clear_battle_room) and FINISHED
        // of the new search
        room.finished_sent.retain(|o| o != &m.open_id);
        crate::rpc::handlers::room_sent_reset(battle_id, &m.open_id);
        if !room.members.iter().any(|x| x.open_id == m.open_id) {
            room.members.push(m.clone());
        }
        room.assign_team_number(m);
        if room.stage == RoomStage::DsReady {
            add_join_key(room, m);
        }
        if let Some(PlayerStatus::Searching { matching_id }) = st.players.get(&m.open_id).cloned() {
            st.players.insert(m.open_id.clone(), PlayerStatus::Matched { matching_id, battle_id: battle_id.to_string() });
        }
    }
}

/// Room group limit: by map and group size, on top — the MATCH_ROOM_TEAMS cap (tests)
fn room_teams(level_id: i64, party: usize) -> usize {
    let cap = std::env::var("MATCH_ROOM_TEAMS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0);
    cap.map_or(max_teams(level_id, party), |c| c.min(max_teams(level_id, party)))
}

/// Seats in the room (players)
pub fn room_seats(level_id: i64, party: usize) -> usize {
    room_teams(level_id, party) * party.max(1)
}

/// Launcher lobby room: no members and no server yet, the tick will assign a free one, the driver will hand over the hall task.
/// Groups of one size `party` (`capacity` and the map options follow from it, host_task). Returns battle_id.
pub fn open_lobby_room(level_id: i64, party: usize) -> String {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    st.next_id += 1;
    let battle_id = format!("l{}_{}", level_id, st.next_id);
    let mut room = BattleRoom::new(battle_id.clone(), level_id, Vec::new(), None);
    room.party_size = party.max(1);
    room.lobby = true;
    room.staff_hall = super::maintenance::is_on();
    st.rooms.insert(battle_id.clone(), room);
    battle_id
}

/// Why the player was not taken into the lobby room
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinRefused {
    /// the server has not received the hall task yet
    NotReady,
    /// the hall does not accept: the match began, no room, too little time left on the timer
    Closed,
    Full,
    /// already left this hall: the client does not return to a room with the same battle_id
    Left,
}

fn join_state_locked(st: &State, battle_id: &str, joining: &[Member]) -> Result<(), JoinRefused> {
    let Some(room) = st.rooms.get(battle_id) else { return Err(JoinRefused::Closed) };
    if joining.iter().any(|m| room.left.contains(&m.open_id)) {
        return Err(JoinRefused::Left);
    }
    if room.ds_address.is_none() || !allocator::has_host(battle_id) {
        return Err(JoinRefused::NotReady);
    }
    if !allocator::hall_open(battle_id) {
        return Err(JoinRefused::Closed);
    }
    let new = joining.iter().filter(|m| !room.members.iter().any(|x| x.open_id == m.open_id)).count();
    if room.members.len() + new > room_seats(room.level_id, room.party_size) {
        return Err(JoinRefused::Full);
    }
    // mixed rooms: solo in a trio room — each group takes its own spawn point set
    if groups_of(room.members.iter().chain(joining.iter())) > spawn_sets(room.level_id, room.party_size) {
        return Err(JoinRefused::Full);
    }
    Ok(())
}

/// Whether the hall will accept `need` more players (sign-up to a launcher tab while the room is open)
pub fn room_join_state(battle_id: &str, need: usize) -> Result<(), JoinRefused> {
    let joining: Vec<Member> = (0..need).map(|i| Member { open_id: format!("#new{}", i), user_id: 0 }).collect();
    join_state_locked(&state().lock().unwrap(), battle_id, &joining)
}

/// The "Start" button with the launcher lobby: the group goes straight into the hall of room `battle_id`, bypassing the queue. Returns matching_id;
/// "match found" and the new roster for the mod go out on the next tick, after the caller's startMatching event.
pub fn join_room(members: Vec<Member>, battle_id: &str) -> Result<String, JoinRefused> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    join_state_locked(st, battle_id, &members)?;
    remove_members_locked(st, &members);
    let finished = abandon_locked(st, &members, offline_grace());
    st.next_id += 1;
    let matching_id = st.next_id.to_string();
    for m in &members {
        st.players.insert(m.open_id.clone(), PlayerStatus::Searching { matching_id: matching_id.clone() });
    }
    add_members_locked(st, battle_id, &members);
    st.lobby_joined.push((battle_id.to_string(), members));
    drop(guard);
    for b in finished.iter().filter(|b| *b != battle_id) {
        close_room(b);
    }
    Ok(matching_id)
}

/// Tickets of pool `key` (oldest first) — into open halls; returns (room, added members)
fn join_open_halls(st: &mut State, key: (i64, usize), room_teams: usize) -> Vec<(String, Vec<Member>)> {
    let (level_id, party) = key;
    let mut out = Vec::new();
    loop {
        let Some(heap) = st.pools.get_mut(&key) else { break };
        let Some(&Reverse((_, id))) = heap.peek() else { break };
        let Some(ticket) = st.tickets.get(&id) else {
            heap.pop(); // cancelled
            continue;
        };
        if ticket.level_id != level_id || ticket.queue != party {
            heap.pop();
            continue;
        }
        let need = ticket.members.len();
        let mix = mixed();
        let Some(battle_id) = st
            .rooms
            .values()
            .filter(|r| !r.lobby && r.level_id == level_id && room_accepts_for(r.party_size, party, mix) && r.stage >= RoomStage::DsInit)
            // seats — by room size (in its own room room_teams × party, as before)
            .filter(|r| {
                let seats = if r.party_size == party { room_teams * party } else { room_seats(level_id, r.party_size) };
                r.members.len() + need <= seats && allocator::hall_open(&r.battle_id)
            })
            // each group takes its own spawn point set (mixed, Hard skirmish, Challenger Mode)
            .filter(|r| groups_of(r.members.iter().chain(ticket.members.iter())) <= spawn_sets(level_id, r.party_size))
            .filter(|r| !ticket.members.iter().any(|m| r.left.contains(&m.open_id)))
            .min_by_key(|r| r.created)
            .map(|r| r.battle_id.clone())
        else {
            break;
        };
        heap.pop();
        let ticket = st.tickets.remove(&id).expect("ticket exists");
        add_members_locked(st, &battle_id, &ticket.members);
        let room = st.rooms.get(&battle_id).expect("room exists");
        println!("[Matchmaking] open hall {}: + ticket {} ({}), in room {}", battle_id, id,
            ticket.members.iter().map(|m| m.open_id.as_str()).collect::<Vec<_>>().join(","), room.members.len());
        out.push((battle_id, ticket.members));
    }
    out
}

/// One matchmaker pass: assemble rooms from the queues, assign servers, broadcast found matches.
pub fn tick() {
    let cap = std::env::var("MATCH_ROOM_TEAMS").ok().and_then(|v| v.parse::<usize>().ok()).filter(|n| *n > 0);
    let min_teams = env_num("MATCH_MIN_TEAMS", env_num("MATCH_MIN_PLAYERS", 1)).max(1) as usize;
    let wait = Duration::from_secs(env_num("MATCH_WAIT_SECS", 15));
    let ttl = Duration::from_secs(env_num("DS_ROOM_TTL", 45 * 60));

    let open = open_hall();
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let mut joined: Vec<(String, Vec<Member>)> = std::mem::take(&mut st.lobby_joined);

    // 1. Room assembly: in a pool (level, group size) take the oldest tickets — up to room_teams groups
    let keys: Vec<(i64, usize)> = st.pools.keys().copied().collect();
    for key in keys {
        let (level_id, party) = key;
        let room_teams = cap.map_or(max_teams(level_id, party), |c| c.min(max_teams(level_id, party)));
        // groups in a room — no more than the spawn point sets: a pool of three may also hold solo/duo (Hard skirmish,
        // Challenger Mode); players — no more than the seats (room_teams × party)
        let group_cap = if cap.is_some() { room_teams } else { spawn_sets(level_id, party).max(room_teams) };
        if open {
            // open hall: first into an already open hall of the same map and group size while there are seats
            joined.extend(join_open_halls(st, key, room_teams));
            // a new room — only if there is somewhere to open it, and right away (no waiting: filling happens in the hall)
            if !allocator::has_free(map_path(level_id)) {
                continue;
            }
        }
        let wait = if open { Duration::ZERO } else { wait };
        loop {
            let heap = st.pools.get_mut(&key).unwrap();
            let mut picked: Vec<u64> = Vec::new();
            let mut skipped: Vec<Reverse<(u128, u64)>> = Vec::new();
            let mut oldest: Option<Instant> = None;
            let mut players = 0;
            while let Some(Reverse((order, id))) = heap.pop() {
                let Some(ticket) = st.tickets.get(&id) else { continue }; // cancelled
                if ticket.level_id != level_id || ticket.queue != party {
                    continue;
                }
                // mixed: in a solo pool groups up to 3 — seats (players) no more than a solo room has
                if players + ticket.members.len() > room_teams * party {
                    skipped.push(Reverse((order, id)));
                    continue;
                }
                players += ticket.members.len();
                oldest.get_or_insert(ticket.enqueued);
                picked.push(id);
                if picked.len() == group_cap || players == room_teams * party {
                    break;
                }
            }
            let count = picked.len();
            let full = count == group_cap || players == room_teams * party;
            let ready = count >= min_teams && (full || oldest.map_or(false, |t| t.elapsed() >= wait));
            let heap = st.pools.get_mut(&key).unwrap();
            if !ready {
                // Put everything back into the heap in the previous order
                for id in &picked {
                    let t = &st.tickets[id];
                    let order = t.enqueued.duration_since(*EPOCH.get_or_init(Instant::now)).as_millis();
                    heap.push(Reverse((order, *id)));
                }
                for item in skipped {
                    heap.push(item);
                }
                break;
            }
            for item in skipped {
                heap.push(item);
            }
            st.next_id += 1;
            let battle_id = format!("b{}_{}", level_id, st.next_id);
            let mut members = Vec::new();
            for id in picked {
                if let Some(t) = st.tickets.remove(&id) {
                    for m in t.members {
                        if let Some(PlayerStatus::Searching { matching_id }) = st.players.get(&m.open_id).cloned() {
                            st.players.insert(
                                m.open_id.clone(),
                                PlayerStatus::Matched { matching_id, battle_id: battle_id.clone() },
                            );
                        }
                        members.push(m);
                    }
                }
            }
            println!("[Matchmaking] room {} assembled: {} groups of {} ({} players)", battle_id, count, party, members.len());
            let mut room = BattleRoom::new(battle_id.clone(), level_id, members, None);
            room.party_size = party; // group size — from the queue pool
            st.rooms.insert(battle_id, room);
            if open {
                break; // one room per free server; the remaining tickets go into its hall (next tick)
            }
        }
    }
    // those added to an open hall get "match found" (then room events, like everyone) and the server mod gets the new roster
    for (battle_id, added) in &joined {
        let Some(room) = st.rooms.get(battle_id) else { continue };
        for m in added {
            notify_matched(room, &st.players, Some(&m.open_id));
        }
        let list = added.iter().map(|m| serde_json::json!({
            "open_id": m.open_id,
            "team_id": crate::lobby::team::battle_team_id(m.user_id),
        })).collect();
        allocator::host_add_members(battle_id, list);
    }

    // 2. Servers for assembled rooms and broadcast; old and finished ones (results from everyone / game closed) are closed
    let grace = offline_grace();
    let mut expired = Vec::new();
    for room in st.rooms.values_mut() {
        if room.created.elapsed() >= ttl {
            expired.push(room.battle_id.clone());
            continue;
        }
        if room.stage == RoomStage::DsReady {
            for m in &room.members {
                if room.results.contains(&m.open_id) || hub::is_online(&m.open_id) {
                    room.offline_since.remove(&m.open_id);
                } else {
                    room.offline_since.entry(m.open_id.clone()).or_insert_with(Instant::now);
                }
            }
            if room.finished(grace) {
                println!("[Matchmaking] room {}: results {} of {}, the others out of the game longer than {} s",
                    room.battle_id, room.results.len(), room.members.len(), grace.as_secs());
                expired.push(room.battle_id.clone());
                continue;
            }
        }
        // the server crashed while loading the hall (nobody had connected to it yet) — the room goes to another free server; room
        // events 2–4 will go out with the new address
        if room.ds_address.is_some() {
            if let Some(addr) = allocator::failover(&room.battle_id, &room.map_path) {
                println!("[Matchmaking] room {}: server {:?} → {}", room.battle_id, room.ds_address, addr);
                room.ds_address = Some(addr);
            }
        }
        if room.ds_address.is_none() {
            room.ds_address = allocator::allocate(&room.battle_id, &room.map_path);
            if room.ds_address.is_some() {
                room.stage = RoomStage::DsInit;
                room.ds_init_ts = now_ts();
            }
        }
        if room.ds_address.is_some() && !room.notified {
            room.notified = true;
            notify_matched(room, &st.players, None);
        }
    }
    drop(guard);
    for battle_id in expired {
        close_room(&battle_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Solo players get different numbers, the number does not change on re-entering and leaving the room
    #[test]
    fn team_numbers_are_stable() {
        let _g = serial();
        let mut room = BattleRoom::for_test("tn", 40003015, &[("tn_a", 91_000_001), ("tn_b", 91_000_002)]);
        assert_eq!(room.team_no["tn_a"], 1);
        assert_eq!(room.team_no["tn_b"], 2);
        room.members.retain(|m| m.open_id != "tn_a");
        room.assign_team_number(&Member { open_id: "tn_c".into(), user_id: 91_000_003 });
        room.assign_team_number(&Member { open_id: "tn_a".into(), user_id: 91_000_001 });
        assert_eq!(room.team_no["tn_c"], 3);
        assert_eq!(room.team_no["tn_a"], 1);
    }

    /// Tests share the matchmaker state and the server pool (`tick` takes others' tickets, `allocate` — others' slot): one at a
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }
    #[test]
    fn match_cancel_resent_until_acknowledged() {
        let _serial = serial();
        for id in 1..=3 {
            note_match_event("mr_a", "r1", id);
        }
        // CANCEL was not sent — nothing to resend
        assert!(match_cancel_resend("mr_a", "r1", 3).is_none());
        assert!(match_cancel_msg("mr_a", 7, "r1", MATCH_EXIT_DESTROY).is_some());
        // the client acknowledges 3 (did not take CANCEL 4) — resend of the same event 4, then a CANCEL_RESEND_SECS pause
        let again = match_cancel_resend("mr_a", "r1", 3).expect("resend");
        assert_eq!((again.event_logs[0].event_id, again.snapshot.unwrap().last_event_id), (4, 4));
        assert!(match_cancel_resend("mr_a", "r1", 3).is_none(), "at most once per 3 s");
        // acknowledged 4 — no more sending
        match_log().lock().unwrap().get_mut(&("mr_a".to_string(), "r1".to_string())).unwrap().last_resend = None;
        assert!(match_cancel_resend("mr_a", "r1", 4).is_none());
        // closed by clearing (without CANCEL) — no resend
        note_match_event("mr_b", "r2", 3);
        mark_match_closed("mr_b", "r2");
        assert!(match_cancel_resend("mr_b", "r2", 3).is_none());
    }

    #[test]
    fn match_cancel_follows_last_event() {
        let _serial = serial();
        let id_of = |m: Option<tgf::ScBattleMatchingLogSync>| m.map(|m| (m.event_logs[0].event_id, m.snapshot.unwrap().match_status));
        let cancel = tgf::EnMatchStatus::Cancel as i32;
        // as in the reference: 1 startMatching, 2 matched, 3 finished → 4 CANCEL, once
        for (id, _) in [(1, ()), (2, ()), (3, ())] {
            note_match_event("mc_a", "m1", id);
        }
        assert_eq!(id_of(match_cancel_msg("mc_a", 1, "m1", MATCH_EXIT_DESTROY)), Some((4, cancel)));
        assert!(match_cancel_msg("mc_a", 1, "m1", MATCH_EXIT_DESTROY).is_none());
        // cancel during search — 2
        note_match_event("mc_b", "m2", 1);
        assert_eq!(id_of(match_cancel_msg("mc_b", 2, "m2", MATCH_EXIT_SELF_QUIT)), Some((2, cancel)));
        // search unknown (backend restarted) — 4
        assert_eq!(id_of(match_cancel_msg("mc_c", 3, "m3", MATCH_EXIT_DESTROY)), Some((4, cancel)));
        assert!(match_cancel_msg("mc_c", 3, "", MATCH_EXIT_DESTROY).is_none());
        // closed together with the room (dirty_clear_battle_room) — a late heartbeat does not get CANCEL
        note_match_event("mc_d", "m4", 3);
        mark_match_closed("mc_d", "m4");
        assert!(match_cancel_msg("mc_d", 4, "m4", MATCH_EXIT_DESTROY).is_none());
    }

    #[test]
    fn room_stages_and_init_requests() {
        let _serial = serial();
        allocator::register("127.0.0.9:7001", None);
        let members = vec![Member { open_id: "test_stage_a".into(), user_id: 9_000_001 }];
        let room = start_direct(members, 40003017, "t").expect("free server");
        assert_eq!(room.stage, RoomStage::DsInit);
        // until the server replies the key is open_id without a token
        let k = room.ds_key_of(9_000_001).unwrap();
        assert_eq!((k.ds_user_key.as_str(), k.token.as_str()), ("test_stage_a", ""));

        let first: Vec<_> = take_init_requests().into_iter().filter(|r| r.battle_id == room.battle_id).collect();
        assert_eq!(first.len(), 1, "init_player is requested once");
        assert!(take_init_requests().iter().all(|r| r.battle_id != room.battle_id));
        retry_init(&room.battle_id);
        assert!(take_init_requests().iter().any(|r| r.battle_id == room.battle_id), "after retry — again");

        let keys = vec![DsKey { user_id: 9_000_001, ds_user_key: "test_stage_a".into(), token: "T".into(), encryption_key: String::new() }];
        assert!(set_ds_ready(&room.battle_id, keys));
        let r = super::room(&room.battle_id).unwrap();
        assert_eq!(r.stage, RoomStage::DsReady);
        assert_eq!(r.ds_key_of(9_000_001).unwrap().token, "T");
        assert!(take_init_requests().iter().all(|x| x.battle_id != room.battle_id));
        close_room(&room.battle_id);
        assert!(super::room(&room.battle_id).is_none());
    }

    #[test]
    fn match_events_like_reference() {
        let _serial = serial();
        // reference statuses: MATCHING = 1, CAN_JOIN_IN = 8, FINISHED = 20; is_matching only with MATCHING
        let members = vec![Member { open_id: "test_fin_a".into(), user_id: 9_000_002 }];
        let s = match_snapshot(40003016, &members, tgf::EnMatchStatus::CanJoinIn, 2);
        assert_eq!((s.match_status, s.last_event_id, s.is_matching), (8, 2, false));
        assert_eq!(s.matching_level.as_ref().map(|l| l.level_id), Some(40003016));
        assert!(match_snapshot(40003016, &members, tgf::EnMatchStatus::Matching, 1).is_matching);

        allocator::register("127.0.0.9:7002", None);
        let room = start_direct(members, 40003016, "t").expect("free server");
        notify_finished(&room.battle_id, "test_fin_a"); // player offline: push not delivered (false), but marked
        assert!(super::room(&room.battle_id).unwrap().finished_sent.iter().any(|o| o == "test_fin_a"));
        assert!(!notify_finished(&room.battle_id, "test_fin_a"), "FINISHED — once");
        assert!(!notify_finished(&room.battle_id, "nobody"));
        close_room(&room.battle_id);
    }

    #[test]
    fn team_limits_by_map() {
        let _serial = serial();
        // dungeon: solo 18 (20 match sets, but 18 in the hall), duo 10, trio 6; castle: solo 14 (Hard 13), duo 8, trio 6
        assert_eq!((max_teams(40003015, 1), max_teams(40003015, 2), max_teams(40003015, 3)), (18, 10, 6));
        assert_eq!((max_teams(40003016, 1), max_teams(40003016, 2), max_teams(40003016, 3)), (14, 8, 6));
        assert_eq!(max_teams(40003020, 1), 13);
    }

    /// Solo, duo and trio — separate queues: a room holds only groups of one size (03.11)
    #[test]
    fn mixed_rooms_pool_and_limits() {
        // mixed: the room size is a cap (solo — only solo, duo — duo and solo, trio — all); without the flag — exact
        assert!(room_accepts_for(1, 1, true) && !room_accepts_for(1, 2, true) && !room_accepts_for(1, 3, true));
        assert!(room_accepts_for(2, 1, true) && room_accepts_for(2, 2, true) && !room_accepts_for(2, 3, true));
        assert!(room_accepts_for(3, 1, true) && room_accepts_for(3, 2, true) && room_accepts_for(3, 3, true));
        assert!(room_accepts_for(2, 2, false) && !room_accepts_for(2, 1, false) && !room_accepts_for(3, 2, false));
        // max_teams as before: solo 40003015 — 18 (16+4, hall 18), trio — 6 (20/3); trio has 16 point sets
        assert_eq!((max_teams(40003015, 1), max_teams(40003015, 3)), (18, 6));
        assert_eq!((spawn_sets(40003015, 3), spawn_sets(40003016, 1), spawn_sets(40003016, 2)), (16, 14, 8));
        // groups are counted by match team: two from one party — one group
        let m = |u: u64| Member { open_id: format!("g{}", u), user_id: u };
        assert_eq!(groups_of([m(9_200_001), m(9_200_002)].iter()), 2);
    }

    /// 08.11: Hard — skirmish mode (all groups into rooms of three), Challenger Mode — solo and duo against trio, otherwise strict
    #[test]
    fn skirmish_and_challenger_queue_to_trio() {
        assert_eq!((queue_size(40003017, 1, false), queue_size(40003017, 2, false)), (1, 2));
        assert_eq!((queue_size(40003017, 1, true), queue_size(40003017, 2, true)), (3, 3));
        assert_eq!((queue_size(40003019, 1, false), queue_size(40003020, 2, false)), (3, 3));
        assert!(skirmish(40003019) && skirmish(40003020) && !skirmish(40003018) && !skirmish(40003015));
    }

    #[test]
    fn queues_split_by_party_size() {
        let _serial = serial();
        std::env::set_var("MATCH_WAIT_SECS", "0"); // other tests do not call tick()
        let lvl = 49_999_001;
        enqueue(vec![Member { open_id: "q_solo".into(), user_id: 9_100_001 }], lvl, false);
        enqueue(vec![Member { open_id: "q_duo_a".into(), user_id: 9_100_002 }, Member { open_id: "q_duo_b".into(), user_id: 9_100_003 }], lvl, false);
        tick();
        let room_of = |o: &str| match status(o) {
            Some(PlayerStatus::Matched { battle_id, .. }) => Some(battle_id),
            _ => None,
        };
        let (solo, duo) = (room_of("q_solo").expect("solo in a room"), room_of("q_duo_a").expect("duo in a room"));
        assert_ne!(solo, duo, "solo and duo — different rooms");
        assert_eq!(room_of("q_duo_b").as_deref(), Some(duo.as_str()));
        assert_eq!(super::room(&duo).unwrap().members.len(), 2);
        assert_eq!((super::room(&solo).unwrap().party_size, super::room(&duo).unwrap().party_size), (1, 2));
        close_room(&solo);
        close_room(&duo);
    }

    /// Launcher lobby: an empty room gets a server, does not close by itself and accepts a group only into its own hall
    #[test]
    fn lobby_room_waits_empty_and_joins_directly() {
        let _serial = serial();
        allocator::register("127.0.0.9:7020", None);
        let lvl = 49_999_003;
        let battle = open_lobby_room(lvl, 2);
        let a = Member { open_id: "ll_a".into(), user_id: 9_300_001 };
        let b = Member { open_id: "ll_b".into(), user_id: 9_300_002 };
        assert_eq!(join_room(vec![a.clone(), b.clone()], &battle), Err(JoinRefused::NotReady), "no server yet");
        let room = super::room(&battle).expect("an empty room lives");
        assert_eq!((room.party_size, room.lobby, room.stage), (2, true, RoomStage::WaitingServer));
        // like the matchmaker tick (the tick itself cannot be used here: it takes the tickets of parallel tests)
        let addr = allocator::allocate(&battle, &room.map_path).expect("server assigned");
        {
            let mut g = state().lock().unwrap();
            let r = g.rooms.get_mut(&battle).unwrap();
            r.ds_address = Some(addr.clone());
            r.stage = RoomStage::DsInit;
        }
        let closes = || super::room(&battle).unwrap().finished(Duration::MAX);
        let port: u16 = addr.rsplit_once(':').unwrap().1.parse().unwrap();
        allocator::heartbeat("127.0.0.9", &addr, port, "ready", 1, "");
        let host = serde_json::json!({ "battle_id": battle, "waiting_timeout": 300, "members": [] });
        assert!(allocator::set_host(&addr, &battle, host));
        assert!(set_ds_ready(&battle, vec![]));
        // the hall is loading (06.11: phase hosting counted as a closed hall — the first tick closed the empty room)
        allocator::heartbeat("127.0.0.9", &addr, port, "hosting", 1, &battle);
        assert!(allocator::hall_open(&battle));
        assert!(!closes(), "hall loading: an empty room does not close");
        allocator::heartbeat("127.0.0.9", &addr, port, "waiting", 1, &battle);
        assert_eq!(allocator::room_phase(&battle).as_deref(), Some("waiting"));
        assert!(allocator::hall_join_secs(&battle).is_some_and(|s| s > 270 && s <= 275));
        assert!(!closes(), "hall empty, but the timer is running");

        assert!(join_room(vec![a.clone(), b.clone()], &battle).is_ok());
        assert!(matches!(status("ll_a"), Some(PlayerStatus::Matched { battle_id, .. }) if battle_id == battle));
        assert!(token_of("ll_b").is_some(), "entry key issued");
        // stands in the hall longer than DS_ARRIVE_SECS: not a "non-arrival", the countdown restarts after the hall
        let long_ago = Instant::now().checked_sub(Duration::from_secs(600)).unwrap();
        state().lock().unwrap().rooms.get_mut(&battle).unwrap().battling_at.insert("ll_a".into(), long_ago);
        assert!(!take_not_arrived().iter().any(|(r, _)| r == &battle));
        assert!(super::room(&battle).unwrap().battling_at["ll_a"].elapsed() < Duration::from_secs(5));
        state().lock().unwrap().rooms.get_mut(&battle).unwrap().battling_at.clear();
        assert!(state().lock().unwrap().lobby_joined.iter().any(|(r, m)| r == &battle && m.len() == 2), "next tick: \"match found\" and the roster to the mod");

        // whoever left does not get back in, the only one leaving does not close the hall
        leave_room("ll_a", &battle);
        leave_room("ll_b", &battle);
        assert!(super::room(&battle).is_some(), "the hall waits for those signed up until the end of the timer");
        assert_eq!(join_room(vec![a], &battle), Err(JoinRefused::Left));

        // nobody came by the end of the timer: the mod reports empty, the room closes
        let (_, done) = members_absent(&battle, &[], true);
        assert!(done);
        assert!(super::room(&battle).is_none());
        assert_eq!(room_join_state(&battle, 1), Err(JoinRefused::Closed));
    }

    /// Open hall: a ticket of the same pool goes into the hall of an already assigned room, the mod gets the new roster (`action: members`)
    #[test]
    fn open_hall_takes_new_tickets() {
        let _serial = serial();
        allocator::register("127.0.0.9:7010", None);
        let lvl = 49_999_002;
        let room = start_direct(vec![Member { open_id: "oh_a".into(), user_id: 9_200_001 }], lvl, "t").expect("free server");
        let addr = room.ds_address.clone().unwrap();
        let port: u16 = addr.rsplit_once(':').unwrap().1.parse().unwrap();
        allocator::heartbeat("127.0.0.9", &addr, port, "ready", 1, "");
        let host = serde_json::json!({ "battle_id": room.battle_id, "waiting_timeout": 120, "members": [{ "open_id": "oh_a", "team_id": 1 }] });
        assert!(allocator::set_host(&addr, &room.battle_id, host));
        assert_eq!(allocator::heartbeat("127.0.0.9", &addr, port, "ready", 1, "")["action"], "host");
        assert_eq!(allocator::heartbeat("127.0.0.9", &addr, port, "waiting", 1, &room.battle_id)["action"], "none");

        enqueue(vec![Member { open_id: "oh_b".into(), user_id: 9_200_002 }], lvl, false);
        let joined = {
            let mut g = state().lock().unwrap();
            join_open_halls(&mut g, (lvl, 1), 18)
        };
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].0, room.battle_id);
        assert!(matches!(status("oh_b"), Some(PlayerStatus::Matched { battle_id, .. }) if battle_id == room.battle_id));
        assert_eq!(super::room(&room.battle_id).unwrap().members.len(), 2);
        // entry token — for whoever opened the hall, for someone added before the server replied, and for someone added after
        assert!(set_ds_ready(&room.battle_id, vec![]));
        assert!(token_of("oh_a").is_some() && token_of("oh_b").is_some());
        enqueue(vec![Member { open_id: "oh_d".into(), user_id: 9_200_004 }], lvl, false);
        {
            let mut g = state().lock().unwrap();
            assert_eq!(join_open_halls(&mut g, (lvl, 1), 18).len(), 1);
        }
        assert!(token_of("oh_d").is_some(), "added after DsReady");
        assert!(allocator::host_add_members(&room.battle_id, vec![serde_json::json!({ "open_id": "oh_b", "team_id": 2 })]));
        let rsp = allocator::heartbeat("127.0.0.9", &addr, port, "waiting", 1, &room.battle_id);
        assert_eq!(rsp["action"], "members");
        assert_eq!(rsp["host"]["members"].as_array().unwrap().len(), 2);
        assert_eq!(allocator::heartbeat("127.0.0.9", &addr, port, "waiting", 1, &room.battle_id)["action"], "none", "once");

        // the hall is full (1 group) — the ticket waits in the queue
        enqueue(vec![Member { open_id: "oh_c".into(), user_id: 9_200_003 }], lvl, false);
        let none = {
            let mut g = state().lock().unwrap();
            join_open_halls(&mut g, (lvl, 1), 2)
        };
        assert!(none.is_empty());
        assert!(matches!(status("oh_c"), Some(PlayerStatus::Searching { .. })));
        cancel("oh_c", "test");
        // the match began — the hall no longer accepts
        allocator::heartbeat("127.0.0.9", &addr, port, "entering", 1, &room.battle_id);
        assert!(!allocator::hall_open(&room.battle_id));
        close_room(&room.battle_id);
    }

    /// Search cancel with a match found before the match: the player leaves the room; during the match — no
    #[test]
    fn leave_room_before_battle() {
        let _serial = serial();
        allocator::register("127.0.0.9:7030", None);
        let a = Member { open_id: "lv_a".into(), user_id: 9_400_001 };
        let b = Member { open_id: "lv_b".into(), user_id: 9_400_002 };
        let room = start_direct(vec![a, b], 49_999_004, "t").expect("server");
        let addr = room.ds_address.clone().unwrap();
        let port: u16 = addr.rsplit_once(':').unwrap().1.parse().unwrap();
        allocator::heartbeat("127.0.0.9", &addr, port, "ready", 1, "");
        assert!(allocator::set_host(&addr, &room.battle_id, serde_json::json!({ "waiting_timeout": 120, "members": [] })));
        assert_eq!(matched_before_battle("lv_a").as_deref(), Some(room.battle_id.as_str()), "hall — the match has not begun");
        leave_room("lv_a", &room.battle_id);
        assert!(status("lv_a").is_none());
        assert_eq!(super::room(&room.battle_id).unwrap().members.len(), 1);
        assert_eq!(closed_room_of("lv_a").as_deref(), Some(room.battle_id.as_str()));
        // the match began — a search cancel does not take them out of the room
        allocator::heartbeat("127.0.0.9", &addr, port, "battle", 1, &room.battle_id);
        assert_eq!(matched_before_battle("lv_b"), None);
        // the last one left — the room is closed
        leave_room("lv_b", &room.battle_id);
        assert!(super::room(&room.battle_id).is_none());
    }

    /// The player went to a new search and got into another room — the token and room are taken from the new one, not the old one
    #[test]
    fn token_from_current_room_not_old_one() {
        let _serial = serial();
        allocator::register("127.0.0.9:7020", None);
        allocator::register("127.0.0.9:7021", None);
        let me = Member { open_id: "tk_a".into(), user_id: 9_300_001 };
        let other = Member { open_id: "tk_b".into(), user_id: 9_300_002 };
        let old = start_direct(vec![me.clone(), other.clone()], 49_999_003, "t").expect("server");
        assert!(set_ds_ready(&old.battle_id, vec![]));
        let old_token = token_of("tk_a").unwrap();
        // new search: the old room waits for the second one, tk_a is in it with a result
        let new = start_direct(vec![me.clone()], 49_999_003, "t").expect("server");
        assert!(set_ds_ready(&new.battle_id, vec![]));
        assert!(super::room(&old.battle_id).is_some(), "the old room is alive (tk_b without a result)");
        let t = token_of("tk_a").unwrap();
        assert_ne!(t, old_token);
        assert_eq!(room_of_player("tk_a").as_deref(), Some(new.battle_id.as_str()));
        close_room(&old.battle_id);
        close_room(&new.battle_id);
    }

    #[test]
    fn room_closes_after_all_results() {
        let _serial = serial();
        allocator::register("127.0.0.9:7003", None);
        let members = vec![
            Member { open_id: "test_res_a".into(), user_id: 9_000_003 },
            Member { open_id: "test_res_b".into(), user_id: 9_000_004 },
        ];
        let room = start_direct(members, 40003017, "t").expect("free server");
        assert_eq!(member_result("test_res_a"), None, "a result from one of two — the room lives");
        assert_eq!(member_result("test_res_a"), None, "a repeat does not count as the second");
        assert_eq!(member_result("test_res_b").as_deref(), Some(room.battle_id.as_str()));
        assert!(super::room(&room.battle_id).is_none());
        assert!(status("test_res_a").is_none(), "match status removed");
        assert_eq!(closed_room_of("test_res_b").as_deref(), Some(room.battle_id.as_str()));
        assert!(is_closed(&room.battle_id));

        // there will be no result, the player is back in the lobby and started a search — the old room does not wait for them
        allocator::register("127.0.0.9:7004", None);
        let m = vec![Member { open_id: "test_res_c".into(), user_id: 9_000_005 }];
        let old = start_direct(m.clone(), 40003017, "t").expect("free server");
        let t = enqueue(m.clone(), 40003017, false);
        assert!(super::room(&old.battle_id).is_none(), "closed by the new search");
        cancel("test_res_c", "test");
        let _ = t;

        // waiting hall: one of two did not come — wait for the second; an empty hall — close
        allocator::register("127.0.0.9:7005", None);
        let two = vec![Member { open_id: "test_abs_a".into(), user_id: 9_000_006 }, Member { open_id: "test_abs_b".into(), user_id: 9_000_007 }];
        let r = start_direct(two, 40003017, "t").expect("free server");
        assert_eq!(missed_hall("test_abs_a"), None, "not marked yet");
        assert_eq!(members_absent(&r.battle_id, &["test_abs_a".into()], false), (vec!["test_abs_a".to_string()], false));
        assert!(members_absent(&r.battle_id, &["test_abs_a".into()], false).0.is_empty(), "a repeat does not count");
        // 12.11: marked "did not come" and never reached the server — no match data given; the second (not marked) travels
        assert_eq!(missed_hall("test_abs_a").as_deref(), Some(r.battle_id.as_str()));
        assert_eq!(missed_hall("test_abs_b"), None);
        assert_eq!(members_absent(&r.battle_id, &[], true), (vec!["test_abs_b".to_string()], true));
        assert!(is_closed(&r.battle_id));
        assert_eq!(member_result("test_res_a"), None, "the room is already gone");

        // R1-stuck: after startBattling one reached the server (battle_user_data), the second did not; on the deadline — only that one
        allocator::register("127.0.0.9:7007", None);
        let two = vec![Member { open_id: "test_arr_a".into(), user_id: 9_000_010 }, Member { open_id: "test_arr_b".into(), user_id: 9_000_011 }];
        let ar = start_direct(two, 40003017, "t").expect("free server");
        {
            let mut g = state().lock().unwrap();
            let room = g.rooms.get_mut(&ar.battle_id).unwrap();
            let past = Instant::now().checked_sub(Duration::from_secs(600)).unwrap();
            room.battling_at.insert("test_arr_a".into(), past);
            room.battling_at.insert("test_arr_b".into(), past);
        }
        mark_arrived("test_arr_a");
        let late = take_not_arrived();
        assert!(late.contains(&(ar.battle_id.clone(), "test_arr_b".to_string())));
        assert!(!late.iter().any(|(_, o)| o == "test_arr_a"), "the one who arrived is not in the list");
        assert!(!take_not_arrived().iter().any(|(b, _)| b == &ar.battle_id), "once");
        close_room(&ar.battle_id);

        // returned to a match they were already in: the server does not request the data again, must not kick. A newcomer — on the deadline, as before
        allocator::register("127.0.0.9:7008", None);
        let two = vec![Member { open_id: "test_back_a".into(), user_id: 9_000_012 }, Member { open_id: "test_back_b".into(), user_id: 9_000_013 }];
        let back = start_direct(two, 40003017, "t").expect("free server");
        mark_arrived("test_back_a");
        notify_finished(&back.battle_id, "test_back_a"); // false: in the test the player is offline, the countdown is started anyway
        notify_finished(&back.battle_id, "test_back_b");
        {
            let mut g = state().lock().unwrap();
            let room = g.rooms.get_mut(&back.battle_id).unwrap();
            let past = Instant::now().checked_sub(Duration::from_secs(600)).unwrap();
            room.battling_at.values_mut().for_each(|at| *at = past);
        }
        let late = take_not_arrived();
        assert!(!late.iter().any(|(_, o)| o == "test_back_a"), "the one who returned is not in the list");
        assert!(late.contains(&(back.battle_id.clone(), "test_back_b".to_string())));
        assert!(rooms_summary().0 >= 1);
        close_room(&back.battle_id);

        // "Return to match": the match is running and there is no result — rejoin gives their room; after a result — no
        allocator::register("127.0.0.9:7006", None);
        let m = vec![Member { open_id: "test_rj_a".into(), user_id: 9_000_008 }, Member { open_id: "test_rj_b".into(), user_id: 9_000_009 }];
        let rj = start_direct(m, 40003017, "t").expect("free server");
        assert_eq!(rejoin("test_rj_a"), None, "until the server is ready there is nowhere to return");
        set_ds_ready(&rj.battle_id, vec![]);
        assert_eq!(rejoin("test_rj_a").map(|r| r.1), Some(rj.battle_id.clone()));
        member_result("test_rj_a");
        assert_eq!(rejoin("test_rj_a"), None, "there is a result — no return to the match");
        assert!(rejoin("test_rj_b").is_some());
        close_room(&rj.battle_id);

        // someone who left the game (no gateway connection) longer than grace — not waited for
        let mut r = BattleRoom::new("x".into(), 40003017, vec![Member { open_id: "a".into(), user_id: 1 }, Member { open_id: "b".into(), user_id: 2 }], None);
        r.results.push("a".into());
        r.offline_since.insert("b".into(), Instant::now());
        assert!(!r.all_done(Duration::from_secs(300)));
        assert!(r.all_done(Duration::ZERO));
    }
}
