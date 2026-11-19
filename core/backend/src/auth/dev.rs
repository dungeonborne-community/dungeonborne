//! Provider `dev`: for local tests of the launcher and relay without external services: a page with a name field,
//! identity = the entered name. Do not enable on a public server (anyone can log in under any name).

use super::provider::{AuthProvider, Denied, Identity, Start};
use super::web::html_escape;
use crate::server_toml::ServerToml;
use async_trait::async_trait;
use std::collections::HashMap;

pub struct DevProvider;

impl DevProvider {
    pub fn new(cfg: &ServerToml) -> Self {
        if cfg.auth.require_relay() && !cfg.web.public_url.contains("127.0.0.1") && !cfg.web.public_url.contains("localhost") {
            eprintln!("[Auth] WARNING: provider dev is enabled on public address {} — anyone can log in", cfg.web.public_url);
        }
        Self
    }
}

#[async_trait]
impl AuthProvider for DevProvider {
    fn id(&self) -> &'static str {
        "dev"
    }

    fn start(&self, state: &str) -> Start {
        Start::Page(format!(
            r#"<form method="get" action="/auth/dev/callback">
<input type="hidden" name="state" value="{}">
<p>Test login (dev). Player name:</p>
<p><input name="name" maxlength="32" autofocus required> <button type="submit">Log in</button></p>
</form>"#,
            html_escape(state)
        ))
    }

    async fn callback(&self, q: &HashMap<String, String>) -> Result<(String, Identity), (Option<String>, Denied)> {
        let state = q.get("state").cloned().unwrap_or_default();
        let name = q.get("name").map(|s| s.trim().to_string()).unwrap_or_default();
        if name.is_empty() || name.chars().count() > 32 {
            return Err((Some(state), Denied::new("denied", "Name: 1 to 32 characters")));
        }
        Ok((state, Identity { subject: name.clone(), display_name: name.clone(), login: name, avatar_url: None, roles: Vec::new() }))
    }
}
