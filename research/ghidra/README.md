# research/ghidra — analysing the client's native code in Ghidra

When it is needed: a game rule lives in native code (the client's C++), not in Blueprint or Lua — for example, what
`StartLocalBattle` does, where `LOCAL_GAME_MODE_OPENID` comes from, which function to call from `dbnative`. Everything visible in
the paks (BP, Lua, tables) is analysed without Ghidra: `docs/RESEARCH.md`.

**The exe on disk is packed** (code sections are zeros), so static Ghidra on it is useless. What is analysed is **the exe image
carved from a full process dump** of the game: in memory the code is already decrypted.

## What you need

- Ghidra 12.2 + JDK 21: `cargo xtask setup --apply --tool=ghidra-dist,jdk` (into `home\tools\ghidra` and
  `home\toolchains\jdk`, ~750 MB, no administrator rights). `. <home>\env.ps1` sets `JAVA_HOME`.
  Whether it is present: `cargo ws doctor` (the `ghidra` line).
- **Exe image**: ready-made, from the artifact store (on request only, 171 MB):
  `cargo ws secrets run artifact_store -- cargo ws bootstrap --apply --artifact="client exe image"` →
  `home\re\ghidra\images\db_image.bin`. Build: TimeDateStamp `0x67598c54`, base `0x140000000`, size `0xaaf5000`,
  sha256 `d6ff66e977de9e803641955d265038065f783f3d00133fefd0d220f8dda6fd2e`; the code (`.text`) is fully decrypted,
  the `.tvm0` section is code virtualized by the protection, Ghidra cannot read it. With the image, steps 1–2 below are not needed: go straight to 3.
- **Process dump**: only if the owner provides it via a link (9 GB, memory of someone else's machine: do not publish, do not
  forward, delete after the work). Put it in `home\re\ghidra\dumps\`.
- Python 3.12 for the scripts below.

Folders (created by `ws bootstrap`): `home\re\ghidra\dumps` — dumps, `images` — carved exe images,
`projects` — Ghidra projects.

## Steps

1. **Dump map:** `python -I research\ghidra\scripts\mdmap.py <dump> home\re\ghidra\images\map.json` — modules and
   memory ranges (Memory64List); the JSON holds the base, size and ranges of `Dungeonborne.exe` (base 0x140000000).
2. **Exe image:** `python -I research\ghidra\scripts\carve.py <dump> <json> home\re\ghidra\images\db_image.bin`.
3. **Ghidra project** without auto-analysis (it takes hours on 300 MB of code):
   `<ghidra>\support\analyzeHeadless.bat home\re\ghidra\projects DB -import home\re\ghidra\images\db_image.bin
   -processor x86:LE:64:default -loader BinaryLoader -loader-baseAddr 0x140000000 -noanalysis`
4. **Find a function:** by a string from the game log — `strs.py` (what is at an address), `xref.py <image> <string>`
   (where the string is and who references it: rip-relative lea), function bounds — `pdata.py <image> <address>` (the .pdata table).
   UFunction names are {name, exec-thunk} pairs in .rdata (they are visible in the exe on disk too).
5. **Decompile a tree:** `<ghidra>\support\analyzeHeadless.bat home\re\ghidra\projects DB -process -readOnly
   -noanalysis -scriptPath research\ghidra\scripts -postScript DecompTree.java <out.c> <depth> <address>` —
   the function and everything it calls, down to the given depth.
6. **Verification**: only with a mod in the game (UE4SS hook, log), on your own test stand. Conclusions go to `research/<topic>` or `docs/`,
   with addresses "for this build".

## Rules

- Do not read live process memory, do not patch or unpack the exe on disk, do not bypass the anti-cheat.
- Decrypting paks with the key and analysing the INTL login: only when the owner says so (`CLAUDE.md`).
- Decompiled game code stays only in the team copy's `research`, never in the public edition.

A full worked example (owner has it): the chain `StartLocalBattle` → `LOCAL_GAME_MODE_OPENID`, 01.11.2026.
