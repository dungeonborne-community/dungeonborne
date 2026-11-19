//! The "Loot" screen: average drop by source and editing chances and weights right in the group. Values are edited in the usual
//! `lt_*` tables, so undo, draft, transfer and patch work as everywhere; here only the calculation and the choice of cells.

use std::collections::{BTreeSet, HashMap};

use serde_json::{json, Value as Json};

use crate::edit::Session;
use crate::kit::Kit;
use crate::loot::{self, cat_code, cat_en, cat_from_code, cat_ru, drop_view, group_summary, locate, pool_label_en, Cat, DropSummary, DropView, GroupView, PoolView, ITEMS, POOL_TOTAL, RATE_MAX, SLOTS};
use crate::view::KitView;

/// How many groups to list in the warning about a shared pool
const SHARED_SHOWN: usize = 6;

pub(crate) struct LootData {
    doc: Json,
    cats: HashMap<i64, Cat>,
    prices: HashMap<i64, f64>,
    /// Group labels: template key -> ru, en, kind, sources
    groups: Json,
    kinds: Vec<Json>,
    /// Which template groups use the pool
    pool_groups: HashMap<String, Vec<String>>,
    base: DropView,
}

impl LootData {
    pub(crate) fn load(kit: &Kit) -> Option<LootData> {
        if ![SLOTS, ITEMS].iter().all(|t| kit.table_names().iter().any(|n| n == t)) {
            return None;
        }
        let bytes = std::fs::read(kit.root.join("raw").join("loot_template.json")).ok()?;
        let file: Json = serde_json::from_slice(&bytes).ok()?;
        if file["format"].as_str() != Some(loot::TEMPLATE_FORMAT) {
            return None;
        }
        let cats: HashMap<i64, Cat> = file["cats"].as_object()?.iter().filter_map(|(k, v)| Some((k.parse().ok()?, cat_from_code(v.as_str()?)))).collect();
        let prices: HashMap<i64, f64> = file["prices"].as_object()?.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_f64()?))).collect();
        let doc = file["template"].clone();
        let mut pool_groups: HashMap<String, Vec<String>> = HashMap::new();
        for (g, def) in doc["groups"].as_object().into_iter().flatten() {
            let mut seen = BTreeSet::new();
            for sl in def["slots"].as_array().into_iter().flatten() {
                for p in slot_pools(sl) {
                    if seen.insert(p.clone()) {
                        pool_groups.entry(p).or_default().push(g.clone());
                    }
                }
            }
        }
        let base = drop_view(&doc, &cats, &prices);
        Some(LootData { cats, prices, groups: file["groups"].clone(), kinds: file["kinds"].as_array().cloned().unwrap_or_default(), pool_groups, base, doc })
    }
}

/// Pools of a template slot: keys as text
fn slot_pools(sl: &Json) -> Vec<String> {
    let text = |p: &Json| match p {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    };
    if sl["pool"].is_null() {
        sl["pools"].as_array().into_iter().flatten().map(text).collect()
    } else {
        vec![text(&sl["pool"])]
    }
}

fn pool_of<'a>(v: &'a DropView, slot: &crate::loot::SlotView, key: &str) -> Option<(&'a PoolView, f64)> {
    slot.pools.iter().find(|p| v.pools[p.0].key == key).map(|&(i, sh)| (&v.pools[i], sh))
}

/// Share of an item in the pool: 0…1 and the average count per drop
fn item_share(p: &PoolView, id: i64) -> (f64, f64) {
    let (mut w, mut n) = (0.0, 0.0);
    for r in p.rows.iter().filter(|r| r.0 == id) {
        w += r.3 as f64;
        n += (r.3 * r.1) as f64;
    }
    (w / POOL_TOTAL as f64, if w > 0.0 { n / w } else { 0.0 })
}

fn round6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}

fn summary_json(s: &DropSummary, en: bool) -> Json {
    json!({
        "items": round6(s.items),
        "gold": round6(s.gold),
        "cats": s.by_cat.iter().map(|(c, f)| json!({ "id": cat_code(*c), "label": if en { cat_en(*c) } else { cat_ru(*c) }, "value": round6(*f) })).collect::<Vec<_>>()
    })
}

fn rule_word(key: &str, en: bool, kit: &Kit) -> String {
    let fields = if en { &kit.config["fields_en"] } else { &kit.config["fields"] };
    fields[key].as_str().unwrap_or(key).to_string()
}

impl KitView {
    pub(crate) fn has_loot(&self) -> bool {
        self.loot.is_some()
    }

    /// The template with the designer's edits
    fn edited_doc(&self, data: &LootData, s: &Session) -> Json {
        let mut doc = data.doc.clone();
        for (&(ti, row, pid), text) in &s.edits {
            let t = &self.tables[ti];
            if !loot::is_template_table(&t.name) {
                continue;
            }
            let Some(r) = t.row_pos(row).map(|p| &t.rows[p]) else { continue };
            if let Some(place) = locate(&t.name, &r.key, r.n, &t.paths[pid as usize].path) {
                loot::apply_to_doc(&mut doc, &place, text);
            }
        }
        doc
    }

    fn map_label(&self, id: i64, en: bool) -> String {
        let lang = if en { "en" } else { "ru" };
        self.kit.catalog.as_ref().and_then(|c| c["maps"][id.to_string()][lang].as_str().map(str::to_string)).unwrap_or_else(|| id.to_string())
    }

    /// Template groups with the designer's edits: which of them the edit touches (directly or through a pool)
    fn edited_groups(&self, data: &LootData, s: &Session) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for &(ti, row, _) in s.edits.keys() {
            let t = &self.tables[ti];
            let Some(r) = t.row_pos(row).map(|p| &t.rows[p]) else { continue };
            if t.name == SLOTS {
                out.insert(r.key.clone());
            } else if t.name == ITEMS {
                out.extend(data.pool_groups.get(&r.key).into_iter().flatten().cloned());
            }
        }
        out
    }

    /// Group list on the left
    pub fn loot_groups(&self, q: &Json) -> Result<Json, String> {
        let data = self.loot.as_ref().ok_or("the kit has no loot template")?;
        let en = q["lang"].as_str() == Some("en");
        let lang = if en { "en" } else { "ru" };
        let map = q["map"].as_i64();
        let kind = q["kind"].as_str().unwrap_or_default();
        let only_mine = q["only_mine"].as_bool() == Some(true);
        let words: Vec<String> = q["search"].as_str().unwrap_or_default().to_lowercase().split_whitespace().map(str::to_string).collect();
        let s = self.session.lock().unwrap();
        let doc = self.edited_doc(data, &s);
        let view = drop_view(&doc, &data.cats, &data.prices);
        let base: HashMap<&str, &GroupView> = data.base.groups.iter().map(|g| (g.key.as_str(), g)).collect();
        let mine = self.edited_groups(data, &s);

        let mut maps: Vec<i64> = data.base.groups.iter().filter(|g| g.sources > 0).flat_map(|g| g.maps.iter().copied()).collect();
        maps.sort_unstable();
        maps.dedup();
        let mut counts: HashMap<&str, usize> = HashMap::new();
        let mut groups = Vec::new();
        for g in view.groups.iter().filter(|g| g.sources > 0 && map.is_none_or(|m| g.maps.contains(&m))) {
            let label = &data.groups[&g.group];
            let group_kind = label["kind"].as_str().unwrap_or("other");
            *counts.entry(group_kind).or_insert(0) += 1;
            if !kind.is_empty() && group_kind != kind {
                continue;
            }
            let is_mine = mine.contains(&g.group);
            if only_mine && !is_mine {
                continue;
            }
            let title = label[lang].as_str().unwrap_or(&g.group);
            let hay = format!("{} {} {}", title, g.group, label["sources"].as_array().into_iter().flatten().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(" ")).to_lowercase();
            if !words.iter().all(|w| hay.contains(w)) {
                continue;
            }
            let now = group_summary(&view, g, &data.cats);
            let was = base.get(g.key.as_str()).map_or_else(DropSummary::default, |b| group_summary(&data.base, b, &data.cats));
            groups.push(json!({
                "key": g.key, "title": title, "kind": group_kind, "sources": g.sources, "mine": is_mine,
                "items": round6(now.items), "items_was": round6(was.items), "gold": round6(now.gold), "gold_was": round6(was.gold),
                "mode": g.key.split_once('@').map(|x| x.1)
            }));
        }
        // What matters more for balance first: monsters and chests, not small pickups. The kit sets the order of kinds.
        let rank = |g: &Json| data.kinds.iter().position(|k| k["id"] == g["kind"]).unwrap_or(usize::MAX);
        groups.sort_by_key(rank);
        let kinds: Vec<Json> = data.kinds.iter().filter_map(|k| {
            let id = k["id"].as_str()?;
            counts.contains_key(id).then(|| json!({ "id": id, "label": k[lang], "count": counts[id] }))
        }).collect();
        Ok(json!({ "groups": groups, "kinds": kinds, "maps": maps.iter().map(|&m| json!({ "id": m, "label": self.map_label(m, en) })).collect::<Vec<_>>() }))
    }

    /// Slots, pools and items of one group
    pub fn loot_group(&self, key: &str, en: bool) -> Result<Json, String> {
        let data = self.loot.as_ref().ok_or("the kit has no loot template")?;
        let lang = if en { "en" } else { "ru" };
        let s = self.session.lock().unwrap();
        let doc = self.edited_doc(data, &s);
        let view = drop_view(&doc, &data.cats, &data.prices);
        let g = view.groups.iter().find(|g| g.key == key).ok_or(format!("group {} is not in the template", key))?;
        let was = data.base.groups.iter().find(|x| x.key == key);
        let (si, ii) = (self.table_index(SLOTS).ok_or("no slot table")?, self.table_index(ITEMS).ok_or("no item table")?);
        let (st, it) = (&self.tables[si], &self.tables[ii]);
        let index = |t: &'_ crate::view::TableView| -> HashMap<(String, usize), usize> { t.rows.iter().map(|r| ((r.key.clone(), r.n), r.row)).collect() };
        let (slot_rows, item_rows) = (index(st), index(it));
        let label = &data.groups[&g.group];
        let group_title = |gk: &str| data.groups[gk][lang].as_str().unwrap_or(gk).to_string();

        let mut slots = Vec::new();
        for (n, sl) in data.doc["groups"][&g.group]["slots"].as_array().into_iter().flatten().enumerate() {
            let Some(&row) = slot_rows.get(&(g.group.clone(), n)) else { continue };
            let now = g.slots.iter().find(|x| x.n == n);
            let then = was.and_then(|w| w.slots.iter().find(|x| x.n == n));
            let mut pools = Vec::new();
            for (pi, pkey) in slot_pools(sl).iter().enumerate() {
                let def = &data.doc["pools"][pkey];
                let shown = now.and_then(|x| pool_of(&view, x, pkey));
                let then_pool = then.and_then(|x| pool_of(&data.base, x, pkey));
                let total_w: f64 = doc["pools"][pkey]["items"].as_array().into_iter().flatten().map(|i| i["w"].as_f64().unwrap_or(1.0)).sum();
                let mut items = Vec::new();
                for (ix, item) in def["items"].as_array().into_iter().flatten().enumerate() {
                    let Some(&irow) = item_rows.get(&(pkey.clone(), ix)) else { continue };
                    let Some(r) = it.row_pos(irow).map(|p| &it.rows[p]) else { continue };
                    let id = loot::int(&item["id"]);
                    let w_now = doc["pools"][pkey]["items"][ix]["w"].as_f64().unwrap_or(1.0);
                    // One item may be in a pool twice: the share is split by the template weights
                    let same_w: f64 = doc["pools"][pkey]["items"].as_array().into_iter().flatten().filter(|x| loot::int(&x["id"]) == id).map(|x| x["w"].as_f64().unwrap_or(1.0)).sum();
                    let part = if same_w > 0.0 { w_now / same_w } else { 0.0 };
                    let chance = |pool: Option<(&PoolView, f64)>, slot: Option<&crate::loot::SlotView>| -> Option<(f64, f64, f64)> {
                        let (p, sh) = pool?;
                        let (share, avg) = item_share(p, id);
                        let rate = slot?.rate as f64 / RATE_MAX as f64;
                        Some((share * part, share * part * sh * rate, avg))
                    };
                    let current = chance(shown, now);
                    let before = chance(then_pool, then);
                    let cat = data.cats.get(&id).copied().unwrap_or(Cat::Other);
                    let mut cnt = Vec::new();
                    for c in 0.. {
                        let Some(pid) = it.pid(&format!("cnt[{}]", c)) else { break };
                        if r.value_of(it, pid).is_none() {
                            break;
                        }
                        cnt.push(self.cell_ref(ii, irow, pid, &s, en, ""));
                    }
                    items.push(json!({
                        "row": irow, "id": id, "title": r.title[usize::from(en)], "icon": r.icon, "rarity": r.rarity, "rarity_label": self.rarity_label(&r.rarity, en),
                        "cat": cat_code(cat), "cat_label": if en { cat_en(cat) } else { cat_ru(cat) },
                        "w": it.pid("w").map(|p| self.cell_ref(ii, irow, p, &s, en, "")), "cnt": cnt,
                        "template_share": if total_w > 0.0 { round6(w_now / total_w) } else { 0.0 },
                        "share": current.map(|x| round6(x.0)), "chance": current.map(|x| round6(x.1)), "avg": current.map(|x| round6(x.2)),
                        "chance_was": before.map(|x| round6(x.1))
                    }));
                }
                let others: Vec<String> = data.pool_groups.get(pkey).into_iter().flatten().filter(|o| **o != g.group).cloned().collect();
                let pool_label = if en { pool_label_en(pkey, def, &data.cats) } else { def["name"].as_str().unwrap_or_default().to_string() };
                pools.push(json!({
                    "key": pkey, "label": pool_label, "share": shown.map(|x| round6(x.1)),
                    "pr": st.pid(&format!("pr[{}]", pi)).filter(|p| st.row_pos(row).is_some_and(|x| st.rows[x].value_of(st, *p).is_some())).map(|p| self.cell_ref(si, row, p, &s, en, "")),
                    "shared": others.len(), "shared_titles": others.iter().take(SHARED_SHOWN).map(|o| group_title(o)).collect::<Vec<_>>(),
                    "items": items
                }));
            }
            slots.push(json!({
                "n": n, "row": row,
                "chance": st.pid("chance").map(|p| self.cell_ref(si, row, p, &s, en, "")),
                "rate": now.map(|x| round6(x.rate as f64 / RATE_MAX as f64)),
                "rate_was": then.map(|x| round6(x.rate as f64 / RATE_MAX as f64)),
                "template": sl["chance"].as_f64(),
                "pools": pools
            }));
        }

        let now = group_summary(&view, g, &data.cats);
        let before = was.map_or_else(DropSummary::default, |w| group_summary(&data.base, w, &data.cats));
        let mode = g.key.split_once('@').map(|x| x.1);
        Ok(json!({
            "key": g.key, "group": g.group, "title": label[lang], "kind": label["kind"],
            "sources": label["sources"],
            "maps": g.maps.iter().map(|&m| self.map_label(m, en)).collect::<Vec<_>>(),
            "types": g.types,
            "mode": mode.map(|m| json!({ "id": m, "title": doc["modes"][m]["name"] })),
            "rules": self.rules_json(&doc, mode, en),
            "summary": summary_json(&now, en),
            "summary_was": summary_json(&before, en),
            "slots": slots
        }))
    }

    /// What the rules add to the template: readable, for the group card
    fn rules_json(&self, doc: &Json, mode: Option<&str>, en: bool) -> Vec<Json> {
        let mut out = Vec::new();
        let mut section = |v: &Json, scope: &str| {
            let word = |k: &str| rule_word(k, en, &self.kit);
            let remove: Vec<String> = v["remove"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|x| match x {
                    Json::String(k) => word(k),
                    id => {
                        let key = id.to_string();
                        self.kit.catalog.as_ref().and_then(|c| c["items"][&key][if en { "en" } else { "ru" }].as_str().map(str::to_string)).unwrap_or(key)
                    }
                })
                .collect();
            let mult = |kind: &str| -> Vec<Json> { v[kind].as_object().into_iter().flatten().map(|(k, m)| json!({ "label": word(k), "value": m })).collect() };
            let (chance, weight, count) = (mult("chance"), mult("weight"), mult("count"));
            let quality = v["quality"].as_f64().filter(|q| *q != 0.0);
            if !remove.is_empty() || !chance.is_empty() || !weight.is_empty() || !count.is_empty() || quality.is_some() {
                out.push(json!({ "scope": scope, "remove": remove, "chance": chance, "weight": weight, "count": count, "quality": quality }));
            }
        };
        section(&doc["rules"], "rules");
        if let Some(m) = mode {
            section(&doc["modes"][m], m);
        }
        out
    }
}
