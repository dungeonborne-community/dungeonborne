//! ds-agent — supervisor of game servers on the server machine (Robot, Windows), version 1 (03.11).
//!
//! Every `check_secs`, for each instance (port): is the port held (`netstat -ano -p UDP`), what the backend knows about the slot
//! (`GET <backend>/ds/slots`: phase and age of the mod heartbeat), the decision — `decide`. Start and exit go through the already
//! proven `start_headless.ps1` from the kit (mutexes, logs, `*.new` updates, busy-port check).
//! The game process under ACE is not killed — only `quit` via the mod's control file; if it does not exit — an alert in the log.
//!
//! The agent does not register in the backend (`/ds/register` would route init_player to the agent), it only reads `/ds/slots`.
//! Run: as administrator, in an RDP session (the game needs a session), `ds-agent.exe [path\ds-agent.toml]`.

mod decide;

use decide::{Action, Inst, Limits, Obs, Slot};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Deserialize)]
#[serde(default)]
struct Config {
    /// server kit folder (start_headless.ps1, server.json, logs)
    kit: PathBuf,
    /// backend HTTP for servers (same as the mod's): `http://10.0.0.2:9091`
    backend: String,
    /// public slot address in DS_POOL (without port); empty — from server.json (PublicIp)
    public_ip: String,
    /// instance ports
    ports: Vec<u16>,
    /// idle map (-Map start_headless): waiting hall
    map: String,
    /// per-match process (mod -dbstandby): the server waits for an assignment in the lobby without a map and exits after the match
    standby: bool,
    check_secs: u64,
    #[serde(flatten)]
    limits: LimitsCfg,
    /// port on 127.0.0.1 — protection against a second agent
    lock_port: u16,
}

#[derive(Deserialize)]
#[serde(default)]
struct LimitsCfg {
    start_grace_secs: u64,
    silent_secs: u64,
    stuck_secs: u64,
    quit_wait_secs: u64,
    max_starts: usize,
    window_secs: u64,
    idle_secs: u64,
}

impl Default for LimitsCfg {
    fn default() -> Self {
        let d = Limits::default();
        LimitsCfg {
            start_grace_secs: d.start_grace,
            silent_secs: d.silent,
            stuck_secs: d.stuck,
            quit_wait_secs: d.quit_wait,
            max_starts: d.max_starts,
            window_secs: d.window,
            idle_secs: d.idle,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            kit: PathBuf::from(r"C:\DungeonbornServer\Kit"),
            backend: "http://10.0.0.2:9091".into(),
            public_ip: String::new(),
            ports: vec![7777],
            map: "/TgfMaps/Scenes/Dungeon/Level/L_Dungeon_Matching_01".into(),
            standby: false,
            check_secs: 15,
            limits: LimitsCfg::default(),
            lock_port: 9095,
        }
    }
}

struct Logger {
    file: Option<std::fs::File>,
}

impl Logger {
    fn line(&mut self, msg: &str) {
        let s = format!("{} {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), msg);
        // 10.11: println! panics if the console is unavailable (DbAgent task window) — the agent silently died with code 101.
        // File first, console — without panicking
        if let Some(f) = self.file.as_mut() {
            let _ = writeln!(f, "{}", s);
            let _ = f.flush();
        }
        let _ = writeln!(std::io::stdout().lock(), "{}", s);
    }
}

/// `--check`: one pass — what it sees and what it would do, without start/quit (check over SSH)
fn check_only() -> bool {
    std::env::args().any(|a| a == "--check")
}

fn load_config() -> Result<Config, String> {
    let path = std::env::args().skip(1).find(|a| !a.starts_with("--")).map(PathBuf::from).unwrap_or_else(|| {
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("ds-agent.toml"))).unwrap_or_else(|| "ds-agent.toml".into())
    });
    let mut cfg: Config = match std::fs::read_to_string(&path) {
        Ok(t) => toml::from_str(t.trim_start_matches('\u{feff}')).map_err(|e| format!("{}: {}", path.display(), e))?,
        Err(_) => Config::default(),
    };
    if cfg.public_ip.is_empty() {
        let sj = cfg.kit.join("server.json");
        let v: serde_json::Value = std::fs::read_to_string(&sj)
            .ok()
            .and_then(|t| serde_json::from_str(t.trim_start_matches('\u{feff}')).ok())
            .ok_or_else(|| format!("no public_ip in the config and cannot read {}", sj.display()))?;
        cfg.public_ip = v["PublicIp"].as_str().unwrap_or_default().to_string();
    }
    if cfg.public_ip.is_empty() || cfg.ports.is_empty() {
        return Err("public_ip and ports required".into());
    }
    Ok(cfg)
}

/// UDP ports held by processes: port -> pid
fn udp_ports() -> Option<HashMap<u16, u32>> {
    let out = Command::new("netstat").args(["-ano", "-p", "UDP"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut m = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() >= 4 && f[0].eq_ignore_ascii_case("UDP") {
            if let (Some(port), Ok(pid)) = (f[1].rsplit_once(':').and_then(|(_, p)| p.parse::<u16>().ok()), f[f.len() - 1].parse::<u32>()) {
                m.insert(port, pid);
            }
        }
    }
    Some(m)
}

/// `GET <backend>/ds/slots` → slot address -> (phase, heartbeat age). None — backend did not reply.
fn backend_slots(backend: &str) -> Option<HashMap<String, Slot>> {
    let rest = backend.strip_prefix("http://")?;
    let (host, _) = rest.split_once('/').unwrap_or((rest, ""));
    let addr = host.to_socket_addrs().ok()?.next()?;
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(3)).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(s, "GET /ds/slots HTTP/1.0\r\nHost: {}\r\nConnection: close\r\n\r\n", host).ok()?;
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let (head, body) = text.split_once("\r\n\r\n")?;
    if !head.starts_with("HTTP/1.1 200") && !head.starts_with("HTTP/1.0 200") {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let mut m = HashMap::new();
    for sl in v["slots"].as_array()? {
        let addr = sl["addr"].as_str().unwrap_or_default().to_string();
        m.insert(addr, Slot {
            phase: sl["phase"].as_str().unwrap_or_default().to_string(),
            last_hb_secs: sl["last_hb_secs"].as_u64(),
            want: sl["want"].as_bool(),
        });
    }
    Some(m)
}

/// start_headless.ps1 with arguments; output — line by line into the agent log.
/// No console and in its own process group: the game with -log attaches to the parent's console, and Ctrl+C / closing the
/// agent window killed the servers it had started (03.11 10:26 Robot time — server 7777 vanished on agent restart).
fn headless(kit: &Path, args: &[&str], log: &mut Logger) -> Result<(), String> {
    let script = kit.join("start_headless.ps1");
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script).args(args).current_dir(kit);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    cmd.stdin(std::process::Stdio::null());
    let out = cmd.output().map_err(|e| e.to_string())?;
    // PowerShell output is in the console code page, not UTF-8: Cyrillic may come out garbled
    for line in String::from_utf8_lossy(&out.stdout).lines().chain(String::from_utf8_lossy(&out.stderr).lines()) {
        if !line.trim().is_empty() {
            log.line(&format!("  | {}", line.trim_end()));
        }
    }
    if out.status.success() { Ok(()) } else { Err(format!("code {}", out.status.code().unwrap_or(-1))) }
}

/// System clock time (s): survives an agent restart, unlike Instant
fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// 10.11: when the agent itself started servers — `Kit/run/ds-agent-starts.json` {"7797": unix}. After an agent restart
/// a recently started server is waited for rather than a second one started (until the port is bound and a heartbeat comes it is indistinguishable from "no server")
fn load_starts(path: &Path) -> HashMap<u16, u64> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<HashMap<String, u64>>(&s).ok())
        .map(|m| m.into_iter().filter_map(|(k, v)| k.parse().ok().map(|p| (p, v))).collect())
        .unwrap_or_default()
}

fn save_starts(path: &Path, m: &HashMap<u16, u64>) {
    let j: HashMap<String, u64> = m.iter().map(|(k, v)| (k.to_string(), *v)).collect();
    if let Ok(s) = serde_json::to_string(&j) {
        let _ = std::fs::write(path, s);
    }
}

fn main() {
    let cfg = match load_config() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ds-agent: {}", e);
            std::process::exit(2);
        }
    };
    if check_only() {
        let ports = udp_ports();
        let slots = backend_slots(&cfg.backend);
        println!("backend {}: {}", cfg.backend, if slots.is_some() { "responding" } else { "NOT responding" });
        for p in &cfg.ports {
            let key = format!("{}:{}", cfg.public_ip, p);
            let o = Obs {
                bound: ports.as_ref().map_or(true, |m| m.contains_key(p)),
                slot: slots.as_ref().and_then(|m| m.get(&key).cloned()),
            };
            let mut inst = Inst::new(*p);
            let act = inst.step(0, &o, &Limits::default());
            println!(
                "port {}: held={} (pid {:?}), slot {} = {:?} -> {:?}, action {:?}",
                p,
                o.bound,
                ports.as_ref().and_then(|m| m.get(p)),
                key,
                o.slot,
                inst.state,
                act
            );
        }
        return;
    }
    let _lock = match TcpListener::bind(("127.0.0.1", cfg.lock_port)) {
        Ok(l) => l,
        Err(_) => {
            eprintln!("ds-agent: already running (127.0.0.1:{} is taken)", cfg.lock_port);
            std::process::exit(3);
        }
    };
    let _ = std::fs::create_dir_all(cfg.kit.join("logs"));
    // 10.11: panic — into ds-agent.log (its text used to go to the console window and get lost); the DbAgent task brings the agent back up
    let panic_log = cfg.kit.join("logs").join("ds-agent.log");
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let s = format!("{} AGENT PANIC: {}\n{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"), info, bt);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&panic_log) {
            let _ = writeln!(f, "{}", s);
        }
        let _ = writeln!(std::io::stderr().lock(), "{}", s);
    }));
    let mut log = Logger {
        file: std::fs::OpenOptions::new().create(true).append(true).open(cfg.kit.join("logs").join("ds-agent.log")).ok(),
    };
    let l = &cfg.limits;
    let limits = Limits {
        start_grace: l.start_grace_secs,
        silent: l.silent_secs,
        stuck: l.stuck_secs,
        quit_wait: l.quit_wait_secs,
        max_starts: l.max_starts,
        window: l.window_secs,
        idle: l.idle_secs,
        ..Limits::default()
    };
    log.line(&format!(
        "ds-agent {}: kit {}, backend {}, slots {}:{:?}, map {}, check every {} s",
        env!("CARGO_PKG_VERSION"),
        cfg.kit.display(),
        cfg.backend,
        cfg.public_ip,
        cfg.ports,
        cfg.map,
        cfg.check_secs
    ));

    let starts_path = cfg.kit.join("run").join("ds-agent-starts.json");
    let mut starts = load_starts(&starts_path);
    let boot = unix_now();
    let mut insts: Vec<Inst> = cfg
        .ports
        .iter()
        .map(|p| {
            let mut i = Inst::new(*p);
            if let Some(&at) = starts.get(p).filter(|&&at| boot.saturating_sub(at) < limits.start_grace) {
                i.resume_starting(at);
                log.line(&format!("port {}: started by the previous agent {} s ago — waiting for it, not starting a second one", p, boot - at));
            }
            i
        })
        .collect();
    let mut backend_was_ok = true;
    // the backend just came up: the slot table is empty until the first mod heartbeats — do not judge (04.11 07:44: after 30 s
    // of backend downtime the agent considered a live 7777 in standby "gone" — no port without a map — and started a second process)
    const BACKEND_GRACE: u64 = 60;
    let mut backend_back_at: Option<u64> = None;
    // a pause between starts of different instances: the second starts only after killmutex removes the first one's mutexes,
    // and the first does not create them right away (start_headless -Count waits 45 s, 02.11)
    const START_GAP: u64 = 60;
    let mut last_start: Option<u64> = None;
    loop {
        let now = unix_now();
        let ports = udp_ports();
        let slots = backend_slots(&cfg.backend);
        if slots.is_some() != backend_was_ok {
            backend_was_ok = slots.is_some();
            log.line(if backend_was_ok { "backend responding again" } else { "backend not responding — not judging server health" });
            if backend_was_ok {
                backend_back_at = Some(now);
            }
        }
        // without the backend there is no heartbeat: a process in standby (no map, no port) is indistinguishable from a gone one — do nothing
        if slots.is_none() || backend_back_at.map_or(false, |t| now - t < BACKEND_GRACE) {
            std::thread::sleep(Duration::from_secs(cfg.check_secs.max(5)));
            continue;
        }
        for inst in insts.iter_mut() {
            if inst.state != decide::State::Up && last_start.map_or(false, |t| now - t < START_GAP) {
                continue; // another instance was just started — this one on the next tick
            }
            let o = Obs {
                // netstat failed — treat the port as held (do not start a second server blindly)
                bound: ports.as_ref().map_or(true, |m| m.contains_key(&inst.port)),
                slot: slots.as_ref().and_then(|m| m.get(&format!("{}:{}", cfg.public_ip, inst.port)).cloned()),
            };
            let port_no = inst.port;
            // 10.11: a panic on one slot does not bring down the agent (before — all slots unattended until a manual start)
            let ticked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let before = inst.state.clone();
            let act = inst.step(now, &o, &limits);
            let port = inst.port.to_string();
            match act {
                Action::None => {}
                Action::Start(why) => {
                    last_start = Some(now);
                    log.line(&format!("port {}: start — {}", inst.port, why));
                    let mut args = vec!["-Port", port.as_str(), "-Map", cfg.map.as_str()];
                    if cfg.standby {
                        args.push("-Standby");
                    }
                    starts.insert(inst.port, now);
                    save_starts(&starts_path, &starts);
                    if let Err(e) = headless(&cfg.kit, &args, &mut log) {
                        log.line(&format!("port {}: start_headless: {}", inst.port, e));
                    }
                }
                Action::Quit(why) => {
                    log.line(&format!("port {}: quit — {}", inst.port, why));
                    if let Err(e) = headless(&cfg.kit, &["-Quit", "-Port", &port], &mut log) {
                        log.line(&format!("port {}: start_headless -Quit: {}", inst.port, e));
                    }
                }
                Action::Alert(msg) => log.line(&format!("ALERT port {}: {}", inst.port, msg)),
            }
            if inst.state != before {
                log.line(&format!("port {}: {:?} -> {:?}", inst.port, before, inst.state));
            }
            }));
            if ticked.is_err() {
                log.line(&format!("port {}: processing error (panic, text above) — slot skipped this tick", port_no));
            }
        }
        std::thread::sleep(Duration::from_secs(cfg.check_secs.max(5)));
    }
}
