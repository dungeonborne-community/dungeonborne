# What we took from the Chinese patches (checked 08.11)

Two community patches:
- **"Dungeonborne联机补丁"** — network play (LAN): `zz_*` paks + the `联机启动器.exe` launcher. Copies: `G:\Dungeonborne联机补丁`,
  `G:\3`, `G:\4`, `E:\qwe`, a build translated into Russian — `G:\6`.
- **"Dungeonborne单机PVE补丁"** — single-player with a local server: `zz_SoloLogin_9_P`, `zz_SoloLoot_9_P`,
  `zz_SoloMatch_9_P`, `LoginService.exe`, saves `存档\gateway.db`. Copies: `G:\2`, `G:\5`, `G:\multiplayer`.

Compared by sha256 of the files in our installs (C:\Games\Dungeonborne, the Steam install, Robot) against the patch files.

## Installed for players and on the server (catalog `mods/catalog.toml`)

| Our pak | Origin | Contents | Changes |
|---|---|---|---|
| `zz_LIPatch_P` | PVE: this is `zz_SoloLogin_9_P` under another name (sha256 matches) | `DebugConfig.json`, LevelInfinite login Lua (`AuthLogin*.luac`) — login without a real account | unchanged |
| `zz_Loot_P` | LAN patch (matches) | 4 loot tables | unchanged; overridden by the balance pak — **has no effect** |
| `zz_Market_P` | LAN patch, early version (59 631 B; the latest patch has 90 890 B) | `market_product_item` | unchanged; the merchant table, our balance builds it too |
| `zz_Quest_P` | LAN patch (matches) | `activity_center`, `quest_list` | unchanged |
| `zzzzz_RebuildPrivate_P` | LAN patch, trimmed by us (04.11) | `DebugConfig.json` (same as theirs), `affix_pool.bytes` (ours, differs; overridden by balance) | their UnLua (`Private*.lua`, `LocalDev`) and `loot.bytes` removed |
| `zz_MatchLocal_P` | name from the LAN patch | now — the game's **original** `WBP_MatchMainButton.lua` | their button wrappers removed (04.11); the pak only overrides the old copy players have |

Ours, not Chinese: `zzzzzz_Balance_P`, `zz_RelicQuality_P`, `zzzzzzz_BPFix_P`, `zzzzzzz_ChatFix_P`, `zzzzzzz_ServerNet_P`,
`zz_Jester_P` (dev), all UE4SS mods (`DungeonborneRuClient/Server`, `R14`, `ClientProbe`, `dbnative`). Our `dwmapi.dll` and
`UE4SS.dll` do not match the patch files.

Not installed (in the patches, not in ours): `zz_Items*_P`, `zz_Legend*_P`, `zz_LegendFX*_P`, `zz_IceStaff*_P`, `zz_Frag_P`,
`zz_GearGate_P`, `zz_Heirloom_P`, `zz_Trade_P`, `zz_SafeBoxUI_P`, `zz_LoginAuth_P`, `zz_NoHosts_P`, `zz_CompReq_P`,
`zz_SoloLoot_9_P`, `zz_SoloMatch_9_P` (PVE paks on Robot moved to `Content\_pve_disabled_paks`).

## Data and knowledge (not player files)

- **Loot template** `content/loot/loot.toml` (30.10) — a port of the `zz_SoloLoot_9_P` tables: 159 groups, 136 pools. On 08.11 22 dead
  groups were removed: their 27 sources `BP_REA_*` (970001–970020, 990001–990007) were added by the PVE authors to their `loot_source_map`, they do not exist in the game.
- **Affix format** `DItemData.affix_data` — from the PVE `gateway.db` saves (HANDOFF_CLAUDE_CODE.md).
- **Server behaviour** — recordings of the exchange with the PVE `LoginService` (`rpc_dump/PVE*.jsonl`): many handlers are made "as PVE".
  Their server code is not used — the backend is our own (Rust).
- **The launcher** `联机启动器.exe` — only analysed (ilspycmd; the analysis and antivirus check are in the owner's
  document docs/owner/SECURITY.md), not used; Defender flags it as a trojan (heuristic).
