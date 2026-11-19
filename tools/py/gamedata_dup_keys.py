"""Which GameData tables lose rows: load_bytes_map puts rows into a HashMap by key, duplicate keys overwrite each other.
  python -I tools/gamedata_dup_keys.py      (from the backups root)
Compares the key from src/generated/generated_server_code.rs with the JSON copies ExcelConfigjson/<table>.json.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import json
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")
code = open(str(W("src/generated/generated_server_code.rs")), encoding="utf-8").read()
rows = re.findall(r'(\w+): load_bytes_map\(&format!\("\{\}/(\w+)\.bytes", dir\), \|r: &[\w:]+\| (.+?)\),\n', code)

bad = []
for field, table, key_expr in rows:
    path = str(W(f"ExcelConfigjson/{table}.json"))
    if not os.path.exists(path):
        print(f"?  {table}: no {path}")
        continue
    data = json.load(open(path, encoding="utf-8"))
    if isinstance(data, dict):
        data = next((v for v in data.values() if isinstance(v, list)), [])
    keys = [k.replace("r#", "") for k in re.findall(r"r\.((?:r#)?\w+)", key_expr)]
    seen = {}
    for row in data:
        k = tuple(json.dumps(row.get(name, 0), sort_keys=True) for name in keys)
        seen[k] = seen.get(k, 0) + 1
    lost = len(data) - len(seen)
    if lost:
        bad.append((lost, table, keys, len(data)))

for lost, table, keys, total in sorted(bad, reverse=True):
    print(f"{table}: key {keys} — {total} rows, {lost} lost")
print(f"\nchecked {len(rows)} tables, with losses: {len(bad)}")
