//! Patch edits in the loot template text: only the numbers themselves change, comments and order stay.
//! Which value lies where is decided by `balance_kit::loot::locate`, as in the app.
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use balance_kit::loot::{locate, number_json, Place};
use balance_kit::{PatchChange, TableData};
use serde_json::Value as Json;
use toml_edit::{value, Array, DocumentMut, Item, Value as TomlValue};

fn toml_number(text: &str) -> Option<TomlValue> {
    match number_json(text) {
        Json::Number(n) => Some(match n.as_i64() {
            Some(i) => TomlValue::from(i),
            None => TomlValue::from(n.as_f64()?),
        }),
        _ => None,
    }
}

/// New value in the old place: the spaces around it stay
fn set(item: &mut Item, new: TomlValue) {
    let decor = item.as_value().map(|v| v.decor().clone());
    let mut v = new;
    if let Some(d) = decor {
        *v.decor_mut() = d;
    }
    *item = Item::Value(v);
}

fn set_cnt(it: &mut Item, i: usize, new: TomlValue) -> Result<(), String> {
    if it.get("cnt").is_none() {
        // No cnt — a single piece: the value cnt[0] appears as a new field
        if i != 0 {
            return Err(format!("cnt[{}]: the item has a single count", i));
        }
        it["cnt"] = value(new);
        return Ok(());
    }
    let cnt = &mut it["cnt"];
    if cnt.as_array().is_some() {
        let a: &mut Array = cnt.as_array_mut().unwrap();
        return match a.get_mut(i) {
            Some(slot) => {
                let decor = slot.decor().clone();
                *slot = new;
                *slot.decor_mut() = decor;
                Ok(())
            }
            None => Err(format!("cnt[{}]: the list has {} numbers", i, a.len())),
        };
    }
    if i != 0 {
        return Err(format!("cnt[{}]: the item has a single count", i));
    }
    set(cnt, new);
    Ok(())
}

fn apply_one(doc: &mut DocumentMut, place: &Place, new: TomlValue) -> Result<(), String> {
    match place {
        Place::SlotChance { group, slot } => {
            let s = doc["groups"][group]["slots"].get_mut(*slot).filter(|x| x.is_table_like()).ok_or(format!("groups.{}: no slot {}", group, slot))?;
            set(&mut s["chance"], new);
        }
        Place::SlotPr { group, slot, i } => {
            let s = doc["groups"][group]["slots"].get_mut(*slot).filter(|x| x.is_table_like()).ok_or(format!("groups.{}: no slot {}", group, slot))?;
            let a = s.get_mut("pr").and_then(Item::as_array_mut).ok_or(format!("groups.{} slot {}: no pr", group, slot))?;
            let x = a.get_mut(*i).ok_or(format!("groups.{} slot {}: no pr[{}]", group, slot, i))?;
            let decor = x.decor().clone();
            *x = new;
            *x.decor_mut() = decor;
        }
        Place::ItemW { pool, item } => {
            let it = doc["pools"][pool]["items"].get_mut(*item).filter(|x| x.is_table_like()).ok_or(format!("pools.{}: no item {}", pool, item))?;
            set(&mut it["w"], new);
        }
        Place::ItemCnt { pool, item, i } => {
            let it = doc["pools"][pool]["items"].get_mut(*item).filter(|x| x.is_table_like()).ok_or(format!("pools.{}: no item {}", pool, item))?;
            set_cnt(it, *i, new).map_err(|e| format!("pools.{} item {}: {}", pool, item, e))?;
        }
        Place::Rule { section, kind, key } => {
            let mut cur: &mut Item = doc.as_item_mut();
            for s in section {
                cur = &mut cur[s.as_str()];
            }
            match key {
                Some(k) => {
                    let target = &mut cur[kind.as_str()][k.as_str()];
                    if target.is_none() {
                        *target = value(new);
                    } else {
                        set(target, new);
                    }
                }
                None => {
                    let target = &mut cur[kind.as_str()];
                    if target.is_none() {
                        *target = value(new);
                    } else {
                        set(target, new);
                    }
                }
            }
        }
    }
    Ok(())
}

/// New template text and the number of applied edits; the file is not touched.
pub fn apply(path: &Path, changes: &[PatchChange], tables: &BTreeMap<String, TableData>) -> Result<(String, usize), Vec<String>> {
    let text = std::fs::read_to_string(path).map_err(|e| vec![format!("{}: {}", path.display(), e)])?;
    let body = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let mut doc: DocumentMut = body.parse().map_err(|e| vec![format!("{}: {}", path.display(), e)])?;
    let mut errors = Vec::new();
    let mut applied = 0;
    // Place of a value: table row number -> (key, index among the rows with this key)
    let mut ids: HashMap<&str, HashMap<usize, (String, usize)>> = HashMap::new();
    for c in changes.iter().filter(|c| balance_kit::loot::is_template_table(&c.table)) {
        let Some(t) = tables.get(&c.table) else {
            errors.push(format!("{}: no such table", c.table));
            continue;
        };
        let table_ids = ids.entry(c.table.as_str()).or_insert_with(|| row_ids(t));
        let at = format!("{} row {:?} {}", c.table, c.row, c.path);
        let Some((key, n)) = c.row.and_then(|r| table_ids.get(&r)) else {
            errors.push(format!("{}: no row", at));
            continue;
        };
        let Some(place) = locate(&c.table, key, *n, &c.path) else {
            errors.push(format!("{}: no such place in the template", at));
            continue;
        };
        let Some(new) = toml_number(&c.new) else {
            errors.push(format!("{}: {} is not a number", at, c.new));
            continue;
        };
        match apply_one(&mut doc, &place, new) {
            Ok(()) => applied += 1,
            Err(e) => errors.push(format!("{}: {}", at, e)),
        }
    }
    if errors.is_empty() {
        Ok((doc.to_string(), applied))
    } else {
        Err(errors)
    }
}

pub fn row_ids(t: &TableData) -> HashMap<usize, (String, usize)> {
    let mut out = HashMap::new();
    let mut count: HashMap<&str, usize> = HashMap::new();
    for v in &t.values {
        if out.contains_key(&v.row) {
            continue;
        }
        let n = count.entry(v.key.as_str()).or_insert(0);
        out.insert(v.row, (v.key.clone(), *n));
        *n += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEMPLATE: &str = "# header\n[rules]\nremove = []\n\n# group\n[groups.g1]\nname = \"Chest\" # name\nslots = [\n  { id = 91, pool = 900, chance = 50 },\n  { id = 92, pools = [900, 901], pr = [70, 30], chance = 10.5 },\n]\n\n[pools.900]\nname = \"Gold\"\nitems = [\n  { id = 1, cnt = [2, 4], w = 30000 },\n  { id = 2, w = 5 },\n  { id = 3, cnt = 7, w = 5 },\n]\n";

    fn edit(place: Place, new: &str) -> String {
        let mut doc: DocumentMut = TEMPLATE.parse().unwrap();
        apply_one(&mut doc, &place, toml_number(new).unwrap()).unwrap();
        doc.to_string()
    }

    fn only_change(after: &str, before_line: &str, after_line: &str) {
        assert_eq!(after, TEMPLATE.replace(before_line, after_line), "only one value changes");
    }

    #[test]
    fn numbers_change_and_nothing_else() {
        only_change(&edit(Place::SlotChance { group: "g1".into(), slot: 0 }, "12.5"), "chance = 50 }", "chance = 12.5 }");
        only_change(&edit(Place::SlotPr { group: "g1".into(), slot: 1, i: 1 }, "40"), "pr = [70, 30]", "pr = [70, 40]");
        only_change(&edit(Place::ItemW { pool: "900".into(), item: 1 }, "0"), "{ id = 2, w = 5 }", "{ id = 2, w = 0 }");
        only_change(&edit(Place::ItemCnt { pool: "900".into(), item: 0, i: 1 }, "9"), "cnt = [2, 4]", "cnt = [2, 9]");
        only_change(&edit(Place::ItemCnt { pool: "900".into(), item: 2, i: 0 }, "8"), "cnt = 7", "cnt = 8");
    }

    #[test]
    fn a_missing_count_appears() {
        let out = edit(Place::ItemCnt { pool: "900".into(), item: 1, i: 0 }, "3");
        let doc: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(doc["pools"]["900"]["items"][1]["cnt"].as_integer(), Some(3));
        assert!(out.starts_with("# header"), "{out}");
    }

    #[test]
    fn rules_appear_in_their_section() {
        let out = edit(Place::Rule { section: vec!["rules".into()], kind: "chance".into(), key: Some("gold".into()) }, "1.5");
        let doc: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(doc["rules"]["chance"]["gold"].as_float(), Some(1.5));
        assert!(out.contains("# group") && out.contains("# name"), "{out}");
        let out = edit(Place::Rule { section: vec!["rules".into()], kind: "quality".into(), key: None }, "0.5");
        let doc: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(doc["rules"]["quality"].as_float(), Some(0.5));
    }

    #[test]
    fn a_wrong_place_is_an_error() {
        let mut doc: DocumentMut = TEMPLATE.parse().unwrap();
        assert!(apply_one(&mut doc, &Place::SlotChance { group: "g1".into(), slot: 9 }, TomlValue::from(1)).is_err());
        assert!(apply_one(&mut doc, &Place::ItemCnt { pool: "900".into(), item: 0, i: 5 }, TomlValue::from(1)).is_err());
        assert!(apply_one(&mut doc, &Place::SlotPr { group: "g1".into(), slot: 0, i: 0 }, TomlValue::from(1)).is_err());
    }
}
