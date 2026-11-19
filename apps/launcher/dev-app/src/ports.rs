//! Who holds the backend ports, in words a developer can act on, and the shared port lock
//! (`backups\dbtest\portlock.ps1`, one owner at a time across Claude sessions and the user).

use std::path::Path;

use serde::{Deserialize, Serialize};

use dbl::error::Result;
use dbl::winutil::{port_owner, process_path};

use crate::winx::{self, Job};

pub const LOCK_OWNER: &str = "dev-app";
const LOCK_FILE: &str = r"<data>\locks\game-ports.lock";

/// Ports the local backend needs: game HTTP 80/443, dir 8085, gateways 6601-6604, web API 8090, relay 7443.
pub const BACKEND_PORTS: &[(u16, &str)] = &[
    (80, "game HTTP"),
    (443, "game HTTPS"),
    (8085, "Dir"),
    (6601, "gateway"),
    (6602, "gateway"),
    (6603, "gateway"),
    (6604, "gateway"),
    (8090, "web API (server.toml)"),
    (7443, "relay (server.toml)"),
];
/// Shown for information: not a reason to refuse the start.
pub const INFO_PORTS: &[(u16, &str)] = &[(9091, "ds-agent HTTP"), (9092, "admin API")];

#[derive(Debug, Clone, Serialize)]
pub struct PortState {
    pub port: u16,
    pub role: &'static str,
    pub busy: bool,
    pub pid: u32,
    pub process: String,
    pub path: String,
    /// Human explanation of the owner and what to do.
    pub who: String,
    pub ours: bool,
    pub blocking: bool,
}

fn explain(pid: u32, name: &str, ours: bool) -> String {
    if ours {
        return "our backend (started from dev-app)".into();
    }
    match name.to_ascii_lowercase().as_str() {
        _ if pid == 4 => "HTTP.sys (System): a Windows service holds the port — `netsh http show servicestate`, IIS/WinRM/SSDP".into(),
        "dbserver-rs.exe" => "a backend started elsewhere (a terminal or another Claude session) — stop it".into(),
        "cargo.exe" => "cargo (backend build/run started elsewhere)".into(),
        "sololobby.exe" | "solodir.exe" | "loginservice.exe" => format!("PVE server ({name}) — close it or switch to the PVE sniffer profile"),
        "dungeonborne-launcher.exe" => "player launcher: its local relay (leave the game in the launcher)".into(),
        "dbl-mock-server.exe" => "launcher mock (cargo run -p dbl-mock-server)".into(),
        "dungeonborne-dev.exe" => "this app (the admin API mock or a second dev-app window)".into(),
        "dungeonborne.exe" => "the game".into(),
        "svchost.exe" => "Windows service (svchost)".into(),
        "?" => format!("pid {pid}: process name unavailable (another user / elevated rights)"),
        _ => format!("{name} (pid {pid})"),
    }
}

fn state(port: u16, role: &'static str, job: Option<&Job>, blocking_list: bool) -> PortState {
    match port_owner(port) {
        None => PortState { port, role, busy: false, pid: 0, process: String::new(), path: String::new(), who: String::new(), ours: false, blocking: false },
        Some((pid, name)) => {
            let ours = job.is_some_and(|j| j.contains(pid));
            PortState {
                port,
                role,
                busy: true,
                pid,
                path: process_path(pid).map(|p| p.display().to_string()).unwrap_or_default(),
                who: explain(pid, &name, ours),
                process: name,
                ours,
                blocking: blocking_list && !ours,
            }
        }
    }
}

pub fn check(job: Option<&Job>) -> Vec<PortState> {
    BACKEND_PORTS
        .iter()
        .map(|&(p, r)| state(p, r, job, true))
        .chain(INFO_PORTS.iter().map(|&(p, r)| state(p, r, job, false)))
        .collect()
}

pub fn listening(port: u16) -> bool {
    port_owner(port).is_some()
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockInfo {
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub since: String,
    #[serde(default)]
    pub purpose: String,
}

/// Current lock holder (read directly, no PowerShell); None = free.
pub fn lock_status() -> Option<LockInfo> {
    let raw = std::fs::read(LOCK_FILE).ok()?;
    let text = String::from_utf8_lossy(&raw);
    Some(serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(LockInfo {
        owner: "?".into(),
        since: "?".into(),
        purpose: "broken lock".into(),
    }))
}

/// `portlock.ps1 -Acquire`: Ok(text) when we own the lock, Err with the script's explanation otherwise.
pub fn acquire(backups: &Path, purpose: &str) -> Result<String> {
    let script = backups.join("dbtest").join("portlock.ps1");
    let (code, out) = winx::powershell_file(&script, &["-Acquire", "-Owner", LOCK_OWNER, "-Purpose", purpose])?;
    match code {
        0 => Ok(out),
        _ => Err(dbl::error::Error::new("port_lock", out)),
    }
}

pub fn release(backups: &Path) -> Result<String> {
    let script = backups.join("dbtest").join("portlock.ps1");
    let (_, out) = winx::powershell_file(&script, &["-Release", "-Owner", LOCK_OWNER])?;
    Ok(out)
}
