//! A new character's starting gear from the DB: table `starter_loadout`, one record per class (`starter_loadout:<role_type_id>`).
//! If there is a record and `enabled = true` — role_create gives exactly that (without upgrade_starter_item_id); none or disabled —
//! as before, from ExcelRoleDefaultEquipment (the class's create_default_item_config_ids).
//!
//! Class template (`class_template`) — starting gear from ExcelRoleDefaultEquipment plus jewelry (rings and
//! a necklace for the class's main stats, `JEWELRY`). The `starter_loadout` record is built from it (tier
//! `STARTER_TIER`).
//! Tier — the fourth digit from the end (181010000 grey → 181014000 purple); if the item has no such tier — the nearest lower one
//! (regular jewelry 185xxxxxx has tiers 1–4).
//!
//! At startup `seed` creates missing records and updates its own (their `note` has `SEED_MARK`) to the current template.
//! A record edited by an admin (SQL) is not touched by seed if the `(seed:` mark is removed from `note` — that way the edit survives
//! a restart. Affects only new characters; already created ones do not change.

use crate::db::GameDb;
use crate::game_data::GameData;
use serde::{Deserialize, Serialize};

pub const TABLE: &str = "starter_loadout";
/// Starting gear tier (thousands digit in type_id): 2 green, 3 blue, 4 purple, 5 legendary
pub const STARTER_TIER: i32 = 4;
/// Mark of a record maintained by seed (updated at startup); without it the record is manual
const SEED_MARK: &str = "(seed: ";

/// Class jewelry: (ring for both hands, necklace) — regular jewelry 1850[12]T00V, tiers 1–4 (common … purple).
/// Variant V — stat of the MAJOR affix: 0 strength, 1 agility, 2 stamina, 3 intellect, 4 will, 5 faith.
/// Not 195xxxxxx: that is mithril-mode jewelry, it has only tiers 1–2.
const JEWELRY: &[(i32, i32, i32)] = &[
    (400001, 185011000, 185021001), // swordsman: strength / agility
    (400002, 185011000, 185021002), // warrior: strength / stamina
    (400003, 185011005, 185021004), // priest: faith / will
    (400004, 185011001, 185021000), // rogue: agility / strength
    (400005, 185011003, 185021004), // wizard: intellect / will
    (400006, 185011000, 185021002), // death knight: strength / stamina
    (400007, 185011003, 185021004), // lich: intellect / will
    (400008, 185011001, 185021005), // druid: agility / faith
];
const LEFT_RING: i32 = 17;
const RIGHT_RING: i32 = 18;
const NECKLACE: i32 = 16;

const SCHEMA: &str = r#"
DEFINE TABLE OVERWRITE starter_loadout SCHEMAFULL;
DEFINE FIELD OVERWRITE role_type_id ON TABLE starter_loadout TYPE int;
DEFINE FIELD OVERWRITE enabled ON TABLE starter_loadout TYPE bool DEFAULT true;
DEFINE FIELD OVERWRITE note ON TABLE starter_loadout TYPE string DEFAULT "";
DEFINE FIELD OVERWRITE wearing ON TABLE starter_loadout TYPE array<object> DEFAULT [];
DEFINE FIELD OVERWRITE wearing[*].item_id ON TABLE starter_loadout TYPE int ASSERT $value > 0;
DEFINE FIELD OVERWRITE wearing[*].count ON TABLE starter_loadout TYPE int DEFAULT 1 ASSERT $value >= 1;
DEFINE FIELD OVERWRITE wearing[*].wear_part ON TABLE starter_loadout TYPE int
    ASSERT $value IN [1, 2, 3, 4, 11, 12, 13, 14, 15, 16, 17, 18, 41, 42, 43, 51, 101, 102, 103, 104, 105, 106, 107, 108, 109];
DEFINE FIELD OVERWRITE backpack ON TABLE starter_loadout TYPE array<object> DEFAULT [];
DEFINE FIELD OVERWRITE backpack[*].item_id ON TABLE starter_loadout TYPE int ASSERT $value > 0;
DEFINE FIELD OVERWRITE backpack[*].count ON TABLE starter_loadout TYPE int DEFAULT 1 ASSERT $value >= 1;
DEFINE FIELD OVERWRITE backpack[*].wear_part ON TABLE starter_loadout TYPE int DEFAULT 0;
DEFINE INDEX OVERWRITE idx_starter_role_type ON TABLE starter_loadout COLUMNS role_type_id UNIQUE;
"#;

/// Loadout item. `wear_part` — EnItemWearingPart slot (1 right hand … 51 throwable); in the backpack — 0
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct StarterItem {
    pub item_id: i32,
    #[serde(default = "one")]
    pub count: i32,
    #[serde(default)]
    pub wear_part: i32,
}

fn one() -> i32 {
    1
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StarterLoadout {
    pub role_type_id: i32,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub wearing: Vec<StarterItem>,
    #[serde(default)]
    pub backpack: Vec<StarterItem>,
}

fn yes() -> bool {
    true
}

/// Table schema (re-running is safe — OVERWRITE)
pub async fn setup(db: &GameDb) {
    match db.db.query(SCHEMA).await.and_then(|r| r.check()) {
        Ok(_) => println!("[Starter] schema {}: ready", TABLE),
        Err(e) => eprintln!("[Starter] schema {}: {}", TABLE, e),
    }
}

/// The same item in the needed tier; if there is none — the nearest lower tier (variant suffix — its own, then 0, 1, 2).
/// Not gear (torch, consumables) — unchanged.
pub fn item_at_tier(gd: &GameData, base: i32, tier: i32) -> i32 {
    if base < 180_000_000 {
        return base;
    }
    let prefix = (base / 10_000) * 10_000;
    let suffix = base % 1000;
    for t in (0..=tier.clamp(0, 9)).rev() {
        for suf in [suffix, 0, 1, 2] {
            let cand = prefix + t * 1000 + suf;
            if gd.item_type.contains_key(&cand) {
                return cand;
            }
        }
    }
    base
}

/// Class template: (equipped, backpack) from ExcelRoleDefaultEquipment + JEWELRY in free slots.
/// Tiers — as in the game table. None — the class is not in role_type.
pub fn class_template(gd: &GameData, role_type_id: i32) -> Option<(Vec<StarterItem>, Vec<StarterItem>)> {
    let cfg = gd.role_type.get(&role_type_id)?;
    let (mut wearing, mut backpack) = (Vec::new(), Vec::new());
    for id in &cfg.create_default_item_config_ids {
        let Some(eq) = gd.role_default_equipment.get(id) else { continue };
        let count = eq.count.max(1);
        // like generate_starter_items: equipped if the storage is RoleWearing (3) or a slot is given
        if eq.storage_type == 3 || eq.wear_part > 0 {
            wearing.push(StarterItem { item_id: eq.item_id, count, wear_part: eq.wear_part });
        } else {
            backpack.push(StarterItem { item_id: eq.item_id, count, wear_part: 0 });
        }
    }
    if let Some(&(_, ring, neck)) = JEWELRY.iter().find(|j| j.0 == role_type_id) {
        for (wear_part, item_id) in [(LEFT_RING, ring), (RIGHT_RING, ring), (NECKLACE, neck)] {
            if gd.item_type.contains_key(&item_id) && !wearing.iter().any(|i| i.wear_part == wear_part) {
                wearing.push(StarterItem { item_id, count: 1, wear_part });
            }
        }
    }
    Some((wearing, backpack))
}

/// Class loadout for starter_loadout: template, equipped — of tier `tier`, backpack — as in the table.
/// None — the class has no starting items or none was raised to the tier (mithril, dummy).
pub fn loadout_at_tier(gd: &GameData, role_type_id: i32, tier: i32) -> Option<StarterLoadout> {
    let (wearing, backpack) = class_template(gd, role_type_id)?;
    let mut upgraded = 0;
    let wearing = wearing
        .into_iter()
        .map(|i| {
            let item_id = item_at_tier(gd, i.item_id, tier);
            upgraded += (item_id != i.item_id) as usize;
            StarterItem { item_id, ..i }
        })
        .collect();
    (upgraded > 0).then(|| StarterLoadout {
        role_type_id,
        // 08.11 user decision: starting gear is the grey one from ExcelRoleDefaultEquipment; seed's own records are disabled
        enabled: false,
        note: format!("class template, tier {} {}ExcelRoleDefaultEquipment + jewelry)", tier, SEED_MARK),
        wearing,
        backpack,
    })
}

/// Missing records for creatable classes (environment = AllAvailable); does not touch existing ones
pub async fn seed(db: &GameDb, gd: &GameData) {
    let mut ids: Vec<i32> = gd
        .role_type
        .iter()
        .filter(|(_, r)| r.environment == crate::tgf::EnEnvironmentType::AllAvailable as i32)
        .map(|(id, _)| *id)
        .collect();
    ids.sort();
    let (mut created, mut updated) = (Vec::new(), Vec::new());
    for id in ids {
        let Some(lo) = loadout_at_tier(gd, id, STARTER_TIER) else { continue };
        match db.get::<StarterLoadout>(TABLE, &id.to_string()).await {
            None => created.push(id),
            // own record — to the current template and disabled (08.11); a manual one — not touched
            Some(old) if old.note.contains(SEED_MARK) => {
                if old.enabled == lo.enabled && old.wearing == lo.wearing && old.backpack == lo.backpack && old.note == lo.note {
                    continue;
                }
                updated.push(id);
            }
            Some(_) => continue,
        }
        db.save(TABLE, &id.to_string(), &lo).await;
    }
    if !created.is_empty() || !updated.is_empty() {
        println!("[Starter] {}: class loadouts created {:?}, updated {:?}", TABLE, created, updated);
    }
}

/// The class's enabled loadout from the DB
pub async fn get(db: &GameDb, role_type_id: i32) -> Option<StarterLoadout> {
    db.get::<StarterLoadout>(TABLE, &role_type_id.to_string()).await.filter(|l| l.enabled)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real SurrealKV in a temp folder: seed from the game tables, read, the schema rejects a wrong slot
    #[tokio::test]
    async fn seed_template_and_schema() {
        let gd = GameData::load_all("ExcelConfig");
        let dir = format!("target/test_starter_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        setup(&db).await;
        seed(&db, &gd).await;

        // swordsman 400001: right hand 181020000 → 181024000 (purple), helmet 183010000 → 183014000,
        // throwable unchanged, jewelry of the same tier: strength rings, agility necklace
        // 08.11: seed records are created disabled — role_create gives the grey gear from ExcelRoleDefaultEquipment
        assert!(get(&db, 400001).await.is_none(), "seed disabled");
        let lo = db.get::<StarterLoadout>(TABLE, "400001").await.expect("swordsman loadout");
        assert!(!lo.enabled);
        assert!(lo.wearing.contains(&StarterItem { item_id: 181024000, count: 1, wear_part: 1 }), "{:?}", lo.wearing);
        assert!(lo.wearing.contains(&StarterItem { item_id: 183014000, count: 1, wear_part: 13 }));
        assert!(lo.wearing.iter().any(|i| i.wear_part == 51 && i.item_id == 100012));
        assert!(lo.wearing.contains(&StarterItem { item_id: 185014000, count: 1, wear_part: 17 }));
        assert!(lo.wearing.contains(&StarterItem { item_id: 185014000, count: 1, wear_part: 18 }));
        assert!(lo.wearing.contains(&StarterItem { item_id: 185024001, count: 1, wear_part: 16 }));
        // jewelry follows the stage tier: casual green, normal blue, hard purple
        assert_eq!(item_at_tier(&gd, 185011003, 2), 185012003);
        assert_eq!(item_at_tier(&gd, 185011003, 3), 185013003);
        assert_eq!(item_at_tier(&gd, 185021004, 4), 185024004);
        assert_eq!(item_at_tier(&gd, 181010000, 3), 181013000);
        // all 8 classes have jewelry
        for id in 400001..=400008 {
            let lo = db.get::<StarterLoadout>(TABLE, &id.to_string()).await.expect("class loadout");
            assert_eq!(lo.wearing.iter().filter(|i| (16..=18).contains(&i.wear_part)).count(), 3, "class {}", id);
        }
        assert!(get(&db, 499999).await.is_none(), "dummy (OnlyEdit) not seeded");

        // own record (even one enabled manually via enabled) is updated to the template and disabled
        let mut stale = lo.clone();
        stale.enabled = true;
        stale.wearing.retain(|i| i.wear_part != 16);
        db.save(TABLE, "400001", &stale).await;
        seed(&db, &gd).await;
        assert!(get(&db, 400001).await.is_none(), "enabled = false kept");
        let refreshed = db.get::<StarterLoadout>(TABLE, "400001").await.unwrap();
        assert_eq!(refreshed.wearing, lo.wearing, "necklace restored from the template");

        // a manual edit (no seed mark in note) survives a repeated seed
        let mut manual = db.get::<StarterLoadout>(TABLE, "400002").await.unwrap();
        manual.note = "manual".into();
        manual.wearing.retain(|i| i.wear_part != 16);
        db.save(TABLE, "400002", &manual).await;
        seed(&db, &gd).await;
        let kept = db.get::<StarterLoadout>(TABLE, "400002").await.unwrap();
        assert_eq!(kept.wearing, manual.wearing);

        // SCHEMAFULL: slot 99 fails the ASSERT
        let bad = db
            .db
            .query("CREATE starter_loadout:1 CONTENT { role_type_id: 1, wearing: [{ item_id: 181015000, wear_part: 99 }] }")
            .await
            .and_then(|r| r.check());
        assert!(bad.is_err(), "wrong wear_part accepted");
    }
}
