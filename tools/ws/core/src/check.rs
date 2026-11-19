//! ws check — rules of workspace.toml [workspace.rules] (docs/WORKSPACE_PLAN.md, section 2):
//!   layout     only entries from [workspace.layout] root in the root; members in place, each with a member.toml
//!   members    git-tracked files live in members or in shared folders ([workspace.layout] shared)
//!   depends    Cargo.toml path dependencies into another member — only if it is in depends_on
//!   paths      drive letters in code and configs — only in local.toml and known-absolute-paths files
//!   binaries   exe/dll/pak/archives not in git
//!   secrets    keys, .env, password databases not in git
//!   encoding   .ps1 with Cyrillic — UTF-8 with BOM (otherwise PowerShell 5.1 breaks parsing)
//!   visibility each member has [member] visibility = public | team | owner; public does not depend on closed ones
//!   components config/components.toml: every member in one component, needs exist, no cycles (docs/COMPONENTS_PLAN.md)
//!   core       [workspace.core] paths changed on a branch (or uncommitted on main) - a note, docs/EXTENDING.md
//!   sandbox    member code does not refer to the sandbox/ folder (experiments, not in git)
//!   leaks      public-level files have no public IPs, owner markers ([workspace.rules] owner-markers) or game
//!              data ([workspace.rules] game-files) — warnings while there is no export (plan, section 13.4)
use crate::visibility::{glob, Classifier, LeakScan, Tier};
use crate::{rel, str_list, Finding, Level, Workspace};
use regex::Regex;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command;

const CODE_EXT: [&str; 14] = [
    "rs", "py", "ps1", "psm1", "sh", "bat", "cmd", "toml", "json", "js", "mjs", "lua", "yaml",
    "yml",
];
const BINARY_EXT: [&str; 10] = [
    "exe", "dll", "pak", "ucas", "utoc", "zip", "rar", "7z", "pdb", "msi",
];
const SECRET_EXT: [&str; 6] = ["pem", "key", "pfx", "p12", "kdbx", "env"];

pub fn run(ws: &Workspace) -> Vec<Finding> {
    let mut out = Vec::new();
    layout(ws, &mut out);
    let all = match tracked(&ws.root) {
        Ok(f) => f,
        Err(e) => {
            out.push(Finding::new(Level::Error, "git", "", e));
            return out;
        }
    };
    // a partial checkout (ws components): files of components not on disk are in the index, but not checked
    let files: Vec<String> = all
        .iter()
        .filter(|x| ws.root.join(x).exists())
        .cloned()
        .collect();
    let all: BTreeSet<String> = all.into_iter().collect();
    let mut off_disk_links = 0usize;
    let shared = ws.layout("shared");
    let root_entries: BTreeSet<String> = ws.layout("root").into_iter().collect();
    // machine data paths: from [workspace.rules] machine-paths (regex); system C:\Windows, Program Files and paths
    // on Robot (C:\DungeonbornServer) are not the developer machine, the rule leaves them alone
    let mut pats = str_list(
        ws.manifest
            .get("workspace")
            .and_then(|w| w.get("rules"))
            .and_then(|r| r.get("machine-paths")),
    );
    if pats.is_empty() {
        pats = vec![
            r"(^|[^A-Za-z0-9_\\])[D-G]:[\\/]".into(),
            r"(^|[^A-Za-z0-9_\\])[Cc]:[\\/][Uu]sers[\\/]".into(),
        ];
    }
    // machine-paths-code: only code lines (not comments, not documents and not the export): typical game places on C:
    // are fine as an example in text, but in code they belong in local.toml
    pats.extend(str_list(
        ws.manifest
            .get("workspace")
            .and_then(|w| w.get("rules"))
            .and_then(|r| r.get("machine-paths-code")),
    ));
    let machine: Vec<Regex> = pats.iter().filter_map(|p| Regex::new(p).ok()).collect();
    // the root folder as a path: at the start, after a quote, space, ( = : , or ../ — not `ui-sandbox/` or `$sandbox\`;
    // the folder name is assembled: this file is member code too
    let sandbox_ref =
        Regex::new(&format!(r#"(^|["'\s(=:,]|\.\.[\\/]){}box[\\/]"#, "sand")).unwrap();
    core_changes(ws, &mut out);
    visibility(ws, &mut out);
    crate::components::check(ws, &mut out);
    crate::preview::check(ws, &mut out);
    let rules = ws.manifest.get("workspace").and_then(|w| w.get("rules"));
    let tiers = Classifier::new(ws);
    let leaks = LeakScan::new(
        &str_list(rules.and_then(|r| r.get("owner-markers"))),
        &str_list(rules.and_then(|r| r.get("public-ips-ok"))),
    );
    // leaks looks at the text after the export replacements (export/public.toml): you see what goes out
    let scrub = crate::export::Scrub::load(ws).unwrap_or_else(|e| {
        out.push(Finding::new(Level::Error, "leaks", crate::export::SPEC, e));
        crate::export::Scrub::none()
    });
    let game_files: Vec<Regex> = str_list(rules.and_then(|r| r.get("game-files")))
        .iter()
        .map(|p| glob(p))
        .collect();
    for f in &files {
        let name = f.rsplit('/').next().unwrap_or(f);
        let ext = name
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        let member = ws.member_of(f);
        // members: a file outside members — only a root entry from layout or in a shared folder
        if member.is_none()
            && !root_entries.contains(f.as_str())
            && !shared.iter().any(|s| f.starts_with(&format!("{s}/")))
        {
            out.push(Finding::new(
                Level::Error,
                "members",
                f,
                "file outside members (workspace.toml members) and shared folders",
            ));
        }
        if BINARY_EXT.contains(&ext.as_str()) {
            out.push(Finding::new(
                Level::Error,
                "binaries",
                f,
                "binary or archive in git - belongs in home (out, tools, artifacts)",
            ));
        }
        let secret_name = SECRET_EXT.contains(&ext.as_str()) && !name.ends_with(".env.example")
            || name == ".env"
            || name.starts_with("aes_key");
        if secret_name {
            out.push(Finding::new(
                Level::Error,
                "secrets",
                f,
                "secret file in git - belongs in the vault",
            ));
        }
        let path = ws.root.join(f);
        let public = tiers.tier(ws, f) == Tier::Public;
        if public && game_files.iter().any(|r| r.is_match(f)) {
            out.push(Finding::new(
                Level::Warn,
                "leaks",
                f,
                "game data in a public file - mark it team (member.toml [visibility])",
            ));
        }
        if public
            && (CODE_EXT.contains(&ext.as_str())
                || matches!(ext.as_str(), "md" | "txt" | "html" | "css"))
        {
            if let Some((line, what)) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|t| leaks.first(&scrub.apply(&t)))
            {
                out.push(Finding::new(
                    Level::Warn,
                    "leaks",
                    format!("{f}:{line}"),
                    format!("{what} in a public file - move it to local.toml / a team file, or mark the file team"),
                ));
            }
        }
        // Windows PowerShell 5.1 reads a .ps1 without BOM as ANSI: Cyrillic turns into "“" and breaks parsing
        if ext == "ps1" || ext == "psm1" {
            if let Ok(b) = std::fs::read(&path) {
                if !b.starts_with(&[0xEF, 0xBB, 0xBF]) && b.iter().any(|&c| c > 127) {
                    out.push(Finding::new(Level::Error, "encoding", f, "non-ASCII .ps1 without BOM: PowerShell 5.1 reads it as ANSI - save as UTF-8 with BOM"));
                }
            }
        }
        // secrets inside text files
        if CODE_EXT.contains(&ext.as_str()) || ext == "md" || ext == "txt" {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains("-----BEGIN")
                    && text.contains("PRIVATE KEY-----")
                    && !f.ends_with(".rs")
                {
                    out.push(Finding::new(
                        Level::Error,
                        "secrets",
                        f,
                        "file contains a private key (BEGIN ... PRIVATE KEY)",
                    ));
                }
                if CODE_EXT.contains(&ext.as_str()) && !path_exempt(ws, f, &shared) {
                    for (i, line) in text.lines().enumerate() {
                        let t = line.trim_start_matches('\u{feff}').trim_start();
                        if is_comment(t, &ext) {
                            continue;
                        }
                        // a path in the trailing comment of the line does not count
                        let code = match comment_start(line, &ext) {
                            Some(c) => &line[..c],
                            None => line,
                        };
                        if member.is_some() && sandbox_ref.is_match(code) {
                            out.push(Finding::new(
                                Level::Warn,
                                "sandbox",
                                format!("{f}:{}", i + 1),
                                "member code refers to the sandbox folder - it is not in git and nothing may depend on it; move what is needed into a member",
                            ));
                        }
                        if machine.iter().any(|r| r.is_match(code)) {
                            let shown: String = line.trim().chars().take(110).collect();
                            out.push(Finding::new(
                                Level::Error,
                                "paths",
                                format!("{f}:{}", i + 1),
                                format!("machine path: {shown}"),
                            ));
                        }
                    }
                }
            }
        }
        if ext == "md" {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for (line, target) in broken_links(&ws.root, f, &text) {
                    // the target is in git, only not on disk (a component that is not installed)
                    if in_git(&all, f, &target) {
                        off_disk_links += 1;
                        continue;
                    }
                    out.push(Finding::new(
                        Level::Warn,
                        "links",
                        format!("{f}:{line}"),
                        format!("link to a missing file: {target}"),
                    ));
                }
            }
        }
        if name == "Cargo.toml" {
            cargo_deps(ws, f, &mut out);
        }
    }
    if off_disk_links > 0 {
        out.push(Finding::new(
            Level::Note,
            "links",
            "",
            format!(
                "{off_disk_links} link(s) lead into components that are not on disk - not checked (ws components)"
            ),
        ));
    }
    out
}

/// Link `target` of markdown file `f` points to a tracked file or folder (in git, maybe not on disk).
fn in_git(all: &BTreeSet<String>, f: &str, target: &str) -> bool {
    let t = target.split('#').next().unwrap_or("");
    let dir = Path::new(f).parent().unwrap_or(Path::new(""));
    let p = crate::normalize(&dir.join(t))
        .to_string_lossy()
        .replace('\\', "/");
    let p = p.trim_end_matches('/');
    all.contains(p)
        || all
            .range(format!("{p}/")..)
            .next()
            .is_some_and(|x| x.starts_with(&format!("{p}/")))
}

/// Relative links `](target)` of markdown file `f` whose target (without `#anchor`) does not exist: (line, target).
/// Addresses with a scheme, anchors, mail and `<…>` are not checked; links inside ``` blocks are not either.
fn broken_links(root: &Path, f: &str, text: &str) -> Vec<(usize, String)> {
    let re = Regex::new(r"\]\(([^)\s]+)\)").unwrap();
    let dir = Path::new(f).parent().unwrap_or(Path::new(""));
    let mut out = Vec::new();
    let mut fence = false;
    for (i, line) in text.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        for c in re.captures_iter(line) {
            let t = &c[1];
            if t.contains("://")
                || t.starts_with('#')
                || t.starts_with("mailto:")
                || t.starts_with('<')
            {
                continue;
            }
            let file = t.split('#').next().unwrap_or(t);
            if file.is_empty() {
                continue;
            }
            let p = if let Some(abs) = file.strip_prefix('/') {
                root.join(abs)
            } else {
                root.join(dir).join(file)
            };
            if !p.exists() {
                out.push((i + 1, t.to_string()));
            }
        }
    }
    out
}

/// Member levels: marked or not, valid value, a public one does not depend on closed ones.
fn visibility(ws: &Workspace, out: &mut Vec<Finding>) {
    let tier = |p: &str| {
        ws.members
            .iter()
            .find(|m| m.path == p)
            .and_then(|m| Tier::parse(&m.visibility))
    };
    for m in &ws.members {
        let at = format!("{}/member.toml", m.path);
        match Tier::parse(&m.visibility) {
            None if m.visibility.is_empty() => out.push(Finding::new(
                Level::Warn,
                "visibility",
                at,
                "no [member] visibility (public | team | owner) - counted as the workspace default",
            )),
            None => out.push(Finding::new(
                Level::Error,
                "visibility",
                at,
                format!(
                    "visibility = \"{}\" - must be public, team or owner",
                    m.visibility
                ),
            )),
            Some(Tier::Public) => {
                for d in &m.depends_on {
                    if tier(d).is_some_and(|t| t != Tier::Public) {
                        out.push(Finding::new(
                            Level::Warn,
                            "visibility",
                            at.clone(),
                            format!("public member depends on non-public {d}"),
                        ));
                    }
                }
            }
            Some(_) => {}
        }
    }
}

/// Rule core: files of [workspace.core] paths changed (modified, deleted, renamed — not added) since the branch left
/// main; on main itself — uncommitted changes only. A note: the core may change when a task needs it, the pull request
/// says why (docs/EXTENDING.md).
fn core_changes(ws: &Workspace, out: &mut Vec<Finding>) {
    let core = str_list(
        ws.manifest
            .get("workspace")
            .and_then(|w| w.get("core"))
            .and_then(|c| c.get("paths")),
    );
    if core.is_empty() {
        return;
    }
    let git = |args: &[&str]| -> Option<String> {
        Command::new("git")
            .args(["-c", "safe.directory=*", "-c", "core.quotepath=false"])
            .args(args)
            .current_dir(&ws.root)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let base = if branch == "main" || branch == "HEAD" {
        "HEAD".to_string()
    } else {
        ["origin/main", "main"]
            .iter()
            .find_map(|b| git(&["merge-base", "HEAD", b]))
            .unwrap_or_else(|| "HEAD".into())
    };
    let Some(diff) = git(&["diff", "--name-only", "--diff-filter=MDR", &base]) else {
        return;
    };
    let hit: Vec<&str> = diff
        .lines()
        .filter(|f| {
            core.iter()
                .any(|c| *f == c.as_str() || f.starts_with(&format!("{c}/")))
        })
        .collect();
    if hit.is_empty() {
        return;
    }
    let since = if base == "HEAD" {
        "uncommitted".to_string()
    } else {
        format!("since main ({})", &base[..base.len().min(7)])
    };
    let shown: Vec<&str> = hit.iter().take(8).copied().collect();
    out.push(Finding::new(
        Level::Note,
        "core",
        "",
        format!(
            "{} core file(s) changed, {since}: {}{} - fine when the task needs it: keep it small, say why in the pull request (docs/EXTENDING.md)",
            hit.len(),
            shown.join(", "),
            if hit.len() > shown.len() { ", ..." } else { "" }
        ),
    ));
}

/// The paths rule does not apply to: local*.toml, shared folders (docs, agents, .claude), the member's known-absolute-paths files.
fn path_exempt(ws: &Workspace, f: &str, shared: &[String]) -> bool {
    if f == "config/local.toml"
        || f == "config/local.example.toml"
        || shared.iter().any(|s| f.starts_with(&format!("{s}/")))
    {
        return true;
    }
    match ws.member_of(f) {
        Some(m) => {
            let inner = f.strip_prefix(&format!("{}/", m.path)).unwrap_or(f);
            m.known_absolute
                .iter()
                .any(|k| inner == k || inner.ends_with(&format!("/{k}")) || f == k)
        }
        None => false,
    }
}

/// Start of a trailing comment: " #" for toml/py/ps1/sh/yaml, " //" for rs/js, " --" for lua (rough: strings are not considered).
fn comment_start(line: &str, ext: &str) -> Option<usize> {
    let marker = match ext {
        "rs" | "js" | "mjs" => " //",
        "lua" => " --",
        "toml" | "py" | "ps1" | "psm1" | "sh" | "yaml" | "yml" => " #",
        _ => return None,
    };
    line.find(marker)
}

fn is_comment(t: &str, ext: &str) -> bool {
    match ext {
        "rs" | "js" | "mjs" => t.starts_with("//") || t.starts_with("/*") || t.starts_with('*'),
        "lua" => t.starts_with("--"),
        "bat" | "cmd" => t.to_ascii_lowercase().starts_with("rem ") || t.starts_with("::"),
        "json" => false,
        _ => t.starts_with('#'),
    }
}

fn layout(ws: &Workspace, out: &mut Vec<Finding>) {
    let allowed: BTreeSet<String> = ws
        .layout("root")
        .into_iter()
        .chain(ws.layout("groups"))
        .chain(ws.layout("shared"))
        .collect();
    if allowed.is_empty() {
        out.push(Finding::new(
            Level::Warn,
            "layout",
            "workspace.toml",
            "no [workspace.layout] root/groups/shared - root not checked",
        ));
    }
    if let Ok(rd) = std::fs::read_dir(&ws.root) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n == ".git" || allowed.is_empty() {
                continue;
            }
            let top_member = ws
                .members
                .iter()
                .any(|m| m.path == n || m.path.starts_with(&format!("{n}/")));
            if !allowed.contains(&n) && !top_member {
                out.push(Finding::new(
                    Level::Error,
                    "layout",
                    n,
                    "unexpected entry in root (allowed: workspace.toml [workspace.layout])",
                ));
            }
        }
    }
    for m in &ws.members {
        let dir = ws.root.join(&m.path);
        // a partial checkout: members of components that are not installed are not on disk (ws components)
        // with local.toml [components] the components rule reports missing folders (and how to fix it)
        if !dir.is_dir()
            && (!crate::components::member_expected(ws, &m.path)
                || crate::components::installed(ws).is_some())
        {
            continue;
        }
        if !dir.is_dir() {
            out.push(Finding::new(
                Level::Error,
                "layout",
                &m.path,
                "member listed in workspace.toml but folder is missing",
            ));
        } else if !dir.join("member.toml").is_file() {
            out.push(Finding::new(
                Level::Error,
                "layout",
                &m.path,
                "member has no member.toml",
            ));
        } else if m.name.is_empty() || m.kind.is_empty() {
            out.push(Finding::new(
                Level::Error,
                "layout",
                format!("{}/member.toml", m.path),
                "no [member] name or kind",
            ));
        }
        for d in &m.depends_on {
            if !ws.members.iter().any(|x| &x.path == d) {
                out.push(Finding::new(
                    Level::Error,
                    "depends",
                    format!("{}/member.toml", m.path),
                    format!("depends_on: '{d}' is not a member"),
                ));
            }
        }
    }
}

/// Cargo.toml path dependencies: a path leading into another member — it must be in depends_on.
fn cargo_deps(ws: &Workspace, f: &str, out: &mut Vec<Finding>) {
    let Some(me) = ws.member_of(f) else { return };
    let Ok(doc) = crate::read_toml(&ws.root.join(f)) else {
        return;
    };
    let dir = Path::new(f).parent().unwrap_or(Path::new(""));
    let mut tables: Vec<&toml::Value> = Vec::new();
    for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
        tables.extend(doc.get(key));
        tables.extend(doc.get("workspace").and_then(|w| w.get(key)));
    }
    if let Some(t) = doc.get("target").and_then(|t| t.as_table()) {
        for v in t.values() {
            for key in ["dependencies", "dev-dependencies", "build-dependencies"] {
                tables.extend(v.get(key));
            }
        }
    }
    // `x.workspace = true` takes the path from [workspace.dependencies] of the root Cargo.toml (plan section 13.1)
    let root_doc = crate::read_toml(&ws.root.join("Cargo.toml")).ok();
    let root_deps = root_doc
        .as_ref()
        .and_then(|d| d.get("workspace"))
        .and_then(|w| w.get("dependencies"));
    let mut paths = dep_paths(&tables, root_deps);
    paths.extend(
        str_list(doc.get("workspace").and_then(|w| w.get("members")))
            .into_iter()
            .map(|p| (false, p)),
    );
    for (from_root, p) in paths {
        let base = if from_root { Path::new("") } else { dir };
        let target = rel(&ws.root, &crate::normalize(&ws.root.join(base).join(&p)));
        if target.starts_with("..") {
            out.push(Finding::new(
                Level::Error,
                "depends",
                f,
                format!("path = \"{p}\" points outside the workspace"),
            ));
            continue;
        }
        match ws.member_of(&target) {
            Some(other) if other.path != me.path && !me.depends_on.contains(&other.path) => out
                .push(Finding::new(
                    Level::Error,
                    "depends",
                    f,
                    format!(
                        "path = \"{p}\" is member {}, not in depends_on of {}",
                        other.path, me.path
                    ),
                )),
            None => out.push(Finding::new(
                Level::Error,
                "depends",
                f,
                format!("path = \"{p}\" does not point into a member"),
            )),
            _ => {}
        }
    }
}

/// Paths of path dependencies from dependency tables: (true, path from the root) for `x.workspace = true`,
/// (false, path from the Cargo.toml folder) for `x = { path = … }`.
fn dep_paths(tables: &[&toml::Value], root_deps: Option<&toml::Value>) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    for t in tables.iter().filter_map(|t| t.as_table()) {
        for (name, v) in t {
            if let Some(p) = v.get("path").and_then(|p| p.as_str()) {
                out.push((false, p.to_string()));
            } else if v.get("workspace").and_then(|w| w.as_bool()) == Some(true) {
                if let Some(p) = root_deps
                    .and_then(|r| r.get(name))
                    .and_then(|d| d.get("path"))
                    .and_then(|p| p.as_str())
                {
                    out.push((true, p.to_string()));
                }
            }
        }
    }
    out
}

/// Git-tracked files (from the root, with "/").
pub fn tracked(root: &Path) -> Result<Vec<String>, String> {
    let o = Command::new("git")
        .args([
            "-c",
            "safe.directory=*",
            "-c",
            "core.quotepath=false",
            "ls-files",
            "-z",
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("git ls-files: {e}"))?;
    if !o.status.success() {
        return Err(format!(
            "git ls-files: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&o.stdout)
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_machine() -> Vec<Regex> {
        [
            r"(^|[^A-Za-z0-9_\\])[D-G]:[\\/]",
            r"(^|[^A-Za-z0-9_\\])[Cc]:[\\/][Uu]sers[\\/]",
        ]
        .iter()
        .map(|p| Regex::new(p).unwrap())
        .collect()
    }

    #[test]
    fn sandbox_refs() {
        let re = Regex::new(&format!(r#"(^|["'\s(=:,]|\.\.[\\/]){}box[\\/]"#, "sand")).unwrap();
        let s = |x: &str| x.replace("SB", "sandbox");
        assert!(re.is_match(&s(r#"open("SB/x.py")"#)));
        assert!(re.is_match(&s(r"path = ..\SB\tool.ps1")));
        assert!(re.is_match(&s("SB/notes.md")));
        assert!(!re.is_match(&s("dist/ui-SB/kit.zip")), "another folder");
        assert!(!re.is_match(&s(r#""$SB\$role""#)), "a PowerShell variable");
    }

    #[test]
    fn links_checked() {
        let root = std::env::temp_dir().join(format!("ws-links-{}", std::process::id()));
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/a.md"), "").unwrap();
        let text = "[ok](a.md#x) [web](https://x.org/y) [anchor](#h) [gone](b.md)\n```\n[code](c.md)\n```\n[up](../docs/a.md)\n";
        let got = broken_links(&root, "docs/index.md", text);
        std::fs::remove_dir_all(&root).ok();
        assert_eq!(got, vec![(1, "b.md".to_string())]);
    }

    #[test]
    fn machine_paths_only_data_drives() {
        let m = default_machine();
        let hit = |s: &str| m.iter().any(|r| r.is_match(s));
        // drive letters are assembled from parts: otherwise the export leak check itself sees a machine path here
        assert!(hit(&format!(r#"x = r"{}:\Games\Game""#, 'F')));
        assert!(hit(&format!("root = '{}:/data'", 'G')));
        assert!(hit(&format!(r"C:\{}\someone\AppData", "Users")));
        assert!(!hit(r"C:\Windows\System32"), "system path");
        assert!(!hit(r"C:\GameServer\Kit"), "battle server path");
        assert!(
            !hit(r"if ($line -notmatch '^(\d\d:\d\d:\d\d)')"),
            "regex, not a path"
        );
        assert!(!hit("let x = \"X:/nope\";"), "test drives");
    }

    #[test]
    fn trailing_comment_is_not_code() {
        let line = format!(r"no-secrets = true   # secrets in {}:\DBDev\vault", 'F');
        let c = comment_start(&line, "toml").unwrap();
        assert!(!default_machine().iter().any(|r| r.is_match(&line[..c])));
        assert!(
            is_comment(&format!("-- {}:/x", 'G'), "lua")
                && is_comment("# x", "ps1")
                && is_comment("// x", "rs")
        );
    }

    #[test]
    fn workspace_inherited_path_deps() {
        let member: toml::Value = toml::from_str(
            "[dependencies]\nws-core.workspace = true\nserde.workspace = true\nlocal = { path = \"../x\" }\nhttp = \"1\"",
        )
        .unwrap();
        let root: toml::Value = toml::from_str(
            "[workspace.dependencies]\nws-core = { path = \"tools/ws/core\" }\nserde = \"1\"",
        )
        .unwrap();
        let deps = member.get("dependencies").unwrap();
        let mut got = dep_paths(
            &[deps],
            root.get("workspace").and_then(|w| w.get("dependencies")),
        );
        got.sort();
        assert_eq!(
            got,
            vec![
                (false, "../x".to_string()),
                (true, "tools/ws/core".to_string())
            ]
        );
    }
}
