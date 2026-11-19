-- LuaProbe: a probe inside the client's Lua (UnLua) — the counterpart of the UE4SS mod ClientProbe (docs/CLIENT_PROBE.md).
-- Started from WBP_MatchMainButton.lua (lobby). Settings: <Win64>/ue4ss/Mods/ClientProbe/lua_config.txt;
-- no file or enabled=0 — does nothing.
--   LCALL/LRET — calls of UI module and service functions (modules=…): arguments and result;
--                already loaded modules are wrapped, and those loaded later via require
--   FIELD      — first read of a field of a proto object (UTGFProtoTgf*) or a struct (FTGF*): which fields of the server
--                response the client actually reads
--   TRACE      — (trace=1) every call of a Lua function from files whose path contains the trace_filter substring
--   LERR/LERRRET — texts for the player: UE.UToastWindow.ShowToast*, ShowAlert/ShowConfirm, PromptProtocolError and
--                GetProtocolErrorText (returns the error text) — calls from Lua
-- The probe does not touch auto instances (-dbserver / -dbclient on the command line).
-- Line format as in ClientProbe: <epoch_ms>\t<seq>\tlua\t<type>\t<text>; file <log_dir>/<client>_<date>_lua.log.
-- Probe errors must not break the game: wrappers call the original directly, logging is inside pcall.

local M = {}
local Started = false

local Cfg = {
    enabled = "0",
    log_dir = "<repo>/probe/logs",
    name = "",
    modules = "Game.Service;Game.System;Mail;Chat;Social;Team;PVP;Inventory;UserRole;BattlePass;Task;Mithril;Blacksmith;Unlock;PlayerGrade;Banner",
    skip_modules = "Game.System.Match;Table;Core;Base;Common;Text;Data",
    skip = "Tick;OnPaint;OnMouse;GetDesired;NativeTick",
    call_limit = "40",
    fields = "1",
    field_prefixes = "UTGFProtoTgf;FTGF",
    trace = "0",
    trace_filter = "Mail;Chat;Social;Team;PVP",
    skip_cmdline = "-dbserver;-dbclient",
}

local function Split(s)
    local t = {}
    for x in (s or ""):gmatch("[^;]+") do t[#t + 1] = x end
    return t
end

local function ReadConfig()
    local f = io.open("ue4ss/Mods/ClientProbe/lua_config.txt", "r")
    if not f then return false end
    for line in f:lines() do
        if not line:match("^%s*[#;]") then
            local k, v = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
            if k then Cfg[k] = v end
        end
    end
    f:close()
    return true
end

-- ---------------------------------------------------------------- log

local Out, Seq = nil, 0
local T0, C0, Calibrated = os.time(), os.clock(), false
local Quiet = false -- inside the probe: do not write FIELD from our own formatting

local function NowMs()
    if not Calibrated then
        local t = os.time()
        if t ~= T0 then T0, C0, Calibrated = t, os.clock(), true end
    end
    return math.floor(T0 * 1000 + (os.clock() - C0) * 1000)
end

local function W(kind, text)
    if not Out then return end
    Seq = Seq + 1
    local s = tostring(text):gsub("[\r\n\t]", " ")
    if #s > 8000 then s = s:sub(1, 8000) .. "..." end
    Out:write(NowMs(), "\t", Seq, "\tlua\t", kind, "\t", s, "\n")
    Out:flush()
end

-- ---------------------------------------------------------------- value formatting

local Fmt
Fmt = function(v, depth)
    local t = type(v)
    if t == "string" then
        if #v > 160 then v = v:sub(1, 160) .. "..." end
        return string.format("%q", v)
    end
    if t == "number" or t == "boolean" or t == "nil" then return tostring(v) end
    if t == "userdata" then
        local ok, s = pcall(function() return v:GetName() end)
        if ok and type(s) == "string" then
            local okc, c = pcall(function() return v:GetClass():GetName() end)
            return (okc and c or "UObject") .. ":" .. s
        end
        local okn, n = pcall(function() return v:Num() end) -- TArray
        if okn and type(n) == "number" then return "TArray[" .. n .. "]" end
        local okt, ts = pcall(tostring, v)
        return okt and ts or "userdata"
    end
    if t == "table" then
        if depth <= 0 then return "{...}" end
        local parts, n = {}, 0
        local ok = pcall(function()
            for k, x in pairs(v) do
                n = n + 1
                if n > 6 then parts[#parts + 1] = "..."; break end
                parts[#parts + 1] = tostring(k) .. "=" .. Fmt(x, depth - 1)
            end
        end)
        return ok and ("{" .. table.concat(parts, ", ") .. "}") or "{?}"
    end
    return t
end

local function FmtList(list, from, to)
    local parts = {}
    for i = from, to do parts[#parts + 1] = Fmt(list[i], 1) end
    return table.concat(parts, ", ")
end

-- ---------------------------------------------------------------- module calls

local CallLimit = 40
local Counts = {}
local function Allowed(key)
    local c = (Counts[key] or 0) + 1
    Counts[key] = c
    if c <= CallLimit or c % 500 == 0 then return true, c end
    return false, c
end

local Wrapped = setmetatable({}, { __mode = "k" }) -- wrappers (so as not to wrap twice)
local WrappedTables = setmetatable({}, { __mode = "k" })
local ModPrefixes, ModSkip, FnSkip = {}, {}, {}

-- "Mail" matches "Mail" and "Mail.WBP_MailWindow", but not "MailFoo"
local function Matches(name, list)
    for _, p in ipairs(list) do
        if name == p or name:sub(1, #p + 1) == p .. "." then return true end
    end
    return false
end

local function WantModule(name)
    return type(name) == "string" and Matches(name, ModPrefixes) and not Matches(name, ModSkip)
        and not name:find("LuaProbe", 1, true)
end

local function SkipFn(key)
    if key:sub(1, 2) == "__" or key == "Super" then return true end
    for _, s in ipairs(FnSkip) do
        if key:find(s, 1, true) then return true end
    end
    return false
end

local function WrapFn(name, fn, kind)
    local callKind, retKind = kind or "LCALL", kind and (kind .. "RET") or "LRET"
    local w = function(...)
        local allowed, c = Allowed(name)
        if not allowed and not kind then return fn(...) end
        local n = select("#", ...)
        local args = { ... }
        Quiet = true
        pcall(W, callKind, name .. "(" .. FmtList(args, 1, n) .. ")" .. (c > CallLimit and (" #" .. c) or ""))
        Quiet = false
        local r = table.pack(fn(...))
        if r.n > 0 and r[1] ~= nil then
            Quiet = true
            pcall(W, retKind, name .. " -> " .. FmtList(r, 1, r.n))
            Quiet = false
        end
        return table.unpack(r, 1, r.n)
    end
    Wrapped[w] = true
    return w
end

local function WrapTable(modname, tbl, depth)
    if WrappedTables[tbl] or depth > 2 then return 0 end
    WrappedTables[tbl] = true
    local n = 0
    local keys = {}
    for k, v in pairs(tbl) do keys[#keys + 1] = { k, v } end
    for _, kv in ipairs(keys) do
        local k, v = kv[1], kv[2]
        if type(k) == "string" and not SkipFn(k) then
            if type(v) == "function" and not Wrapped[v] then
                rawset(tbl, k, WrapFn(modname .. "." .. k, v))
                n = n + 1
            elseif type(v) == "table" and k:match("^%u") and getmetatable(v) == nil then
                n = n + WrapTable(modname .. "." .. k, v, depth + 1)
            end
        end
    end
    return n
end

local function WrapModule(name, mod)
    if type(mod) ~= "table" then return 0 end
    local ok, n = pcall(WrapTable, name, mod, 1)
    if ok and n > 0 then W("HDR", "module " .. name .. ": " .. n .. " functions") end
    return ok and n or 0
end

local function HookRequire()
    local orig = require
    require = function(name, ...)
        local r1, r2 = orig(name, ...)
        if WantModule(name) then pcall(WrapModule, name, r1) end
        return r1, r2
    end
end

-- ---------------------------------------------------------------- texts for the player (toasts, alerts, errors)

-- Static UE functions called from Lua: UE.<Class>.<Function> — we replace the class field with a wrapper (type LERR,
-- result — LERRRET). The UE4SS layer does not read the strings (it corrupted the heap), so this layer writes the texts.
local TEXT_STATICS = {
    { "UToastWindow", { "ShowToast", "ShowToastWithObject", "ShowToastWithActor", "ShowToastWithActorAndTime" } },
    { "UAsyncAction_ShowAlert", { "ShowAlert" } },
    { "UAsyncAction_ShowConfirm", { "ShowConfirm" } },
    { "UTGFNetworkFunctionLibrary", { "PromptProtocolError", "GetProtocolErrorText" } },
}

local function WrapTextStatics()
    local n = 0
    for _, entry in ipairs(TEXT_STATICS) do
        local ok, cls = pcall(function() return UE[entry[1]] end)
        if ok and type(cls) == "table" then
            for _, fname in ipairs(entry[2]) do
                local okf, orig = pcall(function() return cls[fname] end)
                if okf and type(orig) == "function" and not Wrapped[orig] then
                    rawset(cls, fname, WrapFn(entry[1] .. "." .. fname, orig, "LERR"))
                    n = n + 1
                end
            end
        end
    end
    W("HDR", "texts: wrapped static functions " .. n)
end

-- ---------------------------------------------------------------- field reads

local Seen = {}
local Patched = setmetatable({}, { __mode = "k" })

local function PatchMeta(name)
    local ok, mt = pcall(function() return UE[name] end)
    if not ok or type(mt) ~= "table" or Patched[mt] then return false end
    local idx = rawget(mt, "__index")
    if idx == nil then return false end
    Patched[mt] = true
    rawset(mt, "__index", function(self, k)
        if not Quiet then
            local key = name .. "." .. tostring(k)
            if not Seen[key] then
                Seen[key] = true
                Quiet = true
                pcall(W, "FIELD", key)
                Quiet = false
            end
        end
        if type(idx) == "function" then return idx(self, k) end
        return idx[k]
    end)
    return true
end

local function PatchFields()
    local ok, targets = pcall(require, "Game.System.Match.LuaProbeTargets")
    if not ok or type(targets) ~= "table" then
        W("WARN", "no LuaProbeTargets: " .. tostring(targets))
        return
    end
    local prefixes = Split(Cfg.field_prefixes)
    local t0, n, tried = os.clock(), 0, 0
    for _, list in ipairs({ targets.classes or {}, targets.structs or {} }) do
        for _, name in ipairs(list) do
            for _, p in ipairs(prefixes) do
                if name:sub(1, #p) == p then
                    tried = tried + 1
                    if PatchMeta(name) then n = n + 1 end
                    break
                end
            end
        end
    end
    W("HDR", string.format("поля: обёрнуто %d из %d классов/структур за %.0f мс", n, tried, (os.clock() - t0) * 1000))
end

-- ---------------------------------------------------------------- call tracing

local function StartTrace()
    local filters = Split(Cfg.trace_filter)
    local tcount = {}
    debug.sethook(function()
        local info = debug.getinfo(2, "Sn")
        if not info or info.what ~= "Lua" then return end
        local src = info.short_src or "?"
        for _, f in ipairs(filters) do
            if src:find(f, 1, true) then
                local key = src .. ":" .. (info.linedefined or 0)
                local c = (tcount[key] or 0) + 1
                tcount[key] = c
                if c <= CallLimit or c % 500 == 0 then
                    W("TRACE", key .. " " .. (info.name or "?") .. (c > CallLimit and (" #" .. c) or ""))
                end
                return
            end
        end
    end, "c")
end

-- ---------------------------------------------------------------- startup

function M.Start()
    if Started then return end
    Started = true
    if not ReadConfig() then return end
    -- auto instances (-dbserver / -dbclient) are not touched; -dbprobe enables the probe bypassing enabled and these flags
    local okc, cmd = pcall(function() return UE.UKismetSystemLibrary.GetCommandLine() end)
    local padded = " " .. ((okc and type(cmd) == "string") and cmd:lower() or "") .. " "
    if not padded:find(" -dbprobe ", 1, true) then
        if Cfg.enabled ~= "1" then return end
        for _, flag in ipairs(Split(Cfg.skip_cmdline)) do
            if padded:find(" " .. flag:lower() .. " ", 1, true) then return end
        end
    end
    local client = (Cfg.name ~= "" and Cfg.name) or (io.open("local_save.txt", "r") and "alt" or "steam")
    if padded:find(" -dbserver ", 1, true) then
        client = client .. "-srv"
    elseif padded:find(" -dbclient ", 1, true) then
        client = client .. "-cli"
    end
    local path = string.format("%s/%s_%s_lua.log", Cfg.log_dir, client, os.date("%Y%m%d-%H%M%S"))
    Out = io.open(path, "w")
    if not Out then
        -- this machine has no log_dir (someone else's copy, owner's default path) — like ClientProbe: into the game's Saved
        path = (os.getenv("LOCALAPPDATA") or ".") .. string.format("/Dungeonborne/Saved/RebuildProbe/%s_%s_lua.log",
            client, os.date("%Y%m%d-%H%M%S"))
        Out = io.open(path, "w")
    end
    if not Out then return end
    CallLimit = tonumber(Cfg.call_limit) or 40
    ModPrefixes, ModSkip, FnSkip = Split(Cfg.modules), Split(Cfg.skip_modules), Split(Cfg.skip)
    W("HDR", "LuaProbe client=" .. client .. " " .. _VERSION)

    -- Already loaded modules (services are loaded at game start). Risk: a handler subscribed with the original function
    -- before wrapping will not be unsubscribed by the wrapper later (comparison by reference) — if the UI acts strange, wrap_loaded=0.
    local n = 0
    if Cfg.wrap_loaded ~= "0" then
        local names = {}
        for name in pairs(package.loaded) do names[#names + 1] = name end
        for _, name in ipairs(names) do
            if WantModule(name) then n = n + WrapModule(name, package.loaded[name]) end
        end
    end
    W("HDR", "wrapped functions in loaded modules: " .. n)
    HookRequire()
    pcall(WrapTextStatics)
    if Cfg.fields == "1" then pcall(PatchFields) end
    if Cfg.trace == "1" then pcall(StartTrace) end
end

return M
