"""LIPatch (zz_LIPatch_P): the game's LevelInfinite login scripts, patched so the client logs in without the official
LevelInfinite service. The edit is the community patch's ("Dungeonborne单机PVE补丁", docs/CN_PATCH_IMPORTS.md),
recorded 19.11 as byte edits against the game's own files: in each of the 4 compiled scripts (Lua 5.3 bytecode)
2–4 instructions (conditions and jumps) are replaced — 8–14 bytes per file, the size does not change. Only those bytes
are here, no game code: the originals come from your own copy (`cargo ws dump paks --only=lipatch --apply`).

Every file is checked by sha256 before (the game build this edit is for) and after (the community patch's result);
on another game build the script stops instead of guessing.
py -3 -I patch_lipatch.py <originals root> <out root>   (both laid out as Dungeonborne/Plugins/LevelInfinite/...)"""
import hashlib, sys
from pathlib import Path

DIR = "Dungeonborne/Plugins/LevelInfinite/Content/LevelInfinite/Luac/Luac_common/AuthLogin"
# file: (sha256 of the game's original, sha256 after the edit, [(offset, expected hex, new hex), ...])
EDITS = {
    "AuthLoginConsoleController32.luac": ("36eff37feb0fe034d7d06f608567fe754557b01ebfdf9507bb4b0c03032ec4e5", "dcef3a6c9599bdd0e70061a5cdda68ac2c82196f9f03efd1c753b9ecbb3cea0e", [(13103, "800080", "c0ff7f"), (15729, "d740", "1380"), (15732, "00", "80"), (21132, "81004500", "13c00880"), (32730, "28414802", "13001580")]),
    "AuthLoginConsoleController64.luac": ("39a4b5b4b39be8cb87d211a2074b33f6f691136e5ee78059614849e9fb2622af", "74b68dec3c7db3e6c7d818feca22cd7f0ffc9c5b541696332b448ec6b130219f", [(13103, "800080", "c0ff7f"), (15729, "d740", "1380"), (15732, "00", "80"), (21132, "81004500", "13c00880"), (32730, "28414802", "13001580")]),
    "AuthLoginController32.luac": ("45d71ec8e2e3de8a791f27d592ef679f5fce1986ce46986703854122e5cd1b65", "e6bf53393f9f18dd752ea2825d6066e7177702311607795f94281584da71e561", [(30890, "e9c04000", "13402880"), (64494, "10", "25"), (64496, "0002", "8000"), (64504, "00", "80")]),
    "AuthLoginController64.luac": ("758bbdebda8b4153596660fa1608a46d5a9c316c341b901ae57942c55b9134d7", "cd18dbe1c60bbf8d7c693080b65970d85a60da149a45f1841d1168c2efe49ce5", [(30890, "e9c04000", "13402880"), (64494, "10", "25"), (64496, "0002", "8000"), (64504, "00", "80")]),
}


def main():
    src, dst = Path(sys.argv[1]), Path(sys.argv[2])
    for name, (sha_in, sha_out, edits) in EDITS.items():
        f = src / DIR / name
        if not f.is_file():
            sys.exit(f"{name}: not in {src / DIR}")
        data = bytearray(f.read_bytes())
        if hashlib.sha256(data).hexdigest() != sha_in:
            sys.exit(f"{name}: not the game build this edit is for (sha256 differs) - stopped")
        for off, want, new in edits:
            want, new = bytes.fromhex(want), bytes.fromhex(new)
            if data[off:off + len(want)] != want:
                sys.exit(f"{name}: unexpected bytes at {off} - stopped")
            data[off:off + len(new)] = new
        if hashlib.sha256(data).hexdigest() != sha_out:
            sys.exit(f"{name}: result differs from the community patch - stopped")
        out = dst / DIR / name
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(bytes(data))
        print(f"ok: {name} ({len(edits)} edits)")


if __name__ == "__main__":
    main()
