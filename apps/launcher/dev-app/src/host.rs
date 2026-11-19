//! Host Game (docs/DS_PIPELINE.md): the headless game server is the client exe with DungeonborneRuServer
//! (`-dbserver`); the mod reports its phase in `<dbtools>\run\dbserver-status.json` and takes commands from
//! `dbserver-control.txt` (quit / probe). DS mode also switches the match button of the clients to the native
//! flow (`RebuildProbe\match-mode.txt = native`) and restores the old value afterwards, like dbtest\play.ps1.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::Serialize;

use dbl::error::{Error, Result};

use crate::clients::read_text;
use crate::config;

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    /// Raw status JSON of the mod (phase, map, …); empty if the file is missing.
    pub raw: String,
    pub phase: String,
    /// Seconds since the status file was written.
    pub age_s: i64,
    pub match_mode: String,
}

fn run_dir(dbtools: &Path) -> PathBuf {
    dbtools.join("run")
}

pub fn status(dbtools: &Path) -> ServerStatus {
    let path = run_dir(dbtools).join("dbserver-status.json");
    let raw = read_text(&path).unwrap_or_default();
    let phase = serde_json::from_str::<serde_json::Value>(raw.trim())
        .ok()
        .and_then(|v| v.get("phase").and_then(|p| p.as_str()).map(str::to_string))
        .unwrap_or_default();
    let age_s = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(-1);
    ServerStatus { raw: raw.trim().to_string(), phase, age_s, match_mode: match_mode().unwrap_or_default() }
}

/// Clears the control files before a new server so a stale `quit` does not stop it at once.
pub fn reset_control(dbtools: &Path) -> Result<()> {
    let run = run_dir(dbtools);
    std::fs::create_dir_all(&run)?;
    for f in ["dbserver-control.txt", "dbclient-match.txt"] {
        std::fs::write(run.join(f), b"")?;
    }
    std::fs::write(run.join("dbserver-fakeds.txt"), b"0")?;
    Ok(())
}

/// Direct control-file write. For the server only `quit`: portal/exits/probe go through the backend admin API
/// (`/admin/v1/host/command`), which also waits for the mod's answer.
pub fn control(dbtools: &Path, role: &str, word: &str) -> Result<()> {
    let ok = match role {
        "dbserver" => word == "quit",
        "dbclient" => matches!(word, "quit" | "probe"),
        _ => false,
    };
    if !ok {
        return Err(Error::new("bad_control", format!("{role}: {word}")));
    }
    std::fs::write(run_dir(dbtools).join(format!("{role}-control.txt")), word.as_bytes())?;
    Ok(())
}

fn match_mode_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(r"Dungeonborne\Saved\RebuildProbe\match-mode.txt"))
}

fn backup_path() -> PathBuf {
    config::data_dir().join("match-mode.before.txt")
}

pub fn match_mode() -> Option<String> {
    read_text(&match_mode_path()?).map(|s| s.trim().to_string())
}

/// `native` for the duration of a DS session; the previous value (or its absence) is kept for `restore`.
pub fn set_native() -> Result<()> {
    let path = match_mode_path().ok_or_else(|| Error::new("no_localappdata", ""))?;
    let backup = backup_path();
    if !backup.exists() {
        std::fs::create_dir_all(config::data_dir())?;
        let was = read_text(&path).unwrap_or_else(|| "<none>".into());
        std::fs::write(&backup, was)?;
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(&path, b"native")?;
    Ok(())
}

pub fn restore_match_mode() -> Result<bool> {
    let backup = backup_path();
    let Some(was) = read_text(&backup) else { return Ok(false) };
    let path = match_mode_path().ok_or_else(|| Error::new("no_localappdata", ""))?;
    if was == "<none>" {
        let _ = std::fs::remove_file(&path);
    } else {
        std::fs::write(&path, was)?;
    }
    std::fs::remove_file(backup)?;
    Ok(true)
}

/// `GET /ds/agents` of the backend (debug view of registered ds-agents, their queues and pending replies).
pub async fn ds_agents(http: &reqwest::Client, base: &str) -> Result<serde_json::Value> {
    let url = dbl::net::check_url(&format!("{}/ds/agents", base.trim_end_matches('/')))?;
    let resp = http.get(url).timeout(std::time::Duration::from_secs(3)).send().await?;
    if !resp.status().is_success() {
        return Err(Error::new("http_status", resp.status().to_string()));
    }
    resp.json().await.map_err(Error::from)
}
