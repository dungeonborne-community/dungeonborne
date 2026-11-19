# Map of net mode checks in BP (static search, 30.10)

Source: retoc to-legacy with filters `GA_`, `B_TGF_`, `BP_Tgf_`, `BP_TGF_`, `BP_Fighter`, `Enchant`
(1782 assets, `research\bp_legacy`), search for the function name in the asset's name map (`grep -aw`). This is a list of assets
where the function is called at least once, not a count of call sites. CFG: `research\cfg\<asset>.txt` (kismet-analyzer gen-cfg).
Not covered: assets outside these filters (UI `WBP_`, animations `ABP_` partly, maps), native C++ TGF.

| Function | Assets | What it means for "listen as dedicated" |
|---|---|---|
| `IsStandalone` | 24 | false on the server and on clients, as on the official DS; branches work natively |
| `K2_HasAuthority` | 127 | semantics as on DS (the server has no pawn of its own) |
| `HasAuthority` (Actor) | 48 | same |
| `IsDedicatedServer` | 17 | **differs**: a headless listen server answers false. Needs a hook → true only in the `-dbserver` process |
| `IsLocallyControlled` | 15 | as on DS |
| `GetNetMode`, `IsServer` | 0 | not called in these BPs |

## IsStandalone (24)
Base ability classes, so almost all abilities were affected on the old listen host:
`GA_TGF_Base`, `GA_TGF_Active_Base`, `GA_TGF_Skill_Base`, `GA_TGF_MeleeWeapon_Combo_Base`,
`GA_TGF_MeleeWeapon_Charge_Base`, `GA_TGF_MeleeWeapon_AttackWithDefence_Base`, `GA_TGF_RangedWeapon_Player_Base`,
`GA_TGF_FunctionLibrary`. Others: `BP_Fighter_HeroSample`, `BP_Tgf_Hero_Default`, `B_TGF_MeleeWeaponBase`,
`B_TGF_Soul`, Relic Enchant (Dagger_03, Mace, Sword), `GA_TGF_ADS`, `GA_TGF_DeathToQuit`, `GA_TGF_DruidPanther_Q_*` (3),
`GA_TGF_ShieldProjectile`, `GA_TGF_Staff_Ice`, `GA_TGF_Staff_Laser`, `GA_TalentRogue_Steal`.

## IsDedicatedServer (17) — branches to analyse
`BP_Fighter_HeroSample`, `BP_TGF_PlayerState`, `BP_Tgf_Character_Default`, `BP_Tgf_Hero_Default`, `BP_Tgf_Monster`,
`B_TGF_Monster_Track_Bullet_Base`, `B_TGF_Normal_ProjectileBase`, `B_TGF_Track_Projectile_WizardFireEye`,
`GA_TGF_Active_Base`, `GA_TGF_DeathKnight_E`, `GA_TGF_InteractWithMap`, `GA_TGF_InteractWithMapByPress`,
`GA_TGF_MarkPoint`, `GA_TGF_PortalDetect`, `GA_TGF_Rebirth`, `GA_TGF_TalentWizard_FireEye`, `GA_TGF_Wizard_Floating`.
Example `BP_TGF_PlayerState`: `HasAuthority && !IsDedicatedServer → …` is work "for the local player on the server"
(listen host). A headless server without its own player does not need it → hook `IsDedicatedServer = true` in `-dbserver`.
To check: whether UE4SS can override the return value of a native UFUNCTION (`EX_CallMath`).

## IsLocallyControlled (15)
`ABP_TGF_Character_Base`, `ABP_Tgf_SwordMaster_RuneSword_SkillHelper`, `BP_Fighter_HeroSample`, `BP_TGF_PlayerState`,
`BP_Tgf_Hero_Default`, `B_TGF_MeleeWeaponBase`, `B_TGF_Track_Projectile_WizardFireEye`, `GA_TGF_Active_Base`,
`GA_TGF_Charge_Skill_Base`, `GA_TGF_DruidPanther_Q_Human`, `GA_TGF_DruidPanther_Q_Panther`, `GA_TGF_Fighter_E3`,
`GA_TGF_Lich_Q`, `GA_TGF_Staff_Laser`, `GA_TGF_Wizard_Q_New`.
