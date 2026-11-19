"""Picks one or more edits per editable table of a kit: python m2_gen.py <kit dir> <out.json>"""
import csv, json, re, sys
from pathlib import Path

kit = Path(sys.argv[1])
cfg = json.load(open(kit / "kit.json", encoding="utf-8"))
cat = json.load(open(kit / "catalog.json", encoding="utf-8"))
sys.path.insert(0, str(kit / "tools"))
import kitlib


def wc(p, t):
    return kitlib.wildcard(p, t)


edits = []
for t in cfg["table"]:
    # Loot has its own acceptance (s2_loot_accept.py): its values live in the template, not in the tables
    if not t["editable"] or t.get("source"):
        continue
    name = t["name"]
    rows = list(csv.DictReader(open(kit / "data" / f"{name}.csv", encoding="utf-8-sig", newline="")))
    types = cat["field_types"][name]
    ed = [r for r in rows if r["path"] and any(wc(p, r["path"]) for p in t["editable"])]
    picked = 0
    for r in ed:
        if r["path"].endswith("min_value") or r["path"].endswith(".unit") or kitlib.as_number(r["value"]) in (None, 0):
            continue
        kind = types[kitlib.generic(r["path"])]
        n = kitlib.as_number(r["value"])
        new = str(int(n) + 2) if kind in kitlib.INT_RANGES else format(n * 1.25, ".6g")
        edits.append({"table": name, "row": int(r["row"]), "path": r["path"], "key": r["key"], "old": r["value"], "new": new, "why": f"{kind} value"})
        picked += 1
        if picked == 2:
            break
    # a float field too, when the table has one among its rows further down
    for r in ed:
        kind = types[kitlib.generic(r["path"])]
        if kind in ("float", "double") and "." in r["value"] and r["path"].endswith("max_value"):
            n = kitlib.as_number(r["value"])
            new = format(n * 1.25, ".6g")
            if not any(e["table"] == name and e["row"] == int(r["row"]) and e["path"] == r["path"] for e in edits):
                edits.append({"table": name, "row": int(r["row"]), "path": r["path"], "key": r["key"], "old": r["value"], "new": new, "why": "fraction"})
            break
# an empty cell: durability that the table has as a column but this row doesn't
gear = list(csv.DictReader(open(kit / "data" / "gear_template.csv", encoding="utf-8-sig", newline="")))
has = {r["row"] for r in gear if r["path"] == "durability"}
empty = next(r for r in gear if r["row"] not in has)
edits.append({"table": "gear_template", "row": int(empty["row"]), "path": "durability", "key": empty["key"], "old": "", "new": "77", "why": "empty cell"})
json.dump(edits, open(sys.argv[2], "w", encoding="utf-8"), ensure_ascii=False, indent=1)
for e in edits:
    print(e["table"], e["row"], e["path"][:60], e["old"], "->", e["new"], e["why"])
