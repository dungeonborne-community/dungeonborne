//! Identity of relay connections. The relay connects to the game's own ports from 127.0.0.1:<ephemeral port> and, before the first
//! byte, registers that address → account. The gateway (0x1001), DirServer and the game's HTTPS login look up their client's address here:
//! a record exists: a player via the launcher; none: a direct connection (with `require = "relay"` → denied, except our own
//! game servers: they enter the lobby themselves, without the launcher; IPs from `DS_POOL` and `DS_TRUSTED_IPS`).
//! Each record has a `kill` signal: `kick(open_id)` drops all of the player's connections (ban, `auth::ban`).

use super::accounts::Account;
use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::Notify;

struct Entry {
    acc: Arc<Account>,
    kill: Arc<Notify>,
}

fn map() -> &'static Mutex<HashMap<SocketAddr, Entry>> {
    static M: OnceLock<Mutex<HashMap<SocketAddr, Entry>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a connection; returns the signal on which the relay drops it
pub fn insert(local: SocketAddr, acc: Arc<Account>) -> Arc<Notify> {
    let kill = Arc::new(Notify::new());
    map().lock().unwrap().insert(local, Entry { acc, kill: kill.clone() });
    kill
}

pub fn remove(local: &SocketAddr) {
    map().lock().unwrap().remove(local);
}

/// Account of a connection by client address, as the listener sees it
pub fn lookup(peer: &SocketAddr) -> Option<Arc<Account>> {
    map().lock().unwrap().get(peer).map(|e| e.acc.clone())
}

/// Drop all relay connections of this player; returns how many were dropped
pub fn kick(open_id: &str) -> usize {
    let m = map().lock().unwrap();
    let mut n = 0;
    for e in m.values().filter(|e| e.acc.open_id == open_id) {
        // notify_one remembers the signal even if the relay has not reached the wait yet
        e.kill.notify_one();
        n += 1;
    }
    n
}

/// IPs allowed into the lobby directly: pool servers (`DS_POOL`) + `DS_TRUSTED_IPS`, comma-separated
/// (the server address as the backend sees it, if it differs from the public one: private network, NAT)
fn trusted_ds() -> &'static HashSet<IpAddr> {
    static S: OnceLock<HashSet<IpAddr>> = OnceLock::new();
    S.get_or_init(|| {
        let extra = std::env::var("DS_TRUSTED_IPS").unwrap_or_default();
        let set: HashSet<IpAddr> = crate::lobby::allocator::pool_ips()
            .into_iter()
            .chain(extra.split(',').filter_map(|s| s.trim().parse().ok()))
            .collect();
        if super::require_relay() {
            println!("[Auth] without relay, only game servers are let into the lobby: {:?}", set);
        }
        set
    })
}

fn admit_with(found: Option<Arc<Account>>, peer: &SocketAddr, strict: bool, trusted: &HashSet<IpAddr>) -> Result<Option<Arc<Account>>, ()> {
    match found {
        Some(a) => Ok(Some(a)),
        None if strict && !trusted.contains(&peer.ip().to_canonical()) => Err(()),
        None => Ok(None),
    }
}

/// Direct connection of our own game server: strict mode (`require = "relay"`) and IP from `DS_POOL`/`DS_TRUSTED_IPS`.
/// Outside strict mode always false (locally players connect directly too)
pub fn is_game_server(ip: IpAddr) -> bool {
    super::require_relay() && trusted_ds().contains(&ip.to_canonical())
}

/// Decision for a game listener: Ok(Some) = player via relay, Ok(None) = direct and allowed
/// (not strict mode or our own game server), Err = deny
pub fn admit(peer: &SocketAddr) -> Result<Option<Arc<Account>>, ()> {
    admit_with(lookup(peer), peer, super::require_relay(), trusted_ds())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_mode_lets_only_game_servers_in_directly() {
        let trusted: HashSet<IpAddr> = ["203.0.113.20".parse().unwrap()].into_iter().collect();
        let ds: SocketAddr = "203.0.113.20:50123".parse().unwrap();
        let ds_v6: SocketAddr = "[::ffff:203.0.113.20]:50123".parse().unwrap();
        let stranger: SocketAddr = "5.6.7.8:40000".parse().unwrap();
        assert!(matches!(admit_with(None, &ds, true, &trusted), Ok(None)), "game server: direct");
        assert!(matches!(admit_with(None, &ds_v6, true, &trusted), Ok(None)), "same IP via an IPv6 address");
        assert!(admit_with(None, &stranger, true, &trusted).is_err(), "stranger without launcher: denied");
        assert!(matches!(admit_with(None, &stranger, false, &trusted), Ok(None)), "not strict mode: as before");
    }
}
