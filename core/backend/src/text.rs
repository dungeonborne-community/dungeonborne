//! Server texts for players, with translations: server description and message of the day (`server.toml`), maintenance
//! (`maintenance.txt`), season name (`seasons/<id>/season.toml`). The launcher shows them in its own language.
//!
//! In TOML the field is a string (as before, one language) or a table of languages:
//! ```toml
//! motd = "Season 2: new dungeons"
//! motd = { ru = "Сезон 2: новые подземелья", en = "Season 2: new dungeons" }
//! ```
//! In the JSON for the launcher the old field stays a string in the primary language (old launchers read it too), next to it —
//! `<field>_i18n: { ru, en }`, if there are translations.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{json, Value};

/// Primary language of server texts: old launchers use it, and so does everyone whose language has no translation
pub const BASE: &str = "ru";

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Text {
    Plain(String),
    ByLang(BTreeMap<String, String>),
}

impl Default for Text {
    fn default() -> Self {
        Text::Plain(String::new())
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::Plain(s.to_string())
    }
}

impl Text {
    /// Text in the primary language (or the first one available)
    pub fn base(&self) -> &str {
        match self {
            Text::Plain(s) => s,
            Text::ByLang(m) => m.get(BASE).or_else(|| m.values().next()).map_or("", String::as_str),
        }
    }

    /// Text in language `lang` if present; otherwise the primary one
    pub fn get(&self, lang: Option<&str>) -> &str {
        match (self, lang) {
            (Text::ByLang(m), Some(l)) => m.get(l).map_or_else(|| self.base(), String::as_str),
            _ => self.base(),
        }
    }

    /// `<field>_i18n` for the launcher: all languages, `null` — no translations
    pub fn i18n_json(&self) -> Value {
        match self {
            Text::ByLang(m) if m.len() > 1 => json!(m),
            _ => Value::Null,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.base().trim().is_empty()
    }

    /// Text file with translations (`maintenance.txt`): up to the first `[en]` line — primary language, after it — the translation;
    /// a `[xx]` line opens language xx. Empty parts do not count.
    pub fn parse_sections(text: &str) -> Text {
        let mut parts: BTreeMap<String, Vec<&str>> = BTreeMap::new();
        let mut lang = BASE.to_string();
        for line in text.lines() {
            let t = line.trim();
            let code = t.strip_prefix('[').and_then(|r| r.strip_suffix(']'));
            if let Some(c) = code.filter(|c| (2..=3).contains(&c.len()) && c.bytes().all(|b| b.is_ascii_lowercase())) {
                lang = c.to_string();
                continue;
            }
            parts.entry(lang.clone()).or_default().push(line);
        }
        let map: BTreeMap<String, String> = parts
            .into_iter()
            .map(|(l, lines)| (l, lines.join("\n").trim().to_string()))
            .filter(|(_, t)| !t.is_empty())
            .collect();
        match map.len() {
            0 => Text::default(),
            1 if map.contains_key(BASE) => Text::Plain(map[BASE].clone()),
            _ => Text::ByLang(map),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_string_or_table() {
        #[derive(Deserialize)]
        struct S {
            a: Text,
            b: Text,
        }
        let s: S = toml::from_str("a = \"Привет\"\nb = { ru = \"Привет\", en = \"Hello\" }\n").unwrap();
        assert_eq!(s.a.base(), "Привет");
        assert_eq!(s.a.get(Some("en")), "Привет", "no translation — primary");
        assert_eq!(s.a.i18n_json(), Value::Null);
        assert_eq!(s.b.base(), "Привет");
        assert_eq!(s.b.get(Some("en")), "Hello");
        assert_eq!(s.b.get(Some("de")), "Привет");
        assert_eq!(s.b.i18n_json(), json!({ "ru": "Привет", "en": "Hello" }));
        let only_en: Text = toml::from_str::<BTreeMap<String, Text>>("x = { en = \"Hi\" }").unwrap().remove("x").unwrap();
        assert_eq!(only_en.base(), "Hi", "no primary — the first one available");
    }

    #[test]
    fn maintenance_file_sections() {
        assert_eq!(Text::parse_sections("  \n"), Text::default());
        assert_eq!(Text::parse_sections("Обновление, 10 минут\n"), Text::Plain("Обновление, 10 минут".into()));
        let t = Text::parse_sections("Тех. работы до 19:00 МСК\n[en]\nMaintenance until 16:00 UTC\n");
        assert_eq!(t.base(), "Тех. работы до 19:00 МСК");
        assert_eq!(t.get(Some("en")), "Maintenance until 16:00 UTC");
        // [EN] or [english] are not language markers: they stay text
        assert_eq!(Text::parse_sections("a\n[EN]\nb").base(), "a\n[EN]\nb");
        assert_eq!(Text::parse_sections("[en]\nOnly English").base(), "Only English");
    }
}
