//! File visibility (docs/WORKSPACE_PLAN.md, section 13.3–13.4): public ⊂ team ⊂ owner.
//!   public — goes to the public edition (cargo xtask export public, item 7);
//!   team   — to the team (Forgejo, team package);
//!   owner  — owner only (prod addresses, accounts, decompilation, personal rules).
//!
//! Where it comes from:
//!   member       member.toml [member] visibility; [visibility] team / owner — stricter parts of the member (patterns from
//!                its folder);
//!   outside members  workspace.toml [workspace.visibility] public / team / owner (patterns from the root), else default;
//!   agents/memory/*.md  the `visibility:` field in the note header; no field — owner.
//!
//! With several matches the strictest level wins. Pattern: `*` — inside a folder, `**` — any number of folders;
//! a matched folder comes with all its contents.
use crate::{str_list, Workspace};
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Tier {
    Public,
    Team,
    Owner,
}

impl Tier {
    pub fn parse(s: &str) -> Option<Tier> {
        match s {
            "public" => Some(Tier::Public),
            "team" => Some(Tier::Team),
            "owner" => Some(Tier::Owner),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tier::Public => "public",
            Tier::Team => "team",
            Tier::Owner => "owner",
        }
    }

    pub const ALL: [Tier; 3] = [Tier::Public, Tier::Team, Tier::Owner];
}

/// Path pattern → regex (whole): `**` — any folders, `*` — inside a folder; a matched folder comes with all its contents.
pub fn glob(pattern: &str) -> Regex {
    let p = pattern.trim_matches('/');
    let mut re = String::from("^");
    let mut chars = p.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                if chars.peek() == Some(&'/') {
                    chars.next();
                    re.push_str("(?:.*/)?");
                } else {
                    re.push_str(".*");
                }
            }
            '*' => re.push_str("[^/]*"),
            '?' => re.push_str("[^/]"),
            c => re.push_str(&regex::escape(&c.to_string())),
        }
    }
    // a folder matched — so everything inside it too
    re.push_str("(?:/.*)?");
    re.push('$');
    Regex::new(&re).unwrap_or_else(|_| Regex::new("^$").unwrap())
}

struct Rules {
    public: Vec<Regex>,
    team: Vec<Regex>,
    owner: Vec<Regex>,
}

impl Rules {
    fn from(public: Vec<String>, team: Vec<String>, owner: Vec<String>) -> Rules {
        let c = |v: Vec<String>| v.iter().map(|p| glob(p)).collect();
        Rules {
            public: c(public),
            team: c(team),
            owner: c(owner),
        }
    }

    /// The strictest matched level.
    fn strictest(&self, rel: &str) -> Option<Tier> {
        let hit = |v: &[Regex]| v.iter().any(|r| r.is_match(rel));
        if hit(&self.owner) {
            Some(Tier::Owner)
        } else if hit(&self.team) {
            Some(Tier::Team)
        } else if hit(&self.public) {
            Some(Tier::Public)
        } else {
            None
        }
    }
}

/// Classifier of workspace files by levels (patterns are compiled once).
pub struct Classifier {
    default: Tier,
    shared: Rules,
    members: Vec<(String, Option<Tier>, Rules)>,
}

impl Classifier {
    pub fn new(ws: &Workspace) -> Classifier {
        let v = ws
            .manifest
            .get("workspace")
            .and_then(|w| w.get("visibility"));
        let list = |k: &str| str_list(v.and_then(|v| v.get(k)));
        let default = v
            .and_then(|v| v.get("default"))
            .and_then(|d| d.as_str())
            .and_then(Tier::parse)
            .unwrap_or(Tier::Team);
        let members = ws
            .members
            .iter()
            .map(|m| {
                (
                    m.path.clone(),
                    Tier::parse(&m.visibility),
                    Rules::from(vec![], m.vis_team.clone(), m.vis_owner.clone()),
                )
            })
            .collect();
        Classifier {
            default,
            shared: Rules::from(list("public"), list("team"), list("owner")),
            members,
        }
    }

    /// File level (path from the root with "/"). Memory notes are read from disk (field in the header).
    pub fn tier(&self, ws: &Workspace, rel: &str) -> Tier {
        if rel.starts_with("agents/memory/") && rel.ends_with(".md") {
            let text = std::fs::read_to_string(ws.root.join(rel)).unwrap_or_default();
            return memory_tier(&text).unwrap_or(Tier::Owner);
        }
        let member = self
            .members
            .iter()
            .filter(|(p, _, _)| rel == p || rel.starts_with(&format!("{p}/")))
            .max_by_key(|(p, _, _)| p.len());
        match member {
            Some((p, base, rules)) => {
                let inner = rel.strip_prefix(&format!("{p}/")).unwrap_or("");
                let base = base.unwrap_or(self.default);
                rules.strictest(inner).map_or(base, |t| t.max(base))
            }
            None => self.shared.strictest(rel).unwrap_or(self.default),
        }
    }
}

/// `visibility: team` in the note header (between the first `---`).
pub fn memory_tier(text: &str) -> Option<Tier> {
    let body = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = body.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }
    for l in lines {
        let t = l.trim();
        if t == "---" {
            break;
        }
        if let Some(v) = t.strip_prefix("visibility:") {
            return Tier::parse(v.trim().trim_matches('"'));
        }
    }
    None
}

/// Leak search in a public file: public IPv4 (except loopback, private and documentation ones), owner markers.
pub struct LeakScan {
    ip: Regex,
    markers: Vec<Regex>,
    /// well-known addresses that say nothing about our machines (1.1.1.1, examples in tests)
    ip_ok: Vec<String>,
}

impl LeakScan {
    pub fn new(markers: &[String], ip_ok: &[String]) -> LeakScan {
        LeakScan {
            ip: Regex::new(r"\b(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})\b").unwrap(),
            markers: markers.iter().filter_map(|m| Regex::new(m).ok()).collect(),
            ip_ok: ip_ok.to_vec(),
        }
    }

    /// First finding: (line number, what was found).
    pub fn first(&self, text: &str) -> Option<(usize, String)> {
        text.lines()
            .enumerate()
            .find_map(|(i, l)| self.line(l).into_iter().next().map(|w| (i + 1, w)))
    }

    /// All findings (for the export check).
    pub fn all(&self, text: &str) -> Vec<(usize, String)> {
        text.lines()
            .enumerate()
            .flat_map(|(i, l)| self.line(l).into_iter().map(move |w| (i + 1, w)))
            .collect()
    }

    fn line(&self, line: &str) -> Vec<String> {
        let mut out = Vec::new();
        for c in self.ip.captures_iter(line) {
            let o: Vec<u32> = (1..=4).map(|k| c[k].parse().unwrap_or(999)).collect();
            if o.iter().any(|&x| x > 255) || !public_ip(&o) || self.ip_ok.iter().any(|a| *a == c[0])
            {
                continue;
            }
            out.push(format!("IP address {}", &c[0]));
        }
        for r in &self.markers {
            if let Some(m) = r.find(line) {
                out.push(format!("owner marker '{}'", m.as_str()));
            }
        }
        out
    }
}

/// An address that says something about our machines: not loopback, not private, not 0.x, not documentation
/// (192.0.2.x, 198.51.100.x, 203.0.113.x), not a network address.
fn public_ip(o: &[u32]) -> bool {
    let doc = matches!(
        (o[0], o[1], o[2]),
        (192, 0, 2) | (198, 51, 100) | (203, 0, 113)
    );
    let private = o[0] == 10
        || o[0] == 127
        || (o[0] == 192 && o[1] == 168)
        || (o[0] == 172 && (16..32).contains(&o[1]));
    // .0 at the end is a network address, not a machine; versions like 4.27.2.0 look the same
    let special = o[0] == 0 || o[0] >= 224 || o[3] == 0;
    !(doc || private || special)
}

/// All git-tracked files with their levels (for ws roles and the future export).
pub fn classify(ws: &Workspace) -> Result<Vec<(String, Tier)>, String> {
    let files = crate::check::tracked(&ws.root)?;
    let c = Classifier::new(ws);
    Ok(files
        .into_iter()
        .map(|f| {
            let t = c.tier(ws, &f);
            (f, t)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globs() {
        assert!(glob("assets/excel").is_match("assets/excel/x.json"));
        assert!(glob("assets/excel").is_match("assets/excel"));
        assert!(!glob("assets/excel").is_match("assets/excelsior"));
        assert!(glob("docs/private_*").is_match("docs/private_notes/a.c"));
        assert!(glob("**/*.uasset").is_match("game/x/y.uasset"));
        assert!(glob("**/*.uasset").is_match("y.uasset"));
        assert!(!glob("*.md").is_match("docs/a.md"));
    }

    #[test]
    fn memory_header() {
        assert_eq!(
            memory_tier("---\nname: x\nmetadata:\n  visibility: team\n---\nbody"),
            Some(Tier::Team)
        );
        assert_eq!(memory_tier("---\nname: x\n---\nvisibility: public"), None);
    }

    #[test]
    fn leaks() {
        let s = LeakScan::new(&["OwnerName".into()], &["1.1.1.1".into()]);
        assert!(
            s.first("dns 1.1.1.1").is_none(),
            "address from the allowlist"
        );
        assert!(s.first("repo OwnerName/dungeonborne").is_some());
        assert!(s.first("addr = \"127.0.0.1:8080\"; v = 1.2.0.0").is_none());
        assert!(s
            .first("relay 203.0.113.10:7443, lan 192.168.1.5")
            .is_none());
        assert_eq!(s.first("x\nserver 5.6.7.8").map(|f| f.0), Some(2));
    }
}
