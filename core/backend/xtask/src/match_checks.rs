//! Match search checks from match_checks.toml → the competition_level table (server and client pak).
//! Source tables are not changed: a copy of competition_level goes into a separate build folder.

use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const FILE: &str = crate::ws::MATCH_CHECKS;
const TABLE: &str = "competition_level";
const OUT: &str = "core/backend/target/xtask_checks"; // temporary tables (xtask runs from the workspace root)

struct Config {
    rarity_on: bool,
    rarity_levels: BTreeMap<String, Vec<String>>,
    score_on: bool,
    score_levels: BTreeMap<String, i64>,
    cost_on: bool,
}

impl Config {
    fn parse(text: &str) -> Result<Self, String> {
        let v: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
        let on = |section: &str| v.get(section).and_then(|s| s.get("enabled")).and_then(|e| e.as_bool()).unwrap_or(true);
        let levels = |section: &str| v.get(section).and_then(|s| s.get("levels")).and_then(|l| l.as_table()).cloned().unwrap_or_default();
        let mut rarity_levels = BTreeMap::new();
        for (id, list) in levels("gear_rarity") {
            let names = list.as_array().ok_or_else(|| format!("gear_rarity.levels.{}: a list is required", id))?;
            let names = names.iter().map(|n| n.as_str().map(str::to_string).ok_or_else(|| format!("gear_rarity.levels.{}: strings are required", id))).collect::<Result<_, _>>()?;
            rarity_levels.insert(id, names);
        }
        let mut score_levels = BTreeMap::new();
        for (id, n) in levels("gear_score") {
            score_levels.insert(id.clone(), n.as_integer().ok_or_else(|| format!("gear_score.levels.{}: a number is required", id))?);
        }
        Ok(Self { rarity_on: on("gear_rarity"), rarity_levels, score_on: on("gear_score"), score_levels, cost_on: on("level_cost") })
    }
}

/// A table row per the settings. Returns true if anything changed.
fn apply_row(c: &Config, row: &mut Value) -> bool {
    let id = row["competition_level_id"].as_i64().unwrap_or(0).to_string();
    let Some(obj) = row.as_object_mut() else { return false };
    let before = obj.clone();
    if !c.rarity_on {
        obj.remove("battle_gear_rarity_require");
    }
    if let Some(list) = c.rarity_levels.get(&id) {
        obj.insert("battle_gear_rarity_require".into(), json!({ "rarity": list }));
    }
    if !c.score_on {
        obj.remove("battle_affix_score_require");
    }
    if let Some(n) = c.score_levels.get(&id) {
        obj.insert("battle_affix_score_require".into(), json!(n));
    }
    if !c.cost_on {
        if let Some(d) = obj.get_mut("detail").and_then(|d| d.as_object_mut()) {
            d.remove("competition_costs");
        }
    }
    *obj != before
}

/// Table folder for the build: `src` as is or its copy with a changed competition_level
pub fn apply(src: &Path) -> Result<PathBuf, String> {
    let text = match std::fs::read_to_string(FILE) {
        Ok(t) => t,
        Err(_) => return Ok(src.to_path_buf()),
    };
    let cfg = Config::parse(&text).map_err(|e| format!("{}: {}", FILE, e))?;
    let table = src.join(format!("{}.json", TABLE));
    let raw = std::fs::read_to_string(&table).map_err(|e| format!("{}: {}", table.display(), e))?;
    let mut v: Value = serde_json::from_str(&raw).map_err(|e| format!("{}: {}", table.display(), e))?;
    let mut changed = 0;
    for row in v["rows"].as_array_mut().ok_or_else(|| format!("{}: no rows", table.display()))? {
        changed += apply_row(&cfg, row) as usize;
    }
    println!("{}: rows changed in {}: {}", FILE, TABLE, changed);
    if changed == 0 {
        return Ok(src.to_path_buf());
    }
    let out = Path::new(OUT);
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {}", out.display(), e))?;
    for (name, p) in crate::tables::list_tables(&src.to_string_lossy(), "json") {
        std::fs::copy(&p, out.join(format!("{}.json", name))).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    let body = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    std::fs::write(out.join(format!("{}.json", TABLE)), body).map_err(|e| e.to_string())?;
    Ok(out.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row() -> Value {
        json!({
            "competition_level_id": 40003017,
            "battle_gear_rarity_require": { "rarity": ["EN_ITEM_RARITY_EPIC"] },
            "detail": { "competition_costs": [{ "item_id": 1, "count": 1 }], "match_making_pool_id": 7 },
        })
    }

    #[test]
    fn rarity_off_removes_limit_only() {
        let c = Config::parse("[gear_rarity]\nenabled = false").unwrap();
        let mut r = row();
        assert!(apply_row(&c, &mut r));
        assert!(r.get("battle_gear_rarity_require").is_none());
        assert_eq!(r["detail"]["match_making_pool_id"], 7);
        assert_eq!(r["detail"]["competition_costs"][0]["count"], 1);
    }

    #[test]
    fn levels_set_values_and_defaults_keep_row() {
        let c = Config::parse("[gear_rarity]\nenabled = false\nlevels = { 40003017 = [\"EN_ITEM_RARITY_UNIQUE\"] }\n[gear_score]\nlevels = { 40003017 = 120 }").unwrap();
        let mut r = row();
        apply_row(&c, &mut r);
        assert_eq!(r["battle_gear_rarity_require"]["rarity"][0], "EN_ITEM_RARITY_UNIQUE");
        assert_eq!(r["battle_affix_score_require"], 120);
        let mut same = row();
        assert!(!apply_row(&Config::parse("").unwrap(), &mut same), "without settings the row does not change");
    }

    #[test]
    fn cost_off_removes_costs() {
        let c = Config::parse("[level_cost]\nenabled = false").unwrap();
        let mut r = row();
        apply_row(&c, &mut r);
        assert!(r["detail"].get("competition_costs").is_none());
        assert_eq!(r["detail"]["match_making_pool_id"], 7);
    }
}
