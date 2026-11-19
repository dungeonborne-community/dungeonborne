# tools/py — python development tools

Run from the workspace root: `python -I tools/py/<name>.py --help` (a window with `home\env.ps1`: the workspace's
portable Python 3.12; AI agents: `"$DB_PYTHON" -I tools/py/<name>.py`). Scripts take machine data paths from
`config/local.toml` via `wspaths.py` (do not write your own paths in scripts). Each script's header says what it does.

| Group | Scripts |
|---|---|
| paths | `wspaths.py` — shared workspace and `local.toml` paths for the other scripts |
| protocol and logs | `rpc_evidence.py` (everything about one RPC), `probe_timeline.py` (probe logs + `rpc_dump`), `compare_flow.py`, `compare_heads.py`, `pbdesc.py`, `gen_probe_targets.py` |
| client analysis | `curvetable.py` (CurveTable from UAssetGUI JSON), `mdinfo.py` (crash minidump) — `docs/RESEARCH.md` |
| login and launcher | `auth_e2e.py`, `auth_login.py` (login via relay and web API on the stand) |
| stand and match | `gm.py` (GM commands via the stand admin API), `player_result.py`, `stat_watch.py` (stats from the battle server) |
| balance, loot, seasons | `loot_audit.py`, `loot_census.py`, `pvp_chests.py`, `auction_all.py`, `season_hide_items.py`, `season_merchant_prices.py`, `season_set_affixes.py`, `gamedata_dup_keys.py`, `item_icons.py` |

The `season_*` and `loot_*` scripts write to `content/` — a data change that a human reviews before committing.
