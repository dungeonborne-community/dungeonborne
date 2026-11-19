//! Taking a kit from a zip or a folder into the app's catalog. A kit is a foreign file: only data is taken from it
//! (`manifest.json`, `kit.json`, `catalog.json`, `data/`, `raw/`, `icons/`), `tools/`, `llm/` and the book are not extracted.

use std::fs;
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::kit::Kit;

/// An error with a permanent code: the interface picks the text in the user's language by it
#[derive(Clone, Debug, PartialEq)]
pub struct KitError {
    pub code: &'static str,
    pub message: String,
}

impl KitError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        KitError { code, message: message.into() }
    }
}

impl std::fmt::Display for KitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub total: u64,
    pub entries: usize,
    pub file: u64,
    pub icon: u64,
    /// How many times a file larger than 1 MB may be compressed
    pub ratio: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { total: 200 << 20, entries: 20_000, file: 64 << 20, icon: 4 << 20, ratio: 1000 }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    pub root: PathBuf,
    pub version: String,
    pub files: usize,
    /// Files the app does not need (`tools/`, `llm/`, the book)
    pub skipped: usize,
    /// Files in `icons/` whose content does not look like PNG, JPEG or WebP
    pub bad_icons: usize,
}

fn safe_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && name.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.'))
}

/// Whether the app needs the file; `rel` — the path inside the kit with '/'
fn wanted(rel: &str) -> bool {
    if matches!(rel, "manifest.json" | "kit.json" | "catalog.json") {
        return true;
    }
    let Some((dir, name)) = rel.split_once('/') else { return false };
    let (stem_ok, ext) = (safe_name(name), name.rsplit('.').next().unwrap_or("").to_ascii_lowercase());
    stem_ok
        && match dir {
            "data" => ext == "csv",
            "raw" => ext == "json",
            "icons" => matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "webp"),
            _ => false,
        }
}

/// Picture type by content
pub fn image_type(head: &[u8]) -> Option<&'static str> {
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if head.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// A zip entry name is dangerous: goes outside the folder, absolute, with a drive or a colon (NTFS streams)
fn unsafe_entry(name: &str) -> bool {
    let n = name.replace('\\', "/");
    n.starts_with('/') || n.contains(':') || n.contains('\0') || n.split('/').any(|s| s == "..")
}

struct Writer<'a> {
    dest: &'a Path,
    limits: Limits,
    total: u64,
    files: usize,
    skipped: usize,
    bad_icons: usize,
}

impl Writer<'_> {
    fn copy(&mut self, rel: &str, source: &mut dyn Read) -> Result<(), KitError> {
        let is_icon = rel.starts_with("icons/");
        let limit = if is_icon { self.limits.icon } else { self.limits.file };
        let too_large = || KitError::new("kit_too_large", format!("{}: file larger than {} bytes", rel, limit));
        let path = self.dest.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut out = fs::File::create(&path).map_err(io_error)?;
        let mut head = [0u8; 12];
        let mut head_len = 0;
        let mut written = 0u64;
        let mut buf = [0u8; 64 * 1024];
        loop {
            let n = source.read(&mut buf).map_err(|e| KitError::new("kit_damaged", format!("{}: {}", rel, e)))?;
            if n == 0 {
                break;
            }
            written += n as u64;
            self.total += n as u64;
            if written > limit {
                return Err(too_large());
            }
            if self.total > self.limits.total {
                return Err(KitError::new("kit_too_large", format!("kit larger than {} bytes", self.limits.total)));
            }
            if head_len < head.len() {
                let take = (head.len() - head_len).min(n);
                head[head_len..head_len + take].copy_from_slice(&buf[..take]);
                head_len += take;
            }
            out.write_all(&buf[..n]).map_err(io_error)?;
        }
        drop(out);
        if is_icon && image_type(&head[..head_len]).is_none() {
            fs::remove_file(&path).map_err(io_error)?;
            self.bad_icons += 1;
        } else {
            self.files += 1;
        }
        Ok(())
    }
}

fn io_error(e: io::Error) -> KitError {
    KitError::new("kit_io", e.to_string())
}

fn extract_zip<R: Read + Seek>(reader: R, w: &mut Writer) -> Result<(), KitError> {
    let mut zip = zip::ZipArchive::new(reader).map_err(|e| KitError::new("kit_damaged", e.to_string()))?;
    if zip.len() > w.limits.entries {
        return Err(KitError::new("kit_too_large", format!("{} files in the archive", zip.len())));
    }
    // All names are checked before extraction, including those that are not needed
    let mut roots = Vec::new();
    for name in zip.file_names() {
        if unsafe_entry(name) {
            return Err(KitError::new("kit_unsafe", format!("unsafe path in the archive: {}", name)));
        }
        let n = name.replace('\\', "/");
        if n == "manifest.json" {
            roots.push(String::new());
        } else if let Some(prefix) = n.strip_suffix("/manifest.json") {
            if !prefix.contains('/') {
                roots.push(format!("{}/", prefix));
            }
        }
    }
    let prefix = match roots.as_slice() {
        [one] => one.clone(),
        [] => return Err(KitError::new("kit_not_found", "no manifest.json in the archive")),
        _ => return Err(KitError::new("kit_not_found", "several kits in the archive")),
    };
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| KitError::new("kit_damaged", e.to_string()))?;
        let name = file.name().replace('\\', "/");
        // S_IFLNK: a link could lead an entry outside the kit folder
        if file.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err(KitError::new("kit_unsafe", format!("link in the archive: {}", name)));
        }
        if file.is_dir() {
            continue;
        }
        let Some(rel) = name.strip_prefix(prefix.as_str()).filter(|r| wanted(r)).map(str::to_string) else {
            w.skipped += 1;
            continue;
        };
        if file.size() > (1 << 20) && file.size() / file.compressed_size().max(1) > w.limits.ratio {
            return Err(KitError::new("kit_too_large", format!("{}: suspiciously highly compressed", rel)));
        }
        w.copy(&rel, &mut file)?;
    }
    Ok(())
}

fn copy_folder(src: &Path, w: &mut Writer) -> Result<(), KitError> {
    let mut seen = 0usize;
    let mut take = |w: &mut Writer, path: &Path, rel: &str| -> Result<(), KitError> {
        seen += 1;
        if seen > w.limits.entries {
            return Err(KitError::new("kit_too_large", format!("more than {} files in the folder", w.limits.entries)));
        }
        let meta = fs::symlink_metadata(path).map_err(io_error)?;
        if !meta.is_file() || !wanted(rel) {
            w.skipped += 1;
            return Ok(());
        }
        let mut f = fs::File::open(path).map_err(io_error)?;
        w.copy(rel, &mut f)
    };
    for name in ["manifest.json", "kit.json", "catalog.json"] {
        let p = src.join(name);
        if p.exists() {
            take(w, &p, name)?;
        }
    }
    for dir in ["data", "raw", "icons"] {
        let d = src.join(dir);
        if !fs::symlink_metadata(&d).map(|m| m.is_dir()).unwrap_or(false) {
            continue;
        }
        for e in fs::read_dir(&d).map_err(io_error)? {
            let e = e.map_err(io_error)?;
            let name = e.file_name().to_string_lossy().into_owned();
            take(w, &e.path(), &format!("{}/{}", dir, name))?;
        }
    }
    Ok(())
}

/// Kit version as a folder name: only what cannot leave the catalog
fn version_dir(version: &str) -> Result<&str, KitError> {
    let ok = !version.is_empty() && version.len() <= 64 && version.bytes().next().is_some_and(|c| c.is_ascii_alphanumeric()) && safe_name(version);
    ok.then_some(version).ok_or(KitError::new("kit_invalid", format!("invalid kit version: {:?}", version)))
}

/// Kit (zip or folder) -> `dest/<version>/`. The old copy of this version is replaced only when the new one passed the check.
pub fn import(src: &Path, dest: &Path, limits: Limits) -> Result<Imported, KitError> {
    let meta = fs::metadata(src).map_err(|e| KitError::new("kit_not_found", format!("{}: {}", src.display(), e)))?;
    fs::create_dir_all(dest).map_err(io_error)?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tmp = dest.join(format!(".incoming-{}-{}", std::process::id(), stamp));
    let result = (|| {
        fs::create_dir_all(&tmp).map_err(io_error)?;
        let mut w = Writer { dest: &tmp, limits, total: 0, files: 0, skipped: 0, bad_icons: 0 };
        if meta.is_dir() {
            if !src.join("manifest.json").is_file() {
                return Err(KitError::new("kit_not_found", "no manifest.json in the folder"));
            }
            copy_folder(src, &mut w)?;
        } else {
            let f = fs::File::open(src).map_err(io_error)?;
            extract_zip(io::BufReader::new(f), &mut w)?;
        }
        let kit = Kit::open(&tmp).map_err(|e| KitError::new("kit_invalid", e))?;
        let version = version_dir(kit.version())?.to_string();
        // Every table of kit.json must be readable, otherwise the error would surface only when the table is opened
        for name in kit.table_names() {
            kit.table_data(&name).map_err(|e| KitError::new("kit_invalid", e))?.ok_or(KitError::new("kit_invalid", name))?;
        }
        let target = dest.join(&version);
        if target.exists() {
            fs::remove_dir_all(&target).map_err(io_error)?;
        }
        fs::rename(&tmp, &target).map_err(io_error)?;
        Ok(Imported { root: target, version, files: w.files, skipped: w.skipped, bad_icons: w.bad_icons })
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wanted_files() {
        for ok in ["manifest.json", "kit.json", "catalog.json", "data/a.csv", "data/a.orig.csv", "raw/a.json", "icons/1.png", "icons/1.JPG", "icons/1.webp"] {
            assert!(wanted(ok), "{ok}");
        }
        for no in ["balance.xlsx", "tools/check.py", "llm/PROMPT.md", "data/a.exe", "data/x/a.csv", "icons/1.svg", "icons/.hidden.png", "raw/a.csv", "README.md", "data/a b.csv"] {
            assert!(!wanted(no), "{no}");
        }
    }

    #[test]
    fn unsafe_names() {
        for bad in ["../x", "a/../../x", "..\\x", "/abs", "\\abs", "C:\\x", "C:/x", "a:stream"] {
            assert!(unsafe_entry(bad), "{bad}");
        }
        for ok in ["a/b.csv", "kit-1.0/manifest.json", "a..b/c"] {
            assert!(!unsafe_entry(ok), "{ok}");
        }
    }

    #[test]
    fn image_types() {
        assert_eq!(image_type(b"\x89PNG\r\n\x1a\nxxxx"), Some("image/png"));
        assert_eq!(image_type(b"\xff\xd8\xff\xe0"), Some("image/jpeg"));
        assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
        assert_eq!(image_type(b"<svg xmlns="), None);
        assert_eq!(image_type(b""), None);
    }

    #[test]
    fn version_names() {
        assert!(version_dir("20261106-542ba6f5").is_ok());
        for bad in ["", "..", "../x", "a/b", ".x", "a b"] {
            assert!(version_dir(bad).is_err(), "{bad}");
        }
    }
}
