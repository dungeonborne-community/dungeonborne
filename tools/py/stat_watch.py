#!/usr/bin/env python3
"""Combat math watcher: expected hero attributes (our side) versus actual ones (DS mod).

Run (from the backups folder):
    python -X utf8 tools/stat_watch.py expected [--role GUID | --open OPEN_ID] [--dump rpc_dump/OUR.jsonl]
    python -X utf8 tools/stat_watch.py report [--log <stand>/run/statwatch-*.jsonl] [--out FILE.md]
    python -X utf8 tools/stat_watch.py selftest      # expected values for all roles of the dump — checks that everything computes

Expected (from balance/*.json, as the game sees them — pak zzzzzz_Balance_P):
  * role base — deploy_role.role_affix.attribute (the backend builds it from role_level_upgrade);
  * BASIC — fixed_rule of the affix_template: offset {min,max,unit} with the item's random_level, fixed_value — as is;
  * MAJOR/MINOR — random_level 0..9999 -> affix_value_template[(the template's affix_value_id, attribute)].rarity_range[rarity],
    rarity = UeSource_ItemType[item_type.ue_source_type_id].rarity (ROTTED=1 … EXCEPTIONAL=8);
  * value (CONFIRMED 05.11 on DS) = min + floor(level × n / 10000) × unit, n = (max − min)/unit + 1 — a uniform
    choice among discrete values, the maximum is reachable (bucket mode). The linear formula min + (max − min) × level / 10000
    with rounding (raw/floor/round/ceil/rtot) is kept for comparison — it does not match the game;
  * set points (gear_set_random) — affix_value_template of the same kind, summed over what is equipped -> level by
    affix_gear_set_rule.unlock_level; legendaries — affix_legendary_rule (ability_set). These are abilities, not GAS attributes:
    they are not visible in snapshots, the report only lists them.
Attribute id -> GAS set field — by map_attribute_offsets (src/rpc/handlers/affix.rs) and the set field names
(CXXHeaderDump LyraGame.hpp). Where the field depends on the hand/slot (weapon damage and attack level, defense level, block) —
the mapping is a HYPOTHESIS, marked "hand/slot".
Secondary attributes from primary ones (Strength, Agility…) — from the mod's flcfg record (the game's FirstLevelAttrDataAsset).
"""
from __future__ import annotations
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, STAND, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)

import argparse
import glob
import json
import math
import os
import statistics
import sys
from collections import defaultdict

BAL = str(W("balance"))
DUMP = str(W("rpc_dump/OUR.jsonl"))
RUN = str(STAND / "run")  # stand StatWatch logs (local.toml [stand] work)

FL, ATK, DEF, SPD, REC = ("TgfFirstLevelAttributeSet", "TgfAttackAttributeSet", "TgfDefendAttributeSet",
                          "TgfSpeedAttributeSet", "TgfRecoverAttributeSet")
MAG, SOUL, NAT, BUF, STE, TRG = ("TgfMagicAttributeSet", "TgfSoulEnergyAttributeSet", "TgfNaturalEnergyAttributeSet",
                                 "TgfBuffArgAttributeSet", "TgfStatEffectAttributeSet", "TgfTriggerEffectAttributeSet")
ELEM = ["None", "Fire", "Ice", "Thunder", "Light", "Dark"]
FIRST = ["Strength", "Dexterity", "Stamina", "Intelligence", "Will", "Faith"]

RARITY = {"EN_ITEM_RARITY_ROTTED": 1, "EN_ITEM_RARITY_NORMAL": 2, "EN_ITEM_RARITY_UNCOMMON": 3,
          "EN_ITEM_RARITY_ENHANCED": 4, "EN_ITEM_RARITY_EPIC": 5, "EN_ITEM_RARITY_LEGENDARY": 6,
          "EN_ITEM_RARITY_UNIQUE": 7, "EN_ITEM_RARITY_EXCEPTIONAL": 8}
AFFIX_TYPE = {"EN_BATTLE_AFFIX_TYPE_BASIC": 5, "EN_BATTLE_AFFIX_TYPE_MAJOR": 10, "EN_BATTLE_AFFIX_TYPE_MINOR": 20,
              "EN_BATTLE_AFFIX_TYPE_LEGENDARY": 30}
LOCATION = {"EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_NONE": 0, "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_EQUIP": 1,
            "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_INVENTORY": 2, "EN_BATTLE_AFFIX_EFFECTIVE_LOCATION_ROLE_HOLD": 3}
PART_NAME = {1: "pair1 right", 2: "pair1 left", 3: "pair2 right", 4: "pair2 left", 11: "torso", 12: "legs",
             13: "helmet", 14: "shoes", 15: "gloves", 16: "amulet", 17: "ring L", 18: "ring R",
             101: "extra weapon 1", 102: "extra weapon 2"}

# attribute id (battle_attribute_description / map_attribute_offsets) -> field name in the template (proto) and target in GAS.
# Target: a list of (set, field) or a rule string "hand:<suffix>" / "slot:def" (depends on the item's slot).
ATTR = {
    1001: ("first_level_strength", [(FL, "Strength")]), 1002: ("first_level_dexterity", [(FL, "Dexterity")]),
    1003: ("first_level_stamina", [(FL, "Stamina")]), 1004: ("first_level_intelligence", [(FL, "Intelligence")]),
    1005: ("first_level_will", [(FL, "Will")]), 1006: ("first_level_faith", [(FL, "Faith")]),
    1021: ("all_first_level", [(FL, f) for f in FIRST]),
    1101: ("attack_right_weapon_attack", "hand:WeaponAttack"), 1102: ("attack_attack_bonus", [(ATK, "AttackBonus")]),
    1103: ("attack_attack_boost_rate", [(ATK, "AttackBoostRate")]), 1104: ("attack_critical_rate", [(ATK, "CriticalRate")]),
    1105: ("attack_crit_damage_rate", [(ATK, "CritDamageRate")]),
    1106: ("attack_element_boost_rate_none", [(ATK, "ElementBoostRateNone")]),
    1107: ("attack_element_boost_rate_all", [(ATK, "ElementBoostRate" + e) for e in ELEM[1:]]),
    1108: ("attack_element_boost_rate_fire", [(ATK, "ElementBoostRateFire")]),
    1109: ("attack_element_boost_rate_ice", [(ATK, "ElementBoostRateIce")]),
    1110: ("attack_element_boost_rate_thunder", [(ATK, "ElementBoostRateThunder")]),
    1111: ("attack_element_boost_rate_light", [(ATK, "ElementBoostRateLight")]),
    1112: ("attack_element_boost_rate_dark", [(ATK, "ElementBoostRateDark")]),
    1113: ("attack_def_ignore_base", [(ATK, "DefIgnoreBase")]),
    1114: ("attack_physical_penetration", [(ATK, "ElementResPierceNone")]),
    1115: ("attack_fire_penetration", [(ATK, "ElementResPierceFire")]),
    1116: ("attack_cold_penetration", [(ATK, "ElementResPierceIce")]),
    1117: ("attack_lightning_penetration", [(ATK, "ElementResPierceThunder")]),
    1118: ("attack_divine_penetration", [(ATK, "ElementResPierceLight")]),
    1119: ("attack_shadow_penetration", [(ATK, "ElementResPierceDark")]),
    1120: ("attack_right_att_gear_level", "hand:AttGearLevel"),
    1121: ("attack_extra_att_gear_level", [(ATK, "ExtraAttGearLevel")]),
    1122: ("attack_skill_boost_rate", [(ATK, "SkillBoostRate")]),
    2001: ("defend_max_health_base", [(DEF, "MaxHealthBase")]),
    2002: ("defend_max_health_percent", [(DEF, "MaxHealthPercent")]),
    2003: ("defend_max_health_extra", [(DEF, "MaxHealthExtra")]),
    2004: ("defend_def_gear_level", "slot:DefGearLevel"),
    2005: ("defend_extra_def_gear_level", [(DEF, "ExtraDefGearLevel")]),
    2006: ("defend_guarded_damage_reduce_rate", "hand:GuardedDamageReduceRate"),
    2007: ("defend_element_resistance_rate_none", [(DEF, "ElementResistanceRateNone")]),
    2008: ("defend_element_resistance_rate_all", [(DEF, "ElementResistanceRate" + e) for e in ELEM[1:]]),
    2009: ("defend_element_resistance_rate_fire", [(DEF, "ElementResistanceRateFire")]),
    2010: ("defend_element_resistance_rate_ice", [(DEF, "ElementResistanceRateIce")]),
    2011: ("defend_element_resistance_rate_thunder", [(DEF, "ElementResistanceRateThunder")]),
    2012: ("defend_element_resistance_rate_light", [(DEF, "ElementResistanceRateLight")]),
    2013: ("defend_element_resistance_rate_dark", [(DEF, "ElementResistanceRateDark")]),
    2014: ("defend_extra_guarded_damage_reduce_rate", [(DEF, "ExtraGuardedDamageReduceRate")]),
    3001: ("speed_move_speed_base", [(SPD, "MoveSpeedBase")]),
    3002: ("speed_level_interact_speed_rate", [(SPD, "LevelInteractSpeedRate")]),
    4001: ("recover_life_leach_rate", [(REC, "LifeLeachRate")]), 4002: ("recover_heal_boost_rate", [(REC, "HealBoostRate")]),
    4003: ("recover_heal_on_hit_base", [(REC, "HealOnHit")]), 4004: ("recover_heal_on_kill_base", [(REC, "HealOnKill")]),
    4005: ("recover_heal_over_time_base", [(REC, "HealOverTime")]),
    4006: ("recover_shield_boost_rate", [(REC, "ShieldBoostRate")]), 4007: ("recover_heal_wound_rate", [(REC, "HealWoundRate")]),
    4008: ("recover_skill_cooldown_scale_rate", [(REC, "SkillCooldownScaleRate")]),
    4009: ("recover_gain_gold_out_of_portal", [(REC, "GainGoldOutOfPortal")]),
    4010: ("recover_exp_boost_rate_out_of_portal", [(REC, "ExpBoostRateOutOfPortal")]),
    5001: ("magic_max_magic_base", [(MAG, "MaxMagicBase")]), 5002: ("magic_magic_over_time", [(MAG, "MagicOverTime")]),
    5003: ("magic_magic_on_hit", [(MAG, "MagicOnHit")]), 5004: ("magic_magic_on_kill", [(MAG, "MagicOnKill")]),
    5005: ("magic_magic_on_hurt", [(MAG, "MagicOnHurt")]), 5006: ("magic_magic_generate_rate", [(MAG, "MagicGenerateRate")]),
    5101: ("soul_energy_max_soul_energy_base", [(SOUL, "MaxSoulEnergyBase")]),
    5102: ("soul_energy_soul_energy_on_hit", [(SOUL, "SoulEnergyOnHit")]),
    5103: ("soul_energy_soul_energy_on_kill", [(SOUL, "SoulEnergyOnKill")]),
    5104: ("soul_energy_soul_energy_on_hurt", [(SOUL, "SoulEnergyOnHurt")]),
    5105: ("soul_energy_soul_energy_generate_rate", [(SOUL, "SoulEnergyGenerateRate")]),
    5201: ("natural_energy_max_natural_energy_base", [(NAT, "MaxNaturalEnergyBase")]),
    5202: ("natural_energy_natural_energy_over_time", [(NAT, "NaturalEnergyOverTime")]),
    5203: ("natural_energy_natural_energy_on_hit", [(NAT, "NaturalEnergyOnHit")]),
    5204: ("natural_energy_natural_energy_on_kill", [(NAT, "NaturalEnergyOnKill")]),
    5205: ("natural_energy_natural_energy_on_hurt", [(NAT, "NaturalEnergyOnHurt")]),
    5206: ("natural_energy_natural_energy_generate_rate", [(NAT, "NaturalEnergyGenerateRate")]),
    6001: ("buff_arg_killing_max_layer_max_health_percent", [(BUF, "KillingMaxLayerMaxHealthPercent")]),
    6002: ("buff_arg_killing_max_layer_attack_boost_rate", [(BUF, "KillingMaxLayerAttackBoostRate")]),
    6003: ("buff_arg_killing_max_layer_critical_rate", [(BUF, "KillingMaxLayerCriticalRate")]),
    6101: ("stat_effect_move_speed_with_hit_running", [(STE, "MoveSpeedWithHitRunning")]),
    6102: ("stat_effect_heal_over_time_with_hit_running", [(STE, "HealOverTimeWithHitRunning")]),
    6103: ("stat_effect_critical_rate_with_hit_running", [(STE, "CriticalRateWithHitRunning")]),
    6104: ("stat_effect_life_leach_rate_with_hit_running", [(STE, "LifeLeachRateWithHitRunning")]),
    6105: ("stat_effect_attack_boost_rate_with_full_health", [(STE, "AttackBoostRateWithFullHealth")]),
    6106: ("stat_effect_critical_rate_with_full_health", [(STE, "CriticalRateWithFullHealth")]),
    6201: ("trigger_effect_heal_on_open_chest", [(TRG, "HealOnOpenChest")]),
    6202: ("trigger_effect_magic_on_open_chest", [(TRG, "MagicOnOpenChest")]),
    6203: ("trigger_effect_gain_gold_on_open_chest", [(TRG, "GainGoldOnOpenChest")]),
    6204: ("trigger_effect_gain_gold_on_kill", [(TRG, "GainGoldOnKill")]),
    6205: ("trigger_effect_heal_on_defend_success", [(TRG, "HealOnDefendSuccess")]),
    6206: ("trigger_effect_damage_on_defend_success", [(TRG, "DamageOnDefendSuccess")]),
    6207: ("trigger_effect_additional_fixed_damage_on_causing_hit", [(TRG, "AdditionalFixedDamageOnCausingHit")]),
    6208: ("trigger_effect_add_shield_on_cast_skill", [(TRG, "AddShieldOnCastSkill")]),
    6209: ("trigger_effect_additional_element_none_damage_on_causing_hit", [(TRG, "AdditionalElementNoneDamageOnCausingHit")]),
    6210: ("trigger_effect_additional_element_fire_damage_on_causing_hit", [(TRG, "AdditionalElementFireDamageOnCausingHit")]),
    # 6211/6212 — by the field numbers of proto DBattleAttributeOffset (this is how DS applies them, verified 05.11: id 6211 -> Ice);
    # battle_attribute_description and affix.rs::map_attribute_offsets swap them (FIX_LIST BAL3)
    6211: ("trigger_effect_additional_element_ice_damage_on_causing_hit", [(TRG, "AdditionalElementIceDamageOnCausingHit")]),
    6212: ("trigger_effect_additional_element_thunder_damage_on_causing_hit", [(TRG, "AdditionalElementThunderDamageOnCausingHit")]),
    6213: ("trigger_effect_additional_element_light_damage_on_causing_hit", [(TRG, "AdditionalElementLightDamageOnCausingHit")]),
    6214: ("trigger_effect_additional_element_dark_damage_on_causing_hit", [(TRG, "AdditionalElementDarkDamageOnCausingHit")]),
}
FIELD2ID = {v[0]: k for k, v in ATTR.items()}
GEAR_SETS = {11001: "fleet_foot_work", 11002: "kleptomancy", 11003: "undying_grasp", 11004: "thunder_law",
             11005: "bone_plating", 11006: "avoid_death", 11007: "scorching_touch", 11008: "summoning_aery",
             11009: "coin_master (Midas Touch)"}
# bucket — the game's formula (confirmed 05.11 on a live DS: 34 of 34 hero attributes exactly): the roll uniformly picks one
# of n = (max − min)/unit + 1 discrete values, idx = floor(level × n / 10000), value = min + idx × unit.
# raw/floor/round/ceil — linear formula min + (max − min) × level / 10000 with rounding (NOT how the game does it, for comparison).
ROUNDINGS = ("bucket", "raw", "floor", "round", "ceil")
# rtot — no rounding per item, the sum of rolls is rounded to unit once
MODES = ROUNDINGS + ("rtot",)


def targets_of(aid: int, part: int):
    """(set, field, hypothesis?) for attribute aid of an item in slot part."""
    rule = ATTR.get(aid, (None, None))[1]
    if rule is None:
        return [], True
    if isinstance(rule, list):
        return rule, False
    kind, suffix = rule.split(":")
    if kind == "hand":
        side = {1: "Right", 3: "Right", 2: "Left", 4: "Left", 101: "Additional", 102: "Additional"}.get(part, "Right")
        if suffix == "GuardedDamageReduceRate" and side == "Additional":
            side = "Extra"
        if suffix == "WeaponAttack" or suffix == "AttGearLevel":
            return [(ATK, side + suffix)], True
        return [(DEF, side + suffix)], True
    # slot:DefGearLevel. Observation 05.11 (DS, 2 heroes): defense level from gloves, shoes, amulet and rings does not get
    # into GAS (ExtraDefGearLevel = only MINOR 2005), TgfDefendAttributeSet has no fields for these slots
    slot = {1: "Right", 3: "Right", 2: "Left", 4: "Left", 11: "Chest", 12: "Trousers", 13: "Helmet"}.get(part)
    if slot is None:
        return [], True
    return [(DEF, slot + "DefGearLevel")], True


# ------------------------------------------------------------------------------------------------------------------
# Tables
def load(name):
    with open(os.path.join(BAL, name + ".json"), encoding="utf8") as f:
        return json.load(f)["rows"]


class Tables:
    def __init__(self):
        self.tpl = {r["item_id"]: r for r in load("affix_template")}
        self.val = {}
        for r in load("affix_value_template"):
            d = r["data"]
            for a in d.get("affix_data", []):
                self.val[(r["affix_value_id"], r["attribute_type"], AFFIX_TYPE[a["affix_type"]])] = (
                    a.get("rarity_range", {}), d.get("unit", 1.0))
        self.item = {r["type_id"]: r for r in load("item_type")}
        self.ue = {r["type_id"]: r for r in load("UeSource_ItemType")}
        self.gear_rule = defaultdict(list)
        for r in load("affix_gear_set_rule"):
            self.gear_rule[r["gear_set_type"]].append((r["unlock_level"], r["level"], r.get("text", "")))
        self.legend = {r["affix_alias_id"]: r.get("offset", {}) for r in load("affix_legendary_rule")}
        self.names = {}
        p = str(W("names/strings_en.tsv"))
        if os.path.exists(p):
            with open(p, encoding="utf8") as f:
                for line in f:
                    parts = line.rstrip("\n").split("\t")
                    if len(parts) >= 3:
                        self.names[parts[1]] = parts[2]

    def rarity(self, type_id):
        it = self.item.get(type_id, {})
        ue = self.ue.get(it.get("ue_source_type_id", type_id), {})
        return RARITY.get(ue.get("rarity"), 0), ue.get("ue_path", "")

    def item_name(self, type_id):
        return self.names.get(f"Item_Name_{type_id}", str(type_id))

    def attr_name(self, aid):
        f = ATTR.get(aid, ("?",))[0]
        return self.names.get("affix_" + f, f)


def quantize(raw, unit, mode):
    if mode == "raw" or not unit:
        return raw
    q = raw / unit
    q = {"floor": math.floor(q + 1e-9), "round": math.floor(q + 0.5), "ceil": math.ceil(q - 1e-9)}[mode]
    return q * unit


def roll_value(lo, hi, unit, level):
    raw = lo + (hi - lo) * level / 10000.0
    d = {m: quantize(raw, unit, m) for m in ROUNDINGS if m != "bucket"}
    if hi == lo or not unit:
        d["bucket"] = lo if hi == lo else raw
    else:
        n = int(round((hi - lo) / unit)) + 1
        d["bucket"] = lo + min(n - 1, (int(level) * n) // 10000) * unit
    d["unit"] = unit if lo != hi else None  # roll step (None — a value without spread)
    return d


# ------------------------------------------------------------------------------------------------------------------
# Equipment from the dump
def find_roles(dump=DUMP):
    """role_guid -> {deploy_role, open, ts, method} — the latest by time; battle_fetch takes priority (this is what DS saw)."""
    roles = {}
    with open(dump, encoding="utf8", errors="replace") as f:
        for line in f:
            if '"deploy_role"' not in line or '"S2C"' not in line:
                continue
            try:
                o = json.loads(line)
            except ValueError:
                continue
            method = o.get("method", "").rsplit(".", 1)[-1]
            stack = [o.get("body")]
            while stack:
                x = stack.pop()
                if isinstance(x, dict):
                    dr = x.get("deploy_role")
                    if isinstance(dr, dict) and dr.get("basic", {}).get("role_guid"):
                        g = int(dr["basic"]["role_guid"])
                        prev = roles.get(g)
                        rec = {"deploy_role": dr, "open": o.get("who"), "ts": o["ts_ms"], "method": method}
                        if prev is None or o["ts_ms"] >= prev["ts"]:
                            rec["hist"] = (prev or {}).get("hist", [])
                            roles[g] = rec
                        roles[g].setdefault("hist", []).append(rec)
                    stack.extend(v for v in x.values() if isinstance(v, (dict, list)))
                elif isinstance(x, list):
                    stack.extend(v for v in x if isinstance(v, (dict, list)))
    return roles


def item_contribs(T: Tables, item, active_pair=1):
    """Contributions of one equipped item: list of dict(aid, value{mode}, kind, loc, note, active)."""
    b = item.get("basic", {})
    tid, part = int(b.get("type_id", 0)), int(b.get("position", {}).get("x", 0))
    rar, ue_path = T.rarity(tid)
    tpl = T.tpl.get(tid, {}).get("data", {})
    vid = tpl.get("affix_value_id")
    out, notes = [], []
    pair = 1 if part in (1, 2) else 2 if part in (3, 4) else 0
    held = pair == 0 or pair == active_pair

    def add(aid, vals, kind, loc, note=""):
        active = loc != 3 or held
        if loc == 2:
            active = False
            note = (note + " ROLE_INVENTORY: inventory only").strip()
        out.append(dict(aid=aid, v=vals, kind=kind, loc=loc, note=note, active=active, part=part, tid=tid))

    affix = item.get("data", {}).get("affix_data", []) or []
    rolled = defaultdict(dict)  # (type, location) -> {aid: level}
    gear_pts = []
    legend = []
    for a in affix:
        at = AFFIX_TYPE.get(a.get("affix_type"), 0)
        loc = LOCATION.get(a.get("location"), 1)
        for k, rv in (a.get("attribute_random") or {}).items():
            lvl = int(((rv.get("random_data") or [{}])[0]).get("random_level", 0))
            rolled[(at, loc)][int(k)] = lvl
        for k, rv in (a.get("gear_set_random") or {}).items():
            lvl = int(((rv.get("random_data") or [{}])[0]).get("random_level", 0))
            gear_pts.append((int(k), at, lvl))
        for k in (a.get("ability_set_random") or {}):
            legend.append(int(k))

    # BASIC: fixed_rule of the template (offset — by random_level, fixed_value — always)
    for fr in tpl.get("fixed_rule", []):
        if AFFIX_TYPE.get(fr.get("affix_type")) != 5:
            continue
        loc = LOCATION.get(fr.get("location"), 1)
        for field, spec in (fr.get("attribute") or {}).items():
            aid = FIELD2ID.get(field)
            if aid is None:
                notes.append(f"no id for field {field}")
                continue
            if "offset" in spec:
                o = spec["offset"]
                lvl = rolled.get((5, loc), {}).get(aid)
                if lvl is None:
                    lvl = next((v.get(aid) for (t, _), v in rolled.items() if t == 5 and aid in v), None)
                if lvl is None:
                    add(aid, roll_value(o["min_value"], o["max_value"], o.get("unit", 1), 0), "BASIC", loc,
                        "no random_level in affix_data — 0 taken")
                else:
                    add(aid, roll_value(o["min_value"], o["max_value"], o.get("unit", 1), lvl), "BASIC", loc,
                        f"lvl {lvl}")
            elif "fixed_value" in spec:
                v = spec["fixed_value"]
                if v:
                    add(aid, dict({m: v for m in ROUNDINGS}, unit=None), "BASIC-fixed", loc)
    # MAJOR / MINOR
    for (at, loc), m in rolled.items():
        if at not in (10, 20):
            continue
        for aid, lvl in m.items():
            vt = T.val.get((vid, aid, at))
            if not vt:
                add(aid, {mm: 0.0 for mm in ROUNDINGS}, {10: "MAJOR", 20: "MINOR"}[at], loc,
                    f"NO affix_value_template ({vid},{aid}) — value 0")
                continue
            rr, unit = vt
            r = rr.get(str(rar))
            if not r:
                add(aid, {mm: 0.0 for mm in ROUNDINGS}, {10: "MAJOR", 20: "MINOR"}[at], loc,
                    f"no rarity_range[{rar}]")
                continue
            add(aid, roll_value(r["min_value"], r["max_value"], unit, lvl), {10: "MAJOR", 20: "MINOR"}[at], loc,
                f"lvl {lvl} r{rar} [{r['min_value']}..{r['max_value']}] unit {unit}")
    # set points
    gears = []
    for gid, at, lvl in gear_pts:
        vt = T.val.get((vid, gid, at if at in (10, 20) else 10))
        pts = {mm: 0.0 for mm in ROUNDINGS}
        if vt and vt[0].get(str(rar)):
            r = vt[0][str(rar)]
            pts = roll_value(r["min_value"], r["max_value"], vt[1], lvl)
        gears.append(dict(gid=gid, pts=pts, lvl=lvl, part=part))
    return dict(tid=tid, part=part, rarity=rar, ue_path=ue_path, vid=vid, contribs=out, gear=gears,
                legend=[(x, T.legend.get(x, {}).get("ability_set_id")) for x in legend], notes=notes,
                has_tpl=bool(tpl))


def expected_for(T: Tables, dr, active_pair=1):
    """Expected GAS field values: {(set, field): {mode: value}}, contributions, sets, legendaries."""
    exp = defaultdict(lambda: {m: 0.0 for m in MODES})
    rolled_raw = defaultdict(lambda: [0.0, None])  # (set, field) -> [sum of raw rolls, unit]
    src = defaultdict(list)
    hyp = set()
    for k, v in ((dr.get("role_affix") or {}).get("attribute") or {}).items():
        aid = int(k)
        tg, h = targets_of(aid, 1)
        for t in tg:
            for m in MODES:
                exp[t][m] += float(v)
            src[t].append(("role base", aid, float(v)))
            if h:
                hyp.add(t)
    items = []
    gear_total = defaultdict(lambda: {m: 0.0 for m in ROUNDINGS})
    for it in (dr.get("wearing") or {}).get("items", []) or []:
        ic = item_contribs(T, it, active_pair)
        items.append(ic)
        for c in ic["contribs"]:
            if not c["active"]:
                continue
            tg, h = targets_of(c["aid"], c["part"])
            for t in tg:
                for m in ROUNDINGS:
                    exp[t][m] += c["v"][m]
                if c["v"].get("unit"):
                    rolled_raw[t][0] += c["v"]["raw"]
                    rolled_raw[t][1] = c["v"]["unit"]
                else:
                    exp[t]["rtot"] += c["v"]["raw"]
                src[t].append((f"{T.item_name(c['tid'])} [{PART_NAME.get(c['part'], c['part'])}] {c['kind']}",
                               c["aid"], c["v"]["raw"]))
                if h:
                    hyp.add(t)
        for g in ic["gear"]:
            for m in ROUNDINGS:
                gear_total[g["gid"]][m] += g["pts"][m]
    for t, (raw, unit) in rolled_raw.items():
        exp[t]["rtot"] += quantize(raw, unit, "round")
    gear_levels = {}
    for gid, pts in gear_total.items():
        lvls = {}
        for m in ROUNDINGS:
            lv = 0
            for unlock, level, _ in sorted(T.gear_rule.get(gid, [])):
                if pts[m] + 1e-9 >= unlock:
                    lv = max(lv, level)
            lvls[m] = lv
        gear_levels[gid] = (pts, lvls)
    return exp, src, hyp, items, gear_levels


# ------------------------------------------------------------------------------------------------------------------
# Mod log
def read_log(paths):
    recs = []
    for p in paths:
        with open(p, encoding="utf8", errors="replace") as f:
            for line in f:
                line = line.strip()
                if not line:
                    continue
                try:
                    r = json.loads(line)
                except ValueError:
                    continue
                r["_file"] = os.path.basename(p)
                recs.append(r)
    return recs


def set_class(key):
    return key.split("/", 1)[0] if "/" in key else key.rsplit("_", 1)[0]


def flatten(sets):
    """{class/object: {field: [base, cur]}} -> {(class, field): (base, cur, key)}; duplicate classes — both."""
    out = {}
    for key, vals in sets.items():
        cls = set_class(key)
        for f, bc in vals.items():
            k = (cls, f)
            if k in out:  # a second set of the same class (Custom*): the sum goes in a separate row
                out[(cls + "#2", f)] = (bc[0], bc[1], key)
            else:
                out[k] = (bc[0], bc[1], key)
    return out


def fmt(x):
    if x is None:
        return "—"
    if isinstance(x, float):
        if abs(x) >= 100 or x == int(x):
            return f"{x:.6g}"
        return f"{x:.4g}"
    return str(x)


def close(a, b, tol=2e-5):
    """Comparison with the game's float32: absolute 2e-5 or relative 2e-6."""
    return a is not None and b is not None and abs(a - b) <= max(tol, abs(b) * 2e-6)


def derived_from_flcfg(flcfg, fl_vals):
    """Contributions of primary stats to secondary ones: {(set class, field): (value, [(primary, mul)])}."""
    out = defaultdict(lambda: [0.0, []])
    if not flcfg:
        return out
    for it in flcfg.get("items", []):
        base = it.get("base")
        bv = fl_vals.get(base)
        if bv is None:
            continue
        for a in it.get("affects", []):
            k = ((a.get("set") or "?").replace("Default__", ""), a.get("attr"))
            out[k][0] += bv * float(a.get("mul") or 0)
            out[k][1].append((base, a.get("mul")))
    return out


# ------------------------------------------------------------------------------------------------------------------
def cmd_expected(args):
    T = Tables()
    roles = find_roles(args.dump)
    sel = [g for g, r in roles.items() if (not args.role or g == args.role) and (not args.open or r["open"] == args.open)]
    if not sel:
        print("no such roles in the dump")
        return 1
    for g in sel:
        r = roles[g]
        print(render_expected(T, g, r, args.pair))
    return 0


def render_expected(T, g, r, pair=1):
    dr = r["deploy_role"]
    exp, src, hyp, items, gears = expected_for(T, dr, pair)
    L = [f"## Role {g} ({dr['basic'].get('role_name', '')}, type {dr['basic'].get('type_id')}, lvl {dr.get('role_level')}) "
         f"open {r['open']} — from {r['method']}", "", "Items:"]
    for ic in items:
        L.append(f"- [{PART_NAME.get(ic['part'], ic['part'])}] {T.item_name(ic['tid'])} ({ic['tid']}) rarity {ic['rarity']}"
                 f" value_id {ic['vid']}" + ("" if ic["has_tpl"] else " — NO affix_template"))
        for c in ic["contribs"]:
            v = c["v"]
            var = "" if len({round(v[m], 6) for m in ROUNDINGS}) == 1 else \
                " (linear " + " / ".join(f"{m} {fmt(v[m])}" for m in ROUNDINGS if m != "bucket") + ")"
            L.append(f"    - {c['kind']} {c['aid']} {T.attr_name(c['aid'])} = {fmt(v['bucket'])}{var}"
                     f"{'' if c['active'] else ' [inactive]'} {c['note']}")
        for gg in ic["gear"]:
            L.append(f"    - set {gg['gid']} {GEAR_SETS.get(gg['gid'], '')}: points {fmt(gg['pts']['bucket'])} (lvl {gg['lvl']})")
        for alias, ab in ic["legend"]:
            L.append(f"    - legendary {alias} -> ability_set {ab}")
        for n in ic["notes"]:
            L.append(f"    - ! {n}")
    if gears:
        L.append("")
        L.append("Sets (sum of points -> level by affix_gear_set_rule; an ability, not visible in attributes):")
        for gid, (pts, lv) in gears.items():
            L.append(f"- {gid} {GEAR_SETS.get(gid, '')}: points {fmt(pts['bucket'])} -> "
                     f"level {lv['bucket']} (thresholds {sorted(u for u, _, _ in T.gear_rule.get(gid, []))})")
    L += ["", "| Set.field | expected (bucket, as in the game) | linear raw | floor | round | ceil | sources |",
          "|---|---|---|---|---|---|---|"]
    for t in sorted(exp):
        e = exp[t]
        s = "; ".join(f"{a} {b}={fmt(c)}" for a, b, c in src[t])
        L.append(f"| {t[0].replace('Tgf', '').replace('AttributeSet', '')}.{t[1]}{' (hand/slot)' if t in hyp else ''} | "
                 f"{fmt(e['bucket'])} | {fmt(e['raw'])} | {fmt(e['floor'])} | {fmt(e['round'])} | {fmt(e['ceil'])} | {s} |")
    return "\n".join(L) + "\n"


def role_at(r, ts_sec):
    """Equipment DS had received by the time of the snapshot: the last battle_fetch_battle_user_data no later than ts,
    otherwise the last of any kind no later than ts, otherwise the very last."""
    # strictly before the snapshot: +60 s captured the state after the raid (death = loss of equipment), 05.11
    lim = ts_sec * 1000 if ts_sec else None
    hist = [h for h in r.get("hist", []) if lim is None or h["ts"] <= lim]
    fetch = [h for h in hist if h["method"] == "battle_fetch_battle_user_data"]
    for cand in (fetch, hist):
        if cand:
            return max(cand, key=lambda h: h["ts"])
    return r


def active_pair_from_snap(T, dr, held):
    right = (held or {}).get("right") or ""
    if not right:
        return 1
    for it in (dr.get("wearing") or {}).get("items", []) or []:
        b = it.get("basic", {})
        part = int(b.get("position", {}).get("x", 0))
        if part in (1, 3):
            _, ue = T.rarity(int(b.get("type_id", 0)))
            key = right.replace("Default__", "")
            key = key[1:] if key.startswith("WID_") else key
            key = key[:-2] if key.endswith("_C") else key
            if ue and "/" + key + "." in ue:
                return 1 if part == 1 else 2
    return 1


def cmd_report(args):
    T = Tables()
    paths = args.log or sorted(glob.glob(os.path.join(RUN, "statwatch-2*.jsonl")))[-1:]
    if not paths:
        print("no statwatch-*.jsonl logs")
        return 1
    recs = read_log(paths)
    roles = find_roles(args.dump)
    by_open = defaultdict(list)
    for g, r in roles.items():
        by_open[r["open"]].append(g)
    flcfg = next((r for r in recs if r.get("t") == "flcfg"), None)
    snaps = [r for r in recs if r.get("t") == "snap"]
    hits = [r for r in recs if r.get("t") == "hit"]
    hps = [r for r in recs if r.get("t") == "hp"]
    calls = [r for r in recs if r.get("t") == "calls"]
    hooks = next((r for r in recs if r.get("t") == "hooks"), {})
    L = [f"# Combat math check ({', '.join(os.path.basename(p) for p in paths)})", "",
         f"Records: snapshots {len(snaps)}, hits {len(hits)}, unexplained health drops {len(hps)}.",
         f"Hooks: {json.dumps(hooks.get('res', {}), ensure_ascii=False)}",
         f"Triggers (last counter): {json.dumps(calls[-1]['calls'] if calls else {}, ensure_ascii=False)}", ""]
    if flcfg:
        L.append("## Primary stats -> secondary (the game's FirstLevelAttrDataAsset)")
        for it in flcfg.get("items", []):
            L.append(f"- {it.get('base')}: " + ", ".join(f"{a.get('set')}.{a.get('attr')} ×{a.get('mul')}"
                                                       for a in it.get("affects", [])))
        L.append("")
    else:
        L.append("No flcfg record — secondary from primary are not checked.\n")

    # the first snapshot of each hero object (right after equipment is issued); later stats change due to loot, buffs, weapon swap
    first, later = {}, defaultdict(list)
    for s in snaps:
        w = s.get("who", {})
        key = (w.get("guid") or w.get("open"), w.get("name"))
        if key not in first:
            first[key] = s
        else:
            later[key].append(s)
    last = first
    findings = []
    mode_hits = defaultdict(int)
    mode_total = [0]
    for key, s in last.items():
        w = s.get("who", {})
        changes = []
        a0 = flatten(s.get("sets", {}))
        for s2 in later.get(key, []):
            a2 = flatten(s2.get("sets", {}))
            for k2, v2 in a2.items():
                v0 = a0.get(k2, (0, 0, ""))
                if not k2[1].startswith("Cur") and k2[1] not in ("CalculatedShield", "AddShield", "DeathZoneTime") \
                        and abs((v2[1] or 0) - (v0[1] or 0)) > 1e-6:
                    changes.append(f"{s2.get('ts')} {k2[0].replace('Tgf', '').replace('AttributeSet', '')}.{k2[1]} "
                                   f"{fmt(v0[1])}→{fmt(v2[1])}")
            a0 = a2
        g = int(w["guid"]) if str(w.get("guid", "")).isdigit() and int(w["guid"]) in roles else None
        if g is None and w.get("open") in by_open:
            g = max(by_open[w["open"]], key=lambda x: roles[x]["ts"])
        L.append(f"## Hero {w.get('name')} open {w.get('open')} guid {w.get('guid')} (snapshot ts {s.get('ts')}, weapon {s.get('held')})")
        act = flatten(s.get("sets", {}))
        if g is None:
            L.append("Role not found in the dump — actual values only.\n")
            continue
        r = role_at(roles[g], s.get("ts"))
        pair = active_pair_from_snap(T, r["deploy_role"], s.get("held"))
        exp, src, hyp, items, gears = expected_for(T, r["deploy_role"], pair)
        fl_cur = {f: act.get((FL, f), (None, None))[1] for f in FIRST}
        der = derived_from_flcfg(flcfg, {k: v for k, v in fl_cur.items() if v is not None})
        L.append(f"Equipment — from {r['method']} (ts {r['ts']}), active weapon pair {pair}. Checked against the hero's first snapshot.")
        if changes:
            L.append(f"Stat changes later in the raid ({len(changes)}): " + "; ".join(changes[:12]) + (" …" if len(changes) > 12 else ""))
        L += ["", "| Set.field | expected | from primary | actual Base | actual Cur | verdict | sources |",
              "|---|---|---|---|---|---|---|"]
        keys = sorted(set(exp) | set(k for k in der if k in act) | set(k for k in act if k[0] in (FL, ATK, DEF, SPD, REC, MAG, SOUL, NAT, BUF, STE, TRG)))
        for t in keys:
            e = exp.get(t)
            dv = der.get(t, [0.0, []])[0] if t in der else 0.0
            b, c, _ = act.get(t, (None, None, None))
            if e is None and not dv and (c in (None, 0)) and (b in (None, 0)):
                continue
            verdict = ""
            matched = []
            if b is None and c is None:
                verdict = "not in the snapshot" if (e and abs(e["bucket"]) > 1e-9) or dv else ""
                if not verdict:
                    continue
            elif e is not None or dv:
                mode_total[0] += 1
                for m in MODES:
                    ev = (e[m] if e else 0.0)
                    for label, val in (("Cur", c), ("Base", b)):
                        if close(val, ev + dv):
                            matched.append(f"{m}{'+prim' if dv else ''}→{label}")
                        elif dv and close(val, ev):
                            matched.append(f"{m}→{label}")
                if matched:
                    verdict = "ok " + ",".join(sorted(set(matched)))[:60]
                    for m in MODES:
                        if any(x.split("+")[0].split("→")[0] == m for x in matched):
                            mode_hits[m] += 1
                else:
                    verdict = "**MISMATCH**"
                    findings.append(dict(hero=w.get("name"), guid=g, attr=f"{t[0]}.{t[1]}",
                                         exp=(e["bucket"] if e else 0.0), der=dv, base=b, cur=c, hyp=t in hyp))
            elif c not in (None, 0):
                verdict = "not in expected"
            s_txt = "; ".join(f"{a} {bb}={fmt(cc)}" for a, bb, cc in src.get(t, []))[:160]
            L.append(f"| {t[0].replace('Tgf', '').replace('AttributeSet', '')}.{t[1]}{' (hand/slot)' if t in hyp else ''} | "
                     f"{fmt(e['bucket']) if e else '—'} | {fmt(dv) if dv else ''} | {fmt(b)} | {fmt(c)} | {verdict} | {s_txt} |")
        if gears:
            L.append("")
            for gid, (pts, lv) in gears.items():
                L.append(f"- set {gid} {GEAR_SETS.get(gid, '')}: points {fmt(pts['bucket'])} -> level {lv['bucket']} "
                         "(ability; not checked by attributes)")
        L.append("")
    L += buffs_section(snaps)
    L.append("## Roll formula")
    L.append("How many attributes matched for each variant: " + ", ".join(f"{m} {mode_hits[m]}" for m in MODES) +
             f" of {mode_total[0]} checked (rtot — the sum of rolls rounded once)" + "\n")

    crit_recs = [r for r in recs if r.get("t") == "crit"]
    L += hit_analysis(hits, hps, last, crit_recs)
    L.append("## Attribute mismatches")
    if findings:
        for f in findings:
            L.append(f"- {f['hero']} {f['attr']}: expected {fmt(f['exp'])}{' + ' + fmt(f['der']) + ' from primary' if f['der'] else ''}"
                     f", in the game Base {fmt(f['base'])} / Cur {fmt(f['cur'])}{' (hand/slot mapping — a hypothesis)' if f['hyp'] else ''}")
    else:
        L.append("- none")
    text = "\n".join(L) + "\n"
    out = args.out or os.path.join(RUN, "statwatch-report-" + os.path.basename(paths[-1]).replace("statwatch-", "").replace(".jsonl", ".md"))
    with open(out, "w", encoding="utf8") as f:
        f.write(text)
    print(text)
    print(f"[report: {out}]")
    return 0


def buffs_section(snaps):
    """Active buffs and conditional effects: CurrentValue − BaseValue across snapshots (the expected value from items is checked against Base)."""
    L = ["## Buffs and conditional effects (Cur − Base)",
         "The equipment check uses BaseValue: the game writes the role base, items and the contribution of primary stats into the base. "
         "Everything on top (CurrentValue − BaseValue) is active GameplayEffects: conditional item effects "
         "(\"at full health\", \"in combat\"), potions, auras, skills. They are not part of the expected value.", ""]
    rows = []
    prev = {}
    for s in snaps:
        w = s.get("who", {})
        key = w.get("name")
        cur = {}
        for k, (b, c, _) in flatten(s.get("sets", {})).items():
            if k[1].startswith("Cur") or k[1] in ("AddShield", "CalculatedShield", "DeathZoneTime"):
                continue
            if b is not None and c is not None and abs(c - b) > 1e-6:
                cur[k] = c - b
        if cur != prev.get(key):
            held = (s.get("held") or {}).get("right", "").replace("Default__", "")
            rows.append(f"| {s.get('ts')} | {key} {w.get('guid', '')} | {held} | " +
                        ("; ".join(f"{k[0].replace('Tgf', '').replace('AttributeSet', '')}.{k[1]} {v:+.4g}" for k, v in sorted(cur.items()))
                         or "none") + " |")
            prev[key] = cur
    if rows:
        L += ["| ts | hero | in hand | Cur − Base |", "|---|---|---|---|"] + rows
    else:
        L.append("No buffs (Cur = Base in all snapshots).")
    L.append("")
    return L


def attr_of(attrs, cls, field, cur=True):
    if not attrs:
        return None
    for key, vals in attrs.items():
        if set_class(key) == cls and field in vals:
            return vals[field][1 if cur else 0]
    return None


def hit_analysis(hits, hps, last_snaps, crits=None):
    """Hits. Reconstructed formula (05.11, the hero's long sword against monsters, matches to 1e-5):
        damage = K[GE, target] × (1 + AttackBoostRate) × (crit ? CritDamageRate : 1) + D,
        D = Σ AdditionalElement*DamageOnCausingHit × (1 − (target's ElementResistanceRateNone − ElementResPierceNone)).
    K contains weapon damage, the GE hit coefficient, elemental boost and attack level versus defense level — not separated.
    Check: for each (source, GE, target class) the values (damage − D)/(1 + AttackBoostRate) must give exactly two
    clusters with the ratio CritDamageRate."""
    L = ["## Hits"]
    if not hits and not hps:
        return L + ["No hits.", ""]
    kinds = defaultdict(int)
    for h in hits:
        kinds[h.get("kind")] += 1
    L.append(f"Via hooks: {dict(kinds)}; health drops bypassing hooks: {len(hps)} "
             f"(damage {fmt(sum(x.get('dmg', 0) for x in hps))}).")
    real = [h for h in hits if (h.get("dmg") or 0) > 0 and not (h.get("hp1") is not None and h.get("hp1") <= 0)]
    capped = [h for h in hits if (h.get("dmg") or 0) > 0 and h.get("hp1") is not None and h.get("hp1") <= 0]
    zero = [h for h in hits if not (h.get("dmg") or 0) > 0]
    L.append(f"Hits with damage: {len(real)}; finishing (damage cut by remaining HP) {len(capped)}; without damage {len(zero)}.")

    # crit: crit records (mod v2) or the hit's ctx
    rates_by_src = {}
    for h in hits:
        r_ = attr_of(h.get("src_attrs"), ATK, "CriticalRate")
        if r_ is not None:
            rates_by_src[(h.get("src") or {}).get("name")] = r_
    crit_list = list(crits or [])
    if not crit_list:
        for h in hits:
            c = (h.get("ctx") or {}).get("crit")
            if c:
                crit_list.append(dict(c, src=(h.get("src") or {}).get("name")))
    if crit_list:
        n = agree = crits_n = 0
        exp_c = var = 0.0
        for c in crit_list:
            rate = rates_by_src.get(c.get("src"))
            if rate is None:
                continue
            n += 1
            ret = bool(c.get("ret"))
            crits_n += ret
            exp_c += rate
            var += rate * (1 - rate)
            agree += ret == ((c.get("rnd") if c.get("rnd") is not None else 1) < rate)
        if n:
            L.append(f"- Crit (IsCritical): {n} checks, result = (random < CriticalRate) in {agree}/{n}; crits {crits_n}, "
                     f"expected {exp_c:.1f} (z = {(crits_n - exp_c) / (math.sqrt(var) or 1):+.2f}).")
        else:
            L.append(f"- Crit: IsCritical records {len(crit_list)}, the source's CriticalRate is unknown.")
    else:
        L.append("- Crit: no IsCritical records (mod v1 did not bind the hero's crit: ASC on PlayerState) — crit is derived from damage.")

    def extra_d(h):
        sa, ta = h.get("src_attrs") or {}, h.get("trg_attrs") or {}
        flat = sum(attr_of(sa, TRG, f"AdditionalElement{e}DamageOnCausingHit") or 0.0 for e in ELEM)
        flat += attr_of(sa, TRG, "AdditionalFixedDamageOnCausingHit") or 0.0
        res = attr_of(ta, DEF, "ElementResistanceRateNone") or 0.0
        pierce = attr_of(sa, ATK, "ElementResPierceNone") or 0.0
        return flat * (1 - (res - pierce)), flat

    groups = defaultdict(list)
    for h in real:
        ex = h.get("extra") or {}
        ge = ",".join(ex.get("ges") or [])
        sa = h.get("src_attrs") or {}
        d, flat = extra_d(h)
        ab = attr_of(sa, ATK, "AttackBoostRate") or 0.0
        cdr = attr_of(sa, ATK, "CritDamageRate") or 1.0
        y = (h["dmg"] - d) / (1 + ab)
        ta = h.get("trg_attrs") or {}
        dsig = "res %s, def.lvl %s" % (fmt(attr_of(ta, DEF, "ElementResistanceRateNone") or 0.0),
                                        fmt(sum(attr_of(ta, DEF, f) or 0.0 for f in ("ChestDefGearLevel", "HelmetDefGearLevel",
                                            "TrousersDefGearLevel", "RightDefGearLevel", "LeftDefGearLevel", "ExtraDefGearLevel"))))
        groups[((h.get("src") or {}).get("cls"), ge, dsig)].append(
            dict(n=h.get("n"), y=y, cdr=cdr, crit=((h.get("ctx") or {}).get("crit") or {}).get("ret")))
    L.append("- Formula: damage = K × (1 + AttackBoostRate) × (crit ? CritDamageRate : 1) + D, "
             "D = additional \"on hit\" damage × (1 − (target resistance − pierce)). Check by groups (source, GE, target):")
    L.append("")
    L.append("| source | GE | target defense | n | K (non-crit) | K·CDR (crit) | crit/non-crit | CritDamageRate | max. deviation | crits by damage / by hook |")
    L.append("|---|---|---|---|---|---|---|---|---|---|")
    ks = {}
    worst = 0.0
    outside = 0
    for (sc, ge, tc), rows in sorted(groups.items(), key=lambda kv: (str(kv[0][0]), kv[0][1], str(kv[0][2]))):
        ys = sorted(r["y"] for r in rows)
        cdr = rows[0]["cdr"]
        lo = ys[0]
        non = [y for y in ys if abs(y / lo - 1) < 0.005]
        cr = [y for y in ys if cdr > 1.0001 and abs(y / (lo * cdr) - 1) < 0.005]
        other = [y for y in ys if y not in non and y not in cr]
        k = statistics.mean(non)
        if not cr and not other and len(rows) > 1:
            other_note = " (one cluster: all non-crit or all crit)"
        else:
            other_note = ""
        dev = max([abs(y / k - 1) for y in non] + [abs(y / (k * cdr) - 1) for y in cr] + [0.0])
        worst = max(worst, dev)
        outside += len(other)
        ratio = (statistics.mean(cr) / k) if cr else None
        hook = sum(1 for r in rows if r["crit"] is True)
        ks[(sc, ge, tc)] = k
        L.append(f"| {str(sc).replace('BP_', '')[:30]} | {ge.replace('GE_TGF_', '')[:36]} | {tc} | "
                 f"{len(rows)} | {fmt(k)} | {fmt(k * cdr) if cr else '—'} | {fmt(ratio) if ratio else '—'} | {fmt(cdr)} | "
                 f"{dev * 100:.4f}%{other_note}{' + outside the model: ' + ', '.join(fmt(y) for y in other) if other else ''} | {len(cr)}/{hook} |")
    L.append("")
    L.append(f"Maximum deviation from the formula: {worst * 100:.4f}%; values outside the model: {outside} (≈0% — the formula is consistent;"
             " \"outside the model\" — neither non-crit nor crit of this group: block, a buff on the target or a single crit hit in the group).")
    by_st = defaultdict(dict)
    for (sc, ge, tc), k in ks.items():
        by_st[(sc, tc)][ge] = k
    for (sc, tc), m in by_st.items():
        if len(m) >= 2:
            base_ge = min(m, key=lambda g: m[g])
            L.append(f"- Hit coefficients {str(sc).replace('BP_', '')} against {str(tc).replace('BP_', '')}: "
                     + ", ".join(f"{g.replace('GE_TGF_', '')} ×{m[g] / m[base_ge]:.4f}" for g in sorted(m)))
    by_sg = defaultdict(dict)
    for (sc, ge, tc), k in ks.items():
        by_sg[(sc, ge)][tc] = k
    for (sc, ge), m in by_sg.items():
        if len(m) >= 2:
            L.append(f"- {ge.replace('GE_TGF_', '')} from {str(sc).replace('BP_', '')}: K by target " +
                     ", ".join(f"{str(t).replace('BP_', '')} {fmt(v)}" for t, v in sorted(m.items(), key=lambda x: str(x[0]))))

    L += ["", "Hits:", "", "| # | who | whom | GE | damage | HP before→after | AttackBoost | CritDmg | target res. | D | (damage−D)/(1+AB) |",
          "|---|---|---|---|---|---|---|---|---|---|---|"]
    for h in hits[-60:]:
        ex = h.get("extra") or {}
        sa = h.get("src_attrs") or {}
        d, _ = extra_d(h)
        ab = attr_of(sa, ATK, "AttackBoostRate") or 0.0
        y = ((h.get("dmg") or 0) - d) / (1 + ab)
        L.append(f"| {h.get('n')} | {str((h.get('src') or {}).get('name')).replace('BP_', '')} | {str((h.get('trg') or {}).get('name')).replace('BP_', '')} | "
                 f"{','.join(ex.get('ges') or []).replace('GE_TGF_', '') or ex.get('ge', '')} | {fmt(h.get('dmg'))} | "
                 f"{fmt(h.get('hp0'))}→{fmt(h.get('hp1'))} | {fmt(ab)} | {fmt(attr_of(sa, ATK, 'CritDamageRate'))} | "
                 f"{fmt(attr_of(h.get('trg_attrs'), DEF, 'ElementResistanceRateNone'))} | {fmt(d)} | {fmt(y)} |")
    if hps:
        by = defaultdict(lambda: [0, 0.0])
        for x in hps:
            k = (x.get("who") or {}).get("cls")
            by[k][0] += 1
            by[k][1] += x.get("dmg", 0)
        L.append("")
        L.append("Health drops bypassing hooks (DoT, traps, damage from C++): " + "; ".join(f"{k} {n} times, {fmt(d)}" for k, (n, d) in by.items()))
    L.append("")
    return L


def cmd_selftest(args):
    T = Tables()
    roles = find_roles(args.dump)
    bad = 0
    for g, r in roles.items():
        try:
            exp, src, hyp, items, gears = expected_for(T, r["deploy_role"])
            notes = [n for ic in items for c in ic["contribs"] for n in [c["note"]] if "NO " in n or "no " in n]
            print(f"{g} {r['deploy_role']['basic'].get('role_name', ''):<14} items {len(items):2} attributes {len(exp):3}"
                  f" sets {len(gears)}" + (f" remarks {len(notes)}: {notes[:2]}" if notes else ""))
        except Exception as e:  # noqa: BLE001
            bad += 1
            print(f"{g} ERROR {e!r}")
    return 1 if bad else 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    e = sub.add_parser("expected")
    e.add_argument("--role", type=int)
    e.add_argument("--open")
    e.add_argument("--pair", type=int, default=1, help="active weapon pair (1/2) for ROLE_HOLD")
    e.add_argument("--dump", default=DUMP)
    r = sub.add_parser("report")
    r.add_argument("--log", nargs="*")
    r.add_argument("--out")
    r.add_argument("--dump", default=DUMP)
    s = sub.add_parser("selftest")
    s.add_argument("--dump", default=DUMP)
    a = ap.parse_args()
    return {"expected": cmd_expected, "report": cmd_report, "selftest": cmd_selftest}[a.cmd](a)


if __name__ == "__main__":
    sys.exit(main())
