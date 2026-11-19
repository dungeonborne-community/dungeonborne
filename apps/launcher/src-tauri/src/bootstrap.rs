//! The launcher in place of the game's own bootstrap (`<install>\Dungeonborne.exe`, UE `BootstrapPackagedGame`), so that
//! "Play" in Steam opens the launcher. The original is kept as `Dungeonborne.bootstrap.exe` and put back on disable.
//! A copy of the launcher is recognised by `MARKER` (with its version), the original bootstrap by its sha256; any other
//! file there is kept as the backup too, never overwritten. The copy is written by the launcher itself (no
//! "downloaded from the internet" mark), so Windows does not show SmartScreen when Steam starts it.

use std::path::{Path, PathBuf};

use crate::error::{bail, Error, Result};
use crate::profile::Install;

/// Bootstrap of the final game build (both installs, Steam appid 2448970), signed by Proxima Beta.
pub const ORIGINAL_SHA256: &str = "7def8203d1d01e703cb5d5804775818ffc37db531ea923d2ade38c9d34725018";
const PREFIX: &[u8] = b"DBL-LAUNCHER-SELF:";
/// Kept in the binary so a copy can be recognised and its version read: `DBL-LAUNCHER-SELF:<version>;`.
/// `#[used]` alone does not survive the MSVC linker's /OPT:REF, so `keep_marker` touches it at start.
static MARKER: &str = concat!("DBL-LAUNCHER-SELF:", env!("CARGO_PKG_VERSION"), ";");

pub fn keep_marker() {
    std::hint::black_box(MARKER.as_ptr());
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", content = "version", rename_all = "snake_case")]
pub enum Root {
    /// the game's original bootstrap
    Original,
    /// a copy of this launcher (its version)
    Launcher(String),
    /// something else (another build of the bootstrap, a mod)
    Other,
    Missing,
}

pub fn root_exe(inst: &Install) -> PathBuf {
    inst.root.join("Dungeonborne.exe")
}

pub fn backup_exe(inst: &Install) -> PathBuf {
    inst.root.join("Dungeonborne.bootstrap.exe")
}

/// `<version>` of the first `DBL-LAUNCHER-SELF:<semver>;` (the bare prefix may also sit elsewhere in the binary).
fn launcher_version(bytes: &[u8]) -> Option<String> {
    let mut from = 0;
    while let Some(i) = bytes[from..].windows(PREFIX.len()).position(|w| w == PREFIX) {
        let at = from + i + PREFIX.len();
        let rest = &bytes[at..bytes.len().min(at + 64)];
        if let Some(v) = rest
            .iter()
            .position(|&b| b == b';')
            .and_then(|end| std::str::from_utf8(&rest[..end]).ok())
            .filter(|v| semver::Version::parse(v).is_ok())
        {
            return Some(v.to_owned());
        }
        from = at;
    }
    None
}

pub fn root_kind(inst: &Install) -> Root {
    let exe = root_exe(inst);
    let Ok(bytes) = std::fs::read(&exe) else { return Root::Missing };
    if crate::net::sha256_hex(&bytes) == ORIGINAL_SHA256 {
        return Root::Original;
    }
    // the marker is in this binary too, so this also finds older/newer copies of the launcher
    match launcher_version(&bytes) {
        Some(v) => Root::Launcher(v),
        None => Root::Other,
    }
}

/// This process was started as `<install>\Dungeonborne.exe` (from Steam or the game folder): that install's root.
pub fn running_as_bootstrap() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    if !exe.file_name()?.to_string_lossy().eq_ignore_ascii_case("Dungeonborne.exe") {
        return None;
    }
    let root = exe.parent()?.to_path_buf();
    root.join("Dungeonborne").join("Binaries").join("Win64").join("Dungeonborne.exe").is_file().then_some(root)
}

fn is_current_exe(p: &Path) -> bool {
    let canon = |x: &Path| std::fs::canonicalize(x).ok();
    std::env::current_exe().ok().and_then(|e| canon(&e)).is_some_and(|e| canon(p).as_ref() == Some(&e))
}

/// Puts this launcher in place of the bootstrap (or refreshes an older copy). Returns true if a file was written.
pub fn install(inst: &Install) -> Result<bool> {
    let exe = root_exe(inst);
    let backup = backup_exe(inst);
    match root_kind(inst) {
        Root::Launcher(v) => {
            let older = semver::Version::parse(&v).ok() < semver::Version::parse(crate::update::CURRENT).ok();
            if !older || is_current_exe(&exe) {
                return Ok(false);
            }
        }
        Root::Original if backup.exists() => {
            // put back by Steam's file check; the backup already holds it
            std::fs::remove_file(&exe)?;
        }
        Root::Other if backup.exists() => bail!("bootstrap_foreign", "{}", exe.display()),
        Root::Original | Root::Other => std::fs::rename(&exe, &backup)?,
        Root::Missing => {}
    }
    // a fresh file written by us: no zone mark, no SmartScreen on start
    let me = std::fs::read(std::env::current_exe()?)?;
    let tmp = inst.root.join("Dungeonborne.exe.dbl_tmp");
    std::fs::write(&tmp, &me)?;
    if exe.exists() {
        std::fs::remove_file(&exe).map_err(|e| Error::new("bootstrap_busy", e.to_string()))?;
    }
    std::fs::rename(&tmp, &exe)?;
    Ok(true)
}

/// Puts the original bootstrap back. A running copy is renamed to `Dungeonborne.old.exe` (removed on the next start).
pub fn uninstall(inst: &Install) -> Result<()> {
    let exe = root_exe(inst);
    let backup = backup_exe(inst);
    if !backup.exists() {
        if matches!(root_kind(inst), Root::Launcher(_)) {
            bail!("bootstrap_no_backup", "{}", backup.display());
        }
        return Ok(()); // nothing of ours there
    }
    if !matches!(root_kind(inst), Root::Launcher(_) | Root::Missing) {
        bail!("bootstrap_foreign", "{}", exe.display());
    }
    if exe.exists() {
        let old = inst.root.join("Dungeonborne.old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)?;
    }
    std::fs::rename(&backup, &exe)?;
    Ok(())
}

/// Leftovers in an install root: a copy that was running while it was replaced or removed.
pub fn cleanup(inst: &Install) {
    let _ = std::fs::remove_file(inst.root.join("Dungeonborne.old.exe"));
    let _ = std::fs::remove_file(inst.root.join("Dungeonborne.exe.dbl_tmp"));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(tag: &str) -> (PathBuf, Install) {
        let root = std::env::temp_dir().join(format!("dbl_boot_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("Dungeonborne/Binaries/Win64")).unwrap();
        std::fs::write(root.join("Dungeonborne/Binaries/Win64/Dungeonborne.exe"), b"game").unwrap();
        let inst = Install::open(root.to_str().unwrap()).unwrap();
        (root, inst)
    }

    #[test]
    fn marker_is_in_the_binary() {
        keep_marker();
        let me = std::fs::read(std::env::current_exe().unwrap()).unwrap();
        assert_eq!(launcher_version(&me).as_deref(), Some(crate::update::CURRENT));
        // built at run time: a literal marker here would sit in this test binary before the real one
        let p = format!("{}{}", "DBL-LAUNCHER-", "SELF:");
        assert_eq!(launcher_version(format!("xx {p}1.0.0; yy").as_bytes()).as_deref(), Some("1.0.0"));
        assert_eq!(launcher_version(format!("{p} {p}1.2.0;").as_bytes()).as_deref(), Some("1.2.0"));
        assert!(launcher_version(b"plain").is_none());
    }

    #[test]
    fn install_keeps_the_original_and_uninstall_restores_it() {
        keep_marker();
        let (root, inst) = fake("cycle");
        std::fs::write(root_exe(&inst), b"someone's bootstrap").unwrap();
        assert_eq!(root_kind(&inst), Root::Other);

        assert!(install(&inst).unwrap());
        assert_eq!(root_kind(&inst), Root::Launcher(crate::update::CURRENT.into()));
        assert_eq!(std::fs::read(backup_exe(&inst)).unwrap(), b"someone's bootstrap");
        assert!(!install(&inst).unwrap(), "same version: nothing to do");

        // an unknown file appears while a backup exists: left alone
        std::fs::write(root_exe(&inst), b"unknown").unwrap();
        assert_eq!(install(&inst).unwrap_err().code, "bootstrap_foreign");
        assert!(uninstall(&inst).is_err());
        std::fs::write(root_exe(&inst), b"x").unwrap();
        std::fs::remove_file(root_exe(&inst)).unwrap();
        assert!(install(&inst).unwrap(), "missing root exe: the launcher goes in");
        assert_eq!(std::fs::read(backup_exe(&inst)).unwrap(), b"someone's bootstrap");

        uninstall(&inst).unwrap();
        assert_eq!(std::fs::read(root_exe(&inst)).unwrap(), b"someone's bootstrap");
        assert!(!backup_exe(&inst).exists());
        cleanup(&inst);
        assert!(!root.join("Dungeonborne.old.exe").exists());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn uninstall_never_touches_foreign_files() {
        let (root, inst) = fake("foreign");
        std::fs::write(root_exe(&inst), b"a mod's exe").unwrap();
        std::fs::write(backup_exe(&inst), b"bootstrap").unwrap();
        assert_eq!(uninstall(&inst).unwrap_err().code, "bootstrap_foreign");
        std::fs::remove_file(backup_exe(&inst)).unwrap();
        uninstall(&inst).unwrap(); // no backup, not ours: nothing to do
        assert_eq!(std::fs::read(root_exe(&inst)).unwrap(), b"a mod's exe");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
