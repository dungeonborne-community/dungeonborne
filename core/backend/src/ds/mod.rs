//! Backend DS layer (docs/DS_PIPELINE.md, T1).
//!
//! TGF protocol: `tgf.DsRpcService` lives on the game server and is called by the backend (init_player, kickout…),
//! `tgf.DsBattleService` lives in the backend and is called by the server (player_game_over, ds_end…; incoming handlers —
//! `rpc/handlers/ds_handlers.rs`). The native transport (Tencent DS SDK / DSA agent) is not available to us, so next to
//! the server runs ds-agent, which talks to the backend over HTTP with the same proto messages:
//!   - `agent_link` — HTTP for ds-agent: registration, backend → DS command queue, replies, DS → backend events;
//!   - `rpc_client` — outgoing DsRpcService calls through the agent queue;
//!   - `driver`     — background pass: sends init_player to rooms in DsInit and moves them to DsReady.

pub mod agent_link;
pub mod driver;
pub mod rpc_client;
pub mod watchdog;

use crate::db::GameDb;
use crate::game_data::GameData;
use std::sync::Arc;

/// Starts the DS layer: HTTP for ds-agent + room driver. Called from main after tables and the DB are loaded.
pub fn spawn(db: GameDb, game_data: Arc<GameData>) {
    // the "Klotho" tab of the launcher lobby checks the player's run on sign-up (trial_ensure_open) — it needs the DB and tables
    crate::lobby::launcher_lobby::set_backend(db.clone(), game_data.clone());
    let (db2, gd2) = (db.clone(), game_data.clone());
    tokio::spawn(async move {
        if let Err(e) = agent_link::run(db2, gd2).await {
            println!("[DS-Link] failed to start: {}", e);
        }
    });
    tokio::spawn(driver::run(db, game_data));
    tokio::spawn(watchdog::run());
}

/// Proto bytes in the agent JSON are hex (no new dependencies; trivial for an agent in PowerShell/Rust).
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn from_hex(s: &str) -> Result<Vec<u8>, String> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return Err("odd hex length".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("hex: {}", e)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let data = vec![0u8, 1, 0x7f, 0x80, 0xff];
        assert_eq!(to_hex(&data), "00017f80ff");
        assert_eq!(from_hex("00017F80ff").unwrap(), data);
        assert!(from_hex("abc").is_err());
    }
}
