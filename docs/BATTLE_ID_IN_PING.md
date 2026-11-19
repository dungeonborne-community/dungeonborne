# Game ID in the native ping widget

Analysis from 10.11: the widget blueprint (retoc → kismet-analyzer), UE4SS dumps, Ghidra. I did not edit the mods and did not check in game.
Working files: `<data>\research\ping_widget`:
- `ping.json` — the widget;
- `ubergraph.txt` — its bytecode;
- `g1.c` — native code.

## Conclusion
**In the original the match ID is already shown in the ping widget.** No custom label is needed: it is enough to fill
`LyraGameState.BattleId`. Right now this string is empty for us, so the block is empty.

## Widget
- Widget: `/Game/TGF/UI/Game/Ping/WBP_TgfGame_Ping`, parent `TickerUserWidget`, ticks once per 1 s.
- Where it sits: in the match HUD via UI Extension. `LAS_TgfGame_StandardHUD` puts it into the `HUD.Slot.Ping` point of the
  `WBP_TgfGame_HUDLayout` layout, and `Extension_Ping` is a variable of that layout. The lobby has no such layout.
- Tree:
  ```
  CanvasPanel_0
    Image_121 (background T_Ping_Bg)
    HorizontalBox_0 (scale 0.8)
      HorizontalBox → Ping           CommonTextBlock, font Default 14, shadow (0,1) black
      Spacer
      HorizontalBoxBattleId → ID     CommonTextBlock, style TextStyle_TGF_CommonWhite3_18, colour (1,0,1,1) — magenta
      Spacer_46
      HorizontalBoxTest → AlivingTime, ServerFPS   (shown only if PC.ShowPing)
      Spacer_1
      UTCTimeBox → UTCTime
  ```
- Logic (bytecode):
  - **Construct:**
    - `ID.SetText(LyraGameState.BattleId)`;
    - subscription to `LyraGameState.OnBattleIdRep` → `ID.SetText(new id)`;
    - `ListenForGameplayMessages(TGF.HUD.BattleId)`: `HudBooleanMessage.bValue` true → `HorizontalBoxBattleId` Visible,
      false → Collapsed. The message is sent only by the cheat `UTgfLevelCheats.ShowBattleId`.
  - `HorizontalBoxBattleId` visibility is not set in the blueprint, i.e. Visible by default.
  - The ping is shown by `SetFormalPing` (`GetPredictPing`, `GetPingLevelByConfig`, format `PingTips`).
- `LyraGameState.BattleId`:
  - `FString`, offset `0x330`, `ReplicatedUsing=OnRep_BattleId`;
  - `OnRep_BattleId` (`0x142fb1640`) copies the string and calls `OnBattleIdRep(BattleId)`;
  - the property is registered in `GetLifetimeReplicatedProps` (`0x142f92ea0`). Whether it uses the push model there, I could not
    tell from the code.

## Plan (step by step, no custom widget)
1. **Server (RuServer), main option.** When Host.Battle is known (in the same place as `GameIdTick` now):
   ```lua
   local KSL = StaticFindObject("/Script/Engine.Default__KismetSystemLibrary")
   KSL:SetStringPropertyByName(GS, FName("BattleId"), "300138003")
   GS:ForceNetUpdate()
   ```
   - The engine itself writes the string with the game's allocator. This is not an FString assignment from Lua — reading and writing
     FStrings from Lua has already corrupted the heap for us (ClientProbe, 31.10).
   - Lua string → FString UFunction parameter already works (`ClientMessage`).
   - Clients will get `OnRep_BattleId`, and the widget will set the text itself. New widgets after a map change will take the value
     in Construct.
   - No client mod is needed, everyone will see the ID.
   - **Check on the stand:** if the property is push-based, a write via reflection will not reach clients. Then — step 2.
2. **Client (RuClient), fallback option.** On `RUGID <n>` (the hook already exists):
   - `GS` = the GameState of the owner's (PC) world, `IsValid`;
   - `KSL:SetStringPropertyByName(GS, FName("BattleId"), n)`, then `GS:OnRep_BattleId()` — the widget updates via
     the delegate;
   - remove our `W_TgfArrowCursor_C` and `AddToViewport`.

   The repeat every 30 s already covers a map change: a new world has a new GameState.
3. **Style, optional.** The native ID text colour is magenta, looks like a debug one. To match the ping:
   ```lua
   W = FindFirstOf("WBP_TgfGame_Ping_C")
   W.ID:SetColorAndOpacity({SpecifiedColor={R=1,G=1,B=1,A=0.8}, ColorUseRule=0})
   ```
   Repeat on every `RUGID`: the widget is recreated together with the HUD. Text: only the number (as in the original) or
   `"ID 300138003"`. I did not check Cyrillic in the `CommonWhite3_18` style font.

## Risks
- **UE4SS crash:** low. No work with `WidgetTree` and slots, no FString reads. Only UFunction calls
  (`SetStringPropertyByName`, `OnRep_BattleId`, `SetColorAndOpacity`). Before the calls — `IsValid` on GS, PC and W.
- **Anticheat:** the same as for the current mods (Lua UFunction calls on the client). Option 1 does not touch the client at all.
- **Lobby:** no widget. The lobby GameState is not a match one, no need to write to it. `RUGID` comes only from the battle server.
- **Not checked in game:**
  - whether the write from the server reaches clients (push model);
  - that the text is visible without `ShowBattleId`;
  - what the line looks like.
