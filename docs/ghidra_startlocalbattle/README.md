# StartLocalBattle and LOCAL_GAME_MODE_OPENID (Ghidra, 01.11.2026)

Source: full memory dump `UECC-Windows-E56ED7E0…\UEMinidump.dmp` (9 GB, install C:\Games\Dungeonborne).
The Dungeonborne.exe image was carved from it (base 0x140000000, all code already decrypted) and loaded into Ghidra
as Raw Binary without auto-analysis; functions were decompiled with the script `research/ghidra/scripts/DecompTree.java` (scripts live there since 17.11, the workflow is in `research/ghidra/README.md`).

## Addresses (this build)
| What | Address |
|---|---|
| `execStartLocalBattle` (UFunction wrapper) | 0x145910ce0 |
| `UBattleService::StartLocalBattle(FName)` | 0x14599f2d0 |
| response lambda | 0x145982b70 |
| `UBattleService::OnFetchBattleUserDataResponse` | 0x145992a10 |
| `execStartBeginnerLevel` → body | 0x145910c10 → 0x14599efc0 |
| manager: Reset / SetLocalMode(+0x246=1) | 0x142e870a0 / 0x142e8a830 |
| manager: AddBattleUserData(DBattleUserData) | 0x142e9b2f0 |
| manager: IsDsMode | 0x142e9e8b0 |
| `UBattleDsUtility::InitPlayerGameData(mgr, PS)` | 0x142e9cee0 |
| `PlayerState::SetOpenId` (only when Role==Authority) | 0x142fe79a0 (field PS+0xA00) |
| string `LOCAL_GAME_MODE_OPENID` / `LOCAL_GAME_MODE_PLAYER_{}` | 0x14771de58 / 0x14771e3a0 |

## Chain
1. `StartLocalBattle(Map)`: `this+0x3C = Map`, `this+0x38 = 1`, sends an empty `battle_fetch_battle_user_data`
   with a callback (`this` captured). It does not open the map itself.
2. `OnFetchBattleUserDataResponse`: if error != 0 — log `error = %d` and an error on screen. Otherwise:
   - `rsp.battle_user_data.ds_player_key.ds_user_key = "LOCAL_GAME_MODE_OPENID"` (overwrites what the server sent);
   - mgr->Reset(); mgr->SetLocalMode() (byte mgr+0x246 = 1);
   - mgr->AddBattleUserData(data): key = `ds_player_key.ds_user_key`, entry in TMap mgr+0x80;
   - extra options (if this+0x50), 0x14599dc50, then OpenLevel(this+0x3C, absolute, options "").
3. `InitPlayerGameData(mgr, PS)` for each PlayerState (not DS mode):
   - mgr+0x246 != 0 → `PS.MyOpenId = "LOCAL_GAME_MODE_OPENID"`;
   - mgr+0x246 == 0 → `PS.MyOpenId = "LOCAL_GAME_MODE_PLAYER_<n>"` (counter mgr+0x248) + generates data under this key;
   - looks up data in mgr+0x80 by PS.MyOpenId and applies it (0x142ecfd20), PS+0x840 = 1 (HasInited).
   In DS mode (IsDsMode) the OpenId is not overwritten, the key is looked up in the table mgr+0x30.
4. `StartBeginnerLevel` does the same without the network: builds stub data with the key `LOCAL_GAME_MODE_OPENID`
   and opens the map (options `CanReturnHall=true`, if the flag is set).

## Conclusion for the listen server
That is why all players get the same data set: there is one response, its key is always `LOCAL_GAME_MODE_OPENID`, and
every PS gets this same OpenId (the value from `?OpenId=` is overwritten). For per-player data you need either DS mode
(IsDsMode, keys by ds_user_key), or several entries in mgr+0x80 under the real OpenIds and no OpenId overwrite.
