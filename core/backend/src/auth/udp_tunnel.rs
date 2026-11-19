//! Battle UDP tunnel through the launcher relay: only for players whose direct UDP to the game server gets cut along the way
//! (06.11: with some Russian ISPs every flow is cut after the first ~11 packets on any port). Everyone else plays
//! directly and loses nothing in ping.
//!
//! If the tunnel is on and the launcher supports it (`caps` in the relay preface), the client gets the launcher address `LOCAL`; the launcher
//! wraps datagrams into `[length u16 LE][data]` frames over the relay TLS, and the relay sends them to the room's game server.
//!
//! Two parties turn it on: the player from the launcher (`/api/launcher/v1/net`; the launcher detects drops from the relay log itself and asks
//! the player) and a GM with `udp on|off`. 09.11 (user's decision): the server does not turn the tunnel on by itself on `network_block_loading`,
//! it only releases the player from the room. The last choice and its reason are in the table
//! `udp_tunnel` (key: open_id): complaints are investigated with it, and on game login it takes precedence over the old `state.udp_tunnel`.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// Choice table
pub const TABLE: &str = "udp_tunnel";
/// Reason from the launcher is foreign text: length is limited
const MAX_REASON: usize = 300;

/// Player's last choice: who turned the tunnel on or off and why
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Choice {
    pub on: bool,
    /// `server` (by itself, on network_block_loading), `gm`, `launcher`
    pub source: String,
    pub reason: String,
    pub at: i64,
}

pub(crate) fn clean_reason(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_REASON).collect::<String>().trim().to_string()
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Turn on or off and record who and why: in the log and in the table
pub async fn choose(db: &crate::db::GameDb, open_id: &str, on: bool, source: &str, reason: &str) -> Choice {
    set_on(open_id, on);
    let c = Choice { on, source: source.to_string(), reason: clean_reason(reason), at: now() };
    db.save(TABLE, open_id, &c).await;
    println!(
        "[Net] UDP tunnel {} for {} ({}): {}",
        if on { "on" } else { "off" },
        open_id,
        source,
        if c.reason.is_empty() { "no reason" } else { &c.reason }
    );
    c
}

/// Game login: the choice from the table, and if there is none, the player's old flag
pub async fn restore(db: &crate::db::GameDb, open_id: &str, legacy_on: bool) -> bool {
    let on = db.get::<Choice>(TABLE, open_id).await.map_or(legacy_on, |c| c.on);
    set_on(open_id, on);
    on
}

/// Address the client gets: the launcher's local port
pub const LOCAL: &str = "127.0.0.1:27777";
/// A UDP datagram is never larger; anything longer is a frame error
pub const MAX_FRAME: usize = 2048;
/// Launcher capability in the preface `caps`
pub const CAP: &str = "udp";

#[derive(Default)]
struct State {
    capable: HashSet<String>,
    on: HashSet<String>,
}

fn state() -> &'static Mutex<State> {
    static S: OnceLock<Mutex<State>> = OnceLock::new();
    S.get_or_init(Default::default)
}

/// The player's launcher sent a preface with `caps`
pub fn set_capable(open_id: &str, capable: bool) {
    let mut s = state().lock().unwrap();
    if capable {
        s.capable.insert(open_id.to_string());
    } else {
        s.capable.remove(open_id);
    }
}

pub fn capable(open_id: &str) -> bool {
    state().lock().unwrap().capable.contains(open_id)
}

pub fn set_on(open_id: &str, on: bool) {
    let mut s = state().lock().unwrap();
    if on {
        s.on.insert(open_id.to_string());
    } else {
        s.on.remove(open_id);
    }
}

pub fn is_on(open_id: &str) -> bool {
    state().lock().unwrap().on.contains(open_id)
}

/// The tunnel really works: on and the launcher supports it (an old launcher will not bring the tunnel up, so the address is not changed)
pub fn active(open_id: &str) -> bool {
    let s = state().lock().unwrap();
    s.on.contains(open_id) && s.capable.contains(open_id)
}

/// Game server address for this client
pub fn client_ds(open_id: &str, real: &str) -> String {
    if active(open_id) {
        LOCAL.to_string()
    } else {
        real.to_string()
    }
}

/// The client's loading complaint looks like a direct UDP cut: server alive, direct connection, launcher supports the tunnel, it is off
pub fn direct_load_failed(open_id: &str, reported_addr: &str, server_alive: bool, in_this_room: bool) -> bool {
    in_this_room && server_alive && reported_addr != LOCAL && capable(open_id) && !is_on(open_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_problem_players_get_the_tunnel() {
        let (a, real) = ("udp_t_a", "1.2.3.4:7777");
        assert_eq!(client_ds(a, real), real, "direct by default");
        set_on(a, true);
        assert_eq!(client_ds(a, real), real, "an old launcher will not bring the tunnel up");
        set_capable(a, true);
        assert_eq!(client_ds(a, real), LOCAL);
        set_on(a, false);
        assert_eq!(client_ds(a, real), real, "off");
    }

    #[test]
    fn reasons_are_trimmed() {
        assert_eq!(clean_reason("  drops 443\n×5  "), "drops 443×5");
        assert_eq!(clean_reason(&"я".repeat(400)).chars().count(), MAX_REASON);
    }

    #[test]
    fn enable_rules() {
        let a = "udp_t_b";
        assert!(!direct_load_failed(a, "1.2.3.4:7777", true, true), "launcher does not support it");
        set_capable(a, true);
        assert!(direct_load_failed(a, "1.2.3.4:7777", true, true));
        assert!(!direct_load_failed(a, "1.2.3.4:7777", false, true), "server is dead: not the player's network");
        assert!(!direct_load_failed(a, "1.2.3.4:7777", true, false), "another room");
        assert!(!direct_load_failed(a, LOCAL, true, true), "the tunnel did not help already");
        set_on(a, true);
        assert!(!direct_load_failed(a, "1.2.3.4:7777", true, true), "already on");
    }
}
