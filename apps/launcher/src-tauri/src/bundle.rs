//! Client runtime shipped inside the exe (`bundle/client.zip`, built by `tools/make_client_bundle.py`): UE4SS core with
//! its built-in mods, the emulator dll, the game's cacert.pem. Server mods are `dbmod/1` packages (profile.rs).
//! Laid into the game folder before every start; only files whose sha256 differs are written, and an original file is
//! backed up once before it is replaced. `retired` lists files older bundles laid (mod paks, Lua mods): such a file is
//! removed only while it is unchanged (sha256 of one of those versions), so files of the player stay.

use std::io::{Cursor, Read};
use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{bail, Error, Result};
use crate::net::sha256_hex;
use crate::profile::{safe_rel, sha256_file, Install};
use crate::store::data_dir;

static ZIP: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../bundle/client.zip"));

#[derive(Deserialize)]
struct Meta {
    schema: String,
    version: String,
    files: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    dirs: Vec<String>,
    #[serde(default)]
    retired: std::collections::BTreeMap<String, Vec<String>>,
}

fn archive() -> Result<zip::ZipArchive<Cursor<&'static [u8]>>> {
    zip::ZipArchive::new(Cursor::new(ZIP)).map_err(|e| Error::new("bundle_invalid", e.to_string()))
}

fn meta() -> Result<Meta> {
    let mut z = archive()?;
    let mut s = String::new();
    z.by_name("bundle.json").map_err(|e| Error::new("bundle_invalid", e.to_string()))?.read_to_string(&mut s)?;
    let m: Meta = serde_json::from_str(&s).map_err(|e| Error::new("bundle_invalid", e.to_string()))?;
    if m.schema != "dblbundle/1" {
        bail!("bundle_invalid", "schema {}", m.schema);
    }
    Ok(m)
}

pub fn version() -> String {
    meta().map(|m| m.version).unwrap_or_default()
}

fn backup_dir(inst: &Install) -> PathBuf {
    data_dir().join("bundle_backup").join(inst.key())
}

pub struct Installed {
    pub version: String,
    pub written: usize,
    pub removed: usize,
}

/// Lays the bundle into the install; with `retire` also removes unchanged files of older bundles.
pub fn install(inst: &Install, retire: bool) -> Result<Installed> {
    let m = meta()?;
    let mut z = archive()?;
    let mut written = 0;
    for d in &m.dirs {
        let rel = safe_rel(d).ok_or_else(|| Error::new("bundle_invalid", d.clone()))?;
        std::fs::create_dir_all(inst.root.join(rel))?;
    }
    for (name, want) in &m.files {
        let rel = safe_rel(name).ok_or_else(|| Error::new("bundle_invalid", name.clone()))?;
        let dst = inst.root.join(&rel);
        if dst.is_file() && sha256_file(&dst)? == *want {
            continue;
        }
        let mut data = Vec::new();
        z.by_name(name).map_err(|e| Error::new("bundle_invalid", format!("{name}: {e}")))?.read_to_end(&mut data)?;
        if sha256_hex(&data) != *want {
            bail!("bundle_invalid", "{name}: sha256");
        }
        if dst.is_file() {
            let bak = backup_dir(inst).join(&rel);
            if !bak.exists() {
                std::fs::create_dir_all(bak.parent().unwrap_or(&bak))?;
                std::fs::copy(&dst, &bak)?;
            }
        }
        std::fs::create_dir_all(dst.parent().unwrap_or(&inst.root))?;
        let tmp = dst.with_extension("dbl_tmp");
        std::fs::write(&tmp, &data)?;
        std::fs::rename(&tmp, &dst)?;
        written += 1;
    }
    let removed = if retire { retire_old(inst, &m.retired)? } else { 0 };
    Ok(Installed { version: m.version, written, removed })
}

fn retire_old(inst: &Install, retired: &std::collections::BTreeMap<String, Vec<String>>) -> Result<usize> {
    let keep = [inst.root.clone(), inst.paks(), inst.ue4ss_mods(), inst.win64()];
    let mut removed = 0;
    for (name, shas) in retired {
        let Some(rel) = safe_rel(name) else { continue };
        let dst = inst.root.join(&rel);
        if !dst.is_file() || !shas.contains(&sha256_file(&dst)?) {
            continue;
        }
        std::fs::remove_file(&dst)?;
        removed += 1;
        // folders left empty (old Lua mods), never the base folders themselves
        let mut dir = dst.parent().map(PathBuf::from);
        while let Some(d) = dir {
            if keep.contains(&d) || !d.starts_with(&inst.root) || std::fs::remove_dir(&d).is_err() {
                break;
            }
            dir = d.parent().map(PathBuf::from);
        }
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    fn fake_install(tag: &str) -> (std::path::PathBuf, crate::profile::Install) {
        let tmp = std::env::temp_dir().join(format!("dbl_bundle_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let win64 = tmp.join("Dungeonborne").join("Binaries").join("Win64");
        std::fs::create_dir_all(&win64).unwrap();
        std::fs::create_dir_all(tmp.join("Dungeonborne/Content/Paks")).unwrap();
        std::fs::write(win64.join("Dungeonborne.exe"), b"").unwrap();
        let inst = crate::profile::Install::open(tmp.to_str().unwrap()).unwrap();
        (tmp, inst)
    }

    #[test]
    fn embedded_bundle_is_runtime_only() {
        let m = super::meta().unwrap();
        assert!(m.files.keys().any(|f| f.ends_with("Binaries/Win64/dwmapi.dll")));
        assert!(m.files.keys().all(|f| super::safe_rel(f).is_some()));
        let ini = m.files.keys().find(|f| f.ends_with("UE4SS-settings.ini")).unwrap();
        let mut z = super::archive().unwrap();
        let mut s = String::new();
        std::io::Read::read_to_string(&mut z.by_name(ini).unwrap(), &mut s).unwrap();
        assert!(s.lines().any(|l| l.trim() == "ConsoleEnabled = 0"), "UE4SS console is off");
        // server mods are dbmod/1 packages now; cheat and dev mods are not shipped at all
        assert!(!m.files.keys().any(|f| f.contains("/Content/Paks/")));
        for dev in ["CheatManagerEnablerMod", "DungeonborneRuClient", "ClientProbe", "shared/types"] {
            assert!(!m.files.keys().any(|f| f.contains(dev)), "{dev}");
        }
        assert!(m.retired.keys().any(|f| f.ends_with("zzzzzzz_BPFix_P.utoc")), "old paks are retired");
        assert!(m.retired.keys().all(|f| !m.files.contains_key(f)));
        // every mod folder the bundle lays is UE4SS's own: isolation (profile.rs) leaves it in place
        for f in m.files.keys().chain(m.dirs.iter()) {
            let Some((_, rest)) = f.split_once("ue4ss/Mods/") else { continue };
            if let Some((dir, _)) = rest.split_once('/') {
                assert!(crate::profile::UE4SS_BUILTIN_MODS.contains(&dir), "{dir} is not in the built-in list");
            }
        }
    }

    #[test]
    fn install_writes_only_changes_and_backs_up_originals() {
        let (tmp, inst) = fake_install("install");
        let win64 = inst.win64();
        std::fs::write(win64.join("steam_api64.dll"), b"original").unwrap();
        let total = super::meta().unwrap().files.len();

        let r = super::install(&inst, false).unwrap();
        assert_eq!(r.written, total);
        assert!(win64.join("dwmapi.dll").is_file());
        let bak = super::backup_dir(&inst).join("Dungeonborne").join("Binaries").join("Win64").join("steam_api64.dll");
        assert_eq!(std::fs::read(&bak).unwrap(), b"original");

        assert_eq!(super::install(&inst, true).unwrap().written, 0, "nothing is written the second time");
        std::fs::write(win64.join("dwmapi.dll"), b"broken").unwrap();
        assert_eq!(super::install(&inst, false).unwrap().written, 1, "the damaged file is restored");

        let _ = std::fs::remove_dir_all(&tmp);
        let _ = std::fs::remove_dir_all(super::backup_dir(&inst));
    }

    #[test]
    fn retire_removes_only_unchanged_files() {
        let (tmp, inst) = fake_install("retire");
        let old_mod = inst.ue4ss_mods().join("OldMod").join("Scripts");
        std::fs::create_dir_all(&old_mod).unwrap();
        std::fs::write(old_mod.join("main.lua"), b"old bundle").unwrap();
        std::fs::write(inst.paks().join("zz_Old_P.pak"), b"edited by the player").unwrap();
        let retired = [
            ("Dungeonborne/Binaries/Win64/ue4ss/Mods/OldMod/Scripts/main.lua", b"old bundle".as_slice()),
            ("Dungeonborne/Content/Paks/zz_Old_P.pak", b"old bundle pak".as_slice()),
            ("Dungeonborne/Content/Paks/zz_Gone_P.pak", b"never installed".as_slice()),
        ]
        .into_iter()
        .map(|(p, b)| (p.to_string(), vec![crate::net::sha256_hex(b)]))
        .collect();

        assert_eq!(super::retire_old(&inst, &retired).unwrap(), 1);
        assert!(!inst.ue4ss_mods().join("OldMod").exists(), "empty mod folder pruned");
        assert!(inst.ue4ss_mods().is_dir() && inst.paks().is_dir(), "base folders stay");
        assert!(inst.paks().join("zz_Old_P.pak").is_file(), "changed file stays");
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
