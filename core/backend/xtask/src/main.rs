// cargo xtask — development tools. Run from any project folder (alias in .cargo/config.toml).
//
//   cargo xtask balance build [--deploy]     balance\*.json -> ExcelConfig\*.bytes (server) + zzzzzz_Balance_P.pak (client)
//   cargo xtask balance verify               self-check: .bytes -> JSON -> .bytes without changes
//   cargo xtask balance export               ExcelConfig -> balance (OVERWRITES balance\)
//   cargo xtask balance csv [table…] [--plain]         balance\*.json -> balance_csv\*.csv (column = field; all by default)
//   cargo xtask balance csv-long [table…] [--plain]    balance\*.json -> balance_csv\*.long.csv (row = one value)
//   cargo xtask balance csv-import [table…]            balance_csv\*.csv / *.long.csv -> balance\*.json (all by default)
//   cargo xtask balance names-import [file] [--lang=en]  localization from the game (NameDump mod, F6) -> names\strings.tsv (en: strings_en.tsv)
//   cargo xtask balance dump <.bytes folder> <JSON folder>  any tables (e.g. an unpacked mod pak) -> JSON
//   cargo xtask loot export [JSON folder] [--force] [--maps=all|0,10101,…]
//                                                    loot tables (balance by default) -> template loot\loot.toml
//   cargo xtask loot show [--map=10103]              average loot of each source group per the template
//   cargo xtask loot build [--deploy]                loot\loot.toml -> balance\loot*.json (+ balance build);
//                                                    a season with its own loot\loot.toml — its template -> seasons\<id>\balance\
//   cargo xtask balance kit [--out=folder] [--no-zip]  balance kit for players: Excel, LLM, Python + .zip (balance_kit\README.md)
//   cargo xtask balance patch <patch.json> [--dry-run]  players' edits (patch) -> tables of the patch's season (or balance\), checked against "was"
//   cargo xtask season [use <id|none> | new <id> [--title=…] [--loot]]  seasons (seasons\<id>\), active, new
//   cargo xtask mods build|check|install|verify [--profile=client|server|dev-client|dev-server] [--game=<name|path>]
//                                                    UE4SS mods and paks per game/catalog/catalog.toml (mods.rs, MODS_DESIGN.md)
//
// Season: build, kit, csv/csv-long/csv-import, loot build/show use the active season (seasons\active.txt); --season=<id> —
// another one, --season=none — base only. Season tables seasons\<id>\balance\*.json override balance\*.json (season.rs);
// season CSV — seasons\<id>\balance_csv\.
// Details — BALANCE.md.
mod csv_view;
mod kit;
mod loot;
mod loot_edit;
mod match_checks;
mod mods;
mod names;
mod season;
mod tables;
mod ws;

use std::path::{Path, PathBuf};
use std::process::Command;
use ws::{BALANCE, BALANCE_CSV, ORIGINALS, PAK_STAGE, SERVER};

const PAK_NAME: &str = "zzzzzz_Balance_P"; // 6×z — loads after zzzzz_RebuildPrivate_P

fn main() {
    // The root is the workspace (the folder with workspace.toml), all data paths are from it (ws.rs).
    // XTASK_ROOT — another folder with the same layout: acceptance applies the patch to a copy without touching the working tables.
    // Paths from arguments (patch file, --out, --patch) are relative to the launch folder, as usual in a console.
    let launch_dir = std::env::current_dir().unwrap_or_default();
    std::env::set_current_dir(ws::root()).unwrap();
    let user_path = |s: &str| -> String { launch_dir.join(s).to_string_lossy().into_owned() };

    let args: Vec<String> = std::env::args()
        .skip(1)
        .map(|a| match a.split_once('=') {
            Some((k @ ("--out" | "--patch"), v)) => format!("{}={}", k, user_path(v)),
            _ => a,
        })
        .collect();
    let flags: Vec<&str> = args.iter().filter(|a| a.starts_with("--")).map(|s| s.as_str()).collect();
    let pos: Vec<&str> = args.iter().filter(|a| !a.starts_with("--")).map(|s| s.as_str()).collect();
    // season of the command: --season=<id|none>, else the active one (seasons\active.txt)
    let season_flag = flag_value(&flags, "--season");
    let needs_season = matches!(
        pos.as_slice(),
        ["balance", "build"]
            | ["balance", "kit"]
            | ["balance", "export"]
            | ["balance", "csv", ..]
            | ["balance", "csv-long", ..]
            | ["balance", "csv-import", ..]
            | ["loot", "build"]
            | ["loot", "show"]
            | ["loot", "verify-view"]
    );
    let season = if needs_season {
        match season::resolve(season_flag) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}", e);
                std::process::exit(1);
            }
        }
    } else {
        None
    };
    if needs_season {
        println!("[{}]", season::label(season.as_deref()));
    }
    let season = season.as_deref();
    let ok = match pos.as_slice() {
        ["season", rest @ ..] => season::command(rest, flag_value(&flags, "--title"), flags.contains(&"--loot")),
        ["mods", rest @ ..] => mods::command(rest, flag_value(&flags, "--profile"), flag_value(&flags, "--game"), season_flag),
        ["balance", "build"] => balance_build_only(flags.contains(&"--deploy"), None, season),
        // export writes ExcelConfig (base + season) into the base — with a season the season tables would get into the shared base
        ["balance", "export"] if season.is_some() => {
            eprintln!("balance export would overwrite balance\\ with the server tables, and ExcelConfig also has the season built in — repeat with --season=none after balance build --season=none");
            false
        }
        ["balance", "verify"] => {
            let s = tables::Schema::load();
            tables::verify(&s, SERVER) & tables::verify(&s, ORIGINALS)
        }
        ["balance", "export"] => {
            tables::export(&tables::Schema::load(), SERVER, BALANCE);
            true
        }
        // any folder with .bytes (e.g. an unpacked mod pak) -> JSON, for comparison
        ["balance", "dump", src, out] => {
            tables::export(&tables::Schema::load(), &user_path(src), &user_path(out));
            true
        }
        ["loot", "export"] => loot::export(BALANCE, flags.contains(&"--force"), maps_flag(&flags)),
        ["loot", "export", from] => loot::export(from, flags.contains(&"--force"), maps_flag(&flags)),
        ["loot", "verify-view"] => loot::verify_view(season, flag_value(&flags, "--patch")),
        ["loot", "show"] => loot::show(season, flag_value(&flags, "--map").and_then(|v| v.parse().ok())),
        // only loot tables go into the pak: the server affix_pool is not given to the client yet (see BACKLOG)
        ["loot", "build"] => loot::build(season) && (!flags.contains(&"--deploy") || balance_build_only(true, Some(&loot::CLIENT_TABLES), season)),
        ["balance", "csv", names @ ..] => balance_csv(names, flags.contains(&"--plain"), false, season),
        ["balance", "csv-long", names @ ..] => balance_csv(names, flags.contains(&"--plain"), true, season),
        ["balance", "csv-import", names @ ..] => balance_csv_import(names, season),
        ["balance", "kit"] => kit::kit(BALANCE, season, flag_value(&flags, "--out"), !flags.contains(&"--no-zip")),
        // the patch season comes from the patch itself (the kit writes it); --season — only a cross-check
        ["balance", "patch", file] => kit::patch(BALANCE, &user_path(file), season_flag, flags.contains(&"--dry-run")),
        ["balance", "names-import"] => names_import(None, flag_value(&flags, "--lang")),
        ["balance", "names-import", file] => names_import(Some(&user_path(file)), flag_value(&flags, "--lang")),
        _ => {
            eprintln!("{}", include_str!("main.rs").lines().take(25).collect::<Vec<_>>().join("\n"));
            false
        }
    };
    if !ok {
        std::process::exit(1);
    }
}

/// only — which tables to put into the client pak (the server is built whole).
/// season — season tables on top of the base; the season result (the pak and .bytes differing from the original) is copied into seasons\<id>\
fn balance_build_only(deploy: bool, only: Option<&[&str]>, season: Option<&str>) -> bool {
    let s = tables::Schema::load();
    let src = match season::merged(BALANCE, season) {
        Ok(p) => p,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    if let Some(id) = season {
        let ov = season::overrides(Some(id));
        println!("season {} tables on top of the base: {}", id, if ov.is_empty() { "—".into() } else { ov.join(", ") });
    }
    let src = match match_checks::apply(&src) {
        Ok(p) => p,
        Err(e) => {
            println!("{}", e);
            return false;
        }
    };
    let stage = Path::new(PAK_STAGE).join(PAK_NAME);
    let _ = std::fs::remove_dir_all(&stage);
    let pak_dir = stage.join("Dungeonborne/Content/AdditionalData/ExcelConfig");
    let mut client = match tables::build(&s, &src.to_string_lossy(), SERVER, ORIGINALS, &pak_dir) {
        Ok(c) => c,
        Err(errors) => {
            println!("ERRORS ({}), these tables are not built (server and pak keep the old version):", errors.len());
            errors.iter().take(50).for_each(|e| println!("  {}", e));
            return false;
        }
    };
    if let Some(only) = only {
        for name in client.iter().filter(|n| !only.contains(&n.as_str())) {
            let _ = std::fs::remove_file(pak_dir.join(format!("{}.bytes", name)));
            println!("  {} — not into the pak (server only)", name);
        }
        client.retain(|n| only.contains(&n.as_str()));
    }
    let pak = Path::new(PAK_STAGE).join(format!("{}.pak", PAK_NAME));
    let _ = std::fs::remove_file(&pak);
    if client.is_empty() {
        println!("client pak not needed: all tables match the game original");
        if deploy {
            for p in installed_paks() {
                if p.exists() {
                    std::fs::remove_file(&p).unwrap();
                    println!("removed the old pak: {}", p.display());
                }
            }
        }
        return true;
    }
    // The same format as the other mod paks: V8B, mount ../../../, no encryption
    let Some(repak) = ws::tool("repak") else {
        println!("no repak: set [tools] repak in local.toml (example — local.example.toml)");
        return false;
    };
    let st = Command::new(repak)
        .args(["pack", "--version", "V8B", "--mount-point", "../../../", "--quiet"])
        .arg(&stage)
        .arg(&pak)
        .status()
        .expect("repak");
    if !st.success() {
        println!("repak pack failed");
        return false;
    }
    println!("pak: {} ({} tables)", pak.display(), client.len());
    if let Some(id) = season {
        if let Err(e) = save_season_build(id, &pak, &client) {
            println!("{}", e);
            return false;
        }
    }
    if deploy {
        if game_running() {
            println!("the game is running — close it and repeat with --deploy");
            return false;
        }
        for p in installed_paks() {
            if p.parent().map(|d| d.exists()).unwrap_or(false) {
                std::fs::copy(&pak, &p).unwrap();
                println!("installed: {}", p.display());
            }
        }
    }
    println!("the server reads the tables at startup — restart the server");
    true
}

/// The season build result goes into its folder: pak\zzzzzz_Balance_P.pak (clients, distribution) and server\ExcelConfig\*.bytes
/// (server tables differing from the game original) — the season moves to another server together with its folder
fn save_season_build(id: &str, pak: &Path, client: &[String]) -> Result<(), String> {
    let sdir = season::dir(id);
    let pak_out = sdir.join("pak");
    std::fs::create_dir_all(&pak_out).map_err(|e| e.to_string())?;
    std::fs::copy(pak, pak_out.join(format!("{}.pak", PAK_NAME))).map_err(|e| format!("{}: {}", pak_out.display(), e))?;
    let srv = sdir.join("server").join("ExcelConfig");
    if srv.is_dir() {
        for (_, p) in tables::list_tables(&srv.to_string_lossy(), "bytes") {
            let _ = std::fs::remove_file(p);
        }
    }
    std::fs::create_dir_all(&srv).map_err(|e| e.to_string())?;
    for name in client {
        let from = Path::new(SERVER).join(format!("{}.bytes", name));
        std::fs::copy(&from, srv.join(format!("{}.bytes", name))).map_err(|e| format!("{}: {}", from.display(), e))?;
    }
    println!("season {}: {} and {} tables in {}", id, pak_out.join(format!("{}.pak", PAK_NAME)).display(), client.len(), srv.display());
    Ok(())
}

/// Table CSV: without a season — balance\*.json -> balance_csv\; with a season — the effective tables (season one, else base)
/// -> seasons\<id>\balance_csv\ (the season has its own CSV: an edit will not get into the base)
fn csv_dir(season: Option<&str>) -> PathBuf {
    match season {
        Some(id) => season::dir(id).join("balance_csv"),
        None => PathBuf::from(BALANCE_CSV),
    }
}

fn balance_csv(names: &[&str], plain: bool, long: bool, season: Option<&str>) -> bool {
    let s = tables::Schema::load();
    let fmt = if plain { csv_view::Format::plain() } else { csv_view::Format::excel_ru() };
    let labels = names::Names::load(&s, BALANCE);
    println!("labels: {}", names::summary(&labels));
    let dir = csv_dir(season);
    std::fs::create_dir_all(&dir).unwrap();
    let all: std::collections::BTreeMap<String, PathBuf> =
        tables::list_tables(BALANCE, "json").into_keys().map(|n| { let p = season::table_path(BALANCE, season, &n); (n, p) }).collect();
    let picked: Vec<(String, PathBuf)> = if names.is_empty() {
        all.into_iter().collect()
    } else {
        names.iter().filter_map(|n| all.get(*n).map(|p| (n.to_string(), p.clone())).or_else(|| { println!("  no table {}", n); None })).collect()
    };
    let own = season::overrides(season);
    let mut ok = true;
    for (name, json) in picked {
        let (wide, longp) = csv_paths(&dir, &name);
        // Keep one form per table — otherwise csv-import cannot tell which one was edited
        let (out, other) = if long { (longp, wide) } else { (wide, longp) };
        let res = if long {
            csv_view::export_long(&s, &json, &out, &fmt, &labels).map(|(rows, lines)| format!("{} rows, {} values", rows, lines))
        } else {
            csv_view::export_table(&s, &json, &out, &fmt, &labels).map(|rows| format!("{} rows", rows))
        };
        match res {
            Ok(info) => {
                let _ = std::fs::remove_file(&other);
                let mark = if season.is_some() && own.contains(&name) { " [season]" } else { "" };
                println!("  {:<40} {}{} -> {}", name, info, mark, out.display());
            }
            Err(e) => {
                println!("  {:<40} ERROR: {}", name, e);
                ok = false;
            }
        }
    }
    ok
}

/// value of a flag like --name=value
fn flag_value<'a>(flags: &[&'a str], name: &str) -> Option<&'a str> {
    flags.iter().find_map(|f| f.strip_prefix(name).and_then(|r| r.strip_prefix('=')))
}

/// --maps=all | --maps=0,10101,… ; without the flag — the maps from our map_detail
fn maps_flag(flags: &[&str]) -> Option<Option<Vec<i64>>> {
    flag_value(flags, "--maps").map(|v| if v == "all" { None } else { Some(v.split(',').filter_map(|x| x.trim().parse().ok()).collect()) })
}

fn csv_paths(dir: &Path, table: &str) -> (PathBuf, PathBuf) {
    (dir.join(format!("{}.csv", table)), dir.join(format!("{}.long.csv", table)))
}

/// CSV -> JSON. With a season: the season table if it exists; else a season copy of the base one, and it stays only if
/// the CSV really changes it (otherwise the season would override the base with a copy that changes nothing)
fn balance_csv_import(names: &[&str], season: Option<&str>) -> bool {
    let s = tables::Schema::load();
    let dir = csv_dir(season);
    if !dir.is_dir() {
        println!("missing {} — run cargo xtask balance csv first", dir.display());
        return false;
    }
    // file name without .csv: "x" — wide form, "x.long" — long form
    let mut tables_in_csv: Vec<String> = tables::list_tables(&dir.to_string_lossy(), "csv").keys().map(|k| k.trim_end_matches(".long").to_string()).collect();
    tables_in_csv.dedup();
    let picked: Vec<String> = if names.is_empty() { tables_in_csv } else { tables_in_csv.into_iter().filter(|k| names.contains(&k.as_str())).collect() };
    let own = season::overrides(season);
    let mut ok = true;
    let mut new_in_season = Vec::new();
    for name in &picked {
        // where we write: the base, the season's own table or a new season copy of the base one
        let (json, fresh) = match season {
            None => (Path::new(BALANCE).join(format!("{}.json", name)), false),
            Some(id) => {
                let p = season::balance_dir(id).join(format!("{}.json", name));
                if own.contains(name) {
                    (p, false)
                } else {
                    std::fs::create_dir_all(season::balance_dir(id)).unwrap();
                    std::fs::copy(Path::new(BALANCE).join(format!("{}.json", name)), &p).unwrap();
                    (p, true)
                }
            }
        };
        let (wide, longp) = csv_paths(&dir, name);
        let before = std::fs::read(&json).ok(); // the CSV changes nothing — the file stays byte for byte the same
        let res = match (wide.exists(), longp.exists()) {
            (true, true) => Err(vec![format!("both {} and {} exist — keep one", wide.display(), longp.display())]),
            (false, true) => csv_view::import_long(&s, &longp, &json),
            _ => csv_view::import_table(&s, &wide, &json),
        };
        match res {
            Ok((n, changed)) => {
                if let (0, Some(b), false) = (changed, &before, fresh) {
                    std::fs::write(&json, b).unwrap();
                }
                if fresh && changed == 0 {
                    let _ = std::fs::remove_file(&json); // changes nothing — stays the base one
                } else if fresh {
                    new_in_season.push(name.clone());
                }
                println!("  {:<40} {:>6} rows, changed {}{}", name, n, changed, if fresh && changed > 0 { " — now the season's own" } else { "" });
            }
            Err(errors) => {
                if fresh {
                    let _ = std::fs::remove_file(&json);
                }
                ok = false;
                println!("  {:<40} NOT imported ({} errors):", name, errors.len());
                errors.iter().take(20).for_each(|e| println!("      {}", e));
            }
        }
    }
    if let (Some(id), false) = (season, new_in_season.is_empty()) {
        println!("tables added to season {}: {} ({})", id, new_in_season.join(", "), season::balance_dir(id).display());
    }
    if ok {
        println!("next: cargo xtask balance build");
    }
    ok
}

/// NameDump mod dump (table\tkey\ttext) -> names\strings.tsv. By default — %LOCALAPPDATA%\Dungeonborne\Saved\names_dump.tsv
/// lang: None/ru — names/strings.tsv (CSV labels); en — names/strings_en.tsv (the second label of the balance kit)
fn names_import(file: Option<&str>, lang: Option<&str>) -> bool {
    let target = match lang {
        None | Some("ru") => names::STRINGS_TSV,
        Some("en") => names::STRINGS_EN_TSV,
        Some(other) => {
            println!("--lang={}: ru and en are supported", other);
            return false;
        }
    };
    let src = match file {
        Some(f) => PathBuf::from(f),
        None => PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_default()).join("Dungeonborne/Saved/names_dump.tsv"),
    };
    let text = match std::fs::read_to_string(&src) {
        Ok(t) => t,
        Err(e) => {
            println!("{}: {} — start the game and press F6 in the lobby (NameDump mod)", src.display(), e);
            return false;
        }
    };
    let mut per_table: std::collections::BTreeMap<&str, usize> = Default::default();
    let mut lines = 0;
    for l in text.lines() {
        if let Some(t) = l.split('\t').next() {
            *per_table.entry(t).or_default() += 1;
            lines += 1;
        }
    }
    // dump language by the letters of the texts: Cyrillic in an "English" dump — the client was not switched (and vice versa)
    let texts: String = text.lines().filter_map(|l| l.splitn(3, '\t').nth(2)).collect();
    let cyr = texts.chars().filter(|c| ('\u{0400}'..='\u{04FF}').contains(c)).count();
    let lat = texts.chars().filter(|c| c.is_ascii_alphabetic()).count();
    let looks_ru = cyr * 2 > lat;
    if (target == names::STRINGS_EN_TSV) == looks_ru {
        println!("the dump looks {} (Cyrillic {}, Latin {}), but the import goes to {} — wrong client language? Not written.",
            if looks_ru { "Russian" } else { "English" }, cyr, lat, target);
        return false;
    }
    std::fs::create_dir_all(ws::NAMES).unwrap();
    std::fs::write(target, &text).unwrap();
    println!("{} rows from {} tables -> {}", lines, per_table.len(), target);
    for (t, n) in per_table.iter().filter(|(_, n)| **n >= 50) {
        println!("  {:<60} {}", t, n);
    }
    println!("next: cargo xtask balance csv … — labels will come from localization");
    true
}

/// Balance pak in all game installations from local.toml [games] (server, clients): otherwise an installation keeps the old pak
/// (01.11: a loot pak with the no_gear mode did not reach the second client — equipment was failing)
fn installed_paks() -> Vec<PathBuf> {
    ws::games().into_iter().map(|(_, r)| r.join("Dungeonborne/Content/Paks").join(format!("{}.pak", PAK_NAME))).collect()
}

fn game_running() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Dungeonborne.exe", "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("Dungeonborne.exe"))
        .unwrap_or(false)
}
