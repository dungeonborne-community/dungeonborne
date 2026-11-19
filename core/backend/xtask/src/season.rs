// Seasons: seasons/<id>/ — an overlay on top of the framework base. Balance table layers (bottom up):
//   1. game original   assets/excel/ExcelConfig/*.bytes  (not edited)
//   2. server base     balance/*.json                    (all tables; edits shared by all seasons)
//   3. season          seasons/<id>/balance/*.json       (only the tables the season changes, whole)
// Effective table = the season one if it exists, else the base one.
//
// Active season — seasons/active.txt (`cargo xtask season use <id|none>`). The commands balance build / kit / patch and
// loot build / show use it by default; --season=<id> — another season, --season=none — base only.
use std::path::{Path, PathBuf};

pub const SEASONS: &str = crate::ws::SEASONS;
const ACTIVE: &str = "content/seasons/active.txt";
/// where "base + season" is assembled for balance build (the cargo target folder — temporary)
const MERGED: &str = "core/backend/target/xtask_season"; // temporary tables (xtask runs from the workspace root)

pub fn dir(id: &str) -> PathBuf {
    Path::new(SEASONS).join(id)
}

pub fn balance_dir(id: &str) -> PathBuf {
    dir(id).join("balance")
}

/// Seasons — folders seasons/<id>/
pub fn list() -> Vec<String> {
    let mut v: Vec<String> = std::fs::read_dir(SEASONS)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

pub fn active() -> Option<String> {
    let s = std::fs::read_to_string(ACTIVE).ok()?;
    let s = s.trim().trim_start_matches('\u{feff}').to_string();
    (!s.is_empty() && s != "none").then_some(s)
}

/// Season of the command: --season=<id> | --season=none | no flag — the active one
pub fn resolve(flag: Option<&str>) -> Result<Option<String>, String> {
    let id = match flag {
        Some("none") | Some("") => return Ok(None),
        Some(id) => id.to_string(),
        None => match active() {
            Some(id) => id,
            None => return Ok(None),
        },
    };
    if !dir(&id).is_dir() {
        return Err(format!("no season {} (no folder {}); available: {}", id, dir(&id).display(), list().join(", ")));
    }
    Ok(Some(id))
}

pub fn label(season: Option<&str>) -> String {
    match season {
        Some(id) => format!("season {}", id),
        None => "no season (base)".into(),
    }
}

/// The season table if it exists, else the base one
pub fn table_path(base: &str, season: Option<&str>, table: &str) -> PathBuf {
    if let Some(id) = season {
        let p = balance_dir(id).join(format!("{}.json", table));
        if p.exists() {
            return p;
        }
    }
    Path::new(base).join(format!("{}.json", table))
}

/// Season tables: name -> file (no balance folder — empty)
fn season_tables(id: &str) -> std::collections::BTreeMap<String, PathBuf> {
    let d = balance_dir(id);
    if d.is_dir() {
        crate::tables::list_tables(&d.to_string_lossy(), "json")
    } else {
        Default::default()
    }
}

/// Tables the season overrides
pub fn overrides(season: Option<&str>) -> Vec<String> {
    season.map(|id| season_tables(id).into_keys().collect()).unwrap_or_default()
}

/// Folder with the effective tables: without a season — the base itself; with a season — a copy of the base with the season tables on top
pub fn merged(base: &str, season: Option<&str>) -> Result<PathBuf, String> {
    let Some(id) = season else { return Ok(PathBuf::from(base)) };
    let out = Path::new(MERGED).join(id);
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {}", out.display(), e))?;
    for (name, p) in crate::tables::list_tables(base, "json") {
        std::fs::copy(&p, out.join(format!("{}.json", name))).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    for (name, p) in season_tables(id) {
        if !Path::new(base).join(format!("{}.json", name)).exists() {
            return Err(format!("{}: table {} is not in the base {} — a season only overrides game tables", p.display(), name, base));
        }
        std::fs::copy(&p, out.join(format!("{}.json", name))).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    Ok(out)
}

/// cargo xtask season [use <id|none> | new <id> [--title=…] [--loot]]
pub fn command(args: &[&str], title: Option<&str>, with_loot: bool) -> bool {
    match args {
        [] => {
            let act = active();
            println!("active season: {} ({})", act.as_deref().unwrap_or("none — base only"), ACTIVE);
            for id in list() {
                let ov = overrides(Some(&id));
                println!(
                    "  {}{:<20} balance tables on top of the base: {}{}",
                    if act.as_deref() == Some(id.as_str()) { "* " } else { "  " },
                    id,
                    ov.len(),
                    if ov.is_empty() { String::new() } else { format!(" ({})", ov.join(", ")) }
                );
            }
            println!("to switch: cargo xtask season use <id|none>, then cargo xtask balance build");
            true
        }
        ["use", id] => {
            let id = if *id == "none" { None } else { Some(*id) };
            if let Some(id) = id {
                if !dir(id).is_dir() {
                    println!("no season {}; available: {}", id, list().join(", "));
                    return false;
                }
            }
            std::fs::create_dir_all(SEASONS).unwrap();
            std::fs::write(ACTIVE, format!("{}\n", id.unwrap_or("none"))).unwrap();
            println!("active: {} -> {}", label(id), ACTIVE);
            println!("next: cargo xtask balance build [--deploy] — server and client pak for this season");
            true
        }
        ["new", id] => new_season(id, title, with_loot),
        _ => {
            println!(
                "cargo xtask season                                  — seasons and the active one\n\
                 cargo xtask season use <id|none>                     — switch the active one\n\
                 cargo xtask season new <id> [--title=…] [--loot]     — a new season (--loot — its own loot template from the base)"
            );
            false
        }
    }
}

/// Season skeleton: seasons/<id>/ with balance/ (empty — everything from the base), kit.toml, README.md; --loot — a copy of the base loot template
fn new_season(id: &str, title: Option<&str>, with_loot: bool) -> bool {
    let ok_id = !id.is_empty() && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !matches!(id, "none" | "active");
    if !ok_id {
        println!("season id — lowercase Latin letters, digits and '-' (for example winter-2026); not none/active");
        return false;
    }
    let d = dir(id);
    if d.exists() {
        println!("{} already exists", d.display());
        return false;
    }
    // the season's own name: --title, else its id (the kit title below has its own default)
    let name = title.map(str::to_string).unwrap_or_else(|| id.to_string());
    let title = title.map(str::to_string).unwrap_or_else(|| format!("Dungeonborne — баланс сезона {}", id));
    std::fs::create_dir_all(balance_dir(id)).unwrap();
    let nl = "\r\n";
    let kit = [
        format!("# Balance kit of season {} — extends the shared balance_kit/kit.toml.", id),
        format!("# Build: cargo xtask balance kit --season={}  ->  seasons/{}/kit/{}-balance-<version>/", id, id, id),
        "# Here: own title, notes (added to the shared ones) and [[table]] (with the same name — changes table fields, with a new one — adds).".into(),
        String::new(),
        format!("title = {:?}", title),
        String::new(),
        "notes = [".into(),
        "  # \"How the season differs from the usual game — players and the LLM will see it on the first sheet of the book.\",".into(),
        "]".into(),
        String::new(),
    ]
    .join(nl);
    std::fs::write(d.join("kit.toml"), kit).unwrap();
    // 18.11: without season.toml the backend (SEASON=<id>) does not load the season and runs the base game
    let season_toml = [
        format!("# Season {:?} (docs/SEASONS_FORMAT.md). Backend: SEASON={}.", name, id),
        format!("id = {:?}", id),
        format!("name = {:?}   # a string or {{ ru = \"...\", en = \"...\" }}", name),
        "plugin = \"\"   # season modules are not supported yet: only core flags".into(),
        format!("state_key = {:?}", id),
        String::new(),
        "[features]".into(),
        "# empty: every flag from content/seasons/base.toml; set here only what this season changes".into(),
        "# (flags: docs/SEASONS_FORMAT.md; an unknown key is an error and the season is not loaded)".into(),
        String::new(),
        "[server]".into(),
        "maps = []   # battle maps as before (DS_POOL)".into(),
        String::new(),
    ]
    .join(nl);
    std::fs::write(d.join("season.toml"), season_toml).unwrap();
    let mut files = vec![
        "| File | What it is |".to_string(),
        "|---|---|".into(),
        "| `balance\\*.json` | season balance tables on top of the base `backups\\balance\\` — only those the season changes, whole |".into(),
        "| `season.toml` | id, name, flags on top of `base.toml` (`docs/SEASONS_FORMAT.md`); the backend loads it with `SEASON=<id>` |".into(),
        "| `kit.toml` | balance kit for players (`cargo xtask balance kit` → `kit\\`) |".into(),
    ];
    if with_loot {
        let to = d.join("loot");
        std::fs::create_dir_all(&to).unwrap();
        std::fs::copy(crate::loot::TEMPLATE, to.join("loot.toml")).unwrap();
        files.push("| `loot\\loot.toml` | season loot template (a copy of the base at creation time) → `cargo xtask loot build` |".into());
    }
    files.push("| `pak\\`, `server\\ExcelConfig\\` | result of `cargo xtask balance build` with this season: the pak for players and the server tables |".into());
    let readme = [
        format!("# Season {}", id),
        String::new(),
        title.clone(),
        String::new(),
        "## Season files (this folder)".into(),
        String::new(),
        files.join(nl),
        String::new(),
        "## How to edit".into(),
        String::new(),
        format!("- enable: `cargo xtask season use {}`, then the commands work with this season;", id),
        "- tables in CSV: `cargo xtask balance csv <table>` → `balance_csv\\` here, edit, `cargo xtask balance csv-import`".into(),
        "  (only the tables that the CSV really changes get into the season's `balance\\`);".into(),
        "- kit for players: `cargo xtask balance kit`; their patches: `cargo xtask balance patch <patch>`;".into(),
        "- build: `cargo xtask balance build [--deploy]`. Details — `backups\\BALANCE.md`, \"Seasons\".".into(),
        String::new(),
    ]
    .join(nl);
    std::fs::write(d.join("README.md"), readme).unwrap();
    println!("season {} -> {}", id, d.display());
    println!("  balance\\   — empty: all tables from the base until the season changes them");
    println!("  season.toml — id, name, flags on top of base.toml (empty [features])");
    println!("  kit.toml   — name and notes of the kit for players");
    if with_loot {
        println!("  loot\\loot.toml — a copy of the base loot template");
    }
    println!("next: cargo xtask season use {} — and edit (the season README.md)", id);
    true
}
