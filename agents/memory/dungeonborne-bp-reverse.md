---
name: dungeonborne-bp-reverse
description: "How to read Dungeonborne blueprint bytecode (retoc + kismet-analyzer), where the AES key file and usmap live"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Reading BP logic of the game (UE 5.1, IoStore paks V11 with encrypted index):
- AES key: user keeps it in `<data>\aes_key.txt` (one line `0x…`). Read it from the file into a variable;
  never print/echo it or put it in chat, logs, or files.
- Mappings: `C:\Steam\steamapps\common\Dungeonborne\Dungeonborne\Binaries\Win64\ue4ss\Dungeonborne-5.1.1-0+++UE5+Release-5.1-cdbc0b66.usmap` (UE4SS Ctrl+Numpad6).
- Tools in `<data>\tools_re`: `retoc\bin\retoc.exe`, `kismet-analyzer\bin\kismet-analyzer-e8982e9-win-x64\kismet-analyzer.exe`; also `<data>\FModel`.
- Find asset: `retoc -a $key list --path <Paks dir>` (paths only with `--path`).
- Extract: `retoc -a $key to-legacy --version UE5_1 --no-shaders --no-script-objects --filter <Name> <Paks> <out>`.
- Decompile: `kismet-analyzer --ue-version VER_UE5_1 -m <usmap> gen-cfg <x.uasset> <existing out dir>` → `.txt` CFG
  (don't pipe through `Select-Object -First` — it kills the process before writing).
- Pattern found: BP branches `HasAuthority && !IsStandalone → return` skip client logic for the listen host's own pawn;
  fix by calling `ExecuteUbergraph_<Class>(entry)` of the skipped block from UE4SS (see [[dungeonborne-workflow]]).
- Scope agreed with user: only targeted assets for fixing mods; no anti-cheat bypass / memory dumping.
  Refined 2026-10-30: reading the game's own crash dumps (UEMinidump.dmp, WinDbg/own parser) and debugger output the
  user captured is fine for crash diagnosis; not doing: ACE bypass/disable, unpacking the exe to disk, patching the exe.
  Parser for minidumps (exception, registers, module+offset): scratchpad `mdinfo.py` (rewrite if gone).
  2026-10-31: do NOT read the live game process memory (ReadProcessMemory / SeDebugPrivilege) — out of the agreed
  scope; it also fails (ACCESS_DENIED) and auto mode blocks SeDebugPrivilege.
- Crash call stack: UE writes the real unwound stack into the instance's `ue-*.log` (`LogWindows: Error: [Callstack]`),
  better than scanning minidump stacks. exe sections are named `.std` and code is packed on disk (zeros), decrypted on
  demand in memory; `.pdata` (function bounds) and `.rdata` (UFunction `{name, exec-thunk}` pairs) are readable on disk.
- 2026-11-01: user supplied a FULL 9 GB game crash dump (Saved\Crashes\UECC-…E56ED7E0…) and asked to read native C++
  with Ghidra (`G:\ghidra\ghidra_12.2_PUBLIC`, headless works with the installed JDK). Method: carve exe image from the
  dump (Memory64List, base 0x140000000) → Ghidra Raw Binary `-noanalysis` → `DecompTree.java` on given addresses;
  function starts via `.pdata`, UFunction `{name, exec}` pairs in `.rdata`. Scripts + results:
  `docs\ghidra_startlocalbattle\` (StartLocalBattle → LOCAL_GAME_MODE_OPENID chain).
- Carved image + Ghidra project (reuse, read-only: `analyzeHeadless <gproj> DB -process -readOnly -noanalysis
  -postScript DecompTree.java out depth addr`): only on the owner's PC, in a Claude session scratchpad
  `%LOCALAPPDATA%\Temp\claude\C--Users-<user>--claude-spares-spare-9vAt9y\aa206458-f33c-4596-afba-f154221ad6cd\scratchpad\`
  (`db_image.bin`, `gproj\DB.gpr`). Find funcs by wide log string → rip-relative lea xref → `.pdata` (scripts in
  `docs\ghidra_startlocalbattle\scripts`). 03.11: UNetConnection::Close = 0x14516efa0 (research\netclose).
- 13.11: player minidumps (UEMinidump.dmp) hold only ~256 B of exe code, but stream 65536 = UE CrashContext XML with
  full unwound callstacks of all threads (parse the minidump directory). Code for the same exe build: carve from the
  local full dump `%LOCALAPPDATA%\Dungeonborne\Saved\Crashes\UECC-…E56ED7E0…\UEMinidump.dmp` (mdmap.py + carve.py),
  Ghidra BinaryLoader base 0x140000000. Known: GDynamicRHI vtable +0x318 = RHIGetRenderQueryResult (0x14374a030);
  0x143cef620 = occlusion-cull fetch (SceneVisibility).
- BP patch tooling: `<repo>\bp_patches` (README, `patch_relic.py`, `patch_avatar.py`, `ug.py`);
  retoc can't `to-legacy` a lone mod container — verify by CFG before packing and by a game run.
- Lyra/GFCM: GameFeature AddComponents creates REPLICATED components only on authority; clients get them as `_<n>`
  replicated subobjects — that is normal, not a missing client component.
