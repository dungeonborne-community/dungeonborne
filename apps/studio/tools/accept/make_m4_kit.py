"""Makes a NEWER test kit from a real kit zip, as if the server had moved on:
    python make_m4_kit.py <kit.zip> <out.zip>
Only for acceptance. Version +1 day; in gear_template the 2nd durability row gets 151, the 3rd 200, the 4th 201; the class at
row 5 of role_level_upgrade loses its rows (key removed), a new row is inserted at the very top of gear_template."""
import csv, io, json, re, sys, zipfile
from pathlib import Path

src, out = Path(sys.argv[1]), Path(sys.argv[2])
zin = zipfile.ZipFile(src)
root = next(n for n in zin.namelist() if n.endswith("/manifest.json"))[: -len("manifest.json")]
manifest = json.loads(zin.read(root + "manifest.json").decode("utf-8"))
old_version = manifest["version"]
date, tail = old_version.split("-")
manifest["version"] = f"{int(date) + 1}-{tail}"


def read(name):
    text = zin.read(root + name).decode("utf-8-sig")
    return list(csv.DictReader(io.StringIO(text, newline="")))


def dump(rows):
    b = io.StringIO(newline="")
    w = csv.DictWriter(b, fieldnames=["row", "key", "title", "path", "value", "name"], lineterminator="\n")
    w.writeheader()
    w.writerows(rows)
    return ("﻿" + b.getvalue()).encode("utf-8")


gear = read("data/gear_template.csv")
with_durability = [r["row"] for r in gear if r["path"] == "durability"]
plan = {with_durability[1]: "151", with_durability[2]: "200", with_durability[3]: "201"}
for r in gear:
    if r["path"] == "durability" and r["row"] in plan:
        r["value"] = plan[r["row"]]
# a new row at the top: every row number moves by one
for r in gear:
    r["row"] = str(int(r["row"]) + 1)
gear = [{"row": "0", "key": "999999999", "title": "Новый", "path": "item_id", "value": "999999999", "name": "Новый"}] + gear

roles = read("data/role_level_upgrade.csv")
gone = next(r["key"] for r in roles if r["row"] == "5")
kept = [r for r in roles if r["key"] != gone]
numbers = {}
for r in kept:
    numbers.setdefault(r["row"], str(len(numbers)))
    r["row"] = numbers[r["row"]]

replaced = {"data/gear_template.csv": dump(gear), "data/role_level_upgrade.csv": dump(kept), "manifest.json": json.dumps(manifest, ensure_ascii=False, indent=1).encode("utf-8")}
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    for n in zin.namelist():
        if n.endswith("/"):
            continue
        rel = n[len(root):]
        if rel.startswith(("data/", "raw/", "icons/")) or rel in ("manifest.json", "kit.json", "catalog.json"):
            z.writestr(n, replaced.get(rel, zin.read(n)))
print(old_version, "->", manifest["version"], "removed class", gone)
