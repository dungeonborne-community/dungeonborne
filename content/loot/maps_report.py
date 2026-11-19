"""All maps: map_detail + map_select + competition_level + PVE loot. Arguments: <balance> <strings.tsv> <pve json> <out>"""
import json, os, sys, collections

bal, strings_tsv, pve, out = sys.argv[1:5]
sys.stdout = open(out, "w", encoding="utf-8")

def rows(d, t):
    p = os.path.join(d, t + ".json")
    if not os.path.exists(p): return []
    with open(p, encoding="utf-8-sig") as f: return json.load(f)["rows"]

loc = {}
with open(strings_tsv, encoding="utf-8") as f:
    for line in f:
        p = line.rstrip("\n").split("\t")
        if len(p) >= 3: loc.setdefault(p[1], p[2])

def short(path): return path.rsplit("/", 1)[-1]

detail = {r["map_id"]: r for r in rows(bal, "map_detail")}
select = rows(bal, "map_select")
comp = {r["competition_level_id"]: r for r in rows(bal, "competition_level")}
feat = {r["model_feature_id"]: loc.get(r.get("model_feature_name", ""), r.get("model_feature_name", "")) for r in rows(bal, "map_select_model_feature")}
pve_loot = collections.Counter()
for r in rows(pve, "loot_ue_source_choose_group"):
    pve_loot[r.get("map_id", 0)] += 1
our_loot = collections.Counter(r.get("map_id", 0) for r in rows(bal, "loot_ue_source_choose_group"))

# which map_id the lobby map selection refers to
selected_by = collections.defaultdict(list)
for s in select:
    selected_by[s["map_id"]].append(s)

print("=== map_select — maps in the selection menu (lobby)")
print(f"{'id':<9} {'map_id':>6} {'diff':>4} {'level_id':>9} {'type':>5} {'players':>7}  name / features")
for s in select:
    name = loc.get(s.get("map_name", ""), s.get("map_name", ""))
    feats = ", ".join(feat.get(f, str(f)) for f in s.get("model_feature_ids", []))
    c = comp.get(s.get("level_id"))
    cinfo = f" | competition: pvp_mode={c.get('pvp_mode','')} gear_rarity>={c.get('battle_gear_rarity_require','')}" if c else ""
    print(f"{s['map_name_id']:<9} {s['map_id']:>6} {s.get('difficulty',0):>4} {s.get('level_id',0):>9} {s.get('level_type',0):>5} {s.get('join_members',0):>7}  {name}"
          f"{' — ' + feats if feats else ''}{cinfo}")

print("\n=== map_detail — all maps (map_id), levels, loot")
print(f"{'map_id':>6} {'loot_map':>8} {'settle type':<12} {'in menu':<10} {'PVEloot':>7} {'ourLoot':>7}  levels (floors)")
for mid, r in sorted(detail.items()):
    levels = [short(c["ue_path"]) + ("" if "LEVEL_ONE" in c.get("room_type", "") or "WAITING" in c.get("room_type", "") else f"[{c.get('room_type','').rsplit('_',1)[-1]}]")
              for c in r.get("map_config", []) if "WAITING" not in c.get("room_type", "")]
    settle = r.get("settle_interface_type", "").replace("EN_SETTLE_INTERFACE_TYPE_", "")
    menu = ",".join(s["map_name_id"] for s in selected_by.get(mid, [])) or "-"
    print(f"{mid:>6} {r.get('loot_map_id',0):>8} {settle:<12} {menu:<10} {pve_loot.get(r.get('loot_map_id',0),0):>7} {our_loot.get(r.get('loot_map_id',0),0):>7}  {' + '.join(levels)}"
          f"  opts={r.get('option_args',[])}")

print("\n=== map_id from PVE loot that are not in map_detail")
miss = sorted(m for m in pve_loot if m not in {r.get('loot_map_id', 0) for r in detail.values()} and m != 0)
print(f"  {len(miss)}: {miss}")
