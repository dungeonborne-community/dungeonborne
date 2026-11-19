"""Loot table check (audit stage 1, 11.11). Only reads the tables, writes a Markdown report.

    python -I -X utf8 tools/loot_audit.py [--season playtest] [--out loot/audit_report.md]

Loot tables are taken from the season (seasons/<id>/balance/loot*.json) if they are there, otherwise from balance/.
Roll formula — as in the game (research/loot/GAME_CODE_ANSWERS.md, A1/A2):
  n = min(default_rate, max_rate); rolls = n // 10000 + one more with probability (n % 10000) / 10000;
  pool — by weights pr, item — by weights rate within the pool; cnt — quantity. We have no chance bonuses.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse
import collections
import csv
import json
import re
from pathlib import Path

LOOT_TABLES = ["loot", "loot_rate", "loot_choose_group_rate", "loot_ue_source_choose_group"]
MAPS = {10101: "Crypt normal", 10102: "Crypt Classic", 10103: "Crypt Hard",
        10201: "Castle normal", 10202: "Castle Classic", 10203: "Castle Hard"}
DIFF_ORDER = [(10101, 10102, 10103), (10201, 10202, 10203)]
RARITY = ["ROTTED", "NORMAL", "UNCOMMON", "ENHANCED", "EPIC", "EXCEPTIONAL", "LEGENDARY", "UNIQUE"]
RARITY_RU = {"ROTTED": "rotted", "NORMAL": "grey", "UNCOMMON": "green", "ENHANCED": "blue", "EPIC": "purple",
             "EXCEPTIONAL": "pink", "LEGENDARY": "legend.", "UNIQUE": "unique"}
GOLD = 100069
NOLOOT = re.compile(r"Noloot|Trap_|Dummy|Tutorial|ActorSpawnPoint|AltarBase|Altar_(Attack|Clarity|Defend|Heal|MithrilRing|MoonEye|Recover|Relive|Speed)|FlowerCoffin|CollectableBase_C|Homeland")
BOSSES = r"Reaper|Wendigo|Executioner|Cyclop|Khazra|Curesd_King|Shadow_King|Werewolf|Cerberus|Lich"
ELITE = r"Armor_Hammer|Armor_Sword|Beholder|Treant|Dragonborn"


def rows(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))["rows"]


def source_class(name):
    n = name or ""
    if re.search(r"Altar_Special", n): return "seal altar"
    if re.search(r"Altar", n): return "altar/special chest"
    if re.search(r"LegendWeapon", n): return "legend. weapon on the floor"
    if re.search(r"CollectableBase", n): return "item on the floor"
    if re.search(r"Mimic|Monster_Chest", n): return "mimic"
    if re.search(r"Goblin_?Thief|GoblinThief", n): return "goblin thief"
    if re.search(r"Chest_Gold|TreasureChest_Special|FlawlessTrial", n): return "altar/special chest"
    if re.search(r"Loot_Gold", n): return "gold on the floor"
    if re.search(r"Chest", n): return "chest"
    if re.search(BOSSES, n): return "boss"
    if re.search(r"Monster|Skeleton|Werewolf", n):
        return "elite" if re.search(ELITE, n) else "monster"
    if re.search(r"Barrel|Box|Woodbox|Woodenvat|Pot|Crate|Vase|vase|Vat|Case|Rock|Door|Wall|Fridge|Destroy|fish|Claypot", n):
        return "barrels/crates/pots"
    return "other"


class Data:
    def __init__(self, season):
        self.season = season
        sdir = W("seasons") / season / "balance" if season else None
        self.src_of = {}
        for t in LOOT_TABLES:
            p = sdir / f"{t}.json" if sdir and (sdir / f"{t}.json").exists() else W("balance") / f"{t}.json"
            self.src_of[t] = p
        self.item = {r["type_id"]: r for r in rows(W("balance/item_type.json"))}
        self.ue = {r["type_id"]: r for r in rows(W("balance/UeSource_ItemType.json"))}
        self.srcname = {r["id"]: r["name"] for r in rows(W("balance/loot_source_map.json"))}
        self.names = {}
        with open(W("balance_csv/item_type.csv"), encoding="utf-8-sig") as f:
            for r in csv.reader(f, delimiter=";"):
                if r and r[0].isdigit():
                    self.names[int(r[0])] = r[1]
        self.pool = collections.defaultdict(list)
        for r in rows(self.src_of["loot"]):
            self.pool[r["random_id"]].append(r)
        self.rate = {r["random_id"]: r for r in rows(self.src_of["loot_rate"])}
        self.slot = {r["loot_id"]: r for r in rows(self.src_of["loot_choose_group_rate"])}
        self.src = {}
        self.src_dups = []
        for r in rows(self.src_of["loot_ue_source_choose_group"]):
            k = (r["ue_loot_source_id"], r.get("map_id", 0), r.get("loot_type", 0))
            if k in self.src:
                self.src_dups.append(k)
            self.src[k] = r

    # --- items ---
    def rarity(self, t):
        u = self.item.get(t, {}).get("ue_source_type_id") or t
        return self.ue.get(u, {}).get("rarity", "").replace("EN_ITEM_RARITY_", "") or "?"

    def cat(self, t):
        if t == GOLD: return "gold"
        r = self.item.get(t, {})
        major, minor = r.get("item_major_type", ""), r.get("item_minor_type", "")
        if minor.startswith("EN_ITEM_MINOR_TYPE_EQUIPMENT"): return "equipment"
        if major == "EN_ITEM_MAJOR_TYPE_GEM": return "gem"
        if major == "EN_ITEM_MAJOR_TYPE_CRAFTS": return "treasure"
        if minor == "EN_ITEM_MINOR_TYPE_CONSUMABLES_POTION": return "potion"
        if minor == "EN_ITEM_MINOR_TYPE_CONSUMABLES_THROWABLE": return "throwable"
        if major == "EN_ITEM_MAJOR_TYPE_CONSUMABLES": return "consumable"
        if major == "EN_ITEM_MAJOR_TYPE_CURRENCY": return "currency"
        return "other"

    def price(self, t):
        return self.item.get(t, {}).get("system_repurchase_price") or 0

    def name(self, t):
        return self.names.get(t) or str(t)

    def sname(self, s):
        return self.srcname.get(s, str(s))

    # --- expectation from one source ---
    def expect(self, sid, m, lt=0):
        """Average from one Loot call: {key: average count}; keys — 'eq:<rarity>', categories, 'gold', 'value'."""
        out = collections.Counter()
        r = self.src.get((sid, m, lt))
        if not r:
            return None
        for lid in r.get("loot_ids", []):
            s = self.slot.get(lid)
            if not s:
                continue
            n = min(s["default_rate"], s["max_rate"])
            rolls = n / 10000  # expected number of rolls: whole + fraction
            pairs = [(p, w) for p, w in zip(s["random_ids"], s["pr"]) if w > 0]
            tw = sum(w for _, w in pairs) or 1
            for pid, w in pairs:
                items = [i for i in self.pool.get(pid, []) if i["rate"] > 0]
                sw = sum(i["rate"] for i in items) or 1
                for i in items:
                    p = rolls * (w / tw) * (i["rate"] / sw)
                    t, c = i["item_id"], i["cnt"]
                    k = self.cat(t)
                    if k == "equipment":
                        out["eq:" + self.rarity(t)] += p
                        out["equipment"] += p
                    elif k == "gold":
                        out["gold"] += p * c
                    else:
                        out[k] += p
                    if k not in ("gold", "equipment"):
                        out["value"] += p * c * self.price(t)
        return out

    def p_at_least_one(self, sid, m, pred, lt=0):
        """Probability of at least one item for which pred(item_id) is true, from one call."""
        r = self.src.get((sid, m, lt))
        if not r:
            return 0.0
        miss = 1.0
        for lid in r.get("loot_ids", []):
            s = self.slot.get(lid)
            if not s:
                continue
            n = min(s["default_rate"], s["max_rate"])
            pairs = [(p, w) for p, w in zip(s["random_ids"], s["pr"]) if w > 0]
            tw = sum(w for _, w in pairs) or 1
            q = 0.0  # chance of the wanted item in one roll
            for pid, w in pairs:
                items = [i for i in self.pool.get(pid, []) if i["rate"] > 0]
                sw = sum(i["rate"] for i in items) or 1
                q += (w / tw) * sum(i["rate"] for i in items if pred(i["item_id"])) / sw
            full, frac = divmod(n, 10000)
            miss *= (1 - q) ** full * (1 - q * frac / 10000)
        return 1 - miss


def fmt(x, nd=2):
    return f"{x:.{nd}f}".rstrip("0").rstrip(".") if x else "0"


def audit(d, out):
    L = []
    w = L.append
    w(f"# Loot table audit (season `{d.season or 'base'}`)\n")
    w("Generated by `tools/loot_audit.py`, do not edit by hand. Formula — as in the game (research/loot/GAME_CODE_ANSWERS.md).\n")
    w("Tables: " + ", ".join(f"`{t}` ← `{p.relative_to(ROOT)}`" for t, p in d.src_of.items()) + "\n")

    # 1. integrity
    w("## 1. Integrity\n")
    probs = []
    for (sid, m, lt), r in d.src.items():
        for lid in r.get("loot_ids", []):
            if lid not in d.slot:
                probs.append(f"source {d.sname(sid)} map {m} type {lt}: no slot {lid}")
    for lid, s in d.slot.items():
        if len(s["random_ids"]) != len(s["pr"]):
            probs.append(f"slot {lid}: random_ids {len(s['random_ids'])} ≠ pr {len(s['pr'])}")
        for pid in s["random_ids"]:
            if pid not in d.pool:
                probs.append(f"slot {lid}: no pool {pid}")
        if s["default_rate"] > s["max_rate"]:
            probs.append(f"slot {lid}: default_rate {s['default_rate']} > max_rate {s['max_rate']} (cut to max)")
        if s["default_rate"] <= 0:
            probs.append(f"slot {lid}: chance {s['default_rate']}")
    for pid, items in d.pool.items():
        sw = sum(i["rate"] for i in items)
        if sw != 30000:
            probs.append(f"pool {pid}: sum of weights {sw} (30000 expected; does not affect the chance, the game normalizes)")
        for i in items:
            if i["item_id"] not in d.item:
                probs.append(f"pool {pid}: item {i['item_id']} is not in item_type")
            elif d.rarity(i["item_id"]) == "?":
                probs.append(f"pool {pid}: {i['item_id']} {d.name(i['item_id'])} has no UeSource_ItemType row (the client will not show it)")
            if i["cnt"] <= 0:
                probs.append(f"pool {pid}: {i['item_id']} cnt {i['cnt']}")
        if pid not in d.rate:
            probs.append(f"pool {pid}: no loot_rate row")
    if d.src_dups:
        probs.append(f"repeated keys (source, map, type): {len(d.src_dups)}, e.g. {d.src_dups[:3]}")
    used_slots = {lid for r in d.src.values() for lid in r.get("loot_ids", [])}
    used_pools = {p for lid in used_slots if lid in d.slot for p in d.slot[lid]["random_ids"]}
    multi = [lid for lid, s in d.slot.items() if min(s["default_rate"], s["max_rate"]) > 10000]
    w(f"- source rows {len(d.src)}, slots {len(d.slot)} (unreferenced {len(set(d.slot) - used_slots)}), "
      f"pools {len(d.pool)} (unreferenced {len(set(d.pool) - used_pools)}), item rows {sum(map(len, d.pool.values()))}")
    w(f"- slots with chance > 100% (several rolls): {len(multi)}")
    w(f"- problems: **{len(probs)}**")
    for p in probs[:60]:
        w(f"  - {p}")
    if len(probs) > 60:
        w(f"  - … and {len(probs) - 60} more")
    w("")

    # 2. items
    w("## 2. Items\n")
    in_loot = {i["item_id"] for p in used_pools for i in d.pool.get(p, [])}
    eq_all = [t for t, r in d.item.items() if r.get("item_minor_type", "").startswith("EN_ITEM_MINOR_TYPE_EQUIPMENT")]
    by_r = collections.Counter(d.rarity(t) for t in eq_all)
    by_r_loot = collections.Counter(d.rarity(t) for t in eq_all if t in in_loot)
    w("| Rarity | equipment in the game | of them in loot |")
    w("|---|---|---|")
    for rr in RARITY + ["?"]:
        if by_r.get(rr):
            w(f"| {RARITY_RU.get(rr, rr)} | {by_r[rr]} | {by_r_loot.get(rr, 0)} |")
    cats = collections.Counter(d.cat(t) for t in in_loot)
    w("\nIn loot by category: " + ", ".join(f"{k} {v}" for k, v in cats.most_common()))
    odd = [t for t in in_loot if d.cat(t) in ("currency", "other", "gem")]
    if odd:
        w("\nUnusual items in loot (currency/other/gems — check whether the client shows them):")
        for t in sorted(odd):
            w(f"- {t} {d.name(t)} [{d.cat(t)}]")
    forb = sorted(t for t in in_loot if d.item.get(t, {}).get("is_forbidden_bring_into_battle") and d.cat(t) == "equipment")
    if forb:
        w(f"\nEquipment with `is_forbidden_bring_into_battle` in loot: {len(forb)} ({', '.join(map(str, forb[:10]))})")
    w("")

    # 3. sources
    w("## 3. Sources per map (loot type 0)\n")
    all_src = set(d.srcname)
    w("| Map | sources with a row | from loot_source_map without a row | id without a name |")
    w("|---|---|---|---|")
    for m in [0] + list(MAPS):
        have = {s for (s, mm, lt) in d.src if mm == m and lt == 0}
        miss = sorted(all_src - have)
        noname = sorted(s for s in have if s not in d.srcname)
        w(f"| {m} {MAPS.get(m, 'default')} | {len(have)} | {len(miss)} | {len(noname)} |")
    for m in MAPS:
        have = {s for (s, mm, lt) in d.src if mm == m and lt == 0}
        miss = sorted(x for x in all_src - have if not NOLOOT.search(d.sname(x)))
        if miss:
            w(f"\n{m}: in loot_source_map without a row (no loot), except the known empty ones (Noloot, traps, buff altars, tutorial): " +", ".join(d.sname(s) for s in miss[:40])
              + (f" … {len(miss) - 40} more" if len(miss) > 40 else ""))
    w("\nLoot types 1, 2, 5 versus 0 (the game passes only 0, altars 3/4):")
    for lt in (1, 2, 3, 4, 5):
        diff = [(s, m) for (s, m, t), r in d.src.items() if t == lt and d.src.get((s, m, 0), {}).get("loot_ids") != r.get("loot_ids")]
        w(f"- type {lt}: rows {sum(1 for k in d.src if k[2] == lt)}, differ from type 0: {len(diff)}"
          + (f" ({', '.join(sorted({d.sname(s) for s, _ in diff})[:6])})" if diff else ""))
    w("")

    # 4. expectation by source class
    w("## 4. Average from one source by class (type 0)\n")
    w("Numbers — the average count per opening/kill. \"value\" — the sum of `system_repurchase_price` of non-equipment loot "
      "(treasures, potions…), gold separately.\n")
    classes = collections.defaultdict(set)
    for (s, m, lt) in d.src:
        classes[source_class(d.sname(s))].add(s)
    for cl in sorted(classes):
        w(f"### {cl}\n")
        w("| Map | src. | items | " + " | ".join(RARITY_RU[r] for r in RARITY) + " | gold | value of the rest |")
        w("|---|---|---|" + "---|" * len(RARITY) + "---|---|")
        for m in MAPS:
            es = [d.expect(s, m) for s in classes[cl]]
            es = [e for e in es if e is not None]
            if not es:
                continue
            avg = collections.Counter()
            for e in es:
                for k, v in e.items():
                    avg[k] += v / len(es)
            items = sum(v for k, v in avg.items() if not k.startswith("eq:") and k not in ("gold", "value"))
            w(f"| {MAPS[m]} | {len(es)} | {fmt(items)} | " + " | ".join(fmt(avg.get('eq:' + r, 0), 3) for r in RARITY)
              + f" | {fmt(avg['gold'], 1)} | {fmt(avg['value'], 0)} |")
        w("")

    # 5. rare items
    w("## 5. Chance of at least one item of a rarity from one source, % (maximum per class)\n")
    w("| Class | Rarity | " + " | ".join(MAPS.values()) + " |")
    w("|---|---|" + "---|" * len(MAPS))
    for cl in sorted(classes):
        for rr in ("EPIC", "EXCEPTIONAL", "LEGENDARY", "UNIQUE"):
            vals = []
            for m in MAPS:
                ps = [d.p_at_least_one(s, m, lambda t, rr=rr: d.cat(t) == "equipment" and d.rarity(t) == rr) for s in classes[cl]]
                vals.append(max(ps) if ps else 0)
            if any(v > 0 for v in vals):
                w(f"| {cl} | {RARITY_RU[rr]} | " + " | ".join(fmt(v * 100, 2) for v in vals) + " |")
    w("")

    # 6. difficulty: normal ≤ Classic ≤ Hard
    w("## 6. Violations of growth by difficulty (normal → Classic → Hard)\n")
    w("Compared: \"rarity points\" (blue 1, purple 3, pink 9, legend. 27, unique 27), number of items, gold and value of the rest. "
      "A drop of more than 10% on a harder map is listed.\n")
    score_w = {"ENHANCED": 1, "EPIC": 3, "EXCEPTIONAL": 9, "LEGENDARY": 27, "UNIQUE": 27}
    bad = collections.defaultdict(list)
    for chain in DIFF_ORDER:
        for s in {s for (s, m, lt) in d.src if m in chain and lt == 0}:
            es = [d.expect(s, m) for m in chain]
            if any(e is None for e in es):
                continue
            for metric, f in (("rarity", lambda e: sum(e.get("eq:" + r, 0) * k for r, k in score_w.items())),
                              ("items", lambda e: sum(v for k, v in e.items() if not k.startswith("eq:") and k not in ("gold", "value"))),
                              ("gold", lambda e: e.get("gold", 0)),
                              ("value of the rest", lambda e: e.get("value", 0))):
                vals = [f(e) for e in es]
                for a, b, ma, mb in ((vals[0], vals[1], chain[0], chain[1]), (vals[1], vals[2], chain[1], chain[2])):
                    if a > 0 and b < a * 0.9:
                        bad[(source_class(d.sname(s)), metric, ma, mb)].append((d.sname(s), a, b))
    if not bad:
        w("No violations.")
    for (cl, metric, ma, mb), lst in sorted(bad.items()):
        ex = lst[0]
        w(f"- **{cl}**, {metric}: {MAPS[ma]} → {MAPS[mb]} drops for {len(lst)} src. (e.g. {ex[0]}: {fmt(ex[1])} → {fmt(ex[2])})")
    w("")

    # 7. H_ versus normal
    w("## 7. `H_` monsters versus normal ones (the same class without `H_`)\n")
    pairs = []
    by_name = {v: k for k, v in d.srcname.items()}
    for s, n in d.srcname.items():
        if "_H_" in n:
            base = n.replace("_H_", "_")
            if base in by_name:
                pairs.append((s, by_name[base]))
    same = collections.Counter()
    for m in MAPS:
        for h, b in pairs:
            rh, rb = d.src.get((h, m, 0)), d.src.get((b, m, 0))
            same[(m, "same" if rh and rb and rh["loot_ids"] == rb["loot_ids"] else
                  "no row" if not rh or not rb else "different loot")] += 1
    w(f"Pairs: {len(pairs)}. " + "; ".join(f"{MAPS[m]}: " + ", ".join(f"{k} {same[(m, k)]}" for k in ("same", "different loot", "no row") if same[(m, k)])
                                            for m in MAPS))
    w("")

    # 8. season versus base
    if d.season:
        w("## 8. Season versus base\n")
        base_slot = {r["loot_id"]: r for r in rows(W("balance/loot_choose_group_rate.json"))}
        ch = [(lid, base_slot[lid]["default_rate"], s["default_rate"]) for lid, s in d.slot.items()
              if lid in base_slot and base_slot[lid]["default_rate"] != s["default_rate"]]
        new = [lid for lid in d.slot if lid not in base_slot]
        w(f"- slots with a different chance: {len(ch)}; new slots: {len(new)}")
        for lid, a, b in ch[:50]:
            pools = d.slot[lid]["random_ids"]
            w(f"  - {lid}: {fmt(a / 100)}% → {fmt(b / 100)}% (pools {pools})")
        w("")

    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Path(out).write_text("\n".join(L) + "\n", encoding="utf-8")
    print(f"{out}: integrity problems {len(probs)}, difficulty violations {len(bad)}")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--season", default="playtest", help="season id; empty — base only")
    ap.add_argument("--out", default=str(W("loot") / "audit_report.md"))
    a = ap.parse_args()
    audit(Data(a.season or None), a.out)


if __name__ == "__main__":
    main()
