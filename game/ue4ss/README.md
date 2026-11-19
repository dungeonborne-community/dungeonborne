# game/ue4ss — UE4SS mods

Lua mods for [UE4SS](https://github.com/UE4SS-RE/RE-UE4SS). UE4SS itself is not shipped here: install it from its
release into the game's `Binaries\Win64` folder (the launcher does this for players).

| Mod | Where it runs | What it does |
|---|---|---|
| `DungeonborneRuClient` | game client | client fixes and settings for community servers (`settings.ini`) |
| `DungeonborneRuServer` | dedicated server | server-side rules for matches hosted by the community backend |
| `DungeonborneR14` | game client | crash workaround; pairs with a Blueprint fix pak (`game/bp-patches` in the team copy, not in the public edition) |
| `CrashTracer` | game client | log around hero initialisation (`crashtracer_pawn_init.log`) for crash hunting |

**Team copy only** (debug and research mods, not in the public edition): `ClientProbe` (client probe log,
`docs/CLIENT_PROBE.md`), `DungeonborneStatWatch` (stats on the dedicated server), `DungeonborneDruidProbe`,
`DungeonborneArenaScan`, `NameDump`, `DSTest`, `JesterSwap` (the jester boss, `game/unreal/jester`). The UE4SS stock
mods (`BPModLoaderMod`, `Keybinds`, `shared`…) are copies from its release. The native helper `dbnative` is in
`native/dbnative` (`build.bat`, MSVC); its built dll comes from the artifact store (`ws bootstrap`).

`Mods/mods.txt` and `Mods/mods.json` choose which mods UE4SS loads; `UE4SS-settings.ini` is the UE4SS configuration
used with them. Mods only use names and paths of the game's classes and assets; no game files are included.
Mods are packaged and installed by `cargo xtask mods` in `core/backend` (`game/catalog/README.md`).
