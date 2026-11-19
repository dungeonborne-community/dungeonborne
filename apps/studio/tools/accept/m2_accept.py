"""Acceptance of section 9 of the spec:
   python m2_accept.py <kit dir> <edits.json> <app patch> <scratch>
1. the app's patch passes check.py and `balance patch --dry-run`;
2. the same edits through the book and make_patch.py give the same changes;
3. on a COPY of the project: apply, `balance build`, and the JSON tables hold exactly the expected values."""
import json, os, shutil, subprocess, sys
from pathlib import Path
from openpyxl import load_workbook

kit, edits_file, app_patch, scratch = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3]), Path(sys.argv[4])
# workspace root (docs/WORKSPACE_PLAN.md); xtask comes from the shared target (CARGO_TARGET_DIR) or core/backend/target
ws = Path(__file__).resolve().parents[4]
target = Path(os.environ.get("CARGO_TARGET_DIR") or ws / "core" / "backend" / "target")
xtask = target / "release" / "xtask.exe"
env = dict(os.environ, PYTHONUTF8="1")
edits = json.load(open(edits_file, encoding="utf-8"))
sys.path.insert(0, str(kit / "tools"))
import kitlib

fails = 0


def report(name, ok, detail=""):
    global fails
    fails += not ok
    print(f"{'ok ' if ok else 'BAD'} {name}" + (f": {detail}" if detail else ""))


def run(args, **kw):
    return subprocess.run(args, capture_output=True, text=True, encoding="utf-8", env=kw.pop("env", env), **kw)


patch = json.load(open(app_patch, encoding="utf-8"))
print("patch", patch["base"], patch["season"], patch["author"], patch["comment"], patch["created"], patch["app"], len(patch["changes"]), "changes")

# 1. check.py and the server's dry run
r = run([sys.executable, str(kit / "tools" / "check.py"), str(app_patch), "--kit", str(kit), "--json"])
res = json.loads(r.stdout)
report("check.py accepts the app's patch", r.returncode == 0 and not res["errors"], str(res["errors"][:3]))
print("   warnings:", sorted(x["code"] for x in res["warnings"]))
r = run([str(xtask), "balance", "patch", str(app_patch), "--dry-run"], cwd=str(ws))
report("balance patch --dry-run accepts it", r.returncode == 0, (r.stderr or r.stdout)[-400:])
tables = {c["table"] for c in patch["changes"]}
# The loot template has its own acceptance: s2_loot_accept.py
wanted_tables = {x["name"] for x in json.load(open(kit / "kit.json", encoding="utf-8"))["table"] if x.get("editable") and not x.get("source")}
report(f"all {len(wanted_tables)} editable tables are covered", tables == wanted_tables, str(sorted(wanted_tables - tables)))

# 2. the same edits in the book
book = kit / "balance.xlsx"
print("   loading the book…")
wb = load_workbook(book)
meta = wb[kitlib.META_SHEET]
sheets = {row[1]: (row[0], row[2]) for row in list(meta.iter_rows(values_only=True))[2:] if row and row[0]}
for e in edits:
    sheet, layout = sheets[e["table"]]
    ws = wb[sheet]
    paths = [c.value for c in ws[2]]
    done = False
    value = kitlib.typed(e["new"])
    if layout == "long":
        for row in ws.iter_rows(min_row=3):
            if row[0].value == e["row"] and row[4].value == e["path"]:
                row[6].value = value
                done = True
                break
    else:
        col = paths.index(e["path"])
        for row in ws.iter_rows(min_row=3):
            if row[0].value == e["row"]:
                row[col].value = value
                done = True
                break
    report(f"found in the book: {e['table']} {e['row']} {e['path'][:50]}", done)
edited = scratch / "m2_book.xlsx"
wb.save(edited)
excel_patch = scratch / "m2_excel_patch.json"
r = run([sys.executable, str(kit / "tools" / "make_patch.py"), str(edited), "--kit", str(kit), "--author", "m2", "--comment", "acceptance", "-o", str(excel_patch)])
report("make_patch.py ran", r.returncode == 0 and excel_patch.exists(), r.stderr[-300:])
excel = json.load(open(excel_patch, encoding="utf-8"))


def norm(p):
    # title is a display name: the book adds the other language and the unit to it, the server doesn't read it
    return sorted((c["table"], c["row"], str(c["key"]), c["path"], str(c["old"]), str(c["new"])) for c in p["changes"])


a, b = norm(patch), norm(excel)
report("same changes as through Excel", a == b, f"only app {set(a) - set(b)}; only excel {set(b) - set(a)}" if a != b else f"{len(a)} changes")
for k in ("format", "base", "season", "author", "comment"):
    report(f"same header: {k}", patch[k] == excel[k], f"{patch[k]!r} / {excel[k]!r}")
print("   titles:", [(c["title"][:24], d["title"][:24]) for c, d in zip(sorted(patch["changes"], key=lambda x: (x["table"], x["row"], x["path"])), sorted(excel["changes"], key=lambda x: (x["table"], x["row"], x["path"])))][:3])

# 3. apply and build on a copy
copy = target / "m2-accept"
if copy.exists():
    shutil.rmtree(copy)
copy.mkdir(parents=True)
for rel in ["content/balance/tables", "content/seasons", "content/names", "content/loot", "content/balance/kit/kit.toml", "core/backend/Configure.pb", "core/backend/assets/excel/ExcelConfig", "config/local.toml"]:
    src, dst = ws / rel, copy / rel
    if src.is_dir():
        shutil.copytree(src, dst, ignore=shutil.ignore_patterns("kit", "pak", "server", "balance_csv"))
    else:
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(src, dst)
(copy / "core/backend/ExcelConfig").mkdir(parents=True)
for f in (ws / "core/backend/ExcelConfig").iterdir():
    if f.is_file():
        shutil.copy(f, copy / "core/backend/ExcelConfig" / f.name)
root_env = dict(env, XTASK_ROOT=str(copy))
season_flag = ["--season=" + (patch["season"] or "none")]
r = run([str(xtask), "balance", "patch", str(app_patch)], env=root_env)
report("balance patch applies it", r.returncode == 0, (r.stderr or r.stdout)[-500:])
print("  ", "\n   ".join(r.stdout.splitlines()[-9:]))


def table_path(table):
    if patch["season"]:
        own = copy / "content/seasons" / patch["season"] / "balance" / f"{table}.json"
        if own.exists():
            return own
    return copy / "content/balance/tables" / f"{table}.json"


for e in edits:
    v = kitlib.raw_at.__wrapped__(None) if False else None
    rows = json.load(open(table_path(e["table"]), encoding="utf-8-sig"))["rows"]
    cur = rows[e["row"]]
    for tok in __import__("re").findall(r"[^.\[\]]+|\[[^\]]*\]", e["path"]):
        if tok.startswith("["):
            k = tok[1:-1]
            cur = cur[int(k)] if isinstance(cur, list) else cur.get(k)
        else:
            cur = cur.get(tok)
    report(f"value in JSON: {e['table']} {e['row']} {e['path'][:44]}", kitlib.same(str(cur), e["new"]), f"{cur} want {e['new']}")
r = run([str(xtask), "balance", "build"] + season_flag, env=root_env)
report("balance build builds", r.returncode == 0 and "ERRORS" not in r.stdout, r.stdout[-400:])
print("  ", "\n   ".join(l for l in r.stdout.splitlines() if "pak" in l or "change" in l)[:400])
print("FAILS", fails)
