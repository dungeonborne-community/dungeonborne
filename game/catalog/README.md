# Mods and paks: building from the repository

What it is and why: [docs/MODS_DESIGN.md](../../docs/MODS_DESIGN.md), plan — [docs/MODS_REFACTOR_PLAN.md](../../docs/MODS_REFACTOR_PLAN.md).
The list of all parts and profiles — [catalog.toml](catalog.toml). The working game install is no longer a source: everything
that goes to players and to Robot is built from here.

## Once per PC

This machine's paths (Lua, repak, game installs) — in `config/local.toml` at the workspace root: `[tools]`, `[games]` (template — `config/local.example.toml` at the root).

## Commands

```
cargo xtask mods check                                     check: catalog, Lua, forbidden calls, dbnative, pak overrides
cargo xtask mods build   --profile=client                  out/mods/client/ + manifest.json (sha256 of every file, commit)
cargo xtask mods install --profile=dev-client --game=client   build and lay out into an install (game closed)
cargo xtask mods verify  --profile=dev-server --game=steam    compare an install with the composition
```

Profiles: `client` — players, `server` — Robot, `dev-client` and `dev-server` — local installs (`--game` — a name from
`config/local.toml` or a path). `install` copies only changed files. Mod enabling (`mods.txt`, `enabled.txt`) and
install settings (`native_data.txt`, `teams.txt`…) are left alone — `verify` shows the differences: which profile mods
are disabled, which extra ones are enabled, which paks are not from the profile.

Season: parts with `season = "<id>"` get into the build only when the season is active — by default `seasons/active.txt`
(`cargo xtask season use <id|none>`), otherwise `--season=<id|none>`. The composition writes the season into `manifest.json`. Currently active:
`playtest` (the lower relics pak is part of it; the Jester instead of the Reaper is not in playtest, dev only for now).

Paks with `build = "repak"` are built from a folder; identical sources give the same file (cache `out/mods/.cache`: repak
writes files in a different order, and without the cache the package for players would change for no reason).

## Launcher packages (players)

```
cargo xtask mods build --profile=client
cd ..\launcher
python -I tools/make_mods.py --src <home>\out\mods\client   # the client for --exe — local.toml [games] client
```

Package composition — `launcher/tools/mods.json` (the `package` field in the catalog — the same id). Deploying to the server is a separate step
with confirmation (`deploy/vm/README.md`, "Client mods").

## Robot

Kit scripts — `ops/deploy/robot` (`start_headless.ps1`, `install_gameserver.ps1`, `killmutex.ps1`,
`ds-agent.toml`). Kit mods and paks — `out/mods/server/Dungeonborne/…`: `Content/Paks` -> `Kit\Paks`,
`Binaries/Win64` -> `Kit\Win64`. The UE4SS runtime (UE4SS.dll, settings with consoles disabled, dwmapi.dll, Goldberg) is not
in the catalog yet — taken from the previous kit. Deploy — only with confirmation, comparison with the prod install —
a read-only snapshot (MODS_REFACTOR_PLAN, section 3).

## A new part

An `[[item]]` record in `catalog.toml` (the fields are described at the top of the file), `cargo xtask mods check`, a mod README following the template from
MODS_DESIGN §13.
