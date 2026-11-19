---
name: dungeonborne-support-button
description: "14.11: the \"Support\" button — all 4 places call the global CustomerService:Open; interception = UnLua wrapper CustomerService.lua in a pak (like zz_RelicQuality_P)"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Analysis 14.11 (read-only, bytecode `tools\research\player\lua_orig`, luac54 -l -l):
- `Game/Service/CustomerService/CustomerService.lua` — a global service table (`ServiceManager.RegisterService("CustomerService.CustomerService")` in `Game/Service/Init.lua`). `EnableCustomerService = true` hard-coded → the button is always visible. `Open(scene)` → `SDKService.OpenUrl` → `USDKWebviewService:OpenUrl(url,1,false,true,false,"{}",true)` — the INTL webview, not a browser. The yzfchat URL is dead.
- Callers of `CustomerService:Open`: ESC menu `WBP_SettingPopup`, login `WBP_LoginWindow`, ban window `Unlock/WBP_UnlockBanWindow`, native `SettingService.Lua_OpenCustomerService`. `GetUserCareer` is also needed by `WBP_ActivityDailyTask_*` — do not throw out the whole module.
- Discord on the login screen: `JumpUtils.JumpByHref("OpenUrl::https://discord.com/invite/dungeonborne")` (native UJumpManager).
- UnLua has `io`, `os` (the RelicQuality wrapper writes a log via io.open) → signal the launcher with a file; system browser — `UE.UKismetSystemLibrary.LaunchURL`.

User 14.11: button → "a form in the launcher", and asked about a report on fatal error. The launcher can already do it: `src-tauri/src/bugreport.rs crash_likely` (exit code ≠ 0 or no "Log file closed" in Dungeonborne.log), `lib.rs:1410` writes exit_code — but only when the player opened the form himself. Crash traces: `Win64/ue4ss/crash_*.dmp` (~38 MB, over the 9 MB limit), `crash_ue4ss.log`, `Saved/Logs/Dungeonborne.log`. CrashSight (`CrashSight64.dll`, `Win64/CrashSightLog`, `wesight/crashsight_data` — compressed/encrypted) sends reports to Tencent `pc.crashsight.wetest.net` by itself (answers 200). User decision 14.11: do not touch CrashSight for now.

Proposed: a wrapper pak `CustomerService.lua` + `_Orig`, replacing only `Open`. Waits for the user's decision. Related to [[dungeonborne-mods-design]], [[dungeonborne-launcher-auth]].
