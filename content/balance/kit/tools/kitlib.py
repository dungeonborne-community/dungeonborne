"""Shared by the balance kit scripts: reading the kit, edit rules, comparing values.

Kit (folder): manifest.json (version), kit.json (tables and what can be edited in them), data/<table>.csv
(long form: row,key,title,path,value,name — one line = one value), raw/ (source JSON, for reference).
"""
import csv
import json
import math
import re
import sys
from pathlib import Path

PATCH_FORMAT = "dbbalance-patch/1"
META_SHEET = "_meta"


def kit_root(arg=None):
    """Kit folder: the argument, otherwise the folder above tools/"""
    root = Path(arg) if arg else Path(__file__).resolve().parent.parent
    if not (root / "manifest.json").exists():
        sys.exit(f"does not look like a balance kit: no {root / 'manifest.json'}")
    return root


def load_kit(root):
    manifest = json.loads((root / "manifest.json").read_text(encoding="utf-8"))
    kit = json.loads((root / "kit.json").read_text(encoding="utf-8"))
    return manifest, kit


def tables_of(kit):
    return {t["name"]: t for t in kit.get("table", [])}


def load_values(root, table):
    """Long form of a table: list of dicts row,key,title,path,value,name (row is int)"""
    with open(root / "data" / f"{table}.csv", encoding="utf-8-sig", newline="") as f:
        out = []
        for r in csv.DictReader(f):
            r["row"] = int(r["row"])
            out.append(r)
        return out


_RAW = {}


def raw_at(root, table, row, path):
    """Value from raw/<table>.json by a long-form path: number type (int / float) as in the game.
    None — no such value (field at its default)."""
    if table not in _RAW:
        _RAW[table] = json.loads((root / "raw" / f"{table}.json").read_text(encoding="utf-8-sig"))["rows"]
    rows = _RAW[table]
    if not (0 <= row < len(rows)):
        return None
    cur = rows[row]
    for tok in re.findall(r"[^.\[\]]+|\[[^\]]*\]", path):
        if tok.startswith("["):
            k = tok[1:-1]
            if isinstance(cur, list):
                cur = cur[int(k)] if k.isdigit() and int(k) < len(cur) else None
            elif isinstance(cur, dict):
                cur = cur.get(k)
            else:
                return None
        elif isinstance(cur, dict):
            cur = cur.get(tok)
        else:
            return None
        if cur is None:
            return None
    return cur


def load_orig(root, table):
    """The game's original of the same table (data/<table>.orig.csv) in the same form as load_values; None — no original
    (the table is ours only, or the kit was built without originals)"""
    p = root / "data" / f"{table}.orig.csv"
    if not p.exists():
        return None
    with open(p, encoding="utf-8-sig", newline="") as f:
        out = []
        for r in csv.DictReader(f):
            r["row"] = int(r["row"])
            out.append(r)
        return out


def row_ids(values):
    """row -> (key, n): a row is identified by key and its number among rows with the same key, so added/removed
    rows do not shift the comparison with the original"""
    ids, seen, count = {}, set(), {}
    for v in values:
        if v["row"] in seen:
            continue
        seen.add(v["row"])
        n = count.get(v["key"], 0)
        count[v["key"]] = n + 1
        ids[v["row"]] = (v["key"], n)
    return ids


def diff_orig(values, orig):
    """Differences from the game's original: {(row, path): value in the original}; None as the value — the row is not in the original.
    Plus a summary: {"changed": N values, "new_rows": N, "removed_rows": N}. orig None — ({}, None)"""
    if orig is None:
        return {}, None
    ids, oids = row_ids(values), row_ids(orig)
    orow = {rid: row for row, rid in oids.items()}
    ov = {(oids[v["row"]], v["path"]): v["value"] for v in orig if v["path"]}
    cur = {(ids[v["row"]], v["path"]): v for v in values if v["path"]}
    out, new_rows = {}, set()
    for (rid, path), v in cur.items():
        if rid not in orow:
            out[(v["row"], path)] = None
            new_rows.add(v["row"])
        elif not same(ov.get((rid, path), ""), v["value"]):
            out[(v["row"], path)] = ov.get((rid, path), "")
    # fields that became the default value (they are absent from the current table)
    rows_of_id = {rid: row for row, rid in ids.items()}
    for (rid, path), o in ov.items():
        if rid in rows_of_id and (rid, path) not in cur and o != "":
            out[(rows_of_id[rid], path)] = o
    removed = sum(1 for rid in orow if rid not in rows_of_id)
    return out, {"changed": sum(1 for o in out.values() if o is not None), "new_rows": len(new_rows), "removed_rows": removed}


def by_row_path(values):
    return {(v["row"], v["path"]): v for v in values}


def wildcard(pattern, text):
    """'*' — any characters (including '.', '[', ']'), the rest literally — as in xtask"""
    return re.fullmatch(".*".join(re.escape(p) for p in pattern.split("*")), text) is not None


def editable(table_cfg, path):
    return any(wildcard(p, path) for p in table_cfg.get("editable", []))


_NUMBER = re.compile(r"[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?", re.ASCII)


def as_number(text):
    """A finite number; NaN, inf and 1_000 are not numbers (as in crates/balance-kit)"""
    t = str(text).strip().replace(",", ".")
    if not _NUMBER.fullmatch(t):
        return None
    n = float(t)
    return n if math.isfinite(n) else None


def same(a, b):
    a, b = str(a).strip(), str(b).strip()
    if a == b:
        return True
    x, y = as_number(a), as_number(b)
    return x is not None and y is not None and abs(x - y) <= 1e-9 * max(abs(x), abs(y), 1.0)


def is_int_text(text):
    return re.fullmatch(r"-?\d+", str(text).strip(), re.ASCII) is not None


# field types from catalog.json (field_types), if the kit has it
INT_RANGES = {
    "int32": (-(2 ** 31), 2 ** 31 - 1),
    "uint32": (0, 2 ** 32 - 1),
    "int64": (-(2 ** 63), 2 ** 63 - 1),
    "uint64": (0, 2 ** 64 - 1),
}
NUMBER_KINDS = set(INT_RANGES) | {"float", "double"}
_CATALOG = {}


def field_kind(root, table, path):
    """Field type by the game's schema (int32, float, enum…); None — an old kit without a catalog, or an unknown field"""
    if root not in _CATALOG:
        p = root / "catalog.json"
        _CATALOG[root] = json.loads(p.read_text(encoding="utf-8-sig")) if p.exists() else {}
    return _CATALOG[root].get("field_types", {}).get(table, {}).get(generic(path))


def cell_text(value, like=""):
    """Excel cell value -> text as in the kit (12.0 -> 12 if it was an integer before)"""
    if value is None:
        return ""
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        if float(value).is_integer() and (is_int_text(like) or isinstance(value, int)):
            return str(int(value))
        return format(float(value), ".10g")
    return str(value).strip()


def typed(text):
    """Kit text -> value for an Excel cell (a number if it is a number)"""
    if is_int_text(text):
        return int(text)
    if re.fullmatch(r"-?\d+(\.\d+)?([eE][-+]?\d+)?", str(text).strip()):
        return float(text)
    return text


def field_label(kit, path, name=""):
    """Readable column name: known fields from kit.fields + the key's label (rarity, attribute…)"""
    fields = kit.get("fields", {})
    # offset is a min/max wrapper, not a field: the attribute above it matters
    parts = [p for p in re.sub(r"\[[^\]]*\]", "", path).split(".") if p and p != "offset"]
    words = [fields.get(p, p) for p in parts[-2:]]
    if len(words) > 1 and words[-1] == "значение":
        words = words[:-1]
    label = " ".join(dict.fromkeys(words))
    if name:
        label += f" · {name}"
    idx = re.findall(r"\[([^\]]*)\]", path)
    return f"{label} ({','.join(idx)})" if idx else label


def load_en(root, table):
    """English labels (data/<table>.en.csv, if the kit was built with names/strings_en.tsv):
    ({row: title}, {(row, path): name})"""
    p = root / "data" / f"{table}.en.csv"
    titles, names = {}, {}
    if p.exists():
        with open(p, encoding="utf-8-sig", newline="") as f:
            for r in csv.DictReader(f):
                row = int(r["row"])
                titles.setdefault(row, r["title"])
                if r["path"]:
                    names[(row, r["path"])] = r["name"]
    return titles, names


def en_label(kit, path):
    """English name of the attribute in the path (kit.fields_en), otherwise empty"""
    fe = kit.get("fields_en", {})
    for seg in reversed(re.sub(r"\[[^\]]*\]", "", path).split(".")):
        if seg in fe:
            return fe[seg]
    return ""


def both(ru, en):
    """Label "<ru> / <en>" — if an English label exists and differs"""
    return f"{ru} / {en}" if en and en != ru else ru


def unit_of(kit, path, row_attr=None):
    """Value unit: "share" (0.05 = 5 %) / "points" — by the attribute in the path or by the row's attribute_type"""
    units = kit.get("attr_units", {})
    for seg in re.sub(r"\[[^\]]*\]", "", path).split("."):
        if seg in units:
            return units[seg]
    if row_attr not in (None, ""):
        name = kit.get("attr_ids", {}).get(str(row_attr))
        if name:
            return units.get(name, "")
    return ""


def generic(path):
    """Path without indices: data.affix_data[0].rarity_range[3].max_value -> data.affix_data[*].rarity_range[*].max_value"""
    return re.sub(r"\[\d+\]", "[*]", path)


def ranges(values):
    """generic path -> (min, max) over the kit's numeric values (the original's range)"""
    out = {}
    for v in values:
        n = as_number(v["value"]) if v["path"] else None
        if n is None:
            continue
        g = generic(v["path"])
        lo, hi = out.get(g, (n, n))
        out[g] = (min(lo, n), max(hi, n))
    return out


def fmt_range(r):
    if not r:
        return ""
    lo, hi = (format(x, ".6g") for x in r)
    return lo if lo == hi else f"{lo}…{hi}"
