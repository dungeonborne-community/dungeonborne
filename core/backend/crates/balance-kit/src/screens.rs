//! Screens on top of the tables: item card, class curves, bulk edit. Table names are named directly here: these are game
//! tables, and each screen is enabled only if the needed tables are in the kit (`KitView::features`).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value as Json};

use crate::check::Severity;
use crate::edit::{CellKey, Session};
use crate::values::{as_number, is_int_text};
use crate::view::{KitView, Query, TableView};

const ITEMS: &str = "affix_template";
const ITEM_TYPES: &str = "item_type";
const TRADE: &str = "trade_commodity";
const DISMANTLE: &str = "gear_dismantle";
const CRAFT: &str = "craft_table";
const GEAR: &str = "gear_template";
const MARKET: &str = "market_product_item";
const AFFIX_VALUES: &str = "affix_value_template";
const CLASSES: &str = "role_level_upgrade";
const AFFIX_GROUP_PATH: &str = "data.affix_value_id";
const REWARD_PATH: &str = "product.reward_items[0].type_id";
/// How many edits the preview shows: beyond that only the totals matter
const PREVIEW_LIMIT: usize = 300;
const ROLL_SHOWN: usize = 12;

/// 10 significant digits: removes the noise of adding fractions (0.30000000000000004)
fn number_text(x: f64) -> String {
    let rounded: f64 = format!("{:.9e}", x).parse().unwrap_or(x);
    format!("{}", rounded)
}

/// All roll values (`FIX_LIST` BAL2): value = min + floor(roll × n / 10000) × unit, n = (max − min) / unit + 1, roll 0…9999.
/// `count: null` — the step is not set, and the values cannot be computed.
pub fn roll_json(min: Option<f64>, max: Option<f64>, unit: Option<f64>) -> Json {
    let (Some(lo), Some(hi)) = (min, max) else { return json!({ "count": null }) };
    if hi < lo {
        return json!({ "count": null });
    }
    if hi == lo {
        return json!({ "count": 1, "values": [number_text(lo)], "more": false });
    }
    let Some(step) = unit.filter(|u| *u > 0.0) else { return json!({ "count": null }) };
    let steps = (hi - lo) / step;
    // The range is not divisible by the step: by the formula the value may exceed max, and what the game does was not checked on the server
    if (steps - steps.round()).abs() > 1e-6 {
        return json!({ "count": null, "uneven": true });
    }
    let n = steps.round() + 1.0;
    let top = (9999.0 * n / 10000.0).floor();
    let at = |i: f64| number_text(lo + i * step);
    let count = top as u64 + 1;
    let values: Vec<String> = if count as usize <= ROLL_SHOWN {
        (0..count).map(|i| at(i as f64)).collect()
    } else {
        (0..5).map(|i| at(f64::from(i))).chain((0..5).rev().map(|i| at(top - f64::from(i)))).collect()
    };
    json!({ "count": count, "values": values, "more": count as usize > ROLL_SHOWN })
}

#[derive(Debug)]
pub struct BulkSpec {
    pub table: String,
    pub query: Query,
    /// Selected rows; `None` — all rows of the current filter
    pub rows: Option<Vec<usize>>,
    pub paths: Vec<String>,
    /// Change the maximum together with the minimum, and vice versa
    pub pairs: bool,
    pub op: String,
    pub value: f64,
}

impl BulkSpec {
    pub fn from_json(s: &Json) -> Result<BulkSpec, String> {
        let op = s["op"].as_str().unwrap_or_default().to_string();
        if !matches!(op.as_str(), "mul" | "add" | "set") {
            return Err(format!("unknown operation: {}", op));
        }
        let value = s["value"].as_f64().filter(|v| v.is_finite()).ok_or("a number is required")?;
        Ok(BulkSpec {
            table: s["table"].as_str().unwrap_or_default().to_string(),
            query: Query::from_json(&s["query"]),
            rows: s["rows"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|r| r as usize)).collect()),
            paths: s["paths"].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(str::to_string)).collect(),
            pairs: s["pairs"].as_bool().unwrap_or(false),
            op,
            value,
        })
    }
}

impl TableView {
    pub(crate) fn pid(&self, path: &str) -> Option<u32> {
        self.path_ids.get(path).copied()
    }

    fn number_at(&self, row: usize, path: &str, ti: usize, s: &Session) -> Option<f64> {
        let pid = self.pid(path)?;
        if let Some(e) = s.edits.get(&(ti, row, pid)) {
            return as_number(e);
        }
        let pos = self.row_pos(row)?;
        as_number(self.rows[pos].value_of(self, pid)?)
    }
}

impl KitView {
    pub(crate) fn table_index(&self, name: &str) -> Option<usize> {
        self.tables.iter().position(|t| t.name == name)
    }

    pub fn features(&self) -> Json {
        json!({ "items": self.table_index(ITEMS).is_some() || self.table_index(ITEM_TYPES).is_some(), "curves": self.table_index(CLASSES).is_some(), "loot": self.has_loot() })
    }

    pub(crate) fn cell_ref(&self, ti: usize, row: usize, pid: u32, s: &Session, en: bool, row_unit: &str) -> Json {
        let t = &self.tables[ti];
        let value = t.row_pos(row).and_then(|p| t.rows[p].value_of(t, pid));
        let mut c = t.column_json(pid, en);
        c["table"] = json!(t.name);
        c["row"] = json!(row);
        c["row_unit"] = json!(row_unit);
        c["cell"] = t.cell_json(ti, row, pid, value, s);
        c
    }

    fn word(&self, key: &str, en: bool) -> String {
        let fields = if en { &self.kit.config["fields_en"] } else { &self.kit.config["fields"] };
        fields[key].as_str().unwrap_or(key).to_string()
    }

    /// Items for the list on the left
    pub fn list_items(&self, q: &Json) -> Json {
        let Some(ti) = self.table_index(ITEM_TYPES).or_else(|| self.table_index(ITEMS)) else { return json!({ "items": [], "kinds": [], "rarities": [] }) };
        let t = &self.tables[ti];
        let s = self.session.lock().unwrap();
        let en = q["lang"].as_str() == Some("en");
        let words: Vec<String> = q["search"].as_str().unwrap_or_default().to_lowercase().split_whitespace().map(str::to_string).collect();
        let (kind, rarity) = (q["kind"].as_str().unwrap_or_default(), q["rarity"].as_str().unwrap_or_default());
        let only_mine = q["only_mine"].as_bool() == Some(true);
        // An edit in any table about an item counts as an edit of the item: their icon field holds its id
        let mine: BTreeSet<&str> = s
            .edits
            .keys()
            .filter_map(|k| {
                let x = &self.tables[k.0];
                x.row_pos(k.1).map(|p| x.rows[p].icon_key.as_str())
            })
            .collect();
        // The type table also has service entries: show items with a name and everything that has properties
        let with_props: BTreeSet<&str> = self.table_index(ITEMS).map(|i| self.tables[i].rows.iter().map(|r| r.key.as_str()).collect()).unwrap_or_default();
        let named = |key: &str| self.kit.catalog.as_ref().is_none_or(|c| c["items"][key]["ru"].is_string());
        let items: Vec<Json> = t
            .rows
            .iter()
            .filter(|r| named(&r.key) || with_props.contains(r.key.as_str()))
            .filter(|r| (kind.is_empty() || r.kind == kind) && (rarity.is_empty() || r.rarity == rarity) && words.iter().all(|w| r.haystack.contains(w)))
            .filter_map(|r| {
                let is_mine = mine.contains(r.key.as_str());
                if only_mine && !is_mine {
                    return None;
                }
                Some(json!({ "id": r.key, "title": r.title[usize::from(en)], "icon": r.icon, "kind": r.kind, "rarity": r.rarity, "mine": is_mine }))
            })
            .collect();
        let rarity_order = |r: &String| self.kit.catalog.as_ref().and_then(|c| c["rarities"][r]["value"].as_i64()).unwrap_or(i64::MAX);
        let mut rarities = t.rarities.clone();
        rarities.sort_by_key(rarity_order);
        let labels: Vec<Json> = rarities.iter().map(|r| json!({ "id": r, "label": self.rarity_label(r, en) })).collect();
        json!({ "items": items, "kinds": t.kinds, "rarities": labels })
    }

    pub(crate) fn rarity_label(&self, id: &str, en: bool) -> String {
        self.kit.catalog.as_ref().and_then(|c| c["rarities"][id][if en { "en" } else { "ru" }].as_str()).unwrap_or(id).to_string()
    }

    /// Everything about an item on one screen. `rarity` — which rarity to show in affixes; empty — the item's rarity.
    pub fn get_item(&self, id: &str, rarity: &str, en: bool) -> Result<Json, String> {
        // The type table knows all items, and the property template only those that have properties
        let r = [ITEM_TYPES, ITEMS]
            .iter()
            .filter_map(|n| self.table_index(n))
            .find_map(|i| self.tables[i].rows.iter().find(|r| r.key == id))
            .ok_or(format!("item {} is not in the kit", id))?;
        let s = self.session.lock().unwrap();
        let mut out = json!({
            "id": r.key, "title": r.title[usize::from(en)], "icon": r.icon, "kind": r.kind, "rarity": r.rarity,
            "rarity_label": self.rarity_label(&r.rarity, en)
        });

        out["base"] = json!([]);
        if let Some(ti) = self.table_index(ITEMS) {
            let t = &self.tables[ti];
            if let Some(row) = t.rows.iter().find(|x| x.key == id) {
                self.item_properties(ti, t, row, rarity, en, &s, &mut out);
            }
        }
        self.item_economy(id, en, &s, &mut out);

        // Durability
        if let Some(gi) = self.table_index(GEAR) {
            let g = &self.tables[gi];
            if let (Some(gr), Some(pid)) = (g.rows.iter().find(|x| x.key == id), g.pid("durability")) {
                out["durability"] = self.cell_ref(gi, gr.row, pid, &s, en, "");
            }
        }

        // Merchant: products that give this item
        if let Some(mi) = self.table_index(MARKET) {
            let m = &self.tables[mi];
            let mut goods = Vec::new();
            if let Some(reward) = m.pid(REWARD_PATH) {
                for g in m.rows.iter().filter(|g| g.value_of(m, reward) == Some(id)) {
                    let mut fields = Vec::new();
                    for (pid, _) in &g.cells {
                        if m.paths[*pid as usize].editable {
                            let mut f = self.cell_ref(mi, g.row, *pid, &s, en, "");
                            // Price — in which currency: the cost_type field next to the price
                            let path = &m.paths[*pid as usize].path;
                            if let Some(prefix) = path.rsplit_once('.').map(|x| x.0).filter(|_| path.contains("cost_items")) {
                                let currency = m.pid(&format!("{}.cost_type", prefix)).and_then(|p| g.value_of(m, p)).unwrap_or_default();
                                let name = self.kit.catalog.as_ref().and_then(|c| c["items"][currency][if en { "en" } else { "ru" }].as_str()).unwrap_or(currency);
                                f["currency"] = json!(name);
                            }
                            fields.push(f);
                        }
                    }
                    goods.push(json!({ "product": g.key, "fields": fields }));
                }
            }
            out["market"] = Json::Array(goods);
        }

        Ok(out)
    }

    /// Base properties and the affix group of the item from the property template
    fn item_properties(&self, ti: usize, t: &TableView, r: &crate::view::RowInfo, rarity: &str, en: bool, s: &Session, out: &mut Json) {
        // Base properties: data.fixed_rule[i].attribute.<attribute>.(fixed_value | offset.min_value | offset.max_value)
        let units = self.kit.config["attr_units"].as_object().cloned().unwrap_or_default();
        let mut base: BTreeMap<(usize, String), Map<String, Json>> = BTreeMap::new();
        for (pid, _) in &r.cells {
            let path = &t.paths[*pid as usize].path;
            let Some(rest) = path.strip_prefix("data.fixed_rule[") else { continue };
            let Some((rule, rest)) = rest.split_once("].attribute.") else { continue };
            let Some((attr, tail)) = rest.split_once('.') else { continue };
            if !t.paths[*pid as usize].editable {
                continue;
            }
            let slot = match tail {
                "fixed_value" => "fixed",
                "offset.min_value" => "min",
                "offset.max_value" => "max",
                _ => continue,
            };
            let unit = units.get(attr).and_then(|u| u.as_str()).unwrap_or_default();
            let e = base.entry((rule.parse().unwrap_or(0), attr.to_string())).or_default();
            e.insert("attr".into(), json!(attr));
            e.insert("label".into(), json!(self.word(attr, en)));
            e.insert(slot.into(), self.cell_ref(ti, r.row, *pid, &s, en, unit));
        }
        let mut props = Vec::new();
        for ((rule, attr), mut e) in base {
            if e.contains_key("min") {
                let prefix = format!("data.fixed_rule[{}].attribute.{}.offset.", rule, attr);
                e.insert(
                    "roll".into(),
                    roll_json(
                        t.number_at(r.row, &format!("{}min_value", prefix), ti, &s),
                        t.number_at(r.row, &format!("{}max_value", prefix), ti, &s),
                        t.number_at(r.row, &format!("{}unit", prefix), ti, &s),
                    ),
                );
            }
            props.push(Json::Object(e));
        }
        // What is in effect first: zeros at the end
        props.sort_by_key(|p| {
            let zero = p["fixed"]["cell"]["v"].as_str().is_some_and(|v| as_number(v) == Some(0.0));
            usize::from(zero)
        });
        out["base"] = Json::Array(props);
        // Group affixes: a shared table, an edit affects all items of the group
        let group = t.pid(AFFIX_GROUP_PATH).and_then(|p| r.value_of(t, p)).map(str::to_string);
        if let (Some(group), Some(ai)) = (group, self.table_index(AFFIX_VALUES)) {
            let a = &self.tables[ai];
            let shared = t.pid(AFFIX_GROUP_PATH).map_or(0, |p| t.rows.iter().filter(|x| x.value_of(t, p) == Some(group.as_str())).count());
            let wanted = if rarity.is_empty() { r.rarity.as_str() } else { rarity };
            let order = self.kit.catalog.as_ref().and_then(|c| c["rarities"][wanted]["value"].as_i64()).unwrap_or(2);
            let mut rarity_keys = BTreeSet::new();
            for p in &a.paths {
                if let Some(k) = p.path.split_once("rarity_range[").and_then(|x| x.1.split_once(']')).and_then(|x| x.0.parse::<i64>().ok()) {
                    rarity_keys.insert(k);
                }
            }
            let rarities: Vec<Json> = self
                .kit
                .catalog
                .as_ref()
                .and_then(|c| c["rarities"].as_object())
                .map(|m| {
                    m.iter()
                        .filter(|(_, v)| v["value"].as_i64().is_some_and(|x| x > 0 && rarity_keys.contains(&x)))
                        .map(|(k, _)| json!({ "id": k, "label": self.rarity_label(k, en) }))
                        .collect()
                })
                .unwrap_or_default();
            let attr_ids = self.kit.config["attr_ids"].as_object().cloned().unwrap_or_default();
            let mut rows = Vec::new();
            for x in a.rows.iter().filter(|x| x.key == group) {
                let step = a.number_at(x.row, "data.unit", ai, &s);
                let side = |i: usize, s: &Session| -> Json {
                    let base = format!("data.affix_data[{}].rarity_range[{}].", i, order);
                    let (lo, hi) = (a.pid(&format!("{}min_value", base)), a.pid(&format!("{}max_value", base)));
                    let (Some(lo), Some(hi)) = (lo, hi) else { return Json::Null };
                    json!({
                        "min": self.cell_ref(ai, x.row, lo, s, en, &x.unit),
                        "max": self.cell_ref(ai, x.row, hi, s, en, &x.unit),
                        "roll": roll_json(a.number_at(x.row, &format!("{}min_value", base), ai, s), a.number_at(x.row, &format!("{}max_value", base), ai, s), step)
                    })
                };
                let attr = a.pid("attribute_type").and_then(|p| x.value_of(a, p)).unwrap_or_default();
                rows.push(json!({
                    "row": x.row, "title": x.title[usize::from(en)], "unit": x.unit,
                    "attr": attr_ids.get(attr).and_then(|n| n.as_str()).unwrap_or(attr),
                    "major": side(0, &s), "minor": side(1, &s)
                }));
            }
            out["affix"] = json!({ "group": group, "shared": shared, "rarity": wanted, "rarities": rarities, "rows": rows });
        }
    }

    fn item_name(&self, id: &str, en: bool) -> String {
        self.kit.catalog.as_ref().and_then(|c| c["items"][id][if en { "en" } else { "ru" }].as_str()).unwrap_or(id).to_string()
    }

    /// Editable fields of a row; `name_of` labels a field with the item it belongs to
    fn row_fields(&self, ti: usize, r: &crate::view::RowInfo, en: bool, s: &Session, name_of: impl Fn(&str) -> Option<String>) -> Vec<Json> {
        let t = &self.tables[ti];
        r.cells
            .iter()
            .map(|(p, _)| *p)
            .filter(|p| t.paths[*p as usize].editable)
            .map(|p| {
                let mut f = self.cell_ref(ti, r.row, p, s, en, &r.unit);
                if let Some(n) = name_of(&t.paths[p as usize].path) {
                    f["item"] = json!(n);
                }
                f
            })
            .collect()
    }

    /// Sell price, market, dismantling, crafting. Each part exists only if its table is in the kit.
    fn item_economy(&self, id: &str, en: bool, s: &Session, out: &mut Json) {
        if let Some(i) = self.table_index(ITEM_TYPES) {
            let t = &self.tables[i];
            if let (Some(r), Some(p)) = (t.rows.iter().find(|x| x.key == id), t.pid("system_repurchase_price")) {
                out["sell"] = self.cell_ref(i, r.row, p, s, en, "");
            }
        }
        if let Some(i) = self.table_index(TRADE) {
            let t = &self.tables[i];
            if let (Some(item), Some(market)) = (t.pid("item_id"), t.pid("market_id")) {
                let rows: Vec<Json> = t
                    .rows
                    .iter()
                    .filter(|r| r.value_of(t, item) == Some(id))
                    .map(|r| json!({ "market": r.value_of(t, market).unwrap_or_default(), "fields": self.row_fields(i, r, en, s, |_| None) }))
                    .collect();
                out["trade"] = Json::Array(rows);
            }
        }
        if let Some(i) = self.table_index(DISMANTLE) {
            let t = &self.tables[i];
            if let Some(r) = t.rows.iter().find(|x| x.key == id) {
                let sibling = |path: &str| -> Option<String> {
                    let prefix = path.strip_suffix(".count")?;
                    let p = t.pid(&format!("{}.type_id", prefix))?;
                    Some(self.item_name(r.value_of(t, p)?, en))
                };
                out["dismantle"] = Json::Array(self.row_fields(i, r, en, s, sibling));
            }
        }
        if let Some(i) = self.table_index(CRAFT) {
            let t = &self.tables[i];
            if let Some(product) = t.pid("product[0].type_id") {
                let rows: Vec<Json> = t
                    .rows
                    .iter()
                    .filter(|r| r.value_of(t, product) == Some(id))
                    .map(|r| {
                        let sibling = |path: &str| -> Option<String> {
                            let prefix = path.strip_suffix(".count")?;
                            let p = t.pid(&format!("{}.type_id", prefix))?;
                            Some(self.item_name(r.value_of(t, p)?, en))
                        };
                        json!({ "recipe": r.key, "fields": self.row_fields(i, r, en, s, sibling) })
                    })
                    .collect();
                out["craft"] = Json::Array(rows);
            }
        }
    }

    /// Curve of one attribute for classes that have more than one level
    pub fn class_curves(&self, path: &str, en: bool) -> Result<Json, String> {
        let ti = self.table_index(CLASSES).ok_or("the kit has no class table")?;
        let t = &self.tables[ti];
        let level_pid = t.pid("role_level").ok_or("the class table has no level")?;
        let s = self.session.lock().unwrap();

        // Attributes: those that change with level first
        let mut attrs: Vec<(bool, u32)> = Vec::new();
        for (i, p) in t.paths.iter().enumerate() {
            if !p.editable || !p.path.ends_with("fixed_value") {
                continue;
            }
            let values: BTreeSet<&str> = t.rows.iter().filter_map(|r| r.value_of(t, i as u32)).collect();
            attrs.push((values.len() > 1, i as u32));
        }
        attrs.sort_by_key(|(varies, i)| (!varies, *i));
        let picked = if path.is_empty() { attrs.first().map(|a| a.1) } else { t.pid(path) };
        let attr_list: Vec<Json> = attrs
            .iter()
            .map(|(varies, i)| {
                let mut c = t.column_json(*i, en);
                c["varies"] = json!(varies);
                c
            })
            .collect();

        let catalog = self.kit.catalog.as_ref();
        let mut by_class: BTreeMap<&str, Vec<&crate::view::RowInfo>> = BTreeMap::new();
        for r in &t.rows {
            by_class.entry(r.key.as_str()).or_default().push(r);
        }
        let mut classes = Vec::new();
        for (id, rows) in by_class {
            let hidden = catalog.is_some_and(|c| c["classes"][id]["hidden"].as_bool() == Some(true));
            if hidden || rows.len() < 2 {
                continue;
            }
            let levels: Vec<Json> = rows
                .iter()
                .filter_map(|r| {
                    let level = as_number(r.value_of(t, level_pid)?)?;
                    let pid = picked?;
                    Some(json!({ "level": level, "ref": self.cell_ref(ti, r.row, pid, &s, en, "") }))
                })
                .collect();
            classes.push(json!({ "id": id, "title": rows[0].title[usize::from(en)], "levels": levels }));
        }
        Ok(json!({ "attrs": attr_list, "path": picked.map(|p| t.paths[p as usize].path.clone()), "classes": classes }))
    }

    /// Rows the bulk edit acts on
    fn bulk_rows(&self, ti: usize, spec_query: &Query, rows: &Option<Vec<usize>>, s: &Session) -> Vec<usize> {
        let t = &self.tables[ti];
        if let Some(list) = rows {
            return list.iter().copied().filter(|r| t.row_pos(*r).is_some()).collect();
        }
        let sel = t.select(ti, spec_query, s);
        let found: BTreeSet<usize> = if sel.long { sel.ids.iter().map(|i| t.values[*i as usize].row).collect() } else { sel.ids.iter().map(|i| t.rows[*i as usize].row).collect() };
        found.into_iter().collect()
    }

    /// Which fields can be chosen for a bulk edit of these rows
    pub fn bulk_fields(&self, table: &str, query: &Json, rows: &Json) -> Result<Json, String> {
        let ti = self.table_index(table).ok_or(format!("table {} is not in the kit", table))?;
        let t = &self.tables[ti];
        let q = Query::from_json(query);
        let s = self.session.lock().unwrap();
        let picked = rows.as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|r| r as usize)).collect());
        let target = self.bulk_rows(ti, &q, &picked, &s);
        let mut counts: BTreeMap<u32, usize> = BTreeMap::new();
        for row in &target {
            if let Some(p) = t.row_pos(*row) {
                for (pid, v) in &t.rows[p].cells {
                    if t.paths[*pid as usize].editable && as_number(&t.values[*v as usize].value).is_some() {
                        *counts.entry(*pid).or_default() += 1;
                    }
                }
            }
        }
        let fields: Vec<Json> = counts
            .iter()
            .map(|(pid, n)| {
                let mut c = t.column_json(*pid, q.en);
                c["count"] = json!(n);
                c
            })
            .collect();
        Ok(json!({ "rows": target.len(), "fields": fields }))
    }

    /// (what to change, how many values are already like that, how many cells are empty)
    fn bulk_targets(&self, spec: &BulkSpec, s: &Session) -> Result<(Vec<(CellKey, String)>, usize, usize), String> {
        let ti = self.table_index(&spec.table).ok_or(format!("table {} is not in the kit", spec.table))?;
        let t = &self.tables[ti];
        let tables = self.check_tables(&crate::patch::Patch {
            format: String::new(),
            base: String::new(),
            season: None,
            author: String::new(),
            comment: String::new(),
            changes: vec![crate::patch::PatchChange {
                table: spec.table.clone(),
                row: None,
                key: String::new(),
                title: String::new(),
                path: String::new(),
                old: String::new(),
                new: String::new(),
            }],
        });
        let kinds = tables.lock().unwrap().get(&spec.table).map(|d| d.kinds.clone()).unwrap_or_default();

        let mut paths: BTreeSet<u32> = BTreeSet::new();
        for path in &spec.paths {
            let pid = t.pid(path).ok_or(format!("field {} is not in the table", path))?;
            if !t.paths[pid as usize].editable {
                return Err(format!("field {} cannot be changed", path));
            }
            paths.insert(pid);
            if spec.pairs {
                let other = match (path.strip_suffix("min_value"), path.strip_suffix("max_value")) {
                    (Some(b), _) => Some(format!("{}max_value", b)),
                    (_, Some(b)) => Some(format!("{}min_value", b)),
                    _ => None,
                };
                if let Some(p) = other.and_then(|o| t.pid(&o)).filter(|p| t.paths[*p as usize].editable) {
                    paths.insert(p);
                }
            }
        }
        let mut wanted = Vec::new();
        let (mut skipped, mut empty) = (0, 0);
        for row in self.bulk_rows(ti, &spec.query, &spec.rows, s) {
            let Some(pos) = t.row_pos(row) else { continue };
            let r = &t.rows[pos];
            for pid in &paths {
                let key = (ti, row, *pid);
                let kit = r.value_of(t, *pid);
                let current = s.edits.get(&key).map(String::as_str).or(kit);
                let Some(now) = current.and_then(as_number) else {
                    // Empty cells are not filled: a bulk edit changes what already exists
                    empty += 1;
                    continue;
                };
                let p = &t.paths[*pid as usize];
                let percent = p.value_like && (p.unit == "доля" || r.unit == "доля");
                let amount = if percent && spec.op != "mul" { spec.value / 100.0 } else { spec.value };
                let result = match spec.op.as_str() {
                    "mul" => now * spec.value,
                    "add" => now + amount,
                    _ => amount,
                };
                let integer = match kinds.get(&crate::values::generic(&p.path)) {
                    Some(k) => k.is_integer(),
                    None => kit.is_some_and(is_int_text),
                };
                let text = if integer { format!("{}", result.round() as i64) } else { number_text(result) };
                if crate::values::same(&text, current.unwrap_or_default()) {
                    skipped += 1;
                    continue;
                }
                wanted.push((key, text));
            }
        }
        Ok((wanted, skipped, empty))
    }

    pub fn preview_bulk(&self, spec: &BulkSpec) -> Result<Json, String> {
        let s = self.session.lock().unwrap();
        let (wanted, skipped, empty) = self.bulk_targets(spec, &s)?;
        let mut trial = Session::default();
        trial.edits = s.edits.clone();
        self.commit(&mut trial, wanted.clone());
        let new_errors = wanted.iter().filter(|(k, _)| trial.issues.get(k).is_some_and(|i| i.severity == Severity::Error)).count();
        let new_warnings = wanted.iter().filter(|(k, _)| trial.issues.get(k).is_some_and(|i| i.severity == Severity::Warning)).count();
        let shown: Vec<Json> = wanted
            .iter()
            .take(PREVIEW_LIMIT)
            .map(|(k, text)| {
                let t = &self.tables[k.0];
                let r = t.row_pos(k.1).map(|p| &t.rows[p]);
                let en = spec.query.en;
                let current = s.edits.get(k).map(String::as_str).or_else(|| r.and_then(|x| x.value_of(t, k.2))).unwrap_or_default();
                let mut c = json!({
                    "row": k.1, "key": r.map(|x| x.key.as_str()).unwrap_or_default(),
                    "title": r.map(|x| x.title[usize::from(en)].as_str()).unwrap_or_default(),
                    "unit": r.map(|x| x.unit.as_str()).unwrap_or_default(),
                    "field": t.column_json(k.2, en), "old": current, "new": text
                });
                if let Some(i) = trial.issues.get(k) {
                    c["issue"] = json!({ "severity": if i.severity == Severity::Error { "error" } else { "warning" }, "code": i.code });
                }
                c
            })
            .collect();
        Ok(json!({ "total": wanted.len(), "skipped": skipped, "empty": empty, "errors": new_errors, "warnings": new_warnings, "changes": shown }))
    }

    /// Applies the same as the preview showed, in one undo step
    pub fn apply_bulk(&self, spec: &BulkSpec) -> Result<Json, String> {
        let mut s = self.session.lock().unwrap();
        let (wanted, _, _) = self.bulk_targets(spec, &s)?;
        let changed = self.commit(&mut s, wanted);
        let mut out = self.status_of(&s);
        out["changed"] = json!(changed);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The formula from FIX_LIST BAL2, over all rolls
    fn all(lo: f64, hi: f64, unit: f64) -> Vec<f64> {
        let n = ((hi - lo) / unit).round() + 1.0;
        let mut out: Vec<f64> = Vec::new();
        for level in 0..10000u32 {
            let v = lo + (f64::from(level) * n / 10000.0).floor() * unit;
            if out.last().is_none_or(|l| (l - v).abs() > 1e-9) {
                out.push(v);
            }
        }
        out
    }

    #[test]
    fn roll_matches_the_formula() {
        for (lo, hi, unit) in [(4.0, 12.0, 1.0), (0.08, 0.1, 0.01), (100.0, 110.0, 2.0), (0.008, 0.012, 0.001)] {
            let r = roll_json(Some(lo), Some(hi), Some(unit));
            let want = all(lo, hi, unit);
            assert_eq!(r["count"].as_u64().unwrap() as usize, want.len(), "{lo} {hi} {unit}");
            let got: Vec<f64> = r["values"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
            for (g, w) in got.iter().zip(want.iter()) {
                assert!((g - w).abs() < 1e-6, "{got:?} {want:?}");
            }
        }
    }

    #[test]
    fn roll_special_cases() {
        assert_eq!(roll_json(Some(5.0), Some(5.0), None)["values"], json!(["5"]));
        assert_eq!(roll_json(Some(1.0), Some(5.0), None)["count"], Json::Null);
        assert_eq!(roll_json(Some(1.0), Some(5.0), Some(0.0))["count"], Json::Null);
        assert_eq!(roll_json(Some(9.0), Some(5.0), Some(1.0))["count"], Json::Null);
        assert_eq!(roll_json(None, Some(5.0), Some(1.0))["count"], Json::Null);
        let uneven = roll_json(Some(5.0), Some(9.0), Some(3.0));
        assert_eq!((uneven["count"].as_u64(), uneven["uneven"].as_bool()), (None, Some(true)), "{uneven}");
        let many = roll_json(Some(0.0), Some(1000.0), Some(1.0));
        assert_eq!((many["count"].as_u64(), many["more"].as_bool()), (Some(1001), Some(true)));
        assert_eq!(many["values"].as_array().unwrap().len(), 10);
        assert_eq!(many["values"][9], "1000");
    }

    #[test]
    fn numbers_have_no_noise() {
        assert_eq!(number_text(0.1 + 0.2), "0.3");
        assert_eq!(number_text(152.5), "152.5");
        assert_eq!(number_text(1.0), "1");
    }
}
