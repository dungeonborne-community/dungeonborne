-- Mods/DungeonborneRuClient/Scripts/main.lua
-- FIX FOR CRASH 0x142f602c6: make sure UTgfEquipmentManagerComponent exists (r14 != 0) before BP_Avatar_Default_C spawns!
local LogFile = io.open("crash_ue4ss.log", "w")
local LogCount = 0
local MAX_LOG_LINES = 6000

local function Log(msg)
    if LogFile and LogCount < MAX_LOG_LINES then
        LogCount = LogCount + 1
        local s = tostring(msg)
        if #s > 300 then s = s:sub(1, 300) .. "..." end
        LogFile:write(os.date("%H:%M:%S") .. " | " .. s .. "\n")
        LogFile:flush()
    end
end

-- 12.11: mod settings — INI files in the mod folder, read at game start:
--   server_settings.ini — written by the launcher before every launch from the server settings (season, [client.DungeonborneRuClient.*]
--                          in seasons/*.toml → client_settings of the manifest). Players must not edit it: it is overwritten.
--   settings.ini        — local ones (stand, debugging); docs/MOD_SETTINGS.md. Without the files — the defaults in the code.
-- A value is taken from the server, otherwise from settings.ini. Format: [section], key = value, comments ; or #.
local function ReadModIni(name)
    local out = {}
    local f = io.open("ue4ss/Mods/DungeonborneRuClient/" .. name, "r") or io.open("Mods/DungeonborneRuClient/" .. name, "r")
    if not f then return out end
    local sec = ""
    for line in f:lines() do
        line = line:gsub("[;#].*$", "")
        local s = line:match("^%s*%[([%w_]+)%]%s*$")
        if s then
            sec = s
        else
            local k, v = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
            if k then
                out[sec] = out[sec] or {}
                out[sec][k] = v
            end
        end
    end
    f:close()
    return out
end
local ModIni = { Server = ReadModIni("server_settings.ini"), Local = ReadModIni("settings.ini") }

-- the setting's value (a string) and its source: "server" | "local"; none — nil
function ModSetting(section, key)
    local s = ModIni.Server[section]
    if s and s[key] ~= nil and s[key] ~= "" then return s[key], "server" end
    local l = ModIni.Local[section]
    if l and l[key] ~= nil and l[key] ~= "" then return l[key], "local" end
    return nil, nil
end

-- Migration from the old flag files (before 12.11): the setting from ModSetting, otherwise line 1 of the first old file
-- found (paths as before). The old files keep working until they are removed from the installs (Robot, stand).
function ModFlag(section, key, ...)
    local v, src = ModSetting(section, key)
    if v ~= nil then return v, src end
    for _, path in ipairs({ ... }) do
        local f = io.open(path, "r")
        if f then
            local line = f:read("*l") or ""
            f:close()
            return (line:gsub("^%s+", ""):gsub("%s+$", "")), "file " .. path
        end
    end
    return nil, nil
end

-- 07.11 (docs/CRASH_LONG_PLAY.md): all loops and delays — in the game thread. They used to be LoopAsync / ExecuteWithDelay: their
-- body ran in a service thread concurrently with the game thread, and that crashed UE4SS during long play. LoopInGameThreadWithDelay
-- does not stop on return true (like LoopAsync) — GameLoop does it via CancelDelayedAction and catches errors.
-- Nested ExecuteInGameThread calls inside are kept: in the game thread they only defer work to the next frame.
-- The timer is REGISTERED only in the game thread: the mod loads in the UE4SS thread, NotifyOnNewObject callbacks run in
-- the thread that creates the object. Registering from there while the game thread runs this mod's timers hangs the process
-- (07.11: the client hung while loading the mod, CPU 0). So first ExecuteInGameThread (the queue is safe from any
-- thread), and only there — LoopInGameThreadWithDelay / ExecuteInGameThreadWithDelay.
local function GameLoop(ms, fn)
    ExecuteInGameThread(function()
        local handle
        handle = LoopInGameThreadWithDelay(ms, function()
            local ok, stop = pcall(fn)
            if not ok then Log("[LOOP] error: " .. tostring(stop))
            elseif stop == true and handle then CancelDelayedAction(handle) end
        end)
    end)
end

local function GameDelay(ms, fn)
    ExecuteInGameThread(function() ExecuteInGameThreadWithDelay(ms, fn) end)
end

local function ShortName(obj)
    if not obj then return "nil" end
    local ok, full = pcall(function() return obj:GetFullName() end)
    if not ok or not full then return "err" end
    return full:match("([^%.%s]+)$") or full
end

-- 02.11: deferred calls (ExecuteWithDelay) do not hold a Lua UObject wrapper — after leaving a match (kick, drop, map
-- change) the world is destroyed, and a call on an old wrapper crashes in UE4SS.dll (access violation, pcall does not catch it; client
-- crashes 3–15 s after a disconnect, ue-client-7777.log 02.11). We store the path and look up a live object before use.
local function PathOf(obj)
    local full = ""
    pcall(function() full = obj:GetFullName() end)
    return full:match("^%S+%s+(.+)$") or ""
end

local function LiveObject(path)
    if path == "" then return nil end
    local obj = nil
    pcall(function() obj = StaticFindObject(path) end)
    if obj and obj:IsValid() then return obj end
    return nil
end

-- 05.11 (B13): the game server process (-dbserver). The mod runs there too (dbnative, commands, map rules), but
-- creation notifications for frequently created objects — pawns, ChildActorComponent, weapons, projectiles — are not set
-- on the server: the callback is called in the middle of object construction, and function calls on a half-built object crashed UE4SS
-- (04.11: three crashes on Robot in UWorld::SpawnActor ← SpawnDefaultPawnAtTransform, already without CrashTracer). Whatever the
-- server needs from them (full hero HP) — by polling live heroes (ServerHeroPoll), like HeroFix in RuServer.
local IsServerProc = false
pcall(function()
    local cl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary"):GetCommandLine():ToString():lower()
    IsServerProc = (" " .. cl .. " "):find("%s%-dbserver%s") ~= nil
end)

-- NotifyOnNewObject only on the client (see IsServerProc)
local function ClientNotify(path, fn)
    if IsServerProc then
        Log("[SERVER] no creation notification for " .. path)
        return
    end
    NotifyOnNewObject(path, fn)
end

Log("=== UE4SS 0x142f602c6 R14 EQUIPMENT MANAGER FIX + V32 COMBAT ===")
Log("process: " .. (IsServerProc and "game server (-dbserver)" or "client"))

local HeroHooksInstalled = false
local HookedMap = {}
local CombatHookedMap = {}
local HookedCombatClasses = {}
local ActiveChargeAbility = nil
local ChargeAvatarAddr = nil -- hero address of the active charge ability (see ReceiveTick)
local ChargeStartClock = 0
local LastChargeHookScan = 0
local ProjectileFiredAddr = nil  -- address of the ability that already had ProcessProjectile in this activation
local RelicBranchAddr = nil      -- address of the ability for which the client Relic branch was already called in this activation

-- Release of a heavy/charge attack (LMB). Native chain: LyraHeroComponent (Completed) -> ASC:AbilityInputTagReleased
--   -> AbilitySpecInputReleased -> InvokeReplicatedEvent(InputReleased) -> UAbilityTask_WaitInputRelease.OnRelease
--   -> BP OnRelease_EA7A… -> ProcessRelease + post effects.
--   "native" — the mod does NOT touch the GA and input at all (no hooks on GA functions, nor on the hero's InpActEvt/ReceiveTick).
--              Hypothesis check: the release was broken by our own hooks (SafeCombatHook was hooked on ALL GA functions,
--              including ExecuteUbergraph and OnRelease_* itself — the handler of the WaitInputRelease delegate).
--   "rpc"    — if 0.05 s after releasing LMB the ability has not released by itself — call its OnRelease_EA7A…(TimeHeld),
--              i.e. the same that WaitInputRelease would call in standalone. Details and history — in ReceiveTick.
--              Hooks only on K2_ActivateAbility/K2_OnEndAbility of the charge classes (to know the active ability).
--   "legacy" — the old workaround: a direct call of OnRelease_/ProcessRelease (bypassing WaitInputRelease, without post effects).
-- 30.10: "native" checked — without hooks the release still does not arrive (so our hooks are not the cause).
-- 30.10: the cause is NM_ListenServer (host_from_lobby, ?listen): in standalone (localbattle) the release works natively.
--        On a listen host the LMB release does not reach ASC.AbilitySpecInputReleased — "rpc" delivers it there itself.
-- 30.10: ServerSetInputReleased did not help: the ASC gets the release (InputPressed=false), but on listen the ability
--        has no WaitInputRelease. TellServerRelease gave a strike without the Relic wave; by the local vs listen trace what is needed
--        is exactly the OnRelease_ callback (it does ProcessRelease + TellServerRelease, after which the wave fires).
local CHARGE_RELEASE_MODE = "rpc"

-- Diagnostics: a trace of ALL functions of the classes from ChargeTraceClasses ([#n] Class:Function(args) rel=ChargeReleased t=s)
-- + a log of the Relic wave projectile appearing. To compare standalone (works natively) vs listen. Disable after the analysis.
-- 01.11: disabled — the client crashed (AV in the game from UE4SS) right after the trace of releasing a relic mace charge:
-- hooks on all GA functions parse the event arguments (Instigator/Target/OptionalObject + GetClass/GetFullName)
local CHARGE_TRACE = false
local ChargeTraceClasses = {
    "/Game/TGF/Ability/GameAbility/MeleeWeapon/GA_TGF_MeleeWeapon_Charge_Base.GA_TGF_MeleeWeapon_Charge_Base_C",
    "/Game/TGF/Ability/GameAbility/MeleeWeapon/GA_TGF_MeleeWeapon_Charge_Enchant_Base.GA_TGF_MeleeWeapon_Charge_Enchant_Base_C",
    "/Game/TGF/Equipment/WeaponPair/SwordRelic/GameAbility/GA_MeleeWeapon_SwordRelic_Enchant.GA_MeleeWeapon_SwordRelic_Enchant_C",
    -- 30.10: two daggers after a heavy "break" the character (does not hit, does not switch weapons) — check whether the ability hangs
    "/Game/TGF/Equipment/WeaponPair/DaggerDagger/GameAbility/GA_MeleeWeapon_DaggerDagger_Heavy.GA_MeleeWeapon_DaggerDagger_Heavy_C",
    "/Game/TGF/Equipment/Weapon/Dagger/GameAbility/GA_MeleeWeapon_Dagger_Charge.GA_MeleeWeapon_Dagger_Charge_C",
}

-- R14 stub mode — file Mods/DungeonborneRuClient/r14_mode.txt (read for every new pawn):
--   all    — as before, on all pawns (default if there is no file)
--   client — only in the client process (not the server): 31.10 on the server the stub shadowed the native
--            TgfEquipmentManagerComponent — GetComponentByClass returned it (empty)
--   off    — do not add (check whether crash 0x142f602c6 reproduces without it)
--   temp   — like client, but the stub is temporary: 31.10 it turned out the server's native manager does reach the client, only
--            later than the body timer (~2 s); when it arrives, the stub (and the inventory one) is removed (see TempStubs below)
-- 31.10 hybrid "listen as dedicated": the native UBattleDsUtility::InitPlayerGameData on listen overwrites MyOpenId
-- (which came from the connection URL ?OpenId=) with LOCAL_GAME_MODE_OPENID. We remember it before the call — LocalDev uses it to
-- find the player's loadout (RebuildProbe/ps-openid.txt, "<PlayerState name>=<OpenId>")
--
-- 01.11 per-player data (native_data.txt: line 1 — the backend URL up to the open_id value, e.g.
-- http://127.0.0.1:9091/ds/battle_user_data?open_id= ; the line "realkey" — also a record under the real OpenId).
-- The native helper native/dbnative.dll, before the native InitPlayerGameData, puts into UBattleDsUtility the native record
-- DBattleUserData of this player under the key LOCAL_GAME_MODE_OPENID (a fresh one — the native function does the full
-- initialization), and after it gives the player back the real MyOpenId. The game code is not patched (ue4ss/native/dbnative).
local RebuildProbeDir = (os.getenv("LOCALAPPDATA") or ".") .. "/Dungeonborne/Saved/RebuildProbe/"
local NativeDir, NativeFn, NativeLoadTried = nil, nil, false
local NativeTag = nil -- suffix of the dbnative exchange files by -port (see NativeCall)

local function NativeDataConfig()
    local url = ModSetting("native", "data_url")
    if url then
        local on = function(k) return (ModSetting("native", k) or ""):match("^1") ~= nil end
        return { url = url, realkey = on("realkey"), keepopenid = on("keepopenid"), noclose = on("noclose") }
    end
    local f = io.open("ue4ss/Mods/DungeonborneRuClient/native_data.txt", "r")
        or io.open("Mods/DungeonborneRuClient/native_data.txt", "r")
    if not f then return nil end
    local url = (f:read("*l") or ""):match("^%s*(%S+)")
    local rest = f:read("*a") or ""
    f:close()
    if not url then return nil end
    -- keepopenid: do not give back the real MyOpenId after InitPlayerGameData (LOCAL_GAME_MODE_OPENID stays)
    -- noclose: on a kick do not close the connection natively (UNetConnection::Close), only the fallback (03.11)
    return { url = url, realkey = rest:find("realkey", 1, true) ~= nil, keepopenid = rest:find("keepopenid", 1, true) ~= nil,
        noclose = rest:find("noclose", 1, true) ~= nil }
end

local function NativeCall(args)
    if not NativeLoadTried then
        NativeLoadTried = true
        for _, dir in ipairs({ "ue4ss\\Mods\\DungeonborneRuClient\\native\\", "Mods\\DungeonborneRuClient\\native\\" }) do
            local fn, err = package.loadlib(dir .. "dbnative.dll", "dbn_call")
            if fn then NativeDir, NativeFn = dir, fn; break end
            Log("[NATIVE] loadlib " .. dir .. ": " .. tostring(err))
        end
    end
    if not NativeFn then return { ok = "0", err = "dbnative.dll not loaded" } end
    -- 03.11: an instance with -port=N (N != 7777) has files req-N.txt / rsp-N.txt — the same rule in dbnative.c and RuServer
    if not NativeTag then
        NativeTag = ""
        pcall(function()
            local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
            local port = tonumber(ksl:GetCommandLine():ToString():lower():match("%-port=(%d+)") or "")
            if port and port ~= 7777 then NativeTag = "-" .. port end
        end)
    end
    local f = io.open(NativeDir .. "req" .. NativeTag .. ".txt", "w")
    if not f then return { ok = "0", err = "cannot write req.txt" } end
    for k, v in pairs(args) do f:write(k .. "=" .. tostring(v) .. "\n") end
    f:close()
    os.remove(NativeDir .. "rsp" .. NativeTag .. ".txt")
    NativeFn()
    local rsp = {}
    local r = io.open(NativeDir .. "rsp" .. NativeTag .. ".txt", "r")
    if r then
        for line in r:lines() do
            local k, v = line:match("^([%w_]+)=(.*)$")
            if k then rsp[k] = v end
        end
        r:close()
    end
    return rsp
end

local NativeDataPending = {} -- PlayerState address -> real OpenId (record planted, waiting for the end of InitPlayerGameData)
local TeamOfOpen = {}        -- OpenId -> team_id from the match data (user_parameter.team_id: party id or a solo player's user_id)

-- 03.11: swap retry on a network error — up to 3 attempts with a 0.5 s pause (WinHTTP timeouts 3–5 s: worst case ~16 s
-- in the game thread, only when backend is unavailable)
local SWAP_TRIES, SWAP_PAUSE_MS = 3, 500

-- 03.11: kicking a player without match data. On Robot (08:45) ClientReturnToMainMenuWithTextReason + ClientWasKicked +
-- K2_DestroyActor in one tick did not kick the players: deleting the controller right after an RPC — the RPCs do not arrive, and deleting
-- the controller does not close the connection (AGameSession::KickPlayer does the same — it is meant for a client that leaves
-- by itself on ClientWasKicked). Now like RuServer SendToLobby (checked during the post-match reset): ClientTravel of the client to the
-- lobby, the controller is left alone; after KICK_CLOSE_SEC, if the connection is still alive — the server closes it with the native
-- UNetConnection::Close (dbnative op=closeconn; if that fails — delete the pawn and the controller), after another KICK_CLOSE_SEC —
-- log whether the connection remained.
local KICK_CLOSE_SEC = 5
local KICK_LOBBY_URL = "/TgfMaps/SceneAssets/RuinedCrypt/Demo/Maps/RuinedCrypt_03"

local function RemoteConn(pc)
    local conn = nil
    pcall(function() conn = pc.Player end)
    if not (conn and conn:IsValid()) then return nil end
    local isLocal = false
    pcall(function() isLocal = pc:IsLocalController() end)
    if isLocal or conn:GetFullName():find("LocalPlayer", 1, true) then return nil end
    return conn
end

local function KickRemote(psPath, open, code, reason)
    ExecuteInGameThread(function()
        pcall(function()
            local ps = LiveObject(psPath)
            local pc = ps and ps:GetOwner()
            if not (pc and pc:IsValid() and RemoteConn(pc)) then
                Log("[KICK] " .. open .. ": " .. code .. " — no remote controller, not kicking")
                return
            end
            local pcPath = PathOf(pc)
            pcall(function() pc:ClientMessage(reason, FName("None"), 10.0) end)
            local ok = pcall(function() pc:ClientTravel(KICK_LOBBY_URL, 0, false, { A = 0, B = 0, C = 0, D = 0 }) end)
            if not ok then ok = pcall(function() pc:ClientTravel(KICK_LOBBY_URL, 0, false) end) end
            Log(string.format("[KICK] %s: backend %s — to the lobby (ClientTravel ok=%s), check in %d s", open, code,
                tostring(ok), KICK_CLOSE_SEC))
            GameDelay(KICK_CLOSE_SEC * 1000, function()
                ExecuteInGameThread(function()
                    pcall(function()
                        local live = LiveObject(pcPath)
                        local conn = live and live:IsValid() and RemoteConn(live)
                        if not conn then
                            Log("[KICK] " .. open .. ": left by himself")
                            return
                        end
                        -- 03.11 (Robot 09:01: 3 of 7 did not leave on ClientTravel): native server-side connection close —
                        -- UNetConnection::Close via dbnative; then the engine cleans up the controller itself (Logout)
                        local cfg = NativeDataConfig()
                        local rsp = (cfg and cfg.noclose) and { ok = "0", err = "noclose in native_data.txt" }
                            or NativeCall({ op = "closeconn", conn = string.format("0x%X", conn:GetAddress()) })
                        if rsp.ok == "1" then
                            Log(string.format("[KICK] %s: still on the server after %d s — connection closed (Close, state %s -> %s%s)",
                                open, KICK_CLOSE_SEC, tostring(rsp.state_before), tostring(rsp.state_after),
                                rsp.already == "1" and ", was already closed" or ""))
                        else
                            -- fallback (like AGameSession::KickPlayer): without a pawn and a controller there is no playing
                            local pawn = nil
                            pcall(function() pawn = live.Pawn end)
                            local okPawn = pawn and pawn:IsValid() and pcall(function() pawn:K2_DestroyActor() end)
                            pcall(function() live:ClientWasKicked(FText(reason)) end)
                            local okPc = pcall(function() live:K2_DestroyActor() end)
                            Log(string.format("[KICK] %s: closeconn failed (%s) — pawn deleted=%s, controller deleted=%s", open,
                                tostring(rsp.err), tostring(okPawn), tostring(okPc)))
                        end
                        GameDelay(KICK_CLOSE_SEC * 1000, function()
                            ExecuteInGameThread(function()
                                local left = LiveObject(pcPath)
                                local still = left and left:IsValid() and RemoteConn(left)
                                Log(string.format("[KICK] %s: %s", open, still and "connection still alive" or "no connection"))
                            end)
                        end)
                    end)
                end)
            end)
        end)
    end)
end

pcall(function()
    RegisterHook("/Script/LyraGame.BattleDsUtility:InitPlayerGameData", function(self, psParam)
        pcall(function()
            local ps = psParam:get()
            if not (ps and ps:IsValid()) then return end
            local open = ps.MyOpenId:ToString()
            if not open:match("^%d+$") then return end
            local name = ps:GetFName():ToString()
            local f = io.open(RebuildProbeDir .. "ps-openid.txt", "a")
            if f then f:write(name .. "=" .. open .. string.char(10)); f:close() end
            Log("[OPENID] " .. name .. " = " .. open .. " (before the native InitPlayerGameData)")

            local cfg = NativeDataConfig()
            if not cfg or ps.HasInitedPlayerData then return end
            -- 02.11: the player's connection — dbnative takes the token from his connection address and passes it to backend
            local conn = ""
            pcall(function() conn = string.format("0x%X", ps:GetOwner().Player:GetAddress()) end)
            local rsp = NativeCall({
                op = "swap",
                mgr = string.format("0x%X", self:get():GetAddress()),
                open_id = open,
                url = cfg.url,
                realkey = cfg.realkey and 1 or 0,
                conn = conn,
                -- 03.11: on a network error (no answer from backend) dbnative retries the request; on 403/409 — answer at once
                tries = SWAP_TRIES,
                pause_ms = SWAP_PAUSE_MS,
            })
            Log("[NATIVE] swap " .. name .. " " .. open .. ": ok=" .. tostring(rsp.ok) .. " " .. tostring(rsp.err or rsp.bytes or "")
                .. " (attempts " .. tostring(rsp.attempts or "?") .. ")")
            if rsp.ok == "1" then NativeDataPending[ps:GetAddress()] = cfg.keepopenid and "" or open end
            local team = tonumber(rsp.team_id or "")
            if team and team ~= 0 then TeamOfOpen[open] = team end
            -- 02.11/03.11: no own match data (any ok ~= 1: 409 raid_finished, 403 token, network after retries,
            -- dbnative error) — the player is kicked, not let in with someone else's LOCAL_GAME_MODE_OPENID record.
            -- The host (local controller) is left alone. How — see KickRemote.
            if rsp.ok ~= "1" and conn ~= "" then
                local code = tostring(rsp.err or ""):match("HTTP (%d+)") or "net"
                KickRemote(PathOf(ps), open, code, code == "409" and "Этот рейд для вас уже завершён" or "Нет допуска в этот рейд")
            end
        end)
    end, function(self, psParam)
        pcall(function()
            local ps = psParam:get()
            if not (ps and ps:IsValid()) then return end
            local addr = ps:GetAddress()
            local open = NativeDataPending[addr]
            if not open then return end
            NativeDataPending[addr] = nil
            local name = ps:GetFName():ToString()
            local inited = ps.HasInitedPlayerData == true
            local rsp = open ~= "" and NativeCall({ op = "setopenid", ps = string.format("0x%X", addr), open_id = open })
                or { ok = "skip" }
            Log(string.format("[NATIVE] after InitPlayerGameData %s: inited=%s, MyOpenId=%s (setopenid ok=%s %s)",
                name, tostring(inited), ps.MyOpenId:ToString(), tostring(rsp.ok), tostring(rsp.err or "")))
            -- LocalDev (native_data=1) does not touch heroes with this mark
            if inited then
                local f = io.open(RebuildProbeDir .. "ps-native.txt", "a")
                if f then f:write(name .. "=" .. open .. string.char(10)); f:close() end
            end
        end)
    end)
end)

local function R14Mode()
    local v = ModFlag("r14", "mode", "ue4ss/Mods/DungeonborneRuClient/r14_mode.txt", "Mods/DungeonborneRuClient/r14_mode.txt")
    return (v or ""):match("^(%a+)") or "all"
end

-- Whether this process is the server for the pawn's world: there is a live GameMode on the same map (a client in a match has no GameMode).
-- Object names only: 31.10 KismetSystemLibrary:IsServer(pawn) from NotifyOnNewObject (the pawn is still
-- being constructed) coincided with crashes inside UE4SS.dll.
local function MapOfName(full)
    return (full or ""):match("%s(/[^%.%s]+)%.[^:]+:PersistentLevel") or ""
end

-- 31.10: without FindAllOf — calling it from NotifyOnNewObject (the object is still being constructed) coincided with UE4SS.dll crashes.
-- Maps with a GameMode are remembered when the GameMode is created (it appears in the world before pawns).
local GameModeMaps = {}
NotifyOnNewObject("/Script/Engine.GameModeBase", function(gm)
    pcall(function()
        local full = gm:GetFullName()
        if full:find("Default__", 1, true) then return end
        local map = MapOfName(full)
        if map ~= "" then GameModeMaps[map] = true end
    end)
end)

local function IsServerWorld(obj)
    local full = ""
    pcall(function() full = obj:GetFullName() end)
    local map = MapOfName(full)
    return map ~= "" and GameModeMaps[map] == true
end

-- The listen host's own pawn (server and local player in one process) — only it needs the listen workarounds:
-- charge release, the Relic branch, hiding souls. 31.10, the "listen as dedicated" scheme: a headless server has no own pawn
-- (the host is an observer), on a pure client the process is not a server → the workarounds do not run. Without this, on a client the release
-- workaround cancelled an RMB charge (it looks at LMB), and hiding souls on the server replicated and hid them for the DK.
-- Cache by pawn address: IsServerWorld goes over the GameModes, too expensive for every tick.
local ListenHostPawnCache = {}
local function IsListenHostPawn(pawn)
    local addr = nil
    pcall(function() addr = pawn:GetAddress() end)
    if not addr then return false end
    local cached = ListenHostPawnCache[addr]
    if cached ~= nil then return cached end
    local locally = false
    pcall(function() locally = pawn:IsLocallyControlled() end)
    -- before a controller possesses it, the pawn is not "own" — not cached, otherwise the listen host would lose the workarounds forever
    if not locally then return false end
    local result = IsServerWorld(pawn)
    ListenHostPawnCache[addr] = result
    return result
end

-- The process is a listen host with its own hero (for objects without a pawn, e.g. souls)
local function HasListenHostHero()
    for _, h in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        local ok, yes = pcall(function() return h:IsValid() and IsListenHostPawn(h) end)
        if ok and yes then return true end
    end
    return false
end

-- Make sure UTgfEquipmentManagerComponent exists on the pawn, so that r14 at 0x142f602c6 is never 0x0!
-- 31.10: add = false — check only (ChildCompCreated). Adding from PawnCreated (NotifyOnNewObject) three times
-- a day coincided with crash UE4SS.dll+0xba20e4; the "add from a UCS pre-hook" variant gave another client crash
-- (UE4SS.dll+0x3e8895) — PawnCreated is kept as the one that passed most runs. A real fix is needed (BACKLOG R14).
-- temp mode: temporary stubs by the pawn's short name. When another (native,
-- replicated) TgfEquipmentManagerComponent arrives on the same pawn — after 1 s we remove the stubs so that GetComponentByClass returns
-- the native one (with items). Removal — not from NotifyOnNewObject, but deferred in the game thread.
local TempStubs = {}
if R14Mode() == "temp" then NotifyOnNewObject("/Script/LyraGame.TgfEquipmentManagerComponent", function(comp)
    local full = ""
    pcall(function() full = comp:GetFullName() end)
    if full:find("Default__", 1, true) then return end
    local pawnName, compName = full:match("%.([%w_]+)%.([%w_]+)$")
    local stub = pawnName and TempStubs[pawnName]
    if not stub or compName == stub.EqName or stub.Removing then return end
    stub.Removing = true
    local waited = os.clock() - stub.Since
    GameDelay(1000, function()
        ExecuteInGameThread(function()
            local okEq = pcall(function() if stub.Eq:IsValid() then stub.Eq:K2_DestroyComponent(stub.Eq) end end)
            local okInv = pcall(function() if stub.Inv and stub.Inv:IsValid() then stub.Inv:K2_DestroyComponent(stub.Inv) end end)
            local now = "?"
            pcall(function()
                now = ShortName(stub.Pawn:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")))
            end)
            Log(string.format("[R14_TEMP] %s: native %s arrived after %.2f s, stubs removed (eq=%s inv=%s) -> GetByClass=%s",
                pawnName, compName, waited, tostring(okEq), tostring(okInv), now))
            TempStubs[pawnName] = nil
        end)
    end)
end) end -- only in temp mode (r14_mode is read when the mod loads)

local function EnsureEquipmentManagerOnPawn(pawn, src, add)
    if not pawn then return end
    -- defer: no stubs anywhere — the crash is covered by the deferred avatar (see PendingAvatars below). Also here: no
    -- function calls on the pawn from NotifyOnNewObject (including the check GetComponentByClass).
    if R14Mode() == "defer" then return end
    if add == false then
        pcall(function()
            local eqCls = StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
            local existing = pawn:GetComponentByClass(eqCls)
            Log(string.format("  [R14_CHECK:%s] Pawn=%s EquipMgr=%s", tostring(src), ShortName(pawn), ShortName(existing)))
        end)
        return
    end
    local mode = R14Mode()
    if mode == "off" then Log("  [R14_OFF:" .. tostring(src) .. "] " .. ShortName(pawn)); return end
    if (mode == "client" or mode == "temp") and IsServerWorld(pawn) then Log("  [R14_SKIP_SERVER:" .. tostring(src) .. "] " .. ShortName(pawn)); return end
    pcall(function()
        if not pawn:IsValid() then return end
        local eqCls = StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
        if not eqCls or not eqCls:IsValid() then return end

        local existing = pawn:GetComponentByClass(eqCls)
        if existing and existing:IsValid() then
            Log(string.format("  [R14_CHECK:%s] Pawn=%s already has EquipMgr=%s", tostring(src), ShortName(pawn), ShortName(existing)))
            return
        end

        -- Try to add the component via AddComponentByClass
        local newComp = nil
        pcall(function()
            newComp = pawn:AddComponentByClass(eqCls, false, {
                Rotation = {X = 0.0, Y = 0.0, Z = 0.0, W = 1.0},
                Translation = {X = 0.0, Y = 0.0, Z = 0.0},
                Scale3D = {X = 1.0, Y = 1.0, Z = 1.0}
            }, false)
        end)

        local checkAfter = pawn:GetComponentByClass(eqCls)
        Log(string.format("  [R14_FIX:%s] Added UTgfEquipmentManagerComponent on %s -> Result=%s (GetByClass=%s)",
            tostring(src), ShortName(pawn), ShortName(newComp), ShortName(checkAfter)))

        -- Also make sure UTgfInventoryManagerComponent exists on the pawn
        local invStub = nil
        local invCls = StaticFindObject("/Script/LyraGame.TgfInventoryManagerComponent")
        if invCls and invCls:IsValid() then
            local exInv = pawn:GetComponentByClass(invCls)
            if not exInv or not exInv:IsValid() then
                pcall(function()
                    invStub = pawn:AddComponentByClass(invCls, false, {
                        Rotation = {X = 0.0, Y = 0.0, Z = 0.0, W = 1.0},
                        Translation = {X = 0.0, Y = 0.0, Z = 0.0},
                        Scale3D = {X = 1.0, Y = 1.0, Z = 1.0}
                    }, false)
                end)
            end
        end
        if mode == "temp" and newComp then
            TempStubs[ShortName(pawn)] = { Pawn = pawn, Eq = newComp, Inv = invStub, EqName = ShortName(newComp), Since = os.clock() }
        end
    end)
end

local function SafeHook(funcPath)
    if not funcPath or HookedMap[funcPath] then return end
    local ok = pcall(function()
        RegisterHook(funcPath, function(self) end, function(self) end)
    end)
    if ok then
        HookedMap[funcPath] = true
    end
end

local function HookSpecificClasses()
    local targetClasses = {
        "/Script/LyraGame.TgfAvatarHelperComponent",
        "/Script/LyraGame.TgfAvatarActor",
        "/Script/LyraGame.TgfCharacter",
        "/Script/Engine.ChildActorComponent",
        "/Game/TGF/Character/Suit/BP_Avatar_Default.BP_Avatar_Default_C",
        "/Game/TGF/Character/Shared/Cosmetic/BP_AvatarActor.BP_AvatarActor_C",
        "/Game/TGF/Character/Hero/BP_Tgf_Character_Default.BP_Tgf_Character_Default_C",
        "/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C",
        "/Game/TGF/Character/Hero/Fighter/HeroSample/BP_Fighter_HeroSample.BP_Fighter_HeroSample_C"
    }
    for _, clsPath in ipairs(targetClasses) do
        pcall(function()
            local cls = StaticFindObject(clsPath)
            if cls and cls:IsValid() then
                cls:ForEachFunction(function(fn)
                    local full = fn:GetFullName()
                    local path = full:match("^Function%s+(.+)$")
                    if path and not path:find("ReceiveTick") then
                        SafeHook(path)
                    end
                end)
            end
        end)
    end
end

-- The ability spec in ASC.ActivatableAbilities.Items (CurrentSpecHandle of UGameplayAbility is not reflected).
-- Returns Handle (int), InputPressed, the list of DynamicAbilityTags tags (InputTag.* is there).
-- Instances are compared by GetAddress(): UE4SS creates different Lua wrappers for one UObject, and "==" does not equate them
-- (because of this the first version had spec=nil).
local function FindChargeSpec(asc, ability)
    local found = nil
    local want = ability:GetAddress()
    local ok, err = pcall(function()
        asc.ActivatableAbilities.Items:ForEach(function(_, elem)
            if found then return end
            local spec = elem:get()
            local match = false
            for _, arrName in ipairs({ "ReplicatedInstances", "NonReplicatedInstances" }) do
                pcall(function()
                    spec[arrName]:ForEach(function(_, inst)
                        if inst:get():GetAddress() == want then match = true end
                    end)
                end)
            end
            if match then
                local tags = {}
                pcall(function()
                    spec.DynamicAbilityTags.GameplayTags:ForEach(function(_, t)
                        tags[#tags + 1] = t:get().TagName:ToString()
                    end)
                end)
                found = { Handle = spec.Handle.Handle, InputPressed = spec.InputPressed, Tags = tags }
            end
        end)
    end)
    if not ok then Log("    FindChargeSpec error: " .. tostring(err)) end
    return found
end

local PendingRpcFallback = nil -- { Ability, Asc, Handle, At }
local RpcTriedAbility = nil

local function TriggerActiveChargeRelease(reason)
    if not ActiveChargeAbility then return end
    pcall(function()
        if not ActiveChargeAbility:IsValid() then
            ActiveChargeAbility = nil
            return
        end
        if ActiveChargeAbility.ChargeReleased == false then
            local held = math.max(0.05, os.clock() - ChargeStartClock)
            if CHARGE_RELEASE_MODE == "rpc" then
                -- one attempt per activation (otherwise ReceiveTick would call it every frame)
                if RpcTriedAbility and RpcTriedAbility:GetAddress() == ActiveChargeAbility:GetAddress() then return end
                RpcTriedAbility = ActiveChargeAbility
                -- First give the native path a chance: in standalone (localbattle) the ability releases itself within 0.05 s,
                -- and the mod does nothing. Not released (listen host) — OnRelease_ in ReceiveTick.
                PendingRpcFallback = { Ability = ActiveChargeAbility, Held = held, Reason = reason, At = os.clock() + 0.05, Stage = 1 }
            else
                Log(string.format(">>> FORCE_RELEASE(%s) held=%.2fs", reason, held))
                pcall(function() ActiveChargeAbility:OnRelease_EA7AD5B24572E8EDB0D634A9215200E9(held) end)
                if ActiveChargeAbility.ChargeReleased == false then
                    pcall(function() ActiveChargeAbility:ProcessRelease(held) end)
                end
            end
        end
    end)
end

-- Heavy/charge melee attacks: all inherit GA_TGF_MeleeWeapon_Charge_Base_C (Relic — via Charge_Enchant_Base),
-- OnRelease_EA7AD5B2…(TimeHeld) and ProcessRelease(TimeHeld) are declared in the base and shared by all.
-- On a listen host WaitInputRelease does not get the LMB release — we do the release ourselves (ReceiveTick -> TriggerActiveChargeRelease).
-- The hook is set on every class: LongSword/MaceShield override K2_ActivateAbility, and the base hook does not fire for them.
-- The class list — from DumpGame/UE4SS_SDK (descendants of Charge_Base / Charge_Enchant_Base).
local ChargeAbilityClasses = {
    "/Game/TGF/Ability/GameAbility/MeleeWeapon/GA_TGF_MeleeWeapon_Charge_Base.GA_TGF_MeleeWeapon_Charge_Base_C",
    "/Game/TGF/Ability/GameAbility/MeleeWeapon/GA_TGF_MeleeWeapon_Charge_Enchant_Base.GA_TGF_MeleeWeapon_Charge_Enchant_Base_C",
    "/Game/TGF/Equipment/Weapon/Sword/GameAbility/Iteration/GA_MeleeWeapon_Sword_Heavy_Iteration.GA_MeleeWeapon_Sword_Heavy_Iteration_C",
    "/Game/TGF/Equipment/Weapon/Mace/GameAbility/Iteration/GA_MeleeWeapon_Mace_Heavy_Iteration.GA_MeleeWeapon_Mace_Heavy_Iteration_C",
    "/Game/TGF/Equipment/Weapon/Dagger/GameAbility/GA_MeleeWeapon_Dagger_Charge.GA_MeleeWeapon_Dagger_Charge_C",
    "/Game/TGF/Equipment/Weapon/LongSword/GameAbility/GameAbilityIteration/GA_MeleeWeapon_LongSword_Heavy_Iteration.GA_MeleeWeapon_LongSword_Heavy_Iteration_C",
    "/Game/TGF/Equipment/WeaponPair/SwordShield/GameAbility/Iteration/GA_MeleeWeapon_SwordShield_Heavy_Iteration.GA_MeleeWeapon_SwordShield_Heavy_Iteration_C",
    "/Game/TGF/Equipment/WeaponPair/MaceShield/GameAbility/Iteration/GA_MeleeWeapon_MaceShield_Heavy_Iteration.GA_MeleeWeapon_MaceShield_Heavy_Iteration_C",
    "/Game/TGF/Equipment/WeaponPair/SwordSword/GameAbility/V2/GA_MeleeWeapon_SwordSword_Heavy_V2.GA_MeleeWeapon_SwordSword_Heavy_V2_C",
    "/Game/TGF/Equipment/WeaponPair/DaggerDagger/GameAbility/GA_MeleeWeapon_DaggerDagger_Heavy.GA_MeleeWeapon_DaggerDagger_Heavy_C",
    "/Game/TGF/Equipment/WeaponPair/SwordRelic/GameAbility/GA_MeleeWeapon_SwordRelic_Enchant.GA_MeleeWeapon_SwordRelic_Enchant_C",
    "/Game/TGF/Equipment/WeaponPair/MaceRelic/GameAbility/GA_MeleeWeapon_MaceRelic_Enchant.GA_MeleeWeapon_MaceRelic_Enchant_C",
    "/Game/TGF/Equipment/WeaponPair/DaggerRelic/GameAbliity/ver03/GA_MeleeWeapon_DaggerRelic_Enchant_03.GA_MeleeWeapon_DaggerRelic_Enchant_03_C",
}

-- Short class names (GA_..._C) to recognise an instance by GetFullName()
local ChargeAbilityNames = {}
for _, p in ipairs(ChargeAbilityClasses) do
    ChargeAbilityNames[#ChargeAbilityNames + 1] = p:match("%.([%w_]+)$")
end

local function IsChargeAbilityName(fullName)
    for _, n in ipairs(ChargeAbilityNames) do
        if fullName:find(n, 1, true) then return true end
    end
    return false
end

-- Hook ONLY the start/end of the ability (before — all GA functions, including ExecuteUbergraph and OnRelease_*).
-- The GA distance flags are set in NotifyOnNewObject(GA_TGF_Base_C) below, without hooks on functions.
local function SafeCombatHook(funcPath)
    if not funcPath or CombatHookedMap[funcPath] then return end
    local shortName = funcPath:match(":([^:]+)$") or ""
    if shortName ~= "K2_ActivateAbility" and shortName ~= "K2_OnEndAbility" then
        return
    end
    local ok = pcall(function()
        RegisterHook(funcPath, function(self)
            local selfObj = nil
            local selfName = "nil"
            pcall(function()
                selfObj = self:get()
                selfName = selfObj:GetFullName()
            end)
            if not selfObj then return end

            -- Before: the substring "Heavy"/"Charge" — did not catch Relic (*_Enchant). Now — by the class list.
            if IsChargeAbilityName(selfName) then
                if shortName == "K2_ActivateAbility" then
                    ActiveChargeAbility = selfObj
                    -- 01.11: whose hero this is — the release workaround in ReceiveTick touches only the ability of its own pawn
                    -- (on a headless server the host's hidden pawn is alive: before, it "released" the client's charge after 0.1 s)
                    ChargeAvatarAddr = nil
                    pcall(function() ChargeAvatarAddr = selfObj:GetAvatarActorFromActorInfo():GetAddress() end)
                    ChargeStartClock = os.clock()
                    RpcTriedAbility = nil -- an InstancedPerActor instance is reused between strikes
                    ProjectileFiredAddr = nil
                    RelicBranchAddr = nil
                    Log(">>> CHARGE_START " .. (selfName:match("(GA_[%w_]+_C)") or selfName))
                elseif shortName == "K2_OnEndAbility" or shortName == "DeInitParams" then
                    ActiveChargeAbility = nil
                end
            end
        end)
    end)
    if ok then
        CombatHookedMap[funcPath] = true
    end
end

-- Trace (CHARGE_TRACE): a compact line per call, arguments — number/bool/FName/object name
local TraceSeq = 0
local TraceHooked = {}
local ProjectileNotifyInstalled = false

local function ParamStr(p)
    local ok, v = pcall(function() return p:get() end)
    if not ok then return "?" end
    local t = type(v)
    if t == "number" then return string.format("%.4g", v) end
    if t == "boolean" or t == "string" or t == "nil" then return tostring(v) end
    local s = nil
    pcall(function() s = v:ToString() end)
    if not s then pcall(function() s = v:GetFName():ToString() end) end
    return s or t
end

local function EnsureTraceHooks()
    if not CHARGE_TRACE then return end
    for _, clsPath in ipairs(ChargeTraceClasses) do
        if not TraceHooked[clsPath] then
            pcall(function()
                local cls = StaticFindObject(clsPath)
                if not (cls and cls:IsValid()) then return end
                TraceHooked[clsPath] = true
                local clsShort = clsPath:match("%.GA_([%w_]+)_C$") or "?"
                cls:ForEachFunction(function(fn)
                    local path = fn:GetFullName():match("^Function%s+(.+)$")
                    local fname = path and path:match(":([^:]+)$")
                    if not fname or fname == "UpdateShine" then return end
                    pcall(function()
                        RegisterHook(path, function(self, ...)
                            TraceSeq = TraceSeq + 1
                            local args = {}
                            for _, p in ipairs({ ... }) do args[#args + 1] = ParamStr(p) end
                            -- For EventReceived_* the tag and the event payload matter (to repeat it on listen)
                            if fname:find("^EventReceived_") then
                                local first = ({ ... })[1]
                                pcall(function()
                                    local p = first:get()
                                    local parts = { "tag=" .. p.EventTag.TagName:ToString() }
                                    local function obj(name)
                                        local o = nil
                                        pcall(function() o = p[name] end)
                                        local s = "nil"
                                        pcall(function() if o and o:IsValid() then s = ShortName(o) .. "<" .. ShortName(o:GetClass()) .. ">" end end)
                                        parts[#parts + 1] = name .. "=" .. s
                                    end
                                    obj("Instigator"); obj("Target"); obj("OptionalObject"); obj("OptionalObject2")
                                    pcall(function() parts[#parts + 1] = string.format("Magnitude=%.3f", p.EventMagnitude) end)
                                    pcall(function() parts[#parts + 1] = "TargetData#=" .. tostring(#p.TargetData.Data) end)
                                    args[1] = table.concat(parts, " ")
                                end)
                            end
                            local rel = "?"
                            pcall(function() rel = tostring(self:get().ChargeReleased) end)
                            Log(string.format("[#%d] %s:%s(%s) rel=%s t=%.2f", TraceSeq, clsShort, fname,
                                table.concat(args, ","), rel, os.clock() - ChargeStartClock))
                        end)
                    end)
                end)
                Log("TRACE hooked " .. clsShort)
            end)
        end
    end
    -- The frost wave projectile of Relic: did it appear (and by whom/when)
    if not ProjectileNotifyInstalled then
        local ok = pcall(function()
            NotifyOnNewObject("/Game/TGF/Equipment/WeaponPair/SwordRelic/GameAbility/B_TGF_Normal_Projectile_SwordRelicIce.B_TGF_Normal_Projectile_SwordRelicIce_C",
                function(Proj)
                    local role = "?"
                    pcall(function() role = tostring(Proj.Role) end)
                    Log(string.format("*** PROJECTILE SwordRelicIce spawned role=%s t=%.2f", role, os.clock() - ChargeStartClock))
                end)
        end)
        if ok then ProjectileNotifyInstalled = true end
    end
end

-- The Relic wave (sword/mace/dagger with a relic) on a listen host. BP bytecode analysis (retoc + kismet-analyzer, 30.10):
--   Charge_Enchant_Base (server): on activation spawns a hidden projectile in advance, waits for GameplayEvent.SpawnParamReady,
--     on it checks Hero.SpawnParam (VerifyServerSpanwProjectilePos) and ProcessProjectile moves the projectile there.
--   <Relic>_Enchant.DealCastCharge: Super; if HasAuthority and NOT IsStandalone -> return; otherwise (the "client" branch)
--     computes the position from the camera (GetSpawnOffsetPos + Get Adjust Collision Spawn Pos) and calls
--     Hero.SpawnProjectileWithClientParam -> SpawnParam = position, SendGameplayEvent(SpawnParamReady).
--   On dedicated the return is correct (the client sends the position by RPC), but a listen host for ITS OWN hero is a server and not standalone:
--   nobody runs the branch. Fix: after DealCastCharge of the own hero on a listen host we call the same BP branch —
--   ExecuteUbergraph_<class>(the branch entry point). Other heroes on the host (their clients send the RPC themselves) are left alone.
-- UE4SS hooks on BP functions fire AFTER execution (visible from the trace order).
local ENCHANT_BASE = "/Game/TGF/Ability/GameAbility/MeleeWeapon/GA_TGF_MeleeWeapon_Charge_Enchant_Base.GA_TGF_MeleeWeapon_Charge_Enchant_Base_C"
-- entry point of the "client" branch in ExecuteUbergraph — the block after the IsStandalone check (from the kismet-analyzer CFG)
local RELIC_CLIENT_BRANCH = {
    -- the BP patch (bp_patches, IsStandalone -> IsLocallyControlled) removed 30.10: with a second player charge attacks got cancelled
    { cls = "/Game/TGF/Equipment/WeaponPair/SwordRelic/GameAbility/GA_MeleeWeapon_SwordRelic_Enchant.GA_MeleeWeapon_SwordRelic_Enchant_C",
      ubergraph = "ExecuteUbergraph_GA_MeleeWeapon_SwordRelic_Enchant", entry = 104 },
    { cls = "/Game/TGF/Equipment/WeaponPair/MaceRelic/GameAbility/GA_MeleeWeapon_MaceRelic_Enchant.GA_MeleeWeapon_MaceRelic_Enchant_C",
      ubergraph = "ExecuteUbergraph_GA_MeleeWeapon_MaceRelic_Enchant", entry = 89 },
    -- the _03_6002 descendant has no DealCastCharge of its own — the parent's hook catches it
    { cls = "/Game/TGF/Equipment/WeaponPair/DaggerRelic/GameAbliity/ver03/GA_MeleeWeapon_DaggerRelic_Enchant_03.GA_MeleeWeapon_DaggerRelic_Enchant_03_C",
      ubergraph = "ExecuteUbergraph_GA_MeleeWeapon_DaggerRelic_Enchant_03", entry = 89 },
}
local ProjectileHookInstalled = false

local function IsListenHostOwnHero(ab)
    local actor = ab:GetAvatarActorFromActorInfo()
    if not (actor and actor:IsValid()) then return false end
    local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    return ab:K2_HasAuthority() and actor:IsLocallyControlled() and not ksl:IsStandalone(actor)
end

local function EnsureRelicProjectileHooks()
    if not ProjectileHookInstalled then
        local cls = StaticFindObject(ENCHANT_BASE)
        if cls and cls:IsValid() then
            ProjectileHookInstalled = pcall(function()
                RegisterHook(ENCHANT_BASE .. ":ProcessProjectile", function(self)
                    pcall(function() ProjectileFiredAddr = self:get():GetAddress() end)
                end)
            end)
        end
    end
    -- relic classes load on equip — hook them as they appear
    for _, r in ipairs(RELIC_CLIENT_BRANCH) do
        if not r.hooked then
            local cls = StaticFindObject(r.cls)
            if cls and cls:IsValid() then
                r.hooked = pcall(function()
                    RegisterHook(r.cls .. ":DealCastCharge", function(self)
                        pcall(function()
                            local ab = self:get()
                            local addr = ab:GetAddress()
                            if RelicBranchAddr == addr then return end -- the branch was already called in this activation
                            if not IsListenHostOwnHero(ab) then return end
                            RelicBranchAddr = addr
                            local ok, err = pcall(function() ab[r.ubergraph](ab, r.entry) end)
                            -- the branch synchronously sends SpawnParamReady -> EventReceived_F72DFA85 -> ProcessProjectile
                            Log(string.format("    RELIC listen: %s(%d) ok=%s %s -> ProcessProjectile %s", r.ubergraph, r.entry,
                                tostring(ok), ok and "" or tostring(err),
                                ProjectileFiredAddr == addr and "called" or "NOT called (position failed the server check?)"))
                        end)
                    end)
                end)
                Log(string.format("RELIC hook %s DealCastCharge=%s", r.cls:match("([^%.]+)$"), tostring(r.hooked)))
            end
        end
    end
end

local function EnsureChargeHooks()
    EnsureTraceHooks()
    EnsureRelicProjectileHooks()
    local weapons = FindAllOf("B_WeaponBase_C")
    if weapons then
        for _, w in ipairs(weapons) do
            pcall(function() w.UseHitValidation = false end)
        end
    end
    if CHARGE_RELEASE_MODE == "native" then return end

    -- Charge classes only. A class is hooked once it is loaded (StaticFindObject),
    -- so the function is called repeatedly: on hero creation and every 2 s from ReceiveTick.
    for _, clsPath in ipairs(ChargeAbilityClasses) do
        pcall(function()
            local cur = StaticFindObject(clsPath)
            if cur and cur:IsValid() then
                local clsFull = cur:GetFullName()
                if not HookedCombatClasses[clsFull] then
                    HookedCombatClasses[clsFull] = true
                    cur:ForEachFunction(function(fn)
                        local path = fn:GetFullName():match("^Function%s+(.+)$")
                        if path then SafeCombatHook(path) end
                    end)
                end
            end
        end)
    end
end

-- Server-side distance check flags for all abilities (used to be set from a hook on every GA function).
-- NotifyOnNewObject also fires for descendants of GA_TGF_Base_C (instances are created on grant/activation).
NotifyOnNewObject("/Game/TGF/Ability/GameAbility/GA_TGF_Base.GA_TGF_Base_C", function(Ability)
    pcall(function()
        Ability.UseLooseServerDistCheck = false
        Ability.CloseLooseServerDistCheck = true
    end)
end)

-- Reference map rules from map_detail.json (option_args + dynamic_option_args per group of up to 3 players)
local MapRulesByLevelName = {
    L_Dungeon_Official_01         = { MapId = 10101, MatchType = 1, Options = { 5001, 10101, 1010103 }, Duration = 2400.0 },
    L_Dungeon_Official_01_Classic = { MapId = 10102, MatchType = 1, Options = { 5001, 10102, 1010203 }, Duration = 2400.0 },
    L_Dungeon_Official_01_Hard    = { MapId = 10103, MatchType = 1, Options = { 5001, 10103, 1010303 }, Duration = 2400.0 },
    L_GothicCastle_WP_Main         = { MapId = 10201, MatchType = 1, Options = { 5002, 10201, 1020103 }, Duration = 2400.0 },
    L_GothicCastle_WP_Main_Classic = { MapId = 10202, MatchType = 1, Options = { 5002, 10202, 1020203 }, Duration = 2400.0 },
    L_GothicCastle_WP_Main_Hard    = { MapId = 10203, MatchType = 1, Options = { 5002, 10203, 1020303 }, Duration = 2400.0 },
    -- 06.11 second floor: one map for regular and Classic, so on the server MapId and options come from the task (ru-map-options.txt)
    L_Dungeon_Official_02_Hard        = { MapId = 10103, MatchType = 1, Options = { 5001, 10103, 1010303 }, Duration = 2400.0, Floor2 = true },
    L_Dungeon_Official_02             = { MapId = 10101, MatchType = 1, Options = { 5001, 10101, 1010103 }, Duration = 2400.0, Floor2 = true },
    L_GothicCastle_WP_Main_SecondFloor_Hard = { MapId = 10203, MatchType = 1, Options = { 5002, 10203, 1020303 }, Duration = 2400.0, Floor2 = true },
    L_GothicCastle_WP_Main_SecondFloor = { MapId = 10201, MatchType = 1, Options = { 5002, 10201, 1020103 }, Duration = 2400.0, Floor2 = true },
    -- 05.11 20:4x: arenas (L_Common_PVP0X_*) removed from here — MapId on an arena broke the PVP mode (zone, "modulo 0"); FIX_LIST R1-loot
}

local function DetectMapRules(obj)
    local full = ""
    pcall(function() full = obj:GetFullName() end)
    -- Check from the longest names (_Classic / _Hard), so as not to confuse them with the base map name
    local orderedNames = {
        "L_Dungeon_Official_02_Hard",
        "L_Dungeon_Official_02",
        "L_GothicCastle_WP_Main_SecondFloor_Hard",
        "L_GothicCastle_WP_Main_SecondFloor",
        "L_Dungeon_Official_01_Classic",
        "L_Dungeon_Official_01_Hard",
        "L_Dungeon_Official_01",
        "L_GothicCastle_WP_Main_Classic",
        "L_GothicCastle_WP_Main_Hard",
        "L_GothicCastle_WP_Main",
    }
    for _, name in ipairs(orderedNames) do
        if full:find(name, 1, true) then
            return name, MapRulesByLevelName[name]
        end
    end
    return nil, nil
end

-- 09.11: the map options file is per server (RuServer writes ru-map-options-<port>.txt, 7777 — without the port)
local MapOptionsFile = nil
local function MapOptionsPath()
    if not MapOptionsFile then
        local Tag = ""
        pcall(function()
            local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
            local port = tonumber(ksl:GetCommandLine():ToString():lower():match("%-port=(%d+)") or "")
            if port and port ~= 7777 then Tag = "-" .. port end
        end)
        MapOptionsFile = "ru-map-options" .. Tag .. ".txt"
    end
    return MapOptionsFile
end

local function ApplyStandardMapRulesToGameState(gs, src)
    if not gs then return end
    pcall(function()
        if not gs:IsValid() then return end
        local mapName, rules = DetectMapRules(gs)
        if not rules then return end

        if rules.Floor2 and IsServerProc then
            pcall(function()
                local F = io.open(MapOptionsPath(), "r")
                if not F then return end
                local Text = F:read("*a") or ""
                F:close()
                local Id = tonumber(Text:match("map=(%d+)") or "")
                if Id then
                    rules = { MapId = Id, MatchType = rules.MatchType, Duration = rules.Duration, Options = { rules.Options[1], Id, Id * 100 + 3 } }
                end
            end)
        end
        gs.MapId = rules.MapId
        -- 06.11: on the server — options by group size from the backend task (RuServer writes ru-map-options.txt): they determine
        -- the number and type of portals and the start sets. Only this map's options are taken (1010201 -> map 10102), otherwise — trio as before.
        local Opts = rules.Options
        if IsServerProc then
            pcall(function()
                local F = io.open(MapOptionsPath(), "r")
                if not F then return end
                local Line = F:read("*l") or ""
                F:close()
                local Own = { rules.Options[1], rules.MapId }
                for V in Line:gmatch("%d+") do
                    if math.floor(tonumber(V) / 100) == rules.MapId then Own[#Own + 1] = tonumber(V) end
                end
                if #Own > 2 then Opts = Own end
            end)
        end
        if rules.MapOnly then
            Log(string.format("[MAP_RULES:%s] %s -> MapId=%d (arena: loot map only)", tostring(src), mapName, rules.MapId))
            return
        end
        gs:SetMatchType(rules.MatchType)

        -- 01.11: options assigned by the game are left alone — spawn rooms use them to decide whom to spawn
        -- (BP_ActorSpawnOptionComponent.CanSpawn -> GameState.HasAllOptions). Before, all options were removed here and
        -- 5001/<map>/<map>03 were set (the three-player variant) — regular mobs stopped spawning, only
        -- bosses and dangerous rooms remained. Reference ones — only if by the start of Playing the GameState has no options at all
        -- (not earlier: options added when the GameState was created apparently cancelled the native AssignOptionIds).
        local curLen = #gs.OptionIds
        local cur = {}
        for i = 1, curLen do cur[#cur + 1] = tostring(gs.OptionIds[i]) end
        -- map_rules_keep.txt = 1 — the mode above (leave the game's options alone); without the file — as on 31.10: remove all, set the reference ones
        local keep = (ModFlag("map", "rules_keep", "ue4ss/Mods/DungeonborneRuClient/map_rules_keep.txt",
            "Mods/DungeonborneRuClient/map_rules_keep.txt") or ""):match("^1") ~= nil
        local added = false
        if not keep then
            for _, oldId in ipairs(cur) do
                pcall(function() gs:RemoveOption(tonumber(oldId)) end)
            end
        end
        if not keep or (curLen == 0 and src == "Playing_Activate") then
            for _, optId in ipairs(Opts) do
                if not gs:HasOption(optId) then
                    gs:AddOption(optId)
                end
            end
            added = true
        end

        Log(string.format("[MAP_RULES:%s] %s -> MapId=%d, MatchType=%d, game options=[%s]%s",
            tostring(src), mapName, rules.MapId, rules.MatchType, table.concat(cur, ","),
            added and (" -> set [" .. table.concat(Opts, ",") .. "]" .. (keep and "" or " (as on 31.10)")) or ""))
    end)
end

local function ApplyRulesToAllGameStates(src)
    local states = FindAllOf("B_TGF_MatchGameState_C") or {}
    for _, gs in ipairs(states) do
        ApplyStandardMapRulesToGameState(gs, src)
    end
end

local GameStateHooked = false
local function EnsureGameStateHooks()
    if GameStateHooked then return end
    pcall(function()
        RegisterHook("/Game/B_TGF_MatchGameState.B_TGF_MatchGameState_C:AssignOptionIds",
            function(self) end,
            function(self)
                ApplyStandardMapRulesToGameState(self:get(), "PostAssignOptionIds")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/B_TGF_MatchGameState.B_TGF_MatchGameState_C:ReceiveBeginPlay",
            function(self)
                ApplyStandardMapRulesToGameState(self:get(), "PreBeginPlay")
            end,
            function(self)
                ApplyStandardMapRulesToGameState(self:get(), "PostBeginPlay")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/B_LyraGameMode.B_LyraGameMode_C:OnReady_77628FB54EFC0F1E298D21806990A1C0",
            function(self)
                ApplyRulesToAllGameStates("GM_OnReady_Pre")
            end,
            function(self)
                ApplyRulesToAllGameStates("GM_OnReady_Post")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/TGF/Experiences/Phases/Phase_TgfGame_WarmUp.Phase_TgfGame_WarmUp_C:K2_ActivateAbility",
            function(self)
                ApplyRulesToAllGameStates("WarmUp_Activate")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/TGF/Experiences/Phases/Phase_TgfGame_Playing.Phase_TgfGame_Playing_C:K2_ActivateAbility",
            function(self)
                ApplyRulesToAllGameStates("Playing_Activate")
            end
        )
    end)
    GameStateHooked = true
end

NotifyOnNewObject("/Script/LyraGame.LyraGameState", function(NewGS)
    EnsureGameStateHooks()
    ApplyStandardMapRulesToGameState(NewGS, "NewGameState")
end)

-- Team mode on a Listen server: true = Co-op (one team/squad with the Host), false = Free-For-All PvP
-- 02.11 (party, M2): teams.txt next to the mod:
--   party (default) — team by team_id from the player's match data (one party — one team, a solo player — his own): the native
--     ALyraPlayerState::SetGenericTeamId via dbnative op=setteam; team_id -> MyTeamID 1, 2, … in order of appearance.
--     The native assignment on listen gives everyone a new team in order of joining and ignores team_id (M1, 13:34: party 1/-1
--     and 2/-1). A player without team_id keeps the native team. The squad (MySquadID) is left alone — the native game has -1;
--   native — leave the teams alone (what the game sets — a [TEAMS] native line);
--   coop — everyone gets the "host's" team and squad (as before 02.11).
local COOP_SAME_TEAM = true
local function TeamsMode()
    local v = ModFlag("teams", "mode", "ue4ss/Mods/DungeonborneRuClient/teams.txt", "Mods/DungeonborneRuClient/teams.txt")
    return (v or ""):match("^(%a+)") or "party"
end
local TeamsLogged = {}
local TeamIndex, TeamNext = {}, 1 -- team_id -> MyTeamID

local ApplyPartyTeams

-- 02.11 (M1): the spawn point depends on the team (an ALyraPlayerStartSet is bound to a team, teammates take
-- points of the same set), while party set the team only in PreGetDefaultPawnClass — already after the start was chosen (the party
-- appeared in different places). ChoosePlayerStart is a BlueprintNativeEvent: the hook fires before the native choice; here
-- team_id by OpenId from the address (dbnative op=teamof — GET /ds/team_of on backend) and the team is set in advance.
-- 11.11: the team — anew for every match. TeamOfOpen lived for the whole server run: in the second and later matches the hook took
-- the number from the previous match and set nothing, both DM players appeared in the center at one point (shared set team=-1).
-- Now the cache is reset when the GameState changes, the player's first start choice in a match asks backend, and the team is
-- always set on the start choice (force), even if the number already matches.
local TeamBattleKey, TeamAsked = nil, {}
pcall(function()
    RegisterHook("/Script/Engine.GameModeBase:ChoosePlayerStart", function(Ctx, PlayerParam)
        pcall(function()
            if TeamsMode() ~= "party" then return end
            local pc = PlayerParam:get()
            local ps = pc and pc.PlayerState
            if not (ps and ps:IsValid()) then return end
            local open = ps.MyOpenId:ToString()
            if not open:match("^%d+$") then return end
            local key = "?"
            pcall(function() key = string.format("%X", FindFirstOf("GameStateBase"):GetAddress()) end)
            if key ~= TeamBattleKey then
                TeamBattleKey, TeamAsked = key, {}
                TeamIndex, TeamNext = {}, 1
                for k in pairs(TeamOfOpen) do TeamOfOpen[k] = nil end
            end
            if not TeamAsked[open] then
                local cfg = NativeDataConfig()
                if not cfg then return end
                local rsp = NativeCall({ op = "teamof", open_id = open, url = cfg.url })
                local team = tonumber(rsp.team_id or "")
                if team and team ~= 0 then
                    TeamOfOpen[open] = team
                    TeamAsked[open] = true
                end
                Log(string.format("[TEAMS] ChoosePlayerStart %s: teamof ok=%s team_id=%s %s", open, tostring(rsp.ok),
                    tostring(rsp.team_id), tostring(rsp.err or "")))
            end
            ApplyPartyTeams({ ps }, "ChoosePlayerStart", true)
        end)
    end)
end)

ApplyPartyTeams = function(pStates, reason, force)
    for _, ps in ipairs(pStates) do
        pcall(function()
            if not ps:IsValid() then return end
            local open = ps.MyOpenId:ToString()
            local team = TeamOfOpen[open]
            if not team then return end
            -- 07.11: the room team number 1…253 from backend matches the FactionId of items (docs/ITEM_FLAG.md).
            -- Larger values — a party or player id outside a room: number in order of appearance, as before
            local idx = (team >= 1 and team <= 253) and team or TeamIndex[team]
            if not idx then
                idx = TeamNext
                TeamNext = TeamNext < 253 and TeamNext + 1 or 1 -- 254 — the team of the host observer (RuServer, 03.11)
                TeamIndex[team] = idx
            end
            local cur = ps:GetTeamId()
            if cur == idx and not force then return end
            local rsp = NativeCall({ op = "setteam", ps = string.format("0x%X", ps:GetAddress()), team = idx })
            Log(string.format("[TEAMS] party %s open_id=%s team_id=%s -> MyTeamID %s (was %s, %s) ok=%s %s", ShortName(ps), open,
                tostring(team), tostring(idx), tostring(cur), tostring(reason), tostring(rsp.ok), tostring(rsp.err or rsp.after or "")))
        end)
    end
end

-- Sync of the new player's (Client's) PlayerState on the Server and recount of live fighters in BP_DeathZone_C
local function SyncServerPlayerStatesAndDeathZone(reason)
    pcall(function()
        local pStates = FindAllOf("BP_TGF_PlayerState_C") or {}
        if #pStates == 0 then return end

        -- 1. Find the Host's reference PlayerState (which already has PawnData)
        local hostPS = nil
        for _, ps in ipairs(pStates) do
            if ps:IsValid() then
                local pd = nil
                pcall(function() pd = ps.PawnData end)
                if pd and pd:IsValid() then
                    hostPS = ps
                    break
                end
            end
        end

        -- 2. Initialize the PlayerStates of the connected clients
        for idx, ps in ipairs(pStates) do
            if ps:IsValid() and hostPS and ps ~= hostPS then
                pcall(function()
                    local curPD = ps.PawnData
                    if not curPD or not curPD:IsValid() then
                        ps.PawnData = hostPS.PawnData
                        pcall(function() ps:OnRep_PawnData() end)
                    end
                    -- 31.10: native_init.txt = 1 — the flag is not set, so that the native UBattleDsUtility::InitPlayerGameData
                    -- (called by the hero when the controller possesses it) does not bail out with "LyraPlayerState Has Inited!"
                    local nativeInit = (ModFlag("native", "init", "ue4ss/Mods/DungeonborneRuClient/native_init.txt",
                        "Mods/DungeonborneRuClient/native_init.txt") or ""):match("^1") ~= nil
                    if not nativeInit then ps.HasInitedPlayerData = true end
                    -- 06.11: only on the first initialization of this PlayerState. Before, the reset ran on every pass and erased the Shadow scene
                    -- a second after entering a purple portal (the native TgfPlayerGameFlow sets it)
                    SceneTagInited = SceneTagInited or {}
                    local sceneKey = string.format("%X", ps:GetAddress())
                    if not SceneTagInited[sceneKey] then
                        SceneTagInited[sceneKey] = true
                        ps:SetPlayerSceneTag(0) -- TgfPlayerSceneTagDefault
                    end

                    -- before overwriting: in coop mode — only the first (native) record, in native — every change
                    local key = string.format("%X|%s", ps:GetAddress(), ShortName(ps))
                    local now = tostring(ps:GetTeamId()) .. "/" .. tostring(ps:GetSquadId())
                    if TeamsLogged[key] == nil or (TeamsMode() == "native" and TeamsLogged[key] ~= now) then
                        TeamsLogged[key] = now
                        local open = ""
                        pcall(function() open = ps.MyOpenId:ToString() end)
                        Log(string.format("[TEAMS] native %s open_id=%s Team/Squad=%s (mode %s, %s)", ShortName(ps), open, now,
                            TeamsMode(), tostring(reason)))
                    end
                    if COOP_SAME_TEAM and TeamsMode() == "coop" then
                        local hostTeam = hostPS:GetTeamId()
                        local hostSquad = hostPS:GetSquadId()
                        ps.MyTeamID = { TeamID = hostTeam }
                        ps.MySquadID = hostSquad
                    end
                    Log(string.format("[GM_PLAYER:%s] Synced PS[%d]=%s | Team=%s | Squad=%s",
                        tostring(reason), idx, ShortName(ps),
                        tostring(ps:GetTeamId()), tostring(ps:GetSquadId())))
                end)
            end
        end

        -- 2b. Teams by team_id of the match data (teams.txt = party, default) — for all PS, including the "reference" one
        if TeamsMode() == "party" then ApplyPartyTeams(pStates, reason) end

        -- 3. Update AliveFighterNumber in BP_DeathZone_C by the number of live heroes (excluding the Host Observer)
        local heroes = FindAllOf("BP_Fighter_HeroSample_C") or {}
        local aliveHeroes = 0
        for _, h in ipairs(heroes) do
            if h:IsValid() then
                local dead = false
                local spec = false
                pcall(function() dead = h:IsDeadOrDying() end)
                pcall(function() spec = h:IsOnlySpectator() or h.bHidden end)
                if not dead and not spec then
                    aliveHeroes = aliveHeroes + 1
                end
            end
        end

        if aliveHeroes > 0 then
            local dzList = FindAllOf("BP_DeathZone_C") or {}
            for _, dz in ipairs(dzList) do
                if dz:IsValid() then
                    pcall(function()
                        if dz.AliveFighterNumber < aliveHeroes then
                            dz.AliveFighterNumber = aliveHeroes
                            Log(string.format("[DEATHZONE:%s] Updated AliveFighterNumber=%d", tostring(reason), aliveHeroes))
                        end
                    end)
                end
            end
        end
    end)
end

local GameModePlayerHooksInstalled = false
local function EnsureGameModePlayerHooks()
    if GameModePlayerHooksInstalled then return end
    pcall(function()
        RegisterHook("/Game/B_LyraGameMode.B_LyraGameMode_C:GetDefaultPawnClassForController",
            function(self, InController)
                SyncServerPlayerStatesAndDeathZone("PreGetDefaultPawnClass")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:OnReady_7E0A8FA9496C1EC097B3C591D67AA8E1",
            function(self)
                SyncServerPlayerStatesAndDeathZone("HeroOnReady")
            end
        )
    end)
    pcall(function()
        RegisterHook("/Game/TGF/Environment/DeathZone/BP_DeathZone.BP_DeathZone_C:OnReady_43E6AFBB4C533D213E9540B8F279A53C",
            function(self)
                ApplyRulesToAllGameStates("DeathZone_OnReady_Pre")
            end,
            function(self)
                local dz = self:get()
                pcall(function()
                    local preLen = #dz.PrecomputedDiscs
                    local p2 = (preLen >= 2) and #dz.PrecomputedDiscs[2].PortalSpawnInfos or -1
                    local p3 = (preLen >= 3) and #dz.PrecomputedDiscs[3].PortalSpawnInfos or -1
                    local p5 = (preLen >= 5) and #dz.PrecomputedDiscs[5].PortalSpawnInfos or -1
                    Log(string.format("[DEATHZONE_READY] PrecomputedDiscs=%d | Portals: Disc2=%d, Disc3=%d, Disc5=%d",
                        preLen, p2, p3, p5))
                end)
                SyncServerPlayerStatesAndDeathZone("DeathZone_OnReady_Post")
            end
        )
    end)
    GameModePlayerHooksInstalled = true
end

NotifyOnNewObject("/Script/LyraGame.LyraGameMode", function(NewGM)
    EnsureGameStateHooks()
    EnsureGameModePlayerHooks()
    pcall(function()
        local _, rules = DetectMapRules(NewGM)
        if rules then
            NewGM.MatchType = rules.MatchType
            NewGM.bCanSpawnActor = true
            NewGM.SingleMatchDuration = rules.Duration
        end
    end)
end)

-- 03.11 diagnostics "projectiles double" (DK hook E, sword wave): who creates projectile copies — the server or the client,
-- own (Role 3 Authority on the client = a local "fake" projectile) or replicated (Role 1 Simulated).
-- [PROJ] lines in the install's crash_ue4ss.log. The object is not touched at creation — after 0.2 s, by path (LiveObject).
-- 06.11: off by default. Client F: crashed at 09:57:37 (AV 0x0, ucrtbase -> UE4SS, fighting crossbowmen): the block reads after 0.2 s
-- an arrow object that may already be destroyed. Enable — file ru-projdiag.txt = 1 next to the exe.
local ProjDiag = { Proc = nil, N = 0, On = (function()
    return (ModFlag("diag", "projectiles", "ru-projdiag.txt") or ""):match("^1") ~= nil
end)() }
pcall(function()
    ClientNotify("/Script/LyraGame.TgfProjectileBase", function(Proj)
        if not ProjDiag.On or ProjDiag.N >= 400 then return end
        ProjDiag.N = ProjDiag.N + 1
        local path = PathOf(Proj)
        if path == "" or path:find("Default__", 1, true) then return end
        GameDelay(200, function()
            ExecuteInGameThread(function()
                local p = LiveObject(path)
                if not p then Log("[PROJ] " .. path:match("([^%.]+)$") .. " vanished within 0.2 s"); return end
                if not ProjDiag.Proc then
                    ProjDiag.Proc = "client"
                    pcall(function()
                        local cl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary"):GetCommandLine():ToString():lower()
                        if cl:find("%-dbserver") then ProjDiag.Proc = "server" end
                    end)
                end
                local role, rep, owner, inst = "?", "?", "?", "?"
                pcall(function() role = tostring(p.Role) end)
                pcall(function() rep = tostring(p.bReplicates) end)
                pcall(function() owner = ShortName(p:GetOwner()) end)
                pcall(function() inst = ShortName(p:GetInstigator()) end)
                Log(string.format("[PROJ] %s %s role=%s replicates=%s owner=%s instigator=%s", ProjDiag.Proc,
                    path:match("([^%.]+)$") or path, role, rep, owner, inst))
            end)
        end)
    end)
end)

-- on the server weapons are fixed by HeroFix in RuServer (by polling, without notifications)
ClientNotify("/Game/TGF/Equipment/Weapon/B_WeaponBase.B_WeaponBase_C", function(Weapon)
    -- 31.10: do not write into a half-built object — on the client the weapon is created when the equipment arrives, and a write
    -- from the notification coincided with crash UE4SS.dll+0x3e881b; we write on the next tick of the game thread
    ExecuteInGameThread(function()
        pcall(function() if Weapon:IsValid() then Weapon.UseHitValidation = false end end)
    end)
end)

-- When any ChildActorComponent is created on a hero, make sure the Owner has UTgfEquipmentManagerComponent!
-- R14 "defer" (31.10, a fix following Lyra: the component waits for data instead of slipping in a stub). Crash 0x142f602c6: on
-- the client the avatar (the hero's ChildActorComponent → BP_Avatar_Default_C) is created during the pawn's UCS and iterates armor slots
-- through the equipment manager, while the native manager (GameFeature, replicated) arrives later → null. Here: on
-- the client we remember the class of the avatar component and null ChildActorClass (the avatar is not created), and when the pawn
-- gets the manager — on the game thread we restore SetChildActorClass and update the body (OnRep_Avatar).
-- Inside NotifyOnNewObject only reading and one property write — function calls on half-built objects
-- coincided with UE4SS.dll crashes (+0xba20e4, +0x3e88xx).
-- Timeline of the hero body creation (31.10): which comes first — the avatar or the equipment manager, and where the avatar comes from
-- (a pawn component or an actor over the network). Only GetFullName (name + outer chain) — no function calls and no reading
-- of properties of half-built objects. Lines [BODY t=<ms>].
-- 31.10: the timeline is captured (BACKLOG, R14), disabled — extra load on UE4SS. Enable: true.
local BODY_TRACE = false
local BodyT0 = os.clock()
local function BodyLog(kind, obj)
    if not BODY_TRACE then return end
    local full = "?"
    pcall(function() full = obj:GetFullName() end)
    if not full:find("BP_Fighter_HeroSample_C", 1, true) and not full:find("Avatar", 1, true) then return end
    if full:find("Default__", 1, true) then return end
    Log(string.format("[BODY t=%d] %s %s", math.floor((os.clock() - BodyT0) * 1000), kind, full:sub(1, 230)))
end
-- only managers and avatars: notifications on TgfCharacterAvatarComponent caught every monster (dozens) —
-- extra load on UE4SS, after them came crash UE4SS.dll+0x2aee17
if BODY_TRACE then
    for _, path in ipairs({ "/Script/LyraGame.TgfAvatarActor", "/Script/LyraGame.TgfEquipmentManagerComponent" }) do
        pcall(function() NotifyOnNewObject(path, function(obj) BodyLog(path:match("%.([%w_]+)$"), obj) end) end)
    end
end

local PendingAvatars = {}   -- { Comp, Pawn, Since }
local AVATAR_CLASS = "/Game/TGF/Character/Suit/BP_Avatar_Default.BP_Avatar_Default_C"
local AVATAR_WAIT_SEC = 20  -- the manager never came — restore anyway (better a crash in the log than a pawn without a body)
local AvatarLoopStarted = false

local function StartAvatarLoop()
    if AvatarLoopStarted then return end
    AvatarLoopStarted = true
    GameLoop(100, function()
        if #PendingAvatars == 0 then return false end
        ExecuteInGameThread(function()
            local eqCls = StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
            for i = #PendingAvatars, 1, -1 do
                local p = PendingAvatars[i]
                local alive = false
                pcall(function() alive = p.Comp:IsValid() and p.Pawn:IsValid() end)
                if not alive then
                    table.remove(PendingAvatars, i)
                else
                    local mgr = nil
                    pcall(function() mgr = p.Pawn:GetComponentByClass(eqCls) end)
                    local ready = mgr ~= nil and mgr:IsValid()
                    local waited = os.clock() - p.Since
                    if ready or waited > AVATAR_WAIT_SEC then
                        table.remove(PendingAvatars, i)
                        local cls = StaticFindObject(AVATAR_CLASS)
                        local ok, err = pcall(function() p.Comp:SetChildActorClass(cls) end)
                        pcall(function() p.Pawn:OnRep_Avatar() end)
                        local child = "nil"
                        pcall(function() child = ShortName(p.Comp.ChildActor) end)
                        Log(string.format("[R14_DEFER] %s: avatar %s restored after %.2f s (manager %s) ok=%s%s -> ChildActor=%s",
                            ShortName(p.Pawn), ShortName(cls), waited, ready and ShortName(mgr) or "DID NOT ARRIVE", tostring(ok),
                            ok and "" or (" err=" .. tostring(err)), child))
                    end
                end
            end
        end)
        return false
    end)
end

ClientNotify("/Script/Engine.ChildActorComponent", function(Comp)
    pcall(function()
        local fullName = Comp:GetFullName()
        if fullName:find("BP_Fighter_HeroSample_C", 1, true) then
            local owner = Comp:GetOwner()
            local role = owner and owner:IsValid() and owner.Role or -1
            local cName = ShortName(Comp)
            Log(string.format("[CHILD_COMP] %s | Owner=%s | Role=%s", cName, ShortName(owner), tostring(role)))
            BodyLog("ChildActorComponent", Comp)
            if not (owner and owner:IsValid()) then return end
            if R14Mode() == "defer" then
                -- 31.10: READING ChildActorClass here (the component is still being constructed) crashed the server in UE4SS.dll —
                -- we read nothing, only write nil. The avatar is not TgfTileTarget (a component from the hero BP's list), but
                -- a component added by the UCS; the class to restore is the known BP_Avatar_Default_C.
                if not cName:find("TgfTileTarget", 1, true) and not IsServerWorld(owner) then
                    pcall(function() Comp.ChildActorClass = nil end)
                    table.insert(PendingAvatars, { Comp = Comp, Pawn = owner, Since = os.clock() })
                    StartAvatarLoop()
                    Log(string.format("  [R14_DEFER] %s: %s — avatar deferred until the manager arrives", ShortName(owner), cName))
                end
                return
            end
            EnsureEquipmentManagerOnPawn(owner, "ChildCompCreated", false)
        end
    end)
end)

-- The hero's second resource (mana / soul energy / primal energy) — for diagnostics.
-- Souls (B_TGF_Soul_C) from enemies: log 30.10 (mage, listen) — the resources are right (Magic=300/300, no SoulEnergy), the soul
-- spawns with VisibleTag=TGF.Character.ActorTag.CareerSpec.SoulEnergy and VisibleToLocalPlayer=false, but is still
-- visible: it is apparently hidden by the client branch of the BP, which does not run for the own hero on a listen host.
-- So we hide them ourselves (display only; soul collection by the server is left alone).
local function ResourceLine(h)
    local out = {}
    pcall(function()
        out[#out+1] = string.format("type=%s race=%s career=%s", tostring(h.ActorRoleTypeId), tostring(h.ActorRace), tostring(h.ActorCareer))
    end)
    local ac = nil
    pcall(function() ac = h.AttributeComponent end)
    for _, s in ipairs({ { "TgfMagicAttributeSet", "MaxMagicBase", "CurMagic" },
                         { "TgfSoulEnergyAttributeSet", "MaxSoulEnergyBase", "CurSoulEnergy" },
                         { "TgfNaturalEnergyAttributeSet", "MaxNaturalEnergyBase", "CurNaturalEnergy" } }) do
        local v = "none"
        pcall(function()
            local set = ac:GetAttributeSet(StaticFindObject("/Script/LyraGame." .. s[1]))
            if set and set:IsValid() then
                v = string.format("%.0f/%.0f", set[s[3]].CurrentValue, set[s[2]].CurrentValue)
            end
        end)
        out[#out+1] = s[2]:gsub("^Max", ""):gsub("Base$", "") .. "=" .. v
    end
    return table.concat(out, " ")
end

-- Full HP and resources for a new hero on the host (30.10: a client joining a listen host appeared with 100 HP).
-- LocalDev (UnLua) on the host equips the joined player — the maximums do not change at once. So once a second
-- we look at the pawn and, when max HP has not changed for two polls in a row and the equipment is on (or REFILL_MAX_WAIT s passed),
-- once set Cur = Max: HP (native RoleDisplayAttributeUtility.GetMaxHealth), mana, souls (DK/lich),
-- primal energy (druid). Only on the server (HasAuthority) — the values replicate to the client.
local REFILL_MAX_WAIT = 12
-- 5th field — the base maximum for the class (role_level_upgrade, level 1): for joined players LocalDev does not set the class attributes
-- (log 30.10: DK with SoulEnergy=0/0 — neither the resource nor souls from enemies); if the maximum is 0 — set the base.
local RESOURCE_SETS = {
    { "TgfMagicAttributeSet", "CurMagic", "MaxMagicBase", "MaxMagicPercent", { [400003] = 300, [400005] = 300, [400102] = 300 } },
    { "TgfSoulEnergyAttributeSet", "CurSoulEnergy", "MaxSoulEnergyBase", "MaxSoulEnergyPercent", { [400006] = 100, [400007] = 100 } },
    { "TgfNaturalEnergyAttributeSet", "CurNaturalEnergy", "MaxNaturalEnergyBase", "MaxNaturalEnergyPercent", { [400008] = 120 } },
}

local function SetCurrent(set, prop, v)
    local old = set[prop]
    set[prop].BaseValue = v
    set[prop].CurrentValue = v
    pcall(function() set["OnRep_" .. prop](set, old) end)
end

local function HeroMaxHealth(h)
    local asc = h.PlayerState:GetLyraAbilitySystemComponent()
    return StaticFindObject("/Script/LyraGame.Default__RoleDisplayAttributeUtility"):GetMaxHealth(asc)
end

local function RefillHero(h)
    local ac = h.AttributeComponent
    local out = {}
    local maxHp = HeroMaxHealth(h)
    local def = ac:GetAttributeSet(StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet"))
    if def and def:IsValid() and maxHp > 0 then
        out[#out + 1] = string.format("HP %.0f->%.0f", def.CurHealth.CurrentValue, maxHp)
        SetCurrent(def, "CurHealth", maxHp)
    end
    for _, s in ipairs(RESOURCE_SETS) do
        pcall(function()
            local set = ac:GetAttributeSet(StaticFindObject("/Script/LyraGame." .. s[1]))
            if not (set and set:IsValid()) then return end
            local classBase = s[5][h.ActorRoleTypeId]
            if set[s[3]].CurrentValue <= 0 and classBase then
                out[#out + 1] = string.format("%s=%d", s[3], classBase)
                SetCurrent(set, s[3], classBase)
            end
            local max = set[s[3]].CurrentValue * (1 + set[s[4]].CurrentValue)
            if max <= 0 then return end -- a resource not of this class
            out[#out + 1] = string.format("%s %.0f->%.0f", s[2]:gsub("^Cur", ""), set[s[2]].CurrentValue, max)
            SetCurrent(set, s[2], max)
        end)
    end
    return table.concat(out, " ")
end

local function RefillWhenReady(path, tries, lastMax)
    GameDelay(1000, function()
        ExecuteInGameThread(function()
            local ok, err = pcall(function()
                local h = LiveObject(path)
                if not h then return end
                if h.Role ~= 3 then return end -- ROLE_Authority: host/server only
                local okMax, maxHp = pcall(HeroMaxHealth, h) -- the PlayerState may not exist yet
                if not okMax then maxHp = 0 end
                local equips = 0
                pcall(function() equips = h:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")):GetAllEquips():Length() end)
                local stable = maxHp > 0 and lastMax and math.abs(maxHp - lastMax) < 0.5 and equips > 0
                if stable or (tries >= REFILL_MAX_WAIT and maxHp > 0) then
                    Log(string.format("[REFILL] %s after %d s (equipment %d): %s", ShortName(h), tries, equips, RefillHero(h)))
                    return
                end
                if tries < REFILL_MAX_WAIT * 3 then RefillWhenReady(path, tries + 1, maxHp) end
            end)
            if not ok then Log("[REFILL] error: " .. tostring(err)) end
        end)
    end)
end

-- Loot diagnostics 30.10: in no_gear mode (Crypt 10101–10103 without equipment) rings/necklaces drop, although the
-- Crypt pools do not have them. Loot is generated by the native UBattleUtility::Loot(Owner, SourceName, UserKey, PlayerStates,
-- LootPlayerState, ClassName, LootType) — it takes the map from the GameState. We log every call and the items created right after it
-- (rings/necklaces 185…/186…/195… — marked with !!!).
local LootDiagUntil, LootDiagLines = 0, 0
local LootHookInstalled = false

local function LootMapId()
    local id = "?"
    pcall(function()
        local gs = FindFirstOf("GameStateBase")
        id = tostring(gs.MapId)
    end)
    return id
end

local function EnsureLootDiag()
    if LootHookInstalled then return end
    LootHookInstalled = pcall(function()
        RegisterHook("/Script/LyraGame.BattleUtility:Loot", function(_, Owner, SourceName, UserKey, PlayerStates, LootPlayerState, ClassName, LootType)
            if LootDiagLines >= 400 then return end
            LootDiagLines = LootDiagLines + 1
            local src, cls, lt, own = "?", "?", "?", "?"
            pcall(function() src = SourceName:get():ToString() end)
            pcall(function() cls = ClassName:get():ToString() end)
            pcall(function() lt = tostring(LootType:get()) end)
            pcall(function() own = ShortName(Owner:get()) end)
            Log(string.format("[LOOT] map=%s loot_type=%s source=%s class=%s owner=%s", LootMapId(), lt, src, cls, own))
            LootDiagUntil = os.clock() + 0.5
        end)
    end)
    pcall(function()
        NotifyOnNewObject("/Script/LyraGame.TgfInventoryItemInstance", function(inst)
            if os.clock() > LootDiagUntil or LootDiagLines >= 400 then return end
            GameDelay(100, function()
                ExecuteInGameThread(function()
                    pcall(function()
                        if not inst:IsValid() then return end
                        local t = inst:GetTypeId()
                        local jewel = (t // 1000000 == 185 or t // 1000000 == 186 or t // 1000000 == 195)
                        LootDiagLines = LootDiagLines + 1
                        Log(string.format("    [LOOT item] %s%d", jewel and "!!! " or "", t))
                    end)
                end)
            end)
        end)
    end)
    Log("LOOT diag hook installed=" .. tostring(LootHookInstalled) .. " map=" .. LootMapId())
end

-- F8: what the game's table loader sees (ExcelLoaderConfigGroupWrapper). The source table is indexed by
-- (UeLootSourceId, MapId, LootType): our zzzzzz_Balance_P has 10916 rows, the old zz_Loot_P — 262 (map 0 only).
-- Log 30.10: Loot is called with GameState.MapId=10102, but the drop is as for map 0 — check whether there are rows for 10102 at all.
local LOOT_WRAPPER = "/Script/TGFBattleProtocol.Default__ExcelLoaderConfigGroupWrapper"
local LootProbeSource = 129 -- BP_Tgf_H_Monster_Goblin_CrossBow_C
pcall(function()
    RegisterHook("/Script/TGFBattleProtocol.ExcelLoaderConfigGroupWrapper:GetRowExcelLootUeSourceChooseGroup_Of_UeLootSourceIdMapIdLootType",
        function(_, Src, Map, Type)
            if LootDiagLines >= 400 then return end
            LootDiagLines = LootDiagLines + 1
            local s, m, t = "?", "?", "?"
            pcall(function() s, m, t = tostring(Src:get()), tostring(Map:get()), tostring(Type:get()) end)
            Log(string.format("[LOOT lookup] source=%s map=%s type=%s", s, m, t))
        end)
end)
RegisterKeyBind(Key.F8, function()
    ExecuteInGameThread(function()
        local ok, err = pcall(function()
            local w = StaticFindObject(LOOT_WRAPPER)
            Log(string.format("[F8 LOOT] wrapper=%s rows=%s GameState.MapId=%s", tostring(w and w:IsValid()),
                tostring(w:GetExcelLootUeSourceChooseGroup_SizeOf_UeLootSourceIdMapIdLootType()), LootMapId()))
            for _, m in ipairs({ 0, 10101, 10102, 10103, 10201 }) do
                local row, ids = nil, "no row"
                pcall(function() row = w:GetRowExcelLootUeSourceChooseGroup_Of_UeLootSourceIdMapIdLootType(LootProbeSource, m, 0, {}) end)
                pcall(function()
                    if row and row:IsValid() then
                        local list = {}
                        local arr = row:GetAllOfLootIds()
                        if type(arr) == "table" then for _, v in ipairs(arr) do list[#list + 1] = tostring(v) end
                        else arr:ForEach(function(_, v) list[#list + 1] = tostring(v:get()) end) end
                        ids = string.format("map=%s loot_ids=[%s]", tostring(row:GetMapId({})), table.concat(list, ","))
                    end
                end)
                Log(string.format("[F8 LOOT]   (%d, %d, 0) -> %s", LootProbeSource, m, ids))
            end
        end)
        if not ok then Log("[F8 LOOT] error: " .. tostring(err)) end
    end)
end)

-- NET_TRACE: a map of net-mode checks in BPs. IsStandalone / K2_HasAuthority / IsServer are called from bytecode
-- via UFUNCTION, UE4SS catches them. We log which class asked and what it got, deduplicated (class+function+mode — once
-- per match). Capture in localbattle (standalone) and in host_from_lobby (listen) and compare: differing answers = cut branches.
-- Enable: NET_TRACE=true. Native C++ calls of GetNetMode bypassing UFUNCTION do not get here — BP only.
-- 31.10: disabled. Post-hooks on the native IsStandalone/IsDedicatedServer in UE4SS 3.0.1 give garbage instead of the answer
-- and coincided with two crashes inside UE4SS.dll (pointer = string bytes). The net mode is now checked by the probe of
-- DungeonborneRuServer (a direct KismetSystemLibrary call), the branch map — research\NETMODE_MAP.md.
local NET_TRACE = false
local NetHooksInstalled = false
local NetSeen = {}
local NetModeName = { [0] = "Standalone", [1] = "DedicatedServer", [2] = "ListenServer", [3] = "Client" }

-- KismetSystemLibrary has no GetNetMode — the mode is assembled from IsStandalone / IsDedicatedServer / IsServer.
-- The same functions are hooked by us: NetInProbe keeps the hook from calling itself recursively. The world mode does not change
-- during a match — cache by map name.
local NetInProbe = false
local NetModeCache = {}
local function WorldNetMode(ctx)
    local key = "?"
    pcall(function() key = ctx:GetWorld():GetFullName() end)
    if NetModeCache[key] then return NetModeCache[key] end
    local m = -1
    NetInProbe = true
    pcall(function()
        local lib = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
        if lib:IsStandalone(ctx) then m = 0
        elseif lib:IsDedicatedServer(ctx) then m = 1
        elseif lib:IsServer(ctx) then m = 2
        else m = 3 end
    end)
    NetInProbe = false
    if m ~= -1 then NetModeCache[key] = m end
    return m
end

local function NetCtx(ctxObj)
    local cls, local_, auth = "?", "?", "?"
    pcall(function() cls = ctxObj:GetClass():GetFName():ToString() end)
    -- ability context -> its avatar; an actor -> itself
    local actor = nil
    pcall(function() actor = ctxObj:GetAvatarActorFromActorInfo() end)
    if not (actor and actor:IsValid()) then actor = ctxObj end
    pcall(function() local_ = tostring(actor:IsLocallyControlled()) end)
    pcall(function() auth = tostring(actor:HasAuthority()) end)
    return cls, local_, auth
end

local function LogNetCheck(func, ctxObj, ret)
    if not NET_TRACE or NetInProbe then return end
    local cls, loc, auth = NetCtx(ctxObj)
    local nm = WorldNetMode(ctxObj)
    local keyk = func .. "|" .. cls .. "|" .. tostring(nm)
    if NetSeen[keyk] then return end
    NetSeen[keyk] = true
    Log(string.format("[NET %s] netmode=%s(%s) caller=%s locallyCtrl=%s hasAuth=%s -> %s",
        func, tostring(nm), NetModeName[nm] or "?", cls, loc, auth, tostring(ret)))
end

local function EnsureNetTrace()
    if NetHooksInstalled or not NET_TRACE then return end
    NetHooksInstalled = true
    -- The function's answer is known only in the post-callback; on 31.10 ret was nil in the pre-callback. What exactly UE4SS 3.0.1
    -- passes to the post-callback is unknown: we take the last argument (get() if available), otherwise log "?".
    local function Ret(...)
        local n = select("#", ...)
        if n == 0 then return "?" end
        local a = select(n, ...)
        local v = a
        pcall(function() v = a:get() end)
        return v
    end
    -- IsStandalone / IsDedicatedServer (WorldContextObject) -> bool; self = CDO, 1st parameter = context
    for _, fn in ipairs({ "IsStandalone", "IsDedicatedServer" }) do
        pcall(function()
            RegisterHook("/Script/Engine.KismetSystemLibrary:" .. fn, function() end, function(self, ctx, ...)
                local r = Ret(...)
                pcall(function() LogNetCheck(fn, ctx:get(), r) end)
            end)
        end)
    end
    -- K2_HasAuthority() on an ability: self = the ability
    pcall(function()
        RegisterHook("/Script/GameplayAbilities.GameplayAbility:K2_HasAuthority", function() end, function(self, ...)
            local r = Ret(...)
            pcall(function() LogNetCheck("K2_HasAuthority", self:get(), r) end)
        end)
    end)
    Log("NET_TRACE hooks installed")
end

-- The hero's attack attributes (F7) — diagnostics 30.10 "damage disappears or ×4": compare the host and joined players.
-- Suspicion: LocalDev copies the host's attributes to the joined player (CopyAttrValueFromASC) and sets stats to 30 if 0.
local function AttackLine(h)
    local out = {}
    local ac = h.AttributeComponent
    for _, s in ipairs({ { "TgfAttackAttributeSet", { "RightWeaponAttack", "LeftWeaponAttack", "AdditionalWeaponAttack", "AttackBonus", "AttackBoostRate", "CriticalRate", "CritDamageRate", "SkillBoostRate" } },
                         { "TgfFirstLevelAttributeSet", { "Strength", "Dexterity", "Intelligence", "Will", "Faith" } } }) do
        pcall(function()
            local set = ac:GetAttributeSet(StaticFindObject("/Script/LyraGame." .. s[1]))
            if not (set and set:IsValid()) then out[#out + 1] = s[1] .. "=none"; return end
            for _, f in ipairs(s[2]) do
                pcall(function() out[#out + 1] = string.format("%s=%.3g", f, set[f].CurrentValue) end)
            end
        end)
    end
    return table.concat(out, " ")
end

local SoulNotifyInstalled = false
local SoulLogged = 0
local function EnsureSoulNotify()
    if SoulNotifyInstalled then return end
    SoulNotifyInstalled = pcall(function()
        NotifyOnNewObject("/Game/TGF/Character/Hero/Race/Undead/GameAbility/B_TGF_Soul.B_TGF_Soul_C", function(Soul)
            -- VisibleToLocalPlayer is computed in BeginPlay — read it on the next frame
            GameDelay(50, function()
                ExecuteInGameThread(function()
                    pcall(function()
                        if not Soul:IsValid() then return end
                        local vis = Soul.VisibleToLocalPlayer
                        -- only a listen host with its own hero: a client hides them itself (its BP branch works), and on
                        -- a headless server there is no local player — hiding would replicate and hide souls for everyone
                        if vis == false and HasListenHostHero() then Soul:SetActorHiddenInGame(true) end
                        if SoulLogged >= 5 then return end
                        SoulLogged = SoulLogged + 1
                        local tag = "?"
                        pcall(function() tag = Soul.VisibleTag.TagName:ToString() end)
                        Log(string.format("*** SOUL spawned VisibleTag=%s VisibleToLocalPlayer=%s -> hidden=%s", tag,
                            tostring(vis), tostring(Soul.bHidden)))
                        for _, h in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
                            Log("    hero " .. ShortName(h) .. " " .. ResourceLine(h))
                        end
                    end)
                end)
            end)
        end)
    end)
    Log("SOUL notify installed=" .. tostring(SoulNotifyInstalled))
end

-- F7: which abilities are active on the own hero right now (diagnostics "the character stopped hitting / switching weapons":
-- a hung active ability with a blocking group keeps the others from activating) + its resources.
RegisterKeyBind(Key.F7, function()
    ExecuteInGameThread(function()
        EnsureSoulNotify()
        pcall(function()
            local heroes = FindAllOf("BP_Fighter_HeroSample_C") or {}
            for _, h in ipairs(heroes) do
                Log("[F7 RES] " .. ShortName(h) .. " " .. ResourceLine(h))
                Log("[F7 ATK] " .. ShortName(h) .. " " .. AttackLine(h))
                local role = -1
                pcall(function() role = h.Role end)
                local ps = h.PlayerState
                local asc = ps and ps:GetLyraAbilitySystemComponent()
                if asc and asc:IsValid() then
                    -- GetAllActiveGameplayAbilitys returns all granted ones (54) — we print only the really active ones,
                    -- one line each (Log cuts a line at 300 characters)
                    local total, active = 0, 0
                    local res = asc:GetAllActiveGameplayAbilitys()
                    local function add(a)
                        local ab = a
                        pcall(function() if a.get then ab = a:get() end end)
                        total = total + 1
                        local isActive, ending = false, "?"
                        pcall(function() isActive = ab.bIsActive == true; ending = tostring(ab.bIsAbilityEnding) end)
                        if isActive then
                            active = active + 1
                            Log(string.format("[F7 ACTIVE]   %s ending=%s", ShortName(ab), ending))
                        end
                    end
                    if type(res) == "table" then for _, a in ipairs(res) do add(a) end
                    else pcall(function() res:ForEach(function(_, a) add(a) end) end) end
                    Log(string.format("[F7 ACTIVE] %s Role=%s | granted %d, active %d", ShortName(h), tostring(role), total, active))
                end
            end
        end)
    end)
end)

ClientNotify("/Script/Engine.Pawn", function(NewPawn)
    local fullName = ""
    pcall(function() fullName = NewPawn:GetFullName() end)
    if not fullName:find("BP_Fighter_HeroSample_C", 1, true) then return end

    Log("NEW HERO PAWN: " .. ShortName(NewPawn))
    BodyLog("Pawn", NewPawn)
    EnsureEquipmentManagerOnPawn(NewPawn, "PawnCreated")
    -- HookSpecificClasses() disabled: empty hooks on ALL functions of TgfCharacter/Hero/Avatar (including attribute getters
    -- with a return value) do nothing, but interfere with the native calculation/display of stats ("Weapon damage").
    EnsureChargeHooks()
    EnsureSoulNotify()
    EnsureLootDiag()
    EnsureNetTrace()
    local PawnPath = PathOf(NewPawn)
    RefillWhenReady(PawnPath, 1, nil)
    -- resources after match initialization (attributes are not set at once)
    for _, ms in ipairs({ 3000, 15000 }) do
        GameDelay(ms, function()
            ExecuteInGameThread(function()
                -- 31.10: in temp mode reading resources after 3 s coincided with crash UE4SS.dll+0x3ecee5 — skipped
                local Pawn = LiveObject(PawnPath)
                if Pawn and R14Mode() ~= "temp" then
                    pcall(function() Log(string.format("[RES +%ds] %s %s", ms // 1000, ShortName(Pawn), ResourceLine(Pawn))) end)
                end
                EnsureSoulNotify()
            end)
        end)
    end

    -- Hero hooks — once per process (31.10: before, they were re-hooked on every new pawn and piled up)
    if HeroHooksInstalled then return end
    HeroHooksInstalled = true

    RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:UserConstructionScript", function(self)
        local pawn = self:get()
        local role = -1
        pcall(function() role = pawn.Role end)
        Log(">>> ENTER Hero:UCS | Pawn=" .. ShortName(pawn) .. " | Role=" .. tostring(role))
        BodyLog("UCS", pawn)
        EnsureEquipmentManagerOnPawn(pawn, "EnterUCS")
    end, function(self)
        Log("<<< EXIT Hero:UCS")
    end)

    -- In "native" mode neither the hero's input events nor ReceiveTick are touched — the game delivers the release itself.
    -- The hook on InpActEvt_IA_Weapon_Fire_Left_By_Press is removed in all modes (a hook on a hero input event),
    -- instead — a periodic EnsureChargeHooks from ReceiveTick.
    if CHARGE_RELEASE_MODE == "native" then return end

    pcall(function()
        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:ReceiveTick", function(self, DeltaSeconds)
            -- Weapon classes load on equip/switch — hook them every 2 s so as not to miss the first strike
            local now = os.clock()
            if now - LastChargeHookScan >= 2.0 then
                LastChargeHookScan = now
                EnsureChargeHooks()
            end
            -- The release workaround — only for the listen host's own pawn (see IsListenHostPawn): on a pure client it
            -- cancelled an RMB charge, a headless server has no own pawn.
            -- lazy check: only when there is something to release (otherwise — a function call every tick for every hero)
            if not (PendingRpcFallback or ActiveChargeAbility) then return end
            local tickPawn = nil
            pcall(function() tickPawn = self:get() end)
            if not (tickPawn and IsListenHostPawn(tickPawn)) then return end
            -- an ability not of this hero (a client's charge while the host's pawn ticks) — left alone
            local tickAddr = nil
            pcall(function() tickAddr = tickPawn:GetAddress() end)
            if not ChargeAvatarAddr or ChargeAvatarAddr ~= tickAddr then return end
            -- Deferred heavy release ("rpc" mode):
            --   Stage 1 (+0.05 s after LMB up): released natively (standalone) — do nothing; not released (listen) —
            --            call OnRelease_EA7A…(TimeHeld) — the callback WaitInputRelease calls in standalone
            --            (then the BP itself does ProcessRelease + TellServerRelease and the Relic wave in DealCastCharge).
            --            On listen the ability does not create WaitInputRelease, so nobody called the callback.
            --   Stage 2 (+0.2 s): log the result.
            -- 30.10 diagnostics: Spec.InputPressed=false (the ASC gets the release), ServerSetInputReleased -> ChargeReleased
            --            stays false => on listen the ability has no WaitInputRelease, the server waits for TellServerRelease.
            local fb = PendingRpcFallback
            if fb and now >= fb.At then
                PendingRpcFallback = nil
                pcall(function()
                    if not fb.Ability:IsValid() then return end
                    if fb.Stage == 2 then
                        Log(string.format("    -> ChargeReleased=%s", tostring(fb.Ability.ChargeReleased)))
                        return
                    end
                    if fb.Ability.ChargeReleased ~= false then return end -- released natively (standalone)
                    local asc = fb.Ability:GetAbilitySystemComponentFromActorInfo()
                    local spec = asc and FindChargeSpec(asc, fb.Ability)
                    Log(string.format(">>> LMB_UP(%s) held=%.2fs not released natively | spec=%s InputPressed=%s tags=%s",
                        fb.Reason, fb.Held, tostring(spec and spec.Handle), tostring(spec and spec.InputPressed),
                        spec and table.concat(spec.Tags, ",") or "-"))
                    -- Call exactly the WaitInputRelease callback — as the engine would call it in standalone.
                    -- Trace 30.10 (local vs listen): natively OnRelease_ -> ProcessRelease + TellServerRelease, and only
                    -- after that EventReceived_F72DFA85 -> ProcessProjectile (the Relic wave) comes into Enchant.DealCastCharge.
                    -- A TellServerRelease alone gave a strike without the wave.
                    local ok, err = pcall(function() fb.Ability:OnRelease_EA7AD5B24572E8EDB0D634A9215200E9(fb.Held) end)
                    Log(string.format("    OnRelease_EA7A(%.2f) ok=%s %s", fb.Held, tostring(ok), ok and "" or tostring(err)))
                    PendingRpcFallback = { Ability = fb.Ability, At = os.clock() + 0.2, Stage = 2 }
                end)
            end
            if not ActiveChargeAbility then return end
            pcall(function()
                if not ActiveChargeAbility:IsValid() or ActiveChargeAbility.ChargeReleased == true then
                    ActiveChargeAbility = nil
                    return
                end
                local held = os.clock() - ChargeStartClock
                if held < 0.10 then return end
                local pawn = self:get()
                local pc = pawn:GetController()
                if pc and pc:IsValid() then
                    local timeDown = pc:GetInputKeyTimeDown({KeyName = FName("LeftMouseButton")})
                    if type(timeDown) == "number" and timeDown <= 0.0 then
                        TriggerActiveChargeRelease("LMB_UP")
                    end
                end
            end)
        end)
    end)
end)
-- 05.11 (B13): on the server instead of NotifyOnNewObject(Pawn) — once a second we go over live heroes (fresh objects from
-- FindAllOf) and start RefillWhenReady for new ones (full HP and resources, as the pawn creation callback did). No hooks, no
-- calls on half-built objects; charge and soul hooks are not needed on the server (they are only for the listen host's hero).
if IsServerProc then
    local Seen = {}
    GameLoop(1000, function()
        ExecuteInGameThread(function()
            local ok, err = pcall(function()
                local alive = {}
                for _, p in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
                    if p:IsValid() then
                        local path = PathOf(p)
                        if path ~= "" and not path:find("Default__", 1, true) then
                            local addr = p:GetAddress()
                            alive[addr] = true
                            if not Seen[addr] then
                                Seen[addr] = true
                                RefillWhenReady(path, 1, nil)
                            end
                        end
                    end
                end
                for addr in pairs(Seen) do
                    if not alive[addr] then Seen[addr] = nil end
                end
            end)
            if not ok then Log("[SERVER] hero poll: error " .. tostring(err)) end
        end)
        return false
    end)
    Log("[SERVER] hero poll (full HP) enabled instead of the pawn creation notification")
end

-- 05.11 (Sh4g): results window probe — client only, no hooks. Every 0.5 s: UMatchService:ShouldShowBattleResult()
-- (flag +0x1b0 is set by the battle_settle_sync handler, the lobby shows the window in CheckAutoPopupSystems) and whether there is
-- WBP_HallWindow_C. In crash_ue4ss.log — changes only: [RESULTPROBE] show=… hall=… map=…
if not IsServerProc then
    local Probe = { Show = nil, Hall = nil, Map = nil }
    GameLoop(500, function()
        ExecuteInGameThread(function()
            pcall(function()
                local Show, Hall, Map = "?", "none", "?"
                local Ms = FindFirstOf("MatchService")
                if Ms and Ms:IsValid() then
                    local Ok, V = pcall(function() return Ms:ShouldShowBattleResult() end)
                    Show = Ok and tostring(V) or "error"
                else
                    Show = "no MatchService"
                end
                local H = FindFirstOf("WBP_HallWindow_C")
                if H and H:IsValid() then
                    local Vis = "?"
                    pcall(function() Vis = tostring(H:IsInViewport()) end)
                    Hall = "present, on screen=" .. Vis
                end
                pcall(function()
                    local Gs = FindFirstOf("GameStateBase")
                    Map = Gs and Gs:IsValid() and (Gs:GetFullName():match("/([%w_]+)%.[%w_]+:") or "?") or "none"
                end)
                if Show ~= Probe.Show or Hall ~= Probe.Hall or Map ~= Probe.Map then
                    Probe.Show, Probe.Hall, Probe.Map = Show, Hall, Map
                    Log(string.format("[RESULTPROBE] %s show=%s hall=%s map=%s", os.date("%H:%M:%S"), Show, Hall, Map))
                end
            end)
        end)
        return false
    end)
end

-- --- Hero fixes on the client (04.11, moved from CrashTracer FixHeroPawn; docs/fix_listen.md) -------------------------
-- CrashTracer did them from the OnReady hook set from NotifyOnNewObject(Pawn) (that is where UE4SS crashed, B13). Here — without
-- hooks: once a second we go over BP_Fighter_HeroSample_C heroes (fresh objects from FindAllOf, no old wrappers), those
-- seen for at least 1 s are fixed once; affix tooltips — again at 1/3/8/15 s (the equipment arrives later).
-- Only the client part (the server one — DungeonborneRuServer HeroFix):
--   mesh       — ticks of Mesh/ShellMesh (ShellMesh visibility and animation layer relinking removed 10.11, see ClientFixVisual);
--   gravity    — ActorMapComponent disabled (on the client it computes from its own hero and zeroed gravity for others),
--               GravityScale 1.28 only if zeroed; MovementMode and replicated fields are left alone;
--   tooltips   — on equipped items with a buffer: AffixDisplayConfigAttached(), then OnRep_DItemBuffer only if the display is empty.
-- The server process (-dbserver) skips it. ru-herofix.txt = 0 next to main.lua — disable.
local ClientHeroFix = { Seen = {}, Done = {}, Affix = {}, Skip = nil }
local AFFIX_PASSES = { 1, 3, 8, 15 }
local AFFIX_SLOTS = { 1, 2, 3, 4, 11, 12, 13, 14, 15, 16, 17, 18 }

local function ClientHeroFixSkip()
    if ClientHeroFix.Skip ~= nil then return ClientHeroFix.Skip end
    local skip = false
    pcall(function()
        local cl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary"):GetCommandLine():ToString():lower()
        if cl:find("%-dbserver") then skip = true end
    end)
    pcall(function()
        local v = ModFlag("herofix", "enabled", "ue4ss/Mods/DungeonborneRuClient/Scripts/ru-herofix.txt",
            "Mods/DungeonborneRuClient/Scripts/ru-herofix.txt")
        if (v or ""):match("^0") then skip = true end
    end)
    ClientHeroFix.Skip = skip
    Log("[HEROFIX] client hero fixes: " .. (skip and "disabled" or "enabled"))
    return skip
end

local function ClientFixVisual(p)
    local role = -1
    pcall(function() role = p.Role end)
    pcall(function()
        p.DisableActorMap = true
        local amc = p.ActorMapComponent
        if amc and amc:IsValid() then
            amc:SetComponentTickEnabled(false)
            amc:Deactivate()
        end
        local move = p.CharacterMovement
        if move and move:IsValid() and move.GravityScale == 0 then move.GravityScale = 1.28 end
    end)
    pcall(function()
        local mainMesh = p.Mesh
        if mainMesh and mainMesh:IsValid() then
            mainMesh:SetComponentTickEnabled(true)
            mainMesh.VisibilityBasedAnimTickOption = 0
        end
        local shellMesh = p.ShellMesh
        if shellMesh and shellMesh:IsValid() then
            shellMesh:SetComponentTickEnabled(true)
            shellMesh.VisibilityBasedAnimTickOption = 0
        end
    end)
    -- 10.11: removed ShellMesh visibility (for others SetHiddenInGame/SetVisibility(false/true, with children), for own
    -- SetOwnerNoSee) and layer relinking (LinkDefaultAnimLayers + LinkCurrentCachedAnimLayer) — a legacy of the listen server.
    -- With the native dedicated server they broke: the layers — the weapon animation at start ("as without ABP", cured by switching weapons)
    -- and the idle of dual daggers. Checked on the stand 10.11 with ru-herofix.txt = 0: everything is fine without them.
    return role
end

-- tooltips of equipped items: how many items were fixed
local function ClientFixAffixes(p)
    local fixed = 0
    pcall(function()
        local eqMgr = p:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent"))
        if not (eqMgr and eqMgr:IsValid()) then return end
        for _, sId in ipairs(AFFIX_SLOTS) do
            pcall(function()
                local eq = eqMgr:GetEquipBySlot(sId, -1)
                local inst = eq and eq.InventoryInstance
                if not (inst and inst:IsValid()) then return end
                if #inst.DItemBuffer.Buffer <= 0 then return end
                if not inst:IsAffixDisplayConfigAttached() then inst:AffixDisplayConfigAttached() end
                if not inst:IsAffixDisplayConfigAttached() then return end
                local empty = #inst.Buffes <= 0 and #inst.MainBasicBuff <= 0 and #inst.TopAffixs <= 0
                if empty then
                    inst:OnRep_DItemBuffer()
                    pcall(function() eqMgr:UpdateItemInstance(inst) end)
                    fixed = fixed + 1
                end
            end)
        end
    end)
    return fixed
end

local function ClientHeroFixTick()
    if ClientHeroFixSkip() then return end
    local now, alive = os.time(), {}
    for _, p in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if p:IsValid() then
            local addr = p:GetAddress()
            local path = PathOf(p)
            if path ~= "" and not path:find("Default__", 1, true) then
                alive[addr] = true
                ClientHeroFix.Seen[addr] = ClientHeroFix.Seen[addr] or now
                local age = now - ClientHeroFix.Seen[addr]
                if not ClientHeroFix.Done[addr] and age >= 1 then
                    ClientHeroFix.Done[addr] = true
                    local role = ClientFixVisual(p)
                    Log(string.format("[HEROFIX] %s role=%s: mesh/gravity", ShortName(p), tostring(role)))
                end
                local pass = ClientHeroFix.Affix[addr] or 0
                local nextAge = AFFIX_PASSES[pass + 1]
                if nextAge and age >= nextAge then
                    ClientHeroFix.Affix[addr] = pass + 1
                    local n = ClientFixAffixes(p)
                    if n > 0 then Log(string.format("[HEROFIX] %s: affix tooltips fixed on %d items (after %d s)", ShortName(p), n, age)) end
                end
            end
        end
    end
    for addr in pairs(ClientHeroFix.Seen) do
        if not alive[addr] then
            ClientHeroFix.Seen[addr], ClientHeroFix.Done[addr], ClientHeroFix.Affix[addr] = nil, nil, nil
        end
    end
end

GameLoop(1000, function()
    ExecuteInGameThread(function()
        local ok, err = pcall(ClientHeroFixTick)
        if not ok then Log("[HEROFIX] error: " .. tostring(err)) end
    end)
    return false
end)

-- 05.11: auto-exit after a PVP match (custom games, DeathMatch arenas).
-- Server: Phase_PVP_GameOver sets BP_Tgf_PVPPhaseHint.GameOverEndTime = server time + DelayTime (~5 s), waits and
-- calls the native UBattleDsUtility::GameOver(). That one is closed by the "dedicated + DS SDK" check: on our listen server it is empty,
-- so the session does not end and the client stays in the match. The WBP_TGF_PVP_GameOver window at zero only stops the timer.
-- Here: when the window is shown and the server time has reached GameOverEndTime (+1 s margin), we call the handler of the
-- "Exit" button of this window — the same as pressing it (OnQuitGame + OpenLevel). Switch: ru-pvpleave.txt = 0.
-- Analysis: backups\tools\research\room\PVP_START_END.md.
local PvpLeave = {
    Done = {},
    BTN = "BndEvt__WBP_TGF_PVP_GameOver_ButtonLeave_K2Node_ComponentBoundEvent_0_ButtonClicked__DelegateSignature",
    Off = nil, Logged = {},
}

function PvpLeave.Skip()
    if IsServerProc then return true end
    if PvpLeave.Off == nil then
        local v = (ModFlag("pvpleave", "enabled", "ru-pvpleave.txt") or ""):lower()
        PvpLeave.Off = (v == "0" or v == "off")
        Log("[PVPLEAVE] auto-exit after PVP " .. (PvpLeave.Off and "disabled" or "enabled"))
    end
    return PvpLeave.Off
end

function PvpLeave.Tick()
    if PvpLeave.Skip() then return end
    local w = FindFirstOf("WBP_TGF_PVP_GameOver_C")
    if not (w and w:IsValid()) then return end
    local key = w:GetAddress()
    if PvpLeave.Done[key] then return end
    local hint = FindFirstOf("BP_Tgf_PVPPhaseHint_C")
    if not (hint and hint:IsValid()) then return end
    local endTime = tonumber(hint.GameOverEndTime) or 0
    if endTime <= 0 then return end
    local gs = StaticFindObject("/Script/Engine.Default__GameplayStatics"):GetGameState(w)
    if not (gs and gs:IsValid()) then return end
    local now = gs:GetServerWorldTimeSeconds()
    if not PvpLeave.Logged[key] then
        PvpLeave.Logged[key] = true
        Log(string.format("[PVPLEAVE] match result shown: %.1f s until exit", endTime - now))
    end
    if now < endTime + 1.0 then return end
    PvpLeave.Done[key] = true
    local ok, err = pcall(function() w[PvpLeave.BTN](w) end)
    Log(string.format("[PVPLEAVE] time is up (server %.1f >= %.1f): exit to the lobby %s %s", now, endTime,
        tostring(ok), ok and "" or tostring(err)))
end

GameLoop(500, function()
    ExecuteInGameThread(function()
        local ok, err = pcall(PvpLeave.Tick)
        if not ok then Log("[PVPLEAVE] error: " .. tostring(err)) end
    end)
    return false
end)

-- 05.11: connection timeout to the match server (user's request: "if the client cannot connect — cancel
-- the connection and send to the main lobby, time out 60 seconds"). Client only.
-- "Connecting": there is a live PendingNetGame (connection to the DS in progress) OR we are already in a net client world, but there is no own pawn
-- yet (loading hangs / the server did not let us in). "In a match": a net client world and the local controller has a pawn.
-- After the first entry into a match in this world the timer no longer starts (dead spectators without a pawn are not kicked).
-- Cancel — like the PVP "Exit" button: TgfSystemBlueprintLibrary::OnQuitGame (the client forgets the room: URoomService
-- battle_id/snapshot are cleared, Ghidra 0x142e030f0, nothing is sent to backend) + OpenLevel("Empty")
-- (Browse itself cancels the PendingNetGame / closes the connection to the server).
-- Switch/time: ru-connect-timeout.txt next to the exe: "0"/"off" — disabled, a number — seconds (default 60).
local ConnTimeout = { Limit = nil, Start = nil, Why = "", Entered = {}, Fired = 0, LastLog = 0 }

function ConnTimeout.Cfg()
    if ConnTimeout.Limit ~= nil then return ConnTimeout.Limit end
    local limit = 60
    local v = (ModFlag("connect", "timeout", "ru-connect-timeout.txt") or ""):lower()
    if v == "0" or v == "off" then limit = 0 elseif tonumber(v) then limit = tonumber(v) end
    ConnTimeout.Limit = limit
    Log("[CONNECT_TIMEOUT] " .. (limit > 0 and ("match connection timeout " .. limit .. " s") or "disabled"))
    return limit
end

function ConnTimeout.LocalPC()
    for _, pc in ipairs(FindAllOf("PlayerController") or {}) do
        if pc:IsValid() then
            local isLocal = false
            pcall(function() isLocal = pc:IsLocalController() end)
            if isLocal and not PathOf(pc):find("Default__", 1, true) then return pc end
        end
    end
    return nil
end

function ConnTimeout.Leave(pc, why)
    ConnTimeout.Fired = ConnTimeout.Fired + 1
    local lib = StaticFindObject("/Script/LyraGame.Default__TgfSystemBlueprintLibrary")
    local okQuit = lib and lib:IsValid() and pcall(function() lib:OnQuitGame() end)
    local gs = StaticFindObject("/Script/Engine.Default__GameplayStatics")
    local okOpen = pc and gs and pcall(function() gs:OpenLevel(pc, FName("Empty"), true, "") end)
    Log(string.format("[CONNECT_TIMEOUT] %d s without entering the match (%s): cancel — OnQuitGame %s, OpenLevel(Empty) %s",
        ConnTimeout.Limit, why, tostring(okQuit), tostring(okOpen)))
end

function ConnTimeout.Tick()
    if IsServerProc or ConnTimeout.Cfg() <= 0 then return end
    local now = os.time()
    local pending = FindFirstOf("PendingNetGame")
    local connecting = pending ~= nil and pending:IsValid()
    local pc = ConnTimeout.LocalPC()
    local netClient, hasPawn, worldKey = false, false, ""
    if pc then
        local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
        pcall(function() netClient = not ksl:IsServer(pc) end)
        pcall(function() local p = pc.Pawn; hasPawn = p ~= nil and p:IsValid() end)
        worldKey = MapOfName(pc:GetFullName())
        if worldKey == "" then worldKey = PathOf(pc) end
    end
    if netClient and hasPawn then
        if not ConnTimeout.Entered[worldKey] then
            ConnTimeout.Entered[worldKey] = true
            if ConnTimeout.Start then
                Log(string.format("[CONNECT_TIMEOUT] entered the match in %d s (%s)", now - ConnTimeout.Start, worldKey))
            end
        end
        ConnTimeout.Start = nil
        return
    end
    local waiting = connecting or (netClient and not ConnTimeout.Entered[worldKey])
    if not waiting then
        if ConnTimeout.Start then
            Log(string.format("[CONNECT_TIMEOUT] connecting stopped by itself after %d s (%s)", now - ConnTimeout.Start,
                ConnTimeout.Why))
        end
        ConnTimeout.Start = nil
        if not netClient then ConnTimeout.Entered = {} end   -- back in the lobby: the next match is counted anew
        return
    end
    local why = connecting and "connection to the server (PendingNetGame)" or ("loading the server world without a pawn " .. worldKey)
    if not ConnTimeout.Start then
        ConnTimeout.Start, ConnTimeout.Why, ConnTimeout.LastLog = now, why, now
        Log("[CONNECT_TIMEOUT] start of connecting to the match: " .. why)
        return
    end
    if why ~= ConnTimeout.Why then
        Log(string.format("[CONNECT_TIMEOUT] %d s: %s", now - ConnTimeout.Start, why))
        ConnTimeout.Why = why
    end
    if now - ConnTimeout.Start >= ConnTimeout.Limit then
        ConnTimeout.Start = nil
        if netClient then ConnTimeout.Entered[worldKey] = true end  -- do not kick again in this world
        ConnTimeout.Leave(pc, why)
    end
end

GameLoop(1000, function()
    ExecuteInGameThread(function()
        local ok, err = pcall(ConnTimeout.Tick)
        if not ok then Log("[CONNECT_TIMEOUT] error: " .. tostring(err)) end
    end)
    return false
end)

-- ---------------------------------------------------------------------------------------------------------------
-- 12.11 StealthReveal — an invisible enemy rogue is visible up close (docs/ROGUE_STEALTH.md, a temporary solution in WORKAROUNDS).
-- Material formula (LOCAL, SM5 shader from the paks): StealthVisibleDistance is not a distance but a STRENGTH 0..1 (under saturate):
-- visibility ~ R·Fade·lerp(FarDistanceVisibility, 1, S), S = saturate(StealthVisibleDistance); Distance and Velocity are
-- dropped in the shader (the BP writes them for nothing). The native code does not set S for us (0) — an enemy rogue is never visible.
-- The mod: FarDistanceVisibility = 0, S = strength by distance from the own pawn to the rogue (update_ms), optionally Visibility —
-- a blend to an almost opaque look up close (solid_distance). Every scan_ms we look for BP_MaterialState_Stealth_C states with
-- CloakType 2/3 (enemy) and MIDs with the parent MI_Character_CharacterInvisibility_Albedo_Distance on the rogue's meshes
-- (ACharacter.Mesh -> AttachChildren; a function's array result and TMap are not readable from Lua in UE4SS 3.0.1).
-- Settings — section [stealth] (ModSetting): from the server (season, [client.DungeonborneRuClient.stealth]) via the launcher.
local StealthReveal = {
    Cfg = { enabled = 1, distance = 1500, full_distance = 0, opacity_near = 1, opacity_far = 0, curve = 1,
        -- 13.11: solid visibility up close by default (an outline from strength S is almost invisible in dark places — "the mod does not work");
        -- search for meshes and materials every 250 ms (was 1 s: materials recreated on a new stealth entry
        -- kept the native values)
        solid_distance = 400, solid_max = 1, update_ms = 100, scan_ms = 250,
        -- 19.11 "ghost" (stand + re analysis, sandbox/stealth-re/NOTES.md): the own rogue and the enemy use the SAME stealth MID;
        -- the BP's OnOriginReplace gives the own one (CloakType 1) Visibility = 1 and ISEmissive = 1 — the white smoky look of the
        -- own hands; the enemy (2/3) gets 0/0. ghost = 1: Visibility = the strength curve by distance, ISEmissive = 1 inside
        -- `distance`, StealthVisibleDistance = glow (in the shader emissive = ISE·Vis·(BaseColor + 10·S) — with S 0.7 the enemy
        -- would glow ~8x brighter than the own hands, so 0 by default). ghost = 0 — the previous outline mode
        ghost = 1, glow = 0 },
    Targets = {}, -- { {pawn, mids} }
    OwnPawn = nil,
    TeamSub = nil,
    MeshCls = nil,
    LastScan = -1e9,
    Logged = {},
    MeshNames = {},
    DiagAt = 0,
    DiagN = 0,
}

do
    local c, from = StealthReveal.Cfg, {}
    for k in pairs(c) do
        local v, src = ModSetting("stealth", k)
        if tonumber(v) then c[k], from[#from + 1] = tonumber(v), k .. "<" .. src end
    end
    Log(string.format("[STEALTH] settings: enabled=%s distance=%s full_distance=%s near=%s far=%s curve=%s solid=%s/%s ghost=%s glow=%s update=%s ms (%s)",
        tostring(c.enabled), tostring(c.distance), tostring(c.full_distance), tostring(c.opacity_near),
        tostring(c.opacity_far), tostring(c.curve), tostring(c.solid_distance), tostring(c.solid_max), tostring(c.ghost),
        tostring(c.glow), tostring(c.update_ms),
        #from > 0 and table.concat(from, " ") or "all default"))
end

function StealthReveal.Valid(o)
    if o == nil then return false end
    local ok, v = pcall(function() return o:IsValid() end)
    return ok and v
end

function StealthReveal.Get(fn)
    local ok, v = pcall(fn)
    if ok then return v end
    return nil
end

function StealthReveal.Name(o)
    if not StealthReveal.Valid(o) then return "" end
    return StealthReveal.Get(function() return o:GetFName():ToString() end) or ""
end

function StealthReveal.Addr(o)
    return StealthReveal.Get(function() return o:GetAddress() end) or 0
end

function StealthReveal.Loc(a)
    local v = StealthReveal.Get(function() return a:K2_GetActorLocation() end)
    if not v then return nil end
    local x = StealthReveal.Get(function() return v.X end)
    local y = StealthReveal.Get(function() return v.Y end)
    local z = StealthReveal.Get(function() return v.Z end)
    if type(x) ~= "number" or type(y) ~= "number" or type(z) ~= "number" then return nil end
    return x, y, z
end

-- Invisibility MIDs on the hero's meshes: Mesh and everything attached to it (at most 40 components)
function StealthReveal.Mids(pawn)
    local V, G = StealthReveal.Valid, StealthReveal.Get
    if not V(StealthReveal.MeshCls) then StealthReveal.MeshCls = StaticFindObject("/Script/Engine.MeshComponent") end
    local out, seenC, seenM = {}, {}, {}
    StealthReveal.MeshNames = {}
    local root = G(function() return pawn.Mesh end)
    if not V(root) then return out end
    -- 13.11: it used to be at most 40 components — for a rogue in armor with a skin (AvatarSubMesh_*) some meshes were missed
    local q, n = { root }, 0
    while #q > 0 and n < 200 do
        local c = table.remove(q, 1)
        n = n + 1
        local a = StealthReveal.Addr(c)
        if V(c) and not seenC[a] then
            seenC[a] = true
            if G(function() return c:IsA(StealthReveal.MeshCls) end) then
                local cnt = G(function() return c:GetNumMaterials() end) or 0
                local mats = {}
                for i = 0, math.min(cnt, 32) - 1 do mats[#mats + 1] = G(function() return c:GetMaterial(i) end) end
                mats[#mats + 1] = G(function() return c.OverlayMaterial end)
                for _, m in ipairs(mats) do
                    if V(m) then
                        local ma = StealthReveal.Addr(m)
                        if not seenM[ma] then
                            seenM[ma] = true
                            if StealthReveal.Name(G(function() return m.Parent end)) == "MI_Character_CharacterInvisibility_Albedo_Distance" then
                                out[#out + 1] = m
                                StealthReveal.MeshNames[#StealthReveal.MeshNames + 1] = StealthReveal.Name(c)
                            end
                        end
                    end
                end
            end
            pcall(function()
                c.AttachChildren:ForEach(function(_, e)
                    local k = e:get()
                    if V(k) then q[#q + 1] = k end
                end)
            end)
        end
    end
    return out
end

-- 17.11: strength is written every frame. Before, it was written only by a timer once per update_ms — between writes the native code/BP could
-- restore their values (the "read" check came right after our own write and did not show it). Now a hook on
-- OnTickState of the stealth state itself (before and after the BP, every frame), new MIDs are caught in OnOriginReplace, the timer —
-- only search and a fallback write if the hook did not attach. The "foreign write" counters in the log show who overwrites:
-- "before" — between frames (native code), "after" — inside OnTickState (BP).
StealthReveal.States = {}      -- state address -> target { pawn, mids, ... }
StealthReveal.Skip = {}        -- addresses of checked states that are not an enemy (own rogue, ally, inactive)
StealthReveal.Hooked = false
StealthReveal.LastHookAt = -1e9
StealthReveal.FN = nil
local STEALTH_PARENT = "MI_Character_CharacterInvisibility_Albedo_Distance"

function StealthReveal.Names()
    if not StealthReveal.FN then
        StealthReveal.FN = { svd = FName("StealthVisibleDistance"), vis = FName("Visibility"),
            far = FName("FarDistanceVisibility"), ise = FName("ISEmissive") }
    end
    return StealthReveal.FN
end

function StealthReveal.Scan()
    local V, G = StealthReveal.Valid, StealthReveal.Get
    if not V(StealthReveal.TeamSub) then
        StealthReveal.TeamSub = nil
        for _, s in ipairs(G(function() return FindAllOf("LyraTeamSubsystem") end) or {}) do
            if V(s) and not StealthReveal.Name(s):find("Default__", 1, true) then StealthReveal.TeamSub = s; break end
        end
    end
    local own = StealthReveal.TeamSub and G(function() return StealthReveal.TeamSub:GetOwnPlayer() end) or nil
    StealthReveal.OwnPawn = V(own) and G(function() return own.Pawn end) or nil
    local N = StealthReveal.Names()
    local targets, states, skip = {}, {}, {}
    for _, st in ipairs(G(function() return FindAllOf("BP_MaterialState_Stealth_C") end) or {}) do
        if V(st) and not StealthReveal.Name(st):find("Default__", 1, true) then
            StealthReveal.HookState(st)
            local sa = StealthReveal.Addr(st)
            local mgr = G(function() return st.Owner:Get() end)
            if not V(mgr) then mgr = G(function() return st.Owner end) end
            local cur = V(mgr) and G(function() return mgr.CurrentMaterialState end) or nil
            local cloak = V(mgr) and G(function() return mgr.CloakType end) or 0
            local active = V(cur) and StealthReveal.Addr(cur) == sa
            local pawn = (active and (cloak == 2 or cloak == 3)) and G(function() return mgr:GetOwner() end) or nil
            if V(pawn) then
                local mids = StealthReveal.Mids(pawn)
                for _, m in ipairs(mids) do
                    pcall(function() m:SetScalarParameterValue(N.far, 0) end)
                end
                if #mids > 0 then
                    local old = StealthReveal.States[sa] or {}
                    local r = { pawn = pawn, mids = mids, last = old.last, m1 = old.m1,
                        frames = old.frames or 0, overPre = old.overPre or 0, overPost = old.overPost or 0,
                        overVal = old.overVal, loops = old.loops or 0 }
                    targets[#targets + 1] = r
                    states[sa] = r
                else
                    skip[sa] = true
                end
                local key = StealthReveal.Name(pawn)
                if not StealthReveal.Logged[key] then
                    StealthReveal.Logged[key] = true
                    Log(string.format("[STEALTH] enemy rogue %s in stealth: materials %d [%s]", key, #mids,
                        table.concat(StealthReveal.MeshNames, " ")))
                end
            else
                skip[sa] = true
            end
        end
    end
    StealthReveal.Targets, StealthReveal.States, StealthReveal.Skip = targets, states, skip
end

-- blend to an almost opaque look (Visibility) closer than solid_distance; 0 — leave alone
function StealthReveal.Solid(d)
    local c = StealthReveal.Cfg
    if c.solid_distance <= 0 or d >= c.solid_distance then return 0 end
    return c.solid_max * (c.solid_distance - d) / c.solid_distance
end

-- stealth reveal strength S (StealthVisibleDistance, 0..1) by distance
function StealthReveal.Opacity(d)
    local c = StealthReveal.Cfg
    if d >= c.distance then return 0 end
    if d <= c.full_distance then return c.opacity_near end
    local t = (c.distance - d) / math.max(1, c.distance - c.full_distance)
    return c.opacity_far + (c.opacity_near - c.opacity_far) * (t ^ math.max(0.01, c.curve))
end

-- write the strength (and Visibility) into all MIDs of the target; phase: "pre"/"post" (frame hook) or "loop" (timer)
function StealthReveal.Apply(r, phase)
    local V, G, N = StealthReveal.Valid, StealthReveal.Get, StealthReveal.Names()
    if not V(r.pawn) or not V(StealthReveal.OwnPawn) then return end
    local ox, oy, oz = StealthReveal.Loc(StealthReveal.OwnPawn)
    local x, y, z = StealthReveal.Loc(r.pawn)
    if not ox or not x then return end
    local d = math.sqrt((x - ox) ^ 2 + (y - oy) ^ 2 + (z - oz) ^ 2)
    local v, sv = StealthReveal.Opacity(d), StealthReveal.Solid(d)
    local useSolid = StealthReveal.Cfg.solid_distance > 0
    -- ghost: the curve goes into Visibility (the own look), the outline strength — glow, ISEmissive on inside `distance`
    local ghost = StealthReveal.Cfg.ghost == 1
    local ise = 0
    if ghost then
        sv, useSolid, ise = v, true, (v > 0 and 1 or 0)
        v = (v > 0) and StealthReveal.Cfg.glow or 0
    end
    -- check "someone overwrites the strength": what is in the first MID before our write (the same MID as last time)
    local m1 = r.mids[1]
    local a1 = StealthReveal.Addr(m1)
    if r.last and r.m1 == a1 and V(m1) then
        local cur = G(function() return m1:K2_GetScalarParameterValue(N.svd) end)
        if phase == "pre" then r.frames = r.frames + 1 end
        if phase == "loop" then r.loops = r.loops + 1 end
        if type(cur) == "number" and math.abs(cur - r.last) > 1e-4 then
            if phase == "post" then r.overPost = r.overPost + 1 else r.overPre = r.overPre + 1 end
            r.overVal = cur
        end
    end
    local okn = 0
    for _, m in ipairs(r.mids) do
        if V(m) then
            if pcall(function() m:SetScalarParameterValue(N.svd, v) end) then okn = okn + 1 end
            pcall(function() m:SetScalarParameterValue(N.far, 0) end)
            -- the BP resets Visibility to 0 on mesh changes — write every time
            if useSolid then pcall(function() m:SetScalarParameterValue(N.vis, sv) end) end
            if ghost then pcall(function() m:SetScalarParameterValue(N.ise, ise) end) end
        end
    end
    r.last, r.m1, r.vis, r.ise = v, a1, sv, ise
    -- diagnostics: every 2 s, at most 300 lines. frames — how many times the hook fired before the BP; foreign write before/after —
    -- how many times the strength in the MID turned out not ours before the write (before — between frames, after — inside OnTickState BP)
    if phase ~= "pre" and os.time() - StealthReveal.DiagAt >= 2 and StealthReveal.DiagN < 300 then
        StealthReveal.DiagAt, StealthReveal.DiagN = os.time(), StealthReveal.DiagN + 1
        Log(string.format("[STEALTH] d=%.0f strength=%.3f solid=%.3f emissive=%d written %d/%d; frames %d, timer %d, foreign write before %d after %d (was %s)",
            d, v, sv, ise, okn, #r.mids, r.frames, r.loops, r.overPre, r.overPost, tostring(r.overVal)))
        r.frames, r.loops, r.overPre, r.overPost, r.overVal = 0, 0, 0, 0, nil
    end
end

-- frame hook: OnTickState of the stealth state (before and after the BP)
function StealthReveal.OnStateTick(self, phase)
    local st = StealthReveal.Get(function() return self:get() end)
    if not StealthReveal.Valid(st) then return end
    local sa = StealthReveal.Addr(st)
    if StealthReveal.Skip[sa] then return end
    local r = StealthReveal.States[sa]
    local now = os.clock() * 1000
    if not r then
        -- a new state (the rogue has just gone into stealth) — find its meshes at once, do not wait for the timer
        if now - StealthReveal.LastScan < 100 then return end
        StealthReveal.LastScan = now
        StealthReveal.Scan()
        r = StealthReveal.States[sa]
        if not r then return end
    end
    StealthReveal.LastHookAt = now
    StealthReveal.Apply(r, phase)
end

-- a new stealth MID (recreated on a new stealth entry / mesh change): into the target at once and write
function StealthReveal.OnReplace(self, newMid)
    StealthReveal.LastScan = -1e9 -- full search on the nearest timer step
    local V, G, N = StealthReveal.Valid, StealthReveal.Get, StealthReveal.Names()
    local st = G(function() return self:get() end)
    local r = V(st) and StealthReveal.States[StealthReveal.Addr(st)] or nil
    local m = newMid and G(function() return newMid:get() end) or nil
    if not r or not V(m) then return end
    if StealthReveal.Name(G(function() return m.Parent end)) ~= STEALTH_PARENT then return end
    local ma = StealthReveal.Addr(m)
    for _, x in ipairs(r.mids) do if StealthReveal.Addr(x) == ma then return end end
    r.mids[#r.mids + 1] = m
    pcall(function() m:SetScalarParameterValue(N.far, 0) end)
    if r.last then pcall(function() m:SetScalarParameterValue(N.svd, r.last) end) end
    -- the BP sets Visibility/ISEmissive of a new MID to 0 for an enemy — give it the current ghost values at once
    if r.vis then pcall(function() m:SetScalarParameterValue(N.vis, r.vis) end) end
    if r.ise then pcall(function() m:SetScalarParameterValue(N.ise, r.ise) end) end
end

-- hooks on the state class (the class path is taken from the found object; one attempt)
function StealthReveal.HookState(st)
    if StealthReveal.Hooked then return end
    StealthReveal.Hooked = true
    local full = StealthReveal.Get(function() return st:GetClass():GetFullName() end) or ""
    local cls = full:match("^%S+%s+(.+)$")
    if not cls then
        Log("[STEALTH] state class path not found (" .. full .. ") — writing by timer only")
        return
    end
    local ok1, e1 = pcall(function()
        RegisterHook(cls .. ":OnTickState",
            function(self) pcall(StealthReveal.OnStateTick, self, "pre") end,
            function(self) pcall(StealthReveal.OnStateTick, self, "post") end)
    end)
    local ok2, e2 = pcall(function()
        RegisterHook(cls .. ":OnOriginReplace", function() end,
            function(self, _, newMid) pcall(StealthReveal.OnReplace, self, newMid) end)
    end)
    local ok3, e3 = pcall(function()
        RegisterHook(cls .. ":OnMeshUpdate", function() end, function() StealthReveal.LastScan = -1e9 end)
    end)
    Log(string.format("[STEALTH] hooks %s: OnTickState %s, OnOriginReplace %s, OnMeshUpdate %s", cls,
        ok1 and "yes" or ("no " .. tostring(e1)), ok2 and "yes" or ("no " .. tostring(e2)),
        ok3 and "yes" or ("no " .. tostring(e3))))
end

-- timer: search every scan_ms; write — only if the frame hook is silent (did not attach or is not called)
function StealthReveal.Tick()
    local now = os.clock() * 1000
    if now - StealthReveal.LastScan >= StealthReveal.Cfg.scan_ms then
        StealthReveal.LastScan = now
        StealthReveal.Scan()
    end
    if now - StealthReveal.LastHookAt < 300 then return end
    for _, r in ipairs(StealthReveal.Targets) do StealthReveal.Apply(r, "loop") end
end

if not IsServerProc and StealthReveal.Cfg.enabled == 1 then
    GameLoop(math.max(16, StealthReveal.Cfg.update_ms), function()
        StealthReveal.Tick()
        return false
    end)
end
