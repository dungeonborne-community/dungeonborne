"""Set bonuses on blue and purple equipment (season playtest, user's decision 09.11).

Builds from the base balance/affix_pool.json and balance/affix_template.json the seasonal tables
seasons/<season>/balance/affix_pool.json and affix_template.json:

- a new property set SET_POOL — all set bonuses except Midas (coin_master — green only, as in the original),
  weights SETS; 13.11 — Kleptomancy ("Алхимик навеселе", in the original it was on no item at all) added with a small weight,
  all weights of the season's pools ×WEIGHT_SCALE;
- for each blue (ENHANCED) and purple (EPIC) item of groups 181–185 (weapons, armor, rings, amulets) the set is added
  as one more variant to ONE additional property (MINOR) — the one with the most variants (on a tie — the first).
  No more than one set bonus per item; chance ≈ 1 / (variants + 1).
Bonus points for rarities 4/5 already exist in affix_value_template (101–104), we do not touch them.

    python -I tools/season_set_affixes.py [--season playtest]
Then `cargo xtask balance build --season=<season>`.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse, json, re, sys
from pathlib import Path

SET_POOL = 100802
# 13.11 (user's decision "give it a small weight"): Kleptomancy ("Алхимик навеселе") — in the set, but with weight 2 versus
# 10 for the others (2.8% of set bonuses versus ~13.9%): it has one level at 8 points, assembled with two items.
SETS = {"gear_set_fleet_foot_work": 10, "gear_set_undying_grasp": 10, "gear_set_thunder_law": 10,
        "gear_set_bone_plating": 10, "gear_set_avoid_death": 10, "gear_set_scorching_touch": 10,
        "gear_set_summoning_aery": 10, "gear_set_kleptomancy": 2}
# weights are integers and all = 1 in the base: to give a small weight, all weights of all season pools ×10 — the battle server merges the properties
# of a variant's pools into one weighted list, so the ratios of the other properties do not change
WEIGHT_SCALE = 10
RARITIES = {"EN_ITEM_RARITY_ENHANCED", "EN_ITEM_RARITY_EPIC"}
GROUPS = ("181", "182", "183", "184", "185")
MINOR = "EN_BATTLE_AFFIX_TYPE_MINOR"


def load(p):
    return json.loads(Path(p).read_text(encoding="utf-8"))


def save(p, d):
    d["header"]["count"] = len(d["rows"])
    Path(p).parent.mkdir(parents=True, exist_ok=True)
    Path(p).write_text(json.dumps(d, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


PURPLE_RULES = (40111, 40112, 40113)  # monster_rule_id conditions for purple (for blue 40101…40103 — not touched)
NEW_RULE_BASE = 40121                   # own ids for 40111 variants with a different pool list (weapons of different types)
# level_id in affix_monster_pool_rule: in the original — map_id (10101…), just in case also level ids 40003015…40003020
# 13.11: 10112/10113/10213 — second-floor loot maps (without seals; RuServer LootMap), rules as for 10102/10103/10203
LEVELS = [10101, 10102, 10103, 10201, 10202, 10203, 10112, 10113, 10213, 40003015, 40003016, 40003017, 40003018, 40003019,
          40003020]


def purple_by_source(tpl, rarity):
    """11.11 (user's decision): set bonus on PURPLE — only from bosses (100%: the property is taken only from
    the set pool) and elites (as before, ~21–25%), from other sources (chests, monsters) — 0%.

    For a monster the battle server looks up affix_monster_pool_rule (monster_rule_id, monster_id = id from loot_source_map, level_id)
    and replaces the variant's pool list (research/loot/GAME_CODE_ANSWERS.md, E1). Therefore: the set pool is removed from the purple
    item's own variant list, and bosses and elites get it back via rule rows. 40111 has different lists
    (weapons of different types) — a rule row replaces the list for all items with this id, so the other lists get
    their own ids 40121…."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import loot_audit as la
    src = load(str(W("balance/loot_source_map.json")))["rows"]
    def kind(n):
        return "boss" if re.search(r"Shadow_King|Wendigo", n) else la.source_class(n)
    bosses = sorted(r["id"] for r in src if kind(r["name"]) == "boss")
    elite = sorted(r["id"] for r in src if kind(r["name"]) == "elite")
    lists = {}  # rule id -> pool list without sets
    next_id = NEW_RULE_BASE
    n_items = 0
    for r in tpl["rows"]:
        if rarity(r["item_id"]) != "EN_ITEM_RARITY_EPIC":
            continue
        for rr in r["data"].get("random_rule", []):
            for v in rr.get("pool_random", []):
                rid = v.get("condition", {}).get("monster_rule_id")
                if rid not in PURPLE_RULES or SET_POOL not in v.get("affix_pool_id", []):
                    continue
                base = [p for p in v["affix_pool_id"] if p != SET_POOL]
                if rid in lists and lists[rid] != base:
                    same = [k for k, l in lists.items() if l == base and k >= NEW_RULE_BASE]
                    if same:
                        rid = same[0]
                    else:
                        rid, next_id = next_id, next_id + 1
                lists.setdefault(rid, base)
                v["affix_pool_id"] = base
                v["condition"]["monster_rule_id"] = rid
                n_items += 1
    data = load(str(W("balance/affix_monster_pool_rule.json")))
    keep = [x for x in data["rows"] if x["rule_id"] not in PURPLE_RULES]  # the original's special pools for blue stay
    for rid, base in sorted(lists.items()):
        for lvl in LEVELS:
            keep += [{"rule_id": rid, "monster_id": m, "level_id": lvl, "affix_pool_ids": [SET_POOL]} for m in bosses]
            keep += [{"rule_id": rid, "monster_id": m, "level_id": lvl, "affix_pool_ids": base + [SET_POOL]} for m in elite]
    data["rows"] = keep
    data["header"]["description"] = "Season: purple gear set bonus only from bosses (100%) and elite (as before), 0% elsewhere."
    print(f"purple: items {n_items}, rules {len(lists)} ({sorted(lists)}), bosses {len(bosses)}, elites {len(elite)}, "
          f"affix_monster_pool_rule rows {len(keep)}")
    return data


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--season", default="playtest")
    a = ap.parse_args()
    out = W("seasons") / a.season / "balance"

    item_type = {r["type_id"]: r for r in load(str(W("balance/item_type.json")))["rows"]}
    ue = {r["type_id"]: r for r in load(str(W("balance/UeSource_ItemType.json")))["rows"]}

    def rarity(t):
        u = item_type.get(t, {}).get("ue_source_type_id") or t
        return ue.get(u, {}).get("rarity", "")

    pools = load(str(W("balance/affix_pool.json")))
    ids = {r["affix_pool_id"] for r in pools["rows"]}
    if SET_POOL in ids:
        sys.exit(f"pool {SET_POOL} already exists in the base")
    # as with Midas pool 100801: only its own keys
    known = {k for r in pools["rows"] for k in r["attribute_weight"]}
    missing = [s for s in SETS if s not in known]
    if missing:
        sys.exit(f"no such properties in the base pools: {missing}")
    scaled = 0
    for r in pools["rows"]:
        for v in r["attribute_weight"].values():
            if isinstance(v, dict) and isinstance(v.get("weight"), int):
                v["weight"] *= WEIGHT_SCALE
                scaled += 1
    row = {"affix_pool_id": SET_POOL, "attribute_weight": {k: {"weight": w} for k, w in SETS.items()}}
    pools["rows"].append(row)
    pools["header"]["description"] = (pools["header"].get("description", "") + f" Season {a.season}: all weights x{WEIGHT_SCALE}; pool {SET_POOL} = gear set affixes except coin_master, kleptomancy at low weight (blue/purple gear).").strip()
    print(f"weights ×{WEIGHT_SCALE}: {scaled}")

    tpl = load(str(W("balance/affix_template.json")))
    changed = {}
    for r in tpl["rows"]:
        t = r["item_id"]
        if str(t)[:3] not in GROUPS or rarity(t) not in RARITIES:
            continue
        rules = [x for x in r["data"].get("random_rule", []) if x.get("affix_type") == MINOR and x.get("pool_random")]
        if not rules:
            continue
        if any("gear_set" in json.dumps(x) for x in r["data"]["random_rule"]):
            continue
        best = max(rules, key=lambda x: len(x["pool_random"][0].get("affix_pool_id", [])))  # max — the first on a tie
        best["pool_random"][0]["affix_pool_id"].append(SET_POOL)
        changed[t] = len(best["pool_random"][0]["affix_pool_id"])
    rules = purple_by_source(tpl, rarity)
    save(out / "affix_pool.json", pools)
    save(out / "affix_template.json", tpl)
    save(out / "affix_monster_pool_rule.json", rules)
    by = {}
    for t, n in changed.items():
        by.setdefault((rarity(t)[15:], n), 0)
        by[(rarity(t)[15:], n)] += 1
    print(f"pool {SET_POOL}: {len(SETS)} sets; items changed {len(changed)}")
    for (rr, n), c in sorted(by.items()):
        print(f"  {rr:9s} variants in the property {n} (chance ~{100 / n:.0f}%): {c} items")


if __name__ == "__main__":
    main()
