-- Mods/DungeonborneRuServer/Scripts/main.lua
-- Headless game server: a listen client (-nullrhi) with no human involved.
-- Enabled ONLY by the -dbserver command-line flag — for regular clients the mod does nothing.
-- Environment variables do not work: Dungeonborne.exe restarts itself under ACE with a clean environment,
-- while command-line arguments are passed on to the child process.
--
-- Chain:
--   login    -> the game does the SDK login itself; the mod presses "Start game" (WBP_LoginWindow_C:StartGame) until IsLogin
--   lobby    -> wait for the lobby map RuinedCrypt_03
--   start    -> after -dbdelay s: BattleService:StartLocalBattle(<map>?listen)
--   hosting  -> the world has a GameMode (we are the server) on the match map
--   observer -> host player: bOnlySpectator, StartSpectatingOnly, DestroyCharacter (like host_observer_on in LocalDev)
--   ready    -> wait for players; dbserver-status.json is read by ds-agent
--   standby  -> (-dbstandby=1) in the lobby without a map until a host task arrives; after the match exiting -> quit (one process per match)
--
-- Client role (-dbclient) — a second instance for autotests without a human:
--   login -> lobby -> connect (console `open <ip:port>`) -> connected (own hero in the match world)
--   or, if the mod folder has dbclient-match.txt (native | local): login -> lobby -> match (click the match button
--   WBP_MatchMainButton_C:OnBtnMatchClicked — like a human) -> connecting -> connected. native — the native match
--   (pre_search/search → room → dsInitFinish → the client travels to the server by itself; needs match-mode.txt = native in
--   RebuildProbe, set by dbtest\run.ps1 -Match native), local — the regular LocalDev button (StartLocalBattle).
--
-- Command-line flags:
--   -dbserver             enable the mod (server role)
--   -dbclient             enable the mod (client role)
--   -dbconnect=ip:port    where the client connects (default 127.0.0.1:7777)
--   -dbroot=path          mod files folder (default RebuildProbe); server and client use different file names
--   -dbmap=...            map path (default: Classic crypt)
--   -dbdelay=N            pause before StartLocalBattle / open, s (default 5)
--   -dblogindelay=N       pause between WaitingStartGame and clicking "Start game", s (default 3)
--   -dbobserver=0         do not turn the host into an observer
--   -dbstandby=1          one process per match: wait for a task in the lobby, exit after the match (ds-agent starts a new one)
-- Files in the mod folder: <role>.log, <role>-status.json, <role>-control.txt (role = dbserver | dbclient).
-- Control: <role>-control.txt with the word quit -> console command quit
-- (the process runs under ACE: taskkill gives Access denied, it can only exit by itself).
--
-- 30.10: FindFirstOf may return the template object Default__<Class>; calling a UFunction on it crashed the game
-- (ACCESS_VIOLATION 0x60 on StartGame on WBP_LoginWindow_C). Hence FindLive() only.

local Root = (os.getenv("LOCALAPPDATA") or ".") .. "/Dungeonborne/Saved/RebuildProbe/"
local Role = "dbserver" -- dbserver | dbclient; prefix of the mod files
local Inst = ""         -- "-<port>" for a server with -port=N (N != 7777): each instance has its own files
local Port = 7777
local DEFAULT_MAP = "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic"
local MapPath = DEFAULT_MAP
local ConnectAddr = "127.0.0.1:7777"
local StartDelay = 5
local LoginClickDelay = 3
local UseObserver = true
-- 04.11 standby mode (-dbstandby=1, ds-agent): one process per match. After the lobby the map is NOT loaded (GameMode starts
-- the game itself), we wait for a host task; the hall and the match — on command; after the match — exit (heartbeat exiting), the agent starts a new one.
-- ExitAt — when the exit began, ExitSent — backend got exiting, QuitWritten — quit written to the control file.
local Standby = { On = false, ExitAt = nil, ExitSent = false, QuitWritten = false }
local CONNECT_TIMEOUT_SEC = 90
local FakeDsArg = false -- -dbfakeds=1: IsDedicatedServer = true in the server process
local MatchMode = nil -- client: "native" | "local" from dbclient-match.txt — press the match button instead of open
local MATCH_RETRY_SEC = 120
local PROBE_AFTER_SEC = 10
local ProbeDone = false
local Probe, InstallFakeDs, InstallRepTrace, AutoProbe, RepFlags -- defined below, needed earlier (Init, client steps, CheckControlAsync)
local ApplyAffinity, LogPerf -- 03.11: measuring several servers (below, near NativeCall)
local BeginReset -- 03.11: server reset after the room closes (below, near SlotTick); needed in SendResults
local ReportKill -- 03.11: who killed the dead player (below, near Ready); called on a "death" result
local Epoch = 0      -- map load number for backend (/ds/slot): os.time() at process start and on every reset
local SlotAddr = ""  -- -dbslot=ip:port — slot address as in backend DS_POOL (empty — backend looks it up by IP and port)
local ResetOldGM = nil -- address and name of the GameMode before the map reload: Hosting waits for another one
local OnHost          -- 03.11: room task from /ds/slot (action=host) — waiting hall (below, near WaitingTick)
local Host = nil      -- current task: { Battle, Scenes, Timeout, Members, Total, Phase = loading/waiting/entering/battle }
local IdleMap = nil   -- idle map (-dbmap; with a task — the hall from it): the server returns to it after a reset
local TravelPending = nil -- { From, To, At }: after travel, on ready, the mod's match state is taken anew

local LOBBY_MAP = "RuinedCrypt_03"
-- map name -> map_id (map_detail), for InitGameMap
local MAP_IDS = {
    L_Dungeon_Official_01 = 10101, L_Dungeon_Official_01_Classic = 10102, L_Dungeon_Official_01_Hard = 10103,
    L_GothicCastle_WP_Main = 10201, L_GothicCastle_WP_Main_Classic = 10202, L_GothicCastle_WP_Main_Hard = 10203,
}
local TICK_MS = 1000
local HOSTING_TIMEOUT_SEC = 180
local OBSERVER_DELAY_SEC = 5 -- the host's pawn must appear before we remove it

local LogFile = nil
local function Log(msg)
    local s = tostring(msg)
    if #s > 300 then s = s:sub(1, 300) .. "..." end
    print("[DungeonborneRuServer] " .. s .. "\n")
    if not LogFile then LogFile = io.open(Root .. Role .. Inst .. ".log", "a") end
    if LogFile then
        LogFile:write(os.date("%H:%M:%S") .. " | " .. s .. "\n")
        LogFile:flush()
    end
end

local function ReadFile(Name)
    local File = io.open(Root .. Name, "r")
    if not File then return nil end
    local Content = File:read("*a"); File:close()
    return Content
end

local function WriteFile(Name, Content)
    local File = io.open(Root .. Name, "w")
    if not File then return false end
    File:write(Content); File:close()
    return true
end

local function IsValid(Obj)
    return Obj ~= nil and Obj:IsValid()
end

local function FullName(Obj)
    local Ok, Name = pcall(function() return Obj:GetFullName() end)
    return Ok and Name or ""
end

-- Live instance of a class: not a Default__ template and not from the /Script package
local function FindLive(ClassName)
    for _, Obj in ipairs(FindAllOf(ClassName) or {}) do
        if IsValid(Obj) and not FullName(Obj):find("Default__", 1, true) then return Obj end
    end
    return nil
end

-- World map from the actor's full name: "<Class> /Path/Map.Map:PersistentLevel.<Name>"
local function MapOf(Actor)
    return FullName(Actor):match("%s(/[^%.%s]+)%.[^:]+:PersistentLevel") or ""
end

-- --- State -------------------------------------------------------------------------------
local Phase = "init"
local PhaseSince = os.time()
local HostedMap = ""

local function WriteStatus()
    WriteFile(Role .. Inst .. "-status.json", string.format(
        '{"role":"%s","phase":"%s","map":"%s","observer":%s,"updated_at":"%s"}',
        Role, Phase, HostedMap, tostring(UseObserver), os.date("%Y-%m-%d %H:%M:%S")))
end

local function SetPhase(NewPhase, Detail)
    if Phase == NewPhase then return end
    Phase, PhaseSince = NewPhase, os.time()
    Log("phase -> " .. NewPhase .. (Detail and (" | " .. Detail) or ""))
    WriteStatus()
end

local Ksl = nil
local function KismetSystem()
    if not IsValid(Ksl) then Ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary") end
    return IsValid(Ksl) and Ksl or nil
end

local function ArgValue(CommandLine, Name)
    return CommandLine:match("%-" .. Name .. "=\"([^\"]*)\"") or CommandLine:match("%-" .. Name .. "=(%S+)")
end

-- --- Steps -------------------------------------------------------------------------------
-- Flags are read from the UE command line (KismetSystemLibrary.GetCommandLine; library CDO — static functions)
local function Init()
    local Lib = KismetSystem()
    if not Lib then return end
    local CommandLine = Lib:GetCommandLine():ToString()
    local Padded = " " .. CommandLine:lower() .. " "
    if Padded:find("%s%-dbserver%s") then
        Role = "dbserver"
    elseif Padded:find("%s%-dbclient%s") then
        Role = "dbclient"
    else
        print("[DungeonborneRuServer] no -dbserver / -dbclient — mod disabled\n")
        Phase = "off"
        return
    end
    local CustomRoot = ArgValue(CommandLine, "dbroot")
    if CustomRoot then Root = CustomRoot:gsub("\\", "/"):gsub("/?$", "/") end
    MapPath = ArgValue(CommandLine, "dbmap") or DEFAULT_MAP
    IdleMap = MapPath
    ConnectAddr = ArgValue(CommandLine, "dbconnect") or ConnectAddr
    StartDelay = tonumber(ArgValue(CommandLine, "dbdelay") or "") or StartDelay
    LoginClickDelay = tonumber(ArgValue(CommandLine, "dblogindelay") or "") or LoginClickDelay
    UseObserver = ArgValue(CommandLine, "dbobserver") ~= "0"
    Standby.On = ArgValue(CommandLine, "dbstandby") == "1"
    -- flag or file <mod folder>/dbserver-fakeds.txt = 1 (a file — so the admin launch dbtools.ps1 need not change)
    FakeDsArg = ArgValue(CommandLine, "dbfakeds") == "1" or (ReadFile("dbserver-fakeds.txt") or ""):match("^%s*1") ~= nil
    -- 03.11: several servers from one install — with -port=N (N != 7777) own files <role>-N.log/-status/-control
    -- and exchange with dbnative via req-N.txt / rsp-N.txt (same rule in dbnative.c)
    Port = tonumber(ArgValue(CommandLine, "port") or "") or 7777
    SlotAddr = ArgValue(CommandLine, "dbslot") or ""
    Epoch = os.time()
    if Port ~= 7777 then
        Inst = "-" .. Port
        if LogFile then LogFile:close(); LogFile = nil end
    end
    if not WriteFile(Role .. Inst .. "-status.json", "{}") then
        os.execute('mkdir "' .. Root:gsub("/", "\\") .. '" 2>nul')
    end
    Log("cmdline: " .. CommandLine)
    if Role == "dbclient" then
        local Mode = (ReadFile("dbclient-match.txt") or ""):match("^%s*(%a+)")
        if Mode == "native" or Mode == "local" or Mode == "localthen" then MatchMode = Mode end
        Log(string.format("config | role=client connect=%s match=%s delay=%d root=%s", ConnectAddr,
            tostring(MatchMode), StartDelay, Root))
        -- 19.11: dbclient-usmap.txt = 1 -> only the game's mappings (phase usmap, Standby.UsmapStep), then quit;
        -- its lines starting with / are assets loaded before the dump (their Blueprint classes get into the usmap)
        local UsmapText = ReadFile("dbclient-usmap.txt") or ""
        Standby.Usmap = UsmapText:match("^%s*1") ~= nil
        Standby.UsmapLoad = {}
        for Line in UsmapText:gmatch("[^\r\n]+") do
            if Line:match("^%s*/") then table.insert(Standby.UsmapLoad, (Line:gsub("^%s+", ""):gsub("%s+$", ""))) end
        end
    else
        Log(string.format("config | role=server map=%s delay=%d observer=%s fakeds=%s root=%s", MapPath, StartDelay,
            tostring(UseObserver), tostring(FakeDsArg), Root))
        ApplyAffinity()
    end
    InstallFakeDs()
    InstallRepTrace()
    SetPhase(Standby.Usmap and "usmap" or "login")
end

local function IsLoggedIn()
    local Svc = FindLive("GameLoginService")
    if not Svc then return false end
    local Ok, Value = pcall(function() return Svc:IsLogin() end)
    return Ok and Value == true
end

-- A client in a match has no GameMode (it exists only on the server) — then the map comes from GameState
local function CurrentMap()
    local GM = FindLive("GameModeBase")
    if GM then return MapOf(GM) end
    local GS = FindLive("GameStateBase")
    return GS and MapOf(GS) or ""
end

-- StartLocalBattle first sends battle_fetch_battle_user_data to the gateway: without a login it times out -101 after 16 s.
-- The game does the SDK login (Steam) itself; only the "Start game" button is needed (ULoginWindow::StartGame).
-- 30.10: StartGame() before the WaitingStartGame state crashes the game (AV 0x60, frame 1) — wait for OnEnterStateWaitingStartGame.
-- A hook on /Script/TGFGame.LoginWindow does not fire: ProcessEvent goes to the blueprint override.
-- Hook the functions of WBP_LoginWindow_C (the class is loaded once the window is alive).
local LOGIN_BP = "/Game/TGF/UI/WBP/WBP_LoginWindow.WBP_LoginWindow_C:"
local LOGIN_STATES = { "None", "QueryDir", "SDKLogin", "WaitingSDKLogin", "WaitingStartGame",
    "ConnectServer", "GameLogin", "SyncPlayerInfo", "LoadHall" }
local LoginState, LoginStateAt = nil, nil
local WindowSeenAt = nil
local StartGameAt = nil
local LoginHooksDone = false

local function InstallLoginHooks()
    if LoginHooksDone then return end
    LoginHooksDone = true
    local Done = {}
    for _, State in ipairs(LOGIN_STATES) do
        local Ok = pcall(function()
            RegisterHook(LOGIN_BP .. "OnEnterState" .. State, function()
                LoginState, LoginStateAt = State, os.time()
                Log("login: state -> " .. State)
            end)
        end)
        Done[#Done + 1] = State .. "=" .. tostring(Ok)
    end
    Log("login: hooks " .. table.concat(Done, " "))
end

-- 30.10: a direct ULoginWindow::StartGame() crashes (AV 0x60) even in WaitingStartGame — call the button click
-- handler from the blueprint, as when a human presses it (it prepares the server/save and calls StartGame itself).
local START_BUTTON_EVENT = "BndEvt__WBP_LoginWindow_StartGameButton_K2Node_ComponentBoundEvent_1_OnButtonClickedEvent__DelegateSignature"

local LOGIN_RETRY_DELAY = 2
local AlertDismissed = false

local function DismissAlerts()
    local Count = 0
    for _, Alert in ipairs(FindAllOf("AlertWindow") or {}) do
        if IsValid(Alert) and not FullName(Alert):find("Default__", 1, true) then
            local Text = ""
            pcall(function() Text = Alert.AlertWindowArgs.Message:ToString() end)
            local Ok, Err = pcall(function() Alert:OnConfirmButtonClicked() end)
            Count = Count + 1
            Log("login: alert «" .. Text .. "» OK ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
        end
    end
    if Count == 0 then Log("login: no live AlertWindow (state=" .. tostring(LoginState) .. ")") end
end

local function CallStartGame(Window, Why)
    StartGameAt = os.time()
    AlertDismissed = false
    local Server = nil
    pcall(function() Server = Window.CurrentServerItem end)
    Log("login: CurrentServerItem valid=" .. tostring(IsValid(Server)) .. " " .. (IsValid(Server) and FullName(Server) or ""))
    local Ok, Err = pcall(function() Window[START_BUTTON_EVENT](Window) end)
    Log("login: клик StartGameButton (" .. Why .. ") ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
end

local function Login()
    if IsLoggedIn() then
        SetPhase("lobby", "IsLogin=true map=" .. CurrentMap())
        return
    end
    local Window = FindLive("WBP_LoginWindow_C")
    if not Window then
        if os.time() - PhaseSince > 90 then
            Log("login: no live WBP_LoginWindow_C for 90 s (objects " .. #(FindAllOf("WBP_LoginWindow_C") or {}) .. ")")
            PhaseSince = os.time()
        end
        return
    end
    if not WindowSeenAt then
        WindowSeenAt = os.time()
        Log("login: window " .. FullName(Window))
        InstallLoginHooks()
    end
    if StartGameAt then
        -- After a failure the window returns to WaitingStartGame — then wait and press again
        if LoginState == "WaitingStartGame" and LoginStateAt > StartGameAt then
            Log("login: window returned to WaitingStartGame — retry")
            StartGameAt = nil
            LoginStateAt = os.time() - LoginClickDelay + LOGIN_RETRY_DELAY
        elseif not AlertDismissed and os.time() - StartGameAt >= 5 then
            -- 30.10: the first connection after start fails (connect failed 201) and an error window hangs;
            -- a human presses OK -> WaitingStartGame -> "Start game" again -> login succeeds
            AlertDismissed = true
            DismissAlerts()
        elseif os.time() - StartGameAt > 60 then
            Log("login: 60 s after the click still no IsLogin (state=" .. tostring(LoginState) .. ")")
            StartGameAt = os.time()
        end
        return
    end
    -- 30.10: a click 2 s after WaitingStartGame → connect failed 201 ext:-6 (gateway without retries),
    -- a manual click later works — wait LoginClickDelay (SDK login/token get ready)
    if LoginState == "WaitingStartGame" and os.time() - LoginStateAt >= LoginClickDelay then
        CallStartGame(Window, "WaitingStartGame +" .. LoginClickDelay .. " s")
    -- Fallback: the state may have come before the hooks; the window lives 30 s without events
    elseif LoginState == nil and os.time() - WindowSeenAt >= 30 then
        CallStartGame(Window, "fallback: 30 s without state events")
    end
end

-- Wait for the lobby (like host_from_lobby: StartLocalBattle is called from the lobby)
local function Lobby()
    local Map = CurrentMap()
    if not Map:find(LOBBY_MAP, 1, true) then
        if os.time() - PhaseSince > 120 then
            Log("lobby: no lobby map within 120 s (map=" .. Map .. ")")
            PhaseSince = os.time()
        end
        return
    end
    local ClientNext = MatchMode and "match" or "connect"
    SetPhase(Role == "dbclient" and ClientNext or (Standby.On and "standby" or "start"), "map=" .. Map)
end

-- --- Client ------------------------------------------------------------------------------
local ConnectAt = nil

-- Address for open + URL options from dbclient-urlopts.txt (31.10: "?OpenId=<steam id>" — as in the native connection
-- to the DS "%s?OpenId=%s?token=%s"; the server uses it to learn who connected)
local function ConnectUrl()
    return ConnectAddr .. ((ReadFile("dbclient-urlopts.txt") or ""):match("^%s*(%S*)") or "")
end

local function Connect()
    if os.time() - PhaseSince < StartDelay then return end
    local PC = FindLive("PlayerController")
    local Lib = KismetSystem()
    if not (PC and Lib) then return end
    ConnectAt = os.time()
    local Url = ConnectUrl()
    local Ok, Err = pcall(function() Lib:ExecuteConsoleCommand(PC, "open " .. Url, PC) end)
    Log("connect: open " .. Url .. " ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
    if not Ok then PhaseSince = os.time(); return end
    SetPhase("connecting")
end

-- Match button in the lobby. Its handler OnBtnMatchClicked is an UnLua Lua function (not a UFunction, cannot be called
-- from UE4SS, 31.10: "attempt to call a TrivialObject"), and a UMG button click is a delegate. So we call what
-- the button ends up in:
--   native — UMatchService:StartMatch(LevelType, LevelId, Region) with the selected level (original button:
--            OnTryMatchEvent → native match); UI checks (tickets, gear score) are skipped;
--   local  — BattleService:StartLocalBattle(map) — the same as LocalDev.StartSelectedBattle.
local LOCAL_LEVEL_MAPS = {
    [40003016] = "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main",
    [40003018] = "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Classic",
    [40003020] = "/TgfMaps/Scenes/GothicCastle/L_GothicCastle_WP_Main_Hard",
    [40003015] = "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01",
    [40003017] = "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Classic",
    [40003019] = "/TgfMaps/Scenes/Dungeon/L_Dungeon_Official_01_Hard",
}

local function Match()
    if os.time() - PhaseSince < StartDelay then return end
    local Svc = FindLive("MatchService")
    if not Svc then
        if os.time() - PhaseSince > 60 then
            Log("match: no live MatchService within 60 s")
            PhaseSince = os.time()
        end
        return
    end
    local LevelId, LevelType, Region = 40003017, 2001, "eu"
    pcall(function()
        local Info = Svc:GetSelectedLevelInfo()
        if Info.LevelId and Info.LevelId ~= 0 then LevelId = Info.LevelId end
        if Info.LevelType and Info.LevelType ~= 0 then LevelType = Info.LevelType end
    end)
    ConnectAt = os.time()
    local Ok, Err
    if MatchMode == "localthen" then
        -- 31.10: the client first runs StartLocalBattle itself (its UBattleService gets ITS OWN match data), then,
        -- already in its own local match, opens the server (step localbattle)
        local Map = LOCAL_LEVEL_MAPS[LevelId] or DEFAULT_MAP
        local Battle = FindLive("BattleService")
        Ok, Err = pcall(function() Battle:StartLocalBattle(FName(Map)) end)
        Log(string.format("match: localthen StartLocalBattle(%s) ok=%s%s", Map, tostring(Ok), Ok and "" or (" err=" .. tostring(Err))))
        if not Ok then PhaseSince = os.time(); return end
        SetPhase("localbattle")
        return
    elseif MatchMode == "native" then
        Ok, Err = pcall(function() Svc:StartMatch(LevelType, LevelId, Region) end)
        Log(string.format("match: MatchService:StartMatch(%d, %d, %s) ok=%s%s", LevelType, LevelId, Region,
            tostring(Ok), Ok and "" or (" err=" .. tostring(Err))))
    else
        local Map = LOCAL_LEVEL_MAPS[LevelId] or DEFAULT_MAP
        local Battle = FindLive("BattleService")
        Ok, Err = pcall(function() Battle:StartLocalBattle(FName(Map)) end)
        Log(string.format("match: StartLocalBattle(%s) for level %d ok=%s%s", Map, LevelId, tostring(Ok),
            Ok and "" or (" err=" .. tostring(Err))))
    end
    if not Ok then PhaseSince = os.time(); return end
    SetPhase("connecting")
end

-- localthen: wait for own local match (not the lobby, own pawn), after 5 s — open to the server
local LocalBattleSince = nil
local function LocalBattle()
    local Map = CurrentMap()
    local Pawn = nil
    pcall(function() Pawn = FindLive("PlayerController").Pawn end)
    local InBattle = Map ~= "" and not Map:find(LOBBY_MAP, 1, true) and not Map:find("/Maps/Empty", 1, true) and IsValid(Pawn)
    if not InBattle then
        LocalBattleSince = nil
        if os.time() - PhaseSince > 120 then Log("localbattle: no own match within 120 s (map=" .. Map .. ")"); PhaseSince = os.time() end
        return
    end
    LocalBattleSince = LocalBattleSince or os.time()
    if os.time() - LocalBattleSince < 5 then return end
    local PC = FindLive("PlayerController")
    local Lib = KismetSystem()
    ConnectAt = os.time()
    local Url = ConnectUrl()
    local Ok, Err = pcall(function() Lib:ExecuteConsoleCommand(PC, "open " .. Url, PC) end)
    Log("localbattle: in own match (" .. Map .. "), open " .. Url .. " ok=" .. tostring(Ok))
    if not Ok then PhaseSince = os.time(); return end
    SetPhase("connecting")
end

-- Connected: the map is not the lobby and the world has our hero (the pawn of our PlayerController)
local function Connecting()
    local Map = CurrentMap()
    local Pawn = nil
    pcall(function() Pawn = FindLive("PlayerController").Pawn end)
    -- /Game/TGF/Maps/Empty — transition map during travel (DefaultPawn there), not a match
    if Map ~= "" and not Map:find(LOBBY_MAP, 1, true) and not Map:find("/Maps/Empty", 1, true) and IsValid(Pawn) then
        HostedMap = Map
        SetPhase("connected", "map=" .. Map .. " pawn=" .. FullName(Pawn))
        return
    end
    local Timeout = MatchMode and MATCH_RETRY_SEC or CONNECT_TIMEOUT_SEC
    if os.time() - ConnectAt > Timeout then
        local Retry = MatchMode and "match" or "connect"
        Log("connect: not in a match after " .. Timeout .. " s (map=" .. Map .. ") — again " .. Retry)
        if Map:find(LOBBY_MAP, 1, true) or Map == "" then SetPhase(Retry) else ConnectAt = os.time() end
    end
end

local function Connected()
    local Map = CurrentMap()
    if Map ~= HostedMap then SetPhase("ended", "map changed: " .. Map); return end
    AutoProbe()
end

local function Start()
    local Battle = FindLive("BattleService")
    if not Battle then
        if os.time() - PhaseSince > 60 then
            Log("start: BattleService did not appear within 60 s")
            PhaseSince = os.time()
        end
        return
    end
    if os.time() - PhaseSince < StartDelay then return end
    -- 31.10 experiment: the map via the DS path — UBattleDsUtility.InitGameMap(map_id, timeout, IsIndependentMapMode).
    -- <mod folder>/dbserver-initmap.txt: only — InitGameMap only (will the game open the map itself), before — InitGameMap,
    -- then StartLocalBattle as before. Empty — as before.
    local InitMode = (ReadFile("dbserver-initmap.txt") or ""):match("^%s*(%a+)")
    if InitMode == "only" or InitMode == "before" then
        local MapId = MAP_IDS[MapPath:match("([^/]+)$") or ""] or 10102
        local Ds = FindLive("BattleDsUtility")
        local Ok, Err = pcall(function() Ds:InitGameMap(MapId, 2400, false) end)
        Log(string.format("InitGameMap(%d, 2400, false) [%s] ok=%s%s", MapId, InitMode, tostring(Ok), Ok and "" or (" err=" .. tostring(Err))))
        if InitMode == "only" then
            SetPhase("hosting", "InitGameMap only")
            return
        end
    end
    local Target = MapPath .. "?listen"
    -- 01.11: like LocalDev.StartSelectedBattle (PrivateWorld.Prepare) — without it the server loads World Partition cells
    -- only around its own (observer) point, and mobs spawn only there, clients see nothing
    -- Enabled by the file dbserver-wpstream.txt = 0 (did not affect mob spawning, off by default)
    if (ReadFile("dbserver-wpstream.txt") or ""):match("^%s*0") then
        pcall(function()
            local Lib = KismetSystem()
            local PC = FindLive("PlayerController")
            Lib:ExecuteConsoleCommand(PC, "wp.Runtime.EnableServerStreaming 0", PC)
            Log("wp.Runtime.EnableServerStreaming 0")
        end)
    end
    -- 31.10: open — the map via the console command open <map>?listen, without StartLocalBattle (and without its
    -- battle_fetch_battle_user_data): what the native InitPlayerGameData then does with players
    if InitMode == "open" then
        local Lib = KismetSystem()
        local PC = FindLive("PlayerController")
        local Ok, Err = pcall(function() Lib:ExecuteConsoleCommand(PC, "open " .. Target, PC) end)
        Log("open " .. Target .. " ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
        if not Ok then PhaseSince = os.time(); return end
        SetPhase("hosting", "open")
        return
    end
    -- The MapName parameter is an FName: UE4SS does not convert a string itself (a Lua string crashed UE4SS, AV 0x70)
    local Ok, Err = pcall(function() Battle:StartLocalBattle(FName(Target)) end)
    Log("BattleService:StartLocalBattle(" .. Target .. ") ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
    if not Ok then PhaseSince = os.time(); return end
    SetPhase("hosting", FullName(Battle))
end

-- B13d: Standby.HostingRetries — StartLocalBattle retries after a hosting timeout (reset on success). Without new
-- locals: the main block already has 200 of them (Lua limit).
Standby.HostingRetries = 0

local function Hosting()
    -- after a reset (StartLocalBattle on the same map) the old GameMode is still alive until GC collects it. Before 05.11 we took
    -- the first live one (FindLive): if the old one always came first, the new map "did not load" for hours (B13d,
    -- 11:42 and 13:23). Now — the first live GameMode that is not the old one and is on the right map.
    local GM, Map, Seen = nil, "", {}
    for _, G in ipairs(FindAllOf("GameModeBase") or {}) do
        if IsValid(G) and not FullName(G):find("Default__", 1, true) then
            local M = MapOf(G)
            local Old = ResetOldGM and string.format("%X %s", G:GetAddress(), FullName(G)) == ResetOldGM
            Seen[#Seen + 1] = (Old and "old " or "") .. (M ~= "" and M or "?")
            if not Old and M == MapPath then GM, Map = G, M end
        end
    end
    if not GM then
        if os.time() - PhaseSince > HOSTING_TIMEOUT_SEC then
            Log("hosting: map " .. MapPath .. " did not load within " .. HOSTING_TIMEOUT_SEC .. " s (GameMode: " .. (#Seen > 0 and table.concat(Seen, ", ") or "none") .. ")")
            Standby.HostingRetries = Standby.HostingRetries + 1
            if Standby.HostingRetries <= 1 then
                -- StartLocalBattle once more (the start phase calls Start on MapPath)
                SetPhase("start", "load retry " .. MapPath)
            else
                -- did not help — the process exits (on Robot start_headless/ds-agent starts a new one) instead of hanging in hosting
                Log("hosting: the retry did not help — process exit (quit via the control file)")
                WriteFile(Role .. Inst .. "-control.txt", "quit")
                PhaseSince = os.time()
            end
        end
        return
    end
    Standby.HostingRetries = 0
    HostedMap = Map
    ResetOldGM = nil
    Log("hosting: GameMode " .. FullName(GM))
    SetPhase(UseObserver and "observer" or "ready", "map=" .. Map)
end

-- Same as LocalDev host_observer_on, but without LocalDev (it starts only from the lobby)
-- The host's local controller (FindLive("PlayerController") may return someone else's after players join)
local function LocalPC()
    for _, C in ipairs(FindAllOf("BP_TgfPlayerController_C") or {}) do
        local IsLocal = false
        pcall(function() IsLocal = C:IsLocalController() end)
        if IsValid(C) and IsLocal and not FullName(C):find("Default__", 1, true) then return C end
    end
    return FindLive("PlayerController")
end

-- Steps from dbserver-observer.txt; the word late — 03.11 experiment: a real observer not at once, but
-- LATE_OBSERVER_SEC after the first player joins this map (at once — only quit hide freeze)
local function ObserverSteps()
    local Steps, Late = {}, false
    for W in (ReadFile("dbserver-observer.txt") or ""):gmatch("%a+") do
        if W == "late" then Late = true else Steps[W] = true end
    end
    if next(Steps) == nil then Steps = { quit = true, hide = true, freeze = true } end
    return Steps, Late
end

local PARK_DZ = 30000 -- park step: how many cm to lift the host's pawn above the map
local function MakeObserver(Steps)
    local PC = LocalPC()
    if not PC then return "no_player_controller" end
    local Pawn = nil
    pcall(function() Pawn = PC.Pawn end)
    local PS = nil
    pcall(function() PS = PC.PlayerState end)
    local GM = FindLive("GameModeBase")
    local Done = {}
    local function Try(Name, F)
        local Ok = pcall(F)
        Done[#Done + 1] = Name .. "=" .. tostring(Ok)
    end
    -- 01.11: DestroyCharacter breaks mob spawning: rooms (UPlayerDistanceCheckerComponent) take the player count from the
    -- world counter but iterate them via GetPlayerController(i), where the host is always index 0. Without the host's pawn the counter
    -- seems to be one less — the client with the last index is not checked, rooms are not populated for clients (mobs
    -- were only where the host's pawn stood for the first 5 s). We do not remove the host's pawn: hide it, no collision, immobile.
    -- dbserver-observer.txt — a space-separated list of steps from spec quit hide freeze destroy (spec = bOnlySpectator +
    -- StartSpectatingOnly, quit = IsSelfQuit, hide = hide + no collision, freeze = DisableMovement — without collision
    -- it otherwise falls through the floor, destroy = DestroyCharacter). Without the file — quit hide freeze.
    -- park — lift the pawn by PARK_DZ and disable damage (03.11).
    if IsValid(Pawn) and Steps.freeze then
        Try("Freeze", function()
            local Move = Pawn.CharacterMovement
            Move:StopMovementImmediately()
            Move:DisableMovement()
        end)
    end
    if IsValid(PS) then
        if Steps.spec then Try("bOnlySpectator", function() PS.bOnlySpectator = true end) end
        if Steps.quit then Try("IsSelfQuit", function() PS.IsSelfQuit = true end) end -- the host does not count as a live fighter
    end
    if IsValid(Pawn) and Steps.hide then
        Try("Hide", function() Pawn:SetActorHiddenInGame(true) end)
        Try("NoCollision", function() Pawn:SetActorEnableCollision(false) end)
    end
    -- park (03.11): do not destroy the host's pawn and do not make it an observer (breaks mob spawning — live test 15:30), but
    -- move it high above the map: movement is already off (freeze — does not fall), damage is off (zone/mobs will not kill it —
    -- otherwise the LOCAL record goes to 3/2), there are no room triggers above the map; KillZ is below, we do not lower it there
    if IsValid(Pawn) and Steps.park then
        Try("Park", function()
            local L = Pawn:K2_GetActorLocation()
            Pawn:K2_SetActorLocation({ X = L.X, Y = L.Y, Z = L.Z + PARK_DZ }, false, {}, true)
        end)
        Try("NoDamage", function() Pawn.bCanBeDamaged = false end)
        -- 03.11 (live test 3): a dead player spectating could switch to the host — the native target choice
        -- (UTgfSpectatorManagerComponent::TrySpectate) iterates the pawns the client has. The host's pawn is needed only
        -- by the server (room counters), not by clients: without replication its channel closes and it is not among spectate targets
        Try("NoReplicate", function() Pawn:SetReplicates(false) end)
    end
    if Steps.spec then Try("StartSpectatingOnly", function() PC:StartSpectatingOnly() end) end
    if IsValid(GM) and IsValid(Pawn) and Steps.destroy then
        Try("DestroyCharacter", function() GM:DestroyCharacter(PC) end)
    end
    local Names = {}
    for K in pairs(Steps) do Names[#Names + 1] = K end
    table.sort(Names)
    Done[#Done + 1] = "steps=" .. table.concat(Names, ",")
    return table.concat(Done, " ")
end

local LATE_OBSERVER_SEC = 5
local LateObserver = nil -- { Steps, Since } — the late observer for this map is not applied yet
local function Observer()
    if os.time() - PhaseSince < OBSERVER_DELAY_SEC then return end
    local Steps, Late = ObserverSteps()
    LateObserver = Late and { Steps = Steps } or nil
    local Result = MakeObserver(Late and { quit = true, hide = true, freeze = true } or Steps)
    Log("observer: " .. Result)
    if Result == "no_player_controller" then PhaseSince = os.time(); return end
    SetPhase("ready", "map=" .. HostedMap)
end

AutoProbe = function()
    if not ProbeDone and os.time() - PhaseSince >= PROBE_AFTER_SEC then
        ProbeDone = true
        Probe("auto +" .. PROBE_AFTER_SEC .. " s")
    end
end

-- 31.10 R14: server probe one tick after the connected client's hero appears (before the client crashes after ~2 s):
-- native equipment manager, rep (GetIsReplicated), item count -> dbserver-hero-probe.txt
local HeroCount, HeroProbeAt = 0, nil
-- The hero's equipment/inventory managers do not reach the client (31.10: their objects are not created on the client at all).
-- dbserver-fixrep.txt = 1: on first sight of the hero SetIsReplicated(false), then (true). Flags before/after
-- are written to dbserver.log ([REP]).
local REP_COMPS = { "/Script/LyraGame.TgfEquipmentManagerComponent", "/Script/LyraGame.TgfInventoryManagerComponent" }
local RepSeen = {}
local function FixRepOnHero(H, Fix, Mode)
    for _, Path in ipairs(REP_COMPS) do
        pcall(function()
            local C = H:GetComponentByClass(StaticFindObject(Path))
            if not IsValid(C) then Log("[REP] " .. (FullName(H):match("([^%.]+)$") or "?") .. " no " .. Path:match("%.([%w_]+)$")); return end
            local Before = RepFlags(C)
            local Did = ""
            -- 31.10: bReplicates is already true, but nothing reaches the client — toggling re-adds the component to the actor's
            -- replicated list (UpdateReplicatedComponent / AddComponentForReplication)
            if Fix then
                C:SetIsReplicated(false)
                C:SetIsReplicated(true)
                Did = " -> SetIsReplicated(false/true): " .. RepFlags(C)
            end
            Log(string.format("[REP] %s %s %s%s", FullName(H):match("([^%.]+)$") or "?", C:GetFName():ToString(), Before, Did))
        end)
    end
    -- comparison: components that do reach the client (hero attributes — a default subobject;
    -- the controller's TgfLyraInteractComponent — from the same GameFeature, the client had it as _<n>)
    pcall(function()
        Log("[REP] " .. (FullName(H):match("([^%.]+)$") or "?") .. " AttributeComponent " .. RepFlags(H.AttributeComponent))
    end)
    pcall(function()
        local PC = H:GetController()
        local C = IsValid(PC) and PC:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfLyraInteractComponent"))
        if IsValid(C) then Log("[REP] " .. (FullName(PC):match("([^%.]+)$") or "?") .. " TgfLyraInteractComponent " .. RepFlags(C)) end
    end)
    -- 31.10: even after toggling nothing reaches the client — the hero is not updated after the first send?
    -- Log dormancy/frequency and (Fix) wake it: FlushNetDormancy + ForceNetUpdate
    local Net = {}
    for _, P in ipairs({ "NetDormancy", "NetUpdateFrequency", "MinNetUpdateFrequency", "NetPriority", "bAlwaysRelevant",
        "bOnlyRelevantToOwner", "bReplicates", "bReplicateUsingRegisteredSubObjectList" }) do
        local V = "?"
        pcall(function() V = tostring(H[P]) end)
        Net[#Net + 1] = P .. "=" .. V
    end
    local Woke = ""
    -- 31.10: flags are the same on components that arrived and those that did not — look for a filter on the hero/player side:
    -- ALyraCharacter.ForceAlwaysRelevent / DisableDistanceSlowReplicateRate, ALyraPlayerState.HasInitedPlayerData
    -- (the native DS sets it after the player data from backend). Mode 3 — set it and wake the hero.
    local PS = nil
    pcall(function() PS = H:GetLyraPlayerState() end)
    if not IsValid(PS) then pcall(function() PS = H.PlayerState end) end
    local function Gates()
        local G = {}
        pcall(function() G[#G + 1] = "ForceAlwaysRelevent=" .. tostring(H.ForceAlwaysRelevent) end)
        pcall(function() G[#G + 1] = "DisableDistanceSlowReplicateRate=" .. tostring(H.DisableDistanceSlowReplicateRate) end)
        pcall(function() G[#G + 1] = "PS.HasInitedPlayerData=" .. tostring(PS.HasInitedPlayerData) end)
        return table.concat(G, " ")
    end
    local GateLine = Gates()
    if Mode == "3" then
        pcall(function() H.ForceAlwaysRelevent = true end)
        pcall(function() H.DisableDistanceSlowReplicateRate = true end)
        pcall(function() PS.HasInitedPlayerData = true end)
        local A = pcall(function() H:FlushNetDormancy() end)
        local B = pcall(function() H:ForceNetUpdate() end)
        GateLine = GateLine .. " -> " .. Gates() .. string.format(" Flush=%s Force=%s", tostring(A), tostring(B))
    end
    Log("[REP] " .. (FullName(H):match("([^%.]+)$") or "?") .. " gates " .. GateLine)
    if Fix then
        -- 31.10: even the woken hero does not send the managers — with RegisteredSubObjectList the components apparently did not get into
        -- the actor's list; switch the hero and managers to the old ReplicateSubobjects path (via ReplicatedComponents)
        local L = pcall(function()
            H.bReplicateUsingRegisteredSubObjectList = false
            for _, Path in ipairs(REP_COMPS) do
                local C = H:GetComponentByClass(StaticFindObject(Path))
                if IsValid(C) then C.bReplicateUsingRegisteredSubObjectList = false end
            end
        end)
        Woke = Woke .. string.format(" -> legacy subobjects=%s (actor now %s)", tostring(L),
            tostring(H.bReplicateUsingRegisteredSubObjectList))
        local A = pcall(function() H:FlushNetDormancy() end)
        local B = pcall(function() H:ForceNetUpdate() end)
        Woke = Woke .. string.format(" FlushNetDormancy=%s ForceNetUpdate=%s", tostring(A), tostring(B))
    end
    Log(string.format("[REP] %s net %s%s", FullName(H):match("([^%.]+)$") or "?", table.concat(Net, " "), Woke))
end
-- dbserver-fixrep.txt = 2: before clients join, disable RegisteredSubObjectList in the templates (CDO) of the hero and managers —
-- new heroes replicate components from birth via the old ReplicateSubobjects path (no race with the first send)
local CdoPatched = false
local REP_CDOS = { "/Game/TGF/Character/Hero/HeroSample/BP_Fighter_HeroSample.Default__BP_Fighter_HeroSample_C",
    "/Script/LyraGame.Default__TgfEquipmentManagerComponent", "/Script/LyraGame.Default__TgfInventoryManagerComponent" }
local function PatchRepCdos()
    if CdoPatched then return end
    CdoPatched = true
    for _, Path in ipairs(REP_CDOS) do
        local Ok, Res = pcall(function()
            local O = StaticFindObject(Path)
            if not IsValid(O) then return "no object" end
            local Was = tostring(O.bReplicateUsingRegisteredSubObjectList)
            O.bReplicateUsingRegisteredSubObjectList = false
            return Was .. " -> " .. tostring(O.bReplicateUsingRegisteredSubObjectList)
        end)
        Log("[REP] CDO " .. (Path:match("%.([%w_]+)$") or Path) .. " bRegSubObjList " .. tostring(Res) .. (Ok and "" or " (error)"))
    end
end
local function WatchClientHeroes()
    local N = 0
    local FixMode = (ReadFile("dbserver-fixrep.txt") or ""):match("^%s*(%d)")
    local Fix = FixMode == "1"
    if FixMode == "2" then PatchRepCdos() end
    for _, H in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(H) and not FullName(H):find("Default__", 1, true) then
            N = N + 1
            local Key = FullName(H)
            if not RepSeen[Key] then RepSeen[Key] = true; FixRepOnHero(H, Fix, FixMode) end
        end
    end
    if N > HeroCount then HeroProbeAt = os.time() + 2 end
    HeroCount = N
    if HeroProbeAt and os.time() >= HeroProbeAt then
        HeroProbeAt = nil
        Probe("client hero", "-hero")
    end
end

-- 01.11 tick rate: a listen server ticks at the frame rate (NetServerMaxTickRate applies only to a real
-- dedicated). Once a second take the frame duration, every 10 s — a [TICK] line in the log (avg/min/max rate).
local TickSamples = {}
local Gps = nil
local function SampleTick(Ctx)
    if not IsValid(Gps) then Gps = StaticFindObject("/Script/Engine.Default__GameplayStatics") end
    local Dt = nil
    pcall(function() Dt = Gps:GetWorldDeltaSeconds(Ctx) end)
    if type(Dt) ~= "number" or Dt <= 0 then return end
    TickSamples[#TickSamples + 1] = Dt
    if #TickSamples < 10 then return end
    local Sum, Min, Max = 0, math.huge, 0
    for _, V in ipairs(TickSamples) do Sum = Sum + V; Min = math.min(Min, V); Max = math.max(Max, V) end
    local MaxFps = "?"
    pcall(function() MaxFps = string.format("%.0f", KismetSystem():GetConsoleVariableFloatValue("t.MaxFPS")) end)
    Log(string.format("[TICK] %.1f fps (кадр %.1f мс, мин %.1f / макс %.1f fps) t.MaxFPS=%s", #TickSamples / Sum,
        Sum / #TickSamples * 1000, 1 / Max, 1 / Min, MaxFps))
    TickSamples = {}
    LogPerf()
end

-- dbnative.dll (lives in the DungeonborneRuClient mod): op=perf — Windows does not throttle the server process
local NativeFn, NativeDir = nil, nil
local function NativeCall(Op, Args)
    if not NativeFn then
        for _, Dir in ipairs({ "ue4ss\\Mods\\DungeonborneRuClient\\native\\", "Mods\\DungeonborneRuClient\\native\\" }) do
            local Fn = package.loadlib(Dir .. "dbnative.dll", "dbn_call")
            if Fn then NativeFn, NativeDir = Fn, Dir; break end
        end
    end
    if not NativeFn then return nil, "dbnative.dll not loaded" end
    local F = io.open(NativeDir .. "req" .. Inst .. ".txt", "w")
    if not F then return nil, "cannot write req.txt" end
    F:write("op=" .. Op .. "\n")
    for K, V in pairs(Args) do F:write(K .. "=" .. tostring(V) .. "\n") end
    F:close()
    os.remove(NativeDir .. "rsp" .. Inst .. ".txt")
    NativeFn()
    local R = io.open(NativeDir .. "rsp" .. Inst .. ".txt", "r")
    local Rsp = R and R:read("*a") or ""
    if R then R:close() end
    return Rsp
end

local function NativePerf(Args)
    local Rsp, Err = NativeCall("perf", Args)
    return Rsp and (Rsp:gsub("\n", " ")) or Err
end

-- 03.11 (measuring several servers): dbserver-affinity.txt — lines "<port>=<mask>" (0x3 = logical CPUs 0 and 1).
-- The mask is set right at start (from Init), so that loading this instance does not disturb the others either.
ApplyAffinity = function()
    local Mask = nil
    for Line in (ReadFile("dbserver-affinity.txt") or ""):gmatch("[^\r\n]+") do
        local P, M = Line:match("^%s*(%d+)%s*=%s*(%S+)")
        if P and tonumber(P) == Port then Mask = M end
    end
    if Mask then Log("[PERF] affinity " .. Mask .. ": " .. NativePerf({ affinity = Mask })) end
end

-- A [PERF] line for every [TICK]: CPU share (100% = one logical processor fully busy), memory, threads
local LastCpuMs, LastNowMs = nil, nil
LogPerf = function()
    local Rsp = NativeCall("stats", {})
    if not Rsp then return end
    local V = {}
    for K, X in Rsp:gmatch("([%w_]+)=(%S+)") do V[K] = tonumber(X) end
    if not V.cpu_ms then return end
    local Cpu = "?"
    if LastCpuMs and V.now_ms > LastNowMs then
        Cpu = string.format("%.0f", (V.cpu_ms - LastCpuMs) / (V.now_ms - LastNowMs) * 100)
    end
    LastCpuMs, LastNowMs = V.cpu_ms, V.now_ms
    Log(string.format("[PERF] cpu=%s%% ws=%dMB private=%dMB peak_ws=%dMB threads=%d handles=%d", Cpu, V.ws_mb or 0,
        V.private_mb or 0, V.peak_ws_mb or 0, V.threads or 0, V.handles or 0))
end

-- dbserver-tick.txt: "<rate> [priority]" — server frame cap (t.MaxFPS) and process priority above normal.
-- Without the file — 60; 0 — touch nothing (measure as is).
local TickApplied = false
local function ApplyTickRate()
    if TickApplied then return end
    TickApplied = true
    local Cfg = ReadFile("dbserver-tick.txt") or ""
    local Fps = tonumber(Cfg:match("^%s*(%d+)") or "60") or 60
    if Fps <= 0 then Log("[TICK] dbserver-tick.txt = 0: tick rate left alone"); return end
    local Prio = Cfg:find("priority", 1, true) ~= nil
    Log("[TICK] perf: " .. NativePerf({ priority = Prio and "above" or "" }))
    pcall(function()
        local PC = FindLive("PlayerController")
        KismetSystem():ExecuteConsoleCommand(PC, "t.MaxFPS " .. Fps, PC)
    end)
    Log(string.format("[TICK] t.MaxFPS %d, priority above normal: %s", Fps, tostring(Prio)))
end

-- 03.11 "safe set" for a headless host: dbserver-lite.txt — space-separated words:
--   widgets  — remove all host widgets (UWidgetLayoutLibrary.RemoveAllWidgets): an observer needs no HUD;
--   textures — texture streaming pool 1 MB, load only small mips (r.Streaming.PoolSize, r.MipMapLODBias);
--   niagara  — Niagara particles at minimum quality (fx.Niagara.QualityLevel 0).
-- Gameplay (bones, animations, tick rate) is not touched. Each CVar is read back — the log shows whether the build has it.
local LITE_CVARS = {
    textures = { { "r.Streaming.PoolSize", 1 }, { "r.MipMapLODBias", 15 } },
    niagara = { { "fx.Niagara.QualityLevel", 0 } },
}
local LiteApplied, LiteWidgetsAt = false, nil
local function ApplyLite()
    if LiteApplied then return end
    LiteApplied = true
    local Cfg = " " .. (ReadFile("dbserver-lite.txt") or ""):lower() .. " "
    local PC = FindLive("PlayerController")
    local Ksl = KismetSystem()
    for Word, List in pairs(LITE_CVARS) do
        if Cfg:find("%s" .. Word .. "%s") then
            for _, CV in ipairs(List) do
                local Got = "?"
                pcall(function()
                    Ksl:ExecuteConsoleCommand(PC, CV[1] .. " " .. CV[2], PC)
                    Got = tostring(Ksl:GetConsoleVariableIntValue(CV[1]))
                end)
                Log(string.format("[LITE] %s %s -> %s%s", CV[1], CV[2], Got, Got == tostring(CV[2]) and "" or " (no such CVar?)"))
            end
        end
    end
    -- widgets: now and once more after 30 s (part of the UI is created after entering the world)
    if Cfg:find("%swidgets%s") then LiteWidgetsAt = os.time() end
    if Cfg:match("^%s*$") then Log("[LITE] dbserver-lite.txt empty or missing — set disabled") end
end

local function LiteWidgets()
    if not LiteWidgetsAt then return end
    local Age = os.time() - LiteWidgetsAt
    if Age ~= 0 and Age < 30 then return end
    local Ok, Err = pcall(function()
        local Wll = StaticFindObject("/Script/UMG.Default__WidgetLayoutLibrary")
        Wll:RemoveAllWidgets(FindLive("PlayerController"))
    end)
    Log("[LITE] RemoveAllWidgets (" .. (Age == 0 and "at once" or "after 30 s") .. "): " .. (Ok and "ok" or tostring(Err)))
    if Age ~= 0 then LiteWidgetsAt = nil end
end

-- 01.11: which loot tables the game loaded (pak zzzzzz_Balance_P). Slots of the goblin crossbowman (source 129) in the
-- Crypt: in the no_gear build (no equipment and gold) 919021 -> pool 990008, 919022 -> 990009; in the old
-- pak (full PVE loot) — 990049 / 990051. One [LOOTCHECK] line in the log per match.
local LootChecked = false
local function LootCheck()
    if LootChecked then return end
    LootChecked = true
    local Out = {}
    local Ok, Err = pcall(function()
        -- live table API (like LocalDev setup_native_match): the Default__ExcelLoaderConfigGroupWrapper template has no rows
        local Mgr = FindLive("TGFTableManager")
        local W = Mgr:GetCurrentGroupAPI({})
        local MapOk = false
        pcall(function() local R = W:GetRowExcelMapDetail_Of_MapId(10101, {}); MapOk = R ~= nil and R:IsValid() end)
        Out[#Out + 1] = "api=" .. tostring(W and W:IsValid()) .. " map10101=" .. tostring(MapOk)
        for _, Slot in ipairs({ 919020, 919021, 919022 }) do
            local Ids, Note = {}, ""
            local OkRow, ErrRow = pcall(function()
                local Row = W:GetRowExcelLootChooseGroupRate_Of_LootId(Slot, {})
                if not (Row and Row:IsValid()) then Note = " no row"; return end
                -- as in the F8 probe (GetAllOfLootIds): an array or a table; fallback — by index
                local OkAll = pcall(function()
                    local Arr = Row:GetAllOfRandomIds()
                    local function Val(V)
                        local Ok, X = pcall(function() return V:get() end)
                        return tostring(Ok and X or V)
                    end
                    if type(Arr) == "table" then for _, V in ipairs(Arr) do Ids[#Ids + 1] = Val(V) end
                    else Arr:ForEach(function(_, V) Ids[#Ids + 1] = tostring(V:get()) end) end
                end)
                if not OkAll or #Ids == 0 then
                    local N = Row:GetRandomIdsSize()
                    for I = 0, (tonumber(N) or 0) - 1 do Ids[#Ids + 1] = tostring(Row:GetRandomIds(I, {})) end
                end
            end)
            if not OkRow then Note = " error " .. tostring(ErrRow):sub(1, 120) end
            Out[#Out + 1] = string.format("%d->[%s]%s", Slot, table.concat(Ids, ","), Note)
        end
    end)
    local Line = table.concat(Out, " ")
    local Verdict = Line:find("990008", 1, true) and "new tables (no_gear)"
        or Line:find("990049", 1, true) and "OLD tables (full PVE loot)" or "not recognised"
    Log("[LOOTCHECK] " .. (Ok and Line or ("error " .. tostring(Err))) .. " => " .. Verdict)
end

-- 02.11 player result probe (docs/ghidra_gameover): the server's native PlayerGameDead/Escape/Quit find the player's record
-- by OpenId in UBattleDsUtility (map +0x80), dump the inventory from the pawn into that record (DumpUserDataToCache, status 3)
-- and send player_game_over only on a dedicated server with the DS SDK. The probe only reads: once a second the record statuses (dbnative
-- op=gostate), at status 3 — the serialized record into gameover-*.bin next to dbnative.dll; PlayerState state
-- (OpenId, death, portal exit) and BattleDsUtility UFunction calls. [GAMEOVER] lines in the role log.
-- dbserver-gameover-probe.txt = 0 — disable.
-- Result to backend (docs/DS_PIPELINE.md "Native raid result"): a record with the real OpenId (realkey in native_data.txt)
-- at status 3 -> dbnative op=goreport -> POST /ds/player_result. Once per record; network/5xx — retry once a second
-- up to 10 attempts; 4xx — the backend answer is final (409 — already applied). [RAIDRESULT] lines.
local GoProbe = { On = nil, Entries = {}, Players = {}, Hooked = false, Calls = {}, Pending = {}, ResultUrl = nil, Finished = {} }

-- One raid — one entry (02.11): whoever already has a result taken in this match (GoProbe.Finished, lives until the server process ends =
-- one raid) is kicked on re-entry (open ip), and a second result (leave/self-quit after a portal exit)
-- is not sent to backend — otherwise backend would apply it as a death and take the items.
-- Native counterpart (docs/ghidra_gameover): ALyraGameMode::PreLogin 0x142fa17f0 -> validator 0x142ea8f10 refuses with
-- "Player Game Status Invalid" if the player's record has flag +0x238 (set by PlayerQuit 0x142ea84a0), re-entry
-- is allowed by UBattleDsUtility+0x1fc. All of this — only on a dedicated server with the DS SDK (0x142e9e8b0), off on listen.
-- dbserver-rejoin.txt = 1 — allow re-entry (like +0x1fc), then a new result after it is sent as well.
-- The same OpenId while the first one is still in the match is always kicked (one Goldberg ID in two clients).
local function RejoinAllowed()
    if GoProbe.Rejoin == nil then
        GoProbe.Rejoin = (ReadFile("dbserver-rejoin.txt") or ""):match("^%s*1") ~= nil
        Log("[JOINGUARD] raid re-entry after a result: " .. (GoProbe.Rejoin and "allowed" or "forbidden")
            .. " (dbserver-rejoin.txt)")
    end
    return GoProbe.Rejoin
end

-- OpenId of a remote player — from the address he connected with (UNetConnection::RequestURL via dbnative
-- op=connurl), like the native PreLogin; MyOpenId during InitPlayerGameData is sometimes LOCAL_GAME_MODE_OPENID.
-- No address (dbnative did not find it) — MyOpenId. Returns OpenId ("" — none), its source, the address
local function RemoteOpenId(PC, PS)
    local Conn = nil
    pcall(function() Conn = PC.Player end)
    if IsValid(Conn) then
        local Rsp = NativeCall("connurl", { conn = string.format("0x%X", Conn:GetAddress()) }) or ""
        if Rsp:find("ok=1", 1, true) then
            return Rsp:match("open_id=([^\n]*)") or "", "address", Rsp:match("url=([^\n]*)") or ""
        end
    end
    local Open = ""
    pcall(function() Open = PS.MyOpenId:ToString() end)
    if Open == "LOCAL_GAME_MODE_OPENID" then Open = "" end
    return Open, "MyOpenId", ""
end

-- Once a second (Ready), independent of the probe, without remembering "new" objects (02.11: the game gives a returning player both
-- old PlayerStates and the old controller — checks based on them let him through). For each remote controller with a
-- live connection (PC.Player) — OpenId: from the connection address (RequestURL), otherwise MyOpenId. Re-entry:
--   * without OpenId (console open ip) — the native PreLogin would not let him in ("Invalid Player"), and he would get
--     someone else's data (the shared LOCAL_GAME_MODE_OPENID record);
--   * the result of this OpenId is already taken, and the connection was not among this OpenId's live ones at result time (GoProbe.FinishedConns;
--     02.11 12:13: a player who exited took longer than 10 s to leave, and the old rule "result > 10 s ago" sent him himself to the lobby),
--     unless dbserver-rejoin.txt allows it; a connection from that list alive longer than STALE_AFTER_SEC after a portal
--     exit — "did not leave after the result" (a dead player is left alone: he may be watching his squad); no list (result seen
--     without connections) — the old rule REJOIN_AFTER_SEC;
--   * a second live connection of the same OpenId (the one that appeared earlier stays).
-- Action — send the client to the lobby (ClientTravel, as the client itself leaves after a portal exit); if after
-- CLOSE_AFTER_SEC the connection is alive — the server closes it (ClientWasKicked + controller removal, like
-- AGameSession::KickPlayer). The host's own controller (IsLocalController) is not checked.
local NO_OPENID_GRACE = 3   -- checks in a row without OpenId if the address was not read (MyOpenId may not have arrived yet)
local REJOIN_AFTER_SEC = 10
local STALE_AFTER_SEC = 60
local CLOSE_AFTER_SEC = 5
local LOBBY_URL = "/TgfMaps/SceneAssets/RuinedCrypt/Demo/Maps/RuinedCrypt_03"

local function Short(Obj)
    return Obj and (FullName(Obj):match("([^%.%s]+)$") or "?") or "none"
end

local function SendToLobby(PC, Text)
    pcall(function() PC:ClientMessage(Text, FName("None"), 10.0) end)
    local Ok = pcall(function() PC:ClientTravel(LOBBY_URL, 0, false, { A = 0, B = 0, C = 0, D = 0 }) end)
    if not Ok then Ok = pcall(function() PC:ClientTravel(LOBBY_URL, 0, false) end) end
    return Ok
end

-- Take a player out of the match like the exit button: the native server RPC ALyraPlayerController::ServerSelfQuitGame (on the server
-- its implementation runs -> PlayerGameDead/QuitPlayerState -> DumpUserDataToCache -> record status 3: a dead player —
-- reason 2, alive — 1 SELF_QUIT), after LEAVE_TRAVEL_SEC — ClientTravel to the lobby. A ClientTravel alone did not start the native
-- exit handling (02.11 12:32: the dead player left, the record stayed at status 1).
local LEAVE_TRAVEL_SEC = 3
local function LeavePlayers(Arg, Text)
    local N, Quit = 0, 0
    GoProbe.TravelAt = GoProbe.TravelAt or {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, PC = "", nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() PC = PS:GetOwner() end)
            local IsLocal = false
            pcall(function() IsLocal = PC:IsLocalController() end)
            if (Arg == "all" or Arg == Open) and IsValid(PC) and not IsLocal and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                if pcall(function() PC:ServerSelfQuitGame() end) then Quit = Quit + 1 end
                GoProbe.TravelAt[Open] = { At = os.time() + LEAVE_TRAVEL_SEC, Text = Text }
                N = N + 1
            end
        end
    end
    return N, Quit
end

-- kill: after KILL_LEAVE_SEC the dead player "presses exit" (otherwise, while spectating, there is no death result); then — ClientTravel
local KILL_LEAVE_SEC = 5
local function KillLeaves()
    local Now = os.time()
    for Open, At in pairs(GoProbe.LeaveAt or {}) do
        if Now >= At then
            GoProbe.LeaveAt[Open] = nil
            local N, Quit = LeavePlayers(Open, "Вы погибли")
            Log(string.format("[HOSTCMD] kill %s: after %d s — exit (ServerSelfQuitGame ok=%d/%d)", Open, KILL_LEAVE_SEC, Quit, N))
        end
    end
    for Open, T in pairs(GoProbe.TravelAt or {}) do
        if Now >= T.At then
            GoProbe.TravelAt[Open] = nil
            local Moved = 0
            for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
                local O, PC = "", nil
                pcall(function() O = PS.MyOpenId:ToString() end)
                pcall(function() PC = PS:GetOwner() end)
                local Conn = nil
                pcall(function() Conn = PC.Player end)
                if O == Open and IsValid(PC) and IsValid(Conn) and SendToLobby(PC, T.Text) then Moved = Moved + 1 end
            end
            Log(string.format("[HOSTCMD] %s: to lobby (ClientTravel) %d, record %s", Open, Moved,
                (GoProbe.Entries or {})[Open] or "?"))
        end
    end
end

local function JoinGuard()
    pcall(KillLeaves)
    GoProbe.ConnOpen = GoProbe.ConnOpen or {} -- connection key -> { Open, From, Url } (the address does not change)
    GoProbe.FirstSeen = GoProbe.FirstSeen or {}
    GoProbe.NoOpen = GoProbe.NoOpen or {}
    GoProbe.Action = GoProbe.Action or {}     -- connection key -> { At, Closed }
    GoProbe.AliveNote = GoProbe.AliveNote or {}
    local Now = os.time()
    local All = {}
    for _, PC in ipairs(FindAllOf("BP_TgfPlayerController_C") or {}) do
        if IsValid(PC) and not FullName(PC):find("Default__", 1, true) then
            local IsLocal = false
            pcall(function() IsLocal = PC:IsLocalController() end)
            local Conn = nil
            pcall(function() Conn = PC.Player end)
            -- 03.11: after DestroyCharacter of the host (late observer) it has a new controller, IsLocalController = false,
            -- and its "connection" is a ULocalPlayer: not a remote player
            if not IsLocal and IsValid(Conn) and FullName(Conn):find("LocalPlayer", 1, true) then IsLocal = true end
            if not IsLocal and IsValid(Conn) then
                local Key = string.format("%X|%s", Conn:GetAddress(), Short(Conn))
                local PS = nil
                pcall(function() PS = PC.PlayerState end)
                if not IsValid(PS) then PS = nil end
                local C = GoProbe.ConnOpen[Key]
                if not C then
                    local Open, From, Url = RemoteOpenId(PC, PS)
                    C = { Open = Open, From = From, Url = Url }
                    if From == "address" then GoProbe.ConnOpen[Key] = C end
                end
                if not GoProbe.FirstSeen[Key] then
                    GoProbe.FirstSeen[Key] = Now
                    Log(string.format("[JOINGUARD] соединение %s: OpenId %s (из: %s), PC %s, PS %s", Short(Conn),
                        C.Open ~= "" and C.Open or "none", C.From, Short(PC), Short(PS)))
                end
                All[#All + 1] = { PC = PC, PS = PS, Conn = Conn, Key = Key, Open = C.Open, From = C.From, Url = C.Url }
            end
        end
    end
    -- live connections of each OpenId in this tick: on a result (GameOverProbe) they are copied into GoProbe.FinishedConns
    GoProbe.LastConns = {}
    for _, P in ipairs(All) do
        if P.Open ~= "" then
            GoProbe.LastConns[P.Open] = GoProbe.LastConns[P.Open] or {}
            GoProbe.LastConns[P.Open][P.Key] = true
        end
    end
    -- which of the live connections with the same OpenId appeared first
    local First = {}
    for _, P in ipairs(All) do
        if P.Open ~= "" and not GoProbe.Action[P.Key] then
            local F = First[P.Open]
            if not F or GoProbe.FirstSeen[P.Key] < GoProbe.FirstSeen[F] then First[P.Open] = P.Key end
        end
    end
    for _, P in ipairs(All) do
        local A = GoProbe.Action[P.Key]
        if A then
            if not A.Closed and Now - A.At >= CLOSE_AFTER_SEC then
                A.Closed = true
                local OkKick = pcall(function() P.PC:ClientWasKicked(FText(A.Text)) end)
                local OkDestroy = pcall(function() P.PC:K2_DestroyActor() end)
                Log(string.format("[JOINGUARD] %s: connection %s alive after %d s — closed by the server (kicked=%s destroy=%s)",
                    A.Open, Short(P.Conn), Now - A.At, tostring(OkKick), tostring(OkDestroy)))
            end
        else
            local Why, Text = nil, "Этот рейд для вас уже завершён"
            if P.Open == "" then
                GoProbe.NoOpen[P.Key] = (GoProbe.NoOpen[P.Key] or 0) + 1
                if P.From == "address" or GoProbe.NoOpen[P.Key] >= NO_OPENID_GRACE then
                    Why, Text = "no OpenId in the connection address" .. (P.Url ~= "" and (" (" .. P.Url .. ")") or ""),
                        "Вход в рейд только через лобби"
                end
            elseif GoProbe.HallLeft and GoProbe.HallLeft[P.Open] then
                Why = string.format("left the waiting hall %d s ago — raid closed by backend", Now - GoProbe.HallLeft[P.Open])
                Text = "Вы покинули зал ожидания — рейд идёт без вас"
            elseif GoProbe.Finished[P.Open] and not RejoinAllowed() then
                local Age = Now - GoProbe.Finished[P.Open]
                local Old = GoProbe.FinishedConns and GoProbe.FinishedConns[P.Open]
                if Old and not Old[P.Key] then
                    Why = string.format("re-entry (result %d s ago, new connection)", Age)
                elseif Old and Age > STALE_AFTER_SEC and (GoProbe.FinishedReason or {})[P.Open] == "3" then
                    -- only after a portal exit (reason 3): a dead player may stay to watch his squad
                    Why = string.format("did not leave after the result (%d s)", Age)
                elseif not Old and Age > REJOIN_AFTER_SEC then
                    Why = string.format("re-entry (result %d s ago)", Age)
                elseif not GoProbe.AliveNote[P.Key] then
                    GoProbe.AliveNote[P.Key] = true
                    Log(string.format("[JOINGUARD] %s: connection %s still alive %d s after the result", P.Open, Short(P.Conn), Age))
                end
            elseif First[P.Open] and First[P.Open] ~= P.Key then
                Why, Text = "already in the match on another connection", "Вы уже в этом рейде"
            end
            if Why then
                local Ok = SendToLobby(P.PC, Text)
                GoProbe.Action[P.Key] = { At = Now, Closed = false, Text = Text, Open = P.Open ~= "" and P.Open or "?" }
                Log(string.format("[JOINGUARD] %s: %s (соединение %s, PC %s, PS %s) — перевод в лобби ok=%s",
                    P.Open ~= "" and P.Open or "?", Why, Short(P.Conn), Short(P.PC), Short(P.PS), tostring(Ok)))
            end
        end
    end
end

-- Backend address (http://host:port) — the first line of the client mod's native_data.txt; nil — no file yet
local BackendBaseUrl = nil
local function BackendBase()
    if BackendBaseUrl then return BackendBaseUrl end
    for _, P in ipairs({ "ue4ss\\Mods\\DungeonborneRuClient\\native_data.txt", "Mods\\DungeonborneRuClient\\native_data.txt" }) do
        local F = io.open(P, "r")
        if F then
            local Base = (F:read("*l") or ""):match("^%s*(https?://[^/%s]+)")
            F:close()
            if Base then BackendBaseUrl = Base; break end
        end
    end
    return BackendBaseUrl
end

local function ResultUrl()
    if GoProbe.ResultUrl == nil then
        local Base = BackendBase()
        GoProbe.ResultUrl = Base and (Base .. "/ds/player_result") or false
    end
    return GoProbe.ResultUrl
end

-- 04.11 (B12, research/hallleave): leaving the waiting hall. The native PlayerGameFinish with scene WAITING_ROOM
-- (UBattleDsUtility+0x1f4 = 1, set by InitGameMap) removes the player from the record and token maps, sends
-- PlayerWaitingRoomLogoutDelegate (hall counter), does not take the inventory from the pawn and sends player_game_over with
-- batle_info.finish_scenes = 1. We (listen, without InitGameMap) always have scene 0, so the mod decides itself that this is the hall:
-- a room task in phase waiting/entering and the transition to the match not started yet (native boundary — scene change on transition).
-- Such a result goes with ?scene=waiting_room (backend: hall_leave — items, exp and quests are untouched). The player does not
-- count as having finished the raid and is removed from the roster (will not end up in absent). 13.11: we do not let him back into this match
-- (GoProbe.HallLeft, JoinGuard — to the lobby): a disconnect at the end of the hall timer gave a "left the hall" result (backend closed
-- the raid, items at home), and the reconnected player got a pawn and went into the raid with the same items — a dupe.
local function InHall()
    return Host ~= nil and (Host.Phase == "waiting" or Host.Phase == "entering") and not TravelPending
end

local function HallLeave(Key)
    -- 13.11 (dupe): on this result backend closes the raid without losses — the player does not enter this match again (JoinGuard: to the lobby)
    GoProbe.HallLeft = GoProbe.HallLeft or {}
    GoProbe.HallLeft[Key] = os.time()
    if Host and Host.Members[Key] then
        Host.Members[Key] = nil
        Host.Total = math.max(0, Host.Total - 1)
    end
    Log(string.format("[RAIDRESULT] %s: left the waiting hall (%s) — result with scene=waiting_room, removed from the roster, total %d",
        Key, Host and Host.Phase or "?", Host and Host.Total or 0))
end

-- 11.11 arena (DM, "Clotho"): an opponent left — the match did not end. Native path: LyraTeamSubsystem.OnTeamAllSelfQuit ->
-- BP_Tgf_PVPPhaseHint.TeamAllQuit(TeamId, TeamColor) -> QuitTeam, end of phase -> GenerateWinner (the other team wins)
-- -> ServerQuit (PVPGameWinTeam, arena result) -> "Victory" window. Here the delegate does not come (the exit goes through the mod's result),
-- so after the result of the leaver (status 3) the mod calls TeamAllQuit itself if nobody from his team is left in the arena
-- and the other team exists. Once per match, not after the match end. A global table: the main chunk has a 200-local limit.
ArenaQuit = { Done = {} }

function ArenaQuit.Check(Key)
    local Battle = Host and Host.Battle or ""
    if Battle == "" or not (Host and #Host.Scenes == 1) or ArenaQuit.Done[Battle] or (GoProbe.ArenaDone or {})[Battle] then return end
    local LeftTeam, LeftColor, Others, Same = nil, nil, {}, 0
    local Gone = GoProbe.ResultSent or {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Team, Color = "", nil, nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Team = tonumber(PS:GetTeamId()) end)
            pcall(function() Color = tonumber(PS:GetTeamColor()) end)
            if Open:match("^%d+$") and Team then
                if Open == Key then
                    LeftTeam, LeftColor = Team, Color
                elseif not Gone[Open] then
                    Others[#Others + 1] = Team
                end
            end
        end
    end
    if not LeftTeam then
        Log(string.format("[ARENA] %s: %s left — his PlayerState not found, not checking the match end", Battle, Key))
        return
    end
    local Enemy = 0
    for _, T in ipairs(Others) do
        if T == LeftTeam then Same = Same + 1 else Enemy = Enemy + 1 end
    end
    if Same > 0 or Enemy == 0 then
        Log(string.format("[ARENA] %s: %s left (team %d) — %d left in the team, %d opponents, match goes on", Battle, Key,
            LeftTeam, Same, Enemy))
        return
    end
    local Hint = nil
    for _, H in ipairs(FindAllOf("BP_Tgf_PVPPhaseHint_C") or {}) do
        if IsValid(H) and not FullName(H):find("Default__", 1, true) then Hint = H end
    end
    if not Hint then
        Log(string.format("[ARENA] %s: the whole team %d left, but there is no BP_Tgf_PVPPhaseHint", Battle, LeftTeam))
        return
    end
    ArenaQuit.Done[Battle] = true
    local Ok, Err = pcall(function() Hint:TeamAllQuit(LeftTeam, LeftColor or LeftTeam) end)
    Log(string.format("[ARENA] %s: the whole team %d left (color %s), %d opponents -> TeamAllQuit: %s", Battle, LeftTeam,
        tostring(LeftColor), Enemy, Ok and "ok" or tostring(Err)))
end

local function SendResults(Mgr)
    for Key, P in pairs(GoProbe.Pending) do
        local Url = ResultUrl()
        if Url and P.Hall then Url = Url .. "?scene=waiting_room" end
        -- ascension potion: exp for the match (native counterpart — DDsUserBattleInfo.add_exp in the dedicated server result)
        local Bonus = (GoProbe.BonusExp or {})[Key] or 0
        if Url and Bonus > 0 then Url = Url .. (Url:find("?", 1, true) and "&" or "?") .. "bonus_exp=" .. Bonus end
        if not Url then
            Log("[RAIDRESULT] " .. Key .. ": no backend URL (native_data.txt)")
            GoProbe.Pending[Key] = nil
        else
            P.Tries = P.Tries + 1
            local Rsp = NativeCall("goreport", { mgr = string.format("0x%X", Mgr:GetAddress()), key = Key, url = Url }) or ""
            local Http = tonumber(Rsp:match("http=(%d+)") or "")
            local Line = Rsp:gsub("\n", " ")
            if Http and Http < 500 then
                Log(string.format("[RAIDRESULT] %s: POST -> %d (attempt %d) %s", Key, Http, P.Tries, Line))
                GoProbe.Pending[Key] = nil
                -- 04.11: for /ds/player_left (GameOverProbe) — the result is delivered; the connection is already gone — no signal needed
                GoProbe.ResultSent = GoProbe.ResultSent or {}
                GoProbe.Left = GoProbe.Left or {}
                GoProbe.ResultSent[Key] = os.time()
                if not (GoProbe.LastConns or {})[Key] then GoProbe.Left[Key] = { Done = true, Why = "left before the result" } end
                -- 03.11: the room's last result (and 409 on a repeat while the slot waits for a reset) -> reset the server
                local Closed = Rsp:match('"room_closed"%s*:%s*"([^"]+)"')
                if Closed then BeginReset(Closed, "room_closed in the result response " .. Key) end
                -- 11.11: left the arena — if his team is empty, the native match end (ArenaQuit)
                if not Closed and not P.Hall then
                    local OkQ, ErrQ = pcall(ArenaQuit.Check, Key)
                    if not OkQ then Log("[ARENA] exit check error: " .. tostring(ErrQ)) end
                end
            elseif P.Tries >= 10 then
                Log(string.format("[RAIDRESULT] %s: not sent after %d attempts: %s", Key, P.Tries, Line))
                GoProbe.Pending[Key] = nil
            elseif P.Tries == 1 then
                Log(string.format("[RAIDRESULT] %s: failed, retrying: %s", Key, Line))
            end
        end
    end
end

local function GoHooks()
    if GoProbe.Hooked then return end
    GoProbe.Hooked = true
    -- 11.11 score table after a PVP match (user's decision, docs/PVP_SETTLE.md). At the end of an arena match the BP
    -- BP_Tgf_PVPPhaseHint.ServerQuit calls BattleDsUtility.PVPGameWinTeam(winner faction_id); the native result further on is closed
    -- by the DS SDK check. Here: each player's stats (the controller's TgfActorStatisticComponent — like
    -- MakeStatisticDataFromPlayer in the BP: IntStat(1,1) player kills, IntStat(51,0) deaths, FloatStat 3 damage, 4 healing,
    -- 5 damage taken) -> POST /ds/arena_result; backend sends players battle_settle_sync -> WBP_TGF_PVP_Settle window in the lobby.
    -- Only a "pr…" match with one scene (custom game), once per match. Switch: dbserver-arenaresult.txt = 0.
    local function ArenaOff()
        local F = io.open("dbserver-arenaresult.txt", "r")
        local V = F and (F:read("*l") or ""):gsub("%s", "") or ""
        if F then F:close() end
        return V == "0" or V:lower() == "off"
    end

    local function ArenaSend(Winner)
        if ArenaOff() then return end
        local Battle = Host and Host.Battle or ""
        -- 11.11: arena — a match with one scene (custom game "pr…" and "Trial of Clotho" from the lobby), not by prefix
        if Battle == "" or not (Host and #Host.Scenes == 1) or (GoProbe.ArenaDone or {})[Battle] then return end
        GoProbe.ArenaDone = GoProbe.ArenaDone or {}
        GoProbe.ArenaDone[Battle] = true
        local Rows, Seen = {}, {}
        for _, C in ipairs(FindAllOf("TgfActorStatisticComponent") or {}) do
            if IsValid(C) and not FullName(C):find("Default__", 1, true) then
                local Ok, Err = pcall(function()
                    local PC = C:GetOwner()
                    if not IsValid(PC) then return end
                    local PS = PC.PlayerState
                    if not IsValid(PS) then return end
                    local Open = PS.MyOpenId:ToString()
                    if not Open:match("^%d+$") or Seen[Open] then return end
                    Seen[Open] = true
                    local Team = PS:GetTeamId()
                    local Kills, Deaths = C:GetIntStat(1, 1), C:GetIntStat(51, 0)
                    local Dmg, Heal, Taken = C:GetFloatStat(3), C:GetFloatStat(4), C:GetFloatStat(5)
                    Rows[#Rows + 1] = string.format('{"open_id":"%s","team":%d,"kills":%d,"deaths":%d,"damage":%d,"heal":%d,"taken":%d}',
                        Open, tonumber(Team) or 0, tonumber(Kills) or 0, tonumber(Deaths) or 0, math.floor(tonumber(Dmg) or 0),
                        math.floor(tonumber(Heal) or 0), math.floor(tonumber(Taken) or 0))
                end)
                if not Ok then Log("[ARENA] stats: " .. tostring(Err)) end
            end
        end
        -- BO5 rounds (PVPGameWinTeamRound): round number -> winning faction, for the BO5_TRIAL results window
        local Rounds = {}
        local R = GoProbe.ArenaRounds
        if R and R.Battle == Battle then
            for Round, Win in pairs(R.List) do
                Rounds[#Rounds + 1] = string.format('{"round":%d,"winner":%d}', Round, Win)
            end
        end
        local Body = string.format('{"battle_id":"%s","winner":%d,"players":[%s],"rounds":[%s]}', Battle, tonumber(Winner) or 0,
            table.concat(Rows, ","), table.concat(Rounds, ","))
        Log("[ARENA] match result: " .. Body)
        local Base = BackendBase()
        if not Base then return end
        local Rsp = NativeCall("http", { url = Base .. "/ds/arena_result", body = Body }) or ""
        Log("[ARENA] POST /ds/arena_result -> " .. (Rsp:match("http=(%d+)") or ("none: " .. (Rsp:match("err=([^\n]*)") or "?"))) ..
            " " .. (Rsp:match("body=([^\n]*)") or ""):sub(1, 120))
    end

    local function ArenaHook()
        local Ok, E = pcall(function()
            RegisterHook("/Script/LyraGame.BattleDsUtility:PVPGameWinTeam", function(Ctx, FactionId)
                local Winner = 0
                pcall(function() Winner = FactionId:get() end)
                local Ok2, E2 = pcall(ArenaSend, Winner)
                if not Ok2 then Log("[ARENA] error: " .. tostring(E2)) end
            end)
        end)
        Log("[ARENA] hook PVPGameWinTeam: " .. (Ok and "ok" or tostring(E)))
        -- end of each BO5 round: Server Notify Single Settle -> PVPGameWinTeamRound(number, winning faction)
        local OkR, ER = pcall(function()
            RegisterHook("/Script/LyraGame.BattleDsUtility:PVPGameWinTeamRound", function(Ctx, RoundId, FactionId)
                pcall(function()
                    local Battle = Host and Host.Battle or ""
                    if Battle == "" then return end
                    if not (GoProbe.ArenaRounds and GoProbe.ArenaRounds.Battle == Battle) then
                        GoProbe.ArenaRounds = { Battle = Battle, List = {} }
                    end
                    local Round, Win = tonumber(RoundId:get()) or 0, tonumber(FactionId:get()) or 0
                    GoProbe.ArenaRounds.List[Round] = Win
                    Log(string.format("[ARENA] %s: round %d — faction %d won", Battle, Round, Win))
                end)
            end)
        end)
        Log("[ARENA] hook PVPGameWinTeamRound: " .. (OkR and "ok" or tostring(ER)))
    end
    ArenaHook()
    local Names = { "PlayerDeadPlayerState", "IsPlayerGameOver", "GameOver", "OnMapFinish", "PlayerSetLogin" }
    local Done = {}
    for _, Name in ipairs(Names) do
        local Ok = pcall(function()
            RegisterHook("/Script/LyraGame.BattleDsUtility:" .. Name, function(Ctx, A1, A2)
                local N = (GoProbe.Calls[Name] or 0) + 1
                GoProbe.Calls[Name] = N
                if N > 20 then return end -- IsPlayerGameOver may be called often
                local Who = ""
                pcall(function() Who = " " .. A1:get():GetFullName():match("([^%.]+)$") end)
                local Extra = ""
                pcall(function() if A2 ~= nil then Extra = " arg2=" .. tostring(A2:get()) end end)
                Log(string.format("[GAMEOVER] вызов %s #%d%s%s", Name, N, Who, Extra))
                -- 05.11 custom game ("pr…" match, one-scene arena): at the end of a PVP-mode match the game does not set
                -- player results (records stay "in match"), so treat it as an empty hall: phase=empty -> backend broadcasts
                -- "match over" (5–6 FINISHED), closes the match room, the room returns to preparation, the slot gets reset
                -- 11.11: and "Trial of Clotho" from the launcher lobby — any one-scene match (arena), not only "pr…"
                if Name == "GameOver" and Host and (Host.Battle or "") ~= "" and #Host.Scenes == 1
                    and Host.Phase ~= "empty" then
                    Host.Phase = "empty" -- not battle -> slot pulse in fast mode (SLOT_HB_FAST_SEC), leaves by itself
                    Log(string.format("[HOST] %s: arena match end — telling backend (phase=empty)", Host.Battle))
                end
            end)
        end)
        Done[#Done + 1] = Name .. (Ok and "" or "(none)")
    end
    Log("[GAMEOVER] UFunction hooks: " .. table.concat(Done, ", "))
    -- ascension potion: GA_TGF_UseFlaskOfExp calls LyraPlayerState.PlayerAddExp; on listen the value does not reach the result.
    -- Accumulate by OpenId: PlayerState changes on floor transition, GoProbe lives for the whole match (ResetState — a new table)
    local OkExp = pcall(function()
        RegisterHook("/Script/LyraGame.LyraPlayerState:PlayerAddExp", function(Self, Amount)
            pcall(function()
                local Open, Add = Self:get().MyOpenId:ToString(), Amount:get()
                if not (Add and Add > 0) or Open == "" then return end
                GoProbe.BonusExp = GoProbe.BonusExp or {}
                GoProbe.BonusExp[Open] = (GoProbe.BonusExp[Open] or 0) + Add
                Log(string.format("[EXP] %s: potion +%d, total for the match %d", Open, Add, GoProbe.BonusExp[Open]))
            end)
        end)
    end)
    Log("[EXP] hook PlayerAddExp: " .. tostring(OkExp))
end

local function GoPlayers()
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Escaped, Dead = "?", "?", "?"
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Escaped = tostring(PS:GetIsEscapeQuit()) end)
            pcall(function()
                local Pawn = PS.PawnPrivate
                Dead = IsValid(Pawn) and tostring(Pawn:IsDeadOrDying()) or "no pawn"
            end)
            local Key = string.format("%X|%s", PS:GetAddress(), FullName(PS):match("([^%.%s]+)$") or "")
            local State = string.format("open_id=%s escaped=%s dead=%s", Open, Escaped, Dead)
            if GoProbe.Players[Key] ~= State then
                GoProbe.Players[Key] = State
                Log(string.format("[GAMEOVER] PS %s %s", FullName(PS):match("([^%.]+)$") or "?", State))
            end
        end
    end
end

-- /ds/player_left for players with a delivered result who are not among Connected (OpenId -> anything). Called by
-- GameOverProbe (match, connections from JoinGuard) and ResetStep (reset after the room). 05.11 (local): the last
-- player's result closes the room, the server goes straight to resetting, where GameOverProbe does not run — the signal was not sent,
-- the delayed result was delivered only after player_get_info, when the lobby was already open (no results window).
function GoProbe.LeftStep(Connected, Battle)
    for Key in pairs(GoProbe.ResultSent or {}) do
        GoProbe.Left = GoProbe.Left or {}
        local L = GoProbe.Left[Key] or { Tries = 0 }
        GoProbe.Left[Key] = L
        if not L.Done and not Connected[Key] then
            local Base = BackendBase()
            if not Base then
                L.Done, L.Why = true, "no backend URL"
            else
                L.Tries = L.Tries + 1
                local Url = string.format("%s/ds/player_left?open_id=%s&battle_id=%s", Base, Key, Battle or "")
                local Rsp = NativeCall("http", { url = Url, body = "{}" }) or ""
                local Http = tonumber(Rsp:match("http=(%d+)") or "")
                if (Http and Http < 500) or L.Tries >= 5 then L.Done = true end
                Log(string.format("[RAIDRESULT] %s: connection closed %d s after the result -> /ds/player_left: %s (attempt %d) %s",
                    Key, os.time() - GoProbe.ResultSent[Key], Http and tostring(Http) or ("none: " .. (Rsp:match("err=([^\n]*)") or "?")),
                    L.Tries, (Rsp:match("body=([^\n]*)") or ""):sub(1, 120)))
            end
        end
    end
end

local function GameOverProbe()
    -- dbserver-gameover-probe.txt = 0 disables only the probe (UFunction hooks, PS lines); results and JoinGuard keep working
    if GoProbe.On == nil then GoProbe.On = (ReadFile("dbserver-gameover-probe.txt") or ""):match("^%s*0") == nil end
    if GoProbe.On then
        GoHooks()
        pcall(GoPlayers)
    end
    local OkGuard, ErrGuard = pcall(JoinGuard)
    if not OkGuard then Log("[JOINGUARD] error: " .. tostring(ErrGuard)) end
    -- 04.11 (research/battleroom, contract with backend): the player's result is delivered (SendResults), and his connection is no longer
    -- among the live ones (JoinGuard of this tick) — the client left the map and is loading the lobby. Once per player per match (GoProbe
    -- lives until reset/transition) — GET/POST /ds/player_left?open_id=&battle_id=: backend sends the delayed result pushes
    -- (dirty_role, quests, mail, battle_settle_sync) before the lobby opens — the results screen is checked only when entering
    -- it, and pushes that reached the client while leaving the raid map hung its receive. Left before the result — not sent.
    -- Network/5xx — retry in the next ticks, up to 5 attempts.
    GoProbe.LeftStep(GoProbe.LastConns or {}, Host and Host.Battle or "")
    local Mgr = FindLive("BattleDsUtility")
    if not Mgr then return end
    local Rsp, Err = NativeCall("gostate", { mgr = string.format("0x%X", Mgr:GetAddress()) })
    if not Rsp then
        if not GoProbe.Err then GoProbe.Err = true; Log("[GAMEOVER] " .. tostring(Err)) end
        return
    end
    if not Rsp:find("ok=1", 1, true) then
        if GoProbe.LastRsp ~= Rsp then GoProbe.LastRsp = Rsp; Log("[GAMEOVER] gostate: " .. Rsp:gsub("\n", " ")) end
        return
    end
    for Line in Rsp:gmatch("e%d+=([^\n]+)") do
        local Key, St, Reason, Type, Ptr = Line:match("^(.-)|(-?%d+)|(-?%d+)|(.-)|(.-)|")
        if Key then
            GoProbe.Status = GoProbe.Status or {}
            GoProbe.Status[Key] = St
            local Was = GoProbe.Entries[Key]
            local Now = St .. "/" .. Reason .. "/" .. Ptr
            if Was ~= Now then
                GoProbe.Entries[Key] = Now
                Log(string.format("[GAMEOVER] запись %s: статус %s причина %s тип %s объект %s (было %s)", Key, St, Reason,
                    Type, Ptr, Was or "-"))
                if St == "3" then
                    local D = NativeCall("gostate", { mgr = string.format("0x%X", Mgr:GetAddress()), dump = Key }) or ""
                    local Bytes = D:match("e%d+=" .. Key:gsub("%p", "%%%0") .. "|[^\n]*|(%d+)\n") or "?"
                    Log(string.format("[GAMEOVER] запись %s снята: %s байт -> %sgameover-*.bin", Key, Bytes, NativeDir or "?"))
                    if Key ~= "LOCAL_GAME_MODE_OPENID" and GoProbe.Finished[Key] and not RejoinAllowed() then
                        Log(string.format("[RAIDRESULT] %s: second result in the same raid (reason %s) — not sending", Key, Reason))
                    elseif Key ~= "LOCAL_GAME_MODE_OPENID" and not GoProbe.Pending[Key] and InHall() then
                        -- B12: leaving the hall is not a raid result (Finished is not set, the kill is not reported)
                        HallLeave(Key)
                        GoProbe.Pending[Key] = { Tries = 0, Hall = true }
                    elseif Key ~= "LOCAL_GAME_MODE_OPENID" and not GoProbe.Pending[Key] then
                        GoProbe.Finished[Key] = os.time()
                        -- connections of this OpenId alive at result time (JoinGuard of this tick): do not count them as re-entry
                        GoProbe.FinishedConns = GoProbe.FinishedConns or {}
                        local Conns = (GoProbe.LastConns or {})[Key]
                        GoProbe.FinishedConns[Key] = Conns and next(Conns) and Conns or nil
                        GoProbe.FinishedReason = GoProbe.FinishedReason or {}
                        GoProbe.FinishedReason[Key] = Reason
                        if Reason == "2" then
                            local OkK, ErrK = pcall(ReportKill, Key)
                            if not OkK then Log("[KILL] error: " .. tostring(ErrK)) end
                        end
                        GoProbe.Pending[Key] = { Tries = 0 }
                    end
                end
            end
        end
    end
    SendResults(Mgr)
end

-- --- Server slot in backend and reset between matches (03.11, contract with backend: docs/DS_PIPELINE.md, M3) ----------
-- Room closed (all results delivered, ds_end or timeout) -> the slot in backend is "resetting", no new rooms go to it.
-- The mod learns this from room_closed in the /ds/player_result response or from action=reset in the heartbeat response
-- POST /ds/slot (every SLOT_HB_SEC from the start phase; without a heartbeat for 60 s backend considers the slot dead). Reset:
-- those remaining — to the lobby, after RESET_KICK_SEC live connections are closed, the mod's match state (JoinGuard, results,
-- hero probe, loot) is zeroed, Epoch = os.time(), the map is reloaded with the same StartLocalBattle(<map>?listen)
-- (relaunch experience 31.10: the map reloads, battle_fetch is sent again), the host is an observer again. Until the reset
-- reaches ready, the heartbeat has phase=resetting; ready with a new epoch -> backend returns the slot to free.
local SLOT_HB_SEC = 10
local SLOT_HB_FAST_SEC = 2
local RESET_KICK_SEC = 5
local RESET_GRACE_SEC = 30    -- how long we wait for clients to leave by themselves after "match over" (we do not cut the results screen)
local RESET_MAX_WAIT_SEC = 60 -- after this the map is reloaded even if someone stayed
local SlotHbAt = 0
local SlotLast = nil      -- last heartbeat state line (only changes go to the log)
local Resetting = nil     -- { Battle, Why, At } — a reset is in progress until the phase reaches ready

local function RemoteControllers()
    local Out = {}
    for _, PC in ipairs(FindAllOf("BP_TgfPlayerController_C") or {}) do
        if IsValid(PC) and not FullName(PC):find("Default__", 1, true) then
            local IsLocal, Conn = false, nil
            pcall(function() IsLocal = PC:IsLocalController() end)
            pcall(function() Conn = PC.Player end)
            if not IsLocal and IsValid(Conn) and FullName(Conn):find("LocalPlayer", 1, true) then IsLocal = true end
            if not IsLocal and IsValid(Conn) then Out[#Out + 1] = PC end
        end
    end
    return Out
end

-- The mod's match state — as in a freshly started process. Settings (probe on/off, hooks, rejoin, URL) stay.
-- UBattleDsUtility records (GameInstance — survive a map reload) are taken as the baseline: a result is sent
-- only when a record changes in the new match.
-- KeepExp (10.11): ascension potion exp for the match is carried over — hall -> match and floor -> floor in the same match (before,
-- what was drunk on the first floor was lost when exiting from the second: "of them potion 0"); a new match (slot reset) — from zero.
local function ResetState(KeepExp)
    local Old = GoProbe
    GoProbe = { On = Old.On, Hooked = Old.Hooked, Calls = Old.Calls, Rejoin = Old.Rejoin, ResultUrl = Old.ResultUrl,
        LeftStep = Old.LeftStep, Entries = {}, Players = {}, Pending = {}, Finished = {} }
    if KeepExp and Old.BonusExp and next(Old.BonusExp) then
        GoProbe.BonusExp = Old.BonusExp
        local L = {}
        for Open, N in pairs(Old.BonusExp) do L[#L + 1] = Open .. " " .. tostring(N) end
        Log("[EXP] potion exp carried over to the new map: " .. table.concat(L, ", "))
    end
    local Mgr = FindLive("BattleDsUtility")
    local Rsp = Mgr and NativeCall("gostate", { mgr = string.format("0x%X", Mgr:GetAddress()) }) or ""
    local N = 0
    for Line in Rsp:gmatch("e%d+=([^\n]+)") do
        local Key, St, Reason, _, Ptr = Line:match("^(.-)|(-?%d+)|(-?%d+)|(.-)|(.-)|")
        if Key then
            GoProbe.Entries[Key] = St .. "/" .. Reason .. "/" .. Ptr
            GoProbe.Status = GoProbe.Status or {}
            GoProbe.Status[Key] = St
            N = N + 1
        end
    end
    HeroCount, HeroProbeAt, RepSeen = 0, nil, {}
    LootChecked, LiteApplied, LiteWidgetsAt = false, false, nil
    TickSamples = {}
    return N
end

BeginReset = function(Battle, Why)
    if Resetting then return end
    Resetting = { Battle = Battle or "?", Why = Why, At = os.time() }
    Host, TravelPending = nil, nil
    -- this process's map has not been raised yet (start or restart in the middle of a reset) — nothing to reload:
    -- reaching ready with a new epoch is enough
    if Standby.On and Phase == "standby" then
        Log(string.format("[SLOT] сброс %s (%s) в standby: карты нет — новый epoch, остаёмся в лобби", Resetting.Battle, Why))
        Resetting, Epoch, SlotHbAt = nil, os.time(), 0
        return
    end
    if Phase ~= "ready" and Phase ~= "ended" then
        Log(string.format("[SLOT] сброс %s (%s): карта ещё не поднята (фаза %s) — только отметка", Resetting.Battle, Why, Phase))
        return
    end
    -- 03.11 (live test 14:46): ClientTravel "to the lobby" right after the result cut the connection before the client got
    -- "match over" from backend — the client considered the room BATTLING and reconnected by itself (native 0x145993920), already into
    -- the reloaded map. So we push nobody out: wait until clients leave by themselves (after "match over"),
    -- after RESET_GRACE_SEC — ClientTravel to the lobby for those left (like the exit button), reload — when there are no connections
    -- or after RESET_MAX_WAIT_SEC.
    Log(string.format("[SLOT] сброс после комнаты %s (%s): ждём ухода игроков (на сервере %d)", Resetting.Battle, Why,
        #RemoteControllers()))
    SetPhase("resetting", "room " .. Resetting.Battle)
end

local function ResetStep()
    -- 05.11: players with a delivered result who left the map — /ds/player_left (in a match GameOverProbe does it). OpenId
    -- of live ones — from their PlayerState; if even one is not read — not sent in this tick (an extra signal would deliver the result early
    -- while the client is still on the raid map)
    pcall(function()
        local Conn, Sure = {}, true
        for _, PC in ipairs(RemoteControllers()) do
            local Ok = pcall(function() Conn[PC.PlayerState.MyOpenId:ToString()] = true end)
            if not Ok then Sure = false end
        end
        if Sure then GoProbe.LeftStep(Conn, Resetting and Resetting.Battle or "") end
    end)
    if os.time() - PhaseSince < RESET_KICK_SEC then return end
    local Left = RemoteControllers()
    local Waited = os.time() - PhaseSince
    if #Left > 0 and Waited < RESET_MAX_WAIT_SEC then
        if Waited >= RESET_GRACE_SEC then
            Resetting.Kicked = Resetting.Kicked or {}
            for _, PC in ipairs(Left) do
                local Key = FullName(PC)
                if not Resetting.Kicked[Key] then
                    Resetting.Kicked[Key] = true
                    -- 03.11 (live test 16:13): the native ClientReturnToMainMenuWithTextReason took the client through the start menu —
                    -- a hall without own hero (no role subscription), tabs broke. As before — ClientTravel to the lobby, but
                    -- only after waiting: "match over" is already at the client by then, and it does not reconnect
                    local Ok = SendToLobby(PC, "Рейд завершён")
                    Log(string.format("[SLOT] сброс: %s ещё на сервере через %d с — в лобби (ClientTravel ok=%s)", Short(PC), Waited,
                        tostring(Ok)))
                end
            end
        end
        return
    end
    local Kicked = #Left
    if Kicked > 0 then Log(string.format("[SLOT] сброс: через %d с на сервере ещё %d — перезагружаю всё равно", Waited, Kicked)) end
    if Standby.On then
        -- one process per match: do not reload the map (GameMode would start the game itself) — exit, ds-agent starts a new one
        Standby.ExitAt, SlotHbAt = os.time(), 0
        SetPhase("exiting", "after room " .. Resetting.Battle)
        return
    end
    local Entries = ResetState()
    local GM = FindLive("GameModeBase")
    ResetOldGM = GM and string.format("%X %s", GM:GetAddress(), FullName(GM)) or nil
    Epoch = os.time()
    -- dbnative raid session (backend uses it to answer 409 raid_finished to players of an already finished raid) — a new one
    local Session = (NativeCall("newsession", { epoch = Epoch }) or ""):match("session=([^\n]*)") or "unchanged"
    Log(string.format("[SLOT] сброс: закрыто соединений %d, записей итогов взято как исходные %d, epoch %d, session %s — перезагрузка карты",
        Kicked, Entries, Epoch, Session))
    if IdleMap and IdleMap ~= MapPath then
        Log("[SLOT] сброс: обратно на карту простоя " .. IdleMap)
        MapPath = IdleMap
    end
    SetPhase("start", "reset")
end

-- Phase for backend: while a reset is running — resetting, otherwise ready / hosting (start..observer) / ended
local function SlotPhase()
    if Standby.ExitAt then return "exiting" end
    if Resetting then return "resetting" end
    if Host and Host.Phase ~= "loading" then return Host.Phase end -- waiting / entering / battle
    if Phase == "ready" or Phase == "ended" or Phase == "standby" then return Phase end
    return "hosting"
end

-- 12.11 (user's decision): a character has at most one relic of each kind — weapon (slots 1–4, both pairs),
-- clothing (11–15), jewelry (16–18); 3 in total. The lobby is checked by backend (src/rpc/handlers/relic_limit.rs), in a match — here:
-- every RelicLimit.EverySec we check what players wear, a second relic of a kind is moved to the bag (RemoveEquipToInventory).
-- We remove the new one: the one that was not there at the previous check; otherwise — the one in the higher slot. Bag full — log it and
-- keep trying; after leaving the raid backend removes the extra one itself. A relic is item 220000000…229999999.
RelicLimit = { EverySec = 2, At = 0, Seen = {}, Kinds = { { 1, 2, 3, 4 }, { 11, 12, 13, 14, 15 }, { 16, 17, 18 } },
    Names = { "weapon", "clothing", "jewelry" }, Full = {} }

function RelicLimit.Tick()
    if Role ~= "dbserver" or not (Host and Host.Phase == "battle") then return end
    if os.time() - RelicLimit.At < RelicLimit.EverySec then return end
    RelicLimit.At = os.time()
    RelicLimit.EqClass = RelicLimit.EqClass or StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
    if not IsValid(RelicLimit.EqClass) then return end
    local Seen = {}
    for _, PC in ipairs(FindAllOf("BP_TgfPlayerController_C") or {}) do
        if IsValid(PC) and not FullName(PC):find("Default__", 1, true) then
            local P = nil
            pcall(function() P = PC.Pawn end)
            local Eq = nil
            if IsValid(P) then pcall(function() Eq = P:GetComponentByClass(RelicLimit.EqClass) end) end
            if IsValid(Eq) then
                local Key = Short(P)
                local Old = RelicLimit.Seen[Key] or {}
                local Now = {}
                for Kind, Slots in ipairs(RelicLimit.Kinds) do
                    local Worn = {}
                    for _, Slot in ipairs(Slots) do
                        local E = nil
                        pcall(function() E = Eq:GetEquipBySlot(Slot, -1) end)
                        local Id = 0
                        if IsValid(E) then
                            pcall(function() Id = tonumber(E:GetEquipId()) or 0 end)
                            if Id == 0 and IsValid(E.InventoryInstance) then
                                pcall(function() Id = tonumber(E.InventoryInstance:GetItemId()) or 0 end)
                            end
                        end
                        if Id >= 220000000 and Id <= 229999999 then
                            Worn[#Worn + 1] = { Slot = Slot, Id = Id }
                            Now[Slot] = Id
                        end
                    end
                    if #Worn > 1 then
                        -- keep the one that was there at the previous check (in the lower slot), remove the rest
                        local Keep = nil
                        for _, W in ipairs(Worn) do
                            if Old[W.Slot] == W.Id then Keep = Keep or W end
                        end
                        Keep = Keep or Worn[1]
                        for _, W in ipairs(Worn) do
                            if W ~= Keep then
                                local Can = false
                                pcall(function() Can = Eq:CanRemoveEquipToInventory(W.Slot) end)
                                if Can then
                                    local Ok, Err = pcall(function() Eq:RemoveEquipToInventory(W.Slot) end)
                                    Now[W.Slot] = nil
                                    Log(string.format("[RELIC] %s: second relic of kind '%s' %d in slot %d (kept %d in %d) — moved to the bag%s",
                                        Key, RelicLimit.Names[Kind], W.Id, W.Slot, Keep.Id, Keep.Slot, Ok and "" or (": error " .. tostring(Err))))
                                elseif not RelicLimit.Full[Key .. W.Slot] then
                                    RelicLimit.Full[Key .. W.Slot] = true
                                    Log(string.format("[RELIC] %s: second relic of kind '%s' %d in slot %d — nowhere to move it (bag?), will keep trying",
                                        Key, RelicLimit.Names[Kind], W.Id, W.Slot))
                                end
                            end
                        end
                    end
                end
                Seen[Key] = Now
            end
        end
    end
    RelicLimit.Seen = Seen
end

-- 13.11 MobBones (was SeedBones 12.11; docs/FIX_LIST, WORKAROUNDS K26): on a headless server mob meshes have
-- VisibilityBasedAnimTickOption = 1 (AlwaysTickPose) — the animation runs, but bones and physics bodies are not updated
-- (ShouldUpdateTransform = bRecentlyRendered || AlwaysTickPoseAndRefreshBones, bRecentlyRendered is always false on the server).
-- The bone hitbox freezes in the spawn pose (for a skeleton — a heap on the floor). Misses: the druid tree strike
-- (BP_TGF_Summoned_Seed_C, a trace between branch sockets) and projectiles: the Projectile channel (GameTraceChannel7) is blocked by the
-- TgfCharacterMesh profile, while the capsule lets it through (LOCAL analysis 13.11: relic dagger B_TGF_Normal_Projectile_DaggerRelicIce
-- through a skeleton). Every Every s we set 0 on the mesh of each mob and summon within Radius cm of a live player (BP_Tgf_Monster
-- rolls back to CachedVisibilityBasedAnimTickOption — we set it again); for those farther away we restore their value (CPU).
-- Settings: <dbroot>/dbserver-mobbones.txt "enabled=1 radius=3000 every=1", re-read every 10 s.
-- A global table: the main block of the file already has ~200 locals (Lua limit).
MobBones = { Enabled = true, Radius = 3000, Every = 1, At = 0, CfgAt = 0, Orig = {}, Logged = 0, LastNote = "" }

function MobBones.Config()
    if os.time() - MobBones.CfgAt < 10 then return end
    MobBones.CfgAt = os.time()
    local Cfg = ReadFile("dbserver-mobbones.txt") or ""
    local Was = string.format("%s/%d/%d", tostring(MobBones.Enabled), MobBones.Radius, MobBones.Every)
    for K, V in Cfg:gmatch("(%a+)%s*=%s*([%d%.]+)") do
        local N = tonumber(V)
        if K == "enabled" then MobBones.Enabled = N ~= 0
        elseif K == "radius" and N and N > 0 then MobBones.Radius = N
        elseif K == "every" and N and N >= 1 then MobBones.Every = math.floor(N) end
    end
    local Now = string.format("%s/%d/%d", tostring(MobBones.Enabled), MobBones.Radius, MobBones.Every)
    if Now ~= Was then
        Log(string.format("[MOBBONES] settings: enabled=%s radius=%d every=%d", tostring(MobBones.Enabled), MobBones.Radius, MobBones.Every))
    end
end

-- Restore the meshes' value (left the radius, disabled, end of match). Keep — addresses to leave alone
function MobBones.Restore(Keep, Meshes)
    local Back = 0
    for Addr, Orig in pairs(MobBones.Orig) do
        if not (Keep and Keep[Addr]) then
            local M = Meshes and Meshes[Addr]
            if M and IsValid(M) then
                pcall(function() M.VisibilityBasedAnimTickOption = Orig end)
                Back = Back + 1
            end
            MobBones.Orig[Addr] = nil
        end
    end
    return Back
end

function MobBones.Tick()
    if Role ~= "dbserver" then return end
    if not (Host and Host.Phase == "battle") then
        if next(MobBones.Orig) then MobBones.Orig = {} end -- the match ended: the meshes went away with the map
        return
    end
    if os.time() - MobBones.At < MobBones.Every then return end
    MobBones.At = os.time()
    MobBones.Config()

    -- mobs and summons: every LyraCharacter except heroes, plus druid trees (if they are not LyraCharacter)
    local Mobs, Meshes, SeenMob = {}, {}, {}
    local function AddMob(A)
        if not IsValid(A) then return end
        local Name = FullName(A)
        if Name:find("Default__", 1, true) or Name:find("Hero", 1, true) then return end
        local M = nil
        pcall(function() M = A.Mesh end)
        if not IsValid(M) then return end
        local Addr = nil
        pcall(function() Addr = M:GetAddress() end)
        if not Addr or SeenMob[Addr] then return end
        SeenMob[Addr] = true
        Meshes[Addr] = M
        Mobs[#Mobs + 1] = { A = A, M = M, Addr = Addr }
    end
    for _, A in ipairs(FindAllOf("LyraCharacter") or {}) do AddMob(A) end
    for _, A in ipairs(FindAllOf("BP_TGF_Summoned_Seed_C") or {}) do AddMob(A) end

    if not MobBones.Enabled then
        local Back = MobBones.Restore(nil, Meshes)
        if Back > 0 then Log(string.format("[MOBBONES] disabled: restored the value of %d meshes", Back)) end
        return
    end

    -- live players
    local Players = {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            pcall(function()
                local Pawn = PS.PawnPrivate
                if PS.MyOpenId:ToString() ~= "LOCAL_GAME_MODE_OPENID" and IsValid(Pawn) and not Pawn:IsDeadOrDying() then
                    local L = Pawn:K2_GetActorLocation()
                    Players[#Players + 1] = { L.X, L.Y, L.Z }
                end
            end)
        end
    end

    local R2, Keep, Near, Set = MobBones.Radius ^ 2, {}, 0, 0
    for _, Mob in ipairs(Mobs) do
        local X, Y, Z = nil, nil, nil
        pcall(function() local L = Mob.A:K2_GetActorLocation(); X, Y, Z = L.X, L.Y, L.Z end)
        local Close = false
        if X then
            for _, P in ipairs(Players) do
                if (P[1] - X) ^ 2 + (P[2] - Y) ^ 2 + (P[3] - Z) ^ 2 <= R2 then Close = true; break end
            end
        end
        if Close then
            Near = Near + 1
            Keep[Mob.Addr] = true
            pcall(function()
                local Cur = Mob.M.VisibilityBasedAnimTickOption
                if Cur ~= 0 then
                    if MobBones.Orig[Mob.Addr] == nil then MobBones.Orig[Mob.Addr] = Cur end
                    Mob.M.VisibilityBasedAnimTickOption = 0
                    Set = Set + 1
                elseif MobBones.Orig[Mob.Addr] == nil then
                    MobBones.Orig[Mob.Addr] = false -- already 0 but not set by us: leave it when it leaves the radius
                end
            end)
        end
    end
    -- false — the value was 0 not because of us: nothing to restore
    for Addr, Orig in pairs(MobBones.Orig) do
        if Orig == false and not Keep[Addr] then MobBones.Orig[Addr] = nil end
    end
    local Back = MobBones.Restore(Keep, Meshes)

    local Note = string.format("mobs %d, players %d, near %d", #Mobs, #Players, Near)
    if MobBones.Logged < 30 and (Note ~= MobBones.LastNote or Set > 0 or Back > 0) then
        MobBones.Logged = MobBones.Logged + 1
        MobBones.LastNote = Note
        Log(string.format("[MOBBONES] %s; enabled %d, restored %d", Note, Set, Back))
    end
end

local function SlotTick()
    if Role ~= "dbserver" then return end
    if Phase == "init" or Phase == "off" or Phase == "login" or Phase == "lobby" or Phase == "quit" then return end
    if Resetting and Phase == "ready" then
        Log(string.format("[SLOT] сброс после комнаты %s завершён за %d с, epoch %d", Resetting.Battle,
            os.time() - Resetting.At, Epoch))
        Resetting = nil
        SlotHbAt = 0 -- report ready at once
    end
    -- when idle and in the hall — every SLOT_HB_FAST_SEC (the host task comes only in the response), in a match — SLOT_HB_SEC
    local Fast = (Host and Host.Phase ~= "battle") or (not Host and Phase == "ready" and #RemoteControllers() == 0)
        or Phase == "standby" or Phase == "exiting"
    if os.time() - SlotHbAt < (Fast and SLOT_HB_FAST_SEC or SLOT_HB_SEC) then return end
    SlotHbAt = os.time()
    local Base = BackendBase()
    if not Base then return end
    local S = {}
    for K, X in (NativeCall("stats", {}) or ""):gmatch("([%w_]+)=(%S+)") do S[K] = tonumber(X) end
    local Players = Phase == "ready" and #RemoteControllers() or 0
    local Absent = {}
    for _, Open in ipairs(Host and Host.Absent or {}) do Absent[#Absent + 1] = '"' .. Open .. '"' end
    local Body = string.format('{"port":%d,"slot":"%s","phase":"%s","battle_id":"%s","absent":[%s],"epoch":%d,"players":%d,"ws_mb":%d,"private_mb":%d,"map":"%s"}',
        Port, SlotAddr, SlotPhase(), Host and Host.Battle or "", table.concat(Absent, ","), Epoch, Players, S.ws_mb or 0,
        S.private_mb or 0, HostedMap)
    local Rsp = NativeCall("http", { url = Base .. "/ds/slot", body = Body }) or ""
    local Http = Rsp:match("http=(%d+)")
    if Standby.ExitAt and Http then Standby.ExitSent = true end
    local Action = Rsp:match('"action"%s*:%s*"(%a+)"') or "?"
    local State = string.format("phase=%s epoch=%d http=%s action=%s%s", SlotPhase(), Epoch, Http or "none", Action,
        Rsp:find('"unknown_slot"%s*:%s*true') and " unknown_slot (check -dbslot and DS_POOL)" or "")
    if not Http then State = State .. " " .. (Rsp:match("err=([^\n]*)") or "") end
    if State ~= SlotLast then
        SlotLast = State
        Log("[SLOT] heartbeat " .. State)
    end
    if Action == "reset" and not Resetting then
        BeginReset(Rsp:match('"battle_id"%s*:%s*"([^"]+)"'), "action=reset in heartbeat")
    elseif (Action == "host" or Action == "members") and not Resetting then
        OnHost(Rsp)
    end
    -- 09.11: host command from backend in the heartbeat response ("cmd": "ban <open_id>") — a channel from the VM to Robot, where the control
    -- file is unavailable. Only a safe list (currently — ban).
    local SlotCmd = Rsp:match('"cmd"%s*:%s*"([^"]+)"')
    if SlotCmd then
        local V, A = SlotCmd:match("^%s*(%a+)%s*(%S*)")
        if V == "ban" and A and A:match("^%d+$") then
            Log("[SLOT] backend command: " .. SlotCmd)
            ExecuteInGameThread(function()
                -- HostCommand is declared below SlotTick: taken via a global reference (RuHostCommand, set after it)
                local Ok, Err = pcall(function() return RuHostCommand(V, A) end)
                if not Ok then Log("[SLOT] command " .. SlotCmd .. ": error " .. tostring(Err)) end
            end)
        else
            Log("[SLOT] backend command rejected: " .. SlotCmd)
        end
    end
end

-- --- Waiting hall (03.11, prototype; docs/WAITING_ROOM.md) ----------------------------------------------------
-- On listen the native hall is not enabled: "room ready" (UBattleDsUtility +0x244, RoomReady) is set only in
-- DS mode, and the native transition to the match (OnMapFinish → MapTravel) without it is OpenLevel without ?listen. So the mod drives
-- the native BP_TgfWaiting itself and does the transition with servertravel (clients reconnect and get the data again).
-- The hall map is the usual -dbmap; the match map is line 1 of dbserver-waiting.txt. Commands in <role>-control.txt:
--   wr_ready <sec>          — OnRoomReady() on BP_TgfWaiting + WaitingUserRemainTime = <sec> (waiting countdown)
--   wr_count <cur> <total>  — OnUserCountChanged(cur, total): with cur >= total > 0 — the entry countdown at once
--   wr_travel [map]         — transition now (without a map — the match map from dbserver-waiting.txt)
-- The native BP calls EnterGame at the end of the entry countdown — the mod's post-hook does the transition.
local WAITING_COMP = "/Game/TGF/Components/GameStateComponents/BP_TgfWaiting.BP_TgfWaiting_C"
local WrHooked = false

local function BattleMapAfterWaiting()
    return (ReadFile("dbserver-waiting.txt") or ""):match("^%s*(/TgfMaps/[%w_/]+)")
end

local function WaitingComp()
    return FindLive("BP_TgfWaiting_C")
end

local function WaitingState(C)
    local V = {}
    for _, F in ipairs({ "IsWaitingUser", "WaitingUserRemainTime", "IsEnteringGame", "EnteringGameRemainTime",
        "CurrentUserCount", "TotalUserCount" }) do
        local Ok, X = pcall(function() return C[F] end)
        V[#V + 1] = F .. "=" .. (Ok and tostring(X) or "?")
    end
    return table.concat(V, " ")
end

local function TravelTo(Map, Why)
    if TravelPending then return "transition already in progress" end
    local PC = FindLive("PlayerController")
    local Lib = KismetSystem()
    if not (PC and Lib) then return "no PlayerController" end
    local GM = FindLive("GameModeBase")
    ResetOldGM = GM and string.format("%X %s", GM:GetAddress(), FullName(GM)) or nil
    TravelPending = { From = HostedMap, To = Map, At = os.time() }
    MapPath = Map
    local Ok, Err = pcall(function() Lib:ExecuteConsoleCommand(PC, "servertravel " .. Map .. "?listen", PC) end)
    Log(string.format("[WAITING] servertravel %s?listen (%s): ok=%s%s", Map, Why, tostring(Ok), Ok and "" or (" " .. tostring(Err))))
    if not Ok then TravelPending = nil; return "error " .. tostring(Err) end
    SetPhase("hosting", "servertravel " .. Map)
    return "ok"
end

-- Hall protection (03.11, LOCAL analysis: research/waiting_cfg/BP_TgfWaiting.txt): the native OnPlayerLoginSuccess(Character)
-- puts loose tags TGF.Procedure.WaitingRoom + TGF.Ability.BuffState.NoDamage on the hero (AddWaitingTagsToCharacter) and
-- InvSee, OnWaitingRoomLeave removes them before EnterGame. On listen the login sometimes comes without a hero (not yet / the cast
-- failed) — no tags, damage goes through in the hall (prod 16:45). We add them to those the native call did not handle, exactly once:
-- AddLooseGameplayTags counts copies, an extra one would survive the removal and go into the match.
-- Tagged: pawn address -> true (tags present: native call or ours); Seen: when first seen in the hall;
-- TAG_DELAY: s, let the native OnPlayerLoginSuccess fire first (one table — 200-local limit in main)
local Hall = { Tagged = {}, Seen = {}, TAG_DELAY = 3, CheckAt = 0 }

-- Tag NoDamage on the hero's ASC: true/false, nil — could not check (then — only by the native login hook)
function Hall.HasNoDamage(P)
    local Ok, Has = pcall(function()
        local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
        local ASC = Lib:GetAbilitySystemComponent(P)
        if not IsValid(ASC) then return nil end
        return ASC:HasMatchingGameplayTag({ TagName = FName("TGF.Ability.BuffState.NoDamage") })
    end)
    if Ok and type(Has) == "boolean" then return Has end
    return nil
end

function Hall.Protect()
    if os.time() == Hall.CheckAt then return end -- at most once a second
    Hall.CheckAt = os.time()
    local C = WaitingComp()
    if not C then return end
    local HostPawn = nil
    pcall(function() HostPawn = LocalPC().Pawn end)
    for _, P in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        local Addr = IsValid(P) and not FullName(P):find("Default__", 1, true) and P:GetAddress()
        local HasPS = false
        pcall(function() HasPS = IsValid(P.PlayerState) end)
        if Addr and HasPS and not Hall.Tagged[Addr] and not (HostPawn and IsValid(HostPawn) and HostPawn:GetAddress() == Addr) then
            Hall.Seen[Addr] = Hall.Seen[Addr] or os.time()
            if os.time() - Hall.Seen[Addr] >= Hall.TAG_DELAY then
                Hall.Tagged[Addr] = true
                -- the tag is already there (the native login made it) — do not add a second copy: the removal before the match removes one
                local Has = Hall.HasNoDamage(P)
                if Has then
                    Log(string.format("[WAITING] hall tags of %s already present", Short(P)))
                else
                    local Ok, Err = pcall(function() C:AddWaitingTagsToCharacter(P); C:AddInvSeeToCharacter(P) end)
                    Log(string.format("[WAITING] hall tags (NoDamage) for hero %s: ok=%s (tag check: %s)%s", Short(P), tostring(Ok),
                        tostring(Has), Ok and "" or (" " .. tostring(Err))))
                end
            end
        end
    end
end

local function HookWaiting()
    if WrHooked or not WaitingComp() then return end
    WrHooked = true
    local Ok = pcall(function()
        RegisterHook(WAITING_COMP .. ":EnterGame", function()
            Log("[WAITING] EnterGame (native BP: entry countdown finished)")
            ExecuteInGameThread(function()
                if Host and Host.Phase == "empty" then
                    Log("[WAITING] hall empty — not going into the match, waiting for reset from backend")
                    return
                end
                local Map = (Host and Host.Scenes[2]) or BattleMapAfterWaiting()
                if not Map then Log("[WAITING] no match map (dbserver-waiting.txt) — no transition"); return end
                Log("[WAITING] переход: " .. TravelTo(Map, "EnterGame"))
            end)
        end)
    end)
    pcall(function()
        RegisterHook(WAITING_COMP .. ":StartEnteringGameCountdown", function()
            if Host and Host.Phase == "waiting" then
                -- members of the roster who never came in — backend does not wait for their result (absent); nobody came in — we do not go into the match
                -- (phase=empty: backend closes the room and answers action=reset)
                local N, Absent = 0, {}
                for Open in pairs(Host.Members) do
                    if (GoProbe.LastConns or {})[Open] then N = N + 1 else Absent[#Absent + 1] = Open end
                end
                Host.Absent = Absent
                Host.Phase = N == 0 and "empty" or "entering"
                SlotHbAt = 0 -- tell backend at once
                Log(string.format("[HOST] %s: %s, joined %d of %d%s", Host.Battle, Host.Phase, N, Host.Total,
                    #Absent > 0 and (", did not come: " .. table.concat(Absent, ",")) or ""))
            end
        end)
    end)
    -- a native login into the hall with a hero — it put the hall tags itself (Hall.Protect leaves this hero alone)
    pcall(function()
        RegisterHook(WAITING_COMP .. ":OnPlayerLoginSuccess", function(Self, Character)
            pcall(function()
                local P = Character:get()
                if IsValid(P) then Hall.Tagged[P:GetAddress()] = true end
            end)
        end)
    end)
    for _, Fn in ipairs({ "OnRoomReady", "FinishWaitingUser", "StartEnteringGameCountdown", "OnPlayerLoginSuccess" }) do
        pcall(function()
            RegisterHook(WAITING_COMP .. ":" .. Fn, function() Log("[WAITING] " .. Fn) end)
        end)
    end
    Log("[WAITING] hook EnterGame: " .. tostring(Ok) .. ", match map: " .. tostring(BattleMapAfterWaiting()))
end

local function WaitingCommand(Verb, Arg1, Arg2)
    local C = WaitingComp()
    if not C then return "no BP_TgfWaiting (not a hall map?)" end
    HookWaiting()
    if Verb == "wr_ready" then
        local Sec = tonumber(Arg1) or 120
        local Ok, Err = pcall(function() C:OnRoomReady() end)
        local OkT = pcall(function() C.WaitingUserRemainTime = Sec end)
        return string.format("OnRoomReady ok=%s%s, WaitingUserRemainTime=%d ok=%s | %s", tostring(Ok),
            Ok and "" or (" " .. tostring(Err)), Sec, tostring(OkT), WaitingState(C))
    elseif Verb == "wr_count" then
        local Cur, Total = tonumber(Arg1) or 0, tonumber(Arg2) or 0
        local Ok, Err = pcall(function() C:OnUserCountChanged(Cur, Total) end)
        return string.format("OnUserCountChanged(%d, %d) ok=%s%s | %s", Cur, Total, tostring(Ok), Ok and "" or (" " .. tostring(Err)),
            WaitingState(C))
    elseif Verb == "wr_travel" then
        local Map = (Arg1 ~= nil and Arg1 ~= "") and Arg1 or BattleMapAfterWaiting()
        if not Map then return "no map: wr_travel <map> or dbserver-waiting.txt" end
        return TravelTo(Map, "command")
    end
    return "?"
end

-- Every 10 s in the hall — BP_TgfWaiting state to the log (changes only), and after the transition — a new match for the mod
local WrLast, WrAt = nil, 0

-- Task from the /ds/slot response: {action:"host", host:{battle_id, scenes:[hall, match, …], waiting_timeout, members:[{open_id, team_id}]}}
-- (option_args are not applied yet — options are set by the client mod by MapId)
local function ParseHost(Rsp)
    local H = { Battle = Rsp:match('"host"%s*:%s*{.-"battle_id"%s*:%s*"([^"]+)"') or Rsp:match('"battle_id"%s*:%s*"([^"]+)"'),
        Scenes = {}, Members = {}, Total = 0, Phase = "loading", At = os.time() }
    for S in (Rsp:match('"scenes"%s*:%s*%[(.-)%]') or ""):gmatch('"([^"]+)"') do H.Scenes[#H.Scenes + 1] = S end
    H.Timeout = tonumber(Rsp:match('"waiting_timeout"%s*:%s*([%d%.]+)') or "") or 120
    H.Capacity = tonumber(Rsp:match('"capacity"%s*:%s*(%d+)') or "")
    -- 08.11: launcher lobby hall — minimum players for a match and how long to hold the countdown beyond the timer (HallHold)
    H.MinPlayers = tonumber(Rsp:match('"min_players"%s*:%s*(%d+)') or "") or 0
    H.HoldSecs = tonumber(Rsp:match('"hold_secs"%s*:%s*(%d+)') or "") or 0
    -- 08.11: the task's map_id — for the loot map (LootMap, docs/LEYLINE_SEALS.md)
    H.MapId = tonumber(Rsp:match('"map_id"%s*:%s*(%d+)') or "")
    -- 06.11: map options by group size (dynamic_option_args, solo/duo/trio): the number and type of portals and the spawn
    -- points depend on them. The options are set by RuClient on this same server (MAP_RULES) — passed via a file next to the exe, it checks the map.
    H.OptionArgs = (Rsp:match('"option_args"%s*:%s*%[(.-)%]') or ""):gsub("%s", "")
    -- 09.11: each server has its own file (ru-map-options-<port>.txt, 7777 — without the port): all Robot ports start from
    -- one folder, neighbours overwrote a shared file — the Castle in solo got trio options (docs/CASTLE_SPAWN.md)
    pcall(function()
        local F = io.open("ru-map-options" .. Inst .. ".txt", "w")
        if F then F:write(H.OptionArgs, "\nmap=", Rsp:match('"map_id"%s*:%s*(%d+)') or "", "\n"); F:close() end
    end)
    if HostFixLastOpt ~= H.OptionArgs then
        HostFixLastOpt = H.OptionArgs
        Log("[HOST] map options by group: [" .. H.OptionArgs .. "]")
    end
    for Open in (Rsp:match('"members"%s*:%s*%[(.-)%]') or ""):gmatch('"open_id"%s*:%s*"?(%d+)"?') do
        if not H.Members[Open] then H.Members[Open] = true; H.Total = H.Total + 1 end
    end
    return H
end

-- Hall ready: native waiting countdown with the room timeout
local function StartWaiting()
    local C = WaitingComp()
    if not C then return false end
    HookWaiting()
    local Ok, Err = pcall(function() C:OnRoomReady() end)
    pcall(function() C.WaitingUserRemainTime = Host.Timeout end)
    Host.Phase, Host.Counted = "waiting", -1
    Hall.Tagged, Hall.Seen = {}, {}
    SlotHbAt = 0
    Log(string.format("[HOST] %s: зал готов, отсчёт %d с, ждём %d игроков (OnRoomReady ok=%s%s)", Host.Battle, Host.Timeout,
        Host.Total, tostring(Ok), Ok and "" or (" " .. tostring(Err))))
    return true
end

OnHost = function(Rsp)
    local H = ParseHost(Rsp)
    if not H.Battle then return end
    if Host and Host.Battle == H.Battle then
        -- open hall (backend MATCH_OPEN_HALL): the same hall, new members — append to the roster (03.11)
        local Added = {}
        for Open in pairs(H.Members) do
            if not Host.Members[Open] then Host.Members[Open] = true; Host.Total = Host.Total + 1; Added[#Added + 1] = Open end
        end
        if H.Capacity then Host.Capacity = H.Capacity end
        if #Added > 0 then
            Log(string.format("[HOST] %s: added to the hall %s, total %d", Host.Battle, table.concat(Added, ","), Host.Total))
        end
        return
    end
    -- 05.11: one scene — a custom game arena (no waiting hall), see WaitingTick; before, two were required
    if not H.Scenes[1] then
        Log("[HOST] задание без сцен — пропускаю: " .. Rsp:gsub("\n", " "):sub(1, 200))
        return
    end
    Host = H
    IdleMap = H.Scenes[1]
    Log(string.format("[HOST] задание %s: зал %s -> бой %s, игроков %d, таймаут %d с", H.Battle, H.Scenes[1], H.Scenes[2],
        H.Total, H.Timeout))
    if Phase == "ready" and HostedMap == H.Scenes[1] and WaitingComp() then
        StartWaiting()
    elseif Phase == "start" then
        MapPath = H.Scenes[1] -- the map has not been requested yet — the right hall at once
        H.LoadIssued = true
    end
    -- otherwise (ready/ended on another map, or the map is loading) — WaitingTick loads the hall on ready
end

local function LoadWaiting()
    local GM = FindLive("GameModeBase")
    ResetOldGM = GM and string.format("%X %s", GM:GetAddress(), FullName(GM)) or nil
    MapPath = Host.Scenes[1]
    Host.LoadIssued = true
    Log("[HOST] loading hall " .. MapPath)
    SetPhase("start", "hall for " .. Host.Battle)
end

-- Room roster members who joined (live connections with an OpenId from members; the host does not count) -> BP_TgfWaiting
local function CountMembers()
    local C = WaitingComp()
    if not C then return end
    local N = 0
    for Open in pairs(Host.Members) do
        if (GoProbe.LastConns or {})[Open] then N = N + 1 end
    end
    local Cur, Tot = -1, -1
    pcall(function() Cur, Tot = C.CurrentUserCount, C.TotalUserCount end)
    -- "total" is the hall capacity, not the roster: on "all joined" the native BP goes into the match at once (10 s countdown), but the hall
    -- must wait the whole timeout (decision 03.11, WORKAROUNDS K18). Who did not come — still by Host.Members.
    local Show = math.max(Host.Capacity or 0, Host.Total + 1)
    -- the native OnPlayerLoginSuccess overwrites the numbers with its own (with the host, Total 0) — we put ours back
    if N ~= Host.Counted or Cur ~= N or Tot ~= Show then
        local Ok = pcall(function() C:OnUserCountChanged(N, Show) end)
        if N ~= Host.Counted then
            Log(string.format("[HOST] %s: в зале %d из %d, показываю из %d (OnUserCountChanged ok=%s)", Host.Battle, N, Host.Total,
                Show, tostring(Ok)))
        end
        Host.Counted = N
    end
end

-- 08.11 (user's decision): the launcher lobby hall does not start the match while it has fewer than Host.MinPlayers players: the mod does not
-- let the native countdown (WaitingUserRemainTime) go below HALL_HOLD_REMAIN. No longer than Host.HoldSecs beyond the
-- hall timer — then the match with those who came (players will not get stuck). Globals: 200-local limit.
HALL_HOLD_REMAIN = 30
function HallHold()
    local Min = Host and Host.MinPlayers or 0
    if Min <= 0 then return end
    local C = WaitingComp()
    if not C then return end
    Host.WaitingAt = Host.WaitingAt or os.time()
    local N = math.max(Host.Counted or 0, 0)
    local Limit = (Host.Timeout or 120) + (Host.HoldSecs or 0)
    if N >= Min or os.time() - Host.WaitingAt >= Limit then
        if Host.Holding then
            Host.Holding = false
            Log(string.format("[HALL] %s: countdown released — %d of %d needed in the hall%s", Host.Battle, N, Min,
                N >= Min and "" or string.format(", waited %d s — match with those who came", os.time() - Host.WaitingAt)))
        end
        return
    end
    local Remain = nil
    pcall(function() Remain = tonumber(C.WaitingUserRemainTime) end)
    if Remain and Remain < HALL_HOLD_REMAIN then
        pcall(function() C.WaitingUserRemainTime = HALL_HOLD_REMAIN end)
        if not Host.Holding or os.time() - (Host.HoldLogAt or 0) >= 30 then
            Host.HoldLogAt = os.time()
            Log(string.format("[HALL] %s: %d of %d needed in the hall — holding the countdown at %d s (no longer than %d s from opening)", Host.Battle,
                N, Min, HALL_HOLD_REMAIN, Limit))
        end
        Host.Holding = true
    end
end

local function WaitingTick()
    if TravelPending and Phase == "ready" then
        local N = ResetState(true)
        Log(string.format("[WAITING] переход %s -> %s за %d с: состояние боя мода взято заново (записей итогов как исходные %d)",
            TravelPending.From, TravelPending.To, os.time() - TravelPending.At, N))
        TravelPending = nil
        if Host then
            Host.Phase = "battle"
            SlotHbAt = 0
            Log(string.format("[HOST] %s: бой (%d с от задания)", Host.Battle, os.time() - Host.At))
        end
    end
    if Host and Phase == "ready" then
        if Host.Phase == "loading" then
            -- 05.11 custom game: arena (map_detail 30x0y) — one scene without a waiting hall; loaded — straight into the match
            if #Host.Scenes == 1 and HostedMap == Host.Scenes[1] then
                Host.Phase = "battle"
                SlotHbAt = 0
                Log(string.format("[HOST] %s: арена %s без зала — бой (%d с от задания)", Host.Battle, HostedMap, os.time() - Host.At))
                -- 05.11 20:4x: here we set GameState.MapId = 30101..30302 (arena loot) — rolled back: it did not fix the loot, but broke
                -- the mode (with MapId the arena turned on the shrinking zone, "modulo 0" in the BP). The arena MapId stays 0. FIX_LIST R1-loot.
            elseif HostedMap == Host.Scenes[1] and WaitingComp() then StartWaiting()
            elseif not Host.LoadIssued or HostedMap ~= Host.Scenes[1] then LoadWaiting() end
        end
        if Host.Phase == "waiting" then
            CountMembers()
            pcall(Hall.Protect)
            local OkHh, ErrHh = pcall(HallHold)
            if not OkHh then Log("[HALL] error: " .. tostring(ErrHh)) end
        end
    end
    if Phase ~= "ready" or os.time() - WrAt < 10 then return end
    WrAt = os.time()
    local C = WaitingComp()
    if not C then return end
    HookWaiting()
    local S = WaitingState(C)
    if S ~= WrLast then WrLast = S; Log("[WAITING] " .. S) end
end

-- --- Who killed whom (03.11, deadmoney: a "moonstone" for a player kill) ---------------------------------------
-- Killer sources in order:
--   1) UTgfAttributeComponent::MulticastDeathEvent(DamageInstigator, DamageTarget) — NetMulticast, the server calls it
--      on death via ProcessEvent, so the UE4SS hook fires (03.11: a hook on OnPlayerKillEvent — the listener of
--      Lyra.Elimination.Message — did not fire on the server);
--   2) native stats ALyraPlayerState.RelationIntStatistics (3 Kill / 4 BeKilled by OpenId) — 03.11 on the server
--      empty at the moment of death, kept in case the game fills it;
--   3) the last hit on the victim — UTgfAttributeComponent::ClientDamageDirectionEvent(.., DamageInstigator,
--      DamageTarget) (a client RPC, the server calls it on every hit); the last 3 — to the log for analysis.
-- The instigator (pawn, controller, PlayerState or projectile) is reduced to a PlayerState: PlayerState / Instigator / Owner.
-- A player killer (numeric OpenId, not the victim himself) -> POST /ds/kill {killer, victim, battle_id, session}, once per
-- victim per raid. Teammates are filtered out by backend.
local DAMAGE_KEEP = 3
local DamageHooked = false

local function PlayerStateOf(Obj, Depth)
    if not IsValid(Obj) or (Depth or 0) > 3 then return nil end
    if FullName(Obj):find("PlayerState", 1, true) then return Obj end
    local PS = nil
    pcall(function() PS = Obj.PlayerState end)
    if IsValid(PS) then return PS end
    local Next = nil
    pcall(function() Next = Obj.Instigator end)
    if IsValid(Next) and Next ~= Obj then
        PS = PlayerStateOf(Next, (Depth or 0) + 1)
        if PS then return PS end
    end
    pcall(function() Next = Obj:GetOwner() end)
    if IsValid(Next) and Next ~= Obj then return PlayerStateOf(Next, (Depth or 0) + 1) end
    return nil
end

local function OpenOfObject(Obj)
    if not IsValid(Obj) then return "", "none" end
    local PS = PlayerStateOf(Obj, 0)
    local Open = ""
    if PS then pcall(function() Open = PS.MyOpenId:ToString() end) end
    return Open, Short(Obj)
end

local function NoteDamage(Instigator, Target, Kind)
    local Victim = OpenOfObject(Target)
    if Victim == "" then return end
    local Open, Name = OpenOfObject(Instigator)
    GoProbe.Damage = GoProbe.Damage or {}
    local List = GoProbe.Damage[Victim] or {}
    List[#List + 1] = { Open = Open, Name = Name, At = os.time(), Kind = Kind }
    if #List > DAMAGE_KEEP then table.remove(List, 1) end
    GoProbe.Damage[Victim] = List
    if Kind == "death" then
        GoProbe.DeathBy = GoProbe.DeathBy or {}
        GoProbe.DeathBy[Victim] = { Open = Open, Name = Name }
    end
end

-- --- Wendigo treasury door probe (08.11, docs/WENDIGO_DOOR.md §4; temporary, for the stand) ---------------------
-- Complaint: after Wendigo dies the door does not move even on the client. The game's chain: spawn point (BP_ActorSpawnPoint,
-- ControlledMechanism = right leaf) -> monster (DeathTriggerMechanism) -> death with HasAuthority -> OnTriggered(1) on
-- the door -> bIsOn (replicated) + tick of UMechanismMoveNode. Host command: wendigo [kill|open]; after Wendigo dies
-- for 10 s once a second [WENDIGO] with the state of the leaves.
-- a global table: the main block of the file has a 200-local limit
Wendigo = { WatchUntil = 0, Pending = nil }

function Wendigo.Field(Obj, Name)
    local V = "?"
    pcall(function()
        local X = Obj[Name]
        if type(X) == "userdata" and X.IsValid then
            V = X:IsValid() and (FullName(X):match("([^%.]+)$") or "obj") or "nil"
        else
            V = tostring(X)
        end
    end)
    return V
end

function Wendigo.Doors()
    local Out = {}
    for _, Cls in ipairs({ "BP_Mechanism_StoneDoor_Right_C", "BP_Mechanism_StoneDoor_Left_C" }) do
        for _, D in ipairs(FindAllOf(Cls) or {}) do
            if IsValid(D) and not FullName(D):find("Default__", 1, true) then Out[#Out + 1] = D end
        end
    end
    return Out
end

function Wendigo.Dump(Tag)
    local Lines = {}
    for _, M in ipairs(FindAllOf("BP_Tgf_Monster_Wendigo_C") or {}) do
        if IsValid(M) and not FullName(M):find("Default__", 1, true) then
            local Arr = "?"
            pcall(function() Arr = tostring(#M.DeathTriggerMechanisms) end)
            local Dead = "?"
            pcall(function() Dead = tostring(M:IsDeadOrDying()) end)
            Lines[#Lines + 1] = string.format("monster %s: DeathTriggerMechanism=%s Param=%s Mechanisms#=%s dead=%s",
                FullName(M):match("([^%.]+)$") or "?", Wendigo.Field(M, "DeathTriggerMechanism"), Wendigo.Field(M, "DeathTriggerParam"), Arr, Dead)
        end
    end
    for _, P in ipairs(FindAllOf("BP_ActorSpawnPoint_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            local Mech = Wendigo.Field(P, "ControlledMechanism")
            if Mech ~= "nil" and Mech ~= "?" then
                Lines[#Lines + 1] = string.format("point %s: ControlledMechanism=%s TriggerParam=%s",
                    FullName(P):match("([^%.]+)$") or "?", Mech, Wendigo.Field(P, "TriggerParam"))
            end
        end
    end
    for _, D in ipairs(Wendigo.Doors()) do
        local L, Tick, MB, MLoc = "?", "?", "?", "?"
        pcall(function() local V = D:K2_GetActorLocation(); L = string.format("%.0f,%.0f,%.0f", V.X, V.Y, V.Z) end)
        pcall(function()
            local C = D.MoveBase
            MB = tostring(C.bIsOn)
            Tick = tostring(C:IsComponentTickEnabled())
            local V = C:K2_GetComponentLocation(); MLoc = string.format("%.0f,%.0f,%.0f", V.X, V.Y, V.Z)
        end)
        Lines[#Lines + 1] = string.format("leaf %s: bIsOn=%s MoveBase.bIsOn=%s tick=%s root=%s MoveBase=%s Dormancy=%s",
            FullName(D):match("([^%.]+)$") or "?", Wendigo.Field(D, "bIsOn"), MB, Tick, L, MLoc, Wendigo.Field(D, "NetDormancy"))
    end
    if #Lines == 0 then Lines[1] = "neither Wendigo nor door leaves (wrong map?)" end
    for _, Line in ipairs(Lines) do Log("[WENDIGO] " .. Tag .. ": " .. Line) end
    return #Lines
end

-- Insurance (08.11): the native chain does not open the door when the monster has no link (DeathTriggerMechanism = nil;
-- 08.11 14:57 reproduced on the stand too), and the spawn points nearby have no link either. Only if a player killed Wendigo
-- (user's decision): 2 s after the death, stone leaves within WENDIGO_DOOR_R of it, if still closed, get
-- OnTriggered(1), like the "wendigo open" command.
WENDIGO_DOOR_R = 8000 -- there is one stone door per floor (docs/SECRET_ROOM_BOSS.md); Wendigo may have moved away from it

function Wendigo.NearDoors(Loc)
    local Out = {}
    for _, D in ipairs(Wendigo.Doors()) do
        pcall(function()
            local L = D:K2_GetActorLocation()
            local Dist = math.sqrt((L.X - Loc.X) ^ 2 + (L.Y - Loc.Y) ^ 2 + (L.Z - Loc.Z) ^ 2)
            if Dist <= WENDIGO_DOOR_R then Out[#Out + 1] = { Door = D, Dist = Dist } end
        end)
    end
    return Out
end

function Wendigo.OnDeath(Target, Instigator)
    local Name = ""
    pcall(function() if IsValid(Target) then Name = FullName(Target) end end)
    if not Name:find("Wendigo", 1, true) then return end
    Wendigo.WatchUntil = os.time() + 3
    local Link = Wendigo.Field(Target, "DeathTriggerMechanism")
    local Killer, KillerName = "", "?"
    pcall(function() Killer, KillerName = OpenOfObject(Instigator) end)
    local Loc = nil
    pcall(function() Loc = Target:K2_GetActorLocation() end)
    Log(string.format("[WENDIGO] death at (%s): monster's link to the mechanism = %s, killer %s (%s)",
        Loc and string.format("%.0f %.0f %.0f", Loc.X, Loc.Y, Loc.Z) or "?", Link, Killer ~= "" and Killer or "not a player", tostring(KillerName)))
    Wendigo.Pending = (Loc and Killer ~= "") and { At = os.time() + 2, Loc = Loc, Link = Link, Killer = Killer } or nil
    if Killer == "" then Log("[WENDIGO] insurance not enabled: not killed by a player") end
    Wendigo.Dump("death")
end

function Wendigo.Tick()
    if os.time() <= Wendigo.WatchUntil then Wendigo.Dump("after death") end
    local Pd = Wendigo.Pending
    if not Pd or os.time() < Pd.At then return end
    Wendigo.Pending = nil
    local Near = Wendigo.NearDoors(Pd.Loc)
    if #Near == 0 then
        Log(string.format("[WENDIGO] insurance: no door leaves within %d cm of Wendigo", WENDIGO_DOOR_R))
        return
    end
    local Res = {}
    for _, N in ipairs(Near) do
        local On = false
        pcall(function() On = N.Door.bIsOn == true end)
        local DoorName = FullName(N.Door):match("([^%.]+)$") or "?"
        if On then
            Res[#Res + 1] = string.format("%s (%.0f cm) already open", DoorName, N.Dist)
        else
            local Ok, Err = pcall(function() N.Door:OnTriggered(1) end)
            Res[#Res + 1] = string.format("%s (%.0f cm) OnTriggered(1): %s", DoorName, N.Dist, Ok and "ok" or tostring(Err))
        end
    end
    Log(string.format("[WENDIGO] insurance (killed by %s, link %s): %s", Pd.Killer, Pd.Link, table.concat(Res, "; ")))
    Wendigo.WatchUntil = os.time() + 3
end

-- --- Pass quests (08.11): monsters, chests, damage to players ---------------------------------------------------
-- Backend does not see who killed whom and who opened what in a match — we count it here, per player, and every QUEST_FLUSH_SEC send
-- the increments POST /ds/quest_progress {open_id, monsters, chests, damage} (backend: KILL_TYPE target 11, LOOT 41,
-- PLAYER_CAUSED_TOTAL_DAMAGE 1; only while the player's raid is open). Source — the player's native counter
-- (QuestStats.Poll); the death hook is a fallback for monsters. Increments of the last seconds before leaving the raid may
-- be lost (backend answers 409 — raid closed).
-- Globals: the main chunk has a 200-local limit.
QUEST_FLUSH_SEC = 5
QuestStats = { Pending = {}, At = 0, Last = {}, CompSeen = {} }

function QuestStats.Add(Open, Key, N)
    if type(Open) ~= "string" or not Open:match("^%d+$") or not N or N <= 0 then return end
    local P = QuestStats.Pending[Open] or { monsters = 0, chests = 0, damage = 0, hookchests = 0, hookmonsters = 0 }
    P[Key] = (P[Key] or 0) + N
    QuestStats.Pending[Open] = P
end

-- death: fallback monster count (as with chests — the larger of the hook and the native counter counts); victim —
-- ETgfActorType 11–14 (monster, elite, boss, summon; docs/QUEST_HOOKS.md), without ActorType — any non-player
function QuestStats.OnDeath(Instigator, Target)
    local T = nil
    pcall(function() T = tonumber(Target.ActorType) end)
    if T then
        if T < 11 or T > 14 then return end
    elseif OpenOfObject(Target) ~= "" then
        return
    end
    local Killer = OpenOfObject(Instigator)
    QuestStats.Add(Killer, "hookmonsters", 1)
end

-- 10.11: on prod the component's PlayerState field is empty ("components 3; no players") — damage to players never
-- reached the quests. Player — by PlayerState, otherwise by the component owner (controller), as with chests and deaths.
-- Ok: open_id, source; not found: nil, reason for the log
function QuestStats.OpenOf(C)
    local PS, Open = nil, ""
    pcall(function() PS = C.PlayerState end)
    if IsValid(PS) then pcall(function() Open = PS.MyOpenId:ToString() end) end
    if Open:match("^%d+$") then return Open, "PlayerState" end
    local Owner = nil
    pcall(function() Owner = C:GetOwner() end)
    local ByOwner = OpenOfObject(Owner)
    if ByOwner:match("^%d+$") then return ByOwner, "owner" end
    return nil, string.format("owner %s, PlayerState %s, open_id '%s' / '%s'", IsValid(Owner) and Short(Owner) or "none",
        IsValid(PS) and "present" or "none", Open, ByOwner)
end

-- The player's native counter UTgfActorStatisticComponent (on the controller, server only; docs/QUEST_HOOKS.md, LOCAL analysis):
-- GetIntStat(1, 11) — killed monsters (all undead: elites, bosses, mimics, summons), GetIntStat(5, 41) — opened chests
-- (once per chest, to the first opener), GetFloatStat(7) — damage to enemy players. Values accumulate over the component's
-- lifetime: we send the increment; a new component or a value lower than before — counting from zero.
function QuestStats.Poll()
    local Found, Seen, Miss, Err = 0, {}, {}, nil
    for _, C in ipairs(FindAllOf("TgfActorStatisticComponent") or {}) do
        if IsValid(C) and not FullName(C):find("Default__", 1, true) then
            Found = Found + 1
            local Ok, E = pcall(function()
                local Open, Why = QuestStats.OpenOf(C)
                if not Open then
                    Miss[#Miss + 1] = Why
                    return
                end
                local Now = { comp = C:GetAddress(), monsters = tonumber(C:GetIntStat(1, 11)) or 0,
                    chests = tonumber(C:GetIntStat(5, 41)) or 0, damage = tonumber(C:GetFloatStat(7)) or 0 }
                local L = QuestStats.Last[Open]
                if not L or L.comp ~= Now.comp then L = { comp = Now.comp, monsters = 0, chests = 0, damage = 0 } end
                for _, K in ipairs({ "monsters", "chests", "damage" }) do
                    local D = Now[K] - L[K]
                    if D < 0 then D = Now[K] end
                    QuestStats.Add(Open, K, D)
                end
                QuestStats.Last[Open] = Now
                QuestStats.CompSeen[Open] = true
                Seen[#Seen + 1] = string.format("%s (%s) monsters %d chests %d damage %.0f", Open, Why, Now.monsters, Now.chests, Now.damage)
            end)
            if not Ok then Err = tostring(E) end
        end
    end
    -- diagnostics: once a minute and on every new map — whether the native counter was found and what is in it
    if QuestStats.DiagMap ~= HostedMap or os.time() - (QuestStats.DiagAt or 0) >= 60 then
        QuestStats.DiagMap, QuestStats.DiagAt = HostedMap, os.time()
        Log(string.format("[QUEST] native counter: components %d; %s%s%s", Found, #Seen > 0 and table.concat(Seen, "; ") or "no players",
            #Miss > 0 and (" | without a player: " .. table.concat(Miss, "; ")) or "", Err and (" | error: " .. Err:sub(1, 160)) or ""))
    end
end

-- Fallback chest count (08.11: the native counter gave 0): a chest on opening calls
-- TgfStatBlueprintFunctionLibrary.AddActorIntStat(pawn, 5, 41, 1) (docs/QUEST_HOOKS.md). The larger of the
-- hook and the counter increment counts — a chest is not counted twice.
function QuestStats.HookStat()
    if QuestStats.StatHooked then return end
    QuestStats.StatHooked = true
    local Ok, E = pcall(function()
        RegisterHook("/Script/LyraGame.TgfStatBlueprintFunctionLibrary:AddActorIntStat", function(Ctx, Actor, Type, TypeId, Value)
            pcall(function()
                if Type:get() ~= 5 or TypeId:get() ~= 41 then return end
                local Pawn = Actor:get()
                if not IsValid(Pawn) then return end
                local Open = OpenOfObject(Pawn)
                QuestStats.Add(Open, "hookchests", tonumber(Value:get()) or 1)
                Log("[QUEST] chest opened (AddActorIntStat): " .. tostring(Open))
            end)
        end)
    end)
    Log("[QUEST] hook AddActorIntStat: " .. (Ok and "ok" or tostring(E)))
end

function QuestStats.Flush(Force)
    if not Force and os.time() - QuestStats.At < QUEST_FLUSH_SEC then return end
    QuestStats.At = os.time()
    QuestStats.HookStat()
    QuestStats.Poll()
    if next(QuestStats.Pending) == nil then return end
    local Base = BackendBase()
    if not Base then return end
    local Pending = QuestStats.Pending
    QuestStats.Pending = {}
    for Open, P in pairs(Pending) do
        P.chests = math.max(P.chests or 0, P.hookchests or 0)
        P.monsters = math.max(P.monsters or 0, P.hookmonsters or 0)
        local Body = string.format('{"open_id":"%s","monsters":%d,"chests":%d,"damage":%d}', Open, P.monsters or 0, P.chests or 0,
            math.floor(P.damage or 0))
        local Rsp = NativeCall("http", { url = Base .. "/ds/quest_progress", body = Body }) or ""
        Log(string.format("[QUEST] %s: monsters %d, chests %d, damage %d -> %s %s", Open, P.monsters or 0, P.chests or 0,
            math.floor(P.damage or 0), Rsp:match("http=(%d+)") or ("none: " .. (Rsp:match("err=([^\n]*)") or "?")),
            (Rsp:match("body=([^\n]*)") or ""):sub(1, 80)))
    end
end

local function HookDamage()
    if DamageHooked then return end
    DamageHooked = true
    local Res = {}
    for Fn, Kind in pairs({ MulticastDeathEvent = "death", ClientDamageDirectionEvent = "hit" }) do
        local Ok = pcall(function()
            RegisterHook("/Script/LyraGame.TgfAttributeComponent:" .. Fn, function(Self, A, B, C, D)
                pcall(function()
                    if Kind == "death" then NoteDamage(A:get(), B:get(), Kind); Wendigo.OnDeath(B:get(), A:get()); QuestStats.OnDeath(A:get(), B:get())
                    else NoteDamage(C:get(), D:get(), Kind) end
                end)
            end)
        end)
        Res[#Res + 1] = Fn .. "=" .. tostring(Ok)
    end
    Log("[KILL] damage hooks: " .. table.concat(Res, " "))
end

local function RelationEntries(PS)
    local Out = {}
    pcall(function()
        PS.RelationIntStatistics.RelationIntStatistics:ForEach(function(_, E)
            local Entry = E:get()
            local T, O, V = 0, "", 0
            pcall(function() T = tonumber(Entry.Type) or 0 end)
            pcall(function() O = Entry.OpenID:ToString() end)
            pcall(function() V = tonumber(Entry.Value) or 0 end)
            Out[#Out + 1] = { Type = T, Open = O, Value = V }
        end)
    end)
    return Out
end

local function PlayerStates()
    local Out = {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open = ""
            pcall(function() Open = PS.MyOpenId:ToString() end)
            Out[#Out + 1] = { PS = PS, Open = Open }
        end
    end
    return Out
end

ReportKill = function(Victim)
    local Killer, From, Seen = nil, nil, {}
    local Death = (GoProbe.DeathBy or {})[Victim]
    if Death and Death.Open ~= "" then Killer, From = Death.Open, "DeathEvent" end
    for _, P in ipairs(PlayerStates()) do
        for _, E in ipairs(RelationEntries(P.PS)) do
            if E.Value > 0 and E.Type ~= 0 then Seen[#Seen + 1] = string.format("%s:%d:%s", P.Open, E.Type, E.Open) end
            if not Killer and P.Open == Victim and E.Type == 4 and E.Value > 0 then Killer, From = E.Open, "stats" end
            if not Killer and E.Type == 3 and E.Open == Victim and E.Value > 0 and P.Open:match("^%d+$") then
                Killer, From = P.Open, "stats"
            end
        end
    end
    local Hits = {}
    for _, H in ipairs((GoProbe.Damage or {})[Victim] or {}) do
        Hits[#Hits + 1] = string.format("%s %s(%s) -%ds", H.Kind, H.Open ~= "" and H.Open or "-", H.Name, os.time() - H.At)
    end
    if not Killer then
        local List = (GoProbe.Damage or {})[Victim] or {}
        for I = #List, 1, -1 do
            if List[I].Open:match("^%d+$") and List[I].Open ~= Victim then Killer, From = List[I].Open, "last hit"; break end
        end
    end
    local IsPlayer = Killer and Killer:match("^%d+$") and Killer ~= Victim
    Log(string.format("[KILL] %s died, killer: %s%s%s | death: %s | hits: %s | stats: %s", Victim,
        Killer or "not a player", From and (" (" .. From .. ")") or "", IsPlayer and " — sending /ds/kill" or "",
        Death and ((Death.Open ~= "" and Death.Open or "-") .. "(" .. Death.Name .. ")") or "none",
        #Hits > 0 and table.concat(Hits, ", ") or "none", #Seen > 0 and table.concat(Seen, " ") or "empty"))
    if not IsPlayer then return end
    local Base = BackendBase()
    if not Base then return end
    local Session = (NativeCall("session", {}) or ""):match("session=([^\n]*)") or ""
    local Body = string.format('{"killer":"%s","victim":"%s","battle_id":"%s","session":"%s"}', Killer, Victim,
        Host and Host.Battle or "", Session)
    local Rsp = NativeCall("http", { url = Base .. "/ds/kill", body = Body }) or ""
    Log("[KILL] POST /ds/kill -> " .. (Rsp:match("http=(%d+)") or ("none: " .. (Rsp:match("err=([^\n]*)") or "?"))) ..
        " " .. (Rsp:match("body=([^\n]*)") or ""):sub(1, 120))
end

-- 03.11 (live test): the host observer must not be anyone's teammate nor be visible. The client mod assigns players
-- teams 1, 2, … in order, while the host's PlayerState has 1 by default — the first player ended up in a team with the host (party
-- HUD). Every HOST_FIX_SEC: host team HOST_TEAM (dbnative setteam), the pawn hidden and without collision (the native BP of the hall
-- and map reload may restore visibility; destroying the pawn is not allowed — it breaks mob spawning, 01.11).
local HOST_TEAM = 254
local HOST_FIX_SEC = 5
local HostFixAt, HostFixLast = 0, nil
local function EnsureHost()
    if not UseObserver or os.time() - HostFixAt < HOST_FIX_SEC then return end
    HostFixAt = os.time()
    local PC = nil
    for _, C in ipairs(FindAllOf("BP_TgfPlayerController_C") or {}) do
        local IsLocal = false
        pcall(function() IsLocal = C:IsLocalController() end)
        if IsValid(C) and IsLocal and not FullName(C):find("Default__", 1, true) then PC = C; break end
    end
    if not PC then return end
    local PS, Pawn = nil, nil
    pcall(function() PS = PC.PlayerState end)
    pcall(function() Pawn = PC.Pawn end)
    local Done = {}
    -- 06.11: the zone measures distance only in the plane — the lifted host pawn took its damage and died (08:29:58, a false
    -- death in the kill feed, LOCAL record 3/2). Update Damage skips the Shadow scene marker (=1), RuClient sets 0 — repeat it.
    if IsValid(PS) then
        pcall(function() if tostring(PS:GetPlayerSceneTag()) ~= "1" then PS:SetPlayerSceneTag(1); Done[#Done + 1] = "scene=Shadow" end end)
    end
    if IsValid(PS) then
        local Team = -1
        pcall(function() Team = PS:GetTeamId() end)
        if Team ~= HOST_TEAM then
            local Rsp = NativeCall("setteam", { ps = string.format("0x%X", PS:GetAddress()), team = HOST_TEAM }) or ""
            Done[#Done + 1] = string.format("team %s -> %d ok=%s", tostring(Team), HOST_TEAM, Rsp:match("ok=(%d)") or "?")
        end
    end
    if IsValid(Pawn) then
        local Hidden = true
        pcall(function() Hidden = Pawn.bHidden end)
        if not Hidden then
            pcall(function() Pawn:SetActorHiddenInGame(true) end)
            pcall(function() Pawn:SetActorEnableCollision(false) end)
            Done[#Done + 1] = "pawn hidden again"
        end
    end
    local Line = table.concat(Done, ", ")
    if Line ~= "" and Line ~= HostFixLast then Log("[OBSERVER] host: " .. Line) end
    HostFixLast = Line
end

-- Late observer (dbserver-observer.txt with the word late): LATE_OBSERVER_SEC after the first player appears on
-- this map — the steps from the file (e.g. spec destroy). Every 30 s — the number of characters, to see whether mobs stopped
-- spawning (01.11: removing the host's pawn before players joined broke room population).
local CountAt = 0
local function LateObserverTick()
    if os.time() - CountAt >= 30 then
        CountAt = os.time()
        local All, Heroes = #(FindAllOf("LyraCharacter") or {}), #(FindAllOf("BP_Fighter_HeroSample_C") or {})
        Log(string.format("[OBSERVER] characters LyraCharacter=%d, heroes=%d, players on the server %d", All, Heroes,
            #RemoteControllers()))
    end
    if not LateObserver then return end
    if #RemoteControllers() == 0 then LateObserver.Since = nil; return end
    LateObserver.Since = LateObserver.Since or os.time()
    if os.time() - LateObserver.Since < LATE_OBSERVER_SEC then return end
    local Steps = LateObserver.Steps
    LateObserver = nil
    Log("[OBSERVER] late observer (player on the server " .. LATE_OBSERVER_SEC .. " s): " .. MakeObserver(Steps))
end

-- 03.11 (live test 4): the host can still be spectated although its pawn is not replicated — the list of spectate targets
-- (UTgfSpectatorManagerComponent +0xa8, native TrySpectate 0x142eda880) is not built from the pawns visible to the client.
-- The server watches target changes: ServerChangeViewTarget (client -> server) and SetCurrentViewTarget (server -> all);
-- if the target is the host's pawn/controller/PlayerState, immediately the native TrySpectate(false) on this component (next target).
local Spect = { Hooked = false, SkipAt = {}, ZoneAt = 0 } -- one table: the main Lua block has a 200-local limit
function Spect.IsHost(Obj)
    if not IsValid(Obj) then return false end
    local PS = nil
    if FullName(Obj):find("PlayerState", 1, true) then PS = Obj else pcall(function() PS = Obj.PlayerState end) end
    local Open = ""
    if IsValid(PS) then pcall(function() Open = PS.MyOpenId:ToString() end) end
    if Open == "LOCAL_GAME_MODE_OPENID" then return true end
    local HostPC = LocalPC()
    local HostPawn = nil
    pcall(function() HostPawn = HostPC.Pawn end)
    return (IsValid(HostPawn) and FullName(Obj) == FullName(HostPawn)) or (IsValid(HostPC) and FullName(Obj) == FullName(HostPC))
end

function Spect.Skip(Comp, Target, Via)
    if not Spect.IsHost(Target) then return end
    local Key = FullName(Comp)
    if Spect.SkipAt[Key] == os.time() then return end -- do not loop if there are no other targets
    Spect.SkipAt[Key] = os.time()
    ExecuteInGameThread(function()
        local Ok = pcall(function() Comp:TrySpectate(false) end)
        Log(string.format("[OBSERVER] spectating the host (%s, %s) — next target ok=%s", Via, Short(Target), tostring(Ok)))
    end)
end

function Spect.Hook()
    if Spect.Hooked then return end
    Spect.Hooked = true
    local Base = "/Script/LyraGame.TgfSpectatorManagerComponent:"
    local Res = {}
    for _, Fn in ipairs({ "ServerChangeViewTarget", "SetCurrentViewTarget" }) do
        local Ok = pcall(function()
            RegisterHook(Base .. Fn, function(Self, Target)
                pcall(function() Spect.Skip(Self:get(), Target:get(), Fn) end)
            end)
        end)
        Res[#Res + 1] = Fn .. "=" .. tostring(Ok)
    end
    Log("[OBSERVER] spectate hooks: " .. table.concat(Res, " "))
end

-- Death zone diagnostics (live test 4: "the zone deals no damage"): BP_DeathZone damage goes via the Update Damage loop only
-- in the active phase TGF.GamePhase.Playing; for a PlayerState without a hero the loop decrements AliveFighterNumber
function Spect.ZoneTick()
    if os.time() - Spect.ZoneAt < 30 then return end
    Spect.ZoneAt = os.time()
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone then return end
    local V = {}
    for _, F in ipairs({ "State", "Current Disc Index", "Dynamic Radius", "AliveFighterNumber" }) do
        local Ok, X = pcall(function() return Zone[F] end)
        V[#V + 1] = F .. "=" .. (Ok and tostring(X) or "?")
    end
    local Playing = "?"
    pcall(function()
        local Sub = FindLive("LyraGamePhaseSubsystem")
        Playing = tostring(Sub:IsPhaseActive({ TagName = FName("TGF.GamePhase.Playing") }))
    end)
    Log("[ZONE] " .. table.concat(V, " ") .. " Playing=" .. Playing)
end

-- 06.11 PVPVE diagnostics (read only, does not change behaviour): floors/portals, options, spawn rooms, zone damage.
-- Lines [DIAG], [ZONEDMG], [MOBFALL]. Host commands: diag | zonetest [radius] | spawnrooms (check "all mobs at once").
-- IsValid first: GetFName() on a null object (an empty ActorToSpawn of a point) is a native UE4SS crash, AV at address 0x18
-- (06.11 07:58 and 08:01), pcall does not catch it. An element of a class array may be a wrapper — then :get().
function Spect.ObjName(E)
    if E == nil then return "None" end
    local Obj = E
    local Ok, Valid = pcall(function() return Obj:IsValid() end)
    if not Ok then
        if not pcall(function() Obj = E:get() end) or Obj == nil then return "?" end
        Ok, Valid = pcall(function() return Obj:IsValid() end)
    end
    if not (Ok and Valid) then return "None" end
    local N = "?"
    pcall(function() N = Obj:GetFName():ToString() end)
    return N
end

-- a long list — in several lines: Log truncates at 300 characters
function Spect.LogList(Prefix, Items)
    local Line = ""
    for _, S in ipairs(Items) do
        if #Line + #S > 240 then Log(Prefix .. Line); Line = "" end
        -- one entry longer than a line — in parts (UTF-8 characters do not occur in class names)
        while #S > 240 do
            Log(Prefix .. S:sub(1, 240))
            S = "… " .. S:sub(241)
        end
        Line = Line .. (Line == "" and "" or ", ") .. S
    end
    Log(Prefix .. (Line == "" and "-" or Line))
end

function Spect.DiagFloor()
    local Zone, Ds, Gs = FindLive("BP_DeathZone_C"), FindLive("BattleDsUtility"), FindLive("LyraGameState")
    local function Get(F)
        local Ok, V = pcall(F)
        return Ok and tostring(V) or "error"
    end
    Log(string.format("[DIAG] floor: map=%s scenes=%d HasNextFloor=%s ForceSpawnRedPortal=%s IsOptionArgsInit=%s IsRoomReady=%s",
        tostring(HostedMap), Host and #Host.Scenes or 0, Get(function() return Ds:HasNextFloor() end),
        Get(function() return Zone.ForceSpawnRedPortal end), Get(function() return Ds:IsOptionArgsInit() end),
        Get(function() return Ds:IsRoomReady() end)))
    local Ids, Args = {}, {}
    pcall(function() for I = 1, #Gs.OptionIds do Ids[#Ids + 1] = tostring(Gs.OptionIds[I]) end end)
    pcall(function()
        local A = Ds:OptionArgs()
        for I = 1, #A do Args[#Args + 1] = tostring(A[I]) end
    end)
    Log(string.format("[DIAG] options GameState=[%s] native OptionArgs=[%s] MapId=%s", table.concat(Ids, ","), table.concat(Args, ","),
        Get(function() return Gs.MapId end)))
    if not Zone then Log("[DIAG] no death zone on the map"); return end
    Log(string.format("[DIAG] portal points on the map: %s, scrolls: %s", Get(function() return #Zone.PotentialPortalPlacements end), Get(function() return #Zone.PotentialTeleportScrollPlacements end)))
    pcall(function()
        for I = 1, #Zone.Discs do
            local D, Ge = Zone.Discs[I], {}
            pcall(function() for J = 1, #D.DamageGEs do Ge[#Ge + 1] = Spect.ObjName(D.DamageGEs[J]) end end)
            Log(string.format("[DIAG] disk %d: R=%.0f outer=%.0f inner=%.0f pause=%.0f interval=%.0f dur=%.0f MaxZoneTime=%.0f portals=%d GE=[%s]", I, D.Radius, D.OuterRadius, D.InnerRadius, D.Pause,
                D.Interval, D.Duration, D.MaxDeathZoneTime, #D.PortalSpawnSettings, table.concat(Ge, ",")))
        end
    end)
    -- Portal counters — from the subsystem (nested struct arrays with FTransform are not read from Lua: risk of a UE4SS crash).
    Spect.DiagPortals()
    -- Portal settings of the disks: time, chance, type by map options (LevelArgs; bDefault — if the options did not match)
    pcall(function()
        for I = 1, #Zone.Discs do
            local Set = Zone.Discs[I].PortalSpawnSettings
            local Items = {}
            for J = 1, #Set do
                local E, Sel = Set[J], {}
                pcall(function()
                    for K = 1, #E.PortalTypeSelections do
                        local X, Args, Probs = E.PortalTypeSelections[K], {}, {}
                        pcall(function() for A = 1, #X.LevelArgs do Args[#Args + 1] = tostring(X.LevelArgs[A]) end end)
                        pcall(function()
                            for A = 1, #X.PortalTypeProbabilities do
                                local Pr = X.PortalTypeProbabilities[A]
                                Probs[#Probs + 1] = string.format("%s=%.2f", (Spect.ObjName(Pr.PortalClass):gsub("BP_", ""):gsub("_C$", "")), Pr.Probability)
                            end
                        end)
                        Sel[#Sel + 1] = string.format("%s[%s]{%s}", X.bDefault and "D" or "", table.concat(Args, ","), table.concat(Probs, ","))
                    end
                end)
                Items[#Items + 1] = string.format("t=%.0f p=%.2f %s", E.SpawnTime, E.SpawnProbability, table.concat(Sel, " "))
            end
            if #Items > 0 then Spect.LogList(string.format("[DIAG] disk %d portal settings: ", I), Items) end
        end
    end)
end

function Spect.DiagPortals()
    local Mgr = FindLive("TgfPortalManagerSubsystem")
    if not Mgr then Log("[DIAG] portals: no TgfPortalManagerSubsystem"); return end
    local Items = {}
    for T, Name in ipairs({ "BlueDevice", "RedDevice", "ShadowDevice", "GreyDevice", "BluePortal", "RedPortal", "ShadowPortal", "GreyPortal", "FixedBlue", "FixedRed" }) do
        local Ok, N = pcall(function() return Mgr:GetAvailablePortalNums(T) end)
        Items[#Items + 1] = Name .. "=" .. (Ok and tostring(N) or "?")
    end
    Log("[DIAG] portals available: " .. table.concat(Items, " "))
end

-- Rooms/sets/points: how they spawn (by phase or by proximity), who is in them, how many the map options cut off
function Spect.DiagRooms(Tag)
    local Gs = FindLive("LyraGameState")
    local Have = {}
    pcall(function() for I = 1, #Gs.OptionIds do Have[tonumber(tostring(Gs.OptionIds[I])) or 0] = true end end)
    local Rooms = FindAllOf("BP_ActorSpawnRoom_C") or {}
    local Denied, DenyCls = {}, {}
    local Kind, S = {}, { Rooms = 0, ByPhase = 0, ByDist = 0, Fresh = 0, TickOn = 0, Sets = 0, Points = 0, Thr = {}, Opt = {}, Deny = 0 }
    local function Options(Comp)
        local Ok, Key, Pass = pcall(function()
            local Ids = {}
            local All = true
            for I = 1, #Comp.Options do
                local V = tonumber(tostring(Comp.Options[I])) or 0
                Ids[#Ids + 1] = tostring(V)
                if not Have[V] then All = false end
            end
            if #Ids == 0 then return "", true end
            local Act = tonumber(tostring(Comp.Action)) or 0
            return string.format("%s[%s]", Act == 0 and "need" or "except", table.concat(Ids, ",")), (Act == 0) == All
        end)
        if not Ok or Key == "" then return true end
        local E = S.Opt[Key] or { N = 0, Pass = Pass }
        E.N = E.N + 1
        S.Opt[Key] = E
        if not Pass then S.Deny = S.Deny + 1 end
        return Pass
    end
    for _, R in ipairs(Rooms) do
        if IsValid(R) and not FullName(R):find("Default__", 1, true) then
            S.Rooms = S.Rooms + 1
            local Ph = false
            pcall(function() Ph = R.bSpawnWhenPhaseStarts end)
            Kind[R:GetAddress()] = Ph and "phase" or "dist"
            if Ph then S.ByPhase = S.ByPhase + 1 else S.ByDist = S.ByDist + 1 end
            pcall(function() if R.bFirstTimeActuallySpawn then S.Fresh = S.Fresh + 1 end end)
            pcall(function() if R.PlayerDistanceChecker:IsComponentTickEnabled() then S.TickOn = S.TickOn + 1 end end)
            pcall(function()
                local T = string.format("%.0f", R.DistanceThreshold)
                S.Thr[T] = (S.Thr[T] or 0) + 1
            end)
            pcall(function() if not Options(R.ActorSpawnOptionComponent) then Denied[R:GetAddress()] = true end end)
        end
    end
    local SetKind = {}
    for _, V in ipairs(FindAllOf("BP_ActorSpawnSet_C") or {}) do
        if IsValid(V) and not FullName(V):find("Default__", 1, true) then
            S.Sets = S.Sets + 1
            pcall(function() SetKind[V:GetAddress()] = Kind[V:GetAttachParentActor():GetAddress()] or "no room" end)
            pcall(function()
                local Own = not Options(V.ActorSpawnOptionComponent)
                if Own or Denied[V:GetAttachParentActor():GetAddress()] then Denied[V:GetAddress()] = true end
            end)
        end
    end
    local Cls = { phase = {}, dist = {}, ["no room"] = {} }
    for _, P in ipairs(FindAllOf("BP_ActorSpawnPoint_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            S.Points = S.Points + 1
            local K = "no room"
            pcall(function() K = SetKind[P:GetAttachParentActor():GetAddress()] or Kind[P:GetAttachParentActor():GetAddress()] or K end)
            local N = "?"
            pcall(function() N = Spect.ObjName(P.ActorToSpawn) end)
            if N == "?" or N == "None" then pcall(function() N = "actor:" .. Spect.ObjName(P.DataActor) end) end
            Cls[K][N] = (Cls[K][N] or 0) + 1
            pcall(function()
                local Own = not Options(P.ActorSpawnOptionComponent)
                if Own or Denied[P:GetAttachParentActor():GetAddress()] then DenyCls[N] = (DenyCls[N] or 0) + 1 end
            end)
        end
    end
    Log(string.format("[DIAG] %s rooms=%d by_phase=%d by_proximity=%d not_spawned_yet=%d proximity_check_ticking=%d sets=%d points=%d cut_by_options=%d",
        Tag, S.Rooms, S.ByPhase, S.ByDist, S.Fresh, S.TickOn, S.Sets, S.Points, S.Deny))
    local function Top(Map, N)
        local A = {}
        for K, V in pairs(Map) do A[#A + 1] = { K, V } end
        table.sort(A, function(X, Y) return X[2] > Y[2] end)
        local Out = {}
        for I = 1, math.min(N, #A) do Out[I] = A[I][1] .. "×" .. A[I][2] end
        return Out
    end
    if Tag == "start" then
        Spect.LogList("[DIAG] proximity threshold: ", Top(S.Thr, 12))
        local Opt = {}
        for K, E in pairs(S.Opt) do Opt[#Opt + 1] = string.format("%s×%d=%s", K, E.N, E.Pass and "yes" or "NO") end
        table.sort(Opt)
        Spect.LogList("[DIAG] set options: ", Opt)
        for K, M in pairs(Cls) do Spect.LogList("[DIAG] points (" .. K .. "): ", Top(M, 24)) end
        Spect.LogList("[DIAG] points behind cut-off options: ", Top(DenyCls, 24))
    end
end

-- Where the exits on the map come from: spawn points with exits/bosses (and what triggers them) and devices already standing
function Spect.DiagExits()
    local Items = {}
    local function Wanted(N)
        return N:find("Extract", 1, true) or N:find("Escape", 1, true) or N:find("Portal", 1, true) or N:find("Reaper", 1, true) or N:find("Khazra", 1, true)
    end
    for _, P in ipairs(FindAllOf("BP_ActorSpawnPoint_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            local N = "?"
            pcall(function() N = Spect.ObjName(P.ActorToSpawn) end)
            if N == "?" or N == "None" then pcall(function() N = Spect.ObjName(P.DataActor) end) end
            if Wanted(N) then
                local L, Par, Kind, Fresh, Mech, Got, Phase = { X = 0, Y = 0, Z = 0 }, "none", "-", "-", "?", "?", "?"
                pcall(function() Got = tostring(#P.SpawnedActors) end)
                pcall(function() L = P:K2_GetActorLocation() end)
                pcall(function()
                    local Up = P:GetAttachParentActor()
                    if IsValid(Up) then
                        Par = Spect.ObjName(Up:GetClass())
                        local Room = Up
                        if Par:find("Set", 1, true) then
                            local Top = Up:GetAttachParentActor()
                            if IsValid(Top) then Room = Top; Par = Par .. ">" .. Spect.ObjName(Top:GetClass()) end
                        end
                        pcall(function() Kind = tostring(Room.bSpawnWhenPhaseStarts) end)
                        pcall(function() Fresh = tostring(Room.bFirstTimeActuallySpawn) end)
                        pcall(function() Phase = Room.SpawnPhaseTag.TagName:ToString() end)
                    end
                end)
                pcall(function() Mech = tostring(#P.DeathTriggerMechanisms) end)
                Items[#Items + 1] = string.format("%s (%.0f %.0f %.0f) parent=%s byPhase=%s phase=%s notSpawned=%s placed=%s mech=%s", N, L.X, L.Y, L.Z, Par, Kind, Phase, Fresh, Got, Mech)
            end
        end
    end
    Log(string.format("[EXITS] points with exits and bosses: %d", #Items))
    for _, S in ipairs(Items) do Log("[EXITS] " .. S) end
    for _, Cls in ipairs({ "BP_ExtractDevice_Base_C", "BP_EscapeDeviceBase_C", "BP_EscapeDevice_Blue_01_C", "BP_EscapeDevice_Red_01_C", "BP_ExtractPortal_Base_C", "BP_TGF_Monster_Reaper_C" }) do
        local List = {}
        for _, A in ipairs(FindAllOf(Cls) or {}) do
            if IsValid(A) and not FullName(A):find("Default__", 1, true) then
                pcall(function()
                    local L = A:K2_GetActorLocation()
                    local Act = "?"
                    pcall(function() Act = tostring(A:IsDeviceActive()) end)
                    List[#List + 1] = string.format("%s (%.0f %.0f %.0f) active=%s", Spect.ObjName(A:GetClass()), L.X, L.Y, L.Z, Act)
                end)
            end
        end
        Spect.LogList("[EXITS] standing " .. Cls .. ": ", List)
    end
end

function Spect.DiagTick()
    if not (Host and Host.Phase == "battle") then return end
    local D = Spect.Diag
    if not D or D.Map ~= HostedMap or D.Battle ~= Host.Battle then
        D = { Map = HostedMap, Battle = Host.Battle, At = os.time(), Step = 0 }
        Spect.Diag = D
    end
    if D.Step == 0 and os.time() - D.At >= 15 then
        D.Step = 1
        Spect.DiagFloor()
        Spect.DiagRooms("start")
        Spect.DiagExits()
    elseif D.Step == 1 and os.time() - D.At >= 120 then
        D.Step = 2
        Spect.DiagRooms("+2 min")
        Spect.DiagFloor()
        Spect.DiagExits()
    end
end

-- 06.11 next floor. Native check: everyone outside the hall is gone -> EndCurrentLevel -> HasNextFloor (native, on listen always
-- false, hence GameOver) -> TravelToNextFloor -> OnMapFinish (on listen does no transition). Override: HasNextFloor=true if
-- the task has a scene after the current one; on TravelToNextFloor — a snapshot of everyone's items (dbnative gofloor -> /ds/floor_snapshot,
-- otherwise on the new map the player gets the items from the profile, without loot) and servertravel, like from the hall into the match.
function Spect.HasFloor()
    return Host ~= nil and Host.Scenes[3] ~= nil and HostedMap == Host.Scenes[2]
end

function Spect.FloorTravel(Why, Target)
    if not Target and not Spect.HasFloor() then
        Log("[FLOOR] no next scene: map " .. tostring(HostedMap))
        return "no next scene"
    end
    local Mgr, Url = FindLive("BattleDsUtility"), ResultUrl()
    Url = Url and Url:gsub("/ds/player_result$", "/ds/floor_snapshot")
    local Done, Failed = 0, 0
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, InRoom = "", false
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() InRoom = PS:GetIsTeleportToWaitingRoom() end)
            if Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" and (InRoom or Why == "command") and Mgr and Url then
                local Rsp = NativeCall("gofloor", { mgr = string.format("0x%X", Mgr:GetAddress()), ps = string.format("0x%X", PS:GetAddress()), key = Open, url = Url }) or ""
                Log(string.format("[FLOOR] snapshot %s: %s", Open, (Rsp:gsub("\n", " "))))
                if Rsp:find("ok=1", 1, true) and Rsp:find("http=200", 1, true) then Done = Done + 1 else Failed = Failed + 1 end
            end
        end
    end
    local To = Target or Host.Scenes[3]
    Log(string.format("[FLOOR] transition (%s) -> %s, snapshots %d, errors %d", tostring(Why), To, Done, Failed))
    return TravelTo(To, "floor")
end

-- 06.11 second floor: the zone of its map has neither damage nor portals (all disks: GE=[], portals=0). User's decision: rules
-- as on a regular map. While the player is on the first floor, we remember the rules of its zone as plain data (class paths); the floor's
-- disk arrays are not touched. Damage — our GEs in the native hooks Apply/RemoveCurrentDamageGEs (when the player enters the zone and on a
-- disk change), portals — on a disk change, with the same disk number, the type — by map options, like the native choice.
function Spect.ClassPath(C)
    local Path = nil
    pcall(function() Path = FullName(C:IsValid() and C or C:get()):match("(/[^%s]+)$") end)
    if not Path then pcall(function() Path = FullName(C:get()):match("(/[^%s]+)$") end) end
    return Path
end

function Spect.LoadClass(Path)
    local C = nil
    pcall(function() C = StaticFindObject(Path) end)
    if not (C and C:IsValid()) then
        local Pkg = Path:match("^(.-)%.[^%.]+$")
        local Asset = Pkg and Pkg:match("([^/]+)$")
        for _, Name in ipairs({ Pkg or "", (Pkg or "") .. "." .. (Asset or "") }) do
            pcall(function() LoadAsset(Name) end)
            pcall(function() C = StaticFindObject(Path) end)
            if C and C:IsValid() then break end
        end
    end
    if not (C and C:IsValid()) then Log("[FLOOR] class not loaded: " .. tostring(Path)); return nil end
    return C
end

function Spect.FloorCapture()
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone then return end
    local Have = {}
    local Gs = FindLive("LyraGameState")
    pcall(function() for I = 1, #Gs.OptionIds do Have[tonumber(tostring(Gs.OptionIds[I])) or 0] = true end end)
    if next(Have) == nil then return end
    local R = { Battle = Host and Host.Battle or "", Ge = nil, GeLast = nil, Portals = {}, Total = 0 }
    for I = 1, #Zone.Discs do
        local D = Zone.Discs[I]
        pcall(function()
            if #D.DamageGEs > 0 then
                local Path = Spect.ClassPath(D.DamageGEs[1])
                if I == #Zone.Discs then R.GeLast = Path elseif not R.Ge then R.Ge = Path end
            end
        end)
        local List = {}
        for J = 1, #D.PortalSpawnSettings do
            local E, Chosen, Default = D.PortalSpawnSettings[J], nil, nil
            for K = 1, #E.PortalTypeSelections do
                local X = E.PortalTypeSelections[K]
                local All = #X.LevelArgs > 0
                for A = 1, #X.LevelArgs do
                    if not Have[tonumber(tostring(X.LevelArgs[A])) or 0] then All = false end
                end
                if All then Chosen = X end
                if X.bDefault then Default = X end
            end
            local Sel = Chosen or Default
            if Sel then
                local Probs = {}
                for A = 1, #Sel.PortalTypeProbabilities do
                    local Pr = Sel.PortalTypeProbabilities[A]
                    local PPath = Spect.ClassPath(Pr.PortalClass)
                    -- user's decision 06.11: on the second floor only blue ones (no grey, no purple); purple exists only on the first
                    if PPath and not PPath:find("Grey", 1, true) and not PPath:find("Shadow", 1, true) then
                        Probs[#Probs + 1] = { Path = PPath, P = Pr.Probability }
                    end
                end
                if #Probs > 0 then
                    List[#List + 1] = { Time = E.SpawnTime, Chance = E.SpawnProbability, Probs = Probs }
                    R.Total = R.Total + 1
                end
            end
        end
        R.Portals[I] = List
    end
    Spect.Rules = R
    Spect.FloorGeLoaded, Spect.FloorQueue = false, nil
    Log(string.format("[FLOOR] first floor rules: damage %s, last disk %s, portals %d (map options applied, grey and purple excluded)", tostring(R.Ge), tostring(R.GeLast), R.Total))
end

function Spect.OnFloor2()
    return Host ~= nil and Host.Scenes[3] ~= nil and HostedMap == Host.Scenes[3] and Spect.Rules ~= nil and Spect.Rules.Battle == Host.Battle
end

function Spect.FloorGe(Zone)
    local R = Spect.Rules
    local Last = false
    pcall(function() Last = Zone["Current Disc Index"] >= #Zone.Discs - 1 end)
    return (Last and R.GeLast) or R.Ge
end

function Spect.FloorApply(Zone, Player)
    if not Spect.OnFloor2() or #Zone.CurrentDamageGEs > 0 then return end
    local Path = Spect.FloorGe(Zone)
    local Cls = Path and Spect.LoadClass(Path)
    if not Cls then Log("[FLOOR] no GE class " .. tostring(Path)); return end
    local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
    local Target = Lib:GetAbilitySystemComponent(Player)
    local Src = Lib:GetAbilitySystemComponent(FindLive("LyraGameState"))
    Src:BP_ApplyGameplayEffectToTarget(Cls, Target, 0, Src:MakeEffectContext())
    Log(string.format("[FLOOR] zone damage: %s -> %s, active %s", Path:match("([^%.]+)$"), Spect.ObjName(Player), tostring(Target:GetGameplayEffectCount(Cls, nil, false))))
end

function Spect.FloorRemove(Player)
    if not Spect.OnFloor2() then return end
    local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
    local Target = Lib:GetAbilitySystemComponent(Player)
    for _, Path in ipairs({ Spect.Rules.Ge or "", Spect.Rules.GeLast or "" }) do
        local Cls = Path ~= "" and Spect.LoadClass(Path)
        if Cls then Target:RemoveActiveGameplayEffectBySourceEffect(Cls, nil, -1) end
    end
end

-- Native placement choice (docs/ghidra_portals): a PotentialPortalPlacements point inside the current circle, not closer than Inner to the center and not
-- occupied. We do not call the native PickRandomPlacementsWithinRange: its occupancy flags are local to Generate, not available outside, so occupancy is
-- live devices and portals within 3 m.
-- 09.11 (user's decision): portals — inside the next zone circle, on all maps. Target circle: the zone's "Next Center/Radius"
-- (where it shrinks to), without it — the current one. Margin 0.85 of the radius, so the portal does not stand right on the border.
function Spect.TargetCircle(Zone)
    local C, R = nil, 0
    pcall(function()
        local NR = Zone["Next Radius"]
        if NR and NR > 0 then C, R = Zone["Next Center"], NR end
    end)
    if not C or R <= 0 then C, R = Zone["Dynamic Center"], Zone["Dynamic Radius"] end
    return C, R * 0.85
end

-- 10.11: the current zone circle (with the same 0.85 margin) — for the guaranteed red (the whole circle, not the next one)
function Spect.CurrentCircle(Zone)
    local C, R = nil, 0
    pcall(function() C, R = Zone["Dynamic Center"], Zone["Dynamic Radius"] end)
    return C, (R or 0) * 0.85
end

function Spect.InCircle(C, R, X, Y)
    return (X - C.X) ^ 2 + (Y - C.Y) ^ 2 < R ^ 2
end

function Spect.NativePlace(Zone, Inner, C, R)
    if not C then C, R = Spect.TargetCircle(Zone) end
    Inner = 0
    local Busy = {}
    for _, Cls in ipairs({ "BP_ExtractDevice_Base_C", "BP_ExtractPortal_Base_C" }) do
        for _, A in ipairs(FindAllOf(Cls) or {}) do
            if IsValid(A) and not FullName(A):find("Default__", 1, true) then
                pcall(function()
                    local L = A:K2_GetActorLocation()
                    Busy[#Busy + 1] = { X = L.X, Y = L.Y }
                end)
            end
        end
    end
    local Pts, Fit = Zone.PotentialPortalPlacements, {}
    for J = 1, #Pts do
        local P = Pts[J]
        local X, Y, Z = P.X, P.Y, P.Z
        local D2 = (X - C.X) ^ 2 + (Y - C.Y) ^ 2
        if D2 < R ^ 2 and D2 >= Inner ^ 2 then
            local Free = true
            for _, B in ipairs(Busy) do
                if (X - B.X) ^ 2 + (Y - B.Y) ^ 2 < 90000 then Free = false; break end
            end
            if Free then Fit[#Fit + 1] = { X = X, Y = Y, Z = Z } end
        end
    end
    if #Fit == 0 then return nil, 0, #Pts end
    return Fit[math.random(#Fit)], #Fit, #Pts
end

-- Place for a portal. Ring = true: the ring up to the next circle (disk portals stand in the part that will soon close, like the native
-- Generate; the next disk's radius from the current center is an approximation, the next disk's center is not read). Ring = false: the whole circle.
-- No native points — a random spawn point inside the circle (they stand on the floor and inside the map).
function Spect.FloorPlace(Zone, I, Ring, Whole)
    -- 09.11: always inside the next circle (Spect.TargetCircle), the ring is no longer used.
    -- 10.11: Whole = true — the whole current circle (the guaranteed red always stood in the center: on disk 5 the next circle is small)
    local C, R
    if Whole then C, R = Spect.CurrentCircle(Zone) end
    if not C or R <= 0 then C, R = Spect.TargetCircle(Zone) end
    local RN = R
    R = R / 0.9 -- below there is a check "R * 0.9": the result is the same 0.85 margin
    for _, In in ipairs({ 0 }) do
        local Ok, At, Fit, All = pcall(Spect.NativePlace, Zone, In, C, RN)
        if not Ok then
            Log("[FLOOR] native portal points: error " .. tostring(At))
            break
        end
        if At then return At, string.format("native point, %d of %d fit, inner=%.0f", Fit, All, In) end
    end
    local Inside = {}
    for _, P in ipairs(FindAllOf("BP_ActorSpawnPoint_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            pcall(function()
                local L = P:K2_GetActorLocation()
                if (L.X - C.X) ^ 2 + (L.Y - C.Y) ^ 2 <= (R * 0.9) ^ 2 then Inside[#Inside + 1] = { X = L.X, Y = L.Y, Z = L.Z } end
            end)
        end
    end
    if #Inside == 0 then return nil, "no points" end
    return Inside[math.random(#Inside)], "spawn point"
end

-- Transform like the native placement choice: random yaw, scale 1
function Spect.PlaceTr(At)
    local Half = math.random() * math.pi
    return { Rotation = { X = 0, Y = 0, Z = math.sin(Half), W = math.cos(Half) }, Translation = At, Scale3D = { X = 1, Y = 1, Z = 1 } }
end

-- Spawn an actor of class Cls at point At
function Spect.SpawnClass(Zone, Cls, At)
    local GS = StaticFindObject("/Script/Engine.Default__GameplayStatics")
    local Tr = Spect.PlaceTr(At)
    local A = nil
    local OkB = pcall(function() A = GS:BeginDeferredActorSpawnFromClass(Zone, Cls, Tr, 2, nil) end)
    if not (OkB and A and A:IsValid()) then pcall(function() A = GS:BeginDeferredActorSpawnFromClass(Zone, Cls, Tr, 2, nil, 1) end) end
    if not (A and A:IsValid()) then return false end
    if not pcall(function() GS:FinishSpawningActor(A, Tr) end) then pcall(function() GS:FinishSpawningActor(A, Tr, 1) end) end
    return true
end

function Spect.FloorSpawn(Zone, I)
    local List = Spect.Rules.Portals[I] or {}
    local Done = 0
    for _, E in ipairs(List) do
        if math.random() <= E.Chance then
            local Roll, Path = math.random(), nil
            local Sum = 0
            for _, X in ipairs(E.Probs) do
                Sum = Sum + X.P
                if Roll <= Sum then Path = X.Path; break end
            end
            Path = Path or (E.Probs[1] and E.Probs[1].Path)
            local Cls = Path and Spect.LoadClass(Path)
            if Path and Spect.IsCastle() and (Path:find("Red", 1, true) or Path:find("Shadow", 1, true)) then
                Cls = nil
                Path = Path:find("Portal", 1, true) and "BP_ExtractPortal" or "BP_ExtractDevice"
            end
            local At, How = Spect.FloorPlace(Zone, I)
            if not Cls and At and Path then
                local Type = (Path:find("Grey", 1, true) and 4) or (Path:find("Shadow", 1, true) and 3) or (Path:find("Red", 1, true) and 2) or 1
                if Path:find("Portal", 1, true) and Type == 1 then Type = 5 end
                local Ok, Err = pcall(function()
                    Zone:SpawnOnePortalByType(Type, Spect.PlaceTr(At))
                end)
                if Ok then
                    Done = Done + 1
                    Log(string.format("[FLOOR] disk %d: native type %d instead of %s (%.0f %.0f %.0f), place: %s", I, Type, Path:match("([^%.]+)$"), At.X, At.Y, At.Z, How))
                else
                    Log("[FLOOR] SpawnOnePortalByType: " .. tostring(Err))
                end
            elseif Cls and At then
                if Spect.SpawnClass(Zone, Cls, At) then
                    Done = Done + 1
                    Log(string.format("[FLOOR] disk %d: %s (%.0f %.0f %.0f), place: %s", I, Path:match("([^%.]+)$"), At.X, At.Y, At.Z, How))
                end
            else
                Log(string.format("[FLOOR] disk %d: not placed %s, place: %s", I, tostring(Path), tostring(How)))
            end
        end
    end
    return Done
end

-- User's decision 06.11: the native red devices of the first floor do not appear (cause not found, docs/WORKAROUNDS.md),
-- so the blue device of disk 5 on a map with a second floor becomes red with a 50% chance. The device's native mechanism: set
-- ConvertTargetType and EnableConvert, then ConvertPortalType (hide, after 0.2 s place the target at the same spot, delete itself).
-- Variant — by group size, like the native disk settings: solo -> RedOne, duo and trio -> Red.
-- User's decision: besides the roll, this disk has one guaranteed red slot on top of the native portals (native point, whole circle).
-- 18.11 (owner's item 7, "red and purple 1:1"): the native disk-5 slot is blue 75% / purple 25% (Robot [DIAG]), no red.
-- A blue device of disk 5 becomes red with RedChance and purple with PurpleChance (together 0.50, as before). Per native
-- slot: red 0.75*RedChance = purple 0.25 + 0.75*PurpleChance -> RedChance = 0.4167, PurpleChance = 0.0833 (both 0.3125).
-- The guaranteed portal is 50/50 (SurePurpleChance) — the totals are equal.
Spect.RedChance, Spect.RedDisc = 0.4167, 5
Spect.PurpleChance = 0.0833
-- 18.11 (owner's item 7, "purple portals are rare — even them out with red"): the guaranteed portal of disk 5 is red or
-- purple (Shadow) 50/50. Purple only where RedSure runs: the first floor of a map with a second floor, not the Castle.
Spect.SurePurpleChance = 0.50
Spect.ShadowPath = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_Shadow.BP_ExtractDevice_Shadow_C"
-- purple variant by group size, as the native disk-5 slot does (Robot [DIAG] 18.11: ExtractDevice -> ShadowOne,
-- BlueTwo -> ShadowTwo, BlueThree -> Shadow); a class that does not load — the base Shadow
Spect.ShadowTargets = {
    BP_ExtractDevice_C = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_ShadowOne.BP_ExtractDevice_ShadowOne_C",
    BP_ExtractDevice_BlueTwo_C = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_ShadowTwo.BP_ExtractDevice_ShadowTwo_C",
    BP_ExtractDevice_BlueThree_C = Spect.ShadowPath,
}

-- 09.11 (user's decision): the Castle has no red and purple portals (purple ones led to the "hall under the map"):
-- neither the roll and the guaranteed red (Spect.RedTick/RedSure), nor native ones — the type in SpawnOnePortalByType is changed to blue
function Spect.IsCastle()
    return tostring(HostedMap or ""):find("GothicCastle", 1, true) ~= nil
end
-- ETgfPortalType (BP_DeathZone:SpawnOnePortalByType): 1 blue device, 2 red device, 3 shadow, 4 grey, 5 blue portal,
-- 6 red, 7 shadow, 8 yellow, 9 old blue (EscapeDevice_Blue), 10 old red (EscapeDevice_Red_01)
Spect.CastleBlue = { [2] = 1, [3] = 1, [6] = 5, [7] = 5, [10] = 9 }

-- Hook of the native SpawnOnePortalByType(Type, Transform): before the BP body — the place moved inside the next circle, in the Castle the type is blue
function Spect.OnPortalSpawn(Zone, TypeP, TrP)
    local Type = TypeP:get()
    local Notes = {}
    if Spect.IsCastle() and Spect.CastleBlue[Type] then
        TypeP:set(Spect.CastleBlue[Type])
        Notes[#Notes + 1] = string.format("Castle: type %d -> %d", Type, Spect.CastleBlue[Type])
    end
    local Tr = TrP:get()
    local P = Tr.Translation
    local C, R = Spect.TargetCircle(Zone)
    if C and R > 0 and not Spect.InCircle(C, R, P.X, P.Y) then
        local At = Spect.NativePlace(Zone, 0)
        if At then
            -- the struct parameter is replaced as a whole (set), fallback — a field of a copy; check — re-read the parameter
            local Ok = pcall(function() TrP:set(Spect.PlaceTr(At)) end)
            if not Ok then pcall(function() TrP:get().Translation = { X = At.X, Y = At.Y, Z = At.Z } end) end
            local Now = P
            pcall(function() Now = TrP:get().Translation end)
            local Moved = Spect.InCircle(C, R, Now.X, Now.Y)
            Notes[#Notes + 1] = string.format("place (%.0f %.0f) -> (%.0f %.0f), circle %.0f: %s", P.X, P.Y, Now.X, Now.Y, R, Moved and "inside" or "NOT moved")
        else
            Notes[#Notes + 1] = string.format("place (%.0f %.0f) outside circle %.0f, no free point inside", P.X, P.Y, R)
        end
    end
    if #Notes > 0 then Log("[PORTALPLACE] " .. table.concat(Notes, "; ")) end
end
Spect.RedTargets = {
    BP_ExtractDevice_C = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_RedOne.BP_ExtractDevice_RedOne_C",
    BP_ExtractDevice_BlueTwo_C = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_Red.BP_ExtractDevice_Red_C",
    BP_ExtractDevice_BlueThree_C = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_Red.BP_ExtractDevice_Red_C",
}

-- 09.11: in the Castle red and purple ones that appeared (native ones are placed bypassing SpawnOnePortalByType — stand 09.11) are turned into
-- blue by the same native mechanism as the roll: ConvertTargetType + EnableConvert + ConvertPortalType. Variant — by the number in the name:
-- One -> regular blue, Two -> BlueTwo, otherwise (Three / no number) -> BlueThree; portals — into a blue portal.
function Spect.CastleTarget(Name)
    if not (Name:find("Red", 1, true) or Name:find("Shadow", 1, true)) then return nil end
    local E = "/Game/TGF/Environment/Escapes/"
    if Name:find("ExtractPortal", 1, true) then return E .. "BP_ExtractPortal.BP_ExtractPortal_C" end
    if Name:find("One", 1, true) then return E .. "BP_ExtractDevice.BP_ExtractDevice_C" end
    if Name:find("Two", 1, true) then return E .. "BP_ExtractDevice_BlueTwo.BP_ExtractDevice_BlueTwo_C" end
    return E .. "BP_ExtractDevice_BlueThree.BP_ExtractDevice_BlueThree_C"
end

function Spect.CastleTick()
    if not Spect.IsCastle() then return end
    if Spect.CastleSeenBattle ~= (Host and Host.Battle) then Spect.CastleSeen, Spect.CastleSeenBattle = {}, Host and Host.Battle end
    for _, Base in ipairs({ "BP_ExtractDevice_Base_C", "BP_ExtractPortal_Base_C" }) do
        for _, D in ipairs(FindAllOf(Base) or {}) do
            if IsValid(D) and not FullName(D):find("Default__", 1, true) then
                local Key = D:GetAddress()
                local Name = Spect.ObjName(D:GetClass())
                local Path = not Spect.CastleSeen[Key] and Spect.CastleTarget(Name)
                if Path then
                    Spect.CastleSeen[Key] = true
                    local Cls = Spect.LoadClass(Path)
                    local Ok, Err = pcall(function()
                        D.ConvertTargetType = Cls
                        D.EnableConvert = true
                        D:ConvertPortalType()
                    end)
                    local L = { X = 0, Y = 0 }
                    pcall(function() L = D:K2_GetActorLocation() end)
                    Log(string.format("[CASTLE] %s (%.0f %.0f) -> %s: %s", Name, L.X, L.Y, Path:match("([^%.]+)$"), Ok and "ok" or ("error " .. tostring(Err))))
                end
            end
        end
    end
end

-- Guaranteed portal of disk 5: red or purple 50/50 (Spect.SurePurpleChance). Red — the variant follows the blue device chosen by
-- the map rules for this group (Spect.Rules, options applied); purple has one variant (BP_ExtractDevice_Shadow)
function Spect.RedSure(Disc)
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone or Spect.IsCastle() then return end
    local Path = "/Game/TGF/Environment/Escapes/BP_ExtractDevice_Red.BP_ExtractDevice_Red_C"
    local Purple = Spect.ShadowPath
    for _, List in pairs(Spect.Rules and Spect.Rules.Portals or {}) do
        for _, E in ipairs(List) do
            for _, X in ipairs(E.Probs) do
                local Blue = X.Path:match("([^%.]+)$") or ""
                if Spect.RedTargets[Blue] then Path = Spect.RedTargets[Blue] end
                if Spect.ShadowTargets[Blue] then Purple = Spect.ShadowTargets[Blue] end
            end
        end
    end
    local Roll = math.random()
    local IsPurple = Roll < Spect.SurePurpleChance
    if IsPurple then Path = Purple end
    Log(string.format("[RED] guaranteed slot: roll %.2f, purple chance %.2f -> %s", Roll, Spect.SurePurpleChance, IsPurple and "purple" or "red"))
    local Cls = Spect.LoadClass(Path)
    if not Cls and IsPurple and Path ~= Spect.ShadowPath then
        Log("[RED] guaranteed purple: no class " .. Path .. ", base Shadow")
        Path = Spect.ShadowPath
        Cls = Spect.LoadClass(Path)
    end
    local At, How = Spect.FloorPlace(Zone, Disc, false, true)
    if Cls and At and Spect.SpawnClass(Zone, Cls, At) then
        local C, R = Spect.CurrentCircle(Zone)
        local Off = C and math.sqrt((At.X - C.X) ^ 2 + (At.Y - C.Y) ^ 2) or -1
        Log(string.format("[RED] guaranteed %s (%.0f %.0f %.0f), place: %s; from center %.0f of %.0f", Path:match("([^%.]+)$"),
            At.X, At.Y, At.Z, How, Off, R / 0.85))
    else
        Log(string.format("[RED] guaranteed not placed: class %s, place %s", tostring(Cls ~= nil), tostring(How)))
    end
end

-- Force (command redroll): no roll, all still blue ones. Returns how many became red
function Spect.RedTick(Force)
    if Spect.IsCastle() then return 0 end
    if not (Spect.Red and Spect.Red.Battle == Host.Battle) then Spect.Red = { Battle = Host.Battle, Seen = {} } end
    local Seen, Done = Spect.Red.Seen, 0
    -- devices appear when the zone is already on its disk; those placed earlier are marked without a roll
    local Disc = -1
    pcall(function() Disc = FindLive("BP_DeathZone_C")["Current Disc Index"] + 1 end)
    for _, D in ipairs(FindAllOf("BP_ExtractDevice_Base_C") or {}) do
        if IsValid(D) and not FullName(D):find("Default__", 1, true) then
            local Name, Key = Spect.ObjName(D:GetClass()), D:GetAddress()
            local Path = Spect.RedTargets[Name]
            if Path and Seen[Key] ~= "red" and Seen[Key] ~= "purple" and (Force or not Seen[Key]) then
                local Roll = math.random()
                local Hit = Force or (Disc == Spect.RedDisc and Roll <= Spect.RedChance)
                -- 18.11: the next slice of the roll — purple (variant by group size, as the native slot)
                local Purple = not Hit and Disc == Spect.RedDisc and Roll <= Spect.RedChance + Spect.PurpleChance and Spect.ShadowTargets[Name]
                Seen[Key] = "blue"
                local L = D:K2_GetActorLocation()
                Log(string.format("[RED] %s (%.0f %.0f %.0f) disk %d: roll %.2f, chance red %.2f purple %.2f on disk %d: %s", Name, L.X, L.Y, L.Z, Disc, Roll, Spect.RedChance, Spect.PurpleChance, Spect.RedDisc, Hit and "red" or (Purple and "purple" or "blue")))
                local Target = (Hit and Path) or Purple
                if Target then
                    local Cls = Spect.LoadClass(Target)
                    if not Cls and Purple then Cls = Spect.LoadClass(Spect.ShadowPath) end
                    if Cls then
                        D.ConvertTargetType = Cls
                        D.EnableConvert = true
                        D:ConvertPortalType()
                        Seen[Key] = Hit and "red" or "purple"
                        Done = Done + 1
                    end
                end
            end
        end
    end
    if Disc == Spect.RedDisc and not Spect.Red.Sure then
        Spect.Red.Sure = true
        Spect.RedSure(Disc)
    end
    return Done
end

-- Purple portal arena (scene type LEVEL_TWO_PURPLE_DOOR). Trial run: commands gothrone and thronetp, every 5 s — where the players,
-- the throne and the boss are. The map has no player starts (docs/FIX_LIST.md PVE1), so appearing on it is what needs checking.
Spect.ThroneMap = "/TgfMaps/Scenes/Common/Level/L_Common_Throne_01"

-- Purple portal (06.11, docs/ghidra_portals): the native TgfPlayerGameFlow.ServerTeleport for both red and purple leads to the hall
-- under the map; purple also sets the Shadow scene (the zone skips it). The native continuation is the next scene with DDsUserBattleInfo.scene_tag,
-- not available on listen. The throne arena is part of the second floor map, so those who went into Shadow from the first floor appear on the floor together with
-- the red ones and the mod moves them to the throne. On the second floor itself there are no purple ones (user's decision). Spect.Shadow: OpenId -> { Done }.
function Spect.ToArena(PS, Pawn, Open, Why)
    local To = { X = 120800, Y = 0, Z = 120 }
    local T = FindLive("BP_Throne_ShadowKing_C")
    if T then
        local L = T:K2_GetActorLocation()
        To = { X = L.X + 800, Y = L.Y, Z = L.Z + 200 }
    end
    Pawn:K2_TeleportTo(To, { Pitch = 0, Yaw = 180, Roll = 0 })
    PS:SetPlayerSceneTag(1)
    pcall(function() PS.IsTeleportToWaitingRoom = false end)
    local Freed = 0
    for _, F in ipairs(FindAllOf("BP_TgfGame_PlayerGameFlow_C") or {}) do
        local Mine = false
        pcall(function() Mine = IsValid(F) and F:GetOwner():GetAddress() == PS:GetAddress() end)
        if Mine then
            pcall(function() F.IsInWaitingRoom = false end)
            pcall(function() F:MultiCastFreePlayer() end)
            pcall(function() F:MultiCastUnFreezePlayer() end)
            Freed = Freed + 1
        end
    end
    Log(string.format("[ARENA] %s (%s): to the throne (%.0f %.0f %.0f), Shadow scene, stream %d", Open, Why, To.X, To.Y, To.Z, Freed))
end

function Spect.ArenaTick()
    if not (Host and Host.Battle and Host.Scenes[3]) then return end
    local First, Floor = HostedMap == Host.Scenes[2], HostedMap == Host.Scenes[3]
    if not (First or Floor) then return end
    if not (Spect.Shadow and Spect.Shadow.Battle == Host.Battle) then Spect.Shadow = { Battle = Host.Battle, Who = {} } end
    local Who = Spect.Shadow.Who
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Tag, Pawn = "", "0", nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Tag = tostring(PS:GetPlayerSceneTag()) end)
            pcall(function() Pawn = PS.PawnPrivate end)
            if Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                if First then
                    if Tag == "1" and not Who[Open] then
                        Who[Open] = { Done = false }
                        Log(string.format("[ARENA] %s: entered purple on the first floor, will go to the throne on the floor", Open))
                    end
                elseif IsValid(Pawn) then
                    if Who[Open] and not Who[Open].Done then
                        Who[Open].Done = true
                        Spect.ToArena(PS, Pawn, Open, "from the first floor")
                    end
                end
            end
        end
    end
end

function Spect.ThroneTick()
    -- the throne and the boss are also in L_Dungeon_Official_02 (references in the map): look where the second floor's purple portal is
    if not (HostedMap == Spect.ThroneMap or (Host and Host.Scenes[3] and HostedMap == Host.Scenes[3])) then return end
    local Now = os.time()
    if Spect.ThroneAt and Now - Spect.ThroneAt < 5 then return end
    Spect.ThroneAt = Now
    local function Where(A)
        local L = A:K2_GetActorLocation()
        return string.format("(%.0f %.0f %.0f)", L.X, L.Y, L.Z)
    end
    local Items = {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Pawn = "?", nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Pawn = PS.PawnPrivate end)
            local Tag = "?"
            pcall(function() Tag = tostring(PS:GetPlayerSceneTag()) end)
            Items[#Items + 1] = Open .. " scene=" .. Tag .. " " .. (IsValid(Pawn) and Where(Pawn) or "no pawn")
        end
    end
    local Throne = FindLive("BP_Throne_ShadowKing_C")
    local Boss = {}
    for _, Cls in ipairs({ "BP_TGF_Monster_Shadow_King_C", "BP_TGF_H_Monster_Shadow_King_C" }) do
        for _, A in ipairs(FindAllOf(Cls) or {}) do
            if IsValid(A) and not FullName(A):find("Default__", 1, true) then Boss[#Boss + 1] = Cls .. " " .. Where(A) end
        end
    end
    Log(string.format("[THRONE] players: %s; throne %s; boss: %s; rooms %d, starts %d, characters %d", table.concat(Items, ", "),
        Throne and Where(Throne) or "none", #Boss > 0 and table.concat(Boss, ", ") or "none",
        #(FindAllOf("BP_ActorSpawnRoom_C") or {}), #(FindAllOf("LyraPlayerStart") or {}), #(FindAllOf("LyraCharacter") or {})))
end

-- 07.11 Message about new portals. Native chain: BP_DeathZone "Broadcast SpawnPortal Message" -> MapHint component ->
-- WBP_TgfGame_DisplayMessage_Portal on clients. The native spawn calls it itself, while the mod's portals (red from blue, guaranteed,
-- second floor) and purple ones, if the game stayed silent, are announced here. Game messages are counted by a hook, no repeats.
-- Devices standing on the map by the first pass are not announced. Command portalmsg: a manual message (check).
Spect.PortalKinds = {
    Blue = { Type = 1, Tag = "TGF.Environment.Portal.Spawn.Message" },
    Red = { Type = 2, Tag = "TGF.Environment.PortalRed.Spawn.Message" },
    Shadow = { Type = 3, Tag = "TGF.Environment.PortalShadow.Spawn.Message" },
    Grey = { Type = 4, Tag = "TGF.Environment.PortalGrey.Spawn.Message" },
}
Spect.PortalMsgDelay = 2

function Spect.PortalKindOf(ClassName)
    for _, K in ipairs({ "Shadow", "Red", "Grey" }) do
        if ClassName:find(K, 1, true) then return K end
    end
    return "Blue"
end

function Spect.PortalSend(Zone, Kind, Spawned)
    local K = Spect.PortalKinds[Kind]
    local Available = 0
    local Mgr = FindLive("TgfPortalManagerSubsystem")
    if Mgr then pcall(function() Available = Mgr:GetAvailablePortalNums(K.Type) end) end
    Spect.PortalSending = true
    local Ok, Err = pcall(function()
        local Fn = Zone["Broadcast SpawnPortal Message"]
        Fn(Zone, math.max(Available, Spawned), { TagName = FName(K.Tag) }, Spawned)
    end)
    Spect.PortalSending = false
    Log(string.format("[PORTALMSG] %s: appeared %d, available %d, %s", Kind, Spawned, Available, Ok and "message sent" or ("error " .. tostring(Err))))
    return Ok
end

function Spect.PortalWatch()
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone then return end
    local Key = tostring(Host and Host.Battle or "") .. "|" .. FullName(Zone)
    if not (Spect.PortalW and Spect.PortalW.Key == Key) then
        Spect.PortalW = { Key = Key, Seen = {}, Pending = {}, Native = {}, Seeded = false }
    end
    local W = Spect.PortalW
    for _, D in ipairs(FindAllOf("BP_ExtractDevice_Base_C") or {}) do
        if IsValid(D) and not FullName(D):find("Default__", 1, true) then
            local Name = D:GetFName():ToString()
            if not W.Seen[Name] then
                W.Seen[Name] = true
                if W.Seeded then
                    local Kind = Spect.PortalKindOf(Spect.ObjName(D:GetClass()))
                    local P = W.Pending[Kind] or { N = 0, At = os.time() }
                    P.N = P.N + 1
                    W.Pending[Kind] = P
                end
            end
        end
    end
    W.Seeded = true
    for Kind, P in pairs(W.Pending) do
        if os.time() - P.At >= Spect.PortalMsgDelay then
            W.Pending[Kind] = nil
            local Told = math.min(W.Native[Kind] or 0, P.N)
            W.Native[Kind] = (W.Native[Kind] or 0) - Told
            if P.N > Told then Spect.PortalSend(Zone, Kind, P.N - Told) end
        end
    end
end

function Spect.FloorTick()
    local OkW, ErrW = pcall(Spect.PortalWatch)
    if not OkW and Spect.PortalErr ~= tostring(ErrW) then
        Spect.PortalErr = tostring(ErrW)
        Log("[PORTALMSG] error: " .. Spect.PortalErr)
    end
    local OkT, ErrT = pcall(Spect.ThroneTick)
    if not OkT then Log("[THRONE] error: " .. tostring(ErrT)) end
    local OkA, ErrA = pcall(Spect.ArenaTick)
    if not OkA and Spect.ArenaErr ~= tostring(ErrA) then
        Spect.ArenaErr = tostring(ErrA)
        Log("[ARENA] error: " .. Spect.ArenaErr)
    end
    if Spect.IsCastle() then
        local OkC, ErrC = pcall(Spect.CastleTick)
        if not OkC then Log("[CASTLE] error: " .. tostring(ErrC)) end
    end
    if Spect.HasFloor() then
        if not (Spect.Rules and Spect.Rules.Battle == Host.Battle) then Spect.FloorCapture() end
        Spect.RedTick()
        return
    end
    if not Spect.OnFloor2() then return end
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone then return end
    -- GE classes — in advance, outside the zone hooks
    if not Spect.FloorGeLoaded then
        Spect.FloorGeLoaded = true
        for _, Path in ipairs({ Spect.Rules.Ge or "", Spect.Rules.GeLast or "" }) do
            if Path ~= "" then Spect.LoadClass(Path) end
        end
    end
    local Queue = Spect.FloorQueue
    Spect.FloorQueue = nil
    for _, Q in ipairs(Queue or {}) do
        if IsValid(Q.Pawn) then
            local OkF, ErrF
            if Q.Apply then OkF, ErrF = pcall(Spect.FloorApply, Zone, Q.Pawn) else OkF, ErrF = pcall(Spect.FloorRemove, Q.Pawn) end
            if not OkF then Log("[FLOOR] damage error: " .. tostring(ErrF)) end
        end
    end
    local Idx = Zone["Current Disc Index"]
    if Spect.FloorDiscKey ~= Host.Battle .. "|" .. tostring(Idx) then
        Spect.FloorDiscKey = Host.Battle .. "|" .. tostring(Idx)
        Log(string.format("[FLOOR] disk %d: portals by first floor rules: %d", Idx + 1, Spect.FloorSpawn(Zone, Idx + 1)))
    end
end

-- Hook once per process: the zone calls ApplyCurrentDamageGEs once when the player enters the zone
function Spect.ZoneHook()
    if Spect.ZoneHooked or not FindLive("BP_DeathZone_C") then return end
    Spect.ZoneHooked = true
    local Base = "/Game/TGF/Environment/DeathZone/BP_DeathZone.BP_DeathZone_C:"
    local Res = {}
    -- 09.11: native portals — inside the next circle, in the Castle without red/purple (Spect.OnPortalSpawn)
    local OkP, ErrP = pcall(function()
        RegisterHook(Base .. "SpawnOnePortalByType", function(Self, TypeP, TrP)
            local Ok, Err = pcall(Spect.OnPortalSpawn, Self:get(), TypeP, TrP)
            if not Ok then Log("[PORTALPLACE] error: " .. tostring(Err)) end
        end)
    end)
    Res[#Res + 1] = "SpawnOnePortalByType=" .. tostring(OkP) .. (OkP and "" or (" " .. tostring(ErrP)))
    for _, Fn in ipairs({ "ApplyCurrentDamageGEs", "RemoveCurrentDamageGEs" }) do
        local Ok = pcall(function()
            RegisterHook(Base .. Fn, function(Self, Player)
                pcall(function()
                    local Z, Ge = Self:get(), {}
                    pcall(function() for I = 1, #Z.CurrentDamageGEs do Ge[#Ge + 1] = Spect.ObjName(Z.CurrentDamageGEs[I]) end end)
                    Log(string.format("[ZONEDMG] %s %s GE=[%s]", Fn, Spect.ObjName(Player), table.concat(Ge, ",")))
                    -- 06.11 10:50: LoadAsset and applying the GE right in this hook (inside a running BP) hung the DS game thread —
                    -- the hook only queues, Spect.FloorTick does the work
                    if Spect.OnFloor2() then
                        Spect.FloorQueue = Spect.FloorQueue or {}
                        Spect.FloorQueue[#Spect.FloorQueue + 1] = { Apply = Fn == "ApplyCurrentDamageGEs", Pawn = Player:get() }
                    end
                end)
            end)
        end)
        Res[#Res + 1] = Fn .. "=" .. tostring(Ok)
    end
    Log("[ZONEDMG] hooks: " .. table.concat(Res, " "))
    local OkMsg = pcall(function()
        RegisterHook(Base .. "Broadcast SpawnPortal Message", function(_, _, Tag, Spawned)
            pcall(function()
                if Spect.PortalSending or not Spect.PortalW then return end
                local Name = Tag:get().TagName:ToString()
                for Kind, K in pairs(Spect.PortalKinds) do
                    if K.Tag == Name then
                        local N = Spawned:get()
                        Spect.PortalW.Native[Kind] = (Spect.PortalW.Native[Kind] or 0) + N
                        Log(string.format("[PORTALMSG] game itself: %s, appeared %d", Kind, N))
                    end
                end
            end)
        end)
    end)
    Log("[PORTALMSG] native message hook: " .. tostring(OkMsg))
    -- portals and level end: what is called on entry and after it
    local Portal = {}
    for _, H in ipairs({ { "/Game/TGF/Environment/Escapes/BP_ExtractPortal_Base.BP_ExtractPortal_Base_C:", "OnEnterPortal" },
        { Base, "EndCurrentLevel" }, { Base, "TravelToNextFloor" }, { Base, "BroadCastGameOverEvent" } }) do
        local Ok = pcall(function()
            RegisterHook(H[1] .. H[2], function(Self)
                pcall(function() Log(string.format("[PORTAL] %s %s", H[2], Spect.ObjName(Self:get()))) end)
                if H[2] == "TravelToNextFloor" then
                    ExecuteInGameThread(function()
                        local OkF, ErrF = pcall(Spect.FloorTravel, "portal")
                        if not OkF then Log("[FLOOR] error: " .. tostring(ErrF)) end
                    end)
                end
            end)
        end)
        Portal[#Portal + 1] = H[2] .. "=" .. tostring(Ok)
    end
    Log("[PORTAL] hooks: " .. table.concat(Portal, " "))
    local OkN = pcall(function()
        RegisterHook("/Script/LyraGame.BattleDsUtility:HasNextFloor", function() end, function()
            if Spect.HasFloor() then return true end
        end)
    end)
    Log("[FLOOR] hook HasNextFloor: " .. tostring(OkN))
end

-- Once a second: who is outside the radius, health, flags that make Update Damage skip the player, active zone GEs
function Spect.ZoneTrace()
    local Zone = FindLive("BP_DeathZone_C")
    if not Zone then return end
    local Now = os.time()
    if Spect.ZoneTraceAt == Now then return end
    Spect.ZoneTraceAt = Now
    local Rst = Spect.ZoneRestore
    if Rst and Now >= Rst.At then
        Spect.ZoneRestore = nil
        pcall(function() Zone["Dynamic Radius"] = Rst.R end)
        Log(string.format("[ZONEDMG] zonetest: radius restored %.0f", Rst.R))
    end
    local R, C = Zone["Dynamic Radius"], Zone["Dynamic Center"]
    Spect.Trace = Spect.Trace or {}
    local AttrCls = StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
    local DefCls = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
    local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Pawn = "", nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Pawn = PS.PawnPrivate end)
            if Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" and IsValid(Pawn) then
                local L = Pawn:K2_GetActorLocation()
                local Dist = math.sqrt((L.X - C.X) ^ 2 + (L.Y - C.Y) ^ 2)
                local Out = Dist > R
                local Hp = -1
                pcall(function() Hp = Pawn:GetComponentByClass(AttrCls):GetAttributeSet(DefCls).CurHealth.CurrentValue end)
                local T = Spect.Trace[Open] or {}
                -- a change of portal flags and the scene marker — to the log at once: this shows where the player went
                local Marks = "?"
                pcall(function()
                    Marks = string.format("scene=%s portal=%s escape=%s", tostring(PS:GetPlayerSceneTag()), tostring(PS:GetIsTeleportToWaitingRoom()), tostring(PS:GetIsEscapeQuit()))
                end)
                if Marks ~= T.Marks then
                    Log(string.format("[PORTAL] %s %s -> %s (x=%.0f y=%.0f z=%.0f) alive=%s in_hall=%s", Open, tostring(T.Marks), Marks, L.X, L.Y, L.Z,
                        tostring(Zone.AliveFighterNumber), tostring(Zone.WaitingRoomFighters)))
                    T.Marks = Marks
                end
                if Out ~= T.Out or (Out and (Now - (T.At or 0) >= 3 or Hp ~= T.Hp)) then
                    local Fx, Scene, Flags, GsAsc = {}, "?", "?", "?"
                    pcall(function() Scene = tostring(PS:GetPlayerSceneTag()) end)
                    pcall(function() Flags = string.format("portal=%s escape=%s selfquit=%s", tostring(PS:GetIsTeleportToWaitingRoom()), tostring(PS:GetIsEscapeQuit()), tostring(PS:GetIsSelfQuit())) end)
                    pcall(function() GsAsc = tostring(IsValid(Lib:GetAbilitySystemComponent(FindLive("LyraGameState")))) end)
                    pcall(function()
                        local Asc = Lib:GetAbilitySystemComponent(Pawn)
                        for I = 1, #Zone.CurrentDamageGEs do
                            local Ge = Zone.CurrentDamageGEs[I]
                            local N = "?"
                            pcall(function() N = tostring(Asc:GetGameplayEffectCount(Ge, nil, false)) end)
                            if N == "?" then pcall(function() N = tostring(Asc:GetGameplayEffectCount(Ge:get(), nil, false)) end) end
                            Fx[#Fx + 1] = Spect.ObjName(Ge) .. "=" .. N
                        end
                    end)
                    Log(string.format("[ZONEDMG] %s out=%s dist=%.0f R=%.0f hp=%.1f scene=%s %s asc_gs=%s state=%s disk=%s GE[%s]", Open, tostring(Out), Dist, R, Hp,
                        Scene, Flags, GsAsc, tostring(Zone.State), tostring(Zone["Current Disc Index"]), table.concat(Fx, ",")))
                    T.At = Now
                end
                T.Out, T.Hp = Out, Hp
                Spect.Trace[Open] = T
            end
        end
    end
end

-- 06.11 own room proximity check (the word prox in dbserver-observer.txt): the native UPlayerDistanceCheckerComponent does not fire without the host's
-- pawn (01.11, 03.11). The same threshold and the room's native event, the native component is disabled so there is no
-- second start. Rooms are rebuilt every 10 s: the native OnReady may enable the component again.
Spect.PROX_EVT = "BndEvt__BP_ActorSpawnRoom_PlayerDistanceChecker_K2Node_ComponentBoundEvent_1_Event_SomePlayerComesClose__DelegateSignature"
function Spect.ProxTick()
    local Key = HostedMap .. "|" .. tostring(Epoch)
    if Spect.ProxKey ~= Key then
        Spect.ProxKey, Spect.ProxRooms, Spect.ProxScanAt, Spect.ProxFired = Key, nil, 0, 0
        Spect.ProxOn = (ReadFile("dbserver-observer.txt") or ""):find("prox", 1, true) ~= nil
    end
    if not Spect.ProxOn then return end
    local Now = os.time()
    if Spect.ProxAt == Now then return end
    Spect.ProxAt = Now
    if not Spect.ProxRooms or Now - Spect.ProxScanAt >= 10 then
        Spect.ProxScanAt = Now
        local Rooms = {}
        for _, R in ipairs(FindAllOf("BP_ActorSpawnRoom_C") or {}) do
            if IsValid(R) and not FullName(R):find("Default__", 1, true) and R.bFirstTimeActuallySpawn and not R.bSpawnWhenPhaseStarts then
                pcall(function()
                    R.PlayerDistanceChecker:SetComponentTickEnabled(false)
                    local T = R.DistanceThreshold
                    Rooms[#Rooms + 1] = { R = R, L = R:K2_GetActorLocation(), T2 = T * T }
                end)
            end
        end
        Spect.ProxRooms = Rooms
    end
    local Players = {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            pcall(function()
                local Pawn = PS.PawnPrivate
                if PS.MyOpenId:ToString() ~= "LOCAL_GAME_MODE_OPENID" and IsValid(Pawn) and not Pawn:IsDeadOrDying() then
                    Players[#Players + 1] = Pawn:K2_GetActorLocation()
                end
            end)
        end
    end
    local Rooms = Spect.ProxRooms
    if #Players > 0 then
        for I = #Rooms, 1, -1 do
            local E = Rooms[I]
            for _, P in ipairs(Players) do
                if (P.X - E.L.X) ^ 2 + (P.Y - E.L.Y) ^ 2 + (P.Z - E.L.Z) ^ 2 <= E.T2 then
                    if IsValid(E.R) and E.R.bFirstTimeActuallySpawn then
                        local Ok, Err = pcall(function() E.R[Spect.PROX_EVT](E.R, P) end)
                        if Ok then Spect.ProxFired = Spect.ProxFired + 1 else Log("[PROX] error: " .. tostring(Err)) end
                    end
                    table.remove(Rooms, I)
                    break
                end
            end
        end
    end
    if Now - (Spect.ProxLogAt or 0) >= 30 then
        Spect.ProxLogAt = Now
        Log(string.format("[PROX] waiting=%d started=%d players=%d", #Rooms, Spect.ProxFired, #Players))
    end
end

-- Mobs: how many in total, how they move; on the second measurement — who fell and who vanished (check for falling through the floor)
function Spect.MobSample()
    local Out, Modes, N = {}, {}, 0
    for _, A in ipairs(FindAllOf("LyraCharacter") or {}) do
        if IsValid(A) and not FullName(A):find("Default__", 1, true) and not FullName(A):find("Hero", 1, true) then
            pcall(function()
                local L = A:K2_GetActorLocation()
                local M = "?"
                pcall(function() M = tostring(A.CharacterMovement.MovementMode) end)
                N = N + 1
                Modes[M] = (Modes[M] or 0) + 1
                Out[A:GetAddress()] = { Z = L.Z, Name = Spect.ObjName(A), M = M }
            end)
        end
    end
    return Out, Modes, N
end

function Spect.MobFallTick()
    local F = Spect.MobFall
    if not F or os.time() < F.At then return end
    local Now, Modes, N = Spect.MobSample()
    local M = {}
    for K, V in pairs(Modes) do M[#M + 1] = "mode" .. K .. "×" .. V end
    if F.Step == 1 then
        Log(string.format("[MOBFALL] +%d s: mobs=%d %s", F.Delay, N, table.concat(M, " ")))
        Spect.MobFall = { Step = 2, At = os.time() + 20, Delay = F.Delay + 20, Base = Now }
        return
    end
    local Fell, Gone, Examples = 0, 0, {}
    for Addr, B in pairs(F.Base) do
        local C = Now[Addr]
        if not C then
            Gone = Gone + 1
        elseif B.Z - C.Z > 300 then
            Fell = Fell + 1
            if #Examples < 12 then Examples[#Examples + 1] = string.format("%s %.0f->%.0f", B.Name, B.Z, C.Z) end
        end
    end
    Log(string.format("[MOBFALL] +%d s: mobs=%d %s fell>3m=%d vanished=%d", F.Delay, N, table.concat(M, " "), Fell, Gone))
    Spect.LogList("[MOBFALL] fallen: ", Examples)
    Spect.MobFall = nil
end

-- Game ID on the player's screen (09.11 — own RuClient label; 10.11 — the native ping widget WBP_TgfGame_Ping, checked on
-- the stand: the number is visible without the client mod): the match number (the number from Host.Battle, l40003016_224725003 -> 224725003) is written to
-- LyraGameState.BattleId (Spect.BattleIdSet, once per map — hall and match).
function Spect.GameIdTick()
    if not Host or not Host.Battle then return end
    local Id = Host.Battle:match("_(%d+)$") or Host.Battle
    pcall(Spect.BattleIdSet, Id)
end

-- 10.11 Game ID in the native ping widget (WBP_TgfGame_Ping, block HorizontalBoxBattleId/ID; docs/BATTLE_ID_IN_PING.md):
-- the string LyraGameState.BattleId (FString, ReplicatedUsing=OnRep_BattleId -> OnBattleIdRep -> ID.SetText). Written with the engine's
-- KismetSystemLibrary.SetStringPropertyByName — an FString is not assigned from Lua (31.10 that corrupted the heap). Once per map.
function Spect.BattleIdSet(Id)
    local Key = tostring(HostedMap) .. "|" .. Id
    if Spect.BattleIdKey == Key then return end
    local KSL = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    if not (KSL and KSL:IsValid()) then return end
    for _, GS in ipairs(FindAllOf("LyraGameState") or {}) do
        if IsValid(GS) and not FullName(GS):find("Default__", 1, true) then
            local Ok, Err = pcall(function()
                KSL:SetStringPropertyByName(GS, FName("BattleId"), Id)
                GS:ForceNetUpdate()
            end)
            Log(string.format("[GAMEID] BattleId=%s on %s: %s", Id, FullName(GS):match("([^%.]+)$") or "?",
                Ok and "ok" or tostring(Err)))
            if Ok then Spect.BattleIdKey = Key end
        end
    end
end

-- 09.11 map options insurance: the group variant options (solo/duo/trio: <map>01/02/03) on the match map's GameState — as in
-- the backend task (Host.OptionArgs, from memory, not from the file). The spawn point sets depend on them (ALyraPlayerStartSet
-- LevelArgs) — the game enables them when the first player joins; foreign options gave trio in solo → 8 starts instead of 14.
-- Checked every tick of the match map; fix and log — only on a mismatch.
function Spect.OptionGuard()
    if not Host or not Host.MapId or not HostedMap or HostedMap:find("Matching", 1, true) then return end
    local Want = {}
    for V in (Host.OptionArgs or ""):gmatch("%d+") do
        local N = tonumber(V)
        if N and math.floor(N / 100) == Host.MapId then Want[#Want + 1] = N end
    end
    if #Want == 0 then return end
    local Gs = FindLive("LyraGameState")
    if not IsValid(Gs) then return end
    local Has, Extra = {}, {}
    for I = 1, #Gs.OptionIds do
        local N = tonumber(Gs.OptionIds[I])
        if N then
            Has[N] = true
            if math.floor(N / 100) == Host.MapId then
                local Wanted = false
                for _, W in ipairs(Want) do if W == N then Wanted = true end end
                if not Wanted then Extra[#Extra + 1] = N end
            end
        end
    end
    local Missing = {}
    for _, W in ipairs(Want) do if not Has[W] then Missing[#Missing + 1] = W end end
    if #Missing == 0 and #Extra == 0 then return end
    for _, N in ipairs(Extra) do pcall(function() Gs:RemoveOption(N) end) end
    for _, N in ipairs(Missing) do pcall(function() Gs:AddOption(N) end) end
    local Now = {}
    pcall(function() for I = 1, #Gs.OptionIds do Now[#Now + 1] = tostring(Gs.OptionIds[I]) end end)
    Log(string.format("[MAP_GUARD] %s: group options differ from the task — removed [%s], set [%s], now [%s]", tostring(HostedMap),
        table.concat(Extra, ","), table.concat(Missing, ","), table.concat(Now, ",")))
end

-- 03.11 census of the map's spawn points (match limits: players/teams per map). Once per map load: the sets
-- ALyraPlayerStartSet (team, LevelArgs — variants for dynamic_option_args, hall/death flags) and how many starts are attached to
-- each; all ALyraPlayerStart (flags WaitingPoint/DeathPoint); GameSession.MaxPlayers.
-- World Partition: only loaded cells count (starts are usually always loaded).
function Spect.Census()
    if Spect.CensusMap == HostedMap then return end
    Spect.CensusMap = HostedMap
    local Sets, BySet, Loose, Wait, Death = {}, {}, 0, 0, 0
    local Starts = FindAllOf("LyraPlayerStart") or {}
    for _, St in ipairs(Starts) do
        if IsValid(St) and not FullName(St):find("Default__", 1, true) then
            local Parent = nil
            pcall(function() Parent = St:GetAttachParentActor() end)
            local Key = IsValid(Parent) and FullName(Parent) or nil
            if Key then BySet[Key] = (BySet[Key] or 0) + 1 else Loose = Loose + 1 end
            pcall(function() if St:IsWaitingPoint() then Wait = Wait + 1 end end)
            pcall(function() if St:IsDeathPoint() then Death = Death + 1 end end)
        end
    end
    local Lines = {}
    for _, S in ipairs(FindAllOf("LyraPlayerStartSet") or {}) do
        if IsValid(S) and not FullName(S):find("Default__", 1, true) then
            local Team, Args, W, D = "?", {}, "?", "?"
            pcall(function() Team = tostring(S:GetTeamId()) end)
            pcall(function() S.LevelArgs:ForEach(function(_, V) Args[#Args + 1] = tostring(V:get()) end) end)
            pcall(function() W = tostring(S.bWaitingPointSet) end)
            pcall(function() D = tostring(S.bDeathPointSet) end)
            Lines[#Lines + 1] = string.format("%s team=%s args=[%s] wait=%s death=%s starts=%d", Short(S), Team,
                table.concat(Args, ","), W, D, BySet[FullName(S)] or 0)
        end
    end
    local Groups = #(FindAllOf("LyraPlayerStartSetGroup") or {})
    local Max = "?"
    pcall(function() Max = tostring(FindLive("GameModeBase").GameSession.MaxPlayers) end)
    Log(string.format("[STARTS] %s: starts %d (without a set %d, hall %d, death %d), sets %d, set groups %d, MaxPlayers=%s",
        HostedMap, #Starts, Loose, Wait, Death, #Lines, Groups, Max))
    for _, L in ipairs(Lines) do Log("[STARTS]   " .. L) end
end

-- --- Hero fixes on the server (04.11, moved from CrashTracer FixHeroPawn; docs/fix_listen.md items 2–3) --------------
-- CrashTracer did them from the OnReady hook, set from NotifyOnNewObject(Pawn) — that is where UE4SS crashed (B13,
-- research\crash7777). Here without hooks and notifications: once a second we go over live heroes BP_Fighter_HeroSample_C, and
-- those seen for at least HeroFix.DELAY s are fixed once (hero address -> done). Authority part only:
--   gravity     — UActorMapComponent on listen is oriented to Player 0 (the host, an observer here) and zeroes
--                 GravityScale for the rest: DisableActorMap, component disabled, GravityScale 1.28, MOVE_None -> Walking;
--                 ForceAlwaysRelevent / DisableDistanceSlowReplicateRate (also by distance to the host);
--   combat      — UseHitValidation = false and ValidDistance on the pawn (hit validation on the server), tick of AGR_CombatManager;
--                 weapons (B_WeaponBase_C) — the same plus the tick of the parent mesh (socket positions for traces);
--   mesh, ticks — tick of Mesh/ShellMesh with AlwaysTickPoseAndRefreshBones (headless does not render — without it the bones stand still),
--                 Inited, SetComponentTick / SetDSMeshTick, CheckMainHandWeapon;
--   fallback    — backpack grid 0x0 -> 7x10, no ASC on the attribute component -> native initialization (as in CrashTracer;
--                 does not fire with data from dbnative).
-- The client part (ShellMesh for others/own, animation layers, affix tooltips) — in DungeonborneRuClient.
-- dbserver-herofix.txt = 0 — disable.
local HeroFix = { Done = {}, Seen = {}, Weapons = {}, On = nil, DELAY = 1, DELAY_FLOOR2 = 3 }

-- 08.11 (complaint: at the start of the second floor players fall through the floor). The native UActorMapComponent keeps the hero's gravity
-- at zero until the map around is loaded; the gravity fix disables it and enables gravity. On the first floor
-- players appear near the host — the floor is already there; on the second floor they arrive where the server is still loading rooms, and with
-- gravity they fall after 1 s. On the second floor we wait DELAY_FLOOR2 s (3 s — user's decision 08.11, was 10) — until then
-- the hero is held by the native mechanism.
function HeroFix.Delay()
    local Floor2 = false
    pcall(function() Floor2 = Host ~= nil and Host.Scenes ~= nil and Host.Scenes[3] ~= nil and HostedMap == Host.Scenes[3] end)
    return Floor2 and HeroFix.DELAY_FLOOR2 or HeroFix.DELAY
end

function HeroFix.Pawn(P)
    local Parts = {}
    local function Try(Name, Fn)
        local Ok = pcall(Fn)
        Parts[#Parts + 1] = Name .. (Ok and "" or "!")
    end
    Try("gravity", function()
        P.DisableActorMap = true
        P.ForceAlwaysRelevent = true
        P.DisableDistanceSlowReplicateRate = true
        P.bFreeze = false
        local Amc = P.ActorMapComponent
        if IsValid(Amc) then
            Amc:SetComponentTickEnabled(false)
            Amc:Deactivate()
        end
        P.DefaultGravityScale = 1.28
        P.ReplicatedGravityScale = 1.28
        P:SetGravityScale(1.28, true, true)
        local Move = P.CharacterMovement
        if IsValid(Move) then
            Move.GravityScale = 1.28
            if Move.MovementMode == 0 then Move:SetMovementMode(1, 0) end
        end
    end)
    Try("combat", function()
        P.UseHitValidation = false
        P.ValidDistance = 99999.0
        local Cm = P.AGR_CombatManager
        if IsValid(Cm) then Cm:SetComponentTickEnabled(true) end
    end)
    Try("mesh", function()
        for _, M in ipairs({ P.Mesh, P.ShellMesh }) do
            if IsValid(M) then
                M:SetComponentTickEnabled(true)
                M.VisibilityBasedAnimTickOption = 0
            end
        end
    end)
    Try("inv", function()
        local Inv = P:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfInventoryManagerComponent"))
        if IsValid(Inv) and (Inv.Size.X == 0 or Inv.Size.Y == 0) then
            Inv:InitInventoryWithSize({ X = 7, Y = 10 })
            pcall(function() Inv:OnRep_Size() end)
            Parts[#Parts + 1] = "grid7x10"
        end
    end)
    Try("equip", function()
        local Eq = P:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent"))
        if IsValid(Eq) then
            pcall(function() Eq:CheckMainHandWeapon() end)
            pcall(function() Eq:SetEquipmentVisible(true, true, true, 0) end)
        end
    end)
    Try("attr", function()
        local A = P.AttributeComponent
        if not IsValid(A) then A = P:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfAttributeComponent")) end
        if not IsValid(A) or IsValid(A.AbilitySystemComponent) then return end
        local PS = P:GetLyraPlayerState()
        local Asc = IsValid(PS) and PS:GetLyraAbilitySystemComponent() or nil
        if IsValid(Asc) then
            A:InitializeWithAbilitySystem(Asc)
            pcall(function() A:InitDefault(true, true) end)
            pcall(function() A:InitFirstLevel() end)
            pcall(function() A:InitTalents() end)
            Parts[#Parts + 1] = "asc-init"
        end
    end)
    Try("tick", function()
        P.Inited = true
        P:SetComponentTick(true, 0)
        P:SetDSMeshTick(true, 0)
    end)
    return table.concat(Parts, ",")
end

function HeroFix.Tick()
    if HeroFix.On == nil then
        HeroFix.On = (ReadFile("dbserver-herofix.txt") or ""):match("^%s*0") == nil
        Log("[HEROFIX] hero fixes on the server: " .. (HeroFix.On and "enabled" or "disabled (dbserver-herofix.txt)"))
    end
    if not HeroFix.On then return end
    local Now, Alive, Delay = os.time(), {}, HeroFix.Delay()
    for _, P in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            local Addr = P:GetAddress()
            Alive[Addr] = true
            HeroFix.Seen[Addr] = HeroFix.Seen[Addr] or Now
            if not HeroFix.Done[Addr] and Now - HeroFix.Seen[Addr] >= Delay then
                HeroFix.Done[Addr] = true
                local Ok, Res = pcall(HeroFix.Pawn, P)
                Log(string.format("[HEROFIX] %s: %s (after %d s)", FullName(P):match("([^%.]+)$") or "?", Ok and Res or ("error " .. tostring(Res)), Delay))
            end
        end
    end
    for Addr in pairs(HeroFix.Seen) do
        if not Alive[Addr] then HeroFix.Seen[Addr], HeroFix.Done[Addr] = nil, nil end
    end
    local WAlive = {}
    for _, W in ipairs(FindAllOf("B_WeaponBase_C") or {}) do
        if IsValid(W) and not FullName(W):find("Default__", 1, true) then
            local Addr = W:GetAddress()
            WAlive[Addr] = true
            if not HeroFix.Weapons[Addr] then
                HeroFix.Weapons[Addr] = true
                pcall(function() W.UseHitValidation = false end)
                pcall(function()
                    local Cm = W.AGR_CombatManager
                    if IsValid(Cm) then Cm:SetComponentTickEnabled(true) end
                    local Root = W:K2_GetRootComponent()
                    local Parent = Root and Root:GetAttachParent()
                    if IsValid(Parent) then
                        Parent:SetComponentTickEnabled(true)
                        Parent.VisibilityBasedAnimTickOption = 0
                    end
                end)
            end
        end
    end
    for Addr in pairs(HeroFix.Weapons) do
        if not WAlive[Addr] then HeroFix.Weapons[Addr] = nil end
    end
end

-- DISABLED 07.11 (rolled back by the user's decision, the call from Ready removed); the code is kept in case it comes back.
-- 07.11 (user's decision, playtest season: base 50% at level 20 + equipment): resistance cap 80% hardcoded.
-- Once a second in the game thread: the hero's ElementResistanceRate* above 0.8 -> 0.8 (CurrentValue; damage is computed by the server).
-- Equipment adds and subtracts its bonus to the value (not a recalculation from zero, checked 07.11 on the stand), so
-- the cap remembers what it cut (Cut) and returns it when the value goes down: real = current + Cut. Without this, after
-- removing an item the resistance would be lower than the real one. After equipping, the value may stay above the cap for up to a second.
HeroFix.RESIST_CAP = 0.8
HeroFix.ResistLogged = {}
HeroFix.ResistCut = {} -- [hero path][element] = cut off by the cap
function HeroFix.ResistCap()
    local DefCls = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
    local Alive = {}
    for _, P in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            pcall(function()
                local D = P.AttributeComponent:GetAttributeSet(DefCls)
                if not IsValid(D) then return end
                local Path = FullName(P)
                Alive[Path] = true
                local Cut = HeroFix.ResistCut[Path] or {}
                HeroFix.ResistCut[Path] = Cut
                for _, N in ipairs({ "None", "Fire", "Ice", "Thunder", "Light", "Dark" }) do
                    local A = D["ElementResistanceRate" .. N]
                    local V = A.CurrentValue
                    local True = V + (Cut[N] or 0)
                    local New = math.min(True, HeroFix.RESIST_CAP)
                    if math.abs(New - V) > 1e-4 then
                        A.CurrentValue = New
                        local Key = Path .. N
                        if New < V and not HeroFix.ResistLogged[Key] then
                            HeroFix.ResistLogged[Key] = true
                            Log(string.format("[RESISTCAP] %s %s %.3f -> %.2f", Path:match("([^%.]+)$") or "?", N, True, New))
                        end
                    end
                    Cut[N] = True - New
                end
            end)
        end
    end
    for Path in pairs(HeroFix.ResistCut) do
        if not Alive[Path] then HeroFix.ResistCut[Path] = nil end
    end
end

-- 08.11 (docs/LEYLINE_SEALS.md, LOCAL analysis): the native Loot takes the loot row by UBattleDsUtility+0x1d4 (loot_map_id),
-- and it is set only by InitGameMap, which we do not call on listen — the field is 0, and every match took the loot of map 0 (the
-- Classic/Hard tables and ley line seals did not work). We set the field via dbnative op=lootmap on every match map (hall, floors)
-- and re-read it after 30 s. Only Crypt/Castle 10101–10299: there are no loot rows for arena maps — chests would be empty.
-- A global table: the main chunk has a 200-local limit.
LootMap = { Key = nil, At = 0, Checked = false, Map = 0, Loot = 0 }

function LootMap.Want()
    local MapId = Host and Host.MapId
    if not MapId or MapId < 10101 or MapId > 10299 then return nil end
    local Loot = MapId
    pcall(function()
        local W = FindLive("TGFTableManager"):GetCurrentGroupAPI({})
        local R = W:GetRowExcelMapDetail_Of_MapId(MapId, {})
        local V = R and R:IsValid() and tonumber(R:GetLootMapId())
        if V and V > 0 then Loot = V end
    end)
    -- 13.11: the second floor without ley line seals and shards — own loot map +10 (10112/10113/10213, seasons/playtest/loot)
    local Level = tostring(HostedMap):match("([^/%.]+)$") or ""
    if (Level:find("_Official_02", 1, true) or Level:find("SecondFloor", 1, true))
        and (Loot == 10102 or Loot == 10103 or Loot == 10203) then
        Loot = Loot + 10
    end
    return MapId, Loot
end

function LootMap.Tick()
    local MapId, Loot = LootMap.Want()
    if not MapId then return end
    local Ds = FindLive("BattleDsUtility")
    if not IsValid(Ds) then return end
    local Addr = string.format("0x%X", Ds:GetAddress())
    local Key = string.format("%s|%s|%d|%s", tostring(HostedMap), tostring(Host.Battle), MapId, Addr)
    if LootMap.Key ~= Key then
        LootMap.Key, LootMap.At, LootMap.Checked, LootMap.Map, LootMap.Loot = Key, os.time(), false, MapId, Loot
        local Rsp = NativeCall("lootmap", { ds = Addr, map = MapId, loot = Loot }) or "no answer"
        Log(string.format("[LOOTMAP] %s: map=%d loot=%d -> %s", tostring(HostedMap):match("([^/]+)$") or "?", MapId, Loot,
            (Rsp:gsub("\n", " "))))
    elseif not LootMap.Checked and os.time() - LootMap.At >= 30 then
        LootMap.Checked = true
        local Rsp = NativeCall("lootmap", { ds = Addr }) or ""
        if tonumber(Rsp:match("loot=(%d+)")) == LootMap.Loot then
            Log("[LOOTMAP] holds after 30 s: " .. (Rsp:gsub("\n", " ")))
        else
            local Again = NativeCall("lootmap", { ds = Addr, map = LootMap.Map, loot = LootMap.Loot }) or "no answer"
            Log("[LOOTMAP] reset after 30 s (" .. (Rsp:gsub("\n", " ")) .. ") — again: " .. (Again:gsub("\n", " ")))
        end
    end
end

local function Ready()
    local GM = FindLive("GameModeBase")
    if not GM or MapOf(GM) ~= HostedMap then
        SetPhase("ended", "match map unloaded (GameMode " .. (GM and MapOf(GM) or "-") .. ")")
        return
    end
    ApplyTickRate()
    ApplyLite()
    LiteWidgets()
    LootCheck()
    local OkLm, ErrLm = pcall(LootMap.Tick)
    if not OkLm then Log("[LOOTMAP] error: " .. tostring(ErrLm)) end
    local OkQs, ErrQs = pcall(QuestStats.Flush)
    if not OkQs then Log("[QUEST] error: " .. tostring(ErrQs)) end
    SampleTick(GM)
    AutoProbe()
    WatchClientHeroes()
    local OkHf, ErrHf = pcall(HeroFix.Tick)
    if not OkHf then Log("[HEROFIX] error: " .. tostring(ErrHf)) end
    -- resistance cap 80% (HeroFix.ResistCap) disabled: rolled back together with +50% resistance by the user's decision 07.11
    local Ok, Err = pcall(GameOverProbe)
    if not Ok then Log("[GAMEOVER] error: " .. tostring(Err)) end
    local OkWr, ErrWr = pcall(WaitingTick)
    if not OkWr then Log("[WAITING] error: " .. tostring(ErrWr)) end
    local OkH, ErrH = pcall(EnsureHost)
    if not OkH then Log("[OBSERVER] error: " .. tostring(ErrH)) end
    pcall(HookDamage)
    pcall(Wendigo.Tick)
    pcall(Spect.Hook)
    local OkFl, ErrFl = pcall(Spect.FloorTick)
    if not OkFl and Spect.FloorErr ~= tostring(ErrFl) then
        Spect.FloorErr = tostring(ErrFl)
        Log("[FLOOR] error: " .. Spect.FloorErr)
    end
    local OkGid, ErrGid = pcall(Spect.GameIdTick)
    if not OkGid and Spect.GameIdErr ~= tostring(ErrGid) then
        Spect.GameIdErr = tostring(ErrGid)
        Log("[GAMEID] error: " .. Spect.GameIdErr)
    end
    local OkOg, ErrOg = pcall(Spect.OptionGuard)
    if not OkOg and Spect.OptionGuardErr ~= tostring(ErrOg) then
        Spect.OptionGuardErr = tostring(ErrOg)
        Log("[MAP_GUARD] error: " .. Spect.OptionGuardErr)
    end
    local OkC, ErrC = pcall(Spect.Census)
    if not OkC then Log("[STARTS] error: " .. tostring(ErrC)) end
    local OkZ, ErrZ = pcall(Spect.ZoneTick)
    if not OkZ then Log("[ZONE] error: " .. tostring(ErrZ)) end
    -- 06.11 PVPVE diagnostics (dbserver-pvediag.txt = 0 — disable)
    if Spect.DiagOn == nil then Spect.DiagOn = not (ReadFile("dbserver-pvediag.txt") or ""):match("^%s*0") end
    if Spect.DiagOn then
        for _, F in ipairs({ Spect.ZoneHook, Spect.ZoneTrace, Spect.DiagTick, Spect.MobFallTick, Spect.ProxTick }) do
            local OkD, ErrD = pcall(F)
            if not OkD and Spect.DiagErr ~= tostring(ErrD) then
                Spect.DiagErr = tostring(ErrD)
                Log("[DIAG] error: " .. Spect.DiagErr)
            end
        end
    end
    local OkL, ErrL = pcall(LateObserverTick)
    if not OkL then Log("[OBSERVER] error: " .. tostring(ErrL)) end
end

-- 30.10: a console quit during ConnectServer hung the process on exit (alive, unkillable under ACE).
-- Hence: quit in the game thread, and if the process is alive after QUIT_FORCE_SEC — the dbnative watchdog (op=quitwatch, its own
-- C thread, 07.11) terminates the process. Before 07.11 this was os.exit from the LoopAsync thread.
local QUIT_FORCE_SEC = 10
local QuitAt = nil

local function GameQuit()
    local PC = FindLive("PlayerController")
    local Lib = KismetSystem()
    if Lib and PC then
        local Ok, Err = pcall(function() Lib:ExecuteConsoleCommand(PC, "quit", PC) end)
        Log("ExecuteConsoleCommand(quit) ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
    else
        Log("quit: no PlayerController/KismetSystemLibrary")
    end
end

-- --- Host commands (02.11): dev-app -> backend (admin API /admin/v1/host/command) -> <role>-control.txt -> here ---
-- Portals — the native UTgfCheatComponent (a PlayerController component), like LocalDev test_exit, but on the server:
--   exits                 — SpawnAllEscapeDevices + ActivateAllEscapeDevices (all exits of the map)
--   portal [open_id|all]  — a blue portal (ETgfPortalType::BluePortal = 5) 2 m in front of the player's hero
--   reaper [open_id|all]  — the Reaper monster (BP_TGF_Monster_Reaper_C) 4 m in front of the hero, facing him
--   hp [number|open_id]   — test: the hero's MaxHealthBase and CurHealth = number (default 99999)
--   kill <open_id|all>    — the hero's death by the native death zone GE (GE_TGF_Damage_DeathZone_InstantDeath, fallback —
--                           GE_TGF_Kill_Self) -> a regular death; after 5 s an exit on behalf of the dead player (without it the death
--                           result is not taken) -> record status 3 reason 2 -> "death" result
--   leave <open_id|all>   — exit on behalf of the player now: the native ServerSelfQuitGame (alive — SELF_QUIT, backend counts
--                           it as a death) and after 3 s ClientTravel to the lobby
-- Result — <role>-command-result.json {cmd, result, at} and a [HOSTCMD] line in the log.
-- --- where / levers (08.11, host commands) -----------------------------------------------------------------------
-- where — player coordinates, where the camera looks, map cell (4000 cm), nearest room, known floor points.
--   Cardinal directions — by calibration: "where north" — the player looks straight up the minimap, his camera angle
--   is saved to ru-north.txt next to the exe; then directions N/NE/E/… (UE yaw grows clockwise seen from above).
-- levers [radius|all] — levers, statues and doors on the server (bIsOn, mechanism, MoveBase); levers on <N> — press the N-th of
--   the last list from the server side (lever: TurnOn; mechanism: OnTriggered(1)). docs/SECRET_ROOM_BOSS.md.
-- Global tables: the main chunk has a 200-local limit.
Where = { North = nil }
Where.COMPASS = { "N", "NE", "E", "SE", "S", "SW", "W", "NW" }
-- known points (LOCAL analysis, docs/SECRET_ROOM_BOSS.md, docs/WENDIGO_DOOR.md)
Where.POINTS = {
    { Map = "L_Dungeon_Official_01", Name = "center (grates)", X = 0, Y = 0, Z = 0 },
    { Map = "L_Dungeon_Official_01", Name = "treasury chest", X = 4002, Y = -2903, Z = 491 },
    { Map = "L_Dungeon_Official_01", Name = "lever 1 (top)", X = 3665, Y = -1854, Z = 220 },
    { Map = "L_Dungeon_Official_01", Name = "lever 2 (bottom)", X = 4497, Y = -1791, Z = -86 },
    { Map = "L_Dungeon_Official_02", Name = "Wendigo door", X = 0, Y = -1340, Z = 4 },
}

function Where.LoadNorth()
    if Where.North == nil then
        local F = io.open("ru-north.txt", "r")
        Where.North = F and tonumber(F:read("*l") or "") or false
        if F then F:close() end
    end
    return Where.North or nil
end

function Where.Side(Yaw)
    local N = Where.LoadNorth()
    if not N then return "?" end
    local B = ((Yaw - N) % 360 + 360) % 360
    return Where.COMPASS[math.floor((B + 22.5) / 45) % 8 + 1]
end

function Where.Players()
    local Out = {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open, Pawn = "", nil
            pcall(function() Open = PS.MyOpenId:ToString() end)
            pcall(function() Pawn = PS.PawnPrivate end)
            if IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                local L, Yaw = nil, nil
                pcall(function() L = Pawn:K2_GetActorLocation() end)
                pcall(function() Yaw = Pawn:GetControlRotation().Yaw end)
                if Yaw == nil then pcall(function() Yaw = Pawn:K2_GetActorRotation().Yaw end) end
                if L then Out[#Out + 1] = { Open = Open, L = L, Yaw = Yaw or 0 } end
            end
        end
    end
    return Out
end

function Where.NearestRoom(L)
    local Best, BestD = "?", nil
    for _, R in ipairs(FindAllOf("BP_ActorSpawnRoom_C") or {}) do
        if IsValid(R) and not FullName(R):find("Default__", 1, true) then
            pcall(function()
                local P = R:K2_GetActorLocation()
                local D = math.sqrt((P.X - L.X) ^ 2 + (P.Y - L.Y) ^ 2)
                if not BestD or D < BestD then Best, BestD = (FullName(R):match("([^%.]+)$") or "?"), D end
            end)
        end
    end
    return Best, BestD
end

function Where.Run(Arg)
    local Ps = Where.Players()
    if #Ps == 0 then return "no players with a pawn" end
    if Arg == "north" then
        local Yaw = Ps[1].Yaw
        local F = io.open("ru-north.txt", "w")
        if not F then return "cannot write ru-north.txt" end
        F:write(string.format("%.1f\n", Yaw))
        F:close()
        Where.North = Yaw
        Log(string.format("[WHERE] north = camera angle %.1f (%s)", Yaw, Ps[1].Open))
        return string.format("north_set yaw=%.1f", Yaw)
    end
    local MapName = tostring(HostedMap):match("([^/]+)$") or "?"
    local Lines = {}
    for _, P in ipairs(Ps) do
        local L = P.L
        local Room, RoomD = Where.NearestRoom(L)
        local Parts = { string.format("%s (%.0f %.0f %.0f) view %.0f° %s cell (%d,%d) room %s (%.0f cm)", P.Open, L.X, L.Y, L.Z,
            P.Yaw, Where.Side(P.Yaw), math.floor(L.X / 4000 + 0.5), math.floor(L.Y / 4000 + 0.5), Room, RoomD or -1) }
        for _, Pt in ipairs(Where.POINTS) do
            if MapName:find(Pt.Map, 1, true) == 1 then
                local Dx, Dy = Pt.X - L.X, Pt.Y - L.Y
                local Dir = math.deg(math.atan(Dy, Dx))
                Parts[#Parts + 1] = string.format("%s: %.0f cm to the %s, height %+.0f", Pt.Name, math.sqrt(Dx * Dx + Dy * Dy), Where.Side(Dir), Pt.Z - L.Z)
            end
        end
        local Line = table.concat(Parts, "; ")
        Log("[WHERE] " .. MapName .. ": " .. Line)
        Lines[#Lines + 1] = Line
    end
    return (Where.LoadNorth() and "" or "north not set (where north); ") .. table.concat(Lines, " | ")
end

Levers = { Last = {} }
Levers.SWITCHES = { "BP_SwitchBase_C", "BP_Handle_01_C", "BP_Handle_02_C" }
Levers.MECHANISMS = { "BP_D19_Base_01_C", "BP_Metal_Door_04_C", "BP_ChainedDoor_02_C", "BP_ChainedDoor_03_C", "BP_EscapeMechanism_C",
    "BP_Mechanism_StoneDoor_Right_C", "BP_Mechanism_StoneDoor_Left_C" }

function Levers.State(A)
    local S = "bIsOn=" .. Wendigo.Field(A, "bIsOn")
    local Mech = Wendigo.Field(A, "ControlledMechanism")
    if Mech ~= "?" then S = S .. " mechanism=" .. Mech end
    pcall(function()
        local C = A.MoveBase
        if C and C:IsValid() then S = S .. " MoveBase.bIsOn=" .. tostring(C.bIsOn) .. " tick=" .. tostring(C:IsComponentTickEnabled()) end
    end)
    return S
end

function Levers.Run(Arg)
    local On = tonumber(Arg:match("^on%s+(%d+)"))
    if On then
        local E = Levers.Last[On]
        if not E or not IsValid(E.A) then return "no object " .. On .. " in the last list (run levers first)" end
        local Ok, Err
        if E.Kind == "lever" then
            Ok, Err = pcall(function() E.A:TurnOn() end)
            if not Ok then Ok, Err = pcall(function() E.A.ControlledMechanism:OnTriggered(1) end) end
        else
            Ok, Err = pcall(function() E.A:OnTriggered(1) end)
        end
        local Line = string.format("%d %s %s: %s; now %s", On, E.Kind, E.Name, Ok and "ok" or tostring(Err), Levers.State(E.A))
        Log("[LEVERS] press " .. Line)
        return Line
    end
    local Ps = Where.Players()
    local Center = Ps[1] and Ps[1].L
    local R = Arg == "all" and math.huge or (tonumber(Arg) or 3000)
    local List, Seen = {}, {}
    local function Add(Cls, Kind)
        for _, A in ipairs(FindAllOf(Cls) or {}) do
            if IsValid(A) and not FullName(A):find("Default__", 1, true) then
                local Key = FullName(A)
                if not Seen[Key] then
                    Seen[Key] = true
                    local L = { X = 0, Y = 0, Z = 0 }
                    pcall(function() L = A:K2_GetActorLocation() end)
                    local D = Center and math.sqrt((L.X - Center.X) ^ 2 + (L.Y - Center.Y) ^ 2 + (L.Z - Center.Z) ^ 2) or 0
                    if D <= R then List[#List + 1] = { A = A, Kind = Kind, Name = Key:match("([^%.]+)$") or "?", L = L, D = D } end
                end
            end
        end
    end
    for _, C in ipairs(Levers.SWITCHES) do Add(C, "lever") end
    for _, C in ipairs(Levers.MECHANISMS) do Add(C, "mechanism") end
    table.sort(List, function(X, Y) return X.D < Y.D end)
    Levers.Last = List
    local NS, NM = 0, 0
    for I, E in ipairs(List) do
        if E.Kind == "lever" then NS = NS + 1 else NM = NM + 1 end
        Log(string.format("[LEVERS] %d %s %s (%.0f %.0f %.0f) %.0f cm: %s", I, E.Kind, E.Name, E.L.X, E.L.Y, E.L.Z, E.D, Levers.State(E.A)))
    end
    return string.format("levers %d, mechanisms %d (%s); list — [LEVERS] lines in the log, press: levers on <N>", NS, NM,
        R == math.huge and "all" or string.format("within %d cm of the player", R))
end

local function CommandResult(Cmd, Result)
    Log("[HOSTCMD] " .. Cmd .. " -> " .. Result)
    WriteFile(Role .. Inst .. "-command-result.json", string.format('{"cmd":"%s","result":"%s","at":"%s"}',
        (Cmd:gsub('"', "'")), (Result:gsub('"', "'")), os.date("%Y-%m-%d %H:%M:%S")))
end

local function CheatComponent()
    for _, C in ipairs(FindAllOf("TgfCheatComponent") or {}) do
        if IsValid(C) and not FullName(C):find("Default__", 1, true) then return C end
    end
    return nil
end

local function HostCommand(Verb, Arg)
    local Cmd = Verb .. (Arg ~= "" and (" " .. Arg) or "")
    if Phase ~= "ready" then return CommandResult(Cmd, "rejected_not_ready phase=" .. Phase) end
    -- 09.11: ban <open_id> — the native "Hand of Justice": the event TGF.GameplayEvent.TriggerSelfDestruct on the player's hero
    -- starts GA_TGF_Hero_Self_Destruct (sanction effect, marker TGF.GamePlay.DeathForCheating, equipment and
    -- inventory are cleared, death, ServerBanQuitGame); the kill feed BP_TgfGame_MapHint on the same event shows
    -- a kill of type BanPlayer (GM judge icon). research/gmjudge.
    if Verb == "ban" then
        if Arg == "" then return CommandResult(Cmd, "rejected: open_id required") end
        local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
        local Notes, Done = {}, 0
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if Open == Arg then
                    if not IsValid(Pawn) then
                        Notes[#Notes + 1] = Open .. " without a hero"
                    else
                        local Ok, Err = pcall(function()
                            local Tag = { TagName = FName("TGF.GameplayEvent.TriggerSelfDestruct") }
                            Lib:SendGameplayEventToActor(Pawn, Tag, { EventTag = Tag, Instigator = Pawn, Target = Pawn, EventMagnitude = 1.0 })
                        end)
                        if Ok then Done = Done + 1 end
                        Notes[#Notes + 1] = Open .. (Ok and " event sent" or (" error " .. tostring(Err)))
                    end
                end
            end
        end
        if #Notes == 0 then Notes[1] = Arg .. " not found on the server" end
        Log("[HOSTCMD] ban: " .. table.concat(Notes, "; "))
        return CommandResult(Cmd, string.format("ban_sent=%d [%s]", Done, table.concat(Notes, "; ")))
    end
    if Verb == "leave" then
        if Arg == "" then return CommandResult(Cmd, "rejected: open_id or all required") end
        local N, Quit = LeavePlayers(Arg, "Вы покинули рейд")
        return CommandResult(Cmd, string.format("self_quit=%d/%d lobby_in=%ds", Quit, N, LEAVE_TRAVEL_SEC))
    end
    -- killstay — like kill, but without the automatic exit for the dead player (03.11: check "died connected, then disconnected")
    local Stay = Verb == "killstay"
    if Stay then Verb = "kill" end
    if Verb == "kill" then
        if Arg == "" then return CommandResult(Cmd, "rejected: open_id or all required") end
        -- 02.11: UTgfAttributeComponent::DamageSelfDestruct deals no damage in shipping (neither false nor true: hp 1020->1020).
        -- Native path — the death zone's instant death GE (that is how the game itself kills outside the zone), fallback — GE_TGF_Kill_Self:
        -- the hero's ASC (or its PlayerState) -> BP_ApplyGameplayEffectToSelf(GE, 1, MakeEffectContext()). Then a regular
        -- death -> PlayerGameDeadPlayerState -> "death" result. The response says which GE worked and HP before/after.
        local GES = {
            "/Game/TGF/Ability/GameplayEffect/Environment/DeathZone/GE_TGF_Damage_DeathZone_InstantDeath.GE_TGF_Damage_DeathZone_InstantDeath_C",
            "/Game/TGF/Ability/GameplayEffect/Buff/BuffArg/GE_TGF_Kill_Self.GE_TGF_Kill_Self_C",
        }
        local function GeClass(Path)
            local C = nil
            pcall(function() C = StaticFindObject(Path) end)
            if not (C and C:IsValid()) then
                pcall(function() LoadAsset(Path:match("^(.-)%.[^%.]+$")) end)
                pcall(function() C = StaticFindObject(Path) end)
            end
            return C and C:IsValid() and C or nil
        end
        local AttrCls = StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
        local DefCls = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
        local Notes = {}
        local Done, Dead, Errors = 0, 0, {}
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if (Arg == "all" or Arg == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                    local Ok, Err = pcall(function()
                        local Attr = Pawn:GetComponentByClass(AttrCls)
                        local function Hp()
                            local V = -1
                            pcall(function() V = Attr:GetAttributeSet(DefCls).CurHealth.CurrentValue end)
                            return V
                        end
                        local function IsDead()
                            local D = false
                            pcall(function() D = Pawn:IsDeadOrDying() == true end)
                            return D
                        end
                        local Asc = nil
                        pcall(function() Asc = Pawn:GetLyraAbilitySystemComponent() end)
                        if not IsValid(Asc) then pcall(function() Asc = PS:GetLyraAbilitySystemComponent() end) end
                        if not IsValid(Asc) then pcall(function() Asc = PS.AbilitySystemComponent end) end
                        if not IsValid(Asc) then error("no AbilitySystemComponent") end
                        local Before, Used = Hp(), {}
                        for _, Path in ipairs(GES) do
                            if IsDead() then break end
                            local Ge = GeClass(Path)
                            local Name = Path:match("([^%.]+)_C$") or Path
                            if not Ge then
                                Used[#Used + 1] = Name .. ":no class"
                            else
                                local OkGe, ErrGe = pcall(function() Asc:BP_ApplyGameplayEffectToSelf(Ge, 1.0, Asc:MakeEffectContext()) end)
                                Used[#Used + 1] = string.format("%s:%s hp=%.0f", Name, OkGe and "ok" or ("error " .. tostring(ErrGe)), Hp())
                            end
                        end
                        Notes[#Notes + 1] = string.format("%s hp %.0f->%.0f dead=%s (%s)", Open, Before, Hp(), tostring(IsDead()),
                            table.concat(Used, ", "))
                    end)
                    if Ok then
                        Done = Done + 1
                        local IsDead = false
                        pcall(function() IsDead = Pawn:IsDeadOrDying() == true end)
                        if IsDead then
                            Dead = Dead + 1
                        end
                        if IsDead and not Stay then
                            GoProbe.LeaveAt = GoProbe.LeaveAt or {}
                            GoProbe.LeaveAt[Open] = os.time() + KILL_LEAVE_SEC
                        end
                    else
                        Errors[#Errors + 1] = Open .. ": " .. tostring(Err)
                    end
                end
            end
        end
        return CommandResult(Cmd, string.format("applied=%d dead_now=%d leave_in=%ds [%s]%s", Done, Dead, KILL_LEAVE_SEC, table.concat(Notes, "; "),
            #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
    end
    -- 05.11: hp [number|open_id] — for tests: for the hero (of all connected players or of open_id) MaxHealthBase and CurHealth
    -- TgfDefendAttributeSet = number (default 99999). An argument of up to 9 digits is a number, longer — open_id (then 99999).
    -- We write BaseValue and CurrentValue of the attribute right on the server (replicated to the client); StatWatch stats will diverge.
    -- 06.11 weapon [number|open_id] — test: the hero's weapon attack (default 5000), does not change items, lasts until the end of the match or a weapon change
    if Verb == "weapon" then
        local Value, Who = 5000, "all"
        if Arg:match("^%d+$") and #Arg <= 9 then Value = tonumber(Arg) elseif Arg ~= "" then Who = Arg end
        local AttrCls = StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
        local AtkCls = StaticFindObject("/Script/LyraGame.TgfAttackAttributeSet")
        local Done, Notes = 0, {}
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if (Who == "all" or Who == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                    local Set = Pawn:GetComponentByClass(AttrCls):GetAttributeSet(AtkCls)
                    for _, Name in ipairs({ "RightWeaponAttack", "LeftWeaponAttack" }) do
                        pcall(function()
                            local A = Set[Name]
                            local Before = A.CurrentValue
                            A.BaseValue = Value
                            A.CurrentValue = Value
                            Notes[#Notes + 1] = string.format("%s %.0f->%.0f", Name, Before, A.CurrentValue)
                            Done = Done + 1
                        end)
                    end
                end
            end
        end
        return CommandResult(Cmd, string.format("weapon_set=%d value=%d [%s]", Done, Value, table.concat(Notes, "; ")))
    end
    -- 06.11 mana [number|open_id] — test: the hero's mana (default 999999): CurMagic and MaxMagicBase (mappings), plus nature and soul energy
    if Verb == "mana" then
        local Value, Who = 999999, "all"
        if Arg:match("^%d+$") and #Arg <= 9 then Value = tonumber(Arg) elseif Arg ~= "" then Who = Arg end
        local AttrCls = StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
        local Done, Notes = 0, {}
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if (Who == "all" or Who == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                    local Comp = Pawn:GetComponentByClass(AttrCls)
                    for _, SetName in ipairs({ "TgfMagicAttributeSet", "TgfNaturalEnergyAttributeSet", "TgfSoulEnergyAttributeSet" }) do
                        local Set = nil
                        pcall(function() Set = Comp:GetAttributeSet(StaticFindObject("/Script/LyraGame." .. SetName)) end)
                        if IsValid(Set) then
                            for _, Name in ipairs({ "MaxMagicBase", "CurMagic", "MaxNaturalEnergyBase", "CurNaturalEnergy", "MaxSoulEnergyBase", "CurSoulEnergy" }) do
                                pcall(function()
                                    local A = Set[Name]
                                    local Before = A.CurrentValue
                                    A.BaseValue = Value
                                    A.CurrentValue = Value
                                    Notes[#Notes + 1] = string.format("%s.%s %.0f->%.0f", SetName, Name, Before, A.CurrentValue)
                                    Done = Done + 1
                                end)
                            end
                        end
                    end
                end
            end
        end
        return CommandResult(Cmd, string.format("mana_set=%d value=%d [%s]", Done, Value, table.concat(Notes, "; ")))
    end
    if Verb == "hp" then
        local Value, Who = 99999, "all"
        if Arg:match("^%d+$") and #Arg <= 9 then Value = tonumber(Arg) elseif Arg ~= "" then Who = Arg end
        local AttrCls = StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
        local DefCls = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
        local Done, Notes, Errors = 0, {}, {}
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if (Who == "all" or Who == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                    local Ok, Err = pcall(function()
                        local Set = Pawn:GetComponentByClass(AttrCls):GetAttributeSet(DefCls)
                        if not IsValid(Set) then error("no TgfDefendAttributeSet") end
                        local Before = Set.CurHealth.CurrentValue
                        Set.MaxHealthBase.BaseValue = Value
                        Set.MaxHealthBase.CurrentValue = Value
                        Set.CurHealth.BaseValue = Value
                        Set.CurHealth.CurrentValue = Value
                        Notes[#Notes + 1] = string.format("%s hp %.0f->%.0f max_base=%.0f", Open, Before, Set.CurHealth.CurrentValue,
                            Set.MaxHealthBase.CurrentValue)
                    end)
                    if Ok then Done = Done + 1 else Errors[#Errors + 1] = Open .. ": " .. tostring(Err) end
                end
            end
        end
        return CommandResult(Cmd, string.format("hp_set=%d value=%d [%s]%s", Done, Value, table.concat(Notes, "; "),
            #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
    end
    -- 07.11 resist <share> — cap test: for all player heroes all resistances = share (CurrentValue), e.g. resist 0.95;
    -- a second later HeroFix.ResistCap must cut it to 0.8 ([RESISTCAP] in the log)
    if Verb == "resist" then
        local Value = tonumber(Arg) or 0.95
        local DefCls = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
        local Done = 0
        for _, P in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
            if IsValid(P) and not FullName(P):find("Default__", 1, true) then
                pcall(function()
                    local D = P.AttributeComponent:GetAttributeSet(DefCls)
                    for _, N in ipairs({ "None", "Fire", "Ice", "Thunder", "Light", "Dark" }) do D["ElementResistanceRate" .. N].CurrentValue = Value end
                    Done = Done + 1
                end)
            end
        end
        HeroFix.ResistLogged = {}
        return CommandResult(Cmd, string.format("resist_set=%d value=%.2f", Done, Value))
    end
    -- 06.11 escapedevices [red|blue] — the native SpawnImmediately on BP_EscapeDeviceSpawnPoint points (check: do they place devices)
    if Verb == "escapedevices" then
        local Want = Arg:lower()
        local Done, Skipped, Errors = 0, 0, {}
        for _, P in ipairs(FindAllOf("BP_EscapeDeviceSpawnPoint_C") or {}) do
            if IsValid(P) and not FullName(P):find("Default__", 1, true) then
                local N = "?"
                pcall(function() N = Spect.ObjName(P.DataActor) end)
                if N == "None" or N == "?" then pcall(function() N = Spect.ObjName(P.ActorToSpawn) end) end
                local Red = N:find("Red", 1, true) ~= nil
                if Want == "" or (Want == "red" and Red) or (Want == "blue" and not Red) then
                    local Ok, Err = pcall(function() P:SpawnImmediately() end)
                    if Ok then Done = Done + 1 elseif #Errors < 2 then Errors[#Errors + 1] = tostring(Err) end
                else
                    Skipped = Skipped + 1
                end
            end
        end
        Spect.DiagExits()
        return CommandResult(Cmd, string.format("called=%d skipped=%d%s", Done, Skipped, #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
    end
    -- 06.11 floorspawn <disk> — on the second floor place the disk's portals by the first floor rules now (check)
    if Verb == "floorspawn" then
        local Zone = FindLive("BP_DeathZone_C")
        if not (Zone and Spect.OnFloor2()) then return CommandResult(Cmd, "not the second floor or no rules") end
        return CommandResult(Cmd, "placed " .. tostring(Spect.FloorSpawn(Zone, tonumber(Arg) or 2)))
    end
    -- 06.11 gothrone — to the throne arena with an item snapshot (check); thronetp [X,Y,Z] — all players to the throne or to the given point
    if Verb == "gothrone" then
        return CommandResult(Cmd, tostring(Spect.FloorTravel("command", Spect.ThroneMap)))
    end
    if Verb == "thronetp" then
        local X, Y, Z = Arg:match("(-?%d+),(-?%d+),(-?%d+)")
        local To = nil
        if X then
            To = { X = tonumber(X), Y = tonumber(Y), Z = tonumber(Z) }
        else
            local T = FindLive("BP_Throne_ShadowKing_C")
            if not T then return CommandResult(Cmd, "no throne: give X,Y,Z") end
            local L = T:K2_GetActorLocation()
            To = { X = L.X + 800, Y = L.Y, Z = L.Z + 200 }
        end
        local Done = 0
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Pawn = nil
                pcall(function() Pawn = PS.PawnPrivate end)
                if IsValid(Pawn) and pcall(function() Pawn:K2_TeleportTo(To, { Pitch = 0, Yaw = 0, Roll = 0 }) end) then
                    Done = Done + 1
                    -- Shadow scene: the death zone skips such players, and the arena is far outside its circle
                    pcall(function() PS:SetPlayerSceneTag(1) end)
                end
            end
        end
        return CommandResult(Cmd, string.format("moved %d to (%.0f %.0f %.0f)", Done, To.X, To.Y, To.Z))
    end
    -- 06.11 redroll — all live blue devices become red now, without a roll (check)
    if Verb == "redroll" then
        if not Spect.HasFloor() then return CommandResult(Cmd, "the map has no second floor") end
        return CommandResult(Cmd, "became red: " .. tostring(Spect.RedTick(true)))
    end
    -- 07.11 itemteams — check of a teammate's equipment badge: players' MyTeamID and items' (role, FactionId) into [TEAMCHECK] lines
    if Verb == "itemteams" then
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                pcall(function()
                    Log(string.format("[TEAMCHECK] player %s MyTeamID=%s", PS.MyOpenId:ToString(), tostring(PS:GetTeamId())))
                end)
            end
        end
        local Seen, Total = {}, 0
        for _, I in ipairs(FindAllOf("TgfInventoryItemInstance") or {}) do
            if IsValid(I) and not FullName(I):find("Default__", 1, true) then
                pcall(function()
                    local Key = string.format("role=%s FactionId=%s", tostring(I.RoleGuid), tostring(I.FactionId))
                    Seen[Key] = (Seen[Key] or 0) + 1
                    Total = Total + 1
                end)
            end
        end
        for Key, N in pairs(Seen) do Log(string.format("[TEAMCHECK] items %d: %s", N, Key)) end
        return CommandResult(Cmd, "items " .. tostring(Total))
    end
    -- 07.11 portalmsg [Blue|Red|Shadow|Grey] — the native portal message to all players (check)
    if Verb == "portalmsg" then
        local Kind = Arg ~= "" and Arg or "Red"
        local Zone = FindLive("BP_DeathZone_C")
        if not Zone or not Spect.PortalKinds[Kind] then return CommandResult(Cmd, "no zone or unknown type") end
        return CommandResult(Cmd, "message " .. tostring(Spect.PortalSend(Zone, Kind, 1)))
    end
    -- 06.11 nextfloor — transition to the next floor now, with an item snapshot of all players (check without a portal).
    if Verb == "nextfloor" then
        return CommandResult(Cmd, tostring(Spect.FloorTravel("command")))
    end
    -- 06.11 PVPVE diagnostics. diag — floor, options, rooms now.
    -- wendigo [kill|open] — Wendigo door probe (temporary, 08.11): state; kill — the native instant death GE for the monster
    -- (then the game itself: death -> mechanism); open — OnTriggered(1) on the leaves manually (check of the fallback fix)
    if Verb == "wendigo" then
        local Res = {}
        if Arg == "kill" or Arg == "killnolink" then
            -- killnolink — insurance check: before the death erase the monster's link to the door (as if the spawn did not pass it)
            if Arg == "killnolink" then
                for _, M in ipairs(FindAllOf("BP_Tgf_Monster_Wendigo_C") or {}) do
                    if IsValid(M) and not FullName(M):find("Default__", 1, true) then
                        Res[#Res + 1] = "link " .. (pcall(function() M.DeathTriggerMechanism = nil end) and "cleared" or "error")
                    end
                end
            end
            local Path = "/Game/TGF/Ability/GameplayEffect/Environment/DeathZone/GE_TGF_Damage_DeathZone_InstantDeath.GE_TGF_Damage_DeathZone_InstantDeath_C"
            local Cls = Spect.LoadClass(Path)
            local Lib = StaticFindObject("/Script/GameplayAbilities.Default__AbilitySystemBlueprintLibrary")
            for _, M in ipairs(FindAllOf("BP_Tgf_Monster_Wendigo_C") or {}) do
                if IsValid(M) and not FullName(M):find("Default__", 1, true) then
                    local Ok, Err = pcall(function()
                        local Asc = Lib:GetAbilitySystemComponent(M)
                        Asc:BP_ApplyGameplayEffectToSelf(Cls, 1, Asc:MakeEffectContext())
                    end)
                    Res[#Res + 1] = "kill " .. (Ok and "ok" or tostring(Err))
                end
            end
            if #Res == 0 then Res[1] = "no Wendigo" end
        elseif Arg == "open" then
            for _, D in ipairs(Wendigo.Doors()) do
                local Ok, Err = pcall(function() D:OnTriggered(1) end)
                Res[#Res + 1] = "open " .. (Ok and "ok" or tostring(Err))
            end
            if #Res == 0 then Res[1] = "no door leaves" end
            Wendigo.WatchUntil = os.time() + 10
        end
        local N = Wendigo.Dump("command " .. (Arg ~= "" and Arg or "state"))
        return CommandResult(Cmd, table.concat(Res, ", ") .. (#Res > 0 and "; " or "") .. N .. " [WENDIGO] lines in the log")
    end
    if Verb == "where" then return CommandResult(Cmd, Where.Run(Arg)) end
    if Verb == "levers" then return CommandResult(Cmd, Levers.Run(Arg)) end
    if Verb == "diag" then
        Spect.DiagFloor()
        if Spect.HasFloor() then Spect.FloorCapture() end
        Spect.DiagRooms("start")
        Spect.DiagExits()
        return CommandResult(Cmd, "see the [DIAG] lines in the log")
    end
    -- 09.11 nextdisc [sec] — portal check: the end time of the zone's current state (pause / interval / shrinking)
    -- is moved sec back (default 3600) — on the next tick the zone moves on by itself, with its own portal
    -- spawn. One command — one step (interval -> shrinking -> next disk).
    if Verb == "nextdisc" then
        local Zone = FindLive("BP_DeathZone_C")
        if not Zone then return CommandResult(Cmd, "no_zone") end
        local Shift = tonumber(Arg) or 3600
        local Parts = {}
        for _, K in ipairs({ "Current Pause End Time", "Current Interval End Time", "Current Round End Time" }) do
            local Ok, Err = pcall(function()
                local V = Zone[K]
                Zone[K] = V - Shift
                Parts[#Parts + 1] = string.format("%s %.0f->%.0f", K:match("Current (%a+)"), V, V - Shift)
            end)
            if not Ok then Parts[#Parts + 1] = K .. " error " .. tostring(Err) end
        end
        local Disc, State = "?", "?"
        pcall(function() Disc = tostring(Zone["Current Disc Index"] + 1) end)
        pcall(function() State = tostring(Zone.State) end)
        return CommandResult(Cmd, string.format("disk %s state %s: %s", Disc, State, table.concat(Parts, "; ")))
    end
    -- zonetest [radius] — shrink the zone radius for 25 s (default 100 cm): players end up outside it without waiting for the shrink.
    if Verb == "zonetest" then
        local Zone = FindLive("BP_DeathZone_C")
        if not Zone then return CommandResult(Cmd, "no_zone") end
        local Old = Zone["Dynamic Radius"]
        Spect.ZoneRestore = { R = (Spect.ZoneRestore and Spect.ZoneRestore.R) or Old, At = os.time() + 25 }
        Zone["Dynamic Radius"] = tonumber(Arg) or 100
        return CommandResult(Cmd, string.format("radius %.0f -> %.0f for 25 s", Old, Zone["Dynamic Radius"]))
    end
    -- spawnrooms — check "all mobs at once": the native proximity event for all rooms that have not spawned yet (as if a player
    -- came close). Then [MOBFALL]: mobs after 3 and 23 s, who fell and who vanished. Frame time — [TICK].
    if Verb == "spawnrooms" then
        local Done, Skipped, Errors = 0, 0, {}
        local Evt = "BndEvt__BP_ActorSpawnRoom_PlayerDistanceChecker_K2Node_ComponentBoundEvent_1_Event_SomePlayerComesClose__DelegateSignature"
        local T0 = os.clock()
        for _, R in ipairs(FindAllOf("BP_ActorSpawnRoom_C") or {}) do
            if IsValid(R) and not FullName(R):find("Default__", 1, true) then
                if R.bFirstTimeActuallySpawn and not R.bSpawnWhenPhaseStarts then
                    local Ok, Err = pcall(function()
                        R.PlayerDistanceChecker:SetComponentTickEnabled(false)
                        R[Evt](R, R:K2_GetActorLocation())
                    end)
                    if Ok then Done = Done + 1 elseif #Errors < 3 then Errors[#Errors + 1] = tostring(Err) end
                else
                    Skipped = Skipped + 1
                end
            end
        end
        Spect.MobFall = { Step = 1, At = os.time() + 3, Delay = 3 }
        return CommandResult(Cmd, string.format("rooms started=%d skipped=%d in %.0f ms%s", Done, Skipped, (os.clock() - T0) * 1000,
            #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
    end
    -- 05.11: reaper [open_id|all] — the Reaper monster 4 m in front of the hero (check of the JesterSwap skin on the client).
    -- GameplayStatics BeginDeferredActorSpawnFromClass + FinishSpawningActor; without a controller — SpawnDefaultController.
    -- Everything inside HostCommand: the main block of RuServer already has 200 locals (Lua limit).
    if Verb == "reaper" then
        local Path = "/Game/TGF/Character/Monster/M_Reaper/BP_TGF_Monster_Reaper.BP_TGF_Monster_Reaper_C"
        local Cls = nil
        pcall(function() Cls = StaticFindObject(Path) end)
        if not (Cls and Cls:IsValid()) then
            pcall(function() LoadAsset("/Game/TGF/Character/Monster/M_Reaper/BP_TGF_Monster_Reaper") end)
            pcall(function() Cls = StaticFindObject(Path) end)
        end
        if not (Cls and Cls:IsValid()) then return CommandResult(Cmd, "no_class " .. Path) end
        local GS = StaticFindObject("/Script/Engine.Default__GameplayStatics")
        local Done, Errors = 0, {}
        for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
            if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
                local Open, Pawn = "", nil
                pcall(function() Open = PS.MyOpenId:ToString() end)
                pcall(function() Pawn = PS.PawnPrivate end)
                if (Arg == "" or Arg == "all" or Arg == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                    local Ok, Err = pcall(function()
                        local P, F = Pawn:K2_GetActorLocation(), Pawn:GetActorForwardVector()
                        local Yaw = math.atan(F.Y, F.X) + math.pi -- facing the hero
                        local Tr = {
                            Rotation = { X = 0, Y = 0, Z = math.sin(Yaw / 2), W = math.cos(Yaw / 2) },
                            Translation = { X = P.X + F.X * 400, Y = P.Y + F.Y * 400, Z = P.Z + 20 },
                            Scale3D = { X = 1, Y = 1, Z = 1 },
                        }
                        local A = nil
                        -- 2 = AdjustIfPossibleButAlwaysSpawn; UE 5.1+ also has TransformScaleMethod — try both signatures
                        local OkB = pcall(function() A = GS:BeginDeferredActorSpawnFromClass(Pawn, Cls, Tr, 2, nil) end)
                        if not (OkB and A and A:IsValid()) then
                            pcall(function() A = GS:BeginDeferredActorSpawnFromClass(Pawn, Cls, Tr, 2, nil, 1) end)
                        end
                        if not (A and A:IsValid()) then error("BeginDeferredActorSpawnFromClass returned nil") end
                        if not pcall(function() GS:FinishSpawningActor(A, Tr) end) then
                            pcall(function() GS:FinishSpawningActor(A, Tr, 1) end)
                        end
                        local Ctrl = nil
                        pcall(function() Ctrl = A.Controller end)
                        if not IsValid(Ctrl) then pcall(function() A:SpawnDefaultController() end) end
                        pcall(function() Ctrl = A.Controller end)
                        Log(string.format("[REAPER] %s: %s ctrl=%s", Open, FullName(A), IsValid(Ctrl) and FullName(Ctrl) or "none"))
                    end)
                    if Ok then Done = Done + 1 else Errors[#Errors + 1] = Open .. ": " .. tostring(Err) end
                end
            end
        end
        return CommandResult(Cmd, string.format("reapers_spawned=%d%s", Done, #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
    end
    local Comp = CheatComponent()
    if not Comp then return CommandResult(Cmd, "no_cheat_component") end
    local World = nil
    pcall(function() World = Comp:GetWorld() end)
    if not IsValid(World) then return CommandResult(Cmd, "no_world") end
    if Verb == "exits" then
        local Ok, Err = pcall(function()
            Comp:SpawnAllEscapeDevices(World)
            Comp:ActivateAllEscapeDevices(World)
        end)
        return CommandResult(Cmd, Ok and "exits_spawned_and_activated" or ("error " .. tostring(Err)))
    end
    -- portal: by the heroes of connected players (open_id from BP_TGF_PlayerState_C.MyOpenId).
    -- 02.11: SpawnOnePortal of the cheat component "successfully" places nothing (the cheat bodies are cut in shipping) —
    -- check via UTgfPortalManagerSubsystem:GetAvailablePortalNums(5), fallback — the game's
    -- ADeathZone:SpawnOnePortalByType(5, FTransform), which the death zone itself uses to place portals.
    -- 06.11: redportal / shadowportal — the same path, ETgfPortalType: 5 blue, 6 red, 7 purple (Shadow)
    local Mgr = FindLive("TgfPortalManagerSubsystem")
    -- spawnportal <name|number> — any type in front of each player: blue red shadow grey (portals 5–8), bluedevice reddevice
    -- shadowdevice greydevice (devices 1–4, they must be activated), fixedblue fixedred (9, 10)
    local PortalType = (Verb == "redportal" and 6) or (Verb == "shadowportal" and 7) or 5
    if Verb == "spawnportal" then
        local Types = { bluedevice = 1, reddevice = 2, shadowdevice = 3, greydevice = 4, blue = 5, red = 6, shadow = 7, grey = 8, fixedblue = 9, fixedred = 10 }
        PortalType = Types[Arg:lower()] or tonumber(Arg)
        if not PortalType or PortalType < 1 or PortalType > 10 then
            return CommandResult(Cmd, "rejected: type blue red shadow grey bluedevice reddevice shadowdevice greydevice fixedblue fixedred or 1..10")
        end
        Arg = "all"
    end
    local function Count()
        local N = -1
        if Mgr then pcall(function() N = Mgr:GetAvailablePortalNums(PortalType) end) end
        return N
    end
    local Zone = FindLive("BP_DeathZone_C") or FindLive("DeathZone")
    local Done, Errors, Ways = 0, {}, {}
    for _, PS in ipairs(FindAllOf("BP_TGF_PlayerState_C") or {}) do
        if IsValid(PS) and not FullName(PS):find("Default__", 1, true) then
            local Open = ""
            pcall(function() Open = PS.MyOpenId:ToString() end)
            local Pawn = nil
            pcall(function() Pawn = PS.PawnPrivate end)
            if (Arg == "" or Arg == "all" or Arg == Open) and IsValid(Pawn) and Open ~= "" and Open ~= "LOCAL_GAME_MODE_OPENID" then
                local Ok, Err = pcall(function()
                    local P, F = Pawn:K2_GetActorLocation(), Pawn:GetActorForwardVector()
                    -- the center of the hero's capsule is ~90 cm above the floor
                    local At = { X = P.X + F.X * 200, Y = P.Y + F.Y * 200, Z = P.Z - 80 }
                    local Before = Count()
                    pcall(function() Comp:SpawnOnePortal(PortalType, At) end)
                    if Count() > Before then Ways[#Ways + 1] = "cheat"; return end
                    if not Zone then error("cheat did not place a portal, DeathZone not found") end
                    Zone:SpawnOnePortalByType(PortalType, {
                        Rotation = { X = 0, Y = 0, Z = 0, W = 1 }, Translation = At, Scale3D = { X = 1, Y = 1, Z = 1 } })
                    local After = Count()
                    -- devices count as available only after activation — for them the counter is not a check
                    if After <= Before and PortalType >= 5 then error(string.format("DeathZone: portals %d -> %d", Before, After)) end
                    Ways[#Ways + 1] = "deathzone"
                end)
                if Ok then Done = Done + 1 else Errors[#Errors + 1] = Open .. ": " .. tostring(Err) end
            end
        end
    end
    if Done > 0 and PortalType == 5 then pcall(function() Comp:ActivateAllEscapeDevices(World) end) end
    CommandResult(Cmd, string.format("portals_spawned=%d via=%s type=%d portals_now=%d%s", Done, table.concat(Ways, ","), PortalType, Count(),
        #Errors > 0 and (" errors=" .. table.concat(Errors, "; ")) or ""))
end
RuHostCommand = HostCommand -- 09.11: for commands from the /ds/slot response (SlotTick earlier in the file)

-- Control file. 07.11: called from the main loop in the game thread (before — from the LoopAsync service thread,
-- docs/CRASH_LONG_PLAY.md); the name is unchanged. Commands with UObjects still go via ExecuteInGameThread (one frame later).
local function CheckControlAsync()
    if QuitAt then
        -- fallback: the dbnative watchdog did not arm. In the game thread: if the thread hung, we never get here
        if not Standby.QuitWatch and os.time() - QuitAt >= QUIT_FORCE_SEC then
            Log("quit: process alive " .. QUIT_FORCE_SEC .. " s after quit — os.exit")
            if LogFile then LogFile:close(); LogFile = nil end
            os.exit(0, true)
        end
        return
    end
    local Control = Role .. Inst .. "-control.txt"
    local Text = ReadFile(Control)
    -- 05.11: PowerShell `echo x > file` writes UTF-16LE with a BOM (ff fe), Set-Content -Encoding UTF8 — UTF-8 with a BOM:
    -- strip the BOM and zero bytes, otherwise the command is not recognised and stays in the file
    if Text then Text = Text:gsub("^\239\187\191", ""):gsub("^\255\254", ""):gsub("\0", "") end
    local HostVerb, HostArg = nil, ""
    if Text then HostVerb, HostArg = Text:match("^%s*(%a+)%s*(%S*)") end
    -- where / levers: the argument is the whole line after the command ("levers on 3")
    if HostVerb == "where" or HostVerb == "levers" then HostArg = Text:match("^%s*%a+%s*(.-)%s*$") or "" end
    if HostVerb == "portal" or HostVerb == "exits" or HostVerb == "kill" or HostVerb == "killstay" or HostVerb == "leave" or HostVerb == "reaper" or HostVerb == "hp" or HostVerb == "mana" or HostVerb == "weapon" or HostVerb == "ban"
        or HostVerb == "diag" or HostVerb == "zonetest" or HostVerb == "nextdisc" or HostVerb == "spawnrooms" or HostVerb == "redportal" or HostVerb == "shadowportal" or HostVerb == "spawnportal" or HostVerb == "gothrone" or HostVerb == "thronetp" or HostVerb == "redroll" or HostVerb == "nextfloor" or HostVerb == "floorspawn" or HostVerb == "escapedevices" or HostVerb == "portalmsg" or HostVerb == "itemteams" or HostVerb == "resist" or HostVerb == "wendigo"
        or HostVerb == "where" or HostVerb == "levers" then
        WriteFile(Control, "")
        ExecuteInGameThread(function()
            local Ok, Err = pcall(HostCommand, HostVerb, HostArg or "")
            if not Ok then CommandResult(HostVerb, "error " .. tostring(Err)) end
        end)
        return
    end
    local WrVerb, WrA1, WrA2 = nil, nil, nil
    if Text then WrVerb, WrA1, WrA2 = Text:match("^%s*(wr_%a+)%s*(%S*)%s*(%S*)") end
    if WrVerb then
        WriteFile(Control, "")
        ExecuteInGameThread(function()
            local Ok, Res = pcall(WaitingCommand, WrVerb, WrA1, WrA2)
            Log("[WAITING] " .. WrVerb .. " " .. (WrA1 or "") .. " " .. (WrA2 or "") .. ": " .. tostring(Res))
            CommandResult(WrVerb, tostring(Res))
        end)
        return
    end
    -- 03.11: exec <console command> — for autotests (e.g. disconnect for the client — a drop in the middle of a match)
    local ExecCmd = Text and Text:match("^%s*exec%s+([^%c]+)")
    if ExecCmd then
        WriteFile(Control, "")
        ExecuteInGameThread(function()
            local PC = FindLive("PlayerController")
            local Ok, Err = pcall(function() KismetSystem():ExecuteConsoleCommand(PC, ExecCmd, PC) end)
            Log("control: exec " .. ExecCmd .. " ok=" .. tostring(Ok) .. (Ok and "" or (" " .. tostring(Err))))
        end)
        return
    end
    if Text and Text:match("^%s*probe") then
        WriteFile(Control, "")
        Log("control: probe")
        ExecuteInGameThread(function()
            local Ok, Err = pcall(Probe, "control")
            if not Ok then Log("probe error: " .. tostring(Err)) end
        end)
        return
    end
    -- 31.10 experiment (b): relaunch[:<map>] — a repeated BattleService:StartLocalBattle in the middle of a match: will the game
    -- request battle_fetch_battle_user_data again and what happens to the map (default — the current map ?listen)
    local Relaunch = Text and Text:match("^%s*relaunch:?(%S*)")
    if Relaunch then
        WriteFile(Control, "")
        local Map = Relaunch ~= "" and Relaunch or (MapPath .. "?listen")
        Log("control: relaunch " .. Map)
        ExecuteInGameThread(function()
            local Battle = FindLive("BattleService")
            local Ok, Err = pcall(function() Battle:StartLocalBattle(FName(Map)) end)
            Log("relaunch: StartLocalBattle(" .. Map .. ") ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
        end)
        return
    end
    if not Text or not Text:match("^%s*quit") then return end
    WriteFile(Control, "")
    QuitAt = os.time()
    Log("control: quit")
    -- 07.11: forced exit — the dbnative watchdog in its own C thread (os.exit below — only if the watchdog did not arm)
    local OkW, RspW = pcall(NativeCall, "quitwatch", { sec = QUIT_FORCE_SEC })
    Standby.QuitWatch = OkW and type(RspW) == "string" and RspW:find("ok=1", 1, true) ~= nil
    Log("quit: dbnative watchdog " .. (Standby.QuitWatch and "armed" or ("not armed: " .. tostring(RspW))))
    ExecuteInGameThread(function()
        SetPhase("quit")
        GameQuit()
    end)
end

-- --- Probe (command probe in <role>-control.txt, and by itself PROBE_AFTER_SEC after ready/connected) ---------
-- Writes <role>-probe.txt: equipment managers per hero (R14 stub vs native), the TGF DS layer
-- (UBattleDsUtility / UDSSDK), KismetSystemLibrary answers about the net mode (check of the IsDedicatedServer override).
local function Call(Obj, Fn, ...)
    local Args = { ... }
    local Ok, Res = pcall(function() return Obj[Fn](Obj, table.unpack(Args)) end)
    if not Ok then return "ERR " .. tostring(Res) end
    if type(Res) == "userdata" then
        local Ok2, Name = pcall(function() return Res:GetFullName() end)
        if Ok2 and Name then return Name end
        local Ok3, Len = pcall(function() return Res:Length() end)
        if Ok3 then return "len=" .. tostring(Len) end
    end
    return tostring(Res)
end

RepFlags = function(C)
    local F = {}
    for _, P in ipairs({ "bReplicates", "bReplicatesRate", "bNetAddressable", "bReplicateUsingRegisteredSubObjectList" }) do
        local V = "?"
        pcall(function() V = tostring(C[P]) end)
        F[#F + 1] = P:gsub("^bReplicateUsingRegisteredSubObjectList$", "bRegSubObjList") .. "=" .. V
    end
    return table.concat(F, " ")
end

Probe = function(Why, Tag)
    local FileBase = Role .. (Tag or "") .. "-probe"
    local Out = { "=== probe " .. Role .. " (" .. Why .. ") " .. os.date("%H:%M:%S") .. " phase=" .. Phase }
    -- every line at once also into <role>-probe-live.txt: on a UE4SS crash (pcall does not catch it) you can see where it fell
    WriteFile(FileBase .. "-live.txt", Out[1] .. "\n")
    local function Add(S)
        Out[#Out + 1] = S
        local F = io.open(Root .. FileBase .. "-live.txt", "a")
        if F then F:write(S, "\n"); F:close() end
    end

    local Ctx = FindLive("GameStateBase") or FindLive("PlayerController")
    local Lib = KismetSystem()
    if Lib and Ctx then
        for _, Fn in ipairs({ "IsStandalone", "IsServer", "IsDedicatedServer" }) do
            Add("netmode " .. Fn .. " = " .. Call(Lib, Fn, Ctx))
        end
    end

    -- Experience and components from the GameFeature (31.10, R14: the client has no own equipment manager, although in
    -- LAS_TgfGame_StandardComponents it is bClientComponent=1 — we check whether the action set is applied on the client at all)
    -- Do not call K2_GetComponentsByClass (UE4SS 3.0.1 + an array result — risk of a UE4SS crash).
    -- Pointwise GetComponentByClass for the components from the action set (c = client only, s = server only, cs = both).
    local GF_COMPS = {
        { "cs", "/Script/LyraGame.TgfEquipmentManagerComponent" },
        { "cs", "/Script/LyraGame.TgfInventoryManagerComponent" },
        { "c", "/Script/LyraGame.TgfInGameServiceComponent" },
        { "c", "/Script/Engine.PhysicalAnimationComponent" },
        { "s", "/Script/LyraGame.TgfActorStatisticComponent" },
        { "cs", "/Script/LyraGame.LyraIndicatorManagerComponent" },
        { "cs", "/Script/LyraGame.TgfLyraInteractComponent" },
        { "c", "/Script/LyraGame.TgfControllerSteamAchievementComponent" },
        { "cs", "/Game/TGF/Camera/BP_CameraViewComponent.BP_CameraViewComponent_C" },
        { "c", "/Game/TGF/Components/PlayerControllerComponents/BP_TeammateMarkerManagerComponent.BP_TeammateMarkerManagerComponent_C" },
        { "c", "/Game/TGF/UI/Game/HUD/HurtNumDisplay/B_TgfGameFollowUIManagerComponent.B_TgfGameFollowUIManagerComponent_C" },
    }
    local function CompNames(A)
        local Names = {}
        for _, E in ipairs(GF_COMPS) do
            pcall(function()
                local Cls = StaticFindObject(E[2])
                if not Cls or not Cls:IsValid() then return end
                local C = A:GetComponentByClass(Cls)
                if C and C:IsValid() then Names[#Names + 1] = E[1] .. ":" .. C:GetFName():ToString() end
            end)
        end
        return #Names .. ": " .. table.concat(Names, " ")
    end
    -- 31.10: the player's match data (battle_fetch_battle_user_data) — whether it lives as a UObject wrapper of the proto and whose
    Add("  .. battle user data")
    pcall(function()
        for _, B in ipairs(FindAllOf("TGFProtoTgfDBattleUserData") or {}) do
            if IsValid(B) and not FullName(B):find("Default__", 1, true) then
                local Uid, Role, Type, Items = "?", "?", "?", "?"
                pcall(function() Uid = tostring(B.UserKey.UserId) end)
                pcall(function() Role = tostring(B.DeployPreview.DeployRole.Basic.RoleGuid) end)
                pcall(function() Type = tostring(B.DeployPreview.DeployRole.Basic.TypeId) end)
                pcall(function() Items = tostring(B.DeployPreview.DeployRole.Wearing.Items:GetArrayNum()) end)
                Add(string.format("battledata %s outer=%s user=%s role=%s type=%s wearing=%s", FullName(B):match("([^%.]+)$") or "?",
                    (FullName(B):match("%s([^%s]+)$") or "?"):sub(1, 90), Uid, Role, Type, Items))
            end
        end
    end)
    Add("  .. experience")
    for _, X in ipairs(FindAllOf("LyraExperienceManagerComponent") or {}) do
        if IsValid(X) and not FullName(X):find("Default__", 1, true) then
            local Exp = "nil"
            pcall(function() Exp = FullName(X.CurrentExperience):match("([^%.]+)$") or "nil" end)
            Add("experience " .. (FullName(X):match("([^%.]+)$") or "?") .. " Current=" .. Exp)
        end
    end
    Add("  .. controllers")
    for _, PC in ipairs(FindAllOf("LyraPlayerController") or {}) do
        if IsValid(PC) and not FullName(PC):find("Default__", 1, true) then
            local Loc = "?"
            pcall(function() Loc = tostring(PC:IsLocalController()) end)
            Add(string.format("pc %s local=%s comps %s", FullName(PC):match("([^%.]+)$") or "?", Loc, CompNames(PC)))
        end
    end

    local EqCls = StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
    local Managers = {}
    for _, C in ipairs(FindAllOf("TgfEquipmentManagerComponent") or {}) do
        if IsValid(C) and not FullName(C):find("Default__", 1, true) then
            local Owner = nil
            pcall(function() Owner = C:GetOwner() end)
            local Key = IsValid(Owner) and FullName(Owner) or "?"
            Managers[Key] = Managers[Key] or {}
            table.insert(Managers[Key], C)
        end
    end
    for _, H in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(H) and not FullName(H):find("Default__", 1, true) then
            local R, Loc = "?", "?"
            pcall(function() R = tostring(H.Role) end)
            pcall(function() Loc = tostring(H:IsLocallyControlled()) end)
            local ByClass = "?"
            pcall(function() ByClass = FullName(H:GetComponentByClass(EqCls)) end)
            Add(string.format("hero %s Role=%s local=%s GetComponentByClass(EquipMgr)=%s",
                FullName(H):match("([^%.]+)$") or "?", R, Loc, ByClass:match("([^%.]+)$") or ByClass))
            Add("    comps " .. CompNames(H))
            -- stats (31.10: those who connected had 30 each and default HP — FirstLevelConfig was taken from the host's pawn)
            pcall(function()
                local AC = H.AttributeComponent
                local Cfg, Curve = "nil", "?"
                pcall(function() Cfg = FullName(AC.FirstLevelConfig):match("([^%.]+)$") or "nil" end)
                pcall(function() Curve = tostring(AC.CurveLevel) end)
                local Parts = {}
                pcall(function()
                    local F = AC:GetAttributeSet(StaticFindObject("/Script/LyraGame.TgfFirstLevelAttributeSet"))
                    for _, N in ipairs({ "Strength", "Dexterity", "Stamina", "Intelligence", "Will", "Faith" }) do
                        pcall(function() Parts[#Parts + 1] = string.format("%s=%.4g", N:sub(1, 3), F[N].CurrentValue) end)
                    end
                end)
                pcall(function()
                    local D = AC:GetAttributeSet(StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet"))
                    Parts[#Parts + 1] = string.format("MaxHPBase=%.4g CurHP=%.4g", D.MaxHealthBase.CurrentValue, D.CurHealth.CurrentValue)
                    -- 07.11: resistances (playtest season: 50% at level 20, cap 80%) and general damage reduction
                    local R = {}
                    for _, N in ipairs({ "None", "Fire", "Ice", "Thunder", "Light", "Dark" }) do
                        pcall(function() R[#R + 1] = string.format("%s=%.3g", N, D["ElementResistanceRate" .. N].CurrentValue) end)
                    end
                    Parts[#Parts + 1] = "Resist " .. table.concat(R, ",")
                    pcall(function() Parts[#Parts + 1] = string.format("DamageReduceRate=%.3g", D.DamageReduceRate.CurrentValue) end)
                end)
                Add(string.format("    stats FirstLevelConfig=%s CurveLevel=%s %s", Cfg, Curve, table.concat(Parts, " ")))
            end)
            -- 31.10 comparison of the native initialization and LocalDev: PlayerState fields, ASC effects and abilities
            pcall(function()
                local PS = nil
                pcall(function() PS = H:GetLyraPlayerState() end)
                if not IsValid(PS) then pcall(function() PS = H.PlayerState end) end
                if not IsValid(PS) then Add("    ps none"); return end
                local F = {}
                for _, P in ipairs({ "MyUserID", "MyRoleGuid", "MyOpenId", "HasInitedPlayerData" }) do
                    local V = "?"
                    pcall(function() V = tostring(PS[P]) end)
                    pcall(function() if type(PS[P]) == "userdata" and PS[P].ToString then V = PS[P]:ToString() end end)
                    F[#F + 1] = P .. "=" .. V
                end
                pcall(function() F[#F + 1] = "PawnData=" .. (FullName(PS.PawnData):match("([^%.]+)$") or "nil") end)
                local ASC = PS.AbilitySystemComponent
                local GE, AB = "?", "?"
                pcall(function() GE = tostring(ASC.ActiveGameplayEffects.GameplayEffects_Internal:GetArrayNum()) end)
                pcall(function() AB = tostring(ASC.ActivatableAbilities.Items:GetArrayNum()) end)
                Add("    ps " .. table.concat(F, " ") .. " ge=" .. GE .. " abilities=" .. AB)
                local Names = {}
                pcall(function()
                    ASC.ActiveGameplayEffects.GameplayEffects_Internal:ForEach(function(_, E)
                        if #Names < 60 then
                            local N = "?"
                            pcall(function() N = E:get().Spec.Def:GetFName():ToString() end)
                            Names[#Names + 1] = N
                        end
                    end)
                end)
                table.sort(Names)
                Add("    ge: " .. table.concat(Names, " "))
            end)
            -- do not read the body's ChildActorComponent: reading ChildActorClass (TSubclassOf) crashes UE4SS 3.0.1 in
            -- UE4SS.dll+0x2aee17, AV 0x18 (31.10, live probe: the last line before the crash — stats)
            Add("    .. avatars")
            -- avatars in the world and their owner (in case the body is not created via ChildActorComponent)
            local AvOk, AvErr = pcall(function()
                for _, A in ipairs(FindAllOf("TgfAvatarActor") or {}) do
                    if IsValid(A) and not FullName(A):find("Default__", 1, true) then
                        local Own = "nil"
                        pcall(function() Own = FullName(A:GetOwner()):match("([^%.]+)$") or "nil" end)
                        local Par = "nil"
                        pcall(function() Par = FullName(A:GetParentComponent()):match("([^%.]+)$") or "nil" end)
                        if Own == (FullName(H):match("([^%.]+)$") or "") then
                            -- body meshes (31.10, BP_AvatarActor patch: on the client UpdateMesh in BeginPlay is skipped)
                            local function MeshName(M)
                                local N = "nil"
                                pcall(function() N = FullName(M.SkinnedAsset):match("([^%.]+)$") or "nil" end)
                                if N == "nil" then pcall(function() N = FullName(M.SkeletalMesh):match("([^%.]+)$") or "nil" end) end
                                return N
                            end
                            local Main = "?"
                            pcall(function() Main = MeshName(A.Mesh) end)
                            Add(string.format("    avatar %s parentComp=%s mesh=%s", FullName(A):match("([^%.]+)$") or "?", Par, Main))
                        end
                    end
                end
            end)
            if not AvOk then Add("    avatar ERR " .. tostring(AvErr)) end
            for _, C in ipairs(Managers[FullName(H)] or {}) do
                local N = "?"
                pcall(function()
                    local E = C:GetAllEquips()
                    N = type(E) == "table" and tostring(#E) or tostring(E:Length())
                end)
                -- rep: whether the manager is replicated (31.10, R14: without stubs the server's native manager does not reach the client)
                Add(string.format("    mgr %s equips=%s %s", FullName(C):match("([^%.]+)$") or "?", N, RepFlags(C)))
                -- 31.10: items — type_id, RoleGuid (whose), Equipped and EquipAttrItems bonuses: how many of them were added
                -- to the attributes (HasAdded). Comparison of the native path and LocalDev: in LocalDev the stats lack the bonuses from items
                local function Item(Eq)
                    local T, G, Eqd, Tot, Added, Sum = "?", "?", "?", 0, 0, {}
                    local Inst = nil
                    pcall(function() Inst = Eq.InventoryInstance end)
                    pcall(function() T = tostring(Inst.type_id) end)
                    pcall(function() G = tostring(Inst.RoleGuid) end)
                    pcall(function() Eqd = tostring(Inst.Equipped) end)
                    pcall(function()
                        Inst.EquipAttrItems:ForEach(function(_, El)
                            local It = El:get()
                            Tot = Tot + 1
                            local Has = It.HasAdded
                            if Has then Added = Added + 1 end
                            if #Sum < 3 then
                                local Nm = "?"
                                pcall(function() Nm = It.Attribute.AttributeName:ToString() end)
                                Sum[#Sum + 1] = string.format("%s=%.1f%s", Nm, It.Value, Has and "+" or "")
                            end
                        end)
                    end)
                    Add(string.format("      item t=%s guid=%s eq=%s attrs=%d added=%d %s", T, G, Eqd, Tot, Added, table.concat(Sum, " ")))
                end
                pcall(function()
                    local E = C:GetAllEquips()
                    if type(E) == "table" then
                        for _, Eq in ipairs(E) do Item(Eq) end
                    else
                        E:ForEach(function(_, El) Item(El:get()) end)
                    end
                end)
            end
        end
    end

    local DsCount = 0
    for _, Ds in ipairs(FindAllOf("BattleDsUtility") or {}) do
        if IsValid(Ds) and not FullName(Ds):find("Default__", 1, true) then
            DsCount = DsCount + 1
            Add("BattleDsUtility " .. FullName(Ds))
            for _, Fn in ipairs({ "IsOptionArgsInit", "GetCurrentUserCount", "GetTotalUserCount",
                                  "GetWaitingRoomTimeout", "HasNextFloor" }) do
                Add("    " .. Fn .. " = " .. Call(Ds, Fn))
            end
        end
    end
    if DsCount == 0 then Add("BattleDsUtility: none alive") end
    for _, ClassName in ipairs({ "DSSDK", "DssdkSubsystem" }) do
        local N = 0
        for _, O in ipairs(FindAllOf(ClassName) or {}) do
            if IsValid(O) and not FullName(O):find("Default__", 1, true) then N = N + 1; Add(ClassName .. " " .. FullName(O)) end
        end
        if N == 0 then Add(ClassName .. ": none alive") end
    end

    WriteFile(FileBase .. ".txt", table.concat(Out, "\n") .. "\n")
    Log("probe (" .. Why .. ") -> " .. FileBase .. ".txt, lines " .. #Out)
end

-- Experiment: can UE4SS override the answer of a native UFUNCTION. In a -dbserver process with -dbfakeds=1
-- IsDedicatedServer answers true (a headless server has no own player — as on the official DS).
-- The post-callback logs what UE4SS passes in the arguments (the first 3 calls) and returns true.
local FakeDs = false
local FakeDsLogged = 0
InstallFakeDs = function()
    FakeDs = Role == "dbserver" and FakeDsArg
    -- 31.10: the override works (probe: IsDedicatedServer = true), but with it the server crashed in UE4SS.dll when a
    -- client joined — the hook is set only when the override is enabled
    if not FakeDs then return end
    local Ok, Err = pcall(function()
        RegisterHook("/Script/Engine.KismetSystemLibrary:IsDedicatedServer", function() end, function(...)
            if FakeDsLogged < 3 then
                FakeDsLogged = FakeDsLogged + 1
                local Parts = {}
                for I = 1, select("#", ...) do
                    local A = select(I, ...)
                    local V = A
                    pcall(function() V = A:get() end)
                    Parts[#Parts + 1] = I .. ":" .. type(A) .. "=" .. tostring(V)
                end
                Log("fakeds post args: " .. table.concat(Parts, " "))
            end
            if FakeDs then return true end
        end)
    end)
    Log("fakeds hook ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))) .. " active=" .. tostring(FakeDs))
end

-- Trace of the hero's RepNotify/RPC on the client (31.10, R14): which native event updates the body when the hero arrives and
-- crashes on GetEquipBySlot(null). Only the dbclient role and the file dbclient-reptrace.txt = 1. Each event immediately
-- as a line in dbclient-reptrace.log (">>" enter, "<<" exit): a crash inside an event — the last ">>" without a pair.
local REP_TRACE = {
    "/Script/LyraGame.LyraCharacter:OnRep_Avatar", "/Script/LyraGame.LyraCharacter:OnRep_DisplayActorCareer",
    "/Script/LyraGame.LyraCharacter:OnRep_DisplayActorRace", "/Script/LyraGame.LyraCharacter:OnRep_MyTeamID",
    "/Script/LyraGame.LyraCharacter:OnRep_Freeze",
    "/Script/LyraGame.TgfCharacterAvatarComponent:OnRep_Avatar",
    "/Script/LyraGame.TgfAvatarHelperComponent:OnRep_MeshParts", "/Script/LyraGame.TgfAvatarHelperComponent:OnRep_LinkAnimLayer",
    "/Script/LyraGame.TgfAvatarHelperComponent:OnRep_AvatarHeadConfig", "/Script/LyraGame.TgfAvatarHelperComponent:OnRep_AvatarGender",
    "/Script/LyraGame.LyraPawnExtensionComponent:OnRep_PawnData",
    "/Script/LyraGame.LyraEquipmentManagerComponent:OnRep_EquipmentList",
    "/Script/LyraGame.TgfEquipmentManagerComponent:OnRep_WeaponSlotList", "/Script/LyraGame.TgfEquipmentManagerComponent:OnRep_CurrentWeaponSlot",
    "/Script/LyraGame.LyraEquipmentInstance:OnRep_Instance",
    "/Script/LyraGame.TgfEquipmentInstance:OnRep_WeaponVisible", "/Script/LyraGame.TgfEquipmentInstance:OnRep_SpawnedActors",
    "/Script/LyraGame.LyraInventoryManagerComponent:OnRep_InventoryList",
    "/Script/LyraGame.TgfInventoryManagerComponent:OnRep_Size", "/Script/LyraGame.TgfInventoryManagerComponent:OnRep_AdditionalSize",
    "/Script/LyraGame.TgfAttributeComponent:OnRep_DeathStateInfo",
    "/Script/LyraGame.LyraPlayerState:OnRep_PawnData", "/Script/LyraGame.LyraPlayerState:OnRep_MyTeamID",
    "/Script/LyraGame.LyraPlayerState:OnRep_MyUserID", "/Script/LyraGame.LyraPlayerState:OnRep_SceneTag",
    "/Script/Engine.Pawn:OnRep_PlayerState", "/Script/Engine.Pawn:OnRep_Controller",
}
InstallRepTrace = function()
    if Role ~= "dbclient" or not (ReadFile("dbclient-reptrace.txt") or ""):match("^%s*1") then return end
    local Path = Root .. Role .. "-reptrace.log"
    local function Line(S)
        local F = io.open(Path, "a")
        if F then F:write(string.format("%s %.3f %s\n", os.date("%H:%M:%S"), os.clock(), S)); F:close() end
    end
    Line("=== reptrace start")
    local Ok, Bad = 0, {}
    for _, Fn in ipairs(REP_TRACE) do
        local Short = Fn:match("%.([%w_]+:[%w_]+)$") or Fn
        local Hooked = pcall(function()
            RegisterHook(Fn, function(Ctx)
                local Name = "?"
                pcall(function() Name = Ctx:get():GetFName():ToString() end)
                Line(">> " .. Short .. " " .. Name)
            end, function()
                Line("<< " .. Short)
            end)
        end)
        if Hooked then Ok = Ok + 1 else Bad[#Bad + 1] = Short end
    end
    Line(string.format("hooks %d/%d%s", Ok, #REP_TRACE, #Bad > 0 and (" missing: " .. table.concat(Bad, " ")) or ""))
    -- object creation: the full name shows which actor it is attached to (properties are not read — the object is not fully built)
    for _, Cls in ipairs({ "/Script/LyraGame.TgfEquipmentManagerComponent", "/Script/LyraGame.TgfInventoryManagerComponent",
        "/Script/Engine.ChildActorComponent", "/Game/TGF/Character/Hero/HeroSample/BP_Fighter_HeroSample.BP_Fighter_HeroSample_C" }) do
        local Short = Cls:match("%.([%w_]+)$") or Cls
        pcall(function()
            NotifyOnNewObject(Cls, function(Obj)
                local Name = "?"
                pcall(function() Name = Obj:GetFullName() end)
                if not Name:find("Default__", 1, true) then Line("new " .. Short .. " " .. Name) end
            end)
        end)
    end
    Log(string.format("reptrace: hooks %d/%d -> %s", Ok, #REP_TRACE, Path))
end

-- standby: in the lobby without a map; host task (OnHost) -> load its hall (then as usual: start -> hosting -> ready -> hall)
function Standby.Step()
    if Host and Host.Phase == "loading" and not Host.LoadIssued then
        MapPath, IdleMap = Host.Scenes[1], Host.Scenes[1]
        Host.LoadIssued = true
        Log("[HOST] standby -> loading hall " .. MapPath .. " for " .. Host.Battle)
        SetPhase("start", "hall for " .. Host.Battle)
    end
end

-- exiting: backend got exiting (or 5 s without an answer) -> quit via the control file (the forced exit is there too)
function Standby.ExitStep()
    if Standby.QuitWritten or not (Standby.ExitSent or os.time() - Standby.ExitAt >= 5) then return end
    Standby.QuitWritten = true
    WriteFile(Role .. Inst .. "-control.txt", "quit")
    Log("[SLOT] standby: match finished — the process exits, ds-agent will start a new one")
end

-- usmap (19.11, tools\stand\usmap.ps1): the client role with dbclient-usmap.txt = 1 dumps the game's mappings once the
-- engine has settled (UE4SS DumpUSMAP writes Win64\ue4ss\*.usmap) and quits: no backend and no patch paks needed, so a new
-- workspace gets the usmap that ws dump needs from its own copy of the game
function Standby.UsmapStep()
    Standby.UsmapAt = Standby.UsmapAt or (os.time() + StartDelay)
    if Standby.UsmapDone or os.time() < Standby.UsmapAt then return end
    Standby.UsmapDone = true
    -- at the login screen the game has not loaded most Blueprint classes; usmap.ps1 -Load names assets to load first
    -- (in a cooked game the class is <Asset>.<Asset>_C — the Blueprint object itself does not exist)
    for _, Path in ipairs(Standby.UsmapLoad or {}) do
        local OkL, Obj = pcall(LoadAsset, Path)
        local OkV, Valid = pcall(function() return OkL and Obj ~= nil and Obj:IsValid() end)
        Log("usmap: LoadAsset " .. Path .. " -> " .. ((OkV and Valid) and "ok" or ("failed " .. tostring(Obj))))
    end
    local Ok, Err = pcall(function() DumpUSMAP() end)
    Log("usmap: DumpUSMAP ok=" .. tostring(Ok) .. (Ok and "" or (" err=" .. tostring(Err))))
    SetPhase("usmap-done", Ok and "ok" or tostring(Err))
    WriteFile(Role .. Inst .. "-control.txt", "quit")
end

local Steps = { init = Init, login = Login, lobby = Lobby, start = Start, hosting = Hosting, observer = Observer, ready = Ready,
    standby = Standby.Step, exiting = Standby.ExitStep, usmap = Standby.UsmapStep,
    resetting = ResetStep,
    connect = Connect, match = Match, localbattle = LocalBattle, connecting = Connecting, connected = Connected }

-- 07.11 (docs/CRASH_LONG_PLAY.md): the main loop in the game thread. It used to be LoopAsync + ExecuteInGameThread — the mod's Lua
-- ran in a service thread concurrently with the game thread, which crashed UE4SS during long runs. LoopInGameThreadWithDelay
-- does not stop on return true — only CancelDelayedAction; an error in the callback does not break the loop.
-- No new locals in the main block (limit 200) — the handle is in Standby.
-- Register from the game thread: the mod is loaded in the UE4SS thread, and registering a timer from there may hang
-- the process (07.11, the client hung while loading RuClient). ExecuteInGameThread is safe from any thread.
ExecuteInGameThread(function() Standby.MainLoop = LoopInGameThreadWithDelay(TICK_MS, function()
    if Phase ~= "init" and Phase ~= "off" then
        local Ok, Err = pcall(CheckControlAsync)
        if not Ok then Log("control error: " .. tostring(Err)) end
    end
    if QuitAt then return end
    local Step = Steps[Phase]
    if Step then
        local Ok, Err = pcall(Step)
        if not Ok then Log("step " .. Phase .. " error: " .. tostring(Err)) end
    end
    local OkSlot, ErrSlot = pcall(SlotTick)
    if not OkSlot then Log("[SLOT] error: " .. tostring(ErrSlot)) end
    local OkRelic, ErrRelic = pcall(RelicLimit.Tick)
    if not OkRelic then Log("[RELIC] error: " .. tostring(ErrRelic)) end
    local OkBones, ErrBones = pcall(MobBones.Tick)
    if not OkBones then Log("[MOBBONES] error: " .. tostring(ErrBones)) end
    if Phase == "off" then CancelDelayedAction(Standby.MainLoop) end -- after ended we keep listening for quit
end) end)

print("[DungeonborneRuServer] loaded, waiting for the command line\n")
