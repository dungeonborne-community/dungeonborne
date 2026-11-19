# Blueprint patches (mod pak `zzzzzzz_BPFix_P`)

We fix BP logic right in the game assets rather than working around it in Lua (UE4SS is fragile at the pawn creation boundary).
Pak: `pak\zzzzzzz_BPFix_P.{utoc,ucas,pak}` → `Dungeonborne\Content\Paks\` of all installs (loads last).

## Building the patch paks from your copy (19.11)

`cargo ws dump paks --apply` builds all four from the game reference (`docs/GAME_DATA.md`) into `build\pak` and
`build\pak_server`: `tools\build_paks.py` runs the pipeline below for each, the pak key comes from the vault session
(`cargo ws secrets unlock`), decrypted assets stay in `home\cache\tmp\bp-paks` (private). Without `--apply` — the plan.

| Pak | What | Script |
|---|---|---|
| `zzzzzzz_BPFix_P` | `BP_Tgf_Hero_Default`: the client's own equipment manager (R14) | `patch_hero.py` |
| `zzzzzzz_ChatFix_P` | `ChatChannelConfigDataTable`: the world chat in the panel | `patch_chat.py` |
| `zzzzzzz_ServerNet_P` | 5 ability/projectile BPs: `IsDedicatedServer` → `IsServer` (battle server only) | `patch_isserver.py` |
| `zz_LIPatch_P` | 4 LevelInfinite login scripts (Lua 5.3 bytecode): login without the official service — the community patch's edit, 8–14 bytes per file | `patch_lipatch.py` |

Checked 19.11 on game build 16716489: BPFix and ChatFix come out byte for byte as the shipped paks, ServerNet — the same
chunks (packing order differs), LIPatch — the same 4 scripts (the shipped pak also had a `DebugConfig.json`, which
`zzzzz_RebuildPrivate_P` overrides anyway). Each script checks the bytes it edits and stops on another game build.

**31.10: the pak contains only `BP_AvatarActor`.** The pak with Relic was not installed; for "listen as dedicated" (the host is
an observer without a hero) the Relic patch gives the same as the original, so it is not included in the pak. Its built assets are in
`<data>\research\bpfix_relic`, the avatar — in `research\bpfix_avatar`, the pak — `research\bpfix_pak_avatar`.

**03.11: server pak `pak_server\zzzzzzz_ServerNet_P` — only into the server install (Steam / Hetzner).**
Import `KismetSystemLibrary.IsDedicatedServer` → `IsServer` (same signature, the bytecode does not change):
`GA_TGF_DeathKnight_E`, `B_Hook`, `B_TGF_Normal_ProjectileBase`, `B_TGF_Monster_Track_Bullet_Base`,
`B_TGF_Track_Projectile_WizardFireEye`. The headless listen server takes the dedicated branches: without the "predicted" hook
of DK E (double projectile), with mob bullet homing, without sounds/effects. Do not install for clients: in standalone (lobby,
localbattle) `IsServer = true`. `tools\patch_isserver.py <json> <patched.json>`; check — the CFG before/after matches
except the function name, the `.uexp` sizes are the same. Sources/work: `research\isserver_{src,work,bpfix,cfg_*}`.

## Patches

**02.11: a separate pak `pak\zzzzzzz_ChatFix_P` — the general (world) chat.** `ChatChannelConfigDataTable`: the row
`Chat_Channel_World` had `bShowInPanel = false` (in the row's unversioned header bit 5 of the zero mask), now `true`:
header `80 0f 20` → `00 0f` and byte `01` after `SendInterval` — the export size is the same (533). UAssetGUI reads the table
as `RawExport`, the edit is on raw bytes (`research\chatfix_work`, built asset — `research\chatfix`). Check:
parsing of all 6 rows matches to the byte. Installed in both installs.

| Asset | Was | Now |
|---|---|---|
| `BP_Tgf_Hero_Default` (UCS) — **R14**, not in the pak (see BACKLOG) | Return | `if not HasAuthority(): AddComponentByClass(TgfEquipmentManagerComponent)` |
| `BP_AvatarActor` (ubergraph, ReceiveBeginPlay) — R14, did not help | `UpdateMesh()` | `if IsServer(Self): UpdateMesh()` |
| `GA_MeleeWeapon_SwordRelic_Enchant` (ubergraph, DealCastCharge) — not in the pak | `if HasAuthority and not IsStandalone(): return` | `if HasAuthority and not GetLyraCharacterFromActorInfo().IsLocallyControlled(): return` |

Meaning: "the server skips computing the wave position if the hero is not local" — on dedicated the same as before, while
a listen host computes the position for its own hero itself (like standalone). Other heroes on the host send the position by RPC, as before.

R14 (`BP_AvatarActor`): on the client the hero body (`BP_Avatar_Default_C`, a descendant of `BP_AvatarActor`) is assembled from
the replicated pawn before the equipment manager appears on it; the native `TgfAvatarActor::UpdateMesh` goes over
5 armor slots via `GetEquipBySlot` on null → `0x142f602c6`. Server, listen host and lobby (standalone) —
`IsServer = true`, everything as before. The client skips only the first `UpdateMesh` from BeginPlay.
Patch: `tools\patch_avatar.py <json> <patched.json>` (the `UpdateMesh` call, 10 bytes in memory → `Jump` + 5×`Nothing`;
tail `JumpIfNot(IsServer(Self)) → UpdateMesh() → Jump` to `PopExecutionFlow`; import `KismetSystemLibrary.IsServer`).

## How it is done (in-memory offsets do not change)

`LetBool(IsStandalone_RV, IsStandalone(Self))` (21 bytes in memory) → `Jump <end>` + 16×`Nothing` of the same length;
before `EX_EndOfScript` we append `IsStandalone_RV = Context(GetLyraCharacterFromActorInfo(), IsLocallyControlled())` and
a `Jump` back to the original `JumpIfNot`. Imports `Pawn` and `Pawn.IsLocallyControlled` are added to the import table.

Pipeline (the key — from `<data>\aes_key.txt`, do not print):
1. `retoc -a $key to-legacy --version UE5_1 --no-shaders --no-script-objects --filter <Name> <Paks> <legacy>`
2. `tools\ug.py tojson <asset> <json>` — UAssetGUI CLI with mappings (`kismet-analyzer from-json` crashes on enum properties)
3. `tools\patch_relic.py <json> <patched.json> <ubergraph export>` — edits the function's raw bytes; checks itself
   with the disassembler `tools\kismet.py` (the in-memory size must match)
4. `tools\ug.py fromjson <patched.json> <bpfix\Dungeonborne\Content\...\Asset.uasset>`
5. Check: `tools\dis.py` and `kismet-analyzer gen-cfg` on the built asset
6. `retoc to-zen --version UE5_1 <bpfix> pak\zzzzzzz_BPFix_P.utoc`. The reverse `to-legacy` of a single mod container
   does not work in retoc ("1 failed") — control via the asset CFG before packing and a game run.
