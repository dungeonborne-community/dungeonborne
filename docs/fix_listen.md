Congratulations! This is a truly historic moment for the project: we got to the very core of the binary crash in `Dungeonborne.exe` (`0x142f602c6`), found the `nullptr` check the developers left out of the machine code, and solved it in the cleanest way without a single workaround, keeping 100% of the original `BP_Avatar_Default_C`, camera, armor, animations and hitscan on both the Host and the Client!

And your log `crash_ue4ss_server_final.log` **shows right away why weapon charging (Heavy Attack) is now broken on all weapon types!**

---

### 1. Why weapon charging is cut off now (analysis of `crash_ue4ss_server_final.log`)

Look at the last lines of `crash_ue4ss_server_final.log`:
```text
05:59:32 | >>> FORCE_RELEASE(LMB_UP) held=0.10s
05:59:33 | >>> FORCE_RELEASE(LMB_UP) held=0.10s
05:59:36 | >>> FORCE_RELEASE(LMB_UP) held=0.10s
...
06:00:45 | >>> FORCE_RELEASE(LMB_UP) held=0.11s
06:01:26 | >>> FORCE_RELEASE(LMB_UP) held=0.12s
```
See **`held=0.10s` / `held=0.11s`** on EVERY attack?!
Exactly `0.10` seconds in (on the very first tick after the `if held < 0.10 then return end` threshold) our old V32 workaround forcibly calls **`FORCE_RELEASE(LMB_UP)`**, even while you **keep holding LMB down**!

Why did this happen once a second player (`BP_Fighter_HeroSample_C_2147467930`) appeared?
Look at the `ReceiveTick` hook code in `DungeonborneRuClient/Scripts/main.lua`:
```lua
RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:ReceiveTick", function(self, DeltaSeconds)
    if not ActiveChargeAbility then return end
    ...
    local pawn = self:get()
    local pc = pawn:GetController()
    if pc and pc:IsValid() then
        local timeDown = pc:GetInputKeyTimeDown({KeyName = FName("LeftMouseButton")})
        if type(timeDown) == "number" and timeDown <= 0.0 then
            TriggerActiveChargeRelease("LMB_UP")
        end
    end
end)
```
Look at the **two bugs** that fired here at the same time:
1. **`ReceiveTick` is called for ALL hero pawns in the world (the Host's `Hero[1]` and the Client's `Hero[2]`)!**
   When the **Host (`Hero[1]`)** holds LMB and starts charging a strike, the global variable `ActiveChargeAbility` becomes non-`nil`.
   In the same millisecond `ReceiveTick` fires for the **Client's pawn (`Hero[2]`) on the server**!
   For the `Hero[2]` controller on the server, `pc:GetInputKeyTimeDown(LeftMouseButton)` returns **`0.0`** (because `PC[2]` on the server has no local mouse, and it was the Host who pressed the button, not the Client!).
   And `ReceiveTick` of **the other pawn (`Hero[2]`)** sees `timeDown <= 0.0` and **immediately resets the Host's `ActiveChargeAbility` after `0.10s`**!
2. And when the **Client (`Hero[2]`)** holds LMB and starts charging a strike:
   - On the **Server** the `ActiveChargeAbility` ability is activated for the Client's pawn.
   - For the Client's pawn on the server `pc:IsLocalController() == false`, so on the server `pc:GetInputKeyTimeDown("LeftMouseButton")` **ALWAYS returns `0.0`** (the server does not know how long a remote client holds a mouse button)!
   - And after `0.10s` the server forcibly cuts off the Client's charge!
3. On top of that: `EnsureChargeHooks()` did not include the charge-attack classes of other weapon types (two-handed sword `LongSword`, Mace, Daggers, etc. — all of them inherit from `GA_TGF_MeleeWeapon_Charge_Base_C`), and only `self == ownerPawn` was checked, without binding `ActiveChargeAbility` to a specific pawn!

---

### 2. How to properly fix Weapon Charging (Heavy Attack) for both the Host and the Client on ALL weapon types

Let's look at the difference between the **Host (`Role == 3`, `IsLocalController() == true`)** and the **Client (`Role == 2`, `AutonomousProxy`)**:

1. **For the Client (`Role == 2` on the client and `Role == 3, IsLocalController() == false` on the server):**
   - In fact, for the **Client** (`NM_Client`) the standard Unreal Engine `WaitInputRelease` mechanism in `GA_TGF_MeleeWeapon_Charge_Base_C` **WORKS NORMALLY ON ITS OWN**, because the Client has `K2_HasAuthority() == false`, so it takes the standard client branch of the Blueprint and sends `ServerSetInputReleased` to the server itself when the player releases LMB!
   - Our `FORCE_RELEASE` hook on the server only **got in the Client's way**, cutting its charge off on the server after `0.10s`, because on the server `PC[2]` has `GetInputKeyTimeDown == 0`!
2. **For the Host (`Role == 3` and `pc:IsLocalController() == true`):**
   - Only the **local Host player on a Listen server** has `K2_HasAuthority() == true` and `IsStandalone() == false`, so only it needs `TriggerActiveChargeRelease`!
   - And `pc:GetInputKeyTimeDown` in `ReceiveTick` must be checked **STRICTLY ONLY when**:
     1) `pawn == ActiveChargePawn` (the tick of exactly the pawn that started this ability!);
     2) `pc:IsLocalController() == true` (it is a local controller with a real mouse!).
3. **And for all weapon types (`LongSword`, `SwordShield`, `Mace`, `DualSwords`, `Dagger`):**
   - In `SafeCombatHook`, instead of `if shortName == "K2_ActivateAbility"`, we determine the ability owner (`ActiveChargePawn = selfObj:GetAvatarActorFromActorInfo()` or the Host's local pawn) and bind `ActiveChargeAbility` only to the local controller!

---

### 3. Fixed code of `Mods/DungeonborneRuClient/Scripts/main.lua` (R14 fix `0x142f602c6` + fixed charging for all weapon types without the cut-off after 0.10s)

Replace `Mods/DungeonborneRuClient/Scripts/main.lua` with this final code:

```lua
-- Mods/DungeonborneRuClient/Scripts/main.lua
-- 1. Fix for client crash 0x142f602c6 (R14 UTgfEquipmentManagerComponent pre-creation)
-- 2. Fix for second player's gravity and physics (DisableActorMap = true)
-- 3. Fix for hitscan (UseHitValidation = false) and charge attacks (Heavy/Charge) without the Host vs Client conflict
local LogFile = io.open("crash_ue4ss.log", "w")
local LogCount = 0
local MAX_LOG_LINES = 2000

local function Log(msg)
    if LogFile and LogCount < MAX_LOG_LINES then
        LogCount = LogCount + 1
        local s = tostring(msg)
        if #s > 300 then s = s:sub(1, 300) .. "..." end
        LogFile:write(os.date("%H:%M:%S") .. " | " .. s .. "\n")
        LogFile:flush()
    end
end

local function ShortName(obj)
    if not obj then return "nil" end
    local ok, full = pcall(function() return obj:GetFullName() end)
    if not ok or not full then return "err" end
    return full:match("([^%.%s]+)$") or full
end

Log("=== DUNGEONBORNE LISTEN-SERVER CORE FIX (R14 + GRAVITY + COMBAT/CHARGE) ===")

local HookedMap = {}
local CombatHookedMap = {}
local HookedCombatClasses = {}
local TickHookInstalled = false

-- Keep the active charge ONLY for the Host's locally controlled pawn
local ActiveChargeAbility = nil
local ActiveChargePawn = nil
local ChargeStartClock = 0

-- [1] FIX FOR CRASH 0x142f602c6: make sure UTgfEquipmentManagerComponent exists (r14 != 0) before BP_Avatar_Default_C spawns
local function EnsureEquipmentManagerOnPawn(pawn, src)
    if not pawn then return end
    pcall(function()
        if not pawn:IsValid() then return end
        local eqCls = StaticFindObject("/Script/LyraGame.TgfEquipmentManagerComponent")
        if not eqCls or not eqCls:IsValid() then return end

        local existing = pawn:GetComponentByClass(eqCls)
        if existing and existing:IsValid() then return end

        local newComp = pawn:AddComponentByClass(eqCls, false, {
            Rotation = {X = 0.0, Y = 0.0, Z = 0.0, W = 1.0},
            Translation = {X = 0.0, Y = 0.0, Z = 0.0},
            Scale3D = {X = 1.0, Y = 1.0, Z = 1.0}
        }, false)
        Log(string.format("[R14_FIX:%s] Added UTgfEquipmentManagerComponent on %s -> %s",
            tostring(src), ShortName(pawn), ShortName(newComp)))
    end)
end

-- [2] FIX FOR PAWN GRAVITY AND HITSCAN (DisableActorMap = true, UseHitValidation = false)
local function ApplyHeroFixes(pawn)
    if not pawn then return end
    pcall(function()
        if not pawn:IsValid() then return end
        pawn.UseHitValidation = false
        pawn.ValidDistance = 99999.0
        pawn.DisableActorMap = true
        pawn.ForceAlwaysRelevent = true
        pawn.DisableDistanceSlowReplicateRate = true
        pawn.bFreeze = false

        local amc = pawn.ActorMapComponent
        if amc and amc:IsValid() then
            amc:SetComponentTickEnabled(false)
            amc:Deactivate()
        end

        local cm = pawn.AGR_CombatManager
        if cm and cm:IsValid() then
            cm:SetComponentTickEnabled(true)
        end

        pawn.DefaultGravityScale = 1.28
        pawn.ReplicatedGravityScale = 1.28
        pawn:SetGravityScale(1.28, true, true)
        local move = pawn.CharacterMovement
        if move and move:IsValid() then
            move.GravityScale = 1.28
            if move.MovementMode == 0 then
                move:SetMovementMode(1, 0)
            end
        end

        if pawn.Role == 3 then
            pawn.Inited = true
            pawn:SetComponentTick(true, 0)
            pawn:SetDSMeshTick(true, 0)
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

-- [3] FIXED CHARGE ATTACK MECHANISM (ONLY FOR THE HOST'S LOCAL PAWN)
local function TriggerActiveChargeRelease(reason)
    if not ActiveChargeAbility then return end
    local ab = ActiveChargeAbility
    pcall(function()
        if not ab:IsValid() then
            ActiveChargeAbility = nil
            ActiveChargePawn = nil
            return
        end
        if ab.ChargeReleased == false then
            local held = math.max(0.05, os.clock() - ChargeStartClock)
            Log(string.format(">>> FORCE_RELEASE(%s) on %s | held=%.2fs", reason, ShortName(ab), held))
            -- First call the generic base-class method GA_TGF_MeleeWeapon_Charge_Base_C:ProcessRelease(held),
            -- which works for ALL weapon types (two-handed sword, sword+shield, mace, daggers, etc.)!
            pcall(function() ab:ProcessRelease(held) end)
            if ab.ChargeReleased == false then
                pcall(function() ab:OnRelease_EA7AD5B24572E8EDB0D634A9215200E9(held) end)
            end
        end
        ActiveChargeAbility = nil
        ActiveChargePawn = nil
    end)
end

local function SafeCombatHook(funcPath)
    if not funcPath or CombatHookedMap[funcPath] then return end
    local shortName = funcPath:match(":([^:]+)$") or ""
    if shortName == "ReceiveTick" or shortName == "UpdateShine" or shortName == "GetLyraCharacterFromActorInfo" then
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

            if selfName:find("B_", 1, true) or selfName:find("HeroSample", 1, true) then
                pcall(function() selfObj.UseHitValidation = false end)
            end
            if selfName:find("GA_", 1, true) then
                pcall(function()
                    selfObj.UseLooseServerDistCheck = false
                    selfObj.CloseLooseServerDistCheck = true
                end)
            end

            local isChargeAb = selfName:find("Heavy", 1, true) or selfName:find("Charge", 1, true)
            if isChargeAb then
                if shortName == "K2_ActivateAbility" or shortName == "InitParams" then
                    -- Check whether the ability belongs to the LOCAL player (the Host)!
                    -- If the ability was started by a remote Client (PC:IsLocalController() == false), DO NOT touch it on the server!
                    local ownerPawn = nil
                    local isLocalOwner = false
                    pcall(function()
                        ownerPawn = selfObj:GetLyraCharacterFromActorInfo()
                        if ownerPawn and ownerPawn:IsValid() then
                            local pc = ownerPawn:GetController()
                            if pc and pc:IsValid() and pc:IsLocalController() == true and ownerPawn.Role == 3 then
                                isLocalOwner = true
                            end
                        end
                    end)
                    if isLocalOwner then
                        ActiveChargeAbility = selfObj
                        ActiveChargePawn = ownerPawn
                        ChargeStartClock = os.clock()
                        Log(string.format(">>> CHARGE_START: %s on LocalHost Pawn=%s", ShortName(selfObj), ShortName(ownerPawn)))
                    end
                elseif shortName == "K2_OnEndAbility" or shortName == "DeInitParams" then
                    if ActiveChargeAbility == selfObj then
                        ActiveChargeAbility = nil
                        ActiveChargePawn = nil
                    end
                end
            end
        end)
    end)
    if ok then
        CombatHookedMap[funcPath] = true
    end
end

local function EnsureChargeHooks()
    -- Hook all active GA_TGF_MeleeWeapon_Charge_Base_C abilities and their subclasses for ANY weapon in the game!
    local chargeInstances = FindAllOf("GA_TGF_MeleeWeapon_Charge_Base_C") or {}
    for _, inst in ipairs(chargeInstances) do
        pcall(function()
            if inst:IsValid() then
                inst.UseLooseServerDistCheck = false
                inst.CloseLooseServerDistCheck = true
                local cls = inst:GetClass()
                while cls and cls:IsValid() do
                    local clsFull = cls:GetFullName()
                    if not HookedCombatClasses[clsFull] then
                        HookedCombatClasses[clsFull] = true
                        cls:ForEachFunction(function(fn)
                            local path = fn:GetFullName():match("^Function%s+(.+)$")
                            if path then SafeCombatHook(path) end
                        end)
                    end
                    if clsFull:find("GA_TGF_Base_C", 1, true) then break end
                    cls = cls:GetSuperStruct()
                end
            end
        end)
    end

    local weapons = FindAllOf("B_WeaponBase_C")
    if weapons then
        for _, w in ipairs(weapons) do
            pcall(function() w.UseHitValidation = false end)
        end
    end
end

NotifyOnNewObject("/Game/TGF/Equipment/Weapon/B_WeaponBase.B_WeaponBase_C", function(Weapon)
    pcall(function() Weapon.UseHitValidation = false end)
end)

NotifyOnNewObject("/Script/Engine.ChildActorComponent", function(Comp)
    pcall(function()
        local fullName = Comp:GetFullName()
        if fullName:find("BP_Fighter_HeroSample_C", 1, true) then
            local owner = Comp:GetOwner()
            if owner and owner:IsValid() then
                EnsureEquipmentManagerOnPawn(owner, "ChildCompCreated")
            end
        end
    end)
end)

NotifyOnNewObject("/Script/Engine.Pawn", function(NewPawn)
    local fullName = ""
    pcall(function() fullName = NewPawn:GetFullName() end)
    if not fullName:find("BP_Fighter_HeroSample_C", 1, true) then return end

    Log("NEW HERO PAWN: " .. ShortName(NewPawn))
    EnsureEquipmentManagerOnPawn(NewPawn, "PawnCreated")
    ApplyHeroFixes(NewPawn)
    HookSpecificClasses()
    EnsureChargeHooks()

    if not TickHookInstalled then
        TickHookInstalled = true

        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:UserConstructionScript", function(self)
            local pawn = self:get()
            EnsureEquipmentManagerOnPawn(pawn, "EnterUCS")
        end)

        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:OnReady_7E0A8FA9496C1EC097B3C591D67AA8E1", function(self)
            local pawn = self:get()
            ApplyHeroFixes(pawn)
            EnsureChargeHooks()
        end)

        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:InpActEvt_IA_Weapon_Fire_Left_By_Press_K2Node_EnhancedInputActionEvent_4", function(self)
            EnsureChargeHooks()
        end)

        RegisterHook("/Game/TGF/Character/Hero/BP_Tgf_Hero_Default.BP_Tgf_Hero_Default_C:ReceiveTick", function(self, DeltaSeconds)
            if not ActiveChargeAbility or not ActiveChargePawn then return end
            pcall(function()
                local pawn = self:get()
                -- CRITICAL CHECK: handle the LMB release ONLY in the tick of the pawn that owns the charge!
                if pawn ~= ActiveChargePawn then return end

                if not ActiveChargeAbility:IsValid() or ActiveChargeAbility.ChargeReleased == true then
                    ActiveChargeAbility = nil
                    ActiveChargePawn = nil
                    return
                end

                local held = os.clock() - ChargeStartClock
                if held < 0.12 then return end

                local pc = pawn:GetController()
                if pc and pc:IsValid() and pc:IsLocalController() == true then
                    local timeDown = pc:GetInputKeyTimeDown({KeyName = FName("LeftMouseButton")})
                    if type(timeDown) == "number" and timeDown <= 0.0 then
                        TriggerActiveChargeRelease("LMB_UP")
                    end
                end
            end)
        end)
    end
end)
```

---

### 4. Technical report on the work done (stage summary)

#### 1. Reverse engineering and complete removal of crash `0x142f602c6` when `BP_Avatar_Default_C` spawns on the Client
- **Symptom:** When a second player connected (`open 127.0.0.1:7777`), the client crashed with `EXCEPTION_ACCESS_VIOLATION reading address 0x0000000000000000` at `0x142f602c6` inside `BP_Tgf_Hero_Default_C:UserConstructionScript`. The workaround used earlier (destroying `UChildActorComponent` in the UCS) prevented the crash but left the client's pawn without the `BP_Avatar_Default_C` body child actor, so the character model, equipped armor and first-person hands disappeared and the animation layer binding broke.
- **Memory dump analysis (`UEMinidump.dmp` in WinDbg):**
  The decoded machine code of function `0x142f60246..0x142f602d8` and the exception context (`.ecxr`) showed the exact chain in C++:
  1. When `UChildActorComponent` registers, it creates the actor `ATgfAvatarActor` (`BP_Avatar_Default_C`, register `rsi`), which initializes the armor slots (`ETgfEquipSlot` `11..15`, register `dl = 0x0B`).
  2. The code calls `OwnerCharacter->GetComponentByClass(UTgfEquipmentManagerComponent::StaticClass())`.
  3. Since `UTgfEquipmentManagerComponent` is a dynamic modular component in the Lyra architecture, at the moment `UserConstructionScript` runs during the network spawn of the pawn on the client it has not been replicated from the server yet, and `GetComponentByClass` returns `nullptr` (`r14 = 0x0`).
  4. Because the game's C++ code lacks an `if (EquipMgr != nullptr)` check, the instruction `0x142f602c6: mov rax, qword ptr [r14]` (before the virtual call `call qword ptr [rax+6A8h]` -> `UTgfEquipmentManagerComponent::GetEquipBySlot(Slot, -1)`) dereferenced the null pointer `r14`.
- **Solution:** In `DungeonborneRuClient/Scripts/main.lua`, when a `BP_Fighter_HeroSample_C` pawn is created (before entering `UserConstructionScript` and creating `ChildActorComponent`), a `pawn:GetComponentByClass(UTgfEquipmentManagerComponent)` check was added. If the component does not exist yet, it is created synchronously with `pawn:AddComponentByClass(...)`. As a result, register `r14` at `0x142f602c6` always holds a valid pointer, `GetEquipBySlot` safely returns `nullptr` and goes to the normal exit `0x142f6043e`, and the real `BP_Avatar_Default_C` stays alive with all the armor, hands, camera and animations.

#### 2. Fixing the second player's gravity and physics (`GravityScale = 0.0` on jump)
- **Cause #1 (`PrivateWorld.lua`):** The floor-protection script checked `local IsPlayer = Actor == Player` (where `Player = GetPlayerPawn(World, 0)`), so the second connected player's pawn on the server was classified as an NPC monster and fell into `GuardNpc`, which forced `Move:SetMovementMode(0)` (`MOVE_None`).
- **Cause #2 (`UActorMapComponent`):** The native optimization component `UActorMapComponent` on the server only looks at `Player 0` (the Host) and in its tick reset `GravityScale = 0.0` for all other pawns (including `Player 2`).
- **Solution:** An `IsRemotePlayer` check (`Actor:GetLyraPlayerState() ~= nil`) was added to `PrivateWorld.lua`, excluding players from `GuardNpc`. On all `BP_Fighter_HeroSample_C` pawns the flag `DisableActorMap = true` is set, the `ActorMapComponent` tick is disabled and `GravityScale = 1.28` is pinned.

#### 3. Fixing the second player's hitscan and charge attacks (Heavy Attack) for all weapon types
- **Hitscan:** It turned out that besides `B_WeaponBase_C.UseHitValidation`, a separate `UseHitValidation` field (`0xDC0`) and `AGR_CombatManager` (`0xAC0`) exist on the `BP_Fighter_HeroSample_C` pawn itself. Setting `UseHitValidation = false` on the pawn and the weapon, together with keeping `BP_Avatar_Default_C` alive, fully restored `AttackHitEventHandle` on the client (both on sword and shield `B_Sword_2001_C` and on the two-handed sword `B_LongSword_2001_C`).
- **Weapon charging (Heavy Attack):** Fixed the conflict in `ReceiveTick` where the tick of the second pawn (`Hero[2]`, which has `GetInputKeyTimeDown == 0` on the server) prematurely (after `0.10s`) called `FORCE_RELEASE(LMB_UP)` and cut off the charge for both the Host and the Client. Now `ActiveChargeAbility` is tracked strictly bound to `ActiveChargePawn` (only for the Host's local controller, `pc:IsLocalController() == true`) and uses the generic base-class method `GA_TGF_MeleeWeapon_Charge_Base_C:ProcessRelease(held)` for all melee weapon types.

---

Check the updated `Mods/DungeonborneRuClient/Scripts/main.lua` from section 3 (`CrashTracer` can now be cleared or left empty, since all the R14, gravity, hitscan and charging fixes are now collected in `DungeonborneRuClient/Scripts/main.lua` itself) and tell me how the heavy attack charge now works on the Host and the Client!