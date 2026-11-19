//! Game instances: client windows and the headless game server, started with admin rights as before
//! (UAC through the launcher's winutil, or directly when dev-app itself runs elevated). A second instance needs
//! the single-instance mutexes closed first: DbTools task when it is registered (no UAC), else killmutex.ps1
//! elevated. Arguments mirror `backups\dbtest\dbtools.ps1` / `client-E.bat`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use dbl::error::{Error, Result};
use dbl::profile::Install;
use dbl::winutil;

use crate::winx;

pub const GAME_EXE: &str = "Dungeonborne.exe";
const DBTOOLS_TASK: &str = "DbTools";
const DBTOOLS_LOG: &str = r"C:\ProgramData\DbTools\dbtools.log";

#[derive(Debug, Clone, Serialize)]
pub struct InstallState {
    pub index: usize,
    pub root: String,
    pub ok: bool,
    pub error: String,
    /// `ue4ss\UE4SS.log` is held open: this install's game is running (process paths are hidden under ACE).
    pub running: bool,
    pub ue4ss: bool,
}

pub fn install_state(index: usize, root: &str) -> InstallState {
    match Install::open(root) {
        Ok(inst) => InstallState {
            index,
            root: root.to_string(),
            ok: true,
            error: String::new(),
            running: log_locked(&inst.win64().join("ue4ss").join("UE4SS.log")),
            ue4ss: inst.win64().join("dwmapi.dll").is_file(),
        },
        Err(e) => InstallState { index, root: root.to_string(), ok: false, error: e.to_string(), running: false, ue4ss: false },
    }
}

/// Read-only open with no sharing: fails while the game writes the log.
fn log_locked(path: &Path) -> bool {
    use std::os::windows::fs::OpenOptionsExt;
    if !path.is_file() {
        return false;
    }
    std::fs::OpenOptions::new().read(true).share_mode(0).open(path).is_err()
}

pub fn game_processes() -> usize {
    winutil::processes().iter().filter(|p| p.name.eq_ignore_ascii_case(GAME_EXE)).count()
}

/// Cached for a minute: the UI polls state every few seconds.
pub fn dbtools_registered() -> bool {
    static CACHE: std::sync::Mutex<Option<(Instant, bool)>> = std::sync::Mutex::new(None);
    let mut c = CACHE.lock().unwrap();
    if let Some((at, v)) = *c {
        if at.elapsed() < Duration::from_secs(60) {
            return v;
        }
    }
    let v = matches!(winx::run_hidden("schtasks.exe", &["/Query", "/TN", DBTOOLS_TASK], None), Ok((0, _)));
    *c = Some((Instant::now(), v));
    v
}

/// Starts the game exe of an install with arguments; elevated unless dev-app already is.
pub fn launch(root: &str, args: &[String]) -> Result<()> {
    let inst = Install::open(root)?;
    let elevate = !winx::is_elevated();
    // The handle is of the loader: under ACE the game restarts itself, nothing to wait for here.
    winutil::shell_execute(&inst.exe(), &winx::join_args(args), Some(&inst.win64()), elevate, false).map(drop)
}

/// `-dbroot` / logs of the n-th auto-connecting client: first uses `run`, the others `run2`, `run3`…
fn run_dir(dbtools: &Path, slot: usize) -> PathBuf {
    dbtools.join(if slot == 0 { "run".to_string() } else { format!("run{}", slot + 1) })
}

fn fwd(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/")
}

pub fn client_args(dbtools: &Path, slot: usize, connect: Option<&str>) -> Vec<String> {
    let Some(addr) = connect else { return vec![] };
    let log = dbtools.join("logs").join(format!("ue-client{}-7777.log", slot + 1));
    let _ = std::fs::create_dir_all(dbtools.join("logs"));
    let _ = std::fs::create_dir_all(run_dir(dbtools, slot));
    vec![
        "-dbclient".into(),
        format!("-dbconnect={addr}"),
        format!("-ABSLOG=\"{}\"", log.display()),
        "-log".into(),
        "-nosplash".into(),
        "-dbdelay=5".into(),
        format!("-dbroot={}", fwd(&run_dir(dbtools, slot))),
    ]
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ServerOpts {
    #[serde(default)]
    pub map: String,
    #[serde(default)]
    pub window: bool,
    #[serde(default = "yes")]
    pub observer: bool,
    #[serde(default)]
    pub probe: bool,
    /// `ip:port` of a DSA agent for the DS SDK (`-dsaip/-dsaport`, see DS_PIPELINE.md 2a).
    #[serde(default)]
    pub dsa: String,
}

fn yes() -> bool {
    true
}

pub fn server_args(dbtools: &Path, o: &ServerOpts) -> Result<Vec<String>> {
    let run = dbtools.join("run");
    let log = dbtools.join("logs").join("ue-server-7777.log");
    std::fs::create_dir_all(&run)?;
    std::fs::create_dir_all(dbtools.join("logs"))?;
    let mut a = vec![
        "-dbserver".to_string(),
        format!("-ABSLOG=\"{}\"", log.display()),
        "-log".into(),
        "-unattended".into(),
        "-nosplash".into(),
        "-dbdelay=5".into(),
        format!("-dbroot={}", fwd(&run)),
    ];
    let map = o.map.trim();
    if !map.is_empty() {
        let valid = map.starts_with("/TgfMaps/") && map[9..].chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/');
        if !valid {
            return Err(Error::new("bad_map", "map: /TgfMaps/..."));
        }
        a.push(format!("-dbmap={map}"));
    }
    if !o.observer {
        a.push("-dbobserver=0".into());
    }
    let dsa = o.dsa.trim();
    if !dsa.is_empty() {
        let (ip, port) = dsa.rsplit_once(':').ok_or_else(|| Error::new("bad_dsa", "dsa: ip:port"))?;
        if ip.parse::<std::net::Ipv4Addr>().is_err() || port.parse::<u16>().is_err() {
            return Err(Error::new("bad_dsa", "dsa: ip:port"));
        }
        a.extend([format!("-dsaip={ip}"), format!("-dsaport={port}"), "-starttoken=dbtest".into(), "-startmode=0".into()]);
    }
    if !o.window {
        a.extend(["-nullrhi".into(), "-nosound".into()]);
    }
    if o.probe {
        a.push("-dbprobe".into());
    }
    Ok(a)
}

/// Closes the single-instance mutexes in every running game. Returns the log of the run.
pub fn killmutex(backups: &Path, dbtools: &Path) -> Result<String> {
    if game_processes() == 0 {
        return Ok("the game is not running — nothing to kill".into());
    }
    if dbtools_registered() {
        return dbtools_request(dbtools, r#"{"action":"killmutex"}"#);
    }
    let script = backups.join("dbtest").join("killmutex.ps1");
    if !script.is_file() {
        return Err(Error::new("not_found", script.display().to_string()));
    }
    let params = format!("-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File \"{}\"", script.display());
    let ps = PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
    let proc = winutil::shell_execute(&ps, &params, Some(script.parent().unwrap()), !winx::is_elevated(), true)?;
    proc.wait();
    let log = script.with_file_name("killmutex.log");
    Ok(read_text(&log).unwrap_or_else(|| format!("no {}", log.display())))
}

/// One whitelisted request to the DbTools admin task (dbtest\README_admin.md): request.json → task → log tail.
pub fn dbtools_request(dbtools: &Path, json: &str) -> Result<String> {
    let req = dbtools.join("request.json");
    std::fs::create_dir_all(dbtools)?;
    let before = read_text(Path::new(DBTOOLS_LOG)).map(|t| t.lines().count()).unwrap_or(0);
    std::fs::write(&req, json)?;
    let (code, out) = winx::run_hidden("schtasks.exe", &["/Run", "/TN", DBTOOLS_TASK], None)?;
    if code != 0 {
        let _ = std::fs::remove_file(&req);
        return Err(Error::new("dbtools", out));
    }
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline && req.exists() {
        std::thread::sleep(Duration::from_millis(300));
    }
    if req.exists() {
        let _ = std::fs::remove_file(&req);
        return Err(Error::new("dbtools", "the DbTools task did not pick up the request within 20 s"));
    }
    std::thread::sleep(Duration::from_millis(1500));
    let log = read_text(Path::new(DBTOOLS_LOG)).unwrap_or_default();
    Ok(log.lines().skip(before).collect::<Vec<_>>().join("\n"))
}

pub fn read_text(path: &Path) -> Option<String> {
    let raw = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&raw).trim_start_matches('\u{feff}').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_args_like_dbtools() {
        let d = std::env::temp_dir().join("dbdev-test-dbtools");
        let a = server_args(&d, &ServerOpts { observer: false, map: "/TgfMaps/Crypt/Classic".into(), ..Default::default() }).unwrap();
        assert_eq!(a[0], "-dbserver");
        assert!(a.contains(&"-dbmap=/TgfMaps/Crypt/Classic".to_string()));
        assert!(a.contains(&"-dbobserver=0".to_string()));
        assert!(a.contains(&"-nullrhi".to_string()));
        assert!(server_args(&d, &ServerOpts { map: "/TgfMaps/x;calc".into(), ..Default::default() }).is_err());
        assert!(server_args(&d, &ServerOpts { dsa: "127.0.0.1:9090".into(), ..Default::default() }).unwrap().contains(&"-dsaport=9090".to_string()));
    }

    #[test]
    fn client_slots() {
        let d = std::env::temp_dir().join("dbdev-test-dbtools");
        assert!(client_args(&d, 0, None).is_empty());
        let a = client_args(&d, 1, Some("127.0.0.1:7777"));
        assert!(a.iter().any(|s| s.ends_with("/run2")));
    }
}
