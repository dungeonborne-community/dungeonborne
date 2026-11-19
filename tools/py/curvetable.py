"""Game CurveTable (e.g. CareerSkillDamageCurveTable) from UAssetGUI JSON -> lines "name [(time, value)…]".

UAssetGUI does not parse CurveTable rows: they are in Exports[0].Extras (base64). Format (17.11, verified on
CareerSkillDamageCurveTable: 68 rows, the numbers matched the skill tooltips):
  int32 rows, u8 mode; per row: FName (int32 index in NameMap, int32 number), 3 bytes, int32 count,
  count × (float time, float value), 6 bytes of tail.

    python -I tools/py/curvetable.py <table.json> [--grep Priest]
JSON: UAssetGUI tojson <uasset> <json> VER_UE5_1 dungeonborne (docs/RESEARCH.md).
"""
import argparse
import base64
import json
import struct
import sys


def rows(path):
    j = json.load(open(path, encoding="utf-8"))
    names = j["NameMap"]
    d = base64.b64decode(j["Exports"][0]["Extras"])
    n = struct.unpack("<i", d[:4])[0]
    o = 5  # int32 rows + u8 mode
    for r in range(n):
        ni, _num = struct.unpack("<ii", d[o:o + 8])
        cnt = struct.unpack("<i", d[o + 11:o + 15])[0]
        if cnt < 0 or cnt > 1000:
            raise ValueError(f"row {r} at byte {o}: bad key count {cnt} - not a CurveTable of this format")
        keys = [tuple(round(x, 4) for x in struct.unpack("<ff", d[o + 15 + 8 * k:o + 23 + 8 * k])) for k in range(cnt)]
        yield (names[ni] if ni < len(names) else f"#{ni}"), keys
        o += 15 + 8 * cnt + 6
    if o != len(d):
        print(f"warning: parsed {o} of {len(d)} bytes", file=sys.stderr)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("json", help="CurveTable JSON from UAssetGUI tojson")
    ap.add_argument("--grep", default="", help="only rows whose name contains this substring (case-insensitive)")
    a = ap.parse_args()
    for name, keys in rows(a.json):
        if a.grep.lower() in name.lower():
            print(name, keys)


if __name__ == "__main__":
    main()
