// cargo xtask mods — UE4SS mods and paks from the repository per game/catalog/catalog.toml (stage 2, docs/MODS_REFACTOR_PLAN.md;
// rules — docs/MODS_DESIGN.md §4, §11). The working game installation is no longer the source.
//
//   mods build   --profile=<p>                 home/out/mods/<p>/ in the installation layout + manifest.json (sha256, commit)
//   mods check                                 catalog, Lua, forbidden calls, dbnative, pak overrides
//   mods install --profile=<p> --game=<name|path>   build and lay out the profile into an installation (game closed)
//   mods verify  --profile=<p> --game=<name|path>   check an installation against the contents: files, enabled mods, extra paks
//
// Profiles: client (players, launcher packages), server (Robot), dev-client, dev-server (local installations).
// Mod enabling (mods.txt / enabled.txt) and installation settings files (native_data.txt etc.) are left alone.
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const CATALOG: &str = crate::ws::MODS_CATALOG;
const WIN64: &str = "Dungeonborne/Binaries/Win64";
const PAKS: &str = "Dungeonborne/Content/Paks";
const PROFILES: [&str; 4] = ["client", "server", "dev-client", "dev-server"];
const PAK_EXTS: [&str; 4] = ["pak", "ucas", "utoc", "sig"];
/// Appended to "file missing" errors: binary paks are not in the public edition (19.11) — how to get each one.
const BRING_OWN: &str = " — not in the repository: patch paks — `cargo ws dump paks --apply` (from your copy of the game), \
     the balance pak — `cargo xtask balance build`; the rest — docs/STORAGE_EXPORT.md";
// UE4SS service thread: Lua in it at the same time as the game thread crashes the game (docs/CRASH_LONG_PLAY.md)
const BANNED: [&str; 3] = ["LoopAsync(", "ExecuteWithDelay(", "ExecuteAsync("];
// UE4SS runtime (built into the launcher), not our mods
const RUNTIME_MODS: [&str; 3] = ["BPModLoaderMod", "BPML_GenericFunctions", "Keybinds"];

struct Item {
    id: String,
    kind: String,
    name: String,
    source: PathBuf,
    build: String,
    profiles: Vec<String>,
    side: String,
    dev: bool,
    overrides: Vec<String>,
    requires: Vec<String>,
    /// part of a season (docs/SEASONS_FORMAT.md): installed only when that season is active
    season: Option<String>,
}

impl Item {
    fn external(&self) -> bool {
        self.source.is_absolute()
    }
}

fn strs(v: &toml::Value, key: &str) -> Vec<String> {
    v.get(key).and_then(|a| a.as_array()).map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn load_catalog() -> Result<Vec<Item>, String> {
    let text = fs::read_to_string(CATALOG).map_err(|e| format!("{CATALOG}: {e}"))?;
    let doc: toml::Value = text.parse().map_err(|e| format!("{CATALOG}: {e}"))?;
    let mut items = Vec::new();
    for v in doc.get("item").and_then(|a| a.as_array()).ok_or(format!("{CATALOG}: no [[item]]"))? {
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        items.push(Item {
            id: s("id"),
            kind: s("kind"),
            name: s("name"),
            source: crate::ws::resolve(&s("source")),
            build: if s("build").is_empty() { "copy".into() } else { s("build") },
            profiles: strs(v, "profiles"),
            side: if s("side").is_empty() { "both".into() } else { s("side") },
            dev: v.get("dev").and_then(|x| x.as_bool()).unwrap_or(false),
            overrides: strs(v, "overrides"),
            requires: strs(v, "requires"),
            season: Some(s("season")).filter(|x| !x.is_empty()),
        });
    }
    Ok(items)
}

/// Built mod packages go to DB_HOME (home/out/mods), not into the repository.
fn out_dir() -> PathBuf {
    crate::ws::home().join("out").join("mods")
}

/// Programs (lua, repak) — local.toml [tools].
fn tool(name: &str) -> Option<PathBuf> {
    crate::ws::tool(name)
}

/// Game installation: a name from local.toml [games] or a path.
fn game_path(arg: &str) -> PathBuf {
    crate::ws::games().into_iter().find(|(k, _)| k == arg).map(|(_, p)| p).unwrap_or_else(|| PathBuf::from(arg))
}

fn sha256(path: &Path) -> String {
    let mut h = Sha256::new();
    h.update(fs::read(path).unwrap_or_default());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

// not part of a mod: enabling (written by the launcher / installation), logs, files exchanged with dbnative, dumps
fn junk(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel).to_ascii_lowercase();
    name == "enabled.txt"
        || name.ends_with(".log")
        || name.ends_with(".dmp")
        || name.contains(".bak")
        || name.ends_with(".exp")
        || name.ends_with(".lib")
        || name.ends_with(".obj")
        || (name.starts_with("req") || name.starts_with("rsp")) && name.ends_with(".txt")
        || name.starts_with("gameover-") && name.ends_with(".bin")
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, PathBuf)>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(&p, base, out);
        } else {
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
            if !junk(&rel) {
                out.push((rel, p));
            }
        }
    }
}

/// Files of a part in the installation layout: (path from the game root, source). For repak the pak is not built yet: None.
fn item_files(it: &Item) -> Result<Vec<(String, Option<PathBuf>)>, String> {
    match (it.kind.as_str(), it.build.as_str()) {
        ("ue4ss", _) => {
            if !it.source.is_dir() {
                return Err(format!("{}: no folder {}", it.id, it.source.display()));
            }
            let mut files = Vec::new();
            walk(&it.source, &it.source, &mut files);
            Ok(files.into_iter().map(|(rel, p)| (format!("{WIN64}/ue4ss/Mods/{}/{rel}", it.name), Some(p))).collect())
        }
        ("pak", "copy") => {
            if it.source.extension().and_then(|e| e.to_str()) != Some("pak") || !it.source.is_file() {
                return Err(format!("{}: no file {}{}", it.id, it.source.display(), BRING_OWN));
            }
            let stem = it.source.with_extension("");
            let mut files = Vec::new();
            for ext in PAK_EXTS {
                let p = stem.with_extension(ext);
                if p.is_file() {
                    files.push((format!("{PAKS}/{}.{ext}", it.name), Some(p)));
                }
            }
            Ok(files)
        }
        ("pak", "repak") => {
            if !it.source.join("Dungeonborne").is_dir() {
                return Err(format!("{}: {} has no Dungeonborne folder{}", it.id, it.source.display(), BRING_OWN));
            }
            Ok(vec![(format!("{PAKS}/{}.pak", it.name), None)])
        }
        _ => Err(format!("{}: unknown kind/build {}/{}", it.id, it.kind, it.build)),
    }
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)?.flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()))?;
        } else {
            fs::copy(&p, to.join(e.file_name()))?;
        }
    }
    Ok(())
}

/// repak writes files into the pak in a different order from build to build (07.11: two paks from the same sources differed
/// in bytes) — the launcher package would change for no reason. Cache: the same set of sources -> the same built pak.
fn repak_pack(src: &Path, out: &Path) -> Result<(), String> {
    let mut files = Vec::new();
    walk(&src.join("Dungeonborne"), src, &mut files);
    let mut h = Sha256::new();
    for (rel, p) in &files {
        h.update(rel.as_bytes());
        h.update([0]);
        h.update(fs::read(p).unwrap_or_default());
    }
    let key: String = h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect();
    let cached = out_dir().join(".cache").join(format!("{}-{key}.pak", out.file_stem().unwrap().to_string_lossy()));
    if cached.is_file() {
        fs::copy(&cached, out).map_err(|e| e.to_string())?;
        return Ok(());
    }
    repak_build(src, out)?;
    fs::create_dir_all(cached.parent().unwrap()).map_err(|e| e.to_string())?;
    fs::copy(out, &cached).map_err(|e| e.to_string())?;
    Ok(())
}

fn repak_build(src: &Path, out: &Path) -> Result<(), String> {
    let repak = tool("repak").ok_or("repak not found: path in local.toml [tools] repak")?;
    // like deploy_paks.ps1 and zz_RelicQuality_P\build.ps1: only Dungeonborne/… goes into the pak, no README and scripts
    let stage = std::env::temp_dir().join(format!("xtask_mods_{}", out.file_stem().unwrap().to_string_lossy()));
    let _ = fs::remove_dir_all(&stage);
    copy_dir(&src.join("Dungeonborne"), &stage.join("Dungeonborne")).map_err(|e| e.to_string())?;
    let st = Command::new(&repak)
        .args(["pack", "--version", "V8B", "--mount-point", "../../../", "--quiet"])
        .arg(&stage)
        .arg(out)
        .status()
        .map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&stage);
    if st.success() { Ok(()) } else { Err(format!("repak pack {} -> code {st}", src.display())) }
}

fn git(args: &[&str]) -> String {
    Command::new("git").args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
}

fn in_profile<'a>(items: &'a [Item], profile: &str) -> Vec<&'a Item> {
    items.iter().filter(|i| i.profiles.iter().any(|p| p == profile)).collect()
}

/// Profile parts with the season considered: base ones always, season ones — only of the active season.
fn in_build<'a>(items: &'a [Item], profile: &str, season: Option<&str>) -> Vec<&'a Item> {
    in_profile(items, profile).into_iter().filter(|i| i.season.is_none() || i.season.as_deref() == season).collect()
}

/// Build season: --season=<id|none>, else the active one (seasons/active.txt), as for balance build.
fn build_season(flag: Option<&str>) -> Result<Option<String>, String> {
    crate::season::resolve(flag)
}

fn check_profile_arg(profile: Option<&str>) -> Result<String, String> {
    match profile {
        Some(p) if PROFILES.contains(&p) => Ok(p.to_string()),
        Some(p) => Err(format!("unknown profile {p}: {}", PROFILES.join(", "))),
        None => Err(format!("--profile=<{}> is required", PROFILES.join("|"))),
    }
}

pub fn command(rest: &[&str], profile: Option<&str>, game: Option<&str>, season: Option<&str>) -> bool {
    let r = match rest {
        ["build"] => check_profile_arg(profile).and_then(|p| build(&p, season).map(|_| ())),
        ["check"] => check(),
        ["install"] => check_profile_arg(profile).and_then(|p| install(&p, game, season)),
        ["verify"] => check_profile_arg(profile).and_then(|p| verify(&p, game)),
        _ => Err("mods build|check|install|verify (cargo xtask without arguments — help)".into()),
    };
    if let Err(e) = &r {
        eprintln!("{e}");
    }
    r.is_ok()
}

fn build(profile: &str, season_flag: Option<&str>) -> Result<PathBuf, String> {
    let items = load_catalog()?;
    let season = build_season(season_flag)?;
    println!("[{}]", crate::season::label(season.as_deref()));
    let out = out_dir().join(profile);
    let _ = fs::remove_dir_all(&out); // the pak cache (home/out/mods/.cache) is left alone
    let mut manifest_items = Vec::new();
    for it in in_build(&items, profile, season.as_deref()) {
        let mut files = serde_json::Map::new();
        for (dst_rel, src) in item_files(it)? {
            let dst = out.join(&dst_rel);
            fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
            match src {
                Some(src) => {
                    fs::copy(&src, &dst).map_err(|e| format!("{}: {e}", src.display()))?;
                }
                None => repak_pack(&it.source, &dst)?,
            }
            files.insert(dst_rel, sha256(&dst).into());
        }
        println!("  {:<22} {:<6} {:<24} {} file(s){}", it.id, it.kind, it.name, files.len(), if it.external() { "  (outside the repository)" } else { "" });
        manifest_items.push(serde_json::json!({ "id": it.id, "kind": it.kind, "name": it.name, "files": files }));
    }
    let manifest = serde_json::json!({
        "schema": "dbmods/1",
        "profile": profile,
        "season": season,
        "commit": git(&["rev-parse", "--short", "HEAD"]),
        "dirty": !git(&["status", "--porcelain"]).is_empty(),
        "items": manifest_items,
    });
    fs::write(out.join("manifest.json"), serde_json::to_string_pretty(&manifest).unwrap()).map_err(|e| e.to_string())?;
    println!("{}: {} parts, contents {}", out.display(), manifest["items"].as_array().unwrap().len(), out.join("manifest.json").display());
    Ok(out)
}

fn lua_compiles(lua: &Path, file: &Path) -> Result<(), String> {
    // compile only: loadfile without running (the file is passed as a string in -e, not as an argument — otherwise lua runs it)
    let p = file.to_string_lossy().replace('\\', "/");
    let o = Command::new(lua)
        .args(["-e", &format!("local f,e=loadfile([[{p}]]); if f then print('ok') else print('ERR '..e) end")])
        .output()
        .map_err(|e| e.to_string())?;
    let s = String::from_utf8_lossy(&o.stdout).trim().to_string();
    if s == "ok" { Ok(()) } else { Err(s) }
}

/// Pak contents: paths of the files inside (the IoStore stub .pak is not parsed — None).
fn pak_contents(it: &Item) -> Option<Vec<String>> {
    match it.build.as_str() {
        "repak" => {
            let mut files = Vec::new();
            walk(&it.source.join("Dungeonborne"), &it.source, &mut files);
            Some(files.into_iter().map(|(r, _)| r).collect())
        }
        _ => {
            if it.source.with_extension("ucas").is_file() {
                return None;
            }
            let o = Command::new(tool("repak")?).arg("list").arg(&it.source).output().ok()?;
            let list: Vec<String> = String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(|l| l.trim().trim_start_matches("../").to_string())
                .map(|l| { let mut s = l.as_str(); while let Some(r) = s.strip_prefix("../") { s = r; } s.to_string() })
                .filter(|l| !l.is_empty())
                .collect();
            if o.status.success() { Some(list) } else { None }
        }
    }
}

fn check() -> Result<(), String> {
    let items = load_catalog()?;
    let (mut errors, mut warns) = (Vec::<String>::new(), Vec::<String>::new());
    let ids: BTreeSet<&str> = items.iter().map(|i| i.id.as_str()).collect();
    // 1. catalog and profiles
    let mut seen = BTreeSet::new();
    for it in &items {
        if !seen.insert(it.id.as_str()) {
                errors.push(format!("{}: duplicate id", it.id));
        }
        for p in &it.profiles {
            if !PROFILES.contains(&p.as_str()) {
                errors.push(format!("{}: unknown profile {p}", it.id));
            }
            if it.dev && (p == "client" || p == "server") {
                errors.push(format!("{}: dev part in profile {p}", it.id));
            }
            if it.side == "server" && p.contains("client") {
                errors.push(format!("{}: server part in profile {p}", it.id));
            }
        }
        if let Some(s) = &it.season {
            if !crate::season::dir(s).join("season.toml").is_file() {
                errors.push(format!("{}: season = \"{s}\" — no seasons/{s}/season.toml", it.id));
            }
        }
        for r in &it.requires {
            if !ids.contains(r.as_str()) {
                errors.push(format!("{}: requires {r} — not in the catalog", it.id));
            } else if let Some(req) = items.iter().find(|x| &x.id == r) {
                for p in &it.profiles {
                    if !req.profiles.contains(p) {
                        errors.push(format!("{}: profile {p} lacks {r}, without which the part does not work", it.id));
                    }
                }
                if req.season.is_some() && req.season != it.season {
                    errors.push(format!("{}: requires {r} of season {:?}, but is itself {:?}", it.id, req.season, it.season));
                }
            }
        }
        match item_files(it) {
            Err(e) => errors.push(e),
            Ok(files) => {
                if it.external() {
                    warns.push(format!("{}: source outside the repository — {}", it.id, it.source.display()));
                }
                if it.kind == "pak" && it.build == "copy" {
                    let has = |e: &str| files.iter().any(|(r, _)| r.ends_with(&format!(".{e}")));
                    if has("ucas") != has("utoc") {
                        errors.push(format!("{}: IoStore pak without the .ucas/.utoc pair", it.id));
                    }
                }
            }
        }
    }
    // 2. Lua: compilation and forbidden calls of the service thread
    let lua = tool("lua");
    if lua.is_none() {
        warns.push("lua not found (local.toml [tools] lua) — Lua compilation not checked".into());
    }
    let mut lua_n = 0;
    for it in items.iter().filter(|i| i.kind == "ue4ss") {
        let mut files = Vec::new();
        walk(&it.source, &it.source, &mut files);
        for (rel, p) in files.iter().filter(|(r, _)| r.ends_with(".lua")) {
            lua_n += 1;
            if let Some(lua) = &lua {
                if let Err(e) = lua_compiles(lua, p) {
                    errors.push(format!("{}/{rel}: {e}", it.name));
                }
            }
            let text = fs::read_to_string(p).unwrap_or_default();
            for (n, line) in text.lines().enumerate() {
                if line.trim_start().starts_with("--") {
                    continue;
                }
                for b in BANNED {
                    if line.contains(b) {
                        errors.push(format!("{}/{rel}:{}: {b} — the UE4SS service thread is forbidden (MODS_DESIGN §5)", it.name, n + 1));
                    }
                }
            }
        }
    }
    // 3. dbnative.dll is built from the current source. Checked by content: build.bat writes dbnative.build.sha256
    // (hashes of dbnative.c and the dll); file times lie after git clone. Without the mark — by time, as before.
    let (dll, src) = (Path::new("game/ue4ss/Mods/DungeonborneRuClient/native/dbnative.dll"), Path::new("game/ue4ss/native/dbnative/dbnative.c"));
    let stamp = fs::read_to_string("game/ue4ss/native/dbnative/dbnative.build.sha256").ok();
    let rec = |k: &str| stamp.as_deref().and_then(|t| t.lines().find_map(|l| l.strip_prefix(k)).map(|v| v.trim().to_string()));
    if !dll.is_file() {
        errors.push("missing game/ue4ss/Mods/DungeonborneRuClient/native/dbnative.dll — build it: game/ue4ss/native/dbnative/build.bat".into());
    } else if let (Some(c), Some(d)) = (rec("c "), rec("d ")) {
        if sha256(src) != c {
            errors.push("dbnative.c changed after the dll was built — rebuild game/ue4ss/native/dbnative/build.bat".into());
        } else if sha256(dll) != d {
            warns.push("dbnative.dll is not the one recorded in dbnative.build.sha256 (not built by build.bat?)".into());
        }
    } else {
        let mtime = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
        if let (Some(d), Some(s)) = (mtime(dll), mtime(src)) {
            if d < s {
                errors.push("dbnative.dll is older than dbnative.c — rebuild game/ue4ss/native/dbnative/build.bat".into());
            }
        }
    }
    // 4. pak overrides in each profile (load order — by name, later = wins)
    let mut contents: BTreeMap<&str, Option<Vec<String>>> = BTreeMap::new();
    for it in items.iter().filter(|i| i.kind == "pak") {
        contents.insert(it.id.as_str(), pak_contents(it));
    }
    let mut shadow_reported = BTreeSet::new();
    for profile in PROFILES {
        let mut paks: Vec<&Item> = in_profile(&items, profile).into_iter().filter(|i| i.kind == "pak").collect();
        paks.sort_by(|a, b| a.name.cmp(&b.name));
        let mut owner: BTreeMap<String, &Item> = BTreeMap::new();
        let mut covered: BTreeMap<&str, usize> = BTreeMap::new();
        for it in &paks {
            let Some(Some(list)) = contents.get(it.id.as_str()) else { continue };
            for f in list {
                if let Some(prev) = owner.get(f) {
                    *covered.entry(prev.id.as_str()).or_default() += 1;
                    if !it.overrides.contains(&prev.name) {
                        errors.push(format!("[{profile}] {f}: present in {} and {} — add overrides = [\"{}\"] to {}, if intended", prev.name, it.name, prev.name, it.id));
                    }
                }
                owner.insert(f.clone(), it);
            }
        }
        for it in &paks {
            if let (Some(Some(list)), Some(n)) = (contents.get(it.id.as_str()), covered.get(it.id.as_str())) {
                if *n == list.len() && shadow_reported.insert(it.id.as_str()) {
                    warns.push(format!("{}: all {} files are overridden by other paks — the pak has no effect", it.name, n));
                }
            }
        }
    }
    let unread: Vec<&str> = contents.iter().filter(|(_, c)| c.is_none()).map(|(id, _)| *id).collect();
    for w in &warns {
        println!("  warning: {w}");
    }
    for e in &errors {
        println!("  ERROR: {e}");
    }
    println!(
        "check: parts {}, Lua files {lua_n}, paks without content inspection (IoStore): {}; errors {}, warnings {}",
        items.len(),
        if unread.is_empty() { "none".into() } else { unread.join(", ") },
        errors.len(),
        warns.len()
    );
    if errors.is_empty() { Ok(()) } else { Err(format!("check: {} errors", errors.len())) }
}

fn game_running() -> bool {
    Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Dungeonborne.exe", "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("dungeonborne.exe"))
        .unwrap_or(true)
}

fn game_root(game: Option<&str>) -> Result<PathBuf, String> {
    let g = game_path(game.ok_or("--game=<name from local.toml [games] or path> is required")?);
    if !g.join(WIN64).join("Dungeonborne.exe").is_file() {
        return Err(format!("{}: not a game installation (no {WIN64}/Dungeonborne.exe)", g.display()));
    }
    Ok(g)
}

fn install(profile: &str, game: Option<&str>, season: Option<&str>) -> Result<(), String> {
    let root = game_root(game)?;
    if game_running() {
        return Err("the game is running — close it (dbnative.dll and the paks are locked)".into());
    }
    let out = build(profile, season)?;
    let manifest: serde_json::Value = serde_json::from_str(&fs::read_to_string(out.join("manifest.json")).unwrap()).unwrap();
    let (mut same, mut copied) = (0, 0);
    for it in manifest["items"].as_array().unwrap() {
        for (rel, sha) in it["files"].as_object().unwrap() {
            let dst = root.join(rel);
            if dst.is_file() && sha256(&dst) == sha.as_str().unwrap() {
                same += 1;
                continue;
            }
            fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
            fs::copy(out.join(rel), &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
            println!("  -> {rel}");
            copied += 1;
        }
    }
    println!("{}: profile {profile}, copied {copied}, unchanged {same}", root.display());
    verify(profile, game)
}

fn enabled_mods(mods: &Path) -> BTreeSet<String> {
    let mut on = BTreeSet::new();
    for line in fs::read_to_string(mods.join("mods.txt")).unwrap_or_default().lines() {
        let line = line.trim();
        if line.starts_with(';') {
            continue;
        }
        if let Some((name, v)) = line.split_once(':') {
            if v.trim() == "1" {
                on.insert(name.trim().to_string());
            }
        }
    }
    if let Ok(rd) = fs::read_dir(mods) {
        for e in rd.flatten() {
            if e.path().join("enabled.txt").is_file() {
                on.insert(e.file_name().to_string_lossy().to_string());
            }
        }
    }
    on
}

fn verify(profile: &str, game: Option<&str>) -> Result<(), String> {
    let root = game_root(game)?;
    let mpath = out_dir().join(profile).join("manifest.json");
    let manifest: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&mpath).map_err(|_| format!("missing {} — run mods build --profile={profile} first", mpath.display()))?)
            .map_err(|e| e.to_string())?;
    let (mut same, mut diff, mut missing) = (0, Vec::new(), Vec::new());
    let mut names = BTreeSet::new();
    let mut paks = BTreeSet::new();
    for it in manifest["items"].as_array().unwrap() {
        let name = it["name"].as_str().unwrap().to_string();
        if it["kind"] == "ue4ss" { names.insert(name.clone()); } else { paks.insert(name.clone()); }
        for (rel, sha) in it["files"].as_object().unwrap() {
            let p = root.join(rel);
            if !p.is_file() {
                missing.push(rel.clone());
            } else if sha256(&p) == sha.as_str().unwrap() {
                same += 1;
            } else {
                diff.push(rel.clone());
            }
        }
    }
    let mods = root.join(WIN64).join("ue4ss/Mods");
    let on = enabled_mods(&mods);
    let off: Vec<&String> = names.iter().filter(|n| !on.contains(*n)).collect();
    let extra: Vec<&String> = on.iter().filter(|n| !names.contains(*n) && !RUNTIME_MODS.contains(&n.as_str()) && mods.join(n).is_dir()).collect();
    let mut extra_paks = BTreeSet::new();
    if let Ok(rd) = fs::read_dir(root.join(PAKS)) {
        for e in rd.flatten() {
            let f = e.file_name().to_string_lossy().to_string();
            if let Some(stem) = f.strip_suffix(".pak") {
                if stem.starts_with('z') && !paks.contains(stem) {
                    extra_paks.insert(stem.to_string());
                }
            }
        }
    }
    println!("verify {} [{profile}, contents {}{}]:", root.display(), manifest["commit"].as_str().unwrap_or("?"), if manifest["dirty"] == true { "+edits" } else { "" });
    println!("  files: match {same}, differ {}, missing {}", diff.len(), missing.len());
    for d in &diff { println!("    differs: {d}"); }
    for m in &missing { println!("    missing: {m}"); }
    if !off.is_empty() { println!("  profile mods not enabled (mods.txt/enabled.txt): {}", off.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")); }
    if !extra.is_empty() { println!("  enabled, but not in the profile: {}", extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")); }
    if !extra_paks.is_empty() { println!("  paks not from the profile: {}", extra_paks.into_iter().collect::<Vec<_>>().join(", ")); }
    if diff.is_empty() && missing.is_empty() { Ok(()) } else { Err("verify: the installation does not match the contents".into()) }
}
