//! The kit in memory for the interface: rows, columns, labels, comparison with the original, filter and sort.
//! The same decisions as in the book (`balance_kit/tools/build_excel.py`), so that the book and the app show the same thing.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value as Json};

use crate::check::{Severity, TableData};
use crate::edit::Session;
use crate::kit::{read_values, Kit};
use crate::patch::Patch;
use crate::values::{as_number, editable, fmt_range, generic, ranges, same, Value};

/// Fallback English names of shared fields: the kit's `fields_en` holds only attributes
const FIELDS_EN: &[(&str, &str)] = &[
    ("fixed_value", "value"),
    ("min_value", "min"),
    ("max_value", "max"),
    ("cost_price", "price"),
    ("origin_price", "list price"),
    ("count", "count"),
    ("durability", "durability"),
    ("rarity_range", "rarity"),
    ("role_level", "Level"),
    ("cost_items", "cost"),
    ("reward_items", "gives"),
];

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Query {
    pub view: String,
    pub search: String,
    pub only_diff: bool,
    pub only_mine: bool,
    pub all_fields: bool,
    pub kind: String,
    pub rarity: String,
    pub sort: String,
    pub desc: bool,
    pub en: bool,
}

impl Query {
    pub fn from_json(q: &Json) -> Query {
        let text = |k: &str| q[k].as_str().unwrap_or_default().to_string();
        Query {
            view: text("view"),
            search: text("search"),
            only_diff: q["only_diff"].as_bool().unwrap_or(false),
            only_mine: q["only_mine"].as_bool().unwrap_or(false),
            all_fields: q["all_fields"].as_bool().unwrap_or(false),
            kind: text("kind"),
            rarity: text("rarity"),
            sort: text("sort"),
            desc: q["desc"].as_bool().unwrap_or(false),
            en: q["lang"].as_str() == Some("en"),
        }
    }
}

pub(crate) struct PathInfo {
    pub(crate) path: String,
    label: [String; 2],
    pub(crate) unit: String,
    pub(crate) editable: bool,
    pub(crate) context: bool,
    /// A cell value, not a service field: only these are shown as percentages
    pub(crate) value_like: bool,
    range: Option<(f64, f64)>,
}

pub(crate) struct RowInfo {
    pub(crate) row: usize,
    pub(crate) key: String,
    /// Index among the rows with the same `key`: together with the hash of non-editable values identifies the row across kit versions
    pub(crate) n: usize,
    pub(crate) title: [String; 2],
    pub(crate) icon: Option<String>,
    /// Value of the icon field: in tables about items it is the item id
    pub(crate) icon_key: String,
    pub(crate) unit: String,
    service: bool,
    new_row: bool,
    pub(crate) kind: String,
    pub(crate) rarity: String,
    /// (path index, value index), ascending by path index
    pub(crate) cells: Vec<(u32, u32)>,
    /// Text for search, lowercase: names in both languages and the key
    pub(crate) haystack: String,
}

pub(crate) struct TableView {
    pub(crate) name: String,
    pub(crate) title: String,
    group: String,
    /// title, about, where in English; empty — the kit has no translation
    en: [String; 3],
    about: String,
    place: String,
    layout: String,
    has_orig: bool,
    source: String,
    editable_values: usize,
    changed: usize,
    new_rows: usize,
    removed_rows: usize,
    pub(crate) values: Vec<Value>,
    names_en: HashMap<(usize, String), String>,
    pub(crate) paths: Vec<PathInfo>,
    pub(crate) path_ids: HashMap<String, u32>,
    pub(crate) rows: Vec<RowInfo>,
    /// (row, path) -> value in the original; `None` — the row is not in the original
    pub(crate) diff: HashMap<(usize, u32), Option<String>>,
    pub(crate) kinds: Vec<String>,
    pub(crate) rarities: Vec<String>,
}

pub struct KitView {
    pub kit: Kit,
    pub(crate) tables: Vec<TableView>,
    icons: HashMap<String, String>,
    last: Mutex<Option<(String, Query, u64, Arc<Selection>)>>,
    pub(crate) session: Mutex<Session>,
    /// Data for checking edits: read on the first edit of the table
    checks: Arc<Mutex<BTreeMap<String, TableData>>>,
    /// Loot template for the "Loot" screen: old kits do not have it
    pub(crate) loot: Option<crate::loot_screen::LootData>,
}

pub(crate) struct Selection {
    /// Wide form — indices in `rows`; long form — indices in `values`
    pub(crate) ids: Vec<u32>,
    /// Wide form: path indices in column order
    columns: Vec<u32>,
    columns_matched: bool,
    pub(crate) long: bool,
}

fn lang_of(en: bool) -> usize {
    usize::from(en)
}

/// A readable column name: known fields + key label (rarity, attribute…) + indices in brackets
fn field_label(fields: &Map<String, Json>, en: bool, path: &str, name: &str) -> String {
    let mut bare = String::new();
    let mut idx = Vec::new();
    let mut rest = path;
    while let Some(open) = rest.find('[') {
        bare.push_str(&rest[..open]);
        let Some(close) = rest[open..].find(']').map(|c| open + c) else { break };
        idx.push(&rest[open + 1..close]);
        rest = &rest[close + 1..];
    }
    bare.push_str(rest);
    if !name.is_empty() {
        idx.pop();
    }
    // `offset` is a min/max wrapper, not a field: the attribute above it matters
    let parts: Vec<&str> = bare.split('.').filter(|p| !p.is_empty() && !matches!(*p, "offset" | "affix_data" | "data" | "product" | "cost_items" | "cost_options") && (name.is_empty() || *p != "rarity_range")).collect();
    let word = |p: &str| -> String {
        if let Some(t) = fields.get(p).and_then(|x| x.as_str()) {
            return t.to_string();
        }
        if en {
            if let Some((_, t)) = FIELDS_EN.iter().find(|(k, _)| *k == p) {
                return t.to_string();
            }
        }
        p.to_string()
    };
    let mut words: Vec<String> = parts.iter().rev().take(2).rev().map(|p| word(p)).collect();
    if words.len() > 1 && (words.last().is_some_and(|w| w == "значение" || w == "value")) {
        words.pop();
    }
    words.dedup();
    let mut label = words.join(" ");
    if !name.is_empty() {
        label.push_str(" · ");
        label.push_str(name);
    }
    if !idx.is_empty() {
        label.push_str(&format!(" ({})", idx.join(",")));
    }
    label
}

fn unit_of(units: &Map<String, Json>, path: &str) -> String {
    let bare: String = {
        let mut out = String::new();
        let mut skip = false;
        for c in path.chars() {
            match c {
                '[' => skip = true,
                ']' => skip = false,
                c if !skip => out.push(c),
                _ => {}
            }
        }
        out
    };
    bare.split('.').find_map(|s| units.get(s).and_then(|x| x.as_str())).unwrap_or_default().to_string()
}

/// row -> (key, index among the rows with the same key): inserting a row does not shift the comparison with the original
fn row_ids(values: &[Value]) -> BTreeMap<usize, (String, usize)> {
    let (mut ids, mut count): (BTreeMap<usize, (String, usize)>, HashMap<&str, usize>) = (BTreeMap::new(), HashMap::new());
    for v in values {
        if ids.contains_key(&v.row) {
            continue;
        }
        let n = count.entry(v.key.as_str()).or_insert(0);
        ids.insert(v.row, (v.key.clone(), *n));
        *n += 1;
    }
    ids
}

impl RowInfo {
    pub(crate) fn value_of<'a>(&self, t: &'a TableView, path: u32) -> Option<&'a str> {
        self.cells.binary_search_by_key(&path, |x| x.0).ok().map(|i| t.values[self.cells[i].1 as usize].value.as_str())
    }
}

impl TableView {
    fn load(kit: &Kit, cfg: &Json, icons: &HashMap<String, String>) -> Result<TableView, String> {
        let name = cfg["name"].as_str().unwrap_or_default().to_string();
        let data = kit.root.join("data");
        let values = read_values(&data.join(format!("{}.csv", name)))?;
        let orig_path = data.join(format!("{}.orig.csv", name));
        let orig = orig_path.exists().then(|| read_values(&orig_path)).transpose()?;
        let en_path = data.join(format!("{}.en.csv", name));
        let en = en_path.exists().then(|| read_values(&en_path)).transpose()?.unwrap_or_default();
        let mut titles_en: HashMap<usize, String> = HashMap::new();
        let mut names_en: HashMap<(usize, String), String> = HashMap::new();
        for v in en {
            titles_en.entry(v.row).or_insert(v.title);
            if !v.path.is_empty() && !v.name.is_empty() {
                names_en.insert((v.row, v.path), v.name);
            }
        }

        let patterns: Vec<String> = cfg["editable"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect();
        let context: Vec<&str> = cfg["context"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect();
        let empty = Map::new();
        let units = kit.config["attr_units"].as_object().unwrap_or(&empty);
        let fields_ru = kit.config["fields"].as_object().unwrap_or(&empty);
        let fields_en = kit.config["fields_en"].as_object().unwrap_or(&empty);
        let attr_ids = kit.config["attr_ids"].as_object().unwrap_or(&empty);
        let range_map = ranges(orig.as_deref().unwrap_or(&values));

        // Column order — as they appear; the key label goes into the header only if it is the same for the whole column
        let mut path_ids: HashMap<String, u32> = HashMap::new();
        let mut order: Vec<String> = Vec::new();
        let mut names_ru: Vec<BTreeSet<&str>> = Vec::new();
        let mut names_en_col: Vec<BTreeSet<&str>> = Vec::new();
        for v in values.iter().filter(|v| !v.path.is_empty()) {
            let id = *path_ids.entry(v.path.clone()).or_insert_with(|| {
                order.push(v.path.clone());
                names_ru.push(BTreeSet::new());
                names_en_col.push(BTreeSet::new());
                order.len() as u32 - 1
            }) as usize;
            names_ru[id].insert(v.name.as_str());
            if let Some(n) = names_en.get(&(v.row, v.path.clone())) {
                names_en_col[id].insert(n.as_str());
            }
        }

        // Comparison with the original
        let mut diff_text: HashMap<(usize, String), Option<String>> = HashMap::new();
        let (mut new_rows, mut removed_rows) = (BTreeSet::new(), 0usize);
        if let Some(orig) = &orig {
            let (ids, oids) = (row_ids(&values), row_ids(orig));
            let orow: HashMap<&(String, usize), usize> = oids.iter().map(|(r, id)| (id, *r)).collect();
            let ov: HashMap<(&(String, usize), &str), &str> =
                orig.iter().filter(|v| !v.path.is_empty()).map(|v| ((&oids[&v.row], v.path.as_str()), v.value.as_str())).collect();
            let mut cur: BTreeSet<(usize, &str)> = BTreeSet::new();
            for v in values.iter().filter(|v| !v.path.is_empty()) {
                cur.insert((v.row, v.path.as_str()));
                let id = &ids[&v.row];
                if !orow.contains_key(id) {
                    diff_text.insert((v.row, v.path.clone()), None);
                    new_rows.insert(v.row);
                } else {
                    let o = ov.get(&(id, v.path.as_str())).copied().unwrap_or("");
                    if !same(o, &v.value) {
                        diff_text.insert((v.row, v.path.clone()), Some(o.to_string()));
                    }
                }
            }
            // Fields that became the default value: they are absent from the current table
            let by_id: HashMap<&(String, usize), usize> = ids.iter().map(|(r, id)| (id, *r)).collect();
            for ((id, path), o) in &ov {
                if let Some(&row) = by_id.get(id) {
                    if !cur.contains(&(row, *path)) && !o.is_empty() {
                        diff_text.insert((row, path.to_string()), Some(o.to_string()));
                    }
                }
            }
            removed_rows = orow.keys().filter(|id| !by_id.contains_key(**id)).count();
        }
        // Paths only from the original also become columns: this difference must be shown
        let extra: BTreeSet<String> = diff_text.keys().filter(|(_, p)| !path_ids.contains_key(p)).map(|(_, p)| p.clone()).collect();
        for p in extra {
            path_ids.insert(p.clone(), order.len() as u32);
            order.push(p);
            names_ru.push(BTreeSet::new());
            names_en_col.push(BTreeSet::new());
        }

        let affix_kinds: HashMap<String, String> = values
            .iter()
            .filter(|v| v.path.ends_with("].affix_type"))
            .filter_map(|v| Some((v.path.split_once("affix_data[")?.1.split_once(']')?.0.to_string(), v.value.clone())))
            .collect();
        let paths: Vec<PathInfo> = order
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let single = |s: &BTreeSet<&str>| -> String {
                    if s.len() == 1 {
                        s.iter().next().unwrap_or(&"").to_string()
                    } else {
                        String::new()
                    }
                };
                let last = p.rsplit('.').next().unwrap_or(p);
                // affix_data[i]: the affix kind is the same in the whole column, the index is replaced by its name
                let kinds = |en: bool| -> Option<(String, &'static str)> {
                    let i = p.split_once("affix_data[")?.1.split_once(']')?.0;
                    let text = affix_kinds.get(i)?;
                    let word = match (text.as_str(), en) {
                        ("EN_BATTLE_AFFIX_TYPE_MAJOR", false) => "основной",
                        ("EN_BATTLE_AFFIX_TYPE_MINOR", false) => "дополн.",
                        ("EN_BATTLE_AFFIX_TYPE_MAJOR", true) => "major",
                        ("EN_BATTLE_AFFIX_TYPE_MINOR", true) => "minor",
                        _ => return None,
                    };
                    Some((format!(" ({})", i), word))
                };
                let named = |mut label: String, en: bool| -> String {
                    if let Some((idx, word)) = kinds(en) {
                        if label.ends_with(&idx) {
                            label.truncate(label.len() - idx.len());
                            label.push_str(" · ");
                            label.push_str(word);
                        }
                    }
                    label
                };
                PathInfo {
                    path: p.clone(),
                    label: [named(field_label(fields_ru, false, p, &single(&names_ru[i])), false), named(field_label(fields_en, true, p, &single(&names_en_col[i])), true)],
                    unit: unit_of(units, p),
                    editable: editable(&patterns, p),
                    context: context.contains(&p.as_str()),
                    value_like: matches!(last, "min_value" | "max_value" | "fixed_value"),
                    range: range_map.get(&generic(p)).copied(),
                }
            })
            .collect();

        let diff: HashMap<(usize, u32), Option<String>> = diff_text.into_iter().map(|((r, p), o)| ((r, path_ids[&p]), o)).collect();
        let editable_values = values.iter().filter(|v| !v.path.is_empty() && editable(&patterns, &v.path)).count();
        let changed = diff.values().filter(|o| o.is_some()).count();

        // Rows
        let icon_field = cfg["icon"].as_str().unwrap_or_default();
        let is_class = icon_field.rsplit('.').next() == Some("role_type_id");
        let catalog = kit.catalog.as_ref();
        let mut by_row: BTreeMap<usize, Vec<u32>> = BTreeMap::new();
        for (i, v) in values.iter().enumerate() {
            by_row.entry(v.row).or_default().push(i as u32);
        }
        let mut rows = Vec::with_capacity(by_row.len());
        let (mut kinds, mut rarities) = (BTreeSet::new(), BTreeSet::new());
        let mut key_count: HashMap<String, usize> = HashMap::new();
        for (row, ids) in by_row {
            let first = &values[ids[0] as usize];
            let n = key_count.entry(first.key.clone()).or_insert(0);
            let row_n = *n;
            *n += 1;
            let mut cells: Vec<(u32, u32)> = ids.iter().filter(|i| !values[**i as usize].path.is_empty()).map(|i| (path_ids[&values[*i as usize].path], *i)).collect();
            cells.sort_unstable();
            let at = |p: &str| ids.iter().map(|i| &values[*i as usize]).find(|v| v.path == p);
            let icon_value = (!icon_field.is_empty()).then(|| at(icon_field)).flatten();
            let icon_key = icon_value.map(|v| v.value.as_str()).unwrap_or_default();
            let class = is_class.then(|| catalog.map(|c| &c["classes"][icon_key])).flatten();
            let item = (!is_class).then(|| catalog.map(|c| &c["items"][icon_key])).flatten();
            let text = |x: Option<&Json>, k: &str| x.and_then(|j| j[k].as_str()).unwrap_or_default().to_string();

            // Name: a merchant product has none of its own, the name of what it gives is taken
            let mut ru = text(class, "ru");
            if ru.is_empty() {
                ru = first.title.clone();
            }
            if ru.is_empty() {
                ru = icon_value.map(|v| v.name.clone()).unwrap_or_default();
            }
            // A product without an item name: its own product name or the name of the skin it gives.
            // A map row: the name from the kit. A rarity row: the rarity name
            let product = catalog.map(|c| {
                if first.path == "map_id" {
                    &c["maps"][first.key.as_str()]
                } else if let Some(r) = first.key.strip_prefix("EN_ITEM_RARITY_") {
                    &c["rarities"][r]
                } else {
                    &c["products"][first.key.as_str()]
                }
            });
            if ru.is_empty() {
                ru = text(product, "ru");
            }
            let mut en = text(class, "en");
            if en.is_empty() {
                en = titles_en.get(&row).cloned().unwrap_or_default();
            }
            if en.is_empty() {
                en = names_en.get(&(row, icon_field.to_string())).cloned().unwrap_or_default();
            }
            if en.is_empty() {
                en = text(product, "en");
            }
            if en.is_empty() {
                en = ru.clone();
            }
            let row_attr = at("attribute_type").map(|v| v.value.as_str()).unwrap_or_default();
            let unit = attr_ids.get(row_attr).and_then(|n| n.as_str()).and_then(|n| units.get(n)).and_then(|u| u.as_str()).unwrap_or_default().to_string();
            let (kind, rarity) = (text(item, "kind"), text(item, "rarity"));
            if !kind.is_empty() {
                kinds.insert(kind.clone());
            }
            if !rarity.is_empty() {
                rarities.insert(rarity.clone());
            }
            let haystack = format!("{} {} {}", ru, en, first.key).to_lowercase();
            rows.push(RowInfo {
                row,
                key: first.key.clone(),
                n: row_n,
                title: [ru, en],
                icon: icons.get(icon_key).cloned(),
                icon_key: icon_key.to_string(),
                unit,
                service: class.is_some_and(|c| c["hidden"].as_bool() == Some(true)),
                new_row: new_rows.contains(&row),
                kind,
                rarity,
                cells,
                haystack,
            });
        }

        let manifest_table = &kit.manifest["tables"][&name];
        Ok(TableView {
            title: cfg["title"].as_str().unwrap_or(&name).to_string(),
            group: cfg["group"].as_str().unwrap_or_default().to_string(),
            en: ["title_en", "about_en", "where_en"].map(|k| cfg[k].as_str().unwrap_or_default().to_string()),
            about: cfg["about"].as_str().unwrap_or_default().to_string(),
            place: cfg["where"].as_str().unwrap_or_default().to_string(),
            layout: cfg["layout"].as_str().unwrap_or("wide").to_string(),
            has_orig: orig.is_some(),
            source: manifest_table["source"].as_str().unwrap_or_default().to_string(),
            editable_values,
            changed,
            new_rows: new_rows.len(),
            removed_rows,
            values,
            names_en,
            paths,
            path_ids,
            rows,
            diff,
            kinds: kinds.into_iter().collect(),
            rarities: rarities.into_iter().collect(),
            name,
        })
    }

    pub(crate) fn title_of(&self, en: bool) -> &str {
        if en && !self.en[0].is_empty() {
            &self.en[0]
        } else {
            &self.title
        }
    }

    pub(crate) fn row_pos(&self, row: usize) -> Option<usize> {
        self.rows.binary_search_by_key(&row, |r| r.row).ok()
    }

    /// v — what to show (with the designer's edit); k — the kit value, only if there is an edit
    pub(crate) fn cell_json(&self, ti: usize, row: usize, path: u32, value: Option<&str>, s: &Session) -> Json {
        let diff = self.diff.get(&(row, path));
        let edit = s.edits.get(&(ti, row, path));
        if value.is_none() && diff.is_none() && edit.is_none() {
            return Json::Null;
        }
        let mut c = json!({ "v": edit.map(String::as_str).or(value).unwrap_or_default() });
        match diff {
            Some(Some(o)) => c["o"] = json!(o),
            Some(None) => c["n"] = json!(true),
            None => {}
        }
        if edit.is_some() {
            c["k"] = json!(value.unwrap_or_default());
        }
        if let Some(i) = s.issues.get(&(ti, row, path)) {
            c[if i.severity == Severity::Error { "e" } else { "w" }] = json!(i.code);
        }
        c
    }

    fn row_head(&self, r: &RowInfo, en: bool) -> Json {
        let mut j = json!({ "row": r.row, "key": r.key, "title": r.title[lang_of(en)] });
        if let Some(i) = &r.icon {
            j["icon"] = json!(i);
        }
        if !r.unit.is_empty() {
            j["unit"] = json!(r.unit);
        }
        if r.service {
            j["service"] = json!(true);
        }
        if r.new_row {
            j["new"] = json!(true);
        }
        j
    }

    pub(crate) fn column_json(&self, id: u32, en: bool) -> Json {
        let p = &self.paths[id as usize];
        let mut c = json!({ "id": id, "path": p.path, "label": p.label[lang_of(en)], "editable": p.editable, "value_like": p.value_like });
        if !p.unit.is_empty() {
            c["unit"] = json!(p.unit);
        }
        if let Some(r) = p.range {
            c["range"] = json!(fmt_range(r));
        }
        c
    }

    pub(crate) fn select(&self, ti: usize, q: &Query, s: &Session) -> Selection {
        let mine = |row: usize, pid: u32| s.edits.contains_key(&(ti, row, pid));
        let mine_rows: BTreeSet<usize> = s.edits.keys().filter(|k| k.0 == ti).map(|k| k.1).collect();
        let long = match q.view.as_str() {
            "long" => true,
            "wide" => false,
            _ => self.layout == "long",
        };
        let words: Vec<String> = q.search.to_lowercase().split_whitespace().map(str::to_string).collect();
        let row_passes = |r: &RowInfo| (q.kind.is_empty() || r.kind == q.kind) && (q.rarity.is_empty() || r.rarity == q.rarity);
        let has_editable = self.paths.iter().any(|p| p.editable);
        let show_path = |p: &PathInfo| q.all_fields || !has_editable || p.editable || p.context;
        let row_of: HashMap<usize, &RowInfo> = self.rows.iter().map(|r| (r.row, r)).collect();

        if long {
            let mut ids: Vec<u32> = Vec::new();
            for (i, v) in self.values.iter().enumerate() {
                if v.path.is_empty() {
                    continue;
                }
                let pid = self.path_ids[&v.path];
                let p = &self.paths[pid as usize];
                if !show_path(p) {
                    continue;
                }
                let r = row_of[&v.row];
                if !row_passes(r) || (q.only_diff && !self.diff.contains_key(&(v.row, pid))) || (q.only_mine && !mine(v.row, pid)) {
                    continue;
                }
                if !words.is_empty() {
                    let hay = format!("{} {} {} {} {}", r.haystack, p.label[0], p.label[1], p.path, v.name).to_lowercase();
                    if !words.iter().all(|w| hay.contains(w)) {
                        continue;
                    }
                }
                ids.push(i as u32);
            }
            self.sort_long(&mut ids, q, &row_of);
            return Selection { ids, columns: Vec::new(), columns_matched: false, long };
        }

        let mut columns: Vec<u32> = self.paths.iter().enumerate().filter(|(_, p)| p.context && show_path(p)).map(|(i, _)| i as u32).collect();
        columns.extend(self.paths.iter().enumerate().filter(|(_, p)| !p.context && show_path(p)).map(|(i, _)| i as u32));
        let diff_rows: BTreeSet<usize> = if q.only_diff { self.diff.keys().map(|(r, _)| *r).collect() } else { BTreeSet::new() };
        let base: Vec<u32> = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, r)| row_passes(r) && (!q.only_diff || diff_rows.contains(&r.row)) && (!q.only_mine || mine_rows.contains(&r.row)))
            .map(|(i, _)| i as u32)
            .collect();
        let mut ids: Vec<u32> = base.iter().copied().filter(|i| words.iter().all(|w| self.rows[*i as usize].haystack.contains(w))).collect();
        let mut columns_matched = false;
        if ids.is_empty() && !words.is_empty() {
            // No row matched: maybe a field was searched for, then show its columns for all rows
            let by_col: Vec<u32> = columns
                .iter()
                .copied()
                .filter(|c| {
                    let p = &self.paths[*c as usize];
                    let hay = format!("{} {} {}", p.label[0], p.label[1], p.path).to_lowercase();
                    words.iter().all(|w| hay.contains(w))
                })
                .collect();
            if !by_col.is_empty() {
                columns = by_col;
                ids = base;
                columns_matched = true;
            }
        }
        self.sort_wide(&mut ids, q);
        Selection { ids, columns, columns_matched, long }
    }

    fn sort_wide(&self, ids: &mut [u32], q: &Query) {
        if q.sort.is_empty() {
            return;
        }
        let lang = lang_of(q.en);
        let column: Option<u32> = q.sort.parse().ok();
        let value = |r: &RowInfo, c: u32| -> &str {
            r.cells.binary_search_by_key(&c, |x| x.0).ok().map_or("", |i| self.values[r.cells[i].1 as usize].value.as_str())
        };
        ids.sort_by(|a, b| {
            let (ra, rb) = (&self.rows[*a as usize], &self.rows[*b as usize]);
            let (x, y): (&str, &str) = match (q.sort.as_str(), column) {
                ("name", _) => (&ra.title[lang], &rb.title[lang]),
                ("key", _) => (&ra.key, &rb.key),
                (_, Some(c)) => (value(ra, c), value(rb, c)),
                _ => ("", ""),
            };
            compare_cells(x, y, q.desc)
        });
    }

    fn sort_long(&self, ids: &mut [u32], q: &Query, row_of: &HashMap<usize, &RowInfo>) {
        if q.sort.is_empty() {
            return;
        }
        let lang = lang_of(q.en);
        ids.sort_by(|a, b| {
            let (va, vb) = (&self.values[*a as usize], &self.values[*b as usize]);
            let (x, y): (&str, &str) = match q.sort.as_str() {
                "name" => (&row_of[&va.row].title[lang], &row_of[&vb.row].title[lang]),
                "key" => (&va.key, &vb.key),
                "label" => (&self.paths[self.path_ids[&va.path] as usize].label[lang], &self.paths[self.path_ids[&vb.path] as usize].label[lang]),
                "value" => (&va.value, &vb.value),
                _ => ("", ""),
            };
            compare_cells(x, y, q.desc)
        });
    }
}

/// Numbers as numbers, text as text; empty ones always at the end
fn compare_cells(a: &str, b: &str, desc: bool) -> Ordering {
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    let o = match (as_number(a), as_number(b)) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    };
    if desc {
        o.reverse()
    } else {
        o
    }
}

impl KitView {
    pub fn load(root: &Path) -> Result<KitView, String> {
        let kit = Kit::open(root)?;
        let mut icons = HashMap::new();
        if let Ok(dir) = std::fs::read_dir(root.join("icons")) {
            for e in dir.flatten() {
                let file = e.file_name().to_string_lossy().into_owned();
                if let Some((stem, _)) = file.rsplit_once('.') {
                    icons.insert(stem.to_string(), file.clone());
                }
            }
        }
        let tables = kit.config["table"].as_array().into_iter().flatten().map(|t| TableView::load(&kit, t, &icons)).collect::<Result<Vec<_>, _>>()?;
        let loot = crate::loot_screen::LootData::load(&kit);
        Ok(KitView { kit, tables, icons, last: Mutex::new(None), session: Mutex::new(Session::default()), checks: Arc::new(Mutex::new(BTreeMap::new())), loot })
    }

    /// Patch table data for `check`; tables not in the kit do not appear in it
    pub(crate) fn check_tables(&self, patch: &Patch) -> Arc<Mutex<BTreeMap<String, TableData>>> {
        {
            let mut map = self.checks.lock().unwrap();
            for c in &patch.changes {
                if !map.contains_key(&c.table) {
                    if let Ok(Some(t)) = self.kit.table_data(&c.table) {
                        map.insert(c.table.clone(), t);
                    }
                }
            }
        }
        self.checks.clone()
    }

    /// Picture file, if it is in the kit: only such names may be given to the interface
    pub fn icon_path(&self, file: &str) -> Option<std::path::PathBuf> {
        let stem = file.rsplit_once('.')?.0;
        (self.icons.get(stem).map(String::as_str) == Some(file)).then(|| self.kit.root.join("icons").join(file))
    }

    pub fn overview(&self) -> Json {
        self.overview_in(false)
    }

    /// Kit texts in the interface language; no translation — in Russian
    pub fn overview_in(&self, en: bool) -> Json {
        let pick = |t: &TableView, i: usize, ru: &str| -> String {
            if en && !t.en[i].is_empty() {
                t.en[i].clone()
            } else {
                ru.to_string()
            }
        };
        let cfg = &self.kit.config;
        let title = if en && cfg["title_en"].is_string() { &cfg["title_en"] } else { &cfg["title"] };
        let notes = if en && cfg["notes_en"].is_array() { &cfg["notes_en"] } else { &cfg["notes"] };
        let tables: Vec<Json> = self
            .tables
            .iter()
            .map(|t| {
                json!({
                    "name": t.name, "title": t.title_of(en), "about": pick(t, 1, &t.about), "where": pick(t, 2, &t.place), "layout": t.layout, "group": t.group,
                    "rows": t.rows.len(), "values": t.values.len(), "editable_values": t.editable_values,
                    "changed": t.changed, "new_rows": t.new_rows, "removed_rows": t.removed_rows,
                    "has_orig": t.has_orig, "source": t.source
                })
            })
            .collect();
        json!({
            "features": self.features(),
            "version": self.kit.version(),
            "title": title,
            "season": self.kit.season(),
            "notes": notes,
            "has_catalog": self.kit.catalog.is_some(),
            "tables": tables
        })
    }

    pub fn query(&self, table: &str, q: &Query, offset: usize, limit: usize) -> Result<Json, String> {
        let ti = self.tables.iter().position(|t| t.name == table).ok_or(format!("table {} is not in the kit", table))?;
        let t = &self.tables[ti];
        let session = self.session.lock().unwrap();
        let gen = session.generation;
        let cached = self.last.lock().unwrap().as_ref().filter(|(n, cq, g, _)| n == table && cq == q && *g == gen).map(|(_, _, _, s)| s.clone());
        let sel = cached.unwrap_or_else(|| {
            let s = Arc::new(t.select(ti, q, &session));
            *self.last.lock().unwrap() = Some((table.to_string(), q.clone(), gen, s.clone()));
            s
        });
        let page = sel.ids.iter().skip(offset).take(limit);
        let rows: Vec<Json> = if sel.long {
            let row_of: HashMap<usize, &RowInfo> = t.rows.iter().map(|r| (r.row, r)).collect();
            page.map(|i| {
                let v = &t.values[*i as usize];
                let pid = t.path_ids[&v.path];
                let mut j = t.row_head(row_of[&v.row], q.en);
                let mut info = t.column_json(pid, q.en);
                let name = if q.en { t.names_en.get(&(v.row, v.path.clone())).map(String::as_str).unwrap_or(&v.name) } else { v.name.as_str() };
                if !name.is_empty() && info["label"].as_str().is_some_and(|l| !l.contains(name)) {
                    info["label"] = json!(format!("{} · {}", info["label"].as_str().unwrap_or_default(), name));
                }
                j["field"] = info;
                j["cell"] = t.cell_json(ti, v.row, pid, Some(&v.value), &session);
                j
            })
            .collect()
        } else {
            page.map(|i| {
                let r = &t.rows[*i as usize];
                let mut j = t.row_head(r, q.en);
                j["cells"] = Json::Array(
                    sel.columns
                        .iter()
                        .map(|c| {
                            let value = r.cells.binary_search_by_key(c, |x| x.0).ok().map(|k| t.values[r.cells[k].1 as usize].value.as_str());
                            t.cell_json(ti, r.row, *c, value, &session)
                        })
                        .collect(),
                );
                j
            })
            .collect()
        };
        let mut out = json!({ "total": sel.ids.len(), "offset": offset, "long": sel.long, "rows": rows });
        // Columns and filter options are needed only with the first page
        if offset == 0 {
            out["columns"] = Json::Array(sel.columns.iter().map(|c| t.column_json(*c, q.en)).collect());
            out["columns_matched"] = json!(sel.columns_matched);
            out["kinds"] = json!(t.kinds);
            let rarity_order = |r: &String| self.kit.catalog.as_ref().and_then(|c| c["rarities"][r]["value"].as_i64()).unwrap_or(i64::MAX);
            let mut rarities = t.rarities.clone();
            rarities.sort_by_key(rarity_order);
            out["rarities"] = Json::Array(
                rarities
                    .iter()
                    .map(|r| {
                        let label = self.kit.catalog.as_ref().and_then(|c| c["rarities"][r][if q.en { "en" } else { "ru" }].as_str()).unwrap_or(r);
                        json!({ "id": r, "label": label })
                    })
                    .collect(),
            );
            out["has_all_fields"] = json!(t.paths.iter().any(|p| !p.editable && !p.context) && t.paths.iter().any(|p| p.editable));
        }
        Ok(out)
    }
}
