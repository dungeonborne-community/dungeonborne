# Glossary

| Term | Meaning |
|---|---|
| **backend** | our game server emulator (`core/backend`): accounts, lobby, inventory, trade, matchmaking. Speaks the client's protocol. |
| **DS, dedicated server, battle server** | the game's own Unreal server binary that runs one match (raid). The backend starts it and talks to it through `ds-agent`. |
| **ds-agent** | small service next to the dedicated servers: starts and stops them on the backend's request. |
| **relay** | the backend's TLS endpoint for the launcher (login, server manifest, mods). The launcher pins its certificate. |
| **launcher** | players' app (`apps/launcher`): picks a server, logs in, installs mods, checks the network. |
| **UE4SS** | open-source scripting system for Unreal Engine games; our mods are Lua scripts for it (`game/ue4ss`). |
| **dbnative** | our native UE4SS module (C++) for things Lua cannot reach. |
| **pak** | Unreal Engine content archive. A mod pak overrides game assets. |
| **season** | a set of server rules and content switched as a whole (`SEASON=<id>`). |
| **balance kit** | tables + tools + AI prompt that Balance Studio opens; the designer returns a **patch**. |
| **ws** | workspace tool (`tools/ws`): `check` before commits, `doctor`/`bootstrap` for the machine, `secrets`, `roles`. |
| **xtask** | `cargo xtask …` — build tasks written in Rust (setup, devkit, ci, dist). |
| **DevKit** | window over `ws` for people who prefer buttons. |
| **workspace / home / vault** | the three folders: git repository / machine data (`DB_HOME`: toolchains, game copies, caches) / secrets (KeePassXC). |
| **config/local.toml** | this machine's paths and choices; not in git (`config/local.example.toml` is the template). |
| **visibility** | `public` ⊂ `team` ⊂ `owner`: who may receive a file or a secret. This repository is the `public` part. |
| **stand** | a local test setup: backend + dedicated server + one or two clients on one PC. |
