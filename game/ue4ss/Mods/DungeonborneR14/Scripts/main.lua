-- R14 (crash 0x142f602c6) — the pair to the BP_Tgf_Hero_Default patch (bp_patches\tools\patch_hero.py, pak zzzzzzz_BPFix_P).
-- The patch on the client (not authority) creates in the hero's UCS an empty NON-replicated TgfEquipmentManagerComponent: the native
-- manager (GameFeature, server only) arrives over the network later than the body timer (~2 s), and without a manager the body crashes
-- on GetEquipBySlot(null). When the native one has arrived, its OnRep fire (the stub never gets them — it is not
-- replicated) — then we remove the stub so that GetComponentByClass returns the native one (with items and weapons).
-- No UE4SS calls in NotifyOnNewObject: only the OnRep hook.
-- 02.11: the stub is removed right in the hook (it fires BEFORE the OnRep body), a retry — on the next tick. Before, it was removed
-- only on the next tick: the native OnRep_CurrentWeaponSlot managed to call the listeners, they got the stub via GetComponentByClass
-- (GetCurrentWeaponSlot "Owner:[]") and sent a server RPC from it — the server dropped the connection
-- («Client attempted to create sub-object», ue-server-7777.log 02.11 08:35–08:52).

local Root = (os.getenv("LOCALAPPDATA") or ".") .. "/Dungeonborne/Saved/RebuildProbe/"
local function Log(msg)
    local f = io.open(Root .. "r14-stub.log", "a")
    if f then f:write(os.date("%H:%M:%S") .. " | " .. msg .. "\n"); f:close() end
end

local function Short(obj)
    local name = "nil"
    pcall(function() name = obj:GetFullName():match("([^%.%s]+)$") end)
    return name
end

local EQ_CLASS = "/Script/LyraGame.TgfEquipmentManagerComponent"
local Done = {} -- native managers for which the stub has already been removed

-- Removes stubs until GetComponentByClass returns the native manager; returns the number removed and the result
local function Strip(native, key)
    local owner = native:GetOwner()
    local cls = StaticFindObject(EQ_CLASS)
    local removed = 0
    for _ = 1, 4 do
        local first = owner:GetComponentByClass(cls)
        if not first or not first:IsValid() or Short(first) == key then break end
        first:K2_DestroyComponent(first)
        removed = removed + 1
    end
    return owner, removed, Short(owner:GetComponentByClass(cls))
end

local function RemoveStubs(native)
    local key = Short(native)
    if Done[key] then return end
    Done[key] = true
    local okNow, owner, removed, now = pcall(Strip, native, key)
    Log(string.format("%s: native %s arrived, removed at once %s -> GetComponentByClass=%s%s",
        okNow and Short(owner) or "?", key, tostring(removed), tostring(now), okNow and "" or (" error " .. tostring(owner))))
    ExecuteInGameThread(function()
        pcall(function()
            if not native:IsValid() then return end
            local o, r, n = Strip(native, key)
            if r > 0 then Log(string.format("%s: removed %d more on the next tick -> %s", Short(o), r, n)) end
        end)
    end)
end

for _, fn in ipairs({ "/Script/LyraGame.LyraEquipmentManagerComponent:OnRep_EquipmentList",
    "/Script/LyraGame.TgfEquipmentManagerComponent:OnRep_WeaponSlotList",
    "/Script/LyraGame.TgfEquipmentManagerComponent:OnRep_CurrentWeaponSlot" }) do
    local ok = pcall(function()
        RegisterHook(fn, function(ctx)
            local comp = nil
            pcall(function() comp = ctx:get() end)
            if comp then RemoveStubs(comp) end
        end)
    end)
    Log("hook " .. fn:match(":(.+)$") .. " ok=" .. tostring(ok))
end
