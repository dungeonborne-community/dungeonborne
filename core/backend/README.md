# core/backend — game server emulator

Rust workspace of its own (not part of the root Cargo workspace):

| Crate / folder | What it is |
|---|---|
| `src` (`dbserver-rs`) | the server: TGCP gateway, directory server, RPC handlers, lobby and matchmaking, login, relay for the launcher, admin API, link to dedicated servers |
| `src/generated` | protocol and table types generated from the game's protobuf descriptors (prost) |
| `ds-agent` | service next to the dedicated servers: starts and stops them on the backend's request |
| `crates/balance-kit` | balance tables, kit and patch format shared with Balance Studio |
| `xtask` | `cargo xtask <balance|season|mods|loot|names|tables>` — data tasks |
| `server.example.toml` | configuration template: copy to `server.toml` and fill in |

## Build and test
From `core/backend`, on Windows or Linux:
```
cargo test -p dbserver-rs
cargo build --release -p dbserver-rs
```
The database is RocksDB, so building needs `libclang` from LLVM. On Windows either without admin rights:
`cargo xtask setup --apply --tool=llvm` at the workspace root (pinned LLVM into `home\toolchains\llvm`) and
`LIBCLANG_PATH` set to its `bin`; or install LLVM (`winget install LLVM.LLVM`, needs admin) — it is then found in
`C:\Program Files\LLVM\bin` by itself. Do not build through sccache (`RUSTC_WRAPPER` empty): with it the rocksdb C++ build fails
under MSVC. The first build compiles RocksDB and takes several minutes; checked 17.11 on Windows: 185 tests pass.
The production server is a Linux build made by the owner; you never need one to work on the code.

## What it needs at runtime
- `server.toml` (from `server.example.toml`) with your addresses; secrets from environment variables. A local stand
  needs none of them: `tools/stand/run_local.ps1` starts the server without accounts and relay. The production
  secrets (relay TLS key, Discord app secret, bot token) stay with the owner; `config/secrets.toml` lists only what
  your edition has.
- Server data from the game: tables and protocol descriptors (`ExcelConfig`, `Configure.pb`, `Network.pb`) and the
  reference packets in `assets/packets`. **Team copy:** they are in this folder. **Public edition:** they are game files
  and are not published; it ships no tool to extract them and no pak key — getting them from a game you own is up to
  you, under the game's terms. Without them the server builds and its tests run, but it does not start.
- For matches: the game's own dedicated server and `ds-agent` on the server machine (locally — the stand,
  `tools/stand/README.md`).

`member.toml` lists the commands; `cargo ws check` at the workspace root checks the layout.
