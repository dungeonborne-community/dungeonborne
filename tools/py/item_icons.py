"""Item and equipment icons -> table (04.11).

Source of the link — ExcelConfigjson/UeSource_ItemType.json (768 items): type_id (item config id, as in
loot/market/craft), brush_data.resource_name — the inventory icon (cell size by position_cfg),
tips_brush_data.resource_name — the large tooltip icon. Equipment is supplemented from UeSource_Equipment.json
(by ue_source_equipment_equip_id: type, subtype, slots).

Images — PNGs exported by FModel (AppSettings: Exports, KeepDirectoryStructure): /Game/A/B.B ->
<exports>/Content/A/B.png. The script copies them to <out>/icons/<type_id>.png and <out>/tips/<type_id>.png and writes
<out>/item_icons.json and .csv; for what is missing — a list of folders still to export in FModel.

  python -I tools/item_icons.py              check + copy
  python -I tools/item_icons.py --check      check only (what was found, which folders are missing)
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W, local, resolve  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import argparse
import csv
import json
import os
import shutil
import sys
from collections import Counter

HERE = os.path.dirname(os.path.abspath(__file__))
DEF_EXPORTS = str(resolve(local("dumps", "fmodel_exports") or "home:dumps/fmodel/Exports/Dungeonborne"))  # FModel export (local.toml [dumps])
DEF_TABLES = str(W("ExcelConfigjson"))
DEF_OUT = str(W("assets/item_icons"))


def asset_of(brush):
    """'/Game/TGF/X/Icon_A.Icon_A' -> '/Game/TGF/X/Icon_A'; empty/None -> None"""
    name = (brush or {}).get("resource_name") or ""
    path = name.split(".")[0]
    return path if path.startswith("/Game/") else None


def png_of(exports, asset):
    return os.path.join(exports, "Content", *asset[len("/Game/"):].split("/")) + ".png"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exports", default=DEF_EXPORTS, help="FModel export folder up to Content")
    ap.add_argument("--tables", default=DEF_TABLES)
    ap.add_argument("--out", default=DEF_OUT)
    ap.add_argument("--check", action="store_true", help="do not copy, report only")
    a = ap.parse_args()

    with open(os.path.join(a.tables, "UeSource_ItemType.json"), encoding="utf-8") as f:
        items = json.load(f)
    with open(os.path.join(a.tables, "UeSource_Equipment.json"), encoding="utf-8") as f:
        equip = {e["equip_id"]: e for e in json.load(f)}

    rows, missing = [], Counter()
    for it in items:
        tid = it["type_id"]
        eq = equip.get(it.get("ue_source_equipment_equip_id"), {})
        pos = it.get("position_cfg", {})
        row = {
            "type_id": tid,
            "item_type": (it.get("item_type") or "").replace("EN_ITEM_UE_TYPE_", ""),
            "rarity": (it.get("rarity") or "").replace("EN_ITEM_RARITY_", ""),
            "slot_type": (it.get("slot_type") or "").replace("EN_SKIN_SLOT_TYPE_", ""),
            "equip_id": it.get("ue_source_equipment_equip_id", ""),
            "equipment_type": (eq.get("equipment_type") or "").replace("EN_GEAR_EQUIPMENT_TYPE_", ""),
            "equip_subtype": (eq.get("equip_subtype") or "").replace("EN_GEAR_EQUIP_SUBTYPE_", ""),
            "grid": "%sx%s" % (pos.get("column_size", ""), pos.get("row_size", "")),
            "stack": pos.get("accumulation_limit", ""),
            "ue_path": it.get("ue_path", ""),
        }
        for key, brush, sub in (("icon", it.get("brush_data"), "icons"), ("tips", it.get("tips_brush_data"), "tips")):
            asset = asset_of(brush)
            row[key + "_asset"] = asset or ""
            row[key + "_file"] = ""
            if not asset:
                continue
            src = png_of(a.exports, asset)
            if not os.path.isfile(src):
                missing[asset.rsplit("/", 1)[0]] += 1
                continue
            rel = "%s/%s.png" % (sub, tid)
            row[key + "_file"] = rel
            if not a.check:
                dst = os.path.join(a.out, sub, "%s.png" % tid)
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.copyfile(src, dst)
        rows.append(row)

    n = len(rows)
    has_icon = sum(1 for r in rows if r["icon_file"])
    has_tips = sum(1 for r in rows if r["tips_file"])
    no_asset = sum(1 for r in rows if not r["icon_asset"])
    print("items %d: icon %d, tooltip %d, no icon in the table %d" % (n, has_icon, has_tips, no_asset))
    if missing:
        print("no PNG (export these folders in FModel: right click -> Save Folder's Packages Textures):")
        for folder, cnt in sorted(missing.items()):
            print("  %-70s %d" % (folder, cnt))

    if a.check:
        return 0
    os.makedirs(a.out, exist_ok=True)
    with open(os.path.join(a.out, "item_icons.json"), "w", encoding="utf-8") as f:
        json.dump(rows, f, ensure_ascii=False, indent=1)
    with open(os.path.join(a.out, "item_icons.csv"), "w", encoding="utf-8-sig", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)
    print("-> %s (icons/, tips/, item_icons.json, item_icons.csv)" % a.out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
