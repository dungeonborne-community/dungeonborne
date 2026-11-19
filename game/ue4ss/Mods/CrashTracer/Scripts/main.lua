-- Mods/CrashTracer/Scripts/main.lua (v15 - in a match: AffixDisplayConfigAttached, then OnRep_DItemBuffer only if the tooltip is empty)
local LogFile = io.open("crashtracer_pawn_init.log", "w")

local function Log(msg)
    if not LogFile then return end
    local line = string.format("%s | %s\n", os.date("%H:%M:%S"), tostring(msg))
    if #line > 480 then line = line:sub(1, 480) .. "...[TRUNC]\n" end
    LogFile:write(line)
    LogFile:flush()
end

local function Safe(fn, fallback)
    local ok, val = pcall(fn)
    if ok and val ~= nil then return val end
    return fallback
end

local function ObjName(obj)
    if not obj then return "nil" end
    if not Safe(function() return obj:IsValid() end, false) then return "invalid" end
    local full = Safe(function() return obj:GetFullName() end, "err")
    return full:match("([^%.%s]+)$") or full
end

local function FixHeroPawn(p, tag)
    if not p or not Safe(function() return p:IsValid() end, false) then return end
    local full = Safe(function() return p:GetFullName() end, "")
    if not full:find("BP_Fighter_HeroSample_C", 1, true) then return end

    local role = Safe(function() return p.Role end, -1)

    -- 1. Gravity fix (DisableActorMap)
    Safe(function()
        p.DisableActorMap = true
        p.ForceAlwaysRelevent = true
        p.DisableDistanceSlowReplicateRate = true
        p.bFreeze = false
        local amc = p.ActorMapComponent
        if amc and amc:IsValid() then
            amc:SetComponentTickEnabled(false)
            amc:Deactivate()
        end
        p.DefaultGravityScale = 1.28
        p.ReplicatedGravityScale = 1.28
        p:SetGravityScale(1.28, true, true)
        local move = p.CharacterMovement
        if move and move:IsValid() then
            move.GravityScale = 1.28
            if move.MovementMode == 0 then move:SetMovementMode(1, 0) end
        end
    end)

    -- 2. Combat system fix on the pawn
    Safe(function()
        p.UseHitValidation = false
        p.ValidDistance = 99999.0
        local cm = p.AGR_CombatManager
        if cm and cm:IsValid() then cm:SetComponentTickEnabled(true) end
    end)

    -- 3. DO NOT TOUCH the CharacterMesh0 skeleton (keep SKM_Mannequin_Invis for the correct camera and weapon sockets!)
    -- Instead, link the weapon animation layer (LinkCurrentCachedAnimLayer) and enable the CharacterMesh0 ticks!
    Safe(function()
        local mainMesh = p.Mesh
        if mainMesh and mainMesh:IsValid() then
            mainMesh:SetComponentTickEnabled(true)
            mainMesh.VisibilityBasedAnimTickOption = 0
        end

        -- For ANOTHER player's pawn on the client (Role == 1) make ShellMesh (SKM_Man_0001) visible, so players see each other from outside!
        -- And for the OWN pawn (Role == 2, first person) hide ShellMesh for the owner (SetOwnerNoSee(true)), so it does not get into the camera!
        local shellMesh = p.ShellMesh
        if shellMesh and shellMesh:IsValid() then
            shellMesh:SetComponentTickEnabled(true)
            shellMesh.VisibilityBasedAnimTickOption = 0
            if role == 1 then
                shellMesh:SetHiddenInGame(false, true)
                shellMesh:SetVisibility(true, true)
                shellMesh:SetOwnerNoSee(false)
            elseif role == 2 then
                shellMesh:SetOwnerNoSee(true)
            end
        end

        -- Force-link the weapon's cached AnimLayer!
        local tickComp = p:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfCharacterClientTickableComponent"))
        if tickComp and tickComp:IsValid() then
            tickComp:LinkCurrentCachedAnimLayer(true)
        end

        local cosm = p.BP_TgfPawnCosmetics
        if cosm and cosm:IsValid() then
            cosm:LinkDefaultAnimLayers()
        end
    end)

    -- 4. Check and activate UTgfEquipmentManagerComponent + UTgfInventoryManagerComponent + stats
    Safe(function()
        local invMgr = p:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfInventoryManagerComponent"))
        if invMgr and invMgr:IsValid() then
            local sx = Safe(function() return invMgr.Size.X end, 0)
            local sy = Safe(function() return invMgr.Size.Y end, 0)
            -- If the grid is 0x0 on the client or server — initialize 10x7, so the inventory UI works!
            if sx == 0 or sy == 0 then
                Safe(function() invMgr:InitInventoryWithSize({X = 7, Y = 10}) end)
                Safe(function() invMgr:OnRep_Size() end)
                sx = Safe(function() return invMgr.Size.X end, 0)
                sy = Safe(function() return invMgr.Size.Y end, 0)
            end
            local bagCnt = Safe(function() return #invMgr:GetAllItems() end, -1)
            Log(string.format("  [INV_MGR:%s] %s (Role=%s) | Grid=%dx%d | BagItems=%s",
                tostring(tag), ObjName(p), tostring(role), sx, sy, tostring(bagCnt)))
        end

        local eqMgr = p:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent"))
        if eqMgr and eqMgr:IsValid() then
            local curSlot = Safe(function() return eqMgr.CurrentWeaponSlot end, "nil")
            local animInstance = ObjName(Safe(function() return eqMgr:GetCurrentWeaponAnimInstance() end))
            local eqList = Safe(function() return eqMgr:GetAllEquips() end, nil)
            local eqCnt = eqList and Safe(function() return #eqList end, 0) or 0

            -- In a match the affix display config is not attached: the attributes are there (H/E), but the tooltip lines are empty (BF/MB/TA=0).
            -- AffixDisplayConfigAttached() only sets a flag; the lines are built by parsing the buffer WHEN the config is attached.
            -- So: first Attached, then OnRep_DItemBuffer — and only if the display is empty.
            -- Log: att=before>after, rep=1 if OnRep was called. Character stats — in [ATTR_SET] below (compare RAtk/INT/HP).
            -- Checked 30.10 (host_from_lobby): equipment tooltips appeared in the match, RAtk/INT/HP did not change.
            -- v13 (no calls) — empty tooltip; v14 (Attached only) — att=1, but BF/MB/TA=0. The order matters.
            local wInfo = {}
            for _, sId in ipairs({1, 2, 3, 4, 11, 12, 13, 14, 15, 16, 17, 18}) do
                local eq = Safe(function() return eqMgr:GetEquipBySlot(sId, -1) end, nil)
                local inst = eq and Safe(function() return eq.InventoryInstance end, nil)
                if inst and Safe(function() return inst:IsValid() end, false) then
                    local bufLen = Safe(function() return #inst.DItemBuffer.Buffer end, -1)
                    local attBefore = Safe(function() return inst:IsAffixDisplayConfigAttached() end, false)
                    if bufLen > 0 and not attBefore then
                        Safe(function() inst:AffixDisplayConfigAttached() end)
                    end
                    local attAfter = Safe(function() return inst:IsAffixDisplayConfigAttached() end, false)
                    local function DisplayCount()
                        return Safe(function() return #inst.Buffes end, -1), Safe(function() return #inst.MainBasicBuff end, -1),
                            Safe(function() return #inst.TopAffixs end, -1)
                    end
                    local bfN, mbN, taN = DisplayCount()
                    local rep = 0
                    if bufLen > 0 and attAfter and bfN <= 0 and mbN <= 0 and taN <= 0 then
                        Safe(function() inst:OnRep_DItemBuffer() end)
                        Safe(function() eqMgr:UpdateItemInstance(inst) end)
                        bfN, mbN, taN = DisplayCount()
                        rep = 1
                    end
                    local tId = Safe(function() return inst:GetTypeId() end, 0)
                    local holdList = Safe(function() return inst:GetHoldAttrItems() end, nil)
                    local eqListAttr = Safe(function() return inst:GetEquipAttrItems() end, nil)
                    local holdN = holdList and Safe(function() return #holdList end, -1) or -1
                    local eqN = eqListAttr and Safe(function() return #eqListAttr end, -1) or -1
                    wInfo[#wInfo + 1] = string.format("S%d:%d(H=%d,E=%d,BF=%d,MB=%d,TA=%d,B=%d,att=%s>%s,rep=%d)",
                        sId, tId, holdN, eqN, bfN, mbN, taN, bufLen, attBefore and 1 or 0, attAfter and 1 or 0, rep)

                    -- On F9 dump the contents of stats and Buffes in detail for S1 (weapon) and S13 (helmet)
                    if tag == "MANUAL_F9" and (sId == 1 or sId == 13) then
                        local eDetails = {}
                        if eqListAttr and eqN > 0 then
                            for k = 1, math.min(eqN, 10) do
                                local item = eqListAttr[k]
                                local aName = Safe(function() return item.Attribute.AttributeName:ToString() end, "?")
                                local val = Safe(function() return item.Value end, -1)
                                local minV = Safe(function() return item.MinValue end, -1)
                                local maxV = Safe(function() return item.MaxValue end, -1)
                                local aType = Safe(function() return item.AffixType end, -1)
                                local added = Safe(function() return item.HasAdded end, false)
                                eDetails[#eDetails + 1] = string.format("%s=%.2f[%.2f..%.2f](t=%s,add=%s)",
                                    aName, val, minV, maxV, tostring(aType), tostring(added))
                            end
                        end
                        Log(string.format("    [DETAIL S%d:%d] EquipAttrs(%d): %s", sId, tId, eqN, table.concat(eDetails, " | ")))
                    end
                end
            end

            Log(string.format("  [EQUIP_MGR:%s] %s (Role=%s) | CurSlot=%s | Equips=%d | %s",
                tostring(tag), ObjName(p), tostring(role), tostring(curSlot), eqCnt, table.concat(wInfo, " ")))

            if role == 3 then
                Safe(function() eqMgr:CheckMainHandWeapon() end)
                -- On F9 toggle the slot back and forth, so that the weapon's HoldAttrItems are applied to UTgfAttackAttributeSet!
                if tag == "MANUAL_F9" then
                    local cSlot = Safe(function() return eqMgr.CurrentWeaponSlot end, 1)
                    local altSlot = (cSlot == 1) and 2 or 1
                    Safe(function() eqMgr:SwitchWeaponSlot(altSlot, true) end)
                    Safe(function() eqMgr:SwitchWeaponSlot(cSlot, true) end)
                end
            end
            Safe(function() eqMgr:SetEquipmentVisible(true, true, true, 0) end)

            -- Check and fix the base stats and Mana/Energy on the pawn via UTgfAttributeComponent
            local attrComp = Safe(function() return p.AttributeComponent end, nil)
                or Safe(function() return p:GetComponentByClass(StaticFindObject("/Script/LyraGame.TgfAttributeComponent")) end, nil)
            if attrComp and Safe(function() return attrComp:IsValid() end, false) then
                local asc = Safe(function() return attrComp.AbilitySystemComponent end, nil)
                if not asc or not Safe(function() return asc:IsValid() end, false) then
                    local ps = Safe(function() return p:GetLyraPlayerState() end, nil) or Safe(function() return p.PlayerState end, nil)
                    asc = ps and Safe(function() return ps:GetLyraAbilitySystemComponent() end, nil)
                    if asc and Safe(function() return asc:IsValid() end, false) then
                        Safe(function() attrComp:InitializeWithAbilitySystem(asc) end)
                        if role == 3 then
                            Safe(function() attrComp:InitDefault(true, true) end)
                            Safe(function() attrComp:InitFirstLevel() end)
                            Safe(function() attrComp:InitTalents() end)
                        end
                    end
                end

                local firstCls = StaticFindObject("/Script/LyraGame.TgfFirstLevelAttributeSet")
                local magicCls = StaticFindObject("/Script/LyraGame.TgfMagicAttributeSet")
                local defCls   = StaticFindObject("/Script/LyraGame.TgfDefendAttributeSet")
                local atkCls   = StaticFindObject("/Script/LyraGame.TgfAttackAttributeSet")
                local firstSet = firstCls and Safe(function() return attrComp:GetAttributeSet(firstCls) end, nil)
                local magicSet = magicCls and Safe(function() return attrComp:GetAttributeSet(magicCls) end, nil)
                local defSet   = defCls   and Safe(function() return attrComp:GetAttributeSet(defCls) end, nil)
                local atkSet   = atkCls   and Safe(function() return attrComp:GetAttributeSet(atkCls) end, nil)

                local strVal = firstSet and Safe(function() return firstSet.Strength.CurrentValue end, -1) or -1
                local intVal = firstSet and Safe(function() return firstSet.Intelligence.CurrentValue end, -1) or -1
                local hpVal  = defSet   and Safe(function() return defSet.CurHealth.CurrentValue end, -1) or -1
                local mpVal  = magicSet and Safe(function() return magicSet.CurMagic.CurrentValue end, -1) or -1
                local maxMp  = magicSet and Safe(function() return magicSet.MaxMagicBase.CurrentValue end, -1) or -1
                local rAtk   = atkSet   and Safe(function() return atkSet.RightWeaponAttack.CurrentValue end, -1) or -1
                local rGear  = atkSet   and Safe(function() return atkSet.RightAttGearLevel.CurrentValue end, -1) or -1

                Log(string.format("  [ATTR_SET:%s] %s (Role=%s, ASC=%s) | STR=%.0f INT=%.0f | HP=%.0f | MP=%.0f/%.0f | RAtk=%.1f RGear=%.0f",
                    tostring(tag), ObjName(p), tostring(role), ObjName(asc), strVal, intVal, hpVal, mpVal, maxMp, rAtk, rGear))
            else
                Log(string.format("  [ATTR_SET:%s] %s (Role=%s) -> NO VALID AttributeComponent!", tostring(tag), ObjName(p), tostring(role)))
            end
        end
    end)

    if role == 3 then
        Safe(function() p.Inited = true end)
        Safe(function() p:SetComponentTick(true, 0) end)
        Safe(function() p:SetDSMeshTick(true, 0) end)
    end
end

local function InspectLobbyOrStandaloneItems(tag)
    local items = FindAllOf("TgfInventoryItemInstance") or {}
    if #items == 0 then
        Log(string.format("  [ITEMS:%s] No TgfInventoryItemInstance in memory", tostring(tag)))
        return
    end
    local summary = {}
    for idx, inst in ipairs(items) do
        if idx <= 20 and Safe(function() return inst:IsValid() end, false) then
            local tId = Safe(function() return inst:GetTypeId() end, 0)
            local bufLen = Safe(function() return #inst.DItemBuffer.Buffer end, -1)
            local holdN = Safe(function() return #inst:GetHoldAttrItems() end, -1)
            local eqN = Safe(function() return #inst:GetEquipAttrItems() end, -1)
            local mbN = Safe(function() return #inst.MainBasicBuff end, -1)
            local taN = Safe(function() return #inst.TopAffixs end, -1)
            summary[#summary + 1] = string.format("#%d:%d(H=%d,E=%d,MB=%d,TA=%d,B=%d)",
                idx, tId, holdN, eqN, mbN, taN, bufLen)
        end
    end
    Log(string.format("  [ITEMS:%s] Total=%d | %s", tostring(tag), #items, table.concat(summary, " ")))
end

local function InspectAndFixAll(tag)
    Log("=== INSPECT & FIX [" .. tostring(tag) .. "] ===")
    local heroes = FindAllOf("BP_Fighter_HeroSample_C") or {}
    if #heroes == 0 then
        local avatars = FindAllOf("TgfAvatarActor") or {}
        Log(string.format("  [LOBBY:%s] No HeroPawn (Lobby/Menu mode) | Avatars=%d", tostring(tag), #avatars))
        InspectLobbyOrStandaloneItems(tag)
        return
    end

    for i, p in ipairs(heroes) do
        if Safe(function() return p:IsValid() end, false) then
            FixHeroPawn(p, tag)
        end
    end

    local weapons = FindAllOf("B_WeaponBase_C") or {}
    for _, w in ipairs(weapons) do
        if Safe(function() return w:IsValid() end, false) then
            Safe(function() w.UseHitValidation = false end)
            Safe(function()
                local cm = w.AGR_CombatManager
                if cm and cm:IsValid() then cm:SetComponentTickEnabled(true) end
                local root = w:K2_GetRootComponent()
                local parent = root and root:GetAttachParent()
                if parent and parent:IsValid() then
                    parent:SetComponentTickEnabled(true)
                    parent.VisibilityBasedAnimTickOption = 0
                end
            end)
        end
    end
end

local Hooked = {}
local function HookOnce(path, preFn, postFn)
    if Hooked[path] then return end
    local ok = pcall(function()
        RegisterHook(path, preFn, postFn)
    end)
    if ok then
        Hooked[path] = true
        Log("HOOKED: " .. path)
    end
end

local function InstallHooks()
    HookOnce("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:OnReady_7E0A8FA9496C1EC097B3C591D67AA8E1", function(self)
        InspectAndFixAll("AUTO_ON_READY")
    end)

    HookOnce("/Game/TGF/Equipment/Weapon/B_TGF_MeleeWeaponBase.B_TGF_MeleeWeaponBase_C:StartProcessHit", function(self)
        local w = Safe(function() return self:get() end, nil)
        Safe(function() w.UseHitValidation = false end)
        local root = Safe(function() return w:K2_GetRootComponent() end, nil)
        local loc = Safe(function() return w:K2_GetActorLocation() end, nil)
        local sock = Safe(function() return root and root:GetAttachSocketName():ToString() end, "nil")
        Log(string.format("START_HIT -> %s | Socket=%s | Pos=(%.1f, %.1f, %.1f)",
            ObjName(w), sock, loc and loc.X or 0, loc and loc.Y or 0, loc and loc.Z or 0))
    end)

    HookOnce("/Game/TGF/Equipment/Weapon/B_TGF_MeleeWeaponBase.B_TGF_MeleeWeaponBase_C:AttackHitEventHandle", function(self)
        local w = Safe(function() return self:get() end, nil)
        Safe(function() w.UseHitValidation = false end)
    end)
end

NotifyOnNewObject("/Script/Engine.Pawn", function(NewPawn)
    local name = ObjName(NewPawn)
    if name:find("Hero", 1, true) or name:find("Fighter", 1, true) then
        Log("NEW Hero Pawn: " .. name)
        InstallHooks()
    end
end)

local function DumpGameModeDiff()
    Log("=== GAMEMODE VS CDO DIFF START ===")
    local gms = FindAllOf("GameModeBase") or {}
    if #gms == 0 then
        Log("No GameModeBase found (Client or no World)")
        return
    end

    for idx, gm in ipairs(gms) do
        if Safe(function() return gm:IsValid() end, false) then
            local cls = Safe(function() return gm:GetClass() end, nil)
            local cdo = cls and Safe(function() return cls:GetCDO() end, nil)
            Log(string.format("GM[%d] FullName: %s | Class: %s | CDO: %s",
                idx, ObjName(gm), ObjName(cls), ObjName(cdo)))

            -- Print the main string and object fields
            local optStr = Safe(function() return gm.OptionsString:ToString() end, "nil")
            local defPawn = ObjName(Safe(function() return gm.DefaultPawnClass end, nil))
            local pcCls = ObjName(Safe(function() return gm.PlayerControllerClass end, nil))
            local psCls = ObjName(Safe(function() return gm.PlayerStateClass end, nil))
            local gsCls = ObjName(Safe(function() return gm.GameStateClass end, nil))
            Log(string.format("  OptionsString: %s", tostring(optStr):sub(1, 200)))
            Log(string.format("  Classes -> Pawn:%s | PC:%s | PS:%s | GS:%s", defPawn, pcCls, psCls, gsCls))

            -- List of components on the GameMode
            Safe(function()
                local comps = gm:K2_GetComponentsByClass(StaticFindObject("/Script/Engine.ActorComponent"))
                if comps then
                    for i = 1, #comps do
                        local c = comps[i]
                        Log(string.format("  Comp[%d]: %s (%s)", i, ObjName(c), ObjName(c:GetClass())))
                    end
                end
            end)

            -- Explicitly dump the key fields of B_LyraGameMode_C and ALyraGameMode (cur vs CDO)
            local gmProps = {
                "OptionsString", "MatchType", "bDeathZoneEnabled", "bCanSpawnActor",
                "WarmUpDuration", "IntermissionDuration", "SingleMatchDuration",
                "StartGamePhase", "BotControllerClass", "DefaultPawnClass",
                "PlayerControllerClass", "PlayerStateClass", "GameStateClass",
                "bUseSeamlessTravel", "bStartPlayersAsSpectators", "bPauseable"
            }
            for _, pName in ipairs(gmProps) do
                local vCur = Safe(function() return gm[pName] end, nil)
                local vDef = cdo and Safe(function() return cdo[pName] end, nil) or nil
                local sCur = (type(vCur) == "userdata") and (Safe(function() return vCur:ToString() end, nil) or ObjName(vCur)) or tostring(vCur)
                local sDef = (type(vDef) == "userdata") and (Safe(function() return vDef:ToString() end, nil) or ObjName(vDef)) or tostring(vDef)
                local mark = (sCur ~= sDef) and "DIFF" or "SAME"
                Log(string.format("  [%s] GM.%-26s : cur=%-26s | def=%-26s",
                    mark, pName, tostring(sCur):sub(1, 35), tostring(sDef):sub(1, 35)))
            end

            -- Also check B_TGF_MatchGameState_C vs CDO
            local gs = Safe(function() return gm.GameState end, nil)
            if gs and Safe(function() return gs:IsValid() end, false) then
                local gsCls = Safe(function() return gs:GetClass() end, nil)
                local gsCdo = gsCls and Safe(function() return gsCls:GetCDO() end, nil)
                Log(string.format("GS FullName: %s | Class: %s", ObjName(gs), ObjName(gsCls)))
                local gsProps = {
                    "MapId", "MatchType", "BattleId", "ServerFPS",
                    "TgfInteractorEvent", "BotCreationComponent",
                    "ExperienceManagerComponent", "PlayerSpawningManagerComponent"
                }
                for _, pName in ipairs(gsProps) do
                    local vCur = Safe(function() return gs[pName] end, nil)
                    local vDef = gsCdo and Safe(function() return gsCdo[pName] end, nil) or nil
                    local sCur = (type(vCur) == "userdata") and (Safe(function() return vCur:ToString() end, nil) or ObjName(vCur)) or tostring(vCur)
                    local sDef = (type(vDef) == "userdata") and (Safe(function() return vDef:ToString() end, nil) or ObjName(vDef)) or tostring(vDef)
                    local mark = (sCur ~= sDef) and "DIFF" or "SAME"
                    Log(string.format("  [%s] GS.%-26s : cur=%-26s | def=%-26s",
                        mark, pName, tostring(sCur):sub(1, 35), tostring(sDef):sub(1, 35)))
                end
                local optLen = Safe(function() return #gs.OptionIds end, -1)
                Log(string.format("  [INFO] GS.OptionIds Count: %s", tostring(optLen)))
            end

            -- Inspection of BP_DeathZone_C (ring and portal timings)
            local dzList = FindAllOf("BP_DeathZone_C") or {}
            for dIdx, dz in ipairs(dzList) do
                if Safe(function() return dz:IsValid() end, false) then
                    local state = tostring(Safe(function() return dz.State end, "?"))
                    local discIdx = tostring(Safe(function() return dz.CurrentDiscIndex end, "?"))
                    local shrink = tostring(Safe(function() return dz.bEnableShrink end, "?"))
                    local alive = tostring(Safe(function() return dz.AliveFighterNumber end, "?"))
                    local curInt = tostring(Safe(function() return dz.CurrentInterval end, "?"))
                    local curDur = tostring(Safe(function() return dz.CurrentDuration end, "?"))
                    local endT = tostring(Safe(function() return dz.CurrentRoundEndTime end, "?"))
                    local discsLen = Safe(function() return #dz.Discs end, -1)
                    Log(string.format("  [DEATHZONE %d] %s | State=%s | DiscIdx=%s/%s | Shrink=%s | Alive=%s",
                        dIdx, ObjName(dz), state, discIdx, tostring(discsLen), shrink, alive))
                    Log(string.format("    CurInterval=%s | CurDuration=%s | RoundEndTime=%s", curInt, curDur, endT))
                    if discsLen and discsLen > 0 then
                        for i = 1, math.min(discsLen, 10) do
                            local d = Safe(function() return dz.Discs[i] end, nil)
                            if d then
                                local r = Safe(function() return d.Radius end, 0)
                                local p = Safe(function() return d.Pause end, 0)
                                local iv = Safe(function() return d.Interval end, 0)
                                local du = Safe(function() return d.Duration end, 0)
                                local pCount = Safe(function() return #d.PortalSpawnSettings end, 0)
                                Log(string.format("    Disc[%d]: Radius=%.0f | Pause=%.1fs | Interval=%.1fs | Duration=%.1fs | PortalSettings=%d",
                                    i, r, p, iv, du, pCount))
                            end
                        end
                    end
                end
            end
        end
    end
    Log("=== GAMEMODE VS CDO DIFF END ===")
end

RegisterKeyBind(Key.F8, function()
    DumpGameModeDiff()
end)

RegisterKeyBind(Key.F9, function()
    InstallHooks()
    InspectAndFixAll("MANUAL_F9")
end)

Log("=== CrashTracer v15 Started (F8 = GameMode Diff, F9 = Fix Pawns) ===")
InstallHooks()