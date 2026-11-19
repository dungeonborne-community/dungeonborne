//! Server patch notes (`dbnotes/1`): what changed on the community server, by update. The manifest's optional
//! `patch_notes` names where they are and the key of the newest entry; the UI marks the tab "new" until the player
//! has seen that key (`Settings::notes_seen`, kept locally). A server without the field simply has no notes.

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::{bail, Error, Result};
use crate::manifest::Manifest;
use crate::net::{self, check_url};

const NOTES_LIMIT: usize = 512 * 1024;
const SECTIONS: [&str; 3] = ["new", "fixed", "balance"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notes {
    pub latest: String,
    pub updates: Vec<Update>,
    /// The notes are those of the server's active season
    #[serde(default)]
    pub season: Option<Season>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Season {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub id: String,
    #[serde(default)]
    pub version: String,
    pub date: String,
    pub title: String,
    /// Language this entry's text is in: the asked one when the server has a translation
    #[serde(default)]
    pub lang: String,
    #[serde(default)]
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    pub kind: String,
    pub items: Vec<String>,
}

#[derive(Deserialize)]
struct Wire {
    schema: String,
    latest: String,
    #[serde(default)]
    updates: Vec<Update>,
    #[serde(default)]
    season: Option<Season>,
}

pub fn parse(body: &[u8]) -> Result<Notes> {
    let w: Wire = serde_json::from_slice(body).map_err(|e| Error::new("notes_invalid", e.to_string()))?;
    if w.schema.strip_prefix("dbnotes/").map(|v| v.split('.').next().unwrap_or("")) != Some("1") {
        bail!("schema", "{}", w.schema);
    }
    let mut updates = w.updates;
    // Unknown sections are dropped rather than shown with a raw name: a newer server may add kinds
    for u in &mut updates {
        u.sections.retain(|s| SECTIONS.contains(&s.kind.as_str()) && !s.items.is_empty());
    }
    Ok(Notes { latest: w.latest, updates, season: w.season })
}

/// `None` — the server doesn't publish notes (no field, or the server answers 404). `lang` — the launcher's language:
/// the server sends its translation where it has one, and the original text elsewhere.
pub async fn fetch(client: &Client, m: &Manifest, lang: &str) -> Result<Option<Notes>> {
    let Some(r) = &m.patch_notes else { return Ok(None) };
    let mut url = check_url(&r.url)?;
    url.query_pairs_mut().append_pair("lang", lang);
    match net::get_bytes(client, &url, NOTES_LIMIT).await {
        Ok(body) => parse(&body).map(Some),
        Err(e) if e.code == "http_status" && e.detail.ends_with("404 Not Found") => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shape the backend (`src/auth/notes.rs`) serves.
    const BACKEND: &str = r#"{"schema":"dbnotes/1","latest":"2026.11.07","updates":[
      {"id":"2026.11.07","version":"2026.11.07","date":"2026-11-07","title":"Обновление",
       "sections":[{"kind":"new","items":["a"]},{"kind":"fixed","items":["b"]},{"kind":"future","items":["c"]}]},
      {"id":"2026-11-01","version":"","date":"2026-11-01","title":"Старое","sections":[{"kind":"balance","items":["d"]}]}]}"#;

    #[test]
    fn parses_backend_shape() {
        let n = parse(BACKEND.as_bytes()).unwrap();
        assert_eq!(n.latest, "2026.11.07");
        assert_eq!(n.updates.len(), 2);
        let kinds: Vec<&str> = n.updates[0].sections.iter().map(|s| s.kind.as_str()).collect();
        assert_eq!(kinds, ["new", "fixed"], "an unknown section is dropped");
        assert!(n.season.is_none());
        assert_eq!(n.updates[0].lang, "", "an older server doesn't say");
        let translated = BACKEND.replace(r#""title":"Обновление","#, r#""title":"Update","lang":"en","#);
        assert_eq!(parse(translated.as_bytes()).unwrap().updates[0].lang, "en");
        let seasonal = BACKEND.replace(r#""updates":"#, r#""season":{"id":"playtest","name":"Плейтест"},"updates":"#);
        assert_eq!(parse(seasonal.as_bytes()).unwrap().season.unwrap().name, "Плейтест");
    }

    #[test]
    fn rejects_other_major() {
        assert_eq!(parse(BACKEND.replace("dbnotes/1", "dbnotes/2").as_bytes()).unwrap_err().code, "schema");
        assert!(parse(b"{}").is_err());
    }
}
