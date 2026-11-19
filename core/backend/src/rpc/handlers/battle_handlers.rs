//! Level selection, matchmaking and the battle room (battle_matching_*, battle_room_*), player data for the match (battle_fetch_battle_user_data, DBattleUserData).
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).

use super::*;

impl GatewaySession {
    /// Gets the saved DLevelSelect from the player's state (default 40003016 — Castle)
    pub fn get_saved_level_select(player: &PlayerModel) -> tgf::DLevelSelect {
        let level_type = player.state["level_select"]["level_type"]
            .as_i64()
            .unwrap_or(2001) as i32;
        let level_id = player.state["level_select"]["level_id"]
            .as_i64()
            .unwrap_or(40003016) as i32;
        let region = player.state["level_select"]["region"]
            .as_str()
            .unwrap_or("eu")
            .to_string();

        tgf::DLevelSelect {
            level_type,
            level_id,
            region,
        }
    }

    /// Converts the selected level_id into level parameters and the map ID
    pub fn build_level_and_map_params(&self, lvl: &tgf::DLevelSelect) -> (tgf::DBattleLevelParameter, i32) {
        Self::level_map_params(lvl)
    }

    /// The same without a session (lobby::matchmaking — pushes of a found match from the tick)
    pub fn level_map_params(lvl: &tgf::DLevelSelect) -> (tgf::DBattleLevelParameter, i32) {
        let (map_id, rule_id, pool_id) = match lvl.level_id {
            40003015 => (10101, 90020, 20029), // Dungeon Casual
            40003017 => (10102, 90024, 20031), // Dungeon Classic
            40003019 => (10103, 90032, 20033), // Dungeon Hard
            40003016 => (10201, 90023, 20030), // Castle Casual
            40003018 => (10202, 90025, 20032), // Castle Classic
            40003020 => (10203, 90033, 20034), // Castle Hard
            50000003 => (30101, 90021, 20021), // Arena DeathMatch
            50000004 => (30102, 90028, 20019), // Arena BO5
            50000005 => (30202, 90029, 20020), // Arena MultiGameMode 2
            50000006 => (30102, 90030, 20027), // Clotho Trial
            50000007 => (30201, 90031, 20028), // Arena DeathMatch 2
            _ => (10201, 90023, 20030),
        };

        let level_param = tgf::DBattleLevelParameter {
            level_type: if lvl.level_type != 0 { lvl.level_type } else { 2001 },
            level_id: lvl.level_id,
            matching_pool_id: pool_id,
            competition_paramteter: Some(tgf::DBattleCompetitionParameter {
                season_id: 1,
                competition_rule_id: rule_id,
                competition_rule_minor_id: 1,
            }),
            region: if lvl.region.is_empty() { "eu".to_string() } else { lvl.region.clone() },
        };

        (level_param, map_id)
    }

    /// Builds the match participant structure (DBattleUserData) with the active character, skills and equipment
    pub fn build_battle_user_data(&self, player: &PlayerModel) -> tgf::DBattleUserData {
        let now = Self::now_ts();
        let active_role = player
            .roles
            .iter()
            .find(|r| r.role_guid == player.hall_role_guid)
            .or_else(|| player.roles.first())
            .and_then(|r| r.data.deploy_role.clone());

        let saved_setting: Option<tgf::DPlayerSettingData> = super::settings::settings_cached(&player.open_id);

        let mut core_fighters = HashMap::new();
        core_fighters.insert(0, tgf::DBattleUserFighterData {});

        tgf::DBattleUserData {
            user_key: Some(tgf::DPlayerIdKey {
                user_id: player.user_id,
                zone_id: 1,
            }),
            user_parameter: Some(tgf::DBattleUserParameter {
                team_id: 1,
                search_timepoint: now,
                pre_search_timepoint: now,
                rank_level: 1,
                player_tier: 1,
                adjust_team_size: 1,
                start_match_team_size: 1,
                faction_id: 1,
                origin_matching_id: format!("match_{}", player.user_id),
                ..Default::default()
            }),
            core_fighters,
            deploy_preview: Some(tgf::DBattleDeployPreview {
                deploy_role: active_role,
                battle_setting: Some(progress_handlers::progress_battle_options(player)),
                user_unlucky: Some(tgf::DUserUnlucky::default()),
                user_soul_bound: Some(tgf::DBattleSoulBound { bound_wear_part: 0 }),
                user_skin: Some(tgf::DBattleSkin { skin_ids: vec![] }),
                client_setting_for_ds: saved_setting,
                tiers_id: 1,
            }),
            user_gamesvr_id: 1,
            ds_player_key: Some(tgf::DBattlePlayerDsKey {
                ds_user_key: player.user_id.to_string(),
                ds_token: "LOCAL_DS_TOKEN".to_string(),
                ds_encryption_key: String::new(),
            }),
            is_robot: false,
            mithril_force_same_faction_rebirths_count: 0,
        }
    }

    /// Returns character data (equipment, skills, stats) when a local match loads (StartLocalBattle)!
    pub(super) async fn battle_fetch_battle_user_data_impl(
        &self,
        _req: crate::tgf::CsBattleFetchBattleUserReq,
    ) -> Option<crate::tgf::ScBattleFetchBattleUserRsp> {
        let open_id = self.clean_open_id();
        let mut player = self
            .db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));
        let user_data = self.local_battle_user_data(&mut player).await;
        println!(
            "[Battle] Sent character data for entering the match (open_id={}, role_guid={})",
            open_id, player.hall_role_guid
        );

        Some(crate::tgf::ScBattleFetchBattleUserRsp {
            data: Some(user_data),
        })
    }

    /// DBattleUserData of a local match — what the native InitPlayerGameData applies to the hero. The same set is returned
    /// to the server by /ds/battle_user_data (data for each listen-server player, the dbnative native helper).
    pub async fn local_battle_user_data(&self, player: &mut PlayerModel) -> tgf::DBattleUserData {
        // As PVE SoloLobby: only user_key, user_parameter{faction_id} and deploy_preview{deploy_role}, plus team_id
        // (party — allies, solo players — each on their own). No core_fighters/ds_player_key/client_setting_for_ds.
        // team_id and faction_id — the room team number, also MyTeamID on the server: the teammate gear icon compares
        // MyTeamID with the item's FactionId (docs/ITEM_FLAG.md). Outside a room — as before
        let full = self.build_battle_user_data(player);
        let number = crate::lobby::matchmaking::team_number(&player.open_id);
        tgf::DBattleUserData {
            user_key: full.user_key,
            user_parameter: Some(tgf::DBattleUserParameter {
                team_id: number.unwrap_or_else(|| crate::lobby::team::battle_team_id(player.user_id)),
                faction_id: number.unwrap_or(1) as i32,
                ..Default::default()
            }),
            deploy_preview: Some(tgf::DBattleDeployPreview {
                deploy_role: full.deploy_preview.and_then(|p| p.deploy_role),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Level/map selection in the lobby (saved to SurrealDB and pushed as dirty_level)
    pub(super) async fn level_select_impl(
        &self,
        req: crate::tgf::CsLevelCompetitionSelectReq,
    ) -> Option<crate::tgf::ScLevelCompetitionSelectRsp> {
        let open_id = self.clean_open_id();
        let mut req = req;
        if let Some(ref mut lvl) = req.level_select {
            println!(
                "[Lobby] Player {} selected map: type={}, id={}, region={}",
                open_id, lvl.level_type, lvl.level_id, lvl.region
            );

            let mut player = self
                .db
                .get::<PlayerModel>("players", &open_id)
                .await
                .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

            let lvl = &*lvl;

            player.state["level_select"] = serde_json::json!({
                "level_type": if lvl.level_type != 0 { lvl.level_type } else { 2001 },
                "level_id": lvl.level_id,
                "region": if lvl.region.is_empty() { "eu" } else { &lvl.region },
            });
            self.db.save("players", &open_id, &player).await;

            self.push(
                "tgf.GameClientService.player_dirty_change",
                "tgf.SCPlayerDirtyChgSync",
                tgf::ScPlayerDirtyChgSync {
                    dirty_team_passive_data: Some(tgf::DTeamPassiveDataDirty {
                        level_select: Some(lvl.clone()),
                    }),
                    ..Default::default()
                },
            );
        }

        Some(crate::tgf::ScLevelCompetitionSelectRsp {
            result_code: 0,
            level_select: req.level_select,
        })
    }

    /// 1. Pre-check before the search
    pub(super) async fn battle_matching_pre_search_impl(
        &self,
        req: crate::tgf::CsBattleMatchingPreSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingPreSearchRsp> {
        let open_id = self.clean_open_id();
        if let Some(ref lvl) = req.level_select {
            let mut player = self
                .db
                .get::<PlayerModel>("players", &open_id)
                .await
                .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

            player.state["level_select"] = serde_json::json!({
                "level_type": if lvl.level_type != 0 { lvl.level_type } else { 2001 },
                "level_id": lvl.level_id,
                "region": if lvl.region.is_empty() { "eu" } else { &lvl.region },
            });
            self.db.save("players", &open_id, &player).await;
        }

        println!("[BattleMatching] pre_search from player {}", open_id);
        Some(crate::tgf::ScBattleMatchingPreSearchRsp {
            lazy_search_random_begin_second: 0,
            lazy_search_random_end_second: 0,
        })
    }

    /// 2. Cancel the pre-search
    pub(super) async fn battle_matching_cancel_pre_search_impl(
        &self,
        _req: crate::tgf::CsBattleMatchingCancelPreSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingCancelPreSearchRsp> {
        Some(crate::tgf::ScBattleMatchingCancelPreSearchRsp {})
    }

    /// 3. Start the match search (with P2P Host / Client coordination)
    pub(super) async fn battle_matching_search_impl(
        &self,
        req: crate::tgf::CsBattleMatchingSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingSearchRsp> {
        Some(self.lobby_matching_search(req).await)
    }

    /// 4. Match search heartbeat (here we push the client to create a local DS!)
    pub(super) async fn battle_matching_heartbeat_impl(
        &self,
        req: crate::tgf::CsBattleMatchingHeartbeatReq,
    ) -> Option<crate::tgf::ScBattleMatchingHeartbeatRsp> {
        // the search is gone (room closed, backend restarted) — "clear search", as on cancel (battle_matching_exit).
        // The EnErrMatchingNotFound error hung the client in an endless "search" (03.11 11:58), FINISHED does not close the search.
        let me = self.clean_open_id();
        let live = self.lobby_matching_heartbeat(&req.matching_id);
        // the room end is deferred until the lobby (hub::has_deferred) — no CANCEL/clear from here, they come with the deferred one
        if !live && crate::lobby::hub::has_deferred(&me) {
            return Some(crate::tgf::ScBattleMatchingHeartbeatRsp {});
        }
        // the client closes the search only on a CANCEL event (reference: ~10 s after FINISHED, MATCHING_DESTROY);
        // the search is gone (room closed, backend restarted) — also CANCEL. Once per (player, search).
        if !req.matching_id.is_empty() && !live {
            let uid = self.load_me().await.user_id;
            if let Some(msg) = crate::lobby::matchmaking::match_cancel_msg(&me, uid, &req.matching_id, crate::lobby::matchmaking::MATCH_EXIT_DESTROY) {
                let id = msg.snapshot.as_ref().map(|s| s.last_event_id).unwrap_or(0);
                self.push(crate::lobby::hub::MATCHING_LOG_METHOD, crate::lobby::hub::MATCHING_LOG_TYPE, msg);
                println!("[Matchmaking] search {} for {}: event {} CANCEL (MATCHING_DESTROY)", req.matching_id, me, id);
            } else if let Some(msg) = crate::lobby::matchmaking::match_cancel_resend(&me, &req.matching_id, req.acknowlodge_event_id) {
                // CANCEL already went out, but the client confirms a lower number — it did not apply it (arrived on the raid map); repeat
                let id = msg.snapshot.as_ref().map(|s| s.last_event_id).unwrap_or(0);
                self.push(crate::lobby::hub::MATCHING_LOG_METHOD, crate::lobby::hub::MATCHING_LOG_TYPE, msg);
                println!("[Matchmaking] search {} for {}: client confirmed {} — repeating CANCEL (event {})", req.matching_id, me,
                    req.acknowlodge_event_id, id);
            }
        }
        if !live
            && !req.matching_id.is_empty()
            && super::lobby_handlers::clear_matching_once(&me, &req.matching_id)
        {
            self.push(
                crate::lobby::hub::DIRTY_METHOD,
                crate::lobby::hub::DIRTY_TYPE,
                crate::tgf::ScPlayerDirtyChgSync { dirty_clear_matching: vec![req.matching_id.clone()], ..Default::default() },
            );
        }
        Some(crate::tgf::ScBattleMatchingHeartbeatRsp {})
    }

    /// 5. Leave the search queue
    pub(super) async fn battle_matching_exit_impl(
        &self,
        req: crate::tgf::CsBattleMatchingExitReq,
    ) -> Option<crate::tgf::ScBattleMatchingExitRsp> {
        // the match is already found, the match has not started: leave the room right away, with "match over", otherwise the client keeps getting
        // connection events (crate::lobby::matchmaking::leave_room)
        let me = self.clean_open_id();
        // a CANCEL (SELF_QUIT) event numbered after the last sent one — first, before the room's "match over" (there
        // CANCEL MATCHING_DESTROY will no longer go out). Before — a snapshot with last_event_id 4 without an event: the client did not
        // accept it and stayed in the search
        let uid = self.load_me().await.user_id;
        if let Some(msg) = crate::lobby::matchmaking::match_cancel_msg(&me, uid, &req.matching_id, crate::lobby::matchmaking::MATCH_EXIT_SELF_QUIT) {
            self.push(crate::lobby::hub::MATCHING_LOG_METHOD, crate::lobby::hub::MATCHING_LOG_TYPE, msg);
        }
        if let Some(battle_id) = crate::lobby::matchmaking::matched_before_battle(&me) {
            crate::rpc::handlers::room_finish_push(&me, &battle_id, None, None);
            crate::lobby::matchmaking::leave_room(&me, &battle_id);
        }
        self.lobby_matching_cancel("matching_exit");
        let open_id = self.clean_open_id();
        if let Some(mut player) = self.db.get::<PlayerModel>("players", &open_id).await {
            player.state["matching_state"] = serde_json::json!("idle");
            self.db.save("players", &open_id, &player).await;
        }

        println!("[BattleMatching] Player {} cancelled the search", open_id);

        self.push(
            "tgf.GameClientService.player_dirty_change",
            "tgf.SCPlayerDirtyChgSync",
            crate::tgf::ScPlayerDirtyChgSync {
                dirty_clear_matching: vec![req.matching_id],
                ..Default::default()
            },
        );

        Some(crate::tgf::ScBattleMatchingExitRsp {})
    }

    /// 6. Decline confirmation of a found match
    pub(super) async fn battle_matching_confirm_refuse_impl(
        &self,
        _req: crate::tgf::CsBattleMatchingConfirmRefuseReq,
    ) -> Option<crate::tgf::ScBattleMatchingConfirmRefuseRsp> {
        let open_id = self.clean_open_id();
        if let Some(mut player) = self.db.get::<PlayerModel>("players", &open_id).await {
            player.state["matching_state"] = serde_json::json!("idle");
            self.db.save("players", &open_id, &player).await;
        }
        Some(crate::tgf::ScBattleMatchingConfirmRefuseRsp {})
    }

    /// 7. Battle room heartbeat (the native client requests room events by battle_id)
    pub(super) async fn battle_room_heartbeat_impl(
        &self,
        req: crate::tgf::CsBattleRoomHeartbeatReq,
    ) -> Option<crate::tgf::ScBattleRoomHeartbeatRsp> {
        self.lobby_battle_room_heartbeat(req).await;
        Some(crate::tgf::ScBattleRoomHeartbeatRsp {})
    }

    /// 8. Player actions in the battle room (loading progress, role/map selection)
    pub(super) async fn battle_room_player_act_impl(
        &self,
        _req: crate::tgf::CsBattleRoomPlayerActReq,
    ) -> Option<crate::tgf::ScBattleRoomPlayerActRsp> {
        Some(crate::tgf::ScBattleRoomPlayerActRsp {})
    }

    /// Called by the dedicated server (DS) when the game session is created and listens on a port
    pub(super) async fn create_game_session_result_event_impl(
        &self,
        req: crate::tgf::CreateGameSessionResultEventReq,
    ) -> Option<crate::google::protobuf::Empty> {
        println!(
            "[DS] Session created: session_id={}, result={}, listen_addr={}, fleet_id={}",
            req.game_session_id, req.result, req.ds_listen_addr, req.fleet_id
        );
        Some(crate::google::protobuf::Empty {})
    }
}
