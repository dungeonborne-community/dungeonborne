# Full client dump: how to get the decrypted game code

How the decrypted exe image for Ghidra is obtained without touching the exe on disk, the memory of a running game or
the anti-cheat (the rules: `AGENTS.md`, "The game client — our rules"). The analysis itself:
`research/ghidra/README.md`. Do it only on your own copy of the game, under its terms.

## Why

`Dungeonborne.exe` on disk is packed: the code sections are zeros, and Ghidra sees nothing in it. In the memory of a
running client the code is already decrypted. We do not read live process memory (rule; anti-cheat and
ACCESS_DENIED), so we take **the full crash dump that the engine writes itself**.

## How (31.10.2026, this is how the dump for StartLocalBattle / LOCAL_GAME_MODE_OPENID was obtained)

1. Start the client with the Unreal switch **`-fullcrashdump`**: on a crash the standard UE handler writes a full
   process memory dump (MiniDumpWithFullMemory) instead of a minidump. It bypasses and injects nothing — it is an engine switch.
   On the test stand: start the client the same way as in the automated run (`tools/stand`), plus `-fullcrashdump` in the arguments.
2. Make the client crash. On 31.10 a known crash was reproduced: ~5 s after changing the hero's outfit in LocalDev
   (the server held the battle for ~4 minutes, the client connected and crashed). Any crash will do, as long as the
   needed code has run at least once (the module is loaded in full anyway).
3. Dump: `%LOCALAPPDATA%\Dungeonborne\Saved\Crashes\UECC-Windows-<id>\UEMinidump.dmp`, ~9 GB. This is the entire
   process memory on your machine.
4. Then the same as the team: `research/ghidra/scripts/mdmap.py` → `carve.py` (exe image, base 0x140000000) →
   Ghidra without auto-analysis → `DecompTree.java` (`research/ghidra/README.md`). Example analysis — `docs/ghidra_startlocalbattle`.

## Before sharing a dump

A full dump contains your client's memory: login and session token, Steam/Goldberg ID, server addresses, chat
fragments, paths on your machine. Therefore:

- take a separate dump on the test stand under a **test account**, not from a game login to prod;
- hand it over **as a link** (not in git, not in the team artifact storage), asking for deletion after the work;
- or hand over only the **carved exe image** (`carve.py`, ~300 MB): it has no heap and no stacks, only the game module.
  This is preferred — the team needs exactly the image for Ghidra.

## What not to do

Do not read live process memory (ReadProcessMemory / SeDebugPrivilege), do not unpack or patch the exe on disk,
do not bypass the anti-cheat. `-fullcrashdump` is the only method used, and it is a standard one.
