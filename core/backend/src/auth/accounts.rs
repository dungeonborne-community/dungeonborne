//! Accounts: (provider, subject) → account number n → the game's open_id and user_id.
//! open_id = OPEN_ID_BASE + n (17 digits, SteamID64 format: the client and the emulator need exactly that), user_id = USER_ID_BASE + n.
//! Both values are computed from n, so open_id is enough for the gateway to find user_id (`user_id_for_open_id`).
//! USER_ID_BASE is above the old user_ids (10 000 000 + 6 SteamID digits, FIX_LIST C4) and keeps role_guid = user_id·100 + k
//! within 32 bits up to ~10 million accounts.

use super::provider::Identity;
use crate::db::GameDb;
use serde::{Deserialize, Serialize};

pub const OPEN_ID_BASE: u64 = 76_561_197_960_265_728;
pub const USER_ID_BASE: u64 = 11_000_000;
/// limit of n: user_id·100 must stay < i32::MAX (role_guid)
pub const MAX_ACCOUNTS: u64 = 10_000_000;

const TABLE: &str = "accounts";
const COUNTER_TABLE: &str = "counters";
const COUNTER_ID: &str = "account";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Account {
    pub provider: String,
    pub subject: String,
    pub n: u64,
    pub open_id: String,
    pub user_id: u64,
    pub display_name: String,
    /// login at the provider (Discord username), updated on login; 12.11: lookup in /ban
    #[serde(default)]
    pub login: String,
    pub avatar_url: Option<String>,
    /// roles from the provider + `player`; recomputed on every login
    pub roles: Vec<String>,
    /// ban on our side (admin), independent of the provider; revoking sessions and kicking from the game: `auth::ban`
    pub banned: bool,
    #[serde(default)]
    pub ban_reason: String,
    #[serde(default)]
    pub banned_by: String,
    #[serde(default)]
    pub banned_at: i64,
    pub created_at: i64,
    pub last_login: i64,
}

#[derive(Serialize, Deserialize, Clone)]
struct Counter {
    value: u64,
}

/// record id: `<provider>_<subject>`, everything except [A-Za-z0-9] becomes `_xx` (byte hex), so different subjects do not collide
pub fn key(provider: &str, subject: &str) -> String {
    let mut safe = String::with_capacity(subject.len());
    for b in subject.bytes() {
        if b.is_ascii_alphanumeric() {
            safe.push(b as char);
        } else {
            safe.push_str(&format!("_{:02x}", b));
        }
    }
    format!("{}_{}", provider, safe)
}

pub fn open_id_for(n: u64) -> String {
    (OPEN_ID_BASE + n).to_string()
}

/// account's user_id by open_id; None if the open_id is not in the accounts range (old save, local test)
pub fn user_id_for_open_id(open_id: &str) -> Option<u64> {
    let n = open_id.parse::<u64>().ok()?.checked_sub(OPEN_ID_BASE)?;
    (1..=MAX_ACCOUNTS).contains(&n).then_some(USER_ID_BASE + n)
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

pub async fn get(db: &GameDb, key: &str) -> Option<Account> {
    db.get::<Account>(TABLE, key).await
}

pub async fn save(db: &GameDb, acc: &Account) {
    db.save(TABLE, &key(&acc.provider, &acc.subject), acc).await;
}

/// Account by open_id (the gateway only knows the player's open_id)
pub async fn by_open_id(db: &GameDb, open_id: &str) -> Option<Account> {
    let mut r = db.db.query("SELECT * FROM accounts WHERE open_id = $o LIMIT 1").bind(("o", open_id.to_string())).await.ok()?;
    r.take::<Vec<Account>>(0).ok()?.into_iter().next()
}

/// Right to GM commands: the account has role `gm` or `admin`; a player without an account only on a server without login
/// (`[auth] require = "none"`, local tests).
pub async fn may_gm(db: &GameDb, open_id: &str) -> bool {
    match by_open_id(db, open_id).await {
        Some(a) => !a.banned && a.roles.iter().any(|r| r == "gm" || r == "admin"),
        None => !super::require_relay(),
    }
}

/// Right to battle (M5): `[auth] battle_roles` empty → everyone; otherwise one of the account's roles or `gm`/`admin`.
/// A player without an account only on a server without login (`require = "none"`). Discord roles are from the last login (K9).
pub async fn may_battle(db: &GameDb, open_id: &str) -> bool {
    let (need, _) = super::battle_policy();
    if need.is_empty() {
        return true;
    }
    match by_open_id(db, open_id).await {
        Some(a) => battle_allowed(&a, need),
        None => !super::require_relay(),
    }
}

fn battle_allowed(a: &Account, need: &[String]) -> bool {
    !a.banned && a.roles.iter().any(|r| r == "gm" || r == "admin" || need.contains(r))
}

/// Login whitelist (`[auth] login_roles`): empty → let in everyone who is not banned
pub fn login_allowed(a: &Account) -> bool {
    let (need, _) = super::login_policy();
    need.is_empty() || battle_allowed(a, need)
}

/// Number for a new account. Accounts are created rarely: a counter in the DB under a process lock.
async fn next_n(db: &GameDb) -> u64 {
    static LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _g = LOCK.lock().await;
    let n = db.get::<Counter>(COUNTER_TABLE, COUNTER_ID).await.map(|c| c.value).unwrap_or(0) + 1;
    db.save(COUNTER_TABLE, COUNTER_ID, &Counter { value: n }).await;
    n
}

/// Login via a provider: find or create the account, update name/avatar/roles
pub async fn upsert(db: &GameDb, provider: &str, id: &Identity) -> Account {
    let k = key(provider, &id.subject);
    let mut roles = vec!["player".to_string()];
    roles.extend(id.roles.iter().filter(|r| r.as_str() != "player").cloned());
    let acc = match get(db, &k).await {
        Some(mut a) => {
            a.display_name = id.display_name.clone();
            a.login = id.login.clone();
            a.avatar_url = id.avatar_url.clone();
            a.roles = roles;
            a.last_login = now();
            a
        }
        None => {
            let n = next_n(db).await;
            if n > MAX_ACCOUNTS {
                eprintln!("[Auth] account number {} is greater than {}: role_guid will not fit in 32 bits", n, MAX_ACCOUNTS);
            }
            let a = Account {
                provider: provider.to_string(),
                subject: id.subject.clone(),
                n,
                open_id: open_id_for(n),
                user_id: USER_ID_BASE + n,
                display_name: id.display_name.clone(),
                login: id.login.clone(),
                avatar_url: id.avatar_url.clone(),
                roles,
                banned: false,
                ban_reason: String::new(),
                banned_by: String::new(),
                banned_at: 0,
                created_at: now(),
                last_login: now(),
            };
            println!("[Auth] new account {} '{}' → open_id {}, user_id {}", k, a.display_name, a.open_id, a.user_id);
            a
        }
    };
    save(db, &acc).await;
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_id_maps_back_to_user_id() {
        assert_eq!(open_id_for(1), "76561197960265729");
        assert_eq!(user_id_for_open_id(&open_id_for(1)), Some(11_000_001));
        assert_eq!(user_id_for_open_id(&open_id_for(MAX_ACCOUNTS)), Some(USER_ID_BASE + MAX_ACCOUNTS));
        // a real SteamID (Goldberg) is not an account
        assert_eq!(user_id_for_open_id("76561198000000902"), None);
        assert_eq!(user_id_for_open_id("abc"), None);
        assert!((USER_ID_BASE + MAX_ACCOUNTS) * 100 + 99 < i32::MAX as u64);
    }

    #[test]
    fn battle_roles() {
        let mut a = Account {
            provider: "dev".into(), subject: "x".into(), n: 1, open_id: open_id_for(1), user_id: USER_ID_BASE + 1,
            display_name: "x".into(), login: "x".into(), avatar_url: None, roles: vec!["player".into()], banned: false,
            ban_reason: String::new(), banned_by: String::new(), banned_at: 0, created_at: 0, last_login: 0,
        };
        let need = vec!["tester".to_string()];
        assert!(!battle_allowed(&a, &need), "only player: no");
        a.roles.push("tester".into());
        assert!(battle_allowed(&a, &need));
        a.roles = vec!["player".into(), "gm".into()];
        assert!(battle_allowed(&a, &need), "gm: always");
        a.banned = true;
        assert!(!battle_allowed(&a, &need));
    }

    #[test]
    fn key_is_safe() {
        assert_ne!(key("dev", "Вася"), key("dev", "Петя"));
        assert_eq!(key("dev", "a b"), "dev_a_20b");
        assert_eq!(key("discord", "80351110224678912"), "discord_80351110224678912");
    }
}
