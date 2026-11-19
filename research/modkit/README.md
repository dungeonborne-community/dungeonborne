# Mod kit (stubs of the game's C++ in UE 5.1): note 05.11 (not started)

## Why
Blueprints and data that inherit from or reference the game's C++ classes cannot be created in an empty project: the editor does not know these classes.
This covers `UTgfGameplayAbility`, `UTgfInventoryItemDefinition`, attributes, damage calculations, ability tasks.
The mod kit is a UE 5.1 project with "dummy" modules of the same names: classes, properties, functions and enums match the game, with no function bodies.
The editor compiles our BPs against these declarations. On cook the assets reference `/Script/LyraGame.TgfGameplayAbility` etc.
In the game these references resolve to the real classes. Only our own assets go into the pak, never the stub modules.

It enables: new abilities (GA/GE/GCNL), items with mechanics (`ID_`/`AID_`), mithril suits and "classes",
our own BP actors (portals, traps, bosses with behaviour), editing existing BPs in the editor instead of bytes.

## What we have
- `backups\ue4ss\UHTHeaderDump` — a UE4SS dump in UHT format. 239 modules, each with `*.Build.cs`, `Public/*.h` with `UCLASS`/`UPROPERTY`/`UFUNCTION`.
  About 70 modules are not in stock UE 5.1. The main ones (number of headers):

  | Module | Headers |
  |---|---|
  | `LyraGame` | 1031 |
  | `TGFGame` | 537 |
  | `TGFBattleProtocol` | 1946 |
  | `TGFFramework` | 45 |
  | `AkAudio` (Wwise) | 184 |
  | `AGRPRO` | 42 |
  | `UnLua` | 13 |
  | `CommonGame`, `CommonUser`, `GameSettings`, `ModularGameplayActors`, `UIExtension`, `GameplayMessageRuntime` | — |
  | `KawaiiPhysics`, `INTLCore`/`LevelInfinite` | — |
  | `TgfGraphics`, `TgfMultiWorld`, `PortalSystem` | — |
- UE 5.1.1 (`E:\UE\UE_5.1`), UnrealBuildTool, Windows SDK 10.0.22621 / 26100.
- The cook → retoc → pak pipeline (`<data>\ue_jester`); the "placeholders at game paths" technique was debugged on the Jester.
- Compiler: Visual Studio "18" Community, MSVC 14.44 / 14.51. **UE 5.1 expects VS 2022 (v143, 14.3x)**: most likely
  the free Build Tools for Visual Studio 2022 with MSVC v143 and Windows SDK are needed. The user installs them.

## Plan
1. Build Tools VS 2022 (user) → check that UBT builds an empty UE 5.1 C++ project.
2. A separate project `<data>\modkit\Dungeonborne`: copy from the dump only the modules the engine lacks.
   Plugin modules as plugins, game modules (`LyraGame`, `TGF*`) as project modules.
3. Get it to compile, this is the main work:
   - remove or stub out what UHT rejects: private types, broken includes, duplicates of engine code;
   - function bodies: empty/default;
   - modules content does not need (`TGFBattleProtocol` etc.): replace with minimal ones or drop them if nothing references them.
4. First test: a BP child of an existing game ability (e.g. a copy of the rogue's Q with different damage) → cook → pak.
   Check in the game on the server and the client.
5. Then: new GA/GE, `ID_`/`AID_` items, mithril suits.

## Risks
- Volume: thousands of headers. The first builds will fail on hundreds of small errors; that is normal dump fixing.
- A version or class layout mismatch silently breaks only properties with different names or types. Check every new asset type.
- The game's modified engine (Tencent): some asset formats may differ. On the Jester the model and materials went through.
- We change nothing in the exe; the "do not patch the exe" rule is not broken.
