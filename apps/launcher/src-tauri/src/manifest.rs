//! `dbserver/1` (+ launcher extension: `auth`, `relay`), `dbserverlist/1`, status endpoint.

use std::time::Instant;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{bail, Error, Result};
use crate::net::{self, check_url, is_sha256_hex};

const MANIFEST_LIMIT: usize = 1 << 20;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: String,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// The description in other languages (`{ru, en}`); `description` is the server's main language
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description_i18n: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub contact: Option<String>,
    pub game: Game,
    #[serde(default)]
    pub connect: Option<Connect>,
    #[serde(default)]
    pub mode: Option<Mode>,
    #[serde(default)]
    pub mods: Vec<ModRef>,
    #[serde(default)]
    pub policy: Policy,
    #[serde(default)]
    pub status_url: Option<String>,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub signature: Option<String>,
    #[serde(default)]
    pub auth: Option<Auth>,
    #[serde(default)]
    pub relay: Option<Relay>,
    /// Newest launcher build the server offers (self-update, update.rs); trusted only by its release signature.
    #[serde(default)]
    pub launcher: Option<LauncherRelease>,
    /// Server patch notes (notes.rs): optional, older servers don't send it.
    #[serde(default)]
    pub patch_notes: Option<PatchNotesRef>,
    /// Settings for client UE4SS mods (12.11): `{ "<mod>": { "<section>": { key: value } } }`. Written before the game
    /// starts to `Mods/<mod>/server_settings.ini` of the mods this launcher lays out; older servers don't send it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_settings: Option<serde_json::Value>,
    /// Project support (support.rs, 16.11): where the JSON with this month's server goal and donation links is;
    /// optional, older servers don't send it
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support: Option<SupportRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupportRef {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchNotesRef {
    pub url: String,
    /// Key of the newest entry: the tab is "new" until the player has seen it
    #[serde(default)]
    pub latest: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherRelease {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub exe_sha256: String,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Connect {
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub ca_cert: Option<String>,
    #[serde(default)]
    pub ca_sha256: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mode {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRef {
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub required: bool,
    /// Where to download the `dbmod/1` package: absolute or relative to the manifest URL.
    /// Without it the mod must already be in the cache.
    #[serde(default)]
    pub url: Option<String>,
    /// Package size in bytes, if the server tells.
    #[serde(default)]
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Policy {
    /// `Some(false)` = paks not from the manifest are moved out of `Paks` while the game runs.
    #[serde(default)]
    pub allow_unlisted_client_mods: Option<bool>,
    #[serde(default)]
    pub accounts: Option<String>,
}

/// Login providers are the server's business (the backend is provider-agnostic); the launcher only opens
/// the browser page the server gives it. Accepts the spec form `provider` and the backend form `providers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Auth {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default)]
    pub providers: Vec<String>,
    pub api: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relay {
    pub address: String,
    pub port: u16,
    #[serde(default)]
    pub cert_sha256: Option<String>,
    /// The server's mod points the game at https://localhost (ServerConfig.json of ru.rebuild-private): no hosts lines,
    /// the INTL login through the open INTL config. Absent (older servers, and until that mod is out) — hosts as before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub no_hosts: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Status {
    #[serde(default)]
    pub online: Option<u32>,
    #[serde(default)]
    pub capacity: Option<u32>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub modset_sha256: Option<String>,
    #[serde(default)]
    pub motd: Option<String>,
    /// `motd` in other languages (`{ru, en}`), also the maintenance text
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motd_i18n: Option<std::collections::BTreeMap<String, String>>,
    /// Filled by the launcher: round trip of the status request.
    #[serde(default)]
    pub ping_ms: Option<u32>,
}

/// Ids end up in file paths and credential names, so keep them boring.
pub fn valid_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(&b))
        && !s.starts_with('.')
        && !s.contains("..")
}

fn valid_version_token(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
}

impl Manifest {
    pub fn validate(&mut self) -> Result<()> {
        let major = self.schema.strip_prefix("dbserver/").map(|v| v.split('.').next().unwrap_or(""));
        if major != Some("1") {
            bail!("schema", "{}", self.schema);
        }
        if !valid_id(&self.id) {
            bail!("manifest_invalid", "id {:?}", self.id);
        }
        if self.name.trim().is_empty() {
            bail!("manifest_invalid", "empty name");
        }
        // Empty = the server did not say which exe it serves: it can be added and logged into, but not played.
        self.game.exe_sha256 = self.game.exe_sha256.trim().to_ascii_lowercase();
        if !self.game.exe_sha256.is_empty() && !is_sha256_hex(&self.game.exe_sha256) {
            bail!("manifest_invalid", "game.exe_sha256");
        }
        if let Some(c) = &mut self.connect {
            if let Some(h) = &mut c.ca_sha256 {
                *h = h.to_ascii_lowercase();
                if !is_sha256_hex(h) {
                    bail!("manifest_invalid", "connect.ca_sha256");
                }
            }
            if let Some(u) = &c.ca_cert {
                check_url(u)?;
                if c.ca_sha256.is_none() {
                    bail!("manifest_invalid", "connect.ca_cert without ca_sha256");
                }
            }
        }
        for m in &mut self.mods {
            if !valid_id(&m.id) || !valid_version_token(&m.version) {
                bail!("manifest_invalid", "mod {:?}@{:?}", m.id, m.version);
            }
            if let Some(h) = &mut m.sha256 {
                *h = h.to_ascii_lowercase();
                if !is_sha256_hex(h) {
                    bail!("manifest_invalid", "mod {} sha256", m.id);
                }
            }
            if m.required {
                if m.sha256.is_none() {
                    bail!("manifest_invalid", "required mod {} has no sha256", m.id);
                }
                if semver::Version::parse(&m.version).is_err() {
                    bail!("manifest_invalid", "required mod {} needs an exact version", m.id);
                }
            }
            if let Some(u) = &m.url {
                if u.contains("://") {
                    check_url(u)?;
                }
            }
        }
        if let Some(a) = &mut self.auth {
            if let Some(p) = a.provider.take() {
                if !a.providers.contains(&p) {
                    a.providers.insert(0, p);
                }
            }
            if a.providers.is_empty() || !a.providers.iter().all(|p| valid_id(p)) {
                bail!("manifest_invalid", "auth.providers {:?}", a.providers);
            }
            check_url(&a.api)?;
        }
        if let Some(r) = &mut self.relay {
            if r.address.is_empty() || r.port == 0 {
                bail!("manifest_invalid", "relay address");
            }
            if let Some(p) = &mut r.cert_sha256 {
                *p = p.to_ascii_lowercase();
                if !is_sha256_hex(p) {
                    bail!("manifest_invalid", "relay.cert_sha256");
                }
            }
        }
        if let Some(u) = &self.status_url {
            check_url(u)?;
        }
        // An optional extra: a bad link only hides the notes, the server stays usable
        if self.patch_notes.as_ref().is_some_and(|n| check_url(&n.url).is_err()) {
            self.patch_notes = None;
        }
        if self.support.as_ref().is_some_and(|r| check_url(&r.url).is_err()) {
            self.support = None;
        }
        Ok(())
    }
}

pub async fn fetch(client: &Client, url: &str) -> Result<Manifest> {
    let url = check_url(url)?;
    let body = net::get_bytes(client, &url, MANIFEST_LIMIT).await?;
    parse(&body)
}

pub fn parse(body: &[u8]) -> Result<Manifest> {
    let mut m: Manifest =
        serde_json::from_slice(body).map_err(|e| Error::new("manifest_invalid", e.to_string()))?;
    m.validate()?;
    Ok(m)
}

#[derive(Deserialize)]
struct ServerList {
    schema: String,
    servers: Vec<String>,
}

pub async fn fetch_list(client: &Client, url: &str) -> Result<Vec<String>> {
    let url = check_url(url)?;
    let body = net::get_bytes(client, &url, MANIFEST_LIMIT).await?;
    let list: ServerList = serde_json::from_slice(&body).map_err(|e| Error::new("manifest_invalid", e.to_string()))?;
    if list.schema.strip_prefix("dbserverlist/").map(|v| v.split('.').next().unwrap_or("")) != Some("1") {
        bail!("schema", "{}", list.schema);
    }
    Ok(list.servers)
}

pub async fn status(client: &Client, m: &Manifest) -> Result<Status> {
    let Some(u) = &m.status_url else { return Ok(Status::default()) };
    let url = check_url(u)?;
    let t = Instant::now();
    let body = net::get_bytes(client, &url, 64 * 1024).await?;
    let mut s: Status = serde_json::from_slice(&body)?;
    s.ping_ms = Some(t.elapsed().as_millis() as u32);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "schema": "dbserver/1", "id": "ru.dungeonborne.main", "name": "RU",
      "game": { "exe_sha256": "62D509A00D984696E021FB5DDBA9F87FD1B27C84F695877345FF5FE9401F58B3" },
      "mods": [ { "id": "ru.r14-fix", "version": "0.3.1", "required": true,
                  "sha256": "0000000000000000000000000000000000000000000000000000000000000000" } ],
      "auth": { "provider": "discord", "api": "https://example.org/api/launcher/v1" },
      "relay": { "address": "relay.example.org", "port": 7443 }
    }"#;

    #[test]
    fn parses_and_normalizes() {
        let m = parse(SAMPLE.as_bytes()).unwrap();
        assert!(m.game.exe_sha256.starts_with("62d509"));
        assert_eq!(m.relay.unwrap().port, 7443);
    }

    #[test]
    fn rejects_unknown_major_and_bad_ids() {
        assert_eq!(parse(SAMPLE.replace("dbserver/1", "dbserver/2").as_bytes()).unwrap_err().code, "schema");
        assert!(parse(SAMPLE.replace("ru.dungeonborne.main", "../evil").as_bytes()).is_err());
        assert!(parse(SAMPLE.replace("https://example.org/api", "http://example.org/api").as_bytes()).is_err());
    }

    /// Shape the backend (`src/auth/web.rs`) actually serves.
    #[test]
    fn backend_manifest_shape() {
        let s = r#"{
          "schema": "dbserver/1", "id": "ru.main", "name": "RU", "description": "", "region": "eu",
          "languages": ["ru"], "website": "", "contact": "",
          "game": { "exe_sha256": "62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3", "label": "x" },
          "connect": { "address": "1.2.3.4", "domains": "default", "ports": { "dir": 8085, "lobby": [6601], "https": 443 } },
          "mods": [], "policy": { "allow_unlisted_client_mods": true, "accounts": "local" },
          "status_url": "https://x.org/dbserver/status",
          "auth": { "providers": ["discord", "dev"], "api": "https://x.org/api/launcher/v1", "require": true },
          "relay": { "address": "x.org", "port": 7443, "cert_sha256": null }
        }"#;
        let m = parse(s.as_bytes()).unwrap();
        assert_eq!(m.auth.unwrap().providers, ["discord", "dev"]);
        assert!(m.relay.unwrap().cert_sha256.is_none());
        // Test configs leave the exe unset and the relay off.
        let t = s
            .replace("62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3", "")
            .replace(r#""relay": { "address": "x.org", "port": 7443, "cert_sha256": null }"#, r#""relay": null"#)
            .replace(r#""require": true"#, r#""require": "none""#);
        let m = parse(t.as_bytes()).unwrap();
        assert!(m.game.exe_sha256.is_empty() && m.relay.is_none());
        let spec = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(spec.auth.unwrap().providers, ["discord"]);
    }

    #[test]
    fn patch_notes_are_optional() {
        assert!(parse(SAMPLE.as_bytes()).unwrap().patch_notes.is_none());
        let with = SAMPLE.replace(r#""relay":"#, r#""patch_notes": { "url": "https://x.org/dbserver/patch_notes", "latest": "v1" }, "relay":"#);
        assert_eq!(parse(with.as_bytes()).unwrap().patch_notes.unwrap().latest, "v1");
        let null = SAMPLE.replace(r#""relay":"#, r#""patch_notes": null, "relay":"#);
        assert!(parse(null.as_bytes()).unwrap().patch_notes.is_none());
        let insecure = with.replace("https://x.org/dbserver", "http://x.org/dbserver");
        assert!(parse(insecure.as_bytes()).unwrap().patch_notes.is_none(), "a bad link hides the notes, the manifest stays");
    }

    #[test]
    fn required_mod_needs_hash() {
        let s = SAMPLE.replace(r#""sha256": "0000000000000000000000000000000000000000000000000000000000000000""#, r#""x": 1"#);
        assert!(parse(s.as_bytes()).is_err());
    }
}
