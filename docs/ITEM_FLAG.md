# Item flag: how it works and why it shows up where it should not

Analysis from 07.11, static only: client Lua, the widget blueprint, Ghidra on the exe image, backend code and traffic recordings. The game was not run. Working files: `<data>\research\item_flag`.

## Conclusion
- **Cause (code checked, not verified on prod):** the backend sends `DBattleUserParameter.faction_id = 1` to every player, and the client writes it into the item's `FactionId`. The icon appears when another player's item has `FactionId` equal to the viewer's `MyTeamID`. A player with `MyTeamID` 1 sees the icon on everyone else's gear, not only on teammates'.
- **`Ownership` is not the cause:** it does not turn the icon on, it only removes it for those in the list. The new `raid_native.rs` logic will not fix this.
- **What to change:** a player's `faction_id` must match their team's `MyTeamID` (section 3).

## 1. Widget and condition
**Icon:** `OverlayTeammate` → `Image_47` with texture `T_Common_ItemEntry_Teammate` in `/Game/TGF/UI/Common/Item/WBP_Common_ItemEntry`. I exported the texture (18×20) to an image: it is a flag.

**Lua:** `Inventory/BP_Common_ItemEntry.lua`, function `WaitToSetup` (lines 162–166):
```lua
local ViewTarget = UE.UTgfGameWidgetLibrary.GetWidgetViewTarget(self)
if ViewTarget and self.CachedItem:IsTeammateInventory(ViewTarget) and self.bShowTopLeftIcon then
  -- OverlayTeammate: SelfHitTestInvisible
else
  -- OverlayTeammate: Collapsed
end
```

**`IsTeammateInventory`** — virtual method, slot `+0x350`, exec `0x142deacb0`:

| Item wrapper | Implementation |
|---|---|
| `_InGame` (raid) | `0x14306eee0`, see below |
| `_Lobby`, `_Activity`, `_Market`, `_Mithril`, `_Trade` | `0x140be1b50`: always `false` |

**In the lobby this icon never turns on.** If someone sees it in the lobby, it is a different image; a screenshot is needed.

**Condition in a raid** (`0x14306eee0`), all of it must hold:
1. The item instance is alive, the item definition is of the right class.
2. Equipment subtype (`def+0x5c`) is not `None`, not `EquipConsumableItem` (41), not `EquipThrowableItem` (51).
3. The viewer is a `LyraCharacter` with a `LyraPlayerState`.
4. `PS.MyRoleGuid` (`+0x880`) is **not equal** to `Item.RoleGuid` (`+0x418`).
5. `PS.MyTeamID` (`+0x879`, 255 → -1) **equals** `Item.FactionId` (`+0x420`).
6. The viewer's key (`MyUserID` `+0x888`, `MyZoneID` `+0x890`) is **not** in `Item.Ownership` (`+0x428`).

## 2. Where the data comes from
All item fields are filled by `0x142f984a0` (instance creation from a `DItem`). A search of LyraGame code found no other writes to these fields except zeroing in constructors; an indirect write is not ruled out. On the battle server the fields are replicated to the client (`Replicated` in the header).

| Field | Source | Who sets it |
|---|---|---|
| `Item.RoleGuid` | `DItem.basic.position.guid` | backend, for gear on entry |
| `Item.Ownership` | `DItem.create_info.ownership.user_keys` | backend |
| `Item.FactionId` | Map `UBattleDsUtility+0x120`: `role_guid → faction`. Role not in the map → 0 | See below |
| Map | `0x142e9b2f0` from `InitPlayerGameData`: key `deploy_preview.deploy_role.basic.role_guid`, value `user_parameter + 0x84` | backend |
| `PS.MyRoleGuid` | `deploy_role.basic.role_guid` (`0x142fe8140`) | backend |
| `PS.MyTeamID` | Our mod, `teams.txt = party` (default): `team_id` → 1, 2, … in order of appearance; spectator 254 | RuClient mod on the battle server |

The map value is `faction_id`: the same record field (`+0x174`) is compared with the argument of `PVPGameWinTeam(faction_id)`. That `user_parameter+0x84` is exactly this field in protobuf is inferred from meaning; I did not compute the offset from the schema.

### What the backend sends now
- `core/backend/src/rpc/handlers/battle_handlers.rs`, lines 96 and 159: `faction_id: 1` for everyone. `team_id` is per player meanwhile: party id or `user_id`.
- `position.guid` equals `role_guid` for all items in `battle_fetch_battle_user_data` (checked against `rpc_dump/OUR.jsonl` and a trial stand: 0 mismatches out of 2821). So your own gear never gets the icon.
- `ownership`: 0 items have it in both dumps.

### What comes out
| Whose item | `RoleGuid` | `FactionId` | Viewed by team 1 | Viewed by team 2+ |
|---|---|---|---|---|
| Own | Own role | 1 | No | No |
| Another player's, taken off them | Their role | 1 | **Icon**, even for an enemy | No, even for a teammate |
| Loot from a chest or a mob | 0 if the server creates it without a `DItem` | 0 | No | No |

Confirmation for team 1: the local stand log has `[TEAMS] native … Team/Squad=1/-1 (mode party)`. So the first team on the server sees an extra icon, and teammates on team 2+ do not see the one they should.

### Not verified
- **What `MyTeamID` players have on prod.** Per the table, only team 1 players see the icon on all other players' gear. If players from different teams complain, the cause is broader. Check: `[TEAMS]` in the DS log from prod and one screenshot. I did not look at the `teams.txt` file on Robot; in `coop` mode everyone gets the host's team, and everyone will see the extra icon.
- **On which screens the icon is visible** (lobby or raid).
- Whether `RoleGuid` is kept on pickup and loot, and what server-created items get: inferred from the single write site, not checked in game.
- The `WaitToSetup` branch for virtual items exits earlier and does not touch `OverlayTeammate`. I did not determine the default visibility from the asset. This does not affect regular gear.

## 3. What the backend should send
Needed: all members of one team have `faction_id` equal to that team's `MyTeamID`, different teams have different ones.

**Option A, recommended.** One source for the number:
1. The backend assigns a team number 1…253 in the match: first team 1, next one 2.
2. `battle_user_data` sends it in `faction_id`, and `/ds/team_of` returns it together with `team_id`.
3. The RuClient mod (`party`) puts this number into `MyTeamID` instead of its own `TeamNext` counter.

As a side effect `faction_id` will start telling sides apart; right now `PVPGameWinTeam` compares the same 1 for everyone. For custom games the number must match the side (`set_room_battle_team`). This touches the backend and the mod and needs an in-game check.

**Option B, quickly remove the extra icon.** `faction_id: 0` outside PVP rooms: `FactionId` will not match any team from 1 up. The icon disappears completely, including for real teammates. First check who else reads `faction_id`: I checked only `PVPGameWinTeam`.

**Option C, no backend.** A battle server mod writes `Inst.FactionId` (`BlueprintReadWrite`, `Replicated`) per the real team after it is assigned. It would have to catch creation of every item, which is fragile.

### New `raid_native.rs` logic
`raid_gear_record`, `raid_team_returns`, `set_owners` fill `create_info.ownership`. For the icon this means only one thing: those in the list do not see the icon.
- It will not fix the extra icon: with an empty `Ownership`, and without it, the condition is decided by `FactionId`.
- On the next entry `position.guid` equals the new owner's role, so your own gear does not get the icon, whatever `Ownership` holds.
- `set_owners(&team_ids)` on findings matches the game's rule and puts the player in the list. `GetOwners` shows the list in the tooltip (`Inventory/BP_ItemTooltipBattle.lua`).
- The icon on a teammate's gear depends on `FactionId`, so the return logic will work with it only after option A.
