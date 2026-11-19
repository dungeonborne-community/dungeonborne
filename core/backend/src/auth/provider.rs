//! Login provider: the framework's extension point. A provider only answers "who is this": it takes the player's browser
//! through its own page/OAuth and returns an `Identity`. Accounts, sessions, bans, relay are the core (`auth::*`).
//!
//! Flow (docs/LAUNCHER_SPEC.md §4.2): launcher `login/start` → browser to `/auth/<id>/start?login_id=…` →
//! `start` (own page or redirect to OAuth, `state` = login_id) → browser returns to
//! `/auth/<id>/callback` → `callback` → the core creates/updates the account and hands out a token on `login/poll`.

use async_trait::async_trait;
use std::collections::HashMap;

/// Identity according to the provider
#[derive(Clone, Debug)]
pub struct Identity {
    /// permanent id at the provider (Discord user id)
    pub subject: String,
    pub display_name: String,
    /// login at the provider (Discord username); 12.11: for lookup in /ban
    pub login: String,
    pub avatar_url: Option<String>,
    /// our roles (`admin`, `gm`, …) derived by the provider; the core adds `player` itself
    pub roles: Vec<String>,
}

/// Provider denial: `code` goes to the launcher (`denied`, `not_in_guild`, `banned`, `error`), `message` to the player
#[derive(Clone, Debug)]
pub struct Denied {
    pub code: &'static str,
    pub message: String,
    /// where to send the player to fix the denial (invite to the community server on `not_in_guild`)
    pub link: Option<String>,
}

impl Denied {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), link: None }
    }

    pub fn with_link(mut self, link: &str) -> Self {
        self.link = (!link.is_empty()).then(|| link.to_string());
        self
    }
}

/// Response to `/auth/<id>/start`
pub enum Start {
    /// send the browser to an external OAuth
    Redirect(String),
    /// own HTML page (dev)
    Page(String),
}

#[async_trait]
pub trait AuthProvider: Send + Sync {
    /// name in `[auth] providers` and in paths `/auth/<id>/…`
    fn id(&self) -> &'static str;
    /// how to start the login; `state` = login_id (one-time, random; it is also the CSRF protection)
    fn start(&self, state: &str) -> Start;
    /// browser return: (state, identity) or denial
    async fn callback(&self, query: &HashMap<String, String>) -> Result<(String, Identity), (Option<String>, Denied)>;
}
