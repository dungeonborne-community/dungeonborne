//! ws bootstrap / ws doctor — machine setup (docs/WORKSPACE_PLAN.md, sections 3–4, 9):
//!   local.toml   missing — a copy of local.example.toml (fill in by hand)
//!   home         DB_HOME folders
//!   tools        tools.lock: system — version check only; copy — from "legacy:"/"home:"; download — curl + sha256;
//!                cargo — cargo install --locked crate@version into home (tools_named — for cargo xtask setup)
//!   artifacts    artifacts.toml: heavy data outside git (paks, launcher bundle, icons) — a copy from the source + sha256
//!   agent env    .claude/settings.local.json "env": DB_HOME, toolchains from home (if already there), local.toml [env]
//! doctor = the same without changes: a report only.
use crate::{copy_tree, sha256_file, str_list, Finding, Level, Workspace};
use std::path::{Path, PathBuf};
use std::process::Command;

/// DB_HOME folders (plan section 3).
pub const HOME_DIRS: [&str; 21] = [
    "toolchains",
    "tools",
    "game/original",
    "dumps",
    "re",
    // exe analysis in Ghidra (research/ghidra/README.md): process dump — via the owner's link, the cut image, projects
    "re/ghidra/dumps",
    "re/ghidra/images",
    "re/ghidra/projects",
    "runs/stand",
    "runs/locks",
    "runs/crashes",
    "worktrees",
    "cache/target",
    "cache/sccache",
    "cache/ue-ddc",
    "cache/tmp",
    "cache/uv",
    "unreal",
    "out",
    "artifacts",
    "artifacts/paks",
];

pub struct Options {
    /// false — doctor: change nothing
    pub apply: bool,
    /// profile from DevKit: owner | dev | designer | agent
    pub profile: String,
    /// how to install programs with kind = "cargo"
    pub build: Build,
    /// `manual = true` artifacts to download this time (ws bootstrap --artifact=<name>); other manual ones are
    /// only an "on request" note
    pub artifacts: Vec<String>,
}

/// A ready exe or a build from source — for programs with kind = "cargo" in tools.lock (and for our builds in xtask).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    /// a ready file from the official releases page; the archive and the exe are checked against sha256 from tools.lock
    Prebuilt,
    /// cargo install --locked: slower, but everything is built from source on this machine
    Source,
}

impl Build {
    pub fn parse(s: &str) -> Option<Build> {
        match s {
            "prebuilt" => Some(Build::Prebuilt),
            "source" => Some(Build::Source),
            _ => None,
        }
    }

    /// Explicit choice, else local.toml [setup] build = "prebuilt" | "source", else prebuilt.
    pub fn choose(ws: &Workspace, explicit: Option<Build>) -> Build {
        explicit
            .or_else(|| {
                ws.local_str("setup", "build")
                    .and_then(|s| Build::parse(&s))
            })
            .unwrap_or(Build::Prebuilt)
    }

    pub fn name(self) -> &'static str {
        match self {
            Build::Prebuilt => "prebuilt",
            Build::Source => "source",
        }
    }
}

/// Machine profiles (as in DevKit).
pub const PROFILES: [&str; 4] = ["owner", "dev", "designer", "agent"];

/// Profile: explicit (flag --profile, choice in DevKit), else local.toml [setup] profile (written by ws bootstrap),
/// else owner.
pub fn choose_profile(ws: &Workspace, explicit: Option<String>) -> String {
    explicit
        .filter(|p| !p.is_empty())
        .or_else(|| ws.local_str("setup", "profile"))
        .unwrap_or_else(|| "owner".into())
}

/// Write `profile = "<p>"` into the [setup] section of local.toml as text (the file's comments stay).
/// Ok(true) — the file was changed.
pub fn remember_profile(local: &Path, profile: &str) -> Result<bool, String> {
    let text = std::fs::read_to_string(local).map_err(|e| format!("{}: {e}", local.display()))?;
    let line = format!(
        "profile = \"{profile}\"   # ws bootstrap --profile; doctor and DevKit read it from here"
    );
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = lines.iter().position(|l| l.trim() == "[setup]");
    match header {
        Some(h) => {
            let end = lines[h + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map_or(lines.len(), |i| h + 1 + i);
            let key = lines[h + 1..end].iter().position(|l| {
                let t = l.trim_start();
                t.strip_prefix("profile")
                    .is_some_and(|r| r.trim_start().starts_with('='))
            });
            match key {
                Some(k) => {
                    let k = h + 1 + k;
                    if lines[k]
                        .split('#')
                        .next()
                        .unwrap_or("")
                        .contains(&format!("\"{profile}\""))
                    {
                        return Ok(false);
                    }
                    lines[k] = line;
                }
                None => lines.insert(h + 1, line),
            }
        }
        None => {
            lines.push(String::new());
            lines.push("[setup]".into());
            lines.push(line);
        }
    }
    let mut new = lines.join("\n");
    new.push('\n');
    std::fs::write(local, new).map_err(|e| format!("{}: {e}", local.display()))?;
    Ok(true)
}

pub fn run(ws: &Workspace, opt: &Options) -> Vec<Finding> {
    let mut out = Vec::new();
    if !PROFILES.contains(&opt.profile.as_str()) {
        out.push(Finding::new(
            Level::Error,
            "local",
            "profile",
            format!(
                "unknown profile {} - one of {}",
                opt.profile,
                PROFILES.join(", ")
            ),
        ));
        return out;
    }
    local_toml(ws, opt, &mut out);
    // local.toml may have just appeared — continue with the re-read settings
    let reopened = Workspace::open(&ws.root).ok();
    let ws = reopened.as_ref().unwrap_or(ws);
    profile(ws, opt, &mut out);
    me(ws, &mut out);
    secrets_to_local(ws, opt, &mut out);
    local_consistency(ws, &mut out);
    home_dirs(ws, opt, &mut out);
    match crate::read_toml(&ws.config("tools.lock")) {
        Ok(doc) => tools(ws, opt, &doc, &mut out),
        Err(e) => out.push(Finding::new(Level::Error, "tools", "tools.lock", e)),
    }
    // artifacts.toml is optional: the public edition does not have it (the owner's private artifacts)
    let art = ws.config("artifacts.toml");
    if !art.is_file() {
        out.push(Finding::new(
            Level::Info,
            "artifacts",
            "artifacts.toml",
            "none in this workspace - nothing to fetch",
        ));
    } else {
        match crate::read_toml(&art) {
            Ok(doc) => artifacts(ws, opt, &doc, &mut out),
            Err(e) => out.push(Finding::new(Level::Error, "artifacts", "artifacts.toml", e)),
        }
    }
    games(ws, &mut out);
    game_reference(ws, opt, &mut out);
    agent_env(ws, opt, &mut out);
    root_note(ws, opt, &mut out);
    out
}

/// local.toml: [home] path matches the effective DB_HOME; no placeholders from the example are left (uXXXXXX etc.).
fn local_consistency(ws: &Workspace, out: &mut Vec<Finding>) {
    if let (Some(env), Some(cfg)) = (
        std::env::var_os("DB_HOME").filter(|h| !h.is_empty()),
        ws.local_str("home", "path"),
    ) {
        let (a, b) = (
            crate::native(std::path::Path::new(&env)),
            crate::native(std::path::Path::new(&cfg)),
        );
        if !a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy())
        {
            out.push(Finding::new(
                Level::Warn,
                "local",
                "local.toml [home] path",
                format!(
                    "{} differs from DB_HOME={} (the variable wins) - fix local.toml",
                    b.display(),
                    a.display()
                ),
            ));
        }
    }
    if let Some(t) = ws.local.as_table() {
        for (sec, v) in t {
            for (k, val) in v.as_table().into_iter().flatten() {
                if val.as_str().is_some_and(|s| s.contains("XXXXXX")) {
                    out.push(Finding::new(
                        Level::Warn,
                        "local",
                        format!("local.toml [{sec}] {k}"),
                        "placeholder from local.example.toml - set the real value",
                    ));
                }
            }
        }
    }
}

/// Secrets that programs need as a setting (secrets.toml `local = "section.key"`, e.g. the pak AES key for
/// retoc): with `ws secrets run <name> -- ws bootstrap --apply` the value from the environment variable is written to local.toml
/// (not into git, not into exports). Without the variable — only a message whether the value is there. 17.11, owner's decision.
fn secrets_to_local(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let Ok(doc) = crate::read_toml(&ws.config("secrets.toml")) else {
        return;
    };
    let local = ws.config("local.toml");
    for sec in doc
        .get("secret")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
    {
        let Some((section, key)) = s(sec, "local").split_once('.') else {
            continue;
        };
        let label = format!("local.toml [{section}] {key}");
        let have = ws.local_str(section, key).is_some_and(|v| !v.is_empty());
        let value = std::env::var(s(sec, "env")).ok().filter(|v| !v.is_empty());
        match value {
            Some(v) if opt.apply && local.is_file() => {
                if v.contains('\'') || v.contains('\n') {
                    out.push(Finding::new(
                        Level::Error,
                        "secrets",
                        label,
                        "value has a quote or a line break - not written",
                    ));
                    continue;
                }
                let line = format!(
                    "{key} = '{v}'   # from secret {} (ws bootstrap), not in git",
                    s(sec, "name")
                );
                match set_local_line(&local, section, key, &line) {
                    Ok(true) => out.push(Finding::new(
                        Level::Done,
                        "secrets",
                        label,
                        format!("written from secret {}", s(sec, "name")),
                    )),
                    Ok(false) => {
                        out.push(Finding::new(Level::Info, "secrets", label, "up to date"))
                    }
                    Err(e) => out.push(Finding::new(Level::Error, "secrets", label, e)),
                }
            }
            _ if have => out.push(Finding::new(Level::Info, "secrets", label, "set")),
            _ => out.push(Finding::new(
                Level::Note,
                "secrets",
                label,
                format!(
                    "not set; for {}: ws secrets run {} -- ws bootstrap --apply",
                    s(sec, "description"),
                    s(sec, "name")
                ),
            )),
        }
    }
    // 18.11: with a vault ws dump takes the pak key from the vault session; 19.11: without one (the public edition) this
    // line is the way to give it (docs/SECRETS.md, "The pak key")
    if ws.local_str("re", "aes_key").is_some_and(|v| !v.is_empty()) {
        out.push(Finding::new(
            Level::Note,
            "secrets",
            "local.toml [re] aes_key",
            "the pak key, in plain text (local.toml is not in git): fine without a vault; with one, keep it there as game_aes_key and remove this line",
        ));
    }
}

/// Line `key = …` in section [section] of local.toml: replace (if it differs) or add; no section — add it
/// at the end. true — the file was changed.
pub fn set_local_line(local: &Path, section: &str, key: &str, line: &str) -> Result<bool, String> {
    let text = std::fs::read_to_string(local).map_err(|e| format!("{}: {e}", local.display()))?;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    let header = lines
        .iter()
        .position(|l| l.trim() == format!("[{section}]"));
    match header {
        Some(h) => {
            let end = lines[h + 1..]
                .iter()
                .position(|l| l.trim_start().starts_with('['))
                .map_or(lines.len(), |i| h + 1 + i);
            let found = lines[h + 1..end].iter().position(|l| {
                l.trim_start()
                    .strip_prefix(key)
                    .is_some_and(|r| r.trim_start().starts_with('='))
            });
            match found {
                Some(k) if lines[h + 1 + k] == line => return Ok(false),
                Some(k) => lines[h + 1 + k] = line.to_string(),
                None => lines.insert(h + 1, line.to_string()),
            }
        }
        None => {
            lines.push(String::new());
            lines.push(format!("[{section}]"));
            lines.push(line.to_string());
        }
    }
    let mut new = lines.join("\n");
    new.push('\n');
    std::fs::write(local, new).map_err(|e| format!("{}: {e}", local.display()))?;
    Ok(true)
}

/// Profile of this machine: bootstrap remembers it in local.toml, doctor says which one it checks with.
/// local.toml [me]: who works at this machine — the AI adapts how it talks, asks and reports (CLAUDE.md).
fn me(ws: &Workspace, out: &mut Vec<Finding>) {
    let get = |k: &str| ws.local_str("me", k).unwrap_or_default();
    let (level, lang, detail) = (get("level"), get("lang"), get("detail"));
    if level.is_empty() && lang.is_empty() && detail.is_empty() {
        out.push(Finding::new(
            Level::Note,
            "me",
            "local.toml [me]",
            "not set - the AI assumes a developer (level dev); set level, lang, detail (config/local.example.toml)",
        ));
        return;
    }
    let bad: Vec<String> = [
        (
            "level",
            &level,
            &["newcomer", "designer", "dev", "owner"][..],
        ),
        ("lang", &lang, &["ru", "en"][..]),
        ("detail", &detail, &["short", "normal", "full"][..]),
    ]
    .iter()
    .filter(|(_, v, ok)| !v.is_empty() && !ok.contains(&v.as_str()))
    .map(|(k, v, ok)| format!("{k} = \"{v}\" (one of: {})", ok.join(", ")))
    .collect();
    if bad.is_empty() {
        let or = |v: &str, d: &str| {
            if v.is_empty() {
                d.to_string()
            } else {
                v.to_string()
            }
        };
        out.push(Finding::new(
            Level::Info,
            "me",
            "local.toml [me]",
            format!(
                "level {}, lang {}, detail {} - the AI talks and reports to this (CLAUDE.md)",
                or(&level, "dev"),
                or(&lang, "the human's"),
                or(&detail, "normal")
            ),
        ));
    } else {
        out.push(Finding::new(
            Level::Warn,
            "me",
            "local.toml [me]",
            bad.join("; "),
        ));
    }
}

fn profile(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let stored = ws.local_str("setup", "profile");
    let local = ws.config("local.toml");
    if opt.apply && local.is_file() && stored.as_deref() != Some(opt.profile.as_str()) {
        match remember_profile(&local, &opt.profile) {
            Ok(_) => out.push(Finding::new(
                Level::Done,
                "local",
                "local.toml [setup] profile",
                format!("{} - ws doctor and DevKit use it from now on", opt.profile),
            )),
            Err(e) => out.push(Finding::new(
                Level::Error,
                "local",
                "local.toml [setup] profile",
                e,
            )),
        }
        return;
    }
    let msg = match stored.as_deref() {
        Some(p) if p == opt.profile => format!("{p} (local.toml [setup] profile)"),
        Some(p) => format!(
            "{} for this run; this machine is {p} (local.toml [setup] profile)",
            opt.profile
        ),
        None => format!(
            "{} (not chosen yet: ws bootstrap --profile=<{}> remembers it)",
            opt.profile,
            PROFILES.join("|")
        ),
    };
    out.push(Finding::new(Level::Info, "local", "profile", msg));
}

fn local_toml(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let local = ws.config("local.toml");
    if local.is_file() {
        return;
    }
    if opt.apply {
        match std::fs::copy(ws.config("local.example.toml"), &local) {
            Ok(_) => out.push(Finding::new(
                Level::Done,
                "local",
                "local.toml",
                "created from local.example.toml - check the paths of this machine",
            )),
            Err(e) => out.push(Finding::new(
                Level::Error,
                "local",
                "local.toml",
                format!("not created: {e}"),
            )),
        }
    } else {
        out.push(Finding::new(
            Level::Error,
            "local",
            "local.toml",
            "missing - ws bootstrap creates it from local.example.toml",
        ));
    }
}

fn home_dirs(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let home = ws.home();
    let missing: Vec<&str> = HOME_DIRS
        .iter()
        .copied()
        .filter(|d| !home.join(d).is_dir())
        .collect();
    if missing.is_empty() {
        out.push(Finding::new(
            Level::Info,
            "home",
            home.display().to_string(),
            format!("all {} folders present", HOME_DIRS.len()),
        ));
        return;
    }
    if opt.apply {
        for d in &missing {
            if let Err(e) = std::fs::create_dir_all(home.join(d)) {
                out.push(Finding::new(
                    Level::Error,
                    "home",
                    home.join(d).display().to_string(),
                    e.to_string(),
                ));
                return;
            }
        }
        out.push(Finding::new(
            Level::Done,
            "home",
            home.display().to_string(),
            format!("folders created: {}", missing.join(", ")),
        ));
    } else {
        out.push(Finding::new(
            Level::Warn,
            "home",
            home.display().to_string(),
            format!("missing folders: {}", missing.join(", ")),
        ));
    }
}

fn for_profile(item: &toml::Value, profile: &str) -> bool {
    let p = str_list(item.get("profiles"));
    p.is_empty() || p.iter().any(|x| x == profile)
}

fn s<'a>(v: &'a toml::Value, k: &str) -> &'a str {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("")
}

fn tools(ws: &Workspace, opt: &Options, doc: &toml::Value, out: &mut Vec<Finding>) {
    for t in doc
        .get("tool")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter(|t| for_profile(t, &opt.profile))
        // components: only for the installed ones (local.toml [components], docs/COMPONENTS_PLAN.md)
        .filter(|t| crate::components::wanted(ws, t))
        // manual = true — installed only by cargo xtask setup when needed (LLVM on a machine without Visual Studio)
        .filter(|t| !t.get("manual").and_then(|v| v.as_bool()).unwrap_or(false))
    {
        tool(ws, t, opt.apply, opt.build, out);
    }
}

/// tools.lock entries with these names (regardless of profile): check, with `apply` — install. For cargo xtask setup.
pub fn tools_named(ws: &Workspace, names: &[&str], apply: bool, build: Build) -> Vec<Finding> {
    let mut out = Vec::new();
    let doc = match crate::read_toml(&ws.config("tools.lock")) {
        Ok(d) => d,
        Err(e) => return vec![Finding::new(Level::Error, "tools", "tools.lock", e)],
    };
    let list: Vec<&toml::Value> = doc
        .get("tool")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .collect();
    for name in names {
        match list.iter().find(|t| s(t, "name") == *name) {
            Some(t) => tool(ws, t, apply, build, &mut out),
            None => out.push(Finding::new(
                Level::Error,
                "tools",
                format!("tool {name}"),
                "not in tools.lock",
            )),
        }
    }
    out
}

/// Main file of a program from tools.lock (kind copy / download / cargo), if it is in place.
pub fn tool_exe(ws: &Workspace, name: &str) -> Option<PathBuf> {
    let doc = crate::read_toml(&ws.config("tools.lock")).ok()?;
    let t = doc
        .get("tool")?
        .as_array()?
        .iter()
        .find(|t| s(t, "name") == name)?
        .clone();
    let exe = crate::native(&ws.resolve(s(&t, "to"))?.join(s(&t, "exe")));
    exe.is_file().then_some(exe)
}

/// Where an external program (kind = "external") is, in order: a path from local.toml (`local = "section.key"`),
/// the file where the kit installs it (`path`, "home:…"), a folder from an environment variable (`env`), non-empty output
/// of a search command (`detect`, %VAR% in the first word is expanded).
fn external_found(ws: &Workspace, t: &toml::Value) -> Option<String> {
    if let Some((section, key)) = s(t, "local").split_once('.') {
        if let Some(p) = ws.local_str(section, key).and_then(|v| ws.resolve(&v)) {
            if p.exists() {
                return Some(p.display().to_string());
            }
        }
    }
    if !s(t, "path").is_empty() {
        if let Some(p) = ws.resolve(s(t, "path")).filter(|p| p.exists()) {
            return Some(p.display().to_string());
        }
    }
    let var = s(t, "env");
    if !var.is_empty() {
        if let Some(p) = std::env::var_os(var)
            .map(PathBuf::from)
            .filter(|p| p.exists())
        {
            return Some(format!("{} ({var})", p.display()));
        }
    }
    let cmd = str_list(t.get("detect"));
    let (exe, args) = cmd.split_first()?;
    let exe = expand_env(exe);
    let out = Command::new(&exe).args(args).output().ok()?;
    let first = String::from_utf8_lossy(&out.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    (out.status.success() && !first.is_empty()).then_some(first)
}

/// `%NAME%` → value of the environment variable (no variable — as is).
fn expand_env(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) => {
                let name = &after[..j];
                match std::env::var(name) {
                    Ok(v) if !name.is_empty() => out.push_str(&v),
                    _ => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[j + 1..];
            }
            None => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// python.exe of the portable Python (tools.lock kind uv-python), if it is installed.
pub fn portable_python(ws: &Workspace) -> Option<PathBuf> {
    let doc = crate::read_toml(&ws.config("tools.lock")).ok()?;
    let t = doc
        .get("tool")?
        .as_array()?
        .iter()
        .find(|t| s(t, "kind") == "uv-python")?;
    let exe = uv_python_dir(ws, t)?.join("python.exe");
    exe.is_file().then_some(exe)
}

/// home/toolchains/python/cpython-<version>-windows-x86_64-none (uv's own folder name).
fn uv_python_dir(ws: &Workspace, t: &toml::Value) -> Option<PathBuf> {
    let to = ws.resolve(s(t, "to"))?;
    Some(to.join(format!("cpython-{}-windows-x86_64-none", s(t, "version"))))
}

/// kind = "uv-python": the portable Python of this workspace. There — fine; not there — a matching Python of the
/// machine is fine too (a note), with --apply uv puts the portable one in (no shims in ~/.local/bin, no registry).
fn uv_python(
    ws: &Workspace,
    t: &toml::Value,
    apply: bool,
    label: &str,
    miss: Level,
    out: &mut Vec<Finding>,
) {
    let version = s(t, "version");
    let Some(dir) = uv_python_dir(ws, t) else {
        out.push(Finding::new(
            Level::Error,
            "tools",
            label,
            "tools.lock: python needs `to` and `version`",
        ));
        return;
    };
    let exe = dir.join("python.exe");
    if exe.is_file() {
        out.push(Finding::new(
            Level::Info,
            "tools",
            label,
            format!("{} ({})", version_line(&exe), dir.display()),
        ));
        return;
    }
    // a Python of the machine (PATH) that matches `expect`
    let system = Command::new("python")
        .arg("--version")
        .output()
        .ok()
        .map(|o| {
            format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            )
        })
        .filter(|v| {
            regex::Regex::new(s(t, "expect"))
                .map(|r| r.is_match(v))
                .unwrap_or(false)
        })
        .map(|v| v.trim().to_string());
    if !apply {
        out.push(match &system {
            Some(v) => Finding::new(
                Level::Note,
                "tools",
                label,
                format!("{v} of the machine is used; ws bootstrap --apply puts the portable Python {version} into {}", dir.display()),
            ),
            None => Finding::new(miss, "tools", label, format!("no Python 3.12+ - {}", s(t, "install"))),
        });
        return;
    }
    let uv = ws.home().join("tools").join("uv").join("uv.exe");
    if !uv.is_file() {
        out.push(Finding::new(
            miss,
            "tools",
            label,
            format!(
                "{} missing (tool uv) - run ws bootstrap --apply again",
                uv.display()
            ),
        ));
        return;
    }
    let to = dir.parent().unwrap_or(&dir).to_path_buf();
    let ok = Command::new(&uv)
        .args([
            "python",
            "install",
            version,
            "--no-bin",
            "--no-registry",
            "--install-dir",
        ])
        .arg(&to)
        .env("UV_CACHE_DIR", ws.home().join("cache").join("uv"))
        .status()
        .map(|st| st.success())
        .unwrap_or(false);
    out.push(if ok && exe.is_file() {
        Finding::new(
            Level::Done,
            "tools",
            label,
            format!("{} ({})", version_line(&exe), dir.display()),
        )
    } else {
        Finding::new(
            miss,
            "tools",
            label,
            format!("uv python install {version} failed - {}", s(t, "install")),
        )
    });
}

/// First line of `exe --version` output (empty if it did not start).
fn version_line(exe: &Path) -> String {
    Command::new(exe)
        .arg("--version")
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        })
        .unwrap_or_default()
}

fn tool(ws: &Workspace, t: &toml::Value, apply: bool, build: Build, out: &mut Vec<Finding>) {
    let name = s(t, "name");
    let label = format!("tool {name}");
    let required = t.get("required").and_then(|v| v.as_bool()).unwrap_or(true);
    let miss = if required { Level::Error } else { Level::Warn };
    match s(t, "kind") {
        "cargo" => {
            let Some(dest) = ws.resolve(s(t, "to")) else {
                return;
            };
            let exe = crate::native(&dest.join(s(t, "exe")));
            let (krate, version) = (s(t, "crate"), s(t, "version"));
            let have = if exe.is_file() {
                version_line(&exe)
            } else {
                String::new()
            };
            if !have.is_empty() && have.ends_with(version) {
                out.push(Finding::new(Level::Info, "tools", label, have));
                return;
            }
            let what = if have.is_empty() {
                format!("missing {}", exe.display())
            } else {
                format!("'{have}', need {version}")
            };
            if !apply {
                out.push(Finding::new(
                    miss,
                    "tools",
                    label,
                    format!("{what} - installed by cargo xtask setup or ws bootstrap"),
                ));
                return;
            }
            let res = match t.get("prebuilt").filter(|_| build == Build::Prebuilt) {
                Some(p) => install_prebuilt(p, &exe).map(|_| "prebuilt"),
                None => cargo_install(krate, version, &dest, &exe).map(|_| "built from source"),
            };
            match res {
                Ok(how) => out.push(Finding::new(
                    Level::Done,
                    "tools",
                    label,
                    format!("installed {krate} {version} ({how}): {}", exe.display()),
                )),
                Err(e) => out.push(Finding::new(miss, "tools", label, e)),
            }
        }
        // A big program that a person installs and only for specific jobs (Visual Studio, Unreal Engine,
        // Ghidra): not an error and not a warning, ws does not install it — only says whether it is there and what it is for.
        "external" => {
            let why = s(t, "for");
            match external_found(ws, t) {
                Some(at) => out.push(Finding::new(
                    Level::Info,
                    "tools",
                    label,
                    format!("{at} (for: {why})"),
                )),
                None => out.push(Finding::new(
                    Level::Note,
                    "tools",
                    label,
                    format!(
                        "not installed; needed only for: {why}. Install: {}",
                        s(t, "install")
                    ),
                )),
            }
        }
        "rust-target" => {
            // a target of the workspace toolchain: rustup runs in the root, so rust-toolchain.toml picks the toolchain
            let target = s(t, "target");
            let installed = Command::new("rustup")
                .args(["target", "list", "--installed"])
                .current_dir(&ws.root)
                .output()
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .any(|l| l.trim() == target)
                })
                .unwrap_or(false);
            if installed {
                out.push(Finding::new(
                    Level::Info,
                    "tools",
                    label,
                    format!("{target} installed"),
                ));
            } else if !apply {
                out.push(Finding::new(
                    miss,
                    "tools",
                    label,
                    format!("{target} missing - {} ({})", s(t, "install"), s(t, "why")),
                ));
            } else {
                let ok = Command::new("rustup")
                    .args(["target", "add", target])
                    .current_dir(&ws.root)
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false);
                out.push(if ok {
                    Finding::new(
                        Level::Done,
                        "tools",
                        label,
                        format!("rustup target add {target}"),
                    )
                } else {
                    Finding::new(
                        miss,
                        "tools",
                        label,
                        format!("rustup target add {target} failed"),
                    )
                });
            }
        }
        "uv-python" => uv_python(ws, t, apply, &label, miss, out),
        "system" => {
            let cmd = str_list(t.get("check"));
            let Some((exe, args)) = cmd.split_first() else {
                return;
            };
            // the program may be given in local.toml [tools] (full path)
            let exe = ws
                .local_str("tools", name)
                .and_then(|p| ws.resolve(&p))
                .map(|p| p.display().to_string())
                .unwrap_or(exe.clone());
            match Command::new(&exe).args(args).output() {
                Ok(o) => {
                    let text = format!(
                        "{}{}",
                        String::from_utf8_lossy(&o.stdout),
                        String::from_utf8_lossy(&o.stderr)
                    );
                    let first = text.lines().next().unwrap_or("").trim().to_string();
                    let expect = s(t, "expect");
                    if !expect.is_empty()
                        && !regex::Regex::new(expect)
                            .map(|r| r.is_match(&text))
                            .unwrap_or(false)
                    {
                        out.push(Finding::new(
                            Level::Warn,
                            "tools",
                            label,
                            format!(
                                "version '{first}', expected /{expect}/ - {}",
                                s(t, "install")
                            ),
                        ));
                    } else {
                        out.push(Finding::new(Level::Info, "tools", label, first));
                    }
                }
                Err(_) => out.push(Finding::new(
                    miss.clone(),
                    "tools",
                    label,
                    format!("not found ({exe}) - {}", s(t, "install")),
                )),
            }
        }
        "copy" | "download" => {
            let Some(dest) = ws.resolve(s(t, "to")) else {
                return;
            };
            let exe = crate::native(&dest.join(s(t, "exe")));
            let want = s(t, "sha256");
            if exe.is_file() {
                match (want.is_empty(), sha256_file(&exe)) {
                    (false, Some(h)) if h != want => out.push(Finding::new(
                        Level::Error,
                        "tools",
                        label,
                        format!(
                            "{}: sha256 {} != {} in tools.lock",
                            exe.display(),
                            &h[..12],
                            &want[..12.min(want.len())]
                        ),
                    )),
                    _ => out.push(Finding::new(
                        Level::Info,
                        "tools",
                        label,
                        format!("present: {}", exe.display()),
                    )),
                }
                return;
            }
            if !apply {
                out.push(Finding::new(
                    miss.clone(),
                    "tools",
                    label,
                    format!(
                        "missing {} - installed by cargo xtask setup or ws bootstrap",
                        exe.display()
                    ),
                ));
                return;
            }
            let res = if s(t, "kind") == "copy" {
                copy_from(ws, s(t, "from"), &dest)
            } else {
                download(t, &dest)
            };
            match res.and_then(|_| verify(&exe, want)) {
                Ok(()) => out.push(Finding::new(
                    Level::Done,
                    "tools",
                    label,
                    format!("installed: {}", exe.display()),
                )),
                Err(e) => out.push(Finding::new(miss.clone(), "tools", label, e)),
            }
        }
        other => out.push(Finding::new(
            Level::Error,
            "tools",
            label,
            format!("unknown kind '{other}'"),
        )),
    }
}

fn artifacts(ws: &Workspace, opt: &Options, doc: &toml::Value, out: &mut Vec<Finding>) {
    let (mut total, mut present) = (0, 0);
    // no local.toml [store]: access to the repository only (a tester, a first look) - what only the store brings is
    // a note, not an error (18.11)
    let no_store =
        ws.local_str("store", "url").is_none() || ws.local_str("store", "user").is_none();
    let mut skipped: Vec<String> = Vec::new();
    for a in doc
        .get("artifact")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .filter(|a| for_profile(a, &opt.profile))
        .filter(|a| crate::components::wanted(ws, a))
    {
        let label = format!("artifact {}", s(a, "name"));
        let Some(path) = ws.resolve(s(a, "path")) else {
            continue;
        };
        // manual = true — big and not needed by everyone (exe image for Ghidra): on request only, no errors or warnings
        let manual = a.get("manual").and_then(|v| v.as_bool()).unwrap_or(false);
        if manual && !path.exists() && !opt.artifacts.iter().any(|n| n == s(a, "name")) {
            if no_store {
                skipped.push(s(a, "name").to_string());
                continue;
            }
            out.push(Finding::new(
                Level::Note,
                "artifacts",
                label,
                format!(
                    "on demand: ws secrets run artifact_store -- ws bootstrap --apply --artifact=\"{}\" ({})",
                    s(a, "name"),
                    s(a, "for")
                ),
            ));
            continue;
        }
        let want = s(a, "sha256");
        let miss = if a.get("required").and_then(|v| v.as_bool()).unwrap_or(true) {
            Level::Error
        } else {
            Level::Warn
        };
        total += 1;
        if path.exists() {
            present += 1;
            if path.is_file() && !want.is_empty() && sha256_file(&path).as_deref() != Some(want) {
                out.push(Finding::new(
                    Level::Warn,
                    "artifacts",
                    label,
                    format!(
                        "{}: sha256 differs from artifacts.toml (rebuilt? update the entry)",
                        path.display()
                    ),
                ));
            }
            continue;
        }
        let stored = s(a, "store_sha256");
        // source: own copy (from, for the owner — legacy:), if it exists on this machine; otherwise the store
        let local_src = Some(s(a, "from"))
            .filter(|f| !f.is_empty())
            .and_then(|f| ws.resolve(f))
            .filter(|p| p.exists());
        if no_store && !stored.is_empty() && local_src.is_none() {
            total -= 1;
            skipped.push(s(a, "name").to_string());
            continue;
        }
        if !opt.apply {
            let how = if stored.is_empty() {
                s(a, "rebuild").to_string()
            } else {
                format!(
                    "ws bootstrap takes it from the artifact store; or {}",
                    s(a, "rebuild")
                )
            };
            out.push(Finding::new(
                miss,
                "artifacts",
                label,
                format!("missing {} - {how}", path.display()),
            ));
            continue;
        }
        let res = if local_src.is_some() || stored.is_empty() {
            copy_from(ws, s(a, "from"), &path).map(|_| "copied")
        } else {
            from_store(ws, a, &path).map(|_| "downloaded from the artifact store")
        };
        match res.and_then(|how| {
            if path.is_file() {
                verify(&path, want).map(|_| how)
            } else {
                Ok(how)
            }
        }) {
            Ok(how) => out.push(Finding::new(
                Level::Done,
                "artifacts",
                label,
                format!("{how}: {}", path.display()),
            )),
            Err(e) => out.push(Finding::new(
                miss,
                "artifacts",
                label,
                format!("{e} - {}", s(a, "rebuild")),
            )),
        }
    }
    if !skipped.is_empty() {
        out.push(Finding::new(
            Level::Note,
            "artifacts",
            "local.toml [store]",
            format!(
                "no artifact store on this machine (repository-only access): skipped {} - {}; build them from \
                 your copy of the game (docs/STORAGE_EXPORT.md); a team member gets team.kdbx and the [store] lines \
                 from the owner (docs/CONTRIBUTING.md)",
                skipped.len(),
                skipped.join(", ")
            ),
        ));
    }
    out.push(Finding::new(
        Level::Info,
        "artifacts",
        "artifacts.toml",
        format!("{present} of {total} present before this run"),
    ));
}

/// Your own copy of the game is required (18.11, docs/GAME_DATA.md): no reference — an error in doctor; bootstrap
/// --apply takes a matching Steam install (a copy of the clean files) or a download_depot download itself.
/// Not for the "agent" profile (a machine for AI agents only).
fn game_reference(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    if opt.profile == "agent" {
        return;
    }
    let root = crate::game::original_root(ws);
    let have = std::fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| e.path().join("original.toml").is_file());
    if have {
        return; // ws game checks it against config/game.toml
    }
    let Some(b) = crate::game::builds(ws).into_iter().next() else {
        return;
    };
    let (level, msg) = crate::game::missing_reference(ws, &b, opt.apply);
    out.push(Finding::new(level, "game", "reference", msg));
}

fn games(ws: &Workspace, out: &mut Vec<Finding>) {
    let Some(t) = ws.local.get("games").and_then(|g| g.as_table()) else {
        out.push(Finding::new(
            Level::Warn,
            "games",
            "local.toml [games]",
            "no game installs listed",
        ));
        return;
    };
    let mut ok = Vec::new();
    for (k, v) in t {
        let Some(p) = v.as_str().and_then(|p| ws.resolve(p)) else {
            continue;
        };
        if p.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
            .is_file()
        {
            ok.push(k.clone());
        } else {
            out.push(Finding::new(
                Level::Warn,
                "games",
                format!("local.toml [games] {k}"),
                format!("{}: not a game install", p.display()),
            ));
        }
    }
    out.push(Finding::new(
        Level::Info,
        "games",
        "local.toml [games]",
        format!(
            "installs ok: {}",
            if ok.is_empty() {
                "none".into()
            } else {
                ok.join(", ")
            }
        ),
    ));
}

/// Artifact from the store (crate::store): object store_sha256 → a file or an unpacked tar (store_tar = true).
fn from_store(ws: &Workspace, a: &toml::Value, path: &Path) -> Result<(), String> {
    use crate::store::{Missing, Store, ENV_READ};
    let store = match Store::open(ws, false) {
        Ok(st) => st,
        Err(Missing::NotConfigured) => {
            return Err("artifact store not configured: local.toml [store] url and user".into())
        }
        Err(Missing::NoPassword(_)) => {
            return Err(format!(
                "no {ENV_READ}: open the vault session (ws secrets unlock, or DevKit) or run ws secrets run artifact_store -- ws bootstrap"
            ))
        }
    };
    let sha = s(a, "store_sha256");
    let part = ws.home().join("cache/tmp").join(format!("{sha}.part"));
    store.fetch(sha, &part)?;
    let tar = a
        .get("store_tar")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    crate::store::install(&part, path, tar)
}

fn copy_from(ws: &Workspace, from: &str, dest: &Path) -> Result<(), String> {
    if from.is_empty() {
        return Err("no source (from) - needs a download or storage (step 5)".into());
    }
    let src = ws
        .resolve(from)
        .ok_or_else(|| format!("{from}: prefix legacy: needs local.toml [legacy] root"))?;
    if !src.exists() {
        return Err(format!("source missing: {}", src.display()));
    }
    let target = if src.is_file() && (dest.is_dir() || dest.extension().is_none()) {
        dest.join(src.file_name().unwrap())
    } else {
        dest.to_path_buf()
    };
    copy_tree(&src, &target)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", src.display(), target.display()))
}

/// download: url (+ sha256 of the archive/file), unzip = true — unpack into to (Windows tar.exe can do zip).
/// A ready exe from the official releases page ([tool.prebuilt]): the archive at url is checked against archive_sha256,
/// unpacked into the exe folder, the exe itself is checked against sha256. Without both hashes it is not installed.
fn install_prebuilt(p: &toml::Value, exe: &Path) -> Result<(), String> {
    if s(p, "archive_sha256").is_empty() || s(p, "sha256").is_empty() {
        return Err("[tool.prebuilt] needs archive_sha256 and sha256".into());
    }
    let dir = exe.parent().ok_or("bad exe path")?;
    let mut t = p.clone();
    if let Some(tab) = t.as_table_mut() {
        tab.insert("unzip".into(), toml::Value::Boolean(true));
    }
    download(&t, dir)?;
    verify(exe, s(p, "sha256"))
}

/// Build from source: --locked — dependency versions from the crate's own Cargo.lock; --force — replace another version.
fn cargo_install(krate: &str, version: &str, dest: &Path, exe: &Path) -> Result<(), String> {
    let st = Command::new("cargo")
        .args(["install", "--locked", "--force", "--root"])
        .arg(dest)
        .arg(format!("{krate}@{version}"))
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !st.success() {
        return Err(format!(
            "cargo install {krate}@{version}: exit code {:?}",
            st.code()
        ));
    }
    if !version_line(exe).ends_with(version) {
        return Err(format!(
            "{}: not version {version} after install",
            exe.display()
        ));
    }
    Ok(())
}

fn download(t: &toml::Value, dest: &Path) -> Result<(), String> {
    let url = s(t, "url");
    if url.is_empty() {
        return Err("no url".into());
    }
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let name = match s(t, "save_as") {
        "" => url.rsplit('/').next().unwrap_or("download"),
        n => n,
    };
    let file = dest.join(name);
    // Download into `<name>.part` and rename only after success: an interrupted download does not stay under the real
    // name (otherwise the next run considers the tool installed and fails on sha256). --retry-all-errors —
    // retry also when the connection drops mid-file (curl 56 "Connection was reset"), not only on connection errors.
    let part = dest.join(format!("{name}.part"));
    // curl from System32 (like bootstrap.ps1), not the first one found in PATH
    let curl: std::path::PathBuf = std::env::var_os("SystemRoot")
        .map(|r| Path::new(&r).join("System32").join("curl.exe"))
        .filter(|p| p.is_file())
        .unwrap_or_else(|| "curl".into());
    // A big file — in chunks over several connections: on some routes (17.11: the path to GitHub's CDN) each
    // connection is throttled (~100 KB/s), not the link; 8 connections are 6–8 times faster. A server without range
    // resume or a small file — a single request, as before. The hash is still checked on the whole file below.
    let parallel = match remote_size(&curl, url) {
        Some(size) if size >= PARALLEL_MIN => {
            fetch_parallel(&curl, url, &part, size, PARALLEL_PARTS).map(|_| true)
        }
        _ => Ok(false),
    };
    match parallel {
        Ok(true) => {}
        Ok(false) => {
            let st = Command::new(&curl)
                .args([
                    "-fsSL",
                    "--retry",
                    "5",
                    "--retry-all-errors",
                    "--retry-delay",
                    "2",
                    "-o",
                ])
                .arg(&part)
                .arg(url)
                .status()
                .map_err(|e| format!("curl: {e}"))?;
            if !st.success() {
                let _ = std::fs::remove_file(&part);
                return Err(format!("curl {url}: exit code {:?}", st.code()));
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            return Err(format!("{url}: {e}"));
        }
    }
    // Hash of the downloaded file — before the rename: an archive is checked against archive_sha256, a file without unpacking (the exe itself) — against
    // sha256. On mismatch the file is deleted and never appears under the real name.
    let unzip = t.get("unzip").and_then(|v| v.as_bool()).unwrap_or(false);
    // sfx = true: a 7-Zip self-extracting archive (PortableGit-*.7z.exe), checked by archive_sha256 like a zip
    let sfx = t.get("sfx").and_then(|v| v.as_bool()).unwrap_or(false);
    let want = match (s(t, "archive_sha256"), unzip || sfx) {
        ("", false) => s(t, "sha256"),
        (a, _) => a,
    };
    if !want.is_empty() && sha256_file(&part).as_deref() != Some(want) {
        let _ = std::fs::remove_file(&part);
        return Err(format!(
            "{url}: sha256 does not match tools.lock - file deleted"
        ));
    }
    std::fs::rename(&part, &file).map_err(|e| format!("{}: {e}", file.display()))?;
    if sfx {
        // the archive unpacks itself: -o<folder> -y (no questions); then it is deleted like a zip
        let st = Command::new(&file)
            .arg(format!("-o{}", crate::native(dest).display()))
            .arg("-y")
            .status()
            .map_err(|e| format!("{}: {e}", file.display()))?;
        let _ = std::fs::remove_file(&file);
        if !st.success() {
            return Err("self-extracting archive failed".into());
        }
    }
    if unzip {
        // strip = N — remove N top folders of the archive (clang+llvm-<version>/bin/… → bin/…)
        let strip = t.get("strip").and_then(|v| v.as_integer()).unwrap_or(0);
        let mut tar = Command::new("tar");
        tar.arg("-xf").arg(&file).arg("-C").arg(dest);
        if strip > 0 {
            tar.arg(format!("--strip-components={strip}"));
        }
        let st = tar.status().map_err(|e| format!("tar: {e}"))?;
        let _ = std::fs::remove_file(&file);
        if !st.success() {
            return Err("unpacking failed".into());
        }
    }
    Ok(())
}

/// Files from this size on are downloaded in chunks (for smaller ones the gain is not worth the extra requests).
const PARALLEL_MIN: u64 = 32 * 1024 * 1024;
/// How many connections at once.
const PARALLEL_PARTS: u64 = 8;

/// File size at the address, if the server serves ranges (`Accept-Ranges: bytes` in the final response after redirects).
fn remote_size(curl: &Path, url: &str) -> Option<u64> {
    let out = Command::new(curl)
        .args(["-fsSIL", "--max-time", "30", url])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    ranged_size(&String::from_utf8_lossy(&out.stdout))
}

/// From `curl -sIL` headers: the size in the last response, if it serves ranges.
fn ranged_size(headers: &str) -> Option<u64> {
    let text = headers.replace("\r\n", "\n").to_ascii_lowercase();
    // headers of the last response (after all 30x)
    let last = text.rsplit("\n\n").find(|b| b.contains("http/"))?;
    if !last.contains("accept-ranges: bytes") {
        return None;
    }
    last.lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse().ok())
}

/// Download `size` bytes into `part` in chunks over `parts` connections. An interrupted chunk resumes from where it broke
/// (up to 30 attempts per chunk, a stalled connection is dropped after 60 s below 1 KB/s), then the chunks are joined in order.
fn fetch_parallel(
    curl: &Path,
    url: &str,
    part: &Path,
    size: u64,
    parts: u64,
) -> Result<(), String> {
    let step = size.div_ceil(parts);
    let piece = |i: u64| part.with_extension(format!("part{i:02}"));
    let handles: Vec<_> = (0..parts)
        .map(|i| {
            let (curl, url, file) = (curl.to_path_buf(), url.to_string(), piece(i));
            let from = i * step;
            let to = ((i + 1) * step).min(size) - 1;
            std::thread::spawn(move || -> Result<(), String> {
                let want = to + 1 - from;
                let _ = std::fs::remove_file(&file);
                for _ in 0..30 {
                    let got = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
                    if got >= want {
                        return Ok(());
                    }
                    // an attempt goes to a temp file (not to memory: a chunk of a big archive is hundreds of MB), then appended.
                    // The name derives from the whole chunk name: with_extension("try") gave one file for all threads (17.11,
                    // the parallel_matches_single test caught the corruption)
                    let attempt = std::path::PathBuf::from(format!("{}.try", file.display()));
                    let _ = Command::new(&curl)
                        .args([
                            "-fsSL",
                            "--connect-timeout",
                            "20",
                            "-y",
                            "60",
                            "-Y",
                            "1024",
                            "-r",
                        ])
                        .arg(format!("{}-{to}", from + got))
                        .arg("-o")
                        .arg(&attempt)
                        .arg(&url)
                        .status()
                        .map_err(|e| format!("curl: {e}"))?;
                    let n = std::fs::metadata(&attempt).map(|m| m.len()).unwrap_or(0);
                    if n > 0 {
                        // the server must not send more than requested; extra data is an error, not glued garbage
                        if n > want - got {
                            let _ = std::fs::remove_file(&attempt);
                            return Err(format!("range {from}-{to}: server sent more than asked"));
                        }
                        let mut src = std::fs::File::open(&attempt).map_err(|e| e.to_string())?;
                        let mut dst = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&file)
                            .map_err(|e| format!("{}: {e}", file.display()))?;
                        std::io::copy(&mut src, &mut dst).map_err(|e| e.to_string())?;
                    } else {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                    }
                    let _ = std::fs::remove_file(&attempt);
                }
                Err(format!("range {from}-{to}: incomplete after 30 tries"))
            })
        })
        .collect();
    let mut errors = Vec::new();
    for h in handles {
        if let Err(e) = h
            .join()
            .unwrap_or_else(|_| Err("download thread panicked".into()))
        {
            errors.push(e);
        }
    }
    let result = if errors.is_empty() {
        (|| -> Result<(), String> {
            let mut out =
                std::fs::File::create(part).map_err(|e| format!("{}: {e}", part.display()))?;
            for i in 0..parts {
                let mut f = std::fs::File::open(piece(i)).map_err(|e| e.to_string())?;
                std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
            }
            let len = out.metadata().map_err(|e| e.to_string())?.len();
            if len != size {
                return Err(format!("joined {len} bytes, expected {size}"));
            }
            Ok(())
        })()
    } else {
        Err(errors.join("; "))
    };
    for i in 0..parts {
        let _ = std::fs::remove_file(piece(i));
    }
    result
}

fn verify(path: &Path, want: &str) -> Result<(), String> {
    if want.is_empty() {
        return Ok(());
    }
    match sha256_file(path) {
        Some(h) if h == want => Ok(()),
        Some(h) => Err(format!(
            "{}: sha256 {} != {} in the list",
            path.display(),
            &h[..12],
            &want[..12.min(want.len())]
        )),
        None => Err(format!("missing {}", path.display())),
    }
}

/// .claude/settings.local.json → env: DB_HOME; CARGO_HOME/RUSTUP_HOME if the toolchains are already in home (step 4);
/// everything from local.toml [env] ("home:" is expanded). Other keys of the file are not touched.
fn agent_env(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let home = ws.home();
    let mut env: Vec<(String, String)> = vec![("DB_HOME".into(), home.display().to_string())];
    for (var, dir) in [
        ("RUSTUP_HOME", "toolchains/rustup"),
        ("CARGO_HOME", "toolchains/cargo"),
    ] {
        if home.join(dir).is_dir() {
            env.push((
                var.into(),
                crate::native(&home.join(dir)).display().to_string(),
            ));
        }
    }
    // the portable Python (tools.lock kind uv-python): agents call it by DB_PYTHON, uv run takes it by UV_PYTHON;
    // PATH of the agents is not touched
    if let Some(py) = portable_python(ws) {
        let py = py.display().to_string();
        env.push(("DB_PYTHON".into(), py.clone()));
        env.push(("UV_PYTHON".into(), py));
    }
    // 19.11 portable git and gh (tools.lock): Claude Code takes Git Bash from CLAUDE_CODE_GIT_BASH_PATH (its bash puts
    // its own git and coreutils on PATH); DB_GIT / DB_GH — absolute paths for PowerShell and scripts; gh reaches the
    // Bash tool's PATH through a SessionStart hook (CLAUDE_ENV_FILE) — the env block cannot extend PATH
    let bash = home.join("tools").join("git").join("bin").join("bash.exe");
    let git = home.join("tools").join("git").join("cmd").join("git.exe");
    let gh = home.join("tools").join("gh").join("bin").join("gh.exe");
    if bash.is_file() {
        env.push((
            "CLAUDE_CODE_GIT_BASH_PATH".into(),
            crate::native(&bash).display().to_string(),
        ));
    }
    if git.is_file() {
        env.push(("DB_GIT".into(), crate::native(&git).display().to_string()));
    }
    let gh_hook = gh
        .is_file()
        .then(|| gh_path_hook(&home.join("tools").join("gh").join("bin")));
    if gh.is_file() {
        env.push(("DB_GH".into(), crate::native(&gh).display().to_string()));
    }
    if let Some(t) = ws.local.get("env").and_then(|e| e.as_table()) {
        for (k, v) in t {
            if let Some(p) = v.as_str().and_then(|p| ws.resolve(p)) {
                env.push((k.clone(), p.display().to_string()));
            }
        }
    }
    let path: PathBuf = ws.root.join(".claude").join("settings.local.json");
    let mut doc: serde_json::Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let cur = doc
        .get("env")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let mut changed: Vec<String> = env
        .iter()
        .filter(|(k, v)| cur.get(k).and_then(|x| x.as_str()) != Some(v.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    let hooks_now = session_start_hooks(&doc);
    let hook_missing = gh_hook
        .as_ref()
        .is_some_and(|h| !hooks_now.iter().any(|c| c == h));
    if hook_missing {
        changed.push("SessionStart hook: gh on PATH".into());
    }
    if changed.is_empty() {
        return;
    }
    if !opt.apply {
        out.push(Finding::new(
            Level::Warn,
            "agent",
            ".claude/settings.local.json",
            format!("env not set: {}", changed.join(", ")),
        ));
        return;
    }
    let obj = doc.as_object_mut().unwrap();
    let e = obj.entry("env").or_insert_with(|| serde_json::json!({}));
    for (k, v) in &env {
        e[k] = serde_json::Value::String(v.clone());
    }
    if let (true, Some(cmd)) = (hook_missing, gh_hook.as_ref()) {
        // our hook is recognised by GH_HOOK_MARK and replaced; other people's hooks stay
        let hooks = obj.entry("hooks").or_insert_with(|| serde_json::json!({}));
        let list = hooks
            .as_object_mut()
            .map(|h| {
                h.entry("SessionStart")
                    .or_insert_with(|| serde_json::json!([]))
            })
            .and_then(|l| l.as_array_mut());
        if let Some(list) = list {
            list.retain(|g| !g.to_string().contains(GH_HOOK_MARK));
            list.push(serde_json::json!({ "hooks": [{ "type": "command", "command": cmd }] }));
        }
    }
    let _ = std::fs::create_dir_all(path.parent().unwrap());
    match std::fs::write(&path, serde_json::to_string_pretty(&doc).unwrap() + "\n") {
        Ok(_) => out.push(Finding::new(
            Level::Done,
            "agent",
            ".claude/settings.local.json",
            format!("env: {} (new sessions pick it up)", changed.join(", ")),
        )),
        Err(e) => out.push(Finding::new(
            Level::Error,
            "agent",
            ".claude/settings.local.json",
            e.to_string(),
        )),
    }
}

/// Marks our SessionStart hook in .claude/settings.local.json (it is rewritten, other hooks are not touched).
const GH_HOOK_MARK: &str = "# ws bootstrap: gh on PATH";

/// The SessionStart hook that puts home/tools/gh/bin on the Bash tool's PATH (Claude Code runs the lines written to
/// $CLAUDE_ENV_FILE before every Bash command; the hook runs in Git Bash, so the path is in its /f/... form).
fn gh_path_hook(bin: &Path) -> String {
    let p = crate::native(bin).display().to_string().replace('\\', "/");
    let posix = match p.split_once(":/") {
        Some((drive, rest)) if drive.len() == 1 => {
            format!("/{}/{}", drive.to_ascii_lowercase(), rest)
        }
        _ => p,
    };
    format!("echo 'export PATH=\"{posix}:$PATH\"' >> \"$CLAUDE_ENV_FILE\" {GH_HOOK_MARK}")
}

/// Commands of the SessionStart hooks in a settings document.
fn session_start_hooks(doc: &serde_json::Value) -> Vec<String> {
    doc.pointer("/hooks/SessionStart")
        .and_then(|l| l.as_array())
        .into_iter()
        .flatten()
        .filter_map(|g| g.get("hooks").and_then(|h| h.as_array()))
        .flatten()
        .filter_map(|h| h.get("command").and_then(|c| c.as_str()).map(String::from))
        .collect()
}

/// First line of the root CLAUDE.md that ws writes: a file without it is someone's own and is left alone.
const ROOT_NOTE_MARK: &str = "<!-- written by ws bootstrap (root_note in tools/ws/core/src/bootstrap.rs): rewritten, do not edit -->";

/// The text of `<root>/CLAUDE.md`: an AI session opened in the install root (next to workspace, home, vault) does not
/// get the project's rules and roles — it tells the human where to open it instead.
pub fn root_note_text(root: &Path, ws: &Path, home: &Path) -> String {
    let ws_name = ws
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    format!(
        "{ROOT_NOTE_MARK}
# Dungeonborne — install root

This folder holds a Dungeonborne install: `{ws_name}/` (the project, git), `home/` (tools, caches, game data — not in
git), `vault/` (the secrets database).

**The work happens in `{ws_name}/`.** The project's rules (its CLAUDE.md), the AI roles (`.claude/agents`), the
environment and the game data paths are loaded only when Claude Code is opened there.

If you are an AI agent and this session started here, in the root `{root}`:

1. Do not start the task here and do not create or edit files here.
2. Tell the human, in their language and in one short step: open the session in `{ws}` — in a terminal
   `cd \"{ws}\"`, then `claude`; or press **Start** in DevKit (`{devkit}`), which opens it there with the task.
3. Never open or read `vault/`.

Exception: the human asked you to look at `bootstrap.ps1` before the install (`docs/CONTRIBUTING.md`) — read it,
give them your conclusion, and let them run it.
",
        root = root.display(),
        ws = ws.display(),
        devkit = home.join("tools").join("devkit").join("DevKit.exe").display(),
    )
}

/// `<root>/CLAUDE.md` for the default layout `<root>/{workspace,home}`: doctor says whether it is there, bootstrap
/// writes it. A CLAUDE.md the human wrote (without the mark) stays as it is.
fn root_note(ws: &Workspace, opt: &Options, out: &mut Vec<Finding>) {
    let home = ws.home();
    let key = |p: &Path| crate::normalize(p).display().to_string().to_lowercase();
    let (Some(root), Some(hp)) = (ws.root.parent(), home.parent()) else {
        return;
    };
    if key(root) != key(hp) {
        return; // home is elsewhere: no common install root to mark
    }
    let path = root.join("CLAUDE.md");
    let want = root_note_text(root, &ws.root, &home);
    let label = path.display().to_string();
    match std::fs::read_to_string(&path) {
        Ok(cur) if cur == want => {}
        Ok(cur) if !cur.starts_with(ROOT_NOTE_MARK) => out.push(Finding::new(
            Level::Note,
            "agent",
            label,
            "the human's own file (no ws mark): left as is; it should send AI sessions to the workspace folder",
        )),
        _ if !opt.apply => out.push(Finding::new(
            Level::Warn,
            "agent",
            label,
            "missing or old: an AI session opened in the install root would not find the project (ws bootstrap --apply)",
        )),
        _ => match std::fs::write(&path, want) {
            Ok(_) => out.push(Finding::new(
                Level::Done,
                "agent",
                label,
                "written: a session opened in the root sends the human to the workspace folder",
            )),
            Err(e) => out.push(Finding::new(Level::Error, "agent", label, e.to_string())),
        },
    }
}

#[cfg(test)]
mod root_note_tests {
    #[test]
    fn text_names_the_workspace() {
        let t = super::root_note_text(
            std::path::Path::new("X:/DB"),
            std::path::Path::new("X:/DB/workspace"),
            std::path::Path::new("X:/DB/home"),
        );
        assert!(t.starts_with(super::ROOT_NOTE_MARK));
        assert!(
            t.contains("`workspace/`")
                && t.contains("cd \"X:/DB/workspace\"")
                && t.contains("DevKit.exe")
        );
    }

    /// doctor warns, bootstrap writes, a second run is silent, the human's own file stays.
    #[test]
    fn written_once_and_never_over_a_human_file() {
        use super::{root_note, Build, Options};
        let base = std::env::temp_dir().join(format!("ws-root-note-{}", std::process::id()));
        let cfg = base.join("workspace").join("config");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::create_dir_all(base.join("home")).unwrap();
        std::fs::write(cfg.join("workspace.toml"), "[workspace]\nmembers = []\n").unwrap();
        // DB_HOME of the test process would point elsewhere; this workspace's home is <base>/home by local.toml
        let home = base.join("home").display().to_string().replace('\\', "/");
        std::fs::write(cfg.join("local.toml"), format!("[home]\npath = '{home}'\n")).unwrap();
        let ws = crate::Workspace::open(&base.join("workspace")).unwrap();
        let opt = |apply| Options {
            apply,
            profile: "dev".into(),
            build: Build::Prebuilt,
            artifacts: vec![],
        };
        let run = |apply| {
            let mut out = Vec::new();
            root_note(&ws, &opt(apply), &mut out);
            out
        };
        if std::env::var_os("DB_HOME").is_some() {
            // ws.home() prefers DB_HOME: the layout is not <base>/{workspace,home}, nothing to check here
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        assert!(matches!(run(false)[0].level, crate::Level::Warn));
        assert!(matches!(run(true)[0].level, crate::Level::Done));
        assert!(run(true).is_empty() && run(false).is_empty());
        let note = base.join("CLAUDE.md");
        assert!(std::fs::read_to_string(&note)
            .unwrap()
            .contains("The work happens in `workspace/`"));
        std::fs::write(&note, "# my own notes\n").unwrap();
        assert!(matches!(run(true)[0].level, crate::Level::Note));
        assert_eq!(std::fs::read_to_string(&note).unwrap(), "# my own notes\n");
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod no_store_tests {
    /// Repository-only access (no local.toml [store]): what only the store brings is one note, never an error.
    #[test]
    fn missing_store_artifacts_are_a_note() {
        use super::{artifacts, Build, Options};
        let base = std::env::temp_dir().join(format!("ws-no-store-{}", std::process::id()));
        let cfg = base.join("workspace").join("config");
        std::fs::create_dir_all(&cfg).unwrap();
        std::fs::write(cfg.join("workspace.toml"), "[workspace]\nmembers = []\n").unwrap();
        let home = base.join("home").display().to_string().replace('\\', "/");
        std::fs::write(cfg.join("local.toml"), format!("[home]\npath = '{home}'\n")).unwrap();
        let ws = crate::Workspace::open(&base.join("workspace")).unwrap();
        let doc: toml::Value = toml::from_str(
            "[[artifact]]\nname = \"dbnative.dll\"\npath = \"home:game/dbnative.dll\"\nstore_sha256 = \"ab\"\n\
             profiles = [\"dev\"]\n\n[[artifact]]\nname = \"exe image\"\npath = \"home:re/img.bin\"\n\
             store_sha256 = \"cd\"\nmanual = true\nprofiles = [\"dev\"]\n",
        )
        .unwrap();
        let opt = Options {
            apply: false,
            profile: "dev".into(),
            build: Build::Prebuilt,
            artifacts: vec![],
        };
        let mut out = Vec::new();
        artifacts(&ws, &opt, &doc, &mut out);
        assert!(
            out.iter()
                .all(|f| !matches!(f.level, crate::Level::Error | crate::Level::Warn)),
            "{:?}",
            out.iter().map(|f| &f.msg).collect::<Vec<_>>()
        );
        let note = out
            .iter()
            .find(|f| f.path == "local.toml [store]")
            .expect("one store note");
        assert!(note.msg.contains("skipped 2") && note.msg.contains("dbnative.dll"));
        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod download_tests {
    use super::{expand_env, fetch_parallel, ranged_size};

    #[test]
    fn env_expanded() {
        std::env::set_var("WS_TEST_DIR", "C:/x");
        assert_eq!(expand_env("%WS_TEST_DIR%/a.exe"), "C:/x/a.exe");
        assert_eq!(expand_env("%WS_NO_SUCH%/a"), "%WS_NO_SUCH%/a");
        assert_eq!(expand_env("100% sure"), "100% sure");
    }

    /// Network: 40 MB from an LLVM release in chunks vs a single request of the same range. `cargo test -p ws-core -- --ignored`
    #[test]
    #[ignore]
    fn parallel_matches_single() {
        let url = "https://github.com/llvm/llvm-project/releases/download/llvmorg-23.1.5/clang%2Bllvm-23.1.5-x86_64-pc-windows-msvc.tar.xz";
        let dir = std::env::temp_dir().join(format!("ws-dl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let size = 40 * 1024 * 1024;
        let part = dir.join("a.part");
        fetch_parallel(std::path::Path::new("curl"), url, &part, size, 8).unwrap();
        let single = dir.join("b");
        let st = std::process::Command::new("curl")
            .args(["-fsSL", "-r", &format!("0-{}", size - 1), "-o"])
            .arg(&single)
            .arg(url)
            .status()
            .unwrap();
        assert!(st.success());
        let (a, b) = (
            std::fs::read(&part).unwrap(),
            std::fs::read(&single).unwrap(),
        );
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(a.len() as u64, size);
        assert!(a == b, "parallel download differs from a single request");
    }

    #[test]
    fn size_from_last_response() {
        let h = "HTTP/1.1 302 Found\r\nLocation: https://x\r\nContent-Length: 0\r\n\r\nHTTP/1.1 200 OK\r\nContent-Length: 901309624\r\nAccept-Ranges: bytes\r\n\r\n";
        assert_eq!(ranged_size(h), Some(901309624));
        let no_ranges = "HTTP/2 200\r\ncontent-length: 5\r\n\r\n";
        assert_eq!(ranged_size(no_ranges), None);
    }
}

#[cfg(test)]
mod profile_tests {
    use super::remember_profile;

    fn tmp(name: &str, text: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("ws-profile-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("local.toml");
        std::fs::write(&f, text).unwrap();
        f
    }

    #[test]
    fn inserts_under_setup_and_keeps_comments() {
        let f = tmp(
            "insert",
            "# top\n[home]\n# path = 'x'\n\n[setup]\n# build = \"prebuilt\"\n\n[dist]\n",
        );
        assert!(remember_profile(&f, "dev").unwrap());
        let t = std::fs::read_to_string(&f).unwrap();
        assert!(t.contains("[setup]\nprofile = \"dev\""), "{t}");
        assert!(
            t.contains("# top") && t.contains("# build = \"prebuilt\"") && t.contains("[dist]")
        );
        let doc: toml::Value = t.parse().unwrap();
        assert_eq!(doc["setup"]["profile"].as_str(), Some("dev"));
    }

    #[test]
    fn replaces_and_is_idempotent() {
        let f = tmp(
            "replace",
            "[setup]\nprofile = \"owner\"\nbuild = \"source\"\n",
        );
        assert!(remember_profile(&f, "agent").unwrap());
        assert!(!remember_profile(&f, "agent").unwrap());
        let doc: toml::Value = std::fs::read_to_string(&f).unwrap().parse().unwrap();
        assert_eq!(doc["setup"]["profile"].as_str(), Some("agent"));
        assert_eq!(doc["setup"]["build"].as_str(), Some("source"));
    }

    #[test]
    fn appends_setup_when_missing() {
        let f = tmp("append", "[home]\n");
        assert!(remember_profile(&f, "designer").unwrap());
        let doc: toml::Value = std::fs::read_to_string(&f).unwrap().parse().unwrap();
        assert_eq!(doc["setup"]["profile"].as_str(), Some("designer"));
    }
}
