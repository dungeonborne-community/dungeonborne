//! `cargo run` of the backend in `backups` with the profile's environment. Output goes to a ring buffer and to
//! the window in batches; the whole process tree lives in a job object so Stop (or closing the app) ends it.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::sync::mpsc;

use dbl::error::{Error, Result};

use crate::config::Profile;
use crate::winx::{Job, CREATE_NO_WINDOW};

const KEEP_LINES: usize = 20_000;
const BATCH_EVERY: Duration = Duration::from_millis(150);

/// Variables the profile controls: never inherited from the environment dev-app was started in.
const PROFILE_VARS: &[&str] = &["RPC_DUMP", "SERVER_TOML", "DB_PATH", "PVE_SNIFF", "BIND_IP"];

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub seq: u64,
    pub ts: i64,
    pub err: bool,
    /// First `[Tag]` of the line (`[RPC]`, `[Auth]`, `[Relay]`, `[GM]`, …), empty if none.
    pub tag: String,
    pub text: String,
}

#[derive(Default)]
pub struct LogBuf {
    lines: VecDeque<LogLine>,
    next: u64,
}

impl LogBuf {
    pub fn push(&mut self, err: bool, text: String) -> LogLine {
        self.next += 1;
        let line = LogLine { seq: self.next, ts: now_ms(), err, tag: tag_of(&text), text };
        if self.lines.len() >= KEEP_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line.clone());
        line
    }

    pub fn tail(&self, n: usize) -> Vec<LogLine> {
        self.lines.iter().skip(self.lines.len().saturating_sub(n)).cloned().collect()
    }

    pub fn clear(&mut self) {
        self.lines.clear();
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn tag_of(text: &str) -> String {
    let head: String = text.chars().take(48).collect();
    let Some(start) = head.find('[') else { return String::new() };
    let Some(len) = head[start..].find(']') else { return String::new() };
    let tag = &head[start..start + len + 1];
    if tag.len() > 2 && tag[1..tag.len() - 1].chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        tag.to_string()
    } else {
        String::new()
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\u{1b}' {
            if it.peek() == Some(&'[') {
                it.next();
                for c in it.by_ref() {
                    if c.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if c != '\r' {
            out.push(c);
        }
    }
    out
}

pub fn env_for(p: &Profile) -> Vec<(&'static str, String)> {
    let mut env = Vec::new();
    if p.rpc_dump {
        env.push(("RPC_DUMP", "1".to_string()));
    }
    if !p.server_toml.trim().is_empty() {
        env.push(("SERVER_TOML", p.server_toml.trim().to_string()));
    }
    if !p.db_path.trim().is_empty() {
        env.push(("DB_PATH", p.db_path.trim().to_string()));
    }
    if p.pve_sniff {
        env.push(("PVE_SNIFF", "127.0.0.1".to_string()));
    }
    if !p.bind_ip.trim().is_empty() {
        env.push(("BIND_IP", p.bind_ip.trim().to_string()));
    }
    env
}

/// The same start as a PowerShell line, for running by hand in a terminal tab.
pub fn command_line(p: &Profile) -> String {
    let mut s: String = env_for(p).iter().map(|(k, v)| format!("$env:{k}='{v}'; ")).collect();
    s.push_str(if p.release { "cargo run --release" } else { "cargo run" });
    s
}

pub struct Running {
    pub pid: u32,
    pub job: Arc<Job>,
    pub profile: String,
    pub started_ms: i64,
}

pub struct Spawned {
    pub running: Running,
    /// Resolves with the exit code when cargo (and so the backend) is gone.
    pub exited: tokio::sync::oneshot::Receiver<Option<i32>>,
}

/// Starts `cargo run`; every output line goes through `sink` in batches.
pub fn spawn(backups: &Path, p: &Profile, buf: Arc<Mutex<LogBuf>>, sink: Arc<dyn Fn(Vec<LogLine>) + Send + Sync>) -> Result<Spawned> {
    if !backups.join("Cargo.toml").is_file() {
        return Err(Error::new("backups_invalid", format!("no {}\\Cargo.toml", backups.display())));
    }
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.arg("run");
    if p.release {
        cmd.arg("--release");
    }
    for v in PROFILE_VARS {
        cmd.env_remove(v);
    }
    cmd.envs(env_for(p))
        .env("CARGO_TERM_COLOR", "never")
        .current_dir(backups)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .kill_on_drop(false);
    let mut child = cmd.spawn().map_err(|e| Error::new("spawn", format!("cargo: {e}")))?;
    let job = Arc::new(Job::new()?);
    if let Some(h) = child.raw_handle() {
        job.assign(h as _)?;
    }
    let pid = child.id().unwrap_or(0);

    let (tx, mut rx) = mpsc::unbounded_channel::<(bool, String)>();
    if let Some(out) = child.stdout.take() {
        tokio::spawn(read_lines(out, false, tx.clone()));
    }
    if let Some(err) = child.stderr.take() {
        tokio::spawn(read_lines(err, true, tx.clone()));
    }
    drop(tx);
    tokio::spawn(async move {
        let mut batch = Vec::new();
        let mut tick = tokio::time::interval(BATCH_EVERY);
        loop {
            tokio::select! {
                msg = rx.recv() => match msg {
                    Some((err, text)) => batch.push(buf.lock().unwrap().push(err, text)),
                    None => break,
                },
                _ = tick.tick() => if !batch.is_empty() { sink(std::mem::take(&mut batch)) },
            }
        }
        if !batch.is_empty() {
            sink(batch);
        }
    });

    let (done_tx, exited) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let code = child.wait().await.ok().and_then(|s| s.code());
        let _ = done_tx.send(code);
    });
    Ok(Spawned { running: Running { pid, job, profile: p.name.clone(), started_ms: now_ms() }, exited })
}

async fn read_lines(r: impl AsyncRead + Unpin, err: bool, tx: mpsc::UnboundedSender<(bool, String)>) {
    let mut r = BufReader::new(r);
    let mut raw = Vec::new();
    loop {
        raw.clear();
        match r.read_until(b'\n', &mut raw).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let text = strip_ansi(String::from_utf8_lossy(&raw).trim_end_matches(['\n', '\r']));
                if tx.send((err, text)).is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags() {
        assert_eq!(tag_of("[RPC] C2S player_get_info"), "[RPC]");
        assert_eq!(tag_of("  [GM] 7656: [\"gold\"]"), "[GM]");
        assert_eq!(tag_of("[DS-Link] ds-agent HTTP"), "[DS-Link]");
        assert_eq!(tag_of("Compiling foo v0.1"), "");
        assert_eq!(tag_of("[a b] x"), "");
    }

    #[test]
    fn ansi() {
        assert_eq!(strip_ansi("\u{1b}[1m\u{1b}[32mCompiling\u{1b}[0m x\r"), "Compiling x");
    }

    #[test]
    fn env() {
        let p = Profile { rpc_dump: true, pve_sniff: true, db_path: "probe/auth_test/db".into(), ..Profile::default() };
        assert_eq!(command_line(&p), "$env:RPC_DUMP='1'; $env:DB_PATH='probe/auth_test/db'; $env:PVE_SNIFF='127.0.0.1'; cargo run");
    }
}
