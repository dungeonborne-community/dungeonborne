# Match quests: where the battle server gets progress from (chests, damage, kills)

Analysis of 08.11 for battle pass quests 16518. Static only: Ghidra on the exe image, chest BPs (retoc →
kismet-analyzer), UE4SS type dump, tables. No code changed, the game was not run. Working files:
`<data>\research\quest_hooks` (`t1.c`…`t6.c` — decompilation, `cfg/` — chest BPs).

## Key point
**The game already has a native quest counter — `UTgfActorStatisticComponent`.** The battle server itself counts kills by type,
opened chests, damage to players, etc. In the original it sent these numbers to the backend at the end of the raid
(`DsPlayerGameOverReq.player_statistics` → `DRoleStatisticsData {interger_stat_map, double_stat_map, relation_stat_map}`),
and the backend advanced quests based on them. A quest's `entity_args` is the statistic's `TypeId`, semantically `ETgfActorType`:
1 = player, 11 = monster, 12 = elite, 13 = boss, 41 = chest.

So the most accurate and simplest path for RuServer is **not to catch events but to read the ready counters**:
`Comp:GetIntStat(type, TypeId)` / `Comp:GetFloatStat(type)` and send the increment to the backend. The hooks below are a fallback
and for verification.

| Quest (`EnQuestProgressType`) | entity_args | Counter | Who writes it |
|---|---|---|---|
| `LOOT` (25) | 41 | int, type 5 `Looting`, TypeId 41 | chest BP, once per chest |
| `PLAYER_CAUSED_TOTAL_DAMAGE` (26) | 1 | float, type 7 `CausedPlayerTotalDamage` | native, every hit |
| `KILL_TYPE` (2) | 11 | int, type 1 `KillingType`, TypeId 11 | native, on kill |

Mapping of the other types (by name, not checked against the backend code): `KILL_ACTOR_STAT_ID` ↔ int 2,
`INTERACTING_STAT_ID` ↔ int 3, `MOVING_AREA` ↔ int 4, `DESTROYING_OBJECT` ↔ int 53, `RESURRECT_TEAM` ↔ int 52,
`BATTLE_SYNTHESIZE_COST_ITEM` ↔ int 54, `PLAYER_CAUSED_TOTAL_HEAL` ↔ float 4, `PLAYER_GOT_TOTAL_DAMAGE` ↔ float 5,
`ALIVE_TIME` ↔ float 2. Conditions `condition_ids` (102001 = `battle_which_level_type` 2001, 201xxx — class,
203xxx — race) are checked by the backend; the battle server does not know about them.

## Where the counter lives
- `UTgfActorStatisticComponent` (`/Script/LyraGame.TgfActorStatisticComponent`) sits on the player's **controller**, only on the
  server (added by the GameFeature component set; in the RuServer probe it is marked "s"). Field `PlayerState`
  (`ALyraPlayerState`) → `MyOpenId`.
- UFunctions for Lua: `GetIntStat(Type, TypeId) → int32`, `GetFloatStat(Type) → double`, `GetRelationIntStat(Type, OpenID)`,
  `AddIntStat`/`AddFloatStat`/`Set…`.
- Storage — the component's own `TMap`s (int: `+0xa8` type → (TypeId → value); float: `+0xf8`), native
  `0x143016240` (int) and `0x143015c90` (float).
- **Why `PlayerState.RelationIntStatistics` was empty on 03.11:** the replicated `PlayerState` arrays
  (`IntStatistics`/`FloatStatistics`/`RelationIntStatistics`) only get pairs registered via
  `SetIntStatisticsRepType`/`SetFloatStatisticsRepType`/`SetRelationIntStatisticsRepType` (native `0x1430435e0`
  returns if the pair is not in the set `+0x198`). Read the component, not the `PlayerState` arrays.
- `UTgfStatBlueprintFunctionLibrary::AddActorIntStat(Actor, Type, TypeId, Value)` (exec `0x142dfdaa0`, implementation
  `0x143015660`): `Actor` must be an `ALyraCharacter`; for types 1, 2, 3, 5, 52, 53 summon points go to the owner;
  then pawn → controller → component. No controller or component — silently nothing.

## 1. Opening a chest (LOOT, 41)
**Native count:** `BP_InteractableChestBase` — event `PerformInteraction on Server(InteractingPawn)`:
1. `bLocked` → `OnOpenFailed(InteractingPawn)`, end.
2. Opens the loot menu for this player (for everyone who opened it).
3. **Only if `bOpened == false`** (first opening of this chest):
   `AddActorIntStat(InteractingPawn, 5, 41, 1)`, `AddActorIntStat(InteractingPawn, 5, 0, 1)` (0 = "any loot"),
   `GiveBonusToPlayer` (GAS event `TGF.GameplayEvent.OpenChest`), then `GenerateLoot` (which calls `BattleUtility::Loot`)
   and `bOpened = true`.

Exactly once per chest, credited to whoever opened it first. `BattleUtility::Loot` has no initiator in its arguments
(`Owner` = the chest, `PlayerStates` — the team for loot), so it is useless for the quest.

Which chests count (subclasses of `BP_InteractableChestBase` that do not override the event):
`BP_TreasureChest_Wood_01…10`, `_Iron_01…05`, `_Gold_01`, `_Gold_02`, `_Gold_01_Special`, `_Mechnism_01`, `_Special_02`
(via `BP_InteractableChest`), `BP_GoblinThief_TreasureChest_Wood_02`, `BP_InteractableChest_Guarded`.
**Do not count:** `BP_InteractableChest_FlawlessTrial` (its own event without the counter), apparently also `BP_TreasureChest_Special_01`
(parent `BP_InteractableBase`). **Mimics** are monsters (`BP_Tgf_Monster_Mimic01…09`, `…_Mimic_Book`): they go into kills
(KILL_TYPE 11), not chests. `BP_TreasureChest_Mimic*` from the mimic potion is an item, not a map chest.

**Hook, if needed:** `AddActorIntStat` is a static UFunction, the BP calls it via `EX_CallMath`; a UE4SS hook on the UFunction
catches such calls too (not verified in game). Catch the pair `(5, 41)`:
```lua
RegisterHook("/Script/LyraGame.TgfStatBlueprintFunctionLibrary:AddActorIntStat",
    function(Ctx, Actor, Type, TypeId, Value)
        pcall(function()
            if Type:get() ~= 5 or TypeId:get() ~= 41 then return end
            local Pawn = Actor:get()
            if not IsValid(Pawn) then return end
            local PS = Pawn.PlayerState                      -- the player's pawn; for a summon the native gives it to the owner itself
            if not IsValid(PS) then return end
            local Open = PS.MyOpenId:ToString()
            QuestAdd(Open, "chest", Value:get())             -- own accumulation function
        end)
    end)
```
Without a hook: `Comp:GetIntStat(5, 41)` (see "General polling" below).

## 2. Damage to players (PLAYER_CAUSED_TOTAL_DAMAGE, 1)
**`ClientDamageDirectionEvent(Direction, bIsGuarded, DamageInstigator, DamageTarget)`:** A = `Direction` (float, the angle
the hit came from, for the indicator), B = `bIsGuarded` (hit into a block). **There is no damage amount.** It is a client RPC,
called by native `0x142f05e20` on every hit.

**Native damage count:** the "damage dealt" handler of the attacker's attribute component (`0x142efe380`, then fires the delegate
`OnDamageCaused`):
- attacker: the component's owner; summon → owner (`0x143027010`); a controller with the statistic component is required;
- only **against an enemy**: the attacker's and target's teams differ (`0x14301bf20`; if the team is undefined — it counts);
- skipped if the effect has tag `TGF.GamePlay.Effect.IgnoreFloatStat` or the attacker has tag `TGF.IgnoreActorStat`;
- `AddFloatStat(3, damage)` — all damage; **`AddFloatStat(7, damage)` if the target is an `ALyraCharacter` with `ActorType`
  Player (1) or BotPlayer (2)**. The damage is final, after armor and block (the same number that goes to the delegate).

The amount cannot be hooked directly: the native and the delegate have no UFunction. BP listeners with damage exist only on monsters
(`AIC_Monster_Default:OnDamageTaken(Actor, InstigatorTags, TargetTags, Damage, Spec)`, `BP_Tgf_Monster` …`DamageEvent`) —
useless for damage to players. `UTgfInteractorBattleComponent:MulticastDamageEvent(Instigator, Tags, DamageValue)` —
that is for destructible objects, not players.
**Conclusion: take `Comp:GetFloatStat(7)`** (double, cumulative over the component's lifetime). Quest "900 damage" = increment ≥ 900.

## 3. Kills (KILL_TYPE, 11) and `MulticastDeathEvent`
**`UTgfAttributeComponent:MulticastDeathEvent(DamageInstigator, DamageTarget)`** is called inline in the native
death handler (`0x142f06600`, via ProcessEvent, so the UE4SS hook fires) **on any death: players,
monsters, elites, bosses, summons** — the only condition: the target has an ASC and no tag `TGF.GamePlay.DeathForCheating`.
Self = the attribute component of the dead actor. The instigator may be a pawn, a controller or a projectile — reduce it to a PlayerState
as in RuServer (`PlayerStateOf`).

**Native kill count** (the killer's "kill made" handler, `0x142f0c580`):
- killer: the component's owner, summon → owner; only against an enemy (same team check);
- by the victim's `ActorType` (`ALyraCharacter.ActorType`, `ETgfActorType`, native `+0x730`):
  - Player/BotPlayer (1, 2) → `KillingType` TypeId **1** + relation `Kill` by the victim's OpenId;
  - Monster/Elite/Boss/Summon (11–14) → TypeId **11**; Elite and Boss (12, 13) also +1 to TypeId **12**; Boss (13) also to **13**;
- then `KillingActorStatId` (type 2) +1 for each id in the victim's `ActorStatIdList`.

**So KILL_TYPE 11 is all monsters, including elites, bosses, mimics and other players' summons.** Elites+bosses only — 12,
bosses only — 13.

Hook (if not reading the counter), filter as in the native:
```lua
RegisterHook("/Script/LyraGame.TgfAttributeComponent:MulticastDeathEvent", function(Self, Inst, Target)
    pcall(function()
        local Victim = Target:get()
        if not IsValid(Victim) then return end
        local T = tonumber(Victim.ActorType) or 0          -- ETgfActorType
        if T < 11 or T > 14 then return end                -- 1/2 — players, the rest — not monsters
        local PS = PlayerStateOf(Inst:get(), 0)            -- from RuServer: PlayerState / Instigator / Owner
        if not PS then return end
        QuestAdd(PS.MyOpenId:ToString(), "kill11", 1)
    end)
end)
```
Differences from the native: a player's summon is reduced to the owner only if the summon's `Instigator`/`Owner` leads to the player;
the hook does not check teams (a monster is always an enemy of the player, so for 11 it does not matter).

## General polling of counters (recommended)
```lua
-- every N seconds and before a player leaves / the map changes; register LoopInGameThreadWithDelay from ExecuteInGameThread
local Last = {}   -- [OpenId] = { comp = address, chest = n, kill = n, dmg = x }
local function PollQuestStats()
    for _, C in ipairs(FindAllOf("TgfActorStatisticComponent") or {}) do
        if IsValid(C) and not FullName(C):find("Default__", 1, true) then
            pcall(function()
                local PS = C.PlayerState
                if not IsValid(PS) then return end
                local Open = PS.MyOpenId:ToString()
                local Now = { comp = C:GetAddress(), chest = C:GetIntStat(5, 41), kill = C:GetIntStat(1, 11),
                              dmg = C:GetFloatStat(7) }
                local L = Last[Open]
                if not L or L.comp ~= Now.comp then L = { comp = Now.comp, chest = 0, kill = 0, dmg = 0 } end
                QuestAdd(Open, "chest", Now.chest - L.chest)       -- new component (new controller) = from zero
                QuestAdd(Open, "kill11", Now.kill - L.kill)
                QuestAdd(Open, "dmg_players", Now.dmg - L.dmg)
                Last[Open] = Now
            end)
        end
    end
end
```
If a value decreased — the component was reset, count from 0.

## What to check in game (in one match)
1. Whether the component exists: `FindAllOf("TgfActorStatisticComponent")` on the server, one per player, `PlayerState` filled in.
   The RuServer probe already looks for it (`GF_COMPS`, "s"). If it is missing — the counters are empty, then hooks only.
2. Open a chest → `GetIntStat(5, 41)` = 1; open the same one with a second player → does not grow.
3. Kill a boar → `GetIntStat(1, 11)` +1; hit a player → `GetFloatStat(7)` grows by the damage shown.
4. Whether the component is reset when moving to the second floor (new controller) — the polling above survives that.
5. PvP: if players on listen are in the same team, the native will not count damage and kills of players (team check). Then
   for damage to players — only our own estimate, see "Damage".
