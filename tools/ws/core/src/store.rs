//! Artifact store (docs/WORKSPACE_PLAN.md, step 5): a private folder over HTTPS (WebDAV), objects by sha256.
//!
//! local.toml [store]:
//!   url      — folder address for reading (team member: a read-only sub-account)
//!   user     — login for this address
//!   push_url, push_user — the same for uploading (owner only: a sub-account with write access to the same folder)
//! Passwords come only from the environment: ARTIFACT_STORE_PASSWORD / ARTIFACT_STORE_PUSH_PASSWORD
//! (`ws secrets run artifact_store -- ws bootstrap`, `ws secrets run artifact_store_push -- ws artifacts push --apply`).
//! The password never gets onto the curl command line: login and password go to it as a config via stdin (`-K -`).
//!
//! An object is a file `<url>/<sha256>`. A file artifact is stored as is; a folder — as tar (`store_tar = true` in artifacts.toml).
//! artifacts.toml: `store_sha256` — the object hash; ws bootstrap downloads it if the artifact is not in place.
use crate::{sha256_file, Workspace};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const ENV_READ: &str = "ARTIFACT_STORE_PASSWORD";
pub const ENV_PUSH: &str = "ARTIFACT_STORE_PUSH_PASSWORD";

pub struct Store {
    url: String,
    user: String,
    password: String,
}

/// What is wrong with the setup: no address (store not configured) or no password in the environment.
pub enum Missing {
    NotConfigured,
    NoPassword(&'static str),
}

impl Store {
    /// Reading (push = false) or uploading (push = true) per local.toml [store] and the password from the environment.
    pub fn open(ws: &Workspace, push: bool) -> Result<Store, Missing> {
        let (k_url, k_user, env) = if push {
            ("push_url", "push_user", ENV_PUSH)
        } else {
            ("url", "user", ENV_READ)
        };
        let (Some(url), Some(user)) = (ws.local_str("store", k_url), ws.local_str("store", k_user))
        else {
            return Err(Missing::NotConfigured);
        };
        // reading: also from an open vault session (artifact_store, for ws bootstrap); uploading: owner, env only
        let password = if push {
            std::env::var(env).ok().filter(|p| !p.is_empty())
        } else {
            crate::vault::env_or_session(ws, "artifact_store", env)
        }
        .ok_or(Missing::NoPassword(env))?;
        Ok(Store {
            url: url.trim_end_matches('/').to_string(),
            user,
            password,
        })
    }

    fn object_url(&self, sha: &str) -> String {
        format!("{}/{sha}", self.url)
    }

    /// curl with the login from stdin; args — the rest. retry — repeat on a dropped connection (not for existence checks: 404 there
    /// is an answer, not a failure).
    fn curl(&self, args: &[&str], retry: bool) -> Result<std::process::ExitStatus, String> {
        let mut cmd = Command::new(curl_exe());
        cmd.args(["-fsS", "-K", "-"]);
        if retry {
            cmd.args(["--retry", "5", "--retry-all-errors", "--retry-delay", "2"]);
        }
        cmd.args(args).stdin(Stdio::piped()).stdout(Stdio::null());
        let mut child = cmd.spawn().map_err(|e| format!("curl: {e}"))?;
        let cfg = format!(
            "user = \"{}:{}\"\n",
            quote(&self.user),
            quote(&self.password)
        );
        child
            .stdin
            .take()
            .ok_or("curl: no stdin")?
            .write_all(cfg.as_bytes())
            .map_err(|e| format!("curl: {e}"))?;
        child.wait().map_err(|e| format!("curl: {e}"))
    }

    /// Download an object by name into `part` without a hash check (release index, signed releases have their own
    /// check). Ok(false) — no such object.
    pub fn get(&self, name: &str, part: &Path) -> Result<bool, String> {
        if let Some(d) = part.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        if !self.exists(name)? {
            return Ok(false);
        }
        let url = self.object_url(name);
        let p = part.to_string_lossy().to_string();
        let st = self.curl(&["-L", "-o", &p, &url], true)?;
        if !st.success() {
            let _ = std::fs::remove_file(part);
            return Err(format!(
                "store: download {name}: curl exit code {:?}",
                st.code()
            ));
        }
        Ok(true)
    }

    /// Download an object into `part` and check sha256; on mismatch the file is deleted.
    pub fn fetch(&self, sha: &str, part: &Path) -> Result<(), String> {
        if !self.get(sha, part)? {
            return Err(format!("store: no object {sha}"));
        }
        if sha256_file(part).as_deref() != Some(sha) {
            let _ = std::fs::remove_file(part);
            return Err(format!(
                "store: {sha}: sha256 does not match - file deleted"
            ));
        }
        Ok(())
    }

    /// Whether the object is in the store (HEAD).
    pub fn exists(&self, sha: &str) -> Result<bool, String> {
        let url = self.object_url(sha);
        // the response code, not -f: 404 — no object, 401 — wrong login/password (that is an error, not "no")
        let mut child = Command::new(curl_exe())
            .args([
                "-sS",
                "-K",
                "-",
                "-I",
                "-o",
                "NUL",
                "-w",
                "%{http_code}",
                &url,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("curl: {e}"))?;
        let cfg = format!(
            "user = \"{}:{}\"\n",
            quote(&self.user),
            quote(&self.password)
        );
        child
            .stdin
            .take()
            .ok_or("curl: no stdin")?
            .write_all(cfg.as_bytes())
            .map_err(|e| format!("curl: {e}"))?;
        let o = child.wait_with_output().map_err(|e| format!("curl: {e}"))?;
        match String::from_utf8_lossy(&o.stdout).trim() {
            c if c.starts_with('2') => Ok(true),
            "404" => Ok(false),
            "401" | "403" => Err(format!(
                "store: {sha}: access denied (HTTP {}) - check [store] user and the password",
                String::from_utf8_lossy(&o.stdout).trim()
            )),
            c => Err(format!("store: check {sha}: HTTP {c:?}")),
        }
    }

    /// Upload a file under the name `name` (PUT).
    pub fn put(&self, sha: &str, file: &Path) -> Result<(), String> {
        let url = self.object_url(sha);
        let f = file.to_string_lossy().to_string();
        let st = self.curl(&["-T", &f, &url], true)?;
        if st.success() {
            Ok(())
        } else {
            Err(format!(
                "store: upload {sha}: curl exit code {:?}",
                st.code()
            ))
        }
    }
}

/// `ws artifacts push [--apply]` (owner): every artifact of artifacts.toml present on this machine goes to the
/// store, its hash to store_sha256. Without apply — only the plan. Already uploaded ones (same hash in the store)
/// are skipped.
pub fn push(ws: &Workspace, apply: bool) -> Vec<crate::Finding> {
    use crate::{Finding, Level};
    let mut out = Vec::new();
    let toml_path = ws.config("artifacts.toml");
    let doc = match crate::read_toml(&toml_path) {
        Ok(d) => d,
        Err(e) => return vec![Finding::new(Level::Error, "store", "artifacts.toml", e)],
    };
    let store = match Store::open(ws, true) {
        Ok(s) => Some(s),
        Err(Missing::NotConfigured) => {
            out.push(Finding::new(
                Level::Error,
                "store",
                "local.toml [store]",
                "push_url and push_user are not set",
            ));
            None
        }
        Err(Missing::NoPassword(env)) => {
            out.push(Finding::new(
                Level::Error,
                "store",
                env,
                "not set: run ws secrets run artifact_store_push -- ws artifacts push",
            ));
            None
        }
    };
    let tmp = ws.home().join("cache/tmp/store-push");
    for a in doc
        .get("artifact")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
    {
        let name = a.get("name").and_then(|v| v.as_str()).unwrap_or("");
        let label = format!("artifact {name}");
        let Some(path) = a
            .get("path")
            .and_then(|v| v.as_str())
            .and_then(|p| ws.resolve(p))
        else {
            continue;
        };
        if !path.exists() {
            out.push(Finding::new(
                Level::Warn,
                "store",
                label,
                format!("{}: not on this machine - skipped", path.display()),
            ));
            continue;
        }
        let (obj, sha, tar) = match object_for(&path, &tmp) {
            Ok(x) => x,
            Err(e) => {
                out.push(Finding::new(Level::Error, "store", label, e));
                continue;
            }
        };
        let size = std::fs::metadata(&obj).map(|m| m.len()).unwrap_or(0);
        let recorded = a.get("store_sha256").and_then(|v| v.as_str()) == Some(sha.as_str());
        let Some(store) = &store else {
            out.push(Finding::new(
                Level::Info,
                "store",
                label,
                format!("{sha:.12} {size} B{}", if tar { " (tar)" } else { "" }),
            ));
            continue;
        };
        let present = match store.exists(&sha) {
            Ok(p) => p,
            Err(e) => {
                out.push(Finding::new(Level::Error, "store", label, e));
                continue;
            }
        };
        if present && recorded {
            out.push(Finding::new(
                Level::Info,
                "store",
                label,
                format!("in the store: {sha:.12}"),
            ));
            continue;
        }
        if !apply {
            out.push(Finding::new(
                Level::Warn,
                "store",
                label,
                format!(
                    "plan: {}{sha:.12} {size} B{}",
                    if present { "record " } else { "upload " },
                    if tar { " (tar)" } else { "" }
                ),
            ));
            continue;
        }
        let res = if present {
            Ok(())
        } else {
            store.put(&sha, &obj)
        };
        match res.and_then(|_| record(&toml_path, name, &sha, tar)) {
            Ok(_) => out.push(Finding::new(
                Level::Done,
                "store",
                label,
                format!(
                    "{} {sha:.12} {size} B",
                    if present { "recorded" } else { "uploaded" }
                ),
            )),
            Err(e) => out.push(Finding::new(Level::Error, "store", label, e)),
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
    out
}

fn quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// curl from System32 (like bootstrap.ps1), not the first one found in PATH.
pub fn curl_exe() -> PathBuf {
    std::env::var_os("SystemRoot")
        .map(|r| Path::new(&r).join("System32").join("curl.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| "curl".into())
}

/// Object to upload: a file — itself; a folder — a tar of its contents in a temp folder. (path, sha256, is tar)
pub fn object_for(path: &Path, tmp: &Path) -> Result<(PathBuf, String, bool), String> {
    if path.is_file() {
        let sha = sha256_file(path).ok_or_else(|| format!("{}: not readable", path.display()))?;
        return Ok((path.to_path_buf(), sha, false));
    }
    if !path.is_dir() {
        return Err(format!("{}: missing", path.display()));
    }
    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "dir".into());
    let tar = tmp.join(format!("{name}.tar"));
    let _ = std::fs::remove_file(&tar);
    let st = Command::new("tar")
        .arg("-cf")
        .arg(&tar)
        .arg("-C")
        .arg(path)
        .arg(".")
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    if !st.success() {
        return Err(format!("tar {}: failed", path.display()));
    }
    // tar on Windows may skip a file with a warning and still exit with 0 (17.11: "Can't add archive to
    // itself" on icons/100058.png — a false match of the file number). An incomplete archive in the store is worse than none.
    let want = count_files(path);
    let listed = Command::new("tar")
        .arg("-tf")
        .arg(&tar)
        .output()
        .map_err(|e| format!("tar: {e}"))?;
    let got = String::from_utf8_lossy(&listed.stdout)
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with('/'))
        .count();
    if !listed.status.success() || got != want {
        let _ = std::fs::remove_file(&tar);
        return Err(format!(
            "tar {}: {got} files in the archive, {want} in the folder - not uploaded, run again",
            path.display()
        ));
    }
    let sha = sha256_file(&tar).ok_or("tar: not readable")?;
    Ok((tar, sha, true))
}

/// How many files are in the folder (recursively, without folders) — checked against the archive.
fn count_files(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .map(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        count_files(&p)
                    } else {
                        1
                    }
                })
                .sum()
        })
        .unwrap_or(0)
}

/// Put a downloaded object in place: a file — rename; a tar — unpack into the folder path.
pub fn install(part: &Path, path: &Path, is_tar: bool) -> Result<(), String> {
    if !is_tar {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        return std::fs::rename(part, path).map_err(|e| format!("{}: {e}", path.display()));
    }
    std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
    let st = Command::new("tar")
        .arg("-xf")
        .arg(part)
        .arg("-C")
        .arg(path)
        .status()
        .map_err(|e| format!("tar: {e}"))?;
    let _ = std::fs::remove_file(part);
    if st.success() {
        Ok(())
    } else {
        Err(format!("unpacking into {} failed", path.display()))
    }
}

/// Write `store_sha256` (and `store_tar = true` for a folder) into the block of artifact `name` in artifacts.toml as text
/// (comments stay). Ok(true) — the file was changed.
pub fn record(artifacts_toml: &Path, name: &str, sha: &str, tar: bool) -> Result<bool, String> {
    let text = std::fs::read_to_string(artifacts_toml).map_err(|e| e.to_string())?;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let want_name = format!("name = \"{name}\"");
    let start = lines
        .iter()
        .position(|l| l.trim() == want_name)
        .ok_or_else(|| format!("artifacts.toml: no artifact {name}"))?;
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.trim() == "[[artifact]]")
        .map_or(lines.len(), |i| start + 1 + i);
    let mut changed = false;
    let mut set = |lines: &mut Vec<String>, end: &mut usize, key: &str, value: &str| {
        let new = format!("{key} = {value}");
        let found = lines[start + 1..*end].iter().position(|l| {
            l.trim_start()
                .strip_prefix(key)
                .is_some_and(|r| r.trim_start().starts_with('='))
        });
        match found {
            Some(i) if lines[start + 1 + i].trim() == new => {}
            Some(i) => {
                lines[start + 1 + i] = new;
                changed = true;
            }
            None => {
                // after path, otherwise right after name
                let at = lines[start + 1..*end]
                    .iter()
                    .position(|l| l.trim_start().starts_with("path"))
                    .map_or(start + 1, |i| start + 2 + i);
                lines.insert(at, new);
                *end += 1;
                changed = true;
            }
        }
    };
    let mut end_mut = end;
    set(
        &mut lines,
        &mut end_mut,
        "store_sha256",
        &format!("\"{sha}\""),
    );
    if tar {
        set(&mut lines, &mut end_mut, "store_tar", "true");
    }
    if changed {
        let mut new = lines.join("\n");
        new.push('\n');
        std::fs::write(artifacts_toml, new).map_err(|e| e.to_string())?;
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_inserts_after_path_and_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("ws-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("artifacts.toml");
        std::fs::write(
            &f,
            "# head\n[[artifact]]\nname = \"a\"\npath = \"home:a\"\nsha256 = \"x\"\n\n[[artifact]]\nname = \"b dir\"\npath = \"ws:b\"\n",
        )
        .unwrap();
        assert!(record(&f, "b dir", "abc", true).unwrap());
        assert!(record(&f, "a", "def", false).unwrap());
        assert!(!record(&f, "a", "def", false).unwrap());
        let doc: toml::Value = std::fs::read_to_string(&f).unwrap().parse().unwrap();
        let arts = doc["artifact"].as_array().unwrap();
        assert_eq!(arts[0]["store_sha256"].as_str(), Some("def"));
        assert!(arts[0].get("store_tar").is_none());
        assert_eq!(arts[1]["store_sha256"].as_str(), Some("abc"));
        assert_eq!(arts[1]["store_tar"].as_bool(), Some(true));
        assert!(std::fs::read_to_string(&f).unwrap().starts_with("# head"));
    }

    #[test]
    fn tar_roundtrip() {
        let dir = std::env::temp_dir().join(format!("ws-store-tar-{}", std::process::id()));
        let src = dir.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("sub/x.txt"), "hello").unwrap();
        let (tar, sha, is_tar) = object_for(&src, &dir.join("tmp")).unwrap();
        assert!(is_tar && sha.len() == 64);
        let part = dir.join("obj.part");
        std::fs::copy(&tar, &part).unwrap();
        let dst = dir.join("dst");
        install(&part, &dst, true).unwrap();
        assert_eq!(
            std::fs::read_to_string(dst.join("sub/x.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn quote_escapes() {
        assert_eq!(quote(r#"a"b\c"#), r#"a\"b\\c"#);
    }
}
