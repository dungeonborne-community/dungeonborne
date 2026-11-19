//! Client mods for the launcher (`dbmod/1`, stage E2, docs/LAUNCHER_SPEC.md §10): folder `[mods] dir` with zip packages and
//! `manifest_mods.json`, built by `launcher/tools/make_mods.py`. When the list is loaded, each package's size and sha256
//! are checked; a package with an error does not get into the manifest. The list is re-read when `manifest_mods.json` changes,
//! so mods are updated by copying files, without a server restart. If some packages failed the check
//! (zip not fully copied yet), the list is re-read every 10 s until it is complete.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

const RETRY: Duration = Duration::from_secs(10);

/// A `manifest_mods.json` row (= an element of the manifest's `mods[]`, `url` = `/mods/<file>`)
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ModEntry {
    pub id: String,
    pub version: String,
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub required: bool,
    pub url: String,
}

#[derive(Clone, Debug)]
pub struct Package {
    pub entry: ModEntry,
    /// zip name in the mods folder
    pub file: String,
}

struct Loaded {
    mtime: Option<SystemTime>,
    list: Arc<Vec<Package>>,
    /// all rows of `manifest_mods.json` passed the check
    complete: bool,
    at: Instant,
}

pub struct Mods {
    dir: PathBuf,
    cache: Mutex<Loaded>,
}

/// Package file name: only `[A-Za-z0-9._-]`, `.zip`, no `..`, so a path from a request does not leave the folder
fn safe_file(name: &str) -> bool {
    name.ends_with(".zip")
        && !name.contains("..")
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn sha256_hex(b: &[u8]) -> String {
    Sha256::digest(b).iter().map(|x| format!("{:02x}", x)).collect()
}

/// (packages, whether all rows passed the check)
fn load(dir: &Path) -> (Vec<Package>, bool) {
    let index = dir.join("manifest_mods.json");
    let entries: Vec<ModEntry> = match std::fs::read(&index) {
        Ok(b) => match serde_json::from_slice(&b) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("[Mods] {}: {}", index.display(), e);
                return (Vec::new(), false);
            }
        },
        Err(_) => return (Vec::new(), true),
    };
    let total = entries.len();
    let mut out = Vec::new();
    for e in entries {
        let file = e.url.rsplit('/').next().unwrap_or_default().to_string();
        if !safe_file(&file) {
            eprintln!("[Mods] {}: invalid file name '{}'", e.id, file);
            continue;
        }
        match std::fs::read(dir.join(&file)) {
            Ok(b) if b.len() as u64 == e.size && sha256_hex(&b) == e.sha256.to_ascii_lowercase() => {
                out.push(Package { entry: e, file })
            }
            Ok(_) => eprintln!("[Mods] {}: {} does not match by size or sha256 — not served", e.id, file),
            Err(err) => eprintln!("[Mods] {}: {}: {}", e.id, file, err),
        }
    }
    let complete = out.len() == total;
    (out, complete)
}

impl Mods {
    pub fn new(dir: &str) -> Self {
        let empty = Loaded { mtime: None, list: Arc::new(Vec::new()), complete: false, at: Instant::now() };
        let m = Self { dir: PathBuf::from(dir), cache: Mutex::new(empty) };
        let n = m.list().len();
        if !dir.is_empty() {
            println!("[Mods] {} mod packages from {}", n, m.dir.display());
        }
        m
    }

    /// Current list (re-read when `manifest_mods.json` changes)
    pub fn list(&self) -> Arc<Vec<Package>> {
        if self.dir.as_os_str().is_empty() {
            return Arc::new(Vec::new());
        }
        let mtime = std::fs::metadata(self.dir.join("manifest_mods.json")).and_then(|m| m.modified()).ok();
        let mut c = self.cache.lock().unwrap();
        let changed = c.mtime != mtime || (mtime.is_none() && !c.list.is_empty());
        if changed || (!c.complete && c.at.elapsed() >= RETRY) {
            let (list, complete) = load(&self.dir);
            if mtime.is_some() && (changed || complete) {
                println!("[Mods] mod list: {} packages{}", list.len(), if complete { "" } else { " (not all passed the check)" });
            }
            *c = Loaded { mtime, list: Arc::new(list), complete, at: Instant::now() };
        }
        c.list.clone()
    }

    /// Path to the zip, if the package is in the list
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        if !safe_file(name) {
            return None;
        }
        self.list().iter().any(|p| p.file == name).then(|| self.dir.join(name))
    }

    /// `modset_sha256` of required mods: lines `id@version:sha256`, by bytes, LF-separated, no trailing LF (§10)
    pub fn required_modset(&self) -> String {
        let mut lines: Vec<String> = self
            .list()
            .iter()
            .filter(|p| p.entry.required)
            .map(|p| format!("{}@{}:{}", p.entry.id, p.entry.version, p.entry.sha256.to_ascii_lowercase()))
            .collect();
        if lines.is_empty() {
            return String::new();
        }
        lines.sort();
        sha256_hex(lines.join("\n").as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_index(dir: &Path, entries: &[ModEntry]) {
        std::fs::write(dir.join("manifest_mods.json"), serde_json::to_vec(entries).unwrap()).unwrap();
    }

    fn entry(id: &str, file: &str, data: &[u8], required: bool) -> ModEntry {
        ModEntry {
            id: id.into(),
            version: "1.0.0".into(),
            sha256: sha256_hex(data),
            size: data.len() as u64,
            required,
            url: format!("/mods/{}", file),
        }
    }

    #[test]
    fn loads_checks_and_reloads() {
        let dir = std::env::temp_dir().join(format!("dbs_mods_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a-1.0.0.zip"), b"good").unwrap();
        std::fs::write(dir.join("b-1.0.0.zip"), b"changed").unwrap();
        let mut bad_hash = entry("b", "b-1.0.0.zip", b"original", true);
        bad_hash.size = 7;
        write_index(&dir, &[entry("a", "a-1.0.0.zip", b"good", true), bad_hash, entry("c", "..evil.zip", b"x", false)]);

        let mods = Mods::new(dir.to_str().unwrap());
        let ids: Vec<_> = mods.list().iter().map(|p| p.entry.id.clone()).collect();
        assert_eq!(ids, ["a"], "bad hash and unsafe name dropped");
        assert!(mods.file("a-1.0.0.zip").is_some());
        assert!(mods.file("b-1.0.0.zip").is_none() && mods.file("../a-1.0.0.zip").is_none());
        assert_eq!(mods.required_modset(), sha256_hex(format!("a@1.0.0:{}", sha256_hex(b"good")).as_bytes()));

        // a new list is picked up without a restart
        std::thread::sleep(std::time::Duration::from_millis(20));
        write_index(&dir, &[entry("a", "a-1.0.0.zip", b"good", true), entry("b", "b-1.0.0.zip", b"changed", false)]);
        let filetime = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(dir.join("manifest_mods.json")).unwrap().set_modified(filetime).unwrap();
        assert_eq!(mods.list().len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn no_dir_no_mods() {
        assert!(Mods::new("").list().is_empty());
        assert!(Mods::new("Z:/nonexistent_dbs_mods").list().is_empty());
    }
}
