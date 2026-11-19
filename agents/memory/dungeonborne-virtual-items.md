---
name: dungeonborne-virtual-items
description: "Client reads as coins only ids 1000-1030 and 2000-29999; other currencies (materials 1037-1039, 1052, 1053) must be sent as DItem with position.x < 0"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Found 06.11 when the blacksmith showed 0 materials although the backend had pushed `dirty_coin_data`. Native `GetCostItemCount` (`FUN_1459b7310`, range check `FUN_142682480`): only ids 1000–1030 and 2000–29999 are read from the coin map. Other currencies in `item_type` (1031, 1037, 1038, 1039, 1052, 1053, 1500001) are "virtual items": a plain `DItem` with `position.x < 0`, stored by `type_id`, with the count replaced on each update (`FUN_1459aee2b`).

The backend keeps them in `state["coins"]`; only the transport differs: `common::split_virtual_coins` in `push_dirty`, and page 0 of the stash in `player_get_info`. The user confirmed in game on 06.11 that materials now show.

**Why:** the earlier FIX_LIST note I3 ("dismantle works") only checked the backend side, so the client never showed these materials.

**How to apply:** a new currency outside the coin ranges goes through these helpers. Direct `hub::push_to` pushes with `dirty_coin_data` bypass `push_dirty` and do not convert.
