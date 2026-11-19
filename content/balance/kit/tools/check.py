"""Checks a patch (from make_patch.py or from an LLM) against the kit — the same the server will do, before sending.

    python tools/check.py patch.json [--kit folder] [--json]

Errors (the patch will not be accepted): wrong table/row/field, the field may not be changed, "old" does not match the kit,
wrong type, min greater than max, one value changed twice.
Warnings: a change of more than 3 times, sign flip, far outside the original's range, the range is not a multiple of the step.

The same rules and codes are in crates/balance-kit/src/check.rs; shared examples — crates/balance-kit/tests/fixtures/cases.json.
"""
import argparse
import json
import sys

import kitlib


def text(v):
    """Value as text: null — empty, numbers as in JSON"""
    if v is None:
        return ""
    return v if isinstance(v, str) else json.dumps(v, ensure_ascii=False)


def unit_paths(base):
    """Where to look for the roll step of a min/max pair: the nearest unit up the path (…offset.unit, data.unit)"""
    out = [base + "unit"]
    prefix = base.rstrip(".")
    while "." in prefix:
        prefix = prefix.rsplit(".", 1)[0]
        out.append(prefix + ".unit")
    if base:
        out.append("unit")
    return out


def check(root, p):
    """-> (errors, warnings): lists of {"code", "at", "message"}; at — index in changes (from 0) or None"""
    manifest, kit = kitlib.load_kit(root)
    tables = kitlib.tables_of(kit)
    errors, warns = [], []

    def err(code, at, msg):
        errors.append({"code": code, "at": at, "message": msg})

    def warn(code, at, msg):
        warns.append({"code": code, "at": at, "message": msg})

    if p.get("format") != kitlib.PATCH_FORMAT:
        err("format", None, f"format must be {kitlib.PATCH_FORMAT}")
    if p.get("base") != manifest["version"]:
        warn("base_differs", None, f"patch from kit {p.get('base')}, kit is {manifest['version']}")
    if "season" not in p:
        warn("season_missing", None, f"no season — the server will apply the patch to the active season (the kit's: {manifest.get('season')})")
    elif p.get("season") != manifest.get("season"):
        err("season_mismatch", None, f"season {p.get('season')!r}, but the kit is for {manifest.get('season')!r} — the patch is for another season")
    if not str(p.get("author") or "").strip():
        warn("author_missing", None, "no author")

    changes = p.get("changes", [])
    cache = {}
    seen = set()
    for i, c in enumerate(changes):
        row = c.get("row") if isinstance(c.get("row"), int) and not isinstance(c.get("row"), bool) else None
        where = f"#{i + 1} {c.get('table')} row {c.get('row')} {c.get('path')}"
        if (c.get("table"), row, c.get("path")) in seen:
            err("duplicate_change", i, f"{where}: this value is changed a second time in the patch")
            continue
        seen.add((c.get("table"), row, c.get("path")))
        t = tables.get(c.get("table"))
        if t is None:
            err("table_unknown", i, f"{where}: the table is not in the kit")
            continue
        if not kitlib.editable(t, text(c.get("path"))):
            err("field_locked", i, f"{where}: this field may not be changed (allowed: {', '.join(t.get('editable', []))})")
            continue
        if c["table"] not in cache:
            vals = kitlib.load_values(root, c["table"])
            orig = kitlib.load_orig(root, c["table"])  # range from the game's original, if the kit has it
            cache[c["table"]] = (vals, kitlib.ranges(orig if orig is not None else vals))
        values, rng = cache[c["table"]]
        row_vals = [v for v in values if v["row"] == row]
        if not row_vals:
            err("row_missing", i, f"{where}: no row {c.get('row')}")
            continue
        key = text(c.get("key"))
        if key.strip() and not kitlib.same(row_vals[0]["key"], key):
            err("key_mismatch", i, f"{where}: key {key}, but the row has {row_vals[0]['key']}")
            continue
        path = c["path"]
        cur = next((v["value"] for v in row_vals if v["path"] == path), "")
        old, new = text(c.get("old")), text(c.get("new"))
        if not kitlib.same(cur, old):
            err("old_mismatch", i, f"{where}: old {old or '(empty)'}, but the kit has {cur or '(empty)'}")
            continue
        if new.strip() == "":
            err("new_empty", i, f"{where}: empty new")
            continue
        o, n = kitlib.as_number(cur), kitlib.as_number(new)
        kind = kitlib.field_kind(root, c["table"], path)
        if kind is not None:
            number_expected = kind in kitlib.NUMBER_KINDS
        else:
            number_expected = o is not None or cur == ""
        if number_expected:
            if n is None:
                err("new_not_number", i, f"{where}: new {new} is not a number")
                continue
            if kind is not None:
                integer = kind in kitlib.INT_RANGES
            else:
                raw = kitlib.raw_at(root, c["table"], row, path)
                integer = isinstance(raw, int) and not isinstance(raw, bool)
            if integer and not kitlib.is_int_text(new):
                err("new_not_integer", i, f"{where}: the field is an integer, but new is {new}")
                continue
            if kind in kitlib.INT_RANGES:
                lo, hi = kitlib.INT_RANGES[kind]
                if not lo <= int(new.strip()) <= hi:
                    err("new_out_of_range", i, f"{where}: new {new} is outside the type's bounds {lo}…{hi}")
                    continue
        limit = next((v for k, v in (t.get("limits") or {}).items() if kitlib.wildcard(k, path)), None)
        if limit is not None and n is not None:
            lo = limit[0] if len(limit) > 0 else None
            hi = limit[1] if len(limit) > 1 else None
            if (lo is not None and n < lo) or (hi is not None and n > hi):
                can = f"{lo:g}…{hi:g}" if lo is not None and hi is not None else f"from {lo:g}" if lo is not None else f"up to {hi:g}"
                err("new_out_of_limits", i, f"{where}: new {new} — allowed {can}")
                continue
        r = rng.get(kitlib.generic(path))
        if r and n is not None:
            span = max(abs(r[0]), abs(r[1]))
            if span > 0 and abs(n) > 3 * span:
                warn("range_far", i, f"{where}: {new} — far outside the original's range {kitlib.fmt_range(r)}")
        if o not in (None, 0) and n is not None:
            if abs(n / o) > 3 or abs(n / o) < 1 / 3:
                warn("ratio_big", i, f"{where}: {cur} -> {new} — more than 3 times")
            if (n < 0) != (o < 0):
                warn("sign_flip", i, f"{where}: sign flip {cur} -> {new}")

    # min/max pairs: the result after the patch, taking other edits of the same patch into account
    planned = {}
    for i, c in enumerate(changes):
        if isinstance(c.get("row"), int) and not isinstance(c.get("row"), bool):
            planned[(c.get("table"), c["row"], c.get("path"))] = (text(c.get("new")), i)
    # rows with one key: after the patch at least one of them has a value above zero
    done_keys = set()
    for (table, row, path), (_, i) in sorted(planned.items(), key=lambda kv: tuple(str(x) for x in kv[0])):
        if table not in cache or tables[table].get("positive_in_key") != path:
            continue
        values = cache[table][0]
        key = next((v["key"] for v in values if v["row"] == row), None)
        if key is None or (table, key) in done_keys:
            continue
        done_keys.add((table, key))
        rows = sorted({v["row"] for v in values if v["key"] == key})
        current = {v["row"]: v["value"] for v in values if v["key"] == key and v["path"] == path}
        after = [planned[(table, r_, path)][0] if (table, r_, path) in planned else current.get(r_, "") for r_ in rows]
        if not any((kitlib.as_number(x) or 0) > 0 for x in after):
            err("key_all_zero", i, f"{table} {key}: {path} is zero in all rows, but at least one must be above zero")

    done = set()
    for (table, row, path), (_, i) in sorted(planned.items(), key=lambda kv: tuple(str(x) for x in kv[0])):
        if table not in cache or not isinstance(path, str):
            continue
        if path.endswith("min_value"):
            base = path[: -len("min_value")]
        elif path.endswith("max_value"):
            base = path[: -len("max_value")]
        else:
            continue
        if (table, row, base) in done:
            continue
        done.add((table, row, base))
        cur = {v["path"]: v["value"] for v in cache[table][0] if v["row"] == row}

        def after(p_):
            return kitlib.as_number(planned[(table, row, p_)][0] if (table, row, p_) in planned else cur.get(p_, ""))

        lo, hi = after(base + "min_value"), after(base + "max_value")
        if lo is None or hi is None:
            continue
        label = f"{table} row {row} {base.rstrip('.')}"
        if lo > hi:
            err("min_gt_max", i, f"{label}: min {lo:g} is greater than max {hi:g}")
            continue
        unit = next((u for u in (kitlib.as_number(cur.get(x, "")) for x in unit_paths(base)) if u is not None), None)
        if unit is None:
            continue
        if unit <= 0:
            warn("unit_invalid", i, f"{label}: step {unit:g} is not above zero")
        else:
            steps = (hi - lo) / unit
            if abs(steps - round(steps)) > 1e-6:
                warn("unit_not_multiple", i, f"{label}: {lo:g}…{hi:g} is not a multiple of step {unit:g}, a roll may miss the bounds")
    return errors, warns


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("patch")
    ap.add_argument("--kit")
    ap.add_argument("--json", action="store_true", help="result as one JSON object")
    a = ap.parse_args()
    root = kitlib.kit_root(a.kit)
    try:
        p = json.load(open(a.patch, encoding="utf-8-sig"))
    except Exception as e:
        sys.exit(f"cannot be read as JSON: {e}")

    errors, warns = check(root, p)
    if a.json:
        print(json.dumps({"errors": errors, "warnings": warns}, ensure_ascii=False))
        sys.exit(1 if errors else 0)
    for w in warns:
        print("! warning:", w["message"])
    for e in errors:
        print("x ERROR:", e["message"])
    n = len(p.get("changes", []))
    if errors:
        print(f"the patch will NOT pass: {len(errors)} errors in {n} changes")
        sys.exit(1)
    print(f"ok: {n} changes, {len(warns)} warnings")


if __name__ == "__main__":
    main()
