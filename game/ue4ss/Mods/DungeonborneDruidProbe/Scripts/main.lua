-- Mods/DungeonborneDruidProbe/Scripts/main.lua
-- 12.11: diagnostics "druid trees do not hit mobs" (user's complaint; BP analysis — LOCAL session).
-- Tree = B_TGF_DruidPanther_Seed_C (descendant of B_TGF_Static_OverlapCheck_Destructable_ProjectileBase): on a timer
-- CapsuleTraceMulti → team filter (CompareTeams(Instigator, target) == DifferentTeams) → players into FinalPlayerTargets,
-- the rest into FinalTargets → PlayAtkAnim → Delay(AtkDelayEffectTime) → ApplyProjectileEffect(random target).
-- Battle server only (-dbserver), read only. Writes <dbroot>/druidprobe-<date-time>.log:
--   SEED   — a new tree: who placed it, team, AtkDelayEffectTime, capsule radius
--   ATK    — PlayAtkAnim (post-hook): FinalTargets / FinalPlayerTargets (names, count)
--   NEAR   — every 1 s while the tree is alive: mobs and heroes within near_cm — distance, MyTeamID of the character and the controller,
--            CompareTeams(Instigator, target) (0 own, 1 foreign, 2 error), HP
--   APPLY  — ApplyProjectileEffect of the tree (before/after): target, class, HP before and after (only apply_hook = 1: the function has
--            a struct parameter FHitResult, hooks on such functions already crashed the game for us in UE4SS — off by default.)
-- Settings — settings.ini next to the mod ([druid] enabled, near_cm, apply_hook).

local Root = "dbtools/run/"
local SEED = "B_TGF_DruidPanther_Seed_C"
local SEED_FN = "/Game/TGF/Character/Hero/DruidPanther/GameAbility/B_TGF_DruidPanther_Seed.B_TGF_DruidPanther_Seed_C:PlayAtkAnim"
local Cfg = { enabled = 1, near_cm = 600, apply_hook = 0, fix_bones = 0 }
local Out, Off, Started = nil, false, false
local Seeds = {}          -- address → { at = os.clock() }
local AtkHooked, ApplyHooked = false, false
local TeamSub, DefCls, AttrCls = nil, nil, nil

local function Log(s) print("[DruidProbe] " .. tostring(s) .. "\n") end

local function W(kind, text)
    if not Out then return end
    Out:write(os.date("%H:%M:%S"), string.format(".%03d", math.floor((os.clock() % 1) * 1000)), "\t", kind, "\t",
        tostring(text):gsub("[\r\n\t]", " "), "\n")
    Out:flush()
end

local function Ok(o)
    if o == nil then return false end
    local ok, v = pcall(function() return o:IsValid() end)
    return ok and v
end

local function Get(fn)
    local ok, v = pcall(fn)
    if ok then return v end
    return nil
end

local function Name(o)
    if not Ok(o) then return "nil" end
    return Get(function() return o:GetFName():ToString() end) or "?"
end

local function Addr(o) return Get(function() return o:GetAddress() end) or 0 end

local function Loc(a)
    local v = Get(function() return a:K2_GetActorLocation() end)
    if not v then return nil end
    local x, y, z = Get(function() return v.X end), Get(function() return v.Y end), Get(function() return v.Z end)
    if type(x) ~= "number" then return nil end
    return x, y, z
end

local function Dist(a, b)
    local ax, ay, az = Loc(a)
    local bx, by, bz = Loc(b)
    if not ax or not bx then return nil end
    return math.sqrt((ax - bx) ^ 2 + (ay - by) ^ 2 + (az - bz) ^ 2)
end

local function Num(v)
    if type(v) ~= "number" then return tostring(v) end
    if v == math.floor(v) then return string.format("%d", v) end
    return string.format("%.2f", v)
end

-- team of the character and its controller (MyTeamID: FGenericTeamId { TeamID })
local function Teams(ch)
    local t = Get(function() return ch.MyTeamID.TeamID end)
    local ctl = Get(function() return ch.Controller end)
    local ct = Ok(ctl) and Get(function() return ctl.MyTeamID.TeamID end) or nil
    if ct == nil and Ok(ctl) then
        local ps = Get(function() return ctl.PlayerState end)
        if Ok(ps) then ct = Get(function() return ps.MyTeamID.TeamID end) end
    end
    return string.format("team=%s ctl=%s(%s)", Num(t), Name(ctl), Num(ct))
end

local function Hp(ch)
    if not Ok(ch) then return "?" end
    AttrCls = (Ok(AttrCls) and AttrCls) or StaticFindObject("/Script/LyraGame.TgfAttributeComponent")
    DefCls = (Ok(DefCls) and DefCls) or StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
    local set = Get(function() return ch:GetComponentByClass(AttrCls):GetAttributeSet(DefCls) end)
    if not Ok(set) then return "?" end
    return Num(Get(function() return set.CurHealth.CurrentValue end))
end

local function Compare(inst, target)
    if not Ok(TeamSub) then
        TeamSub = nil
        for _, s in ipairs(Get(function() return FindAllOf("LyraTeamSubsystem") end) or {}) do
            if Ok(s) and not Name(s):find("Default__", 1, true) then TeamSub = s; break end
        end
    end
    if not TeamSub then return "nosub" end
    local r = Get(function() return TeamSub:CompareTeams(inst, target, {}, {}) end)
    return tostring(r)
end

local function ListArr(arr)
    local names, n = {}, 0
    pcall(function()
        arr:ForEach(function(_, e)
            n = n + 1
            local o = Get(function() return e:get() end)
            if #names < 8 then names[#names + 1] = Name(o) end
        end)
    end)
    return n, table.concat(names, ",")
end

local function InstOf(seed)
    local inst = Get(function() return seed.Instigator end)
    if not Ok(inst) then inst = Get(function() return seed:GetOwner() end) end
    return inst
end

local function HookAtk()
    if AtkHooked then return end
    local ok, err = pcall(function()
        RegisterHook(SEED_FN, function(Ctx)
            local seed = Get(function() return Ctx:get() end)
            if not Ok(seed) then return end
            local n1, t1 = ListArr(Get(function() return seed.FinalTargets end))
            local n2, t2 = ListArr(Get(function() return seed.FinalPlayerTargets end))
            W("ATK", string.format("seed=%x inst=%s targets=%d[%s] players=%d[%s] delay=%s", Addr(seed), Name(InstOf(seed)),
                n1, t1, n2, t2, Num(Get(function() return seed.AtkDelayEffectTime end))))
        end)
    end)
    AtkHooked = ok
    W("HOOK", "PlayAtkAnim: " .. (ok and "ok" or ("error " .. tostring(err))))
end

local function HookApply()
    if ApplyHooked or Cfg.apply_hook ~= 1 then return end
    local Pending = {}
    local ok, err = pcall(function()
        RegisterHook("/Script/LyraGame.TgfProjectileBase:ApplyProjectileEffect", function(Ctx, Target, Hit)
            local p = Get(function() return Ctx:get() end)
            local isSeed = Ok(p) and Name(p):find("DruidPanther_Seed", 1, true) ~= nil
            local t = isSeed and Get(function() return Target:get() end) or nil
            Pending[#Pending + 1] = isSeed and { seed = p, t = t, hp = Hp(t) } or false
            if isSeed then
                -- 12.11 (LOCAL): HitResult — component, bone, penetration, points (skeleton: the hitbox is the bones' physics bodies)
                local h = Get(function() return Hit:get() end)
                local comp = h and (Get(function() return h.Component:Get() end) or Get(function() return h.Component end)) or nil
                local function V(v)
                    if not v then return "?" end
                    local x, y, z = Get(function() return v.X end), Get(function() return v.Y end), Get(function() return v.Z end)
                    return type(x) == "number" and string.format("(%.0f,%.0f,%.0f)", x, y, z) or "?"
                end
                W("APPLY", string.format("before seed=%x target=%s class=%s hp=%s %s | hit comp=%s(%s) bone=%s pen=%s loc=%s impact=%s",
                    Addr(p), Name(t), Name(Get(function() return t:GetClass() end)), Hp(t), Ok(t) and Teams(t) or "",
                    Name(comp), Name(Ok(comp) and Get(function() return comp:GetClass() end) or nil),
                    h and (Get(function() return h.BoneName:ToString() end) or "?") or "?",
                    h and tostring(Get(function() return h.bStartPenetrating end)) or "?",
                    h and V(Get(function() return h.Location end)) or "?", h and V(Get(function() return h.ImpactPoint end)) or "?"))
            end
        end, function()
            local e = table.remove(Pending)
            if e then W("APPLY", string.format("after seed=%x target=%s hp %s -> %s", Addr(e.seed), Name(e.t), e.hp, Hp(e.t))) end
        end)
    end)
    ApplyHooked = ok
    W("HOOK", "ApplyProjectileEffect: " .. (ok and "ok" or ("error " .. tostring(err))))
end

local function Tick()
    -- 12.11 run 1: the real tree in a match — BP_TGF_Summoned_Seed_C (a pawn, controller AIC_TGF_DruidSeed_C)
    local seeds = {}
    for _, cls in ipairs({ SEED, "BP_TGF_Summoned_Seed_C" }) do
        for _, s in ipairs(Get(function() return FindAllOf(cls) end) or {}) do seeds[#seeds + 1] = s end
    end
    local alive = {}
    for _, seed in ipairs(seeds) do
        if Ok(seed) and not Name(seed):find("Default__", 1, true) then
            local a = Addr(seed)
            alive[a] = true
            local inst = InstOf(seed)
            if not Seeds[a] then
                Seeds[a] = { at = os.clock() }
                HookAtk()
                HookApply()
                W("SEED", string.format("seed=%x %s seed_%s inst=%s %s cmp(seed,inst)=%s delay=%s capsule=%s/%s off=%s", a,
                    Name(seed), Teams(seed), Name(inst), Ok(inst) and Teams(inst) or "", Ok(inst) and Compare(seed, inst) or "-",
                    Num(Get(function() return seed.AtkDelayEffectTime end)),
                    Num(Get(function() return seed.CapsuleRadius end)), Num(Get(function() return seed.CapsuleHalfHeight end)),
                    Num(Get(function() return seed.HeightOffset end))))
            end
            local near = {}
            for _, ch in ipairs(Get(function() return FindAllOf("LyraCharacter") end) or {}) do
                if Ok(ch) and not Name(ch):find("Default__", 1, true) and Addr(ch) ~= Addr(inst) then
                    local d = Dist(seed, ch)
                    if d and d <= Cfg.near_cm then
                        local extra = ""
                        -- 12.11 (LOCAL): a skeleton's hitbox is the mesh bones; on a headless server the bones may not move
                        if Name(ch):find("Skeleton", 1, true) then
                            local mesh = Get(function() return ch.Mesh end)
                            -- fix_bones experiment: skeleton bones update even without rendering (AlwaysTickPoseAndRefreshBones);
                            -- the monster BP rolls back the cache — set it every second
                            -- run 3: one mode is not enough (the bones are still at one point) — mob meshes on the server apparently
                            -- have the component tick disabled (as with heroes, HeroFix in RuServer enables it and sets 0)
                            local tickOn = Ok(mesh) and Get(function() return mesh:IsComponentTickEnabled() end)
                            if Cfg.fix_bones == 1 and Ok(mesh) then
                                pcall(function() mesh:SetComponentTickEnabled(true) end)
                                pcall(function() mesh.VisibilityBasedAnimTickOption = 0 end)
                            end
                            local function S(sock)
                                local v = Ok(mesh) and Get(function() return mesh:GetSocketLocation(FName(sock)) end) or nil
                                local x, y, z = v and Get(function() return v.X end), v and Get(function() return v.Y end), v and Get(function() return v.Z end)
                                return type(x) == "number" and string.format("%s=(%.0f,%.0f,%.0f)", sock, x, y, z) or (sock .. "=?")
                            end
                            local ax, ay, az = Loc(ch)
                            extra = string.format(" actor=(%s,%s,%s) %s %s tickopt=%s rendered=%s meshtick=%s", ax and string.format("%.0f", ax) or "?",
                                ay and string.format("%.0f", ay) or "?", az and string.format("%.0f", az) or "?", S("pelvis"), S("foot_l"),
                                Num(Ok(mesh) and Get(function() return mesh.VisibilityBasedAnimTickOption end)),
                                tostring(Ok(mesh) and Get(function() return mesh.bRecentlyRendered end)), tostring(tickOn))
                        end
                        near[#near + 1] = string.format("%s d=%.0f %s cmp(inst)=%s cmp(seed)=%s hp=%s%s", Name(ch), d, Teams(ch),
                            Ok(inst) and Compare(inst, ch) or "-", Compare(seed, ch), Hp(ch), extra)
                    end
                end
            end
            W("NEAR", string.format("seed=%x n=%d | %s", a, #near, table.concat(near, " | ")))
        end
    end
    for a in pairs(Seeds) do
        if not alive[a] then
            W("GONE", string.format("seed=%x lived %.1f s", a, os.clock() - Seeds[a].at))
            Seeds[a] = nil
        end
    end
end

local function ReadSettings()
    local f = io.open("ue4ss/Mods/DungeonborneDruidProbe/settings.ini", "r") or io.open("Mods/DungeonborneDruidProbe/settings.ini", "r")
    if not f then return end
    for line in f:lines() do
        line = line:gsub("[;#].*$", "")
        local k, v = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
        if k and Cfg[k] ~= nil and tonumber(v) then Cfg[k] = tonumber(v) end
    end
    f:close()
end

local function Init()
    local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    if not Ok(ksl) then return false end
    local cl = ksl:GetCommandLine():ToString()
    if not (" " .. cl:lower() .. " "):find("%s%-dbserver%s") then
        Log("no -dbserver — mod disabled")
        Off = true
        return true
    end
    local custom = cl:match("%-dbroot=\"([^\"]*)\"") or cl:match("%-dbroot=(%S+)")
    if custom then Root = custom:gsub("\\", "/"):gsub("/?$", "/") end
    ReadSettings()
    if Cfg.enabled ~= 1 then Log("settings.ini: enabled = 0"); Off = true; return true end
    local name = Root .. "druidprobe-" .. os.date("%Y%m%d-%H%M%S") .. ".log"
    Out = io.open(name, "a")
    if not Out then Log("cannot open " .. name); Off = true; return true end
    W("META", string.format("druidprobe 1 (12.11) near_cm=%s apply_hook=%s", Num(Cfg.near_cm), Num(Cfg.apply_hook)))
    Log("writing " .. name)
    return true
end

local Loop
-- timer registration — from the game thread (docs/CRASH_LONG_PLAY.md, memory: game thread timers)
ExecuteInGameThread(function()
    Loop = LoopInGameThreadWithDelay(1000, function()
        if Off then CancelDelayedAction(Loop); return end
        if not Started then
            local ok, done = pcall(Init)
            if not ok then Log("Init: " .. tostring(done)); Off = true; return end
            if done then Started = true end
            return
        end
        local ok, err = pcall(Tick)
        if not ok then W("ERR", tostring(err)) end
    end)
end)

Log("loaded")
