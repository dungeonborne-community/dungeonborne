//! Moving draft edits onto a new kit version (docs/BALANCE_APP_SPEC.md, section 7).
//! A row is recognized by `key` and the hash of non-editable values: the row number and the index among such `key`s may shift.

use std::collections::HashMap;

use serde_json::{json, Value as Json};

use crate::edit::{row_hash, CellKey, DRAFT_FORMAT};
use crate::values::same;
use crate::view::KitView;

/// Where one draft edit ended up
enum Fate {
    /// The value in the new kit is the same as it was: moved automatically
    Auto(CellKey),
    /// The new kit already contains this value: the edit is not needed
    Already(CellKey),
    /// The value changed, or the row was not fully recognized: a person decides
    Conflict { key: CellKey, partial: bool, server: String },
    /// The row or the field is gone
    Lost(&'static str),
}

/// Whether moving this draft onto the open kit can be offered. Forward only: a draft is not moved onto an older kit.
pub fn draft_fits(d: &Json, version: &str, season: Option<&str>) -> bool {
    let date = |v: &str| v.split('-').next().unwrap_or_default().to_string();
    d["format"].as_str() == Some(DRAFT_FORMAT)
        && d["season"].as_str() == season
        && d.get("rebased_to").is_none()
        && d["changes"].as_array().is_some_and(|c| !c.is_empty())
        && d["base"].as_str().is_some_and(|b| b != version && date(b) <= date(version))
}

impl KitView {
    fn fate(&self, c: &Json, hashes: &mut HashMap<usize, Vec<String>>) -> Fate {
        let text = |k: &str| c[k].as_str().unwrap_or_default();
        let Some(ti) = self.tables.iter().position(|t| t.name == text("table")) else { return Fate::Lost("table_gone") };
        let t = &self.tables[ti];
        let Some(pid) = t.path_ids.get(text("path")).copied().filter(|p| t.paths[*p as usize].editable) else { return Fate::Lost("field_gone") };
        let row_hashes = hashes.entry(ti).or_insert_with(|| (0..t.rows.len()).map(|p| row_hash(t, p)).collect());
        let (key, hash, n) = (text("key"), text("hash"), c["n"].as_u64().unwrap_or(0) as usize);
        let same_key: Vec<usize> = (0..t.rows.len()).filter(|p| t.rows[*p].key == key).collect();
        let exact: Vec<usize> = same_key.iter().copied().filter(|p| row_hashes[*p] == hash).collect();
        let (pos, partial) = match exact.as_slice() {
            [one] => (*one, false),
            [] => match same_key.iter().find(|p| t.rows[**p].n == n) {
                Some(p) => (*p, true),
                None => return Fate::Lost("row_gone"),
            },
            many => match many.iter().find(|p| t.rows[**p].n == n) {
                Some(p) => (*p, false),
                // Several identical rows, and the index did not match: which one is ours is unknown
                None => (many[0], true),
            },
        };
        let row = &t.rows[pos];
        let cell_key = (ti, row.row, pid);
        let server = row.value_of(t, pid).unwrap_or_default();
        if partial {
            return Fate::Conflict { key: cell_key, partial: true, server: server.to_string() };
        }
        if same(server, text("new")) {
            Fate::Already(cell_key)
        } else if same(server, text("old")) {
            Fate::Auto(cell_key)
        } else {
            Fate::Conflict { key: cell_key, partial: false, server: server.to_string() }
        }
    }

    fn rebase_item(&self, id: usize, c: &Json, key: Option<CellKey>, en: bool) -> Json {
        let mut item = json!({ "id": id, "table": c["table"], "path": c["path"], "key": c["key"], "title": c["title"], "old": c["old"], "new": c["new"] });
        if let Some(k) = key {
            let t = &self.tables[k.0];
            item["table_title"] = json!(t.title_of(en));
            item["field"] = t.column_json(k.2, en);
            if let Some(r) = t.row_pos(k.1).map(|p| &t.rows[p]) {
                item["row"] = json!(k.1);
                item["key"] = json!(r.key);
                item["title"] = json!(r.title[usize::from(en)]);
                item["unit"] = json!(r.unit);
                let context: Vec<Json> = r
                    .cells
                    .iter()
                    .filter(|(p, _)| t.paths[*p as usize].context)
                    .map(|(p, v)| json!({ "label": t.column_json(*p, en)["label"], "value": t.values[*v as usize].value }))
                    .collect();
                if !context.is_empty() {
                    item["context"] = Json::Array(context);
                }
            }
        }
        item
    }

    /// What will happen to each draft edit. Changes nothing.
    pub fn plan_rebase(&self, draft: &Json, en: bool) -> Json {
        let mut hashes = HashMap::new();
        let (mut auto, mut already, mut conflicts, mut lost) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for (id, c) in draft["changes"].as_array().into_iter().flatten().enumerate() {
            match self.fate(c, &mut hashes) {
                Fate::Auto(k) => auto.push(self.rebase_item(id, c, Some(k), en)),
                Fate::Already(k) => already.push(self.rebase_item(id, c, Some(k), en)),
                Fate::Conflict { key, partial, server } => {
                    let mut item = self.rebase_item(id, c, Some(key), en);
                    item["server"] = json!(server);
                    item["partial"] = json!(partial);
                    conflicts.push(item);
                }
                Fate::Lost(reason) => {
                    let mut item = self.rebase_item(id, c, None, en);
                    item["reason"] = json!(reason);
                    lost.push(item);
                }
            }
        }
        json!({
            "from": draft["base"], "to": self.kit.version(), "comment": draft["comment"],
            "auto": auto, "already": already, "conflicts": conflicts, "lost": lost
        })
    }

    /// Moves the edits in one undo step. `decisions`: conflict number -> "mine" or "server".
    /// Returns the state of the edits and the draft edits left without a place: they stay in the old draft.
    pub fn apply_rebase(&self, draft: &Json, decisions: &Json) -> Result<Json, String> {
        let mut hashes = HashMap::new();
        let mut wanted = Vec::new();
        let mut lost = Vec::new();
        let mut applied = 0;
        for (id, c) in draft["changes"].as_array().into_iter().flatten().enumerate() {
            let new = c["new"].as_str().unwrap_or_default().to_string();
            match self.fate(c, &mut hashes) {
                Fate::Auto(k) => {
                    wanted.push((k, new));
                    applied += 1;
                }
                Fate::Already(_) => {}
                Fate::Conflict { key, .. } => match decisions[id.to_string()].as_str() {
                    Some("mine") => {
                        wanted.push((key, new));
                        applied += 1;
                    }
                    Some("server") => {}
                    _ => return Err(format!("no decision for edit {}", id)),
                },
                Fate::Lost(_) => lost.push(c.clone()),
            }
        }
        let mut s = self.session.lock().unwrap();
        self.commit(&mut s, wanted);
        if s.comment.is_empty() {
            s.comment = draft["comment"].as_str().unwrap_or_default().to_string();
        }
        let mut out = self.status_of(&s);
        out["applied"] = json!(applied);
        out["lost"] = Json::Array(lost);
        Ok(out)
    }
}
