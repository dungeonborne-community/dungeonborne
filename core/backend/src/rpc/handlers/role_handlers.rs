//! Character creation and the hall: role_create, hall_set_role, role_select_skill; starting equipment and class base attributes (role_level_upgrade by level).
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).

use super::*;
use crate::starter_loadout::{StarterItem, StarterLoadout};

/// Level of a new character. 08.11 user's decision: 1 again (since 07.11 it was 20 — the highest in role_level_upgrade)
const START_ROLE_LEVEL: i32 = 1;

impl GatewaySession {
    /// Upgrades the ID of a grey starting item to a tier whose ExcelAffixTemplate
    /// has BOTH MAJOR (affix_type == 10) AND MINOR (affix_type == 20 with a non-empty pool_random) filled!
    /// In the game tables full server MINOR pools exist for tier 5 (legendary, e.g. 181015000) and tier 2 (green, 181012000).
    pub fn upgrade_starter_item_id(&self, base_item_id: i32, wear_part: i32) -> i32 {
        if !(1..=18).contains(&wear_part) || base_item_id < 180_000_000 {
            return base_item_id;
        }
        let has_complete_pools = |id: i32| -> bool {
            self.game_data.item_type.contains_key(&id)
                && self
                    .game_data
                    .affix_template
                    .get(&id)
                    .and_then(|t| t.data.as_ref())
                    .map(|d| {
                        let has_major = d.random_rule.iter().any(|r| r.affix_type == 10 && !r.pool_random.is_empty());
                        let has_minor = d.random_rule.iter().any(|r| r.affix_type == 20 && !r.pool_random.is_empty());
                        has_major && has_minor
                    })
                    .unwrap_or(false)
        };

        let prefix = (base_item_id / 10000) * 10000;
        let suffix = base_item_id % 1000;
        // Only upward: the tier is not lower than the current one. Otherwise logging in (account_handlers) rewrote worn
        // legendary/purple items (tier 5/4, incl. from starter_loadout and loot) into green tier 2.
        let cur_tier = (base_item_id / 1000) % 10;
        // Pick green tier 2 (e.g. 181012000, 181052001, 182012000), which in the tables has
        // clean base stats and 1 clear minor affix without an overload of 11 attributes!
        for tier in [2, 3, 4].into_iter().filter(|t| *t >= cur_tier) {
            for suf in [suffix, 0, 1, 2] {
                let candidate = prefix + tier * 1000 + suf;
                if has_complete_pools(candidate) {
                    return candidate;
                }
            }
        }
        base_item_id
    }

    /// Builds the character's base attributes STRICTLY for its role_type_id from the role_level_upgrade table!
    /// (Without the 499999 dummy, so that the Wizard does not get DK Soul Energy 5101 and enemies do not drop foreign souls!)
    /// The row — by role level (role_level_upgrade_rows: 20 rows per class; the HashMap stored only level 20).
    /// ROLE_ATTR_LEVEL=<n> — force the row of level n (20 = the old behaviour).
    pub fn build_role_base_attributes(&self, role_type_id: i32, role_level: i32) -> HashMap<i32, f32> {
        let mut attrs = HashMap::new();
        let level = std::env::var("ROLE_ATTR_LEVEL")
            .ok()
            .and_then(|v| v.trim().parse::<i32>().ok())
            .unwrap_or(role_level)
            .max(1);
        let rows = || self.game_data.role_level_upgrade_rows.iter().filter(|r| r.role_type_id == role_type_id);
        // the nearest row not above the level; if there is none — the lowest
        let upg = rows()
            .filter(|r| r.role_level <= level)
            .max_by_key(|r| r.role_level)
            .or_else(|| rows().min_by_key(|r| r.role_level));

        // 1. Read this class's native attributes from role_level_upgrade
        if let Some(upg) = upg {
            if let Some(ref r_attr) = upg.role_attribute {
                for (aid, opt) in Self::map_attribute_offsets(r_attr) {
                    if let Some(ref val_wrapper) = opt {
                        if let Some(ref v_enum) = val_wrapper.value {
                            let s = format!("{:?}", v_enum);
                            if let Some(start) = s.find('(') {
                                if let Some(end) = s.rfind(')') {
                                    if let Ok(fv) = s[start + 1..end].trim().parse::<f32>() {
                                        attrs.insert(aid, fv);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. If the table had no record (fallback), set only base stats without foreign resources
        if attrs.is_empty() {
            attrs.insert(1001, 20.0);
            attrs.insert(1002, 20.0);
            attrs.insert(1003, 20.0);
            attrs.insert(1004, 20.0);
            attrs.insert(1005, 20.0);
            attrs.insert(1006, 20.0);
            attrs.insert(2001, 558.0);
            attrs.insert(3001, 330.0);
        }
        attrs
    }

    /// Generates the starting equipment (wearing) and backpack items (backpack) from ExcelRoleType.
    /// 08.11 user's decision: grey, as in the game tables (without upgrade_starter_item_id); granted on character
    /// creation, after death in a raid (raid_native) and on login if the character wears nothing.
    pub(super) fn generate_starter_items(
        &self,
        role_type_id: i32,
        role_guid: i64,
    ) -> (Vec<tgf::DItem>, Vec<tgf::DItem>) {
        let Some(role_cfg) = self.game_data.role_type.get(&role_type_id) else {
            return (Vec::new(), Vec::new());
        };
        let items: Vec<(StarterItem, bool)> = role_cfg
            .create_default_item_config_ids
            .iter()
            .filter_map(|id| self.game_data.role_default_equipment.get(id))
            .map(|eq| {
                let item = StarterItem { item_id: eq.item_id, count: eq.count.max(1), wear_part: eq.wear_part };
                (item, eq.storage_type == 3 || eq.wear_part > 0)
            })
            .collect();
        self.build_starter_items(&items, role_guid, false)
    }

    /// Loadout items into DItem: (item, whether worn). Worn — into the wear_part slot with affixes
    /// (`upgrade` — the grey tier is raised by upgrade_starter_item_id), the rest — into the 7×10 backpack by rows.
    fn build_starter_items(&self, items: &[(StarterItem, bool)], role_guid: i64, upgrade: bool) -> (Vec<tgf::DItem>, Vec<tgf::DItem>) {
        let mut wearing_items = Vec::new();
        let mut backpack_items = Vec::new();

        let mut bp_x = 0;
        let mut bp_y = 0;
        let mut bp_row_h = 0;

        for (it, worn) in items {
            let item_guid = crate::guid::next_item();
            let count = it.count.max(1);

            if *worn {
                let actual_item_id = if upgrade { self.upgrade_starter_item_id(it.item_id, it.wear_part) } else { it.item_id };
                let affix_data = self.build_item_affix_data(actual_item_id, it.wear_part);

                wearing_items.push(tgf::DItem {
                    basic: Some(tgf::DItemBasic {
                        type_id: actual_item_id,
                        count,
                        guid: item_guid,
                        expire_timepoint: 0,
                        position: Some(tgf::DItemPosition {
                            r#type: 3, // RoleWearing
                            guid: role_guid,
                            x: it.wear_part,
                            y: 0,
                            limit_option: None,
                        }),
                    }),
                    data: Some(tgf::DItemData {
                        affix_data,
                        ..Default::default()
                    }),
                    create_info: None,
                });
            } else {
                // Backpack 7 (x, row_size) × 10 (y): by rows, taking the item shape into account (UeSource_ItemType)
                let shape = self.inv_item_shape(it.item_id);
                if bp_x + shape.w > 7 {
                    bp_x = 0;
                    bp_y += bp_row_h;
                    bp_row_h = 0;
                }
                backpack_items.push(tgf::DItem {
                    basic: Some(tgf::DItemBasic {
                        type_id: it.item_id,
                        count,
                        guid: item_guid,
                        expire_timepoint: 0,
                        position: Some(tgf::DItemPosition {
                            r#type: 2, // Backpack
                            guid: role_guid,
                            x: bp_x,
                            y: bp_y,
                            limit_option: None,
                        }),
                    }),
                    // gear in the backpack (crossbow etc.) — with affixes too: without BASIC "ROLE_HOLD 1101" a weapon has 0 damage (05.11)
                    data: Some(tgf::DItemData { affix_data: self.inv_gear_affixes(it.item_id), ..Default::default() }),
                    create_info: None,
                });

                bp_x += shape.w;
                bp_row_h = bp_row_h.max(shape.h);
            }
        }

        (wearing_items, backpack_items)
    }

    /// Loadout from the DB (starter_loadout): items as they are, without upgrade_starter_item_id
    fn loadout_items(&self, lo: &StarterLoadout, role_guid: i64) -> (Vec<tgf::DItem>, Vec<tgf::DItem>) {
        let items: Vec<(StarterItem, bool)> = lo
            .wearing
            .iter()
            .map(|i| (i.clone(), true))
            .chain(lo.backpack.iter().map(|i| (i.clone(), false)))
            .collect();
        self.build_starter_items(&items, role_guid, false)
    }

    /// 4. Creating a new character (take the exact race, career, skills and equipment from ExcelRoleType!)
    /// A new character for a role_create request: the ExcelRoleType table (race/career/force, starting skills),
    /// starting equipment, base attributes. Does not add it to the player and sends nothing.
    /// `loadout` — loadout from the DB (starter_loadout); None — from ExcelRoleDefaultEquipment.
    pub(super) fn build_new_role(
        &self,
        player: &PlayerModel,
        req: &tgf::CsRoleCreateReq,
        loadout: Option<&StarterLoadout>,
    ) -> (tgf::DUserRole, i32, i32) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let new_guid = (player.user_id * 100 + player.next_guid + 1) as i64;
        let create_idx = (player.roles.len() + 1) as i64;

        // Take the exact race, career, force and starting skills from the ExcelRoleType table
        let role_cfg = self.game_data.role_type.get(&req.role_type_id);
        let (race, career, cfg_force, initial_active_skills, initial_passive_skills) =
            if let Some(cfg) = role_cfg {
                let r = cfg.meta.as_ref().map(|m| m.race).unwrap_or(1);
                let c = cfg.meta.as_ref().map(|m| m.career).unwrap_or(1);
                let f = if cfg.force != 0 { cfg.force } else { 1 };
                (
                    r,
                    c,
                    f,
                    cfg.role_initial_skill.clone(),
                    cfg.role_initial_passive_skill.clone(),
                )
            } else {
                (1, 1, 1, vec![], vec![])
            };

        // Generate the starting equipment (weapons/armor into wearing and consumables into backpack)
        let (wearing_items, backpack_items) = match loadout {
            Some(lo) => self.loadout_items(lo, new_guid),
            None => self.generate_starter_items(req.role_type_id, new_guid),
        };

        let basic = tgf::DUserRoleBasic {
            type_id: req.role_type_id,
            meta: Some(tgf::DRoleMetaIndex {
                race,
                career,
                gender: if req.gender != 0 { req.gender } else { 1 },
                force: if req.force != 0 { req.force } else { cfg_force },
            }),
            role_guid: new_guid,
            role_name: req.role_name.clone(),
            create_index: create_idx,
            create_timepoint: now,
        };

        let attrs = self.build_role_base_attributes(req.role_type_id, START_ROLE_LEVEL);

        let mut dress_up = req.dress_up.clone().unwrap_or_default();
        if dress_up.dress_facade_info.is_none() {
            dress_up.dress_facade_info = Some(tgf::UserRoleSkinWearInfo {
                role_id: new_guid,
                skin_ids: vec![],
                hidden_helmet: false,
            });
        }

        let user_role = tgf::DUserRole {
            basic: Some(basic.clone()),
            last_use_timepoint: now,
            lazy_delete_timepoint: 0,
            force_show_defult_name: false,
            is_save_on_ea_close_for_recover: false,
            is_clear_error_rank_dk_statistics_data: false,
            deploy_role: Some(tgf::DBattleDeployRole {
                basic: Some(basic),
                backpack: Some(tgf::DRoleBackpackData {
                    items: backpack_items,
                    row_size: 7,
                    column_size: 10,
                }),
                wearing: Some(tgf::DRoleWearingData {
                    items: wearing_items,
                }),
                dress_up: Some(dress_up),
                role_level: START_ROLE_LEVEL,
                role_affix: Some(tgf::DBattleAffixAppendData {
                    attribute: attrs,
                    ..Default::default()
                }),
                role_skill: Some(tgf::DRoleSkillData {
                    role_active_skill: initial_active_skills,
                    role_passive_skill: initial_passive_skills,
                }),
                exp: 0,
                small_backpack: Some(tgf::DRoleBackpackData::default()),
                history_role_max_level: START_ROLE_LEVEL,
                history_role_max_total_level: START_ROLE_LEVEL,
                last_adventurer_role_guid: new_guid,
                ..Default::default()
            }),
            statistics: None,
        };
        (user_role, race, career)
    }

    pub(super) async fn role_create_impl(&self, req: tgf::CsRoleCreateReq) -> Option<tgf::ScRoleCreateRsp> {
        let open_id = self.clean_open_id();
        let mut player = self
            .db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

        // A character name is unique across the whole DB (case-insensitive, roles pending deletion also take the name).
        // Session 02.11: two role_create "Char2" within 8 s were both accepted → duplicates in the character list.
        let name = req.role_name.trim().to_lowercase();
        if name.is_empty() {
            self.set_rpc_error(tgf::EnErrorCode::EnErrRoleCreateFailedRoleNameLengthNotReachLowerLimit as i32);
            return Some(tgf::ScRoleCreateRsp { role_guid: 0 });
        }
        let taken = player.roles.iter().any(|r| r.role_name.trim().to_lowercase() == name)
            || self
                .player_load_all()
                .await
                .iter()
                .any(|o| o.open_id != player.open_id && o.roles.iter().any(|r| r.role_name.trim().to_lowercase() == name));
        if taken {
            println!("[Role] role_create '{}' rejected: name taken", req.role_name);
            self.set_rpc_error(tgf::EnErrorCode::EnErrRoleCreateFailedRoleNameAlreadyToken as i32);
            return Some(tgf::ScRoleCreateRsp { role_guid: 0 });
        }

        let loadout = crate::starter_loadout::get(&self.db, req.role_type_id).await;
        let (user_role, race, career) = self.build_new_role(&player, &req, loadout.as_ref());

        let created_guid = player.add_role(
            req.role_type_id as u32,
            req.role_name.clone(),
            user_role.clone(),
        );
        if let Some(r) = player.get_role_mut(created_guid) {
            r.role_level = START_ROLE_LEVEL as u32;
        }
        player.hall_role_guid = created_guid;
        self.apply_bag_sizes(&mut player); // 09.11: backpack width by race
        self.db.save("players", &open_id, &player).await;

        println!(
            "[RoleCreate] Created character '{}' (type_id={}, race={}, career={}, guid={}, equipment: {})",
            req.role_name,
            req.role_type_id,
            race,
            career,
            created_guid,
            if loadout.is_some() { "starter_loadout" } else { "ExcelRoleDefaultEquipment" }
        );

        // Push as in PVE SoloLobby (rpc_dump/PVE.jsonl 02.11): only the new role and the hall. Before, there were also
        // dirty_level_storage / dirty_level_current / dirty_team_passive_data — after them the creation screen did not move
        // on (the client did not send player_get_info, the player pressed "create" again → duplicate). The client takes levels from
        // player_get_info, which it requests itself after this push.
        self.push(
            "tgf.GameClientService.player_dirty_change",
            "tgf.SCPlayerDirtyChgSync",
            tgf::ScPlayerDirtyChgSync {
                dirty_role: vec![user_role.clone()],
                dirty_hall_setting: Some(tgf::DHallSetting { role_guid: created_guid as i64 }),
                ..Default::default()
            },
        );

        // Reset of the new role's and player's cache — as in PVE (two pushes: {EN_ROT_ROLE_BASIC, role}, {EN_ROT_PLAYER, player}).
        // Before, the types were wrong (2/3/5 instead of 4/1): the client did not update the role and did not leave the creation screen.
        self.push_role_cache_expired(player.user_id, created_guid);

        Some(tgf::ScRoleCreateRsp {
            role_guid: created_guid as i64,
        })
    }

    /// 5. Switching the active character in the Hall
    pub(super) async fn hall_set_role_impl(
        &self,
        req: tgf::CsHallSetRoleReq,
    ) -> Option<tgf::ScHallSetRoleRsp> {
        // As in PVE SoloLobby (rpc_dump/PVE.jsonl 02.11): push dirty_role (the selected role) + dirty_hall_setting,
        // then reset the role and player cache — the client updates the hall from them and closes the character list window.
        let open_id = self.clean_open_id();
        let Some(mut player) = self.db.get::<PlayerModel>("players", &open_id).await else {
            return Some(tgf::ScHallSetRoleRsp {});
        };
        let role_guid = req.role_guid as u64;
        // last_use_timepoint of the selected role = now (PVE does this): the client apparently uses it to pick the "current"
        // role in EnterHallWithGuid(Select, Current) — without it the list window does not close after a role switch.
        let now = Self::now_ts();
        let Some(_) = player.get_role_mut(role_guid).map(|r| r.data.last_use_timepoint = now) else {
            println!("[Hall] {}: hall_set_role — no role {}", open_id, role_guid);
            self.set_rpc_error(tgf::EnErrorCode::EnErrRoleNotFound as i32);
            return Some(tgf::ScHallSetRoleRsp {});
        };
        player.hall_role_guid = role_guid;
        let role = player.get_role_mut(role_guid).map(|r| r.data.clone()).unwrap_or_default();
        self.db.save("players", &open_id, &player).await;
        Self::write_client_loadout_file(&player);

        self.push(
            "tgf.GameClientService.player_dirty_change",
            "tgf.SCPlayerDirtyChgSync",
            tgf::ScPlayerDirtyChgSync {
                dirty_role: vec![role],
                dirty_hall_setting: Some(tgf::DHallSetting { role_guid: req.role_guid }),
                ..Default::default()
            },
        );
        self.push_role_cache_expired(player.user_id, role_guid);

        Some(tgf::ScHallSetRoleRsp {})
    }

    /// 6. Selecting / switching the character's active skill
    pub(super) async fn role_select_skill_impl(
        &self,
        req: tgf::CsRoleSelectSkillReq,
    ) -> Option<tgf::ScRoleSelectSkillRsp> {
        let open_id = self.clean_open_id();
        let mut player = self
            .db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

        let mut updated_role = None;

        if let Some(new_skill) = &req.new_skill {
            if let Some(role_model) = player.get_role_mut(req.role_guid as u64) {
                let deploy = role_model
                    .data
                    .deploy_role
                    .get_or_insert_with(tgf::DBattleDeployRole::default);
                let skill_data = deploy
                    .role_skill
                    .get_or_insert_with(tgf::DRoleSkillData::default);

                if let Some(pos) = skill_data
                    .role_active_skill
                    .iter_mut()
                    .find(|s| s.key_position == new_skill.key_position)
                {
                    pos.skill_id = new_skill.skill_id;
                } else {
                    skill_data.role_active_skill.push(new_skill.clone());
                }

                updated_role = Some(role_model.data.clone());
            }
            self.db.save("players", &open_id, &player).await;
        }

        if let Some(dirty_role_data) = updated_role {
            self.push(
                "tgf.GameClientService.player_dirty_change",
                "tgf.SCPlayerDirtyChgSync",
                tgf::ScPlayerDirtyChgSync {
                    dirty_role: vec![dirty_role_data],
                    ..Default::default()
                },
            );
        }

        Some(tgf::ScRoleSelectSkillRsp {
            role_guid: req.role_guid,
            new_skill: req.new_skill,
        })
    }
}
