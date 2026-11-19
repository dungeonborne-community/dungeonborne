//! Discord role watcher (15.11, user: "he has the banned role, he must not be able to log in… close the holes").
//!
//! A launcher session lives `[auth] session_ttl_days` (7 days), while Discord roles were checked only at login via
//! Discord: a player who got the ban role after logging in kept entering with the old session for a week, and the relay connection stayed up
//! for days. Every `[auth.discord] recheck_secs` the watcher asks Discord with the community bot token
//! (`GET /guilds/{guild}/members/{user}`, no privileged intent needed) about every account with a live session:
//! - role `banned_role` → a ban on our side (`ban::set`): the DB event revokes sessions, the LIVE subscription kicks
//!   the player immediately (in a match: "Hand of Justice");
//! - not a server member with `require_guild` → sessions revoked, connections dropped; logging in again only goes through
//!   Discord, which denies with `not_in_guild`;
//! - our roles from `[auth.discord.roles]` are recomputed as at login: gm/admin/whitelist removed in Discord
//!   stop working.
//!
//! Discord errors (network, 5xx, 429 after a retry, unknown response): the account is left alone, retry on the next pass.
//! Protection against a misconfiguration (bot not on the server, wrong guild_id): if "not a member" is more than half of those checked
//! (and more than 3), sessions are not revoked in this pass, only an alert in the log.

use super::accounts::{self, Account};
use super::{ban, registry};
use crate::db::GameDb;
use crate::server_toml::DiscordCfg;
use serde::Deserialize;
use std::time::Duration;

const API: &str = "https://discord.com/api/v10";
/// pause between requests to Discord (the per-route limit is a few requests per second)
const STEP: Duration = Duration::from_millis(300);
/// Discord error codes "unknown member / user": means not on the server
const UNKNOWN_MEMBER: i64 = 10007;
const UNKNOWN_USER: i64 = 10013;

#[derive(Deserialize)]
pub(super) struct Member {
    #[serde(default)]
    pub roles: Vec<String>,
}

/// Discord's answer about a member
pub(super) enum Lookup {
    Member(Member),
    NotMember,
    Error(String),
}

/// What to do with the account
#[derive(Debug, PartialEq)]
pub(super) enum Verdict {
    Keep,
    Ban,
    Revoke,
    /// new our roles (with `player`), if they differ from the stored ones
    Roles(Vec<String>),
}

/// Decision from Discord's answer: the same as at login (`discord.rs`)
pub(super) fn verdict(cfg: &DiscordCfg, acc: &Account, lookup: &Lookup) -> Verdict {
    match lookup {
        Lookup::Error(_) => Verdict::Keep,
        Lookup::NotMember if cfg.require_guild => Verdict::Revoke,
        Lookup::NotMember => Verdict::Keep,
        Lookup::Member(m) => {
            if !cfg.banned_role.is_empty() && m.roles.contains(&cfg.banned_role) {
                return Verdict::Ban;
            }
            let mut roles = vec!["player".to_string()];
            let mut mapped: Vec<String> = m.roles.iter().filter_map(|r| cfg.roles.get(r).cloned()).filter(|r| r != "player").collect();
            mapped.sort();
            mapped.dedup();
            roles.extend(mapped);
            let mut have = acc.roles.clone();
            have.sort();
            let mut want = roles.clone();
            want.sort();
            if have == want {
                Verdict::Keep
            } else {
                Verdict::Roles(roles)
            }
        }
    }
}

struct Watch {
    cfg: DiscordCfg,
    token: String,
    http: reqwest::Client,
}

impl Watch {
    async fn lookup(&self, user_id: &str) -> Lookup {
        let url = format!("{}/guilds/{}/members/{}", API, self.cfg.guild_id, user_id);
        for attempt in 0..2 {
            let rsp = match self.http.get(&url).header("Authorization", format!("Bot {}", self.token)).send().await {
                Ok(r) => r,
                Err(e) => return Lookup::Error(e.to_string()),
            };
            let status = rsp.status();
            if status.is_success() {
                return match rsp.json::<Member>().await {
                    Ok(m) => Lookup::Member(m),
                    Err(e) => Lookup::Error(e.to_string()),
                };
            }
            let body: serde_json::Value = rsp.json().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt == 0 {
                let wait = body["retry_after"].as_f64().unwrap_or(1.0).clamp(0.1, 30.0);
                tokio::time::sleep(Duration::from_secs_f64(wait)).await;
                continue;
            }
            let code = body["code"].as_i64().unwrap_or(0);
            if status == reqwest::StatusCode::NOT_FOUND && (code == UNKNOWN_MEMBER || code == UNKNOWN_USER) {
                return Lookup::NotMember;
            }
            return Lookup::Error(format!("HTTP {} code {}", status, code));
        }
        Lookup::Error("429".into())
    }

    /// One pass: Discord accounts with a live session
    async fn sweep(&self, db: &GameDb) {
        let keys = match active_account_keys(db).await {
            Ok(k) => k,
            Err(e) => {
                eprintln!("[DiscordWatch] sessions: {}", e);
                return;
            }
        };
        let mut checked = Vec::new();
        for k in keys {
            let Some(acc) = accounts::get(db, &k).await else { continue };
            if acc.provider != "discord" || acc.banned {
                continue;
            }
            let lookup = self.lookup(&acc.subject).await;
            if let Lookup::Error(e) = &lookup {
                eprintln!("[DiscordWatch] {} '{}': Discord did not respond ({}) — no changes", acc.open_id, acc.display_name, e);
            }
            let v = verdict(&self.cfg, &acc, &lookup);
            checked.push((k, acc, v));
            tokio::time::sleep(STEP).await;
        }
        let revokes = checked.iter().filter(|c| c.2 == Verdict::Revoke).count();
        let mass = revokes > 3 && revokes * 2 > checked.len();
        if mass {
            eprintln!(
                "[DiscordWatch] ALERT: not on the Discord server {} of {} — looks like a misconfiguration (bot not on the server, guild_id); not revoking sessions",
                revokes,
                checked.len()
            );
        }
        let (mut bans, mut revoked, mut changed) = (0, 0, 0);
        for (k, acc, v) in checked {
            match v {
                Verdict::Keep => {}
                Verdict::Ban => match ban::set(db, &acc.open_id, true, "Discord: ban role", "discord").await {
                    Ok(_) => {
                        bans += 1;
                        println!("[DiscordWatch] {} '{}': ban role in Discord — banned on the server", acc.open_id, acc.display_name);
                    }
                    Err(e) => eprintln!("[DiscordWatch] {} '{}': ban not set: {}", acc.open_id, acc.display_name, e),
                },
                Verdict::Revoke if mass => {}
                Verdict::Revoke => {
                    let _ = db.db.query("DELETE auth_sessions WHERE account_key = $k").bind(("k", k.clone())).await;
                    let kicked = registry::kick(&acc.open_id);
                    revoked += 1;
                    println!(
                        "[DiscordWatch] {} '{}': not on the Discord server — sessions revoked, connections dropped {}",
                        acc.open_id, acc.display_name, kicked
                    );
                }
                Verdict::Roles(roles) => {
                    // re-read: a login or a ban could have arrived between the request and the write
                    if let Some(mut fresh) = accounts::get(db, &k).await {
                        println!("[DiscordWatch] {} '{}': roles {:?} → {:?}", fresh.open_id, fresh.display_name, fresh.roles, roles);
                        fresh.roles = roles;
                        accounts::save(db, &fresh).await;
                        changed += 1;
                    }
                }
            }
        }
        if bans + revoked + changed > 0 {
            println!("[DiscordWatch] pass: bans {}, sessions revoked for {}, roles updated for {}", bans, revoked, changed);
        }
    }
}

/// Keys of accounts with an unexpired session
async fn active_account_keys(db: &GameDb) -> Result<Vec<String>, String> {
    #[derive(Deserialize)]
    struct Row {
        account_key: String,
    }
    let mut r = db
        .db
        .query("SELECT account_key FROM auth_sessions WHERE expires_at > $now GROUP BY account_key")
        .bind(("now", super::sessions::now_secs()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<Row> = r.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| r.account_key).collect())
}

/// Starting the watcher: needs guild_id and the bot token in env `bot_token_env`; otherwise only a warning
pub fn spawn(db: GameDb, cfg: DiscordCfg) {
    if cfg.guild_id.is_empty() {
        return;
    }
    let token = std::env::var(&cfg.bot_token_env).unwrap_or_default();
    if token.trim().is_empty() {
        eprintln!(
            "[DiscordWatch] disabled: no env {} — Discord roles (ban, server membership) are checked only at login",
            cfg.bot_token_env
        );
        return;
    }
    let http = match reqwest::Client::builder().timeout(Duration::from_secs(10)).build() {
        Ok(h) => h,
        Err(e) => {
            eprintln!("[DiscordWatch] disabled: {}", e);
            return;
        }
    };
    let every = Duration::from_secs(cfg.recheck_secs.max(60));
    println!("[DiscordWatch] Discord roles of live sessions every {} s (guild {})", every.as_secs(), cfg.guild_id);
    let w = Watch { cfg, token: token.trim().to_string(), http };
    tokio::spawn(async move {
        // first pass right after start (let the DB and schema come up)
        tokio::time::sleep(Duration::from_secs(20)).await;
        loop {
            w.sweep(&db).await;
            tokio::time::sleep(every).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn cfg() -> DiscordCfg {
        DiscordCfg {
            guild_id: "1".into(),
            require_guild: true,
            banned_role: "666".into(),
            roles: HashMap::from([("10".to_string(), "gm".to_string()), ("11".to_string(), "whitelist".to_string())]),
            ..Default::default()
        }
    }

    fn acc(roles: &[&str]) -> Account {
        Account {
            provider: "discord".into(),
            subject: "42".into(),
            n: 1,
            open_id: accounts::open_id_for(1),
            user_id: accounts::USER_ID_BASE + 1,
            display_name: "x".into(),
            login: "x".into(),
            avatar_url: None,
            roles: roles.iter().map(|s| s.to_string()).collect(),
            banned: false,
            ban_reason: String::new(),
            banned_by: String::new(),
            banned_at: 0,
            created_at: 0,
            last_login: 0,
        }
    }

    fn member(roles: &[&str]) -> Lookup {
        Lookup::Member(Member { roles: roles.iter().map(|s| s.to_string()).collect() })
    }

    #[test]
    fn verdicts_follow_login_rules() {
        let c = cfg();
        // ban role means ban, even with the gm role
        assert_eq!(verdict(&c, &acc(&["player", "gm"]), &member(&["10", "666"])), Verdict::Ban);
        // left the server with require_guild: revoke sessions; without require_guild: leave alone
        assert_eq!(verdict(&c, &acc(&["player"]), &Lookup::NotMember), Verdict::Revoke);
        let open = DiscordCfg { require_guild: false, ..cfg() };
        assert_eq!(verdict(&open, &acc(&["player"]), &Lookup::NotMember), Verdict::Keep);
        // Discord error: leave alone
        assert_eq!(verdict(&c, &acc(&["player"]), &Lookup::Error("x".into())), Verdict::Keep);
        // roles match (order does not matter): no changes
        assert_eq!(verdict(&c, &acc(&["gm", "player"]), &member(&["10", "999"])), Verdict::Keep);
        // gm removed in Discord: the role goes away
        assert_eq!(verdict(&c, &acc(&["player", "gm"]), &member(&["999"])), Verdict::Roles(vec!["player".into()]));
        // whitelist granted: it is added
        assert_eq!(verdict(&c, &acc(&["player"]), &member(&["11"])), Verdict::Roles(vec!["player".into(), "whitelist".into()]));
        // without banned_role in the config, role 666 means nothing
        let nb = DiscordCfg { banned_role: String::new(), ..cfg() };
        assert_eq!(verdict(&nb, &acc(&["player"]), &member(&["666"])), Verdict::Keep);
    }

    /// Keys of accounts with a live session: without expired ones and without duplicates
    #[tokio::test]
    async fn active_keys_from_sessions() {
        let dir = format!("target/test_dwatch_db_{}", super::super::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        super::super::sessions::create(&db, "discord_1", 7).await;
        super::super::sessions::create(&db, "discord_1", 7).await;
        super::super::sessions::create(&db, "discord_2", 7).await;
        db.db
            .query("CREATE auth_sessions:old SET account_key = 'discord_3', created_at = 0, expires_at = 1")
            .await
            .unwrap();
        let mut keys = active_account_keys(&db).await.unwrap();
        keys.sort();
        assert_eq!(keys, vec!["discord_1".to_string(), "discord_2".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
