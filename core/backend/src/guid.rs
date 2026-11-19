//! Item guids. The client stores an item guid in 32 bits (signed), so the server issues guids from one shared
//! counter in the range [ITEM_GUID_START, i32::MAX] (the official server's guids ≈ 1.07·10⁸).
//! It used to be `user_id × 10000 + n` (≈ 10¹¹): the client truncated it to 32 bits and sent back a different guid
//! (role_wear / inventory_item_move "player does not have the item", trade_sales_order_publish -4212) — docs/FIX_LIST.md C1.
//!
//! The counter is in process memory (issuing is synchronous, from any handler), the high-water mark is in the DB `counters:item_guid`
//! with a BLOCK margin: after a restart counting continues from the mark, even if some issued guids were not saved in time.
//! `migrate` at startup rewrites old guids in all saves once (players, trade, mail, gifts).

use crate::db::GameDb;
use crate::structures::PlayerModel;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};

pub const ITEM_GUID_START: i64 = 100_000_000;
pub const ITEM_GUID_MAX: i64 = i32::MAX as i64;
const BLOCK: i64 = 10_000;
const TABLE: &str = "counters";
const COUNTER_ID: &str = "item_guid";
const MIGRATION_ID: &str = "item_guid_migration_v1";

static NEXT: AtomicI64 = AtomicI64::new(ITEM_GUID_START);
/// Before init there is no reserve (tests) — the keeper writes nothing
static RESERVED: AtomicI64 = AtomicI64::new(i64::MAX);

#[derive(Serialize, Deserialize, Clone)]
struct Counter {
    value: i64,
}

/// Next item guid (fits into the client's 32 bits)
pub fn next_item() -> i64 {
    let g = NEXT.fetch_add(1, Ordering::Relaxed);
    if g > ITEM_GUID_MAX {
        eprintln!("[Guid] item guid counter exceeded i32::MAX ({})", g);
    }
    g
}

/// A guid the client cannot hold: greater than i32::MAX (0 — "no guid", the caller decides)
pub fn too_big(guid: i64) -> bool {
    guid > ITEM_GUID_MAX
}

async fn store(db: &GameDb, id: &str, value: i64) {
    db.save(TABLE, id, &Counter { value }).await;
}

/// Counter from the DB high-water mark + a BLOCK reserve ahead. Call before accepting clients.
pub async fn init(db: &GameDb) {
    let hw = db.get::<Counter>(TABLE, COUNTER_ID).await.map(|c| c.value).unwrap_or(ITEM_GUID_START).max(ITEM_GUID_START);
    NEXT.store(hw, Ordering::Relaxed);
    store(db, COUNTER_ID, hw + BLOCK).await;
    RESERVED.store(hw + BLOCK, Ordering::Relaxed);
    println!("[Guid] item guids from {} (reserve up to {})", hw, hw + BLOCK);
}

/// Background: when more than half of the reserve has been issued — advance the mark in the DB
pub fn spawn_keeper(db: GameDb) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
            let (next, reserved) = (NEXT.load(Ordering::Relaxed), RESERVED.load(Ordering::Relaxed));
            if reserved != i64::MAX && next + BLOCK / 2 > reserved {
                let r = next + BLOCK;
                store(&db, COUNTER_ID, r).await;
                RESERVED.store(r, Ordering::Relaxed);
            }
        }
    });
}

// ---------------------------------------------------------------- migration of old guids

/// "Old guid → new" table for the whole run: the same item in different places (player, order, mail)
/// gets one new guid.
pub type GuidMap = HashMap<i64, i64>;

fn guid_key(k: &str) -> bool {
    k == "guid" || k.ends_with("_guid") || k == "guids" || k.ends_with("_guids")
}

fn remap_num(v: &mut Value, map: &mut GuidMap) -> usize {
    match v.as_i64() {
        Some(old) if too_big(old) => {
            *v = Value::from(*map.entry(old).or_insert_with(next_item));
            1
        }
        _ => 0,
    }
}

/// JSON walk: values of guid / *_guid keys (and guids / *_guids arrays) greater than i32::MAX → new guids.
/// Role guids and stash page guids (positions) are below the threshold and are not touched.
pub fn remap_json(v: &mut Value, map: &mut GuidMap) -> usize {
    let mut n = 0;
    match v {
        Value::Object(obj) => {
            for (k, x) in obj.iter_mut() {
                if guid_key(k) {
                    if let Value::Array(a) = x {
                        for e in a.iter_mut() {
                            n += remap_num(e, map);
                        }
                    } else {
                        n += remap_num(x, map);
                    }
                }
                n += remap_json(x, map);
            }
        }
        Value::Array(a) => {
            for x in a.iter_mut() {
                n += remap_json(x, map);
            }
        }
        _ => {}
    }
    n
}

/// The same for a protocol message (tgf::* with serde): via JSON and back. On error the message is unchanged.
pub fn remap_message<T: Serialize + DeserializeOwned>(m: &mut T, map: &mut GuidMap) -> usize {
    let Ok(mut v) = serde_json::to_value(&*m) else { return 0 };
    let n = remap_json(&mut v, map);
    if n > 0 {
        match serde_json::from_value::<T>(v) {
            Ok(x) => *m = x,
            Err(e) => {
                eprintln!("[Guid] failed to rebuild the message after replacing guids: {}", e);
                return 0;
            }
        }
    }
    n
}

/// A player's items: roles (equipped, backpacks), stash, state (merchant showcase, sold, relics, craft…)
pub fn remap_player(p: &mut PlayerModel, map: &mut GuidMap) -> usize {
    let mut n = 0;
    for r in p.roles.iter_mut() {
        n += remap_message(&mut r.data, map);
    }
    n += remap_message(&mut p.stash, map);
    n += remap_json(&mut p.state, map);
    n
}

/// One-time migration at startup (after init): all players + domain tables (crate::rpc::handlers::migrate_item_guids).
pub async fn migrate(db: &GameDb) {
    if db.get::<Counter>(TABLE, MIGRATION_ID).await.is_some() {
        return;
    }
    let players: Vec<PlayerModel> = match db.db.query("SELECT * FROM players").await {
        Ok(mut r) => r.take(0).unwrap_or_default(),
        Err(e) => {
            eprintln!("[Guid] migration: cannot read players: {}", e);
            return;
        }
    };
    let mut map = GuidMap::new();
    let mut changed = 0;
    let mut keys = Vec::new();
    for mut p in players {
        keys.push((p.open_id.clone(), p.user_id));
        let n = remap_player(&mut p, &mut map);
        if n > 0 {
            changed += 1;
            println!("[Guid] migration: {} — guids replaced: {}", p.open_id, n);
            db.save("players", &p.open_id, &p).await;
        }
    }
    let other = crate::rpc::handlers::migrate_item_guids(db, &keys, &mut map).await;
    store(db, MIGRATION_ID, 1).await;
    println!(
        "[Guid] migration v1: players {}, changed {}, {} replacements in domain tables, distinct old guids total {}",
        keys.len(),
        changed,
        other,
        map.len()
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remap_only_big_guids_consistently() {
        let mut map = GuidMap::new();
        let mut v = serde_json::json!({
            "basic": { "guid": 109490200139i64, "position": { "guid": 1094902001i64, "type": 2 } },
            "copy": { "item_guid": 109490200139i64 },
            "removed_guids": [109490200140i64, 5],
            "role_guid": 1094902001i64,
            "count": 109490200139i64
        });
        // basic.guid, copy.item_guid (same old → same new), removed_guids[0]
        assert_eq!(remap_json(&mut v, &mut map), 3);
        assert_eq!(map.len(), 2);
        let g = v["basic"]["guid"].as_i64().unwrap();
        assert!(g >= ITEM_GUID_START && g <= ITEM_GUID_MAX);
        assert_eq!(v["copy"]["item_guid"].as_i64(), Some(g), "one item — one new guid");
        assert_eq!(v["basic"]["position"]["guid"].as_i64(), Some(1094902001), "the role guid in a position is not touched");
        assert_eq!(v["role_guid"].as_i64(), Some(1094902001));
        assert_eq!(v["count"].as_i64(), Some(109490200139), "not a guid key");
        assert_eq!(v["removed_guids"][1].as_i64(), Some(5));
    }

    #[test]
    fn remap_proto_item() {
        let mut map = GuidMap::new();
        let mut item = crate::tgf::DItem {
            basic: Some(crate::tgf::DItemBasic { type_id: 181010000, count: 1, guid: 109490200139, ..Default::default() }),
            ..Default::default()
        };
        assert_eq!(remap_message(&mut item, &mut map), 1);
        assert!(!too_big(item.basic.as_ref().unwrap().guid));
    }
}
