//! ws runs collect — client crashes from %LOCALAPPDATA%\Dungeonborne\Saved\Crashes to home/runs/crashes
//! (docs/WORKSPACE_PLAN.md, section 4). The game sets the path and it cannot be changed; on C: crashes add up to gigabytes.
//! A move, not a deletion: the folder is copied, checked by size and only then removed from C:.
//! Without apply — only a list.
use crate::{copy_tree, Finding, Level};
use std::path::{Path, PathBuf};

pub fn crashes_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| {
        PathBuf::from(l)
            .join("Dungeonborne")
            .join("Saved")
            .join("Crashes")
    })
}

fn size(p: &Path) -> u64 {
    if p.is_file() {
        return p.metadata().map(|m| m.len()).unwrap_or(0);
    }
    std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| size(&e.path())).sum())
        .unwrap_or(0)
}

pub fn collect(src: &Path, dst: &Path, apply: bool) -> Vec<Finding> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(src) else {
        out.push(Finding::new(
            Level::Info,
            "runs",
            src.display().to_string(),
            "no such folder - nothing to collect",
        ));
        return out;
    };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    let total: u64 = dirs.iter().map(|d| size(d)).sum();
    if !apply {
        out.push(Finding::new(
            Level::Info,
            "runs",
            src.display().to_string(),
            format!(
                "crashes: {}, {:.1} GB -> {} (move: ws runs collect --apply)",
                dirs.len(),
                total as f64 / 1e9,
                dst.display()
            ),
        ));
        return out;
    }
    let (mut moved, mut bytes) = (0, 0u64);
    for d in dirs {
        let to = dst.join(d.file_name().unwrap());
        let s = size(&d);
        match copy_tree(&d, &to) {
            Ok(_) if size(&to) == s => {
                if let Err(e) = std::fs::remove_dir_all(&d) {
                    out.push(Finding::new(
                        Level::Warn,
                        "runs",
                        d.display().to_string(),
                        format!("copied but not removed from C: ({e})"),
                    ));
                }
                moved += 1;
                bytes += s;
            }
            Ok(_) => out.push(Finding::new(
                Level::Error,
                "runs",
                d.display().to_string(),
                "copy size mismatch - original kept",
            )),
            Err(e) => out.push(Finding::new(
                Level::Error,
                "runs",
                d.display().to_string(),
                e.to_string(),
            )),
        }
    }
    out.push(Finding::new(
        Level::Done,
        "runs",
        dst.display().to_string(),
        format!("crashes moved: {moved}, {:.1} GB", bytes as f64 / 1e9),
    ));
    out
}
