# Third-party software

This repository contains only our own code. Third-party programs are **downloaded at setup** from their official
releases and checked against sha256 pinned in `config/bootstrap.manifest.toml` and `config/tools.lock`; they are not
redistributed here. Each is under its own license:

| Program | Used for | License | Source |
|---|---|---|---|
| rustup, Rust toolchain | building everything | MIT / Apache-2.0 | rust-lang.org |
| PortableGit (Git for Windows) | git and Git Bash for people and AI agents | GPL-2.0 | github.com/git-for-windows/git |
| GitHub CLI (`gh`) | pull requests and releases from the terminal | MIT | github.com/cli/cli |
| cargo-xwin | building without Visual Studio | MIT | github.com/rust-cross/cargo-xwin |
| Microsoft CRT and Windows SDK (downloaded by cargo-xwin) | linking without Visual Studio | Microsoft's license terms, accepted on download | microsoft.com |
| LLVM / clang | building windows without Visual Studio | Apache-2.0 WITH LLVM-exception | github.com/llvm/llvm-project |
| trunk | building web UIs of the Tauri apps | MIT / Apache-2.0 | github.com/trunk-rs/trunk |
| tauri-cli | building Tauri apps | MIT / Apache-2.0 | github.com/tauri-apps/tauri |
| tailwindcss (standalone) | CSS of the apps | MIT | github.com/tailwindlabs/tailwindcss |
| UE4SS (experimental build v3.0.1-1183) | runtime for the Lua mods, part of the launcher's client runtime (`make_client_bundle.py --upstream`) | MIT | github.com/UE4SS-RE/RE-UE4SS |
| Goldberg Steam Emulator (master 475342f0) | Steam API of the game without Steam, part of the launcher's client runtime | LGPL-3.0 | gitlab.com/Mr_Goldberg/goldberg_emulator |
| KeePassXC (installed by you) | secrets database | GPL-2.0/3.0 | keepassxc.org |

Rust crates are listed in the `Cargo.lock` files and keep their own licenses (mostly MIT / Apache-2.0); fonts under
`apps/launcher/ui/fonts` are under the SIL Open Font License, with their license files next to them.
