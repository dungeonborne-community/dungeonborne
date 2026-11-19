---
name: dungeonborne-mods-design
description: "Mod design and refactoring plan (pak, dll, UE4SS) from 07.11; layer and thread rules for new code; findings on package builds"
metadata:
  visibility: team
  node_type: memory
  type: project
---

On 07.11 `docs/MODS_DESIGN.md` (what goes where and how) and `docs/MODS_REFACTOR_PLAN.md` (analysis and stages 0–7) were written. As of 07.11 nothing is implemented, it waits for the user's decision.

Findings that are not in the code:
- `launcher/tools/make_mods.py` builds packages from the working `C:\Games\Dungeonborne`: `ru.client-hooks` from 06.11 contains `dbnative.dll` from 03.11. Before building a package put a fresh DLL into `F:`, until there is a build from the repository.
- `C:\Games\Dungeonborne` is incomplete (no `zz_LIPatch_P`, `dbnative`, RuClient disabled), looks like an install for the launcher; launching it bypassing the launcher probably hangs before login.
- RuServer takes `dbnative.dll` and `native_data.txt` from the RuClient folder.
- 07.11: local git created in `backups` (a9bf02b) and `launcher` (573b4f9), branch main, no remote; core.autocrlf=false in the repositories (system-wide true). `.gitignore` excludes: keys, DiscordCFG, builds, databases, dumps, binaries, `bundle/client.zip`. Plan: private GitHub + restic → Hetzner Storage Box; gh and restic not installed yet.

**Why:** the user asks to sort out the mixed mods and set rules for the future.

**How to apply:** write new mod code per `MODS_DESIGN.md`, unless the user decided otherwise. Related: [[dungeonborne-lua-thread-crash]], [[dungeonborne-launcher-auth]], [[dungeonborne-build-pc-risk]].
