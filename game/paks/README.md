# game/paks — pak sources

| Folder | What |
|---|---|
| `balance/` | the balance tables pak for the client (`zzzzzz_Balance_P.pak`); built by `cargo xtask balance build` from `core/backend` from `content/balance` and the season |
| `legacy/` | old paks (MatchLocal, RelicQuality, RebuildPrivate) — for reference, not deployed |

A pak is not reproducible byte for byte (repak): compare the contents (`repak unpack`), not the file hash. The `.pak` files themselves
are not in git (`ws check`, rule binaries): the built ones are in the artifact store (`ws bootstrap`). How paks get into the
game — `game/catalog/README.md` (`cargo xtask mods`).
