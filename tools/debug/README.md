# tools/debug — debug probes

`luaprobe/` — the client Lua probe in a separate pak `zzzzzzzz_LuaProbe_P` (`docs/CLIENT_PROBE.md`). It is installed and removed
in all installs from `config/local.toml [games]` (the game is closed):

```powershell
powershell -ExecutionPolicy Bypass -File tools\debug\luaprobe\install_luaprobe.ps1           # build and install
powershell -ExecutionPolicy Bypass -File tools\debug\luaprobe\install_luaprobe.ps1 -Remove   # remove
```

The pak is built by repak from `home\tools` (`[tools] repak`). The probe is silent until enabled (`lua_config.txt` or the
`-dbprobe` flag). The log is written to `log_dir` (`game/ue4ss/Mods/ClientProbe/lua_config.txt`); if that folder does not exist — to
`%LOCALAPPDATA%\Dungeonborne\Saved\RebuildProbe`, as with ClientProbe. It does not go into the launcher or onto the battle
server.
