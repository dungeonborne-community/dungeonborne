//! "Report a bug" (backend `POST bug_report`, contract `docs/specs/bug-report-api.md`): the player's text,
//! versions and the tails of four logs, sent with the player's session; the bot posts it to the Discord forum.
//! No tokens here: the logs never hold them.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::store::LastPlay;

pub const TITLE_MAX: usize = 100;
pub const DESC_MAX: usize = 1500;
/// All files together: the bot's Discord attachment limit is 10 MB
pub const TOTAL_MAX: u64 = 9 << 20;
const MB: u64 = 1 << 20;
/// A crash is offered only for a game run that ended within this time
const CRASH_FRESH_S: i64 = 24 * 3600;
/// UE writes this line on a clean exit; a crash leaves the log without it
const UE_CLOSED: &[u8] = b"Log file closed";

pub const KINDS: &[&str] = &["crash", "game", "launcher"];

#[derive(Debug, Clone)]
pub struct Source {
    /// Part name and file name in the request
    pub name: &'static str,
    pub path: PathBuf,
    pub limit: u64,
}

/// `%LOCALAPPDATA%\Dungeonborne\Saved`: the game's own folder for logs and settings; the game's "Support" button
/// (UnLua pack, LOCAL session) talks to the launcher through two files here
pub fn game_saved_dir() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default()).join("Dungeonborne").join("Saved")
}

/// `...\Saved\Logs\Dungeonborne.log`: the game's own log (the previous run's goes to `-backup-`)
pub fn ue_log_path() -> PathBuf {
    game_saved_dir().join("Logs").join("Dungeonborne.log")
}

// ---------------------------------------------------------------------------------------------- the game's button

/// Rewritten every `PULSE_EVERY_S` while the game started by the launcher runs, removed when it exits. The pack
/// counts the launcher alive while `now - ts < 2 * PULSE_EVERY_S`.
pub const PULSE_FILE: &str = "launcher_alive";
pub const PULSE_EVERY_S: u64 = 5;
/// Written by the pack (via `.tmp` + rename): open the report form. Read and removed by the launcher.
pub const REQUEST_FILE: &str = "bug_report_request.json";
/// An older request is dropped (the launcher was busy, the clock moved)
const REQUEST_MAX_AGE_S: i64 = 300;
const REQUEST_MAX_BYTES: u64 = 4096;

pub fn write_pulse(dir: &Path, now: i64) {
    let body = json!({ "ts": now, "pid": std::process::id(), "version": env!("CARGO_PKG_VERSION") }).to_string();
    let tmp = dir.join(format!("{PULSE_FILE}.tmp"));
    if std::fs::create_dir_all(dir).is_ok() && std::fs::write(&tmp, body).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(PULSE_FILE));
    }
}

pub fn remove_pulse(dir: &Path) {
    let _ = std::fs::remove_file(dir.join(PULSE_FILE));
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameRequest {
    /// Where the button was pressed: `Setting` | `Login` | `Market`
    pub scene: String,
    /// Character name, if the game knew it
    pub role: String,
}

/// The pack's request, if there is one: always removed; `None` when missing, too old or not ours
pub fn take_request(dir: &Path, now: i64) -> Option<GameRequest> {
    let path = dir.join(REQUEST_FILE);
    let md = std::fs::metadata(&path).ok()?;
    let body = if md.len() <= REQUEST_MAX_BYTES { std::fs::read(&path).ok() } else { None };
    let _ = std::fs::remove_file(&path);
    let v: Value = serde_json::from_slice(&body?).ok()?;
    let ts = v["ts"].as_i64().unwrap_or(now);
    if v["source"].as_str().is_some_and(|s| s != "game") || (now - ts).abs() > REQUEST_MAX_AGE_S {
        return None;
    }
    Some(GameRequest {
        scene: clean(v["scene"].as_str().unwrap_or_default(), 32, false),
        role: clean(v["role"].as_str().unwrap_or_default(), 64, false),
    })
}

/// The game just exited: does it look like a crash? (`last` is the run that ended, already saved)
pub fn crash_after_exit(install_root: &Path, last: &LastPlay, now: i64) -> bool {
    let crash_age = age_s(&crash_log_path(install_root));
    let ue_tail = tail(&ue_log_path(), 4096);
    crash_likely(false, crash_age, last, now, ue_tail.as_deref())
}

/// `crash_ue4ss.log` of the install: the client mods' log, rewritten on every game start
pub fn crash_log_path(install_root: &Path) -> PathBuf {
    install_root.join("Dungeonborne").join("Binaries").join("Win64").join("crash_ue4ss.log")
}

/// The four logs of the contract; `crash_ue4ss.log` only when the game folder is known
pub fn sources(logs_dir: &Path, install_root: Option<&Path>) -> Vec<Source> {
    let mut v = vec![
        Source { name: "launcher.log", path: logs_dir.join("launcher.log"), limit: 2 * MB },
        Source { name: "relay.log", path: logs_dir.join("relay.log"), limit: 2 * MB },
    ];
    if let Some(root) = install_root {
        v.push(Source { name: "crash_ue4ss.log", path: crash_log_path(root), limit: 2 * MB });
    }
    v.push(Source { name: "Dungeonborne.log", path: ue_log_path(), limit: 3 * MB });
    v
}

fn cut_to_line(buf: &mut Vec<u8>) {
    if let Some(i) = buf.iter().position(|&b| b == b'\n') {
        buf.drain(..=i);
    }
}

/// The last `limit` bytes of a log as UTF-8 text, starting on a whole line when cut. `None` when missing or empty.
/// Opened for reading only: the game keeps its log open, Windows lets others read it.
/// The server takes text logs only (no NUL bytes, or UTF-16 with its BOM): a tail of a UTF-16 log (UE can write one)
/// loses the BOM, so it goes as UTF-8; NUL bytes (the zero-filled end a crash may leave) are dropped.
pub fn tail(path: &Path, limit: u64) -> Option<Vec<u8>> {
    let mut f = std::fs::File::open(path).ok()?;
    let len = f.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let mut bom = [0u8; 2];
    let utf16 = len >= 2 && f.read_exact(&mut bom).is_ok() && bom == [0xFF, 0xFE];
    // UTF-16: read twice as much (ASCII halves in UTF-8), from an even offset after the BOM
    let want = if utf16 { limit * 2 } else { limit };
    let mut start = len.saturating_sub(want);
    if utf16 {
        start = start.max(2);
        start += start % 2;
    }
    f.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    f.take(want).read_to_end(&mut buf).ok()?;
    let mut cut = start > if utf16 { 2 } else { 0 };
    if utf16 {
        let units: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        buf = String::from_utf16_lossy(&units).into_bytes();
        if buf.len() as u64 > limit {
            buf.drain(..buf.len() - limit as usize);
            cut = true;
        }
    }
    buf.retain(|&b| b != 0);
    if cut {
        cut_to_line(&mut buf);
    }
    (!buf.is_empty()).then_some(buf)
}

/// Cuts the biggest files from the front until all of them fit in `total`; drops files left empty
pub fn fit(files: &mut Vec<(String, Vec<u8>)>, total: u64) {
    loop {
        let sum: u64 = files.iter().map(|f| f.1.len() as u64).sum();
        if sum <= total {
            break;
        }
        let over = (sum - total) as usize;
        let big = files.iter_mut().max_by_key(|f| f.1.len()).expect("sum > 0");
        let cut = over.min(big.1.len());
        big.1.drain(..cut);
        cut_to_line(&mut big.1);
    }
    files.retain(|f| !f.1.is_empty());
}

/// Tails of every source that exists, fitted into `TOTAL_MAX`
pub fn collect(sources: &[Source]) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> =
        sources.iter().filter_map(|s| tail(&s.path, s.limit).map(|b| (s.name.to_string(), b))).collect();
    fit(&mut files, TOTAL_MAX);
    files
}

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub name: String,
    pub size: u64,
}

/// What would go: names and sizes after the limits (no reading of whole files)
pub fn preview(sources: &[Source]) -> Vec<FileInfo> {
    let mut sizes: Vec<(String, u64)> = sources
        .iter()
        .filter_map(|s| std::fs::metadata(&s.path).ok().filter(|m| m.len() > 0).map(|m| (s.name.to_string(), m.len().min(s.limit))))
        .collect();
    // same rule as `fit`, on sizes only
    loop {
        let sum: u64 = sizes.iter().map(|f| f.1).sum();
        if sum <= TOTAL_MAX {
            break;
        }
        let big = sizes.iter_mut().max_by_key(|f| f.1).expect("sum > 0");
        big.1 = big.1.saturating_sub(sum - TOTAL_MAX);
    }
    sizes.into_iter().filter(|f| f.1 > 0).map(|(name, size)| FileInfo { name, size }).collect()
}

/// Player text: trimmed, no control characters (newlines kept when `multiline`), at most `max` characters
pub fn clean(s: &str, max: usize, multiline: bool) -> String {
    let s: String = s
        .replace("\r\n", "\n")
        .chars()
        .map(|c| if c == '\t' { ' ' } else { c })
        .filter(|&c| !c.is_control() || (multiline && c == '\n'))
        .collect();
    s.trim().chars().take(max).collect::<String>().trim_end().to_string()
}

/// Offer "crash": the game is not running, it ran within a day (fresh `crash_ue4ss.log`) and either its process
/// exited with an error code or the game's log has no "Log file closed" at the end.
pub fn crash_likely(running: bool, crash_log_age_s: Option<i64>, last: &LastPlay, now: i64, ue_tail: Option<&[u8]>) -> bool {
    if running || !crash_log_age_s.is_some_and(|a| a <= CRASH_FRESH_S) {
        return false;
    }
    let bad_exit = last.exit_code.is_some_and(|c| c != 0) && last.exited_at > 0 && now - last.exited_at <= CRASH_FRESH_S;
    let unclosed = ue_tail.is_some_and(|t| !t.windows(UE_CLOSED.len()).any(|w| w == UE_CLOSED));
    bad_exit || unclosed
}

/// Seconds since the file was written, `None` when missing
pub fn age_s(path: &Path) -> Option<i64> {
    let m = std::fs::metadata(path).ok()?.modified().ok()?;
    Some(std::time::SystemTime::now().duration_since(m).map(|d| d.as_secs() as i64).unwrap_or(0))
}

/// `sha256` over sorted `id@version:sha256` of the manifest's mods: the same rule as `profile::modset_sha256`, for a
/// server the player has not played on yet
pub fn manifest_modset(mods: &[crate::manifest::ModRef]) -> String {
    let mut lines: Vec<String> =
        mods.iter().map(|m| format!("{}@{}:{}", m.id, m.version, m.sha256.clone().unwrap_or_default())).collect();
    lines.sort();
    crate::net::sha256_hex(lines.join("\n").as_bytes())
}

/// "Windows 11 24H2 (build 26200)"
pub fn os_name() -> String {
    const KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let build = crate::winutil::reg_string(true, KEY, "CurrentBuildNumber");
    let display = crate::winutil::reg_string(true, KEY, "DisplayVersion");
    let Some(build) = build else { return "Windows".into() };
    let name = if build.parse::<u32>().is_ok_and(|b| b >= 22000) { "Windows 11" } else { "Windows 10" };
    match display {
        Some(d) => format!("{name} {d} (build {build})"),
        None => format!("{name} (build {build})"),
    }
}

pub struct Fields<'a> {
    /// `launcher` (the player pressed the button here) | `game` (the game's "Support" button asked)
    pub source: &'a str,
    pub kind: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub game_label: Option<&'a str>,
    /// modset sha256 (the first 12 characters go)
    pub modset: &'a str,
    pub exe_sha256: &'a str,
    pub os: &'a str,
    pub lang: &'a str,
}

/// The `report` part
pub fn report(f: &Fields) -> Value {
    json!({
        "source": if f.source == "game" { "game" } else { "launcher" },
        "kind": f.kind,
        "title": clean(f.title, TITLE_MAX, false),
        "description": clean(f.description, DESC_MAX, true),
        "versions": { "game": f.game_label.unwrap_or_default(), "mods": f.modset.get(..12).unwrap_or(f.modset) },
        "exe_sha256": f.exe_sha256,
        "os": f.os,
        "lang": f.lang,
    })
}

/// Report id from the server: goes into a URL path, so only `[A-Za-z0-9_-]`
pub fn valid_id(id: &str) -> bool {
    (1..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dbl-bugreport-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn tail_cuts_on_a_line() {
        let d = tmp("tail");
        let p = d.join("a.log");
        std::fs::write(&p, b"first line\nsecond line\nthird\n").unwrap();
        assert_eq!(tail(&p, 1000).unwrap(), b"first line\nsecond line\nthird\n");
        assert_eq!(tail(&p, 15).unwrap(), b"third\n", "cut inside 'second line' -> from the next line");
        std::fs::write(&p, b"").unwrap();
        assert!(tail(&p, 10).is_none());
        assert!(tail(&d.join("missing"), 10).is_none());
    }

    #[test]
    fn tail_is_text_for_the_server() {
        let d = tmp("text");
        // a crash left zeros at the end
        let p = d.join("crash.log");
        std::fs::write(&p, b"line one\nline two\n\0\0\0\0").unwrap();
        assert_eq!(tail(&p, 1000).unwrap(), b"line one\nline two\n");
        // UTF-16 LE with BOM -> UTF-8 without NULs, whole lines
        let p = d.join("ue16.log");
        let text = "первая строка\nsecond line\nlast ok\n";
        let mut bytes = vec![0xFF, 0xFE];
        bytes.extend(text.encode_utf16().flat_map(|u| u.to_le_bytes()));
        std::fs::write(&p, &bytes).unwrap();
        assert_eq!(String::from_utf8(tail(&p, 1000).unwrap()).unwrap(), text);
        let t = tail(&p, 20).unwrap();
        assert!(!t.contains(&0));
        assert_eq!(String::from_utf8(t).unwrap(), "last ok\n");
    }

    #[test]
    fn fit_trims_the_biggest() {
        let line = |n: usize| -> Vec<u8> { (0..n).flat_map(|i| format!("{i:09}\n").into_bytes()).collect() };
        let mut files = vec![("a".to_string(), line(100)), ("b".to_string(), line(10)), ("c".to_string(), vec![])];
        fit(&mut files, 500);
        let sum: usize = files.iter().map(|f| f.1.len()).sum();
        assert!(sum <= 500, "{sum}");
        assert_eq!(files.len(), 2, "empty file dropped");
        assert_eq!(files[1].1, line(10), "small file kept whole");
        assert!(files[0].1.starts_with(b"0000000"), "big one starts on a line");
        assert!(files[0].1.ends_with(b"000000099\n"), "keeps the end");
    }

    #[test]
    fn preview_matches_limits() {
        let d = tmp("preview");
        std::fs::write(d.join("x"), vec![b'a'; 3000]).unwrap();
        let s = vec![
            Source { name: "launcher.log", path: d.join("x"), limit: 1000 },
            Source { name: "relay.log", path: d.join("missing"), limit: 1000 },
        ];
        let p = preview(&s);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].size, 1000);
    }

    #[test]
    fn clean_text() {
        assert_eq!(clean("  a\u{7}b\r\nc\td  ", 100, true), "ab\nc d");
        assert_eq!(clean("a\nb", 100, false), "ab");
        assert_eq!(clean("ёжик", 2, false), "ёж", "chars, not bytes");
    }

    #[test]
    fn crash_hint() {
        let now = 1_000_000;
        let ok = LastPlay { exit_code: Some(0), exited_at: now - 60, ..Default::default() };
        let bad = LastPlay { exit_code: Some(0xC000_0005), exited_at: now - 60, ..Default::default() };
        let closed: &[u8] = b"...\n[2026.11.13-21.29.29:885][824]Log file closed, 11/14/26 03:29:29\n";
        let open: &[u8] = b"...\nLogTemp: Error: something\n";
        assert!(!crash_likely(false, Some(60), &ok, now, Some(closed)), "clean exit");
        assert!(crash_likely(false, Some(60), &bad, now, Some(closed)), "error code");
        assert!(crash_likely(false, Some(60), &ok, now, Some(open)), "log not closed");
        assert!(!crash_likely(true, Some(60), &bad, now, Some(open)), "game still running");
        assert!(!crash_likely(false, None, &bad, now, Some(open)), "no crash log");
        assert!(!crash_likely(false, Some(3 * 86400), &bad, now, Some(open)), "old crash log");
        let old = LastPlay { exited_at: now - 3 * 86400, ..bad };
        assert!(!crash_likely(false, Some(60), &old, now, None), "old exit, no game log");
    }

    #[test]
    fn report_json() {
        let title = "x".repeat(150);
        let r = report(&Fields {
            source: "launcher",
            kind: "game",
            title: &title,
            description: "line 1\nline 2",
            game_label: Some("Dungeonborne Win64 final"),
            modset: "0123456789abcdef",
            exe_sha256: "62d5",
            os: "Windows 11",
            lang: "ru",
        });
        assert_eq!(r["source"], "launcher");
        assert_eq!(r["title"].as_str().unwrap().chars().count(), TITLE_MAX);
        assert_eq!(r["description"], "line 1\nline 2");
        assert_eq!(r["versions"]["mods"], "0123456789ab");
        assert_eq!(r["versions"]["game"], "Dungeonborne Win64 final");
    }

    #[test]
    fn pulse_and_request() {
        let d = tmp("link");
        let now = 1_763_150_400;
        write_pulse(&d, now);
        let v: Value = serde_json::from_slice(&std::fs::read(d.join(PULSE_FILE)).unwrap()).unwrap();
        assert_eq!(v["ts"], now);
        assert_eq!(v["pid"], std::process::id());
        remove_pulse(&d);
        assert!(!d.join(PULSE_FILE).exists());

        let req = d.join(REQUEST_FILE);
        assert_eq!(take_request(&d, now), None, "nothing asked");
        std::fs::write(&req, format!(r#"{{"source":"game","scene":"Setting","role":"Вася\u0007","ts":{}}}"#, now - 3)).unwrap();
        assert_eq!(take_request(&d, now), Some(GameRequest { scene: "Setting".into(), role: "Вася".into() }));
        assert!(!req.exists(), "taken once");
        std::fs::write(&req, format!(r#"{{"source":"game","scene":"Login","ts":{}}}"#, now - 3600)).unwrap();
        assert_eq!(take_request(&d, now), None, "too old");
        assert!(!req.exists(), "an old one is removed too");
        std::fs::write(&req, b"{half").unwrap();
        assert_eq!(take_request(&d, now), None, "broken");
        std::fs::write(&req, vec![b' '; 5000]).unwrap();
        assert_eq!(take_request(&d, now), None, "too big");
        assert!(!req.exists());
    }

    #[test]
    fn ids() {
        assert!(valid_id("r_01JD4X"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }
}
