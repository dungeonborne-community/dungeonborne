"""Comparison of our RPCs with references by field set: which fields are present in reference responses (PVE SoloLobby, the official
server from the sniffer) and missing in ours — and vice versa. Run from the backups root:
  python -I tools/compare_flow.py [method ...] [--since HH:MM] [--dir S2C|C2S|both] [--ours rpc_dump/OUR.jsonl]
                                 [--pve rpc_dump/PVE_lobby_0630,rpc_dump/PVE_battle_0720] [--no-sniff] [--max N]
Without methods — all methods seen in our dump for the period. Fields — paths like mails[].attachments_detail[].item;
names are converted to snake_case (the sniffer decodes to camelCase). proto3 default values do not get into JSON,
so "no field" = on our side it is always 0/empty.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import collections
import datetime
import glob
import json
import os
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
args = sys.argv[1:]


def opt(name, default=None):
    if name in args:
        i = args.index(name)
        v = args[i + 1] if i + 1 < len(args) else default
        del args[i:i + 2]
        return v
    return default


def flag(name):
    if name in args:
        args.remove(name)
        return True
    return False


OURS = opt("--ours", str(W("rpc_dump/OUR.jsonl")))
PVE = opt("--pve", ",".join(str(W(x)) for x in (str(W("rpc_dump/PVE_lobby_0630")), "rpc_dump/PVE_battle_0720")))
SINCE = opt("--since")
DIRS = {"S2C", "C2S"} if (opt("--dir", "S2C") == "both") else {opt("--dir", "S2C") if False else "S2C"}
MAXN = int(opt("--max", "60"))
NO_SNIFF = flag("--no-sniff")
METHODS = set(args)


def snake(s):
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", s).lower()


def paths(v, prefix="", out=None):
    if out is None:
        out = set()
    if isinstance(v, dict):
        for k, x in v.items():
            k = snake(k) if not k.isdigit() else "#"
            p = f"{prefix}.{k}" if prefix else k
            out.add(p)
            paths(x, p, out)
    elif isinstance(v, list):
        for x in v:
            paths(x, prefix + "[]", out)
    return out


def short(method):
    return (method or "").rsplit(".", 1)[-1] or "push"


ours = collections.defaultdict(set)
ours_n = collections.Counter()
since_ms = None
if SINCE:
    h, m = map(int, SINCE.split(":"))
    since_ms = int(datetime.datetime.now().replace(hour=h, minute=m, second=0, microsecond=0).timestamp() * 1000)
with open(OURS, encoding="utf-8", errors="replace") as f:
    for line in f:
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if since_ms and d.get("ts_ms", 0) < since_ms:
            continue
        if d.get("dir") not in DIRS:
            continue
        m = short(d.get("method"))
        ours[m] |= paths(d.get("body"))
        ours_n[m] += 1

ref = collections.defaultdict(set)
ref_src = collections.defaultdict(set)
for folder in filter(None, PVE.split(",")):
    for p in glob.glob(os.path.join(folder, "*.json")):
        try:
            d = json.load(open(p, encoding="utf-8"))
        except (ValueError, OSError):
            continue
        if d.get("dir") not in DIRS:
            continue
        m = short(d.get("method"))
        ref[m] |= paths(d.get("body"))
        ref_src[m].add("PVE")

if not NO_SNIFF:
    p = subprocess.run(["cargo", "run", "--quiet", "--example", "sniff_decode", "--", ""],
                       capture_output=True, text=True, encoding="utf-8", errors="replace")
    for chunk in p.stdout.split("\n### ")[0:]:
        head, _, body = chunk.partition("\n")
        mm = re.search(r"\[(C2S|S2C)\] ([\w.]+)", head)
        if not mm or mm.group(1) not in DIRS:
            continue
        try:
            j = json.loads(body.strip() or "{}")
        except ValueError:
            continue
        m = short(mm.group(2))
        ref[m] |= paths(j)
        ref_src[m].add("official")

methods = sorted(METHODS or set(ours))
print(f"our dump: {OURS}{' since ' + SINCE if SINCE else ''}; references: {PVE}{'' if NO_SNIFF else ' + sniffer'}; direction {','.join(DIRS)}\n")
for m in methods:
    r, o = ref.get(m, set()), ours.get(m, set())
    if m not in ref_src:
        print(f"== {m}: ours {ours_n[m]}, no reference")
        continue
    if not r:
        print(f"== {m}: ours {ours_n[m]}, reference {'+'.join(sorted(ref_src[m]))} — empty response" + (f"; we have {len(o)} fields" if o else ""))
        continue
    missing = sorted(r - o)
    extra = sorted(o - r)
    print(f"== {m}: ours {ours_n[m]}, reference {'+'.join(sorted(ref_src[m]))}; missing on our side {len(missing)}, extra {len(extra)}")
    for x in missing[:MAXN]:
        print(f"   - {x}")
    if len(missing) > MAXN:
        print(f"   … {len(missing) - MAXN} more")
    for x in extra[:15]:
        print(f"   + {x}")
