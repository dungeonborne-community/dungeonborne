"""Stage 2 acceptance on a COPY of the project (nothing outside target\\ changes):
   python s2_loot_accept.py <base kit dir> <season kit dir>
1. `loot build` of the untouched template gives the same tables, and the app's calculation matches the build;
2. a patch on the loot tables passes check.py and the dry run without writing anything;
3. applied: only numbers change in the template, comments stay, only loot tables are rebuilt;
4. tables differ by map as the patch says, slot rates are what the patch asked for;
5. the kit built after it shows the new values; bad patches are refused and write nothing."""
import csv
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path

# workspace root (docs/WORKSPACE_PLAN.md); xtask comes from the shared target (CARGO_TARGET_DIR) or core/backend/target
ws = Path(__file__).resolve().parents[4]
target = Path(os.environ.get("CARGO_TARGET_DIR") or ws / "core" / "backend" / "target")
xtask = target / "release" / "xtask.exe"
copy = target / "s2-accept"
env = dict(os.environ, PYTHONUTF8="1", XTASK_ROOT=str(copy))
fails = 0


def report(name, ok, detail=""):
    global fails
    fails += not ok
    print(f"{'ok ' if ok else 'BAD'} {name}" + (f": {detail}" if detail else ""))


def run(*args):
    return subprocess.run([str(xtask), *args], capture_output=True, text=True, encoding="utf-8", env=env)


def values(kit, table):
    with open(kit / "data" / f"{table}.csv", encoding="utf-8-sig", newline="") as f:
        return list(csv.DictReader(f))


def tree_hash(root):
    return {str(p.relative_to(root)): hashlib.md5(p.read_bytes()).hexdigest() for p in sorted(root.rglob("*.json")) if "kit" not in p.relative_to(root).parts}


def fresh_copy():
    if copy.exists():
        shutil.rmtree(copy)
    copy.mkdir(parents=True)
    for rel in ["content/balance/tables", "content/seasons", "content/names", "content/loot", "content/balance/kit", "core/backend/Configure.pb", "core/backend/assets/excel/ExcelConfig", "config/local.toml"]:
        src, dst = ws / rel, copy / rel
        if src.is_dir():
            shutil.copytree(src, dst, ignore=shutil.ignore_patterns("kit", "out", "pak", "server", "balance_csv", "__pycache__"))
        else:
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(src, dst)


def change(rows, row, path, new):
    v = next(x for x in rows if int(x["row"]) == row and x["path"] == path)
    return {"table": None, "row": row, "key": v["key"], "title": v["title"], "path": path, "old": v["value"], "new": str(new)}


def table_rows(kit, season, name):
    own = copy / "content/seasons" / (season or "-") / "balance" / f"{name}.json"
    path = own if own.exists() else copy / "content/balance/tables" / f"{name}.json"
    return json.load(open(path, encoding="utf-8-sig"))["rows"]


def write_patch(path, kit, season, changes):
    manifest = json.load(open(kit / "manifest.json", encoding="utf-8"))
    path.write_text(json.dumps({"format": "dbbalance-patch/1", "base": manifest["version"], "season": season, "author": "s2", "comment": "loot", "created": "2026-11-06", "app": "s2", "changes": changes}, ensure_ascii=False), encoding="utf-8")


def book_route(kit, patch, changes, work):
    """The same edits typed into the book give the same patch"""
    from openpyxl import load_workbook

    sys.path.insert(0, str(kit / "tools"))
    import kitlib

    wb = load_workbook(kit / "balance.xlsx")
    meta = wb[kitlib.META_SHEET]
    sheets = {row[1]: row[0] for row in list(meta.iter_rows(values_only=True))[2:] if row and row[0]}
    missing = []
    for c in changes:
        ws = wb[sheets[c["table"]]]
        paths = [x.value for x in ws[2]]
        if c["path"] not in paths:
            missing.append(c["path"])
            continue
        col = paths.index(c["path"])
        for row in ws.iter_rows(min_row=3):
            if row[0].value == c["row"]:
                row[col].value = kitlib.typed(c["new"])
                break
    report("every change has a cell in the book", not missing, str(missing))
    edited = work / "book.xlsx"
    wb.save(edited)
    out = work / "book_patch.json"
    r = subprocess.run([sys.executable, str(kit / "tools" / "make_patch.py"), str(edited), "--kit", str(kit), "--author", "s2", "--comment", "loot", "-o", str(out)], capture_output=True, text=True, encoding="utf-8", env=env)
    report("make_patch.py ran", r.returncode == 0 and out.exists(), r.stderr[-300:])
    norm = lambda p: sorted((c["table"], c["row"], str(c["key"]), c["path"], str(c["old"]), str(c["new"])) for c in p["changes"])
    a, b = norm(json.load(open(patch, encoding="utf-8"))), norm(json.load(open(out, encoding="utf-8")))
    report("the same changes through the book", a == b, f"only here {set(a) - set(b)}; only book {set(b) - set(a)}" if a != b else f"{len(a)} changes")


def stage(kit, season):
    print(f"--- {kit.name}")
    fresh_copy()
    flag = "--season=" + (season or "none")
    template_path = copy / "content/seasons" / season / "loot" / "loot.toml" if season else copy / "content/loot" / "loot.toml"
    work = copy / "work"
    work.mkdir()

    # 1. untouched
    before = tree_hash(copy / "content/balance/tables") | {"S/" + k: v for k, v in tree_hash(copy / "content/seasons").items()}
    r = run("loot", "verify-view", flag)
    report("the app's calculation matches the build", r.returncode == 0 and ": match" in r.stdout, r.stdout.strip().splitlines()[-1] if r.stdout else r.stderr)
    source_before = table_rows(kit, season, "loot_ue_source_choose_group")
    r = run("loot", "build", flag)
    after = tree_hash(copy / "content/balance/tables") | {"S/" + k: v for k, v in tree_hash(copy / "content/seasons").items()}
    changed = [k for k in after if before.get(k) != after[k]]
    # The season's table may lag behind the base on maps that keep the base's loot ([keep_base]): nothing else may differ
    keep = set(tomllib.load(open(template_path, "rb")).get("keep_base", {}).get("maps", []))
    source_after = table_rows(kit, season, "loot_ue_source_choose_group")
    moved = [x for x, y in zip(source_before, source_after) if x != y]
    outside = [x for x in moved if x.get("map_id") not in keep]
    only_source = all(Path(k).name == "loot_ue_source_choose_group.json" for k in changed)
    report("loot build of the untouched template changes nothing but [keep_base] maps", r.returncode == 0 and only_source and not outside and len(source_before) == len(source_after), f"{changed}, rows on keep_base maps: {len(moved)}, elsewhere: {len(outside)}")

    # 2. the patch
    doc = tomllib.load(open(template_path, "rb"))
    slots, items, rules = values(kit, "lt_slots"), values(kit, "lt_items"), values(kit, "lt_rules")
    first_row = {}
    for v in slots:
        first_row.setdefault(v["key"], int(v["row"]))
    only = lambda m: next(k for k, g in doc["groups"].items() if g.get("maps") == [m] and len(g.get("sources", [])) > 5)
    crypt, classic = only(10101), only(10102)
    changes = []
    rate_of = lambda g, n: float(doc["groups"][g]["slots"][n]["chance"])
    # the same place on two maps gets different values
    c1, c2 = round(rate_of(crypt, 0) * 0.5, 2), round(rate_of(classic, 0) * 0.25, 2)
    changes.append(dict(change(slots, first_row[crypt], "chance", c1), table="lt_slots"))
    changes.append(dict(change(slots, first_row[classic], "chance", c2), table="lt_slots"))
    # a slot that chooses between pools
    with_pr = next((int(v["row"]) for v in slots if v["path"] == "pr[0]"), None)
    if with_pr is not None:
        changes.append(dict(change(slots, with_pr, "pr[0]", 7), table="lt_slots"))
    # weight, a count in a list, a count that was a number, a count that was not there
    rows_by_key = {}
    seen = set()
    for v in items:
        r_ = int(v["row"])
        if r_ not in seen:
            seen.add(r_)
            rows_by_key.setdefault(v["key"], []).append(r_)
    def find_item(test):
        for k, p in doc["pools"].items():
            for n, it in enumerate(p["items"]):
                if test(it) and len(p["items"]) > 1 and k in rows_by_key and n < len(rows_by_key[k]):
                    return k, n, rows_by_key[k][n]
        return None
    listed = find_item(lambda it: isinstance(it.get("cnt"), list) and len(it["cnt"]) > 1)
    scalar = find_item(lambda it: isinstance(it.get("cnt"), int))
    missing = find_item(lambda it: "cnt" not in it and it.get("w", 0) > 0)
    expected_items = []
    if listed:
        k, n, row = listed
        new = int(doc["pools"][k]["items"][n]["cnt"][1]) + 1
        changes.append(dict(change(items, row, "cnt[1]", new), table="lt_items"))
        expected_items.append((k, n, "cnt", 1, new))
    if scalar:
        k, n, row = scalar
        new = int(doc["pools"][k]["items"][n]["cnt"]) + 1
        changes.append(dict(change(items, row, "cnt[0]", new), table="lt_items"))
        expected_items.append((k, n, "cnt", None, new))
    if missing:
        k, n, row = missing
        changes.append(dict(change(items, row, "w", 1234), table="lt_items"))
        changes.append(dict(change(items, row, "cnt[0]", 2), table="lt_items"))
        expected_items.append((k, n, "w", None, 1234))
        expected_items.append((k, n, "cnt", None, 2))
    # rules: base has none, the season has a mode
    changes.append(dict(change(rules, 0, "chance.gold", 1.5), table="lt_rules"))
    if season:
        mode_row = next(int(v["row"]) for v in rules if v["key"].startswith("modes."))
        changes.append(dict(change(rules, mode_row, "quality", 0.7), table="lt_rules"))
    patch = work / "patch.json"
    write_patch(patch, kit, season, changes)
    print("   changes:", len(changes))

    r = subprocess.run([sys.executable, str(kit / "tools" / "check.py"), str(patch), "--kit", str(kit), "--json"], capture_output=True, text=True, encoding="utf-8", env=env)
    res = json.loads(r.stdout)
    report("check.py accepts it", r.returncode == 0 and not res["errors"], str(res["errors"][:3]))
    if not season:
        book_route(kit, patch, changes, work)
    text_before = template_path.read_bytes()
    r = run("balance", "patch", str(patch), "--dry-run")
    report("the dry run accepts it", r.returncode == 0, (r.stderr or r.stdout)[-300:])
    report("the dry run writes nothing", template_path.read_bytes() == text_before and (tree_hash(copy / "content/balance/tables") | {"S/" + k: v for k, v in tree_hash(copy / "content/seasons").items()}) == after)
    r = run("loot", "verify-view", flag, f"--patch={patch}")
    report("edits in memory (app) and in the text (server) give the same drop", r.returncode == 0 and ": match" in r.stdout, (r.stdout or r.stderr)[-300:])

    # 3. applied
    before_all = tree_hash(copy / "content/balance/tables") | {"S/" + k: v for k, v in tree_hash(copy / "content/seasons").items()}
    r = run("balance", "patch", str(patch))
    report("balance patch applies it", r.returncode == 0, (r.stderr or r.stdout)[-400:])
    old_lines, new_lines = text_before.decode("utf-8").splitlines(), template_path.read_text(encoding="utf-8").splitlines()
    report("comments stay", [x for x in old_lines if x.lstrip().startswith("#")] == [x for x in new_lines if x.lstrip().startswith("#")])
    import difflib

    delta = [x for x in difflib.unified_diff(old_lines, new_lines, lineterm="", n=0) if x[:1] in "+-" and x[:3] not in ("+++", "---")]
    removed = [x for x in delta if x[0] == "-"]
    report("only the changed values differ in the template", len(removed) <= len(changes), f"{len(removed)} lines removed for {len(changes)} changes")
    print("   e.g.", delta[:2])
    after_all = tree_hash(copy / "content/balance/tables") | {"S/" + k: v for k, v in tree_hash(copy / "content/seasons").items()}
    touched = sorted(k for k in after_all if before_all.get(k) != after_all[k])
    allowed = {"loot.json", "loot_rate.json", "loot_choose_group_rate.json", "loot_ue_source_choose_group.json"}
    report("only loot tables were rebuilt", all(Path(k).name in allowed for k in touched), str(touched))
    if season:
        report("base tables stay as they were", all(k.startswith("S/") for k in touched), str(touched))
    new_doc = tomllib.load(open(template_path, "rb"))
    report("the template holds the new chances", float(new_doc["groups"][crypt]["slots"][0]["chance"]) == c1 and float(new_doc["groups"][classic]["slots"][0]["chance"]) == c2)
    bad = []
    for k, n, field, i, want in expected_items:
        got = new_doc["pools"][k]["items"][n].get(field)
        got = got[i] if i is not None else got
        if got != want:
            bad.append((k, n, field, got, want))
    report("the template holds the new items", not bad, str(bad))
    report("rules in the template", new_doc["rules"]["chance"]["gold"] == 1.5)

    # 4. tables by map
    src_rows = table_rows(kit, season, "loot_ue_source_choose_group")
    slot_rows = {r_["loot_id"]: r_ for r_ in table_rows(kit, season, "loot_choose_group_rate")}
    name = doc["groups"][crypt]["sources"][0]
    ids = {r_["name"]: r_["id"] for r_ in json.load(open(copy / "content/balance/tables" / "loot_source_map.json", encoding="utf-8-sig"))["rows"]}
    sid = ids[name]
    of_map = lambda m: next(r_ for r_ in src_rows if r_["ue_loot_source_id"] == sid and r_.get("map_id") == m)
    a, b = of_map(10101), of_map(10102)
    # the edited slot is the first one that survives the rules on its map
    rates = lambda row: [slot_rows[i]["default_rate"] for i in row["loot_ids"]]
    print(f"   source {name}: map 10101 rates {rates(a)[:4]}, map 10102 rates {rates(b)[:4]}")
    report("rows of two maps differ", rates(a) != rates(b))
    no_mode = not season
    if no_mode:
        # The patch also multiplies gold by 1.5, and these slots hold only gold
        gold = lambda g: all(it["id"] == 100069 for it in doc["pools"][str(doc["groups"][g]["slots"][0]["pool"])]["items"])
        report("slots hold gold, so the rule applies", gold(crypt) and gold(classic))
        report("slot rate on the first map is as patched", rates(a)[0] == round(c1 * 100 * 1.5), f"{rates(a)[0]} from {c1}")
        report("slot rate on the second map is as patched", rates(b)[0] == round(c2 * 100 * 1.5), f"{rates(b)[0]} from {c2}")
    r = run("loot", "verify-view", flag)
    report("the build still matches after the patch", r.returncode == 0 and ": match" in r.stdout, r.stdout.strip().splitlines()[-1] if r.stdout else r.stderr)

    # 5. the next kit, and bad patches
    out = work / "kit2"
    r = run("balance", "kit", flag, f"--out={out}", "--no-zip")
    report("the next kit is built", r.returncode == 0, r.stderr[-300:])
    if r.returncode == 0:
        again = values(out, "lt_slots")
        got = next(v["value"] for v in again if v["key"] == crypt and v["path"] == "chance")
        report("the next kit shows the new value", float(got) == c1, got)
        manifest = json.load(open(out / "manifest.json", encoding="utf-8"))
        old = json.load(open(kit / "manifest.json", encoding="utf-8"))
        report("the next kit has another version", manifest["version"] != old["version"], manifest["version"])

    current = template_path.read_bytes()
    now_slots, now_items = values(out, "lt_slots"), values(out, "lt_items")
    one_pool = next(k for k, rows in rows_by_key.items() if len(rows) == 1 and doc["pools"][k]["items"][0].get("w", 0) > 0)
    for label, ch, code in [
        ("chance above 100", dict(change(now_slots, first_row[crypt], "chance", 150), table="lt_slots"), "можно"),
        ("empty pool", dict(change(now_items, rows_by_key[one_pool][0], "w", 0), table="lt_items"), "у всех строк равно нулю"),
        ("wrong old value", dict(change(now_slots, first_row[crypt], "chance", 10), table="lt_slots", old="99.5"), "old"),
        ("a locked field", dict(change(now_items, rows_by_key[one_pool][0], "item_id", 1), table="lt_items"), "менять нельзя"),
    ]:
        bad_patch = work / "bad.json"
        write_patch(bad_patch, out, season, [ch])
        r = run("balance", "patch", str(bad_patch))
        report(f"refused: {label}", r.returncode != 0 and code in r.stderr, r.stderr[:160])
        report(f"nothing written: {label}", template_path.read_bytes() == current)


stage(Path(sys.argv[1]), None)
stage(Path(sys.argv[2]), "dead-money")
shutil.rmtree(copy, ignore_errors=True)
print("FAILS", fails)
sys.exit(1 if fails else 0)
