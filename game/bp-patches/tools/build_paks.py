"""Builds the blueprint patch paks from your own copy of the game (19.11), the README pipeline in one command:
retoc to-legacy (the pak key) -> UAssetGUI to JSON (no mappings: the patches work on raw export bytes, ug.py) -> patch_*.py -> back to an asset -> retoc to-zen.

  zzzzzzz_BPFix_P      build/pak/         BP_Tgf_Hero_Default: the client's own equipment manager (R14)  patch_hero.py
  zzzzzzz_ChatFix_P    build/pak/         ChatChannelConfigDataTable: world chat in the panel            patch_chat.py
  zzzzzzz_ServerNet_P  build/pak_server/  5 projectile/ability BPs: IsDedicatedServer -> IsServer        patch_isserver.py
  zz_LIPatch_P         build/pak/         LevelInfinite login scripts: login without the official service patch_lipatch.py
                                          (--only=lipatch; a plain .pak by repak)

Normally run by `cargo ws dump paks --apply` (it gives the key from the vault session and the paths of the game
reference). By hand:  GAME_AES_KEY=<key> py -3 -I build_paks.py --paks <game>/Dungeonborne/Content/Paks
                      [--only bpfix,chatfix,servernet] [--out <game/bp-patches>] [--work <temp folder>]
Each patch script checks what it edits and stops on another game build. The result of the same game build is the same
as the paks the project ships (checked 19.11 chunk by chunk); retoc packs without parallelism so the order is stable."""
import argparse, hashlib, os, shutil, subprocess, sys, tempfile

sys.dont_write_bytecode = True  # the tools' __pycache__ is tracked in git: do not rewrite it
os.environ["PYTHONDONTWRITEBYTECODE"] = "1"  # same for the patch scripts run as subprocesses
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))  # ug.py next to it (python -I does not add the script's folder)
sys.path.insert(0, str(HERE.parents[2] / "tools" / "py"))
import wspaths  # noqa: E402

SERVERNET = ["GA_TGF_DeathKnight_E", "B_Hook", "B_TGF_Normal_ProjectileBase", "B_TGF_Monster_Track_Bullet_Base",
             "B_TGF_Track_Projectile_WizardFireEye"]
PAKS = {
    "bpfix": ("pak/zzzzzzz_BPFix_P", "patch_hero.py", ["BP_Tgf_Hero_Default"]),
    "chatfix": ("pak/zzzzzzz_ChatFix_P", "patch_chat.py", ["ChatChannelConfigDataTable"]),
    "servernet": ("pak_server/zzzzzzz_ServerNet_P", "patch_isserver.py", SERVERNET),
}


def sha(p):
    return hashlib.sha256(Path(p).read_bytes()).hexdigest()


def run(args, what, secret=None):
    r = subprocess.run(args, capture_output=True, timeout=1800)
    if r.returncode != 0:
        out = (r.stdout + r.stderr).decode("utf-8", "replace")
        if secret:
            out = out.replace(secret, "<key>")
        sys.exit(f"{what}: exit {r.returncode}\n{out[-1500:]}")
    return r


LIPATCH_DIR = "Dungeonborne/Plugins/LevelInfinite/Content/LevelInfinite/Luac/Luac_common/AuthLogin/"
LIPATCH_FILES = [LIPATCH_DIR + n for n in ("AuthLoginController64.luac", "AuthLoginController32.luac",
                                          "AuthLoginConsoleController64.luac", "AuthLoginConsoleController32.luac")]


def lipatch(paks, key, work, out):
    """zz_LIPatch_P: the LevelInfinite login scripts of your copy, patched by patch_lipatch.py, packed by repak.
    Without patch_lipatch.py only extracts the originals into <work>/lipatch/orig (research)."""
    repak = str(wspaths.resolve(wspaths.local("tools", "repak") or "home:tools/repak/repak.exe"))
    orig = work / "lipatch" / "orig"
    shutil.rmtree(work / "lipatch", ignore_errors=True)
    orig.mkdir(parents=True)
    for pak in sorted(Path(paks).glob("*.pak")):
        r = subprocess.run([repak, "-a", key, "list", str(pak)], capture_output=True, timeout=600)
        if LIPATCH_DIR not in r.stdout.decode("utf-8", "replace"):
            continue
        args = [repak, "-a", key, "unpack", "-q", "-f", "-o", str(orig)]
        for f in LIPATCH_FILES:
            args += ["-i", f]
        run(args + [str(pak)], f"lipatch: repak unpack {pak.name}", secret=key)
        print(f"lipatch: originals from {pak.name}")
        break
    found = [f for f in LIPATCH_FILES if (orig / f).is_file()]
    if len(found) != len(LIPATCH_FILES):
        sys.exit(f"lipatch: found {len(found)} of {len(LIPATCH_FILES)} login scripts in the game paks")
    for f in found:
        print(f"  {f.rsplit('/', 1)[1]}  {sha(orig / f)[:16]}")
    patch = HERE / "patch_lipatch.py"
    if not patch.is_file():
        print("lipatch: no patch_lipatch.py yet - originals extracted only")
        return
    r = subprocess.run([sys.executable, "-I", "-B", str(patch), str(orig), str(work / "lipatch" / "pak")], capture_output=True)
    print((r.stdout + r.stderr).decode("utf-8", "replace").strip())
    if r.returncode != 0:
        sys.exit("lipatch: patch_lipatch.py refused the scripts")
    dst = Path(out) / "pak" / "zz_LIPatch_P.pak"
    dst.unlink(missing_ok=True)
    run([repak, "pack", "--version", "V8B", str(work / "lipatch" / "pak"), str(dst)], "lipatch: repak pack")
    print(f"lipatch: {dst}  {sha(dst)[:16]}")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--paks", required=True, help="<game>/Dungeonborne/Content/Paks")
    ap.add_argument("--only", default=",".join([*PAKS, "lipatch"]))
    # build/ — not pak/ (the owner's older working paks live there): build/pak, build/pak_server; *.pak/.ucas/.utoc are ignored by git
    ap.add_argument("--out", default=str(HERE.parent / "build"))
    ap.add_argument("--work", default=None)
    a = ap.parse_args()
    key = os.environ.get("GAME_AES_KEY", "").strip()
    if not key:
        sys.exit("no GAME_AES_KEY - run through `cargo ws dump paks --apply` (vault session) or set it for this process")
    os.environ.pop("UG_USMAP", None)  # no mappings (ug.py, 19.11)
    import ug  # noqa: E402  (reads UG_USMAP at import)
    ug.ensure_map()
    retoc = str(wspaths.resolve(wspaths.local("tools", "retoc") or "home:tools/retoc/retoc.exe"))
    work = Path(a.work or tempfile.mkdtemp(prefix="bp-paks-"))
    for name in [n.strip() for n in a.only.split(",") if n.strip()]:
        if name == "lipatch":
            lipatch(a.paks, key, work, a.out)
            continue
        if name not in PAKS:
            sys.exit(f"unknown pak '{name}' (there: {', '.join(PAKS)})")
        out_stem, patch, assets = PAKS[name]
        w = work / name
        shutil.rmtree(w, ignore_errors=True)
        stage = w / "stage"
        for asset in assets:
            legacy = w / "legacy" / asset
            # the key goes only to retoc's argument list, never to the output (run() masks it)
            run([retoc, "-a", key, "to-legacy", "--version", "UE5_1", "--no-shaders", "--no-script-objects",
                 "--filter", asset, a.paks, str(legacy)], f"{asset}: retoc to-legacy", secret=key)
            found = [p for p in legacy.rglob(asset + ".uasset")]
            if len(found) != 1:
                sys.exit(f"{asset}: expected one {asset}.uasset in the game, found {len(found)}")
            rel = found[0].relative_to(legacy)
            js, patched = w / f"{asset}.json", w / f"{asset}.patched.json"
            rc, out = ug.tojson(str(found[0]), str(js))
            if not js.is_file():
                sys.exit(f"{asset}: UAssetGUI tojson failed (rc {rc})\n{out[-800:]}")
            r = subprocess.run([sys.executable, "-I", "-B", str(HERE / patch), str(js), str(patched)], capture_output=True)
            print(f"  {asset}: {(r.stdout + r.stderr).decode('utf-8', 'replace').strip()}")
            if r.returncode != 0:
                sys.exit(f"{asset}: {patch} refused the asset")
            target = stage / rel
            rc, out = ug.fromjson(str(patched), str(target))
            if not target.is_file():
                sys.exit(f"{asset}: UAssetGUI fromjson failed (rc {rc})\n{out[-800:]}")
        dst = Path(a.out) / (out_stem + ".utoc")
        dst.parent.mkdir(parents=True, exist_ok=True)
        for ext in ("utoc", "ucas", "pak"):
            dst.with_suffix("." + ext).unlink(missing_ok=True)
        run([retoc, "to-zen", "--no-parallel", "--version", "UE5_1", str(stage), str(dst)], f"{name}: retoc to-zen")
        for ext in ("pak", "ucas", "utoc"):
            f = dst.with_suffix("." + ext)
            print(f"{name}: {f}  {sha(f)[:16]}")
    print(f"work folder (decrypted assets of your copy — keep it private): {work}")


if __name__ == "__main__":
    main()
