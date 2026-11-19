"""Disassemble a function from UAssetGUI JSON. py -3 -I dis.py <json> <export> <out.txt>"""
import base64, json, struct, sys
sys.path.insert(0, __import__("os").path.dirname(__file__))
from kismet import disasm


def find_script(raw):
    for i in range(0, len(raw) - 8):
        mem, disk = struct.unpack_from("<ii", raw, i)
        if 0 < disk <= len(raw) - i - 8 and 0 < mem < 1_000_000 and raw[i + 8 + disk - 1] == 0x53:
            return i, mem, disk
    raise ValueError("bytecode not found")


if __name__ == "__main__":
    js, name, out = sys.argv[1:4]
    d = json.load(open(js, encoding="utf-8-sig"))
    e = next(x for x in d["Exports"] if x["ObjectName"] == name)
    raw = base64.b64decode(e["Data"])
    at, mem, disk = find_script(raw)
    items, mem_total = disasm(raw[at + 8:at + 8 + disk], d["NameMap"])
    with open(out, "w", encoding="utf-8") as o:
        o.write(f"script @raw {at}: mem={mem} disk={disk}; the disassembler counted mem={mem_total} -> {'OK' if mem_total == mem else 'MISMATCH'}\n")
        for m, dd, depth, t in items:
            o.write(f"{m:5} d{dd:5} {'  ' * depth}{t}\n")
    print(f"mem {mem} vs {mem_total}: {'OK' if mem_total == mem else 'MISMATCH'}")
