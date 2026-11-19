---
name: dungeonborne-mithril-classes
description: "Mithril Order mode as reversed 06.11 (Scav-like, role_type rows, reputation, settle) and the deferred idea of new \"classes\" on its 3 career slots"
metadata:
  visibility: team
  node_type: memory
  type: project
---

The user plans to build new "classes" on the Mithril Order later: "write this down, we will implement and develop the topic later" (05.11). On 06.11 they asked how the mode works; full analysis is in `<data>\research\mithril_classes\README.md`.

Key facts:
- The career list is fixed in the exe (`ETgfActorCareer`) and in the protocol. Paladin (9) exists only in C++ and has no assets. Mithril careers 101–103 exist in both.
- Correction of my 05.11 claim: Mithril items do NOT grant abilities. `AID_Mithril_*` holds only meshes and the base armor class. The digit Y in `19?0XY000` is quality (white/green/blue/purple), not a separate suit.
- A Mithril class is a `role_type` row (400101–400103): `role_skill` (Q and E) → `ability_set_id` → `UeSource_AbilitySet`, plus the starting kit. Talents are by career + race in `FirstLevelAttr`.
- There are only 3 slots: UI and protocol select a role by career.
- Reputation has 4 levels (exp 0/100/200/300), gained from damage dealt. Only white and green gear is in the tables.
- Our backend only stubs the mode (`shop_handlers.rs`): no role creation, prestige always 0, no settle.

Main scheme, chosen by the user on 06.11: a new `role_type` row with a Mithril `career` (101–103) and `force` = ADVENTURER. The lobby windows mostly read `force`, so the character gets the normal hall, stash and raids but is its own class in battle. The Order mode itself is not needed for this, and `career` is not swapped between lobby and battle. Skills are a `role_skill` pool plus `role_select_skill`. At most 3 fully different classes, because talents are by career + race. This is untested in game; the checks are listed in the README.

Character creation window (reversed 06.11 from code, not run in game): a row with `force` = ADVENTURER and `environment` = ALL_AVAILABLE enters the list by itself, but the window has 8 `CareerItem` widgets and all 8 are taken. A 9th widget must be added at runtime to `CareerWrap` + `CareerItems` (UnLua override of the window module, or UE4SS). Details are in the README section "New slot in the creation window".

Trial class 400201 (06.11): the user asked to keep it apart from the main game: "in a separate directory, we do not mix it with the main game". Everything is in `<data>\new_class` (own tables, paks, backend and DB, README there) plus game copies `F:\DB_NewClass\client|server` (hard links for paks/exe/dll). Never install its paks or tables into the main installs or `backups\ExcelConfig`. Confirmed in game by the user on 06.11: 9th slot, character creation, hall and a raid with extraction all work (`type=400201`, pawn `BP_Fighter_HeroSample_C`, `applied: extraction`). A fresh DB needs one manual step: the server account has no character, see `start_game.ps1 -ServerSetup`. All pre-battle settings are in `new_class\class.json` (text via CSV overrides, stats, kit, talents via index patch of `FirstLevelAttr`). The equipment cross came from `CommonFunction.IsMithrilCareer`, wrapped to false. Built 06.11, not yet verified in game.

Paused 06.11 at the user's request ("write all this down, we will look at another task"). Next, when the stand is free: test `class.json` settings together with mage Q on the class (`skills`). Then the new Q: charge like the mage, no magic cost, `FX_Torch_01` from the Moonlight torch (user confirmed), then Reaper `Attack4` explosion, damage and fear. Plan: `research\new_abilities\README.md`. Newer UE versions do not help: cooked assets do not load in the game's 5.1.

Companion deferred ideas, all with BACKLOG sections:
- new equipment: `research\new_equipment\README.md`;
- new abilities: `research\new_abilities\README.md`;
- the **mod kit** they need for new logic: `research\modkit\README.md` (stub modules from `ue4ss\UHTHeaderDump`; needs Build Tools for Visual Studio 2022, only VS 18 is installed).

**Why:** a genuinely new career needs exe changes, which are off-limits.

**How to apply:** when the user returns to this, start from the README. Step 1 is the mode on the backend, step 2 a Mithril role in a raid on our server. Whether `role_skill` from another class works via the table alone is unverified. Related: [[dungeonborne-jester]].
