//! Designer edits on top of an open kit: check, undo, draft `dbbalance-draft/1`, patch `dbbalance-patch/1`.

use std::collections::{BTreeMap, HashMap};

use serde_json::{json, Value as Json};

use crate::check::{check, Expect, Issue, Severity};
use crate::patch::{Patch, PatchChange, PATCH_FORMAT};
use crate::values::{as_number, same};
use crate::view::{KitView, TableView};

pub const DRAFT_FORMAT: &str = "dbbalance-draft/1";
const UNDO_LIMIT: usize = 200;
/// `check` codes for which an edit from a foreign patch has nowhere to go
const NOT_APPLICABLE: &[&str] = &["table_unknown", "field_locked", "row_missing", "key_mismatch", "old_mismatch", "new_empty", "duplicate_change"];

/// (table index, row, path index)
pub(crate) type CellKey = (usize, usize, u32);

#[derive(Clone)]
struct Step {
    key: CellKey,
    before: Option<String>,
    after: Option<String>,
}

#[derive(Default)]
pub(crate) struct Session {
    pub(crate) edits: BTreeMap<CellKey, String>,
    undo: Vec<Vec<Step>>,
    redo: Vec<Vec<Step>>,
    pub(crate) comment: String,
    pub(crate) generation: u64,
    pub(crate) issues: HashMap<CellKey, Issue>,
    errors: usize,
    warnings: usize,
}

/// FNV-1a: the hash protects against nothing, it only identifies a row across kit versions
fn fnv(text: &str, mut h: u64) -> u64 {
    for b in text.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// Hash of the non-editable values of a row: catches shifted rows when `key` and the index among such `key`s matched by chance
pub(crate) fn row_hash(t: &TableView, row_pos: usize) -> String {
    let r = &t.rows[row_pos];
    let mut parts: Vec<(&str, &str)> = r
        .cells
        .iter()
        .filter(|(p, _)| !t.paths[*p as usize].editable)
        .map(|(p, v)| (t.paths[*p as usize].path.as_str(), t.values[*v as usize].value.as_str()))
        .collect();
    parts.sort_unstable();
    let mut h = 0xcbf2_9ce4_8422_2325u64;
    for (p, v) in parts {
        h = fnv(p, h);
        h = fnv("=", h);
        h = fnv(v, h);
        h = fnv("\n", h);
    }
    format!("{:016x}", h)
}

/// A comma in a number becomes a dot; the rest as is
fn normalize(text: &str) -> String {
    let t = text.trim();
    if as_number(t).is_some() {
        t.replace(',', ".")
    } else {
        t.to_string()
    }
}

/// Patch file name: as in make_patch.py, but without characters forbidden in file names
pub fn patch_file_name(author: &str, stamp: &str) -> String {
    let clean = |s: &str| -> String { s.chars().map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_') { c } else { '_' }).collect() };
    let name = clean(author.trim());
    format!("patch-{}-{}.json", if name.is_empty() { "author" } else { &name }, clean(stamp))
}

impl KitView {
    fn locate(&self, table: &str, row: usize, path: &str) -> Result<CellKey, &'static str> {
        let ti = self.tables.iter().position(|t| t.name == table).ok_or("table_unknown")?;
        let t = &self.tables[ti];
        let pid = *t.path_ids.get(path).ok_or("field_locked")?;
        if !t.paths[pid as usize].editable {
            return Err("field_locked");
        }
        t.row_pos(row).ok_or("row_missing")?;
        Ok((ti, row, pid))
    }

    fn kit_value(&self, key: CellKey) -> &str {
        let t = &self.tables[key.0];
        t.row_pos(key.1).and_then(|p| t.rows[p].value_of(t, key.2)).unwrap_or("")
    }

    /// The second half of a min/max pair
    fn pair_of(&self, key: CellKey) -> Option<CellKey> {
        let t = &self.tables[key.0];
        let path = &t.paths[key.2 as usize].path;
        let other = match (path.strip_suffix("min_value"), path.strip_suffix("max_value")) {
            (Some(base), _) => format!("{}max_value", base),
            (_, Some(base)) => format!("{}min_value", base),
            _ => return None,
        };
        t.path_ids.get(&other).map(|p| (key.0, key.1, *p))
    }

    fn changes_of(&self, s: &Session) -> Vec<PatchChange> {
        s.edits
            .iter()
            .map(|(k, new)| {
                let t = &self.tables[k.0];
                let r = t.row_pos(k.1).map(|p| &t.rows[p]);
                PatchChange {
                    table: t.name.clone(),
                    row: Some(k.1),
                    key: r.map(|x| x.key.clone()).unwrap_or_default(),
                    title: r.map(|x| x.title[0].clone()).unwrap_or_default(),
                    path: t.paths[k.2 as usize].path.clone(),
                    old: self.kit_value(*k).to_string(),
                    new: new.clone(),
                }
            })
            .collect()
    }

    fn patch_of(&self, s: &Session, author: &str, comment: &str) -> Patch {
        Patch {
            format: PATCH_FORMAT.to_string(),
            base: self.kit.version().to_string(),
            season: Some(self.kit.season().map(str::to_string)),
            author: author.to_string(),
            comment: comment.to_string(),
            changes: self.changes_of(s),
        }
    }

    /// The same rules as on the server. Cell marks come from here
    fn recheck(&self, s: &mut Session) {
        let patch = self.patch_of(s, "-", "");
        let keys: Vec<CellKey> = s.edits.keys().copied().collect();
        let tables = self.check_tables(&patch);
        let report = check(&patch, &tables.lock().unwrap(), &Expect { version: Some(self.kit.version()), season: Some(self.kit.season()) });
        s.issues.clear();
        s.errors = report.errors.len();
        s.warnings = report.warnings.len();
        // An error matters more than a warning on the same cell
        for i in report.warnings.iter().chain(report.errors.iter()) {
            if let Some(k) = i.at.and_then(|a| keys.get(a)) {
                s.issues.insert(*k, i.clone());
                if i.code == "min_gt_max" {
                    if let Some(pair) = self.pair_of(*k).filter(|p| s.edits.contains_key(p)) {
                        s.issues.insert(pair, i.clone());
                    }
                }
            }
        }
        s.generation += 1;
    }

    pub(crate) fn status_of(&self, s: &Session) -> Json {
        json!({
            "count": s.edits.len(), "errors": s.errors, "warnings": s.warnings,
            "can_undo": !s.undo.is_empty(), "can_redo": !s.redo.is_empty(), "comment": s.comment
        })
    }

    pub fn status(&self) -> Json {
        self.status_of(&self.session.lock().unwrap())
    }

    fn apply_steps(s: &mut Session, steps: &[Step], forward: bool) {
        let mut ordered: Vec<&Step> = steps.iter().collect();
        if !forward {
            ordered.reverse();
        }
        for st in ordered {
            match if forward { &st.after } else { &st.before } {
                Some(v) => s.edits.insert(st.key, v.clone()),
                None => s.edits.remove(&st.key),
            };
        }
    }

    /// One undo entry for the whole batch. An empty value or the kit value removes the edit.
    pub(crate) fn commit(&self, s: &mut Session, wanted: Vec<(CellKey, String)>) -> usize {
        let mut steps = Vec::new();
        for (key, text) in wanted {
            let new = normalize(&text);
            let after = (!new.is_empty() && !same(&new, self.kit_value(key))).then_some(new);
            let before = s.edits.get(&key).cloned();
            if before != after {
                let step = Step { key, before, after };
                Self::apply_steps(s, std::slice::from_ref(&step), true);
                steps.push(step);
            }
        }
        let n = steps.len();
        if n > 0 {
            s.undo.push(steps);
            if s.undo.len() > UNDO_LIMIT {
                s.undo.remove(0);
            }
            s.redo.clear();
            self.recheck(s);
        }
        n
    }

    /// `(table, row, path, new value)`. Cells that cannot be edited are not applied and go to `rejected`.
    pub fn set_values(&self, edits: &[(String, usize, String, String)]) -> Json {
        let mut s = self.session.lock().unwrap();
        let mut wanted = Vec::with_capacity(edits.len());
        let mut rejected = Vec::new();
        for (table, row, path, new) in edits {
            match self.locate(table, *row, path) {
                Ok(k) => wanted.push((k, new.clone())),
                Err(code) => rejected.push(json!({ "table": table, "row": row, "path": path, "code": code })),
            }
        }
        let changed = self.commit(&mut s, wanted);
        let mut out = self.status_of(&s);
        out["changed"] = json!(changed);
        out["rejected"] = Json::Array(rejected);
        out
    }

    pub fn revert(&self, cells: &[(String, usize, String)]) -> Json {
        let edits: Vec<_> = cells.iter().map(|(t, r, p)| (t.clone(), *r, p.clone(), String::new())).collect();
        self.set_values(&edits)
    }

    pub fn undo(&self) -> Json {
        let mut s = self.session.lock().unwrap();
        if let Some(steps) = s.undo.pop() {
            Self::apply_steps(&mut s, &steps, false);
            s.redo.push(steps);
            self.recheck(&mut s);
        }
        self.status_of(&s)
    }

    pub fn redo(&self) -> Json {
        let mut s = self.session.lock().unwrap();
        if let Some(steps) = s.redo.pop() {
            Self::apply_steps(&mut s, &steps, true);
            s.undo.push(steps);
            self.recheck(&mut s);
        }
        self.status_of(&s)
    }

    pub fn set_comment(&self, comment: &str) {
        self.session.lock().unwrap().comment = comment.to_string();
    }

    pub fn list_changes(&self, en: bool) -> Json {
        let s = self.session.lock().unwrap();
        let list: Vec<Json> = s
            .edits
            .iter()
            .map(|(k, new)| {
                let t = &self.tables[k.0];
                let r = t.row_pos(k.1).map(|p| &t.rows[p]);
                let old = self.kit_value(*k);
                let mut c = json!({
                    "table": t.name, "table_title": t.title_of(en), "row": k.1, "path": t.paths[k.2 as usize].path,
                    "key": r.map(|x| x.key.as_str()).unwrap_or_default(),
                    "title": r.map(|x| x.title[usize::from(en)].as_str()).unwrap_or_default(),
                    "field": t.column_json(k.2, en), "old": old, "new": new
                });
                if let Some(x) = r {
                    let context: Vec<Json> = x
                        .cells
                        .iter()
                        .filter(|(p, _)| t.paths[*p as usize].context)
                        .map(|(p, v)| json!({ "label": t.column_json(*p, en)["label"], "value": t.values[*v as usize].value }))
                        .collect();
                    if !context.is_empty() {
                        c["context"] = Json::Array(context);
                    }
                    if let Some(i) = &x.icon {
                        c["icon"] = json!(i);
                    }
                    if !x.unit.is_empty() {
                        c["unit"] = json!(x.unit);
                    }
                }
                if let Some(o) = t.diff.get(&(k.1, k.2)) {
                    match o {
                        Some(text) => c["orig"] = json!(text),
                        None => c["orig_absent"] = json!(true),
                    }
                }
                if let Some(i) = s.issues.get(k) {
                    c["issue"] = json!({ "severity": if i.severity == Severity::Error { "error" } else { "warning" }, "code": i.code, "message": i.message });
                }
                c
            })
            .collect();
        let mut out = self.status_of(&s);
        out["changes"] = Json::Array(list);
        out
    }

    /// Patch for writing to a file; not produced with errors, nor when empty
    pub fn patch_json(&self, author: &str, comment: &str, created: &str, app: &str) -> Result<Json, (&'static str, String)> {
        let s = self.session.lock().unwrap();
        if s.edits.is_empty() {
            return Err(("patch_empty", "no edits".into()));
        }
        if s.errors > 0 {
            return Err(("patch_has_errors", format!("errors: {}", s.errors)));
        }
        let changes: Vec<Json> = self
            .changes_of(&s)
            .into_iter()
            .map(|c| json!({ "table": c.table, "row": c.row, "key": c.key, "title": c.title, "path": c.path, "old": c.old, "new": c.new }))
            .collect();
        Ok(json!({
            "format": PATCH_FORMAT, "base": self.kit.version(), "season": self.kit.season(), "author": author, "comment": comment,
            "created": created, "app": app, "changes": changes
        }))
    }

    /// A foreign patch as edits. Wrong header — refused entirely. Edits with nowhere to go go to `rejected`, the rest are applied
    /// and shown with errors so they can be fixed. All in one undo step.
    pub fn import_patch(&self, patch: &Patch) -> Result<Json, (&'static str, String)> {
        let tables = self.check_tables(patch);
        let expect = Expect { version: Some(self.kit.version()), season: Some(self.kit.season()) };
        let report = check(patch, &tables.lock().unwrap(), &expect);
        if let Some(e) = report.errors.iter().find(|e| e.at.is_none()) {
            return Err(("patch_invalid", e.message.clone()));
        }
        let mut not_applicable: BTreeMap<usize, &Issue> = BTreeMap::new();
        for e in report.errors.iter().filter(|e| NOT_APPLICABLE.contains(&e.code)) {
            if let Some(a) = e.at {
                not_applicable.entry(a).or_insert(e);
            }
        }
        let mut wanted = Vec::new();
        let mut rejected: Vec<Json> = not_applicable.values().map(|e| json!({ "at": e.at, "code": e.code, "message": e.message })).collect();
        for (i, c) in patch.changes.iter().enumerate() {
            if not_applicable.contains_key(&i) {
                continue;
            }
            let Some(row) = c.row else { continue };
            match self.locate(&c.table, row, &c.path) {
                Ok(k) => wanted.push((k, c.new.clone())),
                // The field matches the rules, but the kit has no such column: there is nowhere to show it in the table
                Err(code) => rejected.push(json!({ "at": i, "code": code, "message": format!("#{} {} {}", i + 1, c.table, c.path) })),
            }
        }
        let mut s = self.session.lock().unwrap();
        let applied = wanted.len();
        self.commit(&mut s, wanted);
        if s.comment.is_empty() {
            s.comment = patch.comment.clone();
        }
        let warnings: Vec<Json> = report.warnings.iter().filter(|w| w.at.is_none()).map(|w| json!({ "code": w.code, "message": w.message })).collect();
        let mut out = self.status_of(&s);
        out["applied"] = json!(applied);
        out["rejected"] = Json::Array(rejected);
        out["patch_warnings"] = Json::Array(warnings);
        out["author"] = json!(patch.author);
        Ok(out)
    }

    pub fn draft_json(&self) -> Json {
        let s = self.session.lock().unwrap();
        let changes: Vec<Json> = s
            .edits
            .iter()
            .filter_map(|(k, new)| {
                let t = &self.tables[k.0];
                let p = t.row_pos(k.1)?;
                let r = &t.rows[p];
                Some(json!({
                    "table": t.name, "row": k.1, "key": r.key, "title": r.title[0], "n": r.n, "hash": row_hash(t, p),
                    "path": t.paths[k.2 as usize].path, "old": self.kit_value(*k), "new": new
                }))
            })
            .collect();
        json!({ "format": DRAFT_FORMAT, "base": self.kit.version(), "season": self.kit.season(), "comment": s.comment, "changes": changes })
    }

    /// A draft of the same kit version. Edits that did not match the kit are skipped. Returns their number.
    pub fn load_draft(&self, d: &Json) -> usize {
        if d["format"].as_str() != Some(DRAFT_FORMAT) || d["base"].as_str() != Some(self.kit.version()) {
            return 0;
        }
        let mut s = self.session.lock().unwrap();
        s.comment = d["comment"].as_str().unwrap_or_default().to_string();
        s.edits.clear();
        let mut dropped = 0;
        for c in d["changes"].as_array().into_iter().flatten() {
            let text = |k: &str| c[k].as_str().unwrap_or_default();
            let Some(row) = c["row"].as_u64().map(|r| r as usize) else {
                dropped += 1;
                continue;
            };
            let key = self.locate(text("table"), row, text("path"));
            let fits = key.ok().filter(|k| {
                let t = &self.tables[k.0];
                let hash_ok = t.row_pos(k.1).is_some_and(|p| row_hash(t, p) == text("hash"));
                hash_ok && same(self.kit_value(*k), text("old"))
            });
            match fits {
                Some(k) => {
                    s.edits.insert(k, text("new").to_string());
                }
                None => dropped += 1,
            }
        }
        self.recheck(&mut s);
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_normalized() {
        assert_eq!(normalize(" 1,5 "), "1.5");
        assert_eq!(normalize("12"), "12");
        assert_eq!(normalize("a,b"), "a,b");
    }

    #[test]
    fn file_names_are_safe() {
        assert_eq!(patch_file_name("Ник", "20261106-1200"), "patch-Ник-20261106-1200.json");
        assert_eq!(patch_file_name("../a b:c", "1"), "patch-___a_b_c-1.json");
        assert_eq!(patch_file_name("  ", "1"), "patch-author-1.json");
    }
}
