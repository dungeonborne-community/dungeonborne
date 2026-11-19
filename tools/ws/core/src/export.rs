//! Editions (docs/WORKSPACE_PLAN.md, 13.3): a one-way export without history, like Copybara. The public one is
//! export/public.toml, the team one (without owner files) is export/team.toml. Edition spec — export/<edition>.toml: what else to exclude, which files to put on top (export/public/), which strings
//! to replace with neutral ones (prod addresses, domains, accounts). Only public-level files (ws roles) are exported,
//! git-tracked ones. After writing, every exported line is checked by the same rules as ws check
//! (leaks, game-files, machine paths), but now as errors: a single finding — the export is not good.
use crate::visibility::{classify, glob, LeakScan, Tier};
use crate::{read_toml, str_list, Workspace};
use regex::Regex;
use std::path::{Path, PathBuf};

pub const SPEC: &str = "export/public.toml";
/// Editions that have an edition spec export/<name>.toml.
pub const EDITIONS: [&str; 2] = ["public", "team"];

pub fn spec_file(edition: &str) -> String {
    format!("export/{edition}.toml")
}
/// Marker of an export folder: only such a folder may be recreated. Lies in the root, and after `git init` (xtask) — in `.git`.
pub const MARK: &str = ".dbexport";

/// String replacements on export (and in the leaks check in the private repository — you see what goes out).
pub struct Scrub {
    rules: Vec<(Regex, String)>,
}

impl Scrub {
    pub fn none() -> Scrub {
        Scrub { rules: Vec::new() }
    }

    /// export/public.toml [[replace]]; no file — no replacements.
    pub fn load(ws: &Workspace) -> Result<Scrub, String> {
        Scrub::load_file(ws, SPEC)
    }

    /// [[replace]] of the spec `file`: first the replacements of the specs from `[export] replace_from` (in order), then its own.
    /// No file — no replacements.
    pub fn load_file(ws: &Workspace, file: &str) -> Result<Scrub, String> {
        let path = ws.root.join(file);
        if !path.exists() {
            return Ok(Scrub::none());
        }
        let doc = read_toml(&path)?;
        let mut rules = Vec::new();
        for from in str_list(doc.get("export").and_then(|e| e.get("replace_from"))) {
            let other = read_toml(&ws.root.join(&from))
                .map_err(|e| format!("{file}: replace_from {from}: {e}"))?;
            if other
                .get("export")
                .and_then(|e| e.get("replace_from"))
                .is_some()
            {
                return Err(format!(
                    "{file}: replace_from {from} has its own replace_from - one level only"
                ));
            }
            rules.extend(Scrub::load_file(ws, &from)?.rules);
        }
        for r in doc
            .get("replace")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
        {
            let pat = r.get("pattern").and_then(|x| x.as_str()).unwrap_or("");
            let with = r.get("with").and_then(|x| x.as_str()).unwrap_or("");
            let re = Regex::new(pat).map_err(|e| format!("{file}: replace '{pat}': {e}"))?;
            rules.push((re, with.to_string()));
        }
        Ok(Scrub { rules })
    }

    pub fn apply(&self, text: &str) -> String {
        let mut s = text.to_string();
        for (re, with) in &self.rules {
            if re.is_match(&s) {
                s = re.replace_all(&s, with.as_str()).into_owned();
            }
        }
        s
    }
}

/// Edition spec.
pub struct Spec {
    pub tier: Tier,
    pub overlay: PathBuf,
    pub exclude: Vec<Regex>,
    /// files from which `[[…]]` blocks with visibility above the export level are cut (tools.lock)
    pub filter: Vec<String>,
    pub scrub: Scrub,
    /// commit author in the copy (`[export] author`, `author_email`): not the owner's personal name and mail
    pub author: Option<(String, String)>,
    /// markers beyond [workspace.rules] owner-markers that must not be in this export (`[export] markers`)
    pub markers: Vec<String>,
    /// game data files are allowed (`[export] game_files = true`: the team edition, not the public one)
    pub game_files_ok: bool,
    /// owner-markers that are not a finding for this edition (`[export] allow_markers`, strings as in workspace.toml)
    pub allow_markers: Vec<String>,
    /// addresses beyond public-ips-ok (`[export] ips_ok`: game data, third-party servers from research)
    pub ips_ok: Vec<String>,
    /// files where machine paths are not a finding (`[export] machine_paths_ok`, glob: notes, not scripts)
    pub machine_paths_ok: Vec<Regex>,
    /// indexes (`[export] index`, usually agents/memory/MEMORY.md): exported at any level, but only the lines
    /// whose links `](file)` lead to exported files — the team gets an index of its notes, without owner notes
    pub index: Vec<String>,
}

/// Cut from TOML text the entries (from a `[…]` line to an empty line) whose `visibility = "<level>"` is above
/// `tier`. Lines outside entries (section comments, empty ones) stay.
pub fn filter_blocks(text: &str, tier: Tier) -> String {
    fn above(block: &str, tier: Tier) -> bool {
        block.lines().any(|l| {
            l.trim()
                .strip_prefix("visibility")
                .and_then(|r| r.trim_start().strip_prefix('='))
                .map(|v| {
                    v.split('#')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .trim_matches(['"', '\''])
                })
                .and_then(Tier::parse)
                .is_some_and(|t| t > tier)
        })
    }
    let mut out = String::new();
    let mut block = String::new();
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if block.is_empty() && !t.starts_with('[') {
            out.push_str(line);
            continue;
        }
        if t.is_empty() {
            if !above(&block, tier) {
                out.push_str(&block);
            }
            block.clear();
            out.push_str(line);
            continue;
        }
        block.push_str(line);
    }
    if !block.is_empty() && !above(&block, tier) {
        out.push_str(&block);
    }
    out
}

pub fn spec(ws: &Workspace, edition: &str) -> Result<Spec, String> {
    if !EDITIONS.contains(&edition) {
        return Err(format!(
            "unknown edition '{edition}' (known: {})",
            EDITIONS.join(", ")
        ));
    }
    let file = spec_file(edition);
    let doc = read_toml(&ws.root.join(&file))?;
    let e = doc.get("export");
    let tier = e
        .and_then(|x| x.get("tier"))
        .and_then(|x| x.as_str())
        .unwrap_or("public");
    Ok(Spec {
        tier: Tier::parse(tier).ok_or(format!("{file}: tier '{tier}'"))?,
        overlay: ws.root.join(
            e.and_then(|x| x.get("overlay"))
                .and_then(|x| x.as_str())
                .unwrap_or(&format!("export/{edition}")),
        ),
        exclude: str_list(e.and_then(|x| x.get("exclude")))
            .iter()
            .map(|g| glob(g))
            .collect(),
        filter: str_list(e.and_then(|x| x.get("filter"))),
        scrub: Scrub::load_file(ws, &file)?,
        author: {
            let s = |k: &str| {
                e.and_then(|x| x.get(k))
                    .and_then(|x| x.as_str())
                    .map(String::from)
            };
            s("author").zip(s("author_email"))
        },
        markers: str_list(e.and_then(|x| x.get("markers"))),
        game_files_ok: e
            .and_then(|x| x.get("game_files"))
            .and_then(|x| x.as_bool())
            .unwrap_or(false),
        allow_markers: str_list(e.and_then(|x| x.get("allow_markers"))),
        ips_ok: str_list(e.and_then(|x| x.get("ips_ok"))),
        machine_paths_ok: str_list(e.and_then(|x| x.get("machine_paths_ok")))
            .iter()
            .map(|g| glob(g))
            .collect(),
        index: str_list(e.and_then(|x| x.get("index"))),
    })
}

/// Text file: by extension or without zero bytes at the start.
fn is_text(rel: &str, bytes: &[u8]) -> bool {
    let ext = rel
        .rsplit('/')
        .next()
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    if matches!(
        ext.as_str(),
        "png" | "ico" | "icns" | "jpg" | "jpeg" | "gif" | "webp" | "woff" | "woff2" | "ttf" | "otf"
    ) {
        return false;
    }
    std::str::from_utf8(bytes).is_ok() && !bytes.iter().take(8192).any(|&b| b == 0)
}

/// A finding of the export check.
pub struct Leak {
    pub path: String,
    pub line: usize,
    pub what: String,
}

pub struct Report {
    pub out: PathBuf,
    pub files: usize,
    pub scrubbed: usize,
    pub overlay: usize,
    pub excluded: usize,
    pub leaks: Vec<Leak>,
}

/// Check rules: owner markers, IPs, game files, machine paths — from workspace.toml of the private repository.
pub struct Scan {
    leaks: LeakScan,
    game: Vec<Regex>,
    machine: Vec<Regex>,
    machine_ok: Vec<Regex>,
}

impl Scan {
    /// workspace.toml rules with the amendments of edition spec `sp` (own markers, allowed markers, addresses, files).
    pub fn new(ws: &Workspace, sp: &Spec) -> Scan {
        let rules = ws.manifest.get("workspace").and_then(|w| w.get("rules"));
        let mut machine = str_list(rules.and_then(|r| r.get("machine-paths")));
        if machine.is_empty() {
            machine = vec![
                r"(^|[^A-Za-z0-9_\\])[D-G]:[\\/]".into(),
                r"(^|[^A-Za-z0-9_\\])[Cc]:[\\/][Uu]sers[\\/]".into(),
            ];
        }
        let mut markers: Vec<String> = str_list(rules.and_then(|r| r.get("owner-markers")))
            .into_iter()
            .filter(|m| !sp.allow_markers.contains(m))
            .collect();
        markers.extend(sp.markers.iter().cloned());
        let mut ips_ok = str_list(rules.and_then(|r| r.get("public-ips-ok")));
        ips_ok.extend(sp.ips_ok.iter().cloned());
        Scan {
            leaks: LeakScan::new(&markers, &ips_ok),
            game: if sp.game_files_ok {
                Vec::new()
            } else {
                str_list(rules.and_then(|r| r.get("game-files")))
                    .iter()
                    .map(|g| glob(g))
                    .collect()
            },
            machine: machine.iter().filter_map(|p| Regex::new(p).ok()).collect(),
            machine_ok: sp.machine_paths_ok.clone(),
        }
    }

    pub fn file(&self, rel: &str, text: Option<&str>) -> Vec<Leak> {
        let mut out = Vec::new();
        if self.game.iter().any(|r| r.is_match(rel)) {
            out.push(Leak {
                path: rel.into(),
                line: 0,
                what: "game data file".into(),
            });
        }
        if let Some(t) = text {
            for (line, what) in self.leaks.all(t) {
                out.push(Leak {
                    path: rel.into(),
                    line,
                    what,
                });
            }
            let machine_ok = self.machine_ok.iter().any(|r| r.is_match(rel));
            for (i, l) in t.lines().enumerate().filter(|_| !machine_ok) {
                if let Some(m) = self.machine.iter().find_map(|r| r.find(l)) {
                    out.push(Leak {
                        path: rel.into(),
                        line: i + 1,
                        what: format!("machine path '{}'", m.as_str().trim()),
                    });
                }
            }
        }
        out
    }
}

/// Export into `out`. The folder must be new or a previous export (marker .dbexport) — then it is recreated.
pub fn run(ws: &Workspace, edition: &str, out: &Path, source_id: &str) -> Result<Report, String> {
    let sp = spec(ws, edition)?;
    if out.exists() {
        if !out.join(MARK).exists() && !out.join(".git").join(MARK).exists() {
            return Err(format!(
                "{} exists and is not an export (no {MARK}) - pick another folder",
                out.display()
            ));
        }
        std::fs::remove_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    }
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;

    // files on top (relative paths inside the overlay)
    let mut overlay_files = Vec::new();
    if sp.overlay.is_dir() {
        walk(&sp.overlay, &sp.overlay, &mut overlay_files)?;
    }
    let scan = Scan::new(ws, &sp);
    let mut rep = Report {
        out: out.to_path_buf(),
        files: 0,
        scrubbed: 0,
        overlay: overlay_files.len(),
        excluded: 0,
        leaks: Vec::new(),
    };
    let write = |rel: &str, bytes: Vec<u8>, rep: &mut Report| -> Result<(), String> {
        let dst = out.join(rel);
        if let Some(d) = dst.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        let (data, text) = if is_text(rel, &bytes) {
            let mut t = String::from_utf8(bytes).unwrap_or_default();
            if sp.filter.iter().any(|f| f == rel) {
                t = filter_blocks(&t, sp.tier);
            }
            let s = sp.scrub.apply(&t);
            if s != t {
                rep.scrubbed += 1;
            }
            (s.clone().into_bytes(), Some(s))
        } else {
            (bytes, None)
        };
        rep.leaks.extend(scan.file(rel, text.as_deref()));
        std::fs::write(&dst, data).map_err(|e| format!("{}: {e}", dst.display()))?;
        rep.files += 1;
        Ok(())
    };

    for (rel, tier) in classify(ws)? {
        // index files are written (and checked) once, below, after their lines for absent notes are dropped —
        // 19.11: checking the unfiltered copy here reported markers from notes the edition excludes
        if tier > sp.tier || overlay_files.contains(&rel) || sp.index.contains(&rel) {
            continue;
        }
        if sp.exclude.iter().any(|r| r.is_match(&rel)) {
            rep.excluded += 1;
            continue;
        }
        let src = ws.root.join(&rel);
        let Ok(bytes) = std::fs::read(&src) else {
            continue; // deleted in the working tree, but still in the git index
        };
        write(&rel, bytes, &mut rep)?;
    }
    for rel in &overlay_files {
        let bytes = std::fs::read(sp.overlay.join(rel)).map_err(|e| format!("{rel}: {e}"))?;
        write(rel, bytes, &mut rep)?;
    }
    for rel in &sp.index {
        if overlay_files.contains(rel) {
            continue;
        }
        let text =
            std::fs::read_to_string(ws.root.join(rel)).map_err(|e| format!("index {rel}: {e}"))?;
        let dir = Path::new(rel).parent().unwrap_or(Path::new(""));
        let kept = filter_index(&text, |target| out.join(dir).join(target).is_file());
        write(rel, kept.into_bytes(), &mut rep)?;
    }
    prune_members(out)?;
    std::fs::write(
        out.join(MARK),
        format!(
            "dbexport/1 edition={edition} tier={} source={source_id}\n",
            sp.tier.name()
        ),
    )
    .map_err(|e| e.to_string())?;
    Ok(rep)
}

/// Index lines whose links `](target)` to local files all lead to `exists`; lines without links stay.
fn filter_index(text: &str, exists: impl Fn(&str) -> bool) -> String {
    let link = Regex::new(r"\]\(([^)#\s]+)").unwrap();
    text.split_inclusive('\n')
        .filter(|line| {
            link.captures_iter(line).all(|c| {
                let t = &c[1];
                t.contains("://") || exists(t)
            })
        })
        .collect()
}

/// In the exported config/workspace.toml, remove from `members = [...]` the lines of members absent from the export
/// (the team edition without ops/deploy and other owner members). Lines like `    "path",  # comment`.
fn prune_members(out: &Path) -> Result<(), String> {
    let path = out.join("config").join("workspace.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let pruned = prune_members_text(&text, |m| out.join(m).is_dir());
    if pruned != text {
        std::fs::write(&path, pruned).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(())
}

fn prune_members_text(text: &str, exists: impl Fn(&str) -> bool) -> String {
    let mut out = String::new();
    let mut inside = false;
    for line in text.split_inclusive('\n') {
        let t = line.trim();
        if !inside && t.starts_with("members") && t.ends_with('[') {
            inside = true;
        } else if inside && t.starts_with(']') {
            inside = false;
        } else if inside {
            let member = t
                .strip_prefix('"')
                .and_then(|r| r.split_once('"'))
                .map(|(m, _)| m);
            if member.is_some_and(|m| !exists(m)) {
                continue;
            }
        }
        out.push_str(line);
    }
    out
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let p = e.map_err(|e| e.to_string())?.path();
        if p.is_dir() {
            walk(base, &p, out)?;
        } else if let Ok(r) = p.strip_prefix(base) {
            out.push(r.to_string_lossy().replace('\\', "/"));
        }
    }
    out.sort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrub_rules() {
        let s = Scrub {
            rules: vec![
                (Regex::new(r"5\.6\.7\.8").unwrap(), "203.0.113.10".into()),
                (Regex::new(r"(?i)acme\.test").unwrap(), "example.org".into()),
            ],
        };
        assert_eq!(
            s.apply("https://5.6.7.8/x and dir.ACME.test"),
            "https://203.0.113.10/x and dir.example.org"
        );
        assert_eq!(s.apply("nothing"), "nothing");
    }

    #[test]
    fn blocks() {
        let src = "# head\n\n[[tool]]\nname = \"a\"\n\n# --- re\n[[tool]]\nname = \"b\"\nvisibility = \"team\"  # x\n\n[[tool]]\nname = \"c\"\nvisibility = \"public\"\n";
        let got = filter_blocks(src, Tier::Public);
        assert!(got.contains("name = \"a\"") && got.contains("name = \"c\""));
        assert!(!got.contains("name = \"b\""));
        assert!(got.contains("# --- re"));
        assert_eq!(filter_blocks(src, Tier::Team), src);
    }

    #[test]
    fn index_filtered() {
        let src = "# Index\n- [A](a.md) — team\n- [B](b.md) — owner\n- [W](https://x.org/a) — web\nplain\n";
        let got = filter_index(src, |t| t == "a.md");
        assert_eq!(
            got,
            "# Index\n- [A](a.md) — team\n- [W](https://x.org/a) — web\nplain\n"
        );
    }

    #[test]
    fn members_pruned() {
        let src = "x = 1\nmembers = [\n    \"a/b\",   # keep\n    \"ops/deploy\",  # drop\n]\nother = [\"ops/deploy\"]\n";
        let got = prune_members_text(src, |m| m != "ops/deploy");
        assert_eq!(
            got,
            "x = 1\nmembers = [\n    \"a/b\",   # keep\n]\nother = [\"ops/deploy\"]\n"
        );
    }

    #[test]
    fn text_detection() {
        assert!(is_text("a.rs", b"fn main() {}"));
        assert!(!is_text("a.png", b"abc"));
        assert!(!is_text("a.bin", b"ab\0cd"));
    }
}
