"""<Relic>_Enchant patch: in DealCastCharge "if HasAuthority and not IsStandalone: return" ->
«if HasAuthority and not GetLyraCharacterFromActorInfo().IsLocallyControlled(): return».
In-memory offsets do not change: LetBool(IsStandalone) is replaced with Jump + Nothing of the same length, the new code is appended
before EX_EndOfScript and jumps back to the original JumpIfNot.
py -3 -I patch_relic.py <in.json> <out.json> <ubergraph export>"""
import base64, json, struct, sys, os
sys.path.insert(0, os.path.dirname(__file__))
from kismet import disasm
from dis import find_script

src, dst, ug_name = sys.argv[1:4]
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


def find_import(obj):
    return next(-(i + 1) for i, imp in enumerate(d["Imports"]) if imp["ObjectName"] == obj)


engine = next(-(i + 1) for i, imp in enumerate(d["Imports"]) if imp["ObjectName"] == "/Script/Engine")
pawn = import_idx("Pawn", engine, "Class")
is_local = import_idx("IsLocallyControlled", pawn, "Object")
get_char = find_import("GetLyraCharacterFromActorInfo")
is_standalone = find_import("IsStandalone")

e = next(x for x in d["Exports"] if x["ObjectName"] == ug_name)
raw = bytearray(base64.b64decode(e["Data"]))
at, mem, disk = find_script(raw)
script = bytes(raw[at + 8:at + 8 + disk])
items, _ = disasm(script, names)

# LetBool(LocalVariable IsStandalone_RV, CallMath IsStandalone(Self)) and the JumpIfNot that follows it
let = next(i for i, it in enumerate(items) if it[3] == "LetBool" and items[i + 2][3] == f"CallMath obj={is_standalone}")
m_let, d_let = items[let][0], items[let][1]
jif = next(it for it in items[let:] if it[3].startswith("JumpIfNot") and it[2] == 0)
m_jif, d_jif = jif[0], jif[1]
assert m_jif - m_let == 21 and d_jif - d_let == 25, (m_jif - m_let, d_jif - d_let)
var_bytes = script[d_let + 1:d_let + 1 + 17]   # 0x00 + FFieldPath(IsStandalone_RV)
assert var_bytes[0] == 0x00
fpath = var_bytes[1:]
assert script[-1] == 0x53
m_end = mem - 1                                  # in-memory offset of EX_EndOfScript = start of the new code

new_code = (bytes([0x14]) + var_bytes +                                   # LetBool IsStandalone_RV =
            bytes([0x19]) +                                               #   Context(
            bytes([0x1C]) + struct.pack("<i", get_char) + bytes([0x16]) + #     GetLyraCharacterFromActorInfo()
            struct.pack("<i", 10) + fpath +                               #     skip=10, rvalue=IsStandalone_RV
            bytes([0x1C]) + struct.pack("<i", is_local) + bytes([0x16]) + #     .IsLocallyControlled())
            bytes([0x06]) + struct.pack("<i", m_jif))                     # Jump -> the original JumpIfNot
new_mem = 43 + 5
stub = bytes([0x06]) + struct.pack("<i", m_end) + bytes([0x0B]) * 16     # 21 bytes in memory
new_script = script[:d_let] + stub + script[d_jif:-1] + new_code + bytes([0x53])
mem2, disk2 = mem + new_mem, len(new_script)

raw2 = raw[:at] + struct.pack("<ii", mem2, disk2) + new_script + raw[at + 8 + disk:]
e["Data"] = base64.b64encode(bytes(raw2)).decode()
e["SerialSize"] = len(raw2)

# self-check: the disassembler must reach the end and count mem2
items2, mem_check = disasm(new_script, names)
assert mem_check == mem2, (mem_check, mem2)
json.dump(d, open(dst, "w", encoding="utf-8"), ensure_ascii=False, indent=2)
print(f"imports: Pawn={pawn} IsLocallyControlled={is_local} GetLyraCharacterFromActorInfo={get_char}")
print(f"script mem {mem}->{mem2}, disk {disk}->{disk2}; stub @{m_let} -> Jump {m_end}; new code @{m_end} -> Jump {m_jif}")
for m, dd, depth, t in items2:
    if m_let <= m < m_jif + 14 or m >= m_end:
        print(f"  {m:5} {'  ' * depth}{t}")
