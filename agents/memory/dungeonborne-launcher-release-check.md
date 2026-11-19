---
name: dungeonborne-launcher-release-check
description: "Before signing the launcher: only the exe taken right after cargo tauri build, run it and look at the window; for a test run override BOTH APPDATA and LOCALAPPDATA"
metadata:
  visibility: public
  node_type: memory
  type: feedback
---

Sign the launcher only from a copy taken right after `cargo tauri build`, and before signing run it and look at the window.

**Why:** on 06.11 a broken 1.4.0 was published: `cargo test --release` in the same `CARGO_TARGET_DIR` overwrote `release\dungeonborne-launcher.exe` with a build without the embedded UI; the window showed "127.0.0.1 refused to connect" (dev server 1420). Neither tests, nor the manifest, nor the checksum catch this; it was found by hand after release.

**How to apply:** after `cargo tauri build` immediately copy the exe to a separate file and sign that (`release.py sign --exe <copy>`). The string `127.0.0.1:1420` is present in a working build too, it is not a sign. An open launcher prevents starting a second copy. Window capture: PrintWindow (PW_RENDERFULLCONTENT) — works even under other windows; do not touch the mouse (the machine is in interactive use). The window may open minimized: ShowWindow(SW_SHOWNOACTIVATE).

**Test run isolation (11.11):** `settings.json`/`servers.json` live in `%APPDATA%\DungeonborneLauncher`, cache and logs in `%LOCALAPPDATA%`. Override BOTH variables; from 07.11 to 11.11 only LOCALAPPDATA was overridden, and test windows changed the real launcher settings on the machine (language ru, notes_seen). Login (Credential Manager) is shared across Windows — do not log in/out in a test window. Write the settings file without BOM (`[IO.File]::WriteAllText` with `UTF8Encoding $false`): with a BOM the launcher silently falls back to default settings. Related to [[dungeonborne-launcher-auth]].
