//! ws-core — the Dungeonborne workspace (docs/WORKSPACE_PLAN.md): finding the root, members, local.toml, DB_HOME,
//! structure check (check), machine setup (bootstrap/doctor), game copies (game), log collection (runs).
//! DevKit.exe uses the same library (step 7); ws.exe is the console shell.
pub mod bootstrap;
pub mod check;
pub mod components;
pub mod dump;
pub mod export;
pub mod game;
pub mod preview;
pub mod runs;
pub mod secrets;
pub mod stand;
pub mod store;
pub mod vault;
pub mod visibility;

use std::path::{Component, Path, PathBuf};

/// A finding of a check or setup: an error, a warning or an action done.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Done,
    Info,
    /// info, not a problem: an external program is needed only for specific jobs (tools.lock kind = "external")
    Note,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub level: Level,
    pub rule: &'static str,
    pub path: String,
    pub msg: String,
}

impl Finding {
    pub fn new(
        level: Level,
        rule: &'static str,
        path: impl Into<String>,
        msg: impl Into<String>,
    ) -> Self {
        Finding {
            level,
            rule,
            path: path.into(),
            msg: msg.into(),
        }
    }
}

/// A member (member.toml).
#[derive(Debug, Clone)]
pub struct Member {
    /// path from the root, with "/"
    pub path: String,
    pub name: String,
    pub kind: String,
    pub depends_on: Vec<String>,
    /// member files where absolute paths are allowed (member.toml [known-absolute-paths] files)
    pub known_absolute: Vec<String>,
    /// [member] visibility: public | team | owner (empty — not marked)
    pub visibility: String,
    /// [visibility] team / owner — parts of the member stricter than the member itself (path patterns from the member folder)
    pub vis_team: Vec<String>,
    pub vis_owner: Vec<String>,
}

pub struct Workspace {
    pub root: PathBuf,
    pub manifest: toml::Value,
    pub members: Vec<Member>,
    pub local: toml::Value,
}

/// Settings folder in the root: workspace.toml, tools.lock, secrets.toml, local(.example).toml, artifacts.toml,
/// bootstrap.manifest.toml. Only Cargo.toml, Cargo.lock and rust-toolchain.toml stay in the root (cargo and rustup look for them).
pub const CONFIG: &str = "config";

impl Workspace {
    /// A file from the settings folder (`config/<name>`).
    pub fn config(&self, name: &str) -> PathBuf {
        self.root.join(CONFIG).join(name)
    }

    /// The root is the nearest folder upwards with config/workspace.toml (from `start`).
    pub fn find(start: &Path) -> Result<Workspace, String> {
        let start = start
            .canonicalize()
            .map_err(|e| format!("{}: {}", start.display(), e))?;
        let root = start
            .ancestors()
            .find(|p| p.join(CONFIG).join("workspace.toml").is_file())
            .ok_or_else(|| format!("no {CONFIG}/workspace.toml above {}", start.display()))?
            .to_path_buf();
        Self::open(&root)
    }

    pub fn open(root: &Path) -> Result<Workspace, String> {
        let root = strip_verbatim(root);
        let manifest = read_toml(&root.join(CONFIG).join("workspace.toml"))?;
        let local = read_toml(&root.join(CONFIG).join("local.toml"))
            .unwrap_or(toml::Value::Table(Default::default()));
        let mut members = Vec::new();
        for m in str_list(manifest.get("workspace").and_then(|w| w.get("members"))) {
            // a partial checkout (ws components): a member that is not on disk is read from git
            let doc = read_toml(&root.join(&m).join("member.toml"))
                .ok()
                .or_else(|| member_from_git(&root, &m));
            let get = |k: &str| {
                doc.as_ref()
                    .and_then(|d| d.get("member"))
                    .and_then(|x| x.get(k))
            };
            members.push(Member {
                name: get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                kind: get("kind")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                depends_on: str_list(get("depends_on")),
                known_absolute: str_list(
                    doc.as_ref()
                        .and_then(|d| d.get("known-absolute-paths"))
                        .and_then(|k| k.get("files")),
                )
                .into_iter()
                .map(|f| f.split_whitespace().next().unwrap_or("").to_string())
                .collect(),
                visibility: get("visibility")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                vis_team: str_list(
                    doc.as_ref()
                        .and_then(|d| d.get("visibility"))
                        .and_then(|v| v.get("team")),
                ),
                vis_owner: str_list(
                    doc.as_ref()
                        .and_then(|d| d.get("visibility"))
                        .and_then(|v| v.get("owner")),
                ),
                path: m,
            });
        }
        Ok(Workspace {
            root,
            manifest,
            members,
            local,
        })
    }

    /// The member a path belongs to (from the root, with "/"): the longest matching prefix.
    pub fn member_of(&self, rel: &str) -> Option<&Member> {
        self.members
            .iter()
            .filter(|m| rel == m.path || rel.starts_with(&format!("{}/", m.path)))
            .max_by_key(|m| m.path.len())
    }

    /// A string value from local.toml: [section] key.
    pub fn local_str(&self, section: &str, key: &str) -> Option<String> {
        self.local
            .get(section)
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_str())
            .map(String::from)
    }

    /// DB_HOME: the DB_HOME variable, else local.toml [home] path, else [workspace.home] default from the root (../home).
    pub fn home(&self) -> PathBuf {
        if let Some(h) = std::env::var_os("DB_HOME").filter(|h| !h.is_empty()) {
            return native(Path::new(&h));
        }
        if let Some(h) = self.local_str("home", "path") {
            return native(Path::new(&h));
        }
        let d = self
            .manifest
            .get("workspace")
            .and_then(|w| w.get("home"))
            .and_then(|h| h.get("default"))
            .and_then(|v| v.as_str())
            .unwrap_or("../home");
        normalize(&self.root.join(d))
    }

    /// Path with a prefix: "home:…" — in DB_HOME, "ws:…" — from the workspace root, "legacy:…" — from local.toml [legacy] root
    /// (the old <data>, until the artifacts are in the store), else — as is.
    pub fn resolve(&self, p: &str) -> Option<PathBuf> {
        self.resolve_raw(p).map(|x| native(&x))
    }

    fn resolve_raw(&self, p: &str) -> Option<PathBuf> {
        if let Some(r) = p.strip_prefix("home:") {
            Some(self.home().join(r))
        } else if let Some(r) = p.strip_prefix("ws:") {
            Some(self.root.join(r))
        } else if let Some(r) = p.strip_prefix("legacy:") {
            self.local_str("legacy", "root")
                .map(|l| PathBuf::from(l).join(r))
        } else {
            Some(PathBuf::from(p))
        }
    }

    /// Allowed entries in the root (workspace.toml [workspace.layout] root) and shared folders outside members (shared).
    pub fn layout(&self, key: &str) -> Vec<String> {
        str_list(
            self.manifest
                .get("workspace")
                .and_then(|w| w.get("layout"))
                .and_then(|l| l.get(key)),
        )
    }
}

pub fn read_toml(path: &Path) -> Result<toml::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    text.parse::<toml::Value>()
        .map_err(|e| format!("{}: {}", path.display(), e))
}

pub fn str_list(v: Option<&toml::Value>) -> Vec<String> {
    v.and_then(|a| a.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

/// Removes "." and ".." without touching the disk.
/// member.toml of a member that is not on disk (partial checkout): from HEAD.
fn member_from_git(root: &Path, member: &str) -> Option<toml::Value> {
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("HEAD:{member}/member.toml")])
        .output()
        .ok()?;
    if !o.status.success() {
        return None;
    }
    String::from_utf8_lossy(&o.stdout)
        .parse::<toml::Value>()
        .ok()
}

pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Path separators native to the OS (local.toml uses "/", on Windows output and join use "\").
pub fn native(p: &Path) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(p.to_string_lossy().replace('/', "\\"))
    } else {
        p.to_path_buf()
    }
}

/// canonicalize on Windows gives "\\?\F:\…" — for output and join, without the prefix.
pub fn strip_verbatim(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string())
}

/// Path from the root with "/".
pub fn rel(root: &Path, p: &Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}

pub fn sha256_file(path: &Path) -> Option<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut h = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Some(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Copy of a whole file or folder (folders recursively); existing files are overwritten.
pub fn copy_tree(src: &Path, dst: &Path) -> std::io::Result<u64> {
    if src.is_file() {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p)?;
        }
        return std::fs::copy(src, dst);
    }
    let mut total = 0;
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        total += copy_tree(&e.path(), &dst.join(e.file_name()))?;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_and_rel() {
        assert_eq!(normalize(Path::new("a/b/../c/./d")), PathBuf::from("a/c/d"));
        assert_eq!(rel(Path::new("/r"), Path::new("/r/x/y")), "x/y");
    }

    #[test]
    fn member_of_longest_prefix() {
        let m = |p: &str| Member {
            path: p.into(),
            name: p.into(),
            kind: "x".into(),
            depends_on: vec![],
            known_absolute: vec![],
            visibility: String::new(),
            vis_team: vec![],
            vis_owner: vec![],
        };
        let ws = Workspace {
            root: PathBuf::from("/r"),
            manifest: toml::Value::Table(Default::default()),
            members: vec![m("game"), m("game/ue4ss")],
            local: toml::Value::Table(Default::default()),
        };
        assert_eq!(
            ws.member_of("game/ue4ss/Mods/a.lua").unwrap().path,
            "game/ue4ss"
        );
        assert_eq!(ws.member_of("game/x").unwrap().path, "game");
        assert!(ws.member_of("gamex/y").is_none());
    }
}
