//! Loot template (`loot/loot.toml`) without the game schema: rules, pool building, average drop and the template as three tables.
//! `xtask/src/loot.rs` builds the game tables from it with the same functions; `cargo xtask loot verify-view` cross-checks the result.
//!
//! How loot works: source (chest, monster) -> slots -> each slot drops independently with a chance -> picks a pool ->
//! one item drops from the pool by weights.

use std::collections::HashMap;

use serde_json::{json, Map, Value as Json};

use crate::values::Value;

pub const POOL_TOTAL: i64 = 30000; // sum of pool weights in all game tables
pub const RATE_MAX: i64 = 10000; // slot chance: 10000 = 100 %

pub const TEMPLATE_FORMAT: &str = "dbbalance-loot/1";

/// Tables the template is shown as in the kit
pub const SLOTS: &str = "lt_slots";
pub const ITEMS: &str = "lt_items";
pub const RULES: &str = "lt_rules";

pub fn is_template_table(name: &str) -> bool {
    matches!(name, SLOTS | ITEMS | RULES)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Cat {
    Gold,
    Weapon,
    Armor,
    Jewelry,
    Gem,
    Treasure,
    Potion,
    Throwable,
    Supply,
    Currency,
    Other,
}

/// (category, code, in Russian, in English)
pub const CATS: [(Cat, &str, &str, &str); 11] = [
    (Cat::Gold, "gold", "золото", "gold"),
    (Cat::Weapon, "weapon", "оружие", "weapons"),
    (Cat::Armor, "armor", "броня", "armor"),
    (Cat::Jewelry, "jewelry", "украшения", "accessories"),
    (Cat::Gem, "gem", "самоцветы", "gems"),
    (Cat::Treasure, "treasure", "сокровища", "treasure"),
    (Cat::Potion, "potion", "зелья", "potions"),
    (Cat::Throwable, "throwable", "метательное", "throwables"),
    (Cat::Supply, "supply", "расходники", "supplies"),
    (Cat::Currency, "currency", "валюта", "currency"),
    (Cat::Other, "other", "прочее", "other"),
];

/// Category groups for rules
pub const CAT_GROUPS: [(&str, &[Cat]); 3] = [
    ("equipment", &[Cat::Weapon, Cat::Armor, Cat::Jewelry]),
    ("consumable", &[Cat::Potion, Cat::Throwable, Cat::Supply]),
    ("all", &[Cat::Gold, Cat::Weapon, Cat::Armor, Cat::Jewelry, Cat::Gem, Cat::Treasure, Cat::Potion, Cat::Throwable, Cat::Supply, Cat::Currency, Cat::Other]),
];

pub fn cat_code(c: Cat) -> &'static str {
    CATS.iter().find(|x| x.0 == c).map_or("other", |x| x.1)
}

pub fn cat_ru(c: Cat) -> &'static str {
    CATS.iter().find(|x| x.0 == c).map_or("прочее", |x| x.2)
}

pub fn cat_en(c: Cat) -> &'static str {
    CATS.iter().find(|x| x.0 == c).map_or("other", |x| x.3)
}

pub fn cat_from_code(code: &str) -> Cat {
    CATS.iter().find(|x| x.1 == code).map_or(Cat::Other, |x| x.0)
}

/// Whether a category is in a rule key (code, Russian name or group). None — unknown key.
pub fn rule_matches(key: &str, c: Cat) -> Option<bool> {
    if let Some(x) = CATS.iter().find(|x| x.1 == key || x.2 == key) {
        return Some(x.0 == c);
    }
    CAT_GROUPS.iter().find(|g| g.0 == key).map(|g| g.1.contains(&c))
}

pub fn int(v: &Json) -> i64 {
    v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)).unwrap_or(0)
}

#[derive(Clone, Debug, Default)]
pub struct Rules {
    pub remove_cats: Vec<String>,
    pub remove_ids: std::collections::HashSet<i64>,
    pub chance: Vec<(String, f64)>,
    pub weight: Vec<(String, f64)>,
    pub count: Vec<(String, f64)>,
    /// drop quality: weight of a priced item × (price / geometric mean price in the pool) ^ quality; 0 — no change
    pub quality: f64,
}

impl Rules {
    /// Mode rules on top of [rules]: remove lists are merged, multipliers are multiplied, quality is added
    pub fn with(&self, o: &Rules) -> Rules {
        let mut r = self.clone();
        r.remove_cats.extend(o.remove_cats.iter().cloned());
        r.remove_ids.extend(o.remove_ids.iter().copied());
        r.chance.extend(o.chance.iter().cloned());
        r.weight.extend(o.weight.iter().cloned());
        r.count.extend(o.count.iter().cloned());
        r.quality += o.quality;
        r
    }

    pub fn removes(&self, item: i64, c: Cat) -> bool {
        self.remove_ids.contains(&item) || self.remove_cats.iter().any(|k| rule_matches(k, c) == Some(true))
    }

    /// at — section name for errors ("rules" / "modes.x"); extra — allowed fields besides the rules
    pub fn parse(v: &Json, at: &str, extra: &[&str], errors: &mut Vec<String>) -> Rules {
        let mut r = Rules::default();
        for k in v.as_object().into_iter().flatten().map(|(k, _)| k) {
            if !["remove", "chance", "weight", "count", "quality"].contains(&k.as_str()) && !extra.contains(&k.as_str()) {
                errors.push(format!("[{}]: unknown field \"{}\"", at, k));
            }
        }
        let check = |k: &str, errors: &mut Vec<String>| {
            if rule_matches(k, Cat::Other).is_none() {
                errors.push(format!("[{}]: unknown category \"{}\"", at, k));
            }
        };
        for x in v["remove"].as_array().into_iter().flatten() {
            match x {
                Json::Number(_) => {
                    r.remove_ids.insert(int(x));
                }
                Json::String(s) => {
                    check(s, errors);
                    r.remove_cats.push(s.clone());
                }
                _ => errors.push(format!("[{}] remove: {} — a category or an id is required", at, x)),
            }
        }
        for (name, dst) in [("chance", &mut r.chance), ("weight", &mut r.weight), ("count", &mut r.count)] {
            for (k, m) in v[name].as_object().into_iter().flatten() {
                check(k, errors);
                match m.as_f64() {
                    Some(f) if f >= 0.0 => dst.push((k.clone(), f)),
                    _ => errors.push(format!("[{}] {}.{}: a multiplier ≥ 0 is required", at, name, k)),
                }
            }
        }
        match &v["quality"] {
            Json::Null => {}
            q => match q.as_f64() {
                Some(f) => r.quality = f,
                None => errors.push(format!("[{}] quality: a number is required", at)),
            },
        }
        r
    }
}

pub fn mult(list: &[(String, f64)], c: Cat) -> f64 {
    list.iter().filter(|(k, _)| rule_matches(k, c) == Some(true)).map(|(_, v)| *v).product()
}

/// Weights -> integers summing to POOL_TOTAL (largest remainder method)
pub fn normalize(w: &[f64]) -> Vec<i64> {
    let total: f64 = w.iter().sum();
    // Huge weights (quality from a designer's edit) must not give infinity in the sum
    let (w, total): (Vec<f64>, f64) = if total.is_finite() {
        (w.to_vec(), total)
    } else {
        let top = w.iter().copied().fold(f64::MIN_POSITIVE, f64::max);
        let scaled: Vec<f64> = w.iter().map(|x| x / top).collect();
        let sum = scaled.iter().sum();
        (scaled, sum)
    };
    let exact: Vec<f64> = w.iter().map(|x| x * POOL_TOTAL as f64 / total).collect();
    let mut out: Vec<i64> = exact.iter().map(|x| x.floor() as i64).collect();
    let mut rest = POOL_TOTAL - out.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..w.len()).collect();
    order.sort_by(|&a, &b| (exact[b] - exact[b].floor()).partial_cmp(&(exact[a] - exact[a].floor())).unwrap_or(std::cmp::Ordering::Equal).then(a.cmp(&b)));
    for i in order {
        if rest <= 0 {
            break;
        }
        out[i] += 1;
        rest -= 1;
    }
    out
}

/// Pool rows: (item, count, level, weight)
pub type PoolRow = (i64, i64, i64, i64);

/// One pool by the rules: (rows with weights summing to POOL_TOTAL, category shares, how many items were removed).
/// report — whether to report description errors (a pool is built for each mode, errors are needed once).
#[allow(clippy::too_many_arguments)]
pub fn build_pool(
    key: &str,
    p: &Json,
    rules: &Rules,
    resolve: &dyn Fn(&str) -> Result<i64, String>,
    cats: &HashMap<i64, Cat>,
    prices: &HashMap<i64, f64>,
    report: bool,
    errors: &mut Vec<String>,
) -> (Vec<PoolRow>, Vec<(Cat, f64)>, usize) {
    let mut err = |e: String| {
        if report {
            errors.push(e)
        }
    };
    let mut removed = 0;
    let mut raw: Vec<(i64, i64, i64, f64)> = Vec::new();
    for (n, it) in p["items"].as_array().into_iter().flatten().enumerate() {
        let at = format!("pools.{} items[{}]", key, n);
        for k in it.as_object().into_iter().flatten().map(|(k, _)| k) {
            if !["id", "cnt", "w", "level"].contains(&k.as_str()) {
                err(format!("{}: unknown field \"{}\"", at, k));
            }
        }
        let item = match &it["id"] {
            Json::Number(_) => int(&it["id"]),
            Json::String(name) => match name.parse::<i64>().or_else(|_| resolve(name)) {
                Ok(id) => id,
                Err(e) => {
                    err(format!("{}: {}", at, e));
                    continue;
                }
            },
            _ => {
                err(format!("{}: no id", at));
                continue;
            }
        };
        let Some(&c) = cats.get(&item) else {
            err(format!("{}: item {} is not in item_type", at, item));
            continue;
        };
        let cnts: Vec<i64> = match &it["cnt"] {
            Json::Null => vec![1],
            Json::Array(a) => a.iter().map(int).collect(),
            v => vec![int(v)],
        };
        if cnts.is_empty() {
            err(format!("{}: cnt = []", at));
            continue;
        }
        if rules.removes(item, c) {
            removed += 1;
            continue;
        }
        let w = it["w"].as_f64().unwrap_or(1.0) * mult(&rules.weight, c);
        let cm = mult(&rules.count, c);
        for cnt in cnts.iter() {
            let cnt = if cm == 1.0 { *cnt } else { ((*cnt as f64) * cm).round().max(1.0) as i64 };
            raw.push((item, cnt, it["level"].as_i64().unwrap_or(1), w / cnts.len() as f64));
        }
    }
    if rules.quality != 0.0 {
        // geometric mean price of the pool items (those that have a price)
        let priced: Vec<f64> = raw.iter().filter_map(|r| prices.get(&r.0)).map(|p| p.ln()).collect();
        if !priced.is_empty() {
            let gm = (priced.iter().sum::<f64>() / priced.len() as f64).exp();
            for r in raw.iter_mut() {
                if let Some(p) = prices.get(&r.0) {
                    r.3 *= (p / gm).powf(rules.quality);
                }
            }
        }
    }
    raw.retain(|r| r.3 > 0.0 && r.3.is_finite());
    let weights = if raw.is_empty() { vec![] } else { normalize(&raw.iter().map(|r| r.3).collect::<Vec<_>>()) };
    let rows: Vec<PoolRow> = raw.iter().zip(&weights).map(|(r, w)| (r.0, r.1, r.2, *w)).filter(|r| r.3 > 0).collect();
    let mut share: Vec<(Cat, f64)> = Vec::new();
    for r in &rows {
        let c = cats[&r.0];
        let f = r.3 as f64 / POOL_TOTAL as f64;
        match share.iter_mut().find(|x| x.0 == c) {
            Some(x) => x.1 += f,
            None => share.push((c, f)),
        }
    }
    (rows, share, removed)
}

// ------------------------------------------------------------------------------------- average drop

#[derive(Clone, Debug, PartialEq)]
pub struct SlotView {
    /// Slot number in the template group
    pub n: usize,
    /// Chance with the rules applied: RATE_MAX = 100 %
    pub rate: i64,
    /// (pool index in the view's `pools`, share)
    pub pools: Vec<(usize, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupView {
    /// `g001` or `g001@mode`, as in `loot show`
    pub key: String,
    pub group: String,
    pub sources: usize,
    pub maps: Vec<i64>,
    pub types: Vec<i64>,
    pub slots: Vec<SlotView>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PoolView {
    pub key: String,
    pub rows: Vec<PoolRow>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropView {
    pub pools: Vec<PoolView>,
    pub groups: Vec<GroupView>,
}

/// What drops from the template: the same steps as the table build in xtask, without the tables themselves
pub fn drop_view(doc: &Json, cats: &HashMap<i64, Cat>, prices: &HashMap<i64, f64>) -> DropView {
    let mut errors = Vec::new();
    let no_names = |n: &str| -> Result<i64, String> { Err(n.to_string()) };
    let rules = Rules::parse(&doc["rules"], "rules", &[], &mut errors);
    let mut modes: Vec<(String, Rules)> = Vec::new();
    let mut mode_of_map: HashMap<i64, usize> = HashMap::new();
    for (name, v) in doc["modes"].as_object().into_iter().flatten() {
        let r = Rules::parse(v, name, &["maps", "name"], &mut errors);
        for m in v["maps"].as_array().into_iter().flatten().map(int) {
            mode_of_map.insert(m, modes.len() + 1);
        }
        modes.push((name.clone(), rules.with(&r)));
    }
    let empty = Map::new();
    let pool_defs = doc["pools"].as_object().unwrap_or(&empty);
    let mut out = DropView::default();
    let mut shares: Vec<Vec<(Cat, f64)>> = Vec::new();
    let mut pool_ctx: HashMap<(usize, String), usize> = HashMap::new();
    for (key, p) in pool_defs {
        let (rows, share, _) = build_pool(key, p, &rules, &no_names, cats, prices, false, &mut errors);
        pool_ctx.insert((0, key.clone()), out.pools.len());
        out.pools.push(PoolView { key: key.clone(), rows });
        shares.push(share);
    }
    for (gkey, g) in doc["groups"].as_object().unwrap_or(&empty) {
        let list_or = |many: &str, one: &str| -> Vec<i64> {
            match g[many].as_array() {
                Some(a) if !a.is_empty() => a.iter().map(int).collect(),
                _ => vec![int(&g[one])],
            }
        };
        let (maps, types) = (list_or("maps", "map_id"), list_or("loot_types", "loot_type"));
        let mut parts: Vec<(usize, Vec<i64>)> = Vec::new();
        for &m in &maps {
            let ctx = mode_of_map.get(&m).copied().unwrap_or(0);
            match parts.iter_mut().find(|p| p.0 == ctx) {
                Some(p) => p.1.push(m),
                None => parts.push((ctx, vec![m])),
            }
        }
        parts.sort_by_key(|p| p.0);
        for (ctx, part_maps) in parts {
            let crules = if ctx == 0 { &rules } else { &modes[ctx - 1].1 };
            let mut info = GroupView {
                key: if ctx == 0 { gkey.clone() } else { format!("{}@{}", gkey, modes[ctx - 1].0) },
                group: gkey.clone(),
                sources: g["sources"].as_array().map_or(0, |a| a.len()),
                maps: part_maps,
                types: types.clone(),
                slots: vec![],
            };
            for (n, sl) in g["slots"].as_array().into_iter().flatten().enumerate() {
                let keys: Vec<Json> = if !sl["pool"].is_null() { vec![sl["pool"].clone()] } else { sl["pools"].as_array().cloned().unwrap_or_default() };
                if keys.is_empty() {
                    continue;
                }
                let pr: Vec<i64> = match sl["pr"].as_array() {
                    Some(a) => a.iter().map(int).collect(),
                    None if keys.len() == 1 => vec![100],
                    None => vec![1; keys.len()],
                };
                if pr.len() != keys.len() {
                    continue;
                }
                let mut picked: Vec<(usize, i64)> = Vec::new();
                for (k, w) in keys.iter().zip(&pr) {
                    let ks = match k {
                        Json::String(s) => s.clone(),
                        v => v.to_string(),
                    };
                    let Some(&base) = pool_ctx.get(&(0, ks.clone())) else { continue };
                    let i = match pool_ctx.get(&(ctx, ks.clone())) {
                        Some(&i) => i,
                        None => {
                            let (rows, share, _) = build_pool(&ks, &pool_defs[&ks], crules, &no_names, cats, prices, false, &mut errors);
                            let i = if rows == out.pools[base].rows {
                                base
                            } else {
                                out.pools.push(PoolView { key: ks.clone(), rows });
                                shares.push(share);
                                out.pools.len() - 1
                            };
                            pool_ctx.insert((ctx, ks.clone()), i);
                            i
                        }
                    };
                    if !out.pools[i].rows.is_empty() {
                        picked.push((i, *w));
                    }
                }
                if picked.is_empty() {
                    continue;
                }
                let pr_total: f64 = picked.iter().map(|p| p.1 as f64).sum();
                let sh: Vec<(usize, f64)> = picked.iter().map(|&(i, w)| (i, w as f64 / pr_total)).collect();
                let cmult: f64 = sh.iter().map(|&(i, s)| s * shares[i].iter().map(|&(c, f)| f * mult(&crules.chance, c)).sum::<f64>()).sum();
                let Some(chance) = sl["chance"].as_f64() else { continue };
                let rate = ((chance * 100.0 * cmult).round() as i64).min(RATE_MAX);
                if rate <= 0 {
                    continue;
                }
                info.slots.push(SlotView { n, rate, pools: sh });
            }
            out.groups.push(info);
        }
    }
    out
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DropSummary {
    pub items: f64,
    /// Average amount of gold: items of the "gold" category × count
    pub gold: f64,
    pub by_cat: Vec<(Cat, f64)>,
}

/// Average drop from one source of a group
pub fn group_summary(v: &DropView, g: &GroupView, cats: &HashMap<i64, Cat>) -> DropSummary {
    let mut s = DropSummary::default();
    for sl in &g.slots {
        let p = sl.rate as f64 / RATE_MAX as f64;
        for &(pi, sh) in &sl.pools {
            for &(item, cnt, _, w) in &v.pools[pi].rows {
                let f = p * sh * w as f64 / POOL_TOTAL as f64;
                let cat = *cats.get(&item).unwrap_or(&Cat::Other);
                s.items += f;
                if cat == Cat::Gold {
                    s.gold += f * cnt as f64;
                }
                match s.by_cat.iter_mut().find(|x| x.0 == cat) {
                    Some(x) => x.1 += f,
                    None => s.by_cat.push((cat, f)),
                }
            }
        }
    }
    s.by_cat.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    s
}

// ------------------------------------------------------------------------------------- template as tables

/// Labels for the template tables; the server has none and does not need them: the check looks only at values
pub struct Labels<'a> {
    pub item: &'a dyn Fn(i64) -> String,
    pub group: &'a dyn Fn(&str, &Json) -> String,
    pub pool: &'a dyn Fn(&str, &Json) -> String,
}

fn text_of(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

/// Pool label in English: the template has only Russian names
pub fn pool_label_en(key: &str, p: &Json, cats: &HashMap<i64, Cat>) -> String {
    let mut counts: Vec<(Cat, usize)> = Vec::new();
    for it in p["items"].as_array().into_iter().flatten() {
        let c = cats.get(&int(&it["id"])).copied().unwrap_or(Cat::Other);
        match counts.iter_mut().find(|x| x.0 == c) {
            Some(x) => x.1 += 1,
            None => counts.push((c, 1)),
        }
    }
    if counts.is_empty() {
        return key.to_string();
    }
    counts.sort_by_key(|x| std::cmp::Reverse(x.1));
    counts.iter().take(3).map(|(c, n)| format!("{} {}", cat_en(*c), n)).collect::<Vec<_>>().join(", ")
}

/// All rule keys: what can be multiplied
fn rule_keys() -> Vec<&'static str> {
    CATS.iter().map(|c| c.1).chain(CAT_GROUPS.iter().map(|g| g.0)).collect()
}

/// Field types of the template tables: generic path -> type
pub fn field_kinds(table: &str) -> Vec<(String, crate::values::FieldKind)> {
    use crate::values::FieldKind as F;
    match table {
        SLOTS => vec![("chance".into(), F::Float), ("pr[*]".into(), F::Int32)],
        ITEMS => vec![("w".into(), F::Float), ("cnt[*]".into(), F::Int32)],
        RULES => {
            let mut out = vec![("quality".to_string(), F::Float)];
            for section in ["chance", "weight", "count"] {
                out.extend(rule_keys().into_iter().map(|k| (format!("{}.{}", section, k), F::Float)));
            }
            out
        }
        _ => vec![],
    }
}

fn value(row: usize, key: &str, title: &str, path: &str, v: String, name: &str) -> Value {
    Value { row, key: key.to_string(), title: title.to_string(), path: path.to_string(), value: v, name: name.to_string() }
}

/// The template as long-form values: (table, values). Row order follows the order in the template.
pub fn template_tables(doc: &Json, labels: &Labels) -> Vec<(&'static str, Vec<Value>)> {
    let empty = Map::new();
    let mut slots = Vec::new();
    let mut row = 0;
    for (gkey, g) in doc["groups"].as_object().unwrap_or(&empty) {
        let title = (labels.group)(gkey, g);
        for sl in g["slots"].as_array().into_iter().flatten() {
            let mut push = |path: &str, v: String, name: &str| slots.push(value(row, gkey, &title, path, v, name));
            if !sl["id"].is_null() {
                push("slot", text_of(&sl["id"]), "");
            }
            let pools: Vec<Json> = if !sl["pool"].is_null() { vec![sl["pool"].clone()] } else { sl["pools"].as_array().cloned().unwrap_or_default() };
            for (i, p) in pools.iter().enumerate() {
                let key = text_of(p);
                let pname = doc["pools"][&key].clone();
                push(&format!("pool[{}]", i), key.clone(), &(labels.pool)(&key, &pname));
            }
            for (i, w) in sl["pr"].as_array().into_iter().flatten().enumerate() {
                push(&format!("pr[{}]", i), text_of(w), "");
            }
            push("chance", text_of(&sl["chance"]), "");
            row += 1;
        }
    }
    let mut items = Vec::new();
    let mut row = 0;
    for (pkey, p) in doc["pools"].as_object().unwrap_or(&empty) {
        for it in p["items"].as_array().into_iter().flatten() {
            let id = int(&it["id"]);
            let title = (labels.item)(id);
            let pool_name = (labels.pool)(pkey, p);
            let mut push = |path: &str, v: String, name: &str| items.push(value(row, pkey, &title, path, v, name));
            push("item_id", id.to_string(), &title);
            push("pool", pkey.clone(), &pool_name);
            if !it["level"].is_null() {
                push("level", text_of(&it["level"]), "");
            }
            push("w", text_of(&it["w"]), "");
            match &it["cnt"] {
                Json::Array(a) => a.iter().enumerate().for_each(|(i, c)| push(&format!("cnt[{}]", i), text_of(c), "")),
                Json::Null => push("cnt[0]", "1".into(), ""),
                c => push("cnt[0]", text_of(c), ""),
            }
            row += 1;
        }
    }
    let mut rules = Vec::new();
    let mut sections: Vec<(String, &Json)> = vec![("rules".to_string(), &doc["rules"])];
    for (name, m) in doc["modes"].as_object().unwrap_or(&empty) {
        sections.push((format!("modes.{}", name), m));
    }
    for (row, (key, sec)) in sections.iter().enumerate() {
        let title = if key == "rules" { String::new() } else { text_of(&sec["name"]) };
        let mut push = |path: &str, v: String| rules.push(value(row, key, &title, path, v, ""));
        let list: Vec<String> = sec["remove"].as_array().into_iter().flatten().map(text_of).collect();
        push("remove", list.join(", "));
        let maps: Vec<String> = sec["maps"].as_array().into_iter().flatten().map(text_of).collect();
        push("maps", maps.join(", "));
        for section in ["chance", "weight", "count"] {
            for k in rule_keys() {
                push(&format!("{}.{}", section, k), text_of(&sec[section][k]));
            }
        }
        // 0 and empty mean the same, and the template has both
        let q = text_of(&sec["quality"]);
        push("quality", if crate::values::as_number(&q) == Some(0.0) { String::new() } else { q });
    }
    vec![(SLOTS, slots), (ITEMS, items), (RULES, rules)]
}

/// Where a table value lands in the template
#[derive(Clone, Debug, PartialEq)]
pub enum Place {
    SlotChance { group: String, slot: usize },
    SlotPr { group: String, slot: usize, i: usize },
    ItemW { pool: String, item: usize },
    ItemCnt { pool: String, item: usize, i: usize },
    Rule { section: Vec<String>, kind: String, key: Option<String> },
}

fn index_of(path: &str, prefix: &str) -> Option<usize> {
    path.strip_prefix(prefix)?.strip_prefix('[')?.strip_suffix(']')?.parse().ok()
}

/// `n` — index among the rows with the same `key`
pub fn locate(table: &str, key: &str, n: usize, path: &str) -> Option<Place> {
    match table {
        SLOTS if path == "chance" => Some(Place::SlotChance { group: key.to_string(), slot: n }),
        SLOTS => Some(Place::SlotPr { group: key.to_string(), slot: n, i: index_of(path, "pr")? }),
        ITEMS if path == "w" => Some(Place::ItemW { pool: key.to_string(), item: n }),
        ITEMS => Some(Place::ItemCnt { pool: key.to_string(), item: n, i: index_of(path, "cnt")? }),
        RULES => {
            let section: Vec<String> = key.split('.').map(str::to_string).collect();
            match path.split_once('.') {
                Some((kind, k)) => Some(Place::Rule { section, kind: kind.to_string(), key: Some(k.to_string()) }),
                None => Some(Place::Rule { section, kind: path.to_string(), key: None }),
            }
        }
        _ => None,
    }
}

/// Text -> template number: an integer stays an integer, a comma works as a dot. `Null` — not a number.
pub fn number_json(text: &str) -> Json {
    match text.trim().parse::<i64>() {
        Ok(i) => json!(i),
        Err(_) => crate::values::as_number(text).map_or(Json::Null, |f| json!(f)),
    }
}

/// Object field for writing. The kit is a foreign file: a non-object here must not crash the app.
fn field<'a>(v: &'a mut Json, key: &str) -> Option<&'a mut Json> {
    if v.is_null() {
        *v = json!({});
    }
    Some(v.as_object_mut()?.entry(key).or_insert(Json::Null))
}

fn existing<'a>(v: &'a mut Json, key: &str) -> Option<&'a mut Json> {
    v.as_object_mut()?.get_mut(key)
}

fn slot_of<'a>(doc: &'a mut Json, group: &str, slot: usize) -> Option<&'a mut Json> {
    existing(existing(existing(doc, "groups")?, group)?, "slots")?.get_mut(slot).filter(|s| s.is_object())
}

fn item_of<'a>(doc: &'a mut Json, pool: &str, item: usize) -> Option<&'a mut Json> {
    existing(existing(existing(doc, "pools")?, pool)?, "items")?.get_mut(item).filter(|s| s.is_object())
}

/// Value in the template (JSON): for computing the drop with the designer's edits. false — no such place or not a number.
pub fn apply_to_doc(doc: &mut Json, place: &Place, text: &str) -> bool {
    let v = number_json(text);
    if v.is_null() {
        return false;
    }
    match place {
        Place::SlotChance { group, slot } => match slot_of(doc, group, *slot) {
            Some(s) => {
                s["chance"] = v;
                true
            }
            None => false,
        },
        Place::SlotPr { group, slot, i } => match slot_of(doc, group, *slot).and_then(|s| s["pr"].get_mut(*i)) {
            Some(x) => {
                *x = v;
                true
            }
            None => false,
        },
        Place::ItemW { pool, item } => match item_of(doc, pool, *item) {
            Some(it) => {
                it["w"] = v;
                true
            }
            None => false,
        },
        Place::ItemCnt { pool, item, i } => {
            let Some(it) = item_of(doc, pool, *item) else { return false };
            let mut list: Vec<Json> = match &it["cnt"] {
                Json::Array(a) => a.clone(),
                Json::Null => vec![json!(1)],
                c => vec![c.clone()],
            };
            if *i < list.len() {
                list[*i] = v;
            } else if *i == list.len() {
                list.push(v);
            } else {
                return false;
            }
            it["cnt"] = if list.len() == 1 { list.remove(0) } else { Json::Array(list) };
            true
        }
        Place::Rule { section, kind, key } => {
            let mut cur = &mut *doc;
            for s in section {
                let Some(next) = field(cur, s) else { return false };
                cur = next;
            }
            let Some(slot) = field(cur, kind) else { return false };
            match key {
                Some(k) => match field(slot, k) {
                    Some(x) => *x = v,
                    None => return false,
                },
                None => *slot = v,
            }
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cats() -> HashMap<i64, Cat> {
        [(1, Cat::Gold), (2, Cat::Weapon), (3, Cat::Treasure), (4, Cat::Potion)].into_iter().collect()
    }

    fn doc() -> Json {
        json!({
            "rules": {"remove": [], "chance": {}, "weight": {}, "count": {}, "quality": 0},
            "groups": {
                "g1": {"name": "Chest", "sources": ["a", "b"], "maps": [10101, 10102], "loot_types": [0], "slots": [
                    {"id": 91, "pool": 900, "chance": 50},
                    {"id": 92, "pool": 901, "chance": 100}
                ]},
                "g2": {"name": "Vase", "sources": ["c"], "slots": [{"pools": [900, 901], "pr": [70, 30], "chance": 10}]}
            },
            "pools": {
                "900": {"name": "Gold", "items": [{"id": 1, "cnt": [2, 4], "w": 30000}]},
                "901": {"name": "Mix", "items": [{"id": 2, "w": 10000}, {"id": 3, "w": 20000, "level": 2}, {"id": 4, "cnt": 5, "w": 0}]}
            }
        })
    }

    fn view(d: &Json) -> DropView {
        drop_view(d, &cats(), &HashMap::new())
    }

    #[test]
    fn average_drop() {
        let v = view(&doc());
        let g = &v.groups[0];
        assert_eq!((g.key.as_str(), g.sources), ("g1", 2));
        assert_eq!(g.maps, [10101, 10102]);
        let s = group_summary(&v, g, &cats());
        assert!((s.items - 1.5).abs() < 1e-9, "{s:?}");
        assert!((s.gold - 0.5 * 3.0).abs() < 1e-9, "gold: 50 % × the mean of 2 and 4: {s:?}");
        let treasure = s.by_cat.iter().find(|x| x.0 == Cat::Treasure).unwrap().1;
        assert!((treasure - 2.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn pool_choice_splits_by_pr() {
        let v = view(&doc());
        let g = v.groups.iter().find(|g| g.group == "g2").unwrap();
        assert_eq!(g.maps, [0], "without maps — map 0");
        assert_eq!(g.slots[0].rate, 1000);
        assert!((g.slots[0].pools[0].1 - 0.7).abs() < 1e-9);
    }

    #[test]
    fn rules_change_the_drop() {
        let mut d = doc();
        d["rules"]["remove"] = json!(["gold"]);
        let v = view(&d);
        assert_eq!(v.groups[0].slots.len(), 1, "the slot with an empty pool is gone");
        let mut d = doc();
        d["rules"]["chance"] = json!({"gold": 1.5});
        assert_eq!(view(&d).groups[0].slots[0].rate, 7500);
        d["rules"]["chance"] = json!({"gold": 3});
        assert_eq!(view(&d).groups[0].slots[0].rate, RATE_MAX, "chance not above 100 %");
    }

    #[test]
    fn a_mode_splits_a_group_by_map() {
        let mut d = doc();
        d["modes"] = json!({"no_gold": {"name": "No gold", "maps": [10102], "remove": ["gold"]}});
        let v = view(&d);
        let keys: Vec<(&str, Vec<i64>)> = v.groups.iter().filter(|g| g.group == "g1").map(|g| (g.key.as_str(), g.maps.clone())).collect();
        assert_eq!(keys, [("g1", vec![10101]), ("g1@no_gold", vec![10102])]);
        assert_eq!(v.groups[1].slots.len(), 1);
        assert_eq!(v.groups[1].slots[0].n, 1, "the slot number from the template is kept");
    }

    fn tables(d: &Json) -> Vec<(&'static str, Vec<Value>)> {
        let item = |id: i64| format!("item {}", id);
        let group = |k: &str, _: &Json| format!("group {}", k);
        let pool = |k: &str, _: &Json| format!("pool {}", k);
        template_tables(d, &Labels { item: &item, group: &group, pool: &pool })
    }

    #[test]
    fn template_as_tables() {
        let t = tables(&doc());
        let slots = &t[0].1;
        let get = |vals: &[Value], row: usize, path: &str| vals.iter().find(|v| v.row == row && v.path == path).map(|v| v.value.clone());
        assert_eq!(get(slots, 0, "chance").as_deref(), Some("50"));
        assert_eq!(get(slots, 1, "pool[0]").as_deref(), Some("901"));
        assert_eq!(get(slots, 2, "pr[1]").as_deref(), Some("30"));
        assert_eq!(slots[0].key, "g1");
        let items = &t[1].1;
        assert_eq!(get(items, 0, "cnt[1]").as_deref(), Some("4"));
        assert_eq!(get(items, 1, "cnt[0]").as_deref(), Some("1"), "without cnt — a single piece");
        assert_eq!(get(items, 3, "cnt[0]").as_deref(), Some("5"));
        assert_eq!(items[0].title, "item 1");
        let rules = &t[2].1;
        assert_eq!(get(rules, 0, "chance.gold").as_deref(), Some(""));
        assert_eq!(get(rules, 0, "quality").as_deref(), Some(""), "0 — the same as empty");
    }

    #[test]
    fn places() {
        assert_eq!(locate(SLOTS, "g1", 1, "chance"), Some(Place::SlotChance { group: "g1".into(), slot: 1 }));
        assert_eq!(locate(ITEMS, "900", 0, "cnt[1]"), Some(Place::ItemCnt { pool: "900".into(), item: 0, i: 1 }));
        assert_eq!(locate(RULES, "modes.x", 0, "weight.gold"), Some(Place::Rule { section: vec!["modes".into(), "x".into()], kind: "weight".into(), key: Some("gold".into()) }));
        assert_eq!(locate(RULES, "rules", 0, "quality"), Some(Place::Rule { section: vec!["rules".into()], kind: "quality".into(), key: None }));
        assert_eq!(locate("gear", "1", 0, "w"), None);
    }

    #[test]
    fn edits_reach_the_drop() {
        let mut d = doc();
        assert!(apply_to_doc(&mut d, &locate(SLOTS, "g1", 0, "chance").unwrap(), "25"));
        assert_eq!(view(&d).groups[0].slots[0].rate, 2500);
        assert!(apply_to_doc(&mut d, &locate(ITEMS, "901", 0, "w").unwrap(), "0"));
        assert_eq!(view(&d).pools[1].rows.len(), 1, "weight 0 removes the item");
        assert!(apply_to_doc(&mut d, &locate(ITEMS, "901", 2, "cnt[0]").unwrap(), "7"));
        assert_eq!(d["pools"]["901"]["items"][2]["cnt"], 7);
        assert!(apply_to_doc(&mut d, &locate(ITEMS, "900", 0, "cnt[2]").unwrap(), "9"));
        assert_eq!(d["pools"]["900"]["items"][0]["cnt"], json!([2, 4, 9]));
        assert!(!apply_to_doc(&mut d, &locate(ITEMS, "900", 0, "cnt[9]").unwrap(), "9"), "a hole in the list cannot be made");
        assert!(!apply_to_doc(&mut d, &locate(SLOTS, "g1", 9, "chance").unwrap(), "9"));
        assert!(apply_to_doc(&mut d, &locate(RULES, "rules", 0, "chance.gold").unwrap(), "2"));
        assert_eq!(d["rules"]["chance"]["gold"], 2);
    }
}
