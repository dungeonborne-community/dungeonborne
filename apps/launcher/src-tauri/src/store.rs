//! Persistent launcher state: `%APPDATA%\DungeonborneLauncher\{settings,servers}.json`.
//! Caches, journals and logs go to `%LOCALAPPDATA%\DungeonborneLauncher\`. No secrets here.

use std::path::{Path, PathBuf};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::error::{bail, Result};
use crate::manifest::Manifest;

pub fn config_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("APPDATA").unwrap_or_default()).join("DungeonborneLauncher")
}

pub fn data_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("DungeonborneLauncher")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Game install roots (the folder that contains `Dungeonborne\Binaries\Win64`). Two slots.
    pub installs: Vec<String>,
    pub lang: String,
    pub dev_mode: bool,
    pub run_as_admin: bool,
    /// The launcher stands in for the game's bootstrap in every install (Steam "Play" opens it), bootstrap.rs.
    pub steam_bootstrap: bool,
    /// Server id -> key of the newest patch notes entry the player has opened (notes.rs). Local only.
    pub notes_seen: std::collections::BTreeMap<String, String>,
    /// Look of the UI: `torch`, `crypt`, `blood` (ui/input.css, `[data-theme]`)
    pub theme: String,
    /// 1.7.2: the game goes through hosts even where the server needs none (`relay.no_hosts`). Set by the player in
    /// Settings → Network or by the launcher itself when the game does not reach the server without hosts (lib.rs).
    pub force_hosts: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            installs: vec![String::new(), String::new()],
            lang: "ru".into(),
            dev_mode: false,
            run_as_admin: true,
            steam_bootstrap: false,
            notes_seen: Default::default(),
            theme: "torch".into(),
            force_hosts: false,
        }
    }
}

/// Emulator profile and public account data from `login/poll`, kept per server in
/// `profiles\<server.id>\profile.json`. A cache: the server hands it out again on the next login.
/// The token itself is in Credential Manager only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    /// Login provider id (`discord`, …) and the account id within it.
    #[serde(default)]
    pub provider: Option<String>,
    pub subject: String,
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub open_id: String,
    pub nick: String,
    #[serde(default)]
    pub roles: Vec<String>,
    pub expires_at: i64,
    #[serde(default)]
    pub logged_in_at: i64,
}

impl Account {
    pub fn validate(&self) -> Result<()> {
        if self.open_id.len() != 17 || !self.open_id.bytes().all(|b| b.is_ascii_digit()) {
            bail!("account_invalid", "open_id must be 17 digits");
        }
        Ok(())
    }

    /// Goldberg reads the name from a one-line text file: no control chars, at most 32 chars.
    pub fn goldberg_nick(&self) -> String {
        let n: String = self.nick.chars().filter(|c| !c.is_control()).take(32).collect();
        if n.trim().is_empty() { "Player".into() } else { n }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerEntry {
    pub url: String,
    pub manifest: Manifest,
    /// TOFU: key seen on first connect.
    #[serde(default)]
    pub trusted_key: Option<String>,
    /// The manifest now carries a different key; login and play are blocked until the player confirms.
    #[serde(default)]
    pub key_changed: bool,
    /// Lives in the server's profile folder, not in servers.json.
    #[serde(skip)]
    pub account: Option<Account>,
}

impl ServerEntry {
    pub fn new(url: String, manifest: Manifest) -> Self {
        Self { url, trusted_key: manifest.key.clone(), manifest, key_changed: false, account: None }
    }

    /// Takes a refreshed manifest for the same URL and applies the TOFU rule.
    pub fn update(&mut self, manifest: Manifest) -> Result<()> {
        if manifest.id != self.manifest.id {
            bail!("server_id_changed", "{} -> {}", self.manifest.id, manifest.id);
        }
        if self.trusted_key.is_none() && !self.key_changed {
            self.trusted_key = manifest.key.clone();
        } else if manifest.key != self.trusted_key {
            self.key_changed = true;
        }
        self.manifest = manifest;
        Ok(())
    }

    pub fn accept_key(&mut self) {
        self.trusted_key = self.manifest.key.clone();
        self.key_changed = false;
    }
}

pub fn load<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

pub fn save<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

pub fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn servers_path() -> PathBuf {
    config_dir().join("servers.json")
}

/// When the last game process started by the launcher exited (unix ms). Survives a launcher restart: the anti-cheat
/// (ACE) needs time to let go of the old process, a start right after Alt+F4 fails with "Anti-cheat system works error".
fn last_game_exit_path() -> PathBuf {
    data_dir().join("last_game_exit.json")
}

pub fn last_game_exit_ms() -> u64 {
    load::<u64>(&last_game_exit_path())
}

pub fn set_last_game_exit_ms(ms: u64) -> Result<()> {
    save(&last_game_exit_path(), &ms)
}

/// The last game run started by the launcher, for bug reports (bugreport.rs): which mod set it had and how it ended.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LastPlay {
    pub server_id: String,
    /// `profile::modset_sha256` of the run
    pub modset: String,
    /// `id@version` of every mod laid out
    pub mods: Vec<String>,
    pub started_at: i64,
    /// 0 while the game runs (or the launcher died with it)
    pub exited_at: i64,
    /// Exit code of the process the launcher started
    pub exit_code: Option<u32>,
}

fn last_play_path() -> PathBuf {
    data_dir().join("last_play.json")
}

pub fn last_play() -> LastPlay {
    load(&last_play_path())
}

pub fn set_last_play(p: &LastPlay) -> Result<()> {
    save(&last_play_path(), p)
}

/// Native-code mods the player allowed for a server: `id@<package sha256>` (a new build asks again).
fn native_ok_path(server_id: &str) -> PathBuf {
    config_dir().join("native_ok").join(format!("{server_id}.json"))
}

pub fn native_ok(server_id: &str) -> Vec<String> {
    load(&native_ok_path(server_id))
}

pub fn allow_native(server_id: &str, keys: &[String]) -> Result<()> {
    let mut ok = native_ok(server_id);
    for k in keys {
        if !ok.contains(k) {
            ok.push(k.clone());
        }
    }
    save(&native_ok_path(server_id), &ok)
}

fn profile_path(server_id: &str) -> PathBuf {
    config_dir().join("profiles").join(server_id).join("profile.json")
}

pub fn load_profile(server_id: &str) -> Option<Account> {
    std::fs::read(profile_path(server_id)).ok().and_then(|b| serde_json::from_slice(&b).ok())
}

pub fn save_profile(server_id: &str, account: Option<&Account>) -> Result<()> {
    let path = profile_path(server_id);
    match account {
        Some(a) => save(&path, a),
        None => {
            if let Some(dir) = path.parent() {
                let _ = std::fs::remove_dir_all(dir);
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(key: Option<&str>) -> Manifest {
        let mut m = crate::manifest::parse(
            br#"{"schema":"dbserver/1","id":"a.b","name":"x","game":{"exe_sha256":"62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3"}}"#,
        )
        .unwrap();
        m.key = key.map(str::to_owned);
        m
    }

    #[test]
    fn tofu() {
        let mut e = ServerEntry::new("u".into(), manifest(Some("ed25519:A")));
        e.update(manifest(Some("ed25519:A"))).unwrap();
        assert!(!e.key_changed);
        e.update(manifest(Some("ed25519:B"))).unwrap();
        assert!(e.key_changed);
        e.update(manifest(Some("ed25519:A"))).unwrap();
        assert!(e.key_changed, "stays blocked until the player confirms");
        e.update(manifest(Some("ed25519:B"))).unwrap();
        e.accept_key();
        assert!(!e.key_changed);
        assert_eq!(e.trusted_key.as_deref(), Some("ed25519:B"));
        e.update(manifest(None)).unwrap();
        assert!(e.key_changed, "dropping the key is a change too");
    }
}
