"""Compares PVE loot (zz_SoloLoot_9_P) with ours (balance/). Arguments: <pve json dir> <balance dir> <strings.tsv>"""
import json, sys, collections, os

pve_dir, bal_dir, strings_tsv = sys.argv[1:4]
sys.stdout = open(sys.argv[4], "w", encoding="utf-8")

def rows(d, t):
    p = os.path.join(d, t + ".json")
    if not os.path.exists(p):
        return []
    with open(p, encoding="utf-8-sig") as f:
        return json.load(f)["rows"]

names = {}
with open(strings_tsv, encoding="utf-8") as f:
    for line in f:
        p = line.rstrip("\n").split("\t")
        if len(p) >= 3 and p[1].startswith("Item_Name_"):
            names[int(p[1][10:])] = p[2]

def cat_of(r):
    i, ma, mi = r["type_id"], r.get("item_major_type", ""), r.get("item_minor_type", "")
    if i == 100069: return "gold"
    if mi.endswith("EQUIPMENT_WEAPON"): return "weapon"
    if mi.endswith("EQUIPMENT_ARMOR"): return "armor"
    if mi.endswith("EQUIPMENT_ACCESSORIES"): return "jewelry"
    if ma.endswith("GEM"): return "gem"
    if ma.endswith("CRAFTS"): return "treasure"
    if mi.endswith("POTION"): return "potion"
    if mi.endswith("THROWABLE"): return "throwable"
    if ma.endswith("CONSUMABLES"): return "supply"
    if ma.endswith("CURRENCY"): return "currency"
    return "other"

cats = {r["type_id"]: cat_of(r) for r in rows(bal_dir, "item_type")}

class Loot:
    def __init__(self, d, mapsrc):
        self.pools = collections.defaultdict(list)
        for r in rows(d, "loot"):
            self.pools[r["random_id"]].append((r["item_id"], r.get("cnt", 0), r.get("rate", 0)))
        self.slots = {r["loot_id"]: r for r in rows(d, "loot_choose_group_rate")}
        self.groups = collections.defaultdict(dict)  # (map, type) -> src -> loot_ids
        for r in rows(d, "loot_ue_source_choose_group"):
            self.groups[(r.get("map_id", 0), r.get("loot_type", 0))][r["ue_loot_source_id"]] = r["loot_ids"]
        self.src = {}
        for r in rows(mapsrc, "loot_source_map"):
            self.src.setdefault(r["id"], r["name"])

    def expect(self, loot_ids):
        """average loot: {cat: items}, gold pieces"""
        out, gold = collections.Counter(), 0.0
        for lid in loot_ids:
            s = self.slots.get(lid)
            if not s: continue
            p = s.get("default_rate", 0) / 10000
            rids, pr = s.get("random_ids", []), s.get("pr", [])
            tot = sum(pr) or 1
            for rid, w in zip(rids, pr):
                pool = self.pools.get(rid, [])
                pt = sum(x[2] for x in pool) or 1
                for item, cnt, rate in pool:
                    f = p * w / tot * rate / pt
                    c = cats.get(item, "??")
                    out[c] += f
                    if c == "gold": gold += f * cnt
        return out, gold

pve = Loot(pve_dir, pve_dir)
our = Loot(bal_dir, bal_dir)

print("=== PVE: sizes")
print(f"  pools {len(pve.pools)} (ours {len(our.pools)}), loot rows {sum(len(v) for v in pve.pools.values())} "
      f"(ours {sum(len(v) for v in our.pools.values())}), slots {len(pve.slots)} (ours {len(our.slots)})")
items = {i for p in pve.pools.values() for i, _, _ in p}
unknown = sorted(i for i in items if i not in cats)
print(f"  distinct items {len(items)} (ours {len({i for p in our.pools.values() for i,_,_ in p})}); missing from our item_type: {len(unknown)} {unknown[:10]}")
our_names = set(our.src.values())
new_src = sorted({pve.src.get(s, str(s)) for g in pve.groups.values() for s in g} - our_names)
print(f"  sources missing from our loot_source_map: {len(new_src)}: {new_src[:12]}")
cnt_by = collections.Counter(c for i in items for c in [cats.get(i, '??')])
print("  items by category:", dict(cnt_by.most_common()))

print("\n=== loot_type: how 1..5 differ from 0 (on map 10101)")
base = pve.groups[(10101, 0)]
for t in range(1, 6):
    g = pve.groups.get((10101, t), {})
    diff = [s for s in g if s in base and g[s] != base[s]]
    only0 = [s for s in base if s not in g]
    extra = [s for s in g if s not in base]
    print(f"  type {t}: rows {len(g)}, differ {len(diff)}, missing {len(only0)} (e.g. {[pve.src.get(s) for s in only0[:3]]}), "
          f"extra {[(pve.src.get(s), g[s]) for s in extra]}")

def classify(name):
    n = name.lower()
    if "tutorial" in n: return None
    if "collectable" in n: return None
    if "probe" in n: return None
    if any(k in n for k in ("barrel", "claypot", "pot_", "crate", "box", "vase", "vat", "woodencase", "destruct", "jar")): return "1 barrels/crates/pots"
    if "treasurechest_gold" in n or "chest_gold" in n: return "3 gold chest"
    if "chest" in n or "coffin" in n: return "2 other chests"
    if "_h_monster" in n: return "5 Hard-version monsters (_H_)"
    if "king" in n or "boss" in n or "special" in n or "lich" in n or "executioner" in n: return "6 bosses/special"
    if "monster" in n: return "4 monsters"
    return "7 other"

def summarize(L, key):
    agg = collections.defaultdict(lambda: [0, collections.Counter(), 0.0])
    for s, ids in L.groups.get(key, {}).items():
        cl = classify(L.src.get(s, str(s)))
        if not cl: continue
        e, gold = L.expect(ids)
        a = agg[cl]; a[0] += 1; a[1].update(e); a[2] += gold
    return agg

maps = [(10101, "Crypt normal"), (10102, "Crypt Classic"), (10103, "Crypt Hard"),
        (10201, "Castle normal"), (10202, "Castle Classic"), (10203, "Castle Hard")]
ru = {"gold": "gold", "weapon": "weap", "armor": "armor", "jewelry": "jewel", "gem": "gem", "treasure": "treas",
      "potion": "potion", "throwable": "throw", "supply": "supply", "currency": "curr", "other": "other", "??": "??"}
def line(a):
    n, e, gold = a
    tot = sum(e.values()) / n
    parts = ", ".join(f"{ru[c]} {v/n:.2f}" for c, v in e.most_common() if v / n >= 0.01)
    return f"{tot:.2f} items (≈{gold/n:.0f} gold) | {parts}"

print("\n=== average loot per source (loot_type 0), by source class")
ours = summarize(our, (0, 0))
for cl in sorted(ours):
    print(f"  [{cl[2:]}]")
    print(f"    OURS (all maps)  {ours[cl][0]:>3} src: {line(ours[cl])}")
    for m, title in maps:
        a = summarize(pve, (m, 0)).get(cl)
        if a: print(f"    PVE {title:<14} {a[0]:>3} src: {line(a)}")
