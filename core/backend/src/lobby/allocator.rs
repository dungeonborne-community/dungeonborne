//! Allocation of game servers (headless listen client + the DungeonborneRuServer mod) to match rooms.
//! For now the pool is static, from env: DS_POOL="ip:port@/TgfMaps/...;ip:port@*" (`*` — any map).
//! The address is the one clients connect to (the public IP of the Hetzner machine).
//! Later the pool will be filled by ds-agent (registration, dbserver-status.json status, release on ds_end).
//!
//! Slot states (M3, contract with the mod 03.11): free → taken by a room → room closed → reset → free.
//! Reset: the server mod sends the remaining players to the lobby, clears JoinGuard, reloads the map (epoch + 1). The slot is free
//! again when the mod heartbeat (`POST /ds/slot`, `heartbeat`) arrives with phase=ready and an epoch different from the one at closing
//! (greater — the map was reloaded, smaller — the process was restarted).
//! A server without heartbeat (old mod, dbtest) is freed immediately, as before. If there was a heartbeat but it has been silent longer
//! than DEAD_SECS — the slot is dead, no rooms are given to it.
//!
//! Waiting hall (docs/WAITING_ROOM.md, env DS_WAITING_ROOM=1): the room driver puts a `host` task on the slot (hall map,
//! scene chain, roster and teams); the heartbeat replies `{action: "host", host}` until the mod reports this room's phase
//! (waiting / entering / battle + battle_id). Room events go to clients once the hall is loaded ([`room_events_allowed`]):
//! the client travels to the server only on startBattling (hostflow 03.11 11:04: it got dsInitFinish + startLoading and waited),
//! for it the hall is already the match. Empty hall / no-shows — `phase=empty` / `absent` (ds::agent_link::slot_heartbeat).

use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// A server without heartbeat for longer than this is considered crashed
const DEAD_SECS: u64 = 60;

struct Slot {
    addr: String,
    map: Option<String>,
    battle_id: Option<String>,
    busy_until: Option<Instant>,
    /// room closed, the mod has not reset yet: (closed room, mod epoch at the moment of closing)
    resetting: Option<(String, u64)>,
    /// last mod heartbeat: when, epoch, phase
    last_hb: Option<Instant>,
    epoch: u64,
    phase: String,
    /// mod task for the current room (waiting hall) — until the mod reports its phase
    host: Option<Value>,
    /// room phase as reported by the mod: (battle_id, waiting | entering | battle)
    room_phase: Option<(String, String)>,
    /// since when the current room_phase holds (open hall: how much longer players can be added)
    phase_since: Option<Instant>,
    /// mod epoch when it reported the current room's phase: a different epoch in ready/standby — the server process
    /// was restarted in the middle of the room (R1-stuck 05.11), its match is lost
    room_epoch: u64,
    /// members were added to the task (open hall) — hand the mod `action: members`
    host_dirty: bool,
    /// the mod is in a hall/match, but the backend does not know the room (backend restarted mid-match) — since when
    orphan_since: Option<Instant>,
}

impl Slot {
    fn new(addr: &str, map: Option<String>) -> Self {
        Slot {
            addr: addr.to_string(),
            map,
            battle_id: None,
            busy_until: None,
            resetting: None,
            last_hb: None,
            epoch: 0,
            phase: String::new(),
            host: None,
            room_phase: None,
            phase_since: None,
            room_epoch: 0,
            host_dirty: false,
            orphan_since: None,
        }
    }

    /// Free for a new room (as in allocate_in). With a waiting hall (DS_WAITING_ROOM=1) — only after the first
    /// mod heartbeat: otherwise a room got a slot without a hall task (prod 03.11 17:30: "Start" a minute before
    /// the heartbeat of a restarted server — the server in the hall did not know about the room, the room held the slot until DS_ROOM_TTL)
    fn free_for(&self, map_path: &str) -> bool {
        let waiting_room = std::env::var("DS_WAITING_ROOM").map(|v| v == "1").unwrap_or(false);
        self.battle_id.is_none()
            && self.resetting.is_none()
            && !self.dead()
            && !self.orphan()
            && !(waiting_room && self.last_hb.is_none())
            && self.map.as_deref().map_or(true, |m| m == map_path)
    }

    fn dead(&self) -> bool {
        self.last_hb.map_or(false, |t| t.elapsed() >= Duration::from_secs(DEAD_SECS))
    }

    /// The room left: with a heartbeat mod — into reset, without one — free immediately
    fn vacate(&mut self) {
        self.host = None;
        self.room_phase = None;
        self.phase_since = None;
        self.room_epoch = 0;
        self.host_dirty = false;
        if let Some(b) = self.battle_id.take() {
            if self.last_hb.is_some() {
                self.resetting = Some((b, self.epoch));
            }
        }
        self.busy_until = None;
    }

    /// The mod reports a hall/match for a room the backend does not have: a foreign match (from before the backend restart) is running
    fn orphan(&self) -> bool {
        self.battle_id.is_none() && self.resetting.is_none() && matches!(self.phase.as_str(), "waiting" | "entering" | "battle")
    }

    fn state(&self) -> &'static str {
        if self.dead() {
            "dead"
        } else if self.battle_id.is_some() {
            "busy"
        } else if self.resetting.is_some() {
            "resetting"
        } else if self.orphan() {
            "orphan"
        } else {
            "free"
        }
    }
}

static POOL: OnceLock<Mutex<Vec<Slot>>> = OnceLock::new();

/// How long to keep a server bound to a room if nobody released it explicitly (raid 40 min + margin).
fn slot_ttl() -> Duration {
    let secs = std::env::var("DS_ROOM_TTL").ok().and_then(|v| v.parse().ok()).unwrap_or(45 * 60);
    Duration::from_secs(secs)
}

fn pool() -> &'static Mutex<Vec<Slot>> {
    POOL.get_or_init(|| {
        let spec = std::env::var("DS_POOL").unwrap_or_else(|_| "127.0.0.1:7777@*".to_string());
        let slots: Vec<Slot> = spec
            .split(';')
            .filter(|s| !s.trim().is_empty())
            .map(|s| {
                let (addr, map) = s.trim().split_once('@').unwrap_or((s.trim(), "*"));
                Slot::new(addr, if map == "*" { None } else { Some(map.to_string()) })
            })
            .collect();
        println!("[Allocator] server pool: {}", spec);
        Mutex::new(slots)
    })
}

/// IPs of the pool servers (DS_POOL): they enter the lobby without the launcher relay (`auth::registry::admit`)
pub fn pool_ips() -> Vec<std::net::IpAddr> {
    pool().lock().unwrap().iter().filter_map(|s| s.addr.parse::<std::net::SocketAddr>().ok().map(|a| a.ip())).collect()
}

/// Takes a free server for a room. None — no free servers (the room waits for the next tick).
pub fn allocate(battle_id: &str, map_path: &str) -> Option<String> {
    allocate_in(&mut pool().lock().unwrap(), battle_id, map_path)
}

fn allocate_in(slots: &mut [Slot], battle_id: &str, map_path: &str) -> Option<String> {
    let now = Instant::now();
    for slot in slots.iter_mut() {
        if slot.busy_until.map_or(false, |t| t <= now) {
            println!("[Allocator] {} released by timeout (was {:?})", slot.addr, slot.battle_id);
            slot.vacate();
        }
    }
    let slot = slots.iter_mut().find(|s| s.free_for(map_path))?;
    slot.battle_id = Some(battle_id.to_string());
    slot.busy_until = Some(now + slot_ttl());
    println!("[Allocator] {} -> room {} ({})", slot.addr, battle_id, map_path);
    Some(slot.addr.clone())
}

/// A server from ds-agent (crate::ds::agent_link): add it to the pool if there is no such address yet. Map None — any.
/// Full T2 (states, health by heartbeat, removing vanished ones) — docs/DS_PIPELINE.md.
pub fn register(addr: &str, map: Option<String>) {
    let mut slots = pool().lock().unwrap();
    if slots.iter().any(|s| s.addr == addr) {
        return;
    }
    println!("[Allocator] + {} from ds-agent (map {})", addr, map.as_deref().unwrap_or("*"));
    slots.push(Slot::new(addr, map));
}

pub fn release(battle_id: &str) {
    release_in(&mut pool().lock().unwrap(), battle_id)
}

fn release_in(slots: &mut [Slot], battle_id: &str) {
    for slot in slots.iter_mut() {
        if slot.battle_id.as_deref() == Some(battle_id) {
            slot.vacate();
            let next = if slot.resetting.is_some() { "server reset" } else { "free" };
            println!("[Allocator] {} released (room {}) — {}", slot.addr, battle_id, next);
        }
    }
}

/// Waiting hall: the mod task for room `battle_id` on server `addr` (only a server with heartbeat). true — placed.
pub fn set_host(addr: &str, battle_id: &str, host: Value) -> bool {
    let mut slots = pool().lock().unwrap();
    let Some(s) = slots.iter_mut().find(|s| s.addr == addr && s.battle_id.as_deref() == Some(battle_id) && s.last_hb.is_some()) else {
        return false;
    };
    s.host = Some(host);
    println!("[Allocator] {}: host task for room {}", addr, battle_id);
    true
}

/// There is a free server for the map (open hall: we create the room only when there is somewhere to open it)
pub fn has_free(map_path: &str) -> bool {
    pool().lock().unwrap().iter().any(|s| s.free_for(map_path))
}

/// Before an open hall enters the match there must be this much time left so that a new player has time to load
const HALL_JOIN_MARGIN_SECS: u64 = 25;

/// Hall timer for entry: `waiting_timeout`, and for a lobby hall with a player minimum (`min_players`, 08.11) — plus
/// `hold_secs`: the mod holds the countdown until the minimum gathers, and new players must be let in all that time.
/// 12.11: only with a minimum above 1 — with minimum 1 (lobby fill-based) the mod does not hold the countdown, the hall goes into the match
/// on `waiting_timeout`, while we considered it open for another `hold_secs` and let players into a closing hall (k1ko: entry
/// 2 s before the match → "did not come to the hall", the client stayed in the raid with no room end)
fn hall_timeout(h: &Value) -> u64 {
    let base = h["waiting_timeout"].as_u64().unwrap_or(120);
    if h["min_players"].as_u64().unwrap_or(0) > 1 { base + h["hold_secs"].as_u64().unwrap_or(0) } else { base }
}

/// The room's hall still accepts players: the task is issued, the mod is still loading it or waiting in the hall, and more than
/// HALL_JOIN_MARGIN_SECS remain until the end of `waiting_timeout`
pub fn hall_open(battle_id: &str) -> bool {
    hall_open_in(&pool().lock().unwrap(), battle_id)
}

fn hall_open_in(slots: &[Slot], battle_id: &str) -> bool {
    let Some(s) = slots.iter().find(|s| s.battle_id.as_deref() == Some(battle_id)) else { return false };
    let Some(h) = &s.host else { return false };
    let timeout = hall_timeout(h);
    match s.room_phase.as_ref().filter(|(b, _)| b == battle_id).map(|(_, p)| p.as_str()) {
        // the hall is still loading: the mod already sends this room's battle_id with phase hosting (ready — until the countdown starts)
        None | Some("hosting" | "ready" | "standby") => true,
        Some("waiting") => s.phase_since.map_or(true, |t| t.elapsed().as_secs() + HALL_JOIN_MARGIN_SECS < timeout),
        _ => false,
    }
}

/// Room phase as reported by the mod (waiting / entering / battle); None — not reported yet
pub fn room_phase(battle_id: &str) -> Option<String> {
    let slots = pool().lock().unwrap();
    let s = slots.iter().find(|s| s.battle_id.as_deref() == Some(battle_id))?;
    s.room_phase.as_ref().filter(|(b, _)| b == battle_id).map(|(_, p)| p.clone())
}

/// The server has already been given this room's hall task
pub fn has_host(battle_id: &str) -> bool {
    pool().lock().unwrap().iter().any(|s| s.battle_id.as_deref() == Some(battle_id) && s.host.is_some())
}

/// How many seconds the room's hall still accepts players (see hall_open); None — no hall running
pub fn hall_join_secs(battle_id: &str) -> Option<i64> {
    let slots = pool().lock().unwrap();
    let s = slots.iter().find(|s| s.battle_id.as_deref() == Some(battle_id))?;
    let timeout = hall_timeout(s.host.as_ref()?);
    let since = s.phase_since?.elapsed().as_secs();
    Some((timeout.saturating_sub(since + HALL_JOIN_MARGIN_SECS)) as i64)
}

/// Open hall: add members to the mod task (`members` in host_task format); the mod gets them as
/// `action: members` in the heartbeat reply
pub fn host_add_members(battle_id: &str, members: Vec<Value>) -> bool {
    let mut slots = pool().lock().unwrap();
    let Some(s) = slots.iter_mut().find(|s| s.battle_id.as_deref() == Some(battle_id)) else { return false };
    let Some(Value::Array(list)) = s.host.as_mut().map(|h| &mut h["members"]) else { return false };
    let n = members.len();
    list.extend(members);
    s.host_dirty = true;
    println!("[Allocator] {}: hall {}: added {} (total {})", s.addr, battle_id, n, list.len());
    true
}

/// The slot's server has sent heartbeats (new mod with the /ds/slot contract)
pub fn has_heartbeat(addr: &str) -> bool {
    pool().lock().unwrap().iter().any(|s| s.addr == addr && s.last_hb.is_some() && !s.dead())
}

/// Up to which room event (lobby_handlers: 1 startDsInit, 2 dsInitFinish, 3 startLoading, 4 startBattling)
/// we may proceed. Without a waiting hall — all 4; with a hall — only startDsInit until the mod reports this room's phase
/// (hall loaded), then all 4: the client connects on startBattling.
pub fn room_events_allowed(battle_id: &str) -> i64 {
    room_events_in(&pool().lock().unwrap(), battle_id)
}

fn room_events_in(slots: &[Slot], battle_id: &str) -> i64 {
    let Some(s) = slots.iter().find(|s| s.battle_id.as_deref() == Some(battle_id) && s.host.is_some()) else { return 4 };
    match s.room_phase.as_ref().filter(|(b, _)| b == battle_id).map(|(_, p)| p.as_str()) {
        Some("waiting") | Some("entering") | Some("battle") => 4,
        _ => 1,
    }
}

/// The room's match is already running on the server: without a waiting hall — as soon as the room is on a slot, with a hall — after phase=battle.
/// For reconnecting (user_battle_room at login, docs/RECONNECT.md): the client does not reconnect from the hall.
pub fn battle_started(battle_id: &str) -> bool {
    let slots = pool().lock().unwrap();
    match slots.iter().find(|s| s.battle_id.as_deref() == Some(battle_id)) {
        Some(s) if s.host.is_some() => matches!(&s.room_phase, Some((b, p)) if b == battle_id && p == "battle"),
        Some(_) => true,
        None => false,
    }
}

/// If the room's server stays silent longer than this while loading the hall, the room goes to another server
const FAILOVER_SECS: u64 = 15;

/// Room transfer (04.11 09:24: 7777 crashed in UE4SS while loading the hall, the room waited ~2.5 min for its restart, the client
/// hung in "connecting" meanwhile, and a free 7787 stood right next to it). Conditions: the room's slot has a hall task, the mod has not
/// reported the hall loaded (clients got only startDsInit — nobody connected to the server) and the heartbeat has been silent
/// longer than FAILOVER_SECS. The task moves to a free slot, the old one goes into reset (free after the process restarts).
/// Some(new address) — transferred.
pub fn failover(battle_id: &str, map_path: &str) -> Option<String> {
    failover_in(&mut pool().lock().unwrap(), battle_id, map_path)
}

fn failover_in(slots: &mut [Slot], battle_id: &str, map_path: &str) -> Option<String> {
    let from = slots.iter().position(|s| s.battle_id.as_deref() == Some(battle_id))?;
    let old = &slots[from];
    let hall_loaded = matches!(&old.room_phase, Some((b, p)) if b == battle_id && matches!(p.as_str(), "waiting" | "entering" | "battle"));
    let silent = old.last_hb.map_or(false, |t| t.elapsed() >= Duration::from_secs(FAILOVER_SECS));
    if old.host.is_none() || hall_loaded || !silent {
        return None;
    }
    let to = (0..slots.len()).find(|&i| i != from && slots[i].free_for(map_path) && slots[i].last_hb.is_some())?;
    let (host, busy_until) = (slots[from].host.take(), slots[from].busy_until);
    let old_addr = slots[from].addr.clone();
    slots[from].vacate();
    let new = &mut slots[to];
    new.battle_id = Some(battle_id.to_string());
    new.busy_until = busy_until;
    new.host = host;
    println!("[Allocator] {}: server {} silent for {} s while loading the hall — room moved to {}", battle_id, old_addr,
        FAILOVER_SECS, new.addr);
    Some(new.addr.clone())
}

/// The room is closed but its server is still resetting (for `room_closed` in the 409 reply to a repeated result)
pub fn resetting_after(battle_id: &str) -> bool {
    resetting_in(&pool().lock().unwrap(), battle_id)
}

fn resetting_in(slots: &[Slot], battle_id: &str) -> bool {
    slots.iter().any(|s| s.resetting.as_ref().map_or(false, |(b, _)| b == battle_id))
}

/// Server mod heartbeat (`POST /ds/slot`). The slot — by `slot` (address from DS_POOL), otherwise by (sender IP, port),
/// otherwise by the only slot with this port. Reply — `{action: none|reset|quit, battle_id?}`.
pub fn heartbeat(sender_ip: &str, slot: &str, port: u16, phase: &str, epoch: u64, battle: &str) -> Value {
    heartbeat_in(&mut pool().lock().unwrap(), sender_ip, slot, port, phase, epoch, battle)
}

fn heartbeat_in(slots: &mut [Slot], sender_ip: &str, slot: &str, port: u16, phase: &str, epoch: u64, battle: &str) -> Value {
    let port_of = |a: &str| a.rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok());
    let idx = slots
        .iter()
        .position(|s| !slot.is_empty() && s.addr == slot)
        .or_else(|| slots.iter().position(|s| s.addr == format!("{}:{}", sender_ip, port)))
        .or_else(|| {
            let same: Vec<usize> = (0..slots.len()).filter(|&i| port_of(&slots[i].addr) == Some(port)).collect();
            (same.len() == 1).then(|| same[0])
        });
    let Some(i) = idx else {
        println!("[Allocator] heartbeat {}:{} (slot {:?}) — no such server in the pool", sender_ip, port, slot);
        return json!({ "action": "none", "unknown_slot": true });
    };
    let s = &mut slots[i];
    if s.last_hb.is_none() || s.dead() {
        println!("[Allocator] {}: mod heartbeat (phase {}, epoch {})", s.addr, phase, epoch);
    }
    s.last_hb = Some(Instant::now());
    s.epoch = epoch;
    s.phase = phase.to_string();
    // R1-stuck (05.11 20:47): the server was restarted (quit/crash) in the middle of a room — the new process is free (ready, a different
    // epoch), while the backend has the room "in match": clients get "match running — reconnect", room_create -4637. We hand its
    // battle_id in stale_battle — ds::agent_link closes the room as phase=empty (no result awaited, "match over").
    let stale = match (&s.battle_id, &s.room_phase) {
        (Some(b), Some((rb, _))) if rb == b && battle != b && epoch != s.room_epoch && matches!(phase, "ready" | "standby") => {
            println!("[Allocator] {}: room {} — server restarted (epoch {} → {}), its match is lost", s.addr, b, s.room_epoch, epoch);
            Some(b.clone())
        }
        _ => None,
    };
    // a match the backend does not know about (restart mid-match): no rooms given, it plays out until the room deadline, then reset
    if s.orphan() {
        let since = *s.orphan_since.get_or_insert_with(Instant::now);
        // a waiting hall without a room has nothing to play out — reset immediately (03.11 18:16: a backend restart mid-hall
        // held the only slot for 45 min); a match — plays out until the room deadline
        let hall = phase == "waiting";
        if hall || since.elapsed() >= slot_ttl() {
            println!("[Allocator] {}: {} without a room for {} s — reset", s.addr, if hall { "hall" } else { "match" }, since.elapsed().as_secs());
            s.orphan_since = None;
            s.resetting = Some((battle.to_string(), epoch));
            return json!({ "action": "reset", "battle_id": battle });
        }
    } else {
        s.orphan_since = None;
    }
    if !battle.is_empty() && s.battle_id.as_deref() == Some(battle) && s.room_phase.as_ref() != Some(&(battle.to_string(), phase.to_string())) {
        println!("[Allocator] {}: room {} — phase {}", s.addr, battle, phase);
        s.room_phase = Some((battle.to_string(), phase.to_string()));
        s.phase_since = Some(Instant::now());
        s.room_epoch = epoch;
    }
    let mut rsp = match s.resetting.clone() {
        // smaller epoch — the process was restarted (counting from zero): the map is fresh too
        Some((_, at)) if (phase == "ready" || phase == "standby") && epoch != at => {
            s.resetting = None;
            println!("[Allocator] {} reset (epoch {} → {}) — free", s.addr, at, epoch);
            json!({ "action": "none" })
        }
        Some((b, _)) if phase == "resetting" || phase == "exiting" => json!({ "action": "none", "battle_id": b }),
        Some((b, _)) => json!({ "action": "reset", "battle_id": b }),
        None => match (&s.battle_id, &s.host) {
            // hall task — until the mod reports this room's phase
            (Some(b), Some(h)) if !matches!(&s.room_phase, Some((rb, p)) if rb == b && matches!(p.as_str(), "waiting" | "entering" | "battle")) => {
                json!({ "action": "host", "battle_id": b, "host": h })
            }
            // open hall: new members of a hall already running
            (Some(b), Some(h)) if s.host_dirty => {
                let rsp = json!({ "action": "members", "battle_id": b, "host": h });
                s.host_dirty = false;
                rsp
            }
            _ => json!({ "action": "none" }),
        },
    };
    if let Some(b) = stale {
        rsp["stale_battle"] = json!(b);
    }
    rsp
}

/// Whether a server is needed on each slot (ds-agent scaling, env DS_AUTOSCALE=1): busy ones (room, reset, match without
/// a room) — always; of the rest — enough so that `min_idle` are free, plus one more if players are waiting in the
/// queue (`demand`: no free server — cold start). Live free ones come first — they are not shut down.
/// Order — as in DS_POOL. Reply: address -> needed.
pub fn want_flags(min_idle: usize, demand: bool) -> std::collections::HashMap<String, bool> {
    want_in(&pool().lock().unwrap(), min_idle, demand)
}

fn want_in(slots: &[Slot], min_idle: usize, demand: bool) -> std::collections::HashMap<String, bool> {
    let busy = |s: &Slot| s.battle_id.is_some() || s.resetting.is_some() || s.orphan();
    let alive = |s: &Slot| s.last_hb.is_some() && !s.dead();
    let mut need = min_idle + usize::from(demand);
    let mut out = std::collections::HashMap::new();
    for s in slots.iter().filter(|s| busy(s)) {
        out.insert(s.addr.clone(), true);
    }
    let free: Vec<&Slot> = slots.iter().filter(|s| !busy(s) && alive(s)).chain(slots.iter().filter(|s| !busy(s) && !alive(s))).collect();
    for s in free {
        out.insert(s.addr.clone(), need > 0);
        need = need.saturating_sub(1);
    }
    out
}

/// Seconds since the freshest heartbeat of any pool slot (None — there were none) — the ds::watchdog guard
pub fn freshest_hb_secs() -> Option<u64> {
    pool().lock().unwrap().iter().filter_map(|s| s.last_hb.map(|t| t.elapsed().as_secs())).min()
}

/// Slots for debugging (`GET /ds/slots`)
pub fn snapshot() -> Value {
    let slots = pool().lock().unwrap();
    Value::Array(
        slots
            .iter()
            .map(|s| {
                json!({
                    "addr": s.addr, "map": s.map, "state": s.state(), "battle_id": s.battle_id,
                    "resetting": s.resetting.as_ref().map(|(b, e)| json!({ "battle_id": b, "epoch_at_close": e })),
                    "phase": s.phase, "epoch": s.epoch, "room_phase": s.room_phase, "host": s.host.is_some(),
                    "last_hb_secs": s.last_hb.map(|t| t.elapsed().as_secs()),
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 12.11: hall hold (`hold_secs`) — only with a minimum above 1; with minimum 1 the hall goes into the match by timer
    #[test]
    fn hall_hold_only_with_min_above_one() {
        let h = |min: u64| serde_json::json!({ "waiting_timeout": 180, "min_players": min, "hold_secs": 300 });
        assert_eq!(hall_timeout(&h(0)), 180);
        assert_eq!(hall_timeout(&h(1)), 180);
        assert_eq!(hall_timeout(&h(2)), 480);
    }

    #[test]
    fn slot_reset_cycle() {
        let mut p = vec![Slot::new("127.0.0.8:7787", None), Slot::new("127.0.0.8:7797", Some("/other".into()))];
        let map = "/m";
        let hb = |p: &mut Vec<Slot>, slot: &str, phase: &str, epoch: u64| heartbeat_in(p, "10.0.0.1", slot, 7787, phase, epoch, "");
        // without heartbeat — as before: free immediately
        assert_eq!(allocate_in(&mut p, "r1", map).as_deref(), Some("127.0.0.8:7787"));
        release_in(&mut p, "r1");
        assert_eq!(allocate_in(&mut p, "r2", map).as_deref(), Some("127.0.0.8:7787"));

        // the mod sends heartbeats → after closing, the slot is in reset
        assert_eq!(hb(&mut p, "127.0.0.8:7787", "hosting", 1)["action"], "none");
        release_in(&mut p, "r2");
        assert!(resetting_in(&p, "r2"));
        assert_eq!(allocate_in(&mut p, "r3", map), None, "no room in the middle of a reset");
        let a = hb(&mut p, "", "hosting", 1); // no slot — by the only slot with port 7787
        assert_eq!((a["action"].as_str(), a["battle_id"].as_str()), (Some("reset"), Some("r2")));
        assert_eq!(hb(&mut p, "127.0.0.8:7787", "resetting", 1)["action"], "none");
        assert_eq!(hb(&mut p, "127.0.0.8:7787", "ready", 1)["action"], "reset", "epoch did not grow");
        hb(&mut p, "127.0.0.8:7787", "ready", 2);
        assert!(!resetting_in(&p, "r2"));
        assert_eq!(allocate_in(&mut p, "r3", map).as_deref(), Some("127.0.0.8:7787"));
        assert_eq!(heartbeat_in(&mut p, "10.0.0.1", "nope:1", 1, "ready", 1, "")["unknown_slot"], true);

        // silent longer than DEAD_SECS — dead, no rooms given
        release_in(&mut p, "r3");
        hb(&mut p, "", "ready", 3);
        p[0].last_hb = Instant::now().checked_sub(Duration::from_secs(DEAD_SECS + 1));
        assert_eq!(p[0].state(), "dead");
        assert_eq!(allocate_in(&mut p, "r4", map), None);
    }

    #[test]
    fn restarted_server_reports_stale_battle() {
        let mut p = vec![Slot::new("127.0.0.8:7787", None)];
        let hb = |p: &mut Vec<Slot>, phase: &str, epoch: u64, battle: &str| heartbeat_in(p, "10.0.0.1", "127.0.0.8:7787", 7787, phase, epoch, battle);
        hb(&mut p, "ready", 10, "");
        assert_eq!(allocate_in(&mut p, "a1", "/m").as_deref(), Some("127.0.0.8:7787"));
        assert!(hb(&mut p, "ready", 10, "").get("stale_battle").is_none(), "the mod has not reported the room yet");
        hb(&mut p, "battle", 10, "a1");
        assert!(hb(&mut p, "battle", 10, "a1").get("stale_battle").is_none());
        assert!(hb(&mut p, "ready", 10, "").get("stale_battle").is_none(), "same process and map — not a restart");
        // process restarted: ready with a new epoch, knows nothing about the room
        assert_eq!(hb(&mut p, "ready", 20, "")["stale_battle"], "a1");
        release_in(&mut p, "a1");
        assert!(hb(&mut p, "ready", 20, "").get("stale_battle").is_none(), "after the room is closed — quiet");
    }

    #[test]
    fn waiting_room_host_and_room_events() {
        let mut p = vec![Slot::new("127.0.0.8:7787", None)];
        let hb = |p: &mut Vec<Slot>, phase: &str, battle: &str| heartbeat_in(p, "10.0.0.1", "127.0.0.8:7787", 7787, phase, 1, battle);
        hb(&mut p, "ready", "");
        assert_eq!(allocate_in(&mut p, "w1", "/m").as_deref(), Some("127.0.0.8:7787"));
        assert_eq!(room_events_in(&p, "w1"), 4, "no task — as before");
        p[0].host = Some(json!({ "battle_id": "w1" }));
        assert_eq!(room_events_in(&p, "w1"), 1, "hall not loaded — no address");
        assert_eq!(hb(&mut p, "ready", "")["action"], "host");
        assert_eq!(hb(&mut p, "hosting", "w1")["action"], "host", "loading — the task is repeated");
        assert_eq!(hb(&mut p, "waiting", "w1")["action"], "none");
        assert_eq!(room_events_in(&p, "w1"), 4, "hall loaded — the client travels on startBattling");
        hb(&mut p, "entering", "w1");
        assert_eq!(room_events_in(&p, "w1"), 4);
        hb(&mut p, "battle", "w1");
        assert_eq!(room_events_in(&p, "w1"), 4);
        hb(&mut p, "battle", "other");
        assert_eq!(room_events_in(&p, "w1"), 4, "a foreign battle_id does not change the phase");
        release_in(&mut p, "w1");
        assert!(p[0].host.is_none() && p[0].room_phase.is_none());
        assert_eq!(hb(&mut p, "battle", "w1")["action"], "reset");
    }

    #[test]
    fn failover_moves_room_while_hall_loading() {
        let mut p = vec![Slot::new("s:1", None), Slot::new("s:2", None)];
        heartbeat_in(&mut p, "x", "s:1", 1, "standby", 5, "");
        heartbeat_in(&mut p, "x", "s:2", 2, "standby", 7, "");
        assert_eq!(allocate_in(&mut p, "f1", "/m").as_deref(), Some("s:1"));
        p[0].host = Some(json!({ "battle_id": "f1" }));
        heartbeat_in(&mut p, "x", "s:1", 1, "hosting", 5, "f1");
        assert_eq!(failover_in(&mut p, "f1", "/m"), None, "heartbeat is fresh — leave it alone");
        p[0].last_hb = Instant::now().checked_sub(Duration::from_secs(FAILOVER_SECS + 1));
        assert_eq!(failover_in(&mut p, "f1", "/m").as_deref(), Some("s:2"));
        assert_eq!((p[1].battle_id.as_deref(), p[1].host.is_some()), (Some("f1"), true));
        assert!(p[0].battle_id.is_none() && p[0].host.is_none() && p[0].resetting.is_some(), "the old one — into reset");
        assert_eq!(heartbeat_in(&mut p, "x", "s:2", 2, "standby", 7, "")["action"], "host", "the new one gets the task");
        // the restarted old process — free again
        heartbeat_in(&mut p, "x", "s:1", 1, "standby", 9, "");
        assert_eq!(p[0].state(), "free");

        // the hall is already loaded (clients are travelling to the server) — no transfer
        let mut q = vec![Slot::new("s:1", None), Slot::new("s:2", None)];
        heartbeat_in(&mut q, "x", "s:1", 1, "standby", 5, "");
        heartbeat_in(&mut q, "x", "s:2", 2, "standby", 7, "");
        allocate_in(&mut q, "f2", "/m");
        q[0].host = Some(json!({ "battle_id": "f2" }));
        heartbeat_in(&mut q, "x", "s:1", 1, "waiting", 5, "f2");
        q[0].last_hb = Instant::now().checked_sub(Duration::from_secs(FAILOVER_SECS + 1));
        assert_eq!(failover_in(&mut q, "f2", "/m"), None);
    }

    #[test]
    fn want_keeps_busy_plus_idle_reserve() {
        let mut p = vec![Slot::new("s:1", None), Slot::new("s:2", None), Slot::new("s:3", None), Slot::new("s:4", None)];
        // nothing running: one is needed (spare)
        let w = want_in(&p, 1, false);
        assert_eq!((w["s:1"], w["s:2"], w["s:3"], w["s:4"]), (true, false, false, false));
        // s:1 in standby, s:3 taken by a room: s:3 (taken) and s:1 (live free one — spare) are needed
        heartbeat_in(&mut p, "x", "s:1", 1, "standby", 5, "");
        p[2].battle_id = Some("b".into());
        let w = want_in(&p, 1, false);
        assert_eq!((w["s:1"], w["s:2"], w["s:3"], w["s:4"]), (true, false, true, false));
        // players waiting in the queue — one more (cold start)
        let w = want_in(&p, 1, true);
        assert_eq!((w["s:1"], w["s:2"], w["s:3"], w["s:4"]), (true, true, true, false));
    }

    #[test]
    fn standby_frees_slot_after_exit() {
        let mut p = vec![Slot::new("s:1", None)];
        heartbeat_in(&mut p, "x", "s:1", 1, "standby", 5, "");
        assert_eq!(allocate_in(&mut p, "b1", "/m").as_deref(), Some("s:1"));
        release_in(&mut p, "b1");
        assert_eq!(heartbeat_in(&mut p, "x", "s:1", 1, "exiting", 5, "")["action"], "none", "exiting — not reset");
        assert_eq!(allocate_in(&mut p, "b2", "/m"), None, "taken until the process is restarted");
        heartbeat_in(&mut p, "x", "s:1", 1, "standby", 6, "");
        assert_eq!(allocate_in(&mut p, "b2", "/m").as_deref(), Some("s:1"), "new process in standby — free");
    }

    #[test]
    fn orphan_battle_after_backend_restart() {
        let mut p = vec![Slot::new("127.0.0.8:7787", None)];
        // backend restarted, the server is still in the previous room's match — not free
        assert_eq!(heartbeat_in(&mut p, "10.0.0.1", "127.0.0.8:7787", 7787, "battle", 5, "old")["action"], "none");
        assert_eq!(p[0].state(), "orphan");
        assert_eq!(allocate_in(&mut p, "n1", "/m"), None, "no room in the middle of a foreign match");
        // the match lasts longer than the room deadline — reset; after ready with a new epoch — free
        p[0].orphan_since = Instant::now().checked_sub(slot_ttl() + Duration::from_secs(1));
        assert_eq!(heartbeat_in(&mut p, "10.0.0.1", "127.0.0.8:7787", 7787, "battle", 5, "old")["action"], "reset");
        heartbeat_in(&mut p, "10.0.0.1", "127.0.0.8:7787", 7787, "ready", 6, "");
        assert_eq!(allocate_in(&mut p, "n1", "/m").as_deref(), Some("127.0.0.8:7787"));

        // waiting hall of a foreign room — reset immediately, no deadline
        let mut q = vec![Slot::new("127.0.0.8:7797", None)];
        assert_eq!(heartbeat_in(&mut q, "10.0.0.1", "127.0.0.8:7797", 7797, "waiting", 5, "old")["action"], "reset");
    }
}
