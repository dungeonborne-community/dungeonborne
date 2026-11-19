//! Server changelog for the launcher. Source: `seasons/<id>/patch_notes.toml` of the active season (`SEASON`, `SEASONS_DIR`),
//! and if there is no season or it has no file, `[patch_notes] file` (default `patch_notes.toml` in the working directory).
//! Season notes are served with a `season {id, name}` field: the launcher labels whose list it is.
//!
//! Format is TOML, not Markdown: an entry has fields (date, version, sections), the server checks them on read, and the launcher
//! shows ready strings as text, without parsing markup and without HTML. Example: `seasons/playtest/patch_notes.toml`.
//!
//! ```toml
//! [[update]]
//! version = "2026.11.07"       # optional; the launcher uses it to detect a new entry (otherwise by date)
//! date = "2026-11-07"
//! title = "Выкладка 07.11"
//! new = ["…"]                   # sections: new = new features, fixed = fixes, balance = balance changes
//! fixed = ["…"]
//! balance = ["…"]
//!
//! [update.lang.en]             # translation: the same fields; whatever is missing comes from the main text
//! title = "Update of 7 November"
//! new = ["…"]
//! ```
//!
//! The main text is in the language `lang` at the top of the file (default `ru`). The launcher asks for its language (`?lang=en`) and gets
//! the translation if there is one, otherwise the main text; each entry's `lang` field says which language it is in.
//!
//! No file or a file with an error: no `patch_notes` field in the manifest, `/dbserver/patch_notes` answers 404. A season file
//! with an error is not replaced by the root one: players must not see someone else's list.
//! The file is re-read on change, no server restart needed.

use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub const SCHEMA: &str = "dbnotes/1";
/// File name in the season folder
pub const FILE: &str = "patch_notes.toml";
const MAX_FILE: u64 = 256 * 1024;
const MAX_UPDATES: usize = 200;
const MAX_ITEMS: usize = 100;
const MAX_TEXT: usize = 2000;
const SECTIONS: [&str; 3] = ["new", "fixed", "balance"];

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    /// Language of the main text
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    update: Vec<Update>,
}

/// Entry translation: everything is optional, whatever is missing comes from the main text
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(deny_unknown_fields)]
struct Translation {
    title: Option<String>,
    new: Option<Vec<String>>,
    fixed: Option<Vec<String>>,
    balance: Option<Vec<String>>,
}

impl Translation {
    fn section(&self, name: &str) -> Option<&[String]> {
        match name {
            "new" => self.new.as_deref(),
            "fixed" => self.fixed.as_deref(),
            _ => self.balance.as_deref(),
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
struct Update {
    #[serde(default)]
    version: String,
    date: String,
    title: String,
    #[serde(default)]
    new: Vec<String>,
    #[serde(default)]
    fixed: Vec<String>,
    #[serde(default)]
    balance: Vec<String>,
    /// Translations: language code -> text
    #[serde(default)]
    lang: std::collections::BTreeMap<String, Translation>,
}

impl Update {
    fn key(&self) -> &str {
        if self.version.is_empty() {
            &self.date
        } else {
            &self.version
        }
    }

    fn section(&self, name: &str) -> &[String] {
        match name {
            "new" => &self.new,
            "fixed" => &self.fixed,
            _ => &self.balance,
        }
    }
}

/// Checked notes: the newest entry first
#[derive(Debug)]
pub struct Notes {
    /// Language of the main text
    lang: String,
    updates: Vec<Update>,
    /// (id, name) of the season, if these are its notes
    season: Option<(String, crate::text::Text)>,
}

/// Active season notes: where they are and what the season is called
#[derive(Clone, Debug)]
pub struct SeasonNotes {
    pub id: String,
    pub name: crate::text::Text,
    pub file: PathBuf,
}

impl Notes {
    /// What distinguishes note versions for the launcher: the key of the newest entry
    pub fn latest(&self) -> &str {
        self.updates[0].key()
    }

    /// Notes in language `want` where there is a translation; otherwise the main text
    pub fn json(&self, want: Option<&str>) -> Value {
        let updates: Vec<Value> = self
            .updates
            .iter()
            .map(|u| {
                let tr = want.filter(|l| *l != self.lang).and_then(|l| u.lang.get(l).map(|t| (l, t)));
                let sections: Vec<Value> = SECTIONS
                    .iter()
                    .map(|s| (s, tr.and_then(|(_, t)| t.section(s)).unwrap_or(u.section(s))))
                    .filter(|(_, items)| !items.is_empty())
                    .map(|(s, items)| json!({ "kind": s, "items": items }))
                    .collect();
                let title = tr.and_then(|(_, t)| t.title.as_deref()).unwrap_or(&u.title);
                let lang = tr.map_or(self.lang.as_str(), |(l, _)| l);
                json!({ "id": u.key(), "version": u.version, "date": u.date, "title": title, "lang": lang, "sections": sections })
            })
            .collect();
        let mut out = json!({ "schema": SCHEMA, "latest": self.latest(), "lang": self.lang, "updates": updates });
        if let Some((id, name)) = &self.season {
            out["season"] = json!({ "id": id, "name": name.get(want) });
        }
        out
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

fn clean(s: &str) -> bool {
    !s.trim().is_empty() && s.chars().count() <= MAX_TEXT && !s.chars().any(|c| c.is_control() && c != '\n')
}

pub fn parse(text: &str) -> Result<Notes, String> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
    if f.update.is_empty() {
        return Err("no [[update]] entries at all".into());
    }
    if f.update.len() > MAX_UPDATES {
        return Err(format!("more than {} entries", MAX_UPDATES));
    }
    let lang_ok = |l: &str| (2..=3).contains(&l.len()) && l.bytes().all(|c| c.is_ascii_lowercase());
    let base = f.lang.clone().unwrap_or_else(|| "ru".into());
    if !lang_ok(&base) {
        return Err(format!("lang = \"{}\": need a language code, e.g. ru or en", base));
    }
    let mut keys = std::collections::HashSet::new();
    for (i, u) in f.update.iter().enumerate() {
        let at = format!("[[update]] #{} ({})", i + 1, u.title);
        for (l, t) in &u.lang {
            if !lang_ok(l) || *l == base {
                return Err(format!("{}: translation [update.lang.{}] needs the code of a different language", at, l));
            }
            let items: Vec<&String> = SECTIONS.iter().filter_map(|s| t.section(s)).flatten().collect();
            if t.title.as_deref().is_some_and(|x| !clean(x)) || items.len() > MAX_ITEMS || !items.iter().all(|x| clean(x)) {
                return Err(format!("{}: translation {} has empty or too long text", at, l));
            }
        }
        if !is_date(&u.date) {
            return Err(format!("{}: date must be YYYY-MM-DD, not \"{}\"", at, u.date));
        }
        if !clean(&u.title) || (!u.version.is_empty() && !clean(&u.version)) {
            return Err(format!("{}: empty or too long title/version", at));
        }
        let items: Vec<&String> = SECTIONS.iter().flat_map(|s| u.section(s)).collect();
        if items.is_empty() {
            return Err(format!("{}: no items in new, fixed or balance", at));
        }
        if items.len() > MAX_ITEMS || !items.iter().all(|x| clean(x)) {
            return Err(format!("{}: empty or too long item (or more than {} items)", at, MAX_ITEMS));
        }
        if !keys.insert(u.key().to_string()) {
            return Err(format!("{}: version (or date without version) \"{}\" already belongs to another entry", at, u.key()));
        }
    }
    let mut updates = f.update;
    // Newest first; with the same date, in file order
    updates.sort_by(|a, b| b.date.cmp(&a.date));
    Ok(Notes { lang: base, updates, season: None })
}

pub struct PatchNotes {
    /// Season file (if a season is active), then the common one; the first that exists is taken
    sources: Vec<(PathBuf, Option<(String, crate::text::Text)>)>,
    /// (which source, its modification time) -> notes
    cache: Mutex<(Option<(usize, SystemTime)>, Option<Arc<Notes>>)>,
}

fn load(path: &std::path::Path) -> Option<Notes> {
    let meta = std::fs::metadata(path).ok()?;
    if meta.len() > MAX_FILE {
        eprintln!("[Notes] {}: larger than {} bytes — not served", path.display(), MAX_FILE);
        return None;
    }
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("[Notes] {}: {}", path.display(), e);
            return None;
        }
    };
    match parse(&text) {
        Ok(n) => {
            println!("[Notes] changelog: {} entries, latest {}", n.updates.len(), n.latest());
            Some(n)
        }
        Err(e) => {
            eprintln!("[Notes] {}: {} — changelog not served", path.display(), e);
            None
        }
    }
}

impl PatchNotes {
    pub fn new(file: &str, season: Option<SeasonNotes>) -> Self {
        let mut sources = Vec::new();
        if let Some(s) = season {
            sources.push((s.file, Some((s.id, s.name))));
        }
        if !file.is_empty() {
            sources.push((PathBuf::from(file), None));
        }
        let n = Self { sources, cache: Mutex::new((None, None)) };
        n.current();
        n
    }

    /// Current notes (re-read when the file changes or appears)
    pub fn current(&self) -> Option<Arc<Notes>> {
        let found = self.sources.iter().enumerate().find_map(|(i, (p, _))| std::fs::metadata(p).and_then(|m| m.modified()).ok().map(|t| (i, t)));
        let mut c = self.cache.lock().unwrap();
        if c.0 != found || (found.is_none() && c.1.is_some()) {
            let notes = found.and_then(|(i, _)| {
                let (path, season) = &self.sources[i];
                load(path).map(|mut n| {
                    n.season = season.clone();
                    n
                })
            });
            *c = (found, notes.map(Arc::new));
        }
        c.1.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[[update]]
date = "2026-11-01"
title = "Старое"
fixed = ["Падение"]

[[update]]
version = "2026.11.07"
date = "2026-11-07"
title = "Выкладка 07.11"
new = ["Флажок команды на вещах"]
balance = ["Слоты без ограничения"]
"#;

    #[test]
    fn newest_first_and_sections_in_order() {
        let n = parse(SAMPLE).unwrap();
        assert_eq!(n.latest(), "2026.11.07");
        let j = n.json(None);
        assert_eq!(j["schema"], SCHEMA);
        assert_eq!(j["updates"][0]["title"], "Выкладка 07.11");
        assert_eq!(j["updates"][0]["sections"][0]["kind"], "new");
        assert_eq!(j["updates"][0]["sections"][1]["kind"], "balance");
        assert_eq!(j["updates"][1]["id"], "2026-11-01", "without version: by date");
        assert_eq!(j["updates"][1]["sections"].as_array().unwrap().len(), 1, "empty sections are not served");
    }

    #[test]
    fn translations() {
        let text = format!("{}\n[update.lang.en]\ntitle = \"Update 7.11\"\nnew = [\"Team flag\"]\n", SAMPLE);
        let n = parse(&text).unwrap();
        let en = n.json(Some("en"));
        assert_eq!(en["lang"], "ru", "language of the main text");
        assert_eq!(en["updates"][0]["title"], "Update 7.11");
        assert_eq!(en["updates"][0]["lang"], "en");
        assert_eq!(en["updates"][0]["sections"][0]["items"][0], "Team flag");
        assert_eq!(en["updates"][0]["sections"][1]["items"][0], "Слоты без ограничения", "no section translation: main text");
        assert_eq!(en["updates"][1]["lang"], "ru", "entry without translation");
        assert_eq!(n.json(Some("ru"))["updates"][0]["title"], "Выкладка 07.11");
        assert_eq!(n.json(Some("de"))["updates"][0]["title"], "Выкладка 07.11", "foreign language: main text");
        assert_eq!(n.latest(), "2026.11.07", "translation does not change the entry key");
        for bad in ["[update.lang.ru]\ntitle = \"x\"", "[update.lang.EN]\ntitle = \"x\"", "[update.lang.en]\ntitel = \"x\"", "[update.lang.en]\nnew = [\" \"]"] {
            assert!(parse(&format!("{}\n{}\n", SAMPLE, bad)).is_err(), "{bad}");
        }
        assert!(parse(&format!("lang = \"en\"\n{}", SAMPLE)).is_ok());
        assert!(parse(&format!("lang = \"English\"\n{}", SAMPLE)).is_err());
    }

    #[test]
    fn bad_files_are_refused() {
        for (bad, why) in [
            ("", "empty"),
            ("[[update]]\ndate = \"07.11.2026\"\ntitle = \"x\"\nnew = [\"a\"]", "date"),
            ("[[update]]\ndate = \"2026-11-07\"\ntitle = \"x\"", "no items"),
            ("[[update]]\ndate = \"2026-11-07\"\ntitle = \"x\"\nnew = [\"  \"]", "empty item"),
            ("[[update]]\ndate = \"2026-11-07\"\ntitle = \"x\"\nnews = [\"a\"]", "typo in a section"),
            ("[[update]]\ndate = \"2026-11-07\"\ntitle = \"x\"\nnew = [\"a\"]\n[[update]]\ndate = \"2026-11-07\"\ntitle = \"y\"\nnew = [\"b\"]", "two entries with one key"),
            ("[[update]]\ndate = \"2026-11-07\"\ntitle = \"x\"\nnew = [\"a\u{0007}\"]", "control character"),
        ] {
            assert!(parse(bad).is_err(), "{why}");
        }
    }

    #[test]
    fn file_is_served_and_reread() {
        let dir = std::env::temp_dir().join(format!("dbs_notes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("patch_notes.toml");
        let n = PatchNotes::new(path.to_str().unwrap(), None);
        assert!(n.current().is_none(), "no file");
        std::fs::write(&path, SAMPLE).unwrap();
        assert_eq!(n.current().unwrap().latest(), "2026.11.07");
        std::fs::write(&path, "broken").unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&path).unwrap().set_modified(later).unwrap();
        assert!(n.current().is_none(), "a file with an error is not served");
        std::fs::remove_file(&path).unwrap();
        assert!(n.current().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(PatchNotes::new("", None).current().is_none());
    }

    #[test]
    fn season_notes_first_then_common() {
        let dir = std::env::temp_dir().join(format!("dbs_notes_season_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("seasons/playtest")).unwrap();
        let common = dir.join("patch_notes.toml");
        let own = dir.join("seasons/playtest").join(FILE);
        std::fs::write(&common, "[[update]]\ndate = \"2026-10-01\"\ntitle = \"Common\"\nnew = [\"a\"]\n").unwrap();
        let season = || Some(SeasonNotes { id: "playtest".into(), name: crate::text::Text::ByLang([("ru".to_string(), "Плейтест".to_string()), ("en".to_string(), "Playtest".to_string())].into()), file: own.clone() });

        // No season: the common file, without the season field
        let none = PatchNotes::new(common.to_str().unwrap(), None);
        let j = none.current().unwrap().json(None);
        assert_eq!(j["latest"], "2026-10-01");
        assert!(j.get("season").is_none());

        // Season without its own file: the common one
        let n = PatchNotes::new(common.to_str().unwrap(), season());
        assert_eq!(n.current().unwrap().latest(), "2026-10-01");
        assert!(n.current().unwrap().json(None).get("season").is_none(), "common list is not a season one");

        // The season got a file: its notes, with the season name, without a restart
        std::fs::write(&own, SAMPLE).unwrap();
        let j = n.current().unwrap().json(None);
        assert_eq!(j["latest"], "2026.11.07");
        assert_eq!(j["season"], json!({ "id": "playtest", "name": "Плейтест" }));

        // A season file with an error is not replaced by the common one
        std::fs::write(&own, "broken").unwrap();
        let later = SystemTime::now() + std::time::Duration::from_secs(5);
        std::fs::File::options().write(true).open(&own).unwrap().set_modified(later).unwrap();
        assert!(n.current().is_none());

        // The season file was removed: the common one again
        std::fs::remove_file(&own).unwrap();
        assert_eq!(n.current().unwrap().latest(), "2026-10-01");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_playtest_notes_are_valid() {
        let text = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../content/seasons/playtest/patch_notes.toml")).unwrap();
        // a new entry must not break this test: it checks that the file parses and every entry has an English text
        let n = parse(&text).unwrap();
        assert!(!n.latest().is_empty());
        let en = n.json(Some("en"));
        for u in en["updates"].as_array().unwrap() {
            assert_eq!(u["lang"], "en", "no [update.lang.en] in {}", u["id"]);
        }
    }
}
