// Balance kit for players (balance_kit/README.md):
//   cargo xtask balance kit [--out=folder]       balance\*.json -> kit: data\*.csv (long form, ',' and '.'),
//                                                raw\*.json, kit.json (what may be edited), manifest.json (version,
//                                                sha256), tools\*.py; then python tools\build_excel.py (book + LLM)
//   cargo xtask balance patch <patch.json> [--dry-run]   players' edits -> balance\*.json (checked against "was")
//
// Patch (format dbbalance-patch/1, written by tools\make_patch.py or an LLM):
//   {"format":"dbbalance-patch/1","base":"<kit version>","author":"…","comment":"…",
//    "changes":[{"table":"role_level_upgrade","row":12,"key":"400001","path":"role_attribute.first_level_strength.fixed_value","old":"10","new":"12"}]}
// Patch check rules — crates/balance-kit (the same as in tools\check.py); here the patch is checked against the server tables.
use crate::csv_view::{self, Change};
use crate::names::Names;
use crate::tables::{self, Schema};
use balance_kit::{Patch, TableData, PATCH_FORMAT};
use serde_json::{json, Value as Json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::ws::KIT_DIR;
/// A kit table whose values come from the loot template, not from balance\*.json
const FROM_TEMPLATE: &str = "loot_template";

fn from_template(t: &Json) -> bool {
    t["source"].as_str() == Some(FROM_TEMPLATE)
}

fn table_cfg<'a>(cfg: &'a Json, name: &str) -> &'a Json {
    cfg["table"].as_array().and_then(|a| a.iter().find(|t| t["name"].as_str() == Some(name))).unwrap_or(&Json::Null)
}

fn read_toml(path: &Path) -> Result<Json, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let v: toml::Value = toml::from_str(text).map_err(|e| format!("{}: {}", path.display(), e))?;
    serde_json::to_value(v).map_err(|e| e.to_string())
}

/// Kit config: balance_kit/kit.toml (framework) + seasons/<id>/kit.toml (season, optional).
/// Season: title/about and other keys — replace; notes — are appended; fields — extend;
/// [[table]] with the same name — extends/changes the table fields, with a new name — adds a table.
fn load_kit(season: Option<&str>) -> Result<Json, String> {
    let mut cfg = read_toml(&Path::new(KIT_DIR).join("kit.toml"))?;
    let Some(id) = season else { return Ok(cfg) };
    let path = crate::season::dir(id).join("kit.toml");
    if !path.exists() {
        return Ok(cfg);
    }
    let over = read_toml(&path)?;
    for (k, v) in over.as_object().cloned().unwrap_or_default() {
        match k.as_str() {
            "notes" | "notes_en" => {
                let mut all = cfg[&k].as_array().cloned().unwrap_or_default();
                all.extend(v.as_array().cloned().unwrap_or_default());
                cfg[&k] = Json::Array(all);
            }
            "fields" | "fields_en" => {
                if !cfg[&k].is_object() {
                    cfg[&k] = json!({});
                }
                for (fk, fv) in v.as_object().cloned().unwrap_or_default() {
                    cfg[&k][fk] = fv;
                }
            }
            "table" => {
                let mut tables = cfg["table"].as_array().cloned().unwrap_or_default();
                for t in v.as_array().cloned().unwrap_or_default() {
                    match tables.iter_mut().find(|x| x["name"] == t["name"]) {
                        Some(x) => {
                            for (tk, tv) in t.as_object().cloned().unwrap_or_default() {
                                x[tk] = tv;
                            }
                        }
                        None => tables.push(t),
                    }
                }
                cfg["table"] = Json::Array(tables);
            }
            _ => cfg[&k] = v,
        }
    }
    Ok(cfg)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{:02x}", b)).collect()
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else if e.path().extension().map_or(true, |x| x != "pyc") {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

/// Kit: effective tables (season on top of the base) + the game original next to them (data/<table>.orig.csv) —
/// the book and the LLM files show what the server/season changed relative to the game
pub fn kit(balance: &str, season: Option<&str>, out: Option<&str>, zip: bool) -> bool {
    let mut cfg = match load_kit(season) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return false;
        }
    };
    let s = Schema::load();
    let labels = Names::load(&s, balance);
    // attribute names from localization — into fields (own names from kit.toml take precedence)
    if !cfg["fields"].is_object() {
        cfg["fields"] = json!({});
    }
    let fields = cfg["fields"].as_object_mut().unwrap();
    for (k, v) in labels.attr_field_labels(&s) {
        fields.entry(k).or_insert(Json::String(v));
    }
    // attribute units: display_percentage from battle_attribute_description (attribute_type = field number of
    // DBattleAttributeOffset) -> "доля" (fraction, 0.05 = 5 %), else "очки" (points); attr_ids — number -> field name (rows by attribute_type)
    let pct: std::collections::HashSet<i64> = tables::read_json(&Path::new(balance).join("battle_attribute_description.json"))
        .ok()
        .and_then(|v| v["rows"].as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter(|r| r["display_percentage"].as_bool() == Some(true))
        .filter_map(|r| r["attribute_type"].as_i64())
        .collect();
    let (mut units, mut ids) = (serde_json::Map::new(), serde_json::Map::new());
    if let Some(m) = s.pool.all_messages().find(|m| m.name() == "DBattleAttributeOffset") {
        for f in m.fields() {
            units.insert(f.name().to_string(), json!(if pct.contains(&(f.number() as i64)) { "доля" } else { "очки" }));
            ids.insert(f.number().to_string(), json!(f.name()));
        }
    }
    // English labels — if the second dump is imported (names-import --lang=en)
    let labels_en = Path::new(crate::names::STRINGS_EN_TSV).exists().then(|| Names::load_from(&s, balance, crate::names::STRINGS_EN_TSV));
    if let Some(en) = &labels_en {
        // Own names from kit.toml take precedence, as with the Russian labels
        let mut fields_en: serde_json::Map<String, Json> = en.attr_field_labels(&s).into_iter().map(|(k, v)| (k, Json::String(v))).collect();
        for (k, v) in cfg["fields_en"].as_object().cloned().unwrap_or_default() {
            fields_en.insert(k, v);
        }
        cfg["fields_en"] = Json::Object(fields_en);
    }
    cfg["attr_units"] = Json::Object(units);
    cfg["attr_ids"] = Json::Object(ids);
    let fmt = csv_view::Format::plain();
    let tables_cfg = cfg["table"].as_array().cloned().unwrap_or_default();

    // version: date + the start of sha256 of all kit tables
    let mut hasher = Sha256::new();
    let mut infos = BTreeMap::new();
    // The loot template is part of the version, like the tables: an edit of the template gives a new kit
    let template = if tables_cfg.iter().any(from_template) {
        match crate::loot::read_template(season) {
            Ok((path, doc)) => {
                let bytes = std::fs::read(&path).unwrap_or_default();
                hasher.update(&bytes);
                Some((path, doc, sha256_hex(&bytes)))
            }
            Err(e) => {
                eprintln!("{}", e);
                return false;
            }
        }
    } else {
        None
    };
    for t in tables_cfg.iter().filter(|t| !from_template(t)) {
        let name = t["name"].as_str().unwrap_or_default();
        let json_path = crate::season::table_path(balance, season, name);
        let Ok(bytes) = std::fs::read(&json_path) else {
            eprintln!("missing {}", json_path.display());
            return false;
        };
        hasher.update(&bytes);
        infos.insert(name.to_string(), (json_path, sha256_hex(&bytes)));
    }
    let digest: String = hasher.finalize().iter().take(4).map(|b| format!("{:02x}", b)).collect();
    let version = format!("{}-{}", today(), digest);
    // season — the kit in the season folder (seasons/<id>/kit/<id>-balance-…), base — balance_kit/out/dungeonborne-balance-…
    let dir: PathBuf = out.map(PathBuf::from).unwrap_or_else(|| match season {
        Some(id) => crate::season::dir(id).join("kit").join(format!("{}-balance-{}", id, version)),
        None => Path::new(KIT_DIR).join("out").join(format!("dungeonborne-balance-{}", version)),
    });
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["data", "raw"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    // game originals (.bytes) -> JSON in a temp folder of the kit, from there — data/<table>.orig.csv
    let orig_tmp = dir.join(".orig");
    std::fs::create_dir_all(&orig_tmp).unwrap();
    let originals = tables::list_tables(crate::ORIGINALS, "bytes");
    let overrides = crate::season::overrides(season);

    let mut manifest_tables = serde_json::Map::new();
    if let Some((path, doc, sha)) = &template {
        let cats = crate::loot::item_cats();
        let source = if season.is_some() && *path == crate::loot::own_template(season) { "season" } else { "base" };
        let ru = crate::loot::template_values(doc, &cats, Some(&labels), false);
        let en = labels_en.as_ref().map(|n| crate::loot::template_values(doc, &cats, Some(n), true));
        for (i, (name, values)) in ru.iter().enumerate() {
            if !tables_cfg.iter().any(|t| t["name"].as_str() == Some(*name)) {
                continue;
            }
            let (rows, count) = csv_view::write_values(&dir.join("data").join(format!("{}.csv", name)), &fmt, values).unwrap();
            if let Some(en) = &en {
                csv_view::write_values(&dir.join("data").join(format!("{}.en.csv", name)), &fmt, &en[i].1).unwrap();
            }
            println!("  {:<28} {} rows, {} values (loot template)", name, rows, count);
            manifest_tables.insert(name.to_string(), json!({ "sha256": sha, "rows": rows, "values": count, "type": FROM_TEMPLATE, "source": source, "original": false }));
        }
        std::fs::write(dir.join("raw").join("loot_template.json"), serde_json::to_string(&crate::loot::template_json(doc)).unwrap()).unwrap();
    }
    for t in tables_cfg.iter().filter(|t| !from_template(t)) {
        let name = t["name"].as_str().unwrap_or_default();
        let (json_path, sha) = &infos[name];
        let csv_path = dir.join("data").join(format!("{}.csv", name));
        match csv_view::export_long(&s, json_path, &csv_path, &fmt, &labels) {
            Ok((rows, values)) => {
                let source = if overrides.iter().any(|o| o == name) { "season" } else { "base" };
                println!("  {:<28} {} rows, {} values ({})", name, rows, values, if source == "season" { "season" } else { "base" });
                let raw = tables::read_json(json_path).unwrap_or(Json::Null);
                // the game original of the same table: data/<table>.orig.csv (no original — the table is ours only)
                let has_orig = originals.get(name).and_then(|op| std::fs::read(op).ok()).and_then(|b| tables::decode_table(&s, &b).ok()).map(|(t, _)| {
                    let oj = orig_tmp.join(format!("{}.json", name));
                    tables::write_json(&oj, &tables::table_to_json(&t));
                    csv_view::export_long(&s, &oj, &dir.join("data").join(format!("{}.orig.csv", name)), &fmt, &labels).is_ok()
                });
                manifest_tables.insert(
                    name.to_string(),
                    json!({ "sha256": sha, "rows": rows, "values": values, "type": raw["type"], "source": source, "original": has_orig == Some(true) }),
                );
                std::fs::copy(json_path, dir.join("raw").join(format!("{}.json", name))).unwrap();
                if let Some(en) = &labels_en {
                    // the same rows with English labels (title, name) — data/<table>.en.csv
                    let _ = csv_view::export_long(&s, json_path, &dir.join("data").join(format!("{}.en.csv", name)), &fmt, en);
                }
            }
            Err(e) => {
                eprintln!("  {}: {}", name, e);
                return false;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&orig_tmp);
    let manifest = json!({
        "format": "dbbalance-kit/1",
        "version": version,
        "title": cfg["title"],
        "season": season,
        "patch_format": PATCH_FORMAT,
        "tables": manifest_tables
    });
    std::fs::write(dir.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap()).unwrap();
    std::fs::write(dir.join("kit.json"), serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
    let catalog = catalog(&cfg, &s, balance, season, &labels, labels_en.as_ref());
    println!(
        "  catalog: items {}, classes {}, rarities {}, tables with field types {}",
        catalog["items"].as_object().map_or(0, |m| m.len()),
        catalog["classes"].as_object().map_or(0, |m| m.len()),
        catalog["rarities"].as_object().map_or(0, |m| m.len()),
        catalog["field_types"].as_object().map_or(0, |m| m.len())
    );
    std::fs::write(dir.join("catalog.json"), serde_json::to_string_pretty(&catalog).unwrap()).unwrap();
    // item pictures (icons_dir from kit.toml), on top — own ones from balance_kit/icons
    if let Some(src) = cfg["icons_dir"].as_str().map(Path::new).filter(|p| p.exists()) {
        copy_dir(src, &dir.join("icons")).unwrap();
    }
    for f in ["tools", "icons"] {
        let src = Path::new(KIT_DIR).join(f);
        if src.exists() {
            copy_dir(&src, &dir.join(f)).unwrap();
        }
    }
    // README — into the kit root; PROMPT.md — a template (with {VERSION}…) in tools/, the ready one — llm/PROMPT.md (build_excel.py).
    // A season may have its own: seasons/<id>/kit/README.md, PROMPT.md
    for (f, to) in [("README.md", dir.join("README.md")), ("PROMPT.md", dir.join("tools").join("PROMPT.template.md"))] {
        let own = season.map(|id| crate::season::dir(id).join("kit").join(f)).filter(|p| p.exists());
        let src = own.unwrap_or_else(|| Path::new(KIT_DIR).join(f));
        if src.exists() {
            std::fs::copy(&src, to).unwrap();
        }
    }
    println!("kit {} -> {}", version, dir.display());

    // the Excel book and the LLM files — Python (players have the same script)
    // PYTHONUTF8: script output in UTF-8, like the rest of xtask
    let st = std::process::Command::new("python").env("PYTHONUTF8", "1").arg(dir.join("tools").join("build_excel.py")).arg(&dir).status();
    match st {
        Ok(s) if s.success() => !zip || pack_zip(&dir),
        Ok(s) => {
            eprintln!("build_excel.py: code {}", s.code().unwrap_or(-1));
            false
        }
        Err(e) => {
            eprintln!("python did not start ({}) — to build the book: python {}\\tools\\build_excel.py {}", e, dir.display(), dir.display());
            false
        }
    }
}

/// Kit -> <folder>.zip next to it (to hand out to players; the kit folder inside). Without .thumbs — it is the book's thumbnail cache,
/// build_excel.py recreates it itself. Zip — with the standard Python zipfile (Python is needed for the kit anyway)
fn pack_zip(dir: &Path) -> bool {
    const PY: &str = r#"
import os, sys, zipfile
root = os.path.abspath(sys.argv[1]); out = root + ".zip"; base = os.path.dirname(root)
n = 0
with zipfile.ZipFile(out + ".tmp", "w", zipfile.ZIP_DEFLATED, compresslevel=6) as z:
    for d, dirs, files in os.walk(root):
        dirs[:] = sorted(x for x in dirs if x != ".thumbs")
        for f in sorted(files):
            p = os.path.join(d, f); z.write(p, os.path.relpath(p, base)); n += 1
os.replace(out + ".tmp", out)
print(f"zip: {out} ({n} files, {os.path.getsize(out) / 1048576:.1f} MB)")
"#;
    match std::process::Command::new("python").env("PYTHONUTF8", "1").args(["-I", "-X", "utf8", "-c", PY]).arg(dir).status() {
        Ok(s) if s.success() => true,
        other => {
            eprintln!("zip not built ({:?}); the kit is ready in {}", other.map(|s| s.code()), dir.display());
            false
        }
    }
}

fn today() -> String {
    // without chrono: the date from the system time (UTC)
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let days = secs.div_euclid(86_400);
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}{:02}{:02}", if m <= 2 { y + 1 } else { y }, m, d)
}

/// Patch -> tables of its season (`season` in the patch is written by the kit; no field — an old patch: --season or the active one).
/// Season: the edit goes into seasons/<id>/balance/<table>.json (none — first a copy of the base one), base — into balance\.
pub fn patch(balance: &str, file: &str, season_flag: Option<&str>, dry_run: bool) -> bool {
    let p = match tables::read_json(Path::new(file)) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{}: {}", file, e);
            return false;
        }
    };
    if p["format"].as_str() != Some(PATCH_FORMAT) {
        eprintln!("{}: format {:?}, {} required", file, p["format"], PATCH_FORMAT);
        return false;
    }
    let season: Option<String> = match p.get("season") {
        Some(Json::String(id)) => {
            if let Some(f) = season_flag.filter(|f| *f != id) {
                eprintln!("the patch is for season {}, but --season={} is given — a patch applies to its own season", id, f);
                return false;
            }
            if !crate::season::dir(id).is_dir() {
                eprintln!("the patch is for season {}, but there is no folder {}", id, crate::season::dir(id).display());
                return false;
            }
            Some(id.clone())
        }
        Some(Json::Null) => None, // a kit without a season — base
        _ => match crate::season::resolve(season_flag) {
            Ok(s) => {
                println!("no season in the patch (old kit) — {}", crate::season::label(s.as_deref()));
                s
            }
            Err(e) => {
                eprintln!("{}", e);
                return false;
            }
        },
    };
    let season = season.as_deref();
    let cfg = match load_kit(season) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            return false;
        }
    };
    println!(
        "patch {} ({}): author {}, kit {}, changes {}{}",
        file,
        crate::season::label(season),
        p["author"].as_str().unwrap_or("?"),
        p["base"].as_str().unwrap_or("?"),
        p["changes"].as_array().map_or(0, |a| a.len()),
        p["comment"].as_str().filter(|c| !c.is_empty()).map(|c| format!("\n  «{}»", c)).unwrap_or_default()
    );
    let mut editable: BTreeMap<String, Vec<String>> = cfg["table"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| {
            let pats = t["editable"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
            (t["name"].as_str().unwrap_or_default().to_string(), pats)
        })
        .collect();

    let parsed = match Patch::from_json(&p) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{}: {}", file, e);
            return false;
        }
    };
    let s = Schema::load();
    let template = if parsed.changes.iter().any(|c| from_template(table_cfg(&cfg, &c.table))) {
        match crate::loot::read_template(season) {
            Ok(x) => Some(x),
            Err(e) => {
                eprintln!("{}", e);
                return false;
            }
        }
    } else {
        None
    };
    let checks = match server_tables(&s, balance, season, &parsed, &mut editable, &cfg, template.as_ref().map(|x| &x.1)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{}", e);
            return false;
        }
    };
    let report = balance_kit::check(&parsed, &checks, &balance_kit::Expect::default());
    print_changes(&parsed);
    for w in &report.warnings {
        println!("  ! {}", w.message);
    }
    if !report.ok() {
        for e in &report.errors {
            eprintln!("  x {}", e.message);
        }
        eprintln!("patch not applied ({} errors)", report.errors.len());
        return false;
    }
    let mut errors = Vec::new();
    let mut by_table: BTreeMap<String, Vec<Change>> = BTreeMap::new();
    for c in parsed.changes.iter().filter(|c| !from_template(table_cfg(&cfg, &c.table))) {
        // every change has a row: without it check would have refused
        by_table.entry(c.table.clone()).or_default().push(Change { row: c.row.unwrap_or_default(), key: c.key.clone(), path: c.path.clone(), old: c.old.clone(), new: c.new.clone() });
    }

    // schema: all tables first, writing — only if everything matched
    for (table, changes) in &by_table {
        let json_path = crate::season::table_path(balance, season, table);
        if let Err(errs) = csv_view::apply_changes(&s, &json_path, changes, false) {
            for e in errs {
                errors.push(format!("{}: {}", table, e));
            }
        }
    }
    // Loot template: edits into the template text (comments and order stay) and a trial build of the tables, before any write
    let new_template = match &template {
        Some((path, _)) => match crate::loot_edit::apply(path, &parsed.changes, &checks) {
            Ok((text, count)) => match crate::loot::validate_template(season, &text) {
                Ok(_) => Some((text, count)),
                Err(errs) => {
                    errors.extend(errs.into_iter().map(|e| format!("loot template: {}", e)));
                    None
                }
            },
            Err(errs) => {
                errors.extend(errs.into_iter().map(|e| format!("loot template: {}", e)));
                None
            }
        },
        None => None,
    };
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("  {}", e);
        }
        eprintln!("patch not applied ({} errors)", errors.len());
        return false;
    }
    for (table, changes) in &by_table {
        let mut json_path = crate::season::table_path(balance, season, table);
        if let (Some(id), false) = (season, dry_run) {
            // first edit of the table in the season: a season copy of the base one, the edit goes into it (the base does not change)
            let own = crate::season::balance_dir(id).join(format!("{}.json", table));
            if !own.exists() {
                std::fs::create_dir_all(crate::season::balance_dir(id)).unwrap();
                std::fs::copy(&json_path, &own).unwrap();
                println!("  {:<28} now the season's own: {}", table, own.display());
            }
            json_path = own;
        }
        match csv_view::apply_changes(&s, &json_path, changes, !dry_run) {
            Ok(n) => println!("  {:<28} {} changes -> {}{}", table, n, json_path.display(), if dry_run { " (--dry-run, not written)" } else { "" }),
            Err(errs) => {
                eprintln!("  {}: {}", table, errs.join("; "));
                return false;
            }
        }
    }
    if let Some((text, count)) = &new_template {
        let path = crate::loot::own_template(season);
        if dry_run {
            println!("  loot template: {} changes -> {} (--dry-run, not written)", count, path.display());
        } else {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, text).unwrap();
            println!("  loot template: {} changes -> {}", count, path.display());
            if !crate::loot::build(season) {
                eprintln!("the template is written, but the loot tables did not build");
                return false;
            }
        }
    }
    if !dry_run {
        let flag = season.map(|id| format!(" --season={}", id)).unwrap_or_default();
        println!("next: cargo xtask balance build{} (server + client pak)", flag);
    }
    true
}

/// Patch tables as the server sees them: effective values (season on top of the base), the game original for ranges, types by schema
fn server_tables(s: &Schema, balance: &str, season: Option<&str>, patch: &Patch, editable: &mut BTreeMap<String, Vec<String>>, cfg: &Json, template: Option<&Json>) -> Result<BTreeMap<String, TableData>, String> {
    let originals = tables::list_tables(crate::ORIGINALS, "bytes");
    let mut out = BTreeMap::new();
    let cats = crate::loot::item_cats();
    let template_tables = template.map(|doc| crate::loot::template_values(doc, &cats, None, false));
    for c in &patch.changes {
        if out.contains_key(&c.table) {
            continue;
        }
        let Some(patterns) = editable.remove(&c.table) else { continue };
        if from_template(table_cfg(cfg, &c.table)) {
            let values = template_tables.iter().flatten().find(|t| t.0 == c.table).map(|t| t.1.clone()).ok_or(format!("{}: not in the loot template", c.table))?;
            let kinds = balance_kit::loot::field_kinds(&c.table).into_iter().collect();
            out.insert(c.table.clone(), TableData { kinds, editable: patterns, values, ..TableData::default() }.with_rules(table_cfg(cfg, &c.table)));
            continue;
        }
        let json_path = crate::season::table_path(balance, season, &c.table);
        let v = tables::read_json(&json_path).map_err(|e| format!("{}: {}", json_path.display(), e))?;
        let desc = s.row_type(v["type"].as_str().unwrap_or("")).ok_or(format!("{}: unknown type", c.table))?;
        let orig = originals
            .get(&c.table)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| tables::decode_table(s, &b).ok())
            .and_then(|(t, _)| csv_view::long_values(s, &tables::table_to_json(&t)).ok());
        out.insert(
            c.table.clone(),
            TableData { kinds: csv_view::field_kinds(&desc, &patterns), editable: patterns, values: csv_view::long_values(s, &v)?, orig, ..TableData::default() }.with_rules(table_cfg(cfg, &c.table)),
        );
    }
    Ok(out)
}

fn print_changes(patch: &Patch) {
    for c in &patch.changes {
        let pct = match (balance_kit::as_number(&c.old), balance_kit::as_number(&c.new)) {
            (Some(o), Some(n)) if o != 0.0 => format!(" ({:+.0}%)", (n - o) / o * 100.0),
            _ => String::new(),
        };
        let who = if c.title.is_empty() { &c.key } else { &c.title };
        let old = if c.old.is_empty() { "(empty)" } else { &c.old };
        println!("  {:<24} {:<24} {}: {} -> {}{}", c.table, who, c.path, old, c.new, pct);
    }
}

fn short_enum<'a>(v: &'a Json, prefix: &str) -> Option<&'a str> {
    v.as_str().map(|s| s.strip_prefix(prefix).unwrap_or(s))
}

fn set_text(obj: &mut Json, key: &str, value: Option<&str>) {
    if let Some(x) = value.filter(|x| !x.is_empty()) {
        obj[key] = json!(x);
    }
}

/// catalog.json for the app (docs/BALANCE_APP_SPEC.md, section 3): what the kit tables do not have.
/// The book and make_patch.py do not read it; check.py takes field types from there.
fn catalog(cfg: &Json, s: &Schema, balance: &str, season: Option<&str>, ru: &Names, en: Option<&Names>) -> Json {
    let rows = |name: &str| -> Vec<Json> {
        tables::read_json(&crate::season::table_path(balance, season, name)).ok().and_then(|v| v["rows"].as_array().cloned()).unwrap_or_default()
    };
    let mut items: BTreeMap<i64, Json> = BTreeMap::new();
    for r in rows("item_type") {
        let Some(id) = r["type_id"].as_i64() else { continue };
        let e = items.entry(id).or_insert_with(|| json!({}));
        set_text(e, "ru", ru.item_localized(id));
        set_text(e, "en", en.and_then(|n| n.item_localized(id)));
        set_text(e, "major", short_enum(&r["item_major_type"], "EN_ITEM_MAJOR_TYPE_"));
        set_text(e, "minor", short_enum(&r["item_minor_type"], "EN_ITEM_MINOR_TYPE_"));
    }
    for r in rows("UeSource_ItemType") {
        let Some(id) = r["type_id"].as_i64() else { continue };
        let e = items.entry(id).or_insert_with(|| json!({}));
        set_text(e, "rarity", short_enum(&r["rarity"], "EN_ITEM_RARITY_"));
        set_text(e, "slot", short_enum(&r["slot_type"], "EN_SKIN_SLOT_TYPE_"));
        set_text(e, "kind", short_enum(&r["item_type"], "EN_ITEM_UE_TYPE_"));
    }

    // Classes: names from localization; [classes."<id>"] in kit.toml may correct them.
    // ONLY_EDIT (400000, 499999) — service classes, not available to the player: the app hides them from lists
    let mut classes = serde_json::Map::new();
    for (order, r) in rows("role_type").iter().enumerate() {
        let Some(id) = r["type_id"].as_i64() else { continue };
        let mut e = json!({ "order": order });
        set_text(&mut e, "ru", ru.role_localized(id));
        set_text(&mut e, "en", en.and_then(|n| n.role_localized(id)));
        set_text(&mut e, "career", short_enum(&r["meta"]["career"], "EN_ROLE_CAREER_"));
        set_text(&mut e, "race", short_enum(&r["meta"]["race"], "EN_ROLE_RACE_"));
        if r["environment"].as_str() == Some("EN_ENVIRONMENT_TYPE_ONLY_EDIT") {
            e["hidden"] = json!(true);
        }
        if let Some(over) = cfg["classes"][id.to_string()].as_object() {
            for (k, v) in over {
                e[k] = v.clone();
            }
        }
        classes.insert(id.to_string(), e);
    }

    let mut rarities = serde_json::Map::new();
    if let Some(rarity_enum) = s.pool.get_enum_by_name("tgf.EnItemRarity").or_else(|| s.pool.all_enums().find(|e| e.name() == "EnItemRarity")) {
        for v in rarity_enum.values() {
            let n = v.number() as i64;
            let mut e = json!({ "value": n });
            set_text(&mut e, "ru", ru.rarity_name(n));
            set_text(&mut e, "en", en.and_then(|x| x.rarity_name(n)));
            rarities.insert(v.name().strip_prefix("EN_ITEM_RARITY_").unwrap_or(v.name()).to_string(), e);
        }
    }

    let mut types = serde_json::Map::new();
    for t in cfg["table"].as_array().into_iter().flatten() {
        let name = t["name"].as_str().unwrap_or_default();
        let patterns: Vec<String> = t["editable"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
        if patterns.is_empty() {
            continue;
        }
        if from_template(t) {
            let kinds: serde_json::Map<String, Json> = balance_kit::loot::field_kinds(name).into_iter().map(|(p, k)| (p, json!(k.name()))).collect();
            types.insert(name.to_string(), Json::Object(kinds));
            continue;
        }
        let Ok(v) = tables::read_json(&crate::season::table_path(balance, season, name)) else { continue };
        let Some(desc) = s.row_type(v["type"].as_str().unwrap_or("")) else { continue };
        let kinds: serde_json::Map<String, Json> = csv_view::field_kinds(&desc, &patterns).into_iter().map(|(p, k)| (p, json!(k.name()))).collect();
        types.insert(name.to_string(), Json::Object(kinds));
    }

    // Products without an item name (bundles, skins): the name from product.name or the item name in localization
    let mut products = serde_json::Map::new();
    for r in rows("market_product_item") {
        let Some(id) = r["product_id"].as_i64() else { continue };
        let key = r["product"]["name"].as_str().unwrap_or_default();
        let reward = r["product"]["reward_items"][0]["type_id"].as_i64().unwrap_or_default();
        let name = |n: &Names| -> Option<String> {
            n.text(key).map(str::to_string).or_else(|| ["Facade_Name_", "FacadeName_", "Item_Name_"].iter().find_map(|p| n.text(&format!("{}{}", p, reward)).map(str::to_string)))
        };
        let mut e = json!({});
        set_text(&mut e, "ru", name(ru).as_deref());
        set_text(&mut e, "en", en.and_then(&name).as_deref());
        if e.as_object().is_some_and(|m| !m.is_empty()) {
            products.insert(id.to_string(), e);
        }
    }

    json!({
        "format": balance_kit::CATALOG_FORMAT,
        "products": products,
        "maps": cfg["maps"],
        "items": items.into_iter().map(|(id, v)| (id.to_string(), v)).collect::<serde_json::Map<_, _>>(),
        "classes": classes,
        "rarities": rarities,
        "field_types": types
    })
}
