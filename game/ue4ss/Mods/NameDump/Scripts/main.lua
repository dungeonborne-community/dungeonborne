-- Mods/NameDump/Scripts/main.lua
-- Export of the game's localisation strings for captions in the balance CSV (cargo xtask balance names-import).
-- In the lobby press F6: all StringTables -> %LOCALAPPDATA%\Dungeonborne\Saved\names_dump.tsv
-- Line format: <TableId>\t<Key>\t<text in the game language>   (\t and \n in the text are escaped as \\t, \\n)
-- Read only: calls the game's own KismetStringTableLibrary / KismetTextLibrary, changes nothing.
--
-- v2: UE4SS may return a TArray as userdata (ForEach) or as a Lua table — both variants are handled.
--     If GetRegisteredStringTables is empty/nil — take the loaded StringTable objects (id = asset path).

local OutPath = (os.getenv("LOCALAPPDATA") or ".") .. "\\Dungeonborne\\Saved\\names_dump.tsv"

local function Log(msg) print("[NameDump] " .. tostring(msg) .. "\n") end

local function Esc(s)
    return (tostring(s or ""):gsub("\\", "\\\\"):gsub("\t", "\\t"):gsub("\r", ""):gsub("\n", "\\n"))
end

-- value of an array element / result as a string (string, FName, FString, FText, RemoteUnrealParam)
local function Str(x)
    if x == nil then return nil end
    if type(x) == "string" then return x end
    local ok, s = pcall(function() return x:ToString() end)
    if ok and type(s) == "string" then return s end
    ok, s = pcall(function() return x:get():ToString() end)
    if ok and type(s) == "string" then return s end
    ok, s = pcall(function() return x:get() end)
    if ok and type(s) == "string" then return s end
    return nil
end

-- iterating a TArray (userdata with ForEach) or a Lua table
local function Each(arr, fn)
    if arr == nil then return 0 end
    local n = 0
    if type(arr) == "table" then
        for _, v in ipairs(arr) do n = n + 1; fn(v) end
        return n
    end
    local ok = pcall(function()
        arr:ForEach(function(_, e) n = n + 1; fn(e) end)
    end)
    if not ok then
        pcall(function()
            for i = 1, #arr do n = n + 1; fn(arr[i]) end
        end)
    end
    return n
end

local function TableIds(stLib)
    local ids = {}
    local res = nil
    local ok, err = pcall(function() res = stLib:GetRegisteredStringTables() end)
    Log(string.format("GetRegisteredStringTables -> ok=%s type=%s %s", tostring(ok), type(res), ok and "" or tostring(err)))
    Each(res, function(e) local s = Str(e); if s then ids[#ids + 1] = s end end)
    if #ids == 0 then
        -- fallback: loaded StringTable assets, table id = object path (/Game/....Name.Name)
        local objs = FindAllOf("StringTable") or {}
        for _, o in ipairs(objs) do
            local full = Str(o:GetFullName()) or ""
            local path = full:match("^%S+%s+(.+)$")
            if path then ids[#ids + 1] = path end
        end
        Log(string.format("fallback FindAllOf(StringTable): %d", #ids))
    end
    return ids
end

local function Dump()
    local stLib = StaticFindObject("/Script/Engine.Default__KismetStringTableLibrary")
    local txtLib = StaticFindObject("/Script/Engine.Default__KismetTextLibrary")
    if not (stLib and stLib:IsValid() and txtLib and txtLib:IsValid()) then
        Log("KismetStringTableLibrary / KismetTextLibrary not found")
        return
    end
    local out = io.open(OutPath, "w")
    if not out then
        Log("cannot open " .. OutPath)
        return
    end
    local ids = TableIds(stLib)
    local keysN, failN, emptyTables = 0, 0, 0
    for _, tidStr in ipairs(ids) do
        local tid = FName(tidStr)
        local keys = nil
        pcall(function() keys = stLib:GetKeysFromStringTable(tid) end)
        local n = Each(keys, function(k)
            local key = Str(k)
            if not key then failN = failN + 1; return end
            local text = nil
            pcall(function() text = Str(txtLib:TextFromStringTable(tid, key)) end)
            if text then
                out:write(Esc(tidStr), "\t", Esc(key), "\t", Esc(text), "\n")
                keysN = keysN + 1
            else
                failN = failN + 1
            end
        end)
        if n == 0 then emptyTables = emptyTables + 1 end
    end
    out:close()
    Log(string.format("tables %d (empty %d), strings %d, errors %d -> %s", #ids, emptyTables, keysN, failN, OutPath))
end

RegisterKeyBind(Key.F6, function()
    ExecuteInGameThread(function()
        local ok, err = pcall(Dump)
        if not ok then Log("ERROR: " .. tostring(err)) end
    end)
end)

Log("v2 loaded: F6 in the lobby — localisation export to " .. OutPath)
