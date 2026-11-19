//! Provider `discord`: our login implementation (decision 02.11, docs/LAUNCHER_SPEC.md). OAuth2 authorization code:
//! the server exchanges the code for a token; the app secret lives only in the server env (`[auth.discord] client_secret_env`).
//! Scope `identify`; if `guild_id` is set, also `guilds.members.read`: membership in the community server, roles → our
//! roles (`[auth.discord.roles]`), `banned_role` → denied. The Discord token is not stored anywhere: it is only needed during login.

use super::provider::{AuthProvider, Denied, Identity, Start};
use crate::server_toml::{DiscordCfg, ServerToml};
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashMap;
use std::time::Duration;

const AUTHORIZE: &str = "https://discord.com/oauth2/authorize";
const API: &str = "https://discord.com/api/v10";

pub struct DiscordProvider {
    cfg: DiscordCfg,
    secret: String,
    redirect_uri: String,
    http: reqwest::Client,
}

#[derive(Deserialize)]
struct TokenRsp {
    access_token: String,
}

#[derive(Deserialize)]
struct User {
    id: String,
    username: String,
    global_name: Option<String>,
    avatar: Option<String>,
}

#[derive(Deserialize)]
struct Member {
    #[serde(default)]
    roles: Vec<String>,
    nick: Option<String>,
}

/// percent-encoding of a value for query/form (RFC 3986 unreserved characters stay as is)
pub fn pct(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn query(params: &[(&str, &str)]) -> String {
    params.iter().map(|(k, v)| format!("{}={}", k, pct(v))).collect::<Vec<_>>().join("&")
}

impl DiscordProvider {
    pub fn new(cfg: &ServerToml) -> Result<Self, String> {
        let d = cfg.auth.discord.clone();
        if d.client_id.is_empty() {
            return Err("[auth.discord] client_id is not set".into());
        }
        if cfg.web.public_url.is_empty() {
            return Err("[web] public_url is not set — nowhere to return the browser to".into());
        }
        let secret = std::env::var(&d.client_secret_env).map_err(|_| format!("no env {}", d.client_secret_env))?;
        let redirect_uri = format!("{}/auth/discord/callback", cfg.web.browser_base());
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
        println!("[Auth] discord: client_id {}, redirect {}, guild {}", d.client_id, redirect_uri, if d.guild_id.is_empty() { "-" } else { &d.guild_id });
        Ok(Self { cfg: d, secret, redirect_uri, http })
    }

    fn scope(&self) -> &'static str {
        if self.cfg.guild_id.is_empty() {
            "identify"
        } else {
            "identify guilds.members.read"
        }
    }

    async fn exchange(&self, code: &str) -> Result<String, String> {
        let body = query(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", &self.redirect_uri),
            ("client_id", &self.cfg.client_id),
            ("client_secret", &self.secret),
        ]);
        let rsp = self
            .http
            .post(format!("{}/oauth2/token", API))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !rsp.status().is_success() {
            return Err(format!("token: HTTP {}", rsp.status()));
        }
        Ok(rsp.json::<TokenRsp>().await.map_err(|e| e.to_string())?.access_token)
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, token: &str, path: &str) -> Result<Option<T>, String> {
        let rsp = self.http.get(format!("{}{}", API, path)).bearer_auth(token).send().await.map_err(|e| e.to_string())?;
        if rsp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !rsp.status().is_success() {
            return Err(format!("{}: HTTP {}", path, rsp.status()));
        }
        rsp.json::<T>().await.map(Some).map_err(|e| e.to_string())
    }
}

#[async_trait]
impl AuthProvider for DiscordProvider {
    fn id(&self) -> &'static str {
        "discord"
    }

    fn start(&self, state: &str) -> Start {
        Start::Redirect(format!(
            "{}?{}",
            AUTHORIZE,
            query(&[
                ("response_type", "code"),
                ("client_id", &self.cfg.client_id),
                ("scope", self.scope()),
                ("redirect_uri", &self.redirect_uri),
                ("state", state),
                ("prompt", "none"),
            ])
        ))
    }

    async fn callback(&self, q: &HashMap<String, String>) -> Result<(String, Identity), (Option<String>, Denied)> {
        let state = q.get("state").cloned();
        if q.contains_key("error") {
            return Err((state, Denied::new("denied", "Вход через Discord отменён")));
        }
        let (Some(state), Some(code)) = (state, q.get("code")) else {
            return Err((None, Denied::new("error", "Discord не вернул код входа")));
        };
        let fail = |e: String| {
            eprintln!("[Auth] discord: {}", e);
            (Some(state.clone()), Denied::new("error", "Discord не ответил, попробуйте ещё раз"))
        };
        let token = self.exchange(code).await.map_err(fail)?;
        let user: User = self.get(&token, "/users/@me").await.map_err(fail)?.ok_or_else(|| fail("users/@me: 404".into()))?;

        let mut roles = Vec::new();
        let mut display_name = user.global_name.clone().unwrap_or_else(|| user.username.clone());
        if !self.cfg.guild_id.is_empty() {
            let member: Option<Member> =
                self.get(&token, &format!("/users/@me/guilds/{}/member", self.cfg.guild_id)).await.map_err(fail)?;
            match member {
                None if self.cfg.require_guild => {
                    println!("[Auth] login: denied discord_{} '{}' — not on the Discord server (require_guild)", user.id, display_name);
                    let d = Denied::new("not_in_guild", "Вход только для участников Discord-сервера сообщества. Вступите на сервер и войдите ещё раз.");
                    return Err((Some(state), d.with_link(&self.cfg.invite_url)));
                }
                None => {}
                Some(m) => {
                    if !self.cfg.banned_role.is_empty() && m.roles.contains(&self.cfg.banned_role) {
                        return Err((Some(state), Denied::new("banned", "Доступ к серверу закрыт")));
                    }
                    roles.extend(m.roles.iter().filter_map(|r| self.cfg.roles.get(r).cloned()));
                    if let Some(n) = m.nick.filter(|n| !n.is_empty()) {
                        display_name = n;
                    }
                }
            }
        }
        let avatar_url = user.avatar.as_ref().map(|a| format!("https://cdn.discordapp.com/avatars/{}/{}.png", user.id, a));
        Ok((state, Identity { subject: user.id, display_name, login: user.username, avatar_url, roles }))
    }
}

#[cfg(test)]
mod tests {
    use super::pct;

    #[test]
    fn pct_encodes_reserved() {
        assert_eq!(pct("identify guilds.members.read"), "identify%20guilds.members.read");
        assert_eq!(pct("https://a.b/c?d"), "https%3A%2F%2Fa.b%2Fc%3Fd");
    }
}
