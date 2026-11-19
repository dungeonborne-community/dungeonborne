---
name: dungeonborne-rocksdb
description: "Since 11.11 the backend is on RocksDB (not SurrealKV): build without sccache + libclang; own blob/cache defaults; the WSL script does touch"
metadata:
  visibility: team
  node_type: memory
  type: project
---

On 11.11 the prod SurrealKV database (surrealdb 2.7 / surrealkv 0.9) grew to 59 GB in 3 days and filled the VM disk: the clog journal
is append-only, nobody calls compact(). Switched to RocksDB (`kv-rocksdb`, folder `db_rocks`; on an old-format
folder the backend refuses to start).

- Windows build: `LIBCLANG_PATH="C:/Program Files/LLVM/bin"`, `RUSTC_WRAPPER=` and `CARGO_BUILD_RUSTC_WRAPPER=` empty —
  through sccache MSVC fails on "/WX-" (cl takes the flag for a file). Since 17.11 sccache is removed from core/backend/.cargo/config.toml;
  clang-sys finds LLVM in `C:\Program Files\LLVM\bin` by itself. Verified 17.11: `cargo test -p dbserver-rs` on Windows 185/185.
- `GameDb::rocksdb_defaults()` (db.rs): ENABLE_BLOB_FILES=false (otherwise values ≥4 KB go to blob files without GC — they would
  pile up, as in SurrealKV), BLOCK_CACHE 128 MB (default "RAM/2 − 1 GB"), SYNC_DATA=true. Check via `db_rocks/OPTIONS-*`.
- WSL build script (scratchpad `wsl_build_clean.sh [commit]`): after `git archive` — `touch` all files, otherwise cargo
  does not rebuild (on 11.11 the previous commit's binary went to prod). Check for the string `134217728` in the binary.
- Client settings — a separate `player_settings` record + cache, not the player document.

- Player gear on prod (14.11): `cp -a db_rocks` into /root → `/root/db_dump` (example, build in WSL with
  `RUSTC_WRAPPER=` empty) → `roles[].data` — protobuf bytes of `DUserRole`: 10 deploy_role → 13 wearing → 1 items →
  1 basic (1 type_id, 11 position → 3 x). Rarity: `core/backend/ExcelConfigjson/item_type.json` ue_source_type_id →
  `UeSource_ItemType.json` rarity. Delete the copy and the jsonl after the analysis.

**Why:** the prod disk and lost saves. **How to apply:** make any new large record in the database a separate
table; do not enable blob files without GC. Related: [[dungeonborne-prod-playtest]].
