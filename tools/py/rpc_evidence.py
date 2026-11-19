"""All facts about an RPC method in one output (for implementing handlers):
  python -I tools/rpc_evidence.py <method> [method ...] [--no-sniff] [--max N]
Run from the backups root. Method — the exact RPC name (mail_get_all) or a prefix ending in '_' (mail_).
Sections: handler template, proto type names, our stub, reference rpc_dump dumps (PVE and OUR),
official server sniffer (via `cargo run --example sniff_decode`), ExcelConfigjson tables,
mentions in UE dumps (CXXHeaderDump/UHTHeaderDump). Full protobuf structs: make_llm_prompt.py <method> --stdout.
"""
import sys as _sys, pathlib as _pl; _sys.path.insert(0, str(_pl.Path(__file__).resolve().parent))
from wspaths import ROOT, W  # noqa: E402  workspace paths (docs/WORKSPACE_PLAN.md)
import glob
import json
import os
import re
import subprocess
import sys

sys.stdout.reconfigure(encoding="utf-8")
args = [a for a in sys.argv[1:] if not a.startswith("--")]
NO_SNIFF = "--no-sniff" in sys.argv
MAX = int(sys.argv[sys.argv.index("--max") + 1]) if "--max" in sys.argv else 4000
if "--max" in sys.argv:
    args.remove(sys.argv[sys.argv.index("--max") + 1])
if not args:
    sys.exit(__doc__)

tpl = open("handlers_template.rs.txt", encoding="utf-8").read()
blocks = re.findall(r"(    /\*.*?\*/\s*async fn handle_(\w+).*?\n    \})", tpl, re.S)
route = open(str(W("src/generated/generated_server_code.rs")), encoding="utf-8").read()


def methods_for(key):
    names = [n for _, n in blocks]
    return [n for n in names if (n.startswith(key) if key.endswith("_") else n == key)]


def camel(name):
    return "".join(p.capitalize() for p in name.split("_"))


def cut(s, n=MAX):
    return s if len(s) <= n else s[:n] + f"\n... (truncated, {len(s)} characters total)"


def section(title):
    print(f"\n## {title}\n")


targets = [m for k in args for m in methods_for(k)] or args
for m in targets:
    print(f"\n# ===== {m} =====")

    section("Template (handlers_template.rs.txt)")
    print(next((b for b, n in blocks if n == m), "not in the template"))

    section("Proto names (route_rpc: response type as type_url)")
    r = re.search(rf'"{m}" => \{{\s*if let Ok\(req\) = <([\w:]+) as.*?return Some\(\("([\w.]+)"', route, re.S)
    print(f"req: {r.group(1)}\nrsp type_url: {r.group(2)}" if r else "not found")

    section("Our implementation / stub")
    for f in glob.glob(str(W("src/rpc/handlers.rs"))) + glob.glob(str(W("src/rpc/handlers/*.rs"))):
        for i, line in enumerate(open(f, encoding="utf-8"), 1):
            if re.search(rf"fn (handle_)?{m}(_impl)?\(", line):
                print(f"{f}:{i}: {line.strip()}")

    section("rpc_dump (PVE SoloLobby reference and our server)")
    seen = set()
    for d in sorted(glob.glob(str(W("rpc_dump/*/"))), key=lambda p: (not os.path.basename(p.rstrip("/\\")).startswith("PVE"), p)):
        for f in sorted(glob.glob(os.path.join(d, f"*_{m}.json"))):
            try:
                j = json.load(open(f, encoding="utf-8"))
            except Exception as e:
                print(f"{f}: {e}")
                continue
            body = json.dumps(j.get("body"), ensure_ascii=False, indent=1)
            key = (os.path.basename(d.rstrip("/\\"))[:3], j.get("dir"), body)
            if key in seen:
                continue
            seen.add(key)
            print(f"--- {f} [{j.get('dir')}] {j.get('type')}\n{cut(body)}")
    if not seen:
        print("no dumps")

    if not NO_SNIFF:
        section("Official server sniffer (sniff_decode)")
        p = subprocess.run(["cargo", "run", "--quiet", "--example", "sniff_decode", "--", f".{m}"],
                           capture_output=True, text=True, encoding="utf-8", errors="replace")
        out = p.stdout.strip()
        # identical bodies are shown once
        uniq, seen_b = [], set()
        for chunk in out.split("\n### "):
            head, _, body = chunk.partition("\n")
            k = (head.split("] ")[0][-3:], body)
            if body and k not in seen_b:
                seen_b.add(k)
                uniq.append("### " + chunk.lstrip("# "))
        print(cut("\n".join(uniq)) if uniq else "not in the sniffer" + (f" ({p.stderr.strip()[:300]})" if p.returncode else ""))

    section("ExcelConfigjson tables (by the words of the method)")
    words = [w for w in m.split("_") if len(w) > 3 and w not in ("receive", "update", "change", "get", "send", "sync")]
    tables = sorted({os.path.basename(f) for w in words for f in glob.glob(str(W(f"ExcelConfigjson/*{w}*.json")))})
    print(", ".join(tables) or "none")

    section("UE dumps (CXXHeaderDump/UHTHeaderDump): mentions")
    pat = re.compile(camel(m), re.I)
    hits = 0
    for f in glob.glob(str(W("ue4ss/CXXHeaderDump/*.hpp"))) + glob.glob(str(W("ue4ss/UHTHeaderDump/**/*.h")), recursive=True):
        try:
            for i, line in enumerate(open(f, encoding="utf-8", errors="replace"), 1):
                if pat.search(line):
                    if hits < 25:
                        print(f"{f}:{i}: {line.strip()[:200]}")
                    hits += 1
        except OSError:
            pass
    print(f"({hits} total)" if hits else "none")
