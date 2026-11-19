//! Registry of connected players: open_id -> outgoing push channel of their TCP session.
//! The gateway registers the session after 0x1001 (open_id known) and removes it on disconnect.

use prost::Message;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use tokio::sync::mpsc::UnboundedSender;

pub struct Push {
    pub method: String,
    pub type_url: String,
    pub body: Vec<u8>,
}

struct Entry {
    conn_id: u64,
    tx: UnboundedSender<Push>,
}

static HUB: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
static NEXT_CONN: AtomicU64 = AtomicU64::new(1);

fn hub() -> &'static Mutex<HashMap<String, Entry>> {
    HUB.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Registers a session; a new session of the same player displaces the old one. Returns the id for `unregister`.
pub fn register(open_id: &str, tx: UnboundedSender<Push>) -> u64 {
    let conn_id = NEXT_CONN.fetch_add(1, Ordering::Relaxed);
    hub().lock().unwrap().insert(open_id.to_string(), Entry { conn_id, tx });
    conn_id
}

/// Removes the session only if it is still the current one (not displaced by a repeated login).
pub fn unregister(open_id: &str, conn_id: u64) {
    let mut map = hub().lock().unwrap();
    if map.get(open_id).map(|e| e.conn_id) == Some(conn_id) {
        map.remove(open_id);
    }
}

/// open_id of online players (admin API)
pub fn online_ids() -> Vec<String> {
    hub().lock().unwrap().keys().cloned().collect()
}

/// How many players are online now (server status)
pub fn online() -> usize {
    hub().lock().unwrap().len()
}

pub fn is_online(open_id: &str) -> bool {
    hub().lock().unwrap().contains_key(open_id)
}

/// Sh4f check (agent, 04.11): after a raid the client stalls receiving if the result pushes (dirty_role, quests,
/// battle_settle_sync) arrive while it is leaving the raid map. While deferral is on for a player, `push_to` accumulates pushes,
/// and `take_deferred` hands them out after the reply to the first player_get_info in the lobby. env RAID_DEFER_SETTLE=0 — disable.
struct Deferred {
    active: bool,
    since: std::time::Instant,
    pushes: Vec<Push>,
}

fn deferred() -> &'static Mutex<HashMap<String, Deferred>> {
    static D: OnceLock<Mutex<HashMap<String, Deferred>>> = OnceLock::new();
    D.get_or_init(Default::default)
}

/// 05.11: OFF by default. With deferral battle_settle_sync arrived after the lobby opened, and there was no results window
/// (the lobby checks for a result only when it opens; on 03.11, when the result was sent right on the raid map, the window was there). Enable —
/// RAID_DEFER_SETTLE=1 (Sh4f check).
pub fn defer_enabled() -> bool {
    std::env::var("RAID_DEFER_SETTLE").map(|v| v == "1").unwrap_or(false)
}

/// 05.11 experiment Sh4g: end of room (search CANCEL, room events 5–6, dirty_clear_matching/battle_room) — not on
/// the raid map, but after the client returns to the lobby. In both stuck raids on 05.11 (09:51, 10:12) these pushes arrived on
/// the raid map; on 03.11 15:36 (client did not hang) there was no CANCEL or dirty_clear_battle_room at all. RAID_DEFER_ROOM=1 — enable.
pub fn defer_room_enabled() -> bool {
    std::env::var("RAID_DEFER_ROOM").map(|v| v == "1").unwrap_or(false)
}

/// The player has deferred pushes (deferral is on or the accumulated ones are not handed out yet) — heartbeats send them nothing of their own
pub fn has_deferred(open_id: &str) -> bool {
    deferred().lock().unwrap().get(open_id).map_or(false, |e| e.active || !e.pushes.is_empty())
}

/// Start (true) or stop (false) accumulating pushes for a player; the accumulated ones wait for `take_deferred`
pub fn set_defer(open_id: &str, on: bool) {
    let mut d = deferred().lock().unwrap();
    d.retain(|_, x| x.since.elapsed() < std::time::Duration::from_secs(1800));
    let e = d.entry(open_id.to_string()).or_insert_with(|| Deferred { active: false, since: std::time::Instant::now(), pushes: Vec::new() });
    e.active = on;
    if on {
        e.since = std::time::Instant::now();
    }
}

/// A player's accumulated pushes (and turn deferral off)
pub fn take_deferred(open_id: &str) -> Vec<Push> {
    deferred().lock().unwrap().remove(open_id).map(|d| d.pushes).unwrap_or_default()
}

fn try_defer(open_id: &str, method: &str, type_url: &str, body: &[u8]) -> bool {
    let mut d = deferred().lock().unwrap();
    match d.get_mut(open_id) {
        // and after deferral ends, until the accumulated ones are handed out: otherwise a later push would overtake the deferred ones
        Some(e) if e.active || !e.pushes.is_empty() => {
            e.pushes.push(Push { method: method.to_string(), type_url: type_url.to_string(), body: body.to_vec() });
            true
        }
        _ => false,
    }
}

/// Sends a push to a player if they are online. false — the player is not online.
pub fn push_to<T: Message>(open_id: &str, method: &str, type_url: &str, msg: &T) -> bool {
    if try_defer(open_id, method, type_url, &msg.encode_to_vec()) {
        return true;
    }
    let map = hub().lock().unwrap();
    match map.get(open_id) {
        Some(e) => e
            .tx
            .send(Push { method: method.to_string(), type_url: type_url.to_string(), body: msg.encode_to_vec() })
            .is_ok(),
        None => false,
    }
}

/// Same, but the body is already encoded (forwarding pushes collected by GatewaySession outside the player's session — crate::ds::driver)
pub fn push_raw(open_id: &str, method: &str, type_url: &str, body: Vec<u8>) -> bool {
    let map = hub().lock().unwrap();
    match map.get(open_id) {
        Some(e) => e.tx.send(Push { method: method.to_string(), type_url: type_url.to_string(), body }).is_ok(),
        None => false,
    }
}

pub const DIRTY_METHOD: &str = "tgf.GameClientService.player_dirty_change";
pub const DIRTY_TYPE: &str = "tgf.SCPlayerDirtyChgSync";
pub const MATCHING_LOG_METHOD: &str = "tgf.GameClientService.battle_matching_log_sync";
pub const MATCHING_LOG_TYPE: &str = "tgf.SCBattleMatchingLogSync";

#[cfg(test)]
mod defer_tests {
    use super::*;

    #[test]
    fn deferred_pushes_wait_for_take() {
        let msg = crate::tgf::ScPlayerDirtyChgSync::default();
        set_defer("defer_a", true);
        assert!(push_to("defer_a", DIRTY_METHOD, DIRTY_TYPE, &msg), "offline but deferred — accepted");
        set_defer("defer_a", false);
        assert!(has_deferred("defer_a"));
        assert!(push_to("defer_a", DIRTY_METHOD, DIRTY_TYPE, &msg), "deferral off, but the accumulated ones not handed out — queued too");
        assert_eq!(take_deferred("defer_a").len(), 2);
        assert!(!has_deferred("defer_a"));
        assert!(!push_to("defer_a", DIRTY_METHOD, DIRTY_TYPE, &msg), "all handed out — normal send (offline)");
        assert!(take_deferred("defer_a").is_empty());
    }
}
