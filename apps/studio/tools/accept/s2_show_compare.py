"""The app's average drop against `cargo xtask loot show` on every map:
   python s2_show_compare.py <drop.json from cdp_s2_loot.mjs> <season or none>
`loot show` prints two decimals, so the app's value must be within half of the last printed digit."""
import json
import os
import re
import subprocess
import sys
from pathlib import Path

# xtask of the workspace (docs/WORKSPACE_PLAN.md): the shared target (CARGO_TARGET_DIR) or core/backend/target
xtask = Path(os.environ.get("CARGO_TARGET_DIR") or Path(__file__).resolve().parents[4] / "core" / "backend" / "target") / "release" / "xtask.exe"
drop = json.load(open(sys.argv[1], encoding="utf-8"))
season = sys.argv[2]
line = re.compile(r"^\s+(\S+)\s.*\s(\d+) src\..*\|\s+([\d.]+) items: ?(.*)$")
gold = re.compile(r"≈(\d+) pcs")
fails = 0
groups = 0
for m in drop["maps"]:
    r = subprocess.run([str(xtask), "loot", "show", f"--season={season}", f"--map={m}"], capture_output=True, text=True, encoding="utf-8", env=dict(os.environ, PYTHONUTF8="1"))
    shown = {}
    for text in r.stdout.splitlines():
        x = line.match(text)
        if x:
            g = gold.search(x.group(4))
            shown[x.group(1)] = (int(x.group(2)), x.group(3), g.group(1) if g else None)
    ours = {k: (n, items, gold_) for k, items, gold_, n in drop["all"][str(m)]}
    if set(shown) != set(ours):
        print(f"BAD map {m}: groups differ: only show {sorted(set(shown) - set(ours))[:5]}, only app {sorted(set(ours) - set(shown))[:5]}")
        fails += 1
    for k in set(shown) & set(ours):
        groups += 1
        n, items, g = shown[k]
        if n != ours[k][0] or abs(float(items) - ours[k][1]) > 0.00501 or (g is not None and abs(float(g) - ours[k][2]) > 0.501):
            print(f"BAD map {m} {k}: show {shown[k]}, app {ours[k]}")
            fails += 1
print(f"compared {groups} group rows on {len(drop['maps'])} maps")
print("FAILS", fails)
sys.exit(1 if fails else 0)
