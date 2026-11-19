//! Item affix generation (PVE format: BASIC/MAJOR/MINOR/LEGENDARY, see HANDOFF_CLAUDE_CODE.md).
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).

use super::*;

/// Compatible pool of minor attributes from PrivateLootRules.lua (if pool 100101/100801 in the template gave no stat)
const FALLBACK_MINOR_ATTRS: [i32; 15] = [1001, 1002, 1003, 1004, 1005, 1006, 1103, 1104, 1105, 2002, 2003, 4003, 4004, 4002, 3001];

/// Gear sets: property id → name (the gear_set_* field in DBattleAttributeOffset)
const GEAR_SET_NAMES: [(i32, &str); 9] = [
    (11001, "gear_set_fleet_foot_work"),
    (11002, "gear_set_kleptomancy"),
    (11003, "gear_set_undying_grasp"),
    (11004, "gear_set_thunder_law"),
    (11005, "gear_set_bone_plating"),
    (11006, "gear_set_avoid_death"),
    (11007, "gear_set_scorching_touch"),
    (11008, "gear_set_summoning_aery"),
    (11009, "gear_set_coin_master"),
];

/// A MINOR slot of the template: the random_rule index and the properties that can roll in it
pub(super) struct MinorSlot {
    pub rule_idx: usize,
    pub candidates: Vec<i32>,
}

impl GatewaySession {
    /// Full and exact mapping of all DBattleAttributeOffset fields -> attribute_type per battle_attribute_description.json!
    pub(super) fn map_attribute_offsets(attr: &tgf::DBattleAttributeOffset) -> Vec<(i32, &Option<tgf::DBattleAttributeValueOffset>)> {
        vec![
            (1001, &attr.first_level_strength),
            (1002, &attr.first_level_dexterity),
            (1003, &attr.first_level_stamina),
            (1004, &attr.first_level_intelligence),
            (1005, &attr.first_level_will),
            (1006, &attr.first_level_faith),
            (1021, &attr.all_first_level),
            (1101, &attr.attack_right_weapon_attack),
            (1102, &attr.attack_attack_bonus),
            (1103, &attr.attack_attack_boost_rate),
            (1104, &attr.attack_critical_rate),
            (1105, &attr.attack_crit_damage_rate),
            (1106, &attr.attack_element_boost_rate_none),
            (1107, &attr.attack_element_boost_rate_all),
            (1108, &attr.attack_element_boost_rate_fire),
            (1109, &attr.attack_element_boost_rate_ice),
            (1110, &attr.attack_element_boost_rate_thunder),
            (1111, &attr.attack_element_boost_rate_light),
            (1112, &attr.attack_element_boost_rate_dark),
            (1113, &attr.attack_def_ignore_base),
            (1114, &attr.attack_physical_penetration),
            (1115, &attr.attack_fire_penetration),
            (1116, &attr.attack_cold_penetration),
            (1117, &attr.attack_lightning_penetration),
            (1118, &attr.attack_divine_penetration),
            (1119, &attr.attack_shadow_penetration),
            (1120, &attr.attack_right_att_gear_level),
            (1121, &attr.attack_extra_att_gear_level),
            (1122, &attr.attack_skill_boost_rate),
            (2001, &attr.defend_max_health_base),
            (2002, &attr.defend_max_health_percent),
            (2003, &attr.defend_max_health_extra),
            (2004, &attr.defend_def_gear_level),
            (2005, &attr.defend_extra_def_gear_level),
            (2006, &attr.defend_guarded_damage_reduce_rate),
            (2007, &attr.defend_element_resistance_rate_none),
            (2008, &attr.defend_element_resistance_rate_all),
            (2009, &attr.defend_element_resistance_rate_fire),
            (2010, &attr.defend_element_resistance_rate_ice),
            (2011, &attr.defend_element_resistance_rate_thunder),
            (2012, &attr.defend_element_resistance_rate_light),
            (2013, &attr.defend_element_resistance_rate_dark),
            (2014, &attr.defend_extra_guarded_damage_reduce_rate),
            (3001, &attr.speed_move_speed_base),
            (3002, &attr.speed_level_interact_speed_rate),
            (4001, &attr.recover_life_leach_rate),
            (4002, &attr.recover_heal_boost_rate),
            (4003, &attr.recover_heal_on_hit_base),
            (4004, &attr.recover_heal_on_kill_base),
            (4005, &attr.recover_heal_over_time_base),
            (4006, &attr.recover_shield_boost_rate),
            (4007, &attr.recover_heal_wound_rate),
            (4008, &attr.recover_skill_cooldown_scale_rate),
            (4009, &attr.recover_gain_gold_out_of_portal),
            (4010, &attr.recover_exp_boost_rate_out_of_portal),
            (5001, &attr.magic_max_magic_base),
            (5002, &attr.magic_magic_over_time),
            (5003, &attr.magic_magic_on_hit),
            (5004, &attr.magic_magic_on_kill),
            (5005, &attr.magic_magic_on_hurt),
            (5006, &attr.magic_magic_generate_rate),
            (5101, &attr.soul_energy_max_soul_energy_base),
            (5102, &attr.soul_energy_soul_energy_on_hit),
            (5103, &attr.soul_energy_soul_energy_on_kill),
            (5104, &attr.soul_energy_soul_energy_on_hurt),
            (5105, &attr.soul_energy_soul_energy_generate_rate),
            (5201, &attr.natural_energy_max_natural_energy_base),
            (5202, &attr.natural_energy_natural_energy_over_time),
            (5203, &attr.natural_energy_natural_energy_on_hit),
            (5204, &attr.natural_energy_natural_energy_on_kill),
            (5205, &attr.natural_energy_natural_energy_on_hurt),
            (5206, &attr.natural_energy_natural_energy_generate_rate),
            (6001, &attr.buff_arg_killing_max_layer_max_health_percent),
            (6002, &attr.buff_arg_killing_max_layer_attack_boost_rate),
            (6003, &attr.buff_arg_killing_max_layer_critical_rate),
            (6101, &attr.stat_effect_move_speed_with_hit_running),
            (6102, &attr.stat_effect_heal_over_time_with_hit_running),
            (6103, &attr.stat_effect_critical_rate_with_hit_running),
            (6104, &attr.stat_effect_life_leach_rate_with_hit_running),
            (6105, &attr.stat_effect_attack_boost_rate_with_full_health),
            (6106, &attr.stat_effect_critical_rate_with_full_health),
            (6201, &attr.trigger_effect_heal_on_open_chest),
            (6202, &attr.trigger_effect_magic_on_open_chest),
            (6203, &attr.trigger_effect_gain_gold_on_open_chest),
            (6204, &attr.trigger_effect_gain_gold_on_kill),
            (6205, &attr.trigger_effect_heal_on_defend_success),
            (6206, &attr.trigger_effect_damage_on_defend_success),
            (6207, &attr.trigger_effect_additional_fixed_damage_on_causing_hit),
            (6208, &attr.trigger_effect_add_shield_on_cast_skill),
            (6209, &attr.trigger_effect_additional_element_none_damage_on_causing_hit),
            (6210, &attr.trigger_effect_additional_element_fire_damage_on_causing_hit),
            // 6211/6212 — by proto field numbers (ice = 6211, thunder = 6212), as the DS applies them and the client labels them
            // ("Added Cold/Lightning Damage"); battle_attribute_description swaps the name keys — not by it
            // (FIX_LIST BAL3, StatWatch 05.11)
            (6211, &attr.trigger_effect_additional_element_ice_damage_on_causing_hit),
            (6212, &attr.trigger_effect_additional_element_thunder_damage_on_causing_hit),
            (6213, &attr.trigger_effect_additional_element_light_damage_on_causing_hit),
            (6214, &attr.trigger_effect_additional_element_dark_damage_on_causing_hit),
        ]
    }

    /// Returns the list of attribute IDs that actually have a weight set in the ExcelAffixPool pool (value.is_some())
    pub(super) fn collect_pool_attr_ids(attr: &tgf::DBattleAttributeOffset) -> Vec<i32> {
        let mut ids = Vec::new();
        for (aid, opt) in Self::map_attribute_offsets(attr) {
            if opt.as_ref().and_then(|o| o.value.as_ref()).is_some() {
                ids.push(aid);
            }
        }
        ids
    }

    /// IDs of gear sets (gear_set_*, 11001..11009) that have a weight set in the pool
    pub(super) fn collect_pool_gear_set_ids(attr: &tgf::DBattleAttributeOffset) -> Vec<i32> {
        [
            (11001, &attr.gear_set_fleet_foot_work),
            (11002, &attr.gear_set_kleptomancy),
            (11003, &attr.gear_set_undying_grasp),
            (11004, &attr.gear_set_thunder_law),
            (11005, &attr.gear_set_bone_plating),
            (11006, &attr.gear_set_avoid_death),
            (11007, &attr.gear_set_scorching_touch),
            (11008, &attr.gear_set_summoning_aery),
            (11009, &attr.gear_set_coin_master),
        ]
        .into_iter()
        .filter(|(_, o)| o.as_ref().and_then(|g| g.value.as_ref()).is_some())
        .map(|(id, _)| id)
        .collect()
    }

    /// Weight of a property (attribute < 10000 or gear set 11001..11009) in the pool: the row's `weight`, without a weight (offset/fixed) — 1
    pub(super) fn pool_weight(attr: &tgf::DBattleAttributeOffset, id: i32) -> i64 {
        use tgf::d_battle_attribute_value_offset::Value as A;
        use tgf::d_battle_gear_set_offset::Value as G;
        let w = if id >= 11000 {
            let set = match id {
                11001 => &attr.gear_set_fleet_foot_work,
                11002 => &attr.gear_set_kleptomancy,
                11003 => &attr.gear_set_undying_grasp,
                11004 => &attr.gear_set_thunder_law,
                11005 => &attr.gear_set_bone_plating,
                11006 => &attr.gear_set_avoid_death,
                11007 => &attr.gear_set_scorching_touch,
                11008 => &attr.gear_set_summoning_aery,
                11009 => &attr.gear_set_coin_master,
                _ => return 1,
            };
            match set.as_ref().and_then(|g| g.value.as_ref()) {
                Some(G::Weight(w)) => *w,
                _ => 1,
            }
        } else {
            match Self::map_attribute_offsets(attr).into_iter().find(|(a, _)| *a == id).and_then(|(_, o)| o.as_ref().and_then(|o| o.value.as_ref())) {
                Some(A::Weight(w)) => *w,
                _ => 1,
            }
        };
        w.max(0)
    }

    /// Index by weights for roll 0..9999. All weights equal (or sum 0) — `roll % len`, as before 13.11 (the same rolls).
    pub(super) fn weighted_pick(weights: &[i64], roll: i32) -> usize {
        let total: i64 = weights.iter().sum();
        if total <= 0 || weights.iter().all(|w| *w == weights[0]) {
            return roll as usize % weights.len();
        }
        let mut x = roll as i64 * total / 10000;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len() - 1
    }

    /// Builds DItemData.affix_data in the native PVE SoloLobby format (checked against rpc_dump 30.10, items 185016002/189010001):
    /// BASIC — only offset attributes of fixed_rule; MAJOR — all pool attributes + gear_set_random; MINOR — a record per random_rule;
    /// LEGENDARY — ability_set_random by legendary_rule. Schema version in player.state["affix_schema_ver"] = 6.
    pub fn build_item_affix_data(&self, item_id: i32, wear_part: i32) -> Vec<tgf::DBattleAffixRandomData> {
        self.build_item_affix_data_seeded(item_id, wear_part, 0)
    }

    /// The same with a roll salt: seed 0 — the previous deterministic values, another seed — a new set of affixes.
    pub fn build_item_affix_data_seeded(&self, item_id: i32, wear_part: i32, seed: u64) -> Vec<tgf::DBattleAffixRandomData> {
        self.build_item_affix_data_forced(item_id, wear_part, seed, &HashMap::new())
    }

    /// 15.11 (GM `itemaff`): `forced` — random_rule index (MINOR, affix_type 20) → the property put into this
    /// slot instead of a random one (attribute < 10000 or gear set 11001..11009). These properties do not roll randomly in other
    /// slots. An empty `forced` — the same result as before 15.11.
    pub fn build_item_affix_data_forced(&self, item_id: i32, wear_part: i32, seed: u64, forced: &HashMap<usize, i32>) -> Vec<tgf::DBattleAffixRandomData> {
        let reserved: std::collections::HashSet<i32> = forced.values().copied().collect();
        let mut affix_data = Vec::new();
        let mut used_attrs = std::collections::HashSet::new();
        let template = self.game_data.affix_template.get(&item_id).and_then(|t| t.data.as_ref());

        // 1. BASIC (affix_type = 5), as in PVE: only fixed_rule attributes with `offset`, {random_level} without pool/group,
        //    location from fixed_rule. No offset attributes (armor/jewelry) -> no BASIC record at all.
        if let Some(tpl) = template {
            for f_rule in tpl.fixed_rule.iter().filter(|r| r.affix_type == 5) {
                let Some(attr) = f_rule.attribute.as_ref() else { continue };
                let mut basic_map = HashMap::new();
                for (aid, opt) in Self::map_attribute_offsets(attr) {
                    let is_offset = matches!(
                        opt.as_ref().and_then(|o| o.value.as_ref()),
                        Some(tgf::d_battle_attribute_value_offset::Value::Offset(_))
                    );
                    if is_offset && used_attrs.insert(aid) {
                        basic_map.insert(aid, tgf::DBattleAffixAttributeRandomValueCollectData {
                            random_data: vec![tgf::DBattleAffixAttributeRandomValueData {
                                random_level: Self::affix_roll(item_id, wear_part, (aid as u64) ^ seed),
                                effect: None,
                                affix_pool_value: false,
                                group_index: 0,
                            }],
                        });
                    }
                }
                if !basic_map.is_empty() {
                    affix_data.push(tgf::DBattleAffixRandomData {
                        location: f_rule.location,
                        affix_type: 5,
                        attribute_random: basic_map,
                        ..Default::default()
                    });
                }
            }
        } else if (1..=4).contains(&wear_part) {
            // No template: weapon -> BASIC 1101 (weapon damage), ROLE_HOLD
            let mut basic_map = HashMap::new();
            basic_map.insert(
                1101,
                tgf::DBattleAffixAttributeRandomValueCollectData {
                        random_data: vec![tgf::DBattleAffixAttributeRandomValueData {
                        random_level: Self::affix_roll(item_id, wear_part, (1101) ^ seed), // 0..9999, as in the reference
                        effect: None,
                        affix_pool_value: false,
                        group_index: 0,
                    }],
                },
            );
            used_attrs.insert(1101);
            affix_data.push(tgf::DBattleAffixRandomData {
                location: 3,   // EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD
                affix_type: 5, // EN_BATTLE_AFFIX_TYPE_BASIC
                attribute_random: basic_map,
                ..Default::default()
            });
        }

                // 2. MAJOR/MINOR in the native format: one DBattleAffixRandomData per random_rule,
        //    affix_type as in the template, affix_pool_value=true, group_index=1 (pool_random index), random_level 0..9999
        let rv = |lvl: i32| tgf::DBattleAffixAttributeRandomValueCollectData {
            random_data: vec![tgf::DBattleAffixAttributeRandomValueData {
                random_level: lvl,
                effect: None,
                affix_pool_value: true,
                group_index: 1,
            }],
        };

        if let Some(offset_data) = template {
            for (rule_idx, r_rule) in offset_data.random_rule.iter().enumerate() {
                let Some(pg) = r_rule.pool_random.first() else { continue }; // empty rule -> no slot
                let loc = if r_rule.location != 0 { r_rule.location } else { 1 };
                let mut map = HashMap::new();
                let mut gear_sets = HashMap::new();
                match r_rule.affix_type {
                    10 => {
                        // affix_count_pool 1 -> count 99: all MAJOR pool attributes; gear_set_* -> gear_set_random
                        for &pool_id in &pg.affix_pool_id {
                            if let Some(w) = self.game_data.affix_pool.get(&pool_id).and_then(|p| p.attribute_weight.as_ref()) {
                                for aid in Self::collect_pool_attr_ids(w) {
                                    if aid < 10000 && used_attrs.insert(aid) {
                                        map.insert(aid, rv(Self::affix_roll(item_id, wear_part, (aid as u64) ^ seed)));
                                    }
                                }
                                for gid in Self::collect_pool_gear_set_ids(w) {
                                    gear_sets.insert(gid, tgf::DBattleAffixGearSetRandomCollectData {
                                        random_data: vec![tgf::DBattleAffixGearSetRandomData {
                                            random_level: Self::affix_roll(item_id, wear_part, (gid as u64) ^ seed),
                                            group_index: 1,
                                        }],
                                    });
                                }
                            }
                        }
                    }
                    20 if forced.contains_key(&rule_idx) => {
                        // GM: the property is set; the level — the same roll as a random one (gm_affix_data rewrites it later)
                        let aid = forced[&rule_idx];
                        used_attrs.insert(aid);
                        let lvl = Self::affix_roll(item_id, wear_part, (12_000 + rule_idx as u64) ^ seed);
                        if aid >= 11000 {
                            gear_sets.insert(aid, tgf::DBattleAffixGearSetRandomCollectData {
                                random_data: vec![tgf::DBattleAffixGearSetRandomData { random_level: lvl, group_index: 1 }],
                            });
                        } else {
                            map.insert(aid, rv(lvl));
                        }
                    }
                    20 => {
                        // affix_count_pool 101 -> count 1: one attribute per rule
                        // attributes and gear set points (Midas Touch 11009 in pool 100801 — "green" slots, 05.11)
                        // 09.11: first the rule's set (pool), then a property in it — set chance 1/number of sets, as for
                        // Midas on green (1 in 5) and gear set bonuses on blue/purple (playtest season, pool
                        // 100802). Before, the choice was from all properties of all sets in a row: a set of 7 gear sets would give 30–40%.
                        let groups: Vec<(Vec<i32>, &tgf::DBattleAttributeOffset)> = pg.affix_pool_id.iter()
                            .filter_map(|id| self.game_data.affix_pool.get(id))
                            .filter_map(|p| p.attribute_weight.as_ref())
                            .map(|w| {
                                let mut v: Vec<i32> = Self::collect_pool_attr_ids(w).into_iter().filter(|a| *a < 10000).collect();
                                v.extend(Self::collect_pool_gear_set_ids(w));
                                v.retain(|a| !used_attrs.contains(a) && !reserved.contains(a));
                                v.sort_unstable();
                                v.dedup();
                                (v, w)
                            })
                            .filter(|(v, _)| !v.is_empty())
                            .collect();
                        let (mut cands, pool): (Vec<i32>, Option<&tgf::DBattleAttributeOffset>) = if groups.is_empty() {
                            (FALLBACK_MINOR_ATTRS.iter().copied().filter(|a| !used_attrs.contains(a) && !reserved.contains(a)).collect(), None)
                        } else {
                            let g = Self::affix_roll(item_id, wear_part, (13_000 + rule_idx as u64) ^ seed) as usize % groups.len();
                            (groups[g].0.clone(), Some(groups[g].1))
                        };
                        cands.sort_unstable();
                        cands.dedup();
                        if !cands.is_empty() {
                            // 13.11: the property inside the set — by pool weights (Kleptomancy in 100802 — weight 2 vs 10, season
                            // playtest); with equal weights — as before, roll % count
                            let weights: Vec<i64> = cands.iter().map(|&a| pool.map_or(1, |w| Self::pool_weight(w, a))).collect();
                            let roll = Self::affix_roll(item_id, wear_part, (11_000 + rule_idx as u64) ^ seed);
                            let pick = Self::weighted_pick(&weights, roll);
                            let aid = cands[pick];
                            used_attrs.insert(aid);
                            // MINOR salts are shifted to 10_000+: before, 1000+idx / 2000+idx coincided with MAJOR salts 1001 / 2001–2003
                            let lvl = Self::affix_roll(item_id, wear_part, (12_000 + rule_idx as u64) ^ seed);
                            if aid >= 11000 {
                                gear_sets.insert(aid, tgf::DBattleAffixGearSetRandomCollectData {
                                    random_data: vec![tgf::DBattleAffixGearSetRandomData { random_level: lvl, group_index: 1 }],
                                });
                            } else {
                                map.insert(aid, rv(lvl));
                            }
                        }
                    }
                    _ => {}
                }
                if !map.is_empty() || !gear_sets.is_empty() {
                    affix_data.push(tgf::DBattleAffixRandomData {
                        location: loc,
                        affix_type: r_rule.affix_type, // 10 or 20, not substituted
                        attribute_random: map,
                        gear_set_random: gear_sets,
                        ..Default::default()
                    });
                }
            }

            // 3. LEGENDARY (affix_type = 30): as in PVE — ability_set_random {alias: {random_level, legendary_affix_id: alias}},
            //    location from legendary_rule
            for l_rule in &offset_data.legendary_rule {
                let alias = l_rule.affix_alias_id;
                let mut abil = HashMap::new();
                abil.insert(alias, tgf::DBattleAffixAbilityRandomCollectData {
                    random_data: vec![tgf::DBattleAffixAbilityRandomData {
                        random_level: Self::affix_roll(item_id, wear_part, (3000) ^ seed),
                        group_index: 0,
                        affix_type_id: Some(tgf::d_battle_affix_ability_random_data::AffixTypeId::LegendaryAffixId(alias)),
                    }],
                });
                affix_data.push(tgf::DBattleAffixRandomData {
                    location: l_rule.location,
                    affix_type: tgf::EnBattleAffixType::Legendary as i32,
                    ability_set_random: abil,
                    ..Default::default()
                });
            }
        }

        affix_data
    }

    /// Deterministic roll 0..9999 (values are not rerolled on every login)
    pub(super) fn affix_roll(item_id: i32, wear_part: i32, salt: u64) -> i32 {
        let mut x = ((item_id as u64) << 32) ^ ((wear_part as u64) << 16) ^ salt;
        x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((x ^ (x >> 31)) % 10000) as i32
    }

    // ---- GM: an item with chosen affixes (15.11, user: "can an item with an affix be granted" → "Yes, add it") ----

    /// Property name for GM: from battle_attribute_description without `affix_`; 6211/6212 — by proto fields (in the table
    /// the names are swapped, see map_attribute_offsets); gear sets — gear_set_*
    pub(super) fn affix_attr_name(&self, aid: i32) -> String {
        match aid {
            6211 => return "trigger_effect_additional_element_ice_damage_on_causing_hit".into(),
            6212 => return "trigger_effect_additional_element_thunder_damage_on_causing_hit".into(),
            _ => {}
        }
        if let Some((_, n)) = GEAR_SET_NAMES.iter().find(|(id, _)| *id == aid) {
            return n.to_string();
        }
        self.game_data
            .battle_attribute_description
            .iter()
            .find(|d| d.attribute_type == aid)
            .map(|d| d.name.trim_start_matches("affix_").to_string())
            .unwrap_or_else(|| "?".into())
    }

    /// Property from a GM string: a number (1001, 11002) or a name (`first_level_strength`, with or without `affix_`)
    pub(super) fn affix_attr_parse(&self, s: &str) -> Option<i32> {
        if let Ok(n) = s.parse::<i32>() {
            return Some(n);
        }
        let s = s.trim_start_matches("affix_").to_lowercase();
        let known = self.game_data.battle_attribute_description.iter().map(|d| d.attribute_type).chain([6211, 6212]).chain(GEAR_SET_NAMES.iter().map(|g| g.0));
        known.into_iter().find(|aid| self.affix_attr_name(*aid) == s)
    }

    /// MINOR slots (affix_type 20) of the item template and what can roll in each (without BASIC/MAJOR properties — they
    /// are always on the item)
    pub(super) fn affix_minor_slots(&self, item_id: i32, fixed: &std::collections::HashSet<i32>) -> Vec<MinorSlot> {
        let Some(tpl) = self.game_data.affix_template.get(&item_id).and_then(|t| t.data.as_ref()) else { return vec![] };
        let mut out = Vec::new();
        for (rule_idx, r) in tpl.random_rule.iter().enumerate() {
            if r.affix_type != 20 {
                continue;
            }
            let Some(pg) = r.pool_random.first() else { continue };
            let mut c: Vec<i32> = Vec::new();
            for w in pg.affix_pool_id.iter().filter_map(|id| self.game_data.affix_pool.get(id)).filter_map(|p| p.attribute_weight.as_ref()) {
                c.extend(Self::collect_pool_attr_ids(w).into_iter().filter(|a| *a < 10000));
                c.extend(Self::collect_pool_gear_set_ids(w));
            }
            if c.is_empty() {
                c.extend(FALLBACK_MINOR_ATTRS);
            }
            c.retain(|a| !fixed.contains(a));
            c.sort_unstable();
            c.dedup();
            out.push(MinorSlot { rule_idx, candidates: c });
        }
        out
    }

    /// BASIC and MAJOR properties (affix_type 5 and 10) — present on every such item, independent of the roll
    fn affix_fixed_attrs(data: &[tgf::DBattleAffixRandomData]) -> std::collections::HashSet<i32> {
        data.iter()
            .filter(|a| a.affix_type == 5 || a.affix_type == 10)
            .flat_map(|a| a.attribute_random.keys().chain(a.gear_set_random.keys()).copied().collect::<Vec<_>>())
            .collect()
    }

    /// Item affixes for GM: a normal roll with salt `seed`, the properties from `wishes` (property, level 0..9999) are put on
    /// the item with that level. A BASIC/MAJOR property — only the level changes; otherwise it takes a MINOR slot where it can
    /// roll. `max_all` — all other levels 9999. Err — an item without a template or a property with nowhere to go.
    pub(super) fn gm_affix_data(&self, item_id: i32, wear_part: i32, seed: u64, wishes: &[(i32, i32)], max_all: bool) -> Result<Vec<tgf::DBattleAffixRandomData>, String> {
        if !self.game_data.affix_template.contains_key(&item_id) {
            return Err(format!("{} has no affix template (not gear?)", item_id));
        }
        let mut seen = std::collections::HashSet::new();
        if let Some((aid, _)) = wishes.iter().find(|(aid, _)| !seen.insert(*aid)) {
            return Err(format!("property {} given twice", aid));
        }
        let base = self.build_item_affix_data_forced(item_id, wear_part, seed, &HashMap::new());
        let fixed = Self::affix_fixed_attrs(&base);
        let need: Vec<i32> = wishes.iter().map(|w| w.0).filter(|a| !fixed.contains(a)).collect();
        let slots = self.affix_minor_slots(item_id, &fixed);
        if need.len() > slots.len() {
            return Err(format!("{} has {} minor property slots, but {} requested: {:?}", item_id, slots.len(), need.len(), need));
        }
        // properties into slots: backtracking (slots ≤ 6), a property goes only into a slot where it can roll
        fn assign(i: usize, need: &[i32], slots: &[MinorSlot], used: &mut Vec<bool>, out: &mut HashMap<usize, i32>) -> bool {
            if i == need.len() {
                return true;
            }
            for (k, s) in slots.iter().enumerate() {
                if !used[k] && s.candidates.contains(&need[i]) {
                    used[k] = true;
                    out.insert(s.rule_idx, need[i]);
                    if assign(i + 1, need, slots, used, out) {
                        return true;
                    }
                    used[k] = false;
                    out.remove(&s.rule_idx);
                }
            }
            false
        }
        let mut forced = HashMap::new();
        if !assign(0, &need, &slots, &mut vec![false; slots.len()], &mut forced) {
            let bad: Vec<String> = need.iter().filter(|a| !slots.iter().any(|s| s.candidates.contains(a))).map(|a| format!("{} {}", a, self.affix_attr_name(*a))).collect();
            return Err(if bad.is_empty() {
                format!("properties {:?} do not fit into the slots of {} at the same time (see affixes {})", need, item_id, item_id)
            } else {
                format!("never on {}: {} (see affixes {})", item_id, bad.join(", "), item_id)
            });
        }
        let mut data = self.build_item_affix_data_forced(item_id, wear_part, seed, &forced);
        for a in data.iter_mut() {
            for (aid, c) in a.attribute_random.iter_mut() {
                let want = wishes.iter().find(|w| w.0 == *aid).map(|w| w.1).or(max_all.then_some(9999));
                if let Some(lvl) = want {
                    c.random_data.iter_mut().for_each(|r| r.random_level = lvl);
                }
            }
            for (gid, c) in a.gear_set_random.iter_mut() {
                let want = wishes.iter().find(|w| w.0 == *gid).map(|w| w.1).or(max_all.then_some(9999));
                if let Some(lvl) = want {
                    c.random_data.iter_mut().for_each(|r| r.random_level = lvl);
                }
            }
            if max_all {
                a.ability_set_random.values_mut().flat_map(|c| c.random_data.iter_mut()).for_each(|r| r.random_level = 9999);
            }
        }
        Ok(data)
    }

    /// Lines for GM: item affixes (type, property, level in %)
    pub(super) fn gm_affix_lines(&self, data: &[tgf::DBattleAffixRandomData]) -> Vec<String> {
        let kind = |t: i32| match t {
            5 => "BASIC",
            10 => "MAJOR",
            20 => "MINOR",
            30 => "LEGENDARY",
            _ => "?",
        };
        let pct = |lvl: i32| lvl as f64 / 99.99;
        let mut out = Vec::new();
        for a in data {
            let mut ids: Vec<(i32, i32)> = a
                .attribute_random
                .iter()
                .map(|(k, c)| (*k, c.random_data.first().map_or(0, |r| r.random_level)))
                .chain(a.gear_set_random.iter().map(|(k, c)| (*k, c.random_data.first().map_or(0, |r| r.random_level))))
                .collect();
            ids.sort_unstable();
            for (aid, lvl) in ids {
                out.push(format!("{} {} {} {:.0}%", kind(a.affix_type), aid, self.affix_attr_name(aid), pct(lvl)));
            }
            for (alias, c) in &a.ability_set_random {
                out.push(format!("{} {} {:.0}%", kind(a.affix_type), alias, pct(c.random_data.first().map_or(0, |r| r.random_level))));
            }
        }
        out
    }

    /// GM `affixes <type_id>`: what is always on the item and what can be put into each minor property slot
    pub(super) fn gm_affix_options(&self, item_id: i32, wear_part: i32) -> Result<Vec<String>, String> {
        if !self.game_data.affix_template.contains_key(&item_id) {
            return Err(format!("{} has no affix template (not gear?)", item_id));
        }
        let base = self.build_item_affix_data_forced(item_id, wear_part, 0, &HashMap::new());
        let fixed = Self::affix_fixed_attrs(&base);
        let mut f: Vec<i32> = fixed.iter().copied().collect();
        f.sort_unstable();
        let mut out = vec![format!("always present (level only): {}", f.iter().map(|a| format!("{} {}", a, self.affix_attr_name(*a))).collect::<Vec<_>>().join(", "))];
        for (n, s) in self.affix_minor_slots(item_id, &fixed).iter().enumerate() {
            out.push(format!("slot {}: {}", n + 1, s.candidates.iter().map(|a| format!("{} {}", a, self.affix_attr_name(*a))).collect::<Vec<_>>().join(", ")));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod set_affix_tests {
    use super::*;

    #[test]
    fn weighted_pick_by_weights() {
        // equal weights — as the old roll % len
        for roll in [0, 1, 4999, 9999] {
            assert_eq!(GatewaySession::weighted_pick(&[10, 10, 10], roll), roll as usize % 3);
        }
        // 10/10/2: the last one — only the top 2/22 of the range
        let w = [10, 10, 2];
        let n = (0..10000).filter(|r| GatewaySession::weighted_pick(&w, *r) == 2).count();
        assert_eq!(n, 909);
        assert_eq!(GatewaySession::weighted_pick(&[0, 5], 0), 1);
    }

    /// 09.11 (playtest season, tools/season_set_affixes.py): blue and purple items — at most one gear set bonus,
    /// no Midas (11009), chance ~1/number of property sets; 13.11 — Kleptomancy is rare (weight 2 vs 10); green
    /// weapons — Midas ~1 in 5
    #[tokio::test]
    async fn set_affixes_on_blue_and_purple() {
        let dir = format!("target/test_setaffix_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "setaffix".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let sets_of = |item: i32, seed: u64| -> Vec<i32> {
            s.build_item_affix_data_seeded(item, 0, seed).iter().flat_map(|a| a.gear_set_random.keys().copied()).collect()
        };
        let rarity = |t: i32| s.item_rarity(t);
        // all blue/purple items with pool 100802
        let items: Vec<i32> = s
            .game_data
            .affix_template
            .iter()
            .filter(|(_, t)| t.data.as_ref().is_some_and(|d| d.random_rule.iter().any(|r| r.pool_random.iter().any(|p| p.affix_pool_id.contains(&100802)))))
            .map(|(id, _)| *id)
            .collect();
        if items.is_empty() {
            eprintln!("ExcelConfig without pool 100802 (not the playtest season) — skipped");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }
        // 11.11: for purple items the gear set pool exists only for bosses and elites (affix_monster_pool_rule), not in their own list —
        // vendors and chests give purple items without a gear set bonus; in the templates it remained on 37 blue ones
        assert_eq!(items.len(), 37);
        let (mut with_set, mut total, mut klepto) = (0usize, 0usize, 0usize);
        for &it in &items {
            assert!(matches!(rarity(it), 4), "{} — not blue", it);
            for seed in 1..=400u64 {
                let sets = sets_of(it, seed * 7919);
                assert!(sets.len() <= 1, "{}: more than one gear set bonus {:?}", it, sets);
                assert!(!sets.iter().any(|g| *g == 11009), "{}: Midas {:?}", it, sets);
                klepto += sets.iter().filter(|g| **g == 11002).count();
                with_set += sets.len();
                total += 1;
            }
        }
        let share = with_set as f64 / total as f64;
        assert!((0.12..0.19).contains(&share), "share of items with a bonus {:.3}", share);
        // 13.11: Kleptomancy ("Tipsy Alchemist") — weight 2 of 72 in 100802, ~2.8% of gear set bonuses
        let k = klepto as f64 / with_set as f64;
        assert!((0.01..0.05).contains(&k), "share of Kleptomancy among bonuses {:.3} ({} of {})", k, klepto, with_set);
        // green weapons with Midas: set 1 in 5
        let green = 181012000;
        let midas = (1..=2000u64).filter(|seed| sets_of(green, seed * 104_729).contains(&11009)).count() as f64 / 2000.0;
        assert!((0.15..0.25).contains(&midas), "Midas on green {:.3}", midas);
        // grey and others without the pool — no bonuses
        assert!(sets_of(181010000, 5).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 15.11 GM `itemaff`: empty wishes — the same roll; a MINOR property goes into a slot with the required level and is not
    /// repeated; a foreign property — rejected; max — all levels 9999
    #[tokio::test]
    async fn gm_affix_wishes() {
        let dir = format!("target/test_gmaffix_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "gmaffix".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let levels = |d: &[tgf::DBattleAffixRandomData]| -> Vec<(i32, i32, i32)> {
            let mut v: Vec<(i32, i32, i32)> = d
                .iter()
                .flat_map(|a| {
                    a.attribute_random
                        .iter()
                        .map(|(k, c)| (a.affix_type, *k, c.random_data[0].random_level))
                        .chain(a.gear_set_random.iter().map(|(k, c)| (a.affix_type, *k, c.random_data[0].random_level)))
                        .collect::<Vec<_>>()
                })
                .collect();
            v.sort_unstable();
            v
        };
        // Luxurious ring of strength (purple) and an epic longsword — jewelry and a weapon
        for (item, wp) in [(185014000, 0), (181024000, 1)] {
            assert!(s.game_data.affix_template.contains_key(&item), "{} without a template", item);
            for seed in [1u64, 77, 123_456_789] {
                // without wishes — exactly the normal roll
                let plain = s.build_item_affix_data_seeded(item, wp, seed);
                assert_eq!(levels(&s.gm_affix_data(item, wp, seed, &[], false).unwrap()), levels(&plain));
                let fixed = GatewaySession::affix_fixed_attrs(&plain);
                let slots = s.affix_minor_slots(item, &fixed);
                assert!(!slots.is_empty(), "{}: no MINOR slots", item);
                // one property from each slot (different ones), level 50%
                let mut wishes = Vec::new();
                for sl in &slots {
                    if let Some(a) = sl.candidates.iter().rev().find(|a| !wishes.iter().any(|w: &(i32, i32)| w.0 == **a)) {
                        wishes.push((*a, 5000));
                    }
                }
                let d = s.gm_affix_data(item, wp, seed, &wishes, false).unwrap();
                let got = levels(&d);
                for (aid, lvl) in &wishes {
                    let hits: Vec<_> = got.iter().filter(|g| g.1 == *aid).collect();
                    assert_eq!(hits.len(), 1, "{}: property {} occurs {:?}", item, aid, hits);
                    assert_eq!(hits[0].2, *lvl);
                }
                // as many MINOR slots as in the normal roll
                let minors = |v: &[(i32, i32, i32)]| v.iter().filter(|g| g.0 == 20).count();
                assert_eq!(minors(&got), minors(&levels(&plain)));
                // max: everything 9999
                let m = s.gm_affix_data(item, wp, seed, &wishes[..1], true).unwrap();
                for (_, aid, lvl) in levels(&m) {
                    assert_eq!(lvl, if aid == wishes[0].0 { 5000 } else { 9999 });
                }
            }
            // a property that never occurs on the item, and extra slots — rejected
            assert!(s.gm_affix_data(item, wp, 1, &[(5101, 9999)], false).is_err());
            assert!(s.gm_affix_data(item, wp, 1, &[(1001, 1), (1001, 2)], false).is_err());
        }
        // property names
        assert_eq!(s.affix_attr_parse("first_level_strength"), Some(1001));
        assert_eq!(s.affix_attr_parse("affix_first_level_strength"), Some(1001));
        assert_eq!(s.affix_attr_parse("gear_set_kleptomancy"), Some(11002));
        assert!(s.gm_affix_options(185014000, 0).unwrap().len() >= 2);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
