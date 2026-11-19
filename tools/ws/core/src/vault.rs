//! The vault session (docs/SECRETS.md, "Vault session"): the master password once per working session, not per command.
//!
//! `ws secrets unlock` (or DevKit, "Open the vault") reads the database once and hands the session secrets to a small
//! background process — the vault agent (a copy of ws.exe / DevKit.exe in home/cache/vault, so cargo can still rebuild
//! ws while a session is open). The agent keeps the values in memory only and answers on a named pipe:
//!
//! - **who may ask:** the pipe is created with an ACL for the current Windows user only and rejects remote clients; the
//!   agent also checks the asking process: its exe must be `ws.exe` or `DevKit.exe` under DB_HOME (or CARGO_TARGET_DIR) —
//!   a binary built or installed by this workspace, not an arbitrary script;
//! - **what for:** a secret goes into the session only with `session = true` in secrets.toml and is given out only for
//!   the commands in its `session_for` (`"ws dump"` matches `ws dump …`, `cargo ws dump …`); anything else is refused
//!   and `ws secrets run` falls back to asking for the password, so the human sees the command;
//! - **never:** owner entries (prod, signing, Cloudflare) do not go into a session (secrets.toml refuses
//!   `session = true` on them); values are never written to disk or the log;
//! - **how long:** `--for` (default 4 h, at most 12 h), closes after `--idle` without requests (default 60 min), on
//!   `ws secrets lock`, and with the Windows session (logoff);
//! - **journal:** every request goes to home/runs/vault/<UTC date>.log: time, secret names, result, the asking exe and
//!   the command — no values.
//!
//! The session gives the key, not the permission: decrypting paks still needs the human's word (docs/GAME_DATA.md).
use crate::secrets::{Manifest, Secret, Source};
use crate::Workspace;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const DEFAULT_FOR: u64 = 4 * 3600;
pub const MAX_FOR: u64 = 12 * 3600;
pub const DEFAULT_IDLE: u64 = 3600;
/// The agent gives up if the unlocking process has not handed it the values in this time.
const LOAD_DEADLINE: u64 = 30;
/// Name stem of the agent's exe copies in home/cache/vault.
const AGENT_STEM: &str = "ws-vault-agent";

/// A secret handed out by the session.
#[derive(Debug, Clone)]
pub struct Item {
    pub name: String,
    pub env: String,
    pub value: String,
}

/// An open session: the secrets with the commands each is for, seconds left, seconds until the idle close.
#[derive(Debug, Clone)]
pub struct Status {
    pub secrets: Vec<(String, Vec<String>)>,
    pub left: u64,
    pub idle_left: u64,
}

impl Status {
    pub fn names(&self) -> Vec<String> {
        self.secrets.iter().map(|(n, _)| n.clone()).collect()
    }
}

/// How unlock starts the agent: the arguments for the current exe (ws: `secrets agent`, DevKit: `--vault-agent`).
pub struct Unlock {
    pub for_secs: u64,
    pub idle_secs: u64,
    /// only these session secrets (empty — all of them)
    pub only: Vec<String>,
    pub agent_args: Vec<String>,
}

/// "4h", "90m", "30s", "2h30m"; a bare number — minutes.
pub fn parse_duration(s: &str) -> Result<u64, String> {
    let s = s.trim();
    if let Ok(m) = s.parse::<u64>() {
        return Ok(m * 60);
    }
    let mut total = 0u64;
    let mut num = String::new();
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u64 = num
            .parse()
            .map_err(|_| format!("duration '{s}': use 4h, 90m, 2h30m"))?;
        num.clear();
        total += n * match c {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            _ => return Err(format!("duration '{s}': use 4h, 90m, 2h30m")),
        };
    }
    if !num.is_empty() {
        return Err(format!(
            "duration '{s}': a unit is needed after {num} (h, m, s)"
        ));
    }
    Ok(total)
}

/// The command as the policy sees it: the exe name without folder and `.exe`, lowercase; `cargo <alias> …` becomes
/// `<alias> …` only when the alias comes right after cargo (no `+toolchain`, no `--config`). Empty — never allowed.
pub fn normalize_cmd(cmd: &[String]) -> Vec<String> {
    let stem = |s: &str| {
        let f = s.rsplit(['\\', '/']).next().unwrap_or(s).to_lowercase();
        f.strip_suffix(".exe").map(String::from).unwrap_or(f)
    };
    let mut it = cmd.iter();
    let Some(first) = it.next() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if stem(first) == "cargo" {
        match it.next() {
            Some(a) if !a.starts_with('-') && !a.starts_with('+') => out.push(a.to_lowercase()),
            _ => return Vec::new(),
        }
    } else {
        out.push(stem(first));
    }
    out.extend(it.cloned());
    out
}

/// Does `cmd` match one of the `session_for` patterns ("ws dump" = the command starts with these words)?
pub fn command_allowed(patterns: &[String], cmd: &[String]) -> bool {
    let n = normalize_cmd(cmd);
    patterns.iter().any(|p| {
        let t: Vec<&str> = p.split_whitespace().collect();
        !t.is_empty()
            && n.len() >= t.len()
            && t.iter().zip(&n).all(|(a, b)| a.eq_ignore_ascii_case(b))
    })
}

/// The session secrets of this machine's tier: `session = true`, value kind (the manifest checks the rest).
pub fn session_secrets<'a>(
    m: &'a Manifest,
    tier: &str,
    only: &[String],
) -> Result<Vec<&'a Secret>, String> {
    for n in only {
        match m.secrets.iter().find(|s| &s.name == n) {
            None => return Err(format!("no secret '{n}' in secrets.toml")),
            Some(s) if !s.session => {
                return Err(format!(
                    "'{n}' is not a session secret (secrets.toml session = true)"
                ))
            }
            _ => {}
        }
    }
    let picked: Vec<&Secret> = m
        .secrets
        .iter()
        .filter(|s| s.session && s.allowed(tier) && (only.is_empty() || only.contains(&s.name)))
        .collect();
    if picked.is_empty() {
        return Err("secrets.toml has no session secrets for this vault tier".into());
    }
    Ok(picked)
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// UTC "YYYY-MM-DD" and "YYYY-MM-DDTHH:MM:SSZ" (civil date, Howard Hinnant).
fn utc(secs: u64) -> (String, String) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    let date = format!("{y:04}-{m:02}-{d:02}");
    let time = format!(
        "{date}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    );
    (date, time)
}

/// One line into home/runs/vault/<date>.log (no values ever reach here).
fn journal(home: &Path, op: &str, names: &str, result: &str, exe: &str, cmd: &str) {
    use std::io::Write;
    let (date, time) = utc(now());
    let dir = home.join("runs").join("vault");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(format!("{date}.log")))
    {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let _ = writeln!(
            f,
            "{time}\t{op}\t{}\t{}\t{}\t{}",
            clean(names),
            clean(result),
            clean(exe),
            clean(cmd)
        );
    }
}

/// The pipe of this workspace and user: one session per workspace root.
pub fn pipe_name(ws: &Workspace) -> String {
    use sha2::{Digest, Sha256};
    let user = std::env::var("USERNAME").unwrap_or_default();
    let key = format!(
        "{}|{}",
        ws.root.display().to_string().to_lowercase(),
        user.to_lowercase()
    );
    let h = Sha256::digest(key.as_bytes());
    let hex: String = h.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!(r"\\.\pipe\dbws-vault-{hex}")
}

/// Who the agent trusts as a client, and which agent a client trusts.
#[derive(Clone)]
pub struct Trust {
    /// client exe stems (lowercase, without .exe); empty — any (tests)
    pub client_stems: Vec<String>,
    /// client exe must be under one of these (canonical, lowercase); empty — anywhere (tests)
    pub client_dirs: Vec<String>,
    /// the agent's exe must be here with the AGENT_STEM name; None — not checked (tests)
    pub agent_dir: Option<String>,
}

fn canon_lower(p: &Path) -> String {
    let c = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
    crate::strip_verbatim(&c)
        .display()
        .to_string()
        .to_lowercase()
}

impl Trust {
    pub fn for_workspace(ws: &Workspace) -> Trust {
        let mut dirs = vec![canon_lower(&ws.home())];
        if let Some(t) = std::env::var_os("CARGO_TARGET_DIR").filter(|t| !t.is_empty()) {
            dirs.push(canon_lower(Path::new(&t)));
        }
        Trust {
            // xtask.exe too: `cargo xtask setup` takes artifact_store from the session (session_for "xtask setup")
            client_stems: vec!["ws".into(), "devkit".into(), "xtask".into()],
            client_dirs: dirs,
            agent_dir: Some(canon_lower(&agent_dir(ws))),
        }
    }

    fn client_ok(&self, exe: &str) -> bool {
        let p = exe.to_lowercase();
        let file = p.rsplit('\\').next().unwrap_or(&p);
        let stem = file.strip_suffix(".exe").unwrap_or(file);
        (self.client_stems.is_empty() || self.client_stems.iter().any(|s| s == stem))
            && (self.client_dirs.is_empty()
                || self
                    .client_dirs
                    .iter()
                    .any(|d| p.starts_with(&format!("{d}\\"))))
    }

    fn agent_ok(&self, exe: &str) -> bool {
        let Some(dir) = &self.agent_dir else {
            return true;
        };
        let p = exe.to_lowercase();
        let file = p.rsplit('\\').next().unwrap_or(&p);
        p.starts_with(&format!("{dir}\\")) && file.starts_with(AGENT_STEM)
    }
}

pub fn agent_dir(ws: &Workspace) -> PathBuf {
    ws.home().join("cache").join("vault")
}

// ------------------------------------------------------------------------------------------------------------ client

/// The open session, or None (no agent, or it does not answer).
pub fn status(ws: &Workspace) -> Option<Status> {
    status_on(&pipe_name(ws), &Trust::for_workspace(ws))
}

/// Closes the session. false — there was none.
pub fn lock(ws: &Workspace) -> Result<bool, String> {
    lock_on(&pipe_name(ws), &Trust::for_workspace(ws))
}

/// The values of `names` for the command `cmd` (the command that will receive them). Err — no session, or refused (the
/// message says why, for the fallback to the password).
pub fn get(ws: &Workspace, names: &[String], cmd: &[String]) -> Result<Vec<Item>, String> {
    get_on(&pipe_name(ws), &Trust::for_workspace(ws), names, cmd)
}

/// A secret for this very process: the environment variable, else the session for this process's own command line
/// (`ws dump --apply` asks for game_aes_key as "ws dump …"). None — neither.
pub fn env_or_session(ws: &Workspace, name: &str, env: &str) -> Option<String> {
    if let Some(v) = std::env::var(env).ok().filter(|v| !v.trim().is_empty()) {
        return Some(v);
    }
    let cmd: Vec<String> = std::env::args().collect();
    // ask only when the session holds it for this command (status is not journaled; a doomed request would be)
    let st = status(ws)?;
    let (_, patterns) = st.secrets.iter().find(|(n, _)| n == name)?;
    if !command_allowed(patterns, &cmd) {
        return None;
    }
    get(ws, &[name.to_string()], &cmd)
        .ok()?
        .into_iter()
        .next()
        .map(|i| i.value)
}

fn status_on(pipe: &str, trust: &Trust) -> Option<Status> {
    let r = request(pipe, trust, &json!({"op": "status"}), None).ok()?;
    let secrets = r["secrets"]
        .as_array()?
        .iter()
        .map(|s| {
            let n = s["name"].as_str().unwrap_or("").to_string();
            let f = s["for"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
            (n, f)
        })
        .collect();
    Some(Status {
        secrets,
        left: r["left"].as_u64().unwrap_or(0),
        idle_left: r["idle_left"].as_u64().unwrap_or(0),
    })
}

fn lock_on(pipe: &str, trust: &Trust) -> Result<bool, String> {
    match request(pipe, trust, &json!({"op": "lock"}), None) {
        Ok(_) => Ok(true),
        Err(Error::NoSession) => Ok(false),
        Err(e) => Err(e.to_string()),
    }
}

fn get_on(
    pipe: &str,
    trust: &Trust,
    names: &[String],
    cmd: &[String],
) -> Result<Vec<Item>, String> {
    let r = request(
        pipe,
        trust,
        &json!({"op": "get", "names": names, "cmd": cmd}),
        None,
    )
    .map_err(|e| e.to_string())?;
    Ok(r["values"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| Item {
            name: v["name"].as_str().unwrap_or("").into(),
            env: v["env"].as_str().unwrap_or("").into(),
            value: v["value"].as_str().unwrap_or("").into(),
        })
        .collect())
}

#[derive(Debug)]
enum Error {
    NoSession,
    Refused(String),
    Other(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoSession => write!(f, "no vault session (ws secrets unlock)"),
            Error::Refused(m) => write!(f, "vault session refused: {m}"),
            Error::Other(m) => write!(f, "vault session: {m}"),
        }
    }
}

#[cfg(windows)]
fn request(pipe: &str, trust: &Trust, req: &Value, agent_pid: Option<u32>) -> Result<Value, Error> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::windows::io::AsRawHandle;
    let mut tries = 0;
    let mut f = loop {
        match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(pipe)
        {
            Ok(f) => break f,
            // 231 ERROR_PIPE_BUSY: the agent is answering someone else
            Err(e) if e.raw_os_error() == Some(231) && tries < 100 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            Err(e) if e.raw_os_error() == Some(2) => return Err(Error::NoSession),
            Err(e) => return Err(Error::Other(e.to_string())),
        }
    };
    // the other end must be our agent (or the very process unlock started), not something that took the pipe name
    let pid = win::server_pid(f.as_raw_handle())
        .ok_or_else(|| Error::Other("cannot see the agent process".into()))?;
    if agent_pid.is_some_and(|p| p != pid) {
        return Err(Error::Other(format!(
            "the pipe is served by process {pid}, not the agent just started"
        )));
    }
    let exe = win::image_of(pid).unwrap_or_default();
    if !trust.agent_ok(&exe) {
        return Err(Error::Other(format!(
            "the pipe is served by {exe}, not a vault agent of this workspace"
        )));
    }
    let mut line = req.to_string();
    line.push('\n');
    f.write_all(line.as_bytes())
        .map_err(|e| Error::Other(e.to_string()))?;
    let mut resp = String::new();
    BufReader::new(&f)
        .read_line(&mut resp)
        .map_err(|e| Error::Other(e.to_string()))?;
    let v: Value = serde_json::from_str(&resp)
        .map_err(|_| Error::Other("bad answer from the agent".into()))?;
    if v["ok"].as_bool() == Some(true) {
        Ok(v)
    } else {
        Err(Error::Refused(
            v["error"].as_str().unwrap_or("refused").into(),
        ))
    }
}

#[cfg(not(windows))]
fn request(_: &str, _: &Trust, _: &Value, _: Option<u32>) -> Result<Value, Error> {
    Err(Error::NoSession)
}

/// Opens a session: the values of the session secrets from `src` go to a new agent (an open session is closed first —
/// unlocking again extends it). Returns the status and the session secrets the database has no value for.
#[cfg(windows)]
pub fn unlock(
    ws: &Workspace,
    m: &Manifest,
    src: &Source,
    tier: &str,
    u: &Unlock,
) -> Result<(Status, Vec<String>), String> {
    use std::os::windows::process::CommandExt;
    let picked = session_secrets(m, tier, &u.only)?;
    let mut values = Vec::new();
    let mut missing = Vec::new();
    for s in picked {
        match src.get(s).and_then(|v| String::from_utf8(v).ok()) {
            Some(v) => {
                values.push(json!({"name": s.name, "env": s.env, "value": v, "for": s.session_for}))
            }
            None => missing.push(s.name.clone()),
        }
    }
    if values.is_empty() {
        return Err(format!(
            "the vault has no value for any session secret ({})",
            missing.join(", ")
        ));
    }
    let trust = Trust::for_workspace(ws);
    let pipe = pipe_name(ws);
    let _ = lock_on(&pipe, &trust);
    // the agent runs from a copy: the exe it was built as stays free for cargo to rebuild
    let dir = agent_dir(ws);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let _ = std::fs::remove_file(e.path()); // the ones still running are locked and stay
    }
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let copy = dir.join(format!("{AGENT_STEM}-{}.exe", std::process::id()));
    std::fs::copy(&me, &copy).map_err(|e| format!("{}: {e}", copy.display()))?;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let start = |flags: u32| {
        std::process::Command::new(&copy)
            .args(&u.agent_args)
            .arg(format!("--root={}", ws.root.display()))
            .arg(format!("--parent={}", std::process::id()))
            .current_dir(&ws.root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(flags)
            .spawn()
    };
    // out of the caller's job when allowed, so closing the terminal does not end the session
    let child = start(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| start(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP))
        .map_err(|e| format!("{}: {e}", copy.display()))?;
    let for_secs = u.for_secs.clamp(60, MAX_FOR);
    let idle_secs = u.idle_secs.clamp(60, for_secs);
    let req = json!({"op": "load", "for": for_secs, "idle": idle_secs, "values": values});
    let t0 = std::time::Instant::now();
    loop {
        match request(&pipe, &trust, &req, Some(child.id())) {
            Ok(_) => break,
            Err(Error::NoSession) if t0.elapsed().as_secs() < 10 => {
                std::thread::sleep(std::time::Duration::from_millis(100))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    let st = status_on(&pipe, &trust).ok_or("the agent started but does not answer")?;
    Ok((st, missing))
}

#[cfg(not(windows))]
pub fn unlock(
    _: &Workspace,
    _: &Manifest,
    _: &Source,
    _: &str,
    _: &Unlock,
) -> Result<(Status, Vec<String>), String> {
    Err("the vault session is Windows only for now; use ws secrets run".into())
}

// ------------------------------------------------------------------------------------------------------------- agent

struct Held {
    name: String,
    env: String,
    value: String,
    patterns: Vec<String>,
}

struct Session {
    held: Vec<Held>,
    until: u64,
    idle: u64,
    last: u64,
}

impl Session {
    fn expired(&self, t: u64) -> Option<&'static str> {
        if t >= self.until {
            Some("time is up")
        } else if t >= self.last + self.idle {
            Some("idle")
        } else {
            None
        }
    }
}

/// The agent: `ws secrets agent --root=… --parent=<pid>` / `DevKit.exe --vault-agent …`. Serves until the session
/// ends; the process exits then.
#[cfg(windows)]
pub fn serve(ws: &Workspace, parent: u32) -> Result<(), String> {
    serve_on(
        &pipe_name(ws),
        &ws.home(),
        parent,
        &Trust::for_workspace(ws),
        true,
    )
}

#[cfg(not(windows))]
pub fn serve(_: &Workspace, _: u32) -> Result<(), String> {
    Err("the vault session is Windows only for now".into())
}

/// `exit` — end the process when the session ends (the real agent); tests stop with a `lock` request instead.
#[cfg(windows)]
fn serve_on(pipe: &str, home: &Path, parent: u32, trust: &Trust, exit: bool) -> Result<(), String> {
    use std::sync::{Arc, Mutex};
    let started = now();
    let state: Arc<Mutex<Option<Session>>> = Arc::new(Mutex::new(None));
    let mut h = win::create_pipe(pipe, true)?;
    if exit {
        let st = state.clone();
        let home = home.to_path_buf();
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_secs(5));
            let t = now();
            let why = match st.lock().unwrap().as_ref() {
                None if t >= started + LOAD_DEADLINE => Some("not loaded"),
                None => None,
                Some(s) => s.expired(t),
            };
            if let Some(why) = why {
                journal(&home, "close", "", why, "", "");
                std::process::exit(0);
            }
        });
    }
    // a pipe error ends the session, but never silently (18.11: it used to vanish without a journal line)
    let fatal = |e: String| {
        journal(home, "close", "", &format!("pipe error: {e}"), "", "");
        e
    };
    loop {
        // a client that connected and left before we looked (ERROR_NO_DATA) is not an error: next one
        if !win::connect(h).map_err(fatal)? {
            win::disconnect(h);
            continue;
        }
        // the next instance before this one closes: the name is never free for someone else to take
        let next = win::create_pipe(pipe, false).map_err(fatal)?;
        let stop = handle(h, home, parent, trust, &state);
        win::close(h);
        h = next;
        if stop {
            win::close(h);
            if exit {
                std::process::exit(0);
            }
            return Ok(());
        }
    }
}

/// One request on a connected pipe. true — the session is over (lock).
#[cfg(windows)]
fn handle(
    h: win::Handle,
    home: &Path,
    parent: u32,
    trust: &Trust,
    state: &std::sync::Mutex<Option<Session>>,
) -> bool {
    use std::io::{BufRead, BufReader, Read, Write};
    let file = win::borrow(h);
    let mut line = String::new();
    let _ = BufReader::new((&*file).take(1 << 20)).read_line(&mut line);
    let pid = win::client_pid(h).unwrap_or(0);
    let exe = win::image_of(pid).unwrap_or_default();
    if line.trim().is_empty() {
        // connected and left without asking (a client that did not trust this agent, a probe): note it, go on
        journal(home, "-", "", "left without a request", &exe, "");
        return false;
    }
    let req: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
    let op = req["op"].as_str().unwrap_or("");
    let (resp, stop) = answer(op, &req, pid, parent, &exe, home, trust, state);
    let mut out = resp.to_string();
    out.push('\n');
    let _ = (&*file).write_all(out.as_bytes());
    let _ = file.sync_all(); // FlushFileBuffers: the client has read it before the pipe is disconnected
    stop
}

#[allow(clippy::too_many_arguments)]
fn answer(
    op: &str,
    req: &Value,
    pid: u32,
    parent: u32,
    exe: &str,
    home: &Path,
    trust: &Trust,
    state: &std::sync::Mutex<Option<Session>>,
) -> (Value, bool) {
    let refuse = |m: &str| (json!({"ok": false, "error": m}), false);
    let t = now();
    let mut g = state.lock().unwrap();
    if op == "load" {
        if pid != parent || g.is_some() {
            journal(
                home,
                "load",
                "",
                "refused: not the unlocking process",
                exe,
                "",
            );
            return refuse("load is accepted once, from the process that started the agent");
        }
        let held: Vec<Held> = req["values"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|v| Held {
                name: v["name"].as_str().unwrap_or("").into(),
                env: v["env"].as_str().unwrap_or("").into(),
                value: v["value"].as_str().unwrap_or("").into(),
                patterns: v["for"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect(),
            })
            .collect();
        let for_secs = req["for"]
            .as_u64()
            .unwrap_or(DEFAULT_FOR)
            .clamp(60, MAX_FOR);
        let idle = req["idle"]
            .as_u64()
            .unwrap_or(DEFAULT_IDLE)
            .clamp(60, for_secs);
        let names: Vec<&str> = held.iter().map(|h| h.name.as_str()).collect();
        journal(
            home,
            "open",
            &names.join(","),
            &format!("for {} min, idle {} min", for_secs / 60, idle / 60),
            exe,
            "",
        );
        *g = Some(Session {
            held,
            until: t + for_secs,
            idle,
            last: t,
        });
        return (json!({"ok": true}), false);
    }
    if !trust.client_ok(exe) {
        journal(
            home,
            op,
            "",
            "refused: not a ws/DevKit of this workspace",
            exe,
            "",
        );
        return refuse(
            "only ws.exe, DevKit.exe or xtask.exe of this workspace may ask the vault session",
        );
    }
    let Some(s) = g.as_mut() else {
        return refuse("the session is not loaded yet");
    };
    if let Some(why) = s.expired(t) {
        return refuse(&format!("the session has ended ({why})"));
    }
    match op {
        "status" => {
            let secrets: Vec<Value> = s
                .held
                .iter()
                .map(|h| json!({"name": h.name, "for": h.patterns}))
                .collect();
            (
                json!({"ok": true, "secrets": secrets, "left": s.until - t, "idle_left": (s.last + s.idle).saturating_sub(t)}),
                false,
            )
        }
        "lock" => {
            journal(home, "lock", "", "closed", exe, "");
            *g = None;
            (json!({"ok": true}), true)
        }
        "get" => {
            let names: Vec<String> = req["names"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
            let cmd: Vec<String> = req["cmd"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|x| x.as_str().map(String::from))
                .collect();
            let shown = cmd.join(" ");
            let mut values = Vec::new();
            for n in &names {
                let Some(h) = s.held.iter().find(|h| &h.name == n) else {
                    journal(home, "get", n, "refused: not in the session", exe, &shown);
                    return refuse(&format!("'{n}' is not in the session"));
                };
                if !command_allowed(&h.patterns, &cmd) {
                    journal(home, "get", n, "refused: command not allowed", exe, &shown);
                    return refuse(&format!(
                        "'{n}' is only for: {} (not for this command)",
                        h.patterns.join(", ")
                    ));
                }
                values.push(json!({"name": h.name, "env": h.env, "value": h.value}));
            }
            if values.is_empty() {
                return refuse("no names asked");
            }
            s.last = t;
            journal(home, "get", &names.join(","), "ok", exe, &shown);
            (json!({"ok": true, "values": values}), false)
        }
        _ => refuse("unknown request"),
    }
}

#[cfg(windows)]
mod win {
    //! The few Win32 calls the vault needs: a named pipe for the current user only, and who is on the other end.
    use std::fs::File;
    use std::mem::ManuallyDrop;
    use std::os::windows::io::FromRawHandle;
    use std::path::PathBuf;
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, LocalFree, ERROR_PIPE_CONNECTED, HANDLE, INVALID_HANDLE_VALUE,
    };
    use windows_sys::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
        SDDL_REVISION_1,
    };
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX,
    };
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
        GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
        PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, QueryFullProcessImageNameW,
        PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub type Handle = HANDLE;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide(p: *const u16) -> String {
        let mut n = 0;
        // SAFETY: p is a NUL-terminated string from the system
        unsafe {
            while *p.add(n) != 0 {
                n += 1;
            }
            String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
        }
    }

    /// The SID of the user this process runs as, as a string (S-1-5-21-…).
    fn user_sid() -> Result<String, String> {
        // SAFETY: plain Win32 calls with buffers sized as the API asks
        unsafe {
            let mut tok: HANDLE = std::ptr::null_mut();
            if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut tok) == 0 {
                return Err(format!("OpenProcessToken: {}", GetLastError()));
            }
            let mut len = 0u32;
            GetTokenInformation(tok, TokenUser, std::ptr::null_mut(), 0, &mut len);
            let mut buf = vec![0u8; len as usize];
            let ok = GetTokenInformation(tok, TokenUser, buf.as_mut_ptr().cast(), len, &mut len);
            CloseHandle(tok);
            if ok == 0 {
                return Err(format!("GetTokenInformation: {}", GetLastError()));
            }
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s: *mut u16 = std::ptr::null_mut();
            if ConvertSidToStringSidW(user.User.Sid, &mut s) == 0 {
                return Err(format!("ConvertSidToStringSid: {}", GetLastError()));
            }
            let out = from_wide(s);
            LocalFree(s.cast());
            Ok(out)
        }
    }

    /// A pipe instance only the current user may open (protected DACL: one ACE, full access for that SID), local
    /// clients only. `first` — fail if the name already exists (someone else's pipe, or an agent already running).
    pub fn create_pipe(name: &str, first: bool) -> Result<HANDLE, String> {
        let sddl = wide(&format!("D:P(A;;GA;;;{})", user_sid()?));
        // SAFETY: the descriptor lives until CreateNamedPipeW has copied it, then it is freed
        unsafe {
            let mut sd: *mut core::ffi::c_void = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut sd,
                std::ptr::null_mut(),
            ) == 0
            {
                return Err(format!("security descriptor: {}", GetLastError()));
            }
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd,
                bInheritHandle: 0,
            };
            let open = PIPE_ACCESS_DUPLEX
                | if first {
                    FILE_FLAG_FIRST_PIPE_INSTANCE
                } else {
                    0
                };
            let mode = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
            let w = wide(name);
            let h = CreateNamedPipeW(
                w.as_ptr(),
                open,
                mode,
                PIPE_UNLIMITED_INSTANCES,
                64 * 1024,
                64 * 1024,
                0,
                &sa,
            );
            let err = GetLastError();
            LocalFree(sd);
            if h == INVALID_HANDLE_VALUE {
                return Err(if first && err == 5 {
                    "a vault session is already open (or the pipe name is taken)".into()
                } else {
                    format!("CreateNamedPipe {name}: {err}")
                });
            }
            Ok(h)
        }
    }

    /// Waits for a client. true — connected; false — a client came and already left (ERROR_NO_DATA): disconnect
    /// this instance and wait again.
    pub fn connect(h: HANDLE) -> Result<bool, String> {
        const ERROR_NO_DATA: u32 = 232;
        // SAFETY: h is a pipe handle from create_pipe
        unsafe {
            if ConnectNamedPipe(h, std::ptr::null_mut()) != 0 {
                return Ok(true);
            }
            match GetLastError() {
                ERROR_PIPE_CONNECTED => Ok(true),
                ERROR_NO_DATA => Ok(false),
                e => Err(format!("ConnectNamedPipe: {e}")),
            }
        }
    }

    /// Frees the instance for the next client (after a client that left).
    pub fn disconnect(h: HANDLE) {
        // SAFETY: h is a pipe handle from create_pipe
        unsafe {
            DisconnectNamedPipe(h);
        }
    }

    pub fn close(h: HANDLE) {
        // SAFETY: h is a pipe handle this module created; used once
        unsafe {
            DisconnectNamedPipe(h);
            CloseHandle(h);
        }
    }

    /// The pipe as a File without taking ownership (close() closes it).
    pub fn borrow(h: HANDLE) -> ManuallyDrop<File> {
        // SAFETY: the File never drops, the handle stays valid until close()
        ManuallyDrop::new(unsafe { File::from_raw_handle(h) })
    }

    pub fn client_pid(h: HANDLE) -> Option<u32> {
        let mut pid = 0u32;
        // SAFETY: h is a connected server pipe handle
        (unsafe { GetNamedPipeClientProcessId(h, &mut pid) } != 0).then_some(pid)
    }

    pub fn server_pid(h: std::os::windows::io::RawHandle) -> Option<u32> {
        let mut pid = 0u32;
        // SAFETY: h is an open client pipe handle
        (unsafe { GetNamedPipeServerProcessId(h, &mut pid) } != 0).then_some(pid)
    }

    /// The full exe path of a process.
    pub fn image_of(pid: u32) -> Option<String> {
        // SAFETY: the handle is closed before returning; the buffer size is passed in and out
        unsafe {
            let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if p.is_null() {
                return None;
            }
            let mut buf = vec![0u16; 1024];
            let mut len = buf.len() as u32;
            let ok = QueryFullProcessImageNameW(p, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
            CloseHandle(p);
            (ok != 0).then(|| {
                let s = String::from_utf16_lossy(&buf[..len as usize]);
                crate::strip_verbatim(&PathBuf::from(s))
                    .display()
                    .to_string()
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn commands() {
        let p = v(&["ws dump", "xtask setup"]);
        assert!(command_allowed(&p, &v(&["ws", "dump", "--apply"])));
        assert!(command_allowed(
            &p,
            &v(&[r"X:\h\cache\target\release\ws.exe", "dump"])
        ));
        assert!(command_allowed(
            &p,
            &v(&["cargo", "ws", "dump", "--set=icons", "--apply"])
        ));
        assert!(command_allowed(
            &p,
            &v(&["CARGO.EXE", "xtask", "setup", "--only=ours"])
        ));
        assert!(!command_allowed(&p, &v(&["ws", "bootstrap"])));
        assert!(!command_allowed(&p, &v(&["powershell", "-c", "ws dump"])));
        assert!(!command_allowed(
            &p,
            &v(&["cargo", "--config", "alias.ws='run'", "ws", "dump"])
        ));
        assert!(!command_allowed(
            &p,
            &v(&["cargo", "+nightly", "ws", "dump"])
        ));
        assert!(!command_allowed(&p, &v(&["ws"])));
        assert!(!command_allowed(&p, &[]));
        assert!(!command_allowed(&[], &v(&["ws", "dump"])));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("4h").unwrap(), 4 * 3600);
        assert_eq!(parse_duration("90m").unwrap(), 90 * 60);
        assert_eq!(parse_duration("2h30m").unwrap(), 9000);
        assert_eq!(parse_duration("45").unwrap(), 45 * 60);
        assert!(parse_duration("4x").is_err());
        assert!(parse_duration("4h30").is_err());
    }

    #[test]
    fn utc_dates() {
        assert_eq!(utc(0).1, "1970-01-01T00:00:00Z");
        assert_eq!(utc(1_794_998_000).0, "2026-11-18");
    }

    #[test]
    fn trust_paths() {
        let t = Trust {
            client_stems: v(&["ws", "devkit"]),
            client_dirs: v(&[r"x:\dbdev\home"]),
            agent_dir: Some(r"x:\dbdev\home\cache\vault".into()),
        };
        assert!(t.client_ok(r"X:\DBDev\home\cache\target\release\ws.exe"));
        assert!(t.client_ok(r"X:\DBDev\home\tools\devkit\DevKit.exe"));
        assert!(!t.client_ok(r"X:\DBDev\home\tools\python\python.exe"));
        assert!(!t.client_ok(r"X:\DBDev\homework\ws.exe"));
        assert!(!t.client_ok(r"C:\Temp\ws.exe"));
        assert!(t.agent_ok(r"X:\DBDev\home\cache\vault\ws-vault-agent-123.exe"));
        assert!(!t.agent_ok(r"X:\DBDev\home\cache\vault\evil.exe"));
        assert!(!t.agent_ok(r"C:\Temp\ws-vault-agent-1.exe"));
    }

    /// The agent's decisions without a pipe: who may load, who may ask, what is refused.
    #[test]
    fn agent_answers() {
        let home = std::env::temp_dir().join(format!("dbws-vault-answer-{}", std::process::id()));
        let trust = Trust {
            client_stems: v(&["ws", "devkit"]),
            client_dirs: v(&[r"x:\h"]),
            agent_dir: None,
        };
        let st = std::sync::Mutex::new(None);
        let ws_exe = r"X:\h\cache\target\release\ws.exe";
        let load = json!({"op": "load", "for": 600, "idle": 120, "values": [
            {"name": "artifact_store", "env": "ARTIFACT_STORE_PASSWORD", "value": "pw", "for": ["ws bootstrap"]}]});
        let ask = |exe: &str, op: &str, req: &Value| answer(op, req, 7, 1, exe, &home, &trust, &st);
        // only the unlocking process (parent 1) may load
        assert_eq!(
            answer("load", &load, 7, 1, ws_exe, &home, &trust, &st).0["ok"],
            false
        );
        assert_eq!(
            answer("load", &load, 1, 1, ws_exe, &home, &trust, &st).0["ok"],
            true
        );
        let get = json!({"op": "get", "names": ["artifact_store"], "cmd": ["cargo", "ws", "bootstrap", "--apply"]});
        assert_eq!(ask(ws_exe, "get", &get).0["values"][0]["value"], "pw");
        // a script, or ws.exe from elsewhere, is refused
        assert_eq!(
            ask(r"X:\h\tools\python\python.exe", "get", &get).0["ok"],
            false
        );
        assert_eq!(ask(r"C:\Temp\ws.exe", "get", &get).0["ok"], false);
        assert_eq!(
            ask(r"X:\h\tools\devkit\DevKit.exe", "status", &json!({})).0["ok"],
            true
        );
        let (r, stop) = ask(ws_exe, "lock", &json!({}));
        assert!(r["ok"] == true && stop);
        assert_eq!(ask(ws_exe, "get", &get).0["ok"], false);
        let _ = std::fs::remove_dir_all(&home);
    }

    /// The whole exchange over a real pipe, the agent in a thread of this test process.
    #[cfg(windows)]
    #[test]
    fn pipe_session() {
        let pipe = format!(r"\\.\pipe\dbws-vault-test-{}", std::process::id());
        let home = std::env::temp_dir().join(format!("dbws-vault-test-{}", std::process::id()));
        let any = Trust {
            client_stems: vec![],
            client_dirs: vec![],
            agent_dir: None,
        };
        let me = std::process::id();
        let server = {
            let (pipe, home, any) = (pipe.clone(), home.clone(), any.clone());
            std::thread::spawn(move || serve_on(&pipe, &home, me, &any, false))
        };
        let t0 = std::time::Instant::now();
        let load = json!({"op": "load", "for": 600, "idle": 300, "values": [
            {"name": "game_aes_key", "env": "GAME_AES_KEY", "value": "0xKEY", "for": ["ws dump"]}]});
        loop {
            match request(&pipe, &any, &load, Some(me)) {
                Ok(_) => break,
                Err(Error::NoSession) if t0.elapsed().as_secs() < 5 => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(e) => panic!("{e}"),
            }
        }
        // a second load is refused
        assert!(request(&pipe, &any, &load, None).is_err());
        // clients that connect and leave without a word (18.11: one of these ended the session) change nothing
        for _ in 0..20 {
            drop(
                std::fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(&pipe),
            );
        }
        let st = status_on(&pipe, &any).unwrap();
        assert_eq!(st.names(), v(&["game_aes_key"]));
        assert!(st.left > 500 && st.left <= 600);
        let got = get_on(
            &pipe,
            &any,
            &v(&["game_aes_key"]),
            &v(&["cargo", "ws", "dump", "--apply"]),
        )
        .unwrap();
        assert_eq!(
            (got[0].env.as_str(), got[0].value.as_str()),
            ("GAME_AES_KEY", "0xKEY")
        );
        let e = get_on(
            &pipe,
            &any,
            &v(&["game_aes_key"]),
            &v(&["powershell", "-c", "echo"]),
        )
        .unwrap_err();
        assert!(e.contains("only for: ws dump"), "{e}");
        assert!(get_on(&pipe, &any, &v(&["artifact_store"]), &v(&["ws", "dump"])).is_err());
        assert!(lock_on(&pipe, &any).unwrap());
        server.join().unwrap().unwrap();
        assert!(!lock_on(&pipe, &any).unwrap());
        let log = std::fs::read_dir(home.join("runs").join("vault"))
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        let text = std::fs::read_to_string(log).unwrap();
        assert!(
            text.contains("\topen\tgame_aes_key\t")
                && text.contains("refused: command not allowed")
        );
        assert!(!text.contains("0xKEY"), "the journal must not hold values");
        let _ = std::fs::remove_dir_all(&home);
    }
}
