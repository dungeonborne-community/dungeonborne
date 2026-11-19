//! Workspace paths (docs/WORKSPACE_PLAN.md). xtask runs from the workspace root — the folder with workspace.toml;
//! all paths below are relative to that root. Paths of this machine (game installations, programs, DB_HOME) are in local.toml
//! of the root (not in git, example local.example.toml).
use std::path::{Path, PathBuf};

// --- data edited by people (content/)
pub const SEASONS: &str = "content/seasons";
pub const BALANCE: &str = "content/balance/tables";
pub const BALANCE_CSV: &str = "content/balance/csv";
pub const KIT_DIR: &str = "content/balance/kit";
pub const LOOT_TEMPLATE: &str = "content/loot/loot.toml";
pub const NAMES: &str = "content/names";
pub const MATCH_CHECKS: &str = "content/match_checks.toml";

// --- server data (core/backend)
pub const SERVER: &str = "core/backend/ExcelConfig";
pub const ORIGINALS: &str = "core/backend/assets/excel/ExcelConfig";
pub const SCHEMA: &str = "core/backend/Configure.pb"; // table schemas (protobuf)

// --- mods and paks (game/)
pub const PAK_STAGE: &str = "game/paks/balance";
pub const MODS_CATALOG: &str = "game/catalog/catalog.toml";

/// Workspace root: XTASK_ROOT (acceptance tests work with a copy), else three levels above xtask/ (core/backend/xtask).
pub fn root() -> PathBuf {
    std::env::var_os("XTASK_ROOT").map(PathBuf::from).unwrap_or_else(|| {
        Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(3).expect("workspace root").to_path_buf()
    })
}

/// config/local.toml of the root (empty if there is no file). Read relative to the current folder — after set_current_dir(root()).
pub fn local() -> toml::Value {
    std::fs::read_to_string("config/local.toml").ok().and_then(|t| t.parse().ok()).unwrap_or(toml::Value::Table(Default::default()))
}

/// DB_HOME: the DB_HOME variable, else local.toml [home].path, else ../home next to the workspace.
pub fn home() -> PathBuf {
    if let Some(h) = std::env::var_os("DB_HOME").filter(|h| !h.is_empty()) {
        return PathBuf::from(h);
    }
    local().get("home").and_then(|h| h.get("path")).and_then(|v| v.as_str()).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("../home"))
}

/// Path from the catalog/settings: "home:<path>" — inside DB_HOME, else as is (from the workspace root or absolute).
pub fn resolve(p: &str) -> PathBuf {
    match p.strip_prefix("home:") {
        Some(rest) => home().join(rest),
        None => PathBuf::from(p),
    }
}

/// A program from local.toml [tools] (lua, repak); not there — where the kit installs it (config/tools.lock, DB_HOME/tools);
/// only if the file exists.
pub fn tool(name: &str) -> Option<PathBuf> {
    let set = local().get("tools").and_then(|t| t.get(name)).and_then(|v| v.as_str()).map(resolve);
    let default = match name {
        "lua" => Some(home().join("tools/lua/lua54.exe")),
        "repak" => Some(home().join("tools/repak/repak.exe")),
        _ => None,
    };
    set.filter(|p| p.is_file()).or(default.filter(|p| p.is_file()))
}

/// Game installations from local.toml [games]: name -> path.
pub fn games() -> Vec<(String, PathBuf)> {
    local()
        .get("games")
        .and_then(|g| g.as_table())
        .map(|t| t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), resolve(s)))).collect())
        .unwrap_or_default()
}
