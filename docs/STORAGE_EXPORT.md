# What lives outside git, and what to take from it

The repository holds code, documents, seasons and tables. Binary artifacts and backups live in a storage box
(`docs/CLOUD_ARCHITECTURE.md`, section 3). This is the list of what is there and how to get each piece without that
storage. **The public edition publishes no binary artifacts**: paks and dumps contain game assets and scripts, so
they are rebuilt from your own copy of the game or taken from their upstream projects.

## Artifact store (sub3 read / sub4 write)

Objects addressed by sha256; the list with hashes is `config/artifacts.toml`, `ws bootstrap` downloads what a
workspace needs, `ws artifacts push` uploads new ones. A workspace without access to the storage leaves `[store]`
empty and puts the files where `config/artifacts.toml` says (`path`); the hashes there tell you whether a rebuilt
file is the same.

| Artifact | Size | How to get it without the storage |
|---|---|---|
| balance pak `zzzzzz_Balance_P` | 1.2 MB | `cargo xtask balance build` (tables are in the repository) |
| paks `zz_LIPatch_P`, `zzzzzzz_BPFix_P`, `zzzzzzz_ChatFix_P`, `zzzzzzz_ServerNet_P` (login, client crash fix, world chat, server branches) | 0.5 MB | `cargo ws dump paks --apply` from your copy of the game → `game/bp-patches/build` (checked 19.11: the same content as the shipped paks) |
| `dbnative.dll` (native helper of the client mod) | 0.2 MB | `game/ue4ss/native/dbnative/build.bat` (MSVC, or clang-cl + xwin without Visual Studio) |
| launcher runtime `client.zip` | 9.8 MB | `cargo xtask setup --apply --tool=goldberg,ue4ss`, then `python apps/launcher/tools/make_client_bundle.py --upstream [--ca <your backend CA>]`: the Goldberg Steam emulator and UE4SS releases pinned in `config/tools.lock` (checked 19.11: the same runtime files as the shipped bundle) |
| item icons | 22 MB | `tools/py/item_icons.py` from an FModel export of your copy |
| game usmap | 9 MB | `tools\stand\usmap.ps1` after `cargo ws stand --apply`: the stand client dumps it headless (UE4SS `DumpUSMAP`) into `home\re\ue4ss` (checked 19.11); `ws dump` needs it, the patch paks do not |
| CXXHeaderDump | — | UE4SS in your copy of the game (`docs/GAME_DATA.md`) |
| client Lua (`lua_all`) | 2.4 MB | `ws dump` from your copy of the game |
| client executable image | 179 MB | the method in `docs/owner/RE_TECHNIQUE.md` |
| paks `zz_Loot_P`, `zz_Market_P` | 0.1 MB | from the community patch; both are overridden by the balance pak, a server runs without them |
| pak `zz_Jester_P` (the Jester boss) | 26.5 MB | not public (owner only) |
| signed `ws` / DevKit builds | — | build from source (`cargo xtask setup --from-source`) |

## Backups (sub1, sub2) — never published

- **sub1, VM restic repository:** the production database (accounts, Discord IDs, characters and items of players),
  server settings with secrets, the Discord bot data, Forgejo dumps. Personal data of players — stays private.
- **sub2, owner workstation:** research data, the owner's vault and local settings. Private.

To run your own server you start with an empty database: the backend creates it on first start.

## Secrets — never published

All secrets are in the KeePassXC vault (`config/secrets.toml` lists names and purposes, never values): relay TLS key,
launcher and `ws` signing keys, Discord OAuth secret and bot token, SSH keys, restic and storage passwords. A new
server needs its own: generate them (`docs/SECRETS.md`, `docs/runbooks/secrets.md`).

The game's pak AES key is publicly known and is not a secret of this project; `ws dump` needs it to read your own copy
of the game. Put it into your vault as `game_aes_key` (`docs/SECRETS.md`).
