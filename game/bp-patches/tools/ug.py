"""UAssetGUI CLI: tojson / fromjson. py -3 -I ug.py roundtrip <asset> <work>

19.11: no mappings by default. The bp-patches scripts (patch_hero.py and the others) work on the raw bytes of the
exports, which UAssetGUI keeps only when it has no mappings for the game; with the game's usmap it parses
UserConstructionScript as a function and patch_hero.py refuses it. Until 19.11 the mappings were passed by the name
"Dungeonborne", which UAssetGUI never found (the file in its folder was named in lower case), so every shipped patch pak
was in fact built without them. For reading assets with types (research), set UG_USMAP=<file.usmap>.
"""
import glob, hashlib, os, shutil, subprocess, sys
from pathlib import Path

# machine paths — config/local.toml via tools/py/wspaths.py: UAssetGUI from home (tools.lock)
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools" / "py"))
import wspaths  # noqa: E402

UG = str(wspaths.resolve(wspaths.local("tools", "uassetgui") or "home:tools/uassetgui/UAssetGUI.exe"))

SRC_MAP = os.environ.get("UG_USMAP") or None
MAP_DIR = os.path.join(os.environ["LOCALAPPDATA"], "UAssetGUI", "Mappings")
# the name carries the usmap's hash: UAssetGUI takes mappings by name from one folder per user, a fixed name kept the first
# usmap ever copied there
MAP_NAME = ("Dungeonborne-" + hashlib.sha256(open(SRC_MAP, "rb").read()).hexdigest()[:12]) if SRC_MAP else None


def ensure_map():
    if not SRC_MAP:
        return
    os.makedirs(MAP_DIR, exist_ok=True)
    dst = os.path.join(MAP_DIR, MAP_NAME + ".usmap")
    if not os.path.exists(dst):
        shutil.copy(SRC_MAP, dst)

def run(*args):
    r = subprocess.run([UG, *args], capture_output=True, timeout=600)
    out = (r.stdout + r.stderr).decode("utf-8", "replace")
    return r.returncode, out


def h(p):
    return hashlib.md5(open(p, "rb").read()).hexdigest() if os.path.exists(p) else None


def compare(a, b):
    same = True
    for ext in ("uasset", "uexp"):
        pa, pb = os.path.splitext(a)[0] + "." + ext, os.path.splitext(b)[0] + "." + ext
        s = h(pa) == h(pb)
        same &= s
        print(f"  {ext}: {os.path.getsize(pa) if os.path.exists(pa) else '-'} vs {os.path.getsize(pb) if os.path.exists(pb) else '-'} same={s}")
    return same


def tojson(asset, js):
    return run("tojson", asset, js, "VER_UE5_1", *([MAP_NAME] if MAP_NAME else []))


def fromjson(js, asset):
    os.makedirs(os.path.dirname(asset), exist_ok=True)
    return run("fromjson", js, asset, *([MAP_NAME] if MAP_NAME else []))


if __name__ == "__main__":
    ensure_map()
    cmd = sys.argv[1]
    if cmd == "roundtrip":
        asset, work = sys.argv[2], sys.argv[3]
        os.makedirs(work, exist_ok=True)
        js = os.path.join(work, os.path.splitext(os.path.basename(asset))[0] + ".json")
        rc, out = tojson(asset, js)
        print("tojson rc", rc, out[-500:], "json", os.path.getsize(js) if os.path.exists(js) else "-")
        dst = os.path.join(work, "rt", os.path.basename(asset))
        rc, out = fromjson(js, dst)
        print("fromjson rc", rc, out[-800:])
        compare(asset, dst)
    elif cmd == "tojson":
        print(tojson(sys.argv[2], sys.argv[3]))
    elif cmd == "fromjson":
        print(fromjson(sys.argv[2], sys.argv[3]))
