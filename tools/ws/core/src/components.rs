//! Components (config/components.toml, schema dbcomponents/1): sets of members a person or an AI installs to work on
//! one part of the workspace — docs/COMPONENTS_PLAN.md. This module reads the inventory and checks it (ws check rule
//! `components`) and puts a chosen set on disk (`ws components`: git sparse checkout, cone mode; the choice in
//! local.toml [components] installed).
use crate::visibility::Tier;
use crate::{str_list, Finding, Level, Member, Workspace};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::Command;

pub const FILE: &str = "components.toml";
const AT: &str = "config/components.toml";

#[derive(Debug, Clone)]
pub struct Component {
    pub name: String,
    pub title: String,
    pub about: String,
    /// always installed (exactly one component)
    pub base: bool,
    pub members: Vec<String>,
    pub needs: Vec<String>,
    pub visibility: String,
}

/// The inventory of the workspace: None — there is no config/components.toml (an edition without it).
pub fn load(ws: &Workspace) -> Option<Result<Vec<Component>, String>> {
    let path = ws.config(FILE);
    if !path.is_file() {
        return None;
    }
    Some(crate::read_toml(&path).map(|doc| parse(&doc)))
}

pub fn parse(doc: &toml::Value) -> Vec<Component> {
    let s = |c: &toml::Value, k: &str| c.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    doc.get("component")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|c| Component {
                    name: s(c, "name"),
                    title: s(c, "title"),
                    about: s(c, "about"),
                    base: c.get("base").and_then(|v| v.as_bool()).unwrap_or(false),
                    members: str_list(c.get("members")),
                    needs: str_list(c.get("needs")),
                    visibility: s(c, "visibility"),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Names of the components `name` brings with it: itself, everything it needs (transitively) and the base.
pub fn closure(comps: &[Component], name: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut todo: Vec<String> = comps
        .iter()
        .filter(|c| c.base)
        .map(|c| c.name.clone())
        .collect();
    todo.push(name.to_string());
    while let Some(n) = todo.pop() {
        if out.insert(n.clone()) {
            if let Some(c) = comps.iter().find(|c| c.name == n) {
                todo.extend(c.needs.iter().cloned());
            }
        }
    }
    out
}

/// Presets (`[[preset]]`): named sets of components for DevKit and `ws components set <preset>`.
pub fn presets(ws: &Workspace) -> Vec<(String, String, Vec<String>)> {
    let Ok(doc) = crate::read_toml(&ws.config(FILE)) else {
        return Vec::new();
    };
    doc.get("preset")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(|p| {
            (
                p.get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                p.get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                str_list(p.get("components")),
            )
        })
        .collect()
}

/// ws check, rule `components`.
pub fn check(ws: &Workspace, out: &mut Vec<Finding>) {
    match load(ws) {
        None => {}
        Some(Err(e)) => out.push(Finding::new(Level::Error, "components", AT, e)),
        Some(Ok(comps)) => {
            out.extend(check_list(&ws.members, &comps));
            // installed components and everything they need must be on disk
            if let Some(set) = installed(ws) {
                for name in set.iter().filter(|n| !comps.iter().any(|c| &c.name == *n)) {
                    out.push(Finding::new(
                        Level::Warn,
                        "components",
                        "config/local.toml",
                        format!(
                            "installed lists '{name}', which config/components.toml does not have (another branch?) - ws components remove {name} --apply, or switch back to that branch"
                        ),
                    ));
                }
                let want: BTreeSet<String> = set.iter().flat_map(|c| closure(&comps, c)).collect();
                for c in comps.iter().filter(|c| want.contains(&c.name)) {
                    let missing: Vec<&str> = c
                        .members
                        .iter()
                        .filter(|m| !ws.root.join(m).is_dir())
                        .map(String::as_str)
                        .collect();
                    if !missing.is_empty() {
                        let why = if set.contains(&c.name) {
                            "installed".to_string()
                        } else {
                            let by: Vec<&str> = set
                                .iter()
                                .filter(|s| closure(&comps, s).contains(&c.name))
                                .map(String::as_str)
                                .collect();
                            format!("needed by {}", by.join(", "))
                        };
                        out.push(Finding::new(
                            Level::Error,
                            "components",
                            "config/local.toml",
                            format!(
                                "component {} ({why}) is not on disk: {} - ws components add {} --apply",
                                c.name,
                                missing.join(", "),
                                c.name
                            ),
                        ));
                    }
                }
            }
            let names: BTreeSet<&str> = comps.iter().map(|c| c.name.as_str()).collect();
            // `components = [...]` in tools.lock / artifacts.toml and preset lists name existing components
            for (file, key) in [("tools.lock", "tool"), ("artifacts.toml", "artifact")] {
                let Ok(doc) = crate::read_toml(&ws.config(file)) else {
                    continue;
                };
                for it in doc
                    .get(key)
                    .and_then(|a| a.as_array())
                    .into_iter()
                    .flatten()
                {
                    for c in str_list(it.get("components")) {
                        if !names.contains(c.as_str()) {
                            out.push(Finding::new(
                                Level::Error,
                                "components",
                                format!("config/{file}"),
                                format!(
                                    "{}: components = '{c}' - no such component",
                                    it.get("name").and_then(|v| v.as_str()).unwrap_or("?")
                                ),
                            ));
                        }
                    }
                }
            }
            for (p, _, list) in presets(ws) {
                for c in list
                    .iter()
                    .filter(|c| *c != "all" && !names.contains(c.as_str()))
                {
                    out.push(Finding::new(
                        Level::Error,
                        "components",
                        AT,
                        format!("preset {p}: '{c}' - no such component"),
                    ));
                }
            }
        }
    }
}

/// Installed components: local.toml [components] installed. None — all (a full checkout, or no inventory).
pub fn installed(ws: &Workspace) -> Option<BTreeSet<String>> {
    let list = str_list(ws.local.get("components").and_then(|c| c.get("installed")));
    if list.is_empty() || list.iter().any(|x| x == "all") || load(ws).is_none() {
        None
    } else {
        Some(list.into_iter().collect())
    }
}

/// Is a tools.lock / artifacts.toml entry needed with the installed components? `components = [...]` — when any of
/// them is installed; else an artifact at "ws:<path>" — when the member holding that path is installed; else always.
pub fn wanted(ws: &Workspace, item: &toml::Value) -> bool {
    let Some(set) = installed(ws) else {
        return true;
    };
    let list = str_list(item.get("components"));
    if !list.is_empty() {
        return list.iter().any(|c| set.contains(c));
    }
    let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
    match (path.strip_prefix("ws:"), load(ws)) {
        (Some(p), Some(Ok(comps))) => match ws.member_of(p) {
            Some(m) => comps
                .iter()
                .find(|c| c.members.contains(&m.path))
                .is_none_or(|c| set.contains(&c.name)),
            None => true,
        },
        _ => true,
    }
}

/// Is a member's folder expected on disk (its component installed, or a full checkout)?
pub fn member_expected(ws: &Workspace, member: &str) -> bool {
    let Some(set) = installed(ws) else {
        return true;
    };
    match load(ws) {
        Some(Ok(comps)) => comps
            .iter()
            .find(|c| c.members.iter().any(|m| m == member))
            .is_none_or(|c| set.contains(&c.name)),
        _ => true,
    }
}

/// Names (components, presets, "all") → the full set with everything needed. None — all.
pub fn expand(
    comps: &[Component],
    presets: &[(String, String, Vec<String>)],
    names: &[String],
) -> Result<Option<BTreeSet<String>>, String> {
    let mut out = BTreeSet::new();
    for n in names {
        if n == "all" {
            return Ok(None);
        }
        if let Some((_, _, list)) = presets.iter().find(|(p, _, _)| p == n) {
            if list.iter().any(|x| x == "all") {
                return Ok(None);
            }
            for c in list {
                out.extend(closure(comps, c));
            }
        } else if comps.iter().any(|c| &c.name == n) {
            out.extend(closure(comps, n));
        } else {
            return Err(format!(
                "'{n}' - no such component or preset (ws components list)"
            ));
        }
    }
    if out.is_empty() {
        out.extend(comps.iter().filter(|c| c.base).map(|c| c.name.clone()));
    }
    Ok(Some(out))
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let o = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("git: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).to_string())
    } else {
        Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// Folders for `git sparse-checkout set --cone`: top-level folders without members (docs, agents, config, .claude …;
/// root files are always there in cone mode) and the members of the chosen components.
pub fn sparse_paths(
    ws: &Workspace,
    comps: &[Component],
    set: &BTreeSet<String>,
) -> Result<Vec<String>, String> {
    let top = git(&ws.root, &["ls-tree", "-d", "--name-only", "HEAD"])?;
    let mut out: BTreeSet<String> = top
        .lines()
        .map(str::trim)
        .filter(|d| !d.is_empty())
        .filter(|d| {
            !ws.members
                .iter()
                .any(|m| m.path == *d || m.path.starts_with(&format!("{d}/")))
        })
        .map(String::from)
        .collect();
    for c in comps.iter().filter(|c| set.contains(&c.name)) {
        out.extend(c.members.iter().cloned());
    }
    Ok(out.into_iter().collect())
}

/// One component for a screen (DevKit) or `ws components list`.
#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub name: String,
    pub title: String,
    pub about: String,
    pub base: bool,
    pub members: Vec<String>,
    pub needs: Vec<String>,
    /// tracked files of its members (from git: known without the files on disk)
    pub files: usize,
    /// installed (local.toml [components]), or everything on disk in a full checkout
    pub installed: bool,
    /// external programs (tools.lock kind = "external") that its work needs: Visual Studio, Unreal Engine, Ghidra
    pub external: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Status {
    /// a partial checkout (git sparse checkout)
    pub sparse: bool,
    pub components: Vec<ComponentInfo>,
    /// (name, title, components) — `all` in components means everything
    pub presets: Vec<(String, String, Vec<String>)>,
}

/// What exists and what is installed: for `ws components list` and DevKit.
pub fn status(ws: &Workspace) -> Result<Status, String> {
    let comps = match load(ws) {
        None => return Err("no config/components.toml in this workspace".into()),
        Some(r) => r?,
    };
    let sparse = git(&ws.root, &["config", "--get", "core.sparseCheckout"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false);
    let current = current_set(ws, &comps, sparse);
    let files = git(&ws.root, &["ls-tree", "-r", "--name-only", "HEAD"]).unwrap_or_default();
    let tools = crate::read_toml(&ws.config("tools.lock")).ok();
    let external = |name: &str| -> Vec<String> {
        tools
            .as_ref()
            .and_then(|d| d.get("tool"))
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
            .filter(|t| t.get("kind").and_then(|v| v.as_str()) == Some("external"))
            .filter(|t| str_list(t.get("components")).iter().any(|c| c == name))
            .filter_map(|t| t.get("name").and_then(|v| v.as_str()).map(String::from))
            .collect()
    };
    Ok(Status {
        sparse,
        components: comps
            .iter()
            .map(|c| ComponentInfo {
                name: c.name.clone(),
                title: c.title.clone(),
                about: c.about.clone(),
                base: c.base,
                members: c.members.clone(),
                needs: c.needs.clone(),
                files: files
                    .lines()
                    .filter(|f| c.members.iter().any(|m| f.starts_with(&format!("{m}/"))))
                    .count(),
                installed: c.base || current.as_ref().is_none_or(|s| s.contains(&c.name)),
                external: external(&c.name),
            })
            .collect(),
        presets: presets(ws),
    })
}

/// Installed components: local.toml [components]; a sparse checkout without that record (a fresh partial clone) —
/// the components whose folders are all on disk; None — everything (a full checkout).
fn current_set(ws: &Workspace, comps: &[Component], sparse: bool) -> Option<BTreeSet<String>> {
    installed(ws).or_else(|| {
        sparse.then(|| {
            comps
                .iter()
                .filter(|c| c.base || c.members.iter().all(|m| ws.root.join(m).is_dir()))
                .map(|c| c.name.clone())
                .collect()
        })
    })
}

/// `ws components list | add <names> | remove <names> | set <names>` (without --apply — the plan only).
pub fn run(ws: &Workspace, cmd: &str, names: &[String], apply: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let comps = match load(ws) {
        None => {
            out.push(Finding::new(
                Level::Error,
                "components",
                AT,
                "no config/components.toml in this workspace",
            ));
            return out;
        }
        Some(Err(e)) => {
            out.push(Finding::new(Level::Error, "components", AT, e));
            return out;
        }
        Some(Ok(c)) => c,
    };
    let presets = presets(ws);
    let sparse = git(&ws.root, &["config", "--get", "core.sparseCheckout"])
        .map(|s| s.trim() == "true")
        .unwrap_or(false);
    let all: BTreeSet<String> = comps.iter().map(|c| c.name.clone()).collect();
    let now = current_set(ws, &comps, sparse);
    let current = now.clone().unwrap_or_else(|| all.clone());
    if cmd == "list" {
        let st = match status(ws) {
            Ok(s) => s,
            Err(e) => {
                out.push(Finding::new(Level::Error, "components", AT, e));
                return out;
            }
        };
        println!(
            "components (config/components.toml){}:",
            if st.sparse {
                ""
            } else {
                " - full checkout, everything is on disk"
            }
        );
        for c in &st.components {
            let on = if c.installed { "+" } else { " " };
            let needs = if c.needs.is_empty() {
                String::new()
            } else {
                format!("  needs: {}", c.needs.join(", "))
            };
            let ext = if c.external.is_empty() {
                String::new()
            } else {
                format!("  (programs: {})", c.external.join(", "))
            };
            println!(
                "  {on} {:<9} {:>5} files  {}{needs}{ext}",
                c.name, c.files, c.title
            );
            if !c.base || !c.members.is_empty() {
                println!("                          {}", c.members.join(", "));
            }
        }
        if !st.presets.is_empty() {
            println!("\npresets (ws components set <preset>):");
            for (p, t, l) in &st.presets {
                println!("    {p:<9} {t}: {}", l.join(", "));
            }
        }
        println!("\n+ on disk. ws components which <path>; ws components add|remove|set <names> [--apply]");
        return out;
    }
    if cmd == "which" {
        for p in names {
            let p = p.replace('\\', "/");
            let p = p.trim_start_matches("./").trim_end_matches('/');
            let found = comps.iter().find(|c| {
                c.members
                    .iter()
                    .any(|m| p == m || p.starts_with(&format!("{m}/")))
            });
            let msg = match found {
                Some(c) => {
                    let on = current.contains(&c.name) || c.base;
                    format!(
                        "component {}{}",
                        c.name,
                        if on {
                            " (on disk)".to_string()
                        } else {
                            format!(" (not on disk: ws components add {} --apply)", c.name)
                        }
                    )
                }
                None => "outside members: base (always on disk)".to_string(),
            };
            out.push(Finding::new(Level::Info, "components", p, msg));
        }
        return out;
    }
    if names.is_empty() {
        out.push(Finding::new(
            Level::Error,
            "components",
            cmd,
            "name the components (ws components list)",
        ));
        return out;
    }
    let target: Option<BTreeSet<String>> = match cmd {
        "set" => match expand(&comps, &presets, names) {
            Ok(t) => t,
            Err(e) => {
                out.push(Finding::new(Level::Error, "components", cmd, e));
                return out;
            }
        },
        "add" => match expand(&comps, &presets, names) {
            Ok(None) => None,
            Ok(Some(t)) => now.as_ref().map(|n| n.union(&t).cloned().collect()),
            Err(e) => {
                out.push(Finding::new(Level::Error, "components", cmd, e));
                return out;
            }
        },
        "remove" => {
            let mut t = current.clone();
            for n in names {
                match comps.iter().find(|c| &c.name == n) {
                    // a name left in local.toml by another branch: only drop it from the list
                    None if t.contains(n) => {
                        t.remove(n);
                    }
                    None => {
                        out.push(Finding::new(
                            Level::Error,
                            "components",
                            cmd,
                            format!("'{n}' - no such component"),
                        ));
                        return out;
                    }
                    Some(c) if c.base => {
                        out.push(Finding::new(
                            Level::Error,
                            "components",
                            cmd,
                            format!("{n} is the base - always installed"),
                        ));
                        return out;
                    }
                    Some(_) => {
                        t.remove(n);
                    }
                }
            }
            // what stays must not need what goes
            let keep: BTreeSet<String> = t.iter().flat_map(|c| closure(&comps, c)).collect();
            for n in names.iter().filter(|n| keep.contains(*n)) {
                let by: Vec<&str> = t
                    .iter()
                    .filter(|c| closure(&comps, c).contains(n))
                    .map(String::as_str)
                    .filter(|c| c != n)
                    .collect();
                out.push(Finding::new(
                    Level::Error,
                    "components",
                    cmd,
                    format!("{n} is needed by {} - remove them too", by.join(", ")),
                ));
            }
            if !out.is_empty() {
                return out;
            }
            Some(t)
        }
        _ => {
            out.push(Finding::new(
                Level::Error,
                "components",
                cmd,
                "unknown: list | add | remove | set",
            ));
            return out;
        }
    };
    // members that leave the disk: no uncommitted changes there
    let leaving: Vec<String> = match &target {
        None => Vec::new(),
        Some(t) => comps
            .iter()
            .filter(|c| current.contains(&c.name) && !t.contains(&c.name))
            .flat_map(|c| c.members.clone())
            .collect(),
    };
    if !leaving.is_empty() {
        let mut args = vec!["status", "--porcelain", "--"];
        args.extend(leaving.iter().map(String::as_str));
        match git(&ws.root, &args) {
            Ok(s) if !s.trim().is_empty() => {
                out.push(Finding::new(
                    Level::Error,
                    "components",
                    cmd,
                    format!(
                        "uncommitted changes in folders that would leave the disk - commit or stash first:\n{}",
                        s.trim_end()
                    ),
                ));
                return out;
            }
            Err(e) => {
                out.push(Finding::new(Level::Error, "components", cmd, e));
                return out;
            }
            _ => {}
        }
    }
    let describe = |t: &Option<BTreeSet<String>>| match t {
        None => "all (full checkout)".to_string(),
        Some(t) => t.iter().cloned().collect::<Vec<_>>().join(", "),
    };
    // the same components, but their folders changed (a new member after git pull): the sparse list is refreshed
    let folders_match = |t: &BTreeSet<String>| -> bool {
        let want: BTreeSet<String> = sparse_paths(ws, &comps, t)
            .unwrap_or_default()
            .into_iter()
            .collect();
        let have: BTreeSet<String> = git(&ws.root, &["sparse-checkout", "list"])
            .unwrap_or_default()
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        want == have
    };
    let same = match (&target, &now) {
        (None, None) => !sparse,
        (Some(t), Some(n)) => t == n && sparse && folders_match(t),
        _ => false,
    };
    if same {
        out.push(Finding::new(
            Level::Info,
            "components",
            "installed",
            describe(&target),
        ));
        return out;
    }
    if !apply {
        out.push(Finding::new(
            Level::Warn,
            "components",
            "plan",
            format!(
                "{} -> {}; leaving the disk: {}. Run with --apply",
                describe(&now),
                describe(&target),
                if leaving.is_empty() {
                    "nothing".into()
                } else {
                    leaving.join(", ")
                }
            ),
        ));
        return out;
    }
    let local = ws.config("local.toml");
    if !local.is_file() {
        out.push(Finding::new(
            Level::Error,
            "components",
            "config/local.toml",
            "no local.toml - copy config/local.example.toml first",
        ));
        return out;
    }
    let res = match &target {
        None => git(&ws.root, &["sparse-checkout", "disable"]).map(|_| ()),
        Some(t) => sparse_paths(ws, &comps, t).and_then(|paths| {
            let mut args = vec!["sparse-checkout", "set", "--cone"];
            args.extend(paths.iter().map(String::as_str));
            git(&ws.root, &args).map(|_| ())
        }),
    };
    if let Err(e) = res {
        out.push(Finding::new(Level::Error, "components", cmd, e));
        return out;
    }
    let list = match &target {
        None => "\"all\"".to_string(),
        Some(t) => t
            .iter()
            .map(|c| format!("\"{c}\""))
            .collect::<Vec<_>>()
            .join(", "),
    };
    let line = format!(
        "installed = [{list}]   # ws components; ws bootstrap installs the programs and data of these"
    );
    match crate::bootstrap::set_local_line(&local, "components", "installed", &line) {
        Ok(_) => out.push(Finding::new(
            Level::Done,
            "components",
            "installed",
            format!(
                "{}. Programs and data for them: ws bootstrap --apply",
                describe(&target)
            ),
        )),
        Err(e) => out.push(Finding::new(
            Level::Error,
            "components",
            "config/local.toml",
            e,
        )),
    }
    out
}

pub fn check_list(members: &[Member], comps: &[Component]) -> Vec<Finding> {
    let mut out = Vec::new();
    let mut err = |msg: String| out.push(Finding::new(Level::Error, "components", AT, msg));
    let names: BTreeSet<&str> = comps.iter().map(|c| c.name.as_str()).collect();
    if names.len() != comps.len() {
        err("component names repeat".into());
    }
    if comps.iter().filter(|c| c.base).count() != 1 {
        err("exactly one component must have base = true".into());
    }
    // which component holds each member
    let mut owner: BTreeMap<&str, &str> = BTreeMap::new();
    for c in comps {
        if c.name.is_empty() {
            err("a component without name".into());
        }
        for m in &c.members {
            if !members.iter().any(|x| &x.path == m) {
                err(format!(
                    "{}: '{m}' is not a member (workspace.toml)",
                    c.name
                ));
            } else if let Some(prev) = owner.insert(m, &c.name) {
                err(format!(
                    "member '{m}' is in two components: {prev} and {}",
                    c.name
                ));
            }
        }
        for n in &c.needs {
            if !names.contains(n.as_str()) {
                err(format!("{}: needs '{n}' - no such component", c.name));
            }
        }
    }
    for m in members {
        if !owner.contains_key(m.path.as_str()) {
            err(format!("member '{}' is in no component", m.path));
        }
    }
    // cycles in needs
    for c in comps {
        if reaches(comps, &c.name, &c.name) {
            err(format!("{}: needs leads back to itself", c.name));
        }
    }
    for c in comps {
        let cl = closure(comps, &c.name);
        // a member's depends_on must be installed together with it
        for m in &c.members {
            let Some(mm) = members.iter().find(|x| &x.path == m) else {
                continue;
            };
            for d in &mm.depends_on {
                match owner.get(d.as_str()) {
                    Some(dc) if !cl.contains(*dc) => err(format!(
                        "{m} depends_on {d} (component {dc}), but component {} does not need {dc}",
                        c.name
                    )),
                    _ => {}
                }
            }
        }
        // visibility = the strictest member: export filters cut the entry together with its members
        let strictest = c
            .members
            .iter()
            .filter_map(|m| members.iter().find(|x| &x.path == m))
            .filter_map(|m| Tier::parse(&m.visibility))
            .max();
        match (Tier::parse(&c.visibility), strictest) {
            (None, _) => err(format!(
                "{}: visibility = \"{}\" - must be public, team or owner",
                c.name, c.visibility
            )),
            (Some(v), Some(s)) if v != s => err(format!(
                "{}: visibility = \"{}\", the strictest member is {}",
                c.name,
                v.name(),
                s.name()
            )),
            _ => {}
        }
    }
    out
}

fn reaches(comps: &[Component], from: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut todo: Vec<String> = comps
        .iter()
        .find(|c| c.name == from)
        .map(|c| c.needs.clone())
        .unwrap_or_default();
    while let Some(n) = todo.pop() {
        if n == target {
            return true;
        }
        if seen.insert(n.clone()) {
            if let Some(c) = comps.iter().find(|c| c.name == n) {
                todo.extend(c.needs.iter().cloned());
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, vis: &str, deps: &[&str]) -> Member {
        Member {
            path: p.into(),
            name: p.into(),
            kind: "x".into(),
            depends_on: deps.iter().map(|s| s.to_string()).collect(),
            known_absolute: vec![],
            visibility: vis.into(),
            vis_team: vec![],
            vis_owner: vec![],
        }
    }

    fn comps(text: &str) -> Vec<Component> {
        parse(&text.parse::<toml::Value>().unwrap())
    }

    const OK: &str = r#"
[[component]]
name = "base"
base = true
members = ["tools/ws"]
visibility = "public"
[[component]]
name = "server"
members = ["core/backend", "content"]
visibility = "team"
[[component]]
name = "site"
members = ["web"]
needs = ["server"]
visibility = "team"
"#;

    fn members() -> Vec<Member> {
        vec![
            m("tools/ws", "public", &[]),
            m("core/backend", "public", &[]),
            m("content", "team", &["core/backend"]),
            m("web", "team", &["core/backend", "tools/ws"]),
        ]
    }

    #[test]
    fn valid_inventory() {
        let f = check_list(&members(), &comps(OK));
        assert!(f.is_empty(), "{f:?}");
        let cl = closure(&comps(OK), "site");
        assert_eq!(
            cl.into_iter().collect::<Vec<_>>(),
            vec!["base", "server", "site"]
        );
    }

    #[test]
    fn findings() {
        let mut ms = members();
        ms.push(m("extra", "team", &[]));
        ms[1].depends_on = vec!["web".into()]; // server -> site: not in the closure
        let text = OK
            .replace("needs = [\"server\"]", "needs = [\"server\", \"nope\"]")
            .replace(
                "visibility = \"team\"\n[[component]]\nname = \"site\"",
                "visibility = \"public\"\n[[component]]\nname = \"site\"",
            );
        let f = check_list(&ms, &comps(&text));
        let msgs: Vec<String> = f.iter().map(|x| x.msg.clone()).collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("'extra' is in no component")),
            "{msgs:?}"
        );
        assert!(msgs.iter().any(|m| m.contains("needs 'nope'")), "{msgs:?}");
        assert!(
            msgs.iter().any(|m| m.contains("does not need site")),
            "{msgs:?}"
        );
        assert!(
            msgs.iter().any(|m| m.contains("strictest member is team")),
            "{msgs:?}"
        );
    }

    #[test]
    fn expand_presets_and_needs() {
        let c = comps(OK);
        let presets = vec![
            ("web".to_string(), String::new(), vec!["site".to_string()]),
            ("full".to_string(), String::new(), vec!["all".to_string()]),
        ];
        let got = expand(&c, &presets, &["web".into()]).unwrap().unwrap();
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec!["base", "server", "site"]
        );
        assert!(expand(&c, &presets, &["full".into()]).unwrap().is_none());
        assert!(expand(&c, &presets, &["nope".into()]).is_err());
        let base = expand(&c, &presets, &["base".into()]).unwrap().unwrap();
        assert_eq!(base.into_iter().collect::<Vec<_>>(), vec!["base"]);
    }

    #[test]
    fn cycle() {
        let text = format!("{OK}\n[[component]]\nname = \"a\"\nmembers = []\nneeds = [\"b\"]\nvisibility = \"team\"\n[[component]]\nname = \"b\"\nmembers = []\nneeds = [\"a\"]\nvisibility = \"team\"\n");
        let f = check_list(&members(), &comps(&text));
        assert!(f.iter().any(|x| x.msg.contains("leads back")), "{f:?}");
    }
}
