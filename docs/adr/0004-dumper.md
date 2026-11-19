# 0004. `tools/dumper` and `ws dump`: game data from your own game copy, without a window

- **Date:** 2026-11-18.
- **Decided by:** the owner ("hybrid: ws dump in Rust, a small extractor over the official CUE4Parse").
- **Decision:**
  - a new member `tools/dumper` (C#, `visibility = "team"`, component `research`): `dbdump`, a ~150-line program
    over CUE4Parse (FabianFG, Apache-2.0, the library inside FModel): paks + usmap + key → PNG and JSON, the layout of
    an FModel export;
  - `ws dump` (Rust, `tools/ws/core/src/dump.rs`) owns everything around it: finds the game reference
    `home/game/original/<build>`, the usmap, Oodle and the key, builds `dbdump` with the .NET SDK, runs it per set
    (`tools/dumper/sets.toml`), writes `home/dumps/<build>/{export,index,manifest.tsv}`;
  - the .NET SDK comes from `config/tools.lock` on request (`manual`, component `research`).
- **Why:** everyone has their own copy of the game (`docs/PACKAGES.md`: game data is built on the person's machine,
  not handed out). Until now game data came from a manual FModel export on the owner's machine; the team had no way
  to get monster abilities, effects or curves, and a wiki agent wrote "no data" where the paks have it (rehearsal
  18.11). There is no maintained Rust library that reads UE5.1 assets with unversioned properties; CUE4Parse is the
  reference implementation, so only the reading is in C#, the format and the links are ours.
- **What changes:** `ws dump` command; `docs/GAME_DATA.md` (the game reference, `ws dump`, what data is where);
  decrypting paks stays on a person's word (`docs/RESEARCH.md`) — `ws dump` without `--apply` only shows the plan.
  `dbdump` downloads nothing at run time (CLI tools on the same library fetch native DLLs from the internet).
