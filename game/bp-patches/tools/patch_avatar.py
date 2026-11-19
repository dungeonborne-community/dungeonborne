"""BP_AvatarActor patch (R14): in ReceiveBeginPlay "UpdateMesh()" -> "if IsServer(Self): UpdateMesh()".
On the client the hero body is assembled from the replicated pawn before the equipment manager appears on it, and
the native UpdateMesh crashes on GetEquipBySlot(null) (0x142f602c6). Server, listen host and lobby (standalone) — as before.
In-memory offsets do not change: FinalFunction(UpdateMesh) (10 bytes in memory) -> Jump + Nothing of the same length, the new code
is appended before EX_EndOfScript and jumps back to the PopExecutionFlow that follows the call.
py -3 -I patch_avatar.py <in.json> <out.json>"""
import base64, json, struct, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from kismet import disasm
from dis import find_script

src, dst = sys.argv[1:3]
UG = "ExecuteUbergraph_BP_AvatarActor"
d = json.load(open(src, encoding="utf-8-sig"))
names = d["NameMap"]


def name_idx(n):
    if n not in names:
        names.append(n)
    return names.index(n)


def import_idx(obj, outer, cls, cls_pkg="/Script/CoreUObject"):
    for i, imp in enumerate(d["Imports"]):
        if imp["ObjectName"] == obj and imp["OuterIndex"] == outer:
            return -(i + 1)
    d["Imports"].append({"$type": "UAssetAPI.Import, UAssetAPI", "ObjectName": obj, "OuterIndex": outer, "ClassPackage": cls_pkg,
                         "ClassName": cls, "PackageName": None, "bImportOptional": False})
    for n in (obj, cls, cls_pkg):
        name_idx(n)
    return -len(d["Imports"])


def find_import(obj, outer=None):
    return next(-(i + 1) for i, imp in enumerate(d["Imports"])
                if imp["ObjectName"] == obj and (outer is None or imp["OuterIndex"] == outer))


ksl = find_import("KismetSystemLibrary")
is_server = import_idx("IsServer", ksl, "Object")
avatar = find_import("TgfAvatarActor")
update_mesh = find_import("UpdateMesh", avatar)

e = next(x for x in d["Exports"] if x["ObjectName"] == UG)
raw = bytearray(base64.b64decode(e["Data"]))
at, mem, disk = find_script(raw)
script = bytes(raw[at + 8:at + 8 + disk])
items, _ = disasm(script, names)

# the only UpdateMesh call in the ubergraph — the ReceiveBeginPlay branch, followed by PopExecutionFlow
calls = [i for i, it in enumerate(items) if it[3] == f"FinalFunction obj={update_mesh}"]
assert len(calls) == 1, calls
k = calls[0]
m_call, d_call = items[k][0], items[k][1]
assert items[k + 1][3] == "EndFunctionParms" and items[k + 2][3] == "PopExecutionFlow", items[k:k + 3]
m_next, d_next = items[k + 2][0], items[k + 2][1]
assert m_next - m_call == 10 and d_next - d_call == 6, (m_next - m_call, d_next - d_call)
assert script[-1] == 0x53
m_end = mem - 1                                  # in-memory offset of EX_EndOfScript = start of the new code

m_skip = m_end + 16 + 10
new_code = (bytes([0x07]) + struct.pack("<i", m_skip) +                        # JumpIfNot skip
            bytes([0x68]) + struct.pack("<i", is_server) + bytes([0x17, 0x16]) + #   IsServer(Self)
            bytes([0x1C]) + struct.pack("<i", update_mesh) + bytes([0x16]) +   # UpdateMesh()
            bytes([0x06]) + struct.pack("<i", m_next))                         # skip: Jump -> PopExecutionFlow
new_mem = 16 + 10 + 5
stub = bytes([0x06]) + struct.pack("<i", m_end) + bytes([0x0B]) * 5          # 10 bytes in memory
new_script = script[:d_call] + stub + script[d_next:-1] + new_code + bytes([0x53])
mem2, disk2 = mem + new_mem, len(new_script)

raw2 = raw[:at] + struct.pack("<ii", mem2, disk2) + new_script + raw[at + 8 + disk:]
e["Data"] = base64.b64encode(bytes(raw2)).decode()
e["SerialSize"] = len(raw2)

# self-check: the disassembler must reach the end and count mem2
items2, mem_check = disasm(new_script, names)
assert mem_check == mem2, (mem_check, mem2)
json.dump(d, open(dst, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print(f"imports: IsServer={is_server} UpdateMesh={update_mesh}")
print(f"script mem {mem}->{mem2}, disk {disk}->{disk2}; stub @{m_call} -> Jump {m_end}; new code @{m_end} -> Jump {m_next}")
for m, dd, depth, t in items2:
    if m_call - 30 <= m <= m_next or m >= m_end:
        print(f"  {m:5} {'  ' * depth}{t}")
