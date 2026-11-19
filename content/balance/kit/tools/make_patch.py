"""Edited workbook balance.xlsx -> patch (patch-<author>-<time>.json) and a report of what changed.

    python tools/make_patch.py balance.xlsx --author YourNick [--comment "why"] [-o patch.json]

Only yellow (editable) cells that differ from the kit go into the patch. Edits of grey cells give a warning.
"""
import argparse
import json
import sys
import time
from pathlib import Path

import kitlib

try:
    from openpyxl import load_workbook
except ImportError:
    sys.exit("openpyxl is required: pip install openpyxl")


def read_meta(wb):
    if kitlib.META_SHEET not in wb.sheetnames:
        sys.exit("the workbook has no _meta sheet — it is not a balance kit workbook")
    ws = wb[kitlib.META_SHEET]
    rows = list(ws.iter_rows(values_only=True))
    version = rows[0][1]
    sheets = {r[0]: (r[1], r[2]) for r in rows[2:] if r and r[0]}
    return version, sheets


def changes_of_sheet(ws, table, layout, tcfg, orig, warns):
    paths = [c.value for c in ws[2]]
    keys = {r: v["key"] for (r, _), v in orig.items()}
    out = []

    def add(row, path, cell_value, key, title):
        o = orig.get((row, path))
        old = o["value"] if o else ""
        new = kitlib.cell_text(cell_value, old)
        if kitlib.same(old, new) or (old == "" and new == ""):
            return
        if not kitlib.editable(tcfg, path):
            # grey "value · label" cells are not compared by the label
            if not (isinstance(cell_value, str) and " · " in cell_value and kitlib.same(cell_value.split(" · ")[0], old)):
                warns.append(f"{table} row {row} {path}: grey cell changed ({old} -> {new}) — will not go into the patch")
            return
        out.append({"table": table, "row": row, "key": key, "title": title, "path": path, "old": old, "new": new})

    for cells in ws.iter_rows(min_row=3, values_only=True):
        if not cells or cells[0] is None:
            continue
        row = int(cells[0])
        key = kitlib.cell_text(cells[1])
        title = cells[2] or ""
        orig_key = keys.get(row)
        if orig_key is not None and not kitlib.same(orig_key, key):
            warns.append(f"{table} row {row}: key column changed ({orig_key} -> {key}) — will not go into the patch")
            key = orig_key
        if layout == "long":
            add(row, cells[4], cells[6], key, title)
        else:
            for c, path in enumerate(paths[4:], start=4):
                if path:
                    add(row, path, cells[c] if c < len(cells) else None, key, title)
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("book", help="balance.xlsx after editing")
    ap.add_argument("--author", required=True, help="your nick (who proposes)")
    ap.add_argument("--comment", default="", help="why the edit — shown at review")
    ap.add_argument("-o", "--out", help="patch file (default patch-<author>-<time>.json next to the workbook)")
    ap.add_argument("--kit", help="kit folder (default — above tools/)")
    a = ap.parse_args()

    root = kitlib.kit_root(a.kit)
    manifest, kit = kitlib.load_kit(root)
    tables = kitlib.tables_of(kit)
    wb = load_workbook(a.book, data_only=True)
    version, sheets = read_meta(wb)
    if version != manifest["version"]:
        print(f"! workbook from kit {version}, but the kit is {manifest['version']} — comparing with the current kit")

    changes, warns = [], []
    for sheet, (table, layout) in sheets.items():
        if sheet not in wb.sheetnames or table not in tables:
            continue
        orig = kitlib.by_row_path(kitlib.load_values(root, table))
        changes += changes_of_sheet(wb[sheet], table, layout, tables[table], orig, warns)

    for w in warns:
        print("!", w)
    if not changes:
        print("no changes")
        return
    print(f"changes: {len(changes)}")
    for c in changes:
        o, n = kitlib.as_number(c["old"]), kitlib.as_number(c["new"])
        pct = f" ({(n - o) / o * 100:+.0f}%)" if o not in (None, 0) and n is not None else ""
        print(f"  {c['table']:<24} {c['title'] or c['key']:<24} {c['path']}: {c['old'] or '(empty)'} -> {c['new']}{pct}")

    # season — which season the patch is for (null — server base): the server writes the edit into that season's tables
    patch = {"format": kitlib.PATCH_FORMAT, "base": manifest["version"], "season": manifest.get("season"), "author": a.author,
             "comment": a.comment, "created": time.strftime("%Y-%m-%d %H:%M"), "changes": changes}
    out = Path(a.out) if a.out else Path(a.book).with_name(f"patch-{a.author}-{time.strftime('%Y%m%d-%H%M')}.json")
    out.write_text(json.dumps(patch, ensure_ascii=False, indent=1), encoding="utf-8")
    print(f"patch -> {out}")


if __name__ == "__main__":
    main()
