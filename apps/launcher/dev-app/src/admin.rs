//! Client of the backend admin API (ADMIN_API.md) and the admin login through the launcher web API
//! (`require = "relay"`): same flow as the player launcher (dbl::auth), the token stays in this process and in
//! Windows Credential Manager under `DungeonborneLauncher:dev-app.<server id>`.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use dbl::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Health {
    #[serde(default)]
    pub ok: bool,
    #[serde(default)]
    pub server_id: Option<String>,
    #[serde(default)]
    pub auth_require: String,
    #[serde(default)]
    pub version: String,
    /// Only the mock sets it.
    #[serde(default)]
    pub mock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Player {
    pub open_id: String,
    #[serde(default, deserialize_with = "null_default")]
    pub user_id: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub nick: String,
    #[serde(default, deserialize_with = "null_default")]
    pub hall_role_guid: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub roles: Vec<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default, deserialize_with = "null_default")]
    pub peer_ip: String,
    #[serde(default, deserialize_with = "null_default")]
    pub online_since: i64,
    #[serde(default, deserialize_with = "null_default")]
    pub in_battle: bool,
}

/// `null` → default (the backend sends `user_id: null` for a session without a saved player yet).
fn null_default<'de, D: serde::Deserializer<'de>, T: Default + Deserialize<'de>>(d: D) -> std::result::Result<T, D::Error> {
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}

/// `GET /admin/v1/players`: a bare array (contract) or `{players: [...]}` (early draft).
#[derive(Deserialize)]
#[serde(untagged)]
enum PlayersWire {
    List(Vec<Player>),
    Wrapped { players: Vec<Player> },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GmResult {
    /// Who the command ran on (the backend picks the only online player when the request had no open_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub online: Option<bool>,
    pub result_code: i32,
    #[serde(default)]
    pub result_message: String,
    #[serde(default)]
    pub result_list: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GmRequest {
    /// Empty / absent = the only player online.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_id: Option<String>,
    pub cmd: String,
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// Stable error code: `code` field, else `error` if it is a code word, else from the HTTP status.
fn error_code(status: reqwest::StatusCode, e: &ApiError) -> String {
    if let Some(c) = &e.code {
        return c.clone();
    }
    if !e.error.is_empty() && e.error.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return e.error.clone();
    }
    match status.as_u16() {
        400 => "bad_request",
        401 => "unauthorized",
        403 => "forbidden",
        404 => "player_not_found",
        409 => "not_online",
        503 => "host_unavailable",
        415 => "bad_content_type",
        _ => "http_status",
    }
    .into()
}

pub struct Client<'a> {
    pub http: &'a reqwest::Client,
    pub base: &'a str,
    pub token: Option<&'a str>,
}

impl Client<'_> {
    fn url(&self, path: &str) -> Result<reqwest::Url> {
        let url = dbl::net::check_url(&format!("{}/admin/v1/{path}", self.base.trim_end_matches('/')))?;
        // Admin API is loopback-only by contract; never send the admin token anywhere else.
        if !dbl::net::is_loopback_host(&url) {
            return Err(Error::new("admin_not_loopback", url.to_string()));
        }
        Ok(url)
    }

    fn auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let rb = rb.timeout(Duration::from_secs(10));
        match self.token {
            Some(t) if !t.is_empty() => rb.bearer_auth(t),
            _ => rb,
        }
    }

    async fn decode<T: for<'de> Deserialize<'de>>(resp: reqwest::Response) -> Result<T> {
        let status = resp.status();
        let body = resp.bytes().await?;
        if status.is_success() {
            return serde_json::from_slice(&body).map_err(|e| Error::new("admin_invalid", e.to_string()));
        }
        match serde_json::from_slice::<ApiError>(&body) {
            Ok(e) => {
                let detail = e.message.clone().unwrap_or_else(|| if error_code(status, &e) == e.error { status.to_string() } else { e.error.clone() });
                Err(Error::new(format!("admin.{}", error_code(status, &e)), detail))
            }
            Err(_) => Err(Error::new("http_status", status.to_string())),
        }
    }

    pub async fn health(&self) -> Result<Health> {
        Self::decode(self.http.get(self.url("health")?).timeout(Duration::from_secs(3)).send().await?).await
    }

    pub async fn players(&self) -> Result<Vec<Player>> {
        let p: PlayersWire = Self::decode(self.auth(self.http.get(self.url("players")?)).send().await?).await?;
        Ok(match p {
            PlayersWire::List(v) | PlayersWire::Wrapped { players: v } => v,
        })
    }

    pub async fn gm(&self, open_id: &str, cmd: &str) -> Result<GmResult> {
        let open_id = open_id.trim();
        let body = GmRequest { open_id: (!open_id.is_empty()).then(|| open_id.to_string()), cmd: cmd.to_string() };
        Self::decode(self.auth(self.http.post(self.url("gm")?)).json(&body).send().await?).await
    }

    pub async fn host_status(&self) -> Result<HostStatus> {
        Self::decode(self.auth(self.http.get(self.url("host/status")?)).send().await?).await
    }

    /// The backend waits up to 10 s for the mod's answer.
    pub async fn host_command(&self, cmd: &str) -> Result<HostCommandResult> {
        let cmd = cmd.trim();
        if !host_cmd_valid(cmd) {
            return Err(Error::new("bad_host_cmd", format!("host command: {} [argument]", HOST_VERBS.join(" | "))));
        }
        let rb = self.auth(self.http.post(self.url("host/command")?)).timeout(Duration::from_secs(20));
        Self::decode(rb.json(&serde_json::json!({ "cmd": cmd })).send().await?).await
    }
}

/// `GET /admin/v1/host/status`: the game server as the backend sees it (its `dbserver-*.json` files).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostStatus {
    #[serde(default)]
    pub root: String,
    /// `dbserver-status.json` of the mod (phase, map, …) or null.
    #[serde(default)]
    pub status: serde_json::Value,
    /// `dbserver-command-result.json`: `{cmd, result, at}` of the last host command, or null.
    #[serde(default)]
    pub last_command: serde_json::Value,
}

/// `POST /admin/v1/host/command` answer; `result` is null when the mod did not answer within 10 s (and for `probe`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostCommandResult {
    pub sent: String,
    #[serde(default)]
    pub host_phase: serde_json::Value,
    #[serde(default)]
    pub map: serde_json::Value,
    #[serde(default)]
    pub result: serde_json::Value,
}

/// Commands the backend relays to the game server (anything else is a 400 there).
pub const HOST_VERBS: &[&str] = &["portal", "exits", "probe"];

pub fn host_cmd_valid(cmd: &str) -> bool {
    let verb = cmd.split_whitespace().next().unwrap_or("");
    HOST_VERBS.contains(&verb) && cmd.len() <= 64 && !cmd.contains(char::is_control)
}

/// Login of the developer through the backend web API: manifest → login/start → browser → poll.
pub struct AdminLogin {
    pub server_id: String,
    pub api: String,
    pub browser_url: String,
    pub user_code: Option<String>,
    pub login_id: String,
    pub poll_ms: u64,
    pub expires_s: u64,
}

pub async fn login_start(http: &reqwest::Client, web_url: &str) -> Result<AdminLogin> {
    let manifest_url = format!("{}/.well-known/dbserver.json", web_url.trim_end_matches('/'));
    let m = dbl::manifest::fetch(http, &manifest_url).await?;
    let auth = m.auth.ok_or_else(|| Error::new("no_auth", "the manifest has no auth — no login needed (require = none?)"))?;
    let s = dbl::auth::start(http, &auth.api).await?;
    Ok(AdminLogin {
        server_id: m.id,
        api: auth.api,
        browser_url: s.browser_url,
        user_code: s.user_code,
        login_id: s.login_id,
        poll_ms: s.poll_interval_ms.clamp(250, 5000),
        expires_s: s.expires_in_s.min(900),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminAccount {
    pub server_id: String,
    pub open_id: String,
    pub display_name: String,
    pub roles: Vec<String>,
    pub expires_at: i64,
}

/// Polls until the browser side finishes. Returns the token and who logged in.
pub async fn login_wait(http: &reqwest::Client, l: &AdminLogin) -> Result<(String, AdminAccount)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(l.expires_s);
    loop {
        if tokio::time::Instant::now() > deadline {
            return Err(Error::new("login_expired", ""));
        }
        match dbl::auth::poll(http, &l.api, &l.login_id).await? {
            dbl::auth::Poll::Pending => tokio::time::sleep(Duration::from_millis(l.poll_ms)).await,
            dbl::auth::Poll::Denied { reason, message, .. } => {
                return Err(Error::new(format!("api.{}", reason.unwrap_or_else(|| "denied".into())), message.unwrap_or_default()))
            }
            dbl::auth::Poll::Ok { session_token, expires_at, account } => {
                let acc = AdminAccount {
                    server_id: l.server_id.clone(),
                    open_id: account.open_id,
                    display_name: account.display_name,
                    roles: account.roles,
                    expires_at,
                };
                return Ok((session_token, acc));
            }
        }
    }
}

pub fn cred_key(server_id: &str) -> String {
    format!("dev-app.{server_id}")
}
