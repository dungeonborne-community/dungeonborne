-- Mods/DungeonborneStatWatch/Scripts/main.lua
-- Combat math watcher on the game server (DS). A separate mod: the main block of DungeonborneRuServer already has
-- 200 locals (Lua limit), a new local breaks its loading.
--
-- Enabled by the command-line flag -dbserver (like DungeonborneRuServer) or -dbstatwatch; silent for regular clients.
-- Writes JSON Lines to <dbroot>/statwatch-<date>-<time>.jsonl (dbroot from -dbroot=..., otherwise dbtools/run).
-- Records (field "t"):
--   meta   — start, command line (without tokens), mod version
--   hooks  — which hooks were set
--   flcfg  — FirstLevelAttrDataAsset: primary attribute -> [{secondary, multiplier}]
--   snap   — hero snapshot: all GAS attribute sets, for each FGameplayAttributeData [BaseValue, CurrentValue]
--            (written on change and every SNAP_FORCE_SEC), open_id, role_guid, weapon in hand
--   hit    — a hit via ApplyGE / ApplyAOEWithGA / Before+AfterApplyGEWithSpec: who, whom, GE, DamageRate, WeaponAttack,
--            target health/shield before and after, damage, crit (IsCritical: the random number and the result), CalcHitValue,
--            GetWeaponAttack/GetDefendValue/GetAttackWeight, the source's attack attributes and the target's defence ones
--   hp     — a health/shield drop not explained by any hook (DoT, falling, damage bypassing the BP library)
--   calls  — hook fire counters every STATS_SEC
-- Settings — <dbroot>/statwatch-config.txt, key=value lines: enabled=0|1, snap_sec, snap_force_sec, stats_sec,
-- hooks=ApplyGE,IsCritical,... (list of enabled ones; all by default), monsters=0|1 (monster snapshots on a hit).
--
-- Careful (ClientProbe experience, 31.10): FString strings of live objects are read only where the server mod already does it
-- (MyOpenId) and in the FirstLevelAttr config; everything in pcall. Process memory is not read — only UObject properties via UE4SS.

local VERSION = "statwatch 2 (05.11.2026)"
local Root = "dbtools/run/"
local Cfg = {
    enabled = true, snap_sec = 5, snap_force_sec = 120, stats_sec = 60, monsters = true, -- health poll — 200 ms
    hooks = nil, -- nil = all
}
local Out = nil         -- jsonl file
local Started = false
local Off = false
local T0 = os.clock()

-- Fallback list of set fields (CXXHeaderDump LyraGame.hpp): in case walking the class properties via UE4SS does not work
local FALLBACK = {
    LyraCombatSet = { "BaseDamage", "BaseHeal" },
    LyraHealthSet = { "Health", "MaxHealth", "Healing", "Damage" },
    LyraMovementSet = { "MoveSpeed" },
    TgfAIAttributeSet = { "AngleSpeedBase", "AngleSpeedPercent", "SightDistanceBase", "SightDistancePercent", "HearingDistanceBase", "HearingDistancePercent", "LightDistanceBase", "LightDistancePercent" },
    TgfAttackAttributeSet = { "RightWeaponAttack", "LeftWeaponAttack", "AdditionalWeaponAttack", "AttackBonus", "AttackBoostRate", "MoveFactorRate", "CriticalRate", "CritDamageRate", "EliteBoostRate", "UncontrolledBoostRate", "SkillBoostRate", "ElementBoostRateNone", "ElementBoostRateFire", "ElementBoostRateIce", "ElementBoostRateThunder", "ElementBoostRateLight", "ElementBoostRateDark", "ElementResPierceNone", "ElementResPierceFire", "ElementResPierceIce", "ElementResPierceThunder", "ElementResPierceLight", "ElementResPierceDark", "RightAttGearLevel", "LeftAttGearLevel", "AdditionalAttGearLevel", "ExtraAttGearLevel", "RightAttackWeight", "LeftAttackWeight", "AdditionalAttackWeight", "DefIgnoreBase", "DefIgnorePercent" },
    TgfBuffArgAttributeSet = { "BurningStack", "BurningDamage", "BurningIntervalBase", "BurningIntervalPercent", "FreezingRateBase", "FreezingRatePercent", "FreezingDurationBase", "FreezingDurationPercent", "BloodingDamage", "BloodingDurationBase", "BloodingDurationPercent", "BloodingIntervalBase", "BloodingIntervalPercent", "KillingMaxLayerMaxHealthPercent", "KillingMaxLayerAttackBoostRate", "KillingMaxLayerCriticalRate" },
    TgfDefendAttributeSet = { "CurHealth", "CurShield", "AddShield", "CalculatedShield", "PermanintShield", "DamageReduceRate", "DamageVulnerRate", "EliteReduceRate", "UncontrolledReduceRate", "RightShieldAngle", "LeftShieldAngle", "RightGuardedDamageReduceRate", "LeftGuardedDamageReduceRate", "ExtraGuardedDamageReduceRate", "RightDefendValue", "LeftDefendValue", "DefaultDefendValue", "ElementResistanceRateNone", "ElementResistanceRateFire", "ElementResistanceRateIce", "ElementResistanceRateThunder", "ElementResistanceRateLight", "ElementResistanceRateDark", "RightDefGearLevel", "LeftDefGearLevel", "ChestDefGearLevel", "TrousersDefGearLevel", "HelmetDefGearLevel", "ExtraDefGearLevel", "MaxHealthBase", "MaxHealthPercent", "MaxHealthExtra", "HealingValue", "DamageValue", "CriticalDamage", "CriticalHealing" },
    TgfFirstLevelAttributeSet = { "Strength", "Dexterity", "Stamina", "Intelligence", "Will", "Faith" },
    TgfMagicAttributeSet = { "CurMagic", "MagicOnHit", "MagicOnKill", "MagicOnHurt", "MagicOverTime", "MagicGenerateRate", "MaxMagicBase", "MaxMagicPercent", "MagicDeltaValue" },
    TgfNaturalEnergyAttributeSet = { "CurNaturalEnergy", "NaturalEnergyOnHit", "NaturalEnergyOnKill", "NaturalEnergyOnHurt", "NaturalEnergyOverTime", "NaturalEnergyGenerateRate", "MaxNaturalEnergyBase", "MaxNaturalEnergyPercent", "NaturalEnergyDeltaValue" },
    TgfRecoverAttributeSet = { "LifeLeachRate", "HealWoundRate", "HealOnHit", "HealOnKill", "HealOverTime", "HealBonus", "HealBoostRate", "HealReceiveRate", "ShieldBonus", "ShieldBoostRate", "ShieldReceiveRate", "ResCostReduceRate", "ResGenerateRate", "SkillCooldownScaleRate", "GainGoldOutOfPortal", "ExpBoostRateOutOfPortal" },
    TgfSoulEnergyAttributeSet = { "CurSoulEnergy", "SoulEnergyOnHit", "SoulEnergyOnKill", "SoulEnergyOnHurt", "SoulEnergyGenerateRate", "MaxSoulEnergyBase", "MaxSoulEnergyPercent", "SoulEnergyDeltaValue" },
    TgfSpeedAttributeSet = { "DeathZoneTime", "AttackSpeedRate", "RecoverableItemConsumeSpeedRate", "WeaponChangeSpeedRate", "ThrowableItemConsumeSpeedRate", "MoveSpeedBase", "MoveSpeedPercent", "MoveSpeedRate", "LevelInteractSpeedRate", "LevelInteractSpeedAdditional", "SpellCastSpeedRate", "SpellCastSpeedAdditional" },
    TgfStatEffectAttributeSet = { "MoveSpeedWithHitRunning", "HealOverTimeWithHitRunning", "CriticalRateWithHitRunning", "LifeLeachRateWithHitRunning", "AttackBoostRateWithFullHealth", "CriticalRateWithFullHealth" },
    TgfTriggerEffectAttributeSet = { "AdditionalFixedDamageOnCausingHit", "AdditionalElementNoneDamageOnCausingHit", "AdditionalElementFireDamageOnCausingHit", "AdditionalElementIceDamageOnCausingHit", "AdditionalElementThunderDamageOnCausingHit", "AdditionalElementLightDamageOnCausingHit", "AdditionalElementDarkDamageOnCausingHit", "HealOnOpenChest", "MagicOnOpenChest", "GainGoldOnOpenChest", "GainGoldOnKill", "HealOnDefendSuccess", "DamageOnDefendSuccess", "AddShieldOnCastSkill" },
}

-- ---------------------------------------------------------------------------------------------------------------
-- JSON
local function JStr(s)
    s = tostring(s)
    s = s:gsub('[%c"\\]', function(c)
        if c == '"' then return '\\"' elseif c == "\\" then return "\\\\" elseif c == "\n" then return "\\n" end
        return string.format("\\u%04x", c:byte())
    end)
    return '"' .. s .. '"'
end

local function JNum(n)
    if n ~= n or n == math.huge or n == -math.huge then return "null" end
    if math.type and math.type(n) == "integer" then return tostring(n) end
    if n == math.floor(n) and math.abs(n) < 1e15 then return string.format("%d", n) end
    return string.format("%.7g", n)
end

local Enc
Enc = function(v)
    local tv = type(v)
    if tv == "nil" then return "null"
    elseif tv == "boolean" then return v and "true" or "false"
    elseif tv == "number" then return JNum(v)
    elseif tv == "string" then return JStr(v)
    elseif tv == "table" then
        if v[1] ~= nil or next(v) == nil and getmetatable(v) == "array" then
            local p = {}
            for i = 1, #v do p[i] = Enc(v[i]) end
            return "[" .. table.concat(p, ",") .. "]"
        end
        local keys = {}
        for k in pairs(v) do keys[#keys + 1] = tostring(k) end
        table.sort(keys)
        local p = {}
        for _, k in ipairs(keys) do
            local val = v[k]
            if val == nil then val = v[tonumber(k)] end
            p[#p + 1] = JStr(k) .. ":" .. Enc(val)
        end
        return "{" .. table.concat(p, ",") .. "}"
    end
    return JStr(tostring(v))
end

local function Ms() return math.floor((os.clock() - T0) * 1000) end

local function Emit(rec)
    if Off or not Out then return end
    rec.ts = os.time()
    rec.ms = Ms()
    local ok, line = pcall(Enc, rec)
    if not ok then line = '{"t":"enc_error","err":' .. JStr(line) .. '}' end
    Out:write(line, "\n")
    Out:flush()
end

local function Log(msg)
    print("[StatWatch] " .. tostring(msg) .. "\n")
end

-- ---------------------------------------------------------------------------------------------------------------
-- Objects
local function IsValid(o)
    local ok, r = pcall(function() return o ~= nil and o:IsValid() end)
    return ok and r
end

local function FName(o)
    local ok, n = pcall(function() return o:GetFName():ToString() end)
    return ok and n or "?"
end

local function ClassName(o)
    local ok, n = pcall(function() return o:GetClass():GetFName():ToString() end)
    return ok and n or "?"
end

local function IsCDO(o) return FName(o):find("^Default__") ~= nil end

local function Addr(o)
    local ok, a = pcall(function() return o:GetAddress() end)
    return ok and a or nil
end

local function R(v) -- hook parameter -> value
    local ok, x = pcall(function() return v:get() end)
    if ok then return x end
    return v
end

local Lib = nil
local function BpLib()
    if not IsValid(Lib) then Lib = StaticFindObject("/Script/LyraGame.Default__TgfAbilitySystemBlueprintLibrary") end
    return IsValid(Lib) and Lib or nil
end

-- FGameplayAttributeData fields of a set class (with parents), cache by class name
local FieldCache = {}
local function FieldsOf(Set)
    local cname = ClassName(Set)
    if FieldCache[cname] then return FieldCache[cname] end
    local list, seen = {}, {}
    pcall(function()
        local cls = Set:GetClass()
        local guard = 0
        while cls and cls:IsValid() and guard < 8 do
            guard = guard + 1
            local nm = FName(cls)
            if nm == "AttributeSet" or nm == "LyraAttributeSet" or nm == "Object" then break end
            cls:ForEachProperty(function(P)
                pcall(function()
                    if P:GetClass():GetFName():ToString() ~= "StructProperty" then return end
                    if P:GetStruct():GetFName():ToString() ~= "GameplayAttributeData" then return end
                    local f = P:GetFName():ToString()
                    if not seen[f] then seen[f] = true; list[#list + 1] = f end
                end)
            end)
            cls = cls:GetSuperStruct()
        end
    end)
    local src = "reflect"
    if #list == 0 then
        local base = cname:gsub("^BP_", ""):gsub("_C$", "")
        for k, v in pairs(FALLBACK) do
            if base == k or base:find(k, 1, true) then list = v; src = "fallback"; break end
        end
    end
    FieldCache[cname] = list
    Emit({ t = "fields", cls = cname, src = src, n = #list })
    return list
end

-- The character's attribute sets: ASC.SpawnedAttributes (all, including Custom*), otherwise GetAttributeSet by classes
local SET_CLASSES = { "TgfFirstLevelAttributeSet", "TgfAttackAttributeSet", "TgfDefendAttributeSet", "TgfSpeedAttributeSet",
    "TgfRecoverAttributeSet", "TgfMagicAttributeSet", "TgfSoulEnergyAttributeSet", "TgfNaturalEnergyAttributeSet",
    "TgfBuffArgAttributeSet", "TgfStatEffectAttributeSet", "TgfTriggerEffectAttributeSet", "TgfAIAttributeSet",
    "LyraHealthSet", "LyraCombatSet", "LyraMovementSet" }
local SetCache = {} -- addr -> { at, sets = { {name, obj} } }

local function SetsOf(Actor)
    local a = Addr(Actor)
    local c = a and SetCache[a]
    if c and os.clock() - c.at < 30 then
        local ok = true
        for _, s in ipairs(c.sets) do if not IsValid(s[2]) then ok = false; break end end
        if ok and #c.sets > 0 then return c.sets end
    end
    local sets, seen = {}, {}
    local comp, asc = nil, nil
    pcall(function() comp = Actor.AttributeComponent end)
    pcall(function() asc = comp.AbilitySystemComponent end)
    if not IsValid(asc) then pcall(function() asc = Actor:GetAbilitySystemComponent() end) end
    if IsValid(asc) then
        pcall(function()
            asc.SpawnedAttributes:ForEach(function(_, E)
                local s = E:get()
                if IsValid(s) then
                    local n = FName(s)
                    if not seen[n] then seen[n] = true; sets[#sets + 1] = { n, s } end
                end
            end)
        end)
    end
    if #sets == 0 and IsValid(comp) then
        for _, cn in ipairs(SET_CLASSES) do
            pcall(function()
                local s = comp:GetAttributeSet(StaticFindObject("/Script/LyraGame." .. cn))
                if IsValid(s) then
                    local n = FName(s)
                    if not seen[n] then seen[n] = true; sets[#sets + 1] = { n, s } end
                end
            end)
        end
    end
    if a then SetCache[a] = { at = os.clock(), sets = sets } end
    return sets
end

-- Values: { [set name] = { field = {base, cur} } }; Only — filter of set classes (substring), NonZero — non-zero only
local function ReadAttrs(Actor, Only, NonZero)
    local res = {}
    for _, s in ipairs(SetsOf(Actor)) do
        local name, set = s[1], s[2]
        local keep = true
        if Only then
            keep = false
            local cn = ClassName(set)
            for _, o in ipairs(Only) do if cn:find(o, 1, true) then keep = true; break end end
        end
        if keep then
            local vals = {}
            for _, f in ipairs(FieldsOf(set)) do
                pcall(function()
                    local d = set[f]
                    local b, c = d.BaseValue, d.CurrentValue
                    if not NonZero or b ~= 0 or c ~= 0 then vals[f] = { b, c } end
                end)
            end
            -- key: class/object name (a hero may have two sets of one class, e.g. Custom*)
            if next(vals) then res[ClassName(set) .. "/" .. name] = vals end
        end
    end
    return res
end

-- Health/shield in one call (for the poll and before/after a hit)
local HpSets = {} -- addr -> { def = TgfDefendAttributeSet, lh = LyraHealthSet, at }
local function HealthOf(Actor)
    local a = Addr(Actor)
    local c = a and HpSets[a]
    if not c or not (c.def == nil or IsValid(c.def)) or os.clock() - c.at > 30 then
        c = { at = os.clock() }
        for _, s in ipairs(SetsOf(Actor)) do
            local cn = ClassName(s[2])
            if cn:find("TgfDefendAttributeSet", 1, true) and not c.def then c.def = s[2]
            elseif cn:find("LyraHealthSet", 1, true) and not c.lh then c.lh = s[2] end
        end
        if a then HpSets[a] = c end
    end
    local hp, sh, lhp = nil, nil, nil
    if c.def then
        pcall(function() hp = c.def.CurHealth.CurrentValue end)
        pcall(function() sh = c.def.CurShield.CurrentValue end)
    end
    if c.lh then pcall(function() lhp = c.lh.Health.CurrentValue end) end
    return hp, sh, lhp
end

local IdCache = {} -- addr -> {open, guid, name, cls, hero}
local function Who(Actor)
    if not IsValid(Actor) then return { name = "nil" } end
    local a = Addr(Actor)
    local c = a and IdCache[a]
    if c and c.open and c.open ~= "" and c.open ~= "LOCAL_GAME_MODE_OPENID" then return c end
    local w = { name = FName(Actor), cls = ClassName(Actor) }
    w.hero = w.cls:find("Hero", 1, true) ~= nil
    local ps = nil
    pcall(function() ps = Actor.PlayerState end)
    if IsValid(ps) then
        pcall(function() w.open = ps.MyOpenId:ToString() end)
        pcall(function() w.guid = tostring(ps.MyRoleGuid) end)
    else
        -- projectile/ability: the owner
        local own = nil
        pcall(function() own = Actor:GetOwner() end)
        if IsValid(own) and own ~= Actor then
            local ok, cn = pcall(ClassName, own)
            w.owner = FName(own)
            if ok then w.owner_cls = cn end
        end
    end
    if a then IdCache[a] = w end
    return w
end

-- Attributes of the source and the target for a hit record
local ATK_SETS = { "FirstLevel", "Attack", "StatEffect", "BuffArg", "TriggerEffect", "Speed" }
local DEF_SETS = { "Defend", "LyraHealth", "FirstLevel" }

-- ---------------------------------------------------------------------------------------------------------------
-- Hits
local Calls = {}          -- hook counters
local Ctx = {}            -- source addr -> { at, crit, hitv, watk, defv, weight, vuln }
local Pending = {}        -- stack of unfinished ApplyGE (pre without post)
local Explained = {}      -- target addr -> ms of the last hit explained by a hook
local HitN = 0

local function Count(fn) Calls[fn] = (Calls[fn] or 0) + 1 end

local function CtxOf(Src)
    local a = Addr(Src)
    if not a then return nil end
    local c = Ctx[a]
    if not c or Ms() - c.at > 1500 then c = { at = Ms() }; Ctx[a] = c end
    c.at = Ms()
    return c
end

-- The pawn of an ASC: for a hero the ASC lives on the PlayerState (owner — the PlayerState, not the pawn; 05.11 because of this the crit
-- was not bound to the hit), for a monster — on the character itself
local function ActorOfAsc(Asc)
    local o = nil
    pcall(function() o = Asc:GetOwner() end)
    if not IsValid(o) then pcall(function() o = Asc:GetOwnerActor() end) end
    if IsValid(o) and ClassName(o):find("PlayerState", 1, true) then
        local p = nil
        pcall(function() p = o:GetPawn() end)
        if not IsValid(p) then pcall(function() p = o.PawnPrivate end) end
        if IsValid(p) then o = p end
    end
    return o
end

local function GeNames(Arr)
    local out = {}
    pcall(function()
        Arr:ForEach(function(_, E)
            local g = E:get()
            out[#out + 1] = IsValid(g) and FName(g) or "?"
        end)
    end)
    return out
end

local function BeginHit(kind, Src, Trg, extra)
    local h = { kind = kind, src = Who(Src), trg = Who(Trg), extra = extra, ms0 = Ms() }
    h.hp0, h.sh0, h.lhp0 = HealthOf(Trg)
    if Cfg.monsters or h.src.hero then h.src_attrs = ReadAttrs(Src, ATK_SETS, true) end
    if Cfg.monsters or h.trg.hero then h.trg_attrs = ReadAttrs(Trg, DEF_SETS, true) end
    local c = CtxOf(Src)
    if c then h.ctx = { crit = c.crit, hitv = c.hitv, watk = c.watk, defv = c.defv, weight = c.weight, vuln = c.vuln } end
    h.Trg = Trg
    return h
end

local function EndHit(h)
    if not h then return end
    local hp1, sh1, lhp1 = HealthOf(h.Trg)
    h.hp1, h.sh1, h.lhp1 = hp1, sh1, lhp1
    local d = 0
    if h.hp0 and hp1 then d = d + (h.hp0 - hp1) end
    if h.sh0 and sh1 then d = d + (h.sh0 - sh1) end
    h.dmg = d
    if h.lhp0 and lhp1 then h.ldmg = h.lhp0 - lhp1 end
    local a = Addr(h.Trg)
    if a then Explained[a] = Ms() end
    h.Trg = nil
    HitN = HitN + 1
    h.t = "hit"
    h.n = HitN
    Emit(h)
end

-- post-hook: the last argument is the return value (as in DungeonborneRuClient NET_TRACE)
local function RetOf(...)
    local n = select("#", ...)
    if n == 0 then return nil end
    return R(select(n, ...))
end

local HOOKS = {}
HOOKS.ApplyGE = function()
    -- ApplyGE(Instigator, Target, ASCInstigator, ASCHitActor, GECH, GEs, DamageRate, WeaponAttack)
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:ApplyGE",
        function(_, Inst, Trg, _, _, _, GEs, Rate, WAtk)
            Count("ApplyGE")
            local ok, h = pcall(BeginHit, "ApplyGE", R(Inst), R(Trg), {})
            if ok and h then pcall(function() h.extra = { rate = R(Rate), watk = R(WAtk), ges = GeNames(R(GEs)) } end) end
            Pending[#Pending + 1] = ok and h or { failed = true }
        end,
        function()
            local h = table.remove(Pending)
            if h and not h.failed then pcall(EndHit, h) end
        end)
end
HOOKS.ApplyAOEWithGA = function()
    -- ApplyAOEWithGA(Source, Targets&, GameplayEffectClass, MaxNum, TeamRestrict, AppliedActors&, SourceGA, extraParams)
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:ApplyAOEWithGA",
        function(_, Src, Targets, GE)
            Count("ApplyAOEWithGA")
            local list = {}
            pcall(function()
                local s = R(Src)
                local ge = R(GE)
                R(Targets):ForEach(function(_, E)
                    local t = E:get()
                    if IsValid(t) then list[#list + 1] = BeginHit("ApplyAOEWithGA", s, t, { ge = IsValid(ge) and FName(ge) or "?" }) end
                end)
            end)
            Pending[#Pending + 1] = { aoe = list }
        end,
        function()
            local p = table.remove(Pending)
            if p and p.aoe then for _, h in ipairs(p.aoe) do pcall(EndHit, h) end end
        end)
end
HOOKS.ApplyGEWithSpec = function()
    -- BeforeApplyGEWithSpec(Instigator, Target, Specs&) ... AfterApplyGEWithSpec(Instigator, Target, Specs&)
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:BeforeApplyGEWithSpec", function(_, Inst, Trg)
        Count("BeforeApplyGEWithSpec")
        pcall(function()
            local h = BeginHit("GEWithSpec", R(Inst), R(Trg), {})
            h.spec = true
            Pending[#Pending + 1] = h
        end)
    end)
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:AfterApplyGEWithSpec", function(_, Inst, Trg)
        Count("AfterApplyGEWithSpec")
        pcall(function()
            for i = #Pending, 1, -1 do
                if Pending[i].spec then EndHit(table.remove(Pending, i)); return end
            end
        end)
    end)
end
HOOKS.IsCritical = function()
    -- IsCritical(SrcABS, TrgABS, CriticalRandom) -> bool
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:IsCritical", function() end,
        function(_, SrcAsc, TrgAsc, Rnd, ...)
            Count("IsCritical")
            local ret = RetOf(...)
            pcall(function()
                local src = ActorOfAsc(R(SrcAsc))
                local trg = Who(ActorOfAsc(R(TrgAsc))).name
                local c = CtxOf(src)
                if c then c.crit = { rnd = R(Rnd), ret = ret, trg = trg } end
                Emit({ t = "crit", src = Who(src).name, trg = trg, rnd = R(Rnd), ret = ret })
            end)
        end)
end
HOOKS.CalcHitValue = function()
    -- CalcHitValue(SrcActor, TrgActor, AttackType, Guarded, OverrideAtk, OverrideDef) -> float
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:CalcHitValue", function() end,
        function(_, Src, Trg, AType, Guarded, OAtk, ODef, ...)
            Count("CalcHitValue")
            local ret = RetOf(...)
            pcall(function()
                local c = CtxOf(R(Src))
                local hv = { atype = R(AType), guarded = R(Guarded), oatk = R(OAtk), odef = R(ODef), ret = ret,
                    trg = Who(R(Trg)).name }
                if c then c.hitv = hv end
                Emit({ t = "hitv", src = Who(R(Src)).name, trg = hv.trg, atype = hv.atype, guarded = hv.guarded, ret = ret })
            end)
        end)
end
HOOKS.GetWeaponAttack = function()
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:GetWeaponAttack", function() end,
        function(_, Src, AType, ...)
            Count("GetWeaponAttack")
            local ret = RetOf(...)
            pcall(function()
                local c = CtxOf(R(Src))
                if c then c.watk = { atype = R(AType), ret = ret } end
            end)
        end)
end
HOOKS.GetDefendValue = function()
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:GetDefendValue", function() end,
        function(_, Src, Guarded, ...)
            Count("GetDefendValue")
            local ret = RetOf(...)
            pcall(function()
                local c = CtxOf(R(Src))
                if c then c.defv = { guarded = R(Guarded), ret = ret } end
            end)
        end)
end
HOOKS.GetAttackWeight = function()
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:GetAttackWeight", function() end,
        function(_, Src, AType, ...)
            Count("GetAttackWeight")
            local ret = RetOf(...)
            pcall(function()
                local c = CtxOf(R(Src))
                if c then c.weight = { atype = R(AType), ret = ret } end
            end)
        end)
end
HOOKS.GetVulnerFactorRawValue = function()
    RegisterHook("/Script/LyraGame.TgfAbilitySystemBlueprintLibrary:GetVulnerFactorRawValue", function() end,
        function(_, Src, _, ...)
            Count("GetVulnerFactorRawValue")
            local ret = RetOf(...)
            pcall(function()
                local c = CtxOf(R(Src))
                if c then c.vuln = ret end
            end)
        end)
end

local function HookOn(name)
    if not Cfg.hooks then return true end
    for _, h in ipairs(Cfg.hooks) do if h == name then return true end end
    return false
end

local function InstallHooks()
    local res = {}
    for name, fn in pairs(HOOKS) do
        if HookOn(name) then
            local ok, err = pcall(fn)
            res[name] = ok and "ok" or tostring(err)
        else
            res[name] = "off"
        end
    end
    Emit({ t = "hooks", res = res })
    Log("hooks: " .. Enc(res))
end

-- ---------------------------------------------------------------------------------------------------------------
-- FirstLevelAttrDataAsset: which secondary attributes each primary characteristic gives
local FlcfgDone = false
local function AttrName(GA)
    local n = nil
    pcall(function() n = GA.AttributeName:ToString() end)
    local owner = nil
    pcall(function() owner = FName(GA.AttributeOwner) end)
    return n, owner
end

local function DumpFirstLevelConfig(Actor)
    if FlcfgDone then return end
    local cfg = nil
    pcall(function() cfg = Actor.AttributeComponent.FirstLevelConfig end)
    if not IsValid(cfg) then return end
    FlcfgDone = true
    local items = {}
    pcall(function()
        cfg.FirstLevelAttrs:ForEach(function(_, E)
            local it = E:get()
            local bn, bo = AttrName(it.BaseAttr)
            local aff = {}
            pcall(function()
                it.AffectItems:ForEach(function(_, A)
                    local a = A:get()
                    local an, ao = AttrName(a.AffectAttr)
                    aff[#aff + 1] = { attr = an, set = ao, mul = a.Multiplier }
                end)
            end)
            items[#items + 1] = { base = bn, set = bo, affects = aff }
        end)
    end)
    Emit({ t = "flcfg", asset = FName(cfg), items = items })
end

-- ---------------------------------------------------------------------------------------------------------------
-- Hero snapshots and health poll
local LastSnap = {} -- addr -> { sig, at }
local VOLATILE = { DeathZoneTime = true, Health = true, Damage = true, Healing = true, MagicDeltaValue = true,
    SoulEnergyDeltaValue = true, NaturalEnergyDeltaValue = true, DamageValue = true, HealingValue = true,
    CriticalDamage = true, CriticalHealing = true, CalculatedShield = true, AddShield = true, BurningStack = true,
    BaseDamage = true, BaseHeal = true }
local LastHp = {}   -- addr -> { hp, sh }

local function HeldWeapons(H)
    local w = {}
    local lib = BpLib()
    if not lib then return w end
    for _, right in ipairs({ true, false }) do
        pcall(function()
            local d = lib:GetCurrentWeaponDefinition(H, right, 0)
            if IsValid(d) then w[right and "right" or "left"] = FName(d) end
        end)
    end
    return w
end

local function Heroes()
    local out = {}
    for _, H in ipairs(FindAllOf("BP_Fighter_HeroSample_C") or {}) do
        if IsValid(H) and not IsCDO(H) then out[#out + 1] = H end
    end
    return out
end

local function SnapHeroes()
    for _, H in ipairs(Heroes()) do
        local a = Addr(H)
        local attrs = ReadAttrs(H, nil, false)
        if next(attrs) then
            DumpFirstLevelConfig(H)
            -- signature without the "current" fields (health, mana, shield, damage meta attributes): a snapshot — when stats change
            local stable = {}
            for sn, vals in pairs(attrs) do
                local t2 = {}
                for f, v in pairs(vals) do if not VOLATILE[f] and not f:find("^Cur") then t2[f] = v end end
                stable[sn] = t2
            end
            local sig = Enc(stable)
            local ls = a and LastSnap[a]
            if not ls or ls.sig ~= sig or os.clock() - ls.at >= Cfg.snap_force_sec then
                if a then LastSnap[a] = { sig = sig, at = os.clock() } end
                Emit({ t = "snap", who = Who(H), held = HeldWeapons(H), sets = attrs })
            end
        end
    end
end

-- FindAllOf goes over all objects — expensive; the character list is refreshed every CHARS_SEC, in between — IsValid checks
local CHARS_SEC = 2
local CharList, CharAt = {}, -100
local function Characters()
    if os.clock() - CharAt >= CHARS_SEC then
        CharAt = os.clock()
        CharList = {}
        for _, C in ipairs(FindAllOf("LyraCharacter") or {}) do
            if IsValid(C) and not IsCDO(C) then CharList[#CharList + 1] = C end
        end
        return CharList
    end
    local out = {}
    for _, C in ipairs(CharList) do if IsValid(C) then out[#out + 1] = C end end
    return out
end

local function PollHealth()
    local now = Ms()
    for _, C in ipairs(Characters()) do
        local a = Addr(C)
        if a then
            local hp, sh = HealthOf(C)
            local l = LastHp[a]
            if l and hp and l.hp then
                local d = (l.hp - hp) + ((l.sh or 0) - (sh or 0))
                if d > 0.001 then
                    local ex = Explained[a]
                    if not ex or now - ex > 600 then
                        Emit({ t = "hp", who = Who(C), hp0 = l.hp, hp1 = hp, sh0 = l.sh, sh1 = sh, dmg = d })
                    end
                end
            end
            LastHp[a] = { hp = hp, sh = sh }
        end
    end
end

-- ---------------------------------------------------------------------------------------------------------------
-- Start
local function ArgValue(CommandLine, Name)
    return CommandLine:match("%-" .. Name .. "=\"([^\"]*)\"") or CommandLine:match("%-" .. Name .. "=(%S+)")
end

local function ReadConfig()
    local f = io.open(Root .. "statwatch-config.txt", "r")
    if not f then return end
    for line in f:lines() do
        local k, v = line:match("^%s*([%w_]+)%s*=%s*(.-)%s*$")
        if k == "enabled" then Cfg.enabled = v ~= "0"
        elseif k == "monsters" then Cfg.monsters = v ~= "0"
        elseif k == "hooks" then
            Cfg.hooks = {}
            for h in v:gmatch("[%w_]+") do Cfg.hooks[#Cfg.hooks + 1] = h end
        elseif k and Cfg[k] ~= nil and tonumber(v) then Cfg[k] = tonumber(v) end
    end
    f:close()
end

local function Init()
    local ksl = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    if not IsValid(ksl) then return false end
    local cl = ksl:GetCommandLine():ToString()
    local padded = " " .. cl:lower() .. " "
    if not (padded:find("%s%-dbserver%s") or padded:find("%s%-dbstatwatch%s")) then
        Log("no -dbserver / -dbstatwatch — mod disabled")
        Off = true
        return true
    end
    local custom = ArgValue(cl, "dbroot")
    if custom then Root = custom:gsub("\\", "/"):gsub("/?$", "/") end
    ReadConfig()
    if not Cfg.enabled then Log("statwatch-config.txt: enabled=0"); Off = true; return true end
    local name = Root .. "statwatch-" .. os.date("%Y%m%d-%H%M%S") .. ".jsonl"
    Out = io.open(name, "a")
    if not Out then Log("cannot open " .. name); Off = true; return true end
    local safe = cl:gsub("token=%S+", "token=***"):gsub("starttoken=%S+", "starttoken=***")
    Emit({ t = "meta", version = VERSION, cmdline = safe, cfg = { snap_sec = Cfg.snap_sec, monsters = Cfg.monsters,
        snap_force_sec = Cfg.snap_force_sec, hooks = Cfg.hooks or "all" } })
    Log("writing " .. name)
    InstallHooks()
    return true
end

-- 07.11 (docs/CRASH_LONG_PLAY.md): loops in the game thread (was LoopAsync + ExecuteInGameThread). Stop —
-- CancelDelayedAction: LoopInGameThreadWithDelay does not stop on return true.
local SnapAt, StatsAt = -1e9, -1e9
local InitLoop, PollLoop
-- registration — from the game thread (ExecuteInGameThread): from the mod loading thread it may hang the process
ExecuteInGameThread(function()
InitLoop = LoopInGameThreadWithDelay(1000, function()
    if Off or Started then CancelDelayedAction(InitLoop); return end
    local ok, done = pcall(Init)
    if not ok then Log("Init: " .. tostring(done)); Off = true; return end
    if done then Started = true end
end)

-- health poll (200 ms), hero snapshots (snap_sec), hook counters (stats_sec)
PollLoop = LoopInGameThreadWithDelay(200, function()
    if Off then CancelDelayedAction(PollLoop); return end
    if not Started then return end
    do
        pcall(PollHealth)
        local now = os.clock()
        if now - SnapAt >= Cfg.snap_sec then
            SnapAt = now
            local ok, err = pcall(SnapHeroes)
            if not ok then Emit({ t = "error", where = "snap", err = tostring(err) }) end
        end
        if now - StatsAt >= Cfg.stats_sec then
            StatsAt = now
            local c = {}
            for k, v in pairs(Calls) do c[k] = v end
            Emit({ t = "calls", calls = c, hits = HitN, pending = #Pending })
            -- old cache entries
            for a, c2 in pairs(Ctx) do if Ms() - c2.at > 5000 then Ctx[a] = nil end end
            if #Pending > 50 then Pending = {} end
        end
    end
end)
end)

Log(VERSION .. " loaded")
