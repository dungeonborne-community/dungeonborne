//! Login through the server (spec 4.2): Discord or any other provider the server enables.
//! The launcher never sees provider secrets; it only opens the browser page and polls.

use reqwest::{Client, Response, Url};
use serde::de::DeserializeOwned;
use serde::Deserialize;

use crate::error::{bail, Error, Result};
use crate::net::check_url;
use crate::store::Account;

#[derive(Debug, Deserialize)]
pub struct LoginStart {
    pub login_id: String,
    pub browser_url: String,
    /// Short code the browser page shows too, so the player can match them.
    #[serde(default)]
    pub user_code: Option<String>,
    #[serde(default = "default_poll")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_expires")]
    pub expires_in_s: u64,
}

fn default_poll() -> u64 {
    1000
}

fn default_expires() -> u64 {
    300
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum Poll {
    Pending,
    Denied {
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        message: Option<String>,
        /// e.g. Discord invite for `not_in_guild`
        #[serde(default)]
        link: Option<String>,
    },
    Ok {
        session_token: String,
        expires_at: i64,
        account: AccountWire,
    },
}

#[derive(Debug, Deserialize)]
pub struct AccountWire {
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(alias = "discord_id")]
    pub subject: String,
    #[serde(alias = "discord_name")]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub open_id: String,
    pub nick: String,
    #[serde(default)]
    pub roles: Vec<String>,
}

impl AccountWire {
    pub fn into_account(self, expires_at: i64) -> Result<Account> {
        let a = Account {
            provider: self.provider,
            subject: self.subject,
            display_name: self.display_name,
            avatar_url: self.avatar_url.filter(|u| u.starts_with("https://")),
            open_id: self.open_id,
            nick: self.nick,
            roles: self.roles,
            expires_at,
            logged_in_at: crate::net::unix_now(),
        };
        a.validate()?;
        Ok(a)
    }
}

#[derive(Debug, Deserialize)]
pub struct Refreshed {
    pub session_token: String,
    pub expires_at: i64,
    #[serde(default)]
    pub account: Option<AccountWire>,
}

#[derive(Deserialize)]
struct ApiError {
    error: String,
    #[serde(default)]
    message: Option<String>,
}

fn endpoint(api: &str, path: &str) -> Result<Url> {
    check_url(&format!("{}/{}", api.trim_end_matches('/'), path))
}

async fn decode<T: DeserializeOwned>(resp: Response) -> Result<T> {
    let status = resp.status();
    let body = resp.bytes().await?;
    if status.is_success() {
        return serde_json::from_slice(&body).map_err(|e| Error::new("api_invalid", e.to_string()));
    }
    match serde_json::from_slice::<ApiError>(&body) {
        Ok(e) => Err(Error::new(format!("api.{}", e.error), e.message.unwrap_or_default())),
        Err(_) => bail!("http_status", "{status}"),
    }
}

pub async fn start(client: &Client, api: &str) -> Result<LoginStart> {
    let r: LoginStart = decode(client.post(endpoint(api, "login/start")?).send().await?).await?;
    check_url(&r.browser_url)?;
    Ok(r)
}

pub async fn poll(client: &Client, api: &str, login_id: &str) -> Result<Poll> {
    let mut url = endpoint(api, "login/poll")?;
    url.query_pairs_mut().append_pair("login_id", login_id);
    decode(client.get(url).send().await?).await
}

pub async fn refresh(client: &Client, api: &str, token: &str) -> Result<Refreshed> {
    decode(client.post(endpoint(api, "session/refresh")?).bearer_auth(token).send().await?).await
}

/// Is the session still alive? `api.expired` / `api.banned` errors when it is not.
pub async fn me(client: &Client, api: &str, token: &str) -> Result<()> {
    decode::<serde_json::Value>(client.get(endpoint(api, "me")?).bearer_auth(token).send().await?).await.map(|_| ())
}

/// Lobby of game instances (spec section 12). The view is passed to the UI as is: only the UI reads its fields.
pub async fn lobby_get(client: &Client, api: &str, token: &str) -> Result<serde_json::Value> {
    decode(client.get(endpoint(api, "lobby")?).bearer_auth(token).send().await?).await
}

pub async fn lobby_post(client: &Client, api: &str, token: &str, path: &str, body: serde_json::Value) -> Result<serde_json::Value> {
    decode(client.post(endpoint(api, &format!("lobby/{path}"))?).bearer_auth(token).json(&body).send().await?).await
}

/// Battle UDP tunnel of this account (backend `/net`, docs/LAUNCHER_SPEC.md §14): `{udp_tunnel, capable}`
pub async fn net_get(client: &Client, api: &str, token: &str) -> Result<serde_json::Value> {
    decode(client.get(endpoint(api, "net")?).bearer_auth(token).send().await?).await
}

/// Switches the tunnel; `reason` goes to the server log (what the launcher saw, or "settings")
pub async fn net_set_udp(client: &Client, api: &str, token: &str, on: bool, reason: &str) -> Result<serde_json::Value> {
    let body = serde_json::json!({ "on": on, "reason": reason });
    decode(client.post(endpoint(api, "net/udp_tunnel")?).bearer_auth(token).json(&body).send().await?).await
}

/// The launcher saw the provider cut the connection (backend `/net/report`, LAUNCHER_SPEC §14.1): only a line in the
/// server's log, to see how many players it hits
pub async fn net_report(client: &Client, api: &str, token: &str, r: &crate::netblock::Report) -> Result<()> {
    let body = serde_json::json!({ "kind": r.kind, "detail": r.detail, "at": r.at, "version": env!("CARGO_PKG_VERSION") });
    decode::<serde_json::Value>(client.post(endpoint(api, "net/report")?).bearer_auth(token).json(&body).send().await?).await?;
    Ok(())
}

/// Bug report (backend `bug_report`, `docs/specs/bug-report-api.md`): multipart `report` JSON + log files.
/// Returns the report id. The limit (3 an hour) comes back as `api.rate_limited` with the seconds to wait in `detail`.
pub async fn bug_report(client: &Client, api: &str, token: &str, report: &serde_json::Value, files: Vec<(String, Vec<u8>)>) -> Result<String> {
    let with_files = !files.is_empty();
    let v = match bug_report_post(client, api, token, report, files).await {
        // The server answers 401 / 429 before reading the body (it does not take 9 MB it will refuse) and closes the
        // connection; with megabytes still unsent Windows may reset it and the answer is lost. A tiny probe tells
        // which it was: a report the server refuses as invalid (`kind`), checked only after the session and the limit,
        // so it is never stored. Its 400 means both are fine, the failure was the network.
        Err(e) if e.code == "network" && with_files => {
            let probe = serde_json::json!({ "source": "launcher", "kind": "-", "title": "-" });
            match bug_report_post(client, api, token, &probe, Vec::new()).await {
                Err(p) if p.code == "api.rate_limited" || p.code == "api.expired" || p.code == "api.banned" => return Err(p),
                _ => return Err(e),
            }
        }
        r => r?,
    };
    match v["id"].as_str() {
        Some(id) if crate::bugreport::valid_id(id) => Ok(id.to_string()),
        _ => bail!("api_invalid", "bug_report: no id"),
    }
}

async fn bug_report_post(
    client: &Client,
    api: &str,
    token: &str,
    report: &serde_json::Value,
    files: Vec<(String, Vec<u8>)>,
) -> Result<serde_json::Value> {
    use reqwest::multipart::{Form, Part};
    let mut form = Form::new().part("report", Part::text(report.to_string()).mime_str("application/json")?);
    for (name, data) in files {
        form = form.part(name.clone(), Part::bytes(data).file_name(name).mime_str("text/plain")?);
    }
    // up to 9 MB on a slow line: longer than the client's usual 60 s
    let resp = client
        .post(endpoint(api, "bug_report")?)
        .bearer_auth(token)
        .multipart(form)
        .timeout(std::time::Duration::from_secs(600))
        .send()
        .await?;
    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let v: serde_json::Value = resp.json().await.unwrap_or_default();
        bail!("api.rate_limited", "{}", v["retry_after"].as_u64().unwrap_or(3600));
    }
    decode(resp).await
}

/// `{status: "new" | "posted", url}`; `url` only when it is an https link
pub async fn bug_report_status(client: &Client, api: &str, token: &str, id: &str) -> Result<serde_json::Value> {
    if !crate::bugreport::valid_id(id) {
        bail!("bad_id", "{id}");
    }
    let v: serde_json::Value = decode(client.get(endpoint(api, &format!("bug_report/{id}"))?).bearer_auth(token).send().await?).await?;
    let status = v["status"].as_str().unwrap_or("new");
    let url = v["url"].as_str().filter(|u| u.starts_with("https://"));
    Ok(serde_json::json!({ "status": status, "url": url }))
}

pub async fn logout(client: &Client, api: &str, token: &str) -> Result<()> {
    let resp = client.post(endpoint(api, "logout")?).bearer_auth(token).send().await?;
    if !resp.status().is_success() && resp.status().as_u16() != 401 {
        bail!("http_status", "{}", resp.status());
    }
    Ok(())
}
