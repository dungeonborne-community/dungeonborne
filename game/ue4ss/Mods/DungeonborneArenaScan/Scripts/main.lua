-- Mods/DungeonborneArenaScan/Scripts/main.lua
-- 11.11: a snapshot of arena maps (DM L_Common_PVP0x_DeathMatch and "Clotho" L_Common_PVP0x_MultiGameMode) — read only.
-- Task: place chests on the Clotho maps and consumables on the DM maps at the same spots as on the paired map.
-- 15 s after an arena loads it writes to ArenaScan.log (Win64 folder):
--   CLASS  — all actor classes of the level and their count;
--   ITEM   — every actor of an "interesting" class (chest, item, pickup, spawn, barrel) with coordinates and rotation.
-- Battle server only (-dbserver flag). Changes nothing on the map. A separate mod: RuServer has a 200-local limit.

local Off = false
local Done = {}
local Out = nil

local function Log(Msg)
    print("[ArenaScan] " .. Msg .. "\n")
    if not Out then Out = io.open("ArenaScan.log", "a") end
    if Out then
        Out:write(os.date("%Y-%m-%d %H:%M:%S ") .. Msg .. "\n")
        Out:flush()
    end
end

local function IsValid(Obj) return Obj ~= nil and Obj:IsValid() end

local function FullName(Obj)
    local Ok, Name = pcall(function() return Obj:GetFullName() end)
    return Ok and Name or ""
end

-- Timer only in the game thread (like GameLoop in RuClient: registration from the UE4SS thread hung the process)
local function GameLoop(Ms, Fn)
    ExecuteInGameThread(function()
        local Handle
        Handle = LoopInGameThreadWithDelay(Ms, function()
            local Ok, Stop = pcall(Fn)
            if not Ok then Log("error: " .. tostring(Stop))
            elseif Stop == true and Handle then CancelDelayedAction(Handle) end
        end)
    end)
end

local INTEREST = { "chest", "collect", "pickup", "potion", "item", "loot", "spawn", "destroy", "barrel", "consum", "drop", "supply" }

local function Interesting(Cls)
    local L = Cls:lower()
    for _, K in ipairs(INTEREST) do
        if L:find(K, 1, true) then return true end
    end
    return false
end

local function Scan(Map)
    local Count, Items = {}, {}
    for _, A in ipairs(FindAllOf("Actor") or {}) do
        if IsValid(A) then
            local Full = FullName(A)
            if Full ~= "" and not Full:find("Default__", 1, true) and Full:find("/TgfMaps/", 1, true) then
                local Cls = "?"
                pcall(function() Cls = A:GetClass():GetFName():ToString() end)
                Count[Cls] = (Count[Cls] or 0) + 1
                if Interesting(Cls) then
                    local Loc, Rot, Parent = "?", "?", "-"
                    pcall(function() local V = A:K2_GetActorLocation(); Loc = string.format("%.0f %.0f %.0f", V.X, V.Y, V.Z) end)
                    pcall(function() local R = A:K2_GetActorRotation(); Rot = string.format("%.0f %.0f %.0f", R.Pitch, R.Yaw, R.Roll) end)
                    pcall(function()
                        local P = A:GetAttachParentActor()
                        if IsValid(P) then Parent = P:GetFName():ToString() end
                    end)
                    local Name = "?"
                    pcall(function() Name = A:GetFName():ToString() end)
                    Items[#Items + 1] = string.format("ITEM %s | %s | %s | loc %s | rot %s | parent %s", Map, Cls, Name, Loc, Rot, Parent)
                end
            end
        end
    end
    local Classes = {}
    for Cls, N in pairs(Count) do Classes[#Classes + 1] = string.format("%s=%d", Cls, N) end
    table.sort(Classes)
    Log(string.format("=== %s: classes %d, interesting actors %d", Map, #Classes, #Items))
    for _, C in ipairs(Classes) do Log("CLASS " .. Map .. " | " .. C) end
    table.sort(Items)
    for _, I in ipairs(Items) do Log(I) end
end

local Seen = { Key = nil, At = 0 }
local Spawned = {}

-- 11.11: player spawns on the arena — team, coordinates and the nearest start (its set and the set's team). Analysis of
-- "both in the center at one point" in DM. A pawn is logged once per match, in the first second it is visible.
local function SpawnLog(Map, Key)
    local Starts = nil
    for _, P in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(P) and not FullName(P):find("Default__", 1, true) then
            local Id = Key .. "|" .. string.format("%X", P:GetAddress())
            if not Spawned[Id] then
                Spawned[Id] = true
                if not Starts then
                    Starts = {}
                    for _, St in ipairs(FindAllOf("LyraPlayerStart") or {}) do
                        if IsValid(St) and not FullName(St):find("Default__", 1, true) then
                            local E = { Name = "?", Team = "?", Set = "-" }
                            pcall(function() E.Name = St:GetFName():ToString() end)
                            pcall(function() local V = St:K2_GetActorLocation(); E.X, E.Y, E.Z = V.X, V.Y, V.Z end)
                            pcall(function()
                                local S = St:GetAttachParentActor()
                                if IsValid(S) then
                                    E.Set = S:GetFName():ToString()
                                    E.Team = tostring(S:GetTeamId())
                                end
                            end)
                            if E.X then Starts[#Starts + 1] = E end
                        end
                    end
                end
                local Open, Team, X, Y, Z = "?", "?", nil, nil, nil
                pcall(function()
                    local PS = P:GetPlayerState()
                    if IsValid(PS) then
                        Open = PS.MyOpenId:ToString()
                        Team = tostring(PS:GetTeamId())
                    end
                end)
                pcall(function() local V = P:K2_GetActorLocation(); X, Y, Z = V.X, V.Y, V.Z end)
                local Best, BestD = nil, nil
                if X then
                    for _, E in ipairs(Starts) do
                        local D = math.sqrt((E.X - X) ^ 2 + (E.Y - Y) ^ 2 + (E.Z - Z) ^ 2)
                        if not BestD or D < BestD then Best, BestD = E, D end
                    end
                end
                Log(string.format("SPAWN %s | open_id %s | team %s | loc %s | start %s (%.0f) set %s set team %s",
                    Map, Open, Team, X and string.format("%.0f %.0f %.0f", X, Y, Z) or "?",
                    Best and Best.Name or "?", BestD or -1, Best and Best.Set or "?", Best and Best.Team or "?"))
            end
        end
    end
end

-- 11.11 development on the stand: if arenascan-dev.txt exists (Win64 folder) — the script ArenaScan-run.lua runs once in
-- the game thread (renamed to ArenaScan-run.done.lua), its Log writes to ArenaScan.log. Without arenascan-dev.txt
-- disabled (Robot has no such file). For analysing actor properties and placement trials without restarting the battle server.
local function DevRun()
    local Flag = io.open("arenascan-dev.txt", "r")
    if not Flag then return end
    Flag:close()
    local F = io.open("ArenaScan-run.lua", "r")
    if not F then return end
    local Code = F:read("*a")
    F:close()
    os.remove("ArenaScan-run.done.lua")
    os.rename("ArenaScan-run.lua", "ArenaScan-run.done.lua")
    local Env = setmetatable({ Log = Log, IsValid = IsValid, FullName = FullName }, { __index = _G })
    local Chunk, Err = load(Code, "ArenaScan-run", "t", Env)
    if not Chunk then
        Log("RUN parse error: " .. tostring(Err))
        return
    end
    local Ok, Res = pcall(Chunk)
    Log("RUN " .. (Ok and "ok" or "error") .. ": " .. tostring(Res))
end

local function Tick()
    if Off then return true end
    pcall(DevRun)
    local Gs = FindFirstOf("GameStateBase")
    if not IsValid(Gs) then return end
    local Full = FullName(Gs)
    local Map = Full:match("/(L_Common_PVP0%d_[%w_]+)%.") or Full:match("/(L_TGF_Gym_[%w_]+)%.")
    if not Map then
        Seen.Key = nil
        return
    end
    local Key = Map .. "|" .. string.format("%X", Gs:GetAddress())
    if Seen.Key ~= Key then
        Seen.Key, Seen.At = Key, os.time()
        return
    end
    pcall(SpawnLog, Map, Key)
    if Done[Key] or os.time() - Seen.At < 15 then return end
    Done[Key] = true
    Scan(Map)
end

local function Init()
    local Ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    if not IsValid(Ksl) then return false end
    local Cl = " " .. Ksl:GetCommandLine():ToString():lower() .. " "
    if not Cl:find("%s%-dbserver%s") then
        Off = true
        print("[ArenaScan] no -dbserver — mod disabled\n")
        return true
    end
    Log("enabled: arena snapshot 15 s after the map loads")
    GameLoop(1000, Tick)
    return true
end

GameLoop(1000, function()
    return Init()
end)
