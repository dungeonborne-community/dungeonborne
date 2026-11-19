//! Finding the game folder by itself: a running game, then Steam's libraries (Steam's path from the registry, the
//! libraries from `libraryfolders.vdf`, the game from its `appmanifest_*.acf`). And turning any folder the player picked
//! in "Browse…" (the game folder, `Win64`, the exe's folder, `steamapps\common`) into the install root.
//! Only looks where Steam says the game is: no walking over drives.

use std::path::{Path, PathBuf};

use crate::winutil;

const GAME: &str = "Dungeonborne";

/// The install root: the folder holding `Dungeonborne\Binaries\Win64\Dungeonborne.exe`
pub fn is_root(p: &Path) -> bool {
    p.join(GAME).join("Binaries").join("Win64").join("Dungeonborne.exe").is_file()
}

/// The install root for what the player picked: that folder, one of its parents (picked inside the game),
/// or its `Dungeonborne` child (picked `steamapps\common`).
pub fn root_of(picked: &Path) -> Option<PathBuf> {
    let start = if picked.is_file() { picked.parent()? } else { picked };
    if let Some(r) = start.ancestors().take(6).find(|a| is_root(a)) {
        return Some(r.to_path_buf());
    }
    let child = start.join(GAME);
    is_root(&child).then_some(child)
}

/// The game on this PC: a running `Dungeonborne.exe` first (whatever folder it runs from), then Steam.
pub fn find() -> Option<PathBuf> {
    running().or_else(|| steam_dirs().iter().flat_map(|s| libraries(s)).find_map(|lib| in_library(&lib)))
}

fn running() -> Option<PathBuf> {
    let me = std::process::id();
    winutil::processes()
        .into_iter()
        .filter(|p| p.pid != me && p.name.eq_ignore_ascii_case("Dungeonborne.exe"))
        .filter_map(|p| winutil::process_path(p.pid))
        .find_map(|exe| root_of(&exe))
}

/// Steam's own folder(s): the user's `SteamPath`, then the machine-wide `InstallPath`
fn steam_dirs() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for (machine, key, value) in [
        (false, r"Software\Valve\Steam", "SteamPath"),
        (true, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
        (true, r"SOFTWARE\Valve\Steam", "InstallPath"),
    ] {
        if let Some(v) = winutil::reg_string(machine, key, value) {
            let p = PathBuf::from(v.replace('/', "\\"));
            if p.is_dir() && !out.iter().any(|o| same(o, &p)) {
                out.push(p);
            }
        }
    }
    out
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy().trim_end_matches('\\').eq_ignore_ascii_case(b.to_string_lossy().trim_end_matches('\\'))
}

/// Steam's own folder and every library listed in its `libraryfolders.vdf`
pub fn libraries(steam: &Path) -> Vec<PathBuf> {
    let mut out = vec![steam.to_path_buf()];
    let vdf = std::fs::read_to_string(steam.join("steamapps").join("libraryfolders.vdf")).unwrap_or_default();
    for p in library_paths(&vdf) {
        let p = PathBuf::from(p);
        if !out.iter().any(|o| same(o, &p)) {
            out.push(p);
        }
    }
    out
}

/// `"path" "X:\\SteamLibrary"` (Steam since 2021) and `"1" "X:\\SteamLibrary"` (older files)
fn library_paths(vdf: &str) -> Vec<String> {
    vdf.lines()
        .filter_map(|l| {
            let q = quoted(l);
            let [k, v] = q.as_slice() else { return None };
            let numbered = !k.is_empty() && k.bytes().all(|b| b.is_ascii_digit()) && v.contains(":\\");
            (k.eq_ignore_ascii_case("path") || numbered).then(|| v.clone())
        })
        .collect()
}

/// The quoted strings of one VDF line, `\\` and `\"` unescaped
fn quoted(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Option<String> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (&mut cur, c) {
            (None, '"') => cur = Some(String::new()),
            (None, _) => {}
            (Some(s), '\\') => {
                if let Some(n) = chars.next() {
                    s.push(n);
                }
            }
            (Some(_), '"') => out.push(cur.take().unwrap()),
            (Some(s), c) => s.push(c),
        }
    }
    out
}

/// The game in one library: by its app manifest (`"name" "Dungeonborne"` → `installdir`), else `common\Dungeonborne`
fn in_library(lib: &Path) -> Option<PathBuf> {
    let apps = lib.join("steamapps");
    let common = apps.join("common");
    let by_manifest = std::fs::read_dir(&apps).ok().into_iter().flatten().flatten().find_map(|e| {
        let name = e.file_name().to_string_lossy().to_ascii_lowercase();
        if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
            return None;
        }
        let text = std::fs::read_to_string(e.path()).ok()?;
        let field = |key: &str| text.lines().map(quoted).find(|q| q.len() == 2 && q[0].eq_ignore_ascii_case(key)).map(|q| q[1].clone());
        if !field("name")?.eq_ignore_ascii_case(GAME) {
            return None;
        }
        let root = common.join(field("installdir")?);
        is_root(&root).then_some(root)
    });
    by_manifest.or_else(|| {
        let root = common.join(GAME);
        is_root(&root).then_some(root)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(root: &Path) {
        let win64 = root.join("Dungeonborne/Binaries/Win64");
        std::fs::create_dir_all(&win64).unwrap();
        std::fs::write(win64.join("Dungeonborne.exe"), b"game").unwrap();
    }

    #[test]
    fn vdf_paths_new_and_old() {
        let new = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t\t\"label\"\t\t\"\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"X:\\\\SteamLibrary\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"2483120\"\t\t\"123\"\n\t\t}\n\t}\n}\n";
        assert_eq!(library_paths(new), vec![r"C:\Program Files (x86)\Steam", r"X:\SteamLibrary"]);
        let old = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\t\"1\"\n\t\"1\"\t\t\"X:\\\\Games\\\\Steam\"\n}\n";
        assert_eq!(library_paths(old), vec![r"X:\Games\Steam"], "app sizes and counters are not paths");
    }

    #[test]
    fn finds_in_library_and_picked_folder() {
        let tmp = std::env::temp_dir().join(format!("dbl-locate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let lib = tmp.join("Lib");
        let root = lib.join("steamapps/common/DB Install");
        game(&root);
        assert_eq!(in_library(&lib), None, "folder name differs and no manifest yet");
        std::fs::write(
            lib.join("steamapps/appmanifest_1.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1\"\n\t\"name\"\t\t\"Dungeonborne\"\n\t\"installdir\"\t\t\"DB Install\"\n}\n",
        )
        .unwrap();
        assert_eq!(in_library(&lib), Some(root.clone()));

        assert_eq!(root_of(&root), Some(root.clone()));
        assert_eq!(root_of(&root.join("Dungeonborne/Binaries/Win64")), Some(root.clone()), "picked inside the game");
        assert_eq!(root_of(&root.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe")), Some(root.clone()), "picked the exe");
        let plain = lib.join("steamapps/common/Dungeonborne");
        game(&plain);
        assert_eq!(root_of(&lib.join("steamapps/common")), Some(plain), "picked steamapps\\common");
        assert_eq!(root_of(&lib), None);
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
