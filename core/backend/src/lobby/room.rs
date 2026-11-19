//! Private rooms (room_*): roster by slots, owner, password, map/mode, readiness, invitations.
//! State in memory (like lobby::team). Everything needed for the DRoomStorage snapshot is kept right here
//! (a member's deploy_role is cached on entry/readiness), so changes can be broadcast without the DB —
//! from the lobby tick and on player disconnect.
//!
//! Format (there are no room traffic dumps either in rpc_dump or in the sniffer — only proto + UE headers of PVPService):
//! - the client (`UPVPService`, C++) keeps `TeamMembers`/`WatchMembers`, `IsLeader(SlotId, IsTeamMember)`,
//!   `GetMemberCamp` (CampA/CampB/Watcher), `GetIndexInWatchMembers`. Hence the model: `team_member` — slots
//!   of fixed length (empty slot = DRoomMember without user_key), the first half is side A, the second is B;
//!   `watch_member` — a dense list of spectators. TODO(verify): what the client does with empty slots.
//! - every change is broadcast to all members in one ScPlayerDirtyChgSync: `dirty_private_room_snapshot`
//!   (DUserRoomData with the current room; without a room — `cur_room_data: None`) + `dirty_private_room_action`
//!   (DRoomActionDirty with an event, event_id grows within the room).
//! - an invitation reaches the addressee only as an `invite_notify_action` action (their room snapshot is left alone).
//! - room_* replies are empty, they have no ret_code: the session puts the error code into RpcHead.error_code (set_rpc_error).
//! - the client Lua confirms the slots: WBP_PVP_RoomWIndow iterates TeamMembers[i] with UserKey:GetIsValid(),
//!   two teams of 3 slots (WBP_PVP_Team: Leader/Member1/Member2).
//!
//! env settings: ROOM_TEAM_SLOTS (6 = 3x3), ROOM_WATCH_SLOTS (4), ROOM_MIN_PLAYERS (1), ROOM_DEFAULT_LEVEL (50000003).

use super::{hub, matchmaking};
use crate::tgf;
use crate::tgf::d_room_message_action::Action;
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

pub const ROOM_SVR_ID: u64 = 1;
const INVITE_TTL: i64 = 60;
const IGNORE_ALL_SECS: i64 = 600;
const NAME_MAX_CHARS: usize = 32;
const KEYWORD_MAX_CHARS: usize = 16;
const SUGGEST_PAGE: usize = 20;
const BATTLE_TIMEOUT_SECS: i64 = 3600;

// EnErrorCode codes (schema_dump.proto) — for logs only: room_* replies have no ret_code field
pub const ERR_USER_IS_IN_ROOM: i32 = -4601;
pub const ERR_USER_NOT_IN_ROOM: i32 = -4602;
pub const ERR_ROOM_NOT_EXIST: i32 = -4605;
pub const ERR_MEMBER_IN_WATCH: i32 = -4606;
pub const ERR_POS_PARAM_INVALID: i32 = -4607;
pub const ERR_POS_HAS_MEMBER: i32 = -4609;
pub const ERR_NOT_OWNER: i32 = -4612;
pub const ERR_NAME_TAKEN: i32 = -4613;
pub const ERR_MEMBER_MAX: i32 = -4616;
pub const ERR_KEYWORD_WRONG: i32 = -4617;
pub const ERR_MEMBER_NOT_FULL: i32 = -4618;
pub const ERR_MEMBER_NOT_READY: i32 = -4619;
pub const ERR_ALREADY_IN_BATTLE: i32 = -4620;
pub const ERR_MEMBER_NOT_ONLINE: i32 = -4622;
pub const ERR_CANNOT_CHANGE_OTHER_POS: i32 = -4625;
pub const ERR_SERVICE_NOT_AVAILABLE: i32 = -4628;
pub const ERR_KEYWORD_TOO_LONG: i32 = -4629;
pub const ERR_NAME_TOO_LONG: i32 = -4630;
pub const ERR_CANNOT_INVITE_SELF: i32 = -4633;
pub const ERR_INVITE_NOT_EXIST: i32 = -4634;
pub const ERR_USER_IN_MATCHING: i32 = -4636;
pub const ERR_USER_IN_BATTLE: i32 = -4637;

const STATUS_PREPARE: i32 = tgf::EnRoomStatus::Prepare as i32;
const STATUS_BATTLE: i32 = tgf::EnRoomStatus::Battle as i32;
const POS_TEAM: i32 = tgf::EnRoomPosType::Team as i32;
const POS_WATCH: i32 = tgf::EnRoomPosType::Watch as i32;
const REMOVE_ACTIVE: i32 = tgf::EnRoomRemoveMemberReason::Active as i32;
const REMOVE_KICKOUT: i32 = tgf::EnRoomRemoveMemberReason::Kickout as i32;

fn env_num(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

pub fn team_slots() -> usize {
    env_num("ROOM_TEAM_SLOTS", 6).max(1)
}

pub fn watch_slots() -> usize {
    env_num("ROOM_WATCH_SLOTS", 4)
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

fn key(user_id: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id, zone_id: 1 }
}

pub fn room_key(room_id: i64) -> tgf::DRoomKey {
    tgf::DRoomKey { room_id, room_svr_id: ROOM_SVR_ID }
}

/// Custom game (05.11): the room window choice -> (level_id, map_detail map_id of the arena).
/// Client: /Game/TGF/Tables/PVP/PVPMapConfigDataTable — MapId 1 "Sanctuary Ruins", 2 "Hall of the Departed", 3 "Ruined
/// Fortress"; PVPModeConfigDataTable — ModeId 1 "Team Deathmatch", 2 "First to 4 wins" (BO). map_detail arenas:
/// 30000 + 100·map + mode (30101 L_Common_PVP01_DeathMatch … 30302 L_Common_PVP03_MultiGameMode; maps in order
/// PVP01..03 — an assumption). Level: 50000003 (PVP deathmatch) for mode 1, 50000006 (BO) for mode 2.
/// Not an arena (old ids / no choice) — None, the old level_for_map path.
pub fn pvp_arena(battle_map: i32, battle_type: i32) -> Option<(i64, i32)> {
    if !(1..=3).contains(&battle_map) {
        return None;
    }
    let mode = if battle_type == 2 { 2 } else { 1 };
    let level = if mode == 2 { 50000006 } else { 50000003 };
    Some((level, 30000 + battle_map * 100 + mode))
}

/// Custom game fighter's team by seat: the first half of the seats is side 1, the second is side 2
/// (room window: Team_1 and Team_2 of 3 each). Own id per room and side, so as not to collide with a party/user_id.
pub fn side_team_id(room_id: i64, slot: usize) -> i64 {
    let side = if slot < team_slots().div_ceil(2) { 1 } else { 2 };
    900_000_000 + room_id * 10 + side
}

/// The room's fighters with teams by seat
pub fn fighter_teams(room: &Room) -> Vec<(u64, i64)> {
    room.team.iter().enumerate().filter_map(|(i, m)| m.as_ref().map(|m| (m.user_id, side_team_id(room.room_id, i)))).collect()
}

/// Room battle_map -> match level_id (old path, when the choice is not a PVP arena: map_select id / level_id).
pub fn level_for_map(battle_map: i32) -> i64 {
    match battle_map {
        40003015..=40003020 | 50000003..=50000007 => battle_map as i64,
        1 => 40003017,
        2 => 40003018,
        3 => 50000003,
        5 => 50000006,
        _ => std::env::var("ROOM_DEFAULT_LEVEL").ok().and_then(|v| v.parse().ok()).unwrap_or(50000003),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pos {
    Team(usize),
    Watch(usize),
}

impl Pos {
    /// Old integer position field (from_pos/target_pos): team slots 0..N, then spectators.
    /// TODO(verify): whether the client encodes a spectator position this way in the old field.
    fn legacy(self) -> i32 {
        match self {
            Pos::Team(i) => i as i32,
            Pos::Watch(i) => (team_slots() + i) as i32,
        }
    }

    fn proto(self) -> tgf::DRoomMemberPos {
        match self {
            Pos::Team(i) => tgf::DRoomMemberPos { pos_type: POS_TEAM, pos_idx: i as i32 },
            Pos::Watch(i) => tgf::DRoomMemberPos { pos_type: POS_WATCH, pos_idx: i as i32 },
        }
    }

    /// Position from the request: the new DRoomMemberPos field takes priority over the old integer one.
    pub fn from_req(new_pos: Option<&tgf::DRoomMemberPos>, legacy: i32) -> Option<Pos> {
        match new_pos {
            Some(p) if p.pos_type == POS_TEAM && p.pos_idx >= 0 => Some(Pos::Team(p.pos_idx as usize)),
            // 18.11 (item 10, stand): WATCH with pos_idx -1 — "to the spectators" (the room's kick button sends it); spectators are
            // a dense list and move_member puts a newcomer at the end anyway. Before, -1 fell through to the legacy field target = 0 → team seat 0
            Some(p) if p.pos_type == POS_WATCH => Some(Pos::Watch(p.pos_idx.max(0) as usize)),
            _ if legacy < 0 => None,
            _ if (legacy as usize) < team_slots() => Some(Pos::Team(legacy as usize)),
            _ => Some(Pos::Watch(legacy as usize - team_slots())),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RoomMember {
    pub user_id: u64,
    pub open_id: String,
    pub ready: bool,
    pub deploy_role: Option<tgf::DBattleDeployRole>,
}

impl RoomMember {
    fn proto(&self) -> tgf::DRoomMember {
        tgf::DRoomMember {
            user_key: Some(key(self.user_id)),
            has_ready: self.ready,
            deploy_role: self.deploy_role.clone(),
            user_gamesvr_id: 1,
            client_battle_version: String::new(),
            user_skin: Some(tgf::DBattleSkin { skin_ids: vec![] }),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Room {
    pub room_id: i64,
    pub creator: u64,
    pub owner: u64,
    pub name: String,
    pub keyword: String,
    pub team: Vec<Option<RoomMember>>,
    pub watch: Vec<RoomMember>,
    pub battle_map: i32,
    pub battle_type: i32,
    pub status: i32,
    pub create_time: i64,
    pub battle_id: Option<String>,
    pub battle_data: Option<tgf::DMatchBattleData>,
    pub battle_timeout_ts: i64,
    /// user_id of the current match's fighters (teams from seats — lobby::team::set_room_battle_team; removed in tick)
    pub battle_users: Vec<u64>,
    event_seq: i64,
}

impl Room {
    pub fn members(&self) -> impl Iterator<Item = &RoomMember> {
        self.team.iter().flatten().chain(self.watch.iter())
    }

    pub fn team_members(&self) -> impl Iterator<Item = &RoomMember> {
        self.team.iter().flatten()
    }

    fn find(&self, user_id: u64) -> Option<Pos> {
        if let Some(i) = self.team.iter().position(|m| m.as_ref().map(|m| m.user_id) == Some(user_id)) {
            return Some(Pos::Team(i));
        }
        self.watch.iter().position(|m| m.user_id == user_id).map(Pos::Watch)
    }

    fn get_mut(&mut self, pos: Pos) -> Option<&mut RoomMember> {
        match pos {
            Pos::Team(i) => self.team.get_mut(i).and_then(|m| m.as_mut()),
            Pos::Watch(i) => self.watch.get_mut(i),
        }
    }

    fn take(&mut self, pos: Pos) -> Option<RoomMember> {
        match pos {
            Pos::Team(i) => self.team.get_mut(i).and_then(|m| m.take()),
            Pos::Watch(i) if i < self.watch.len() => Some(self.watch.remove(i)),
            Pos::Watch(_) => None,
        }
    }

    fn is_empty(&self) -> bool {
        self.members().next().is_none()
    }

    /// A free seat for a new member: the first empty team slot, otherwise the end of the spectator list.
    fn free_pos(&self) -> Option<Pos> {
        if let Some(i) = self.team.iter().position(|m| m.is_none()) {
            return Some(Pos::Team(i));
        }
        (self.watch.len() < watch_slots()).then_some(Pos::Watch(self.watch.len()))
    }

    fn put(&mut self, pos: Pos, member: RoomMember) -> Pos {
        match pos {
            Pos::Team(i) => {
                self.team[i] = Some(member);
                pos
            }
            Pos::Watch(_) => {
                self.watch.push(member);
                Pos::Watch(self.watch.len() - 1)
            }
        }
    }

    /// Snapshot for the client. Only members see the password; outsiders get a mask (the room is "locked").
    /// TODO(verify): whether the client compares the password itself (then the mask would break joining from the list).
    pub fn storage(&self, reveal_keyword: bool) -> tgf::DRoomStorage {
        let keyword = if reveal_keyword || self.keyword.is_empty() { self.keyword.clone() } else { "******".to_string() };
        tgf::DRoomStorage {
            room_key: Some(room_key(self.room_id)),
            room_creater: Some(key(self.creator)),
            room_name: self.name.clone(),
            room_keyword: keyword,
            team_member: self
                .team
                .iter()
                .map(|m| m.as_ref().map(RoomMember::proto).unwrap_or_default())
                .collect(),
            watch_member: self.watch.iter().map(RoomMember::proto).collect(),
            battle_map: self.battle_map,
            battle_type: self.battle_type,
            status: self.status,
            room_owner: Some(key(self.owner)),
            create_time: self.create_time as i32,
            match_battle_data: self.battle_data.clone(),
            battle_timeout_tm: self.battle_timeout_ts,
        }
    }

    fn act(&mut self, action: Action) -> tgf::DRoomMessageAction {
        self.event_seq += 1;
        tgf::DRoomMessageAction {
            event_id: self.event_seq,
            timepoint: Some(crate::google::protobuf::Timestamp { seconds: now_ts(), nanos: 0 }),
            action: Some(action),
        }
    }

    fn reset_ready(&mut self) {
        for m in self.team.iter_mut().flatten() {
            m.ready = false;
        }
        for m in self.watch.iter_mut() {
            m.ready = false;
        }
    }
}

#[derive(Clone, Debug)]
pub struct Invite {
    pub room_id: i64,
    pub sender: u64,
    pub target: u64,
    pub expired_ts: i64,
}

#[derive(Default)]
struct State {
    next_id: i64,
    rooms: HashMap<i64, Room>,
    by_user: HashMap<u64, i64>,
    invites: Vec<Invite>,
    ignored_rooms: HashMap<u64, HashSet<i64>>,
    ignore_all_until: HashMap<u64, i64>,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

/// Pushes after a change: (open_id, ScPlayerDirtyChgSync). The session sends to itself via self.push, to the others via hub.
pub type Outbox = Vec<(String, tgf::ScPlayerDirtyChgSync)>;

fn dirty(sender: u64, snapshot: Option<Option<tgf::DRoomStorage>>, actions: &[tgf::DRoomMessageAction]) -> tgf::ScPlayerDirtyChgSync {
    tgf::ScPlayerDirtyChgSync {
        dirty_private_room_snapshot: snapshot.map(|cur| tgf::DUserRoomData { cur_room_data: cur }),
        dirty_private_room_action: actions
            .iter()
            .map(|a| tgf::DRoomActionDirty { sender_user_key: Some(key(sender)), action: Some(a.clone()) })
            .collect(),
        ..Default::default()
    }
}

/// To all members — the room snapshot + events; to those who left (`gone`) — an empty snapshot + the same events.
fn broadcast(room: &Room, sender: u64, actions: &[tgf::DRoomMessageAction], gone: &[String]) -> Outbox {
    let storage = room.storage(true);
    let mut out: Outbox = room
        .members()
        .map(|m| (m.open_id.clone(), dirty(sender, Some(Some(storage.clone())), actions)))
        .collect();
    for open_id in gone {
        out.push((open_id.clone(), dirty(sender, Some(None), actions)));
    }
    out
}

/// Broadcast without a session (tick, disconnect): everything via hub.
pub fn deliver_hub(out: Outbox) {
    for (open_id, msg) in out {
        let online = hub::push_to(&open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
        if !online {
            println!("[Room] {} offline, room snapshot not delivered", open_id);
        }
    }
}

pub fn room_of(user_id: u64) -> Option<Room> {
    let st = state().lock().unwrap();
    st.by_user.get(&user_id).and_then(|id| st.rooms.get(id)).cloned()
}

#[allow(dead_code)]
pub fn room(room_id: i64) -> Option<Room> {
    state().lock().unwrap().rooms.get(&room_id).cloned()
}

/// "My room" snapshot (for resynchronization after an error).
pub fn my_snapshot(user_id: u64, open_id: &str) -> Outbox {
    let cur = room_of(user_id).map(|r| r.storage(true));
    vec![(open_id.to_string(), dirty(user_id, Some(cur), &[]))]
}

fn check_not_busy(open_id: &str) -> Result<(), i32> {
    match matchmaking::status(open_id) {
        Some(matchmaking::PlayerStatus::Searching { .. }) => Err(ERR_USER_IN_MATCHING),
        Some(matchmaking::PlayerStatus::Matched { .. }) => Err(ERR_USER_IN_BATTLE),
        None => Ok(()),
    }
}

fn check_name(st: &State, name: &str, except_room: i64) -> Result<(), i32> {
    if name.chars().count() > NAME_MAX_CHARS {
        return Err(ERR_NAME_TOO_LONG);
    }
    let lower = name.to_lowercase();
    if !name.is_empty() && st.rooms.values().any(|r| r.room_id != except_room && r.name.to_lowercase() == lower) {
        return Err(ERR_NAME_TAKEN);
    }
    Ok(())
}

fn check_keyword(keyword: &str) -> Result<(), i32> {
    if keyword.chars().count() > KEYWORD_MAX_CHARS {
        return Err(ERR_KEYWORD_TOO_LONG);
    }
    Ok(())
}

/// The player's room where they are the owner (for owner-only operations).
fn owned_room<'a>(st: &'a mut State, owner: u64) -> Result<&'a mut Room, i32> {
    let room_id = *st.by_user.get(&owner).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if room.owner != owner {
        return Err(ERR_NOT_OWNER);
    }
    Ok(room)
}

pub fn create(user_id: u64, open_id: &str, deploy: Option<tgf::DBattleDeployRole>, name: &str, keyword: &str) -> Result<Outbox, i32> {
    check_not_busy(open_id)?;
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    if st.by_user.contains_key(&user_id) {
        return Err(ERR_USER_IS_IN_ROOM);
    }
    check_name(st, name, 0)?;
    check_keyword(keyword)?;
    st.next_id += 1;
    let room_id = st.next_id;
    let mut room = Room {
        room_id,
        creator: user_id,
        owner: user_id,
        name: if name.is_empty() { format!("Room {}", room_id) } else { name.to_string() },
        keyword: keyword.to_string(),
        team: vec![None; team_slots()],
        watch: Vec::new(),
        battle_map: 0,
        battle_type: 0,
        status: STATUS_PREPARE,
        create_time: now_ts(),
        battle_id: None,
        battle_data: None,
        battle_timeout_ts: 0,
        battle_users: Vec::new(),
        event_seq: 0,
    };
    room.team[0] = Some(RoomMember { user_id, open_id: open_id.to_string(), ready: false, deploy_role: deploy });
    let actions = vec![room.act(Action::CraeteAction(tgf::DRoomCreateAction {
        room_name: room.name.clone(),
        room_keyword: room.keyword.clone(),
        battle_version: String::new(),
    }))];
    let out = broadcast(&room, user_id, &actions, &[]);
    println!("[Room] {} created room {} \"{}\"", user_id, room_id, room.name);
    st.by_user.insert(user_id, room_id);
    st.rooms.insert(room_id, room);
    Ok(out)
}

/// Entering a room. `invited_by` — the sender of the invitation (entry by invitation without a password).
pub fn enter(
    user_id: u64,
    open_id: &str,
    deploy: Option<tgf::DBattleDeployRole>,
    room_id: i64,
    keyword: &str,
    is_invite: bool,
    invited_by: Option<u64>,
) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let now = now_ts();
    if let Some(cur) = st.by_user.get(&user_id).copied() {
        if cur == room_id {
            // Already inside — just resend the snapshot
            let room = st.rooms.get(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
            return Ok(broadcast(room, user_id, &[], &[]).into_iter().filter(|(o, _)| o == open_id).collect());
        }
        return Err(ERR_USER_IS_IN_ROOM);
    }
    drop(guard);
    check_not_busy(open_id)?;
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room = st.rooms.get(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if room.status == STATUS_BATTLE {
        return Err(ERR_ALREADY_IN_BATTLE);
    }
    // An invitation skips the password check
    st.invites.retain(|i| i.expired_ts > now);
    let invite_pos = st
        .invites
        .iter()
        .position(|i| i.room_id == room_id && i.target == user_id && invited_by.map_or(true, |s| s == i.sender));
    let invite = if is_invite { invite_pos.map(|p| st.invites.remove(p)) } else { None };
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if invite.is_none() && !room.keyword.is_empty() && room.keyword != keyword {
        return Err(if is_invite { ERR_INVITE_NOT_EXIST } else { ERR_KEYWORD_WRONG });
    }
    let pos = room.free_pos().ok_or(ERR_MEMBER_MAX)?;
    let pos = room.put(pos, RoomMember { user_id, open_id: open_id.to_string(), ready: false, deploy_role: deploy });
    let enter = room.act(Action::EnterAction(tgf::DRoomEnterMemberAction {
        user_key: Some(key(user_id)),
        key_word: keyword.to_string(),
        battle_version: String::new(),
        is_invite: invite.is_some(),
        send_invite_user: invite.as_ref().map(|i| key(i.sender)),
    }));
    let add = room.act(Action::AddMemberAction(tgf::DRoomAddMemberAction {
        user_key: Some(key(user_id)),
        pos: pos.legacy(),
        new_pos: Some(pos.proto()),
    }));
    let out = broadcast(room, user_id, &[enter, add], &[]);
    println!("[Room] {} entered room {} ({:?})", user_id, room_id, pos);
    st.by_user.insert(user_id, room_id);
    // Other invitations to this room are no longer needed
    st.invites.retain(|i| !(i.room_id == room_id && i.target == user_id));
    Ok(out)
}

/// Remove a member from the room (leave / kick / disconnect). An empty room is deleted,
/// an owner who leaves hands the room to the first member (team slots, then spectators).
fn remove_locked(st: &mut State, user_id: u64, reason: i32, sender: u64) -> Result<Outbox, i32> {
    let room_id = *st.by_user.get(&user_id).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    let pos = room.find(user_id).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let gone = room.take(pos).ok_or(ERR_USER_NOT_IN_ROOM)?;
    st.by_user.remove(&user_id);
    if room.status != STATUS_BATTLE {
        crate::lobby::team::clear_room_battle_team(user_id); // in a match the team is needed until the match ends (tick removes it)
    }
    let mut actions = vec![room.act(Action::RemoveMemberAction(tgf::DRoomRemovedMemberAction {
        user_key: Some(key(user_id)),
        remove_reason: reason,
    }))];
    if room.is_empty() {
        println!("[Room] room {} is empty and deleted", room_id);
        let out = broadcast(room, sender, &actions, &[gone.open_id]);
        st.rooms.remove(&room_id);
        st.invites.retain(|i| i.room_id != room_id);
        return Ok(out);
    }
    if room.owner == user_id {
        let new_owner = room.members().next().map(|m| m.user_id).unwrap_or_default();
        room.owner = new_owner;
        actions.push(room.act(Action::OwnerChangeAction(tgf::DRoomOwnerChangeAction {
            old_owner: Some(key(user_id)),
            new_owner: Some(key(new_owner)),
        })));
    }
    println!("[Room] {} left room {} (reason={})", user_id, room_id, reason);
    Ok(broadcast(room, sender, &actions, &[gone.open_id]))
}

pub fn leave(user_id: u64) -> Result<Outbox, i32> {
    let mut st = state().lock().unwrap();
    remove_locked(&mut st, user_id, REMOVE_ACTIVE, user_id)
}

pub fn kick(owner: u64, target: u64) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room = owned_room(st, owner)?;
    if target == owner || room.find(target).is_none() {
        return Err(ERR_USER_NOT_IN_ROOM);
    }
    remove_locked(st, target, REMOVE_KICKOUT, owner)
}

/// room_change_member_pos (18.11, owner's decision on item 10, checked on the stand): the room's "kick" button of this client
/// sends the member with new_target_pos {WATCH, pos_idx -1}, not room_kickoff_member. Moving ANOTHER member there = kick out
/// of the room (only the owner may — kick checks it). Everything else — a normal move (a member to the spectators himself, seats).
pub fn change_member_pos(actor: u64, target: u64, new_pos: Option<&tgf::DRoomMemberPos>, legacy: i32) -> Result<Outbox, i32> {
    if target != actor && new_pos.is_some_and(|p| p.pos_type == POS_WATCH && p.pos_idx < 0) {
        return kick(actor, target);
    }
    match Pos::from_req(new_pos, legacy) {
        Some(to) => move_member(actor, target, to),
        None => Err(ERR_POS_PARAM_INVALID),
    }
}

pub fn disband(owner: u64) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room = owned_room(st, owner)?;
    if room.status == STATUS_BATTLE {
        return Err(ERR_ALREADY_IN_BATTLE);
    }
    let room_id = room.room_id;
    let actions = vec![room.act(Action::DisbandAction(tgf::DRoomDisbandAction {}))];
    let gone: Vec<String> = room.members().map(|m| m.open_id.clone()).collect();
    let users: Vec<u64> = room.members().map(|m| m.user_id).collect();
    let mut room = st.rooms.remove(&room_id).unwrap();
    room.team.iter_mut().for_each(|m| *m = None);
    room.watch.clear();
    for u in users {
        st.by_user.remove(&u);
    }
    st.invites.retain(|i| i.room_id != room_id);
    println!("[Room] {} disbanded room {}", owner, room_id);
    Ok(broadcast(&room, owner, &actions, &gone))
}

/// Moving a member: oneself (only to a free seat) or the owner — anyone (an occupied seat — swap).
pub fn move_member(actor: u64, target: u64, to: Pos) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room_id = *st.by_user.get(&actor).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if actor != target && room.owner != actor {
        return Err(ERR_CANNOT_CHANGE_OTHER_POS);
    }
    if room.status == STATUS_BATTLE {
        return Err(ERR_ALREADY_IN_BATTLE);
    }
    let from = room.find(target).ok_or(ERR_USER_NOT_IN_ROOM)?;
    if from == to {
        return Ok(broadcast(room, actor, &[], &[]));
    }
    let occupant = match to {
        Pos::Team(i) if i >= room.team.len() => return Err(ERR_POS_PARAM_INVALID),
        Pos::Team(i) => room.team[i].as_ref().map(|m| m.user_id),
        Pos::Watch(_) => None, // spectators are a dense list, go to the end
    };
    if let Pos::Watch(_) = to {
        if matches!(from, Pos::Team(_)) && room.watch.len() >= watch_slots() {
            return Err(ERR_MEMBER_MAX);
        }
    }
    if occupant.is_some() && actor == target {
        return Err(ERR_POS_HAS_MEMBER);
    }
    let mut actions = Vec::new();
    let mut moved = room.take(from).ok_or(ERR_USER_NOT_IN_ROOM)?;
    moved.ready = false;
    let to_real = match (to, occupant) {
        (Pos::Team(i), Some(other_id)) => {
            // Swap: whoever occupied the slot goes to the moved member's seat
            let mut other = room.team[i].take().unwrap();
            other.ready = false;
            room.team[i] = Some(moved);
            let other_to = room.put(from, other);
            actions.push(room.act(Action::MemberPosAction(tgf::DRoomMemeberPosAction {
                user_key: Some(key(other_id)),
                from_pos: to.legacy(),
                to_pos: other_to.legacy(),
                new_from_pos: Some(to.proto()),
                new_to_pos: Some(other_to.proto()),
            })));
            to
        }
        _ => room.put(to, moved),
    };
    // Spectator positions shift after removal from the middle of the list — the snapshot reflects this
    actions.insert(
        0,
        room.act(Action::MemberPosAction(tgf::DRoomMemeberPosAction {
            user_key: Some(key(target)),
            from_pos: from.legacy(),
            to_pos: to_real.legacy(),
            new_from_pos: Some(from.proto()),
            new_to_pos: Some(to_real.proto()),
        })),
    );
    println!("[Room] {}: {} {:?} -> {:?}", room_id, target, from, to_real);
    Ok(broadcast(room, actor, &actions, &[]))
}

pub fn set_ready(user_id: u64, ready: bool, deploy: Option<tgf::DBattleDeployRole>, preview: Option<tgf::DBattleDeployPreview>) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room_id = *st.by_user.get(&user_id).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if room.status == STATUS_BATTLE {
        return Err(ERR_ALREADY_IN_BATTLE);
    }
    let pos = room.find(user_id).ok_or(ERR_USER_NOT_IN_ROOM)?;
    if matches!(pos, Pos::Watch(_)) {
        return Err(ERR_MEMBER_IN_WATCH);
    }
    let m = room.get_mut(pos).unwrap();
    m.ready = ready;
    if deploy.is_some() {
        m.deploy_role = deploy;
    }
    let actions = vec![room.act(Action::MemberReadyAction(tgf::DRoomMemeberReadyAction {
        user_key: Some(key(user_id)),
        is_ready: ready,
        deploy_role_preview: if ready { preview } else { None },
    }))];
    Ok(broadcast(room, user_id, &actions, &[]))
}

pub fn select_map(owner: u64, map_id: i32) -> Result<Outbox, i32> {
    let mut st = state().lock().unwrap();
    let room = owned_room(&mut st, owner)?;
    room.battle_map = map_id;
    let actions = vec![room.act(Action::MapAction(tgf::DRoomMapAction { room_map_id: map_id }))];
    Ok(broadcast(room, owner, &actions, &[]))
}

pub fn select_battle_type(owner: u64, battle_type: i32) -> Result<Outbox, i32> {
    let mut st = state().lock().unwrap();
    let room = owned_room(&mut st, owner)?;
    room.battle_type = battle_type;
    let actions = vec![room.act(Action::BattleTypeAction(tgf::DRoomBattleTypeAction { battle_type }))];
    Ok(broadcast(room, owner, &actions, &[]))
}

pub fn rename(owner: u64, name: &str) -> Result<Outbox, i32> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let room_id = owned_room(st, owner)?.room_id;
    check_name(st, name, room_id)?;
    let room = owned_room(st, owner)?;
    room.name = name.to_string();
    let actions = vec![room.act(Action::NameAction(tgf::DRoomNameAction { name: name.to_string(), is_from_idip: false }))];
    Ok(broadcast(room, owner, &actions, &[]))
}

pub fn change_keyword(owner: u64, keyword: &str) -> Result<Outbox, i32> {
    check_keyword(keyword)?;
    let mut st = state().lock().unwrap();
    let room = owned_room(&mut st, owner)?;
    room.keyword = keyword.to_string();
    let actions = vec![room.act(Action::KeywordAction(tgf::DRoomKeywordAction { wordkey: keyword.to_string() }))];
    Ok(broadcast(room, owner, &actions, &[]))
}

pub fn transfer_owner(owner: u64, new_owner: u64) -> Result<Outbox, i32> {
    let mut st = state().lock().unwrap();
    let room = owned_room(&mut st, owner)?;
    if room.find(new_owner).is_none() {
        return Err(ERR_USER_NOT_IN_ROOM);
    }
    room.owner = new_owner;
    let actions = vec![room.act(Action::OwnerChangeAction(tgf::DRoomOwnerChangeAction {
        old_owner: Some(key(owner)),
        new_owner: Some(key(new_owner)),
    }))];
    Ok(broadcast(room, owner, &actions, &[]))
}

/// Data for the match start collected by the session (it needs the DB): level, map, members.
pub struct StartInfo {
    pub level_id: i64,
    pub map_id: i32,
    pub participators: Vec<tgf::DBattleUserData>,
    /// custom game arena (map_detail map_id) — see pvp_arena
    pub arena: Option<i32>,
    /// fighters and their teams by seat (already set in lobby::team before participators are built)
    pub fighters: Vec<(u64, i64)>,
}

/// Start checks (owner, everyone in the team ready and online). Returns the match members (team slots).
pub fn check_start(owner: u64) -> Result<Room, i32> {
    let st = state().lock().unwrap();
    let room_id = *st.by_user.get(&owner).ok_or(ERR_USER_NOT_IN_ROOM)?;
    let room = st.rooms.get(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    if room.owner != owner {
        return Err(ERR_NOT_OWNER);
    }
    if room.status == STATUS_BATTLE {
        return Err(ERR_ALREADY_IN_BATTLE);
    }
    let fighters: Vec<&RoomMember> = room.team_members().collect();
    if fighters.len() < env_num("ROOM_MIN_PLAYERS", 1) {
        return Err(ERR_MEMBER_NOT_FULL);
    }
    if fighters.iter().any(|m| m.user_id != owner && !m.ready) {
        return Err(ERR_MEMBER_NOT_READY);
    }
    if fighters.iter().any(|m| !hub::is_online(&m.open_id)) {
        return Err(ERR_MEMBER_NOT_ONLINE);
    }
    Ok(room.clone())
}

/// Match start by the same path as matchmaking: a match room in lobby::matchmaking with a server from allocator;
/// the matchmaker tick broadcasts battle_matching_log_sync (status 4 + room_key), the client goes to battle_room_heartbeat
/// and gets DsInitFinish. Room members additionally get start_battle_action (DMatchBattleData).
/// TODO(verify): which of the two signals the room client actually uses to move into the match.
pub fn start_battle(owner: u64, info: StartInfo) -> Result<Outbox, i32> {
    let room = check_start(owner)?;
    // maintenance: a private match only if all fighters are our own people (gm/admin)
    if room.team_members().any(|m| super::maintenance::blocks(&m.open_id)) {
        for (u, _) in &info.fighters {
            crate::lobby::team::clear_room_battle_team(*u);
        }
        return Err(ERR_SERVICE_NOT_AVAILABLE);
    }
    let members: Vec<matchmaking::Member> = room
        .team_members()
        .map(|m| matchmaking::Member { open_id: m.open_id.clone(), user_id: m.user_id })
        .collect();
    let battle = match matchmaking::start_direct_on(members, info.level_id, "pr", info.arena) {
        Some(b) => b,
        None => {
            for (u, _) in &info.fighters {
                crate::lobby::team::clear_room_battle_team(*u);
            }
            return Err(ERR_SERVICE_NOT_AVAILABLE);
        }
    };

    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let Some(room) = st.rooms.get_mut(&room.room_id) else {
        matchmaking::close_room(&battle.battle_id);
        return Err(ERR_ROOM_NOT_EXIST);
    };
    let now = now_ts();
    let data = tgf::DMatchBattleData {
        room_key: Some(tgf::DBattleRoomKey { battle_id: battle.battle_id.clone() }),
        room_data: Some(tgf::DBattleRoomData {
            battle_server_id: 1,
            create_timepoint: battle.created_ts,
            expired_timepoint: battle.created_ts + BATTLE_TIMEOUT_SECS,
            battle_id: battle.battle_id.clone(),
            ds_address: battle.ds_address.clone().unwrap_or_default(),
            map_data: Some(tgf::DBattleMapData { map_id: info.arena.unwrap_or(info.map_id) }),
            battle_data: Some(tgf::d_battle_room_data::BattleData::PrivateBattle(tgf::DBattleDataPrivateRoom {
                private_room_id: room.room_id,
                private_roomsvr_id: ROOM_SVR_ID,
            })),
            ..Default::default()
        }),
        matched_user_keys: info.participators.iter().filter_map(|p| p.user_key.clone()).collect(),
        participators: info.participators,
        robots: vec![],
    };
    room.status = STATUS_BATTLE;
    room.battle_users = info.fighters.iter().map(|(u, _)| *u).collect();
    room.battle_id = Some(battle.battle_id.clone());
    room.battle_data = Some(data.clone());
    room.battle_timeout_ts = now + BATTLE_TIMEOUT_SECS;
    room.reset_ready();
    // create_battle_action = true (field 112) — the client opens the WBP_PVP_MatchWindow window "server search / start"
    // (C++ 0x145b3cf60, LOCAL analysis 05.11, tools/research/room/PVP_START_END.md); the window goes away when the match map loads
    let actions = vec![
        room.act(Action::StatusAction(tgf::DRoomStatusAction { status: STATUS_BATTLE })),
        room.act(Action::CreateBattleAction(true)),
        room.act(Action::StartBattleAction(data)),
    ];
    println!(
        "[Room] room {} -> match {} (level={}, ds={:?})",
        room.room_id, battle.battle_id, info.level_id, battle.ds_address
    );
    Ok(broadcast(room, owner, &actions, &[]))
}

/// Room list for the lobby: keys page by page (index — page number). TODO(verify): index — page or offset.
pub fn suggest(index: i32) -> Vec<tgf::DRoomKey> {
    let st = state().lock().unwrap();
    let mut rooms: Vec<&Room> = st.rooms.values().collect();
    rooms.sort_by_key(|r| (r.status, std::cmp::Reverse(r.create_time), r.room_id));
    // 05.11: the client sends a random number in index (17542, 12718, 31407 …) — it is not a page number; earlier skip(index*PAGE)
    // gave an empty list and refresh found nothing. We take the page modulo their count (rooms are few — always the first).
    let pages = rooms.len().div_ceil(SUGGEST_PAGE).max(1);
    let page = index.unsigned_abs() as usize % pages;
    rooms
        .into_iter()
        .skip(page * SUGGEST_PAGE)
        .take(SUGGEST_PAGE)
        .map(|r| room_key(r.room_id))
        .collect()
}

/// Rooms by keys and/or by name (search — substring search, otherwise exact match).
pub fn batch_get(viewer: u64, keys: &[tgf::DRoomKey], names: &[String], search: bool) -> Vec<tgf::DRoomStorage> {
    let st = state().lock().unwrap();
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    let mut push = |r: &Room| {
        if seen.insert(r.room_id) {
            out.push(r.storage(r.members().any(|m| m.user_id == viewer)));
        }
    };
    for k in keys {
        if let Some(r) = st.rooms.get(&k.room_id) {
            push(r);
        }
    }
    let names: Vec<String> = names.iter().map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
    if !names.is_empty() {
        let mut rooms: Vec<&Room> = st.rooms.values().collect();
        rooms.sort_by_key(|r| r.room_id);
        for r in rooms {
            let name = r.name.to_lowercase();
            let id = r.room_id.to_string();
            if names.iter().any(|n| if search { name.contains(n.as_str()) || id == *n } else { name == *n }) {
                push(r);
            }
        }
    }
    out
}

/// Invitation: the addressee gets only the invite_notify_action event (their current room is left alone).
/// For those ignoring (the room or everyone) the invitation is silently not delivered.
pub fn invite(sender: u64, target: u64, target_open_id: &str) -> Result<Outbox, i32> {
    if sender == target {
        return Err(ERR_CANNOT_INVITE_SELF);
    }
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let now = now_ts();
    let room_id = *st.by_user.get(&sender).ok_or(ERR_USER_NOT_IN_ROOM)?;
    if st.by_user.get(&target) == Some(&room_id) {
        return Err(ERR_USER_IS_IN_ROOM);
    }
    if !hub::is_online(target_open_id) {
        return Err(ERR_MEMBER_NOT_ONLINE);
    }
    let ignored = st.ignore_all_until.get(&target).map_or(false, |t| *t > now)
        || st.ignored_rooms.get(&target).map_or(false, |s| s.contains(&room_id));
    if ignored {
        println!("[Room] {} ignores invitations (room {})", target, room_id);
        return Ok(vec![]);
    }
    st.invites.retain(|i| i.expired_ts > now && !(i.room_id == room_id && i.target == target));
    st.invites.push(Invite { room_id, sender, target, expired_ts: now + INVITE_TTL });
    let room = st.rooms.get_mut(&room_id).ok_or(ERR_ROOM_NOT_EXIST)?;
    let action = room.act(Action::InviteNotifyAction(tgf::DRoomInviteNotifyAction {
        send_user: Some(key(sender)),
        room_key: Some(room_key(room_id)),
        room_name: room.name.clone(),
    }));
    println!("[Room] {} invites {} to room {}", sender, target, room_id);
    Ok(vec![(target_open_id.to_string(), dirty(sender, None, &[action]))])
}

pub fn reject_invite(user_id: u64, sender: u64, room_id: i64) -> Option<Invite> {
    let mut st = state().lock().unwrap();
    let pos = st.invites.iter().position(|i| i.target == user_id && i.room_id == room_id && (sender == 0 || i.sender == sender))?;
    Some(st.invites.remove(pos))
}

pub fn ignore_room(user_id: u64, room_id: i64) {
    let mut st = state().lock().unwrap();
    st.invites.retain(|i| !(i.target == user_id && i.room_id == room_id));
    st.ignored_rooms.entry(user_id).or_default().insert(room_id);
}

/// "Ignore all invitations" — for IGNORE_ALL_SECS. TODO(verify): whether this is indefinite in the original.
pub fn ignore_all(user_id: u64) {
    let mut st = state().lock().unwrap();
    st.invites.retain(|i| i.target != user_id);
    st.ignore_all_until.insert(user_id, now_ts() + IGNORE_ALL_SECS);
}

/// Lobby tick: rooms whose match has ended (the match room was closed by ds_end/timeout) go back to preparation.
pub fn tick() {
    let mut out = Outbox::new();
    {
        let mut guard = state().lock().unwrap();
        let st = &mut *guard;
        let now = now_ts();
        st.invites.retain(|i| i.expired_ts > now);
        for room in st.rooms.values_mut() {
            if room.status != STATUS_BATTLE {
                continue;
            }
            let finished = match &room.battle_id {
                Some(b) => matchmaking::room(b).is_none() || room.battle_timeout_ts <= now,
                None => true,
            };
            if !finished {
                continue;
            }
            println!("[Room] room {}: match {:?} finished", room.room_id, room.battle_id);
            // custom game match teams — remove from all fighters (including those who left the room during the match)
            for u in room.battle_users.drain(..) {
                crate::lobby::team::clear_room_battle_team(u);
            }
            room.status = STATUS_PREPARE;
            room.battle_id = None;
            room.battle_data = None;
            room.battle_timeout_ts = 0;
            room.reset_ready();
            let actions = vec![
                room.act(Action::FinishBattleAction(true)),
                room.act(Action::StatusAction(tgf::DRoomStatusAction { status: STATUS_PREPARE })),
            ];
            out.extend(broadcast(room, room.owner, &actions, &[]));
        }
    }
    deliver_hub(out);
}

/// A player disconnected: leaves the room (after reconnecting nobody will give the room snapshot back to them).
pub fn on_disconnect(open_id: &str) {
    let out = {
        let mut guard = state().lock().unwrap();
        let st = &mut *guard;
        let user = st
            .rooms
            .values()
            .flat_map(|r| r.members())
            .find(|m| m.open_id == open_id)
            .map(|m| m.user_id);
        match user {
            Some(u) => remove_locked(st, u, REMOVE_ACTIVE, u).unwrap_or_default(),
            None => return,
        }
    };
    deliver_hub(out.into_iter().filter(|(o, _)| o != open_id).collect());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 18.11 (owner's item 10, "kick does not work"): the owner kicks a member in the room lobby — the member gets an empty
    /// snapshot + RemoveMember(Kickout), the others — the room without them; the member can leave/enter again; not the owner — refused
    /// 18.11, stand (owner's item 10): the room's "kick" in this client sends room_change_member_pos with the member and
    /// new_target_pos = {WATCH, pos_idx -1}, not room_kickoff_member. Before: -1 fell through to the legacy field target = 0
    /// (team seat 0, the owner's seat) and the guest swapped with the owner. Owner's decision: this is a kick out of the room.
    /// A member's own move to the spectators stays a move; a non-owner cannot kick
    #[test]
    fn kick_button_watch_minus_one() {
        let watch = |idx: i32| tgf::DRoomMemberPos { pos_type: POS_WATCH, pos_idx: idx };
        assert_eq!(Pos::from_req(Some(&watch(-1)), 0), Some(Pos::Watch(0)), "WATCH -1 — the spectators, not team seat 0");
        assert_eq!(Pos::from_req(Some(&watch(2)), 0), Some(Pos::Watch(2)));
        let (owner, guest, third) = (9_200_001u64, 9_200_002u64, 9_200_003u64);
        create(owner, "w-owner", None, "watch-test-room", "").expect("create");
        let room_id = room_of(owner).unwrap().room_id;
        enter(guest, "w-guest", None, room_id, "", false, None).expect("guest enters");
        enter(third, "w-third", None, room_id, "", false, None).expect("third enters");
        let owner_pos = room_of(owner).unwrap().find(owner);
        // the third moves himself to the spectators — a move, not a kick
        change_member_pos(third, third, Some(&watch(-1)), 0).expect("to the spectators himself");
        assert!(matches!(room_of(owner).unwrap().find(third), Some(Pos::Watch(_))));
        // not the owner — cannot kick another
        assert_eq!(change_member_pos(guest, third, Some(&watch(-1)), 0).err(), Some(ERR_NOT_OWNER));
        // the owner's "kick" on the guest — out of the room, the guest gets RemoveMember(Kickout)
        let out = change_member_pos(owner, guest, Some(&watch(-1)), 0).expect("kick");
        assert!(room_of(guest).is_none(), "the guest is out of the room");
        let r = room_of(owner).unwrap();
        assert_eq!(r.find(owner), owner_pos, "the owner keeps the seat");
        let kicked = out.iter().find(|(id, _)| id == "w-guest").map(|(_, m)| m.clone()).expect("push to the guest");
        assert!(kicked.dirty_private_room_action.iter().any(|a| matches!(&a.action, Some(x) if matches!(&x.action, Some(Action::RemoveMemberAction(r)) if r.remove_reason == REMOVE_KICKOUT))));
        let _ = disband(owner);
    }

    #[test]
    fn owner_kicks_member_in_lobby() {
        let (owner, guest, third) = (9_100_001u64, 9_100_002u64, 9_100_003u64);
        create(owner, "k-owner", None, "kick-test-room", "").expect("create");
        let room_id = room_of(owner).unwrap().room_id;
        enter(guest, "k-guest", None, room_id, "", false, None).expect("guest enters");
        enter(third, "k-third", None, room_id, "", false, None).expect("third enters");
        assert_eq!(kick(guest, third).err(), Some(ERR_NOT_OWNER), "not the owner");
        assert_eq!(kick(owner, owner).err(), Some(ERR_USER_NOT_IN_ROOM), "not oneself");
        let out = kick(owner, guest).expect("kick");
        let to = |o: &str| out.iter().find(|(id, _)| id == o).map(|(_, m)| m.clone()).unwrap_or_else(|| panic!("no push to {}", o));
        let kicked = to("k-guest");
        assert_eq!(kicked.dirty_private_room_snapshot.as_ref().map(|s| s.cur_room_data.is_none()), Some(true), "kicked: empty room");
        let removed = |m: &tgf::ScPlayerDirtyChgSync| {
            m.dirty_private_room_action.iter().any(|a| matches!(&a.action, Some(x) if matches!(&x.action, Some(Action::RemoveMemberAction(r)) if r.remove_reason == REMOVE_KICKOUT && r.user_key.as_ref().map(|k| k.user_id) == Some(guest))))
        };
        assert!(removed(&kicked), "kicked: RemoveMember Kickout");
        for o in ["k-owner", "k-third"] {
            let m = to(o);
            assert!(removed(&m), "{}: RemoveMember Kickout", o);
            let snap = m.dirty_private_room_snapshot.and_then(|s| s.cur_room_data).expect("room snapshot");
            assert!(!format!("{:?}", snap).contains(&format!("user_id: {}", guest)), "{}: the kicked one is not in the snapshot", o);
        }
        assert!(room_of(guest).is_none(), "the kicked one is not in the room");
        assert!(room_of(third).is_some());
        enter(guest, "k-guest", None, room_id, "", false, None).expect("can enter again");
        let _ = disband(owner);
    }

    #[test]
    fn pvp_arena_from_room_choice() {
        // room window: map 1..3, mode 1 (deathmatch) / 2 (first to 4 wins)
        assert_eq!(pvp_arena(1, 1), Some((50000003, 30101)));
        assert_eq!(pvp_arena(3, 1), Some((50000003, 30301)));
        assert_eq!(pvp_arena(2, 2), Some((50000006, 30202)));
        assert_eq!(pvp_arena(2, 0), Some((50000003, 30201)), "mode not selected — deathmatch");
        assert_eq!(pvp_arena(0, 1), None);
        assert_eq!(pvp_arena(40003017, 1), None, "old ids — the previous path");
        assert_eq!(matchmaking::arena_path(30101), "/TgfMaps/Scenes/Common/Level/L_Common_PVP01_DeathMatch");
        assert_eq!(matchmaking::arena_path(30302), "/TgfMaps/Scenes/Common/Level/L_Common_PVP03_MultiGameMode");
        // seats 0–2 — side 1, 3–5 — side 2 (6 seats)
        assert_eq!(side_team_id(7, 0), 900_000_071);
        assert_eq!(side_team_id(7, 2), 900_000_071);
        assert_eq!(side_team_id(7, 3), 900_000_072);
        assert_eq!(side_team_id(7, 5), 900_000_072);
    }
}
