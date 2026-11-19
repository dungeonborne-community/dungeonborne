//! Dungeonborne DevKit — a window over ws-core (docs/WORKSPACE_PLAN.md, step 7). All logic is in `tools/ws/core`,
//! here are only the commands for the page: machine state (doctor/bootstrap), components on this machine,
//! workspace rules (check), secrets
//! (the spec and a KeePass database check — values are never handed out), PC backups, launching tools.
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use ws_core::{Finding, Level, Workspace};

type R<T> = Result<T, String>;

#[derive(Serialize)]
pub struct Item {
    level: &'static str,
    rule: String,
    path: String,
    msg: String,
}

fn items(f: Vec<Finding>) -> Vec<Item> {
    f.into_iter()
        .map(|x| Item {
            level: match x.level {
                Level::Error => "error",
                Level::Warn => "warn",
                Level::Done => "done",
                Level::Info => "ok",
                Level::Note => "note",
            },
            rule: x.rule.to_string(),
            path: x.path,
            msg: x.msg,
        })
        .collect()
}

pub struct AppState {
    root: Mutex<Option<PathBuf>>,
}

/// Workspace root: DBDEV_WORKSPACE, then next to DevKit.exe and above, then the current folder. At every level up
/// the sibling folder `workspace` is checked too — the `DBDev\{workspace,home,vault}` layout, the exe may live in home.
fn find_root() -> Option<PathBuf> {
    if let Some(w) =
        std::env::var_os("DBDEV_WORKSPACE").and_then(|p| Workspace::find(Path::new(&p)).ok())
    {
        return Some(w.root);
    }
    let mut starts: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        starts.extend(exe.parent().map(Path::to_path_buf));
    }
    starts.extend(std::env::current_dir().ok());
    starts.into_iter().find_map(|s| {
        s.ancestors().find_map(|a| {
            [a.to_path_buf(), a.join("workspace")]
                .into_iter()
                .find(|d| d.join("config").join("workspace.toml").is_file())
                .and_then(|d| Workspace::open(&d).ok())
                .map(|w| w.root)
        })
    })
}

fn workspace(state: &State<AppState>) -> R<Workspace> {
    let root = state
        .root
        .lock()
        .unwrap()
        .clone()
        .ok_or("no workspace selected")?;
    Workspace::open(&root)
}

fn git(root: &Path, args: &[&str]) -> String {
    Command::new("git")
        .arg("-c")
        .arg("safe.directory=*")
        .args(args)
        .current_dir(root)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

#[derive(Serialize)]
pub struct Info {
    root: String,
    home: String,
    vault: String,
    members: usize,
    branch: String,
    head: String,
    dirty: usize,
    origin: String,
    /// machine profile: local.toml [setup] profile (written by bootstrap), else owner
    profile: String,
}

#[tauri::command]
fn info(state: State<AppState>) -> R<Info> {
    let ws = workspace(&state)?;
    let vault = ws
        .local_str("vault", "path")
        .and_then(|p| ws.resolve(&p))
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let status = git(&ws.root, &["status", "--porcelain"]);
    Ok(Info {
        root: ws.root.display().to_string(),
        home: ws.home().display().to_string(),
        vault,
        members: ws.members.len(),
        branch: git(&ws.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        head: git(&ws.root, &["log", "-1", "--format=%h %s"]),
        dirty: status.lines().filter(|l| !l.is_empty()).count(),
        origin: git(&ws.root, &["remote", "get-url", "origin"]),
        profile: ws_core::bootstrap::choose_profile(&ws, None),
    })
}

#[tauri::command]
fn pick_root(app: AppHandle, state: State<AppState>) -> R<String> {
    let dir = app
        .dialog()
        .file()
        .blocking_pick_folder()
        .ok_or("no folder selected")?;
    let path = dir.into_path().map_err(|e| e.to_string())?;
    let ws = Workspace::find(&path)?;
    *state.root.lock().unwrap() = Some(ws.root.clone());
    Ok(ws.root.display().to_string())
}

#[tauri::command]
async fn doctor(state: State<'_, AppState>, profile: String, apply: bool) -> R<Vec<Item>> {
    let ws = workspace(&state)?;
    let secrets = if apply {
        session_values(&ws, &["artifact_store"], &["ws", "bootstrap"])
    } else {
        Vec::new()
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _env = SecretEnv::set(&secrets);
        // ready programs or a build from source — local.toml [setup] build (prebuilt by default)
        let build = ws_core::bootstrap::Build::choose(&ws, None);
        items(ws_core::bootstrap::run(
            &ws,
            &ws_core::bootstrap::Options {
                apply,
                profile,
                build,
                artifacts: Vec::new(),
            },
        ))
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn check(state: State<'_, AppState>) -> R<Vec<Item>> {
    let ws = workspace(&state)?;
    tauri::async_runtime::spawn_blocking(move || items(ws_core::check::run(&ws)))
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct MemberDto {
    path: String,
    name: String,
    kind: String,
    depends_on: Vec<String>,
    /// public | team | owner, empty — not marked (docs/WORKSPACE_PLAN.md, 13.3)
    visibility: String,
}

#[tauri::command]
fn members(state: State<AppState>) -> R<Vec<MemberDto>> {
    let ws = workspace(&state)?;
    Ok(ws
        .members
        .iter()
        .map(|m| MemberDto {
            path: m.path.clone(),
            name: m.name.clone(),
            kind: m.kind.clone(),
            depends_on: m.depends_on.clone(),
            visibility: m.visibility.clone(),
        })
        .collect())
}

#[derive(Serialize)]
pub struct ComponentDto {
    name: String,
    title: String,
    about: String,
    base: bool,
    members: Vec<String>,
    needs: Vec<String>,
    files: usize,
    installed: bool,
    /// external programs its work needs (Visual Studio, Unreal Engine, Ghidra) — installed by a human
    external: Vec<String>,
}

#[derive(Serialize)]
pub struct PresetDto {
    name: String,
    title: String,
    components: Vec<String>,
}

#[derive(Serialize)]
pub struct ComponentsDto {
    /// a partial checkout (only some components on disk)
    sparse: bool,
    components: Vec<ComponentDto>,
    presets: Vec<PresetDto>,
}

/// Components of the workspace (config/components.toml) and which are on this machine (docs/COMPONENTS_PLAN.md).
#[tauri::command]
fn components(state: State<AppState>) -> R<ComponentsDto> {
    let ws = workspace(&state)?;
    let st = ws_core::components::status(&ws)?;
    Ok(ComponentsDto {
        sparse: st.sparse,
        components: st
            .components
            .into_iter()
            .map(|c| ComponentDto {
                name: c.name,
                title: c.title,
                about: c.about,
                base: c.base,
                members: c.members,
                needs: c.needs,
                files: c.files,
                installed: c.installed,
                external: c.external,
            })
            .collect(),
        presets: st
            .presets
            .into_iter()
            .map(|(name, title, components)| PresetDto {
                name,
                title,
                components,
            })
            .collect(),
    })
}

/// `ws components set <names>`: without apply — the plan; with apply — git sparse checkout + local.toml.
#[tauri::command]
async fn components_set(
    state: State<'_, AppState>,
    names: Vec<String>,
    apply: bool,
) -> R<Vec<Item>> {
    let ws = workspace(&state)?;
    // names come from the page: only components and presets of the inventory are accepted (ws-core checks them)
    tauri::async_runtime::spawn_blocking(move || {
        items(ws_core::components::run(&ws, "set", &names, apply))
    })
    .await
    .map_err(|e| e.to_string())
}

// ------------------------------------------------------------------------------------------- the human and the agent
// The human's profile (local.toml [me], CLAUDE.md "Talking to the human"), starting a Claude Code session with a task
// file, and the page "Work" (branch, changes, agents' reports in home/runs/agents — agents/ROLES.md).

const LEVELS: [&str; 4] = ["newcomer", "designer", "dev", "owner"];
const LANGS: [&str; 2] = ["ru", "en"];
const DETAILS: [&str; 3] = ["short", "normal", "full"];
const MODES: [&str; 3] = ["quick", "task", "project"];
const ROLES: [&str; 9] = [
    "re", "launcher", "web", "mods", "balance", "backend", "kit", "stand", "design",
];

#[derive(Serialize)]
pub struct MeDto {
    level: String,
    lang: String,
    detail: String,
    /// local.toml [setup] profile — what is installed on the machine (a different thing from the level)
    machine: String,
    /// Claude Code CLI found on this machine (its path; empty — none: "What do you want to do" is not available)
    claude: String,
}

/// Claude Code CLI: on PATH, or where its installers put it (native: ~/.local/bin, npm: %APPDATA%/npm).
fn claude_cli() -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    if let Some(h) = std::env::var_os("USERPROFILE") {
        dirs.push(PathBuf::from(h).join(".local").join("bin"));
    }
    if let Some(a) = std::env::var_os("APPDATA") {
        dirs.push(PathBuf::from(a).join("npm"));
    }
    dirs.iter()
        .flat_map(|d| ["claude.exe", "claude.cmd"].map(|n| d.join(n)))
        .find(|p| p.is_file())
}

#[tauri::command]
fn me(state: State<AppState>) -> R<MeDto> {
    let ws = workspace(&state)?;
    let get = |k: &str| ws.local_str("me", k).unwrap_or_default();
    Ok(MeDto {
        level: get("level"),
        lang: get("lang"),
        detail: get("detail"),
        machine: ws_core::bootstrap::choose_profile(&ws, None),
        claude: claude_cli()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    })
}

#[tauri::command]
fn set_me(state: State<AppState>, level: String, lang: String, detail: String) -> R<()> {
    let ws = workspace(&state)?;
    if !LEVELS.contains(&level.as_str())
        || !LANGS.contains(&lang.as_str())
        || !DETAILS.contains(&detail.as_str())
    {
        return Err("unknown level, language or detail".into());
    }
    let local = ws.config("local.toml");
    if !local.is_file() {
        return Err("no config/local.toml - run Overview, Fix first".into());
    }
    for (k, v) in [("level", &level), ("lang", &lang), ("detail", &detail)] {
        ws_core::bootstrap::set_local_line(&local, "me", k, &format!("{k} = \"{v}\""))?;
    }
    Ok(())
}

/// UTC time as YYYY-MM-DD-HHMM (file names of tasks and reports sort by time).
fn stamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // civil date from days since 1970-01-01 (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    format!(
        "{y:04}-{m:02}-{d:02}-{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60
    )
}

fn slug(text: &str) -> String {
    let s: String = text
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let s: Vec<&str> = s.split('-').filter(|w| !w.is_empty()).take(5).collect();
    let s = s.join("-");
    if s.is_empty() {
        "task".into()
    } else {
        s.chars().take(40).collect()
    }
}

/// A task for Claude Code: a task file in home/runs/agents (mode, role, the human, where the report goes) and a
/// terminal in the workspace with `claude` that is told to read it. Returns the task file.
#[tauri::command]
fn start_agent(state: State<AppState>, mode: String, role: String, task: String) -> R<String> {
    let ws = workspace(&state)?;
    if !MODES.contains(&mode.as_str()) || !(role.is_empty() || ROLES.contains(&role.as_str())) {
        return Err("unknown mode or role".into());
    }
    let claude = claude_cli().ok_or("Claude Code CLI is not installed on this machine")?;
    if task.trim().is_empty() {
        return Err("describe the task".into());
    }
    let dir = ws.home().join("runs").join("agents");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!("{}-{}", stamp(), slug(&task));
    let file = dir.join(format!("{name}.task.md"));
    let report = dir.join(format!("{name}.md"));
    let get = |k: &str| ws.local_str("me", k).unwrap_or_default();
    let role_line = if role.is_empty() {
        "choose yourself (agents/ROLES.md)".to_string()
    } else {
        format!("{role} (.claude/agents/{role}.md)")
    };
    let text = format!(
        "# Task from DevKit ({})\n\n\
         - Mode: {mode} (agents/ROLES.md, \"Work modes\")\n\
         - Role: {role_line}\n\
         - The human: config/local.toml [me] - level {}, lang {}, detail {} (CLAUDE.md, \"Talking to the human\")\n\
         - Report: when done, also write the final report to {} (agents/ROLES.md, \"Reports to the human\")\n\n\
         ## Task\n\n{}\n",
        stamp(),
        if get("level").is_empty() { "dev".to_string() } else { get("level") },
        if get("lang").is_empty() { "-".to_string() } else { get("lang") },
        if get("detail").is_empty() { "normal".to_string() } else { get("detail") },
        report.display(),
        task.trim()
    );
    std::fs::write(&file, text).map_err(|e| e.to_string())?;
    let root = ws.root.display().to_string();
    let prompt = format!("Read the task file {} and do it.", file.display());
    spawn(
        "cmd",
        &[
            "/c",
            "start",
            "DevKit agent",
            "powershell",
            "-NoExit",
            "-Command",
            // by its full path: the installer's folder may not be on this window's PATH yet
            &format!("Set-Location '{root}'; & '{}' '{prompt}'", claude.display()),
        ],
        Some(&ws.root),
    )?;
    Ok(file.display().to_string())
}

#[derive(Serialize)]
pub struct BranchDto {
    name: String,
    when: String,
    subject: String,
}

#[derive(Serialize)]
pub struct ReportDto {
    file: String,
    title: String,
    text: String,
    /// a task file without a report yet: the agent is working or stopped
    pending: bool,
}

#[derive(Serialize)]
pub struct WorkDto {
    branch: String,
    head: String,
    changes: Vec<String>,
    branches: Vec<BranchDto>,
    reports: Vec<ReportDto>,
    /// how to see the result: [preview] of the members (ws preview, docs/PREVIEW.md)
    previews: Vec<PreviewDto>,
}

#[derive(Serialize)]
pub struct PreviewDto {
    name: String,
    member: String,
    url: String,
    note: String,
}

#[tauri::command]
fn work(state: State<AppState>) -> R<WorkDto> {
    let ws = workspace(&state)?;
    let changes: Vec<String> = git(&ws.root, &["status", "--porcelain"])
        .lines()
        .filter(|l| !l.is_empty())
        .take(40)
        .map(String::from)
        .collect();
    let branches = git(
        &ws.root,
        &[
            "for-each-ref",
            "--sort=-committerdate",
            "--count=12",
            "--format=%(refname:short)\t%(committerdate:relative)\t%(subject)",
            "refs/heads",
        ],
    )
    .lines()
    .filter_map(|l| {
        let mut it = l.splitn(3, '\t');
        Some(BranchDto {
            name: it.next()?.to_string(),
            when: it.next()?.to_string(),
            subject: it.next().unwrap_or("").to_string(),
        })
    })
    .collect();
    let dir = ws.home().join("runs").join("agents");
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().to_string())
                .filter(|n| n.ends_with(".md"))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names.reverse();
    let mut reports = Vec::new();
    for n in names.iter().filter(|n| !n.ends_with(".task.md")).take(20) {
        let text = std::fs::read_to_string(dir.join(n)).unwrap_or_default();
        let title = text
            .lines()
            .map(|l| l.trim_start_matches('#').trim())
            .find(|l| !l.is_empty())
            .unwrap_or(n)
            .to_string();
        reports.push(ReportDto {
            file: n.clone(),
            title,
            text: text.chars().take(20_000).collect(),
            pending: false,
        });
    }
    // tasks started from DevKit that have no report yet
    for n in names.iter().filter(|n| n.ends_with(".task.md")).take(10) {
        let report = n.trim_end_matches(".task.md").to_string() + ".md";
        if !names.contains(&report) {
            let text = std::fs::read_to_string(dir.join(n)).unwrap_or_default();
            let title = text
                .split("## Task")
                .nth(1)
                .and_then(|t| t.lines().map(str::trim).find(|l| !l.is_empty()))
                .unwrap_or(n)
                .to_string();
            reports.push(ReportDto {
                file: n.clone(),
                title,
                text,
                pending: true,
            });
        }
    }
    Ok(WorkDto {
        branch: git(&ws.root, &["rev-parse", "--abbrev-ref", "HEAD"]),
        head: git(&ws.root, &["log", "-1", "--format=%h %s"]),
        changes,
        branches,
        reports,
        previews: ws_core::preview::list(&ws)
            .0
            .into_iter()
            .map(|p| PreviewDto {
                url: p.url(),
                name: p.name,
                member: p.member,
                note: p.note,
            })
            .collect(),
    })
}

/// "Open the result": `cargo ws preview <name> --open` in a terminal of its own (the server runs there until Ctrl+C or
/// the window is closed; the browser opens once it answers). It shows the current checkout — the branch on the page.
#[tauri::command]
fn open_preview(state: State<AppState>, name: String) -> R<()> {
    let ws = workspace(&state)?;
    let p = ws_core::preview::find(&ws, &name).ok_or_else(|| format!("no [preview] '{name}'"))?;
    let root = ws.root.display().to_string();
    let env = ws.home().join("env.ps1");
    let env = if env.is_file() {
        format!(". '{}'; ", env.display())
    } else {
        String::new()
    };
    spawn(
        "cmd",
        &[
            "/c",
            "start",
            &format!("Preview {}", p.name),
            "powershell",
            "-NoExit",
            "-Command",
            &format!(
                "{env}Set-Location '{root}'; cargo ws preview {} --open",
                p.name
            ),
        ],
        Some(&ws.root),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_and_stamp() {
        assert_eq!(
            slug("Fix the Cyclops sword drop!"),
            "fix-the-cyclops-sword-drop"
        );
        assert_eq!(slug("Сделай вики"), "task");
        let s = stamp();
        assert_eq!(s.len(), 15, "{s}");
        assert!(s.starts_with("20"), "{s}");
    }
}

// ------------------------------------------------------------------------------------------- vault session
// The same vault session as `ws secrets unlock` (ws_core::vault, docs/SECRETS.md): the human types the master password
// once, a background agent (a copy of this exe started with --vault-agent) keeps the session secrets in memory, and
// both DevKit and ws.exe ask it — each secret only for the commands in its session_for. A secret reaches ws-core
// through the environment of DevKit's own process for the duration of one action (SecretEnv), the way `ws secrets run`
// passes it to a command. Never written to disk, never shown, never given to an agent.

/// The session values of `names` for the action `cmd` ("ws dump"); the ones the session refuses are left out.
fn session_values(ws: &Workspace, names: &[&str], cmd: &[&str]) -> Vec<(String, String, String)> {
    let cmd: Vec<String> = cmd.iter().map(|c| c.to_string()).collect();
    let Some(st) = ws_core::vault::status(ws) else {
        return Vec::new();
    };
    names
        .iter()
        .filter(|n| {
            st.secrets
                .iter()
                .any(|(s, p)| s == *n && ws_core::vault::command_allowed(p, &cmd))
        })
        .filter_map(|n| ws_core::vault::get(ws, &[n.to_string()], &cmd).ok())
        .flatten()
        .map(|i| (i.name, i.env, i.value))
        .collect()
}

/// Sets the secrets as environment variables of this process and removes them when dropped.
struct SecretEnv(Vec<String>);

impl SecretEnv {
    fn set(values: &[(String, String, String)]) -> SecretEnv {
        let mut names = Vec::new();
        for (_, env, v) in values {
            std::env::set_var(env, v);
            names.push(env.clone());
        }
        SecretEnv(names)
    }
}

impl Drop for SecretEnv {
    fn drop(&mut self) {
        for n in &self.0 {
            std::env::remove_var(n);
        }
    }
}

#[derive(Serialize)]
pub struct VaultDto {
    unlocked: bool,
    minutes_left: u64,
    idle_minutes_left: u64,
    names: Vec<String>,
}

fn vault_dto(ws: Option<&Workspace>) -> VaultDto {
    match ws.and_then(ws_core::vault::status) {
        Some(st) => VaultDto {
            unlocked: true,
            minutes_left: st.left.div_ceil(60),
            idle_minutes_left: st.idle_left.div_ceil(60),
            names: st.names(),
        },
        None => VaultDto {
            unlocked: false,
            minutes_left: 0,
            idle_minutes_left: 0,
            names: Vec::new(),
        },
    }
}

#[tauri::command]
async fn vault_state(state: State<'_, AppState>) -> R<VaultDto> {
    let ws = workspace(&state).ok();
    tauri::async_runtime::spawn_blocking(move || vault_dto(ws.as_ref()))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn vault_lock(state: State<'_, AppState>) -> R<VaultDto> {
    let ws = workspace(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        ws_core::vault::lock(&ws)?;
        Ok(vault_dto(Some(&ws)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn vault_unlock(state: State<'_, AppState>, password: String, minutes: u64) -> R<VaultDto> {
    let ws = workspace(&state)?;
    let for_secs = minutes.clamp(5, ws_core::vault::MAX_FOR / 60) * 60;
    tauri::async_runtime::spawn_blocking(move || -> R<VaultDto> {
        let m = ws_core::secrets::manifest(&ws)?;
        let path = ws_core::secrets::db_path(&ws, &m, None)?;
        let tier = ws_core::secrets::local_tier(&ws)?;
        let src = ws_core::secrets::Source::open_kdbx(&path, &password)?;
        let u = ws_core::vault::Unlock {
            for_secs,
            idle_secs: ws_core::vault::DEFAULT_IDLE.min(for_secs),
            only: Vec::new(),
            agent_args: vec!["--vault-agent".into()],
        };
        ws_core::vault::unlock(&ws, &m, &src, &tier, &u)?;
        Ok(vault_dto(Some(&ws)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// `DevKit.exe --vault-agent --root=<workspace> --parent=<pid>`: the agent of the vault session, no window (main.rs
/// calls this before Tauri starts). true — it was the agent and has finished.
pub fn vault_agent() -> bool {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.iter().any(|a| a == "--vault-agent") {
        return false;
    }
    let opt = |k: &str| {
        args.iter()
            .find_map(|a| a.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
    };
    let ws = opt("--root")
        .map(PathBuf::from)
        .and_then(|r| Workspace::open(&r).ok());
    let parent = opt("--parent").and_then(|p| p.parse().ok());
    if let (Some(ws), Some(parent)) = (ws, parent) {
        let _ = ws_core::vault::serve(&ws, parent);
    }
    true
}

// ------------------------------------------------------------------------------------------- game data
// The game reference (ws game) and the dumps (ws dump) — docs/GAME_DATA.md. Buttons for the steps only a human may
// do: the Steam download (their account), decrypting the paks (their word).

#[derive(Serialize)]
pub struct DumpSetDto {
    name: String,
    ok: usize,
    skip: usize,
    error: usize,
}

#[derive(Serialize)]
pub struct GameDto {
    /// references: (build, path, note)
    references: Vec<(String, String, String)>,
    /// the Steam command for the first build of config/game.toml
    steam_command: String,
    steam_download: String,
    steam_download_ready: bool,
    /// the game installed by Steam (empty — none found) and whether its clean files can be copied as the reference
    installed: String,
    installed_ok: bool,
    /// what ws says about the missing reference (ws game), shown when the install cannot be taken
    reference_note: String,
    build: String,
    dump_dir: String,
    dump_files: usize,
    dump_when: String,
    sets: Vec<DumpSetDto>,
}

#[tauri::command]
fn game(state: State<AppState>) -> R<GameDto> {
    let ws = workspace(&state)?;
    let builds = ws_core::game::builds(&ws);
    let b = builds.first().cloned().unwrap_or_default();
    let root = ws_core::game::original_root(&ws);
    let references: Vec<(String, String, String)> = ws_core::game::status(&ws)
        .into_iter()
        .filter(|f| matches!(f.level, Level::Info))
        .map(|f| {
            let build = std::path::Path::new(&f.path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            (build, f.path, f.msg)
        })
        .collect();
    let dl = ws_core::game::steam_download(&b);
    let ready = dl.as_ref().is_some_and(|p| {
        p.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
            .is_file()
    });
    // a normal Steam install: copied (only the clean build's files) when it is the right build and unchanged
    let inst = ws_core::game::installed(&b);
    let installed_ok = inst.as_ref().is_some_and(|i| {
        i.buildid == b.id
            && ws_core::game::clean_files(&ws, &b.id)
                .is_some_and(|c| ws_core::game::check_clean(&i.path, &c).is_ok())
    });
    let reference_note = if references.is_empty() && !b.id.is_empty() {
        ws_core::game::missing_reference(&ws, &b, false).1
    } else {
        String::new()
    };
    let build = references
        .first()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| b.id.clone());
    let dumps = ws.home().join("dumps").join(&build);
    let manifest = dumps.join("manifest.tsv");
    let dump_files = std::fs::read_to_string(&manifest)
        .map(|t| t.lines().filter(|l| !l.is_empty()).count())
        .unwrap_or(0);
    let dump_when = std::fs::metadata(&manifest)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| format!("{}", d.as_secs() / 3600))
        .unwrap_or_default();
    let mut sets = Vec::new();
    if let Ok(sets_def) = ws_core::dump::sets(&ws) {
        for s in sets_def {
            let text = std::fs::read_to_string(dumps.join("index").join(format!("{}.tsv", s.name)))
                .unwrap_or_default();
            let count = |p: &str| {
                text.lines()
                    .filter(|l| l.split('\t').nth(2).is_some_and(|r| r.starts_with(p)))
                    .count()
            };
            sets.push(DumpSetDto {
                name: s.name,
                ok: count("ok"),
                skip: count("skip"),
                error: count("error"),
            });
        }
    }
    let _ = root;
    Ok(GameDto {
        references,
        steam_command: if b.id.is_empty() {
            String::new()
        } else {
            format!("download_depot {} {} {}", b.app, b.depot, b.manifest)
        },
        steam_download: dl.map(|p| p.display().to_string()).unwrap_or_default(),
        steam_download_ready: ready,
        installed: inst
            .as_ref()
            .map(|i| i.path.display().to_string())
            .unwrap_or_default(),
        installed_ok,
        reference_note,
        build,
        dump_dir: dumps.display().to_string(),
        dump_files,
        dump_when,
        sets,
    })
}

/// steam_console | register | dump_plan | dump
#[tauri::command]
async fn game_action(state: State<'_, AppState>, action: String) -> R<Vec<Item>> {
    let ws = workspace(&state)?;
    let secrets = if action == "dump" {
        session_values(&ws, &["game_aes_key"], &["ws", "dump"])
    } else {
        Vec::new()
    };
    match action.as_str() {
        "steam_console" => {
            spawn("cmd", &["/c", "start", "", "steam://open/console"], None)?;
            Ok(Vec::new())
        }
        // the reference from what is there: a Steam install (its clean files are copied) or a download_depot download
        "take" => {
            let b = ws_core::game::builds(&ws)
                .first()
                .cloned()
                .ok_or("no build in config/game.toml")?;
            tauri::async_runtime::spawn_blocking(move || {
                let (level, msg) = ws_core::game::missing_reference(&ws, &b, true);
                if matches!(level, Level::Done) {
                    Ok(vec![Item {
                        level: "done",
                        rule: "game".into(),
                        path: "reference".into(),
                        msg,
                    }])
                } else {
                    Err(msg)
                }
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "register" => {
            let b = ws_core::game::builds(&ws)
                .first()
                .cloned()
                .ok_or("no build in config/game.toml")?;
            let src = ws_core::game::steam_download(&b).ok_or("Steam is not installed")?;
            let dst = ws_core::game::original_root(&ws).join(&b.id);
            tauri::async_runtime::spawn_blocking(move || {
                ws_core::game::adopt_original(&src, &dst, &b.id, &b.depot, &b.manifest, true).map(
                    |msg| {
                        vec![Item {
                            level: "done",
                            rule: "game".into(),
                            path: dst.display().to_string(),
                            msg,
                        }]
                    },
                )
            })
            .await
            .map_err(|e| e.to_string())?
        }
        "dump_plan" | "dump" => {
            let apply = action == "dump";
            if apply
                && !secrets.iter().any(|(n, _, _)| n == "game_aes_key")
                && ws.local_str("re", "aes_key").is_none()
            {
                return Err("open the vault session first (the pak key)".into());
            }
            tauri::async_runtime::spawn_blocking(move || {
                let _env = SecretEnv::set(&secrets);
                items(ws_core::dump::run(
                    &ws,
                    &ws_core::dump::Options {
                        apply,
                        build: None,
                        sets: Vec::new(),
                    },
                ))
            })
            .await
            .map_err(|e| e.to_string())
        }
        _ => Err("unknown action".into()),
    }
}

#[derive(Serialize)]
pub struct SecretDto {
    name: String,
    place: String,
    kind: String,
    target: String,
    env: String,
    used_by: Vec<String>,
    description: String,
    required: bool,
    rotate: bool,
}

#[tauri::command]
fn secrets_list(state: State<AppState>) -> R<(String, Vec<SecretDto>)> {
    let ws = workspace(&state)?;
    let m = ws_core::secrets::manifest(&ws)?;
    let tier = ws_core::secrets::local_tier(&ws)?;
    let db = ws_core::secrets::db_path(&ws, &m, None)
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let list = m
        .secrets
        .iter()
        .map(|s| SecretDto {
            name: s.name.clone(),
            place: format!("{} / {}", s.group, s.title),
            target: if s.kind == "file" {
                s.file.clone()
            } else {
                s.field.clone()
            },
            kind: s.kind.clone(),
            env: s.env.clone(),
            used_by: s.used_by.clone(),
            description: s.description.clone(),
            // an entry above the database tier of this machine (local.toml [vault] tier) is not needed here
            required: s.required && s.allowed(&tier),
            rotate: s.rotate,
        })
        .collect();
    Ok((db, list))
}

/// The database password comes from the page and lives only for the check; secret values are never handed out.
#[tauri::command]
async fn secrets_check(state: State<'_, AppState>, password: String) -> R<Vec<Item>> {
    let ws = workspace(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let m = ws_core::secrets::manifest(&ws)?;
        let path = ws_core::secrets::db_path(&ws, &m, None)?;
        let tier = ws_core::secrets::local_tier(&ws)?;
        let src = ws_core::secrets::Source::open_kdbx(&path, &password)?;
        Ok(items(ws_core::secrets::check(&m, &src, &tier)))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// PC backup: ops/backup/pc/backup.ps1 [-Snapshots]. Output as text (lines with "password" are removed).
#[tauri::command]
async fn backup(state: State<'_, AppState>, action: String) -> R<String> {
    let ws = workspace(&state)?;
    let script = ws
        .root
        .join("ops")
        .join("backup")
        .join("pc")
        .join("backup.ps1");
    if !script.is_file() {
        return Err(format!("missing {}", script.display()));
    }
    let flag = match action.as_str() {
        "snapshots" => "-Snapshots",
        "run" => "",
        _ => return Err("unknown action".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let cmd = format!(
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; & '{}' {}",
            script.display(),
            flag
        );
        let out = Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &cmd])
            .output()
            .map_err(|e| e.to_string())?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let text: String = text
            .lines()
            .filter(|l| !l.to_ascii_lowercase().contains("password"))
            .collect::<Vec<_>>()
            .join("\n");
        if out.status.success() {
            Ok(text)
        } else {
            Err(text)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn spawn(exe: &str, args: &[&str], cwd: Option<&Path>) -> R<()> {
    let mut c = Command::new(exe);
    c.args(args);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    c.spawn().map(|_| ()).map_err(|e| format!("{exe}: {e}"))
}

fn first_existing(cands: &[PathBuf]) -> Option<PathBuf> {
    cands.iter().find(|p| p.is_file()).cloned()
}

/// Open a folder, a program or a page. The set is fixed — the page does not pass paths or commands.
#[tauri::command]
fn open(state: State<AppState>, what: String) -> R<()> {
    let ws = workspace(&state)?;
    let home = ws.home();
    match what.as_str() {
        "workspace" => spawn("explorer", &[&ws.root.display().to_string()], None),
        "home" => spawn("explorer", &[&home.display().to_string()], None),
        "vault" => {
            let v = ws
                .local_str("vault", "path")
                .and_then(|p| ws.resolve(&p))
                .ok_or("no local.toml [vault] path")?;
            spawn("explorer", &[&v.display().to_string()], None)
        }
        "studio" => {
            let target = std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from);
            let mut c = vec![
                ws.root
                    .join("apps/studio/target/release/balance-studio.exe"),
                home.join("cache/target/release/balance-studio.exe"),
            ];
            c.extend(target.map(|t| t.join("release/balance-studio.exe")));
            let exe = first_existing(&c)
                .ok_or("Balance Studio is not built: cargo tauri build in apps/studio")?;
            spawn(&exe.display().to_string(), &[], None)
        }
        "keepass" => {
            let mut c: Vec<PathBuf> = ws
                .local_str("tools", "keepassxc")
                .and_then(|p| ws.resolve(&p))
                .into_iter()
                .collect();
            for base in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
                if let Some(b) = std::env::var_os(base) {
                    c.push(PathBuf::from(&b).join("KeePassXC/KeePassXC.exe"));
                    c.push(PathBuf::from(&b).join("Programs/KeePassXC/KeePassXC.exe"));
                }
            }
            let exe = first_existing(&c)
                .ok_or("KeePassXC not found (winget install KeePassXCTeam.KeePassXC)")?;
            let m = ws_core::secrets::manifest(&ws)?;
            let db = ws_core::secrets::db_path(&ws, &m, None)?;
            spawn(
                &exe.display().to_string(),
                &[&db.display().to_string()],
                None,
            )
        }
        "forgejo" => spawn(
            "rundll32",
            &["url.dll,FileProtocolHandler", "http://localhost:3300/"],
            None,
        ),
        "agent" => {
            // a terminal in the workspace root with Claude Code (CLAUDE.md is there too)
            let root = ws.root.display().to_string();
            spawn(
                "cmd",
                &[
                    "/c",
                    "start",
                    "DevKit agent",
                    "powershell",
                    "-NoExit",
                    "-Command",
                    &format!("Set-Location '{root}'; claude"),
                ],
                Some(&ws.root),
            )
        }
        _ => Err("unknown target".into()),
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            root: Mutex::new(find_root()),
        })
        .invoke_handler(tauri::generate_handler![
            info,
            pick_root,
            doctor,
            check,
            members,
            components,
            components_set,
            me,
            set_me,
            vault_state,
            vault_lock,
            vault_unlock,
            game,
            game_action,
            start_agent,
            open_preview,
            work,
            secrets_list,
            secrets_check,
            backup,
            open
        ])
        .run(tauri::generate_context!())
        .expect("devkit");
}
