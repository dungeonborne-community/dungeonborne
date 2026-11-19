"""Merges client logs (ClientProbe/LuaProbe: probe/logs/*.log) with the server dump (rpc_dump/OUR.jsonl,
RPC_DUMP=1) into one timeline. Run from the backups root:
  python -I tools/probe_timeline.py                     latest session (by the most recent client log)
  python -I tools/probe_timeline.py --all               all logs
  python -I tools/probe_timeline.py --grep mail         only lines with the substring (case-insensitive)
  python -I tools/probe_timeline.py --kinds ERR,EVT,S2C line types (HDR WARN MARK EVT WIN CALL ERR SNAP LCALL LRET FIELD
                                                        TRACE; server C2S/S2C)
  python -I tools/probe_timeline.py --fields            report: which fields the client reads and which of them the server did not send
  python -I tools/probe_timeline.py --summary           counters by RPC, events, errors
  --out file                                            write to a file (default stdout)
  --width N                                             truncate lines to N characters (default 400, 0 — no truncation)
Client ↔ server session: steam_id from the HDR line of the ClientProbe log = who (open_id) in the server dump.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import collections
import datetime
import glob
import json
import os
import re
import sys

sys.stdout.reconfigure(encoding="utf-8")
args = sys.argv[1:]


def opt(name, default=None):
    if name in args:
        i = args.index(name)
        return args[i + 1] if i + 1 < len(args) else default
    return default


ALL = "--all" in args
GREP = (opt("--grep") or "").lower()
KINDS = set(filter(None, (opt("--kinds") or "").upper().split(",")))
WIDTH = int(opt("--width", "400"))
OUT = open(opt("--out"), "w", encoding="utf-8") if opt("--out") else sys.stdout

LOGS = sorted(glob.glob("probe/logs/*.log"), key=os.path.getmtime)
if not LOGS:
    sys.exit("no logs in probe/logs")

# session: logs started no earlier than 30 min before the most recent one
if not ALL:
    newest = os.path.getmtime(LOGS[-1])
    starts = {}
    for p in LOGS:
        with open(p, encoding="utf-8", errors="replace") as f:
            first = f.readline().split("\t", 1)[0]
        starts[p] = int(first) / 1000 if first.isdigit() else os.path.getmtime(p)
    newest_start = max(starts.values())
    LOGS = [p for p in LOGS if starts[p] >= newest_start - 1800]

rows = []  # (ts_ms, order, who, src, kind, text)
steam_of = {}  # client name -> steam_id
for p in LOGS:
    client = os.path.basename(p).split("_", 1)[0]
    with open(p, encoding="utf-8", errors="replace") as f:
        for n, line in enumerate(f):
            parts = line.rstrip("\n").split("\t", 4)
            if len(parts) < 5 or not parts[0].isdigit():
                continue
            ts, seq, src, kind, text = parts
            if kind == "HDR":
                m = re.search(r"steam_id=(\d+)", text)
                if m:
                    steam_of[client] = m.group(1)
            rows.append((int(ts), n, client, src, kind, text))

client_of = {v: k for k, v in steam_of.items()}
t_from = min(r[0] for r in rows) - 2000
t_to = max(r[0] for r in rows) + 2000

server = []
if os.path.exists(str(W("rpc_dump/OUR.jsonl"))):
    with open(str(W("rpc_dump/OUR.jsonl")), encoding="utf-8", errors="replace") as f:
        for n, line in enumerate(f):
            try:
                d = json.loads(line)
            except ValueError:
                continue
            ts = d.get("ts_ms", 0)
            if not (t_from <= ts <= t_to):
                continue
            who = client_of.get(d.get("who", ""), d.get("who") or "?")
            method = (d.get("method") or "").rsplit(".", 1)[-1] or "push"
            body = json.dumps(d.get("body"), ensure_ascii=False, separators=(",", ":"))
            server.append((ts, n, "srv:" + who, "srv", d.get("dir", "?"), f"{method} <{d.get('type', '?')}> {body}"))
rows += server
rows.sort(key=lambda r: (r[0], r[3] != "srv", r[1]))


def stamp(ms):
    return datetime.datetime.fromtimestamp(ms / 1000).strftime("%H:%M:%S.") + f"{ms % 1000:03d}"


def show(r):
    s = f"{stamp(r[0])}  {r[2]:<12} {r[3]:<5} {r[4]:<5} {r[5]}"
    return s if WIDTH <= 0 or len(s) <= WIDTH else s[:WIDTH] + "…"


def camel_to_snake(s):
    return re.sub(r"(?<=[a-z0-9])(?=[A-Z])", "_", s).lower()


if "--fields" in args:
    # keys the server sent in S2C (at any depth)
    sent = collections.Counter()

    def walk(v):
        if isinstance(v, dict):
            for k, x in v.items():
                sent[k] += 1
                walk(x)
        elif isinstance(v, list):
            for x in v:
                walk(x)

    with open(str(W("rpc_dump/OUR.jsonl")), encoding="utf-8", errors="replace") if os.path.exists(str(W("rpc_dump/OUR.jsonl"))) else open(os.devnull) as f:
        for line in f:
            try:
                d = json.loads(line)
            except ValueError:
                continue
            if d.get("dir") == "S2C":
                walk(d.get("body"))
    reads = collections.defaultdict(set)
    for r in rows:
        if r[4] == "FIELD":
            cls, _, field = r[5].partition(".")
            reads[cls].add(field)
    print(f"Fields read by the client (FIELD), {sum(map(len, reads.values()))} in {len(reads)} classes.", file=OUT)
    print("  ✗ — no such key in any S2C server response (rpc_dump/OUR.jsonl): a candidate for missing data\n", file=OUT)
    for cls in sorted(reads):
        fields = sorted(reads[cls])
        marks = []
        for fld in fields:
            snake = camel_to_snake(fld)
            # methods (IsValid, GetName…) and Lua service keys — not response fields
            is_method = not fld[:1].isupper() or re.match(r"^(Get|Set|Is|Has|Add|Remove|Clear|Num|Find|Contains)[A-Z]", fld)
            ok = bool(is_method) or snake in sent or fld in sent
            marks.append(("  " if ok else "✗ ") + fld + ("" if ok else f" ({snake})"))
        print(f"{cls}:", file=OUT)
        for m in marks:
            print("   " + m, file=OUT)
    sys.exit(0)

if "--summary" in args:
    c = collections.Counter()
    for r in rows:
        if r[3] == "srv":
            c[(r[4], r[5].split(" ", 1)[0])] += 1
        elif r[4] in ("EVT", "WIN"):
            c[(r[4], r[5].split(" ", 1)[0])] += 1
        elif r[4] in ("ERR", "CALL", "LCALL"):
            c[(r[4], r[5].split("(", 1)[0])] += 1
    print(f"logs: {', '.join(os.path.basename(p) for p in LOGS)}", file=OUT)
    print(f"clients: {steam_of}", file=OUT)
    for kind in ("C2S", "S2C", "ERR", "EVT", "WIN", "CALL", "LCALL"):
        items = sorted(((k[1], v) for k, v in c.items() if k[0] == kind), key=lambda x: -x[1])
        if items:
            print(f"\n== {kind} ({len(items)})", file=OUT)
            for name, v in items[:80]:
                print(f"  {v:6}  {name}", file=OUT)
    sys.exit(0)

for r in rows:
    if KINDS and r[4] not in KINDS:
        continue
    line = show(r)
    if GREP and GREP not in line.lower():
        continue
    print(line, file=OUT)
