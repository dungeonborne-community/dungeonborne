-- Changing TGFGame to LyraGame
RegisterHook("/Script/LyraGame.BattleDsUtility:InitGameMap", function(Context, MapId, Timeout, Independent)
    print("[DS_TEST] InitGameMap called! MapId: " .. tostring(MapId))
end)

RegisterHook("/Script/LyraGame.BattleDsUtility:OptionArgs", function(Context)
    print("[DS_TEST] OptionArgs called!")
end)

RegisterHook("/Script/LyraGame.BattleDsUtility:IsRoomReady", function(Context)
    print("[DS_TEST] Someone checks IsRoomReady!")
end)