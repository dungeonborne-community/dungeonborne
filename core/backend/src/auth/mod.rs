//! Player login: a core module of the framework (docs/BACKEND_FRAMEWORK_PLAN.md, contract with the launcher:
//! docs/LAUNCHER_SPEC.md §4). The core does not know who confirms identity: that is the provider's job (`AuthProvider`).
//! Discord is our provider implementation (`discord.rs`), `dev` is for local tests. Another server on this engine
//! can enable its own providers in `server.toml` (`[auth] providers`) without touching the core.
//!
//! Parts:
//! - `provider`: the provider trait and `Identity` (who this is according to the provider);
//! - `accounts`: account = (provider, subject) → technical open_id (SteamID64 format) and the game's user_id;
//! - `sessions`: launcher session tokens (only sha256 in the DB), pending logins (login_id → result);
//! - `web`: launcher HTTP API, provider login pages, the `dbserver/1` manifest, status;
//! - `relay`: TLS acceptor of the launcher relay → the game's own ports; the connection's identity goes into `registry`;
//! - `registry`: relay connection address → account: the gateway, DirServer and the game's HTTPS login use it to recognize the player.
//!
//! SteamID is not an identity (the game cannot be obtained on Steam): the server issues the open_id, the launcher writes it into the emulator.

pub mod accounts;
pub mod ban;
pub mod dev;
pub mod discord;
pub mod discord_watch;
pub mod mods;
pub mod notes;
pub mod provider;
pub mod registry;
pub mod relay;
pub mod release;
pub mod sessions;
pub mod udp_tunnel;
pub mod web;

use crate::db::GameDb;
use crate::server_toml::ServerToml;
use provider::AuthProvider;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Shared login state: config, DB, providers, pending logins
pub struct AuthCore {
    pub cfg: ServerToml,
    pub db: GameDb,
    pub providers: HashMap<String, Arc<dyn AuthProvider>>,
    pub pending: sessions::PendingLogins,
    /// client mods for the manifest and for serving (`/mods/<file>`)
    pub mods: mods::Mods,
    /// launcher build for self-update (`/launcher/<file>`)
    pub releases: release::Releases,
    /// server changelog (`/dbserver/patch_notes`)
    pub notes: notes::PatchNotes,
}

static POLICY_RELAY: OnceLock<bool> = OnceLock::new();

/// `[auth] require = "relay"`: game connections without the launcher relay are not let in
pub fn require_relay() -> bool {
    *POLICY_RELAY.get().unwrap_or(&false)
}

/// Battle access: `[auth] battle_roles` and the denial text (`accounts::may_battle`)
static POLICY_BATTLE: OnceLock<(Vec<String>, String)> = OnceLock::new();

pub fn battle_policy() -> (&'static [String], &'static str) {
    match POLICY_BATTLE.get() {
        Some((roles, msg)) => (roles, msg),
        None => (&[], ""),
    }
}

/// Server login: `[auth] login_roles` (whitelist) and the denial text (`accounts::login_allowed`)
static POLICY_LOGIN: OnceLock<(Vec<String>, String)> = OnceLock::new();

pub fn login_policy() -> (&'static [String], &'static str) {
    match POLICY_LOGIN.get() {
        Some((roles, msg)) => (roles, msg),
        None => (&[], ""),
    }
}

impl AuthCore {
    pub fn provider(&self, id: &str) -> Option<&Arc<dyn AuthProvider>> {
        self.providers.get(id)
    }

    pub fn default_provider(&self) -> Option<&str> {
        self.cfg.auth.providers.iter().map(String::as_str).find(|p| self.providers.contains_key(*p))
    }
}

/// Starts the web API and the relay from `server.toml`. Without `[web] bind` / `[relay] bind` the corresponding part is off.
pub fn spawn(db: GameDb, cfg: ServerToml) {
    let _ = POLICY_RELAY.set(cfg.auth.require_relay());
    let _ = POLICY_BATTLE.set((cfg.auth.battle_roles.clone(), cfg.auth.battle_denied_message.clone()));
    if !cfg.auth.battle_roles.is_empty() {
        println!("[Auth] battle: only with roles {:?} (and gm/admin)", cfg.auth.battle_roles);
    }
    let _ = POLICY_LOGIN.set((cfg.auth.login_roles.clone(), cfg.auth.login_denied_message.clone()));
    if !cfg.auth.login_roles.is_empty() {
        println!("[Auth] login: only with roles {:?} (and gm/admin)", cfg.auth.login_roles);
    }
    // ban via the DB: the event revokes sessions, the LIVE subscription drops connections (ban.rs)
    let ban_db = db.clone();
    tokio::spawn(async move {
        ban::setup(&ban_db).await;
        ban::spawn_watch(ban_db);
    });
    // 15.11: Discord roles of live sessions (ban, server membership, gm/admin), not only at login
    if cfg.auth.providers.iter().any(|p| p == "discord") {
        discord_watch::spawn(db.clone(), cfg.auth.discord.clone());
    }
    let mut providers: HashMap<String, Arc<dyn AuthProvider>> = HashMap::new();
    for id in &cfg.auth.providers {
        let p: Arc<dyn AuthProvider> = match id.as_str() {
            "discord" => match discord::DiscordProvider::new(&cfg) {
                Ok(p) => Arc::new(p),
                Err(e) => {
                    eprintln!("[Auth] provider discord disabled: {}", e);
                    continue;
                }
            },
            "dev" => Arc::new(dev::DevProvider::new(&cfg)),
            other => {
                eprintln!("[Auth] unknown provider '{}' in [auth] providers", other);
                continue;
            }
        };
        providers.insert(p.id().to_string(), p);
    }
    if cfg.auth.require_relay() && providers.is_empty() {
        eprintln!("[Auth] require = \"relay\", but there is no working provider: nobody will be able to log in");
    }
    let mods = mods::Mods::new(&cfg.mods.dir);
    let releases = release::Releases::new(&cfg.launcher.dir);
    let season = crate::season::active();
    let notes = notes::PatchNotes::new(
        &cfg.patch_notes.file,
        season.id.as_ref().map(|id| notes::SeasonNotes { id: id.clone(), name: season.title.clone(), file: crate::season::dir(id).join(notes::FILE) }),
    );
    let core = Arc::new(AuthCore { cfg, db, providers, pending: sessions::PendingLogins::default(), mods, releases, notes });
    if !core.cfg.web.bind.is_empty() {
        tokio::spawn(web::run(core.clone()));
    }
    if !core.cfg.relay.bind.is_empty() {
        tokio::spawn(relay::run(core.clone()));
    }
    sessions::spawn_cleanup(core);
}
