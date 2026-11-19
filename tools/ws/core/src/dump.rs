//! ws dump — game data from the game reference into home/dumps/<build>/export (docs/GAME_DATA.md).
//!
//! The game reference is home/game/original/<build> (ws game original, from the person's own Steam). The sets of
//! assets are in tools/dumper/sets.toml; the work is done by dbdump (tools/dumper, C# over CUE4Parse), which this
//! module builds with the .NET SDK and runs once per set. The pak key goes to dbdump only in its environment
//! (GAME_AES_KEY: an open vault session — `ws secrets unlock`, `ws secrets run game_aes_key -- ws dump ...`, or
//! local.toml [re] aes_key without a vault) — never on a command line.
//! After the run: export/ in the layout of an FModel export (Content/TGF/...), index/<set>.tsv (dbdump's result per
//! package) and manifest.tsv (sha256, size, path of every file — two dumps of one build compare by it).
//! Decrypting the paks is done only on a person's word (docs/RESEARCH.md): without --apply this is the plan only.
use crate::{str_list, Finding, Level, Workspace};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const SETS: &str = "tools/dumper/sets.toml";
const PROJECT: &str = "tools/dumper/dbdump.csproj";

#[derive(Debug, Clone)]
pub struct Set {
    pub name: String,
    pub format: String,
    pub about: String,
    pub patterns: Vec<String>,
}

pub struct Options {
    pub apply: bool,
    /// game build (folder in home/game/original); None — the only one there
    pub build: Option<String>,
    /// set names; empty — all
    pub sets: Vec<String>,
}

/// `ws dump paks`: the blueprint patch paks (BPFix, ChatFix, ServerNet) built from the game reference by
/// game/bp-patches/tools/build_paks.py (retoc + UAssetGUI + the patch scripts). The key goes to it only in its
/// environment, like to dbdump. Decrypting the paks is done only on a person's word: without --apply — the plan.
pub fn paks(ws: &Workspace, apply: bool, build: Option<&str>, only: Option<&str>) -> Vec<Finding> {
    let mut out = Vec::new();
    let script = ws.root.join("game/bp-patches/tools/build_paks.py");
    if !script.is_file() {
        out.push(Finding::new(
            Level::Error,
            "dump",
            "paks",
            "game/bp-patches is not on this machine: cargo ws components add mods --apply"
                .to_string(),
        ));
        return out;
    }
    let mut missing = false;
    let (build, reference) = match reference(ws, build) {
        Ok(r) => r,
        Err(e) => {
            out.push(Finding::new(Level::Error, "dump", "reference", e));
            (String::new(), PathBuf::new())
        }
    };
    let paks_dir = reference.join("Dungeonborne").join("Content").join("Paks");
    if build.is_empty() || !paks_dir.is_dir() {
        missing = true;
        if !build.is_empty() {
            out.push(Finding::new(
                Level::Error,
                "dump",
                "paks",
                "no Dungeonborne/Content/Paks in the game reference".to_string(),
            ));
        }
    }
    let key = key(ws, apply);
    if key.is_none() {
        missing = true;
        out.push(Finding::new(
            Level::Error,
            "dump",
            "key",
            "no pak key - config/local.toml [re] aes_key (public, you find it yourself: docs/SECRETS.md, \"The pak key\"), or the vault session (ws secrets unlock)".to_string(),
        ));
    }
    if missing {
        return out;
    }
    let only = only.unwrap_or("bpfix,chatfix,servernet,lipatch");
    if !apply {
        out.push(Finding::new(
            Level::Warn,
            "dump",
            "plan",
            format!(
                "build {build}: {only} -> game/bp-patches/build. This decrypts the game paks with the key: only on a person's word - run with --apply"
            ),
        ));
        return out;
    }
    let python = std::env::var_os("DB_PYTHON")
        .map(PathBuf::from)
        .or_else(|| crate::bootstrap::portable_python(ws))
        .unwrap_or_else(|| PathBuf::from("python"));
    let st = Command::new(python)
        .arg("-I")
        .arg("-B") // the tools' __pycache__ is tracked in git
        .arg(&script)
        .arg("--paks")
        .arg(&paks_dir)
        .arg("--only")
        .arg(only)
        .arg("--work")
        .arg(ws.home().join("cache").join("tmp").join("bp-paks"))
        .env("GAME_AES_KEY", key.unwrap_or_default())
        .status();
    out.push(match st {
        Ok(s) if s.success() => Finding::new(
            Level::Done,
            "dump",
            "paks",
            format!("built {only} -> game/bp-patches/build (home/cache/tmp/bp-paks holds decrypted assets: private)"),
        ),
        Ok(s) => Finding::new(Level::Error, "dump", "paks", format!("build_paks.py failed ({s})")),
        Err(e) => Finding::new(Level::Error, "dump", "paks", format!("python: {e}")),
    });
    out
}

pub fn sets(ws: &Workspace) -> Result<Vec<Set>, String> {
    let doc = crate::read_toml(&ws.root.join(SETS))?;
    let s = |v: &toml::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Ok(doc
        .get("set")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .map(|v| Set {
            name: s(v, "name"),
            format: s(v, "format"),
            about: s(v, "about"),
            patterns: str_list(v.get("patterns")),
        })
        .collect())
}

/// The game reference: home/game/original/<build> with original.toml (ws game original).
fn reference(ws: &Workspace, build: Option<&str>) -> Result<(String, PathBuf), String> {
    let root = crate::game::original_root(ws);
    let mut builds: Vec<String> = std::fs::read_dir(&root)
        .map_err(|_| {
            format!(
                "no game reference in {} - download your build from Steam and register it: ws game original (docs/GAME_DATA.md)",
                root.display()
            )
        })?
        .flatten()
        .filter(|e| e.path().join("original.toml").is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    builds.sort();
    let pick = match build {
        Some(b) if builds.iter().any(|x| x == b) => b.to_string(),
        Some(b) => {
            return Err(format!(
                "no build {b} in {} (there: {})",
                root.display(),
                builds.join(", ")
            ))
        }
        None if builds.len() == 1 => builds[0].clone(),
        None if builds.is_empty() => {
            return Err(format!(
                "no game reference in {} - ws game original (docs/GAME_DATA.md)",
                root.display()
            ))
        }
        None => {
            return Err(format!(
                "several builds in {}: {} - choose with --build=",
                root.display(),
                builds.join(", ")
            ))
        }
    };
    let dir = root.join(&pick);
    Ok((pick, dir))
}

/// The mappings file: local.toml [re] usmap, else the first *.usmap in home/re/ue4ss (artifact "game usmap").
fn usmap(ws: &Workspace) -> Option<PathBuf> {
    if let Some(p) = ws.local_str("re", "usmap").and_then(|p| ws.resolve(&p)) {
        return p.is_file().then_some(p);
    }
    let dir = ws.home().join("re").join("ue4ss");
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("usmap"))
        })
        .collect();
    found.sort();
    found.into_iter().next()
}

/// Oodle: home/tools/oodle (tools.lock, pinned by sha256), else the DLL retoc or repak fetched next to itself.
fn oodle(ws: &Workspace) -> Option<PathBuf> {
    ["oodle", "retoc", "repak"]
        .iter()
        .map(|t| ws.home().join("tools").join(t).join("oo2core_9_win64.dll"))
        .find(|p| p.is_file())
}

/// The pak key: the environment, else the vault session (asked as this command, `ws dump …`), else the old
/// local.toml [re] aes_key. Without --apply only whether there is one: the session is not asked for the value.
fn key(ws: &Workspace, apply: bool) -> Option<String> {
    let local = || {
        ws.local_str("re", "aes_key")
            .filter(|k| !k.trim().is_empty())
    };
    if !apply {
        let in_env = std::env::var("GAME_AES_KEY").is_ok_and(|k| !k.trim().is_empty());
        let in_session =
            crate::vault::status(ws).is_some_and(|s| s.names().iter().any(|n| n == "game_aes_key"));
        return (in_env || in_session).then(String::new).or_else(local);
    }
    crate::vault::env_or_session(ws, "game_aes_key", "GAME_AES_KEY").or_else(local)
}

/// The .NET SDK: home/toolchains/dotnet (tools.lock, on request) or `dotnet` from PATH.
fn dotnet(ws: &Workspace) -> Option<PathBuf> {
    let home = ws
        .home()
        .join("toolchains")
        .join("dotnet")
        .join("dotnet.exe");
    if home.is_file() {
        return Some(home);
    }
    Command::new("dotnet")
        .arg("--list-sdks")
        .output()
        .ok()
        .filter(|o| o.status.success() && !o.stdout.is_empty())
        .map(|_| PathBuf::from("dotnet"))
}

fn dumper_dir(ws: &Workspace) -> PathBuf {
    ws.home().join("tools").join("dbdump")
}

/// dbdump.exe is older than its sources (or missing) — build it.
fn stale(ws: &Workspace) -> bool {
    let exe = dumper_dir(ws).join("dbdump.exe");
    let Ok(built) = std::fs::metadata(&exe).and_then(|m| m.modified()) else {
        return true;
    };
    ["Program.cs", "dbdump.csproj", "packages.lock.json"]
        .iter()
        .any(|f| {
            std::fs::metadata(ws.root.join("tools/dumper").join(f))
                .and_then(|m| m.modified())
                .is_ok_and(|t| t > built)
        })
}

pub fn run(ws: &Workspace, opt: &Options) -> Vec<Finding> {
    let mut out = Vec::new();
    let err = |out: &mut Vec<Finding>, what: &str, msg: String| {
        out.push(Finding::new(Level::Error, "dump", what, msg))
    };
    // the dumper and what it needs (Oodle, usmap, .NET) come with the component research (config/components.toml)
    if !ws.root.join("tools/dumper/member.toml").is_file() {
        err(
            &mut out,
            "component",
            "the game data tools are not on this machine: cargo ws components add research --apply (DevKit: \
             Components), then cargo ws bootstrap --apply (Oodle, usmap) and cargo xtask setup --apply --tool=dotnet"
                .into(),
        );
        return out;
    }
    let all = match sets(ws) {
        Ok(s) => s,
        Err(e) => {
            err(&mut out, SETS, e);
            return out;
        }
    };
    let chosen: Vec<Set> = if opt.sets.is_empty() {
        all.clone()
    } else {
        let mut v = Vec::new();
        for n in &opt.sets {
            match all.iter().find(|s| &s.name == n) {
                Some(s) => v.push(s.clone()),
                None => {
                    let names: Vec<&str> = all.iter().map(|s| s.name.as_str()).collect();
                    err(
                        &mut out,
                        "--set",
                        format!("no set '{n}' (there: {})", names.join(", ")),
                    );
                    return out;
                }
            }
        }
        v
    };
    // everything that is missing is listed at once: a newcomer fixes it in one go, not one error per run
    let mut missing = false;
    let (build, reference) = match reference(ws, opt.build.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            err(&mut out, "reference", e);
            missing = true;
            (String::new(), PathBuf::new())
        }
    };
    let paks = reference.join("Dungeonborne").join("Content").join("Paks");
    let mut need =
        |out: &mut Vec<Finding>, what: &str, p: Option<PathBuf>, hint: &str| -> PathBuf {
            match p {
                Some(p) => {
                    out.push(Finding::new(
                        Level::Info,
                        "dump",
                        what,
                        p.display().to_string(),
                    ));
                    p
                }
                None => {
                    missing = true;
                    out.push(Finding::new(Level::Error, "dump", what, hint.to_string()));
                    PathBuf::new()
                }
            }
        };
    let paks = if build.is_empty() {
        PathBuf::new() // no reference: already said
    } else {
        need(
            &mut out,
            "paks",
            paks.is_dir().then_some(paks),
            "no Dungeonborne/Content/Paks in the game reference",
        )
    };
    let usmap = need(
        &mut out,
        "usmap",
        usmap(ws),
        "no *.usmap in home/re/ue4ss - tools\\stand\\usmap.ps1 makes it from your copy of the game (after ws stand --apply; docs/GAME_DATA.md; or the artifact \"game usmap\" (ws bootstrap))",
    );
    let oodle = need(
        &mut out,
        "oodle",
        oodle(ws),
        "no oo2core_9_win64.dll - ws bootstrap --apply (tools.lock: oodle, component research)",
    );
    let dotnet = need(
        &mut out,
        ".NET SDK",
        dotnet(ws),
        "no .NET SDK - cargo xtask setup --apply --tool=dotnet (tools.lock, on request)",
    );
    let key = key(ws, opt.apply);
    if key.is_none() {
        missing = true;
        err(
            &mut out,
            "key",
            "no pak key - config/local.toml [re] aes_key (public, you find it yourself: docs/SECRETS.md, \"The pak key\"), or the vault session (ws secrets unlock, or DevKit), or ws secrets run game_aes_key -- ws dump ...".into(),
        );
    }
    if missing {
        return out;
    }
    let dumps = ws.home().join("dumps").join(&build);
    let export = dumps.join("export");
    let names: Vec<&str> = chosen.iter().map(|s| s.name.as_str()).collect();
    if !opt.apply {
        out.push(Finding::new(
            Level::Warn,
            "dump",
            "plan",
            format!(
                "build {build}: sets {} -> {}{}. This decrypts the game paks with the key: only on a person's word (docs/RESEARCH.md) - run with --apply",
                names.join(", "),
                export.display(),
                if stale(ws) { "; dbdump will be built first (.NET SDK, NuGet packages by packages.lock.json)" } else { "" }
            ),
        ));
        return out;
    }
    if stale(ws) {
        let st = Command::new(&dotnet)
            .args(["build", "-c", "Release", "--nologo", "-o"])
            .arg(dumper_dir(ws))
            .arg(ws.root.join(PROJECT))
            .env("DOTNET_CLI_TELEMETRY_OPTOUT", "1")
            .env("DOTNET_NOLOGO", "1")
            .status();
        match st {
            Ok(s) if s.success() => out.push(Finding::new(
                Level::Done,
                "dump",
                "dbdump",
                format!("built -> {}", dumper_dir(ws).display()),
            )),
            Ok(s) => {
                err(&mut out, "dbdump", format!("dotnet build failed ({s})"));
                return out;
            }
            Err(e) => {
                err(&mut out, "dbdump", format!("dotnet: {e}"));
                return out;
            }
        }
    }
    let tmp = ws.home().join("cache").join("tmp");
    let index_dir = dumps.join("index");
    if let Err(e) = std::fs::create_dir_all(&tmp).and_then(|_| std::fs::create_dir_all(&index_dir))
    {
        err(&mut out, "dumps", e.to_string());
        return out;
    }
    let key = key.unwrap_or_default();
    for set in &chosen {
        let list = tmp.join(format!("dump-{}.txt", set.name));
        let text: String = set
            .patterns
            .iter()
            .map(|p| format!("{} {p}\n", set.format))
            .collect();
        if let Err(e) = std::fs::write(&list, text) {
            err(&mut out, &set.name, e.to_string());
            return out;
        }
        println!("== dump {} ({})", set.name, set.about);
        let st = Command::new(dumper_dir(ws).join("dbdump.exe"))
            .arg("--paks")
            .arg(&paks)
            .arg("--usmap")
            .arg(&usmap)
            .arg("--oodle")
            .arg(&oodle)
            .arg("--out")
            .arg(&export)
            .arg("--list")
            .arg(&list)
            .env("GAME_AES_KEY", &key)
            .status();
        let _ = std::fs::remove_file(&list);
        let moved = std::fs::rename(
            export.join("index.tsv"),
            index_dir.join(format!("{}.tsv", set.name)),
        );
        match st {
            Ok(s) if s.success() => out.push(Finding::new(
                Level::Done,
                "dump",
                &set.name,
                "exported, index/".to_string() + &set.name + ".tsv",
            )),
            Ok(s) if s.code() == Some(3) => out.push(Finding::new(
                Level::Warn,
                "dump",
                &set.name,
                format!(
                    "exported with errors - see index/{}.tsv (lines with 'error')",
                    set.name
                ),
            )),
            Ok(s) => {
                err(&mut out, &set.name, format!("dbdump failed ({s})"));
                return out;
            }
            Err(e) => {
                err(&mut out, &set.name, format!("dbdump: {e}"));
                return out;
            }
        }
        if let Err(e) = moved {
            out.push(Finding::new(
                Level::Warn,
                "dump",
                &set.name,
                format!("index.tsv: {e}"),
            ));
        }
    }
    match manifest(&export, &dumps.join("manifest.tsv")) {
        Ok((files, bytes)) => out.push(Finding::new(
            Level::Done,
            "dump",
            "manifest.tsv",
            format!(
                "{files} files, {:.1} MB in {} (sha256 of each)",
                bytes as f64 / 1e6,
                export.display()
            ),
        )),
        Err(e) => err(&mut out, "manifest.tsv", e),
    }
    out
}

/// manifest.tsv: "sha256 \t size \t path" of every file under `dir`, paths with '/', sorted — the same dump gives the
/// same manifest. Returns (files, bytes).
pub fn manifest(dir: &Path, to: &Path) -> Result<(usize, u64), String> {
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut text = String::new();
    let mut bytes = 0u64;
    for rel in &files {
        let data = std::fs::read(dir.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        bytes += data.len() as u64;
        let h = Sha256::digest(&data);
        let hex: String = h.iter().map(|b| format!("{b:02x}")).collect();
        text.push_str(&format!("{hex}\t{}\t{rel}\n", data.len()));
    }
    std::fs::write(to, text).map_err(|e| format!("{}: {e}", to.display()))?;
    Ok((files.len(), bytes))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for e in std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out)?;
        } else {
            out.push(crate::rel(root, &p));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_sorted_and_stable() {
        let d = std::env::temp_dir().join(format!("ws-dump-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("export/Content/B")).unwrap();
        std::fs::write(d.join("export/Content/B/z.json"), b"{}").unwrap();
        std::fs::write(d.join("export/Content/a.png"), b"png").unwrap();
        let (n, bytes) = manifest(&d.join("export"), &d.join("m1.tsv")).unwrap();
        manifest(&d.join("export"), &d.join("m2.tsv")).unwrap();
        let m1 = std::fs::read_to_string(d.join("m1.tsv")).unwrap();
        let m2 = std::fs::read_to_string(d.join("m2.tsv")).unwrap();
        std::fs::remove_dir_all(&d).ok();
        assert_eq!((n, bytes), (2, 5));
        assert_eq!(m1, m2);
        let paths: Vec<&str> = m1.lines().map(|l| l.rsplit('\t').next().unwrap()).collect();
        assert_eq!(paths, vec!["Content/B/z.json", "Content/a.png"]);
    }

    #[test]
    fn repo_sets_parse() {
        let ws = Workspace::find(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let s = sets(&ws).unwrap();
        assert!(s.iter().any(|x| x.name == "icons" && x.format == "png"));
        assert!(s
            .iter()
            .all(|x| (x.format == "png" || x.format == "json") && !x.patterns.is_empty()));
    }
}
