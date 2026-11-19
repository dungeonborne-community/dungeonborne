# Patch: game ID in the native ping widget (10.11, "let's do it" — user)

Analysis: `docs/BATTLE_ID_IN_PING.md`. The two edits are independent: the server works without the client, the client without the server.

## 1. RuServer — `ue4ss/Mods/DungeonborneRuServer/Scripts/main.lua`
Insert right after `function Spect.GameIdTick() … end` (around lines 3781–3804):

```lua
-- 10.11 game ID in the native ping widget (WBP_TgfGame_Ping, block HorizontalBoxBattleId/ID): string LyraGameState.BattleId
-- (FString, ReplicatedUsing=OnRep_BattleId → OnBattleIdRep → ID.SetText). Written with the engine's
-- KismetSystemLibrary.SetStringPropertyByName — we do not assign FString from Lua (on 31.10 that corrupted the heap). Once per map.
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
```

In `Spect.GameIdTick()`, after the line `local Id = Host.Battle:match("_(%d+)$") or Host.Battle`, add:

```lua
    pcall(Spect.BattleIdSet, Id)
```

Check on the test stand: the DS log shows `[GAMEID] BattleId=… ok`. On a client without the new RuClient the number appears in the
ping line (purple, the native color), which means the write replicates. If there is no number, the property is push-based and only item 2 works.

## 2. RuClient — `ue4ss/Mods/DungeonborneRuClient/Scripts/main.lua`
Replace the block from the comment `-- 09.11 game ID in the top right corner` (in Russian in the source) to the end of `function GameId.Show … end`
(lines ~2304–2370) with:

```lua
-- 10.11 game ID in the native ping widget (user's decision; replaces our own label from 09.11). The battle server sends
-- the number (RuServer Spect.GameIdTick, ClientMessage "RUGID <number>", every 30 s). We write LyraGameState.BattleId with the engine's
-- SetStringPropertyByName and call OnRep_BattleId: WBP_TgfGame_Ping itself puts the text into the ID block (and a new widget after
-- a map change reads it on creation). The ID color in the original is purple; we paint it like the ping. There is no widget in the lobby.
local GameId = {}

function GameId.Show(Id)
    local KSL = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
    if not (KSL and KSL:IsValid()) then return end
    local Done = 0
    for _, GS in ipairs(FindAllOf("LyraGameState") or {}) do
        if GS and GS:IsValid() and not GS:GetFullName():find("Default__", 1, true) then
            KSL:SetStringPropertyByName(GS, FName("BattleId"), Id)
            GS:OnRep_BattleId()
            Done = Done + 1
        end
    end
    local Painted = 0
    for _, W in ipairs(FindAllOf("WBP_TgfGame_Ping_C") or {}) do
        if W and W:IsValid() and not W:GetFullName():find("Default__", 1, true) then
            local T = W.ID
            if T and T:IsValid() then
                pcall(function() T:SetColorAndOpacity({ SpecifiedColor = { R = 1.0, G = 1.0, B = 1.0, A = 0.8 }, ColorUseRule = 0 }) end)
                Painted = Painted + 1
            end
        end
    end
    if GameId.Last ~= Id then
        GameId.Last = Id
        Log(string.format("[GAMEID] BattleId=%s: GameState %d, ping widgets %d", Id, Done, Painted))
    end
end
```

And in the `ClientMessage` hook below, replace the call (the "Game ID: " label is in Russian in the source):

```lua
-                    local OkS, ErrS = pcall(GameId.Show, "Game ID: " .. Id)
+                    local OkS, ErrS = pcall(GameId.Show, Id)
```

The `GameIdLocalPC` function is not needed after the replacement (it is removed together with the block).

## What to check in a match
- The number sits in the ping line, to the right of the ping, in white, at the ping's scale; our own label in the corner is gone.
- After moving to the second floor the number appears again (within 30 s).
- The client log shows `[GAMEID] BattleId=…: GameState 1, ping widgets 1`; no `[GAMEID]` lines in the lobby.
- No crashes while loading a map (calls only via ExecuteInGameThread; the hook already works that way).
