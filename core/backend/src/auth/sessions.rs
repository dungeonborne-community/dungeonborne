//! Launcher sessions and pending logins.
//! Token: 32 random bytes (hex); the DB stores only the token's sha256 (`auth_sessions:<sha256>`), so a DB leak does
//! not let anyone log in. A pending login (login_id → result) lives in memory for 5 minutes; the result is handed out on `login/poll`
//! once.

use super::accounts::{self, Account};
use super::provider::Denied;
use super::AuthCore;
use crate::db::GameDb;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TABLE: &str = "auth_sessions";
pub const LOGIN_TTL: Duration = Duration::from_secs(300);
/// limit of concurrent pending logins (protects memory from login/start spam)
const MAX_PENDING: usize = 2000;

#[derive(Serialize, Deserialize, Clone)]
struct Session {
    account_key: String,
    created_at: i64,
    expires_at: i64,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Current time, s (for queries to `auth_sessions` from other modules)
pub fn now_secs() -> i64 {
    now()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn random_hex(n: usize) -> String {
    let mut b = vec![0u8; n];
    rand::thread_rng().fill_bytes(&mut b);
    hex(&b)
}

fn token_id(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

/// New account session: (token, expires)
pub async fn create(db: &GameDb, account_key: &str, ttl_days: u32) -> (String, i64) {
    let token = random_hex(32);
    let expires_at = now() + ttl_days.max(1) as i64 * 86_400;
    db.save(TABLE, &token_id(&token), &Session { account_key: account_key.to_string(), created_at: now(), expires_at })
        .await;
    (token, expires_at)
}

/// Account by token: None if there is no such token or it expired. The caller checks the ban (`Account::banned`).
pub async fn resolve(db: &GameDb, token: &str) -> Option<Account> {
    if token.len() != 64 {
        return None;
    }
    let s = db.get::<Session>(TABLE, &token_id(token)).await?;
    if s.expires_at < now() {
        return None;
    }
    accounts::get(db, &s.account_key).await
}

pub async fn revoke(db: &GameDb, token: &str) {
    let _ = db.db.delete::<Option<Session>>((TABLE, token_id(token).as_str())).await;
}

// ---------------------------------------------------------------- pending logins

#[derive(Clone)]
pub enum LoginResult {
    Waiting,
    Ok { token: String, expires_at: i64, account: Account },
    Denied(Denied),
}

#[derive(Clone)]
pub struct Pending {
    pub provider: String,
    /// short code: the launcher and the "done" page show the same one, so the player sees it was really them who logged in
    pub user_code: String,
    pub created: Instant,
    pub result: LoginResult,
}

#[derive(Default)]
pub struct PendingLogins(Mutex<HashMap<String, Pending>>);

fn user_code() -> String {
    const ALPHA: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    let mut b = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut b);
    let s: String = b.iter().map(|x| ALPHA[*x as usize % ALPHA.len()] as char).collect();
    format!("{}-{}", &s[..3], &s[3..])
}

impl PendingLogins {
    /// New login: (login_id, user_code); None if too many are pending
    pub fn start(&self, provider: &str) -> Option<(String, String)> {
        let mut m = self.0.lock().unwrap();
        m.retain(|_, p| p.created.elapsed() < LOGIN_TTL);
        if m.len() >= MAX_PENDING {
            return None;
        }
        let (id, code) = (random_hex(16), user_code());
        m.insert(
            id.clone(),
            Pending { provider: provider.to_string(), user_code: code.clone(), created: Instant::now(), result: LoginResult::Waiting },
        );
        Some((id, code))
    }

    pub fn get(&self, login_id: &str) -> Option<Pending> {
        let m = self.0.lock().unwrap();
        m.get(login_id).filter(|p| p.created.elapsed() < LOGIN_TTL).cloned()
    }

    /// Store the result if the login is still waiting (a repeated callback does not overwrite)
    pub fn finish(&self, login_id: &str, result: LoginResult) -> bool {
        let mut m = self.0.lock().unwrap();
        match m.get_mut(login_id) {
            Some(p) if matches!(p.result, LoginResult::Waiting) && p.created.elapsed() < LOGIN_TTL => {
                p.result = result;
                true
            }
            _ => false,
        }
    }

    /// For poll: the result; a finished one (Ok/Denied) is removed, so the token is handed out once
    pub fn poll(&self, login_id: &str) -> Option<LoginResult> {
        let mut m = self.0.lock().unwrap();
        let p = m.get(login_id).filter(|p| p.created.elapsed() < LOGIN_TTL)?.clone();
        if !matches!(p.result, LoginResult::Waiting) {
            m.remove(login_id);
        }
        Some(p.result)
    }
}

/// Once a minute: expired pending logins and sessions
pub fn spawn_cleanup(core: Arc<AuthCore>) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            core.pending.0.lock().unwrap().retain(|_, p| p.created.elapsed() < LOGIN_TTL);
            let _ = core.db.db.query("DELETE auth_sessions WHERE expires_at < $now").bind(("now", now())).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_result_delivered_once() {
        let p = PendingLogins::default();
        let (id, code) = p.start("dev").unwrap();
        assert_eq!(code.len(), 7);
        assert!(matches!(p.poll(&id), Some(LoginResult::Waiting)));
        assert!(p.finish(&id, LoginResult::Denied(Denied::new("denied", "no"))));
        assert!(!p.finish(&id, LoginResult::Waiting), "second callback does not overwrite");
        assert!(matches!(p.poll(&id), Some(LoginResult::Denied(_))));
        assert!(p.poll(&id).is_none(), "result is handed out once");
    }

    #[test]
    fn token_id_is_hash() {
        let t = random_hex(32);
        assert_eq!(t.len(), 64);
        assert_ne!(token_id(&t), t);
    }
}
