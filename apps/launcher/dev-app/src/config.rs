//! Settings and backend profiles: `%APPDATA%\DungeonborneDev\settings.json`. Paths default to the dev machine
//! layout (backups on G:, two game installs); everything is editable in the Settings tab.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub fn data_dir() -> PathBuf {
    std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("DungeonborneDev")
}

pub fn settings_path() -> PathBuf {
    data_dir().join("settings.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Backend crate (`cargo run` here); also the cwd of the python tools.
    pub backups_dir: String,
    /// DbTools exchange folder: request.json, run\ (mod status/control files), logs\ (UE logs).
    pub dbtools_dir: String,
    /// Game installs (root that contains `Dungeonborne\Binaries\Win64`). Index = install number in profiles.
    pub installs: Vec<String>,
    /// Admin API of the backend (ADMIN_API.md); the built-in mock listens on the same port.
    pub admin_url: String,
    /// Launcher web API of the backend (`[web] bind`), used for the admin login when `require = "relay"`.
    pub web_url: String,
    /// ds-agent HTTP of the backend (docs/DS_PIPELINE.md), read-only here.
    pub ds_agent_url: String,
    pub python: String,
    pub profiles: Vec<Profile>,
    pub selected: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            backups_dir: r"<repo>".into(),
            dbtools_dir: r"<data>\dbtools".into(),
            installs: vec![r"C:\Steam\steamapps\common\Dungeonborne".into(), r"C:\Games\Dungeonborne".into()],
            admin_url: "http://127.0.0.1:9092".into(),
            web_url: "http://127.0.0.1:8090".into(),
            ds_agent_url: "http://127.0.0.1:9091".into(),
            python: "python".into(),
            profiles: default_profiles(),
            selected: 0,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HostMode {
    /// No game server: clients play locally / host from lobby themselves.
    #[default]
    None,
    /// Headless server instance (DungeonborneRuServer `-dbserver`); clients connect to 127.0.0.1:7777.
    Listen,
    /// Same server, but clients use the native match button (match-mode.txt = native) and the backend sends them.
    Ds,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ClientMode {
    /// Plain game window, the human logs in and plays.
    #[default]
    Gui,
    /// `-dbclient -dbconnect=127.0.0.1:7777`: the mod logs in and connects to the local game server.
    Connect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub name: String,
    pub rpc_dump: bool,
    /// Path relative to backups (e.g. `probe/auth_test/server.toml`); empty = backend default (`server.toml` if present).
    pub server_toml: String,
    /// Empty = backend default `surreal_saves`.
    pub db_path: String,
    /// `PVE_SNIFF=127.0.0.1` (PVE SoloLobby must be running, see backups/src/main.rs).
    pub pve_sniff: bool,
    /// Empty = backend default 127.0.0.1.
    pub bind_ip: String,
    pub release: bool,
    /// Install indices to start as clients, in order (second and later go after closing the game mutexes).
    pub clients: Vec<usize>,
    pub client_mode: ClientMode,
    pub host: HostMode,
    pub host_install: usize,
    /// Server map (`/TgfMaps/...`), empty = mod default.
    pub host_map: String,
    pub host_window: bool,
    pub host_observer: bool,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            name: "Profile".into(),
            rpc_dump: true,
            server_toml: String::new(),
            db_path: String::new(),
            pve_sniff: false,
            bind_ip: String::new(),
            release: false,
            clients: vec![0],
            client_mode: ClientMode::Gui,
            host: HostMode::None,
            host_install: 0,
            host_map: String::new(),
            host_window: false,
            host_observer: true,
        }
    }
}

fn default_profiles() -> Vec<Profile> {
    vec![
        Profile { name: "Local backend + 1 client".into(), ..Profile::default() },
        Profile { name: "Local backend + 2 clients".into(), clients: vec![0, 1], ..Profile::default() },
        Profile { name: "Backend only".into(), clients: vec![], ..Profile::default() },
        Profile {
            name: "Auth test (relay, dev, test DB)".into(),
            server_toml: "probe/auth_test/server.toml".into(),
            db_path: "probe/auth_test/db".into(),
            clients: vec![],
            ..Profile::default()
        },
        Profile { name: "PVE sniffer".into(), pve_sniff: true, clients: vec![], ..Profile::default() },
        Profile {
            name: "Host: server + client (connect)".into(),
            host: HostMode::Listen,
            clients: vec![1],
            client_mode: ClientMode::Connect,
            ..Profile::default()
        },
        Profile { name: "DS: server + client (native match)".into(), host: HostMode::Ds, clients: vec![1], ..Profile::default() },
    ]
}

impl Settings {
    pub fn load() -> Self {
        let mut s: Settings = dbl::store::load(&settings_path());
        if s.profiles.is_empty() {
            s.profiles = default_profiles();
        }
        if s.selected >= s.profiles.len() {
            s.selected = 0;
        }
        s
    }

    pub fn save(&self) -> dbl::error::Result<()> {
        dbl::store::save(&settings_path(), self)
    }

    pub fn backups(&self) -> PathBuf {
        PathBuf::from(&self.backups_dir)
    }

    pub fn dbtools(&self) -> PathBuf {
        PathBuf::from(&self.dbtools_dir)
    }
}
