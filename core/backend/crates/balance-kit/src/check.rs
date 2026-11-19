//! Patch check. The same rules live in `balance_kit/tools/check.py`; the codes are shared, see `tests/fixtures/cases.json`.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::Value as Json;

use crate::patch::{Patch, PATCH_FORMAT};
use crate::values::{as_number, editable, fmt_range, generic, is_int_text, ranges, same, wildcard, FieldKind, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Issue {
    pub severity: Severity,
    /// Permanent code: the app picks the text in the user's language by it
    pub code: &'static str,
    /// Index in `changes` (from 0); `None` — about the patch as a whole
    pub at: Option<usize>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Report {
    pub errors: Vec<Issue>,
    pub warnings: Vec<Issue>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    fn error(&mut self, code: &'static str, at: Option<usize>, message: String) {
        self.errors.push(Issue { severity: Severity::Error, code, at, message });
    }

    fn warn(&mut self, code: &'static str, at: Option<usize>, message: String) {
        self.warnings.push(Issue { severity: Severity::Warning, code, at, message });
    }
}

/// The table the patch is checked against: from the kit or from the server
#[derive(Clone, Debug, Default)]
pub struct TableData {
    pub editable: Vec<String>,
    pub values: Vec<Value>,
    /// The game original, if any: warning ranges are computed from it
    pub orig: Option<Vec<Value>>,
    /// generic path -> field type
    pub kinds: BTreeMap<String, FieldKind>,
    /// Rows of the source JSON: a fallback way to tell whether a field is an integer when there are no types
    pub raw_rows: Option<Vec<Json>>,
    /// Value limits from `kit.json`: the first match by path
    pub limits: Vec<Limit>,
    /// A path for which, among the rows with one `key`, a value above zero must remain (item weight in a pool)
    pub positive_in_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Limit {
    pub pattern: String,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl Limit {
    fn text(&self) -> String {
        match (self.min, self.max) {
            (Some(a), Some(b)) => format!("{}…{}", a, b),
            (Some(a), None) => format!("from {}", a),
            (None, Some(b)) => format!("up to {}", b),
            (None, None) => String::new(),
        }
    }
}

impl TableData {
    /// `limits = { "chance" = [0, 100], "w" = [0] }` and `positive_in_key` from the table in `kit.json`
    pub fn with_rules(mut self, cfg: &Json) -> TableData {
        self.limits = cfg["limits"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(k, v)| Limit { pattern: k.clone(), min: v[0].as_f64(), max: v[1].as_f64() })
            .collect();
        self.positive_in_key = cfg["positive_in_key"].as_str().map(str::to_string);
        self
    }
}

/// What to check the patch header against. The server sets neither: it has no kit version, and it takes the season from the patch.
#[derive(Clone, Copy, Debug, Default)]
pub struct Expect<'a> {
    pub version: Option<&'a str>,
    pub season: Option<Option<&'a str>>,
}

/// Value of a raw row at a long-form path
fn raw_at<'a>(row: &'a Json, path: &str) -> Option<&'a Json> {
    let mut cur = row;
    for seg in path.split('.') {
        let (name, mut rest) = match seg.find('[') {
            Some(p) => (&seg[..p], &seg[p..]),
            None => (seg, ""),
        };
        cur = cur.get(name)?;
        while !rest.is_empty() {
            let end = rest.find(']')?;
            let idx = &rest[1..end];
            cur = match cur {
                Json::Array(a) => a.get(idx.parse::<usize>().ok()?)?,
                Json::Object(_) => cur.get(idx)?,
                _ => return None,
            };
            rest = &rest[end + 1..];
        }
    }
    (!cur.is_null()).then_some(cur)
}

struct Indexed<'a> {
    by_row: HashMap<usize, Vec<&'a Value>>,
    ranges: BTreeMap<String, (f64, f64)>,
}

impl<'a> Indexed<'a> {
    fn new(t: &'a TableData) -> Self {
        let mut by_row: HashMap<usize, Vec<&Value>> = HashMap::new();
        for v in &t.values {
            by_row.entry(v.row).or_default().push(v);
        }
        Indexed { by_row, ranges: ranges(t.orig.as_deref().unwrap_or(&t.values)) }
    }

    fn value(&self, row: usize, path: &str) -> &str {
        self.by_row.get(&row).and_then(|vs| vs.iter().find(|v| v.path == path)).map_or("", |v| v.value.as_str())
    }
}

/// Roll step for a min/max pair: the nearest `unit` up the path (`…offset.unit`, `data.unit`)
fn unit_paths(base: &str) -> Vec<String> {
    let mut out = vec![format!("{}unit", base)];
    let mut prefix = base.trim_end_matches('.');
    while let Some(dot) = prefix.rfind('.') {
        prefix = &prefix[..dot];
        out.push(format!("{}.unit", prefix));
    }
    if !base.is_empty() {
        out.push("unit".to_string());
    }
    out
}

pub fn check(patch: &Patch, tables: &BTreeMap<String, TableData>, expect: &Expect) -> Report {
    let mut r = Report::default();
    if patch.format != PATCH_FORMAT {
        r.error("format", None, format!("format must be {}", PATCH_FORMAT));
    }
    if let Some(version) = expect.version {
        if patch.base != version {
            r.warn("base_differs", None, format!("the patch is from kit {}, the kit is {}", patch.base, version));
        }
    }
    if let Some(season) = expect.season {
        match &patch.season {
            None => r.warn("season_missing", None, format!("no season — the server will apply the patch to the active season (the kit has: {:?})", season)),
            Some(s) if s.as_deref() != season => r.error("season_mismatch", None, format!("season {:?}, but the kit is for {:?} — a patch for another season", s, season)),
            _ => {}
        }
    }
    if patch.author.trim().is_empty() {
        r.warn("author_missing", None, "no author".into());
    }

    let mut indexed: HashMap<&str, Indexed> = HashMap::new();
    let mut seen = BTreeSet::new();
    for (i, c) in patch.changes.iter().enumerate() {
        let row_text = c.row.map_or("?".to_string(), |x| x.to_string());
        let at = format!("#{} {} row {} {}", i + 1, c.table, row_text, c.path);
        if !seen.insert((c.table.as_str(), c.row, c.path.as_str())) {
            r.error("duplicate_change", Some(i), format!("{}: this value is changed in the patch a second time", at));
            continue;
        }
        let Some(t) = tables.get(&c.table) else {
            r.error("table_unknown", Some(i), format!("{}: the table is not in the kit", at));
            continue;
        };
        if !editable(&t.editable, &c.path) {
            r.error("field_locked", Some(i), format!("{}: this field cannot be changed (allowed: {})", at, t.editable.join(", ")));
            continue;
        }
        let idx = indexed.entry(c.table.as_str()).or_insert_with(|| Indexed::new(t));
        let Some(row) = c.row.filter(|x| idx.by_row.contains_key(x)) else {
            r.error("row_missing", Some(i), format!("{}: no row {}", at, row_text));
            continue;
        };
        let row_key = idx.by_row[&row][0].key.as_str();
        if !c.key.trim().is_empty() && !same(row_key, &c.key) {
            r.error("key_mismatch", Some(i), format!("{}: key {}, but the row has {}", at, c.key, row_key));
            continue;
        }
        let cur = idx.value(row, &c.path);
        if !same(cur, &c.old) {
            r.error("old_mismatch", Some(i), format!("{}: old {}, but the kit has {}", at, or_empty(&c.old), or_empty(cur)));
            continue;
        }
        if c.new.trim().is_empty() {
            r.error("new_empty", Some(i), format!("{}: empty new", at));
            continue;
        }
        let (o, n) = (as_number(cur), as_number(&c.new));
        let kind = t.kinds.get(&generic(&c.path)).copied();
        let number_expected = match kind {
            Some(k) => k.is_number(),
            None => o.is_some() || cur.is_empty(),
        };
        if number_expected {
            if n.is_none() {
                r.error("new_not_number", Some(i), format!("{}: new {} is not a number", at, c.new));
                continue;
            }
            let integer = match kind {
                Some(k) => k.is_integer(),
                None => t
                    .raw_rows
                    .as_ref()
                    .and_then(|rows| rows.get(row))
                    .and_then(|x| raw_at(x, &c.path))
                    .is_some_and(|x| x.is_i64() || x.is_u64()),
            };
            if integer && !is_int_text(&c.new) {
                r.error("new_not_integer", Some(i), format!("{}: the field is an integer, but new is {}", at, c.new));
                continue;
            }
            if let Some((lo, hi)) = kind.and_then(FieldKind::int_range) {
                if c.new.trim().parse::<i128>().map_or(true, |x| x < lo || x > hi) {
                    r.error("new_out_of_range", Some(i), format!("{}: new {} is outside the type bounds {}…{}", at, c.new, lo, hi));
                    continue;
                }
            }
        }
        if let (Some(limit), Some(x)) = (t.limits.iter().find(|l| wildcard(&l.pattern, &c.path)), n) {
            if limit.min.is_some_and(|m| x < m) || limit.max.is_some_and(|m| x > m) {
                r.error("new_out_of_limits", Some(i), format!("{}: new {} — allowed {}", at, c.new, limit.text()));
                continue;
            }
        }
        if let (Some(&range), Some(x)) = (idx.ranges.get(&generic(&c.path)), n) {
            let span = range.0.abs().max(range.1.abs());
            if span > 0.0 && x.abs() > 3.0 * span {
                r.warn("range_far", Some(i), format!("{}: {} — far outside the original range {}", at, c.new, fmt_range(range)));
            }
        }
        if let (Some(old), Some(x)) = (o.filter(|v| *v != 0.0), n) {
            let ratio = (x / old).abs();
            if !(1.0 / 3.0..=3.0).contains(&ratio) {
                r.warn("ratio_big", Some(i), format!("{}: {} -> {} — more than 3 times", at, cur, c.new));
            }
            if (x < 0.0) != (old < 0.0) {
                r.warn("sign_flip", Some(i), format!("{}: sign change {} -> {}", at, cur, c.new));
            }
        }
    }

    // Min/max pairs: the result after the patch, taking other edits of the same patch into account
    let mut planned: BTreeMap<(&str, usize, &str), (&str, usize)> = BTreeMap::new();
    for (i, c) in patch.changes.iter().enumerate() {
        if let Some(row) = c.row {
            planned.insert((c.table.as_str(), row, c.path.as_str()), (c.new.as_str(), i));
        }
    }
    // Rows with one key: after the patch at least one has a value above zero
    let mut seen_keys = BTreeSet::new();
    for (&(table, row, path), &(_, i)) in &planned {
        let (Some(t), Some(idx)) = (tables.get(table), indexed.get(table)) else { continue };
        if t.positive_in_key.as_deref() != Some(path) {
            continue;
        }
        let Some(key) = idx.by_row.get(&row).map(|vs| vs[0].key.as_str()) else { continue };
        if !seen_keys.insert((table, key)) {
            continue;
        }
        let alive = idx.by_row.iter().filter(|(_, vs)| vs[0].key == key).any(|(&other, _)| {
            let value = planned.get(&(table, other, path)).map_or_else(|| idx.value(other, path), |x| x.0);
            as_number(value).is_some_and(|x| x > 0.0)
        });
        if !alive {
            r.error("key_all_zero", Some(i), format!("{} {}: {} is zero in all rows, but at least one must be above zero", table, key, path));
        }
    }

    let mut seen_pairs = BTreeSet::new();
    for (&(table, row, path), &(_, i)) in &planned {
        let Some(idx) = indexed.get(table) else { continue };
        let base = match path.strip_suffix("min_value").or_else(|| path.strip_suffix("max_value")) {
            Some(b) => b,
            None => continue,
        };
        if !seen_pairs.insert((table, row, base)) {
            continue;
        }
        let after = |p: &str| -> Option<f64> {
            let planned_new = planned.get(&(table, row, p)).map(|x| x.0);
            as_number(planned_new.unwrap_or_else(|| idx.value(row, p)))
        };
        let (min_path, max_path) = (format!("{}min_value", base), format!("{}max_value", base));
        let (Some(lo), Some(hi)) = (after(&min_path), after(&max_path)) else { continue };
        if lo > hi {
            r.error("min_gt_max", Some(i), format!("{} row {} {}: min {} is greater than max {}", table, row, base.trim_end_matches('.'), lo, hi));
            continue;
        }
        let Some(unit) = unit_paths(base).iter().find_map(|p| as_number(idx.value(row, p))) else { continue };
        if unit <= 0.0 {
            r.warn("unit_invalid", Some(i), format!("{} row {} {}: step {} is not above zero", table, row, base.trim_end_matches('.'), unit));
        } else {
            let steps = (hi - lo) / unit;
            if (steps - steps.round()).abs() > 1e-6 {
            r.warn("unit_not_multiple", Some(i), format!("{} row {} {}: {}…{} is not divisible by step {}, a roll may miss the bounds", table, row, base.trim_end_matches('.'), lo, hi, unit));
            }
        }
    }
    r
}

fn or_empty(s: &str) -> &str {
    if s.is_empty() {
        "(empty)"
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_lookup_goes_up() {
        assert_eq!(unit_paths("data.affix_data[0].rarity_range[1]."), ["data.affix_data[0].rarity_range[1].unit", "data.affix_data[0].unit", "data.unit", "unit"]);
        assert_eq!(unit_paths("offset."), ["offset.unit", "unit"]);
        assert_eq!(unit_paths(""), ["unit"]);
    }

    #[test]
    fn raw_paths() {
        let row = serde_json::json!({"a": {"b": [{"c": 1}]}, "m": {"3": {"v": 2.5}}});
        assert_eq!(raw_at(&row, "a.b[0].c"), Some(&serde_json::json!(1)));
        assert_eq!(raw_at(&row, "m[3].v"), Some(&serde_json::json!(2.5)));
        assert_eq!(raw_at(&row, "a.b[1].c"), None);
        assert_eq!(raw_at(&row, "x"), None);
    }
}
