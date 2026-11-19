//! Friends (friend_*) and chat (chat_*). The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Format facts (rpc_dump/sniffer 30.10, UE dumps, client Lua from pakchunk0 — tools/research/social/lua_orig):
//! - friend_*/chat_* responses have no ret_code (the error code lives in the RPC header, not reachable from the domain file): on
//!   rejection we reply with an empty response and write to the log; the client sees the real state from pushes.
//! - The client (USocialService) keeps friends/applications by FUserKey; it takes the nickname/icon/level from the profile cache
//!   (GetUserInfoMapByKeyList → cache_batch_get), so DFriendInfo/DFriendInviteInfo carry only keys and time.
//! - Friendship changes — SCPlayerDirtyChgSync.dirty_friend_event (DFriendEvent): "inviter" — who invited me,
//!   "invitee" — whom I invited. friend_get_all.inviter_list — incoming applications.
//! - Chat: UChatService keeps one channel per EnChatChannelType (UI: World/Team/Private/SysNotify/PrivateRoom),
//!   messages — TMap<sequence, FChatMessage> (a repeated sequence does not duplicate). Delivery — push
//!   SCPlayerDirtyChgSync.dirty_chat_message / dirty_chat_snapshot. The client never called chat_get_all_channels in the dumps,
//!   but sends chat_update_panel on every login — so on open=true we push the player's channel snapshots.
//! - Constants: const.json (friend_max_number 30, friend_invite_expire 7d, friend_suggest_number 100,
//!   friend_total_inviter_limit 100); channel limits — chat_channel_type (max_log_count, words_limit 200, gc_expire).
//!
//! Storage: links — DB table `friend_link` (id "<smaller uid>_<larger uid>"), applications — `friend_invite`
//! (id "<from>_<to>"); PlayerModel is not touched (no race with the other side's save_me). Chat — in process memory only.

use super::common;
use super::GatewaySession;
use crate::lobby::{hub, team};
use crate::structures::PlayerModel;
use crate::tgf;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

const ZONE: u32 = 1;
const FRIEND_LINK: &str = "friend_link";
const FRIEND_INVITE: &str = "friend_invite";

fn key(user_id: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id, zone_id: ZONE }
}

fn now_ts() -> i64 {
    GatewaySession::now_ts()
}

/// "7d", "12w", "30s", "5m", "1h" or just seconds
fn duration_secs(s: &str) -> Option<i64> {
    let s = s.trim();
    let (num, mul) = match s.chars().last()? {
        's' => (&s[..s.len() - 1], 1),
        'm' => (&s[..s.len() - 1], 60),
        'h' => (&s[..s.len() - 1], 3600),
        'd' => (&s[..s.len() - 1], 86_400),
        'w' => (&s[..s.len() - 1], 604_800),
        _ => (s, 1),
    };
    num.trim().parse::<i64>().ok().map(|n| n * mul)
}

/// Active character (selected in the hall or the first one)
fn hall_role(p: &PlayerModel) -> Option<&crate::structures::RoleModel> {
    p.roles.iter().find(|r| r.role_guid == p.hall_role_guid).or_else(|| p.roles.first())
}

// ==================================================================== online participants
// The hub has no list of all sessions, so whoever called friend_*/chat_* (the client sends chat_update_panel and friend_get_all
// on every login) is remembered here; on read, those the hub no longer sees are cut off.

static SOCIAL_ONLINE: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn social_registry() -> &'static Mutex<HashMap<String, u64>> {
    SOCIAL_ONLINE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn social_touch(open_id: &str, user_id: u64) {
    social_registry().lock().unwrap().insert(open_id.to_string(), user_id);
}

/// (open_id, user_id) of all online players seen by social RPCs
fn social_online() -> Vec<(String, u64)> {
    let mut map = social_registry().lock().unwrap();
    map.retain(|open_id, _| hub::is_online(open_id));
    map.iter().map(|(o, u)| (o.clone(), *u)).collect()
}

/// The player's friends for "logged in / left" broadcasts: open_id → (user_id, friends' user_ids). Filled in friend_get_all
/// (the client sends it on every login) and, on friendship changes, by the next friend_get_all.
static PRESENCE: OnceLock<Mutex<HashMap<String, (u64, Vec<u64>)>>> = OnceLock::new();

fn presence() -> &'static Mutex<HashMap<String, (u64, Vec<u64>)>> {
    PRESENCE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A friend's online status on the client — from the profile cache (EN_ROT_PLAYER, login_data_cache), which it subscribes on login and
/// never requests again: there is no RPC on the friends tab (03.11). So on a player's login/logout their online friends get
/// SCCacheExpiredSync {EN_ROT_PLAYER, user_id}: the client re-requests cache_batch_get and sees the new status.
pub(crate) fn presence_changed(open_id: &str, online: bool) {
    let Some((user_id, friends)) = presence().lock().unwrap().get(open_id).cloned() else { return };
    let msg = tgf::ScCacheExpiredSync {
        keys: vec![tgf::DCacheKey { type_id: common::CACHE_PLAYER, zone_id: common::CACHE_ZONE, inst_id: user_id }],
    };
    let mut told = 0;
    for (friend_open_id, friend_id) in social_online() {
        if friends.contains(&friend_id) && hub::push_to(&friend_open_id, common::CACHE_EXPIRED_METHOD, common::CACHE_EXPIRED_TYPE, &msg) {
            told += 1;
        }
    }
    if told > 0 {
        println!("[Friend] {} {} — told online friends: {}", open_id, if online { "online" } else { "left" }, told);
    }
}

/// The player left (gateway: connection closed and not replaced by a new one) — friends get "left", the record is removed
pub(crate) fn presence_offline(open_id: &str) {
    if hub::is_online(open_id) {
        return;
    }
    presence_changed(open_id, false);
    presence().lock().unwrap().remove(open_id);
}

// ==================================================================== friends: data

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct FriendLink {
    a: u64,
    b: u64,
    created_time: i64,
    event_id: i64,
}

impl FriendLink {
    fn other(&self, me: u64) -> u64 {
        if self.a == me { self.b } else { self.a }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct FriendInvite {
    from_user_id: u64,
    to_user_id: u64,
    created_time: i64,
    expired_time: i64,
    event_id: i64,
}

/// The recipient's gift box (friend_gift:<user_id>): DFriendGift as prost bytes (DItem has a HashMap<i32, …>)
const FRIEND_GIFT: &str = "friend_gift";

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct GiftBox {
    next_id: i64,
    gifts: Vec<Vec<u8>>,
}

impl FriendInvite {
    fn expired(&self, now: i64) -> bool {
        self.expired_time > 0 && self.expired_time <= now
    }
}

fn link_id(a: u64, b: u64) -> String {
    format!("{}_{}", a.min(b), a.max(b))
}

fn invite_id(from: u64, to: u64) -> String {
    format!("{}_{}", from, to)
}

/// All friendship changes — under one lock: check + write of both sides without races
static FRIEND_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// event_id of friend events: grows monotonically and does not roll back after a restart (from the current time in ms)
static FRIEND_EVENT_SEQ: AtomicI64 = AtomicI64::new(0);

fn next_event_id() -> i64 {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    let mut prev = FRIEND_EVENT_SEQ.load(Ordering::Relaxed);
    loop {
        let next = (prev + 1).max(now_ms);
        match FRIEND_EVENT_SEQ.compare_exchange(prev, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(p) => prev = p,
        }
    }
}

fn friend_event(event: tgf::d_friend_event::Event) -> tgf::DFriendEvent {
    tgf::DFriendEvent { event_id: next_event_id(), create_timepoint: now_ts(), event: Some(event) }
}

fn invite_info(inv: &FriendInvite, removed: bool) -> tgf::DFriendInviteInfo {
    tgf::DFriendInviteInfo {
        from_user_id: inv.from_user_id,
        from_zone_id: ZONE,
        to_user_id: inv.to_user_id,
        to_zone_id: ZONE,
        created_time: inv.created_time,
        removed_time: if removed { now_ts() } else { 0 },
        expired_time: inv.expired_time,
        event_id: inv.event_id,
    }
}

fn friend_info(link: &FriendLink, other: u64, removed: bool) -> tgf::DFriendInfo {
    tgf::DFriendInfo {
        user_id: other,
        zone_id: ZONE,
        created_time: link.created_time,
        removed_time: if removed { now_ts() } else { 0 },
        expired_time: 0,
        event_id: link.event_id,
    }
}

// ==================================================================== chat: data

const CH_PUBLIC: u32 = tgf::EnChatChannelType::Public as u32;
const CH_WORLD: u32 = tgf::EnChatChannelType::World as u32;
const CH_TEAM: u32 = tgf::EnChatChannelType::Team as u32;
const CH_PRIVATE: u32 = tgf::EnChatChannelType::Private as u32;
const CH_SYS: u32 = tgf::EnChatChannelType::SysNotify as u32;
const CH_ROOM: u32 = tgf::EnChatChannelType::PrivateRoom as u32;

const WORLD_CHANNEL: &str = "world";
const SYS_CHANNEL: &str = "sys_notify";
/// TODO(verify): channel heartbeat intervals (not in the table) — does the client send chat_channel_heartbeat with this step?
const CHAT_HEARTBEAT_SECS: i32 = 10;
const CHAT_HEARTBEAT_RETRY_SECS: i32 = 5;
/// At most one message per second per player (EN_ERR_CHAT_MSG_SEND_FREQUENTLY); the client keeps SendInterval itself
const CHAT_MIN_INTERVAL_MS: i64 = 1000;

fn team_channel(team_id: i64) -> String {
    format!("team_{}", team_id)
}

/// Private room (lobby::room): members of both teams and spectators
fn room_channel(room_id: i64) -> String {
    format!("room_{}", room_id)
}

/// All members of the player's room: (open_id) — teams and spectators
fn room_members(room: &crate::lobby::room::Room) -> Vec<String> {
    room.team.iter().flatten().chain(room.watch.iter()).map(|m| m.open_id.clone()).collect()
}

fn private_channel(user_id: u64) -> String {
    format!("private_{}", user_id)
}

/// The player's personal system channel: replies to chat commands (`/help`, GM) are seen only by them. The client shows
/// system messages in the "General" tab (ChatChannelConfigDataTable: Public.CanShowChannelSet contains SYS_NOTIFY).
fn sys_channel_of(user_id: u64) -> String {
    format!("sys_{}", user_id)
}

/// Channel type by our channel_id
fn chat_type_of(channel_id: &str) -> Option<u32> {
    if channel_id == WORLD_CHANNEL {
        Some(CH_WORLD)
    } else if channel_id == SYS_CHANNEL || channel_id.starts_with("sys_") {
        Some(CH_SYS)
    } else if channel_id.starts_with("team_") {
        Some(CH_TEAM)
    } else if channel_id.starts_with("private_") {
        Some(CH_PRIVATE)
    } else if channel_id.starts_with("room_") {
        Some(CH_ROOM)
    } else {
        None
    }
}

fn chat_channel_key(channel_id: &str, channel_type: u32) -> tgf::DChatChannelIdKey {
    use tgf::d_chat_channel_id_key::CastType;
    let instance = channel_id.rsplit('_').next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
    let cast = match channel_type {
        CH_TEAM | CH_PRIVATE | CH_ROOM => CastType::UnicastInstanceId(instance),
        _ => CastType::BroadcastWorld(true),
    };
    tgf::DChatChannelIdKey {
        channel_id: channel_id.to_string(),
        channel_type,
        channel_zone_id: ZONE,
        cast_type: Some(cast),
    }
}

/// Whether the player may read the channel (private — only their own, team — only the current party)
fn chat_can_read(channel_id: &str, user_id: u64) -> bool {
    match chat_type_of(channel_id) {
        Some(CH_SYS) if channel_id != SYS_CHANNEL => channel_id == sys_channel_of(user_id),
        Some(CH_WORLD) | Some(CH_SYS) => true,
        Some(CH_PRIVATE) => channel_id == private_channel(user_id),
        Some(CH_TEAM) => team::team_of(user_id).map(|t| team_channel(t.team_id) == channel_id).unwrap_or(false),
        Some(CH_ROOM) => crate::lobby::room::room_of(user_id).map(|r| room_channel(r.room_id) == channel_id).unwrap_or(false),
        _ => false,
    }
}

/// The player's channels: world, system, own system (command replies), own private, current party
fn chat_channels_for(user_id: u64) -> Vec<String> {
    let mut v = vec![WORLD_CHANNEL.to_string(), SYS_CHANNEL.to_string(), sys_channel_of(user_id), private_channel(user_id)];
    if let Some(t) = team::team_of(user_id) {
        v.push(team_channel(t.team_id));
    }
    if let Some(r) = crate::lobby::room::room_of(user_id) {
        v.push(room_channel(r.room_id));
    }
    v
}

#[derive(Default)]
struct ChatChannel {
    last_sequence: i64,
    messages: VecDeque<tgf::DChatMessage>,
}

static CHAT: OnceLock<Mutex<HashMap<String, ChatChannel>>> = OnceLock::new();
static CHAT_LAST_SEND: OnceLock<Mutex<HashMap<u64, i64>>> = OnceLock::new();

fn chat_store() -> &'static Mutex<HashMap<String, ChatChannel>> {
    CHAT.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Puts a message into the channel, assigns a sequence; keeps the last max_log messages
const MODAL_METHOD: &str = "tgf.GameClientService.modal_message";
const MODAL_TYPE: &str = "tgf.SCModalMessageSync";

fn chat_append(channel_id: &str, mut msg: tgf::DChatMessage, max_log: usize) -> tgf::DChatMessage {
    let mut store = chat_store().lock().unwrap();
    let ch = store.entry(channel_id.to_string()).or_default();
    ch.last_sequence += 1;
    msg.sequence = ch.last_sequence;
    ch.messages.push_back(msg.clone());
    while ch.messages.len() > max_log.max(1) {
        ch.messages.pop_front();
    }
    msg
}

/// Non-expired channel messages with sequence > after
fn chat_messages_after(channel_id: &str, after: i64) -> Vec<tgf::DChatMessage> {
    let now = now_ts();
    let store = chat_store().lock().unwrap();
    store
        .get(channel_id)
        .map(|ch| {
            ch.messages
                .iter()
                .filter(|m| m.sequence > after)
                .filter(|m| m.expired_timepoint.as_ref().map(|t| t.seconds > now).unwrap_or(true))
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

fn chat_last_seq(channel_id: &str) -> i64 {
    chat_store().lock().unwrap().get(channel_id).map(|c| c.last_sequence).unwrap_or(0)
}

/// Anti-flood: true — sending is allowed (and the timestamp is updated)
fn chat_rate_ok(user_id: u64) -> bool {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or_default();
    let mut map = CHAT_LAST_SEND.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap();
    match map.get(&user_id) {
        Some(&last) if now_ms - last < CHAT_MIN_INTERVAL_MS => false,
        _ => {
            map.insert(user_id, now_ms);
            true
        }
    }
}

/// Sender in DChatMessage: the active character (guid, name, level, race/class/gender, creation number)
fn chat_fill_sender(p: &PlayerModel, msg: &mut tgf::DChatMessage) {
    if let Some(r) = hall_role(p) {
        let basic = r.data.basic.as_ref();
        msg.sender_user_role_guid = r.role_guid as i64;
        msg.sender_user_role_name =
            basic.map(|b| b.role_name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| r.role_name.clone());
        msg.sender_user_role_level = r
            .data
            .deploy_role
            .as_ref()
            .map(|d| d.role_level)
            .filter(|l| *l > 0)
            .unwrap_or(r.role_level.max(1) as i32);
        msg.sender_user_role_meta = basic.and_then(|b| b.meta.clone());
        msg.sender_user_role_create_idx = basic.map(|b| b.create_index).unwrap_or(0);
    }
    msg.sender_type = Some(tgf::d_chat_message::SenderType::SenderUserKey(key(p.user_id)));
}

fn chat_notify(channel_id: &str, messages: Vec<tgf::DChatMessage>) -> tgf::ScPlayerDirtyChgSync {
    tgf::ScPlayerDirtyChgSync {
        dirty_chat_message: vec![tgf::DChatMessageNotify { channel_id: channel_id.to_string(), messages }],
        ..Default::default()
    }
}

/// A system message into the player's system chat channel (kept in its log) as a dirty push: chat command replies and
/// server toasts (toast.rs)
pub(super) fn sys_notify_dirty(user_id: u64, text: &str) -> tgf::ScPlayerDirtyChgSync {
    let ch = sys_channel_of(user_id);
    let now = now_ts();
    let ts = |s: i64| Some(crate::google::protobuf::Timestamp { seconds: s, nanos: 0 });
    let msg = tgf::DChatMessage {
        create_timepoint: ts(now),
        expired_timepoint: ts(now + 86_400),
        channel_type: CH_SYS,
        detail: Some(tgf::DChatMessageDetail {
            command: Some(tgf::d_chat_message_detail::Command::SysNotify(tgf::DChatMessagemSysNotify {
                text: text.to_string(),
                // the client takes the text from CultureTextMap (WBP_Chat_MessageItem.SetSystemMessage → GetCultureText),
                // not from `text`: the same text for all EnMutlLanguageType languages (1 EN … 10 PL)
                texts: (1..=10).map(|l| tgf::DLanguageText { languages: l, content: text.to_string() }).collect(),
                send_time: ts(now),
                ..Default::default()
            })),
            ..Default::default()
        }),
        ..Default::default()
    };
    let m = chat_append(&ch, msg, 100);
    chat_notify(&ch, vec![m])
}

impl GatewaySession {
    // ================================================================ shared domain helpers

    /// Rejection code in the response's RpcHead.error_code (the friend_*/chat_* body has no ret_code)
    fn social_err(&self, code: tgf::EnErrorCode) {
        self.set_rpc_error(code as i32);
    }

    fn social_const(&self, key: &str) -> Option<String> {
        self.game_data.r#const.iter().find(|c| c.key == key).map(|c| c.value.clone())
    }

    fn social_const_i64(&self, key: &str, default: i64) -> i64 {
        self.social_const(key).and_then(|v| v.trim().parse().ok()).unwrap_or(default)
    }

    fn social_const_secs(&self, key: &str, default: i64) -> i64 {
        self.social_const(key).and_then(|v| duration_secs(&v)).unwrap_or(default)
    }

    /// dirty_friend_event push to a player: to self — after the response, to another — if online
    async fn friend_push_events(&self, me: &PlayerModel, user_id: u64, events: Vec<tgf::DFriendEvent>) {
        if events.is_empty() {
            return;
        }
        let msg = tgf::ScPlayerDirtyChgSync { dirty_friend_event: events, ..Default::default() };
        if user_id == me.user_id {
            self.push_dirty(msg);
            return;
        }
        match self.load_by_user_id(user_id).await {
            Some(p) => {
                let online = common::push_to(&p.open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
                println!("[Friend] events -> {} (user_id={}, online={})", p.open_id, user_id, online);
            }
            None => println!("[Friend] no player user_id={} for the push", user_id),
        }
    }

    async fn friend_links_of(&self, user_id: u64) -> Vec<FriendLink> {
        let res = self
            .db
            .db
            .query("SELECT * FROM friend_link WHERE a = $u OR b = $u")
            .bind(("u", user_id))
            .await;
        match res {
            Ok(mut r) => r.take::<Vec<FriendLink>>(0).unwrap_or_else(|e| {
                eprintln!("[Friend] reading friend_link: {}", e);
                vec![]
            }),
            Err(e) => {
                eprintln!("[Friend] querying friend_link: {}", e);
                vec![]
            }
        }
    }

    /// Incoming applications to the player (expired ones are deleted)
    async fn friend_invites_to(&self, user_id: u64) -> Vec<FriendInvite> {
        let res = self
            .db
            .db
            .query("SELECT * FROM friend_invite WHERE to_user_id = $u")
            .bind(("u", user_id))
            .await;
        let all = match res {
            Ok(mut r) => r.take::<Vec<FriendInvite>>(0).unwrap_or_else(|e| {
                eprintln!("[Friend] reading friend_invite: {}", e);
                vec![]
            }),
            Err(e) => {
                eprintln!("[Friend] querying friend_invite: {}", e);
                vec![]
            }
        };
        let now = now_ts();
        let mut live = Vec::new();
        for inv in all {
            if inv.expired(now) {
                self.friend_delete_invite(inv.from_user_id, inv.to_user_id).await;
            } else {
                live.push(inv);
            }
        }
        live
    }

    async fn friend_get_invite(&self, from: u64, to: u64) -> Option<FriendInvite> {
        let inv = self.db.get::<FriendInvite>(FRIEND_INVITE, &invite_id(from, to)).await?;
        if inv.expired(now_ts()) {
            self.friend_delete_invite(from, to).await;
            return None;
        }
        Some(inv)
    }

    async fn friend_delete_invite(&self, from: u64, to: u64) {
        let id = invite_id(from, to);
        if let Err(e) = self.db.db.delete::<Option<FriendInvite>>((FRIEND_INVITE, id.as_str())).await {
            eprintln!("[Friend] deleting {}:{}: {}", FRIEND_INVITE, id, e);
        }
    }

    async fn friend_get_link(&self, a: u64, b: u64) -> Option<FriendLink> {
        self.db.get::<FriendLink>(FRIEND_LINK, &link_id(a, b)).await
    }

    fn friend_max(&self) -> usize {
        self.social_const_i64("friend_max_number", 30).max(1) as usize
    }

    /// Friendship me ↔ other: removes applications in both directions, creates the link, sends events to both.
    /// false — hit the friend limit (EN_ERR_FRIEND_MAX_NUMBER_LIMIT[_OTHER]).
    async fn friend_make(&self, me: &PlayerModel, other: u64) -> bool {
        let max = self.friend_max();
        if self.friend_links_of(me.user_id).await.len() >= max {
            println!("[Friend] {}: friend limit {}", me.user_id, max);
            self.social_err(tgf::EnErrorCode::EnErrFriendMaxNumberLimit);
            return false;
        }
        if self.friend_links_of(other).await.len() >= max {
            println!("[Friend] {}: {} has the friend limit {}", me.user_id, other, max);
            self.social_err(tgf::EnErrorCode::EnErrFriendMaxNumberLimitOther);
            return false;
        }
        let mut mine = Vec::new();
        let mut theirs = Vec::new();
        use tgf::d_friend_event::Event;
        // other → me: for me this is the inviter, for them the invitee
        if let Some(inv) = self.friend_get_invite(other, me.user_id).await {
            self.friend_delete_invite(other, me.user_id).await;
            mine.push(friend_event(Event::RemoveInviter(invite_info(&inv, true))));
            theirs.push(friend_event(Event::RemoveInvitee(invite_info(&inv, true))));
        }
        if let Some(inv) = self.friend_get_invite(me.user_id, other).await {
            self.friend_delete_invite(me.user_id, other).await;
            mine.push(friend_event(Event::RemoveInvitee(invite_info(&inv, true))));
            theirs.push(friend_event(Event::RemoveInviter(invite_info(&inv, true))));
        }
        let link = FriendLink {
            a: me.user_id.min(other),
            b: me.user_id.max(other),
            created_time: now_ts(),
            event_id: next_event_id(),
        };
        self.db.save(FRIEND_LINK, &link_id(me.user_id, other), &link).await;
        mine.push(friend_event(Event::AddFriendData(friend_info(&link, other, false))));
        theirs.push(friend_event(Event::AddFriendData(friend_info(&link, me.user_id, false))));
        println!("[Friend] {} and {} are now friends", me.user_id, other);
        self.friend_push_events(me, me.user_id, mine).await;
        self.friend_push_events(me, other, theirs).await;
        true
    }

    /// Reject the application from → me (remove for both)
    async fn friend_reject_one(&self, me: &PlayerModel, from: u64) {
        let Some(inv) = self.friend_get_invite(from, me.user_id).await else {
            println!("[Friend] {}: no application from {}", me.user_id, from);
            self.social_err(tgf::EnErrorCode::EnErrFriendInviteNotFound);
            return;
        };
        self.friend_delete_invite(from, me.user_id).await;
        use tgf::d_friend_event::Event;
        self.friend_push_events(me, me.user_id, vec![friend_event(Event::RemoveInviter(invite_info(&inv, true)))]).await;
        self.friend_push_events(me, from, vec![friend_event(Event::RemoveInvitee(invite_info(&inv, true)))]).await;
        println!("[Friend] {} rejected the application of {}", me.user_id, from);
    }

    // ================================================================ friends: RPC

    pub(super) async fn friend_get_all_impl(&self, _req: tgf::CsFriendGetAllReq) -> Option<tgf::ScFriendGetAllRsp> {
        let me = self.load_me().await;
        let my_open_id = self.clean_open_id();
        social_touch(&my_open_id, me.user_id);
        let links = self.friend_links_of(me.user_id).await;
        let friend_ids: Vec<u64> = links.iter().map(|l| l.other(me.user_id)).collect();
        let first = presence().lock().unwrap().insert(my_open_id.clone(), (me.user_id, friend_ids)).is_none();
        if first {
            presence_changed(&my_open_id, true);
        }
        let friend_list = links.iter().map(|l| friend_info(l, l.other(me.user_id), false)).collect();
        let inviter_list = self.friend_invites_to(me.user_id).await.iter().map(|i| invite_info(i, false)).collect();
        // Steam friends are not integrated; TODO: together_play_list — recent party/match members
        Some(tgf::ScFriendGetAllRsp { friend_list, inviter_list, steam_friend_list: vec![], together_play_list: vec![] })
    }

    pub(super) async fn friend_invite_impl(&self, req: tgf::CsFriendInviteReq) -> Option<tgf::ScFriendInviteRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        let target = req.user_id;
        if target == 0 || target == me.user_id {
            println!("[Friend] {}: cannot invite {} (self/empty)", me.user_id, target);
            self.social_err(if target == 0 {
                tgf::EnErrorCode::EnErrFriendNotFound
            } else {
                tgf::EnErrorCode::EnErrFriendCanNotInviteSelf
            });
            return Some(Default::default());
        }
        if self.load_by_user_id(target).await.is_none() {
            println!("[Friend] {}: player {} not found", me.user_id, target);
            self.social_err(tgf::EnErrorCode::EnErrFriendNotFound);
            return Some(Default::default());
        }
        if self.friend_get_link(me.user_id, target).await.is_some() {
            println!("[Friend] {}: {} is already a friend", me.user_id, target);
            self.social_err(tgf::EnErrorCode::EnErrFriendAlreadyFriend);
            return Some(Default::default());
        }
        // A counter-application — friendship right away
        if self.friend_get_invite(target, me.user_id).await.is_some() {
            self.friend_make(&me, target).await;
            return Some(Default::default());
        }
        if self.friend_get_invite(me.user_id, target).await.is_some() {
            println!("[Friend] {}: application to {} already sent", me.user_id, target);
            self.social_err(tgf::EnErrorCode::EnErrFriendAlreadySendInvite);
            return Some(Default::default());
        }
        if self.friend_links_of(me.user_id).await.len() >= self.friend_max() {
            println!("[Friend] {}: friend limit", me.user_id);
            self.social_err(tgf::EnErrorCode::EnErrFriendMaxNumberLimit);
            return Some(Default::default());
        }
        let inviter_limit = self.social_const_i64("friend_total_inviter_limit", 100).max(1) as usize;
        if self.friend_invites_to(target).await.len() >= inviter_limit {
            println!("[Friend] {}: {} has too many incoming applications", me.user_id, target);
            self.social_err(tgf::EnErrorCode::EnErrFriendInviterTotalLimit);
            return Some(Default::default());
        }
        let now = now_ts();
        let inv = FriendInvite {
            from_user_id: me.user_id,
            to_user_id: target,
            created_time: now,
            expired_time: now + self.social_const_secs("friend_invite_expire", 7 * 86_400),
            event_id: next_event_id(),
        };
        self.db.save(FRIEND_INVITE, &invite_id(me.user_id, target), &inv).await;
        println!("[Friend] {} invites {}", me.user_id, target);
        use tgf::d_friend_event::Event;
        self.friend_push_events(&me, me.user_id, vec![friend_event(Event::AddInvitee(invite_info(&inv, false)))]).await;
        self.friend_push_events(&me, target, vec![friend_event(Event::AddInviter(invite_info(&inv, false)))]).await;
        Some(Default::default())
    }

    pub(super) async fn friend_accept_invite_impl(&self, req: tgf::CsFriendAcceptInviteReq) -> Option<tgf::ScFriendAcceptInviteRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        if self.friend_get_invite(req.user_id, me.user_id).await.is_none() {
            println!("[Friend] {}: no application from {}", me.user_id, req.user_id);
            self.social_err(tgf::EnErrorCode::EnErrFriendInviteNotFound);
            return Some(Default::default());
        }
        if self.load_by_user_id(req.user_id).await.is_none() {
            self.friend_reject_one(&me, req.user_id).await;
            self.social_err(tgf::EnErrorCode::EnErrFriendNotFound);
            return Some(Default::default());
        }
        self.friend_make(&me, req.user_id).await;
        Some(Default::default())
    }

    pub(super) async fn friend_reject_invite_impl(&self, req: tgf::CsFriendRejectInviteReq) -> Option<tgf::ScFriendRejectInviteRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        self.friend_reject_one(&me, req.user_id).await;
        Some(Default::default())
    }

    pub(super) async fn friend_remove_impl(&self, req: tgf::CsFriendRemoveReq) -> Option<tgf::ScFriendRemoveRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        let other = req.user_id;
        let Some(link) = self.friend_get_link(me.user_id, other).await else {
            println!("[Friend] {}: {} is not a friend", me.user_id, other);
            self.social_err(tgf::EnErrorCode::EnErrFriendNotFound);
            return Some(Default::default());
        };
        let id = link_id(me.user_id, other);
        if let Err(e) = self.db.db.delete::<Option<FriendLink>>((FRIEND_LINK, id.as_str())).await {
            eprintln!("[Friend] deleting {}:{}: {}", FRIEND_LINK, id, e);
        }
        use tgf::d_friend_event::Event;
        self.friend_push_events(&me, me.user_id, vec![friend_event(Event::RemoveFriendData(friend_info(&link, other, true)))]).await;
        self.friend_push_events(&me, other, vec![friend_event(Event::RemoveFriendData(friend_info(&link, me.user_id, true)))]).await;
        println!("[Friend] {} removed {} from friends", me.user_id, other);
        Some(Default::default())
    }

    /// A gift to a friend: a DFriendGift record in the recipient's box (friend_gift:<uid>). The client has no gift UI (Lua has only
    /// the RPC names), no gift tables either: gift_type_id — an item type ×1 if it exists in item_type, otherwise a gift without
    /// items. Limits from const.json: friend_daily_send_gift_limit, one gift per friend per day, expiry friend_gift_expire.
    /// TODO(verify): what the client sends in gift_type_id and where it shows gifts.
    pub(super) async fn friend_send_gift_impl(&self, req: tgf::CsFriendSendGiftReq) -> Option<tgf::ScFriendSendGiftRsp> {
        use tgf::EnErrorCode as E;
        let _guard = FRIEND_LOCK.lock().await;
        let mut me = self.load_me().await;
        let (to, now) = (req.user_id, now_ts());
        let day = now / 86400;
        if to == me.user_id {
            self.social_err(E::EnErrFriendCanNotInviteSelf);
            return Some(Default::default());
        }
        if !self.friend_links_of(me.user_id).await.iter().any(|l| l.other(me.user_id) == to) {
            println!("[Friend] {}: gift to a non-friend {}", me.user_id, to);
            self.social_err(E::EnErrFriendNotFound);
            return Some(Default::default());
        }
        let sent = &me.state["social_gift_sent"];
        let (sent_today, sent_to): (i64, Vec<u64>) = if sent["day"].as_i64() == Some(day) {
            (sent["count"].as_i64().unwrap_or(0), serde_json::from_value(sent["to"].clone()).unwrap_or_default())
        } else {
            (0, vec![])
        };
        if sent_to.contains(&to) {
            self.social_err(E::EnErrFriendGiftAlreadSend);
            return Some(Default::default());
        }
        if sent_today >= self.social_const_i64("friend_daily_send_gift_limit", 30) {
            self.social_err(E::EnErrFriendGiftDailySendLimit);
            return Some(Default::default());
        }

        let mut gift_box: GiftBox = self.db.get(FRIEND_GIFT, &to.to_string()).await.unwrap_or_default();
        gift_box.next_id += 1;
        let gift_item = if self.game_data.item_type.contains_key(&req.gift_type_id) {
            vec![tgf::DItem {
                basic: Some(tgf::DItemBasic { type_id: req.gift_type_id, count: 1, ..Default::default() }),
                ..Default::default()
            }]
        } else {
            vec![]
        };
        let gift = tgf::DFriendGift {
            gift_id: gift_box.next_id,
            from_user_id: me.user_id,
            from_zone_id: ZONE,
            gift_item,
            gift_type_id: req.gift_type_id,
            created_time: now,
            expired_time: now + self.social_const_secs("friend_gift_expire", 30 * 86400),
            removed_time: 0,
            event_id: 0,
        };
        gift_box.gifts.push(prost::Message::encode_to_vec(&gift));
        self.db.save(FRIEND_GIFT, &to.to_string(), &gift_box).await;

        let mut sent_to = sent_to;
        sent_to.push(to);
        me.state["social_gift_sent"] = serde_json::json!({ "day": day, "count": sent_today + 1, "to": sent_to });
        self.save_me(&me).await;
        println!("[Friend] {} -> {}: gift {} (gift_id={})", me.user_id, to, req.gift_type_id, gift.gift_id);
        Some(Default::default())
    }

    /// Collect gifts: empty gift_ids — all non-expired. Items — to the stash (inventory_handlers::inv_give),
    /// daily limit friend_daily_receive_gift_limit. No room — what was collected before that stays, the rest stays in the box.
    pub(super) async fn friend_receive_gift_impl(&self, req: tgf::CsFriendReceiveGiftReq) -> Option<tgf::ScFriendReceiveGiftRsp> {
        use tgf::EnErrorCode as E;
        let _guard = FRIEND_LOCK.lock().await;
        let mut me = self.load_me().await;
        let now = now_ts();
        let day = now / 86400;
        let mut gift_box: GiftBox = self.db.get(FRIEND_GIFT, &me.user_id.to_string()).await.unwrap_or_default();
        let mut gifts: Vec<tgf::DFriendGift> = gift_box
            .gifts
            .iter()
            .filter_map(|b| <tgf::DFriendGift as prost::Message>::decode(b.as_slice()).ok())
            .filter(|g| g.expired_time == 0 || g.expired_time > now)
            .collect();
        let wanted: Vec<i64> = if req.gift_ids.is_empty() { gifts.iter().map(|g| g.gift_id).collect() } else { req.gift_ids.clone() };
        if wanted.iter().any(|id| !gifts.iter().any(|g| g.gift_id == *id)) {
            self.social_err(E::EnErrFriendGiftNotFound);
            return Some(Default::default());
        }
        let recv = &me.state["social_gift_received"];
        let mut received_today = if recv["day"].as_i64() == Some(day) { recv["count"].as_i64().unwrap_or(0) } else { 0 };
        let limit = self.social_const_i64("friend_daily_receive_gift_limit", 30);

        let mut ch = super::inventory_handlers::ItemChanges::default();
        let stash = tgf::DItemPosition { r#type: 1, x: -1, y: -1, ..Default::default() };
        let mut receive_gifts = Vec::new();
        for id in wanted {
            if received_today >= limit {
                self.social_err(E::EnErrFriendGiftDailyReceiveLimit);
                break;
            }
            let Some(i) = gifts.iter().position(|g| g.gift_id == id) else { continue };
            let mut trial = me.clone();
            let mut trial_ch = super::inventory_handlers::ItemChanges::default();
            let ok = gifts[i].gift_item.iter().all(|it| {
                let b = it.basic.clone().unwrap_or_default();
                self.inv_give(&mut trial, b.type_id, b.count.max(1), &stash, &mut trial_ch).is_ok()
            });
            if !ok {
                self.social_err(E::EnErrItemSuitablePositionNotFound);
                break;
            }
            me = trial;
            ch.dirty.extend(trial_ch.dirty);
            ch.coins.extend(trial_ch.coins);
            let mut g = gifts.remove(i);
            g.gift_item = trial_ch.added;
            receive_gifts.push(g);
            received_today += 1;
        }
        if !receive_gifts.is_empty() {
            me.state["social_gift_received"] = serde_json::json!({ "day": day, "count": received_today });
            gift_box.gifts = gifts.iter().map(prost::Message::encode_to_vec).collect();
            self.db.save(FRIEND_GIFT, &me.user_id.to_string(), &gift_box).await;
            self.save_me(&me).await;
            self.push_dirty(ch.to_dirty(&me));
        }
        println!("[Friend] {}: gifts received {}", me.user_id, receive_gifts.len());
        Some(tgf::ScFriendReceiveGiftRsp { receive_gifts })
    }

    /// Suggestions: online players (except self and friends) with a character; force_type — the character's faction
    pub(super) async fn friend_get_suggest_impl(&self, req: tgf::CsFriendGetSuggestReq) -> Option<tgf::ScFriendGetSuggestRsp> {
        let me = self.load_me().await;
        social_touch(&self.clean_open_id(), me.user_id);
        let friends: HashSet<u64> = self.friend_links_of(me.user_id).await.iter().map(|l| l.other(me.user_id)).collect();
        let limit = self.social_const_i64("friend_suggest_number", 100).max(1) as usize;
        let mut suggest_users = Vec::new();
        for (_, uid) in social_online() {
            if suggest_users.len() >= limit {
                break;
            }
            if uid == me.user_id || friends.contains(&uid) {
                continue;
            }
            let Some(p) = self.load_by_user_id(uid).await else { continue };
            let Some(role) = hall_role(&p) else { continue };
            let force = role.data.basic.as_ref().and_then(|b| b.meta.as_ref()).map(|m| m.force).unwrap_or(0);
            if req.force_type != 0 && force != 0 && force != req.force_type {
                continue;
            }
            suggest_users.push(tgf::DTeamSuggestData { user_key: Some(key(uid)), role_guid: role.role_guid as i64 });
        }
        Some(tgf::ScFriendGetSuggestRsp { suggest_users })
    }

    pub(super) async fn friend_accept_all_invite_impl(&self, _req: tgf::CsFriendAcceptAllInviteReq) -> Option<tgf::ScFriendAcceptAllInviteRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        for inv in self.friend_invites_to(me.user_id).await {
            if self.load_by_user_id(inv.from_user_id).await.is_none() {
                self.friend_reject_one(&me, inv.from_user_id).await;
            } else if !self.friend_make(&me, inv.from_user_id).await {
                break; // friend limit — the remaining applications stay
            }
        }
        Some(Default::default())
    }

    pub(super) async fn friend_reject_all_invite_impl(&self, _req: tgf::CsFriendRejectAllInviteReq) -> Option<tgf::ScFriendRejectAllInviteRsp> {
        let _guard = FRIEND_LOCK.lock().await;
        let me = self.load_me().await;
        for inv in self.friend_invites_to(me.user_id).await {
            self.friend_reject_one(&me, inv.from_user_id).await;
        }
        Some(Default::default())
    }

    // ================================================================ chat

    fn chat_config(&self, channel_type: u32) -> tgf::DChatChannelConfigure {
        let row = self.game_data.chat_channel_type.iter().find(|r| r.chat_channel_type == channel_type);
        tgf::DChatChannelConfigure {
            channel_type,
            max_log_count: row.map(|r| r.max_log_count).unwrap_or(300),
            gc_log_count: row.map(|r| r.gc_log_count).unwrap_or(300),
            gc_expire_duration: Some(crate::google::protobuf::Duration {
                seconds: row.and_then(|r| duration_secs(&r.gc_expire_duration)).unwrap_or(86_400),
                nanos: 0,
            }),
            memory_only: row.map(|r| r.memory_only).unwrap_or(true),
            banned_rule: row.and_then(|r| r.banned_rule.clone()),
            banned_duration: row.map(|r| r.banned_duration).unwrap_or(0),
            words_limit: row.map(|r| r.words_limit).filter(|w| *w > 0).unwrap_or(200),
            heartbeat_interval: CHAT_HEARTBEAT_SECS,
            heartbeat_retry_interval: CHAT_HEARTBEAT_RETRY_SECS,
        }
    }

    fn chat_metadata(&self, channel_id: &str) -> tgf::DChatChannelMetadata {
        let channel_type = chat_type_of(channel_id).unwrap_or(0);
        tgf::DChatChannelMetadata {
            channel_key: Some(chat_channel_key(channel_id, channel_type)),
            channel_configure: Some(self.chat_config(channel_type)),
            last_sequence: chat_last_seq(channel_id),
            ..Default::default()
        }
    }

    fn chat_snapshot(&self, channel_id: &str) -> tgf::DChatChannelSnapshot {
        tgf::DChatChannelSnapshot {
            channel_metadata: Some(self.chat_metadata(channel_id)),
            messages: chat_messages_after(channel_id, 0),
            lock: None,
        }
    }

    pub(super) async fn chat_get_all_channels_impl(&self, _req: tgf::CsChatGetAllChannelsReq) -> Option<tgf::ScChatGetAllChannelsRsp> {
        let me = self.load_me().await;
        social_touch(&self.clean_open_id(), me.user_id);
        let channel_metadata = chat_channels_for(me.user_id).iter().map(|c| self.chat_metadata(c)).collect();
        Some(tgf::ScChatGetAllChannelsRsp { channel_metadata })
    }

    pub(super) async fn chat_get_channel_snapshot_impl(&self, req: tgf::CsChatGetChannelSnapshotReq) -> Option<tgf::ScChatGetChannelSnapshotRsp> {
        let me = self.load_me().await;
        if !chat_can_read(&req.channel_id, me.user_id) {
            println!("[Chat] {}: no access to channel '{}'", me.user_id, req.channel_id);
            self.social_err(tgf::EnErrorCode::EnErrChatInvalieChannel);
            return Some(Default::default());
        }
        Some(tgf::ScChatGetChannelSnapshotRsp { channel_snapshot: Some(self.chat_snapshot(&req.channel_id)) })
    }

    /// Heartbeat with sync points: everything newer than last_sequence is sent as a dirty_chat_message push
    pub(super) async fn chat_channel_heartbeat_impl(&self, req: tgf::CsChatChannelHeartbeatReq) -> Option<tgf::ScChatChannelHeartbeatRsp> {
        let me = self.load_me().await;
        social_touch(&self.clean_open_id(), me.user_id);
        let mut notify = Vec::new();
        for sp in &req.heartbeat_data {
            let Some(k) = sp.channel_key.as_ref() else { continue };
            if !chat_can_read(&k.channel_id, me.user_id) {
                continue;
            }
            let messages = chat_messages_after(&k.channel_id, sp.last_sequence);
            if !messages.is_empty() {
                notify.push(tgf::DChatMessageNotify { channel_id: k.channel_id.clone(), messages });
            }
        }
        if !notify.is_empty() {
            self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_chat_message: notify, ..Default::default() });
        }
        Some(Default::default())
    }

    pub(super) async fn chat_send_message_impl(&self, req: tgf::CsChatSendMessageReq) -> Option<tgf::ScChatSendMessageRsp> {
        use tgf::d_chat_message_detail::Command;
        let me = self.load_me().await;
        let my_open_id = self.clean_open_id();
        social_touch(&my_open_id, me.user_id);
        let reject = |code: tgf::EnErrorCode, why: &str| {
            println!("[Chat] {}: message rejected: {}", me.user_id, why);
            self.social_err(code);
            Some(tgf::ScChatSendMessageRsp { msg: None })
        };

        let Some(channel_key) = req.channel_key else { return reject(tgf::EnErrorCode::EnErrChatInvalieChannel, "no channel_key") };
        let mut detail = req.detail.unwrap_or_default();
        let text = match &detail.command {
            Some(Command::Text(t)) => t.trim().to_string(),
            _ => return reject(tgf::EnErrorCode::EnErrChatMessageIllegal, "only text is supported"),
        };
        // Chat commands ("/help", "/item 100066 15"): do not go to the channel, the reply goes to the personal system channel (chat_command)
        if let Some(cmd) = text.strip_prefix('/') {
            self.chat_command(&me, cmd.trim()).await;
            return Some(tgf::ScChatSendMessageRsp { msg: None });
        }
        let mut channel_type = if channel_key.channel_type != 0 {
            channel_key.channel_type
        } else {
            chat_type_of(&channel_key.channel_id).unwrap_or(0)
        };
        if channel_type == CH_PUBLIC {
            channel_type = CH_WORLD; // there is no Public in the UI — treat it as world
        }
        let cfg = self.chat_config(channel_type);
        if text.is_empty() {
            return reject(tgf::EnErrorCode::EnErrChatMessageIllegal, "empty message");
        }
        if text.chars().count() > cfg.words_limit as usize {
            return reject(tgf::EnErrorCode::EnErrChatChannelWordsLimit, "too long");
        }
        detail.command = Some(Command::Text(text));

        let now = now_ts();
        let mut msg = tgf::DChatMessage {
            create_timepoint: Some(crate::google::protobuf::Timestamp { seconds: now, nanos: 0 }),
            expired_timepoint: Some(crate::google::protobuf::Timestamp {
                seconds: now + cfg.gc_expire_duration.as_ref().map(|d| d.seconds).unwrap_or(86_400),
                nanos: 0,
            }),
            channel_type,
            ..Default::default()
        };
        chat_fill_sender(&me, &mut msg);
        let max_log = cfg.max_log_count as usize;

        // (open_id, channel, message) for the broadcast; the sender's copy — into the response
        let (own, deliveries): (tgf::DChatMessage, Vec<(String, String, tgf::DChatMessage)>) = match channel_type {
            CH_WORLD => {
                if !chat_rate_ok(me.user_id) {
                    return reject(tgf::EnErrorCode::EnErrChatMsgSendFrequently, "too often");
                }
                msg.detail = Some(detail);
                let m = chat_append(WORLD_CHANNEL, msg, max_log);
                let to = social_online().into_iter().map(|(o, _)| (o, WORLD_CHANNEL.to_string(), m.clone())).collect();
                (m, to)
            }
            CH_TEAM => {
                let Some(t) = team::team_of(me.user_id) else { return reject(tgf::EnErrorCode::EnErrChatInvalieChannel, "no party") };
                if !chat_rate_ok(me.user_id) {
                    return reject(tgf::EnErrorCode::EnErrChatMsgSendFrequently, "too often");
                }
                let ch = team_channel(t.team_id);
                msg.detail = Some(detail);
                let m = chat_append(&ch, msg, max_log);
                let to = t.members.iter().map(|mb| (mb.open_id.clone(), ch.clone(), m.clone())).collect();
                (m, to)
            }
            CH_ROOM => {
                let Some(room) = crate::lobby::room::room_of(me.user_id) else {
                    return reject(tgf::EnErrorCode::EnErrChatInvalieChannel, "no room");
                };
                if !chat_rate_ok(me.user_id) {
                    return reject(tgf::EnErrorCode::EnErrChatMsgSendFrequently, "too often");
                }
                let ch = room_channel(room.room_id);
                msg.detail = Some(detail);
                let m = chat_append(&ch, msg, max_log);
                let to = room_members(&room).into_iter().map(|o| (o, ch.clone(), m.clone())).collect();
                (m, to)
            }
            CH_PRIVATE => {
                let rid = detail.received_user_key.as_ref().map(|k| k.user_id).unwrap_or(0);
                if rid == 0 || rid == me.user_id {
                    return reject(tgf::EnErrorCode::EnErrChatChannelCannotSendToself, "no recipient or to self");
                }
                let Some(receiver) = self.load_by_user_id(rid).await else { return reject(tgf::EnErrorCode::EnErrFriendNotFound, "recipient not found") };
                if !chat_rate_ok(me.user_id) {
                    return reject(tgf::EnErrorCode::EnErrChatMsgSendFrequently, "too often");
                }
                if let Some(r) = hall_role(&receiver) {
                    detail.received_user_role_guid = r.role_guid;
                    if detail.received_user_role_name.is_empty() {
                        detail.received_user_role_name = r
                            .data
                            .basic
                            .as_ref()
                            .map(|b| b.role_name.clone())
                            .filter(|n| !n.is_empty())
                            .unwrap_or_else(|| r.role_name.clone());
                    }
                }
                detail.received_user_key = Some(key(rid));
                msg.detail = Some(detail);
                // Everyone has their own private feed: a copy to the recipient and a copy to self (own sequences)
                let theirs = chat_append(&private_channel(rid), msg.clone(), max_log);
                let mine = chat_append(&private_channel(me.user_id), msg, max_log);
                let to = vec![
                    (receiver.open_id.clone(), private_channel(rid), theirs),
                    (my_open_id.clone(), private_channel(me.user_id), mine.clone()),
                ];
                (mine, to)
            }
            _ => return reject(tgf::EnErrorCode::EnErrChatInvalieChannel, "not a player channel"),
        };

        let mut delivered = 0;
        for (open_id, ch, m) in deliveries {
            let push = chat_notify(&ch, vec![m]);
            if open_id == my_open_id {
                // to self — with the same sequence as in the response (the client stores messages by sequence, no duplicate)
                self.push_dirty(push);
            } else if common::push_to(&open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &push) {
                delivered += 1;
            }
        }
        println!("[Chat] {} -> type {} seq {}: delivered to {} players", me.user_id, channel_type, own.sequence, delivered);
        Some(tgf::ScChatSendMessageRsp { msg: Some(own) })
    }

    /// Chat command reply: a system message into the personal system channel (seen only by the player, "General" tab).
    /// `CHAT_CMD_MODAL=1` — also as a modal window (SCModalMessageSync), in case the client does not show the system message.
    /// A party action as a `TeamMember` chat message into the recipient's system channel: this is how the client accepts an invitation,
    /// a rejection, a cancellation (UChatService, variant 101 → UTeamService; Ghidra 02.11). The party snapshot does not open the invitation window.
    pub(super) async fn chat_push_team_member(&self, to_user: u64, sender: &PlayerModel, action: tgf::d_chat_message_team_member::Action) {
        let Some(to) = self.load_by_user_id(to_user).await else { return };
        let ch = sys_channel_of(to_user);
        let now = now_ts();
        let ts = |s: i64| Some(crate::google::protobuf::Timestamp { seconds: s, nanos: 0 });
        let mut msg = tgf::DChatMessage {
            create_timepoint: ts(now),
            expired_timepoint: ts(now + 120),
            channel_type: CH_SYS,
            detail: Some(tgf::DChatMessageDetail {
                command: Some(tgf::d_chat_message_detail::Command::TeamMember(tgf::DChatMessageTeamMember { action: Some(action) })),
                ..Default::default()
            }),
            ..Default::default()
        };
        chat_fill_sender(sender, &mut msg);
        let m = chat_append(&ch, msg, 100);
        crate::lobby::hub::push_to(&to.open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &chat_notify(&ch, vec![m]));
        println!("[Team] chat message TeamMember -> {} ({})", to_user, ch);
    }

    /// A party action (`DChatMessageTeamAction`) as a message into the team channel to all members (08.11, Challenger Mode:
    /// a team snapshot is not enough for a member's client — the "challenge mode" checkbox did not change, it waits for ChallengerModelSwitch).
    /// The team channel — the same as the team chat's; the client checks the action number against acknowledge_action_chat_sequence.
    pub(super) fn chat_push_team_action(&self, t: &team::Team, sender: &PlayerModel, action: tgf::d_chat_message_team_action::Action) {
        let ch = team_channel(t.team_id);
        let now = now_ts();
        let ts = |s: i64| Some(crate::google::protobuf::Timestamp { seconds: s, nanos: 0 });
        let mut msg = tgf::DChatMessage {
            create_timepoint: ts(now),
            expired_timepoint: ts(now + 120),
            channel_type: CH_TEAM,
            detail: Some(tgf::DChatMessageDetail {
                command: Some(tgf::d_chat_message_detail::Command::TeamAction(tgf::DChatMessageTeamAction { action: Some(action) })),
                ..Default::default()
            }),
            ..Default::default()
        };
        chat_fill_sender(sender, &mut msg);
        let m = chat_append(&ch, msg, 100);
        let push = chat_notify(&ch, vec![m]);
        for mb in &t.members {
            crate::lobby::hub::push_to(&mb.open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &push);
        }
        println!("[Team] party action {} -> {} members ({})", t.team_id, t.members.len(), ch);
    }

    fn chat_sys_reply(&self, user_id: u64, text: &str) {
        self.push_dirty(sys_notify_dirty(user_id, text)); // 18.11: the message is built by sys_notify_dirty (also used by toast.rs)
        if std::env::var_os("CHAT_CMD_MODAL").is_some() {
            self.push(MODAL_METHOD, MODAL_TYPE, tgf::ScModalMessageSync { title: "Сервер".into(), content: text.to_string() });
        }
    }

    /// Chat commands. For everyone: `help`, `online`. GM/admin (account roles, without an account — with require = "none"):
    /// `gm_exec` commands on self (`item`, `gold`, `ban` …) and `to <nick|open_id> <command>` — on another player.
    async fn chat_command(&self, me: &PlayerModel, line: &str) {
        let mut parts = line.split_whitespace();
        let cmd = parts.next().unwrap_or("").to_lowercase();
        let rest: Vec<&str> = parts.collect();
        let gm = crate::auth::accounts::may_gm(&self.db, &me.open_id).await;
        let reply = match cmd.as_str() {
            "" | "help" | "?" | "помощь" => {
                let mut lines = vec![
                    "Команды чата (в канал не уходят, ответ видите только вы):".to_string(),
                    "/help — этот список".into(),
                    "/online — кто в сети".into(),
                ];
                if gm {
                    lines.push("GM:".into());
                    lines.push("/to <nick|open_id> <command> — a command on another player".into());
                    lines.extend(self.gm_exec(&["help".to_string()], &me.open_id).await.result_list.into_iter().map(|l| format!("/{}", l)));
                }
                lines.join("\n")
            }
            "online" | "who" => {
                let mut names = Vec::new();
                for open_id in hub::online_ids() {
                    if let Some(p) = self.db.get::<PlayerModel>("players", &open_id).await {
                        names.push(hall_role(&p).map(|r| r.role_name.clone()).unwrap_or(open_id));
                    }
                }
                names.sort();
                format!("В сети {}: {}", names.len(), names.join(", "))
            }
            _ if !gm => format!("Неизвестная команда /{} — список: /help", cmd),
            "to" => match rest.split_first() {
                Some((who, tail)) if !tail.is_empty() => match self.chat_find_online(who).await {
                    Ok(open_id) => {
                        // a command on another player: their session, changes — to them via pushes
                        let target = GatewaySession {
                            db: self.db.clone(),
                            game_data: self.game_data.clone(),
                            open_id: open_id.clone(),
                            peer_ip: format!("chat:{}", me.open_id),
                            after_pushes: Mutex::new(Vec::new()),
                            rpc_error: std::sync::atomic::AtomicI32::new(0),
                        };
                        let rsp = target.gm_exec(&[tail.join(" ")], &me.open_id).await;
                        for (m, t, b) in std::mem::take(&mut *target.after_pushes.lock().unwrap()) {
                            hub::push_raw(&open_id, &m, &t, b);
                        }
                        gm_text(&rsp)
                    }
                    Err(e) => e,
                },
                _ => "/to <nick|open_id> <command>".into(),
            },
            _ => gm_text(&self.gm_exec(&[line.to_string()], &me.open_id).await),
        };
        println!("[Chat] {}: /{} → {}", me.user_id, line, reply.replace('\n', " | "));
        self.chat_sys_reply(me.user_id, &reply);
    }

    /// Online player by open_id or character name (case-insensitive)
    async fn chat_find_online(&self, who: &str) -> Result<String, String> {
        let online = hub::online_ids();
        if online.iter().any(|o| o == who) {
            return Ok(who.to_string());
        }
        let mut found = Vec::new();
        for open_id in online {
            if let Some(p) = self.db.get::<PlayerModel>("players", &open_id).await {
                if hall_role(&p).map(|r| r.role_name.to_lowercase() == who.to_lowercase()).unwrap_or(false) {
                    found.push(open_id);
                }
            }
        }
        match found.len() {
            1 => Ok(found.remove(0)),
            0 => Err(format!("\"{}\" is not online", who)),
            _ => Err(format!("\"{}\" — several players, give the open_id", who)),
        }
    }

    /// Opening the chat panel: echo `open` (as the official server) + the player's channel snapshots as a dirty_chat_snapshot push.
    /// TODO(verify): does the client accept channels from dirty_chat_snapshot without chat_get_all_channels?
    pub(super) async fn chat_update_panel_impl(&self, req: tgf::CsChatUpdatePanelReq) -> Option<tgf::ScChatUpdatePanelRsp> {
        let me = self.load_me().await;
        social_touch(&self.clean_open_id(), me.user_id);
        if req.open {
            let snapshots = chat_channels_for(me.user_id).iter().map(|c| self.chat_snapshot(c)).collect();
            self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_chat_snapshot: snapshots, ..Default::default() });
        }
        Some(tgf::ScChatUpdatePanelRsp { open: req.open })
    }
}

// ---- one-time migration of item guids (crate::guid::migrate) ----
/// Gift boxes (key — user_id): item guids in DFriendGift
pub(super) async fn gift_migrate_guids(db: &crate::db::GameDb, user_ids: &[u64], map: &mut crate::guid::GuidMap) -> usize {
    let mut total = 0;
    for uid in user_ids {
        let Some(mut b) = db.get::<GiftBox>(FRIEND_GIFT, &uid.to_string()).await else { continue };
        let mut n = 0;
        for bytes in b.gifts.iter_mut() {
            let Ok(mut g) = <tgf::DFriendGift as prost::Message>::decode(bytes.as_slice()) else { continue };
            let k = crate::guid::remap_message(&mut g, map);
            if k > 0 {
                *bytes = prost::Message::encode_to_vec(&g);
                n += k;
            }
        }
        if n > 0 {
            db.save(FRIEND_GIFT, &uid.to_string(), &b).await;
            total += n;
        }
    }
    total
}

/// GM command reply text for the chat
fn gm_text(rsp: &tgf::ScPlayerGmCommandRsp) -> String {
    if rsp.result_code == 0 {
        rsp.result_list.join("\n")
    } else {
        rsp.result_message.clone()
    }
}
