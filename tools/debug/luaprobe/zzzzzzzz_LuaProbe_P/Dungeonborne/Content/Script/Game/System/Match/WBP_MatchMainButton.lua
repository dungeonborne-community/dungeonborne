-- DEBUG ONLY (04.11): entry point of the client Lua probe (docs/CLIENT_PROBE.md). Neither into the launcher nor onto Robot —
-- installed and removed by debug\luaprobe\install_luaprobe.ps1. The button is the game's original (_Orig — byte for byte from
-- pakchunk0-Windows.pak), on top only the probe start; enabled by ue4ss/Mods/ClientProbe/lua_config.txt (enabled=1)
-- or by the -dbprobe flag.
local Button = require("Game.System.Match.WBP_MatchMainButton_Orig")
pcall(function() require("Game.System.Match.LuaProbe").Start() end)
return Button
