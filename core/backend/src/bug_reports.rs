//! Player bug reports from the launcher → Discord bot (contract: docs/specs/bug-report-api.md).
//!
//! The launcher sends `POST /api/launcher/v1/bug_report` (src/auth/web.rs) — multipart: JSON `report` and up to five log files.
//! The report card is the `bug_reports` table (id = `r_<ULID>`), files are on disk at `<BUG_REPORTS_DIR>/<id>/<name>`
//! (default `bug_reports`): up to 9 MB per report, in RocksDB without blob files such values would bloat the DB.
//! The bot on the same VM picks up new ones via the admin API (src/admin_api.rs, `/admin/v1/bug-reports*`), publishes a forum post
//! and marks the report `posted` with a link. Files of published reports are deleted after 30 days (Discord has them).
//!
//! The backend takes `discord_id`, `open_id`, `battle_id` and the launcher version itself (session, room, User-Agent), not from the body.

use crate::db::GameDb;
use crate::structures::PlayerModel;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const TABLE: &str = "bug_reports";
const MIB: usize = 1024 * 1024;
/// All report files together: the bot's attachment limit in Discord is 10 MB, the rest is for the post text
pub const MAX_TOTAL: usize = 9 * MIB;
/// The whole request body (files + JSON + multipart markup) — route limit
pub const MAX_BODY: usize = 10 * MIB;
/// At most this many reports per hour per player
pub const PER_HOUR: usize = 3;
const HOUR: i64 = 3600;
/// Files of published reports are kept this many seconds, then deleted (the card stays)
const KEEP_POSTED_FILES: i64 = 30 * 86400;

/// multipart part name → max size. `crash_ue4ss.log` has no limit in the contract — same as the other logs
pub const FILES: [(&str, usize); 5] = [
    ("launcher.log", 2 * MIB),
    ("relay.log", 2 * MIB),
    ("crash_ue4ss.log", 2 * MIB),
    ("Dungeonborne.log", 3 * MIB),
    ("screenshot.png", MIB),
];

pub fn file_limit(name: &str) -> Option<usize> {
    FILES.iter().find(|(n, _)| *n == name).map(|(_, max)| *max)
}

const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";
const SCHEMA: &str = r#"
DEFINE INDEX OVERWRITE idx_bug_reports_status ON TABLE bug_reports COLUMNS status, created_at;
DEFINE INDEX OVERWRITE idx_bug_reports_open_id ON TABLE bug_reports COLUMNS open_id;
"#;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct FileMeta {
    pub name: String,
    pub size: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct BugReport {
    /// `r_<ULID>`; matches the record id
    pub rid: String,
    pub open_id: String,
    /// `Account.subject` for the discord provider; empty for other providers (local dev)
    pub discord_id: String,
    pub created_at: i64,
    pub source: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub battle_id: Option<String>,
    pub game_version: Option<String>,
    pub launcher_version: Option<String>,
    pub mods_version: Option<String>,
    pub exe_sha256: Option<String>,
    pub os: Option<String>,
    pub lang: String,
    pub files: Vec<FileMeta>,
    /// `new` → `posted`
    pub status: String,
    #[serde(default)]
    pub thread_id: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub posted_at: i64,
}

/// Refusal: HTTP status, code for the launcher, text
#[derive(Debug, PartialEq)]
pub struct Reject {
    pub status: StatusCode,
    pub code: &'static str,
    pub message: String,
}

pub fn bad(code: &'static str, message: impl Into<String>) -> Reject {
    Reject { status: StatusCode::BAD_REQUEST, code, message: message.into() }
}

pub fn too_large(message: impl Into<String>) -> Reject {
    Reject { status: StatusCode::PAYLOAD_TOO_LARGE, code: "too_large", message: message.into() }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub fn dir() -> PathBuf {
    std::env::var("BUG_REPORTS_DIR").unwrap_or_else(|_| "bug_reports".into()).into()
}

// ---------------------------------------------------------------- id

/// ULID (Crockford base32, 26 chars): 48 bits of time in ms + 80 random bits — sorts by time
fn ulid(ms: u64, rnd: u128) -> String {
    const A: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let v: u128 = ((ms as u128 & 0xFFFF_FFFF_FFFF) << 80) | (rnd & ((1u128 << 80) - 1));
    (0..26).map(|i| A[((v >> (125 - i * 5)) & 31) as usize] as char).collect()
}

pub fn new_id() -> String {
    let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    format!("r_{}", ulid(ms, rand::random::<u128>()))
}

/// id from the request path: only our format — it is a folder name on disk
pub fn valid_id(id: &str) -> bool {
    id.len() == 28 && id.starts_with("r_") && id[2..].bytes().all(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
}

// ---------------------------------------------------------------- report fields

/// Fields from the JSON `report`, already cleaned and trimmed
#[derive(Debug, Clone, PartialEq)]
pub struct Fields {
    pub source: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub game_version: Option<String>,
    pub mods_version: Option<String>,
    pub exe_sha256: Option<String>,
    pub os: Option<String>,
    pub lang: String,
}

/// Single line: control characters → space, surrounding spaces trimmed, at most `max` characters
pub fn clean_line(s: &str, max: usize) -> String {
    let s: String = s.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
    s.trim().chars().take(max).collect::<String>().trim_end().to_string()
}

/// Text with line breaks: `\r` and other control characters (except `\n` and `\t`) removed
pub fn clean_text(s: &str, max: usize) -> String {
    let s: String = s.replace("\r\n", "\n").chars().filter(|c| !c.is_control() || *c == '\n' || *c == '\t').collect();
    s.trim().chars().take(max).collect::<String>().trim_end().to_string()
}

fn opt_line(v: &Value, max: usize) -> Option<String> {
    v.as_str().map(|s| clean_line(s, max)).filter(|s| !s.is_empty())
}

pub fn parse_report(v: &Value) -> Result<Fields, Reject> {
    if !v.is_object() {
        return Err(bad("bad_report", "the report part must be a JSON object"));
    }
    let source = match v["source"].as_str().unwrap_or("launcher") {
        s @ ("launcher" | "game") => s.to_string(),
        _ => return Err(bad("bad_report", "source: launcher | game")),
    };
    let kind = match v["kind"].as_str() {
        Some(k @ ("crash" | "game" | "launcher")) => k.to_string(),
        _ => return Err(bad("bad_report", "kind: crash | game | launcher")),
    };
    let title = clean_line(v["title"].as_str().unwrap_or_default(), 100);
    if title.is_empty() {
        return Err(bad("bad_report", "a title is required (title)"));
    }
    let exe_sha256 = v["exe_sha256"]
        .as_str()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()));
    let lang = v["lang"].as_str().map(|s| s.trim().to_ascii_lowercase()).unwrap_or_default();
    let lang_ok = (2..=10).contains(&lang.len()) && lang.bytes().all(|b| b.is_ascii_lowercase() || b == b'-');
    Ok(Fields {
        source,
        kind,
        title,
        description: clean_text(v["description"].as_str().unwrap_or_default(), 1500),
        game_version: opt_line(&v["versions"]["game"], 64),
        mods_version: opt_line(&v["versions"]["mods"], 64),
        exe_sha256,
        os: opt_line(&v["os"], 64),
        lang: if lang_ok { lang } else { "en".into() },
    })
}

/// Launcher version from User-Agent: the `dbl/<version>` token
pub fn launcher_version(user_agent: &str) -> Option<String> {
    let v = user_agent.split_whitespace().find_map(|t| t.strip_prefix("dbl/"))?;
    let v: String = v.chars().filter(|c| c.is_ascii_graphic()).take(20).collect();
    (!v.is_empty()).then_some(v)
}

// ---------------------------------------------------------------- files

/// A log is text: no NUL bytes (UTF-16 with BOM is text too)
fn looks_like_text(b: &[u8]) -> bool {
    b.starts_with(&[0xFF, 0xFE]) || b.starts_with(&[0xFE, 0xFF]) || !b.contains(&0)
}

/// One file part: known name, size, type. The size is also checked while reading (web.rs), here — finally
pub fn check_file(name: &str, data: &[u8]) -> Result<(), Reject> {
    let Some(max) = file_limit(name) else {
        let names: Vec<&str> = FILES.iter().map(|(n, _)| *n).collect();
        return Err(bad("bad_file", format!("unknown part {name}; allowed: report, {}", names.join(", "))));
    };
    if data.len() > max {
        return Err(too_large(format!("{name}: more than {} MB", max / MIB)));
    }
    let ok = if name.ends_with(".png") { data.starts_with(PNG_MAGIC) } else { looks_like_text(data) };
    if !ok {
        let want = if name.ends_with(".png") { "PNG" } else { "text log" };
        return Err(bad("bad_file", format!("{name}: not a {want}")));
    }
    Ok(())
}

/// All report files: each per `check_file`, no duplicates, together at most MAX_TOTAL
pub fn check_files(files: &[(String, Vec<u8>)]) -> Result<(), Reject> {
    let mut total = 0;
    for (i, (name, data)) in files.iter().enumerate() {
        check_file(name, data)?;
        if files[..i].iter().any(|(n, _)| n == name) {
            return Err(bad("bad_file", format!("{name} twice")));
        }
        total += data.len();
    }
    if total > MAX_TOTAL {
        return Err(too_large(format!("files together exceed {} MB", MAX_TOTAL / MIB)));
    }
    Ok(())
}

// ---------------------------------------------------------------- database

/// Table indexes (re-running is safe — OVERWRITE)
pub async fn setup(db: &GameDb) {
    if let Err(e) = db.db.query(SCHEMA).await.and_then(|r| r.check()) {
        eprintln!("[BugReport] schema: {}", e);
    }
}

/// PER_HOUR limit per hour: Err — in how many seconds a slot frees up. Counted from the DB — a restart does not reset it
pub async fn rate_check(db: &GameDb, open_id: &str, now: i64) -> Result<(), i64> {
    let times: Vec<i64> = match db
        .db
        .query("SELECT VALUE created_at FROM bug_reports WHERE open_id = $o AND created_at > $t")
        .bind(("o", open_id.to_string()))
        .bind(("t", now - HOUR))
        .await
        .and_then(|mut r| r.take(0))
    {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[BugReport] limit {}: {}", open_id, e);
            Vec::new()
        }
    };
    retry_after(&times, now)
}

fn retry_after(times: &[i64], now: i64) -> Result<(), i64> {
    let mut recent: Vec<i64> = times.iter().copied().filter(|t| *t > now - HOUR).collect();
    if recent.len() < PER_HOUR {
        return Ok(());
    }
    recent.sort_unstable();
    // a slot frees up when enough old ones leave the window that PER_HOUR - 1 remain
    let free_at = recent[recent.len() - PER_HOUR] + HOUR;
    Err((free_at - now).max(1))
}

/// The player's match number: the current room, otherwise the last match from the gear records (`raid_gear`, raid_native.rs)
pub async fn battle_of(db: &GameDb, open_id: &str) -> Option<String> {
    if let Some(b) = crate::lobby::matchmaking::room_of_player(open_id) {
        return Some(b);
    }
    let p = db.get::<PlayerModel>("players", open_id).await?;
    last_raid_battle(&p.state)
}

fn last_raid_battle(state: &Value) -> Option<String> {
    let b = &state["raid_gear"].as_array()?.last()?["battle"];
    b.as_str().map(str::to_string).or_else(|| b.as_i64().map(|n| n.to_string())).filter(|s| !s.is_empty())
}

/// Files to disk, then the card into the DB (the bot will not see a report without files). On a DB error the folder is deleted
pub async fn store(db: &GameDb, root: &Path, report: &BugReport, files: &[(String, Vec<u8>)]) -> Result<(), String> {
    let dir = root.join(&report.rid);
    if !files.is_empty() {
        tokio::fs::create_dir_all(&dir).await.map_err(|e| format!("folder {}: {}", dir.display(), e))?;
        for (name, data) in files {
            if let Err(e) = tokio::fs::write(dir.join(name), data).await {
                let _ = tokio::fs::remove_dir_all(&dir).await;
                return Err(format!("{}: {}", name, e));
            }
        }
    }
    let created = db.db.create::<Option<BugReport>>((TABLE, report.rid.as_str())).content(report.clone()).await;
    if let Err(e) = created {
        if !files.is_empty() {
            let _ = tokio::fs::remove_dir_all(&dir).await;
        }
        return Err(format!("database: {}", e));
    }
    Ok(())
}

pub async fn get(db: &GameDb, id: &str) -> Option<BugReport> {
    if !valid_id(id) {
        return None;
    }
    db.get::<BugReport>(TABLE, id).await
}

/// Reports with a status, oldest first
pub async fn list(db: &GameDb, status: &str, limit: usize) -> Result<Vec<BugReport>, String> {
    db.db
        .query("SELECT * FROM bug_reports WHERE status = $s ORDER BY created_at ASC, rid ASC LIMIT $n")
        .bind(("s", status.to_string()))
        .bind(("n", limit as i64))
        .await
        .and_then(|mut r| r.take(0))
        .map_err(|e| e.to_string())
}

/// The bot published a post. None — no such report; a repeated confirmation changes nothing (the first link stays)
pub async fn mark_posted(db: &GameDb, id: &str, thread_id: &str, url: &str, now: i64) -> Result<Option<()>, String> {
    let Some(r) = get(db, id).await else { return Ok(None) };
    if r.status == "posted" {
        return Ok(Some(()));
    }
    db.db
        .query("UPDATE type::thing('bug_reports', $id) SET status = 'posted', thread_id = $t, url = $u, posted_at = $now")
        .bind(("id", id.to_string()))
        .bind(("t", clean_line(thread_id, 32)))
        .bind(("u", clean_line(url, 200)))
        .bind(("now", now))
        .await
        .and_then(|r| r.check())
        .map(|_| Some(()))
        .map_err(|e| e.to_string())
}

/// Path to a report file — only from this report's file list
pub fn file_path(root: &Path, r: &BugReport, name: &str) -> Option<PathBuf> {
    r.files.iter().any(|f| f.name == name).then(|| root.join(&r.rid).join(name))
}

// ---------------------------------------------------------------- responses

/// Report for the bot — exactly section 2 of the contract (+ `exe_sha256`, the bot reads it too)
pub fn admin_json(r: &BugReport) -> Value {
    json!({
        "id": r.rid, "discord_id": r.discord_id, "created_at": r.created_at,
        "source": r.source, "kind": r.kind, "title": r.title, "description": r.description,
        "battle_id": r.battle_id,
        "versions": { "game": r.game_version, "launcher": r.launcher_version, "mods": r.mods_version },
        "exe_sha256": r.exe_sha256, "os": r.os, "lang": r.lang, "files": r.files,
    })
}

/// Status for the launcher
pub fn status_json(r: &BugReport) -> Value {
    json!({ "status": r.status, "url": (!r.url.is_empty()).then_some(&r.url) })
}

// ---------------------------------------------------------------- cleanup

/// Once an hour: files of published reports older than 30 days and folders without a card (write failure) older than a day
pub fn spawn_cleanup(db: GameDb) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
            cleanup(&db, &dir(), now()).await;
        }
    });
}

async fn cleanup(db: &GameDb, root: &Path, now: i64) {
    let Ok(mut rd) = tokio::fs::read_dir(root).await else { return };
    while let Ok(Some(e)) = rd.next_entry().await {
        let name = e.file_name().to_string_lossy().to_string();
        if !valid_id(&name) {
            continue;
        }
        let drop = match get(db, &name).await {
            Some(r) => r.status == "posted" && now - r.posted_at > KEEP_POSTED_FILES,
            None => {
                let age = e.metadata().await.ok().and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok());
                age.is_some_and(|a| a.as_secs() > 86400)
            }
        };
        if drop {
            match tokio::fs::remove_dir_all(e.path()).await {
                Ok(()) => println!("[BugReport] cleanup: files of {} deleted", name),
                Err(err) => eprintln!("[BugReport] cleanup {}: {}", name, err),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Value {
        json!({
            "source": "launcher", "kind": "game", "title": "Inventory is not saved",
            "description": "Picked up an item.\r\nStash is empty.", "versions": { "game": "1.0.3", "mods": "abcdef012345" },
            "exe_sha256": "AB".repeat(32), "os": "Windows 11 23H2", "lang": "ru",
        })
    }

    #[test]
    fn ulid_format_and_order() {
        let a = ulid(1_763_150_400_000, u128::MAX);
        let b = ulid(1_763_150_400_001, 0);
        assert_eq!(a.len(), 26);
        assert!(a < b, "{a} {b}");
        assert_eq!(ulid(0, 0), "0".repeat(26));
        let id = new_id();
        assert!(valid_id(&id), "{id}");
        assert!(!valid_id("r_../../etc/passwd_xxxxxxxxxx"));
        assert!(!valid_id("r_01jd"));
    }

    #[test]
    fn report_fields() {
        let f = parse_report(&fields()).unwrap();
        assert_eq!(f.description, "Picked up an item.\nStash is empty.");
        assert_eq!(f.exe_sha256.as_deref(), Some("ab".repeat(32).as_str()));
        assert_eq!((f.lang.as_str(), f.game_version.as_deref(), f.mods_version.as_deref()), ("ru", Some("1.0.3"), Some("abcdef012345")));

        let mut v = fields();
        v["title"] = json!(format!("  {}\n{}", "я".repeat(120), "x"));
        v["description"] = json!("d".repeat(5000));
        v["exe_sha256"] = json!("not a hash");
        v["lang"] = json!("<script>");
        v.as_object_mut().unwrap().remove("source");
        let f = parse_report(&v).unwrap();
        assert_eq!(f.title.chars().count(), 100);
        assert!(!f.title.contains('\n'));
        assert_eq!(f.description.len(), 1500);
        assert_eq!((f.exe_sha256, f.lang.as_str(), f.source.as_str()), (None, "en", "launcher"));

        for (k, bad_v) in [("kind", json!("drop table")), ("kind", Value::Null), ("source", json!("web")), ("title", json!("  \n "))] {
            let mut v = fields();
            v[k] = bad_v;
            assert_eq!(parse_report(&v).unwrap_err().code, "bad_report", "{k}");
        }
        assert!(parse_report(&json!([1])).is_err());
    }

    #[test]
    fn user_agent_version() {
        assert_eq!(launcher_version("dbl/1.7.4 (windows)").as_deref(), Some("1.7.4"));
        assert_eq!(launcher_version("Mozilla/5.0"), None);
        assert_eq!(launcher_version("dbl/"), None);
    }

    #[test]
    fn files_checked() {
        let png = [PNG_MAGIC, &[0u8; 10][..]].concat();
        let ok = vec![("relay.log".to_string(), b"line\n".to_vec()), ("screenshot.png".to_string(), png.clone())];
        assert!(check_files(&ok).is_ok());
        assert!(check_file("Dungeonborne.log", &[0xFF, 0xFE, b'a', 0]).is_ok(), "UTF-16 with BOM is text");

        assert_eq!(check_file("evil.exe", b"MZ").unwrap_err().code, "bad_file");
        assert_eq!(check_file("../relay.log", b"x").unwrap_err().code, "bad_file");
        assert_eq!(check_file("screenshot.png", b"GIF89a").unwrap_err().code, "bad_file");
        assert_eq!(check_file("launcher.log", b"MZ\0\0").unwrap_err().code, "bad_file");
        let big = check_file("screenshot.png", &[PNG_MAGIC, &vec![0u8; MIB][..]].concat()).unwrap_err();
        assert_eq!(big.status, StatusCode::PAYLOAD_TOO_LARGE);

        let twice = vec![("relay.log".to_string(), b"a".to_vec()), ("relay.log".to_string(), b"b".to_vec())];
        assert_eq!(check_files(&twice).unwrap_err().code, "bad_file");
        // each within its limit, together 2+2+2+3+1 = 10 MB > 9
        let all: Vec<(String, Vec<u8>)> = FILES
            .iter()
            .map(|(n, max)| (n.to_string(), if n.ends_with(".png") { [PNG_MAGIC, &vec![0u8; max - 8][..]].concat() } else { vec![b'a'; *max] }))
            .collect();
        assert_eq!(check_files(&all).unwrap_err().code, "too_large");
    }

    #[test]
    fn hourly_limit() {
        let now = 10_000;
        assert_eq!(retry_after(&[now - 10, now - 20], now), Ok(()));
        // three in an hour: a slot frees up when the oldest leaves the window
        assert_eq!(retry_after(&[now - 100, now - 1000, now - 50], now), Err(HOUR - 1000));
        assert_eq!(retry_after(&[now - 100, now - 1000, now - 50, now - 4000], now), Err(HOUR - 1000));
        assert_eq!(retry_after(&[now - 10, now - 20, now - 30, now - 40], now), Err(HOUR - 30));
    }

    #[test]
    fn last_battle_from_raid_gear() {
        assert_eq!(last_raid_battle(&json!({})), None);
        let s = json!({ "raid_gear": [{ "battle": "48210" }, { "battle": "48213", "team": [] }] });
        assert_eq!(last_raid_battle(&s).as_deref(), Some("48213"));
        assert_eq!(last_raid_battle(&json!({ "raid_gear": [{ "battle": 7 }] })).as_deref(), Some("7"));
    }

    fn report(id: &str, open_id: &str, at: i64, files: Vec<FileMeta>) -> BugReport {
        let f = parse_report(&fields()).unwrap();
        BugReport {
            rid: id.into(), open_id: open_id.into(), discord_id: "111111111111111111".into(), created_at: at,
            source: f.source, kind: f.kind, title: f.title, description: f.description, battle_id: Some("48213".into()),
            game_version: f.game_version, launcher_version: Some("1.7.4".into()), mods_version: f.mods_version,
            exe_sha256: f.exe_sha256, os: f.os, lang: f.lang, files, status: "new".into(),
            thread_id: String::new(), url: String::new(), posted_at: 0,
        }
    }

    /// A real DB in a temp folder: write, limit, list for the bot, posted, file, cleanup
    #[tokio::test]
    async fn store_list_post_cleanup() {
        let tag = crate::auth::sessions::random_hex(4);
        let db = GameDb::init(&format!("target/test_bug_db_{tag}")).await;
        setup(&db).await;
        let root = PathBuf::from(format!("target/test_bug_files_{tag}"));
        let now = 1_763_150_400;

        let (a, b) = (format!("r_{}", ulid(1, 1)), format!("r_{}", ulid(2, 2)));
        let files = vec![("relay.log".to_string(), b"hello".to_vec())];
        let meta = vec![FileMeta { name: "relay.log".into(), size: 5 }];
        // b was created before a — it is first in the list
        store(&db, &root, &report(&a, "o1", now - 10, meta.clone()), &files).await.unwrap();
        store(&db, &root, &report(&b, "o1", now - 20, vec![]), &[]).await.unwrap();
        assert!(store(&db, &root, &report(&a, "o1", now, vec![]), &[]).await.is_err(), "same id a second time");

        assert_eq!(rate_check(&db, "o1", now).await, Ok(()));
        assert_eq!(rate_check(&db, "o2", now).await, Ok(()));
        store(&db, &root, &report(&format!("r_{}", ulid(3, 3)), "o1", now - 5, vec![]), &[]).await.unwrap();
        assert_eq!(rate_check(&db, "o1", now).await, Err(HOUR - 20));

        let new = list(&db, "new", 10).await.unwrap();
        assert_eq!(new.iter().map(|r| r.rid.clone()).collect::<Vec<_>>()[..2], [b.clone(), a.clone()]);
        assert_eq!(list(&db, "new", 1).await.unwrap().len(), 1);

        let j = admin_json(&new[1]);
        assert_eq!(j["id"], json!(a));
        assert_eq!(j["exe_sha256"], json!("ab".repeat(32)));
        assert_eq!(j["versions"], json!({ "game": "1.0.3", "launcher": "1.7.4", "mods": "abcdef012345" }));
        assert_eq!(j["files"], json!([{ "name": "relay.log", "size": 5 }]));
        assert_eq!((j["battle_id"].as_str(), j["discord_id"].as_str()), (Some("48213"), Some("111111111111111111")));

        let r = get(&db, &a).await.unwrap();
        let p = file_path(&root, &r, "relay.log").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hello");
        assert!(file_path(&root, &r, "launcher.log").is_none());
        assert_eq!(status_json(&r), json!({ "status": "new", "url": null }));

        let url = "https://discord.com/channels/1/2";
        assert_eq!(mark_posted(&db, &a, "2", url, now).await, Ok(Some(())));
        assert_eq!(mark_posted(&db, &a, "9", "https://other", now + 1).await, Ok(Some(())), "repeat — 204");
        assert_eq!(mark_posted(&db, "r_00000000000000000000000000", "1", url, now).await, Ok(None));
        let r = get(&db, &a).await.unwrap();
        assert_eq!((r.status.as_str(), r.thread_id.as_str(), r.url.as_str(), r.posted_at), ("posted", "2", url, now));
        assert_eq!(status_json(&r), json!({ "status": "posted", "url": url }));
        assert!(!list(&db, "new", 10).await.unwrap().iter().any(|r| r.rid == a));
        assert_eq!(list(&db, "posted", 10).await.unwrap().len(), 1);

        cleanup(&db, &root, now + KEEP_POSTED_FILES - 10).await;
        assert!(p.exists(), "too early");
        cleanup(&db, &root, now + KEEP_POSTED_FILES + 10).await;
        assert!(!p.exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
