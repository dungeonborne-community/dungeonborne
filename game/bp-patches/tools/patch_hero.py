"""BP_Tgf_Hero_Default patch (R14): in UserConstructionScript, before Return, we append
«if not HasAuthority(): AddComponentByClass(TgfEquipmentManagerComponent, false, Identity, false)».
The native equipment manager (GameFeature, server only) reaches the client later than the body timer (~2 s), and the body
crashes on GetEquipBySlot(null) (0x142f602c6). The client gets its own empty manager in advance — the same stub that
UE4SS used to add (r14), but without UE4SS. Server, listen host and lobby (HasAuthority = true) — unchanged.
The UCS has no jumps, so the new code is simply inserted before EX_Return, offsets before it do not change.
py -3 -I patch_hero.py <in.json> <out.json>"""
import base64, json, struct, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from kismet import disasm
from dis import find_script

src, dst = sys.argv[1:3]
d = json.load(open(src, encoding="utf-8-sig"))
names = d["NameMap"]


def find_import(obj, outer_name=None):
    for i, imp in enumerate(d["Imports"]):
        if imp["ObjectName"] != obj:
            continue
        if outer_name is None or d["Imports"][-imp["OuterIndex"] - 1]["ObjectName"] == outer_name:
            return -(i + 1)
    raise KeyError(obj)


def import_idx(obj, outer, cls, cls_pkg="/Script/CoreUObject"):
    for i, imp in enumerate(d["Imports"]):
        if imp["ObjectName"] == obj and imp["OuterIndex"] == outer:
            return -(i + 1)
    d["Imports"].append({"$type": "UAssetAPI.Import, UAssetAPI", "ObjectName": obj, "OuterIndex": outer, "ClassPackage": cls_pkg,
                         "ClassName": cls, "PackageName": None, "bImportOptional": False})
    for n in (obj, cls, cls_pkg):
        if n not in names:
            names.append(n)
    return -len(d["Imports"])


has_auth = find_import("HasAuthority", "Actor")
# 31.10: the stub was replicated (by class) -> the client sent it to the server, the server dropped the connection ("Client attempted to create
# sub-object"). The result of AddComponentByClass goes straight into Context(...).SetIsReplicated(false).
set_rep = import_idx("SetIsReplicated", find_import("ActorComponent"), "Object")
add_comp = find_import("AddComponentByClass", "Actor")
mgr_cls = find_import("TgfEquipmentManagerComponent", "/Script/LyraGame")

e = next(x for x in d["Exports"] if x["ObjectName"] == "UserConstructionScript")
raw = bytearray(base64.b64decode(e["Data"]))
at, mem, disk = find_script(raw)
script = bytes(raw[at + 8:at + 8 + disk])
items, _ = disasm(script, names)

ret = [it for it in items if it[3] == "Return" and it[2] == 0]
assert len(ret) == 1 and not any(it[3].startswith(("Jump", "JumpIfNot", "PushExecutionFlow")) for it in items), "wrong UCS"
m_ret, d_ret = ret[0][0], ret[0][1]
assert script[d_ret:] == bytes([0x04, 0x0B, 0x53]), script[d_ret:].hex()

identity = struct.pack("<10d", 0, 0, 0, 1, 0, 0, 0, 1, 1, 1)       # quat XYZW, translation, scale
m_add = m_ret + 15 + 5
m_skip = m_add + 1 + 102 + 4 + 8 + 11
new_code = (bytes([0x07]) + struct.pack("<i", m_add) +                    # JumpIfNot ADD
            bytes([0x1C]) + struct.pack("<i", has_auth) + bytes([0x16]) +  #   HasAuthority()
            bytes([0x06]) + struct.pack("<i", m_skip) +                   # Jump SKIP
            bytes([0x19]) +                                               # ADD: Context(
            bytes([0x1C]) + struct.pack("<i", add_comp) +                 #   AddComponentByClass(
            bytes([0x20]) + struct.pack("<i", mgr_cls) +                  #     TgfEquipmentManagerComponent,
            bytes([0x28]) +                                               #     bManualAttachment = false,
            bytes([0x2B]) + identity +                                    #     Identity,
            bytes([0x28]) + bytes([0x16]) +                               #     bDeferredFinish = false),
            struct.pack("<i", 11) + struct.pack("<ii", 0, 0) +            #   skip=11, rvalue = none
            bytes([0x1C]) + struct.pack("<i", set_rep) +                  #   .SetIsReplicated(
            bytes([0x28]) + bytes([0x16]))                                #     false))
new_script = script[:d_ret] + new_code + script[d_ret:]
mem2, disk2 = mem + (m_skip - m_ret), len(new_script)

raw2 = raw[:at] + struct.pack("<ii", mem2, disk2) + new_script + raw[at + 8 + disk:]
e["Data"] = base64.b64encode(bytes(raw2)).decode()
e["SerialSize"] = len(raw2)

items2, mem_check = disasm(new_script, names)
assert mem_check == mem2, (mem_check, mem2)
json.dump(d, open(dst, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print(f"imports: SetIsReplicated={set_rep} HasAuthority={has_auth} AddComponentByClass={add_comp} TgfEquipmentManagerComponent={mgr_cls}")
print(f"script mem {mem}->{mem2}, disk {disk}->{disk2}")
for m, dd, depth, t in items2:
    print(f"  {m:5} {'  ' * depth}{t}")
