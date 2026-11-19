//! The "Loot" screen on a small kit: average drop, edits through tables, rules and modes, a shared pool, an old kit.

use std::collections::HashMap;
use std::path::Path;

use balance_kit::loot::{self, Cat, Labels};
use balance_kit::KitView;
use serde_json::{json, Value as Json};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn view() -> KitView {
    KitView::load(&fixture("kit_loot")).unwrap()
}

fn query(map: Option<i64>, kind: &str) -> Json {
    json!({ "map": map, "kind": kind, "search": "", "only_mine": false, "lang": "ru" })
}

fn keys(list: &Json) -> Vec<&str> {
    list["groups"].as_array().unwrap().iter().map(|g| g["key"].as_str().unwrap()).collect()
}

fn set(v: &KitView, table: &str, row: usize, path: &str, new: &str) {
    v.set_values(&[(table.to_string(), row, path.to_string(), new.to_string())]);
}

fn near(a: &Json, b: f64) -> bool {
    (a.as_f64().unwrap() - b).abs() < 1e-4
}

#[test]
fn the_loot_screen_is_offered_only_with_a_template() {
    assert_eq!(view().features()["loot"], true);
    let old = KitView::load(&fixture("kit")).unwrap();
    assert_eq!(old.features()["loot"], false);
    assert!(old.loot_groups(&query(None, "")).is_err());
}

#[test]
fn groups_by_map_and_kind() {
    let v = view();
    let all = v.loot_groups(&query(None, "")).unwrap();
    assert_eq!(keys(&all), ["g3", "g1", "g1@no_gold", "g2"], "by source kinds; a group without sources is not shown");
    assert_eq!(all["maps"].as_array().unwrap().iter().map(|m| m["label"].as_str().unwrap()).collect::<Vec<_>>(), ["По умолчанию", "Склеп", "Склеп Classic"]);
    assert_eq!(keys(&v.loot_groups(&query(Some(10101), "")).unwrap()), ["g3", "g1"]);
    assert_eq!(keys(&v.loot_groups(&query(Some(10102), "")).unwrap()), ["g1@no_gold"], "the mode of its own map");
    assert_eq!(keys(&v.loot_groups(&query(Some(0), "")).unwrap()), ["g2"]);
    assert_eq!(keys(&v.loot_groups(&query(None, "monster")).unwrap()), ["g3"]);
    let found = v.loot_groups(&json!({ "search": "монстры d", "lang": "ru" })).unwrap();
    assert_eq!(keys(&found), ["g3"]);
    let en = v.loot_groups(&json!({ "map": 10101, "lang": "en" })).unwrap();
    assert_eq!(en["groups"][1]["title"], "Chests: A, B");
    assert_eq!(en["maps"][1]["label"], "Crypt");
    let kinds = v.loot_groups(&query(Some(10101), "")).unwrap();
    assert_eq!(kinds["kinds"].as_array().unwrap().iter().map(|k| (k["id"].as_str().unwrap(), k["count"].as_u64().unwrap())).collect::<Vec<_>>(), [("monster", 1), ("chest", 1)]);
}

#[test]
fn average_drop_of_a_group() {
    let v = view();
    let g = v.loot_group("g1", false).unwrap();
    assert_eq!(g["title"], "Сундуки: A, B");
    assert_eq!(g["sources"], json!(["A", "B"]));
    assert_eq!(g["maps"], json!(["Склеп"]));
    assert!(near(&g["summary"]["items"], 1.5));
    assert!(near(&g["summary"]["gold"], 1.5), "50 % × the mean of 2 and 4");
    let slots = g["slots"].as_array().unwrap();
    assert!(near(&slots[0]["rate"], 0.5));
    assert_eq!(slots[0]["chance"]["cell"]["v"], "50");
    assert_eq!(slots[0]["chance"]["table"], "lt_slots");
    let mixed = &slots[1]["pools"][0];
    assert_eq!(mixed["label"], "Смесь");
    let items = mixed["items"].as_array().unwrap();
    assert_eq!(items.iter().map(|i| i["title"].as_str().unwrap()).collect::<Vec<_>>(), ["Меч", "Камень", "Зелье"]);
    assert!(near(&items[1]["share"], 2.0 / 3.0));
    assert!(near(&items[1]["chance"], 2.0 / 3.0));
    assert_eq!(items[1]["w"]["cell"]["v"], "20000");
    assert_eq!(items[0]["cnt"][0]["cell"]["v"], "1", "without cnt — a single piece");
    assert_eq!(items[2]["cnt"][0]["cell"]["v"], "5");
    assert!(items[2]["share"].as_f64().unwrap_or(0.0) == 0.0, "weight 0 does not drop");
    assert_eq!(items[0]["rarity_label"], "Редкий");
    let gold = &slots[0]["pools"][0]["items"][0];
    assert_eq!(gold["cnt"].as_array().unwrap().len(), 2);
    assert!(near(&gold["avg"], 3.0));
    assert!(v.loot_group("нет", false).is_err());
}

#[test]
fn edits_change_the_drop_and_the_kit_value_stays() {
    let v = view();
    // Slot with gold g1: row 0, chance 50 -> 100
    set(&v, "lt_slots", 0, "chance", "100");
    let g = v.loot_group("g1", false).unwrap();
    assert!(near(&g["summary"]["items"], 2.0));
    assert!(near(&g["summary_was"]["items"], 1.5));
    assert!(near(&g["slots"][0]["rate"], 1.0));
    assert!(near(&g["slots"][0]["rate_was"], 0.5));
    assert_eq!(g["slots"][0]["chance"]["cell"]["k"], "50");
    let listed = v.loot_groups(&query(Some(10101), "")).unwrap();
    assert_eq!(listed["groups"][1]["mine"], true);
    assert!(near(&listed["groups"][1]["items"], 2.0));
    assert!(near(&listed["groups"][1]["items_was"], 1.5));
    assert_eq!(listed["groups"][0]["mine"], false, "the monster does not touch this slot");
    // Weight changes the shares of the other items of the pool
    set(&v, "lt_items", 1, "w", "20000");
    let g = v.loot_group("g3", false).unwrap();
    let items = g["slots"][0]["pools"][0]["items"].as_array().unwrap();
    assert!(near(&items[0]["share"], 0.5));
    assert!(near(&items[0]["chance_was"], 0.2 / 3.0));
    assert!(near(&items[0]["chance"], 0.1));
    let mine = v.loot_groups(&json!({ "only_mine": true, "lang": "ru" })).unwrap();
    assert_eq!(keys(&mine), ["g3", "g1", "g1@no_gold", "g2"], "shared pool: a weight edit touches all groups");
    // Chance 0 removes the slot, and that does not break groups with it
    set(&v, "lt_slots", 0, "chance", "0");
    let g = v.loot_group("g1", false).unwrap();
    assert!(g["slots"][0]["rate"].is_null());
    assert!(g["slots"][0]["pools"][0]["items"][0]["share"].is_null());
    assert!(near(&g["summary"]["gold"], 0.0));
    // An out-of-bounds edit is visible on the cell
    set(&v, "lt_slots", 0, "chance", "150");
    let g = v.loot_group("g1", false).unwrap();
    assert_eq!(g["slots"][0]["chance"]["cell"]["e"], "new_out_of_limits");
}

#[test]
fn rules_and_modes() {
    let v = view();
    let g = v.loot_group("g1@no_gold", false).unwrap();
    assert_eq!(g["mode"]["title"], "Без золота");
    assert_eq!(g["maps"], json!(["Склеп Classic"]));
    assert_eq!(g["rules"][0]["scope"], "no_gold");
    assert_eq!(g["rules"][0]["remove"], json!(["золото"]));
    let slots = g["slots"].as_array().unwrap();
    assert_eq!(slots.len(), 2, "a slot with a removed pool is shown");
    assert!(slots[0]["rate"].is_null(), "the mode has no gold");
    assert_eq!(slots[0]["template"], 50.0, "the template value is the same");
    assert!(near(&g["summary"]["gold"], 0.0));
    // Multiplier from the rules table: gold chance ×1.5 on all maps
    let plain = v.loot_group("g1", false).unwrap();
    assert!(plain["rules"].as_array().unwrap().is_empty());
    set(&v, "lt_rules", 0, "chance.gold", "1.5");
    let g = v.loot_group("g1", false).unwrap();
    assert!(near(&g["slots"][0]["rate"], 0.75), "{}", g["slots"][0]["rate"]);
    assert_eq!(g["slots"][0]["chance"]["cell"]["v"], "50", "the template did not change");
    assert_eq!(g["rules"][0]["chance"][0]["label"], "золото");
    assert_eq!(g["rules"][0]["chance"][0]["value"], 1.5);
}

#[test]
fn a_shared_pool_warns() {
    let v = view();
    let g = v.loot_group("g3", false).unwrap();
    let pool = &g["slots"][0]["pools"][0];
    assert_eq!(pool["shared"], 3, "g1, g2 and the group without a source");
    assert_eq!(pool["shared_titles"][0], "Сундуки: A, B");
}

#[test]
fn a_slot_with_two_pools_shows_shares() {
    let v = view();
    let g = v.loot_group("g2", false).unwrap();
    let pools = g["slots"][0]["pools"].as_array().unwrap();
    assert_eq!(pools.len(), 2);
    assert!(near(&pools[0]["share"], 0.7));
    assert_eq!(pools[0]["pr"]["cell"]["v"], "70");
    assert!(near(&pools[1]["items"][1]["chance"], 0.1 * 0.3 * 2.0 / 3.0));
    let en = v.loot_group("g2", true).unwrap();
    assert_eq!(en["slots"][0]["pools"][1]["label"], "weapons 1, gems 1, potions 1");
    assert_eq!(en["slots"][0]["pools"][1]["items"][0]["title"], "Sword");
}

#[test]
fn patch_from_the_screen_round_trips() {
    let v = view();
    set(&v, "lt_items", 1, "w", "12000");
    set(&v, "lt_rules", 1, "weight.gold", "2");
    let p = v.patch_json("test", "", "2026-11-06", "test").unwrap();
    let changes = p["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    let item = changes.iter().find(|c| c["table"] == "lt_items").unwrap();
    assert_eq!((item["key"].as_str(), item["old"].as_str(), item["new"].as_str()), (Some("901"), Some("10000"), Some("12000")));
    let rule = changes.iter().find(|c| c["table"] == "lt_rules").unwrap();
    assert_eq!((rule["key"].as_str(), rule["old"].as_str(), rule["path"].as_str()), (Some("modes.no_gold"), Some(""), Some("weight.gold")));
}

#[test]
fn the_fixture_is_what_the_template_gives() {
    // Table rows and the template are the same thing: otherwise an edit would land in the wrong slot
    let file: Json = serde_json::from_slice(&std::fs::read(fixture("kit_loot/raw/loot_template.json")).unwrap()).unwrap();
    let doc = &file["template"];
    let item = |id: i64| ["", "Золото", "Меч", "Камень", "Зелье"][id as usize].to_string();
    let group = |k: &str, _: &Json| file["groups"][k]["ru"].as_str().unwrap().to_string();
    let pool = |k: &str, p: &Json| p["name"].as_str().unwrap_or(k).to_string();
    let tables = loot::template_tables(doc, &Labels { item: &item, group: &group, pool: &pool });
    let v = balance_kit::Kit::open(&fixture("kit_loot")).unwrap();
    for (name, values) in tables {
        let data = v.table_data(name).unwrap().unwrap();
        assert_eq!(data.values, values, "{name}");
    }
}

#[test]
fn a_foreign_template_does_not_panic() {
    // A kit is a foreign file: a template of a strange shape must not crash the calculation and the edits
    let cats: HashMap<i64, Cat> = HashMap::new();
    for doc in [json!(null), json!({"groups": 1, "pools": 2, "rules": 3, "modes": 4}), json!({"groups": {"g": {"slots": 5}}, "pools": {"p": {"items": "x"}}})] {
        let _ = loot::drop_view(&doc, &cats, &HashMap::new());
        let mut d = doc.clone();
        for place in [loot::locate(loot::SLOTS, "g", 0, "chance"), loot::locate(loot::ITEMS, "p", 0, "w"), loot::locate(loot::RULES, "rules", 0, "chance.gold"), loot::locate(loot::RULES, "groups.g", 0, "quality")].into_iter().flatten() {
            loot::apply_to_doc(&mut d, &place, "1");
        }
    }
}
