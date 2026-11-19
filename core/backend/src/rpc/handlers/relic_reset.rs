//! One-time reset of 12.11 (user's decision): for all players
//! - relic resources 1037, 1038, 1039, 1052, 1053 → 0 (the starting bonus of 5000 each was removed from the playtest season);
//! - unlocked relics, gems and socketed gems (state["item_heirloom"]) — as for a new player (empty);
//! - crafting recipes unlocked by items (state["item_crafting_recipes"]); craft_group recipes stay for everyone;
//! - relic instances (items) — from the stash, backpacks and worn gear.
//! The mark of the granted bonus (state["playtest_materials"]) stays. Mail and trade are not touched.
//! Runs at startup before accepting clients, once: mark `migrations:relic_reset_20261112`.

use super::inventory_handlers as inv;
use crate::db::GameDb;
use crate::structures::PlayerModel;
use crate::tgf;
use std::collections::HashSet;

const MIGRATION_TABLE: &str = "migrations";
const MIGRATION_ID: &str = "relic_reset_20261112";
pub const MATERIALS: [i32; 5] = [1037, 1038, 1039, 1052, 1053];

#[derive(Default, Debug, PartialEq)]
pub struct Reset {
    pub materials: i64,
    pub relics_unlocked: usize,
    pub gems_unlocked: usize,
    pub recipes: usize,
    pub relic_items: usize,
}

impl Reset {
    fn any(&self) -> bool {
        *self != Reset::default()
    }
}

fn is_relic(item: &tgf::DItem, relic_ids: &HashSet<i32>) -> bool {
    matches!(item.create_info.as_ref().and_then(|c| c.create_data.as_ref()), Some(tgf::d_item_create_info::CreateData::Heirloom(_)))
        || item.basic.as_ref().is_some_and(|b| relic_ids.contains(&b.type_id))
}

/// Reset of one player. relic_ids — gear_heirloom_guide.inst_id (and ids of relics without inst_id).
pub fn reset_player(p: &mut PlayerModel, relic_ids: &HashSet<i32>) -> Reset {
    let mut r = Reset::default();
    for id in MATERIALS {
        let have = super::common::coin(p, id);
        if have > 0 {
            r.materials += have;
            p.state["coins"][id.to_string()] = serde_json::json!(0);
        }
    }
    let h = &p.state["item_heirloom"];
    if !h.is_null() {
        r.relics_unlocked = h["unlock_heirloom_id"].as_array().map_or(0, |a| a.len());
        r.gems_unlocked = h["unlock_gem_id"].as_array().map_or(0, |a| a.len());
        if let Some(o) = p.state.as_object_mut() {
            o.remove("item_heirloom");
        }
    }
    if let Some(a) = p.state["item_crafting_recipes"].as_array() {
        r.recipes = a.len();
        if let Some(o) = p.state.as_object_mut() {
            o.remove("item_crafting_recipes");
        }
    }
    for place in inv::all_places(p) {
        if let Some(list) = inv::container_mut(p, &place) {
            let before = list.len();
            list.retain(|it| !is_relic(it, relic_ids));
            r.relic_items += before - list.len();
        }
    }
    r
}

/// One-time reset over all players (main.rs, after guid::migrate)
/// The "reset done" mark. 12.11: it was read as serde_json::Value — the `id` field (Thing) of a SurrealDB record cannot be read
/// into a Value ("invalid type: enum"), GameDb::get returned None, and the reset ran a second time (16:12 UTC).
#[derive(serde::Deserialize)]
struct Marker {
    #[serde(default)]
    #[allow(dead_code)]
    done: bool,
}

pub async fn run(db: &GameDb, relic_ids: &HashSet<i32>) {
    // a read error is not "no mark": reset nothing
    match db.db.select::<Option<Marker>>((MIGRATION_TABLE, MIGRATION_ID)).await {
        Ok(Some(_)) => return,
        Ok(None) => {}
        Err(e) => {
            eprintln!("[RelicReset] cannot read mark {}:{} ({}) — not starting the reset", MIGRATION_TABLE, MIGRATION_ID, e);
            return;
        }
    }
    let players: Vec<PlayerModel> = match db.db.query("SELECT * FROM players").await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[RelicReset] cannot read players: {}", e);
            return;
        }
    };
    let (total, mut changed) = (players.len(), 0);
    let mut sum = Reset::default();
    for mut p in players {
        let r = reset_player(&mut p, relic_ids);
        if r.any() {
            changed += 1;
            println!(
                "[RelicReset] {}: resources {}, relics unlocked {}, gems {}, recipes {}, relic items {}",
                p.open_id, r.materials, r.relics_unlocked, r.gems_unlocked, r.recipes, r.relic_items
            );
            db.save("players", &p.open_id, &p).await;
            sum.materials += r.materials;
            sum.relics_unlocked += r.relics_unlocked;
            sum.gems_unlocked += r.gems_unlocked;
            sum.recipes += r.recipes;
            sum.relic_items += r.relic_items;
        }
    }
    db.save(MIGRATION_TABLE, MIGRATION_ID, &serde_json::json!({ "done": true, "players": total, "changed": changed })).await;
    println!(
        "[RelicReset] done: players {}, changed {}; removed resources {}, relics {}, gems {}, recipes {}, relic items {}",
        total, changed, sum.materials, sum.relics_unlocked, sum.gems_unlocked, sum.recipes, sum.relic_items
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 12.11: a second run after the mark is written resets nothing (on prod the mark was not read — the reset ran twice)
    #[tokio::test]
    async fn second_run_is_noop_after_marker() {
        let dir = format!("target/test_relic_reset_db_{}", std::process::id());
        let _ = std::fs::remove_dir_all(&dir);
        let db = GameDb::init(&dir).await;
        let ids: HashSet<i32> = HashSet::new();
        let mut p = PlayerModel::new("76561197960265799", 11000099);
        p.state["coins"] = serde_json::json!({ "1037": 50 });
        db.save("players", &p.open_id, &p).await;
        run(&db, &ids).await;
        let after1 = db.get::<PlayerModel>("players", &p.open_id).await.unwrap();
        assert_eq!(after1.state["coins"]["1037"], 0, "the first run resets");
        // the player collected resources again — the second run does not touch them
        let mut p2 = after1.clone();
        p2.state["coins"]["1037"] = serde_json::json!(70);
        db.save("players", &p2.open_id, &p2).await;
        run(&db, &ids).await;
        let after2 = db.get::<PlayerModel>("players", &p.open_id).await.unwrap();
        assert_eq!(after2.state["coins"]["1037"], 70, "mark read — no second reset");
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn item(type_id: i32, heirloom: bool) -> tgf::DItem {
        tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id, count: 1, guid: 100_000_000 + type_id as i64, ..Default::default() }),
            create_info: heirloom.then(|| tgf::DItemCreateInfo {
                create_data: Some(tgf::d_item_create_info::CreateData::Heirloom(tgf::DItemCreateInfoHeirloom::default())),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn resets_materials_relics_recipes_and_relic_items() {
        let mut p = PlayerModel::new("t", 10000001);
        p.state["coins"] = serde_json::json!({ "1037": 5000, "1053": 12, "1012": 7 });
        p.state["gold"] = serde_json::json!(900);
        p.state["playtest_materials"] = serde_json::json!(true);
        p.state["item_heirloom"] = serde_json::json!({ "unlock_heirloom_id": [210110120, 210110121], "unlock_gem_id": [240112000] });
        p.state["item_crafting_recipes"] = serde_json::json!([501]);
        p.stash.element = vec![item(181010000, false), item(555, true), item(777, false)];
        let ids: HashSet<i32> = [777].into_iter().collect();
        let r = reset_player(&mut p, &ids);
        assert_eq!(r, Reset { materials: 5012, relics_unlocked: 2, gems_unlocked: 1, recipes: 1, relic_items: 2 });
        assert_eq!(super::super::common::coin(&p, 1037), 0);
        assert_eq!(super::super::common::coin(&p, 1053), 0);
        assert_eq!(super::super::common::coin(&p, 1012), 7, "other currencies are not touched");
        assert_eq!(p.state["gold"].as_i64(), Some(900));
        assert_eq!(p.state["playtest_materials"].as_bool(), Some(true), "the bonus is not granted again");
        assert!(p.state["item_heirloom"].is_null() && p.state["item_crafting_recipes"].is_null());
        assert_eq!(p.stash.element.len(), 1);
        // second time — nothing to reset
        assert!(!reset_player(&mut p, &ids).any());
    }
}
