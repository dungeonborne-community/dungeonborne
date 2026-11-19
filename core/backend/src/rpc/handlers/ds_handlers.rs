//! DS service (game server ↔ backend): hello_world, init_player, heartbeat, player status/quit/kick, player_game_over (match results, experience, results screen),
//! player_online_*, ds_end (closes the matchmaker room `lobby::matchmaking::close_room` and the allocator slot;
//! game_session_id is treated as battle_id).
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Direction (docs/DS_PIPELINE.md): incoming here — only `tgf.DsBattleService` (player_game_over, ds_end,
//! player_online_*, hello_world). `tgf.DsRpcService` (init_player, heartbeat, query_player_game_status,
//! player_self_quit, player_kickout*) lives on the game server: the backend CALLS it (crate::ds::rpc_client).
//! Their "handlers" below are kept so as not to break the router; normally nothing arrives here — if something does, a warning
//! is logged. DsBattleService calls from ds-agent also arrive here via crate::ds::agent_link (/ds/event).

use super::*;

impl GatewaySession {
    /// Handling of the player list init request on the DS (tgf.DsRpcService.init_player)
    pub(super) async fn init_player_impl(
        &self,
        req: crate::tgf::DsInitPlayerListReq,
    ) -> Option<crate::tgf::DsInitPlayerListRsp> {
        println!(
            "[DS] WARNING: incoming init_player (this is a server method, the backend calls it itself — crate::ds::driver): \
             players={}, is_last_one={}, from {}",
            req.player_data.len(),
            req.is_last_one,
            self.peer_ip
        );

        let key_token = req
            .player_data
            .iter()
            .map(|p| {
                let ds_key = p.ds_player_key.clone().unwrap_or_default();
                let fallback_id = p
                    .user_key
                    .as_ref()
                    .map(|k| k.user_id.to_string())
                    .unwrap_or_default();

                crate::tgf::DsPlayerInitData {
                    ds_user_key: if !ds_key.ds_user_key.is_empty() {
                        ds_key.ds_user_key
                    } else {
                        fallback_id
                    },
                    token: if !ds_key.ds_token.is_empty() {
                        ds_key.ds_token
                    } else {
                        "LOCAL_DS_TOKEN".to_string()
                    },
                    encryption_key: ds_key.ds_encryption_key,
                }
            })
            .collect();

        Some(crate::tgf::DsInitPlayerListRsp {
            ret_code: 0,
            key_token,
        })
    }

    /// Heartbeat from the dedicated server (DS)
    pub(super) async fn heartbeat_impl(
        &self,
        _req: crate::tgf::DsHeartbeatReq,
    ) -> Option<crate::tgf::DsHeartbeatRsp> {
        // Just reply with an empty packet, confirming the Gateway is alive
        Some(crate::tgf::DsHeartbeatRsp {})
    }

    /// Request for player statuses in the current session
    pub(super) async fn query_player_game_status_impl(
        &self,
        req: crate::tgf::DsQueryPlayerGameStatusReq,
    ) -> Option<crate::tgf::DsQueryPlayerGameStatusRsp> {
        println!(
            "[DS] handle_query_player_game_status: requested keys {:?}",
            req.ds_user_keys
        );

        // Return an empty status array (or basic statuses could be built if needed)
        Some(crate::tgf::DsQueryPlayerGameStatusRsp {
            ret_code: 0,
            player_status: vec![],
        })
    }

    /// The player left the match on their own (quit via the menu)
    pub(super) async fn player_self_quit_impl(
        &self,
        req: crate::tgf::DsPlayerSelfQuitReq,
    ) -> Option<crate::tgf::DsPlayerSelfQuitRsp> {
        println!(
            "[DS] handle_player_self_quit: player {} left the match",
            req.ds_user_key
        );

        Some(crate::tgf::DsPlayerSelfQuitRsp {
            ret_code: 0,
            ds_result: None,
        })
    }

    /// DS kicks a player from the match
    pub(super) async fn player_kickout_impl(
        &self,
        req: crate::tgf::DsPlayerKickOutReq,
    ) -> Option<crate::tgf::DsPlayerKickOutRsp> {
        println!(
            "[DS] handle_player_kickout: player {} kicked (reason={})",
            req.ds_user_key, req.kick_reason
        );

        Some(crate::tgf::DsPlayerKickOutRsp {})
    }

    /// DS kicks a player (offline)
    pub(super) async fn player_kickout_off_impl(
        &self,
        req: crate::tgf::DsPlayerKickOutOffReq,
    ) -> Option<crate::tgf::DsPlayerKickOutOffRsp> {
        println!(
            "[DS] handle_player_kickout_off: player {} kicked (reason={})",
            req.ds_user_key, req.kick_reason
        );

        Some(crate::tgf::DsPlayerKickOutOffRsp {})
    }

    /// Native end of the match for a player from the DS (tgf.DsBattleService.player_game_over)
    /// Saves the extracted loot, equipment, small backpack and sends ScBattleSettleSync!
    pub(super) async fn player_game_over_impl(
        &self,
        req: crate::tgf::DsPlayerGameOverReq,
    ) -> Option<crate::tgf::DsPlayerGameOverRsp> {
        println!(
            "[DS] player_game_over: results received={}",
            req.results.len()
        );

        for res in &req.results {
            // Find the player by user_id (ds_user_key) or the current open_id
            let target_player = if let Ok(uid) = res.ds_user_key.parse::<u64>() {
                self.db.find_by_user_id::<PlayerModel>(uid).await
            } else {
                self.db.get::<PlayerModel>("players", &res.ds_user_key).await
            };

            if let Some(mut player) = target_player {
                let role_guid = if player.hall_role_guid != 0 {
                    player.hall_role_guid
                } else {
                    player.roles.first().map(|r| r.role_guid).unwrap_or(0)
                };

                if let Some(role) = player.get_role_mut(role_guid) {
                    if let Some(deploy) = &mut role.data.deploy_role {
                        if let Some(ref wearing) = res.player_wearing {
                            deploy.wearing = Some(wearing.clone());
                        }
                        if let Some(ref backpack) = res.player_backpack {
                            deploy.backpack = Some(backpack.clone());
                        }
                        if let Some(ref small_bp) = res.player_small_backpack {
                            deploy.small_backpack = Some(small_bp.clone());
                        }
                    }
                }
                // Experience from the DS (batle_info.add_exp) — with level-up
                let add_exp = res.batle_info.as_ref().map(|i| i.add_exp).unwrap_or(0);
                if let Some((old, new)) = self.progress_add_role_exp(&mut player, role_guid, add_exp) {
                    println!("[DS] {}: +{} exp, level {} → {}", player.open_id, add_exp, old, new);
                    self.raid_push_role(&player, role_guid);
                }

                let p_open_id = player.open_id.clone();
                self.db.save("players", &p_open_id, &player).await;
                println!(
                    "[DS] Saved match result for player open_id={} (user_id={})",
                    p_open_id, player.user_id
                );

                // Match results screen (ScBattleSettleSync) — to the player via the hub: this request came from the DS, self.push would go to it
                let lvl = Self::get_saved_level_select(&player);
                let (level_param, map_id) = self.build_level_and_map_params(&lvl);

                crate::lobby::hub::push_to(
                    &p_open_id,
                    "tgf.GameClientService.battle_settle_sync",
                    "tgf.SCBattleSettleSync",
                    &crate::tgf::ScBattleSettleSync {
                        result: Some(crate::tgf::DBattlePlayerGameOver {
                            user_key: Some(crate::tgf::DPlayerIdKey {
                                user_id: player.user_id,
                                zone_id: 1,
                            }),
                            role_guid: role_guid as i64,
                            map_data: Some(crate::tgf::DBattleMapData { map_id }),
                            ds_result: Some(res.clone()),
                            ..Default::default()
                        }),
                        gs_result: Some(crate::tgf::DGsUserSettleResult {
                            val_changed: true,
                            ..Default::default()
                        }),
                        level_parameter: Some(level_param),
                    },
                );
            }
        }

        Some(crate::tgf::DsPlayerGameOverRsp { ret_code: 0 })
    }

    // ---- from misc_handlers.rs (31.10) ----
    pub(super) async fn hello_world_impl(&self, req: tgf::DsHelloWorldReq) -> Option<tgf::DsHelloWorldRsp> {
        println!("[DS] hello_world: {:?}", req.text);
        Some(tgf::DsHelloWorldRsp { text: req.text })
    }

    /// The server ended the session: close the room (player statuses + releasing the server in the allocator).
    pub(super) async fn ds_end_impl(&self, req: tgf::DsDsEndReq) -> Option<tgf::DsDsEndRsp> {
        let sid = req.game_session_id.trim();
        let known = !sid.is_empty() && crate::lobby::matchmaking::room(sid).is_some();
        println!("[DS] ds_end: session={:?} end_code={} (room {})", sid, req.end_code, if known { "found" } else { "not found" });
        if !sid.is_empty() {
            crate::lobby::matchmaking::close_room(sid);
            // the room may already be gone (timeout) — release the allocator slot anyway (idempotent)
            crate::lobby::allocator::release(sid);
        }
        // TODO(verify): what the DS sends in game_session_id (the room's battle_id or its own id) — check against the DungeonborneRuServer mod
        Some(tgf::DsDsEndRsp { ret_code: 0 })
    }

    pub(super) async fn player_online_heartbeat_impl(&self, _req: tgf::DsPlayerOnlineHeartbeatReq) -> Option<tgf::DsPlayerOnlineHeartbeatRsp> {
        Some(tgf::DsPlayerOnlineHeartbeatRsp { ret_code: 0 })
    }

    pub(super) async fn player_online_report_impl(&self, req: tgf::DsPlayerOnlineReportReq) -> Option<tgf::DsPlayerOnlineReportRsp> {
        println!("[DS] player_online_report: {} online={}", req.ds_user_key, req.online);
        Some(tgf::DsPlayerOnlineReportRsp { ret_code: 0 })
    }
}
