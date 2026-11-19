---
name: dungeonborne-item-flag
description: "Teammate flag on items: OverlayTeammate shows in raid when RoleGuid differs, FactionId equals MyTeamID, viewer not in Ownership; backend sends faction_id 1 to all"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Reversed 07.11 statically, details in `docs/ITEM_FLAG.md`. The flag is `OverlayTeammate` in `WBP_Common_ItemEntry`, driven by `IsTeammateInventory`. Lobby wrappers always return false; only the in-game wrapper can show it.

The likely cause of the extra flag is that `battle_handlers.rs` sends `faction_id: 1` to every player, while the mod assigns `MyTeamID` 1, 2, … by order. `Ownership` can only remove the flag. Not verified on prod. No fix applied; the user decides.

**How to apply:** when changing `faction_id` or team assignment, keep `faction_id` equal to the team's `MyTeamID`. Related: [[dungeonborne-native-gameover]].
