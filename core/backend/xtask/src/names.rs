// Labels for numeric ids in CSV (read-only — ignored on import).
//
// Sources:
//   attributes — field number in tgf.DBattleAttributeOffset = attribute id (1001 = first_level_strength);
//               if there is a localization dump — the text by the key from battle_attribute_description.name
//   rarity    — enum EnItemRarity / GeneralStrings Rarity_N (3 = UNCOMMON, "Rare" in the game)
//   classes   — role_type: career/race (400005 = WIZARD/HUMAN)
//   items     — item_type.name -> localization text; without a dump — the item type (EQUIPMENT_WEAPON)
//   legendaries— battle_ability_set_description.description -> localization text
//   localization — names/strings.tsv (table\tkey\ttext), dumped from the game by the UE4SS NameDump mod (F6)
//                 and `cargo xtask balance names-import`.
//
// Which fields count as references — REF_FIELDS / MAP_KEY_FIELDS below (by field name).
use crate::tables::Schema;
use serde_json::Value as Json;
use std::collections::HashMap;
use std::path::Path;

pub const STRINGS_TSV: &str = "content/names/strings.tsv";
/// English localization (a second NameDump dump with the client in EN): `names-import --lang=en`
pub const STRINGS_EN_TSV: &str = "content/names/strings_en.tsv";

#[derive(Clone, Copy, PartialEq)]
enum Dict {
    Attr,
    Rarity,
    Role,
    Item,
    Ability,
    /// type_id / item_id etc.: an item, a class or a legendary — whatever is found
    AnyId,
}

/// Field name (last path segment) -> dictionary
const REF_FIELDS: &[(&str, Dict)] = &[
    ("attribute_type", Dict::Attr),
    ("type_id", Dict::AnyId),
    ("item_id", Dict::AnyId),
    ("ue_source_type_id", Dict::AnyId),
    ("coin_type_id", Dict::Item),
    ("access_key_type_id", Dict::Item),
    ("role_type_id", Dict::Role),
    ("career_type_id", Dict::Role),
    ("battle_which_role_type", Dict::Role),
    ("ability_set_id", Dict::Ability),
    ("affix_alias_id", Dict::Ability),
    ("legendary_affix_id", Dict::Ability),
    ("rarity", Dict::Rarity),
    ("tiers_rarity", Dict::Rarity),
    ("standard_rarity", Dict::Rarity),
    ("battle_gear_rarity_require", Dict::Rarity),
];

/// map fields whose KEY is a reference (rarity_range[3] = rarity 3 = UNCOMMON)
const MAP_KEY_FIELDS: &[(&str, Dict)] = &[("rarity_range", Dict::Rarity), ("attribute_random", Dict::Attr)];

pub struct Names {
    attr: HashMap<i64, String>,
    rarity: HashMap<i64, String>,
    role: HashMap<i64, String>,
    /// Class name from localization (role_type.str_name); `role` holds the technical FIGHTER/HUMAN
    role_local: HashMap<i64, String>,
    item: HashMap<i64, String>,
    /// Only real names from localization, without substituting the item type
    item_local: HashMap<i64, String>,
    ability: HashMap<i64, String>,
    strings: HashMap<String, String>,
    pub strings_loaded: usize,
}

fn rows_of(balance: &str, table: &str) -> Vec<Json> {
    crate::tables::read_json(&Path::new(balance).join(format!("{}.json", table)))
        .ok()
        .and_then(|v| v["rows"].as_array().cloned())
        .unwrap_or_default()
}

fn short_enum(s: &str, prefix: &str) -> String {
    s.strip_prefix(prefix).unwrap_or(s).to_string()
}

impl Names {
    pub fn load(s: &Schema, balance: &str) -> Names {
        Self::load_from(s, balance, STRINGS_TSV)
    }

    /// The same with another localization file (STRINGS_EN_TSV — English labels of the balance kit)
    pub fn load_from(s: &Schema, balance: &str, strings_tsv: &str) -> Names {
        // localization: key -> text (keys are unique across tables in practice; on a clash the first one wins)
        let mut strings: HashMap<String, String> = HashMap::new();
        if let Ok(text) = std::fs::read_to_string(strings_tsv) {
            for line in text.lines() {
                let mut it = line.splitn(3, '\t');
                if let (Some(_table), Some(key), Some(val)) = (it.next(), it.next(), it.next()) {
                    if !val.is_empty() {
                        strings.entry(key.to_string()).or_insert_with(|| val.replace("\\n", " ").replace("\\t", " "));
                    }
                }
            }
        }
        let loc = |key: &str| strings.get(key).cloned();

        // attributes: field number of DBattleAttributeOffset = attribute id
        let mut attr = HashMap::new();
        if let Some(m) = s.pool.all_messages().find(|m| m.name() == "DBattleAttributeOffset") {
            for f in m.fields() {
                attr.insert(f.number() as i64, f.name().to_string());
            }
        }
        for r in rows_of(balance, "battle_attribute_description") {
            if let (Some(id), Some(key)) = (r["attribute_type"].as_i64(), r["name"].as_str()) {
                if let Some(t) = loc(key) {
                    attr.insert(id, t);
                }
            }
        }

        // GeneralStrings Rarity_N — the number N matches EnItemRarity (verified: 3 = UNCOMMON = "Rare")
        let mut rarity = HashMap::new();
        if let Some(e) = s.pool.get_enum_by_name("tgf.EnItemRarity").or_else(|| s.pool.all_enums().find(|e| e.name() == "EnItemRarity")) {
            for v in e.values() {
                let n = v.number() as i64;
                let text = loc(&format!("Rarity_{}", n)).map(|t| t.trim_end_matches('.').to_string());
                rarity.insert(n, text.unwrap_or_else(|| short_enum(v.name(), "EN_ITEM_RARITY_")));
            }
        }

        let mut role = HashMap::new();
        let mut role_local = HashMap::new();
        for r in rows_of(balance, "role_type") {
            if let (Some(id), Some(text)) = (r["type_id"].as_i64(), r["str_name"].as_str().and_then(&loc)) {
                role_local.insert(id, text);
            }
            // service classes without meta (499999 — the dummy) get no label
            if let (Some(id), Some(career)) = (r["type_id"].as_i64(), r["meta"]["career"].as_str()) {
                let race = short_enum(r["meta"]["race"].as_str().unwrap_or("?"), "EN_ROLE_RACE_");
                role.insert(id, format!("{}/{}", short_enum(career, "EN_ROLE_CAREER_"), race));
            }
        }

        let mut item = HashMap::new();
        let mut item_local = HashMap::new();
        for r in rows_of(balance, "item_type") {
            if let Some(id) = r["type_id"].as_i64() {
                if let Some(text) = r["name"].as_str().and_then(&loc) {
                    item_local.insert(id, text);
                }
                let name = r["name"].as_str().and_then(&loc).unwrap_or_else(|| {
                    short_enum(r["item_minor_type"].as_str().unwrap_or("item"), "EN_ITEM_MINOR_TYPE_")
                });
                item.insert(id, name);
            }
        }

        let mut ability = HashMap::new();
        for r in rows_of(balance, "battle_ability_set_description") {
            if let (Some(id), Some(key)) = (r["ability_set_id"].as_i64(), r["description"].as_str()) {
                // legendaries have a whole-sentence description in localization; the beginning is enough for a label
                let text = loc(key).unwrap_or_else(|| key.to_string());
                let short: String = text.chars().take(60).collect();
                ability.insert(id, if short.len() < text.len() { format!("{}…", short) } else { text });
            }
        }

        Names { attr, rarity, role, role_local, item, item_local, ability, strings_loaded: strings.len(), strings }
    }

    /// Attribute field name (first_level_strength, attack_critical_rate…) -> name from localization
    /// (balance kit: column labels). Without localization — empty.
    pub fn attr_field_labels(&self, s: &Schema) -> Vec<(String, String)> {
        let Some(m) = s.pool.all_messages().find(|m| m.name() == "DBattleAttributeOffset") else { return Vec::new() };
        m.fields()
            .filter_map(|f| self.attr.get(&(f.number() as i64)).filter(|t| t.as_str() != f.name()).map(|t| (f.name().to_string(), t.clone())))
            .collect()
    }

    /// Localization text by key
    pub fn text(&self, key: &str) -> Option<&str> {
        self.strings.get(key).map(|s| s.as_str())
    }

    pub fn item_localized(&self, id: i64) -> Option<&str> {
        self.item_local.get(&id).map(|s| s.as_str())
    }

    pub fn role_localized(&self, id: i64) -> Option<&str> {
        self.role_local.get(&id).map(|s| s.as_str())
    }

    pub fn rarity_name(&self, id: i64) -> Option<&str> {
        self.rarity.get(&id).map(|s| s.as_str())
    }

    pub fn item(&self, id: i64) -> Option<&str> {
        self.item.get(&id).map(|s| s.as_str())
    }

    /// Item id by name (as in labels); an error if the name is not found or ambiguous
    pub fn item_by_name(&self, name: &str) -> Result<i64, String> {
        let mut ids: Vec<i64> = self.item.iter().filter(|(_, n)| n.as_str() == name).map(|(id, _)| *id).collect();
        ids.sort();
        match ids.as_slice() {
            [id] => Ok(*id),
            [] => Err(format!("item \"{}\" not found", name)),
            _ => Err(format!("the name \"{}\" belongs to several items {:?} — give the id", name, ids)),
        }
    }

    fn lookup(&self, d: Dict, id: i64) -> Option<String> {
        match d {
            Dict::Attr => self.attr.get(&id).cloned(),
            Dict::Rarity => self.rarity.get(&id).cloned(),
            Dict::Role => self.role.get(&id).cloned(),
            Dict::Item => self.item.get(&id).cloned(),
            Dict::Ability => self.ability.get(&id).cloned(),
            Dict::AnyId => self
                .item
                .get(&id)
                .or_else(|| self.role.get(&id))
                .or_else(|| self.ability.get(&id))
                .cloned(),
        }
    }

    /// Label of a value of field `field` (last path segment, without [..]); None — not a reference / not found.
    pub fn for_field(&self, field: &str, value: &Json) -> Option<String> {
        let (_, d) = REF_FIELDS.iter().find(|(n, _)| *n == field)?;
        let id = value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))?;
        self.lookup(*d, id)
    }

    /// Label of a key of map field `field` (rarity_range[3] -> "Rare")
    pub fn for_map_key(&self, field: &str, key: &str) -> Option<String> {
        let (_, d) = MAP_KEY_FIELDS.iter().find(|(n, _)| *n == field)?;
        self.lookup(*d, key.parse().ok()?)
    }

    /// Label of a whole long-form path: map keys and the reference value joined with " · "
    /// (data.affix_data[0].rarity_range[3].max_value = 12 -> "Rare")
    pub fn for_path(&self, path: &str, value: &Json) -> String {
        let mut parts = Vec::new();
        let mut last_field = "";
        for seg in path.split('.') {
            let (name, rest) = seg.split_once('[').map(|(n, r)| (n, Some(r))).unwrap_or((seg, None));
            last_field = name;
            if let Some(rest) = rest {
                for key in rest.split('[').map(|k| k.trim_end_matches(']')) {
                    if let Some(n) = self.for_map_key(name, key) {
                        parts.push(n);
                    }
                }
            }
        }
        if let Some(n) = self.for_field(last_field, value) {
            parts.push(n);
        }
        parts.join(" · ")
    }
}

/// How many entries each dictionary has — for the report
pub fn summary(n: &Names) -> String {
    format!(
        "attributes {}, rarities {}, classes {}, items {}, legendaries {}, localization strings {}",
        n.attr.len(),
        n.rarity.len(),
        n.role.len(),
        n.item.len(),
        n.ability.len(),
        n.strings_loaded
    )
}
