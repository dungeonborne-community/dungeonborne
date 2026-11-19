---
name: dungeonborne-druid-trees
description: "12.11: druid trees (spikes B_TGF_DruidPanther_Seed) do not hit mobs — a target is taken only when CompareTeams == DifferentTeams; suspicion: mobs have no team"
metadata:
  visibility: team
  node_type: memory
  type: project
---

Complaint 12.11: druid trees do not hit mobs, players are hit fine. BP analysis (retoc + kismet-analyzer, session scratchpad 96188d8f…\druid):

- The "tree" is the trap `B_TGF_DruidPanther_Seed` (child of `B_TGF_Static_OverlapCheck_Destructable_ProjectileBase`). On a timer `GetCheckResult` does `CapsuleTraceMulti`, channel TraceTypeQuery4 (= `Lyra_TraceChannel_Weapon`).
- For each hit: cast to LyraCharacter → skip if tag `BuffState.Stealth` → `LyraTeamSubsystem.CompareTeams(Instigator, target)`. The target is taken **only on result 1 (DifferentTeams)**. If someone has no team (INDEX_NONE), the result is InvalidArgument → the target is silently skipped.
- Then players (`EnumUtils.IsPlayer`) go to FinalPlayerTargets (priority), the rest to FinalTargets. The strike hits one random target.
- The summoned dryad tree `BP_TGF_Summoned_Seed` also filters via CompareTeams.
- Suspicion: on our server mobs have no team set (MyTeamID 255 on the character and AI controller), while players get teams from our mods. In the original mobs must have a team: both the trap (FinalTargets) and BP_Tgf_Monster compare teams. Not verified: a server log is needed.

User clarification: other skills hit mobs, the trees "seem to miss" (the strike animation plays). `PlayAtkAnim` runs only with a non-empty target list, so the mob most likely passes the team filter. Suspicions now: damage is lost in `ApplyProjectileEffect`, the target arrays are cleared within `AtkDelayEffectTime`, or the server trace does not see the mob. A DS log was requested from the orchestrator (hooks PlayAtkAnim and ApplyProjectileEffect, mob teams).

Another clarification: 100% misses only on skeletons. On `BP_Tgf_Monster_Skeleton_Sword` (only on it among those compared; zombies and khazra do not have it) the mesh `CharacterMesh0` overrides responses: Weapon/Weapon_Capsule/Projectile = Block, Weapon_Multi/SnapTarget = Overlap, the rest Ignore — the skeleton's hitbox is the mesh bones. The tree looks for targets with CapsuleTraceMulti on Weapon at its feet. Suspicions: (a) on a headless DS the skeleton's bones and bodies do not follow the animation; (b) the HitResult is on a bone for which ApplyProjectileEffect deals no damage; (c) the skeleton is not in FinalTargets. Log requested from the orchestrator.

**Run 12.11 (orchestrator, StatWatch):** the in-game tree is not the spikes but the summon `BP_TGF_Summoned_Seed_C` (AIC_TGF_DruidSeed, damage GE_TGF_Damage_Instant_Seed_Attack1). The first 5 strikes hit the druid himself, then zombies; not a single one on the skeleton. Statically: the tree's team = `LyraTeamSubsystem.FindTeamFromActor(druid pawn)` (character MyTeamID, not PS) → `B_LyraGameMode.SpawnMonster(..., TeamId)` in `BPFL_TGF_Summon_Base.SpawnOneMonster`. Hypothesis: our setteam writes PS.MyTeamID without replication, the pawn's team is stale, so the druid is an enemy of his own tree. Team log requested.

**Result of run 2 (12.11):** teams are not the cause (tree and druid — team 1, skeleton — 200, CompareTeams tree/skeleton = 1). The strikes on the own druid had 0 damage. The cause is (a): on a headless DS the skeleton has VisibilityBasedAnimTickOption=1 (AlwaysTickPose), bRecentlyRendered=false → the engine does not update the bones (`ShouldUpdateTransform`), pelvis and feet are at one point at the feet, the bone hitbox is collapsed. `BP_Tgf_Monster` briefly sets 0 via SwitchOnNetMode everywhere EXCEPT DedicatedServer → the original probably had the same miss. Proposed: a server mod sets Mesh.VisibilityBasedAnimTickOption=0 on skeletons (better only near the tree or players, re-applying it), this is a workaround for WORKAROUNDS.

**Tree strike mechanism (12.11):** AGRPRO: `AGR_Attack_NotifyState` in `AM_Dryad_Attack1` → `UAGR_CombatManager` traces between the tree mesh sockets Start→End, Start2→End2 → `GameplayEvent.MeleeHit` → `GA_TGF_Melee_Monster_Base` → `ApplyMeleeWeaponEffect`. On the DS with tickopt=1 both the tree sockets and the skeleton's physics bodies are frozen, so hits are random. fix_bones experiment (tickopt=0 for skeletons within 6 m of a tree) — user: "it works". I advise setting 0 on the tree itself too. Skeleton bones: `Bip001-Pelvis`, `Bip001-L-Foot` etc., physics bodies `Bone01..51`, `Point002`; the names `pelvis`/`foot_l` do not exist. Tree: `Pelivs_Bone_01`, `Twig_*`, sockets `Start`/`End`.

**Projectiles (13.11):** the same cause for the relic ice dagger (`B_TGF_Normal_Projectile_DaggerRelicIce`, BoxCollision + ReceiveHit). Profiles from DefaultEngine.ini: channel Projectile = GameTraceChannel7; `TgfCharacterMesh` blocks it, `TgfCharacterCapsule` does not (Ignore). So projectiles hit any character only via the mesh physics bodies; on the skeleton they are frozen in the spawn pose (a heap on the floor) → the dagger flies through. The user proposed enabling fix_bones near players (not only near trees) — agreed: for all mobs within ~30 m of players.

**How to apply:** next step — a DS log: the mob's MyTeamID (character and controller), the druid's team, CompareTeams(druid, mob). The orchestrator writes the mod. If mobs have no team, the fix is a server mod giving mobs a separate team; check that the mobs' AI does not change because of it. Related to [[dungeonborne-bp-reverse]].
