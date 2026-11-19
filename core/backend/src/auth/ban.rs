//! Ban via SurrealDB: works no matter who sets `accounts.banned = true` (GM command, admin SQL, another
//! backend replica with a shared DB, stage E6):
//! - `DEFINE EVENT` on `accounts`: on the transition banned → true the DB itself deletes all of the account's sessions (`auth_sessions`),
//!   so the launcher can no longer log in or open the relay with the old token;
//! - `LIVE SELECT … WHERE banned = true`: the backend gets a notification and drops the player's relay connections
//!   (`registry::kick`), so the player is thrown out of the game immediately, not on the next login.
//! The Discord ban role (`[auth.discord] banned_role`) is checked at login; the instant ban is this flag.

use super::accounts::{self, Account};
use super::registry;
use crate::db::GameDb;
use futures::StreamExt;
use serde_json::json;
use std::time::Duration;
use surrealdb::{Action, Notification};

const SCHEMA: &str = r#"
DEFINE INDEX OVERWRITE idx_accounts_open_id ON TABLE accounts COLUMNS open_id UNIQUE;
DEFINE INDEX OVERWRITE idx_sessions_account ON TABLE auth_sessions COLUMNS account_key;
DEFINE EVENT OVERWRITE ban_revokes_sessions ON TABLE accounts
    WHEN $event = "UPDATE" AND $after.banned = true AND $before.banned != true
    THEN { DELETE auth_sessions WHERE account_key = record::id($after.id); };
"#;

/// Indexes and the ban event (re-running is safe: OVERWRITE)
pub async fn setup(db: &GameDb) {
    match db.db.query(SCHEMA).await.and_then(|r| r.check()) {
        Ok(_) => println!("[Auth] accounts schema: indexes and ban event (DEFINE EVENT ban_revokes_sessions)"),
        Err(e) => eprintln!("[Auth] accounts schema: {}", e),
    }
}

/// Subscription to banned accounts: when one appears, drop the player's connections. Reconnects if the stream breaks.
pub fn spawn_watch(db: GameDb) {
    tokio::spawn(async move {
        loop {
            match db.db.query("LIVE SELECT * FROM accounts WHERE banned = true").await {
                Ok(mut r) => match r.stream::<Notification<Account>>(0) {
                    Ok(mut stream) => {
                        println!("[Auth] LIVE SELECT accounts WHERE banned = true — watching bans");
                        while let Some(n) = stream.next().await {
                            match n {
                                Ok(n) if matches!(n.action, Action::Create | Action::Update) && n.data.banned => {
                                    // 09.11: player in a match: "Hand of Justice" to the battle server (ban <open_id> in the /ds/slot response,
                                    // mod RuServer → native GA_TGF_Hero_Self_Destruct, killfeed "ban"); then the connection is dropped
                                    if let Some(battle) = crate::lobby::matchmaking::room_of_player(&n.data.open_id) {
                                        crate::ds::agent_link::queue_slot_cmd(&battle, &format!("ban {}", n.data.open_id));
                                        println!("[Auth] ban {}: in match {} — \"Hand of Justice\" to the battle server", n.data.open_id, battle);
                                    }
                                    let kicked = registry::kick(&n.data.open_id);
                                    println!(
                                        "[Auth] ban {} '{}' ({}): connections dropped {}",
                                        n.data.open_id, n.data.display_name, n.data.ban_reason, kicked
                                    );
                                }
                                Ok(_) => {}
                                Err(e) => eprintln!("[Auth] LIVE ban: {}", e),
                            }
                        }
                        eprintln!("[Auth] LIVE ban: stream closed, reconnecting");
                    }
                    Err(e) => eprintln!("[Auth] LIVE ban: {}", e),
                },
                Err(e) => eprintln!("[Auth] LIVE ban: {}", e),
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

/// Find the account to ban (12.11: previously only open_id and display_name, so `/ban Legend` and `/ban izao4ik` found nothing):
/// open_id, user_id, character name (any of the player's characters, case-insensitive, not only online), Discord name
/// (`display_name`: global_name or server nickname) and Discord login (`login`). Several matches: an error with the list.
async fn find(db: &GameDb, who: &str) -> Result<Account, String> {
    let w = who.trim().to_string();
    let low = w.to_lowercase();
    let uid: i64 = w.parse().unwrap_or(-1);
    let mut r = db
        .db
        .query(
            "SELECT * FROM accounts WHERE open_id = $w OR user_id = $uid OR string::lowercase(display_name) = $low              OR string::lowercase(login ?? '') = $low LIMIT 20",
        )
        .bind(("w", w.clone()))
        .bind(("uid", uid))
        .bind(("low", low.clone()))
        .await
        .map_err(|e| e.to_string())?;
    let mut found: Vec<Account> = r.take(0).map_err(|e| e.to_string())?;
    // character name: players who have a character with that name
    let mut by_char: Vec<(String, String)> = Vec::new(); // (open_id, character name)
    #[derive(serde::Deserialize)]
    struct Names {
        open_id: String,
        #[serde(default)]
        names: Vec<String>,
    }
    if let Ok(mut r) = db.db.query("SELECT open_id, roles.role_name AS names FROM players").await {
        let rows: Vec<Names> = r.take(0).unwrap_or_default();
        for row in rows {
            if let Some(n) = row.names.iter().find(|n| n.to_lowercase() == low) {
                by_char.push((row.open_id, n.clone()));
            }
        }
    }
    let have: std::collections::HashSet<String> = found.iter().map(|a| a.open_id.clone()).collect();
    let extra: Vec<String> = by_char.iter().map(|(o, _)| o.clone()).filter(|o| !have.contains(o)).collect();
    if !extra.is_empty() {
        let mut r = db
            .db
            .query("SELECT * FROM accounts WHERE open_id IN $ids")
            .bind(("ids", extra))
            .await
            .map_err(|e| e.to_string())?;
        let more: Vec<Account> = r.take(0).map_err(|e| e.to_string())?;
        found.extend(more);
    }
    match found.len() {
        0 => Err(format!("'{}' not found (open_id, user_id, character name, Discord name or login)", who)),
        1 => Ok(found.remove(0)),
        _ => Err(format!(
            "'{}': several accounts, specify open_id: {}",
            who,
            found
                .iter()
                .map(|a| {
                    let ch = by_char.iter().find(|(o, _)| *o == a.open_id).map(|(_, n)| format!(", character {}", n)).unwrap_or_default();
                    format!("{} ({} / {}{})", a.open_id, a.display_name, a.login, ch)
                })
                .collect::<Vec<_>>()
                .join("; ")
        )),
    }
}

/// Set or lift a ban. Sessions and connections are removed by the event and the LIVE subscription above.
pub async fn set(db: &GameDb, who: &str, banned: bool, reason: &str, by: &str) -> Result<Account, String> {
    let acc = find(db, who).await?;
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let patch = if banned {
        json!({ "banned": true, "ban_reason": reason, "banned_by": by, "banned_at": now })
    } else {
        json!({ "banned": false, "ban_reason": "", "banned_by": by, "banned_at": 0 })
    };
    db.db
        .query("UPDATE type::thing('accounts', $k) MERGE $p")
        .bind(("k", accounts::key(&acc.provider, &acc.subject)))
        .bind(("p", patch))
        .await
        .and_then(|r| r.check())
        .map_err(|e| e.to_string())?;
    accounts::get(db, &accounts::key(&acc.provider, &acc.subject)).await.ok_or_else(|| "account vanished after UPDATE".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::provider::Identity;
    use crate::auth::sessions;
    use std::sync::Arc;

    /// Real SurrealKV in a temp folder: the event removes sessions, LIVE drops the connection, unban
    #[tokio::test]
    async fn ban_revokes_sessions_and_kicks() {
        let dir = format!("target/test_ban_db_{}", sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        setup(&db).await;
        spawn_watch(db.clone());

        let id = Identity { subject: "42".into(), display_name: "Злодей".into(), login: "villain42".into(), avatar_url: None, roles: vec![] };
        let acc = accounts::upsert(&db, "dev", &id).await;
        let (token, _) = sessions::create(&db, &accounts::key("dev", "42"), 7).await;
        assert!(sessions::resolve(&db, &token).await.is_some());

        // this player's relay connection
        let addr: std::net::SocketAddr = "127.0.0.1:1".parse().unwrap();
        let kill = registry::insert(addr, Arc::new(acc.clone()));
        tokio::time::sleep(Duration::from_millis(300)).await; // LIVE subscription is up

        let banned = set(&db, "злодей", true, "cheats", "admin").await.unwrap();
        assert!(banned.banned && banned.ban_reason == "cheats");
        assert!(sessions::resolve(&db, &token).await.is_none(), "DEFINE EVENT deleted the session");
        tokio::time::timeout(Duration::from_secs(5), kill.notified()).await.expect("LIVE SELECT dropped the connection");
        registry::remove(&addr);

        let un = set(&db, &acc.open_id, false, "", "admin").await.unwrap();
        assert!(!un.banned);
        // 12.11: Discord login, user_id and character name
        assert!(set(&db, "VILLAIN42", true, "", "admin").await.unwrap().banned, "Discord login");
        assert!(!set(&db, &acc.user_id.to_string(), false, "", "admin").await.unwrap().banned, "user_id");
        let mut p = crate::structures::PlayerModel::new(&acc.open_id, acc.user_id);
        p.roles = vec![crate::structures::RoleModel {
            role_guid: 7,
            ord: 0,
            type_id: 1,
            role_name: "Legend".into(),
            role_level: 1,
            data: Default::default(),
        }];
        db.save("players", &acc.open_id, &p).await;
        assert!(set(&db, "legend", true, "", "admin").await.unwrap().banned, "character name");
        assert!(set(&db, "no-such-one", true, "", "admin").await.is_err());
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
