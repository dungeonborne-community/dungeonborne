//! `server.toml` — server identity and policy (docs/BACKEND_FRAMEWORK_PLAN.md stage 1). For now only what is needed
//! by the `dbserver/1` manifest and player login (src/auth); everything else still comes from env (`ServerConfig`).
//! No file — everything default: login without accounts (as before), web API and relay off.
//! Path: env `SERVER_TOML`, otherwise `server.toml` in the working folder. Example — `server.example.toml`.
//! Secrets (Discord app key etc.) are not stored in the file: only the env variable name.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct ServerToml {
    pub server: ServerInfo,
    pub game: GameInfo,
    pub web: WebCfg,
    pub auth: AuthCfg,
    pub relay: RelayCfg,
    pub mods: ModsCfg,
    pub launcher: LauncherCfg,
    pub patch_notes: PatchNotesCfg,
    pub support: SupportCfg,
}

/// 16.11 fundraising bar in the launcher: `url` — JSON with the month goal and links (written by the Discord bot, served by nginx);
/// empty — no `support` field in the manifest
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct SupportCfg {
    pub url: String,
}

/// Server changelog for the launcher (src/auth/notes.rs). A relative path is from the working folder.
/// No file — no `patch_notes` field in the manifest; empty — off.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct PatchNotesCfg {
    pub file: String,
}

impl Default for PatchNotesCfg {
    fn default() -> Self {
        Self { file: "patch_notes.toml".into() }
    }
}

/// Launcher self-update (src/auth/release.rs): folder with `launcher.json` and the exe from `launcher/tools/release.py sign`.
/// A relative path is from the working folder. No folder — no `launcher` field in the manifest.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct LauncherCfg {
    pub dir: String,
}

impl Default for LauncherCfg {
    fn default() -> Self {
        Self { dir: "launcher".into() }
    }
}

/// Client mods for the launcher (src/auth/mods.rs): folder with `dbmod/1` zip packages and `manifest_mods.json`
/// (`launcher/tools/make_mods.py`). A relative path is from the working folder. No folder — `mods: []`.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ModsCfg {
    pub dir: String,
    /// 13.11 (user: "so there are no foreign or extra mods… everyone has different ones"): false — the manifest has
    /// `policy.allow_unlisted_client_mods = false`, the launcher removes foreign paks and UE4SS mods for the duration of the game (1.7.4+).
    /// Default true — as before.
    pub allow_unlisted: bool,
}

impl Default for ModsCfg {
    fn default() -> Self {
        Self { dir: "mods".into(), allow_unlisted: true }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct ServerInfo {
    pub id: String,
    pub name: String,
    /// a string or `{ ru = "…", en = "…" }` (src/text.rs)
    pub description: crate::text::Text,
    pub region: String,
    pub languages: Vec<String>,
    pub website: String,
    pub contact: String,
    pub capacity: u32,
    /// a string or `{ ru = "…", en = "…" }` (src/text.rs)
    pub motd: crate::text::Text,
}

impl Default for ServerInfo {
    fn default() -> Self {
        Self {
            id: "local.dungeonborne.dev".into(),
            name: "Dungeonborne (локальный)".into(),
            description: Default::default(),
            region: String::new(),
            languages: vec!["ru".into()],
            website: String::new(),
            contact: String::new(),
            capacity: 64,
            motd: Default::default(),
        }
    }
}

#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct GameInfo {
    pub exe_sha256: String,
    pub label: String,
}

/// Launcher HTTP API and login pages. In prod TLS is terminated by a reverse proxy (Caddy/nginx with a public certificate),
/// `public_url` — the external address. Empty `bind` — off.
#[derive(Deserialize, Default, Clone, Debug)]
#[serde(default)]
pub struct WebCfg {
    pub bind: String,
    pub public_url: String,
    /// Address of the login pages for the player's browser (OAuth return `/auth/<provider>/callback`); empty — `public_url`.
    /// Without a domain the launcher API goes over https with a pinned certificate, and the browser does not trust such a certificate —
    /// then put `http://<ip>` here (the proxy serves only `/auth/` over http)
    pub browser_url: String,
    /// 15.11: sites the browser will hand the `GET /dbserver/status` response to (CORS, exact Origin match);
    /// empty — `DEFAULT_STATUS_CORS`, `["*"]` — anyone
    pub status_cors: Vec<String>,
}

/// Community site and its test stand (15.11)
/// (16.11: the site is on www, the root redirects; localhost:8821 — site development)
pub const DEFAULT_STATUS_CORS: [&str; 4] = [
    "https://www.dungeonborne-community.com",
    "https://dungeonborne-community.com",
    "https://test.dungeonborne-community.com",
    "http://localhost:8821",
];

impl WebCfg {
    /// `Access-Control-Allow-Origin` value for a request with `origin`: None — the header is not set
    pub fn status_cors_for(&self, origin: &str) -> Option<String> {
        let list: Vec<&str> =
            if self.status_cors.is_empty() { DEFAULT_STATUS_CORS.to_vec() } else { self.status_cors.iter().map(String::as_str).collect() };
        if list.contains(&"*") {
            return Some("*".into());
        }
        (!origin.is_empty() && list.iter().any(|o| o.trim_end_matches('/') == origin)).then(|| origin.to_string())
    }

    pub fn browser_base(&self) -> &str {
        let u = if self.browser_url.is_empty() { &self.public_url } else { &self.browser_url };
        u.trim_end_matches('/')
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct AuthCfg {
    /// `none` — as before: identity from what the client claimed (local tests);
    /// `relay` — only connections through the launcher relay with a live session are let into the game.
    pub require: String,
    /// Enabled providers in order; the first is the default. Implementations: `discord`, `dev`.
    pub providers: Vec<String>,
    pub session_ttl_days: u32,
    /// Account roles allowed into matches (match search, room start); empty — everyone. `gm`/`admin` — always.
    pub battle_roles: Vec<String>,
    /// Refusal text for a player without such a role
    pub battle_denied_message: String,
    /// Login to the server (launcher, relay, session refresh) — only with one of these roles (`whitelist`); empty — everyone.
    /// `gm`/`admin` — always. Roles are taken from `[auth.discord.roles]` at login
    pub login_roles: Vec<String>,
    /// Refusal text for login without such a role
    pub login_denied_message: String,
    pub discord: DiscordCfg,
}

impl Default for AuthCfg {
    fn default() -> Self {
        Self {
            require: "none".into(),
            providers: Vec::new(),
            session_ttl_days: 30,
            battle_roles: Vec::new(),
            battle_denied_message: "Бой пока доступен только участникам с ролью тестера на Discord-сервере сообщества. \
                                    Роль обновится при следующем входе через лаунчер."
                .into(),
            login_roles: Vec::new(),
            login_denied_message: "Вход пока только для участников с ролью Whitelist на Discord-сервере сообщества. \
                                   Получите роль и войдите в лаунчере ещё раз."
                .into(),
            discord: DiscordCfg::default(),
        }
    }
}

impl AuthCfg {
    pub fn require_relay(&self) -> bool {
        self.require == "relay"
    }
}

/// Discord provider (src/auth/discord.rs). App — Discord Developer Portal, redirect
/// `<web.public_url>/auth/discord/callback`.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct DiscordCfg {
    pub client_id: String,
    /// name of the env variable with the client secret
    pub client_secret_env: String,
    /// community Discord server: empty — any Discord account
    pub guild_id: String,
    /// not a member of `guild_id` — login refused
    pub require_guild: bool,
    /// invite to the community server: shown on a `not_in_guild` refusal (login page and launcher)
    pub invite_url: String,
    /// Discord role id → our role (`admin`, `gm`, …); everyone has `player`
    pub roles: HashMap<String, String>,
    /// Discord role that forbids login
    pub banned_role: String,
    /// 15.11: name of the env variable with the community bot token (bot on `guild_id`). Set — the `discord_watch` watcher
    /// rechecks the roles of everyone with a live session; not set — roles are checked only at login
    pub bot_token_env: String,
    /// how often the watcher goes over live sessions, s
    pub recheck_secs: u64,
}

impl Default for DiscordCfg {
    fn default() -> Self {
        Self {
            client_id: String::new(),
            client_secret_env: "DISCORD_CLIENT_SECRET".into(),
            guild_id: String::new(),
            require_guild: false,
            invite_url: String::new(),
            roles: HashMap::new(),
            banned_role: String::new(),
            bot_token_env: "DISCORD_BOT_TOKEN".into(),
            recheck_secs: 300,
        }
    }
}

/// Launcher relay receiver (src/auth/relay.rs). Empty `bind` — off.
#[derive(Deserialize, Clone, Debug)]
#[serde(default)]
pub struct RelayCfg {
    pub bind: String,
    /// address and port the launcher sees in the manifest
    pub public_address: String,
    pub public_port: u16,
    /// relay TLS certificate and key (PEM); the sha256 of the leaf certificate goes into the manifest as a pin
    pub cert: String,
    pub key: String,
    /// where the relay connects to the game's own ports (80/443/8085/6601–6604)
    pub upstream_ip: String,
    /// 11.11: minimum launcher version ("1.7.0"); older by preface (`client: dbl/x.y.z`) — refused with
    /// `launcher_outdated`. Empty — no check. Needed when mods rely on a new launcher (without hosts the game goes to
    /// https://localhost, and the relay of old launchers listens only on 127.0.0.1, not ::1 — loading would hang).
    pub min_client: String,
    /// 12.11: the game runs without hosts — the ru.rebuild-private mod points it at https://localhost and the directory
    /// dir.228918463-2-1.example.org (A 127.0.0.1), INTL login — via the launcher's open INTLConfig.ini.
    /// Goes into the manifest (`relay.no_hosts`), launcher 1.7.1+ then does not require hosts lines. false — as before, with hosts.
    pub no_hosts: bool,
}

impl Default for RelayCfg {
    fn default() -> Self {
        Self {
            bind: String::new(),
            public_address: String::new(),
            public_port: 7443,
            cert: "cert.pem".into(),
            key: "key.pem".into(),
            upstream_ip: "127.0.0.1".into(),
            min_client: String::new(),
            no_hosts: false,
        }
    }
}

impl ServerToml {
    pub fn load() -> Self {
        let path = std::env::var("SERVER_TOML").unwrap_or_else(|_| "server.toml".into());
        match std::fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<ServerToml>(&text) {
                Ok(cfg) => {
                    println!("[ServerToml] {}: server '{}', login require={}, providers {:?}", path, cfg.server.id, cfg.auth.require, cfg.auth.providers);
                    cfg
                }
                Err(e) => panic!("[ServerToml] {}: parse error: {}", path, e),
            },
            Err(_) => Self::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_cors_origins() {
        let mut w = WebCfg::default();
        assert_eq!(w.status_cors_for("https://dungeonborne-community.com").as_deref(), Some("https://dungeonborne-community.com"));
        assert_eq!(w.status_cors_for("https://www.dungeonborne-community.com").as_deref(), Some("https://www.dungeonborne-community.com"));
        assert_eq!(w.status_cors_for("https://evil.example"), None);
        assert_eq!(w.status_cors_for(""), None, "no Origin — no header needed");
        w.status_cors = vec!["https://a.example/".into()];
        assert_eq!(w.status_cors_for("https://a.example").as_deref(), Some("https://a.example"));
        assert_eq!(w.status_cors_for("https://dungeonborne-community.com"), None, "own list replaces the default one");
        w.status_cors = vec!["*".into()];
        assert_eq!(w.status_cors_for("https://evil.example").as_deref(), Some("*"));
    }

    #[test]
    fn example_parses() {
        let cfg: ServerToml = toml::from_str(include_str!("../server.example.toml")).unwrap();
        assert!(cfg.auth.require_relay());
        assert_eq!(cfg.auth.providers[0], "discord");
        assert_eq!(cfg.relay.public_port, 7443);
    }

    /// prod VM config (<data>\deploy/vm\server.toml): parses and agrees with the launcher
    #[test]
    fn vm_config_parses() {
        let Ok(text) = std::fs::read_to_string("../deploy/vm/server.toml") else { return };
        let cfg: ServerToml = toml::from_str(&text).unwrap();
        assert!(cfg.auth.require_relay());
        assert_eq!(cfg.web.public_url, "https://dbserver.example.org", "built-in launcher server URL");
        assert_eq!(cfg.web.browser_base(), "http://203.0.113.10");
        // login whitelist removed 04.11 (owner): let in all Discord server members; the role stayed in the mapping
        assert!(cfg.auth.login_roles.is_empty());
        assert!(cfg.auth.discord.roles.values().any(|r| r == "whitelist"));
        assert_eq!((cfg.relay.public_address.as_str(), cfg.relay.public_port), ("dbserver.example.org", 8443));
        assert_eq!(cfg.relay.upstream_ip, "10.0.0.2");
    }

    #[test]
    fn empty_is_legacy() {
        let cfg: ServerToml = toml::from_str("").unwrap();
        assert!(!cfg.auth.require_relay());
        assert!(cfg.web.bind.is_empty() && cfg.relay.bind.is_empty());
        assert!(!cfg.relay.no_hosts, "no flag — with hosts, as before");
        assert!(cfg.mods.allow_unlisted, "no flag — foreign mods allowed, as before");
        let cfg: ServerToml = toml::from_str("[mods]\nallow_unlisted = false\n").unwrap();
        assert!(!cfg.mods.allow_unlisted);
        let cfg: ServerToml = toml::from_str("[relay]\nno_hosts = true\n").unwrap();
        assert!(cfg.relay.no_hosts);
    }
}
