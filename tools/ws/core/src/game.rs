//! ws game clone — a working copy of the game from the reference (docs/WORKSPACE_PLAN.md, section 3):
//! home/game/original/<build> → home/game/<name>. Game paks (Content/Paks/*.pak|ucas|utoc|sig) are hard links
//! to the reference (three copies weigh almost as one), everything else is copied. Linked files are marked "read-only":
//! the attribute is shared by links, so an accidental overwrite of a pak in a copy will not spoil the reference but fail with an error.
//! Own mod paks (zz*_P) are put next to them under their own names and do not touch the reference.
use crate::{Finding, Level};
use std::path::{Path, PathBuf};

const LINK_EXT: [&str; 4] = ["pak", "ucas", "utoc", "sig"];

#[derive(Default, Debug)]
pub struct Stats {
    pub linked: u64,
    pub copied: u64,
    pub skipped: u64,
    pub bytes_copied: u64,
}

pub fn clone(src: &Path, dst: &Path, apply: bool) -> Result<(Stats, Vec<Finding>), String> {
    if !src
        .join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
        .is_file()
    {
        return Err(format!(
            "{}: not a game install (no Dungeonborne/Binaries/Win64/Dungeonborne.exe)",
            src.display()
        ));
    }
    let mut st = Stats::default();
    let mut notes = Vec::new();
    walk(src, src, dst, apply, &mut st, &mut notes)?;
    Ok((st, notes))
}

fn is_game_pak(rel: &Path) -> bool {
    let ext = rel
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let in_paks = rel.components().any(|c| c.as_os_str() == "Paks");
    in_paks && LINK_EXT.contains(&ext.as_str())
}

fn walk(
    root: &Path,
    dir: &Path,
    dst_root: &Path,
    apply: bool,
    st: &mut Stats,
    notes: &mut Vec<Finding>,
) -> Result<(), String> {
    let rd = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for e in rd.flatten() {
        let p = e.path();
        let rel = p.strip_prefix(root).unwrap();
        let to = dst_root.join(rel);
        if p.is_dir() {
            if apply {
                std::fs::create_dir_all(&to).map_err(|e| format!("{}: {e}", to.display()))?;
            }
            walk(root, &p, dst_root, apply, st, notes)?;
            continue;
        }
        if to.exists() {
            st.skipped += 1;
            continue;
        }
        if is_game_pak(rel) {
            st.linked += 1;
            if apply {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                if let Err(err) = std::fs::hard_link(&p, &to) {
                    // another volume: a link cannot be made — copy
                    notes.push(Finding::new(
                        Level::Warn,
                        "game",
                        rel.display().to_string(),
                        format!("hard link failed ({err}), copied"),
                    ));
                    st.bytes_copied += std::fs::copy(&p, &to).map_err(|e| e.to_string())?;
                }
                let mut perm = std::fs::metadata(&p)
                    .map_err(|e| e.to_string())?
                    .permissions();
                if !perm.readonly() {
                    perm.set_readonly(true);
                    let _ = std::fs::set_permissions(&p, perm);
                }
            }
        } else {
            st.copied += 1;
            if apply {
                if let Some(parent) = to.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                st.bytes_copied +=
                    std::fs::copy(&p, &to).map_err(|e| format!("{}: {e}", to.display()))?;
            } else {
                st.bytes_copied += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok(())
}

/// List of installation files: path from the root (with "/") -> size.
pub fn listing(root: &Path) -> std::collections::BTreeMap<String, u64> {
    fn walk(root: &Path, dir: &Path, out: &mut std::collections::BTreeMap<String, u64>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else if let Ok(m) = e.metadata() {
                out.insert(crate::rel(root, &p), m.len());
            }
        }
    }
    let mut out = Default::default();
    walk(root, root, &mut out);
    out
}

/// ws game original — a depot downloaded from Steam (download_depot) becomes the reference home/game/original/<build>:
/// move (rename on the same disk), original.toml with Steam numbers and the file list, "read-only".
pub fn adopt_original(
    src: &Path,
    dst: &Path,
    build: &str,
    depot: &str,
    manifest: &str,
    apply: bool,
) -> Result<String, String> {
    if !src
        .join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
        .is_file()
    {
        return Err(format!(
            "{}: no Dungeonborne/Binaries/Win64/Dungeonborne.exe - download not finished?",
            src.display()
        ));
    }
    if dst.exists() {
        return Err(format!(
            "{} already exists - the original of this build is never overwritten",
            dst.display()
        ));
    }
    let files = listing(src);
    let total: u64 = files.values().sum();
    let summary = format!(
        "{} files, {:.2} GB: {} -> {}",
        files.len(),
        total as f64 / 1e9,
        src.display(),
        dst.display()
    );
    if !apply {
        return Ok(format!("{summary} - dry run; to do it: --apply"));
    }
    std::fs::create_dir_all(dst.parent().unwrap()).map_err(|e| e.to_string())?;
    if std::fs::rename(src, dst).is_err() {
        crate::copy_tree(src, dst).map_err(|e| format!("copy: {e}"))?;
    }
    write_original(
        dst,
        build,
        depot,
        manifest,
        &files,
        "a clean download from Steam (download_depot)",
    )?;
    Ok(format!("original ready: {summary}"))
}

/// original.toml (Steam numbers and the file list) and "read-only" on the reference files.
fn write_original(
    dst: &Path,
    build: &str,
    depot: &str,
    manifest: &str,
    files: &std::collections::BTreeMap<String, u64>,
    source: &str,
) -> Result<(), String> {
    let total: u64 = files.values().sum();
    let mut toml = format!(
        "# Game reference: {source}, docs/GAME_DATA.md. Do not edit by hand.\n\
         build = \"{build}\"\napp = \"2448970\"\ndepot = \"{depot}\"\nmanifest = \"{manifest}\"\nfiles = {}\nbytes = {total}\n\n[size]\n",
        files.len()
    );
    for (f, s) in files {
        toml.push_str(&format!("\"{}\" = {s}\n", f.replace('"', "\\\"")));
    }
    std::fs::write(dst.join("original.toml"), toml).map_err(|e| e.to_string())?;
    for f in files.keys() {
        let p = dst.join(f);
        if let Ok(m) = std::fs::metadata(&p) {
            let mut perm = m.permissions();
            perm.set_readonly(true);
            let _ = std::fs::set_permissions(&p, perm);
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------- the game installed by Steam
// 18.11: everyone needs their own copy of the game (docs/GAME_DATA.md). The usual one is a normal Steam install
// (steamapps/common/Dungeonborne). It is never moved or changed: only the files of the clean build
// (config/game-<build>.files.tsv, path and size) are copied into the reference; mods and saves next to them are skipped.

/// The files of a clean build: config/game-<build>.files.tsv (path from the install root, size).
pub fn clean_files(
    ws: &crate::Workspace,
    build: &str,
) -> Option<std::collections::BTreeMap<String, u64>> {
    let text = std::fs::read_to_string(ws.config(&format!("game-{build}.files.tsv"))).ok()?;
    let files: std::collections::BTreeMap<String, u64> = text
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let (p, s) = l.split_once('\t')?;
            Some((p.to_string(), s.trim().parse().ok()?))
        })
        .collect();
    (!files.is_empty()).then_some(files)
}

/// Steam libraries: the Steam folder and the folders of steamapps/libraryfolders.vdf.
pub fn steam_libraries() -> Vec<PathBuf> {
    let Some(steam) = steam_path() else {
        return Vec::new();
    };
    let mut libs = vec![steam.clone()];
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf"))
        .unwrap_or_default();
    for line in vdf.lines() {
        let parts: Vec<&str> = line.split('"').collect();
        // \t"path"\t\t"C:\\Steam"  ->  ["\t", "path", "\t\t", "C:\\Steam", ""]
        if parts.len() >= 4 && parts[1] == "path" {
            let p = crate::native(Path::new(&parts[3].replace("\\\\", "\\")));
            if !libs.iter().any(|l| {
                l.to_string_lossy()
                    .eq_ignore_ascii_case(&p.to_string_lossy())
            }) {
                libs.push(p);
            }
        }
    }
    libs
}

/// The game installed by Steam: its folder and Steam's build number (appmanifest_<app>.acf).
#[derive(Debug, Clone)]
pub struct Installed {
    pub path: PathBuf,
    pub buildid: String,
}

pub fn installed(b: &Build) -> Option<Installed> {
    let field = |text: &str, key: &str| {
        text.lines().find_map(|l| {
            let parts: Vec<&str> = l.split('"').collect();
            (parts.len() >= 4 && parts[1] == key).then(|| parts[3].to_string())
        })
    };
    steam_libraries().into_iter().find_map(|lib| {
        let acf = std::fs::read_to_string(
            lib.join("steamapps")
                .join(format!("appmanifest_{}.acf", b.app)),
        )
        .ok()?;
        let dir = lib
            .join("steamapps")
            .join("common")
            .join(field(&acf, "installdir")?);
        dir.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
            .is_file()
            .then(|| Installed {
                path: dir,
                buildid: field(&acf, "buildid").unwrap_or_default(),
            })
    })
}

/// Does an install hold every file of the clean build with its size? Err: what is missing or different.
pub fn check_clean(
    src: &Path,
    clean: &std::collections::BTreeMap<String, u64>,
) -> Result<(), String> {
    let (mut missing, mut differs) = (Vec::new(), Vec::new());
    for (f, size) in clean {
        match std::fs::metadata(src.join(f)) {
            Ok(m) if m.len() == *size => {}
            Ok(_) => differs.push(f.clone()),
            Err(_) => missing.push(f.clone()),
        }
    }
    if missing.is_empty() && differs.is_empty() {
        return Ok(());
    }
    let some = |v: &[String]| v.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
    Err(format!(
        "{} file(s) of the clean build missing{}, {} with another size{} (a modded or broken install)",
        missing.len(),
        if missing.is_empty() { String::new() } else { format!(" ({})", some(&missing)) },
        differs.len(),
        if differs.is_empty() { String::new() } else { format!(" ({})", some(&differs)) },
    ))
}

/// Free bytes on the disk of `p` (its nearest existing folder).
#[cfg(windows)]
pub fn free_bytes(p: &Path) -> Option<u64> {
    use std::os::windows::ffi::OsStrExt;
    let dir = p.ancestors().find(|a| a.exists())?;
    let wide: Vec<u16> = dir
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free = 0u64;
    // SAFETY: a NUL-terminated path and an out pointer; the other outputs are not asked for
    let ok = unsafe {
        windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    (ok != 0).then_some(free)
}

#[cfg(not(windows))]
pub fn free_bytes(_: &Path) -> Option<u64> {
    None
}

/// ws game original installed — copies the clean files of a Steam install into the reference (the install stays as it
/// is). The free space is checked first.
pub fn adopt_installed(
    src: &Path,
    dst: &Path,
    b: &Build,
    clean: &std::collections::BTreeMap<String, u64>,
    apply: bool,
) -> Result<String, String> {
    if dst.exists() {
        return Err(format!(
            "{} already exists - the original of this build is never overwritten",
            dst.display()
        ));
    }
    check_clean(src, clean)?;
    let total: u64 = clean.values().sum();
    let summary = format!(
        "{} files, {:.2} GB: {} -> {} (a copy; the install is not touched)",
        clean.len(),
        total as f64 / 1e9,
        src.display(),
        dst.display()
    );
    if let Some(free) = free_bytes(dst) {
        let need = total + 1_000_000_000;
        if free < need {
            return Err(format!(
                "not enough space for the copy: {:.1} GB free on the disk of {}, {:.1} GB needed",
                free as f64 / 1e9,
                dst.display(),
                need as f64 / 1e9
            ));
        }
    }
    if !apply {
        return Ok(format!("{summary} - dry run; to do it: --apply"));
    }
    let part = dst.with_extension("part");
    let _ = std::fs::remove_dir_all(&part);
    for f in clean.keys() {
        let to = part.join(f);
        std::fs::create_dir_all(to.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::copy(src.join(f), &to).map_err(|e| format!("{}: {e}", to.display()))?;
    }
    check_clean(&part, clean).map_err(|e| format!("the copy does not match: {e}"))?;
    std::fs::rename(&part, dst).map_err(|e| format!("{}: {e}", dst.display()))?;
    write_original(
        dst,
        &b.id,
        &b.depot,
        &b.manifest,
        clean,
        "a copy of the clean files of the game installed by Steam",
    )?;
    Ok(format!("original ready: {summary}"))
}

/// No reference yet: what to do, and with `apply` do what ws can (copy a matching Steam install, register a
/// download_depot download). The copy of the game is required (docs/GAME_DATA.md): the level is Error until it is there.
pub fn missing_reference(ws: &crate::Workspace, b: &Build, apply: bool) -> (Level, String) {
    let root = original_root(ws);
    let dst = root.join(&b.id);
    let dl = steam_download(b);
    if dl.as_ref().is_some_and(|p| {
        p.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")
            .is_file()
    }) {
        let dl = dl.unwrap();
        if !apply {
            return (Level::Error, format!(
                "no game reference; the Steam download is there ({}) - ws bootstrap --apply registers it (or ws game original steam --apply)",
                dl.display()
            ));
        }
        return match adopt_original(&dl, &dst, &b.id, &b.depot, &b.manifest, true) {
            Ok(m) => (Level::Done, m),
            Err(e) => (Level::Error, e),
        };
    }
    let depot = format!(
        "or download the clean build: open steam://open/console and run `download_depot {} {} {}` ({:.0} GB), then ws game original steam --apply",
        b.app, b.depot, b.manifest, b.bytes as f64 / 1e9
    );
    let Some(inst) = installed(b) else {
        return (Level::Error, format!(
            "no game reference in {}: your own copy of the game is required (docs/GAME_DATA.md). Install Dungeonborne in \
             Steam (the account that owns it), then ws bootstrap --apply copies it; {depot}. Another workspace on this \
             machine already has one: local.toml [game] original = '<its home>/game/original'",
            root.display()
        ));
    };
    if inst.buildid != b.id {
        return (Level::Error, format!(
            "no game reference; the game installed by Steam ({}) is build {}, the project needs {} - {depot}",
            inst.path.display(), if inst.buildid.is_empty() { "?" } else { &inst.buildid }, b.id
        ));
    }
    let Some(clean) = clean_files(ws, &b.id) else {
        return (Level::Error, format!("no config/game-{}.files.tsv: the clean file list is needed to copy an install - {depot}", b.id));
    };
    match adopt_installed(&inst.path, &dst, b, &clean, apply) {
        Ok(m) if apply => (Level::Done, m),
        Ok(m) => (Level::Error, format!(
            "no game reference; the game installed by Steam is the right build - ws bootstrap --apply copies it: {m}"
        )),
        Err(e) => (Level::Error, format!(
            "no game reference; the game installed by Steam ({}) cannot be copied: {e}. In Steam: the game → Properties → \
             Installed Files → Verify integrity; {depot}",
            inst.path.display()
        )),
    }
}

/// Where the game references live: local.toml [game] original (a shared folder — another workspace's reference on
/// the same machine, read-only), else home/game/original.
pub fn original_root(ws: &crate::Workspace) -> PathBuf {
    ws.local_str("game", "original")
        .and_then(|p| ws.resolve(&p))
        .unwrap_or_else(|| ws.home().join("game").join("original"))
}

/// A build from config/game.toml.
#[derive(Debug, Clone, Default)]
pub struct Build {
    pub id: String,
    pub app: String,
    pub depot: String,
    pub manifest: String,
    pub files: u64,
    pub bytes: u64,
}

pub fn builds(ws: &crate::Workspace) -> Vec<Build> {
    let Ok(doc) = crate::read_toml(&ws.config("game.toml")) else {
        return Vec::new();
    };
    let s = |v: &toml::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let n = |v: &toml::Value, k: &str| v.get(k).and_then(|x| x.as_integer()).unwrap_or(0) as u64;
    doc.get("build")
        .and_then(|a| a.as_array())
        .into_iter()
        .flatten()
        .map(|v| Build {
            id: s(v, "id"),
            app: s(v, "app"),
            depot: s(v, "depot"),
            manifest: s(v, "manifest"),
            files: n(v, "files"),
            bytes: n(v, "bytes"),
        })
        .collect()
}

/// Steam's folder from the registry (HKCU\Software\Valve\Steam SteamPath); None — Steam is not installed.
pub fn steam_path() -> Option<PathBuf> {
    let o = std::process::Command::new("reg")
        .args(["query", r"HKCU\Software\Valve\Steam", "/v", "SteamPath"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&o.stdout);
    let line = text.lines().find(|l| l.contains("SteamPath"))?;
    let path = line.split("REG_SZ").nth(1)?.trim();
    // Steam keeps it with "/" (c:/program files (x86)/steam): shown to people next to "\" paths, so in the native form
    (!path.is_empty()).then(|| crate::native(Path::new(path)))
}

/// Where `download_depot` puts a build: <Steam>/steamapps/content/app_<app>/depot_<depot>.
pub fn steam_download(b: &Build) -> Option<PathBuf> {
    steam_path().map(|s| {
        s.join("steamapps")
            .join("content")
            .join(format!("app_{}", b.app))
            .join(format!("depot_{}", b.depot))
    })
}

/// ws game — the state of the references and, when there is none, the exact steps (human: one Steam command;
/// agent: ws game original steam --apply).
pub fn status(ws: &crate::Workspace) -> Vec<Finding> {
    let mut out = Vec::new();
    let root = original_root(ws);
    let known = builds(ws);
    let have: Vec<String> = std::fs::read_dir(&root)
        .map(|rd| {
            rd.flatten()
                .filter(|e| e.path().join("original.toml").is_file())
                .map(|e| e.file_name().to_string_lossy().to_string())
                .collect()
        })
        .unwrap_or_default();
    for id in &have {
        let rec = crate::read_toml(&root.join(id).join("original.toml")).ok();
        let files = rec
            .as_ref()
            .and_then(|r| r.get("files"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64;
        let bytes = rec
            .as_ref()
            .and_then(|r| r.get("bytes"))
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u64;
        let check = match known.iter().find(|b| &b.id == id) {
            Some(b) if b.files == files && b.bytes == bytes => {
                " - matches config/game.toml".to_string()
            }
            Some(b) => format!(
                " - differs from config/game.toml ({} files, {} bytes expected)",
                b.files, b.bytes
            ),
            None => " - not in config/game.toml".to_string(),
        };
        out.push(Finding::new(
            Level::Info,
            "game",
            root.join(id).display().to_string(),
            format!(
                "reference {id}: {files} files, {:.1} GB{check}",
                bytes as f64 / 1e9
            ),
        ));
    }
    if have.is_empty() {
        let Some(b) = known.first() else {
            out.push(Finding::new(
                Level::Error,
                "game",
                "config/game.toml",
                "no builds listed",
            ));
            return out;
        };
        let (level, msg) = missing_reference(ws, b, false);
        out.push(Finding::new(level, "game", "reference", msg));
    }
    out
}

/// Result of `diff`: (added, changed, missing) — paths from the installation root.
pub type GameDiff = (Vec<String>, Vec<String>, Vec<String>);

/// ws game diff — how an installation differs from the reference: added files (mods, logs), changed by size
/// (replaced dll, exe), missing. Contents are not compared — only size.
pub fn diff(original: &Path, install: &Path) -> Result<GameDiff, String> {
    let meta = crate::read_toml(&original.join("original.toml"))?;
    let want = meta
        .get("size")
        .and_then(|s| s.as_table())
        .ok_or("original.toml: no [size]")?;
    let have = listing(install);
    let (mut added, mut changed, mut missing) = (Vec::new(), Vec::new(), Vec::new());
    for (f, s) in &have {
        match want.get(f).and_then(|v| v.as_integer()) {
            None => added.push(f.clone()),
            Some(w) if w as u64 != *s => changed.push(f.clone()),
            _ => {}
        }
    }
    for f in want.keys() {
        if !have.contains_key(f) {
            missing.push(f.clone());
        }
    }
    Ok((added, changed, missing))
}

#[cfg(test)]
mod installed_tests {
    use super::*;

    #[test]
    fn clean_check_and_copy() {
        let base = std::env::temp_dir().join(format!("ws-installed-{}", std::process::id()));
        let src = base.join("common").join("Dungeonborne");
        let mut clean = std::collections::BTreeMap::new();
        for (f, body) in [
            ("Dungeonborne/Binaries/Win64/Dungeonborne.exe", "exe"),
            ("Dungeonborne/Content/Paks/a.pak", "pak!"),
        ] {
            std::fs::create_dir_all(src.join(f).parent().unwrap()).unwrap();
            std::fs::write(src.join(f), body).unwrap();
            clean.insert(f.to_string(), body.len() as u64);
        }
        std::fs::create_dir_all(src.join("Dungeonborne/Binaries/Win64/ue4ss")).unwrap();
        std::fs::write(src.join("Dungeonborne/Binaries/Win64/ue4ss/mod.dll"), "mod").unwrap();
        assert!(check_clean(&src, &clean).is_ok());
        let b = Build {
            id: "1".into(),
            app: "2448970".into(),
            depot: "2".into(),
            manifest: "3".into(),
            files: 2,
            bytes: 7,
        };
        let dst = base.join("original").join("1");
        assert!(adopt_installed(&src, &dst, &b, &clean, false)
            .unwrap()
            .contains("dry run"));
        adopt_installed(&src, &dst, &b, &clean, true).unwrap();
        assert!(dst.join("Dungeonborne/Content/Paks/a.pak").is_file());
        assert!(
            !dst.join("Dungeonborne/Binaries/Win64/ue4ss").exists(),
            "mods are not copied"
        );
        assert!(
            src.join("Dungeonborne/Binaries/Win64/ue4ss/mod.dll")
                .is_file(),
            "the install is not touched"
        );
        assert!(dst.join("original.toml").is_file());
        assert!(
            adopt_installed(&src, &dst, &b, &clean, true).is_err(),
            "never overwritten"
        );
        std::fs::write(src.join("Dungeonborne/Content/Paks/a.pak"), "changed!").unwrap();
        let e = check_clean(&src, &clean).unwrap_err();
        assert!(e.contains("1 with another size"), "{e}");
        // the reference files are read-only: clear it for the cleanup
        for f in clean.keys() {
            let p = dst.join(f);
            let mut perm = std::fs::metadata(&p).unwrap().permissions();
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = std::fs::set_permissions(&p, perm);
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn repo_clean_list() {
        let ws = crate::Workspace::find(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let b = builds(&ws).into_iter().next().unwrap();
        let clean = clean_files(&ws, &b.id).expect("config/game-<build>.files.tsv");
        assert_eq!(clean.len() as u64, b.files);
        assert_eq!(clean.values().sum::<u64>(), b.bytes);
    }
}
