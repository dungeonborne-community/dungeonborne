---
name: dungeonborne-ace-restart
description: "ACE \"Anti-cheat system works error (13-131084-2)\" = game restarted seconds after previous process exit; launcher/relay/mods ruled out"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Prod ACE crashes ("random messages", e.g. "Anti-cheat system works error … (13-131084-2)") are caused by starting Dungeonborne.exe a few seconds after the previous instance exited (playtesters did Alt+F4 → immediate relaunch). Reproduced locally 05.11 13:35 on the game server (start 5 s after quit); starts with ~2 min gap had no dialog. A/B 05.11: launcher 1.3.0 + relay + all 11 prod mods against local backend — 16.5 min, no ACE error. Backend only acks player_ace_packet.

**Why:** a day of suspicion fell on the launcher; it is not the cause.
**How to apply:** keep a 30 s gap between a game/DS exit and the next start (DbTools restarts, Robot start_headless, launcher Play). User's rule 06.11: 30 s is enough, the earlier ~2 min was excessive; 5 s reproduced the error. Never try to bypass ACE — only change our own timing. Details: FIX_LIST A1. Related: [[dungeonborne-launcher-auth]], [[dungeonborne-robot-server]].
