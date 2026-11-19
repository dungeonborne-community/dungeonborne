LOG

Both items are feasible. Loading screens for a season are a small job. The Jester instead of the Reaper is a big one, and it hinges on the 3D model.

## 1. Loading screens for a season: yes

The framework already supports this. Everything in `seasons/<id>/pak/` is laid out into the installs by `cargo xtask season deploy <id>`, and the installer distributes it to players. The second picture (the Jester, "DEAD MONEY" on the banners) is a natural fit for the Dead Money loading screen.

How to do it:
- The season pak holds its own textures at the paths of the original backgrounds `T_Loading_BG_*`. Right now the game randomly picks one of 9 backgrounds.
- The picture needs to be horizontal, 16:9. Yours is vertical (1024×1536), it will have to be cropped or extended at the sides. Better give a 1920×1080 version.

Two caveats:
- **Unreal Engine 5.1 is needed.** The textures have to be cooked in an editor of the same version as the game. This machine does not have it, only the Epic launcher in `F:\EpicGames`. Install UE 5.1 from the launcher, it is free, about 40 GB.
- **The old season pak is not removed by itself.** Right now `season deploy` only copies the season's paks. If another season is enabled later, the previous one's pak stays in the install. I will add cleanup.

## 2. The Jester instead of the Reaper at one spot: feasible, but the model is the main thing

The Reaper in the game is `BP_TGF_Monster_Reaper`, its variants `_H_` and `_Special` and the skeleton `SK_Reaper_Skeleton`. Plan:

- **Model.** A 3D model of the Jester bound to the Reaper skeleton is needed. Then all its animations and behaviour (AnimBP) fit, and the combat logic does not need to be redone. The picture is only a reference, a model cannot be made from it. Options:
  - commission a 3D artist;
  - take a ready model (marketplace, Sketchfab) and retarget it to the Reaper skeleton;
  - make a rough one from the picture with an image-to-3D neural network, but it still has to be rigged afterwards.

  Then the model is built in UE 5.1 and put into the season pak.
- **Only on the right map and spot.** The server mod catches the Reaper spawning, checks the map and the spot coordinates and swaps its model. A model swap is not replicated over the network, so the client mod does the same. The other Reapers stay normal.
- **Loot after death.** The regular boss loot drops anyway, it can be tuned in the season loot tables. The server can drop extra loot with the game's native loot function `UBattleUtility::Loot`.
- **Leaving through a blue portal.** We already know how to place a blue portal (the `portal` command places it via the death zone). What remains is to decide how exactly he leaves, question below.

Of the technical parts the riskiest is binding the model to the skeleton correctly. The server and client mod code is something we have done before. In terms of effort the main thing is the model itself.

Your decision on the exit scenario is needed. A dead body will not walk into a portal by itself, so the options are:
- **(a)** At low health the Jester stops taking damage, drops loot, a blue portal opens nearby, he walks into it and disappears. There is no death as such.
- **(b)** He dies as usual, the loot drops, the body dissolves in a portal flash.
- And separately: after he leaves, should the portal remain an exit for players or close?

