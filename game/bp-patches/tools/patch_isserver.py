"""Patch "listen server as dedicated" (03.11): import KismetSystemLibrary.IsDedicatedServer -> IsServer.
Same signature (WorldContextObject -> bool), the bytecode does not change — EX_CallMath refers to the same import index.
The headless listen server answers IsServer = true and takes the dedicated branches (DK E hook without the "predicted" copy,
mob bullet tracking, no projectile launch sound). A client in a match: IsServer = false, like IsDedicatedServer.
Standalone (lobby, localbattle) — IsServer = true: the pak is for the server install only.
py -3 -I patch_isserver.py <in.json> <out.json>"""
import json, sys

src, dst = sys.argv[1:3]
d = json.load(open(src, encoding="utf-8-sig"))
names = d["NameMap"]
if "IsServer" not in names:
    names.append("IsServer")
ksl = [-(i + 1) for i, imp in enumerate(d["Imports"]) if imp["ObjectName"] == "KismetSystemLibrary"]
hits = [imp for imp in d["Imports"] if imp["ObjectName"] == "IsDedicatedServer" and imp["OuterIndex"] in ksl]
if len(hits) != 1:
    sys.exit(f"expected 1 import of KismetSystemLibrary.IsDedicatedServer, found {len(hits)}")
if any(imp["ObjectName"] == "IsServer" and imp["OuterIndex"] in ksl for imp in d["Imports"]):
    sys.exit("the IsServer import already exists — a bytecode edit is needed, not a rename")
hits[0]["ObjectName"] = "IsServer"
json.dump(d, open(dst, "w", encoding="utf-8"), indent=2)
print(f"ok: import {d['Imports'].index(hits[0]) + 1} IsDedicatedServer -> IsServer")
