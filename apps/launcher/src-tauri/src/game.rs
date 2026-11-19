//! Game process: is it running, start it (optionally with UAC), wait until it is really gone.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::Result;
use crate::profile::Install;
use crate::winutil::{self, Proc};

fn lower(p: &Path) -> PathBuf {
    PathBuf::from(p.to_string_lossy().to_lowercase())
}

/// Any `Dungeonborne.exe` from this install. A process whose path Windows hides counts as ours: better
/// to wait than to touch files under a live game. Not counted: this launcher, and a launcher copy sitting in place
/// of the bootstrap (`<root>\Dungeonborne.exe` while `Dungeonborne.bootstrap.exe` holds the original).
pub fn running_in(root: &Path) -> bool {
    let me = std::process::id();
    let launcher_copy = root.join("Dungeonborne.bootstrap.exe").is_file().then(|| lower(&root.join("Dungeonborne.exe")));
    let root = lower(root);
    winutil::processes()
        .into_iter()
        .filter(|p| p.pid != me && p.name.eq_ignore_ascii_case("Dungeonborne.exe"))
        .any(|p| match winutil::process_path(p.pid) {
            None => true,
            Some(path) => {
                let path = lower(&path);
                path.starts_with(&root) && Some(&path) != launcher_copy.as_ref()
            }
        })
}

/// `args`: what Steam passed to the bootstrap (the launcher stands in for it), quoted for the command line.
pub fn launch(inst: &Install, elevate: bool, args: &[String]) -> Result<Proc> {
    let params: Vec<String> =
        args.iter().map(|a| if a.contains([' ', '"']) { format!("\"{}\"", a.replace('"', "")) } else { a.clone() }).collect();
    winutil::shell_execute(&inst.exe(), &params.join(" "), Some(&inst.win64()), elevate, false)
}

/// Blocks: waits for the started process, then for any other game process of this install. Returns the exit code
/// of the started process (a crash leaves a non-zero one; bug reports offer "crash" then).
pub fn wait_exit(proc: Proc, root: &Path) -> u32 {
    let code = proc.wait();
    drop(proc);
    while running_in(root) {
        std::thread::sleep(Duration::from_secs(2));
    }
    code
}
