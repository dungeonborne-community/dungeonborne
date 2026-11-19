"""Balance kit -> balance.xlsx (for people) and llm/*.md (for LLMs).

    python tools/build_excel.py [kit folder]

One sheet per table. Yellow cells may be edited, grey ones are for reference. Orange — the value differs from
the game's original (the cell note holds the original value), the "Differences" sheet lists all such values. Sheet rows 1–2: readable column name
and field path (make_patch.py uses the path to know what changed — do not touch row 2). The "Picture" column: a picture
from icons/<value of the icon field>.png (.jpg) if there is one, otherwise the value itself (the picture is linked by it later).
Requires openpyxl (pip install openpyxl).
"""
import json
import sys
from collections import OrderedDict
from pathlib import Path

import kitlib

try:
    from openpyxl import Workbook
    from openpyxl.comments import Comment
    from openpyxl.drawing.image import Image as XlImage
    from openpyxl.styles import Alignment, Font, PatternFill
    from openpyxl.utils import get_column_letter
except ImportError:
    sys.exit("openpyxl is required: pip install openpyxl")

EDIT = PatternFill("solid", fgColor="FFF2CC")
CHANGED = PatternFill("solid", fgColor="F8CBAD")  # differs from the game's original
NEW_ROW = PatternFill("solid", fgColor="E2EFDA")  # the row is not in the original
HEAD = PatternFill("solid", fgColor="D9E1F2")
GREY = Font(color="808080")
SMALL = Font(color="808080", size=8)
BOLD = Font(bold=True)
FIXED = ["row", "key", "row name", "Picture"]  # first columns of a sheet
ICON_PX = 40


def icon_file(root, value):
    """Icon thumbnail (cache .thumbs/, 64 px): the workbook does not bloat with full-size pictures in every cell"""
    for ext in ("png", "jpg", "jpeg", "webp"):
        p = root / "icons" / f"{value}.{ext}"
        if p.exists():
            break
    else:
        return None
    thumb = root / ".thumbs" / f"{value}.png"
    if not thumb.exists():
        try:
            from PIL import Image as PilImage
        except ImportError:
            return p  # without Pillow openpyxl cannot insert the picture anyway — put_icon keeps the key
        thumb.parent.mkdir(exist_ok=True)
        with PilImage.open(p) as im:
            im.thumbnail((64, 64))
            im.save(thumb, "PNG", optimize=True)
    return thumb


def put_icon(ws, root, cell_ref, row_idx, value):
    """A picture into the cell or (no file) the key value in grey — a placeholder for the picture"""
    if value in ("", None):
        return
    p = icon_file(root, value)
    if p is None:
        ws[cell_ref] = str(value)
        ws[cell_ref].font = SMALL
        return
    try:
        img = XlImage(str(p))
        scale = ICON_PX / max(img.width, img.height, 1)
        img.width, img.height = int(img.width * scale), int(img.height * scale)
        ws.add_image(img, cell_ref)
        ws.row_dimensions[row_idx].height = ICON_PX * 0.78
    except Exception as e:  # no Pillow or a broken file — keep the key
        ws[cell_ref] = f"{value} ({e.__class__.__name__})"
        ws[cell_ref].font = SMALL


def rows_of(values):
    """row -> {path: value-dict}, paths in order of appearance"""
    rows, paths = OrderedDict(), OrderedDict()
    for v in values:
        rows.setdefault(v["row"], OrderedDict())
        if v["path"]:
            rows[v["row"]][v["path"]] = v
            paths.setdefault(v["path"], set()).add(v["name"])
    return rows, paths


def header(ws, labels, paths):
    for c, (label, path) in enumerate(zip(labels, paths), start=1):
        h = ws.cell(row=1, column=c, value=label)
        h.font, h.fill = BOLD, HEAD
        h.alignment = Alignment(wrap_text=True, vertical="top")
        p = ws.cell(row=2, column=c, value=path)
        p.font = SMALL
    ws.row_dimensions[1].height = 42
    ws.freeze_panes = ws.cell(row=3, column=len(FIXED) + 1)
    ws.auto_filter.ref = f"A2:{get_column_letter(len(paths))}2"


def orig_text(o):
    return "not in the game's original" if o is None else f"game's original: {o if o != '' else '(default)'}"


def mark(c, diff, row, path):
    """The cell differs from the original: orange + a note with the original value"""
    if (row, path) not in diff:
        return
    o = diff[(row, path)]
    c.fill = NEW_ROW if o is None else CHANGED
    if o is not None:
        c.comment = Comment(orig_text(o), "original")
        c.comment.width, c.comment.height = 220, 40


def wide_sheet(wb, root, kit, t, values, diff):
    ws = wb.create_sheet(t["name"][:31])
    rows, paths = rows_of(values)
    cols = list(paths)
    titles_en, names_en = kitlib.load_en(root, t["name"])

    def head(p, n):
        label = kitlib.both(kitlib.field_label(kit, p, next(iter(n)) if len(n) == 1 else ""), kitlib.en_label(kit, p))
        unit = kitlib.unit_of(kit, p)
        return f"{label} ({unit})" if unit else label

    labels = FIXED + [head(p, n) for p, n in paths.items()]
    header(ws, labels, ["row", "key", "title", "icon:" + (t.get("icon") or "")] + cols)
    edit_col = [kitlib.editable(t, p) for p in cols]
    for i, (row, cells) in enumerate(rows.items()):
        r = i + 3
        first = next(iter(cells.values()), None) or next(v for v in values if v["row"] == row)
        ws.cell(row=r, column=1, value=row).font = GREY
        ws.cell(row=r, column=2, value=kitlib.typed(first["key"])).font = GREY
        row_unit = kitlib.unit_of(kit, "", cells.get("attribute_type", {}).get("value"))
        # a row without a name (vendor product) — name from the icon field (what is given)
        icon_cell = cells.get(t.get("icon") or "", {})
        title = kitlib.both(first["title"] or icon_cell.get("name", ""), titles_en.get(row, "") or names_en.get((row, t.get("icon") or ""), ""))
        ws.cell(row=r, column=3, value=f"{title} ({row_unit})" if row_unit else title).font = GREY
        icon_val = cells.get(t.get("icon") or "", {}).get("value", "") if t.get("icon") else ""
        if not t.get("source"):  # loot template: thousands of rows, pictures would triple the workbook, the item name is in the row anyway
            put_icon(ws, root, f"D{r}", r, icon_val)
        if any(diff.get((row, p), "") is None for p in cells):
            ws.cell(row=r, column=3).fill = NEW_ROW  # row added by the server/season
        for j, p in enumerate(cols):
            c = ws.cell(row=r, column=len(FIXED) + 1 + j)
            v = cells.get(p)
            if edit_col[j]:
                c.fill = EDIT
                c.value = kitlib.typed(v["value"]) if v else None
            elif v:
                # for reference: value with a label (item, currency…) if each row has its own label
                c.value = f"{v['value']} · {v['name']}" if v["name"] and len(paths[p]) > 1 else kitlib.typed(v["value"])
                c.font = GREY
            mark(c, diff, row, p)
    widths = [6, 11, 24, 9] + [14] * len(cols)
    for c, w in enumerate(widths, start=1):
        ws.column_dimensions[get_column_letter(c)].width = w
    return ws


def long_sheet(wb, root, kit, t, values, diff):
    ws = wb.create_sheet(t["name"][:31])
    # the 8th column "game's original" is for reference only (make_patch reads the path and value columns)
    header(ws, FIXED + ["field", "label", "value", "game's original"], ["row", "key", "title", "icon:" + (t.get("icon") or ""), "path", "name", "value", "orig"])
    icons = {v["row"]: v["value"] for v in values if t.get("icon") and v["path"] == t["icon"]}
    titles_en, _ = kitlib.load_en(root, t["name"])
    seen = set()
    r = 3
    for v in values:
        if not v["path"]:
            continue
        ws.cell(row=r, column=1, value=v["row"]).font = GREY
        ws.cell(row=r, column=2, value=kitlib.typed(v["key"])).font = GREY
        ws.cell(row=r, column=3, value=kitlib.both(v["title"], titles_en.get(v["row"], ""))).font = GREY
        if v["row"] not in seen:
            seen.add(v["row"])
            put_icon(ws, root, f"D{r}", r, icons.get(v["row"], ""))
        ws.cell(row=r, column=5, value=v["path"]).font = GREY
        unit = kitlib.unit_of(kit, v["path"])
        label = kitlib.both(kitlib.field_label(kit, v["path"], v["name"]), kitlib.en_label(kit, v["path"]))
        ws.cell(row=r, column=6, value=f"{label} ({unit})" if unit else label).font = GREY
        c = ws.cell(row=r, column=7, value=kitlib.typed(v["value"]))
        if kitlib.editable(t, v["path"]):
            c.fill = EDIT
        else:
            c.font = GREY
        if (v["row"], v["path"]) in diff:
            o = diff[(v["row"], v["path"])]
            c.fill = NEW_ROW if o is None else CHANGED
            oc = ws.cell(row=r, column=8, value="not in the original" if o is None else (kitlib.typed(o) if o != "" else "(default)"))
            oc.font = GREY
        r += 1
    for c, w in enumerate([6, 11, 24, 9, 48, 28, 12, 14], start=1):
        ws.column_dimensions[get_column_letter(c)].width = w
    return ws


def season_line(manifest):
    s = manifest.get("season")
    return f"Season: {s} — season values on top of the server base" if s else "No season — server base"


def summary_text(summary):
    if summary is None:
        return "no original in the game"
    if not any(summary.values()):
        return "matches the game's original"
    parts = [f"values changed: {summary['changed']}"]
    if summary["new_rows"]:
        parts.append(f"new rows: {summary['new_rows']}")
    if summary["removed_rows"]:
        parts.append(f"rows removed: {summary['removed_rows']}")
    return ", ".join(parts)


def readme_sheet(wb, manifest, kit, summaries):
    ws = wb.active
    ws.title = "How to edit"
    lines = [
        (kit.get("title", "Balance"), BOLD),
        (f"Kit version: {manifest['version']}. {season_line(manifest)}", None),
        ("", None),
        ("1. Change only YELLOW cells. Grey ones are for reference (their changes will not go into the patch).", None),
        ("   ORANGE — the value already differs from the game's original (the original is in the cell note), GREEN — the row is not in the original.", None),
        ("   All differences as a list — the Differences sheet.", None),
        ("2. Do not change sheet rows 1–2 or the row column — the edit finds its place by them.", None),
        ("3. Sorting and filtering are fine. Do not delete or add rows.", None),
        ("4. Save the file and run:  python tools/make_patch.py balance.xlsx --author YourNick", None),
        ("   You get patch-*.json — send that. Check: python tools/check.py patch-*.json", None),
        ("", None),
        ("Good to know:", BOLD),
    ]
    lines += [(f"  • {n}", None) for n in kit.get("notes", [])]
    lines += [("", None), ("Tables:", BOLD)]
    for t in kit.get("table", []):
        ro = "" if t.get("editable") else "  [read only]"
        src = " · season table" if manifest.get("tables", {}).get(t["name"], {}).get("source") == "season" else ""
        lines.append((f"  {t['name']} — {t.get('title', '')}{ro}", None))
        lines.append((f"      {t.get('about', '')}", GREY))
        lines.append((f"      Versus the game's original: {summary_text(summaries.get(t['name']))}{src}", GREY))
        if t.get("where"):
            lines.append((f"      Where it applies: {t['where']}", GREY))
    for i, (text, font) in enumerate(lines, start=1):
        c = ws.cell(row=i, column=1, value=text)
        if font:
            c.font = font
    ws.column_dimensions["A"].width = 120


def diff_sheet(wb, root, kit, tables_data):
    """All differences of the active tables from the game's original — as one list"""
    ws = wb.create_sheet("Differences", 1)
    head = ["table", "row", "key", "row name", "field", "label", "game's original", "now", "change", "editable"]
    for c, h in enumerate(head, start=1):
        x = ws.cell(row=1, column=c, value=h)
        x.font, x.fill = BOLD, HEAD
    r = 2
    for t in kit.get("table", []):
        values, diff = tables_data[t["name"]][0], tables_data[t["name"]][2]
        if not diff:
            continue
        by = kitlib.by_row_path(values)
        titles, icon = {}, t.get("icon") or ""
        for v in values:
            titles.setdefault(v["row"], (v["key"], v["title"]))
        # a row without a name (vendor product) — the name of what is given (icon field)
        for v in values:
            if icon and v["path"] == icon and not titles[v["row"]][1]:
                titles[v["row"]] = (v["key"], v["name"])
        for (row, path), o in sorted(diff.items(), key=lambda x: (x[0][0], x[0][1])):
            v = by.get((row, path))
            cur = v["value"] if v else ""
            key, title = titles.get(row, ("", ""))
            label = kitlib.field_label(kit, path, v["name"] if v else "")
            on, cn = (kitlib.as_number(o) if o else None), kitlib.as_number(cur)
            pct = f"{(cn - on) / on * 100:+.0f}%" if on not in (None, 0) and cn is not None else ""
            row_vals = [t["name"], row, kitlib.typed(key), title, path, label,
                        "not in the original" if o is None else (kitlib.typed(o) if o != "" else "(default)"),
                        kitlib.typed(cur) if cur != "" else "(default)", pct, "yes" if kitlib.editable(t, path) else ""]
            for c, val in enumerate(row_vals, start=1):
                ws.cell(row=r, column=c, value=val)
            ws.cell(row=r, column=8).fill = NEW_ROW if o is None else CHANGED
            r += 1
    if r == 2:
        ws.cell(row=2, column=1, value="All kit tables match the game's original.")
    ws.freeze_panes = "A2"
    ws.auto_filter.ref = f"A1:{get_column_letter(len(head))}{max(r - 1, 1)}"
    for c, w in enumerate([24, 6, 11, 28, 44, 28, 14, 14, 10, 8], start=1):
        ws.column_dimensions[get_column_letter(c)].width = w
    return r - 2


def meta_sheet(wb, manifest, kit):
    ws = wb.create_sheet(kitlib.META_SHEET)
    ws.append(["kit_version", manifest["version"]])
    ws.append(["sheet", "table", "layout"])
    for t in kit.get("table", []):
        ws.append([t["name"][:31], t["name"], t.get("layout", "wide")])
    ws.sheet_state = "hidden"


def llm_files(root, manifest, kit, tables_data):
    """llm/<table>.md: description, what may be edited, field legend and data (editable fields only).
    wide — CSV: table row = CSV line, column = path; long — a block per table row, "path=value"."""
    out = root / "llm"
    out.mkdir(exist_ok=True)
    for t in kit.get("table", []):
        if not t.get("editable"):
            continue  # reference tables — only in the workbook
        all_values, orig, diff, summary = tables_data[t["name"]]
        # "in the original" — range from the game's table (no original — from the current one)
        rng = kitlib.ranges(orig if orig is not None else all_values)
        row_attr = {v["row"]: v["value"] for v in all_values if v["path"] == "attribute_type"}
        values = [v for v in all_values if v["path"] and kitlib.editable(t, v["path"])]
        rows, paths = rows_of(values)
        titles_en, names_en = kitlib.load_en(root, t["name"])
        icon = t.get("icon") or ""
        icon_names = {v["row"]: v["name"] for v in all_values if icon and v["path"] == icon}

        def row_title(row, first):
            """row name; without a name (vendor product) — the name of what is given (icon field)"""
            return kitlib.both(first["title"] or icon_names.get(row, ""), titles_en.get(row, "") or names_en.get((row, icon), ""))

        lines = [
            f"# {t['name']} — {t.get('title', '')}",
            "",
            t.get("about", ""),
            "",
            f"Where it applies: {t.get('where', '?')}.",
            "",
            f"{season_line(manifest)}. Versus the game's original: {summary_text(summary)}.",
            "",
            f"Kit version: {manifest['version']}. Only these paths may be changed: " + ", ".join(f"`{p}`" for p in t.get("editable", [])),
            "row is the table row number, key its first field (id), title the row name. In the patch row, key, path and old exactly as here;",
            "old = the current value; empty = the default value (write \"\" in old).",
            "",
            "## Fields",
        ]
        for p, n in paths.items():
            orig = kitlib.fmt_range(rng.get(kitlib.generic(p)))
            extra = [x for x in (kitlib.unit_of(kit, p), f"in the original {orig}" if orig else "") if x]
            label = kitlib.both(kitlib.field_label(kit, p, next(iter(n)) if len(n) == 1 else ""), kitlib.en_label(kit, p))
            lines.append(f"- `{p}` — {label}" + (" · " + " · ".join(extra) if extra else ""))
        if row_attr:
            lines.append("- the unit of a row's value follows its attribute (attribute_type): given in title in brackets")
        lines += ["", "## Data", ""]
        clean = lambda s: str(s).replace(",", " ").replace("\n", " ")
        if t.get("layout") == "long":
            for row, cells in rows.items():
                first = next(iter(cells.values()))
                lines.append(f"row {row} | key {first['key']} | {clean(row_title(row, first))}")
                lines += [f"  {p}={v['value']}" for p, v in cells.items()]
        else:
            cols = list(paths)
            lines += ["```csv", ",".join(["row", "key", "title"] + cols)]
            for row, cells in rows.items():
                first = next(iter(cells.values()))
                u = kitlib.unit_of(kit, "", row_attr.get(row))
                title = clean(row_title(row, first)) + (f" ({u})" if u else "")
                lines.append(",".join([str(row), first["key"], title] + [cells[p]["value"] if p in cells else "" for p in cols]))
            lines.append("```")
        # differences of editable fields from the game's original: what the server/season already changed
        ed = [(k, o) for k, o in sorted(diff.items()) if kitlib.editable(t, k[1])]
        if ed:
            by = kitlib.by_row_path(all_values)
            lines += ["", "## Differences from the game's original", "",
                      "The data above is current (server/season). Here — which of it is no longer as in the game: `path`: original -> now.", ""]
            for (row, path), o in ed[:400]:
                v = by.get((row, path))
                cur = v["value"] if v else ""
                first = v or {"key": "", "title": ""}
                lines.append(f"- row {row} | key {first['key']} | {clean(row_title(row, first))}: `{path}`: "
                             + ("not in the original" if o is None else (o or "(default)")) + f" -> {cur or '(default)'}")
            if len(ed) > 400:
                lines.append(f"- … and {len(ed) - 400} more (full list — the Differences sheet in balance.xlsx)")
        (out / f"{t['name']}.md").write_text("\n".join(lines) + "\n", encoding="utf-8")
    # instruction template: tools/PROMPT.template.md (old kits — PROMPT.md in the root)
    prompt = next((p for p in (root / "tools" / "PROMPT.template.md", root / "PROMPT.md") if p.exists()), None)
    if prompt:
        notes = "\n".join(f"- {n}" for n in kit.get("notes", []))
        season = manifest.get("season")
        text = (prompt.read_text(encoding="utf-8").replace("{VERSION}", manifest["version"]).replace("{NOTES}", notes)
                .replace("{SEASON_JSON}", json.dumps(season)).replace("{SEASON}", season_line(manifest)))
        (out / "PROMPT.md").write_text(text, encoding="utf-8")


def main():
    root = kitlib.kit_root(sys.argv[1] if len(sys.argv) > 1 else None)
    manifest, kit = kitlib.load_kit(root)
    # table -> (values, game's original, differences {(row, path): original}, summary)
    tables_data = {}
    for t in kit.get("table", []):
        values = kitlib.load_values(root, t["name"])
        orig = kitlib.load_orig(root, t["name"])
        diff, summary = kitlib.diff_orig(values, orig)
        tables_data[t["name"]] = (values, orig, diff, summary)
    print(season_line(manifest))
    wb = Workbook()
    readme_sheet(wb, manifest, kit, {k: v[3] for k, v in tables_data.items()})
    for t in kit.get("table", []):
        values, _, diff, summary = tables_data[t["name"]]
        (long_sheet if t.get("layout") == "long" else wide_sheet)(wb, root, kit, t, values, diff)
        print(f"  {t['name']:<28} {len(values)} values ({t.get('layout', 'wide')}); versus the original: {summary_text(summary)}")
    n = diff_sheet(wb, root, kit, tables_data)
    meta_sheet(wb, manifest, kit)
    path = root / "balance.xlsx"
    wb.save(path)
    llm_files(root, manifest, kit, tables_data)
    print(f"workbook -> {path} (differences from the original: {n}); for LLMs -> {root / 'llm'}")


if __name__ == "__main__":
    main()
