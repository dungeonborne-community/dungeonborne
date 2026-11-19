//! Patch `dbbalance-patch/1`: read leniently (an LLM may have written it), judged by `check`.

use serde_json::Value as Json;

pub const PATCH_FORMAT: &str = "dbbalance-patch/1";

#[derive(Clone, Debug, PartialEq)]
pub struct PatchChange {
    pub table: String,
    /// `None` — no row number, or it is not an integer ≥ 0
    pub row: Option<usize>,
    pub key: String,
    pub title: String,
    pub path: String,
    pub old: String,
    pub new: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub format: String,
    pub base: String,
    /// Outer `None` — no field (old patch), `Some(None)` — the server base, `Some(Some(id))` — a season
    pub season: Option<Option<String>>,
    pub author: String,
    pub comment: String,
    pub changes: Vec<PatchChange>,
}

/// Value as text: an LLM may write `"new": 12` instead of `"new": "12"`
fn text(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

impl Patch {
    pub fn from_json(p: &Json) -> Result<Patch, String> {
        if !p.is_object() {
            return Err("the patch must be a JSON object".into());
        }
        let changes = match &p["changes"] {
            Json::Array(a) => a,
            Json::Null => return Err("the patch has no changes list".into()),
            _ => return Err("changes must be a list".into()),
        };
        Ok(Patch {
            format: text(&p["format"]),
            base: text(&p["base"]),
            season: match p.get("season") {
                Some(Json::String(id)) => Some(Some(id.clone())),
                Some(Json::Null) => Some(None),
                _ => None,
            },
            author: text(&p["author"]),
            comment: text(&p["comment"]),
            changes: changes
                .iter()
                .map(|c| PatchChange {
                    table: text(&c["table"]),
                    row: c["row"].as_u64().map(|r| r as usize),
                    key: text(&c["key"]),
                    title: text(&c["title"]),
                    path: text(&c["path"]),
                    old: text(&c["old"]),
                    new: text(&c["new"]),
                })
                .collect(),
        })
    }

    pub fn read(path: &std::path::Path) -> Result<Patch, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {}", path.display(), e))?;
        let json: Json = serde_json::from_str(text.strip_prefix('\u{feff}').unwrap_or(text)).map_err(|e| format!("not readable as JSON: {}", e))?;
        Patch::from_json(&json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn season_states() {
        let read = |p: Json| Patch::from_json(&p).unwrap().season;
        assert_eq!(read(json!({"changes": []})), None);
        assert_eq!(read(json!({"changes": [], "season": null})), Some(None));
        assert_eq!(read(json!({"changes": [], "season": "dead-money"})), Some(Some("dead-money".into())));
    }

    #[test]
    fn numbers_become_text() {
        let p = Patch::from_json(&json!({"changes": [{"table": "t", "row": 2, "key": 7, "path": "a", "old": 1.5, "new": 12}]})).unwrap();
        let c = &p.changes[0];
        assert_eq!((c.row, c.key.as_str(), c.old.as_str(), c.new.as_str()), (Some(2), "7", "1.5", "12"));
    }

    #[test]
    fn bad_shapes() {
        assert!(Patch::from_json(&json!([])).is_err());
        assert!(Patch::from_json(&json!({})).is_err());
        assert!(Patch::from_json(&json!({"changes": 1})).is_err());
    }
}
