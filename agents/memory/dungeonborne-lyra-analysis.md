---
name: dungeonborne-lyra-analysis
description: "Dungeonborne is built on Epic's open-source Lyra — use public Lyra C++ to understand the native init/net layer and pick hook points"
metadata:
  visibility: team
  node_type: memory
  type: reference
---

Dungeonborne is based on Lyra (Epic's open sample for UE 5, sources in the UE repository under 5.1). Throughout the dump
`LyraCharacter`, `LyraPawnExtensionComponent`, `LyraHeroComponent`, `LyraAbilitySystemComponent`, `ULyraPawnData`.

**Why.** The native layer (pawn initialization, ASC readiness, handing out components via `PawnData`→`AbilitySet`→
GameFeature, net role semantics) lives in C++ and is not visible: the exe is packed (sections `.std`, on-disk code is zeros,
decrypted only in memory), `.uasset` has only BP + C++ stubs. Base Lyra can simply be read from Epic to understand
"how it is meant to work", instead of guessing from logs. This is NOT an exe patch and NOT an ACE bypass — only reading sources to choose anchors.

**Limits.** Only base Lyra is public. TGF changes (`Tgf*`: `TgfAttributeComponent`, `TgfEquipmentManagerComponent`,
`TgfGameplayAbility`) are closed. The logic of specific abilities is in BP bytecode (we read it via retoc+kismet-analyzer,
see [[dungeonborne-bp-reverse]]).

**How it helps (two current tasks):**
1. Equipment manager crash for joining players (`0x142f602c6`, race: the avatar UCS runs before
   `TgfEquipmentManagerComponent` replicates). Currently the R14_FIX workaround creates a dummy on `PawnCreated` → side effect: two managers,
   `GetComponentByClass` picks the empty one → this may be the "disappearing damage". Per Lyra the right anchor is
   to react to data arrival: UFUNCTION `OnRep_PawnData` on `LyraPawnExtensionComponent` (present in the dump, hookable),
   rather than slipping in a stub. Native `CheckPawnReadyToInitialize` etc. — only in Lyra sources, there are no UFUNCTION wrappers.
   Correction 30.10: OnRep_PawnData arrives AFTER the UCS, it cannot prevent the crash — only remove the dummy afterwards.
   The native manager is a dynamic replicated subobject (unique name), it arrives together with the dynamic CAC.
2. The net mode flag breaks abilities on listen (relic wave, souls, charge release): BPs do
   `if HasAuthority and not IsStandalone: return` (or `K2_HasAuthority`/`GetNetMode`), designed for dedicated.
   This is TGF design in BP, Lyra sources have no fix — only confirmation of the net role semantics.

**Lyra source on disk (31.10):** `<data>\Lyra` (Source + Plugins, no Content; API ≥5.1 — has
`IsUsingRegisteredSubObjectList`). There are NO engine plugins `ModularGameplay` (GameFrameworkComponentManager) and
`GameFeatures` (AddComponents) — they are in the engine, not in Lyra. Checked against the source:
- `ULyraEquipmentManagerComponent` ctor: `SetIsReplicatedByDefault(true)` → our server-side R14 stub replicates.
- `ALyraGameMode::SpawnDefaultPawnAtTransform`: `bDeferConstruction` → UCS runs before `PreInitializeComponents`, where
  `AModularCharacter` registers in GameFrameworkComponentManager (GameFeature components arrive after the UCS).
- `ALyraGameMode::FailedToRestartPlayer` repeats `RequestPlayerRestartNextFrame` and calls
  `GetDefaultPawnClassForController` while `PlayerCanRestart` is true — this explains the `[GM_PLAYER:PreGetDefaultPawnClass]` spam.
- Init states: Spawned → DataAvailable (needs PawnData and a controller for authority/local) → DataInitialized → GameplayReady.

**Next step (in progress).** Capture `NET_TRACE` (mod `DungeonborneRuClient`, `NET_TRACE=true`) in localbattle (standalone)
and host_from_lobby (listen), compare `IsStandalone`/`K2_HasAuthority` answers per class = a map of the cut branches.
The event order is also needed (UCS / `OnRep_PawnData` / native manager arrival / avatar spawn) to compare with the
Lyra order and decide: move from the dummy to `OnRep_PawnData`, and one `IsStandalone` hook on our pawn vs targeted calls of
branches (the asset patch `IsStandalone→IsLocallyControlled` broke the charge with a 2nd player). Details — BACKLOG.md.
The full work context (mods, server, deadmoney, loot) — in BACKLOG.md and [[dungeonborne-workflow]].
