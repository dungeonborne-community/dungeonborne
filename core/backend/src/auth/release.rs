//! Launcher self-update: `[launcher] dir` with `launcher.json` and the exe (made by `launcher/tools/release.py sign`).
//! The server only serves the build: it checks that the exe matches `launcher.json` by size and sha256 and puts the
//! `launcher` field into the manifest. The launcher itself checks the signature with its built-in release key; without it the update is not installed,
//! so the build cannot be substituted through the server. `launcher.json` is re-read on change, without a restart.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

#[derive(Deserialize, Clone, Debug)]
pub struct Release {
    pub version: String,
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

pub struct Releases {
    dir: PathBuf,
    cache: Mutex<(Option<SystemTime>, Option<Arc<Release>>)>,
}

/// Exe name: only `[A-Za-z0-9._-]`, `.exe`, no `..`
fn safe_file(name: &str) -> bool {
    name.ends_with(".exe") && !name.contains("..") && name.bytes().all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}

fn load(dir: &std::path::Path) -> Option<Release> {
    let meta = std::fs::read(dir.join("launcher.json")).ok()?;
    let r: Release = match serde_json::from_slice(&meta) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[Launcher] launcher.json: {}", e);
            return None;
        }
    };
    if !safe_file(&r.file) {
        eprintln!("[Launcher] invalid file name '{}'", r.file);
        return None;
    }
    match std::fs::read(dir.join(&r.file)) {
        Ok(b) if b.len() as u64 == r.size && format!("{:x}", Sha256::digest(&b)) == r.sha256.to_ascii_lowercase() => {
            println!("[Launcher] serving launcher build {} ({})", r.version, r.file);
            Some(r)
        }
        Ok(_) => {
            eprintln!("[Launcher] {}: does not match launcher.json by size or sha256 — not served", r.file);
            None
        }
        Err(e) => {
            eprintln!("[Launcher] {}: {}", r.file, e);
            None
        }
    }
}

impl Releases {
    pub fn new(dir: &str) -> Self {
        let r = Self { dir: PathBuf::from(dir), cache: Mutex::new((None, None)) };
        r.current();
        r
    }

    /// Current build (re-read when `launcher.json` changes)
    pub fn current(&self) -> Option<Arc<Release>> {
        if self.dir.as_os_str().is_empty() {
            return None;
        }
        let mtime = std::fs::metadata(self.dir.join("launcher.json")).and_then(|m| m.modified()).ok();
        let mut c = self.cache.lock().unwrap();
        if c.0 != mtime || (mtime.is_none() && c.1.is_some()) {
            *c = (mtime, load(&self.dir).map(Arc::new));
        }
        c.1.clone()
    }

    /// Path to the exe, if it is the current build
    pub fn file(&self, name: &str) -> Option<PathBuf> {
        let r = self.current()?;
        (safe_file(name) && r.file == name).then(|| self.dir.join(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_only_matching_build() {
        let dir = std::env::temp_dir().join(format!("dbs_release_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("dungeonborne-launcher-1.2.0.exe"), b"exe").unwrap();
        let meta = |sha: &str| {
            format!(r#"{{"version":"1.2.0","file":"dungeonborne-launcher-1.2.0.exe","size":3,"sha256":"{}","signature":"00"}}"#, sha)
        };
        std::fs::write(dir.join("launcher.json"), meta(&format!("{:x}", Sha256::digest(b"exe")))).unwrap();
        let r = Releases::new(dir.to_str().unwrap());
        assert_eq!(r.current().unwrap().version, "1.2.0");
        assert!(r.file("dungeonborne-launcher-1.2.0.exe").is_some());
        assert!(r.file("../dungeonborne-launcher-1.2.0.exe").is_none() && r.file("other.exe").is_none());

        std::fs::write(dir.join("launcher.json"), meta(&"00".repeat(32))).unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(dir.join("launcher.json")).unwrap().set_modified(later).unwrap();
        assert!(r.current().is_none(), "exe differs from launcher.json — not served");
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(Releases::new("").current().is_none());
    }
}
