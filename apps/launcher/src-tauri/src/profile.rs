//! Server profile (spec M5/M6): `dbmod/1` packages, pak/UE4SS layout, `cacert.pem`, Goldberg identity.
//! Every change to the game folder is written to a journal *before* it happens, so a crash of the launcher
//! is undone on its next start. Files are touched only while the game is closed.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{bail, Error, Result};
use crate::manifest::{valid_id, ModRef};
use crate::net::{self, check_url, sha256_hex};
use crate::store::data_dir;

const MOD_LIMIT: u64 = 512 << 20;

// ---------------------------------------------------------------------------------------------- install

#[derive(Debug, Clone)]
pub struct Install {
    pub root: PathBuf,
}

impl Install {
    pub fn open(root: &str) -> Result<Self> {
        let root = PathBuf::from(root.trim().trim_end_matches(['\\', '/']));
        if root.as_os_str().is_empty() {
            bail!("install_not_set");
        }
        let inst = Self { root };
        if !inst.exe().is_file() {
            bail!("install_invalid", "{}", inst.exe().display());
        }
        Ok(inst)
    }

    pub fn win64(&self) -> PathBuf {
        self.root.join("Dungeonborne").join("Binaries").join("Win64")
    }
    pub fn exe(&self) -> PathBuf {
        self.win64().join("Dungeonborne.exe")
    }
    pub fn content(&self) -> PathBuf {
        self.root.join("Dungeonborne").join("Content")
    }
    pub fn paks(&self) -> PathBuf {
        self.content().join("Paks")
    }
    pub fn ue4ss_mods(&self) -> PathBuf {
        self.win64().join("ue4ss").join("Mods")
    }
    pub fn cacert(&self) -> PathBuf {
        self.win64().join("cacert.pem")
    }
    /// The INTL SDK's config: open text. The SDK reads it only when no encrypted `INTLConfig.ini.new` lies next to it
    /// (INTLCore.dll INIConfig::GetFilePathByProject, LOCAL 11.11)
    pub fn intl_config(&self) -> PathBuf {
        self.win64().join("INTLConfig.ini")
    }
    pub fn intl_config_encrypted(&self) -> PathBuf {
        self.win64().join("INTLConfig.ini.new")
    }
    pub fn steam_settings(&self) -> PathBuf {
        self.win64().join("steam_settings")
    }
    /// Stable short key for journal/backups of this install.
    pub fn key(&self) -> String {
        sha256_hex(self.root.to_string_lossy().to_lowercase().as_bytes())[..16].to_string()
    }
    fn journal_path(&self) -> PathBuf {
        data_dir().join("applied").join(format!("{}.json", self.key()))
    }
    fn backup_dir(&self) -> PathBuf {
        data_dir().join("applied").join(self.key())
    }
}

pub fn sha256_file(path: &Path) -> Result<String> {
    use sha2::Digest;
    let mut f = std::fs::File::open(path)?;
    let mut h = sha2::Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// `a/b\c` -> path inside a base dir; rejects absolute paths, drive letters and `..`.
pub(crate) fn safe_rel(rel: &str) -> Option<PathBuf> {
    let mut p = PathBuf::new();
    for part in rel.split(['/', '\\']) {
        if part.is_empty() || part == "." || part == ".." || part.contains(':') {
            return None;
        }
        p.push(part);
    }
    (!p.as_os_str().is_empty()).then_some(p)
}

// ---------------------------------------------------------------------------------------------- mods

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModMeta {
    pub schema: String,
    pub id: String,
    pub version: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub side: Option<String>,
    #[serde(default)]
    pub game: Option<ModGame>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub depends: Vec<Dep>,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
    #[serde(default)]
    pub risk: Risk,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModGame {
    #[serde(default)]
    pub exe_sha256: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Component {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dep {
    pub id: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub id: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Risk {
    #[serde(default)]
    pub native_code: bool,
    #[serde(default)]
    pub network: bool,
}

#[derive(Debug, Clone)]
pub struct Prepared {
    pub id: String,
    pub version: String,
    pub pkg_sha256: String,
    pub dir: PathBuf,
    pub meta: ModMeta,
}

impl Prepared {
    pub fn native(&self) -> bool {
        self.meta.risk.native_code
    }
}

fn mod_dir(id: &str, version: &str) -> PathBuf {
    data_dir().join("mods").join(id).join(version)
}

const MARKER: &str = ".dbl_package_sha256";

/// `<dir>.<suffix>` next to a version folder; `with_extension` would cut `2026.1104.1341` at the last dot.
fn sibling(dir: &Path, suffix: &str) -> PathBuf {
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    dir.with_file_name(name)
}

/// Download progress of one package: (bytes done, total if known).
pub type Progress = std::sync::Arc<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Downloads (or takes from cache), unpacks and verifies one mod package.
/// `base` is the manifest URL: `mods[].url` may be relative to it.
pub async fn prepare(client: &Client, r: &ModRef, base: &str, exe_sha256: &str, progress: Progress) -> Result<Prepared> {
    let dir = mod_dir(&r.id, &r.version);
    let cached = std::fs::read_to_string(dir.join(MARKER)).ok();
    let pkg_sha256 = match (cached, &r.sha256) {
        (Some(have), Some(want)) if &have == want => have,
        (Some(have), None) => have,
        _ => {
            let Some(url) = &r.url else { bail!("mod_no_source", "{}@{}", r.id, r.version) };
            let url = resolve_url(base, url)?;
            let limit = r.size.map_or(MOD_LIMIT, |s| s.min(MOD_LIMIT));
            let zip_path = sibling(&dir, "zip");
            let (got, len) = net::download_to(client, &url, &zip_path, limit, |d, t| progress(d, t.or(r.size))).await?;
            let bad = if r.size.is_some_and(|s| s != len) {
                Some(format!("size {len} != {}", r.size.unwrap_or(0)))
            } else {
                r.sha256.as_ref().filter(|want| **want != got).map(|_| format!("package sha256 {got}"))
            };
            if let Some(why) = bad {
                let _ = std::fs::remove_file(&zip_path);
                bail!("mod_hash", "{}@{}: {why}", r.id, r.version);
            }
            let (dir2, got2, zip2) = (dir.clone(), got.clone(), zip_path.clone());
            let r = tokio::task::spawn_blocking(move || unpack(&zip2, &dir2, &got2))
                .await
                .map_err(|e| Error::new("internal", e.to_string()))?;
            let _ = std::fs::remove_file(&zip_path);
            r?;
            got
        }
    };
    let (id, version, exe) = (r.id.clone(), r.version.clone(), exe_sha256.to_string());
    let dir2 = dir.clone();
    let meta = tokio::task::spawn_blocking(move || verify(&dir2, &id, &version, &exe))
        .await
        .map_err(|e| Error::new("internal", e.to_string()))??;
    Ok(Prepared { id: r.id.clone(), version: r.version.clone(), pkg_sha256, dir, meta })
}

fn resolve_url(base: &str, url: &str) -> Result<reqwest::Url> {
    let abs = if url.contains("://") {
        url.to_string()
    } else {
        reqwest::Url::parse(base)
            .and_then(|b| b.join(url))
            .map_err(|e| Error::new("bad_url", format!("{url}: {e}")))?
            .to_string()
    };
    check_url(&abs)
}

/// UE4SS itself (`dwmapi.dll` + `ue4ss\`) is part of the install, like the emulator dll; the launcher only adds Lua mods.
pub fn check_runtime(inst: &Install, set: &[Prepared]) -> Result<()> {
    let needs = set.iter().any(|m| m.meta.components.iter().any(|c| c.kind == "ue4ss"));
    if needs && !(inst.win64().join("dwmapi.dll").is_file() && inst.ue4ss_mods().is_dir()) {
        bail!("ue4ss_missing", "{}", inst.win64().display());
    }
    Ok(())
}

fn unpack(zip_path: &Path, dir: &Path, pkg_sha256: &str) -> Result<()> {
    let tmp = sibling(dir, "unpacking");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp)?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(zip_path)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(rel) = entry.enclosed_name().and_then(|p| safe_rel(&p.to_string_lossy())) else {
            bail!("mod_archive", "unsafe path {:?}", entry.name());
        };
        let out = tmp.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
    }
    std::fs::write(tmp.join(MARKER), pkg_sha256)?;
    let _ = std::fs::remove_dir_all(dir);
    std::fs::rename(&tmp, dir)?;
    Ok(())
}

fn exe_matches(listed: &str, exe: &str) -> bool {
    let l = listed.trim_end_matches('…').to_ascii_lowercase();
    l.len() >= 8 && exe.starts_with(&l)
}

fn verify(dir: &Path, id: &str, version: &str, exe_sha256: &str) -> Result<ModMeta> {
    let meta: ModMeta = serde_json::from_slice(&std::fs::read(dir.join("mod.json"))?)
        .map_err(|e| Error::new("mod_invalid", format!("{id}: mod.json: {e}")))?;
    if meta.schema.strip_prefix("dbmod/").map(|v| v.split('.').next().unwrap_or("")) != Some("1") {
        bail!("mod_invalid", "{id}: schema {}", meta.schema);
    }
    if meta.id != id || meta.version != version || !valid_id(&meta.id) {
        bail!("mod_invalid", "{id}: mod.json says {}@{}", meta.id, meta.version);
    }
    if matches!(meta.side.as_deref(), Some("server")) {
        bail!("mod_invalid", "{id}: server-side mod");
    }
    if let Some(g) = &meta.game {
        if !g.exe_sha256.is_empty() && !g.exe_sha256.iter().any(|l| exe_matches(l, exe_sha256)) {
            bail!("mod_game_mismatch", "{id}");
        }
    }
    for (rel, want) in &meta.files {
        let Some(p) = safe_rel(rel) else { bail!("mod_invalid", "{id}: path {rel}") };
        let got = sha256_file(&dir.join(p)).map_err(|e| Error::new("mod_files", format!("{id}: {rel}: {e}")))?;
        if !got.eq_ignore_ascii_case(want) {
            bail!("mod_files", "{id}: {rel} sha256 mismatch");
        }
    }
    for c in &meta.components {
        let Some(p) = safe_rel(&c.path) else { bail!("mod_invalid", "{id}: path {}", c.path) };
        match c.kind.as_str() {
            "pak" if dir.join(&p).is_file() && c.path.to_ascii_lowercase().ends_with(".pak") => {}
            "ue4ss" if dir.join(&p).is_dir() => {}
            "config" => {}
            "native" => bail!("mod_native_unsupported", "{id}"),
            _ => bail!("mod_invalid", "{id}: component {} {}", c.kind, c.path),
        }
        if !meta.files.keys().any(|f| f.replace('\\', "/").starts_with(&c.path.replace('\\', "/"))) {
            bail!("mod_invalid", "{id}: {} is not covered by files{{}}", c.path);
        }
    }
    Ok(meta)
}

fn dir_size(p: &Path) -> u64 {
    std::fs::read_dir(p).map_or(0, |rd| {
        rd.flatten()
            .map(|e| match e.file_type() {
                Ok(t) if t.is_dir() => dir_size(&e.path()),
                _ => e.metadata().map_or(0, |m| m.len()),
            })
            .sum()
    })
}

/// Size of the mod cache: (bytes, package versions).
pub fn cache_info() -> (u64, usize) {
    let root = data_dir().join("mods");
    let versions = std::fs::read_dir(&root)
        .map(|rd| rd.flatten().filter_map(|e| std::fs::read_dir(e.path()).ok()).map(|v| v.flatten().filter(|x| x.path().is_dir()).count()).sum())
        .unwrap_or(0);
    (dir_size(&root), versions)
}

/// Removes cached package versions that no server lists any more (and leftovers of interrupted downloads).
/// `keep` = (id, version) of every mod in every server manifest. Returns removed entries.
pub fn gc_cache(keep: &std::collections::HashSet<(String, String)>) -> usize {
    let root = data_dir().join("mods");
    let Ok(ids) = std::fs::read_dir(&root) else { return 0 };
    let mut removed = 0;
    for id in ids.flatten() {
        let id_name = id.file_name().to_string_lossy().into_owned();
        let Ok(entries) = std::fs::read_dir(id.path()) else { continue };
        for v in entries.flatten() {
            let name = v.file_name().to_string_lossy().into_owned();
            let p = v.path();
            let stale = if p.is_dir() {
                !keep.contains(&(id_name.clone(), name)) // also `<version>.unpacking`
            } else {
                true // `<version>.zip` / `.part` left by an interrupted download
            };
            if stale {
                let ok = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
                removed += ok.is_ok() as usize;
            }
        }
        let _ = std::fs::remove_dir(id.path()); // only if empty
    }
    removed
}

/// Drops the whole mod cache.
pub fn clear_cache() -> Result<()> {
    let root = data_dir().join("mods");
    if root.exists() {
        std::fs::remove_dir_all(root)?;
    }
    Ok(())
}

/// `depends` / `conflicts` within the selected set.
pub fn resolve(set: &[Prepared]) -> Result<()> {
    for m in set {
        for d in &m.meta.depends {
            let Some(found) = set.iter().find(|x| x.id == d.id) else {
                bail!("mod_depends", "{} -> {}", m.id, d.id);
            };
            if let Some(req) = &d.version {
                let ok = semver::VersionReq::parse(req)
                    .ok()
                    .zip(semver::Version::parse(&found.version).ok())
                    .is_some_and(|(r, v)| r.matches(&v));
                if !ok {
                    bail!("mod_depends", "{} -> {} {req} (have {})", m.id, d.id, found.version);
                }
            }
        }
        for c in &m.meta.conflicts {
            if set.iter().any(|x| x.id == c.id) {
                bail!("mod_conflict", "{} x {}: {}", m.id, c.id, c.reason.clone().unwrap_or_default());
            }
        }
    }
    Ok(())
}

/// sha256 over sorted `id@version:sha256` lines joined by `\n` (no trailing newline).
pub fn modset_sha256(set: &[Prepared]) -> String {
    let mut lines: Vec<String> = set.iter().map(|m| format!("{}@{}:{}", m.id, m.version, m.pkg_sha256)).collect();
    lines.sort();
    sha256_hex(lines.join("\n").as_bytes())
}

// ---------------------------------------------------------------------------------------------- apply / restore

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Op {
    /// We created it; restore deletes it.
    Created { path: PathBuf },
    /// Moved aside; restore moves it back.
    Moved { from: PathBuf, to: PathBuf },
    /// Overwritten; restore copies the backup back.
    Replaced { path: PathBuf, backup: PathBuf },
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    root: PathBuf,
    server_id: String,
    ops: Vec<Op>,
    snapshot: BTreeMap<String, String>,
}

pub struct Plan<'a> {
    pub server_id: &'a str,
    pub mods: &'a [Prepared],
    pub ca_pem: Option<&'a [u8]>,
    pub goldberg: Option<(&'a str, &'a str)>,
    pub isolate_unlisted: bool,
    /// The INTL login goes to this address instead of sg.intlgame.com (no `hosts` for it): `https://localhost` — the
    /// relay's 443 on this PC, and `localhost` is in the server certificate
    pub intl_url: Option<&'a str>,
    /// Server settings for client UE4SS mods (`client_settings` of the manifest) — `server_settings.ini` in the mod folder
    pub client_settings: Option<&'a serde_json::Value>,
}

/// `server_settings.ini` text for one mod: `[section]` + `key = value`; booleans as 1/0, nested objects skipped.
/// Keys and sections are restricted to `[A-Za-z0-9_]`, values to one line — the mod parses it with a simple pattern.
pub fn server_settings_ini(server_id: &str, sections: &serde_json::Value) -> Option<String> {
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    let obj = sections.as_object()?;
    let mut out = format!("; generated by the launcher from the settings of server {server_id}; rewritten on every start\r\n");
    let mut any = false;
    for (sec, vals) in obj {
        let Some(vals) = vals.as_object() else { continue };
        if !ok(sec) {
            continue;
        }
        out.push_str(&format!("\r\n[{sec}]\r\n"));
        for (k, v) in vals {
            if !ok(k) {
                continue;
            }
            let val = match v {
                serde_json::Value::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.replace(['\r', '\n'], " "),
                _ => continue,
            };
            out.push_str(&format!("{k} = {val}\r\n"));
            any = true;
        }
    }
    any.then_some(out)
}

/// The open INTL config: only the address; everything else (keys, switches) stays the SDK's default
pub fn intl_config_text(url: &str) -> String {
    format!("[INTL environment]\r\nINTL_URL = {url}\r\nACCOUNT_URL = {url}\r\nGAME_ID = 30007\r\n")
}

pub fn has_journal(inst: &Install) -> bool {
    inst.journal_path().exists()
}

/// UE4SS's own Lua mods (its release; the runtime bundle, bundle.rs, lays some of them): they stay when foreign UE4SS
/// mods step aside. Whatever else lies in `ue4ss\Mods` is a dev tool or another server's mod.
pub(crate) const UE4SS_BUILTIN_MODS: [&str; 10] = [
    "BPML_GenericFunctions",
    "BPModLoaderMod",
    "CheatManagerEnablerMod",
    "ConsoleCommandsMod",
    "ConsoleEnablerMod",
    "Keybinds",
    "LineTraceMod",
    "SplitScreenMod",
    "jsbLuaProfilerMod",
    "shared",
];

fn is_builtin_ue4ss_mod(name: &str) -> bool {
    UE4SS_BUILTIN_MODS.iter().any(|b| b.eq_ignore_ascii_case(name))
}

/// Folder names the plan's UE4SS components take in `ue4ss\Mods`
fn plan_ue4ss_names(plan: &Plan) -> Vec<String> {
    plan.mods
        .iter()
        .flat_map(|m| m.meta.components.iter().filter(|c| c.kind == "ue4ss"))
        .filter_map(|c| safe_rel(&c.path)?.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect()
}

fn is_vanilla_pak(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    (n.starts_with("pakchunk") && n.contains("-windows")) || n == "global.ucas" || n == "global.utoc"
}

/// What the restore check compares: Paks and UE4SS mods by size+mtime, small files by sha256.
fn snapshot(inst: &Install) -> BTreeMap<String, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let Ok(md) = e.metadata() else { continue };
            if md.is_dir() {
                walk(base, &p, out);
            } else {
                let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
                let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().to_string();
                out.insert(rel, format!("{}:{}", md.len(), mtime));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(&inst.root, &inst.paks(), &mut out);
    walk(&inst.root, &inst.ue4ss_mods(), &mut out);
    for p in [
        inst.cacert(),
        inst.steam_settings().join("force_steamid.txt"),
        inst.steam_settings().join("force_account_name.txt"),
        inst.intl_config(),
        inst.intl_config_encrypted(),
    ] {
        if let Ok(h) = sha256_file(&p) {
            out.insert(p.strip_prefix(&inst.root).unwrap_or(&p).to_string_lossy().to_string(), h);
        }
    }
    out
}

fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dst)?;
        } else {
            std::fs::copy(e.path(), &dst)?;
        }
    }
    Ok(())
}

fn free_name(dir: &Path, name: &std::ffi::OsStr) -> PathBuf {
    let mut p = dir.join(name);
    let mut i = 1;
    while p.exists() {
        p = dir.join(format!("{}.{i}", name.to_string_lossy()));
        i += 1;
    }
    p
}

struct Applier<'a> {
    inst: &'a Install,
    j: Journal,
}

impl Applier<'_> {
    /// Journal first, then act.
    fn record(&mut self, op: Op) -> Result<()> {
        self.j.ops.push(op);
        crate::store::save(&self.inst.journal_path(), &self.j)
    }

    fn create_dir(&mut self, p: &Path) -> Result<()> {
        self.record(Op::Created { path: p.to_path_buf() })?;
        std::fs::create_dir_all(p)?;
        Ok(())
    }

    fn move_aside(&mut self, from: &Path, aside_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(aside_dir)?;
        let to = free_name(aside_dir, from.file_name().unwrap_or_default());
        self.record(Op::Moved { from: from.to_path_buf(), to: to.clone() })?;
        std::fs::rename(from, &to)?;
        Ok(())
    }

    fn write_file(&mut self, p: &Path, data: &[u8]) -> Result<()> {
        if p.exists() {
            let backup = self.inst.backup_dir().join(format!("{}-{}", self.j.ops.len(), p.file_name().unwrap_or_default().to_string_lossy()));
            std::fs::create_dir_all(self.inst.backup_dir())?;
            std::fs::copy(p, &backup)?;
            self.record(Op::Replaced { path: p.to_path_buf(), backup })?;
        } else {
            self.record(Op::Created { path: p.to_path_buf() })?;
        }
        std::fs::write(p, data)?;
        Ok(())
    }
}

/// Lays the profile out in the game folder. On any error everything done so far is undone.
pub fn apply(inst: &Install, plan: &Plan) -> Result<()> {
    if has_journal(inst) {
        bail!("profile_dirty", "{}", inst.root.display());
    }
    let j = Journal { root: inst.root.clone(), server_id: plan.server_id.to_string(), ops: Vec::new(), snapshot: snapshot(inst) };
    let mut a = Applier { inst, j };
    crate::store::save(&inst.journal_path(), &a.j)?;
    match apply_inner(&mut a, plan) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = restore(inst);
            Err(e)
        }
    }
}

fn apply_inner(a: &mut Applier, plan: &Plan) -> Result<()> {
    let inst = a.inst;
    let paks = inst.paks();
    let disabled = inst.content().join("_dbl_disabled");
    let own = format!("~dbl_{}", plan.server_id);

    // 1. Paks: other launcher profiles always out; foreign mods out when the server asks for it.
    let mut aside = Vec::new();
    for e in std::fs::read_dir(&paks)?.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
        if name.starts_with("~dbl_") || (plan.isolate_unlisted && (is_dir || !is_vanilla_pak(&name))) {
            aside.push(e.path());
        }
    }
    for p in aside {
        a.move_aside(&p, &disabled)?;
    }

    // 1b. Foreign UE4SS mods (dev tools, other servers' mods) step aside too when the server asks for it; UE4SS's own
    // mods and the plan's stay. mods.txt / mods.json stay as they are: UE4SS starts a line of mods.txt only for a folder
    // it found in Mods, a line without one is skipped.
    let ue_mods = inst.ue4ss_mods();
    if plan.isolate_unlisted && ue_mods.is_dir() {
        let ours = plan_ue4ss_names(plan);
        let mut aside = Vec::new();
        for e in std::fs::read_dir(&ue_mods)?.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_dir && !is_builtin_ue4ss_mod(&name) && !ours.iter().any(|o| o.eq_ignore_ascii_case(&name)) {
                aside.push(e.path());
            }
        }
        for p in aside {
            a.move_aside(&p, &inst.win64().join("ue4ss").join("_dbl_disabled"))?;
        }
    }

    // 2. Our paks, named by priority so that load order does not depend on the author's file names.
    let pak_dir = paks.join(&own);
    let mut paks_list: Vec<(&Prepared, &Component)> =
        plan.mods.iter().flat_map(|m| m.meta.components.iter().filter(|c| c.kind == "pak").map(move |c| (m, c))).collect();
    paks_list.sort_by_key(|(m, c)| (c.priority, m.id.clone()));
    if !paks_list.is_empty() {
        a.create_dir(&pak_dir)?;
    }
    for (n, (m, c)) in paks_list.iter().enumerate() {
        let src = m.dir.join(safe_rel(&c.path).unwrap_or_default());
        let stem = format!("zzzzzzzz_{:03}_{:02}_{}_P", c.priority.clamp(0, 999), n, m.id.replace('.', "-"));
        for ext in ["pak", "ucas", "utoc", "sig"] {
            let companion = src.with_extension(ext);
            if companion.is_file() {
                std::fs::copy(&companion, pak_dir.join(format!("{stem}.{ext}")))?;
            }
        }
    }

    // 3. UE4SS mods.
    for m in plan.mods {
        for c in m.meta.components.iter().filter(|c| c.kind == "ue4ss") {
            let mods_dir = inst.ue4ss_mods();
            if !mods_dir.is_dir() {
                bail!("ue4ss_missing", "{}", mods_dir.display());
            }
            let src = m.dir.join(safe_rel(&c.path).unwrap_or_default());
            let dst = mods_dir.join(src.file_name().unwrap_or_default());
            if dst.exists() {
                a.move_aside(&dst, &inst.win64().join("ue4ss").join("_dbl_disabled"))?;
            }
            a.create_dir(&dst)?;
            copy_dir(&src, &dst)?;
            std::fs::write(dst.join("enabled.txt"), b"")?;
            // settings the server gives this mod (the folder is ours: restore removes it with the file)
            let name = dst.file_name().unwrap_or_default().to_string_lossy().to_string();
            if let Some(ini) = plan.client_settings.and_then(|cs| cs.get(&name)).and_then(|s| server_settings_ini(plan.server_id, s)) {
                std::fs::write(dst.join("server_settings.ini"), ini.as_bytes())?;
            }
        }
    }

    // 4. cacert.pem = the game's own file + this server's CA.
    if let Some(ca) = plan.ca_pem {
        let orig = std::fs::read(inst.cacert())?;
        let mut data = orig;
        if !data.ends_with(b"\n") {
            data.push(b'\n');
        }
        data.extend_from_slice(ca);
        a.write_file(&inst.cacert(), &data)?;
    }

    // 5. INTL login address: the encrypted config steps aside, the open one is ours while we run.
    if let Some(url) = plan.intl_url {
        if inst.intl_config_encrypted().is_file() {
            a.move_aside(&inst.intl_config_encrypted(), &inst.win64().join("_dbl_disabled"))?;
        }
        a.write_file(&inst.intl_config(), intl_config_text(url).as_bytes())?;
    }

    // 6. Goldberg: steam_settings\force_* override the emulator's own profile while we run.
    if let Some((open_id, nick)) = plan.goldberg {
        let dir = inst.steam_settings();
        if !dir.exists() {
            a.create_dir(&dir)?;
        }
        a.write_file(&dir.join("force_steamid.txt"), open_id.as_bytes())?;
        a.write_file(&dir.join("force_account_name.txt"), nick.as_bytes())?;
    }
    Ok(())
}

/// Undoes the journal of this install (also after a crash). Returns differences against the snapshot.
pub fn restore(inst: &Install) -> Result<Vec<String>> {
    let path = inst.journal_path();
    let Ok(bytes) = std::fs::read(&path) else { return Ok(Vec::new()) };
    let j: Journal = serde_json::from_slice(&bytes)?;
    let mut failed = Vec::new();
    for op in j.ops.iter().rev() {
        let r: std::io::Result<()> = match op {
            Op::Created { path } => {
                if !path.starts_with(&j.root) {
                    Ok(())
                } else if path.is_dir() {
                    std::fs::remove_dir_all(path)
                } else if path.exists() {
                    std::fs::remove_file(path)
                } else {
                    Ok(())
                }
            }
            Op::Moved { from, to } => {
                if to.exists() && !from.exists() {
                    std::fs::rename(to, from)
                } else {
                    Ok(())
                }
            }
            Op::Replaced { path, backup } => {
                if backup.exists() {
                    std::fs::copy(backup, path).map(|_| ())
                } else {
                    Ok(())
                }
            }
        };
        if let Err(e) = r {
            failed.push(format!("{op:?}: {e}"));
        }
    }
    if !failed.is_empty() {
        bail!("restore_failed", "{}", failed.join("; "));
    }
    for d in [inst.content().join("_dbl_disabled"), inst.win64().join("ue4ss").join("_dbl_disabled"), inst.win64().join("_dbl_disabled")] {
        let _ = std::fs::remove_dir(d); // only if empty
    }
    let now = snapshot(inst);
    let mut diff = Vec::new();
    for (k, v) in &j.snapshot {
        match now.get(k) {
            None => diff.push(format!("missing {k}")),
            Some(n) if n != v => diff.push(format!("changed {k}")),
            _ => {}
        }
    }
    diff.extend(now.keys().filter(|k| !j.snapshot.contains_key(*k)).map(|k| format!("extra {k}")));
    let _ = std::fs::remove_dir_all(inst.backup_dir());
    std::fs::remove_file(&path)?;
    Ok(diff)
}

/// Journals left by a crashed launcher: (install root, server id).
pub fn pending_journals() -> Vec<(PathBuf, String)> {
    let Ok(rd) = std::fs::read_dir(data_dir().join("applied")) else { return Vec::new() };
    rd.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_slice::<Journal>(&std::fs::read(e.path()).ok()?).ok())
        .map(|j| (j.root, j.server_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_settings_ini_writes_sections() {
        let v = serde_json::json!({ "stealth": { "distance": 1000, "curve": 1.5, "enabled": true, "bad key": 1, "nested": {} } });
        let ini = server_settings_ini("ru.test", &v).unwrap();
        assert!(ini.contains("[stealth]\r\n"));
        assert!(ini.contains("distance = 1000\r\n") && ini.contains("curve = 1.5\r\n") && ini.contains("enabled = 1\r\n"));
        assert!(!ini.contains("bad key") && !ini.contains("nested"));
        assert!(server_settings_ini("x", &serde_json::json!({})).is_none());
    }

    #[test]
    fn safe_paths() {
        assert!(safe_rel("Paks/a.pak").is_some());
        assert!(safe_rel("ue4ss\\Mod").is_some());
        for bad in ["../x", "a/../b", "C:/x", "/abs", "", "a//b"] {
            assert!(safe_rel(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn sibling_keeps_full_version() {
        let d = Path::new("C:/cache/mods/ru.loot/2026.1104.1341");
        assert_eq!(sibling(d, "zip"), Path::new("C:/cache/mods/ru.loot/2026.1104.1341.zip"));
    }

    fn put(p: &Path, data: &[u8]) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }

    /// An install with UE4SS's own mods, foreign ones (dev tools), an old copy of the server's mod, and the server's package
    fn isolation_fixture(tag: &str) -> (PathBuf, Install, Prepared) {
        let tmp = std::env::temp_dir().join(format!("dbl_isolate_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let game = tmp.join("game");
        put(&game.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe"), b"");
        put(&game.join("Dungeonborne/Content/Paks/pakchunk0-Windows.pak"), b"vanilla");
        let inst = Install::open(game.to_str().unwrap()).unwrap();
        let mods = inst.ue4ss_mods();
        put(&mods.join("mods.txt"), b"BPModLoaderMod : 1\r\nJesterSwap : 1\r\nKeybinds : 1\r\n");
        put(&mods.join("mods.json"), b"[]");
        put(&mods.join("BPModLoaderMod/Scripts/main.lua"), b"builtin");
        put(&mods.join("Keybinds/Scripts/main.lua"), b"builtin");
        put(&mods.join("shared/UEHelpers/UEHelpers.lua"), b"builtin");
        put(&mods.join("JesterSwap/Scripts/main.lua"), b"foreign");
        put(&mods.join("ClientProbe/enabled.txt"), b"");
        put(&mods.join("DungeonborneRuClient/Scripts/main.lua"), b"old copy");
        let pkg = tmp.join("pkg");
        put(&pkg.join("ue4ss/DungeonborneRuClient/Scripts/main.lua"), b"server");
        let meta: ModMeta = serde_json::from_value(serde_json::json!({
            "schema": "dbmod/1", "id": "ru.client-hooks", "version": "1",
            "components": [{ "type": "ue4ss", "path": "ue4ss/DungeonborneRuClient" }]
        }))
        .unwrap();
        let m = Prepared { id: meta.id.clone(), version: meta.version.clone(), pkg_sha256: String::new(), dir: pkg, meta };
        (tmp, inst, m)
    }

    fn plan<'a>(mods: &'a [Prepared], isolate: bool) -> Plan<'a> {
        Plan { server_id: "test.isolate", mods, ca_pem: None, goldberg: None, isolate_unlisted: isolate, intl_url: None, client_settings: None }
    }

    #[test]
    fn isolate_moves_foreign_ue4ss_mods_aside_and_back() {
        let (tmp, inst, m) = isolation_fixture("on");
        let mods = inst.ue4ss_mods();
        let mods_txt = std::fs::read(mods.join("mods.txt")).unwrap();
        let ms = [m];
        apply(&inst, &plan(&ms, true)).unwrap();

        for foreign in ["JesterSwap", "ClientProbe"] {
            assert!(!mods.join(foreign).exists(), "{foreign} moved away for the game session");
        }
        let aside = inst.win64().join("ue4ss").join("_dbl_disabled");
        assert!(aside.join("JesterSwap/Scripts/main.lua").is_file());
        for builtin in ["BPModLoaderMod", "Keybinds", "shared"] {
            assert!(mods.join(builtin).is_dir(), "{builtin} is built in, stays");
        }
        assert_eq!(std::fs::read(mods.join("DungeonborneRuClient/Scripts/main.lua")).unwrap(), b"server", "the server's mod is in place");
        assert_eq!(std::fs::read(mods.join("mods.txt")).unwrap(), mods_txt, "mods.txt is not touched");
        assert!(mods.join("mods.json").is_file());

        let diff = restore(&inst).unwrap();
        assert!(diff.is_empty(), "{diff:?}");
        assert_eq!(std::fs::read(mods.join("JesterSwap/Scripts/main.lua")).unwrap(), b"foreign", "came back");
        assert!(mods.join("ClientProbe/enabled.txt").is_file());
        assert_eq!(std::fs::read(mods.join("DungeonborneRuClient/Scripts/main.lua")).unwrap(), b"old copy");
        assert!(!aside.exists(), "the empty folder is removed");
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn without_isolate_foreign_ue4ss_mods_stay() {
        let (tmp, inst, m) = isolation_fixture("off");
        let ms = [m];
        apply(&inst, &plan(&ms, false)).unwrap();
        assert!(inst.ue4ss_mods().join("JesterSwap/Scripts/main.lua").is_file());
        assert!(inst.ue4ss_mods().join("ClientProbe").is_dir());
        assert!(restore(&inst).unwrap().is_empty());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn vanilla_paks() {
        assert!(is_vanilla_pak("pakchunk0-Windows.pak"));
        assert!(is_vanilla_pak("global.utoc"));
        assert!(!is_vanilla_pak("zz_Loot_P.pak"));
    }

    #[test]
    fn exe_prefix() {
        let exe = "62d509a00d984696e021fb5ddba9f87fd1b27c84f695877345ff5fe9401f58b3";
        assert!(exe_matches("62d509a0…", exe));
        assert!(exe_matches(exe, exe));
        assert!(!exe_matches("62d5", exe));
        assert!(!exe_matches("deadbeef", exe));
    }
}
