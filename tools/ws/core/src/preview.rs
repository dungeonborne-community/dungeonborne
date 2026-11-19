//! ws preview — how a human sees a member's result (agents/ROLES.md, "See it"; docs/PREVIEW.md).
//!
//! A member that shows something (a site, an app's UI, a page) has `[preview]` in member.toml:
//!
//! ```toml
//! [preview]
//! name = "site"                                  # launch.json name (default: the member folder name)
//! run  = ["python", "web/dev_server.py", "8821"] # a server that keeps running and builds what it needs itself
//! dir  = "."                                     # where run starts, from the member folder (default ".")
//! port = 8821
//! path = "/"                                     # the page to open (default "/")
//! note = "the site as on the hosting, from the files"   # what it shows and what it does not
//! env  = { NO_COLOR = "true" }                   # optional environment (trunk refuses NO_COLOR=1)
//! ```
//!
//! `ws preview` lists them; `ws preview <member|name> [--open]` runs one and prints the address (opens the browser);
//! `ws preview --sync [--apply]` writes them into .claude/launch.json for the Claude Code preview (entries with other
//! names are kept). `ws check` (rule preview) wants `[preview]` on site and app members and launch.json in step.
use crate::{Finding, Level, Workspace};
use serde_json::{json, Value};
use std::path::PathBuf;

/// Members of these kinds show something to a person — they need a `[preview]`.
pub const KINDS: [&str; 2] = ["site", "app"];
const LAUNCH: &str = ".claude/launch.json";

#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    /// member path from the root
    pub member: String,
    pub name: String,
    pub run: Vec<String>,
    /// where run starts, from the root, with "/" ("." — the root)
    pub dir: String,
    pub port: u16,
    pub path: String,
    pub note: String,
    /// extra environment of run
    pub env: Vec<(String, String)>,
}

impl Preview {
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}{}", self.port, self.path)
    }

    /// The .claude/launch.json configuration (Claude Code preview): paths relative to the root.
    pub fn launch(&self) -> Value {
        let (exe, args) = self
            .run
            .split_first()
            .map(|(e, a)| (e.clone(), a.to_vec()))
            .unwrap_or_default();
        let mut v = json!({
            "name": self.name,
            "runtimeExecutable": exe,
            "runtimeArgs": args,
            "port": self.port,
            "autoPort": false,
        });
        if self.dir != "." {
            v["cwd"] = json!(self.dir);
        }
        if !self.env.is_empty() {
            v["env"] = Value::Object(
                self.env
                    .iter()
                    .map(|(k, x)| (k.clone(), json!(x)))
                    .collect(),
            );
        }
        v
    }
}

/// `[preview]` of one member.toml. Err — the table is there but wrong.
pub fn parse(member: &str, doc: &toml::Value) -> Result<Option<Preview>, String> {
    let Some(p) = doc.get("preview") else {
        return Ok(None);
    };
    let s = |k: &str| p.get(k).and_then(|v| v.as_str()).map(String::from);
    let run = crate::str_list(p.get("run"));
    if run.is_empty() {
        return Err("[preview] run = [\"program\", \"arg\", ...] is required".into());
    }
    let port = p
        .get("port")
        .and_then(|v| v.as_integer())
        .filter(|n| (1..=65535).contains(n))
        .ok_or("[preview] port = <1-65535> is required")? as u16;
    let rel = s("dir").unwrap_or_else(|| ".".into());
    if rel.contains("..") || rel.starts_with('/') || rel.contains(':') {
        return Err(format!(
            "[preview] dir = '{rel}': a folder inside the member"
        ));
    }
    let dir = match rel.trim_matches('/') {
        "" | "." => member.to_string(),
        r => format!("{member}/{r}"),
    };
    let mut path = s("path").unwrap_or_else(|| "/".into());
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    Ok(Some(Preview {
        member: member.to_string(),
        name: s("name").unwrap_or_else(|| member.rsplit('/').next().unwrap_or(member).to_string()),
        run,
        dir: if dir.is_empty() { ".".into() } else { dir },
        port,
        path,
        note: s("note").unwrap_or_default(),
        env: p
            .get("env")
            .and_then(|e| e.as_table())
            .map(|t| {
                t.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
                    .collect()
            })
            .unwrap_or_default(),
    }))
}

/// The previews of the members on disk, and the problems found on the way.
pub fn list(ws: &Workspace) -> (Vec<Preview>, Vec<Finding>) {
    let mut out = Vec::new();
    let mut problems = Vec::new();
    for m in &ws.members {
        let file = ws.root.join(&m.path).join("member.toml");
        if !file.is_file() {
            continue; // not on disk (ws components)
        }
        let Ok(doc) = crate::read_toml(&file) else {
            continue;
        };
        match parse(&m.path, &doc) {
            Ok(Some(p)) => out.push(p),
            Ok(None) if KINDS.contains(&m.kind.as_str()) => problems.push(Finding::new(
                Level::Warn,
                "preview",
                format!("{}/member.toml", m.path),
                format!(
                    "a {} without [preview]: how does a person see it? (docs/PREVIEW.md)",
                    m.kind
                ),
            )),
            Ok(None) => {}
            Err(e) => problems.push(Finding::new(
                Level::Error,
                "preview",
                format!("{}/member.toml", m.path),
                e,
            )),
        }
    }
    let mut seen = std::collections::BTreeMap::new();
    for p in &out {
        for (what, key) in [("name", p.name.clone()), ("port", p.port.to_string())] {
            if let Some(other) = seen.insert(format!("{what}:{key}"), p.member.clone()) {
                problems.push(Finding::new(
                    Level::Error,
                    "preview",
                    format!("{}/member.toml", p.member),
                    format!("[preview] {what} {key} is also used by {other}"),
                ));
            }
        }
    }
    (out, problems)
}

/// A preview by member path, member folder name or preview name.
pub fn find(ws: &Workspace, key: &str) -> Option<Preview> {
    let key = key.trim_end_matches('/').replace('\\', "/");
    list(ws).0.into_iter().find(|p| {
        p.member == key || p.name == key || p.member.rsplit('/').next() == Some(key.as_str())
    })
}

fn launch_path(ws: &Workspace) -> PathBuf {
    ws.root.join(LAUNCH)
}

/// launch.json with the previews put in: an entry with a preview's name is replaced in place, a missing one is added;
/// the others stay. Returns the new document and what changed.
pub fn synced(ws: &Workspace, previews: &[Preview]) -> Result<(Value, Vec<String>), String> {
    let path = launch_path(ws);
    let mut doc: Value = match std::fs::read_to_string(&path) {
        Ok(t) => serde_json::from_str(&t).map_err(|e| format!("{LAUNCH}: {e}"))?,
        Err(_) => json!({"version": "0.0.1", "configurations": []}),
    };
    let confs = doc
        .get_mut("configurations")
        .and_then(|c| c.as_array_mut())
        .ok_or_else(|| format!("{LAUNCH}: no configurations array"))?;
    let mut changes = Vec::new();
    for p in previews {
        let want = p.launch();
        match confs.iter_mut().find(|c| c["name"] == want["name"]) {
            Some(c) if *c == want => {}
            Some(c) => {
                *c = want;
                changes.push(format!("{}: updated from {}/member.toml", p.name, p.member));
            }
            None => {
                confs.push(want);
                changes.push(format!("{}: added from {}/member.toml", p.name, p.member));
            }
        }
    }
    Ok((doc, changes))
}

/// `ws preview --sync`: plan, or write with apply.
pub fn sync(ws: &Workspace, apply: bool) -> Vec<Finding> {
    let (previews, mut out) = list(ws);
    match synced(ws, &previews) {
        Err(e) => out.push(Finding::new(Level::Error, "preview", LAUNCH, e)),
        Ok((_, changes)) if changes.is_empty() => out.push(Finding::new(
            Level::Info,
            "preview",
            LAUNCH,
            "in step with [preview] of the members",
        )),
        Ok((doc, changes)) => {
            let text = serde_json::to_string_pretty(&doc).unwrap_or_default() + "\n";
            let written = apply && std::fs::write(launch_path(ws), text).is_ok();
            for c in changes {
                out.push(Finding::new(
                    if written { Level::Done } else { Level::Warn },
                    "preview",
                    LAUNCH,
                    c,
                ));
            }
            if !written {
                out.push(Finding::new(
                    Level::Note,
                    "preview",
                    LAUNCH,
                    "write it: cargo ws preview --sync --apply",
                ));
            }
        }
    }
    out
}

/// ws check, rule preview: `[preview]` where a person must see something, and launch.json in step.
pub fn check(ws: &Workspace, out: &mut Vec<Finding>) {
    let (previews, problems) = list(ws);
    out.extend(problems);
    if let Ok((_, changes)) = synced(ws, &previews) {
        if !changes.is_empty() {
            out.push(Finding::new(
                Level::Warn,
                "preview",
                LAUNCH,
                format!(
                    "not in step with [preview] ({}): cargo ws preview --sync --apply",
                    changes.join("; ")
                ),
            ));
        }
    }
}

/// Runs a preview in the foreground (until Ctrl+C); with `open` the browser opens once the port answers.
pub fn run(ws: &Workspace, p: &Preview, open: bool) -> Result<i32, String> {
    let (exe, args) = p.run.split_first().ok_or("empty run")?;
    let dir = if p.dir == "." {
        ws.root.clone()
    } else {
        ws.root.join(&p.dir)
    };
    if open {
        let (port, url) = (p.port, p.url());
        std::thread::spawn(move || {
            let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
            for _ in 0..600 {
                if std::net::TcpStream::connect_timeout(
                    &addr,
                    std::time::Duration::from_millis(200),
                )
                .is_ok()
                {
                    let _ = std::process::Command::new("cmd")
                        .args(["/c", "start", "", &url])
                        .status();
                    return;
                }
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
        });
    }
    let status = std::process::Command::new(exe)
        .args(args)
        .envs(p.env.iter().map(|(k, v)| (k, v)))
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("{exe} (in {}): {e}", dir.display()))?;
    Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(t: &str) -> toml::Value {
        toml::from_str(t).unwrap()
    }

    #[test]
    fn parses() {
        let p = parse("web/landing", &doc("[preview]\nname = \"site\"\nrun = [\"python\", \"web/dev_server.py\", \"8821\"]\nport = 8821"))
            .unwrap()
            .unwrap();
        assert_eq!(
            (p.name.as_str(), p.dir.as_str(), p.url().as_str()),
            ("site", "web/landing", "http://127.0.0.1:8821/")
        );
        assert_eq!(
            p.launch(),
            json!({"name": "site", "runtimeExecutable": "python", "runtimeArgs": ["web/dev_server.py", "8821"],
                   "port": 8821, "autoPort": false, "cwd": "web/landing"})
        );
        let d = parse(
            "apps/devkit",
            &doc("[preview]\nrun = [\"trunk\", \"serve\"]\ndir = \"ui\"\nport = 1440\npath = \"index.html\"\nenv = { NO_COLOR = \"true\" }"),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            (d.name.as_str(), d.dir.as_str(), d.path.as_str()),
            ("devkit", "apps/devkit/ui", "/index.html")
        );
        assert_eq!(d.launch()["env"], json!({"NO_COLOR": "true"}));
        assert!(parse("x", &doc("[member]\nname = \"x\""))
            .unwrap()
            .is_none());
        assert!(parse("x", &doc("[preview]\nport = 1")).is_err());
        assert!(parse("x", &doc("[preview]\nrun = [\"a\"]")).is_err());
        assert!(parse(
            "x",
            &doc("[preview]\nrun = [\"a\"]\nport = 1\ndir = \"../y\"")
        )
        .is_err());
    }

    #[test]
    fn repo_previews() {
        let ws = Workspace::find(std::path::Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let (previews, problems) = list(&ws);
        let errors: Vec<_> = problems
            .iter()
            .filter(|f| matches!(f.level, Level::Error))
            .map(|f| &f.msg)
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        assert!(previews.iter().any(|p| p.member == "web/landing"));
    }
}
