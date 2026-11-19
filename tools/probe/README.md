# tools/probe — client and login probes

| What | Why |
|---|---|
| `auth_test/` | backend configs for checking login on the stand (`server_*.toml`: dev, launcher, Discord, no hosts) and launch scripts `run_*.ps1` |
| `chat_dt/` | the chat channel table from the client (`ChatChannelConfigDataTable`, uasset) — source for the test chat pak |
| `gameover_*.txt` | a decoded example of the match result message (protobuf in text form) |
| `logs/` (created) | client probe logs ClientProbe / LuaProbe (`docs/CLIENT_PROBE.md`), read by `tools/py/probe_timeline.py` |

The client probe is the mod `game/ue4ss/Mods/ClientProbe` and the pak `tools/debug/luaprobe`. Traffic captures of the official servers and
gateway databases are held only by the owner.
