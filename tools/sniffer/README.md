# tools/sniffer — TGCP sniffer

A Rust program that captures and splits TGCP frames (the game gateway protocol) for analysis. A separate Cargo project
(not in the root workspace): `cargo build --release` in this folder. Dumps go to `home\runs`, not to git.

`script.ps1` — a quick look at the first bytes of `dump_*.bin` dumps in the current folder. Capturing official server traffic
and decryption — only with the owner's go-ahead (`CLAUDE.md`, rules).
