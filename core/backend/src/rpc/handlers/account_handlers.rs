//! Login and profile: pre_login, login, player_get_info (assembling all player data for the client), client settings, cache_batch_get and cache resets.
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).

use super::*;

impl GatewaySession {
    /// 1. Pre-login
    pub(super) async fn pre_login_impl(&self, _req: tgf::CsPreLoginReq) -> Option<tgf::ScPreLoginRsp> {
        Some(tgf::ScPreLoginRsp {
            info: Some(tgf::PlayerPreLoginInfo {
                allow_login: true,
                await_queue_index: 0,
                total_index: 0,
                server_full: false,
                await_time: 0,
            }),
        })
    }

    /// Maps for the client (player_get_info.user_level, dirty_level_storage): all unlocked
    pub(super) fn level_storage(_player: &PlayerModel) -> tgf::DUserLevelStorage {
        let now = Self::now_ts();
        let level = [
            40003015, 40003016, 40003017, 40003018, 40003019, 40003020,
            50000003, 50000004, 50000005, 50000006, 50000007,
        ]
        .into_iter()
        .map(|level_id| tgf::DLevelModeCompetitionSeasonLevel { level_id, expired_time: now + 86400 * 365 })
        .collect();
        tgf::DUserLevelStorage {
            level_type: 2001,
            battle_data: None,
            level_mode: Some(tgf::d_user_level_storage::LevelMode::CompetitionSeasonPvpve(tgf::DModeLevelCompetitionSeasonGroup { level })),
        }
    }

    /// Character of the game server account (prefix "Host") or of a stand test client ("Stand"): class 400005 (or the
    /// first from the table), name <prefix><user_id>
    async fn server_role_create(&self, player: &mut PlayerModel, prefix: &str) {
        const SERVER_ROLE_TYPE: i32 = 400005;
        let type_id = if self.game_data.role_type.contains_key(&SERVER_ROLE_TYPE) {
            SERVER_ROLE_TYPE
        } else {
            self.game_data.role_type.keys().copied().min().unwrap_or(SERVER_ROLE_TYPE)
        };
        let req = tgf::CsRoleCreateReq {
            role_type_id: type_id,
            role_name: format!("{prefix}{}", player.user_id),
            gender: 1,
            ..Default::default()
        };
        let (role, _, _) = self.build_new_role(player, &req, None);
        let guid = player.add_role(type_id as u32, req.role_name.clone(), role);
        player.hall_role_guid = guid;
        self.db.save("players", &player.open_id, &*player).await;
        println!("[Login] {prefix} account {} ({}): created character '{}' type={} guid={}", player.open_id, self.peer_ip, req.role_name, type_id, guid);
    }

    /// 2. Main login method (with multi-account support!)
    pub(super) async fn login_impl(&self, req: tgf::CsLoginReq) -> Option<tgf::ScLoginRsp> {
        let open_id = self.clean_open_id();

        let mut player = match self.db.get::<PlayerModel>("players", &open_id).await {
            Some(existing_player) => {
                println!(
                    "[Login] Player loaded from DB: open_id={}, user_id={}, characters={}",
                    existing_player.open_id,
                    existing_player.user_id,
                    existing_player.roles.len()
                );
                let mut p = existing_player;
                crate::auth::udp_tunnel::restore(&self.db, &p.open_id, p.state["udp_tunnel"] == serde_json::json!(true)).await;
                // own people (gm/admin) play during maintenance too — roles from the account (lobby::maintenance)
                if let Some(a) = crate::auth::accounts::by_open_id(&self.db, &p.open_id).await {
                    crate::lobby::maintenance::note_roles(&a.open_id, &a.roles, a.banned);
                }
                // one-time top-up to the starting gold for those created under the old one (50 000)
                if p.state["start_gold"].is_null() {
                    let _ = common::add_coin(&mut p, common::GOLD, common::START_GOLD - 50_000);
                    p.state["start_gold"] = serde_json::json!(common::START_GOLD);
                    self.db.save("players", &open_id, &p).await;
                    println!("[Login] {}: topped up to starting gold {}, now {}", open_id, common::START_GOLD, common::coin(&p, common::GOLD));
                }
                // items without a guid / with a foreign position (raid results before 05.11) — repaired on login
                let repaired = super::inventory_handlers::repair_items(&mut p) + self.inv_repair_gear_affixes(&mut p);
                if repaired > 0 {
                    self.db.save("players", &open_id, &p).await;
                    println!("[Login] {}: items repaired (guid/position): {}", open_id, repaired);
                }
                p
            }
            None => {
                // Generate a unique user_id from the last 6 digits of the SteamID (open_id),
                // so that different players do not conflict on the UNIQUE user_id index in SurrealDB!
                let suffix: u64 = open_id
                    .chars()
                    .filter(|c| c.is_ascii_digit())
                    .rev()
                    .take(6)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
                    .parse()
                    .unwrap_or(1);
                // launcher account (src/auth/accounts.rs): user_id from the account number; otherwise — the old rule (C4)
                let new_user_id = crate::auth::accounts::user_id_for_open_id(&open_id).unwrap_or(10_000_000 + suffix);

                let new_player = PlayerModel::new(&open_id, new_user_id);
                self.db.save("players", &open_id, &new_player).await;
                // the tunnel could have been enabled from the launcher before the first game login
                crate::auth::udp_tunnel::restore(&self.db, &open_id, false).await;

                println!(
                    "[Login] Created and saved a new player: open_id={}, user_id={}",
                    open_id, new_user_id
                );
                new_player
            }
        };

        // The game server enters the lobby as a player (the machine's Goldberg ID); without a character the game stays on the creation screen,
        // the hall does not load and the server mod never reaches ready (Robot 03.11). We create a character for it ourselves.
        // game server: by IP (DS_TRUSTED_IPS, relay mode) or — on a stand without a relay, where client and server are both
        // 127.0.0.1 — by open_id from DS_HOST_OPEN_IDS (08.11: without a character the stand host did not enter the lobby)
        let game_server = self.peer_ip.parse().is_ok_and(crate::auth::registry::is_game_server)
            || (!crate::auth::require_relay()
                && std::env::var("DS_HOST_OPEN_IDS").is_ok_and(|v| v.split(',').any(|o| o.trim() == open_id)));
        // season forge materials (features.start_materials) — once; the key is the old one, as with PLAYTEST=1
        let materials = &crate::season::features().start_materials;
        if !game_server && !materials.is_empty() && player.state["playtest_materials"].is_null() {
            for g in materials {
                let _ = common::add_coin(&mut player, g.item, g.count);
            }
            player.state["playtest_materials"] = serde_json::json!(true);
            self.db.save("players", &open_id, &player).await;
            println!("[Login] {}: season forge materials granted", open_id);
        }
        // stand test clients (no relay, STAND_CLIENT_OPEN_IDS from run_local.ps1): a character too, so an autotest client
        // (run.ps1, -dbclient) gets past the creation screen on a fresh database (19.11, clean public run: it stayed on Login)
        let stand_client = !game_server
            && !crate::auth::require_relay()
            && std::env::var("STAND_CLIENT_OPEN_IDS").is_ok_and(|v| v.split(',').any(|o| o.trim() == open_id));
        if player.roles.is_empty() && (game_server || stand_client) {
            self.server_role_create(&mut player, if game_server { "Host" } else { "Stand" }).await;
        }

        // As the official server (sniffer_log.txt:20) and PVE: heartbeat_interval in seconds (30 — the client sends a ping every
        // 30 s; it was 30000 → there were no pings at all), version = the client's package_version, reconnect_uid and
        // max_session_sequence are not filled.
        Some(tgf::ScLoginRsp {
            open_id: player.open_id,
            user_id: player.user_id,
            reconnect_uid: 0,
            heartbeat_interval: 30,
            is_new_player: player.roles.is_empty(),
            version_type: 0,
            zone_id: 1,
            account_id: player.user_id,
            config_cs_task_timeout: 10000,
            max_session_sequence: 0,
            ban_data: None,
            version: req.package_version,
        })
    }

    /// 3. Loading player info (profile, roles, inventory, hall, map, settings)
    pub(super) async fn player_get_info_impl(
        &self,
        req: tgf::CsPlayerGetInfoReq,
    ) -> Option<tgf::ScPlayerGetInfoRsp> {
        let open_id = self.clean_open_id();
        let mut player = self
            .db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        // Roles whose lazy deletion has expired (role_delete_lazy) — remove before building the response
        if player_handlers::player_purge_lazy_deleted_roles(&mut player, now) {
            self.db.save("players", &open_id, &player).await;
        }

        // Auto-repair of metadata (race, career, skills, starting equipment) for already created characters
        let mut roles_fixed = false;

        for role_model in &mut player.roles {
            let role_type_id = role_model.type_id as i32;
            let role_guid = role_model.role_guid as i64;

            if let Some(cfg) = self.game_data.role_type.get(&role_type_id) {
                if let Some(cfg_meta) = &cfg.meta {
                    if let Some(basic) = &mut role_model.data.basic {
                        let gender = basic.meta.as_ref().map(|m| m.gender).unwrap_or(1);
                        let force = if cfg.force != 0 { cfg.force } else { 1 };
                        let fixed_meta = tgf::DRoleMetaIndex {
                            race: cfg_meta.race,
                            career: cfg_meta.career,
                            gender,
                            force,
                        };
                        if basic.meta.as_ref() != Some(&fixed_meta) {
                            basic.meta = Some(fixed_meta);
                            roles_fixed = true;
                        }

                        if let Some(deploy) = &mut role_model.data.deploy_role {
                            deploy.basic = Some(basic.clone());

                            // Always sync the role's base attributes strictly with the role_level_upgrade table for this role_type_id
                            // (this removes the foreign Soul Energy 5101 from the Wizard and sets the exact class stats from the table!)
                            let level = if deploy.role_level > 0 { deploy.role_level } else { role_model.role_level as i32 };
                            let clean_attrs = self.build_role_base_attributes(role_type_id, level);
                            let role_affix = deploy
                                .role_affix
                                .get_or_insert_with(tgf::DBattleAffixAppendData::default);
                            if role_affix.attribute != clean_attrs {
                                role_affix.attribute = clean_attrs;
                                roles_fixed = true;
                            }

                            // If there are no skills — fill with the starting ones from ExcelRoleType
                            if deploy
                                .role_skill
                                .as_ref()
                                .map(|s| s.role_active_skill.is_empty())
                                .unwrap_or(true)
                            {
                                deploy.role_skill = Some(tgf::DRoleSkillData {
                                    role_active_skill: cfg.role_initial_skill.clone(),
                                    role_passive_skill: cfg.role_initial_passive_skill.clone(),
                                });
                                roles_fixed = true;
                            }

                            // If no starting equipment is worn — grant it from ExcelRoleDefaultEquipment
                            if deploy
                                .wearing
                                .as_ref()
                                .map(|w| w.items.is_empty())
                                .unwrap_or(true)
                            {
                                let (wearing_items, backpack_items) = self.generate_starter_items(
                                    role_type_id,
                                    role_guid,
                                );
                                if !wearing_items.is_empty() || !backpack_items.is_empty() {
                                    deploy.wearing = Some(tgf::DRoleWearingData {
                                        items: wearing_items,
                                    });
                                    deploy.backpack = Some(tgf::DRoleBackpackData {
                                        items: backpack_items,
                                        row_size: 0, // size — apply_bag_sizes below (by race, 09.11)
                                        column_size: 0,
                                    });
                                    roles_fixed = true;
                                }
                            }

                            // Auto-repair of 0 damage on already worn weapons and armor (if there is no full BASIC + MAJOR set)
                            let mut rebuilt_items = Vec::new();
                            if let Some(wearing) = &deploy.wearing {
                                for item in &wearing.items {
                                    let mut cloned = item.clone();
                                    let (item_id, slot) = cloned
                                        .basic
                                        .as_ref()
                                        .map(|b| {
                                            (
                                                b.type_id,
                                                b.position.as_ref().map(|p| p.x).unwrap_or(0),
                                            )
                                        })
                                        .unwrap_or((0, 0));
                                    if (1..=18).contains(&slot) && item_id > 0 {
                                        // 08.11: grey is not raised to green (user's decision) — the type as it is
                                        let upgraded_id = item_id;
                                        if let Some(ref mut b) = cloned.basic {
                                            if b.type_id != upgraded_id {
                                                b.type_id = upgraded_id;
                                                roles_fixed = true;
                                            }
                                        }
                                        let item_data =
                                            cloned.data.get_or_insert_with(tgf::DItemData::default);
                                        // Rebuild affixes when the schema version changes (6 = PVE format: BASIC only with offset, LEGENDARY, gear_set)
                                        let need_schema_upgrade = player.state["affix_schema_ver"].as_i64().unwrap_or(0) != 6;
                                        if need_schema_upgrade || item_id != upgraded_id || item_data.affix_data.is_empty() {
                                            item_data.affix_data = self.build_item_affix_data(upgraded_id, slot);
                                            roles_fixed = true;
                                        }
                                    }
                                    rebuilt_items.push(cloned);
                                }
                            }
                            if roles_fixed {
                                if let Some(wearing) = &mut deploy.wearing {
                                    wearing.items = rebuilt_items;
                                }
                            }
                        }
                    }
                }
            }
        }

        // 09.11: backpack width by race (inventory_handlers::fit_bags), the excess — to a free spot
        if self.apply_bag_sizes(&mut player) {
            roles_fixed = true;
        }

        if roles_fixed {
            player.state["affix_schema_ver"] = serde_json::json!(6);
            self.db.save("players", &open_id, &player).await;
        }
        Self::write_client_loadout_file(&player);

        // Restore the saved client settings (DPlayerSettingData; since 11.11 — the player_settings record)
        let saved_setting: Option<tgf::DPlayerSettingData> = self.settings_load(&open_id).await;

        let mut rsp = tgf::ScPlayerGetInfoRsp {
            zone_offset: 10800, // UTC+3
            ..Default::default()
        };

        // Player profile + client settings
        if req.need_player_info {
            rsp.player_info = Some(tgf::DPlayerInfo {
                player: Some(player_handlers::player_profile(&player)),
                private_options: player_handlers::player_private_options(&player),
                player_level: 1,
                player_stat: Some(tgf::DPlayerStat {
                    register_time: now,
                    last_login_time: now,
                    register_country: "RU".to_string(),
                    register_alpha2: "RU".to_string(),
                    login_country: "RU".to_string(),
                    login_alpha2: "RU".to_string(),
                    stat_cumulative_login_days: 1,
                    stat_consecutive_login_days: 1,
                }),
                player_setting: Some(saved_setting.unwrap_or_default()),
                user_data_version: 1,
                birthday: player_handlers::player_birthday(&player),
                ..Default::default()
            });
        }

        if req.need_player_shared_options {
            rsp.player_shared_options = Some(tgf::DPlayerSharedOptions {});
        }

        // The player's character list
        if req.need_user_role {
            // 09.11: character statistics (stats.rs) — in the role data
            rsp.user_role = player
                .roles
                .iter()
                .map(|r| {
                    let mut d = r.data.clone();
                    d.statistics = Some(super::stats::role_statistics(&player, r.role_guid, self.role_career(r.type_id as i32)));
                    d
                })
                .collect();
        }

        // Which character currently stands in the Hall
        if req.need_user_hall_setting {
            let hall_guid = if player.hall_role_guid != 0 {
                player.hall_role_guid
            } else {
                player.roles.first().map(|r| r.role_guid).unwrap_or(0)
            };
            rsp.user_hall_setting = Some(tgf::DHallSetting {
                role_guid: hall_guid as i64,
            });
        }

        // Inventory (stash: 17x10 pages from player.stash, and gold)
        if req.need_user_invetory {
            // gold + other coins from state["coins"] (common::add_coin)
            let mut coins_data = vec![common::coin_dirty(&player, common::GOLD)];
            let mut virtual_items = Vec::new();
            if let Some(map) = player.state["coins"].as_object() {
                for id in map.keys().filter_map(|k| k.parse::<i32>().ok()).filter(|id| *id != common::GOLD) {
                    if common::is_client_coin(id) {
                        coins_data.push(common::coin_dirty(&player, id));
                    } else {
                        virtual_items.push(common::virtual_item(id, common::coin(&player, id)));
                    }
                }
            }
            let mut inventory_pages = inventory_handlers::stash_pages(&player);
            if let Some(group) = inventory_pages.first_mut().and_then(|page| page.item_group.first_mut()) {
                group.element.extend(virtual_items);
            }
            rsp.user_invetory = Some(tgf::DUserInvetoryData { coins_data, inventory_pages });
        }

        if req.need_user_level {
            let saved_lvl = Self::get_saved_level_select(&player);
            rsp.user_level = Some(tgf::DUserLevelData {
                default_level: Some(saved_lvl),
                level_storage: vec![Self::level_storage(&player)],
                current_select_challenge: Some(Self::challenger_model(&player)),
                ..Default::default()
            });
        }

        // Unlocked modules: as the official server (sniffer_log) and PVE (rpc_dump/PVE.jsonl) — user_unlock is not
        // filled. Before 02.11 all 32 modules of module_unlock_type were sent as "unlocked", including
        // EN_UNLOCK_MODULE_ID_CHAT_SILENCE (1200002) and CHAT_ONLY_ENGLISH (1200016): the client did not allow writing in the general chat
        // (only "Team"). The client decides (UUnlockBlueprintLibrary); role-based blocks — later via global_ban/role_ban.
        // The only record — a ban on the "chat in English only" module: while IsModuleAllowed(1200016) = true,
        // WBP_Chat_DetailPanel strips Cyrillic on input. The ban — in unlock_modules with global_forbid (UUnlockService
        // 0x145bb7210, agent 03.11): a global_ban record without ban_datas {ban_id 0, ban_end_time} bans nothing
        // (that was the case 02.11–03.11, Cyrillic could not be typed).
        if req.need_user_unlock {
            rsp.user_unlock = Some(tgf::DUserModuleUnlockData {
                unlock_modules: vec![tgf::DUnlockedModule {
                    module_id: crate::tgf::config::EnUnlockModuleId::ChatOnlyEnglish as i32,
                    global_forbid: true,
                    ..Default::default()
                }],
                ..Default::default()
            });
        }

        // user_matching — exactly as in the reference (sniffer_log_001: player_get_info with need_user_matching →
        // {matchingHeartbeatInterval: 1, battleHeartbeatInterval: 5}, the rest empty). 31.10: without it the native button
        // sends pre_search, but search does not go out ("[UMatchService::NativeStartPreMatch] already in matching" on
        // a repeated click). The ban below applied to default() — with zero intervals; we do not touch it.
        if req.need_user_matching {
            rsp.user_matching = Some(tgf::DBattlePlayerMatchingData {
                matching_heartbeat_interval: 1,
                battle_heartbeat_interval: 5,
                ..Default::default()
            });
        }
        // user_team — the reference always sends the field, empty `{}` for a solo player. 31.10: without it the native button after
        // pre_search does not send search and blocks itself ("Cannot be used while searching for a match"). The party snapshot is
        // the same as in dirty_team_snapshot (lobby::team); empty for a solo player, as in the reference.
        if req.need_user_team {
            rsp.user_team = Some(self.team_snapshot(player.user_id).await);
        }
        // NO NO NO DO NOT UNCOMMENT
        // if req.need_user_matching {
        //     rsp.user_matching = Some(tgf::DBattlePlayerMatchingData::default());
        // }
        // if req.need_user_team {
        //     rsp.user_team = Some(tgf::DUserTeamData::default());
        // }
        // user_battle_room is not sent even while a match is running: with it the client travels to the server by itself, and the return is via the
        // match button (lobby_matching_search, docs/RECONNECT.md). Instead of a label on the button (there is no native flag for Lua) —
        // a hint window on login.
        if req.need_user_battle_room && self.reconnect_battle_room(&player).await.is_some() {
            self.push(
                "tgf.GameClientService.modal_message",
                "tgf.SCModalMessageSync",
                tgf::ScModalMessageSync {
                    title: "Бой ещё идёт".into(),
                    content: "Ваш персонаж остался в бою. Нажмите «В бой», чтобы вернуться к нему.".into(),
                },
            );
        }
        // if req.need_user_battle_data {
        //     rsp.user_battle_data = Some(tgf::DUserBattleData::default());
        // }
        if req.need_user_quest {
            // pass quests (daily/weekly) — into the quest domain before the snapshot (Q1)
            if self.act_sync_quests(&mut player) {
                self.db.save("players", &open_id, &player).await;
            }
            rsp.user_quest = Some(self.progress_quests_snapshot(&player));
        }
        if req.need_user_gear_wearing {
            rsp.user_gear_wearing = Some(tgf::DUserGearWearing {});
        }
        if req.need_user_sold_items {
            rsp.user_sold_items_list = Some(shop_handlers::market_sold_list(&player));
        }
        if req.need_user_crafting {
            rsp.user_crafting_data = Some(self.item_crafting_data(&player));
        }
        if req.need_user_soul_bound {
            rsp.user_soul_bound_data = Some(self.item_soul_bound_data(&player));
        }
        if req.need_user_activity_center {
            rsp.activity_center_data = Some(self.act_user_list().await);
        }
        if req.need_user_facade {
            rsp.facade_data = Some(self.player_facade_data(&player));
        }
        if req.need_user_heirloom_data {
            rsp.heirloom_data = Some(self.item_heirloom_data(&player));
        }
        if req.need_mithril_force {
            rsp.mithril_force_data = Some(tgf::DUserMithrilForceData { prestige: 0 });
        }
        if req.need_user_statistics {
            rsp.user_statistics = Some(super::stats::user_statistics(&player, &|t| self.role_career(t as i32)));
        }
        if req.need_user_tiers {
            rsp.user_tiers = Some(tgf::DTiersSeasonData::default());
        }
        if req.need_user_adult {
            rsp.adult_data = Some(tgf::DUserAdultData {
                can_chat: true,
                can_chat_voice: true,
            });
        }
        if req.need_trade_market {
            // 07.11: the auction must always be open (user's decision). With 0 the client showed the auction as closed;
            // the field is until when auction access is open: we set it 10 years ahead (checked on the stand)
            rsp.trade_market_logic_data = Some(tgf::DUserTradeMarketLogic {
                unlock_trade_auction_privilege_timestamp: Self::now_ts() + 10 * 365 * 86_400,
                is_trade_unlock: true,
            });
        }
        if req.need_market_video_popup {
            rsp.popup_videos = shop_handlers::market_popup_list(&player);
        }
        // Vendor tabs: loads and saves the player itself (rotation/generation) — that is why it is last
        if req.need_user_market_tabs {
            rsp.user_market_tabs = self.market_user_tabs().await;
        }
        // deferred raid result pushes (hub::set_defer) — after this response, the client is already in the lobby
        let pending = crate::lobby::hub::take_deferred(&open_id);
        if !pending.is_empty() {
            println!("[RaidNative] {}: deferred result pushes ({}) — after player_get_info", open_id, pending.len());
            let mut ap = self.after_pushes.lock().unwrap();
            for p in pending {
                ap.push((p.method, p.type_url, p.body));
            }
        }

        Some(rsp)
    }

    /// Saving client settings (tutorials, hotkeys, selected tabs)
    pub(super) async fn player_setting_data_impl(
        &self,
        req: tgf::CsPlayerSettingDataReq,
    ) -> Option<tgf::ScPlayerSettingDataRsp> {
        // 11.11: a separate player_settings record, without rewriting the player document and without writes when nothing changed
        // (there were ~80 full player saves per minute — the prod database reached 59 GB in 3 days), see settings.rs
        let open_id = self.clean_open_id();
        if let Some(incoming) = req.setting_data {
            self.settings_merge(&open_id, incoming).await;
        }

        Some(tgf::ScPlayerSettingDataRsp {})
    }

    /// 7. Batch cache fetch (profile, characters, equipment, statistics for the Hall and UI)
    pub(super) async fn cache_batch_get_impl(
        &self,
        req: tgf::CsCacheBatchGetReq,
    ) -> Option<tgf::ScCacheBatchGetRsp> {
        let open_id = self.clean_open_id();
        let current_player = self
            .db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let mut results = Vec::new();
        let mut not_found_keys = Vec::new();

        for key in &req.keys_with_status {
            let inst_id = key.inst_id;
            let zone_id = if key.zone_id != 0 { key.zone_id } else { 1 };
            // The object type is req.cache_type (EnRouterObjectType). key.type is the fetch method (EnGetCacheType:
            // Subscribe / HotData), not the type: they used to be mixed up, and a role cache request with Subscribe returned the player profile
            // (the character disappeared from the hall).
            let cache_type = req.cache_type;

            let target_player = if inst_id == current_player.user_id || inst_id == 0 {
                Some(current_player.clone())
            } else {
                self.db.find_by_user_id::<PlayerModel>(inst_id).await
            };

            // Someone else's role (party mate, friend recommendations): role_guid = user_id * 100 + n — owner by guid.
            // Before, the own role was returned, and both party members saw their mate as themselves.
            let own_role = current_player.roles.iter().any(|r| r.role_guid == inst_id);
            let foreign_request =
                (cache_type == 4 || cache_type == 5) && inst_id != 0 && !own_role && inst_id / 100 != current_player.user_id;
            let role_owner = if foreign_request { self.db.find_by_user_id::<PlayerModel>(inst_id / 100).await } else { None };
            let foreign_role = role_owner.as_ref().and_then(|o| o.roles.iter().find(|r| r.role_guid == inst_id).map(|r| (o, r)));

            let role_entry = current_player
                .roles
                .iter()
                .find(|r| r.role_guid == inst_id)
                .or_else(|| {
                    if current_player.hall_role_guid != 0 {
                        current_player
                            .roles
                            .iter()
                            .find(|r| r.role_guid == current_player.hall_role_guid)
                    } else {
                        current_player.roles.first()
                    }
                });

            let build_user_cache = |p: &PlayerModel| -> tgf::DPlayerBasicData {
                let active_role = p
                    .roles
                    .iter()
                    .find(|r| r.role_guid == p.hall_role_guid)
                    .or_else(|| p.roles.first());

                let (cur_type_id, cur_guid, cur_meta) = if let Some(r) = active_role {
                    let meta = r.data.basic.as_ref().and_then(|b| b.meta.clone());
                    (r.type_id as i32, r.role_guid as i64, meta)
                } else {
                    (0, 0, None)
                };

                tgf::DPlayerBasicData {
                    user_key: Some(tgf::DPlayerIdKey {
                        user_id: p.user_id,
                        zone_id,
                    }),
                    account_type: 21,
                    account_login_channel: 21,
                    account_id: p.user_id,
                    // Online by hub: offline — logout_time > login_time (the last profile record).
                    // TODO(verify): what exactly UserRoleCacheService::GetIsOnline compares for friends/rooms
                    login_data_cache: Some(if crate::lobby::hub::is_online(&p.open_id) {
                        tgf::DLoginBasicDataCache {
                            login_time: now,
                            business_register_time: now,
                            business_login_time: now,
                            ..Default::default()
                        }
                    } else {
                        let seen = p.updated_at.min(now);
                        tgf::DLoginBasicDataCache {
                            login_time: seen - 1,
                            logout_time: seen,
                            business_register_time: seen - 1,
                            business_login_time: seen - 1,
                            business_logout_time: seen,
                            ..Default::default()
                        }
                    }),
                    social_information: Some(tgf::DPlayerSocialInformation::default()),
                    profile: Some(player_handlers::player_profile(p)),
                    player_status_cache: Some(tgf::DPlayerStatusCache {
                        is_in_battle: false,
                        is_matching: false,
                        current_role_type_id: cur_type_id,
                        current_role_guid: cur_guid,
                        current_role_meta: cur_meta,
                        ..Default::default()
                    }),
                    player_level: 1,
                    user_data_version: 1,
                    shared_options: Some(tgf::DPlayerSharedOptions {}),
                    ..Default::default()
                }
            };

            let build_role_basic =
                |p: &PlayerModel, r: &crate::structures::RoleModel| -> tgf::DUserRoleBasicAndUserKey {
                    let basic = r.data.basic.clone().unwrap_or_default();
                    let deploy = r.data.deploy_role.clone().unwrap_or_default();

                    // Build a short list of worn equipment (DRoleWearingBrief) for rendering in the Hall
                    let wearing_brief: Vec<tgf::DRoleWearingBrief> = deploy
                        .wearing
                        .as_ref()
                        .map(|w| {
                            w.items
                                .iter()
                                .filter_map(|item| {
                                    let b = item.basic.as_ref()?;
                                    let pos = b.position.as_ref();
                                    Some(tgf::DRoleWearingBrief {
                                        type_id: b.type_id,
                                        x: pos.map(|p| p.x).unwrap_or(0),
                                        y: pos.map(|p| p.y).unwrap_or(0),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();

                    tgf::DUserRoleBasicAndUserKey {
                        user_key: Some(tgf::DPlayerIdKey {
                            user_id: p.user_id,
                            zone_id,
                        }),
                        role: Some(tgf::DUserRoleCacheBasic {
                            type_id: basic.type_id,
                            meta: basic.meta,
                            role_guid: basic.role_guid,
                            role_name: basic.role_name,
                            last_use_timepoint: r.data.last_use_timepoint,
                            create_timepoint: basic.create_timepoint,
                            create_index: basic.create_index,
                            force_show_defult_name: r.data.force_show_defult_name,
                            role_level: deploy.role_level.max(1),
                            is_removed: false,
                            dress_up: deploy.dress_up,
                            wearing: wearing_brief,
                        }),
                    }
                };

            match cache_type {
                // EN_ROT_PLAYER: UserCache (DPlayerBasicData)
                1 => {
                    let p = target_player.as_ref().unwrap_or(&current_player);
                    results.push(tgf::DCacheObjectData {
                        object_data: Some(tgf::d_cache_object_data::ObjectData::UserCache(
                            build_user_cache(p),
                        )),
                    });
                }
                // EN_ROT_ROLE_BASIC: RoleBasic (DUserRoleBasicAndUserKey)
                4 if foreign_request => match foreign_role {
                    Some((o, r)) => results.push(tgf::DCacheObjectData {
                        object_data: Some(tgf::d_cache_object_data::ObjectData::RoleBasic(build_role_basic(o, r))),
                    }),
                    None => not_found_keys.push(tgf::DCacheKey { type_id: cache_type as u32, zone_id, inst_id }),
                },
                5 if foreign_request => match foreign_role {
                    Some((o, r)) => results.push(tgf::DCacheObjectData {
                        object_data: Some(tgf::d_cache_object_data::ObjectData::RoleDetail(tgf::DUserRoleAndUserKey {
                            user_key: Some(tgf::DPlayerIdKey { user_id: o.user_id, zone_id }),
                            role: Some(r.data.clone()),
                        })),
                    }),
                    None => not_found_keys.push(tgf::DCacheKey { type_id: cache_type as u32, zone_id, inst_id }),
                },
                4 => {
                    if let Some(r) = role_entry {
                        results.push(tgf::DCacheObjectData {
                            object_data: Some(tgf::d_cache_object_data::ObjectData::RoleBasic(
                                build_role_basic(&current_player, r),
                            )),
                        });
                    } else {
                        not_found_keys.push(tgf::DCacheKey {
                            type_id: cache_type as u32,
                            zone_id,
                            inst_id,
                        });
                    }
                }
                // EN_ROT_ROLE_DETAIL: RoleDetail (DUserRoleAndUserKey)
                5 => {
                    if let Some(r) = role_entry {
                        results.push(tgf::DCacheObjectData {
                            object_data: Some(tgf::d_cache_object_data::ObjectData::RoleDetail(
                                tgf::DUserRoleAndUserKey {
                                    user_key: Some(tgf::DPlayerIdKey {
                                        user_id: current_player.user_id,
                                        zone_id,
                                    }),
                                    role: Some(r.data.clone()),
                                },
                            )),
                        });
                    } else {
                        not_found_keys.push(tgf::DCacheKey {
                            type_id: cache_type as u32,
                            zone_id,
                            inst_id,
                        });
                    }
                }
                // EN_ROT_USER_GLOBAL: UserGlobalCache (DUserGlobalBasicData)
                6 => {
                    results.push(tgf::DCacheObjectData {
                        object_data: Some(tgf::d_cache_object_data::ObjectData::UserGlobalCache(
                            tgf::DUserGlobalBasicData {
                                user_id: if inst_id != 0 {
                                    inst_id
                                } else {
                                    current_player.user_id
                                },
                                last_login_zone: zone_id,
                            },
                        )),
                    });
                }
                // EN_ROT_USER_MATCH: UserMatch (DUserMatchData)
                7 => {
                    results.push(tgf::DCacheObjectData {
                        object_data: Some(tgf::d_cache_object_data::ObjectData::UserMatch(
                            tgf::DUserMatchData {
                                user_key: Some(tgf::DPlayerIdKey {
                                    user_id: if inst_id != 0 {
                                        inst_id
                                    } else {
                                        current_player.user_id
                                    },
                                    zone_id,
                                }),
                                current_role_comprehensive_score: 0,
                                pvpve_failed_and_succ_score: 0,
                                matching_ban_user: None,
                                trial_data: vec![],
                            },
                        )),
                    });
                }
                _ => {
                    if let Some(r) = current_player.roles.iter().find(|r| r.role_guid == inst_id) {
                        results.push(tgf::DCacheObjectData {
                            object_data: Some(tgf::d_cache_object_data::ObjectData::RoleDetail(
                                tgf::DUserRoleAndUserKey {
                                    user_key: Some(tgf::DPlayerIdKey {
                                        user_id: current_player.user_id,
                                        zone_id,
                                    }),
                                    role: Some(r.data.clone()),
                                },
                            )),
                        });
                    } else {
                        let p = target_player.as_ref().unwrap_or(&current_player);
                        results.push(tgf::DCacheObjectData {
                            object_data: Some(tgf::d_cache_object_data::ObjectData::UserCache(
                                build_user_cache(p),
                            )),
                        });
                    }
                }
            }
        }

        println!(
            "[CacheBatchGet] cache_type={}, keys={}, returned={}",
            req.cache_type,
            req.keys_with_status.len(),
            results.len()
        );

        Some(tgf::ScCacheBatchGetRsp {
            results,
            client_tag: req.client_tag,
            not_found_keys,
        })
    }

    pub(super) async fn cache_expired_impl(
        &self,
        _req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScCacheExpiredSync> {
        Some(tgf::ScCacheExpiredSync { keys: vec![] })
    }

    pub(super) async fn player_all_cache_expired_impl(
        &self,
        _req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScAllCacheExpiredSync> {
        Some(tgf::ScAllCacheExpiredSync {})
    }

    pub(super) async fn rank_cache_expired_sync_impl(
        &self,
        _req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScRankCacheExpiredSync> {
        Some(tgf::ScRankCacheExpiredSync {})
    }
}
