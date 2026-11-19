#[async_trait::async_trait]
pub trait RpcHandlers: Send + Sync {
    async fn handle_hello_world(&self, _req: crate::tgf::DsHelloWorldReq) -> Option<crate::tgf::DsHelloWorldRsp> {
        Some(<crate::tgf::DsHelloWorldRsp as ::core::default::Default>::default())
    }
    async fn handle_init_player(&self, _req: crate::tgf::DsInitPlayerListReq) -> Option<crate::tgf::DsInitPlayerListRsp> {
        Some(<crate::tgf::DsInitPlayerListRsp as ::core::default::Default>::default())
    }
    async fn handle_heartbeat(&self, _req: crate::tgf::DsHeartbeatReq) -> Option<crate::tgf::DsHeartbeatRsp> {
        Some(<crate::tgf::DsHeartbeatRsp as ::core::default::Default>::default())
    }
    async fn handle_query_player_game_status(&self, _req: crate::tgf::DsQueryPlayerGameStatusReq) -> Option<crate::tgf::DsQueryPlayerGameStatusRsp> {
        Some(<crate::tgf::DsQueryPlayerGameStatusRsp as ::core::default::Default>::default())
    }
    async fn handle_player_self_quit(&self, _req: crate::tgf::DsPlayerSelfQuitReq) -> Option<crate::tgf::DsPlayerSelfQuitRsp> {
        Some(<crate::tgf::DsPlayerSelfQuitRsp as ::core::default::Default>::default())
    }
    async fn handle_player_kickout(&self, _req: crate::tgf::DsPlayerKickOutReq) -> Option<crate::tgf::DsPlayerKickOutRsp> {
        Some(<crate::tgf::DsPlayerKickOutRsp as ::core::default::Default>::default())
    }
    async fn handle_player_kickout_off(&self, _req: crate::tgf::DsPlayerKickOutOffReq) -> Option<crate::tgf::DsPlayerKickOutOffRsp> {
        Some(<crate::tgf::DsPlayerKickOutOffRsp as ::core::default::Default>::default())
    }
    async fn handle_create_game_session_result_event(&self, _req: crate::tgf::CreateGameSessionResultEventReq) -> Option<crate::google::protobuf::Empty> {
        Some(<crate::google::protobuf::Empty as ::core::default::Default>::default())
    }
    async fn handle_game_session_end_event(&self, _req: crate::tgf::GameSessionEndEventReq) -> Option<crate::google::protobuf::Empty> {
        Some(<crate::google::protobuf::Empty as ::core::default::Default>::default())
    }
    async fn handle_ds_alias_status_report(&self, _req: crate::tgf::DsAliasStatusReportReq) -> Option<crate::google::protobuf::Empty> {
        Some(<crate::google::protobuf::Empty as ::core::default::Default>::default())
    }
    async fn handle_dsc_status_report(&self, _req: crate::tgf::DscStatusReportReq) -> Option<crate::google::protobuf::Empty> {
        Some(<crate::google::protobuf::Empty as ::core::default::Default>::default())
    }
    async fn handle_player_game_over(&self, _req: crate::tgf::DsPlayerGameOverReq) -> Option<crate::tgf::DsPlayerGameOverRsp> {
        Some(<crate::tgf::DsPlayerGameOverRsp as ::core::default::Default>::default())
    }
    async fn handle_ds_end(&self, _req: crate::tgf::DsDsEndReq) -> Option<crate::tgf::DsDsEndRsp> {
        Some(<crate::tgf::DsDsEndRsp as ::core::default::Default>::default())
    }
    async fn handle_player_online_heartbeat(&self, _req: crate::tgf::DsPlayerOnlineHeartbeatReq) -> Option<crate::tgf::DsPlayerOnlineHeartbeatRsp> {
        Some(<crate::tgf::DsPlayerOnlineHeartbeatRsp as ::core::default::Default>::default())
    }
    async fn handle_player_online_report(&self, _req: crate::tgf::DsPlayerOnlineReportReq) -> Option<crate::tgf::DsPlayerOnlineReportRsp> {
        Some(<crate::tgf::DsPlayerOnlineReportRsp as ::core::default::Default>::default())
    }
    async fn handle_pre_login(&self, _req: crate::tgf::CsPreLoginReq) -> Option<crate::tgf::ScPreLoginRsp> {
        Some(<crate::tgf::ScPreLoginRsp as ::core::default::Default>::default())
    }
    async fn handle_login_notify_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScLoginNotifySync> {
        Some(<crate::tgf::ScLoginNotifySync as ::core::default::Default>::default())
    }
    async fn handle_login(&self, _req: crate::tgf::CsLoginReq) -> Option<crate::tgf::ScLoginRsp> {
        Some(<crate::tgf::ScLoginRsp as ::core::default::Default>::default())
    }
    async fn handle_client_action_log(&self, _req: crate::tgf::CsCommonLoggingReq) -> Option<crate::tgf::ScCommonLoggingRsp> {
        Some(<crate::tgf::ScCommonLoggingRsp as ::core::default::Default>::default())
    }
    async fn handle_ping(&self, _req: crate::tgf::CsPingReq) -> Option<crate::tgf::ScPongRsp> {
        Some(<crate::tgf::ScPongRsp as ::core::default::Default>::default())
    }
    async fn handle_business_ping(&self, _req: crate::tgf::CsBusinessPingReq) -> Option<crate::tgf::ScBusinessPongRsp> {
        Some(<crate::tgf::ScBusinessPongRsp as ::core::default::Default>::default())
    }
    async fn handle_network_latency_report(&self, _req: crate::tgf::CsNetworkLatencyReportReq) -> Option<crate::tgf::ScNetworkLatencyReportRsp> {
        Some(<crate::tgf::ScNetworkLatencyReportRsp as ::core::default::Default>::default())
    }
    async fn handle_network_block_loading(&self, _req: crate::tgf::CsNetworkBlockLoadingReq) -> Option<crate::tgf::ScNetworkBlockLoadingRsp> {
        Some(<crate::tgf::ScNetworkBlockLoadingRsp as ::core::default::Default>::default())
    }
    async fn handle_update_account_data(&self, _req: crate::tgf::CsUpdateAccountDataReq) -> Option<crate::tgf::ScUpdateAccountDataRsp> {
        Some(<crate::tgf::ScUpdateAccountDataRsp as ::core::default::Default>::default())
    }
    async fn handle_telemetry_report(&self, _req: crate::tgf::CsTelemetryReportReq) -> Option<crate::tgf::ScTelemetryReportRsp> {
        Some(<crate::tgf::ScTelemetryReportRsp as ::core::default::Default>::default())
    }
    async fn handle_modal_message(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScModalMessageSync> {
        Some(<crate::tgf::ScModalMessageSync as ::core::default::Default>::default())
    }
    async fn handle_player_get_info(&self, _req: crate::tgf::CsPlayerGetInfoReq) -> Option<crate::tgf::ScPlayerGetInfoRsp> {
        Some(<crate::tgf::ScPlayerGetInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_player_dirty_change(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScPlayerDirtyChgSync> {
        Some(<crate::tgf::ScPlayerDirtyChgSync as ::core::default::Default>::default())
    }
    async fn handle_player_get_info_by_page(&self, _req: crate::tgf::CsPlayerGetInfoByPageReq) -> Option<crate::tgf::ScPlayerGetInfoByPageRsp> {
        Some(<crate::tgf::ScPlayerGetInfoByPageRsp as ::core::default::Default>::default())
    }
    async fn handle_player_batch_get_basic_info(&self, _req: crate::tgf::CsPlayerBatchGetBasicInfoReq) -> Option<crate::tgf::ScPlayerBatchGetBasicInfoRsp> {
        Some(<crate::tgf::ScPlayerBatchGetBasicInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_player_batch_get_id_key(&self, _req: crate::tgf::CsPlayerBatchGetIdKeyReq) -> Option<crate::tgf::ScPlayerBatchGetIdKeyRsp> {
        Some(<crate::tgf::ScPlayerBatchGetIdKeyRsp as ::core::default::Default>::default())
    }
    async fn handle_player_rename(&self, _req: crate::tgf::CsPlayerRenameReq) -> Option<crate::tgf::ScPlayerRenameRsp> {
        Some(<crate::tgf::ScPlayerRenameRsp as ::core::default::Default>::default())
    }
    async fn handle_player_update_icon(&self, _req: crate::tgf::CsPlayerUpdateIconReq) -> Option<crate::tgf::ScPlayerUpdateIconRsp> {
        Some(<crate::tgf::ScPlayerUpdateIconRsp as ::core::default::Default>::default())
    }
    async fn handle_player_update_options(&self, _req: crate::tgf::CsPlayerUpdateOptionsReq) -> Option<crate::tgf::ScPlayerUpdateOptionsRsp> {
        Some(<crate::tgf::ScPlayerUpdateOptionsRsp as ::core::default::Default>::default())
    }
    async fn handle_player_gm_command(&self, _req: crate::tgf::CsPlayerGmCommandReq) -> Option<crate::tgf::ScPlayerGmCommandRsp> {
        Some(<crate::tgf::ScPlayerGmCommandRsp as ::core::default::Default>::default())
    }
    async fn handle_player_setting_data(&self, _req: crate::tgf::CsPlayerSettingDataReq) -> Option<crate::tgf::ScPlayerSettingDataRsp> {
        Some(<crate::tgf::ScPlayerSettingDataRsp as ::core::default::Default>::default())
    }
    async fn handle_player_kickoff_reason(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScPlayerKickoffReasonSync> {
        Some(<crate::tgf::ScPlayerKickoffReasonSync as ::core::default::Default>::default())
    }
    async fn handle_player_ace_packet(&self, _req: crate::tgf::CsPlayerAcePacketReq) -> Option<crate::tgf::ScPlayerAcePacketRsp> {
        Some(<crate::tgf::ScPlayerAcePacketRsp as ::core::default::Default>::default())
    }
    async fn handle_player_ace_packet_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScPlayerAcePacketSync> {
        Some(<crate::tgf::ScPlayerAcePacketSync as ::core::default::Default>::default())
    }
    async fn handle_event_cmd_info_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScEventCmdInfoSync> {
        Some(<crate::tgf::ScEventCmdInfoSync as ::core::default::Default>::default())
    }
    async fn handle_player_uri_redirect(&self, _req: crate::tgf::CsPlayerRedirectReq) -> Option<crate::tgf::ScPlayerRedirectRsp> {
        Some(<crate::tgf::ScPlayerRedirectRsp as ::core::default::Default>::default())
    }
    async fn handle_player_receive_homeland_reward(&self, _req: crate::tgf::CsPlayerReceiveHomelandRewardReq) -> Option<crate::tgf::ScPlayerReceiveHomelandRewardRsp> {
        Some(<crate::tgf::ScPlayerReceiveHomelandRewardRsp as ::core::default::Default>::default())
    }
    async fn handle_report_action(&self, _req: crate::tgf::CsReportActionReq) -> Option<crate::tgf::ScReportActionRsp> {
        Some(<crate::tgf::ScReportActionRsp as ::core::default::Default>::default())
    }
    async fn handle_player_birthday_update(&self, _req: crate::tgf::CsPlayerUpdateBirthDayReq) -> Option<crate::tgf::ScPlayerUpdateBirthDayRsp> {
        Some(<crate::tgf::ScPlayerUpdateBirthDayRsp as ::core::default::Default>::default())
    }
    async fn handle_cache_batch_get(&self, _req: crate::tgf::CsCacheBatchGetReq) -> Option<crate::tgf::ScCacheBatchGetRsp> {
        Some(<crate::tgf::ScCacheBatchGetRsp as ::core::default::Default>::default())
    }
    async fn handle_cache_unwatch(&self, _req: crate::tgf::CsCacheUnwatchReq) -> Option<crate::tgf::ScCacheUnwatchRsp> {
        Some(<crate::tgf::ScCacheUnwatchRsp as ::core::default::Default>::default())
    }
    async fn handle_cache_expired(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScCacheExpiredSync> {
        Some(<crate::tgf::ScCacheExpiredSync as ::core::default::Default>::default())
    }
    async fn handle_cache_update_meta(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScCacheUpdateMetaSync> {
        Some(<crate::tgf::ScCacheUpdateMetaSync as ::core::default::Default>::default())
    }
    async fn handle_player_all_cache_expired(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScAllCacheExpiredSync> {
        Some(<crate::tgf::ScAllCacheExpiredSync as ::core::default::Default>::default())
    }
    async fn handle_mail_get_all(&self, _req: crate::tgf::CsMailGetAllReq) -> Option<crate::tgf::ScMailGetAllRsp> {
        Some(<crate::tgf::ScMailGetAllRsp as ::core::default::Default>::default())
    }
    async fn handle_mail_read(&self, _req: crate::tgf::CsMailReadReq) -> Option<crate::tgf::ScMailReadRsp> {
        Some(<crate::tgf::ScMailReadRsp as ::core::default::Default>::default())
    }
    async fn handle_mail_read_all(&self, _req: crate::tgf::CsMailReadAllReq) -> Option<crate::tgf::ScMailReadAllRsp> {
        Some(<crate::tgf::ScMailReadAllRsp as ::core::default::Default>::default())
    }
    async fn handle_mail_receive_attachment(&self, _req: crate::tgf::CsMailReceiveAttachmentsReq) -> Option<crate::tgf::ScMailReceiveAttachmentsRsp> {
        Some(<crate::tgf::ScMailReceiveAttachmentsRsp as ::core::default::Default>::default())
    }
    async fn handle_mail_receive_attachment_all(&self, _req: crate::tgf::CsMailReceiveAttachmentsAllReq) -> Option<crate::tgf::ScMailReceiveAttachmentsAllRsp> {
        Some(<crate::tgf::ScMailReceiveAttachmentsAllRsp as ::core::default::Default>::default())
    }
    async fn handle_mail_change_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScMailChangeSync> {
        Some(<crate::tgf::ScMailChangeSync as ::core::default::Default>::default())
    }
    async fn handle_mail_receive_attachment_partly(&self, _req: crate::tgf::CsMailReceiveAttachmentsPartlyReq) -> Option<crate::tgf::ScMailReceiveAttachmentsPartlyRsp> {
        Some(<crate::tgf::ScMailReceiveAttachmentsPartlyRsp as ::core::default::Default>::default())
    }
    async fn handle_hall_set_role(&self, _req: crate::tgf::CsHallSetRoleReq) -> Option<crate::tgf::ScHallSetRoleRsp> {
        Some(<crate::tgf::ScHallSetRoleRsp as ::core::default::Default>::default())
    }
    async fn handle_hall_switch(&self, _req: crate::tgf::CsHallSwitchReq) -> Option<crate::tgf::ScHallSwitchRsp> {
        Some(<crate::tgf::ScHallSwitchRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_pre_search(&self, _req: crate::tgf::CsBattleMatchingPreSearchReq) -> Option<crate::tgf::ScBattleMatchingPreSearchRsp> {
        Some(<crate::tgf::ScBattleMatchingPreSearchRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_cancel_pre_search(&self, _req: crate::tgf::CsBattleMatchingCancelPreSearchReq) -> Option<crate::tgf::ScBattleMatchingCancelPreSearchRsp> {
        Some(<crate::tgf::ScBattleMatchingCancelPreSearchRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_search(&self, _req: crate::tgf::CsBattleMatchingSearchReq) -> Option<crate::tgf::ScBattleMatchingSearchRsp> {
        Some(<crate::tgf::ScBattleMatchingSearchRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_heartbeat(&self, _req: crate::tgf::CsBattleMatchingHeartbeatReq) -> Option<crate::tgf::ScBattleMatchingHeartbeatRsp> {
        Some(<crate::tgf::ScBattleMatchingHeartbeatRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_exit(&self, _req: crate::tgf::CsBattleMatchingExitReq) -> Option<crate::tgf::ScBattleMatchingExitRsp> {
        Some(<crate::tgf::ScBattleMatchingExitRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_confirm_refuse(&self, _req: crate::tgf::CsBattleMatchingConfirmRefuseReq) -> Option<crate::tgf::ScBattleMatchingConfirmRefuseRsp> {
        Some(<crate::tgf::ScBattleMatchingConfirmRefuseRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_matching_log_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScBattleMatchingLogSync> {
        Some(<crate::tgf::ScBattleMatchingLogSync as ::core::default::Default>::default())
    }
    async fn handle_battle_room_log_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScBattleRoomLogSync> {
        Some(<crate::tgf::ScBattleRoomLogSync as ::core::default::Default>::default())
    }
    async fn handle_battle_room_heartbeat(&self, _req: crate::tgf::CsBattleRoomHeartbeatReq) -> Option<crate::tgf::ScBattleRoomHeartbeatRsp> {
        Some(<crate::tgf::ScBattleRoomHeartbeatRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_record_list_get(&self, _req: crate::tgf::CsBattleRecordListGetReq) -> Option<crate::tgf::ScBattleRecordListGetRsp> {
        Some(<crate::tgf::ScBattleRecordListGetRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_record_data_get(&self, _req: crate::tgf::CsBattleRecordDataGetReq) -> Option<crate::tgf::ScBattleRecordDataGetRsp> {
        Some(<crate::tgf::ScBattleRecordDataGetRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_record_data_set(&self, _req: crate::tgf::CsBattleRecordDataSetReq) -> Option<crate::tgf::ScBattleRecordDataSetRsp> {
        Some(<crate::tgf::ScBattleRecordDataSetRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_room_player_act(&self, _req: crate::tgf::CsBattleRoomPlayerActReq) -> Option<crate::tgf::ScBattleRoomPlayerActRsp> {
        Some(<crate::tgf::ScBattleRoomPlayerActRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_update_setting(&self, _req: crate::tgf::CsBattleUpdateSettingReq) -> Option<crate::tgf::ScBattleRoomPlayerActRsp> {
        Some(<crate::tgf::ScBattleRoomPlayerActRsp as ::core::default::Default>::default())
    }
    async fn handle_battle_settle_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScBattleSettleSync> {
        Some(<crate::tgf::ScBattleSettleSync as ::core::default::Default>::default())
    }
    async fn handle_battle_fetch_battle_user_data(&self, _req: crate::tgf::CsBattleFetchBattleUserReq) -> Option<crate::tgf::ScBattleFetchBattleUserRsp> {
        Some(<crate::tgf::ScBattleFetchBattleUserRsp as ::core::default::Default>::default())
    }
    async fn handle_level_prestart(&self, _req: crate::tgf::CsLevelPrestartReq) -> Option<crate::tgf::ScLevelPrestartRsp> {
        Some(<crate::tgf::ScLevelPrestartRsp as ::core::default::Default>::default())
    }
    async fn handle_level_start(&self, _req: crate::tgf::CsLevelStartReq) -> Option<crate::tgf::ScLevelStartRsp> {
        Some(<crate::tgf::ScLevelStartRsp as ::core::default::Default>::default())
    }
    async fn handle_level_finish(&self, _req: crate::tgf::CsLevelFinishReq) -> Option<crate::tgf::ScLevelFinishRsp> {
        Some(<crate::tgf::ScLevelFinishRsp as ::core::default::Default>::default())
    }
    async fn handle_level_select(&self, _req: crate::tgf::CsLevelCompetitionSelectReq) -> Option<crate::tgf::ScLevelCompetitionSelectRsp> {
        Some(<crate::tgf::ScLevelCompetitionSelectRsp as ::core::default::Default>::default())
    }
    async fn handle_level_challenge(&self, _req: crate::tgf::CsLevelChallengeReq) -> Option<crate::tgf::ScLevelChallengeRsp> {
        Some(<crate::tgf::ScLevelChallengeRsp as ::core::default::Default>::default())
    }
    async fn handle_map_move(&self, _req: crate::tgf::CsMapMoveReq) -> Option<crate::tgf::ScMapMoveRsp> {
        Some(<crate::tgf::ScMapMoveRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_get_all(&self, _req: crate::tgf::CsFriendGetAllReq) -> Option<crate::tgf::ScFriendGetAllRsp> {
        Some(<crate::tgf::ScFriendGetAllRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_invite(&self, _req: crate::tgf::CsFriendInviteReq) -> Option<crate::tgf::ScFriendInviteRsp> {
        Some(<crate::tgf::ScFriendInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_accept_invite(&self, _req: crate::tgf::CsFriendAcceptInviteReq) -> Option<crate::tgf::ScFriendAcceptInviteRsp> {
        Some(<crate::tgf::ScFriendAcceptInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_reject_invite(&self, _req: crate::tgf::CsFriendRejectInviteReq) -> Option<crate::tgf::ScFriendRejectInviteRsp> {
        Some(<crate::tgf::ScFriendRejectInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_remove(&self, _req: crate::tgf::CsFriendRemoveReq) -> Option<crate::tgf::ScFriendRemoveRsp> {
        Some(<crate::tgf::ScFriendRemoveRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_send_gift(&self, _req: crate::tgf::CsFriendSendGiftReq) -> Option<crate::tgf::ScFriendSendGiftRsp> {
        Some(<crate::tgf::ScFriendSendGiftRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_receive_gift(&self, _req: crate::tgf::CsFriendReceiveGiftReq) -> Option<crate::tgf::ScFriendReceiveGiftRsp> {
        Some(<crate::tgf::ScFriendReceiveGiftRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_get_suggest(&self, _req: crate::tgf::CsFriendGetSuggestReq) -> Option<crate::tgf::ScFriendGetSuggestRsp> {
        Some(<crate::tgf::ScFriendGetSuggestRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_accept_all_invite(&self, _req: crate::tgf::CsFriendAcceptAllInviteReq) -> Option<crate::tgf::ScFriendAcceptAllInviteRsp> {
        Some(<crate::tgf::ScFriendAcceptAllInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_friend_reject_all_invite(&self, _req: crate::tgf::CsFriendRejectAllInviteReq) -> Option<crate::tgf::ScFriendRejectAllInviteRsp> {
        Some(<crate::tgf::ScFriendRejectAllInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_create(&self, _req: crate::tgf::CsGuildCreateReq) -> Option<crate::tgf::ScGuildCreateRsp> {
        Some(<crate::tgf::ScGuildCreateRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_exit(&self, _req: crate::tgf::CsGuildExitReq) -> Option<crate::tgf::ScGuildExitRsp> {
        Some(<crate::tgf::ScGuildExitRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_pay_off(&self, _req: crate::tgf::CsGuildPayOffReq) -> Option<crate::tgf::ScGuildPayOffRsp> {
        Some(<crate::tgf::ScGuildPayOffRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_get_all(&self, _req: crate::tgf::CsGuildGetAllReq) -> Option<crate::tgf::ScGuildGetAllRsp> {
        Some(<crate::tgf::ScGuildGetAllRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_batch_get(&self, _req: crate::tgf::CsGuildBatchGetReq) -> Option<crate::tgf::ScGuildBatchGetRsp> {
        Some(<crate::tgf::ScGuildBatchGetRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_suggest(&self, _req: crate::tgf::CsGuildSuggestReq) -> Option<crate::tgf::ScGuildSuggestRsp> {
        Some(<crate::tgf::ScGuildSuggestRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_set_role(&self, _req: crate::tgf::CsGuildSetRoleReq) -> Option<crate::tgf::ScGuildSetRoleRsp> {
        Some(<crate::tgf::ScGuildSetRoleRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_rename(&self, _req: crate::tgf::CsGuildRenameReq) -> Option<crate::tgf::ScGuildRenameRsp> {
        Some(<crate::tgf::ScGuildRenameRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_set_configure(&self, _req: crate::tgf::CsGuildSetConfigureReq) -> Option<crate::tgf::ScGuildSetConfigureRsp> {
        Some(<crate::tgf::ScGuildSetConfigureRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_send_apply(&self, _req: crate::tgf::CsGuildSendApplyReq) -> Option<crate::tgf::ScGuildSendApplyRsp> {
        Some(<crate::tgf::ScGuildSendApplyRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_approve_apply(&self, _req: crate::tgf::CsGuildApproveApplyReq) -> Option<crate::tgf::ScGuildApproveApplyRsp> {
        Some(<crate::tgf::ScGuildApproveApplyRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_reject_apply(&self, _req: crate::tgf::CsGuildRejectApplyReq) -> Option<crate::tgf::ScGuildRejectApplyRsp> {
        Some(<crate::tgf::ScGuildRejectApplyRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_send_invitation(&self, _req: crate::tgf::CsGuildSendInvitationReq) -> Option<crate::tgf::ScGuildSendInvitationRsp> {
        Some(<crate::tgf::ScGuildSendInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_approve_invitation(&self, _req: crate::tgf::CsGuildApproveInvitationReq) -> Option<crate::tgf::ScGuildApproveInvitationRsp> {
        Some(<crate::tgf::ScGuildApproveInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_reject_invitation(&self, _req: crate::tgf::CsGuildRejectInvitationReq) -> Option<crate::tgf::ScGuildRejectInvitationRsp> {
        Some(<crate::tgf::ScGuildRejectInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_revoke_invitation(&self, _req: crate::tgf::CsGuildRevokeInvitationReq) -> Option<crate::tgf::ScGuildRevokeInvitationRsp> {
        Some(<crate::tgf::ScGuildRevokeInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_remove_member(&self, _req: crate::tgf::CsGuildRemoveMemberReq) -> Option<crate::tgf::ScGuildRemoveMemberRsp> {
        Some(<crate::tgf::ScGuildRemoveMemberRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_add_impeachment(&self, _req: crate::tgf::CsGuildAddImpeachmentReq) -> Option<crate::tgf::ScGuildAddImpeachmentRsp> {
        Some(<crate::tgf::ScGuildAddImpeachmentRsp as ::core::default::Default>::default())
    }
    async fn handle_guild_vote_impeachment(&self, _req: crate::tgf::CsGuildVoteImpeachmentReq) -> Option<crate::tgf::ScGuildVoteImpeachmentRsp> {
        Some(<crate::tgf::ScGuildVoteImpeachmentRsp as ::core::default::Default>::default())
    }
    async fn handle_chat_get_all_channels(&self, _req: crate::tgf::CsChatGetAllChannelsReq) -> Option<crate::tgf::ScChatGetAllChannelsRsp> {
        Some(<crate::tgf::ScChatGetAllChannelsRsp as ::core::default::Default>::default())
    }
    async fn handle_chat_get_channel_snapshot(&self, _req: crate::tgf::CsChatGetChannelSnapshotReq) -> Option<crate::tgf::ScChatGetChannelSnapshotRsp> {
        Some(<crate::tgf::ScChatGetChannelSnapshotRsp as ::core::default::Default>::default())
    }
    async fn handle_chat_channel_heartbeat(&self, _req: crate::tgf::CsChatChannelHeartbeatReq) -> Option<crate::tgf::ScChatChannelHeartbeatRsp> {
        Some(<crate::tgf::ScChatChannelHeartbeatRsp as ::core::default::Default>::default())
    }
    async fn handle_chat_send_message(&self, _req: crate::tgf::CsChatSendMessageReq) -> Option<crate::tgf::ScChatSendMessageRsp> {
        Some(<crate::tgf::ScChatSendMessageRsp as ::core::default::Default>::default())
    }
    async fn handle_chat_update_panel(&self, _req: crate::tgf::CsChatUpdatePanelReq) -> Option<crate::tgf::ScChatUpdatePanelRsp> {
        Some(<crate::tgf::ScChatUpdatePanelRsp as ::core::default::Default>::default())
    }
    async fn handle_team_send_invitation(&self, _req: crate::tgf::CsTeamSendInvitationReq) -> Option<crate::tgf::ScTeamSendInvitationRsp> {
        Some(<crate::tgf::ScTeamSendInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_team_approve_invitation(&self, _req: crate::tgf::CsTeamApproveInvitationReq) -> Option<crate::tgf::ScTeamApproveInvitationRsp> {
        Some(<crate::tgf::ScTeamApproveInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_team_reject_invitation(&self, _req: crate::tgf::CsTeamRejectInvitationReq) -> Option<crate::tgf::ScTeamRejectInvitationRsp> {
        Some(<crate::tgf::ScTeamRejectInvitationRsp as ::core::default::Default>::default())
    }
    async fn handle_team_exit(&self, _req: crate::tgf::CsTeamExitReq) -> Option<crate::tgf::ScTeamExitRsp> {
        Some(<crate::tgf::ScTeamExitRsp as ::core::default::Default>::default())
    }
    async fn handle_team_remove_member(&self, _req: crate::tgf::CsTeamRemoveMemberReq) -> Option<crate::tgf::ScTeamRemoveMemberRsp> {
        Some(<crate::tgf::ScTeamRemoveMemberRsp as ::core::default::Default>::default())
    }
    async fn handle_team_send_battle_confirm(&self, _req: crate::tgf::CsTeamSendBattleConfirmReq) -> Option<crate::tgf::ScTeamSendBattleConfirmRsp> {
        Some(<crate::tgf::ScTeamSendBattleConfirmRsp as ::core::default::Default>::default())
    }
    async fn handle_team_send_battle_unconfirm(&self, _req: crate::tgf::CsTeamSendBattleUnconfirmReq) -> Option<crate::tgf::ScTeamSendBattleUnconfirmRsp> {
        Some(<crate::tgf::ScTeamSendBattleUnconfirmRsp as ::core::default::Default>::default())
    }
    async fn handle_team_transfer_captain(&self, _req: crate::tgf::CsTeamTransferCaptainReq) -> Option<crate::tgf::ScTeamTransferCaptainRsp> {
        Some(<crate::tgf::ScTeamTransferCaptainRsp as ::core::default::Default>::default())
    }
    async fn handle_team_apply_join_team(&self, _req: crate::tgf::CsTeamApplyJoinTeamReq) -> Option<crate::tgf::ScTeamApplyJoinTeamRsp> {
        Some(<crate::tgf::ScTeamApplyJoinTeamRsp as ::core::default::Default>::default())
    }
    async fn handle_team_accept_apply(&self, _req: crate::tgf::CsTeamAcceptApplyReq) -> Option<crate::tgf::ScTeamAcceptApplyRsp> {
        Some(<crate::tgf::ScTeamAcceptApplyRsp as ::core::default::Default>::default())
    }
    async fn handle_team_change_team_finder(&self, _req: crate::tgf::CsTeamChangeTeamFinderReq) -> Option<crate::tgf::ScTeamChangeTeamFinderRsp> {
        Some(<crate::tgf::ScTeamChangeTeamFinderRsp as ::core::default::Default>::default())
    }
    async fn handle_team_reject_apply(&self, _req: crate::tgf::CsTeamRejectApplyReq) -> Option<crate::tgf::ScTeamRejectApplyRsp> {
        Some(<crate::tgf::ScTeamRejectApplyRsp as ::core::default::Default>::default())
    }
    async fn handle_team_match_search(&self, _req: crate::tgf::CsTeamSearchReq) -> Option<crate::tgf::ScTeamSearchRsp> {
        Some(<crate::tgf::ScTeamSearchRsp as ::core::default::Default>::default())
    }
    async fn handle_team_match_check(&self, _req: crate::tgf::CsTeamCheckReq) -> Option<crate::tgf::ScTeamCheckRsp> {
        Some(<crate::tgf::ScTeamCheckRsp as ::core::default::Default>::default())
    }
    async fn handle_team_match_exit(&self, _req: crate::tgf::CsTeamMatchExitReq) -> Option<crate::tgf::ScTeamMatchExitRsp> {
        Some(<crate::tgf::ScTeamMatchExitRsp as ::core::default::Default>::default())
    }
    async fn handle_role_create(&self, _req: crate::tgf::CsRoleCreateReq) -> Option<crate::tgf::ScRoleCreateRsp> {
        Some(<crate::tgf::ScRoleCreateRsp as ::core::default::Default>::default())
    }
    async fn handle_role_delete(&self, _req: crate::tgf::CsRoleDeleteReq) -> Option<crate::tgf::ScRoleDeleteRsp> {
        Some(<crate::tgf::ScRoleDeleteRsp as ::core::default::Default>::default())
    }
    async fn handle_role_delete_lazy(&self, _req: crate::tgf::CsRoleDeleteLazyReq) -> Option<crate::tgf::ScRoleDeleteLazyRsp> {
        Some(<crate::tgf::ScRoleDeleteLazyRsp as ::core::default::Default>::default())
    }
    async fn handle_role_cancel_lazy_delete(&self, _req: crate::tgf::CsRoleCancelLazyDeleteReq) -> Option<crate::tgf::ScRoleCancelLazyDeleteRsp> {
        Some(<crate::tgf::ScRoleCancelLazyDeleteRsp as ::core::default::Default>::default())
    }
    async fn handle_role_wear(&self, _req: crate::tgf::CsRoleWearReq) -> Option<crate::tgf::ScRoleWearRsp> {
        Some(<crate::tgf::ScRoleWearRsp as ::core::default::Default>::default())
    }
    async fn handle_role_weapon_set_swap(&self, _req: crate::tgf::CsRoleWeaponSetSwapReq) -> Option<crate::tgf::ScRoleWeaponSetSwapRsp> {
        Some(<crate::tgf::ScRoleWeaponSetSwapRsp as ::core::default::Default>::default())
    }
    async fn handle_role_take_off(&self, _req: crate::tgf::CsRoleTakeOffReq) -> Option<crate::tgf::ScRoleTakeOffRsp> {
        Some(<crate::tgf::ScRoleTakeOffRsp as ::core::default::Default>::default())
    }
    async fn handle_role_fetch_user_role(&self, _req: crate::tgf::CsRoleFetchUserRoleReq) -> Option<crate::tgf::ScRoleFetchUserRoleRsp> {
        Some(<crate::tgf::ScRoleFetchUserRoleRsp as ::core::default::Default>::default())
    }
    async fn handle_role_move_season(&self, _req: crate::tgf::CsRoleMoveSeasonReq) -> Option<crate::tgf::ScRoleMoveSeasonRsp> {
        Some(<crate::tgf::ScRoleMoveSeasonRsp as ::core::default::Default>::default())
    }
    async fn handle_role_reshape_face_pinching(&self, _req: crate::tgf::CsRoleReshapeFacePinchingReq) -> Option<crate::tgf::ScRoleReshapeFacePinchingRsp> {
        Some(<crate::tgf::ScRoleReshapeFacePinchingRsp as ::core::default::Default>::default())
    }
    async fn handle_role_select_skill(&self, _req: crate::tgf::CsRoleSelectSkillReq) -> Option<crate::tgf::ScRoleSelectSkillRsp> {
        Some(<crate::tgf::ScRoleSelectSkillRsp as ::core::default::Default>::default())
    }
    async fn handle_quest_rewards_receive(&self, _req: crate::tgf::CsQuestRewardsReceiveReq) -> Option<crate::tgf::ScQuestRewardsReceiveRsp> {
        Some(<crate::tgf::ScQuestRewardsReceiveRsp as ::core::default::Default>::default())
    }
    async fn handle_quest_special_progress_report(&self, _req: crate::tgf::CsQuestSpecialProgressReportReq) -> Option<crate::tgf::ScQuestSpecialProgressReportRsp> {
        Some(<crate::tgf::ScQuestSpecialProgressReportRsp as ::core::default::Default>::default())
    }
    async fn handle_quest_item_submit(&self, _req: crate::tgf::CsQuestItemSubmitReq) -> Option<crate::tgf::ScQuestItemSubmitRsp> {
        Some(<crate::tgf::ScQuestItemSubmitRsp as ::core::default::Default>::default())
    }
    async fn handle_rank_get_specify_rank(&self, _req: crate::tgf::CsRankGetSpecifyRankReq) -> Option<crate::tgf::ScRankGetSpecifyRankRsp> {
        Some(<crate::tgf::ScRankGetSpecifyRankRsp as ::core::default::Default>::default())
    }
    async fn handle_rank_get_self_rank(&self, _req: crate::tgf::CsRankGetSelfRankReq) -> Option<crate::tgf::ScRankGetSelfRankRsp> {
        Some(<crate::tgf::ScRankGetSelfRankRsp as ::core::default::Default>::default())
    }
    async fn handle_rank_cache_expired_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScRankCacheExpiredSync> {
        Some(<crate::tgf::ScRankCacheExpiredSync as ::core::default::Default>::default())
    }
    async fn handle_lottery_draw(&self, _req: crate::tgf::CsLotteryDrawReq) -> Option<crate::tgf::ScLotteryDrawRsp> {
        Some(<crate::tgf::ScLotteryDrawRsp as ::core::default::Default>::default())
    }
    async fn handle_market_get_product_list(&self, _req: crate::tgf::CsMarketGetProductListReq) -> Option<crate::tgf::ScMarketGetProductListRsp> {
        Some(<crate::tgf::ScMarketGetProductListRsp as ::core::default::Default>::default())
    }
    async fn handle_market_buy_product(&self, _req: crate::tgf::CsMarketBuyProductReq) -> Option<crate::tgf::ScMarketBuyProductRsp> {
        Some(<crate::tgf::ScMarketBuyProductRsp as ::core::default::Default>::default())
    }
    async fn handle_market_sell_items(&self, _req: crate::tgf::CsMarketSellItemsReq) -> Option<crate::tgf::ScMarketSellItemsRsp> {
        Some(<crate::tgf::ScMarketSellItemsRsp as ::core::default::Default>::default())
    }
    async fn handle_market_buy_sold_item(&self, _req: crate::tgf::CsMarketBuySoldItemReq) -> Option<crate::tgf::ScMarketBuySoldItemRsp> {
        Some(<crate::tgf::ScMarketBuySoldItemRsp as ::core::default::Default>::default())
    }
    async fn handle_market_reset_refresh_effect(&self, _req: crate::tgf::CsMarketResetRefreshEffectReq) -> Option<crate::tgf::ScMarketResetRefreshEffectRsp> {
        Some(<crate::tgf::ScMarketResetRefreshEffectRsp as ::core::default::Default>::default())
    }
    async fn handle_market_video_popup(&self, _req: crate::tgf::CsMarketVideoPopUpReq) -> Option<crate::tgf::ScMarketVideoPopUpRsp> {
        Some(<crate::tgf::ScMarketVideoPopUpRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_get_all(&self, _req: crate::tgf::CsPayGetAllReq) -> Option<crate::tgf::ScPayGetAllRsp> {
        Some(<crate::tgf::ScPayGetAllRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_update_diamond(&self, _req: crate::tgf::CsPayUpdateDiamondReq) -> Option<crate::tgf::ScPayUpdateDiamondRsp> {
        Some(<crate::tgf::ScPayUpdateDiamondRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_direct_buy(&self, _req: crate::tgf::CsPayDirectBuyProductReq) -> Option<crate::tgf::ScPayDirectBuyProductRsp> {
        Some(<crate::tgf::ScPayDirectBuyProductRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_auth_data_expired_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScPayAuthDataExpiredSync> {
        Some(<crate::tgf::ScPayAuthDataExpiredSync as ::core::default::Default>::default())
    }
    async fn handle_pay_check_bill_applied(&self, _req: crate::tgf::CsPayCheckBillAppliedReq) -> Option<crate::tgf::ScPayCheckBillAppliedRsp> {
        Some(<crate::tgf::ScPayCheckBillAppliedRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_update_authorization_data(&self, _req: crate::tgf::CsPayUpdateAuthorizationDataReq) -> Option<crate::tgf::ScPayUpdateAuthorizationDataRsp> {
        Some(<crate::tgf::ScPayUpdateAuthorizationDataRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_cancel(&self, _req: crate::tgf::CsPayCancelReq) -> Option<crate::tgf::ScPayCancelRsp> {
        Some(<crate::tgf::ScPayCancelRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_exchange_free_diamond(&self, _req: crate::tgf::CsPayExchangeFreeDiamondReq) -> Option<crate::tgf::ScPayExchangeFreeDiamondRsp> {
        Some(<crate::tgf::ScPayExchangeFreeDiamondRsp as ::core::default::Default>::default())
    }
    async fn handle_pay_pending_flow_ntf(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScPayPendingFlowNtf> {
        Some(<crate::tgf::ScPayPendingFlowNtf as ::core::default::Default>::default())
    }
    async fn handle_pay_submit_pending_flow(&self, _req: crate::tgf::CsPaySubmitPendingFlowReq) -> Option<crate::tgf::ScPaySubmitPendingFlowRsp> {
        Some(<crate::tgf::ScPaySubmitPendingFlowRsp as ::core::default::Default>::default())
    }
    async fn handle_inventory_item_move(&self, _req: crate::tgf::CsInventoryItemMoveReq) -> Option<crate::tgf::ScInventoryItemMoveRsp> {
        Some(<crate::tgf::ScInventoryItemMoveRsp as ::core::default::Default>::default())
    }
    async fn handle_inventory_item_swap(&self, _req: crate::tgf::CsInventoryItemSwapReq) -> Option<crate::tgf::ScInventoryItemSwapRsp> {
        Some(<crate::tgf::ScInventoryItemSwapRsp as ::core::default::Default>::default())
    }
    async fn handle_inventory_page_add(&self, _req: crate::tgf::CsInventoryPageAddReq) -> Option<crate::tgf::ScInventoryPageAddRsp> {
        Some(<crate::tgf::ScInventoryPageAddRsp as ::core::default::Default>::default())
    }
    async fn handle_use_item(&self, _req: crate::tgf::CsItemUseReq) -> Option<crate::tgf::ScItemUseRsp> {
        Some(<crate::tgf::ScItemUseRsp as ::core::default::Default>::default())
    }
    async fn handle_use_item_bath(&self, _req: crate::tgf::CsBatchItemUseReq) -> Option<crate::tgf::ScBatchItemUseRsp> {
        Some(<crate::tgf::ScBatchItemUseRsp as ::core::default::Default>::default())
    }
    async fn handle_crafting(&self, _req: crate::tgf::CsCraftingReq) -> Option<crate::tgf::ScCraftingRsp> {
        Some(<crate::tgf::ScCraftingRsp as ::core::default::Default>::default())
    }
    async fn handle_crafting_cancel(&self, _req: crate::tgf::CsCraftingCancelReq) -> Option<crate::tgf::ScCraftingCancelRsp> {
        Some(<crate::tgf::ScCraftingCancelRsp as ::core::default::Default>::default())
    }
    async fn handle_crafting_recived(&self, _req: crate::tgf::CsCraftingRecivedReq) -> Option<crate::tgf::ScCraftingRecivedRsp> {
        Some(<crate::tgf::ScCraftingRecivedRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_product_price_basic_info(&self, _req: crate::tgf::CsTradeProductPriceBasicInfoReq) -> Option<crate::tgf::ScTradeProductPriceBasicInfoRsp> {
        Some(<crate::tgf::ScTradeProductPriceBasicInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_product_price_info(&self, _req: crate::tgf::CsTradeProductPriceInfoReq) -> Option<crate::tgf::ScTradeProductPriceInfoRsp> {
        Some(<crate::tgf::ScTradeProductPriceInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_product_percentage_info(&self, _req: crate::tgf::CsTradeProductPercentageInfoReq) -> Option<crate::tgf::ScTradeProductPercentageInfoRsp> {
        Some(<crate::tgf::ScTradeProductPercentageInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_purchase_order_publish(&self, _req: crate::tgf::CsTradePurchaseOrderPublishReq) -> Option<crate::tgf::ScTradePurchaseOrderPublishRsp> {
        Some(<crate::tgf::ScTradePurchaseOrderPublishRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_sales_order_publish(&self, _req: crate::tgf::CsTradeSalesOrderPublishReq) -> Option<crate::tgf::ScTradeSalesOrderPublishRsp> {
        Some(<crate::tgf::ScTradeSalesOrderPublishRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_bid_order_publish(&self, _req: crate::tgf::CsTradeBidOrderPublishReq) -> Option<crate::tgf::ScTradeBidOrderPublishRsp> {
        Some(<crate::tgf::ScTradeBidOrderPublishRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_auction_order_publish(&self, _req: crate::tgf::CsTradeAuctionOrderPublishReq) -> Option<crate::tgf::ScTradeAuctionOrderPublishRsp> {
        Some(<crate::tgf::ScTradeAuctionOrderPublishRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_auction_fetch_order_info(&self, _req: crate::tgf::CsTradeAuctionFetchOrderInfoReq) -> Option<crate::tgf::ScTradeAuctionFetchOrderInfoRsp> {
        Some(<crate::tgf::ScTradeAuctionFetchOrderInfoRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_auction_index(&self, _req: crate::tgf::CsTradeAuctionIndexReq) -> Option<crate::tgf::ScTradeAuctionIndexRsp> {
        Some(<crate::tgf::ScTradeAuctionIndexRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_order_cancel(&self, _req: crate::tgf::CsTradeOrderCancelReq) -> Option<crate::tgf::ScTradeOrderCancelRsp> {
        Some(<crate::tgf::ScTradeOrderCancelRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_order_history(&self, _req: crate::tgf::CsTradeOrderHistoryReq) -> Option<crate::tgf::ScTradeOrderHistoryRsp> {
        Some(<crate::tgf::ScTradeOrderHistoryRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_order_delete(&self, _req: crate::tgf::CsTradeOrderDeleteReq) -> Option<crate::tgf::ScTradeOrderDeleteRsp> {
        Some(<crate::tgf::ScTradeOrderDeleteRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_receive_confirmation_product(&self, _req: crate::tgf::CsTradeReceiveConfirmationProductReq) -> Option<crate::tgf::ScTradeReceiveConfirmationProductRsp> {
        Some(<crate::tgf::ScTradeReceiveConfirmationProductRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_auction_recommend_order_ticket(&self, _req: crate::tgf::CsTradeAuctionRecommendOrderTicketReq) -> Option<crate::tgf::ScTradeAuctionRecommendOrderTicketRsp> {
        Some(<crate::tgf::ScTradeAuctionRecommendOrderTicketRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_product_recommend_order_ticket(&self, _req: crate::tgf::CsTradeProductRecommendOrderTicketReq) -> Option<crate::tgf::ScTradeProductRecommendOrderTicketRsp> {
        Some(<crate::tgf::ScTradeProductRecommendOrderTicketRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_product_search_order_ticket(&self, _req: crate::tgf::CsTradeProductSearchOrderTicketReq) -> Option<crate::tgf::ScTradeProductSearchOrderTicketRsp> {
        Some(<crate::tgf::ScTradeProductSearchOrderTicketRsp as ::core::default::Default>::default())
    }
    async fn handle_trade_career_rarity_affix_gear_set_price(&self, _req: crate::tgf::CsTradeCareerRarityAffixGearsetPriceReq) -> Option<crate::tgf::ScTradeCareerRarityAffixGearsetPriceRsp> {
        Some(<crate::tgf::ScTradeCareerRarityAffixGearsetPriceRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_repair(&self, _req: crate::tgf::CsGearRepairReq) -> Option<crate::tgf::ScGearRepairRsp> {
        Some(<crate::tgf::ScGearRepairRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_inlaid(&self, _req: crate::tgf::CsGearInlaidReq) -> Option<crate::tgf::ScGearInlaidRsp> {
        Some(<crate::tgf::ScGearInlaidRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_dismantle(&self, _req: crate::tgf::CsGearDismantleReq) -> Option<crate::tgf::ScGearDismantleRsp> {
        Some(<crate::tgf::ScGearDismantleRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_soul_bound(&self, _req: crate::tgf::CsGearSoulBoundReq) -> Option<crate::tgf::ScGearSoulBoundRsp> {
        Some(<crate::tgf::ScGearSoulBoundRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_heirloom_unlock(&self, _req: crate::tgf::CsGearHeirloomUnlockReq) -> Option<crate::tgf::ScGearHeirloomUnlockRsp> {
        Some(<crate::tgf::ScGearHeirloomUnlockRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_heirloom_unload(&self, _req: crate::tgf::CsGearHeirloomUnloadReq) -> Option<crate::tgf::ScGearHeirloomUnloadRsp> {
        Some(<crate::tgf::ScGearHeirloomUnloadRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_heirloom_instantiation(&self, _req: crate::tgf::CsGearHeirloomInstantiationReq) -> Option<crate::tgf::ScGearHeirloomInstantiationRsp> {
        Some(<crate::tgf::ScGearHeirloomInstantiationRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_gem_unlock(&self, _req: crate::tgf::CsGearGemUnlockReq) -> Option<crate::tgf::ScGearGemUnlockRsp> {
        Some(<crate::tgf::ScGearGemUnlockRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_heirloom_guide_inlaid_gem(&self, _req: crate::tgf::CsGearHeirloomGuideInlaidGemReq) -> Option<crate::tgf::ScGearHeirloomGuideInlaidGemRsp> {
        Some(<crate::tgf::ScGearHeirloomGuideInlaidGemRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_heirloom_guide_inlaid_clear(&self, _req: crate::tgf::CsGearHeirloomGuideInlaidClearReq) -> Option<crate::tgf::ScGearHeirloomGuideInlaidClearRsp> {
        Some(<crate::tgf::ScGearHeirloomGuideInlaidClearRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_insurance_lock(&self, _req: crate::tgf::CsGearInsuranceLockReq) -> Option<crate::tgf::ScGearInsuranceLockRsp> {
        Some(<crate::tgf::ScGearInsuranceLockRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_insurance_unlock(&self, _req: crate::tgf::CsGearInsuranceUnLockReq) -> Option<crate::tgf::ScGearInsuranceUnLockRsp> {
        Some(<crate::tgf::ScGearInsuranceUnLockRsp as ::core::default::Default>::default())
    }
    async fn handle_gear_insurance_price(&self, _req: crate::tgf::CsGearInsurancePriceReq) -> Option<crate::tgf::ScGearInsurancePriceRsp> {
        Some(<crate::tgf::ScGearInsurancePriceRsp as ::core::default::Default>::default())
    }
    async fn handle_facade_update(&self, _req: crate::tgf::CsFacadeSetReq) -> Option<crate::tgf::ScFacadeSetRsp> {
        Some(<crate::tgf::ScFacadeSetRsp as ::core::default::Default>::default())
    }
    async fn handle_facade_update_role_skin(&self, _req: crate::tgf::CsFacadeUpdateRoleSkinReq) -> Option<crate::tgf::ScFacadeUpdateRoleSkinRsp> {
        Some(<crate::tgf::ScFacadeUpdateRoleSkinRsp as ::core::default::Default>::default())
    }
    async fn handle_facade_save_skin_template(&self, _req: crate::tgf::CsFacadeSaveSkinTemplateReq) -> Option<crate::tgf::ScFacadeSaveSkinTemplateRsp> {
        Some(<crate::tgf::ScFacadeSaveSkinTemplateRsp as ::core::default::Default>::default())
    }
    async fn handle_facade_delete_skin_template(&self, _req: crate::tgf::CsFacadeDeleteSkinTemplateReq) -> Option<crate::tgf::ScFacadeDeleteSkinTemplateRsp> {
        Some(<crate::tgf::ScFacadeDeleteSkinTemplateRsp as ::core::default::Default>::default())
    }
    async fn handle_facade_get_on_sale_facade(&self, _req: crate::tgf::CsFacadeGetOnSaleFacadeReq) -> Option<crate::tgf::ScFacadeGetOnSaleFacadeRsp> {
        Some(<crate::tgf::ScFacadeGetOnSaleFacadeRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_list(&self, _req: crate::tgf::CsActivityCenterListReq) -> Option<crate::tgf::ScActivityCenterListRsp> {
        Some(<crate::tgf::ScActivityCenterListRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_random_rewards(&self, _req: crate::tgf::CsActivityRandomRewardReq) -> Option<crate::tgf::ScActivityRandomRewardRsp> {
        Some(<crate::tgf::ScActivityRandomRewardRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_random_reward_selected(&self, _req: crate::tgf::CsActivityRandomRewardSelectdReq) -> Option<crate::tgf::ScActivityRandomRewardSelectdRsp> {
        Some(<crate::tgf::ScActivityRandomRewardSelectdRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_buy_passport(&self, _req: crate::tgf::CsActivityCenterBuyPassportReq) -> Option<crate::tgf::ScActivityCenterBuyPassportRsp> {
        Some(<crate::tgf::ScActivityCenterBuyPassportRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_passport_receive_rewards(&self, _req: crate::tgf::CsActivityCenterPassportReceiveRewardsReq) -> Option<crate::tgf::ScActivityCenterPassportReceiveRewardsRsp> {
        Some(<crate::tgf::ScActivityCenterPassportReceiveRewardsRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_passport_receive_cycle_rewards(&self, _req: crate::tgf::CsActivityCenterPassportReceiveCycleRewardsReq) -> Option<crate::tgf::ScActivityCenterPassportReceiveCycleRewardsRsp> {
        Some(<crate::tgf::ScActivityCenterPassportReceiveCycleRewardsRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_passport_refresh_quest(&self, _req: crate::tgf::CsActivityCenterPassportRefreshQuestReq) -> Option<crate::tgf::ScActivityCenterPassportRefreshQuestRsp> {
        Some(<crate::tgf::ScActivityCenterPassportRefreshQuestRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_trial_open(&self, _req: crate::tgf::CsActivityCenteTrialOpenReq) -> Option<crate::tgf::ScActivityCenterTrialOpenRsp> {
        Some(<crate::tgf::ScActivityCenterTrialOpenRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_trial_self_quit(&self, _req: crate::tgf::CsActivityCenterTrialSelfQuitReq) -> Option<crate::tgf::ScActivityCenterTrialSelfQuitRsp> {
        Some(<crate::tgf::ScActivityCenterTrialSelfQuitRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_trial_receive_reward(&self, _req: crate::tgf::CsActivityCenterTrialReceiveRewardReq) -> Option<crate::tgf::ScActivityCenterTrialReceiveRewardRsp> {
        Some(<crate::tgf::ScActivityCenterTrialReceiveRewardRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_trial_history_record(&self, _req: crate::tgf::CsActivityCenterTrialHistoryRecordReq) -> Option<crate::tgf::ScActivityCenterTrialHistoryRecordRsp> {
        Some(<crate::tgf::ScActivityCenterTrialHistoryRecordRsp as ::core::default::Default>::default())
    }
    async fn handle_activity_center_trial_record_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScActivityCenterTrialRecordSync> {
        Some(<crate::tgf::ScActivityCenterTrialRecordSync as ::core::default::Default>::default())
    }
    async fn handle_room_create(&self, _req: crate::tgf::CsRoomCreateReq) -> Option<crate::tgf::ScRoomCreateRsp> {
        Some(<crate::tgf::ScRoomCreateRsp as ::core::default::Default>::default())
    }
    async fn handle_room_enter(&self, _req: crate::tgf::CsRoomEnterReq) -> Option<crate::tgf::ScRoomEnterRsp> {
        Some(<crate::tgf::ScRoomEnterRsp as ::core::default::Default>::default())
    }
    async fn handle_room_leave(&self, _req: crate::tgf::CsRoomLeaveReq) -> Option<crate::tgf::ScRoomLeaveRsp> {
        Some(<crate::tgf::ScRoomLeaveRsp as ::core::default::Default>::default())
    }
    async fn handle_room_disband(&self, _req: crate::tgf::CsRoomDisbandReq) -> Option<crate::tgf::ScRoomDisbandRsp> {
        Some(<crate::tgf::ScRoomDisbandRsp as ::core::default::Default>::default())
    }
    async fn handle_room_change_myself_pos(&self, _req: crate::tgf::CsRoomChangeMySelfPosReq) -> Option<crate::tgf::ScRoomChangeMySelfPosRsp> {
        Some(<crate::tgf::ScRoomChangeMySelfPosRsp as ::core::default::Default>::default())
    }
    async fn handle_room_change_member_pos(&self, _req: crate::tgf::CsRoomChangeMemberPosReq) -> Option<crate::tgf::ScRoomChangeMemberPosRsp> {
        Some(<crate::tgf::ScRoomChangeMemberPosRsp as ::core::default::Default>::default())
    }
    async fn handle_room_select_battle_map(&self, _req: crate::tgf::CsRoomSelectBattleMapReq) -> Option<crate::tgf::ScRoomSelectBattleMapRsp> {
        Some(<crate::tgf::ScRoomSelectBattleMapRsp as ::core::default::Default>::default())
    }
    async fn handle_room_select_battle_type(&self, _req: crate::tgf::CsRoomSelectBattleTypeReq) -> Option<crate::tgf::ScRoomSelectBattleTypeRsp> {
        Some(<crate::tgf::ScRoomSelectBattleTypeRsp as ::core::default::Default>::default())
    }
    async fn handle_room_get_ready(&self, _req: crate::tgf::CsRoomGetReadyReq) -> Option<crate::tgf::ScRoomGetReadyRsp> {
        Some(<crate::tgf::ScRoomGetReadyRsp as ::core::default::Default>::default())
    }
    async fn handle_room_cancel_ready(&self, _req: crate::tgf::CsRoomCancelReadyReq) -> Option<crate::tgf::ScRoomCancelReadyRsp> {
        Some(<crate::tgf::ScRoomCancelReadyRsp as ::core::default::Default>::default())
    }
    async fn handle_room_start_battle(&self, _req: crate::tgf::CsRoomStartBattleReq) -> Option<crate::tgf::ScRoomStartBattleRsp> {
        Some(<crate::tgf::ScRoomStartBattleRsp as ::core::default::Default>::default())
    }
    async fn handle_room_name_change(&self, _req: crate::tgf::CsRoomNameChangeReq) -> Option<crate::tgf::ScRoomNameChangeRsp> {
        Some(<crate::tgf::ScRoomNameChangeRsp as ::core::default::Default>::default())
    }
    async fn handle_room_keyword_change(&self, _req: crate::tgf::CsRoomKeywordChangeReq) -> Option<crate::tgf::ScRoomKeywordChangeRsp> {
        Some(<crate::tgf::ScRoomKeywordChangeRsp as ::core::default::Default>::default())
    }
    async fn handle_room_transfer_owner(&self, _req: crate::tgf::CsRoomTransferOwnerReq) -> Option<crate::tgf::ScRoomTransferOwnerRsp> {
        Some(<crate::tgf::ScRoomTransferOwnerRsp as ::core::default::Default>::default())
    }
    async fn handle_room_kickoff_member(&self, _req: crate::tgf::CsRoomKickOffMemberReq) -> Option<crate::tgf::ScRoomKickOffMemberRsp> {
        Some(<crate::tgf::ScRoomKickOffMemberRsp as ::core::default::Default>::default())
    }
    async fn handle_room_suggest(&self, _req: crate::tgf::CsRoomSuggestReq) -> Option<crate::tgf::ScRoomSuggestRsp> {
        Some(<crate::tgf::ScRoomSuggestRsp as ::core::default::Default>::default())
    }
    async fn handle_room_batch_get(&self, _req: crate::tgf::CsRoomBatchGetReq) -> Option<crate::tgf::ScRoomBatchGetRsp> {
        Some(<crate::tgf::ScRoomBatchGetRsp as ::core::default::Default>::default())
    }
    async fn handle_room_accept_invite(&self, _req: crate::tgf::CsRoomInviteAcceptReq) -> Option<crate::tgf::ScRoomInviteAcceptRsp> {
        Some(<crate::tgf::ScRoomInviteAcceptRsp as ::core::default::Default>::default())
    }
    async fn handle_room_send_invite(&self, _req: crate::tgf::CsRoomInviteSendReq) -> Option<crate::tgf::ScRoomInviteSendRsp> {
        Some(<crate::tgf::ScRoomInviteSendRsp as ::core::default::Default>::default())
    }
    async fn handle_room_reject_invite(&self, _req: crate::tgf::CsRoomInviteRejectReq) -> Option<crate::tgf::ScRoomInviteRejectRsp> {
        Some(<crate::tgf::ScRoomInviteRejectRsp as ::core::default::Default>::default())
    }
    async fn handle_room_ignore_room_all_invite(&self, _req: crate::tgf::CsRoomIgnoreRoomAllInviteReq) -> Option<crate::tgf::ScRoomIgnoreRoomAllInviteRsp> {
        Some(<crate::tgf::ScRoomIgnoreRoomAllInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_room_ignore_all_user_invite(&self, _req: crate::tgf::CsRoomIgnoreAllUserInviteReq) -> Option<crate::tgf::ScRoomIgnoreAllUserInviteRsp> {
        Some(<crate::tgf::ScRoomIgnoreAllUserInviteRsp as ::core::default::Default>::default())
    }
    async fn handle_hope_instruct_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScHopeInstructionSync> {
        Some(<crate::tgf::ScHopeInstructionSync as ::core::default::Default>::default())
    }
    async fn handle_hope_certification_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScHopeQueryCertificationSync> {
        Some(<crate::tgf::ScHopeQueryCertificationSync as ::core::default::Default>::default())
    }
    async fn handle_hope_report_execute(&self, _req: crate::tgf::CsHopeReportExecuteReq) -> Option<crate::tgf::ScHopeReportExecuteRsp> {
        Some(<crate::tgf::ScHopeReportExecuteRsp as ::core::default::Default>::default())
    }
    async fn handle_mithril_force_role_init(&self, _req: crate::tgf::CsMithrilForceRoleInitReq) -> Option<crate::tgf::ScMithrilForceRoleInitRsp> {
        Some(<crate::tgf::ScMithrilForceRoleInitRsp as ::core::default::Default>::default())
    }
    async fn handle_mithril_force_role_select(&self, _req: crate::tgf::CsMithrilForceRoleSelectReq) -> Option<crate::tgf::ScMithrilForceRoleSelectRsp> {
        Some(<crate::tgf::ScMithrilForceRoleSelectRsp as ::core::default::Default>::default())
    }
    async fn handle_mithril_force_receive_reward(&self, _req: crate::tgf::CsMithrilForceReceiveRewardReq) -> Option<crate::tgf::ScMithrilForceReceiveRewardRsp> {
        Some(<crate::tgf::ScMithrilForceReceiveRewardRsp as ::core::default::Default>::default())
    }
    async fn handle_mithril_role_wear(&self, _req: crate::tgf::CsMithrilRoleWearReq) -> Option<crate::tgf::ScMithrilRoleWearRsp> {
        Some(<crate::tgf::ScMithrilRoleWearRsp as ::core::default::Default>::default())
    }
    async fn handle_dlc_packet_sync(&self, _req: crate::google::protobuf::Empty) -> Option<crate::tgf::ScNewDlcPacketSync> {
        Some(<crate::tgf::ScNewDlcPacketSync as ::core::default::Default>::default())
    }
    async fn handle_dlc_receive_packet(&self, _req: crate::tgf::CsReceiveDlcPacketReq) -> Option<crate::tgf::ScReceiveDlcPacketRsp> {
        Some(<crate::tgf::ScReceiveDlcPacketRsp as ::core::default::Default>::default())
    }
    async fn handle_refresh_dlc_info(&self, _req: crate::tgf::CsRefreshDlcInfoReq) -> Option<crate::tgf::ScRefreshDlcInfoRsp> {
        Some(<crate::tgf::ScRefreshDlcInfoRsp as ::core::default::Default>::default())
    }

}

pub async fn route_rpc<H: RpcHandlers>(handler: &H, method_name: &str, payload: &[u8]) -> Option<(String, Vec<u8>)> {
    match method_name {

            "hello_world" => {
                if let Ok(req) = <crate::tgf::DsHelloWorldReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hello_world(req).await {
                        return Some(("tgf.DSHelloWorldRsp".to_string(), <crate::tgf::DsHelloWorldRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "init_player" => {
                if let Ok(req) = <crate::tgf::DsInitPlayerListReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_init_player(req).await {
                        return Some(("tgf.DSInitPlayerListRsp".to_string(), <crate::tgf::DsInitPlayerListRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "heartbeat" => {
                if let Ok(req) = <crate::tgf::DsHeartbeatReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_heartbeat(req).await {
                        return Some(("tgf.DSHeartbeatRsp".to_string(), <crate::tgf::DsHeartbeatRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "query_player_game_status" => {
                if let Ok(req) = <crate::tgf::DsQueryPlayerGameStatusReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_query_player_game_status(req).await {
                        return Some(("tgf.DSQueryPlayerGameStatusRsp".to_string(), <crate::tgf::DsQueryPlayerGameStatusRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_self_quit" => {
                if let Ok(req) = <crate::tgf::DsPlayerSelfQuitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_self_quit(req).await {
                        return Some(("tgf.DSPlayerSelfQuitRsp".to_string(), <crate::tgf::DsPlayerSelfQuitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_kickout" => {
                if let Ok(req) = <crate::tgf::DsPlayerKickOutReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_kickout(req).await {
                        return Some(("tgf.DSPlayerKickOutRsp".to_string(), <crate::tgf::DsPlayerKickOutRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_kickout_off" => {
                if let Ok(req) = <crate::tgf::DsPlayerKickOutOffReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_kickout_off(req).await {
                        return Some(("tgf.DSPlayerKickOutOffRsp".to_string(), <crate::tgf::DsPlayerKickOutOffRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "create_game_session_result_event" => {
                if let Ok(req) = <crate::tgf::CreateGameSessionResultEventReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_create_game_session_result_event(req).await {
                        return Some(("google.protobuf.Empty".to_string(), <crate::google::protobuf::Empty as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "game_session_end_event" => {
                if let Ok(req) = <crate::tgf::GameSessionEndEventReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_game_session_end_event(req).await {
                        return Some(("google.protobuf.Empty".to_string(), <crate::google::protobuf::Empty as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "ds_alias_status_report" => {
                if let Ok(req) = <crate::tgf::DsAliasStatusReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_ds_alias_status_report(req).await {
                        return Some(("google.protobuf.Empty".to_string(), <crate::google::protobuf::Empty as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "dsc_status_report" => {
                if let Ok(req) = <crate::tgf::DscStatusReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_dsc_status_report(req).await {
                        return Some(("google.protobuf.Empty".to_string(), <crate::google::protobuf::Empty as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_game_over" => {
                if let Ok(req) = <crate::tgf::DsPlayerGameOverReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_game_over(req).await {
                        return Some(("tgf.DSPlayerGameOverRsp".to_string(), <crate::tgf::DsPlayerGameOverRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "ds_end" => {
                if let Ok(req) = <crate::tgf::DsDsEndReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_ds_end(req).await {
                        return Some(("tgf.DSDsEndRsp".to_string(), <crate::tgf::DsDsEndRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_online_heartbeat" => {
                if let Ok(req) = <crate::tgf::DsPlayerOnlineHeartbeatReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_online_heartbeat(req).await {
                        return Some(("tgf.DSPlayerOnlineHeartbeatRsp".to_string(), <crate::tgf::DsPlayerOnlineHeartbeatRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_online_report" => {
                if let Ok(req) = <crate::tgf::DsPlayerOnlineReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_online_report(req).await {
                        return Some(("tgf.DSPlayerOnlineReportRsp".to_string(), <crate::tgf::DsPlayerOnlineReportRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pre_login" => {
                if let Ok(req) = <crate::tgf::CsPreLoginReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pre_login(req).await {
                        return Some(("tgf.SCPreLoginRsp".to_string(), <crate::tgf::ScPreLoginRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "login_notify_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_login_notify_sync(req).await {
                        return Some(("tgf.SCLoginNotifySync".to_string(), <crate::tgf::ScLoginNotifySync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "login" => {
                if let Ok(req) = <crate::tgf::CsLoginReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_login(req).await {
                        return Some(("tgf.SCLoginRsp".to_string(), <crate::tgf::ScLoginRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "client_action_log" => {
                if let Ok(req) = <crate::tgf::CsCommonLoggingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_client_action_log(req).await {
                        return Some(("tgf.SCCommonLoggingRsp".to_string(), <crate::tgf::ScCommonLoggingRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "ping" => {
                if let Ok(req) = <crate::tgf::CsPingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_ping(req).await {
                        return Some(("tgf.SCPongRsp".to_string(), <crate::tgf::ScPongRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "business_ping" => {
                if let Ok(req) = <crate::tgf::CsBusinessPingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_business_ping(req).await {
                        return Some(("tgf.SCBusinessPongRsp".to_string(), <crate::tgf::ScBusinessPongRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "network_latency_report" => {
                if let Ok(req) = <crate::tgf::CsNetworkLatencyReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_network_latency_report(req).await {
                        return Some(("tgf.SCNetworkLatencyReportRsp".to_string(), <crate::tgf::ScNetworkLatencyReportRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "network_block_loading" => {
                if let Ok(req) = <crate::tgf::CsNetworkBlockLoadingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_network_block_loading(req).await {
                        return Some(("tgf.SCNetworkBlockLoadingRsp".to_string(), <crate::tgf::ScNetworkBlockLoadingRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "update_account_data" => {
                if let Ok(req) = <crate::tgf::CsUpdateAccountDataReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_update_account_data(req).await {
                        return Some(("tgf.SCUpdateAccountDataRsp".to_string(), <crate::tgf::ScUpdateAccountDataRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "telemetry_report" => {
                if let Ok(req) = <crate::tgf::CsTelemetryReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_telemetry_report(req).await {
                        return Some(("tgf.SCTelemetryReportRsp".to_string(), <crate::tgf::ScTelemetryReportRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "modal_message" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_modal_message(req).await {
                        return Some(("tgf.SCModalMessageSync".to_string(), <crate::tgf::ScModalMessageSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_get_info" => {
                if let Ok(req) = <crate::tgf::CsPlayerGetInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_get_info(req).await {
                        return Some(("tgf.SCPlayerGetInfoRsp".to_string(), <crate::tgf::ScPlayerGetInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_dirty_change" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_dirty_change(req).await {
                        return Some(("tgf.SCPlayerDirtyChgSync".to_string(), <crate::tgf::ScPlayerDirtyChgSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_get_info_by_page" => {
                if let Ok(req) = <crate::tgf::CsPlayerGetInfoByPageReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_get_info_by_page(req).await {
                        return Some(("tgf.SCPlayerGetInfoByPageRsp".to_string(), <crate::tgf::ScPlayerGetInfoByPageRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_batch_get_basic_info" => {
                if let Ok(req) = <crate::tgf::CsPlayerBatchGetBasicInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_batch_get_basic_info(req).await {
                        return Some(("tgf.SCPlayerBatchGetBasicInfoRsp".to_string(), <crate::tgf::ScPlayerBatchGetBasicInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_batch_get_id_key" => {
                if let Ok(req) = <crate::tgf::CsPlayerBatchGetIdKeyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_batch_get_id_key(req).await {
                        return Some(("tgf.SCPlayerBatchGetIDKeyRsp".to_string(), <crate::tgf::ScPlayerBatchGetIdKeyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_rename" => {
                if let Ok(req) = <crate::tgf::CsPlayerRenameReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_rename(req).await {
                        return Some(("tgf.SCPlayerRenameRsp".to_string(), <crate::tgf::ScPlayerRenameRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_update_icon" => {
                if let Ok(req) = <crate::tgf::CsPlayerUpdateIconReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_update_icon(req).await {
                        return Some(("tgf.SCPlayerUpdateIconRsp".to_string(), <crate::tgf::ScPlayerUpdateIconRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_update_options" => {
                if let Ok(req) = <crate::tgf::CsPlayerUpdateOptionsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_update_options(req).await {
                        return Some(("tgf.SCPlayerUpdateOptionsRsp".to_string(), <crate::tgf::ScPlayerUpdateOptionsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_gm_command" => {
                if let Ok(req) = <crate::tgf::CsPlayerGmCommandReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_gm_command(req).await {
                        return Some(("tgf.SCPlayerGMCommandRsp".to_string(), <crate::tgf::ScPlayerGmCommandRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_setting_data" => {
                if let Ok(req) = <crate::tgf::CsPlayerSettingDataReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_setting_data(req).await {
                        return Some(("tgf.SCPlayerSettingDataRsp".to_string(), <crate::tgf::ScPlayerSettingDataRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_kickoff_reason" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_kickoff_reason(req).await {
                        return Some(("tgf.SCPlayerKickoffReasonSync".to_string(), <crate::tgf::ScPlayerKickoffReasonSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_ace_packet" => {
                if let Ok(req) = <crate::tgf::CsPlayerAcePacketReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_ace_packet(req).await {
                        return Some(("tgf.SCPlayerAcePacketRsp".to_string(), <crate::tgf::ScPlayerAcePacketRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_ace_packet_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_ace_packet_sync(req).await {
                        return Some(("tgf.SCPlayerAcePacketSync".to_string(), <crate::tgf::ScPlayerAcePacketSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "event_cmd_info_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_event_cmd_info_sync(req).await {
                        return Some(("tgf.SCEventCmdInfoSync".to_string(), <crate::tgf::ScEventCmdInfoSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_uri_redirect" => {
                if let Ok(req) = <crate::tgf::CsPlayerRedirectReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_uri_redirect(req).await {
                        return Some(("tgf.SCPlayerRedirectRsp".to_string(), <crate::tgf::ScPlayerRedirectRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_receive_homeland_reward" => {
                if let Ok(req) = <crate::tgf::CsPlayerReceiveHomelandRewardReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_receive_homeland_reward(req).await {
                        return Some(("tgf.SCPlayerReceiveHomelandRewardRsp".to_string(), <crate::tgf::ScPlayerReceiveHomelandRewardRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "report_action" => {
                if let Ok(req) = <crate::tgf::CsReportActionReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_report_action(req).await {
                        return Some(("tgf.SCReportActionRsp".to_string(), <crate::tgf::ScReportActionRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_birthday_update" => {
                if let Ok(req) = <crate::tgf::CsPlayerUpdateBirthDayReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_birthday_update(req).await {
                        return Some(("tgf.SCPlayerUpdateBirthDayRsp".to_string(), <crate::tgf::ScPlayerUpdateBirthDayRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "cache_batch_get" => {
                if let Ok(req) = <crate::tgf::CsCacheBatchGetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_cache_batch_get(req).await {
                        return Some(("tgf.SCCacheBatchGetRsp".to_string(), <crate::tgf::ScCacheBatchGetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "cache_unwatch" => {
                if let Ok(req) = <crate::tgf::CsCacheUnwatchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_cache_unwatch(req).await {
                        return Some(("tgf.SCCacheUnwatchRsp".to_string(), <crate::tgf::ScCacheUnwatchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "cache_expired" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_cache_expired(req).await {
                        return Some(("tgf.SCCacheExpiredSync".to_string(), <crate::tgf::ScCacheExpiredSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "cache_update_meta" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_cache_update_meta(req).await {
                        return Some(("tgf.SCCacheUpdateMetaSync".to_string(), <crate::tgf::ScCacheUpdateMetaSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "player_all_cache_expired" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_player_all_cache_expired(req).await {
                        return Some(("tgf.SCAllCacheExpiredSync".to_string(), <crate::tgf::ScAllCacheExpiredSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_get_all" => {
                if let Ok(req) = <crate::tgf::CsMailGetAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_get_all(req).await {
                        return Some(("tgf.SCMailGetAllRsp".to_string(), <crate::tgf::ScMailGetAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_read" => {
                if let Ok(req) = <crate::tgf::CsMailReadReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_read(req).await {
                        return Some(("tgf.SCMailReadRsp".to_string(), <crate::tgf::ScMailReadRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_read_all" => {
                if let Ok(req) = <crate::tgf::CsMailReadAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_read_all(req).await {
                        return Some(("tgf.SCMailReadAllRsp".to_string(), <crate::tgf::ScMailReadAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_receive_attachment" => {
                if let Ok(req) = <crate::tgf::CsMailReceiveAttachmentsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_receive_attachment(req).await {
                        return Some(("tgf.SCMailReceiveAttachmentsRsp".to_string(), <crate::tgf::ScMailReceiveAttachmentsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_receive_attachment_all" => {
                if let Ok(req) = <crate::tgf::CsMailReceiveAttachmentsAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_receive_attachment_all(req).await {
                        return Some(("tgf.SCMailReceiveAttachmentsAllRsp".to_string(), <crate::tgf::ScMailReceiveAttachmentsAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_change_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_change_sync(req).await {
                        return Some(("tgf.SCMailChangeSync".to_string(), <crate::tgf::ScMailChangeSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mail_receive_attachment_partly" => {
                if let Ok(req) = <crate::tgf::CsMailReceiveAttachmentsPartlyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mail_receive_attachment_partly(req).await {
                        return Some(("tgf.SCMailReceiveAttachmentsPartlyRsp".to_string(), <crate::tgf::ScMailReceiveAttachmentsPartlyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "hall_set_role" => {
                if let Ok(req) = <crate::tgf::CsHallSetRoleReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hall_set_role(req).await {
                        return Some(("tgf.SCHallSetRoleRsp".to_string(), <crate::tgf::ScHallSetRoleRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "hall_switch" => {
                if let Ok(req) = <crate::tgf::CsHallSwitchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hall_switch(req).await {
                        return Some(("tgf.SCHallSwitchRsp".to_string(), <crate::tgf::ScHallSwitchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_pre_search" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingPreSearchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_pre_search(req).await {
                        return Some(("tgf.SCBattleMatchingPreSearchRsp".to_string(), <crate::tgf::ScBattleMatchingPreSearchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_cancel_pre_search" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingCancelPreSearchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_cancel_pre_search(req).await {
                        return Some(("tgf.SCBattleMatchingCancelPreSearchRsp".to_string(), <crate::tgf::ScBattleMatchingCancelPreSearchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_search" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingSearchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_search(req).await {
                        return Some(("tgf.SCBattleMatchingSearchRsp".to_string(), <crate::tgf::ScBattleMatchingSearchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_heartbeat" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingHeartbeatReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_heartbeat(req).await {
                        return Some(("tgf.SCBattleMatchingHeartbeatRsp".to_string(), <crate::tgf::ScBattleMatchingHeartbeatRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_exit" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingExitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_exit(req).await {
                        return Some(("tgf.SCBattleMatchingExitRsp".to_string(), <crate::tgf::ScBattleMatchingExitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_confirm_refuse" => {
                if let Ok(req) = <crate::tgf::CsBattleMatchingConfirmRefuseReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_confirm_refuse(req).await {
                        return Some(("tgf.SCBattleMatchingConfirmRefuseRsp".to_string(), <crate::tgf::ScBattleMatchingConfirmRefuseRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_matching_log_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_matching_log_sync(req).await {
                        return Some(("tgf.SCBattleMatchingLogSync".to_string(), <crate::tgf::ScBattleMatchingLogSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_room_log_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_room_log_sync(req).await {
                        return Some(("tgf.SCBattleRoomLogSync".to_string(), <crate::tgf::ScBattleRoomLogSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_room_heartbeat" => {
                if let Ok(req) = <crate::tgf::CsBattleRoomHeartbeatReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_room_heartbeat(req).await {
                        return Some(("tgf.SCBattleRoomHeartbeatRsp".to_string(), <crate::tgf::ScBattleRoomHeartbeatRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_record_list_get" => {
                if let Ok(req) = <crate::tgf::CsBattleRecordListGetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_record_list_get(req).await {
                        return Some(("tgf.SCBattleRecordListGetRsp".to_string(), <crate::tgf::ScBattleRecordListGetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_record_data_get" => {
                if let Ok(req) = <crate::tgf::CsBattleRecordDataGetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_record_data_get(req).await {
                        return Some(("tgf.SCBattleRecordDataGetRsp".to_string(), <crate::tgf::ScBattleRecordDataGetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_record_data_set" => {
                if let Ok(req) = <crate::tgf::CsBattleRecordDataSetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_record_data_set(req).await {
                        return Some(("tgf.SCBattleRecordDataSetRsp".to_string(), <crate::tgf::ScBattleRecordDataSetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_room_player_act" => {
                if let Ok(req) = <crate::tgf::CsBattleRoomPlayerActReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_room_player_act(req).await {
                        return Some(("tgf.SCBattleRoomPlayerActRsp".to_string(), <crate::tgf::ScBattleRoomPlayerActRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_update_setting" => {
                if let Ok(req) = <crate::tgf::CsBattleUpdateSettingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_update_setting(req).await {
                        return Some(("tgf.SCBattleRoomPlayerActRsp".to_string(), <crate::tgf::ScBattleRoomPlayerActRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_settle_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_settle_sync(req).await {
                        return Some(("tgf.SCBattleSettleSync".to_string(), <crate::tgf::ScBattleSettleSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "battle_fetch_battle_user_data" => {
                if let Ok(req) = <crate::tgf::CsBattleFetchBattleUserReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_battle_fetch_battle_user_data(req).await {
                        return Some(("tgf.SCBattleFetchBattleUserRsp".to_string(), <crate::tgf::ScBattleFetchBattleUserRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "level_prestart" => {
                if let Ok(req) = <crate::tgf::CsLevelPrestartReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_level_prestart(req).await {
                        return Some(("tgf.SCLevelPrestartRsp".to_string(), <crate::tgf::ScLevelPrestartRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "level_start" => {
                if let Ok(req) = <crate::tgf::CsLevelStartReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_level_start(req).await {
                        return Some(("tgf.SCLevelStartRsp".to_string(), <crate::tgf::ScLevelStartRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "level_finish" => {
                if let Ok(req) = <crate::tgf::CsLevelFinishReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_level_finish(req).await {
                        return Some(("tgf.SCLevelFinishRsp".to_string(), <crate::tgf::ScLevelFinishRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "level_select" => {
                if let Ok(req) = <crate::tgf::CsLevelCompetitionSelectReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_level_select(req).await {
                        return Some(("tgf.SCLevelCompetitionSelectRsp".to_string(), <crate::tgf::ScLevelCompetitionSelectRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "level_challenge" => {
                if let Ok(req) = <crate::tgf::CsLevelChallengeReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_level_challenge(req).await {
                        return Some(("tgf.SCLevelChallengeRsp".to_string(), <crate::tgf::ScLevelChallengeRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "map_move" => {
                if let Ok(req) = <crate::tgf::CsMapMoveReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_map_move(req).await {
                        return Some(("tgf.SCMapMoveRsp".to_string(), <crate::tgf::ScMapMoveRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_get_all" => {
                if let Ok(req) = <crate::tgf::CsFriendGetAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_get_all(req).await {
                        return Some(("tgf.SCFriendGetAllRsp".to_string(), <crate::tgf::ScFriendGetAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_invite" => {
                if let Ok(req) = <crate::tgf::CsFriendInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_invite(req).await {
                        return Some(("tgf.SCFriendInviteRsp".to_string(), <crate::tgf::ScFriendInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_accept_invite" => {
                if let Ok(req) = <crate::tgf::CsFriendAcceptInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_accept_invite(req).await {
                        return Some(("tgf.SCFriendAcceptInviteRsp".to_string(), <crate::tgf::ScFriendAcceptInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_reject_invite" => {
                if let Ok(req) = <crate::tgf::CsFriendRejectInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_reject_invite(req).await {
                        return Some(("tgf.SCFriendRejectInviteRsp".to_string(), <crate::tgf::ScFriendRejectInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_remove" => {
                if let Ok(req) = <crate::tgf::CsFriendRemoveReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_remove(req).await {
                        return Some(("tgf.SCFriendRemoveRsp".to_string(), <crate::tgf::ScFriendRemoveRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_send_gift" => {
                if let Ok(req) = <crate::tgf::CsFriendSendGiftReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_send_gift(req).await {
                        return Some(("tgf.SCFriendSendGiftRsp".to_string(), <crate::tgf::ScFriendSendGiftRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_receive_gift" => {
                if let Ok(req) = <crate::tgf::CsFriendReceiveGiftReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_receive_gift(req).await {
                        return Some(("tgf.SCFriendReceiveGiftRsp".to_string(), <crate::tgf::ScFriendReceiveGiftRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_get_suggest" => {
                if let Ok(req) = <crate::tgf::CsFriendGetSuggestReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_get_suggest(req).await {
                        return Some(("tgf.SCFriendGetSuggestRsp".to_string(), <crate::tgf::ScFriendGetSuggestRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_accept_all_invite" => {
                if let Ok(req) = <crate::tgf::CsFriendAcceptAllInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_accept_all_invite(req).await {
                        return Some(("tgf.SCFriendAcceptAllInviteRsp".to_string(), <crate::tgf::ScFriendAcceptAllInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "friend_reject_all_invite" => {
                if let Ok(req) = <crate::tgf::CsFriendRejectAllInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_friend_reject_all_invite(req).await {
                        return Some(("tgf.SCFriendRejectAllInviteRsp".to_string(), <crate::tgf::ScFriendRejectAllInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_create" => {
                if let Ok(req) = <crate::tgf::CsGuildCreateReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_create(req).await {
                        return Some(("tgf.SCGuildCreateRsp".to_string(), <crate::tgf::ScGuildCreateRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_exit" => {
                if let Ok(req) = <crate::tgf::CsGuildExitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_exit(req).await {
                        return Some(("tgf.SCGuildExitRsp".to_string(), <crate::tgf::ScGuildExitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_pay_off" => {
                if let Ok(req) = <crate::tgf::CsGuildPayOffReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_pay_off(req).await {
                        return Some(("tgf.SCGuildPayOffRsp".to_string(), <crate::tgf::ScGuildPayOffRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_get_all" => {
                if let Ok(req) = <crate::tgf::CsGuildGetAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_get_all(req).await {
                        return Some(("tgf.SCGuildGetAllRsp".to_string(), <crate::tgf::ScGuildGetAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_batch_get" => {
                if let Ok(req) = <crate::tgf::CsGuildBatchGetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_batch_get(req).await {
                        return Some(("tgf.SCGuildBatchGetRsp".to_string(), <crate::tgf::ScGuildBatchGetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_suggest" => {
                if let Ok(req) = <crate::tgf::CsGuildSuggestReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_suggest(req).await {
                        return Some(("tgf.SCGuildSuggestRsp".to_string(), <crate::tgf::ScGuildSuggestRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_set_role" => {
                if let Ok(req) = <crate::tgf::CsGuildSetRoleReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_set_role(req).await {
                        return Some(("tgf.SCGuildSetRoleRsp".to_string(), <crate::tgf::ScGuildSetRoleRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_rename" => {
                if let Ok(req) = <crate::tgf::CsGuildRenameReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_rename(req).await {
                        return Some(("tgf.SCGuildRenameRsp".to_string(), <crate::tgf::ScGuildRenameRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_set_configure" => {
                if let Ok(req) = <crate::tgf::CsGuildSetConfigureReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_set_configure(req).await {
                        return Some(("tgf.SCGuildSetConfigureRsp".to_string(), <crate::tgf::ScGuildSetConfigureRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_send_apply" => {
                if let Ok(req) = <crate::tgf::CsGuildSendApplyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_send_apply(req).await {
                        return Some(("tgf.SCGuildSendApplyRsp".to_string(), <crate::tgf::ScGuildSendApplyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_approve_apply" => {
                if let Ok(req) = <crate::tgf::CsGuildApproveApplyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_approve_apply(req).await {
                        return Some(("tgf.SCGuildApproveApplyRsp".to_string(), <crate::tgf::ScGuildApproveApplyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_reject_apply" => {
                if let Ok(req) = <crate::tgf::CsGuildRejectApplyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_reject_apply(req).await {
                        return Some(("tgf.SCGuildRejectApplyRsp".to_string(), <crate::tgf::ScGuildRejectApplyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_send_invitation" => {
                if let Ok(req) = <crate::tgf::CsGuildSendInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_send_invitation(req).await {
                        return Some(("tgf.SCGuildSendInvitationRsp".to_string(), <crate::tgf::ScGuildSendInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_approve_invitation" => {
                if let Ok(req) = <crate::tgf::CsGuildApproveInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_approve_invitation(req).await {
                        return Some(("tgf.SCGuildApproveInvitationRsp".to_string(), <crate::tgf::ScGuildApproveInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_reject_invitation" => {
                if let Ok(req) = <crate::tgf::CsGuildRejectInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_reject_invitation(req).await {
                        return Some(("tgf.SCGuildRejectInvitationRsp".to_string(), <crate::tgf::ScGuildRejectInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_revoke_invitation" => {
                if let Ok(req) = <crate::tgf::CsGuildRevokeInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_revoke_invitation(req).await {
                        return Some(("tgf.SCGuildRevokeInvitationRsp".to_string(), <crate::tgf::ScGuildRevokeInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_remove_member" => {
                if let Ok(req) = <crate::tgf::CsGuildRemoveMemberReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_remove_member(req).await {
                        return Some(("tgf.SCGuildRemoveMemberRsp".to_string(), <crate::tgf::ScGuildRemoveMemberRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_add_impeachment" => {
                if let Ok(req) = <crate::tgf::CsGuildAddImpeachmentReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_add_impeachment(req).await {
                        return Some(("tgf.SCGuildAddImpeachmentRsp".to_string(), <crate::tgf::ScGuildAddImpeachmentRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "guild_vote_impeachment" => {
                if let Ok(req) = <crate::tgf::CsGuildVoteImpeachmentReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_guild_vote_impeachment(req).await {
                        return Some(("tgf.SCGuildVoteImpeachmentRsp".to_string(), <crate::tgf::ScGuildVoteImpeachmentRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "chat_get_all_channels" => {
                if let Ok(req) = <crate::tgf::CsChatGetAllChannelsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_chat_get_all_channels(req).await {
                        return Some(("tgf.SCChatGetAllChannelsRsp".to_string(), <crate::tgf::ScChatGetAllChannelsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "chat_get_channel_snapshot" => {
                if let Ok(req) = <crate::tgf::CsChatGetChannelSnapshotReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_chat_get_channel_snapshot(req).await {
                        return Some(("tgf.SCChatGetChannelSnapshotRsp".to_string(), <crate::tgf::ScChatGetChannelSnapshotRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "chat_channel_heartbeat" => {
                if let Ok(req) = <crate::tgf::CsChatChannelHeartbeatReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_chat_channel_heartbeat(req).await {
                        return Some(("tgf.SCChatChannelHeartbeatRsp".to_string(), <crate::tgf::ScChatChannelHeartbeatRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "chat_send_message" => {
                if let Ok(req) = <crate::tgf::CsChatSendMessageReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_chat_send_message(req).await {
                        return Some(("tgf.SCChatSendMessageRsp".to_string(), <crate::tgf::ScChatSendMessageRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "chat_update_panel" => {
                if let Ok(req) = <crate::tgf::CsChatUpdatePanelReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_chat_update_panel(req).await {
                        return Some(("tgf.SCChatUpdatePanelRsp".to_string(), <crate::tgf::ScChatUpdatePanelRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_send_invitation" => {
                if let Ok(req) = <crate::tgf::CsTeamSendInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_send_invitation(req).await {
                        return Some(("tgf.SCTeamSendInvitationRsp".to_string(), <crate::tgf::ScTeamSendInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_approve_invitation" => {
                if let Ok(req) = <crate::tgf::CsTeamApproveInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_approve_invitation(req).await {
                        return Some(("tgf.SCTeamApproveInvitationRsp".to_string(), <crate::tgf::ScTeamApproveInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_reject_invitation" => {
                if let Ok(req) = <crate::tgf::CsTeamRejectInvitationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_reject_invitation(req).await {
                        return Some(("tgf.SCTeamRejectInvitationRsp".to_string(), <crate::tgf::ScTeamRejectInvitationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_exit" => {
                if let Ok(req) = <crate::tgf::CsTeamExitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_exit(req).await {
                        return Some(("tgf.SCTeamExitRsp".to_string(), <crate::tgf::ScTeamExitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_remove_member" => {
                if let Ok(req) = <crate::tgf::CsTeamRemoveMemberReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_remove_member(req).await {
                        return Some(("tgf.SCTeamRemoveMemberRsp".to_string(), <crate::tgf::ScTeamRemoveMemberRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_send_battle_confirm" => {
                if let Ok(req) = <crate::tgf::CsTeamSendBattleConfirmReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_send_battle_confirm(req).await {
                        return Some(("tgf.SCTeamSendBattleConfirmRsp".to_string(), <crate::tgf::ScTeamSendBattleConfirmRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_send_battle_unconfirm" => {
                if let Ok(req) = <crate::tgf::CsTeamSendBattleUnconfirmReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_send_battle_unconfirm(req).await {
                        return Some(("tgf.SCTeamSendBattleUnconfirmRsp".to_string(), <crate::tgf::ScTeamSendBattleUnconfirmRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_transfer_captain" => {
                if let Ok(req) = <crate::tgf::CsTeamTransferCaptainReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_transfer_captain(req).await {
                        return Some(("tgf.SCTeamTransferCaptainRsp".to_string(), <crate::tgf::ScTeamTransferCaptainRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_apply_join_team" => {
                if let Ok(req) = <crate::tgf::CsTeamApplyJoinTeamReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_apply_join_team(req).await {
                        return Some(("tgf.SCTeamApplyJoinTeamRsp".to_string(), <crate::tgf::ScTeamApplyJoinTeamRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_accept_apply" => {
                if let Ok(req) = <crate::tgf::CsTeamAcceptApplyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_accept_apply(req).await {
                        return Some(("tgf.SCTeamAcceptApplyRsp".to_string(), <crate::tgf::ScTeamAcceptApplyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_change_team_finder" => {
                if let Ok(req) = <crate::tgf::CsTeamChangeTeamFinderReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_change_team_finder(req).await {
                        return Some(("tgf.SCTeamChangeTeamFinderRsp".to_string(), <crate::tgf::ScTeamChangeTeamFinderRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_reject_apply" => {
                if let Ok(req) = <crate::tgf::CsTeamRejectApplyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_reject_apply(req).await {
                        return Some(("tgf.SCTeamRejectApplyRsp".to_string(), <crate::tgf::ScTeamRejectApplyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_match_search" => {
                if let Ok(req) = <crate::tgf::CsTeamSearchReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_match_search(req).await {
                        return Some(("tgf.SCTeamSearchRsp".to_string(), <crate::tgf::ScTeamSearchRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_match_check" => {
                if let Ok(req) = <crate::tgf::CsTeamCheckReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_match_check(req).await {
                        return Some(("tgf.SCTeamCheckRsp".to_string(), <crate::tgf::ScTeamCheckRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "team_match_exit" => {
                if let Ok(req) = <crate::tgf::CsTeamMatchExitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_team_match_exit(req).await {
                        return Some(("tgf.SCTeamMatchExitRsp".to_string(), <crate::tgf::ScTeamMatchExitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_create" => {
                if let Ok(req) = <crate::tgf::CsRoleCreateReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_create(req).await {
                        return Some(("tgf.SCRoleCreateRsp".to_string(), <crate::tgf::ScRoleCreateRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_delete" => {
                if let Ok(req) = <crate::tgf::CsRoleDeleteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_delete(req).await {
                        return Some(("tgf.SCRoleDeleteRsp".to_string(), <crate::tgf::ScRoleDeleteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_delete_lazy" => {
                if let Ok(req) = <crate::tgf::CsRoleDeleteLazyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_delete_lazy(req).await {
                        return Some(("tgf.SCRoleDeleteLazyRsp".to_string(), <crate::tgf::ScRoleDeleteLazyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_cancel_lazy_delete" => {
                if let Ok(req) = <crate::tgf::CsRoleCancelLazyDeleteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_cancel_lazy_delete(req).await {
                        return Some(("tgf.SCRoleCancelLazyDeleteRsp".to_string(), <crate::tgf::ScRoleCancelLazyDeleteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_wear" => {
                if let Ok(req) = <crate::tgf::CsRoleWearReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_wear(req).await {
                        return Some(("tgf.SCRoleWearRsp".to_string(), <crate::tgf::ScRoleWearRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_weapon_set_swap" => {
                if let Ok(req) = <crate::tgf::CsRoleWeaponSetSwapReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_weapon_set_swap(req).await {
                        return Some(("tgf.SCRoleWeaponSetSwapRsp".to_string(), <crate::tgf::ScRoleWeaponSetSwapRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_take_off" => {
                if let Ok(req) = <crate::tgf::CsRoleTakeOffReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_take_off(req).await {
                        return Some(("tgf.SCRoleTakeOffRsp".to_string(), <crate::tgf::ScRoleTakeOffRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_fetch_user_role" => {
                if let Ok(req) = <crate::tgf::CsRoleFetchUserRoleReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_fetch_user_role(req).await {
                        return Some(("tgf.SCRoleFetchUserRoleRsp".to_string(), <crate::tgf::ScRoleFetchUserRoleRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_move_season" => {
                if let Ok(req) = <crate::tgf::CsRoleMoveSeasonReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_move_season(req).await {
                        return Some(("tgf.SCRoleMoveSeasonRsp".to_string(), <crate::tgf::ScRoleMoveSeasonRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_reshape_face_pinching" => {
                if let Ok(req) = <crate::tgf::CsRoleReshapeFacePinchingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_reshape_face_pinching(req).await {
                        return Some(("tgf.SCRoleReshapeFacePinchingRsp".to_string(), <crate::tgf::ScRoleReshapeFacePinchingRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "role_select_skill" => {
                if let Ok(req) = <crate::tgf::CsRoleSelectSkillReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_role_select_skill(req).await {
                        return Some(("tgf.SCRoleSelectSkillRsp".to_string(), <crate::tgf::ScRoleSelectSkillRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "quest_rewards_receive" => {
                if let Ok(req) = <crate::tgf::CsQuestRewardsReceiveReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_quest_rewards_receive(req).await {
                        return Some(("tgf.SCQuestRewardsReceiveRsp".to_string(), <crate::tgf::ScQuestRewardsReceiveRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "quest_special_progress_report" => {
                if let Ok(req) = <crate::tgf::CsQuestSpecialProgressReportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_quest_special_progress_report(req).await {
                        return Some(("tgf.SCQuestSpecialProgressReportRsp".to_string(), <crate::tgf::ScQuestSpecialProgressReportRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "quest_item_submit" => {
                if let Ok(req) = <crate::tgf::CsQuestItemSubmitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_quest_item_submit(req).await {
                        return Some(("tgf.SCQuestItemSubmitRsp".to_string(), <crate::tgf::ScQuestItemSubmitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "rank_get_specify_rank" => {
                if let Ok(req) = <crate::tgf::CsRankGetSpecifyRankReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_rank_get_specify_rank(req).await {
                        return Some(("tgf.SCRankGetSpecifyRankRsp".to_string(), <crate::tgf::ScRankGetSpecifyRankRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "rank_get_self_rank" => {
                if let Ok(req) = <crate::tgf::CsRankGetSelfRankReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_rank_get_self_rank(req).await {
                        return Some(("tgf.SCRankGetSelfRankRsp".to_string(), <crate::tgf::ScRankGetSelfRankRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "rank_cache_expired_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_rank_cache_expired_sync(req).await {
                        return Some(("tgf.SCRankCacheExpiredSync".to_string(), <crate::tgf::ScRankCacheExpiredSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "lottery_draw" => {
                if let Ok(req) = <crate::tgf::CsLotteryDrawReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_lottery_draw(req).await {
                        return Some(("tgf.SCLotteryDrawRsp".to_string(), <crate::tgf::ScLotteryDrawRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_get_product_list" => {
                if let Ok(req) = <crate::tgf::CsMarketGetProductListReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_get_product_list(req).await {
                        return Some(("tgf.SCMarketGetProductListRsp".to_string(), <crate::tgf::ScMarketGetProductListRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_buy_product" => {
                if let Ok(req) = <crate::tgf::CsMarketBuyProductReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_buy_product(req).await {
                        return Some(("tgf.SCMarketBuyProductRsp".to_string(), <crate::tgf::ScMarketBuyProductRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_sell_items" => {
                if let Ok(req) = <crate::tgf::CsMarketSellItemsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_sell_items(req).await {
                        return Some(("tgf.SCMarketSellItemsRsp".to_string(), <crate::tgf::ScMarketSellItemsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_buy_sold_item" => {
                if let Ok(req) = <crate::tgf::CsMarketBuySoldItemReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_buy_sold_item(req).await {
                        return Some(("tgf.SCMarketBuySoldItemRsp".to_string(), <crate::tgf::ScMarketBuySoldItemRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_reset_refresh_effect" => {
                if let Ok(req) = <crate::tgf::CsMarketResetRefreshEffectReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_reset_refresh_effect(req).await {
                        return Some(("tgf.SCMarketResetRefreshEffectRsp".to_string(), <crate::tgf::ScMarketResetRefreshEffectRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "market_video_popup" => {
                if let Ok(req) = <crate::tgf::CsMarketVideoPopUpReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_market_video_popup(req).await {
                        return Some(("tgf.SCMarketVideoPopUpRsp".to_string(), <crate::tgf::ScMarketVideoPopUpRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_get_all" => {
                if let Ok(req) = <crate::tgf::CsPayGetAllReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_get_all(req).await {
                        return Some(("tgf.SCPayGetAllRsp".to_string(), <crate::tgf::ScPayGetAllRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_update_diamond" => {
                if let Ok(req) = <crate::tgf::CsPayUpdateDiamondReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_update_diamond(req).await {
                        return Some(("tgf.SCPayUpdateDiamondRsp".to_string(), <crate::tgf::ScPayUpdateDiamondRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_direct_buy" => {
                if let Ok(req) = <crate::tgf::CsPayDirectBuyProductReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_direct_buy(req).await {
                        return Some(("tgf.SCPayDirectBuyProductRsp".to_string(), <crate::tgf::ScPayDirectBuyProductRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_auth_data_expired_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_auth_data_expired_sync(req).await {
                        return Some(("tgf.SCPayAuthDataExpiredSync".to_string(), <crate::tgf::ScPayAuthDataExpiredSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_check_bill_applied" => {
                if let Ok(req) = <crate::tgf::CsPayCheckBillAppliedReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_check_bill_applied(req).await {
                        return Some(("tgf.SCPayCheckBillAppliedRsp".to_string(), <crate::tgf::ScPayCheckBillAppliedRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_update_authorization_data" => {
                if let Ok(req) = <crate::tgf::CsPayUpdateAuthorizationDataReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_update_authorization_data(req).await {
                        return Some(("tgf.SCPayUpdateAuthorizationDataRsp".to_string(), <crate::tgf::ScPayUpdateAuthorizationDataRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_cancel" => {
                if let Ok(req) = <crate::tgf::CsPayCancelReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_cancel(req).await {
                        return Some(("tgf.SCPayCancelRsp".to_string(), <crate::tgf::ScPayCancelRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_exchange_free_diamond" => {
                if let Ok(req) = <crate::tgf::CsPayExchangeFreeDiamondReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_exchange_free_diamond(req).await {
                        return Some(("tgf.SCPayExchangeFreeDiamondRsp".to_string(), <crate::tgf::ScPayExchangeFreeDiamondRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_pending_flow_ntf" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_pending_flow_ntf(req).await {
                        return Some(("tgf.SCPayPendingFlowNtf".to_string(), <crate::tgf::ScPayPendingFlowNtf as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "pay_submit_pending_flow" => {
                if let Ok(req) = <crate::tgf::CsPaySubmitPendingFlowReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_pay_submit_pending_flow(req).await {
                        return Some(("tgf.SCPaySubmitPendingFlowRsp".to_string(), <crate::tgf::ScPaySubmitPendingFlowRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "inventory_item_move" => {
                if let Ok(req) = <crate::tgf::CsInventoryItemMoveReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_inventory_item_move(req).await {
                        return Some(("tgf.SCInventoryItemMoveRsp".to_string(), <crate::tgf::ScInventoryItemMoveRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "inventory_item_swap" => {
                if let Ok(req) = <crate::tgf::CsInventoryItemSwapReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_inventory_item_swap(req).await {
                        return Some(("tgf.SCInventoryItemSwapRsp".to_string(), <crate::tgf::ScInventoryItemSwapRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "inventory_page_add" => {
                if let Ok(req) = <crate::tgf::CsInventoryPageAddReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_inventory_page_add(req).await {
                        return Some(("tgf.SCInventoryPageAddRsp".to_string(), <crate::tgf::ScInventoryPageAddRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "use_item" => {
                if let Ok(req) = <crate::tgf::CsItemUseReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_use_item(req).await {
                        return Some(("tgf.SCItemUseRsp".to_string(), <crate::tgf::ScItemUseRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "use_item_bath" => {
                if let Ok(req) = <crate::tgf::CsBatchItemUseReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_use_item_bath(req).await {
                        return Some(("tgf.SCBatchItemUseRsp".to_string(), <crate::tgf::ScBatchItemUseRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "crafting" => {
                if let Ok(req) = <crate::tgf::CsCraftingReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_crafting(req).await {
                        return Some(("tgf.SCCraftingRsp".to_string(), <crate::tgf::ScCraftingRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "crafting_cancel" => {
                if let Ok(req) = <crate::tgf::CsCraftingCancelReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_crafting_cancel(req).await {
                        return Some(("tgf.SCCraftingCancelRsp".to_string(), <crate::tgf::ScCraftingCancelRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "crafting_recived" => {
                if let Ok(req) = <crate::tgf::CsCraftingRecivedReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_crafting_recived(req).await {
                        return Some(("tgf.SCCraftingRecivedRsp".to_string(), <crate::tgf::ScCraftingRecivedRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_product_price_basic_info" => {
                if let Ok(req) = <crate::tgf::CsTradeProductPriceBasicInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_product_price_basic_info(req).await {
                        return Some(("tgf.SCTradeProductPriceBasicInfoRsp".to_string(), <crate::tgf::ScTradeProductPriceBasicInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_product_price_info" => {
                if let Ok(req) = <crate::tgf::CsTradeProductPriceInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_product_price_info(req).await {
                        return Some(("tgf.SCTradeProductPriceInfoRsp".to_string(), <crate::tgf::ScTradeProductPriceInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_product_percentage_info" => {
                if let Ok(req) = <crate::tgf::CsTradeProductPercentageInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_product_percentage_info(req).await {
                        return Some(("tgf.SCTradeProductPercentageInfoRsp".to_string(), <crate::tgf::ScTradeProductPercentageInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_purchase_order_publish" => {
                if let Ok(req) = <crate::tgf::CsTradePurchaseOrderPublishReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_purchase_order_publish(req).await {
                        return Some(("tgf.SCTradePurchaseOrderPublishRsp".to_string(), <crate::tgf::ScTradePurchaseOrderPublishRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_sales_order_publish" => {
                if let Ok(req) = <crate::tgf::CsTradeSalesOrderPublishReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_sales_order_publish(req).await {
                        return Some(("tgf.SCTradeSalesOrderPublishRsp".to_string(), <crate::tgf::ScTradeSalesOrderPublishRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_bid_order_publish" => {
                if let Ok(req) = <crate::tgf::CsTradeBidOrderPublishReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_bid_order_publish(req).await {
                        return Some(("tgf.SCTradeBidOrderPublishRsp".to_string(), <crate::tgf::ScTradeBidOrderPublishRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_auction_order_publish" => {
                if let Ok(req) = <crate::tgf::CsTradeAuctionOrderPublishReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_auction_order_publish(req).await {
                        return Some(("tgf.SCTradeAuctionOrderPublishRsp".to_string(), <crate::tgf::ScTradeAuctionOrderPublishRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_auction_fetch_order_info" => {
                if let Ok(req) = <crate::tgf::CsTradeAuctionFetchOrderInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_auction_fetch_order_info(req).await {
                        return Some(("tgf.SCTradeAuctionFetchOrderInfoRsp".to_string(), <crate::tgf::ScTradeAuctionFetchOrderInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_auction_index" => {
                if let Ok(req) = <crate::tgf::CsTradeAuctionIndexReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_auction_index(req).await {
                        return Some(("tgf.SCTradeAuctionIndexRsp".to_string(), <crate::tgf::ScTradeAuctionIndexRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_order_cancel" => {
                if let Ok(req) = <crate::tgf::CsTradeOrderCancelReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_order_cancel(req).await {
                        return Some(("tgf.SCTradeOrderCancelRsp".to_string(), <crate::tgf::ScTradeOrderCancelRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_order_history" => {
                if let Ok(req) = <crate::tgf::CsTradeOrderHistoryReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_order_history(req).await {
                        return Some(("tgf.SCTradeOrderHistoryRsp".to_string(), <crate::tgf::ScTradeOrderHistoryRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_order_delete" => {
                if let Ok(req) = <crate::tgf::CsTradeOrderDeleteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_order_delete(req).await {
                        return Some(("tgf.SCTradeOrderDeleteRsp".to_string(), <crate::tgf::ScTradeOrderDeleteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_receive_confirmation_product" => {
                if let Ok(req) = <crate::tgf::CsTradeReceiveConfirmationProductReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_receive_confirmation_product(req).await {
                        return Some(("tgf.SCTradeReceiveConfirmationProductRsp".to_string(), <crate::tgf::ScTradeReceiveConfirmationProductRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_auction_recommend_order_ticket" => {
                if let Ok(req) = <crate::tgf::CsTradeAuctionRecommendOrderTicketReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_auction_recommend_order_ticket(req).await {
                        return Some(("tgf.SCTradeAuctionRecommendOrderTicketRsp".to_string(), <crate::tgf::ScTradeAuctionRecommendOrderTicketRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_product_recommend_order_ticket" => {
                if let Ok(req) = <crate::tgf::CsTradeProductRecommendOrderTicketReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_product_recommend_order_ticket(req).await {
                        return Some(("tgf.SCTradeProductRecommendOrderTicketRsp".to_string(), <crate::tgf::ScTradeProductRecommendOrderTicketRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_product_search_order_ticket" => {
                if let Ok(req) = <crate::tgf::CsTradeProductSearchOrderTicketReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_product_search_order_ticket(req).await {
                        return Some(("tgf.SCTradeProductSearchOrderTicketRsp".to_string(), <crate::tgf::ScTradeProductSearchOrderTicketRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "trade_career_rarity_affix_gear_set_price" => {
                if let Ok(req) = <crate::tgf::CsTradeCareerRarityAffixGearsetPriceReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_trade_career_rarity_affix_gear_set_price(req).await {
                        return Some(("tgf.SCTradeCareerRarityAffixGearsetPriceRsp".to_string(), <crate::tgf::ScTradeCareerRarityAffixGearsetPriceRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_repair" => {
                if let Ok(req) = <crate::tgf::CsGearRepairReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_repair(req).await {
                        return Some(("tgf.SCGearRepairRsp".to_string(), <crate::tgf::ScGearRepairRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_inlaid" => {
                if let Ok(req) = <crate::tgf::CsGearInlaidReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_inlaid(req).await {
                        return Some(("tgf.SCGearInlaidRsp".to_string(), <crate::tgf::ScGearInlaidRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_dismantle" => {
                if let Ok(req) = <crate::tgf::CsGearDismantleReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_dismantle(req).await {
                        return Some(("tgf.SCGearDismantleRsp".to_string(), <crate::tgf::ScGearDismantleRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_soul_bound" => {
                if let Ok(req) = <crate::tgf::CsGearSoulBoundReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_soul_bound(req).await {
                        return Some(("tgf.SCGearSoulBoundRsp".to_string(), <crate::tgf::ScGearSoulBoundRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_heirloom_unlock" => {
                if let Ok(req) = <crate::tgf::CsGearHeirloomUnlockReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_heirloom_unlock(req).await {
                        return Some(("tgf.SCGearHeirloomUnlockRsp".to_string(), <crate::tgf::ScGearHeirloomUnlockRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_heirloom_unload" => {
                if let Ok(req) = <crate::tgf::CsGearHeirloomUnloadReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_heirloom_unload(req).await {
                        return Some(("tgf.SCGearHeirloomUnloadRsp".to_string(), <crate::tgf::ScGearHeirloomUnloadRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_heirloom_instantiation" => {
                if let Ok(req) = <crate::tgf::CsGearHeirloomInstantiationReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_heirloom_instantiation(req).await {
                        return Some(("tgf.SCGearHeirloomInstantiationRsp".to_string(), <crate::tgf::ScGearHeirloomInstantiationRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_gem_unlock" => {
                if let Ok(req) = <crate::tgf::CsGearGemUnlockReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_gem_unlock(req).await {
                        return Some(("tgf.SCGearGemUnlockRsp".to_string(), <crate::tgf::ScGearGemUnlockRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_heirloom_guide_inlaid_gem" => {
                if let Ok(req) = <crate::tgf::CsGearHeirloomGuideInlaidGemReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_heirloom_guide_inlaid_gem(req).await {
                        return Some(("tgf.SCGearHeirloomGuideInlaidGemRsp".to_string(), <crate::tgf::ScGearHeirloomGuideInlaidGemRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_heirloom_guide_inlaid_clear" => {
                if let Ok(req) = <crate::tgf::CsGearHeirloomGuideInlaidClearReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_heirloom_guide_inlaid_clear(req).await {
                        return Some(("tgf.SCGearHeirloomGuideInlaidClearRsp".to_string(), <crate::tgf::ScGearHeirloomGuideInlaidClearRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_insurance_lock" => {
                if let Ok(req) = <crate::tgf::CsGearInsuranceLockReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_insurance_lock(req).await {
                        return Some(("tgf.SCGearInsuranceLockRsp".to_string(), <crate::tgf::ScGearInsuranceLockRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_insurance_unlock" => {
                if let Ok(req) = <crate::tgf::CsGearInsuranceUnLockReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_insurance_unlock(req).await {
                        return Some(("tgf.SCGearInsuranceUnLockRsp".to_string(), <crate::tgf::ScGearInsuranceUnLockRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "gear_insurance_price" => {
                if let Ok(req) = <crate::tgf::CsGearInsurancePriceReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_gear_insurance_price(req).await {
                        return Some(("tgf.SCGearInsurancePriceRsp".to_string(), <crate::tgf::ScGearInsurancePriceRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "facade_update" => {
                if let Ok(req) = <crate::tgf::CsFacadeSetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_facade_update(req).await {
                        return Some(("tgf.SCFacadeSetRsp".to_string(), <crate::tgf::ScFacadeSetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "facade_update_role_skin" => {
                if let Ok(req) = <crate::tgf::CsFacadeUpdateRoleSkinReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_facade_update_role_skin(req).await {
                        return Some(("tgf.SCFacadeUpdateRoleSkinRsp".to_string(), <crate::tgf::ScFacadeUpdateRoleSkinRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "facade_save_skin_template" => {
                if let Ok(req) = <crate::tgf::CsFacadeSaveSkinTemplateReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_facade_save_skin_template(req).await {
                        return Some(("tgf.SCFacadeSaveSkinTemplateRsp".to_string(), <crate::tgf::ScFacadeSaveSkinTemplateRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "facade_delete_skin_template" => {
                if let Ok(req) = <crate::tgf::CsFacadeDeleteSkinTemplateReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_facade_delete_skin_template(req).await {
                        return Some(("tgf.SCFacadeDeleteSkinTemplateRsp".to_string(), <crate::tgf::ScFacadeDeleteSkinTemplateRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "facade_get_on_sale_facade" => {
                if let Ok(req) = <crate::tgf::CsFacadeGetOnSaleFacadeReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_facade_get_on_sale_facade(req).await {
                        return Some(("tgf.SCFacadeGetOnSaleFacadeRsp".to_string(), <crate::tgf::ScFacadeGetOnSaleFacadeRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_list" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterListReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_list(req).await {
                        return Some(("tgf.SCActivityCenterListRsp".to_string(), <crate::tgf::ScActivityCenterListRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_random_rewards" => {
                if let Ok(req) = <crate::tgf::CsActivityRandomRewardReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_random_rewards(req).await {
                        return Some(("tgf.SCActivityRandomRewardRsp".to_string(), <crate::tgf::ScActivityRandomRewardRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_random_reward_selected" => {
                if let Ok(req) = <crate::tgf::CsActivityRandomRewardSelectdReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_random_reward_selected(req).await {
                        return Some(("tgf.SCActivityRandomRewardSelectdRsp".to_string(), <crate::tgf::ScActivityRandomRewardSelectdRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_buy_passport" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterBuyPassportReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_buy_passport(req).await {
                        return Some(("tgf.SCActivityCenterBuyPassportRsp".to_string(), <crate::tgf::ScActivityCenterBuyPassportRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_passport_receive_rewards" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterPassportReceiveRewardsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_passport_receive_rewards(req).await {
                        return Some(("tgf.SCActivityCenterPassportReceiveRewardsRsp".to_string(), <crate::tgf::ScActivityCenterPassportReceiveRewardsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_passport_receive_cycle_rewards" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterPassportReceiveCycleRewardsReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_passport_receive_cycle_rewards(req).await {
                        return Some(("tgf.SCActivityCenterPassportReceiveCycleRewardsRsp".to_string(), <crate::tgf::ScActivityCenterPassportReceiveCycleRewardsRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_passport_refresh_quest" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterPassportRefreshQuestReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_passport_refresh_quest(req).await {
                        return Some(("tgf.SCActivityCenterPassportRefreshQuestRsp".to_string(), <crate::tgf::ScActivityCenterPassportRefreshQuestRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_trial_open" => {
                if let Ok(req) = <crate::tgf::CsActivityCenteTrialOpenReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_trial_open(req).await {
                        return Some(("tgf.SCActivityCenterTrialOpenRsp".to_string(), <crate::tgf::ScActivityCenterTrialOpenRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_trial_self_quit" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterTrialSelfQuitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_trial_self_quit(req).await {
                        return Some(("tgf.SCActivityCenterTrialSelfQuitRsp".to_string(), <crate::tgf::ScActivityCenterTrialSelfQuitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_trial_receive_reward" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterTrialReceiveRewardReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_trial_receive_reward(req).await {
                        return Some(("tgf.SCActivityCenterTrialReceiveRewardRsp".to_string(), <crate::tgf::ScActivityCenterTrialReceiveRewardRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_trial_history_record" => {
                if let Ok(req) = <crate::tgf::CsActivityCenterTrialHistoryRecordReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_trial_history_record(req).await {
                        return Some(("tgf.SCActivityCenterTrialHistoryRecordRsp".to_string(), <crate::tgf::ScActivityCenterTrialHistoryRecordRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "activity_center_trial_record_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_activity_center_trial_record_sync(req).await {
                        return Some(("tgf.SCActivityCenterTrialRecordSync".to_string(), <crate::tgf::ScActivityCenterTrialRecordSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_create" => {
                if let Ok(req) = <crate::tgf::CsRoomCreateReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_create(req).await {
                        return Some(("tgf.SCRoomCreateRsp".to_string(), <crate::tgf::ScRoomCreateRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_enter" => {
                if let Ok(req) = <crate::tgf::CsRoomEnterReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_enter(req).await {
                        return Some(("tgf.SCRoomEnterRsp".to_string(), <crate::tgf::ScRoomEnterRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_leave" => {
                if let Ok(req) = <crate::tgf::CsRoomLeaveReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_leave(req).await {
                        return Some(("tgf.SCRoomLeaveRsp".to_string(), <crate::tgf::ScRoomLeaveRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_disband" => {
                if let Ok(req) = <crate::tgf::CsRoomDisbandReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_disband(req).await {
                        return Some(("tgf.SCRoomDisbandRsp".to_string(), <crate::tgf::ScRoomDisbandRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_change_myself_pos" => {
                if let Ok(req) = <crate::tgf::CsRoomChangeMySelfPosReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_change_myself_pos(req).await {
                        return Some(("tgf.SCRoomChangeMySelfPosRsp".to_string(), <crate::tgf::ScRoomChangeMySelfPosRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_change_member_pos" => {
                if let Ok(req) = <crate::tgf::CsRoomChangeMemberPosReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_change_member_pos(req).await {
                        return Some(("tgf.SCRoomChangeMemberPosRsp".to_string(), <crate::tgf::ScRoomChangeMemberPosRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_select_battle_map" => {
                if let Ok(req) = <crate::tgf::CsRoomSelectBattleMapReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_select_battle_map(req).await {
                        return Some(("tgf.SCRoomSelectBattleMapRsp".to_string(), <crate::tgf::ScRoomSelectBattleMapRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_select_battle_type" => {
                if let Ok(req) = <crate::tgf::CsRoomSelectBattleTypeReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_select_battle_type(req).await {
                        return Some(("tgf.SCRoomSelectBattleTypeRsp".to_string(), <crate::tgf::ScRoomSelectBattleTypeRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_get_ready" => {
                if let Ok(req) = <crate::tgf::CsRoomGetReadyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_get_ready(req).await {
                        return Some(("tgf.SCRoomGetReadyRsp".to_string(), <crate::tgf::ScRoomGetReadyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_cancel_ready" => {
                if let Ok(req) = <crate::tgf::CsRoomCancelReadyReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_cancel_ready(req).await {
                        return Some(("tgf.SCRoomCancelReadyRsp".to_string(), <crate::tgf::ScRoomCancelReadyRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_start_battle" => {
                if let Ok(req) = <crate::tgf::CsRoomStartBattleReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_start_battle(req).await {
                        return Some(("tgf.SCRoomStartBattleRsp".to_string(), <crate::tgf::ScRoomStartBattleRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_name_change" => {
                if let Ok(req) = <crate::tgf::CsRoomNameChangeReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_name_change(req).await {
                        return Some(("tgf.SCRoomNameChangeRsp".to_string(), <crate::tgf::ScRoomNameChangeRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_keyword_change" => {
                if let Ok(req) = <crate::tgf::CsRoomKeywordChangeReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_keyword_change(req).await {
                        return Some(("tgf.SCRoomKeywordChangeRsp".to_string(), <crate::tgf::ScRoomKeywordChangeRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_transfer_owner" => {
                if let Ok(req) = <crate::tgf::CsRoomTransferOwnerReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_transfer_owner(req).await {
                        return Some(("tgf.SCRoomTransferOwnerRsp".to_string(), <crate::tgf::ScRoomTransferOwnerRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_kickoff_member" => {
                if let Ok(req) = <crate::tgf::CsRoomKickOffMemberReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_kickoff_member(req).await {
                        return Some(("tgf.SCRoomKickOffMemberRsp".to_string(), <crate::tgf::ScRoomKickOffMemberRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_suggest" => {
                if let Ok(req) = <crate::tgf::CsRoomSuggestReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_suggest(req).await {
                        return Some(("tgf.SCRoomSuggestRsp".to_string(), <crate::tgf::ScRoomSuggestRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_batch_get" => {
                if let Ok(req) = <crate::tgf::CsRoomBatchGetReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_batch_get(req).await {
                        return Some(("tgf.SCRoomBatchGetRsp".to_string(), <crate::tgf::ScRoomBatchGetRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_accept_invite" => {
                if let Ok(req) = <crate::tgf::CsRoomInviteAcceptReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_accept_invite(req).await {
                        return Some(("tgf.SCRoomInviteAcceptRsp".to_string(), <crate::tgf::ScRoomInviteAcceptRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_send_invite" => {
                if let Ok(req) = <crate::tgf::CsRoomInviteSendReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_send_invite(req).await {
                        return Some(("tgf.SCRoomInviteSendRsp".to_string(), <crate::tgf::ScRoomInviteSendRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_reject_invite" => {
                if let Ok(req) = <crate::tgf::CsRoomInviteRejectReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_reject_invite(req).await {
                        return Some(("tgf.SCRoomInviteRejectRsp".to_string(), <crate::tgf::ScRoomInviteRejectRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_ignore_room_all_invite" => {
                if let Ok(req) = <crate::tgf::CsRoomIgnoreRoomAllInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_ignore_room_all_invite(req).await {
                        return Some(("tgf.SCRoomIgnoreRoomAllInviteRsp".to_string(), <crate::tgf::ScRoomIgnoreRoomAllInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "room_ignore_all_user_invite" => {
                if let Ok(req) = <crate::tgf::CsRoomIgnoreAllUserInviteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_room_ignore_all_user_invite(req).await {
                        return Some(("tgf.SCRoomIgnoreAllUserInviteRsp".to_string(), <crate::tgf::ScRoomIgnoreAllUserInviteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "hope_instruct_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hope_instruct_sync(req).await {
                        return Some(("tgf.SCHopeInstructionSync".to_string(), <crate::tgf::ScHopeInstructionSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "hope_certification_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hope_certification_sync(req).await {
                        return Some(("tgf.SCHopeQueryCertificationSync".to_string(), <crate::tgf::ScHopeQueryCertificationSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "hope_report_execute" => {
                if let Ok(req) = <crate::tgf::CsHopeReportExecuteReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_hope_report_execute(req).await {
                        return Some(("tgf.SCHopeReportExecuteRsp".to_string(), <crate::tgf::ScHopeReportExecuteRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mithril_force_role_init" => {
                if let Ok(req) = <crate::tgf::CsMithrilForceRoleInitReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mithril_force_role_init(req).await {
                        return Some(("tgf.SCMithrilForceRoleInitRsp".to_string(), <crate::tgf::ScMithrilForceRoleInitRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mithril_force_role_select" => {
                if let Ok(req) = <crate::tgf::CsMithrilForceRoleSelectReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mithril_force_role_select(req).await {
                        return Some(("tgf.SCMithrilForceRoleSelectRsp".to_string(), <crate::tgf::ScMithrilForceRoleSelectRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mithril_force_receive_reward" => {
                if let Ok(req) = <crate::tgf::CsMithrilForceReceiveRewardReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mithril_force_receive_reward(req).await {
                        return Some(("tgf.SCMithrilForceReceiveRewardRsp".to_string(), <crate::tgf::ScMithrilForceReceiveRewardRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "mithril_role_wear" => {
                if let Ok(req) = <crate::tgf::CsMithrilRoleWearReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_mithril_role_wear(req).await {
                        return Some(("tgf.SCMithrilRoleWearRsp".to_string(), <crate::tgf::ScMithrilRoleWearRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "dlc_packet_sync" => {
                if let Ok(req) = <crate::google::protobuf::Empty as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_dlc_packet_sync(req).await {
                        return Some(("tgf.SCNewDLCPacketSync".to_string(), <crate::tgf::ScNewDlcPacketSync as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "dlc_receive_packet" => {
                if let Ok(req) = <crate::tgf::CsReceiveDlcPacketReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_dlc_receive_packet(req).await {
                        return Some(("tgf.SCReceiveDLCPacketRsp".to_string(), <crate::tgf::ScReceiveDlcPacketRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

            "refresh_dlc_info" => {
                if let Ok(req) = <crate::tgf::CsRefreshDlcInfoReq as ::prost::Message>::decode(payload) {
                    if let Some(rsp) = handler.handle_refresh_dlc_info(req).await {
                        return Some(("tgf.SCRefreshDLCInfoRsp".to_string(), <crate::tgf::ScRefreshDlcInfoRsp as ::prost::Message>::encode_to_vec(&rsp)));
                    }
                }
            }

        _ => {}
    }
    None
}

pub struct GameData {
    pub activity_center: std::collections::HashMap<i32, crate::tgf::config::ExcelActivityCenter>,
    /// All rows of activity_center.bytes (key `id` may repeat)
    pub activity_center_rows: Vec<crate::tgf::config::ExcelActivityCenter>,
    pub activity_center_passprots_rewards: std::collections::HashMap<i32, crate::tgf::config::ExcelActivityCenterPassprotsReards>,
    /// All rows of activity_center_passprots_rewards.bytes (key `id` may repeat)
    pub activity_center_passprots_rewards_rows: Vec<crate::tgf::config::ExcelActivityCenterPassprotsReards>,
    pub activity_center_random_rewards: std::collections::HashMap<i32, crate::tgf::config::ExcelActivityCenterRandomRewards>,
    /// All rows of activity_center_random_rewards.bytes (key `id` may repeat)
    pub activity_center_random_rewards_rows: Vec<crate::tgf::config::ExcelActivityCenterRandomRewards>,
    pub activity_center_random_rewards_pool: std::collections::HashMap<i32, crate::tgf::config::ExcelActivityCenterRandomRewardsPool>,
    /// All rows of activity_center_random_rewards_pool.bytes (key `id` may repeat)
    pub activity_center_random_rewards_pool_rows: Vec<crate::tgf::config::ExcelActivityCenterRandomRewardsPool>,
    pub affix_ability_set_id: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixAbilitySetId>,
    /// All rows of affix_ability_set_id.bytes (key `affix_index_id` may repeat)
    pub affix_ability_set_id_rows: Vec<crate::tgf::config::ExcelAffixAbilitySetId>,
    pub affix_count_random_pool: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixCountRandomPool>,
    /// All rows of affix_count_random_pool.bytes (key `affix_count_pool_id` may repeat)
    pub affix_count_random_pool_rows: Vec<crate::tgf::config::ExcelAffixCountRandomPool>,
    pub affix_gear_set_map_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixGearSetMapRule>,
    /// All rows of affix_gear_set_map_rule.bytes (key `equip_id` may repeat)
    pub affix_gear_set_map_rule_rows: Vec<crate::tgf::config::ExcelAffixGearSetMapRule>,
    pub affix_gear_set_rule: Vec<crate::tgf::config::ExcelAffixGearSetRule>,
    pub affix_legendary_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixLegendaryRule>,
    /// All rows of affix_legendary_rule.bytes (key `affix_alias_id` may repeat)
    pub affix_legendary_rule_rows: Vec<crate::tgf::config::ExcelAffixLegendaryRule>,
    pub affix_level_pool_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixLevelPoolRule>,
    /// All rows of affix_level_pool_rule.bytes (key `rule_id` may repeat)
    pub affix_level_pool_rule_rows: Vec<crate::tgf::config::ExcelAffixLevelPoolRule>,
    pub affix_monster_pool_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixMonsterPoolRule>,
    /// All rows of affix_monster_pool_rule.bytes (key `rule_id` may repeat)
    pub affix_monster_pool_rule_rows: Vec<crate::tgf::config::ExcelAffixMonsterPoolRule>,
    pub affix_pool: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixPool>,
    /// All rows of affix_pool.bytes (key `affix_pool_id` may repeat)
    pub affix_pool_rows: Vec<crate::tgf::config::ExcelAffixPool>,
    pub affix_template: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixTemplate>,
    /// All rows of affix_template.bytes (key `item_id` may repeat)
    pub affix_template_rows: Vec<crate::tgf::config::ExcelAffixTemplate>,
    pub affix_value_template: std::collections::HashMap<i32, crate::tgf::config::ExcelAffixValueTemplate>,
    /// All rows of affix_value_template.bytes (key `affix_value_id` may repeat)
    pub affix_value_template_rows: Vec<crate::tgf::config::ExcelAffixValueTemplate>,
    pub battle_ability_set_description: std::collections::HashMap<i32, crate::tgf::config::ExcelBattleAbilitySetDescription>,
    /// All rows of battle_ability_set_description.bytes (key `ability_set_id` may repeat)
    pub battle_ability_set_description_rows: Vec<crate::tgf::config::ExcelBattleAbilitySetDescription>,
    pub battle_affix_effect_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelBattleAffixEffectRule>,
    /// All rows of battle_affix_effect_rule.bytes (key `id` may repeat)
    pub battle_affix_effect_rule_rows: Vec<crate::tgf::config::ExcelBattleAffixEffectRule>,
    pub battle_attribute_description: Vec<crate::tgf::config::ExcelBattleAttributeDescription>,
    pub chat_channel_type: Vec<crate::tgf::config::ExcelChatChannelType>,
    pub competition_level: std::collections::HashMap<i32, crate::tgf::config::ExcelCompetitionLevel>,
    /// All rows of competition_level.bytes (key `competition_level_id` may repeat)
    pub competition_level_rows: Vec<crate::tgf::config::ExcelCompetitionLevel>,
    pub r#const: Vec<crate::tgf::config::ExcelOriginConstConfig>,
    pub craft_group: std::collections::HashMap<i32, crate::tgf::config::ExcelCraftingGroup>,
    /// All rows of craft_group.bytes (key `group_id` may repeat)
    pub craft_group_rows: Vec<crate::tgf::config::ExcelCraftingGroup>,
    pub craft_table: std::collections::HashMap<i32, crate::tgf::config::ExcelCraftingRecipe>,
    /// All rows of craft_table.bytes (key `id` may repeat)
    pub craft_table_rows: Vec<crate::tgf::config::ExcelCraftingRecipe>,
    pub craft_tag: std::collections::HashMap<i32, crate::tgf::config::ExcelCraftingTag>,
    /// All rows of craft_tag.bytes (key `tag_id` may repeat)
    pub craft_tag_rows: Vec<crate::tgf::config::ExcelCraftingTag>,
    pub cuver_table_source: std::collections::HashMap<i32, crate::tgf::config::ExcelCuverTableSource>,
    /// All rows of cuver_table_source.bytes (key `id` may repeat)
    pub cuver_table_source_rows: Vec<crate::tgf::config::ExcelCuverTableSource>,
    pub dlc_content: std::collections::HashMap<i32, crate::tgf::config::ExcelDlcContent>,
    /// All rows of dlc_content.bytes (key `app_id` may repeat)
    pub dlc_content_rows: Vec<crate::tgf::config::ExcelDlcContent>,
    pub dlc_packet: std::collections::HashMap<i32, crate::tgf::config::ExcelDlcPacket>,
    /// All rows of dlc_packet.bytes (key `app_id` may repeat)
    pub dlc_packet_rows: Vec<crate::tgf::config::ExcelDlcPacket>,
    pub error_code: Vec<crate::tgf::config::ExcelErrorCode>,
    pub excel_001: std::collections::HashMap<i32, crate::tgf::config::ExcelLootRarePoolCntLimit>,
    /// All rows of excel_001.bytes (key `item_id` may repeat)
    pub excel_001_rows: Vec<crate::tgf::config::ExcelLootRarePoolCntLimit>,
    pub excel_002: std::collections::HashMap<i64, crate::tgf::config::ExcelLootRarePoolItemRate>,
    /// All rows of excel_002.bytes (key `random_id` may repeat)
    pub excel_002_rows: Vec<crate::tgf::config::ExcelLootRarePoolItemRate>,
    pub facade: std::collections::HashMap<i32, crate::tgf::config::ExcelFacade>,
    /// All rows of facade.bytes (key `id` may repeat)
    pub facade_rows: Vec<crate::tgf::config::ExcelFacade>,
    pub facade_sort: std::collections::HashMap<i32, crate::tgf::config::ExcelFacadeSort>,
    /// All rows of facade_sort.bytes (key `original_resource_id` may repeat)
    pub facade_sort_rows: Vec<crate::tgf::config::ExcelFacadeSort>,
    pub gear_base_item: std::collections::HashMap<i32, crate::tgf::config::ExcelGearBaseItem>,
    /// All rows of gear_base_item.bytes (key `id` may repeat)
    pub gear_base_item_rows: Vec<crate::tgf::config::ExcelGearBaseItem>,
    pub gear_dismantle: std::collections::HashMap<i32, crate::tgf::config::ExcelGearDismantle>,
    /// All rows of gear_dismantle.bytes (key `item_id` may repeat)
    pub gear_dismantle_rows: Vec<crate::tgf::config::ExcelGearDismantle>,
    pub gear_durability: Vec<crate::tgf::config::ExcelGearDurability>,
    pub gear_gem_guide: std::collections::HashMap<i32, crate::tgf::config::ExcelGearGemGuide>,
    /// All rows of gear_gem_guide.bytes (key `id` may repeat)
    pub gear_gem_guide_rows: Vec<crate::tgf::config::ExcelGearGemGuide>,
    pub gear_gem_inst: std::collections::HashMap<i32, crate::tgf::config::ExcelGearGemInst>,
    /// All rows of gear_gem_inst.bytes (key `item_id` may repeat)
    pub gear_gem_inst_rows: Vec<crate::tgf::config::ExcelGearGemInst>,
    pub gear_heirloom_guide: std::collections::HashMap<i32, crate::tgf::config::ExcelGearHeirloomGuide>,
    /// All rows of gear_heirloom_guide.bytes (key `id` may repeat)
    pub gear_heirloom_guide_rows: Vec<crate::tgf::config::ExcelGearHeirloomGuide>,
    pub gear_insurance_item: std::collections::HashMap<i32, crate::tgf::config::ExcelGearInsuranceItem>,
    /// All rows of gear_insurance_item.bytes (key `id` may repeat)
    pub gear_insurance_item_rows: Vec<crate::tgf::config::ExcelGearInsuranceItem>,
    pub gear_soul_bound_cost: Vec<crate::tgf::config::ExcelGearSoulBoundCost>,
    pub gear_soul_bound_wear_part_random: std::collections::HashMap<i32, crate::tgf::config::ExcelGearSoulBoundWearPartRandom>,
    /// All rows of gear_soul_bound_wear_part_random.bytes (key `group_id` may repeat)
    pub gear_soul_bound_wear_part_random_rows: Vec<crate::tgf::config::ExcelGearSoulBoundWearPartRandom>,
    pub gear_template: std::collections::HashMap<i32, crate::tgf::config::ExcelGearTemplate>,
    /// All rows of gear_template.bytes (key `item_id` may repeat)
    pub gear_template_rows: Vec<crate::tgf::config::ExcelGearTemplate>,
    pub immaculate_trial_grade: std::collections::HashMap<i32, crate::tgf::config::ExcelImmaculateTrialGrade>,
    /// All rows of immaculate_trial_grade.bytes (key `grade_id` may repeat)
    pub immaculate_trial_grade_rows: Vec<crate::tgf::config::ExcelImmaculateTrialGrade>,
    pub immaculate_trial_random_rewards: std::collections::HashMap<i32, crate::tgf::config::ExcelImmaculateTrialRandomRewards>,
    /// All rows of immaculate_trial_random_rewards.bytes (key `random_reward_id` may repeat)
    pub immaculate_trial_random_rewards_rows: Vec<crate::tgf::config::ExcelImmaculateTrialRandomRewards>,
    pub immaculate_trial_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelImmaculateTrialRule>,
    /// All rows of immaculate_trial_rule.bytes (key `activity_rule_id` may repeat)
    pub immaculate_trial_rule_rows: Vec<crate::tgf::config::ExcelImmaculateTrialRule>,
    pub item_router: Vec<crate::tgf::config::ExcelItemRouter>,
    pub item_type: std::collections::HashMap<i32, crate::tgf::config::ExcelItemType>,
    /// All rows of item_type.bytes (key `type_id` may repeat)
    pub item_type_rows: Vec<crate::tgf::config::ExcelItemType>,
    pub level_type: std::collections::HashMap<i32, crate::tgf::config::ExcelLevelType>,
    /// All rows of level_type.bytes (key `match_making_pool_id` may repeat)
    pub level_type_rows: Vec<crate::tgf::config::ExcelLevelType>,
    pub loot: std::collections::HashMap<i32, crate::tgf::config::ExcelItemDropItem>,
    /// All rows of loot.bytes (key `item_id` may repeat)
    pub loot_rows: Vec<crate::tgf::config::ExcelItemDropItem>,
    pub loot_choose_group_rate: std::collections::HashMap<i64, crate::tgf::config::ExcelLootChooseGroupRate>,
    /// All rows of loot_choose_group_rate.bytes (key `loot_id` may repeat)
    pub loot_choose_group_rate_rows: Vec<crate::tgf::config::ExcelLootChooseGroupRate>,
    pub loot_disable_loot_id: std::collections::HashMap<i64, crate::tgf::config::ExcelLootDisableLootId>,
    /// All rows of loot_disable_loot_id.bytes (key `loot_id` may repeat)
    pub loot_disable_loot_id_rows: Vec<crate::tgf::config::ExcelLootDisableLootId>,
    pub loot_magic_found: Vec<crate::tgf::config::ExcelItemMagicFound>,
    pub loot_rate: Vec<crate::tgf::config::ExcelItemLootRate>,
    pub loot_source_map: std::collections::HashMap<i32, crate::tgf::config::ExcelLootSourceMap>,
    /// All rows of loot_source_map.bytes (key `id` may repeat)
    pub loot_source_map_rows: Vec<crate::tgf::config::ExcelLootSourceMap>,
    pub loot_ue_source_choose_group: std::collections::HashMap<i32, crate::tgf::config::ExcelLootUeSourceChooseGroup>,
    /// All rows of loot_ue_source_choose_group.bytes (key `ue_loot_source_id` may repeat)
    pub loot_ue_source_choose_group_rows: Vec<crate::tgf::config::ExcelLootUeSourceChooseGroup>,
    pub loot_unlucky_value: Vec<crate::tgf::config::ExcelLootUnluckyValue>,
    pub lottery_access_key: std::collections::HashMap<i32, crate::tgf::config::ExcelLotteryAccessKey>,
    /// All rows of lottery_access_key.bytes (key `type_id` may repeat)
    pub lottery_access_key_rows: Vec<crate::tgf::config::ExcelLotteryAccessKey>,
    pub lottery_pool: std::collections::HashMap<i32, crate::tgf::config::ExcelLotteryPool>,
    /// All rows of lottery_pool.bytes (key `id` may repeat)
    pub lottery_pool_rows: Vec<crate::tgf::config::ExcelLotteryPool>,
    pub mail_template: std::collections::HashMap<i32, crate::tgf::config::ExcelMailTemplate>,
    /// All rows of mail_template.bytes (key `type_id` may repeat)
    pub mail_template_rows: Vec<crate::tgf::config::ExcelMailTemplate>,
    pub map_detail: std::collections::HashMap<i32, crate::tgf::config::ExcelMapDetail>,
    /// All rows of map_detail.bytes (key `map_id` may repeat)
    pub map_detail_rows: Vec<crate::tgf::config::ExcelMapDetail>,
    pub map_exp: std::collections::HashMap<i32, crate::tgf::config::ExcelMapExp>,
    /// All rows of map_exp.bytes (key `map_id` may repeat)
    pub map_exp_rows: Vec<crate::tgf::config::ExcelMapExp>,
    pub map_select: std::collections::HashMap<String, crate::tgf::config::ExcelMapSelect>,
    /// All rows of map_select.bytes (key `map_name_id` may repeat)
    pub map_select_rows: Vec<crate::tgf::config::ExcelMapSelect>,
    pub map_select_model_feature: std::collections::HashMap<i32, crate::tgf::config::ExcelMapSelectModelFeature>,
    /// All rows of map_select_model_feature.bytes (key `model_feature_id` may repeat)
    pub map_select_model_feature_rows: Vec<crate::tgf::config::ExcelMapSelectModelFeature>,
    pub market_info: Vec<crate::tgf::config::ExcelMarketInfo>,
    pub market_poor_man: Vec<crate::tgf::config::ExcelMarketPoorMan>,
    pub market_popup_video: Vec<crate::tgf::config::ExcelMarketPopupVideo>,
    pub market_product_item: Vec<crate::tgf::config::ExcelMarketProductItem>,
    pub market_refresh_pool: Vec<crate::tgf::config::ExcelMarketRefreshPool>,
    pub market_special_cost: Vec<crate::tgf::config::ExcelMarketSpecialCost>,
    pub market_tab_info: Vec<crate::tgf::config::ExcelMarketTabInfo>,
    pub mithril_force_item: std::collections::HashMap<i32, crate::tgf::config::ExcelMithrilForceItem>,
    /// All rows of mithril_force_item.bytes (key `id` may repeat)
    pub mithril_force_item_rows: Vec<crate::tgf::config::ExcelMithrilForceItem>,
    pub mithril_force_prestige: Vec<crate::tgf::config::ExcelMithrilForcePrestige>,
    pub mithril_force_settlement: Vec<crate::tgf::config::ExcelMithrilForceSettlement>,
    pub mithril_force_settlement_factor: Vec<crate::tgf::config::ExcelMithrilForceSettlementFactor>,
    pub module_unlock_circular: std::collections::HashMap<i32, crate::tgf::config::ExcelModuleUnlockCircular>,
    /// All rows of module_unlock_circular.bytes (key `id` may repeat)
    pub module_unlock_circular_rows: Vec<crate::tgf::config::ExcelModuleUnlockCircular>,
    pub module_unlock_type: std::collections::HashMap<i32, crate::tgf::config::ExcelModuleUnlockType>,
    /// All rows of module_unlock_type.bytes (key `module_id` may repeat)
    pub module_unlock_type_rows: Vec<crate::tgf::config::ExcelModuleUnlockType>,
    pub quest_group_list: std::collections::HashMap<i32, crate::tgf::config::ExcelQuestGroupList>,
    /// All rows of quest_group_list.bytes (key `id` may repeat)
    pub quest_group_list_rows: Vec<crate::tgf::config::ExcelQuestGroupList>,
    pub quest_list: std::collections::HashMap<i32, crate::tgf::config::ExcelQuestList>,
    /// All rows of quest_list.bytes (key `id` may repeat)
    pub quest_list_rows: Vec<crate::tgf::config::ExcelQuestList>,
    pub quest_progress_condition: std::collections::HashMap<i32, crate::tgf::config::ExcelQuestProgressCondition>,
    /// All rows of quest_progress_condition.bytes (key `id` may repeat)
    pub quest_progress_condition_rows: Vec<crate::tgf::config::ExcelQuestProgressCondition>,
    pub quest_progress_type: std::collections::HashMap<i32, crate::tgf::config::ExcelQuestProgressType>,
    /// All rows of quest_progress_type.bytes (key `id` may repeat)
    pub quest_progress_type_rows: Vec<crate::tgf::config::ExcelQuestProgressType>,
    pub quest_trigger_event_type: Vec<crate::tgf::config::ExcelQuestTriggerEventType>,
    pub rank_period_reward_pool: std::collections::HashMap<i32, crate::tgf::config::ExcelRankPeriodRewardPool>,
    /// All rows of rank_period_reward_pool.bytes (key `rank_reward_pool_id` may repeat)
    pub rank_period_reward_pool_rows: Vec<crate::tgf::config::ExcelRankPeriodRewardPool>,
    pub rank_report_setting: std::collections::HashMap<u32, crate::tgf::config::ExcelRankReportSetting>,
    /// All rows of rank_report_setting.bytes (key `rank_instance_id` may repeat)
    pub rank_report_setting_rows: Vec<crate::tgf::config::ExcelRankReportSetting>,
    pub rank_rule: std::collections::HashMap<u32, crate::tgf::config::ExcelRankRule>,
    /// All rows of rank_rule.bytes (key `rank_instance_id` may repeat)
    pub rank_rule_rows: Vec<crate::tgf::config::ExcelRankRule>,
    pub role_default_equipment: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleDefaultEquipment>,
    /// All rows of role_default_equipment.bytes (key `config_id` may repeat)
    pub role_default_equipment_rows: Vec<crate::tgf::config::ExcelRoleDefaultEquipment>,
    pub role_face_pinching_limit: Vec<crate::tgf::config::ExcelRoleFacePinchingLimit>,
    pub role_head_icon: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleHeadIcon>,
    /// All rows of role_head_icon.bytes (key `type_id` may repeat)
    pub role_head_icon_rows: Vec<crate::tgf::config::ExcelRoleHeadIcon>,
    pub role_level_exp: Vec<crate::tgf::config::ExcelRoleLevelExp>,
    pub role_level_upgrade: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleLevelUpgrade>,
    /// All rows of role_level_upgrade.bytes (key `role_type_id` may repeat)
    pub role_level_upgrade_rows: Vec<crate::tgf::config::ExcelRoleLevelUpgrade>,
    pub role_skill: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleSkill>,
    /// All rows of role_skill.bytes (key `skill_id` may repeat)
    pub role_skill_rows: Vec<crate::tgf::config::ExcelRoleSkill>,
    pub role_skin_type: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleSkinType>,
    /// All rows of role_skin_type.bytes (key `skin_id` may repeat)
    pub role_skin_type_rows: Vec<crate::tgf::config::ExcelRoleSkinType>,
    pub role_type: std::collections::HashMap<i32, crate::tgf::config::ExcelRoleType>,
    /// All rows of role_type.bytes (key `type_id` may repeat)
    pub role_type_rows: Vec<crate::tgf::config::ExcelRoleType>,
    pub season_rule: std::collections::HashMap<i32, crate::tgf::config::ExcelSeasonRule>,
    /// All rows of season_rule.bytes (key `season_id` may repeat)
    pub season_rule_rows: Vec<crate::tgf::config::ExcelSeasonRule>,
    pub skill_calculate: std::collections::HashMap<String, crate::tgf::config::ExcelSkillCalculate>,
    /// All rows of skill_calculate.bytes (key `id` may repeat)
    pub skill_calculate_rows: Vec<crate::tgf::config::ExcelSkillCalculate>,
    pub skill_num_exchange: std::collections::HashMap<String, crate::tgf::config::ExcelSkillNumExchange>,
    /// All rows of skill_num_exchange.bytes (key `id` may repeat)
    pub skill_num_exchange_rows: Vec<crate::tgf::config::ExcelSkillNumExchange>,
    pub tiers: std::collections::HashMap<i32, crate::tgf::config::ExcelTiers>,
    /// All rows of tiers.bytes (key `tiers_id` may repeat)
    pub tiers_rows: Vec<crate::tgf::config::ExcelTiers>,
    pub tiers_kill_plyaer_rule: Vec<crate::tgf::config::ExcelTiersKillPlyaerRule>,
    pub tiers_show_group: std::collections::HashMap<i32, crate::tgf::config::ExcelTiersShowGroup>,
    /// All rows of tiers_show_group.bytes (key `type_id` may repeat)
    pub tiers_show_group_rows: Vec<crate::tgf::config::ExcelTiersShowGroup>,
    pub trade_commodity: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeCommodity>,
    /// All rows of trade_commodity.bytes (key `id` may repeat)
    pub trade_commodity_rows: Vec<crate::tgf::config::ExcelTradeCommodity>,
    pub trade_item_require: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeItemRequire>,
    /// All rows of trade_item_require.bytes (key `id` may repeat)
    pub trade_item_require_rows: Vec<crate::tgf::config::ExcelTradeItemRequire>,
    pub trade_market: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeMarket>,
    /// All rows of trade_market.bytes (key `id` may repeat)
    pub trade_market_rows: Vec<crate::tgf::config::ExcelTradeMarket>,
    pub trade_standard_price: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeStandardPrice>,
    /// All rows of trade_standard_price.bytes (key `id` may repeat)
    pub trade_standard_price_rows: Vec<crate::tgf::config::ExcelTradeStandardPrice>,
    pub trade_valid_price: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeValidPrice>,
    /// All rows of trade_valid_price.bytes (key `id` may repeat)
    pub trade_valid_price_rows: Vec<crate::tgf::config::ExcelTradeValidPrice>,
    pub trade_valid_time: std::collections::HashMap<i32, crate::tgf::config::ExcelTradeValidTime>,
    /// All rows of trade_valid_time.bytes (key `id` may repeat)
    pub trade_valid_time_rows: Vec<crate::tgf::config::ExcelTradeValidTime>,
    pub uesource_abilityset: std::collections::HashMap<i32, crate::tgf::config::UeSourceAbilitySet>,
    /// All rows of UeSource_AbilitySet.bytes (key `ue_source_id` may repeat)
    pub uesource_abilityset_rows: Vec<crate::tgf::config::UeSourceAbilitySet>,
    pub uesource_actorskin: std::collections::HashMap<i32, crate::tgf::config::UeSourceActorSkin>,
    /// All rows of UeSource_ActorSkin.bytes (key `skin_id` may repeat)
    pub uesource_actorskin_rows: Vec<crate::tgf::config::UeSourceActorSkin>,
    pub uesource_character: std::collections::HashMap<i32, crate::tgf::config::UeSourceCharacter>,
    /// All rows of UeSource_Character.bytes (key `role_id` may repeat)
    pub uesource_character_rows: Vec<crate::tgf::config::UeSourceCharacter>,
    pub uesource_equipment: std::collections::HashMap<i32, crate::tgf::config::UeSourceEquipment>,
    /// All rows of UeSource_Equipment.bytes (key `equip_id` may repeat)
    pub uesource_equipment_rows: Vec<crate::tgf::config::UeSourceEquipment>,
    pub uesource_equipment_subtype: Vec<crate::tgf::config::UeSourceEquipmentSubType>,
    pub uesource_firstlevelattrdataasset: std::collections::HashMap<i32, crate::tgf::config::UeSourceFirstLevelAttrDataAsset>,
    /// All rows of UeSource_FirstLevelAttrDataAsset.bytes (key `id` may repeat)
    pub uesource_firstlevelattrdataasset_rows: Vec<crate::tgf::config::UeSourceFirstLevelAttrDataAsset>,
    pub uesource_gameplayability: std::collections::HashMap<i32, crate::tgf::config::UeSourceGameplayAbility>,
    /// All rows of UeSource_GameplayAbility.bytes (key `gameplay_ability_id` may repeat)
    pub uesource_gameplayability_rows: Vec<crate::tgf::config::UeSourceGameplayAbility>,
    pub uesource_gameplayeffect: std::collections::HashMap<i32, crate::tgf::config::UeSourceGameplayEffect>,
    /// All rows of UeSource_GameplayEffect.bytes (key `gameplay_effect_id` may repeat)
    pub uesource_gameplayeffect_rows: Vec<crate::tgf::config::UeSourceGameplayEffect>,
    pub uesource_itemtype: std::collections::HashMap<i32, crate::tgf::config::UeSourceItemType>,
    /// All rows of UeSource_ItemType.bytes (key `type_id` may repeat)
    pub uesource_itemtype_rows: Vec<crate::tgf::config::UeSourceItemType>,
    pub uesource_loottype: Vec<crate::tgf::config::UeSourceLootType>,

}
impl GameData {
    pub fn load_all(dir: &str) -> Self {
        Self {
            activity_center: load_bytes_map(&format!("{}/activity_center.bytes", dir), |r: &crate::tgf::config::ExcelActivityCenter| r.id.clone()),
            activity_center_rows: load_bytes_vec(&format!("{}/activity_center.bytes", dir)),
            activity_center_passprots_rewards: load_bytes_map(&format!("{}/activity_center_passprots_rewards.bytes", dir), |r: &crate::tgf::config::ExcelActivityCenterPassprotsReards| r.id.clone()),
            activity_center_passprots_rewards_rows: load_bytes_vec(&format!("{}/activity_center_passprots_rewards.bytes", dir)),
            activity_center_random_rewards: load_bytes_map(&format!("{}/activity_center_random_rewards.bytes", dir), |r: &crate::tgf::config::ExcelActivityCenterRandomRewards| r.id.clone()),
            activity_center_random_rewards_rows: load_bytes_vec(&format!("{}/activity_center_random_rewards.bytes", dir)),
            activity_center_random_rewards_pool: load_bytes_map(&format!("{}/activity_center_random_rewards_pool.bytes", dir), |r: &crate::tgf::config::ExcelActivityCenterRandomRewardsPool| r.id.clone()),
            activity_center_random_rewards_pool_rows: load_bytes_vec(&format!("{}/activity_center_random_rewards_pool.bytes", dir)),
            affix_ability_set_id: load_bytes_map(&format!("{}/affix_ability_set_id.bytes", dir), |r: &crate::tgf::config::ExcelAffixAbilitySetId| r.affix_index_id.clone()),
            affix_ability_set_id_rows: load_bytes_vec(&format!("{}/affix_ability_set_id.bytes", dir)),
            affix_count_random_pool: load_bytes_map(&format!("{}/affix_count_random_pool.bytes", dir), |r: &crate::tgf::config::ExcelAffixCountRandomPool| r.affix_count_pool_id.clone()),
            affix_count_random_pool_rows: load_bytes_vec(&format!("{}/affix_count_random_pool.bytes", dir)),
            affix_gear_set_map_rule: load_bytes_map(&format!("{}/affix_gear_set_map_rule.bytes", dir), |r: &crate::tgf::config::ExcelAffixGearSetMapRule| r.equip_id.clone()),
            affix_gear_set_map_rule_rows: load_bytes_vec(&format!("{}/affix_gear_set_map_rule.bytes", dir)),
            affix_gear_set_rule: load_bytes_vec(&format!("{}/affix_gear_set_rule.bytes", dir)),
            affix_legendary_rule: load_bytes_map(&format!("{}/affix_legendary_rule.bytes", dir), |r: &crate::tgf::config::ExcelAffixLegendaryRule| r.affix_alias_id.clone()),
            affix_legendary_rule_rows: load_bytes_vec(&format!("{}/affix_legendary_rule.bytes", dir)),
            affix_level_pool_rule: load_bytes_map(&format!("{}/affix_level_pool_rule.bytes", dir), |r: &crate::tgf::config::ExcelAffixLevelPoolRule| r.rule_id.clone()),
            affix_level_pool_rule_rows: load_bytes_vec(&format!("{}/affix_level_pool_rule.bytes", dir)),
            affix_monster_pool_rule: load_bytes_map(&format!("{}/affix_monster_pool_rule.bytes", dir), |r: &crate::tgf::config::ExcelAffixMonsterPoolRule| r.rule_id.clone()),
            affix_monster_pool_rule_rows: load_bytes_vec(&format!("{}/affix_monster_pool_rule.bytes", dir)),
            affix_pool: load_bytes_map(&format!("{}/affix_pool.bytes", dir), |r: &crate::tgf::config::ExcelAffixPool| r.affix_pool_id.clone()),
            affix_pool_rows: load_bytes_vec(&format!("{}/affix_pool.bytes", dir)),
            affix_template: load_bytes_map(&format!("{}/affix_template.bytes", dir), |r: &crate::tgf::config::ExcelAffixTemplate| r.item_id.clone()),
            affix_template_rows: load_bytes_vec(&format!("{}/affix_template.bytes", dir)),
            affix_value_template: load_bytes_map(&format!("{}/affix_value_template.bytes", dir), |r: &crate::tgf::config::ExcelAffixValueTemplate| r.affix_value_id.clone()),
            affix_value_template_rows: load_bytes_vec(&format!("{}/affix_value_template.bytes", dir)),
            battle_ability_set_description: load_bytes_map(&format!("{}/battle_ability_set_description.bytes", dir), |r: &crate::tgf::config::ExcelBattleAbilitySetDescription| r.ability_set_id.clone()),
            battle_ability_set_description_rows: load_bytes_vec(&format!("{}/battle_ability_set_description.bytes", dir)),
            battle_affix_effect_rule: load_bytes_map(&format!("{}/battle_affix_effect_rule.bytes", dir), |r: &crate::tgf::config::ExcelBattleAffixEffectRule| r.id.clone()),
            battle_affix_effect_rule_rows: load_bytes_vec(&format!("{}/battle_affix_effect_rule.bytes", dir)),
            battle_attribute_description: load_bytes_vec(&format!("{}/battle_attribute_description.bytes", dir)),
            chat_channel_type: load_bytes_vec(&format!("{}/chat_channel_type.bytes", dir)),
            competition_level: load_bytes_map(&format!("{}/competition_level.bytes", dir), |r: &crate::tgf::config::ExcelCompetitionLevel| r.competition_level_id.clone()),
            competition_level_rows: load_bytes_vec(&format!("{}/competition_level.bytes", dir)),
            r#const: load_bytes_vec(&format!("{}/const.bytes", dir)),
            craft_group: load_bytes_map(&format!("{}/craft_group.bytes", dir), |r: &crate::tgf::config::ExcelCraftingGroup| r.group_id.clone()),
            craft_group_rows: load_bytes_vec(&format!("{}/craft_group.bytes", dir)),
            craft_table: load_bytes_map(&format!("{}/craft_table.bytes", dir), |r: &crate::tgf::config::ExcelCraftingRecipe| r.id.clone()),
            craft_table_rows: load_bytes_vec(&format!("{}/craft_table.bytes", dir)),
            craft_tag: load_bytes_map(&format!("{}/craft_tag.bytes", dir), |r: &crate::tgf::config::ExcelCraftingTag| r.tag_id.clone()),
            craft_tag_rows: load_bytes_vec(&format!("{}/craft_tag.bytes", dir)),
            cuver_table_source: load_bytes_map(&format!("{}/cuver_table_source.bytes", dir), |r: &crate::tgf::config::ExcelCuverTableSource| r.id.clone()),
            cuver_table_source_rows: load_bytes_vec(&format!("{}/cuver_table_source.bytes", dir)),
            dlc_content: load_bytes_map(&format!("{}/dlc_content.bytes", dir), |r: &crate::tgf::config::ExcelDlcContent| r.app_id.clone()),
            dlc_content_rows: load_bytes_vec(&format!("{}/dlc_content.bytes", dir)),
            dlc_packet: load_bytes_map(&format!("{}/dlc_packet.bytes", dir), |r: &crate::tgf::config::ExcelDlcPacket| r.app_id.clone()),
            dlc_packet_rows: load_bytes_vec(&format!("{}/dlc_packet.bytes", dir)),
            error_code: load_bytes_vec(&format!("{}/error_code.bytes", dir)),
            excel_001: load_bytes_map(&format!("{}/excel_001.bytes", dir), |r: &crate::tgf::config::ExcelLootRarePoolCntLimit| r.item_id.clone()),
            excel_001_rows: load_bytes_vec(&format!("{}/excel_001.bytes", dir)),
            excel_002: load_bytes_map(&format!("{}/excel_002.bytes", dir), |r: &crate::tgf::config::ExcelLootRarePoolItemRate| r.random_id.clone()),
            excel_002_rows: load_bytes_vec(&format!("{}/excel_002.bytes", dir)),
            facade: load_bytes_map(&format!("{}/facade.bytes", dir), |r: &crate::tgf::config::ExcelFacade| r.id.clone()),
            facade_rows: load_bytes_vec(&format!("{}/facade.bytes", dir)),
            facade_sort: load_bytes_map(&format!("{}/facade_sort.bytes", dir), |r: &crate::tgf::config::ExcelFacadeSort| r.original_resource_id.clone()),
            facade_sort_rows: load_bytes_vec(&format!("{}/facade_sort.bytes", dir)),
            gear_base_item: load_bytes_map(&format!("{}/gear_base_item.bytes", dir), |r: &crate::tgf::config::ExcelGearBaseItem| r.id.clone()),
            gear_base_item_rows: load_bytes_vec(&format!("{}/gear_base_item.bytes", dir)),
            gear_dismantle: load_bytes_map(&format!("{}/gear_dismantle.bytes", dir), |r: &crate::tgf::config::ExcelGearDismantle| r.item_id.clone()),
            gear_dismantle_rows: load_bytes_vec(&format!("{}/gear_dismantle.bytes", dir)),
            gear_durability: load_bytes_vec(&format!("{}/gear_durability.bytes", dir)),
            gear_gem_guide: load_bytes_map(&format!("{}/gear_gem_guide.bytes", dir), |r: &crate::tgf::config::ExcelGearGemGuide| r.id.clone()),
            gear_gem_guide_rows: load_bytes_vec(&format!("{}/gear_gem_guide.bytes", dir)),
            gear_gem_inst: load_bytes_map(&format!("{}/gear_gem_inst.bytes", dir), |r: &crate::tgf::config::ExcelGearGemInst| r.item_id.clone()),
            gear_gem_inst_rows: load_bytes_vec(&format!("{}/gear_gem_inst.bytes", dir)),
            gear_heirloom_guide: load_bytes_map(&format!("{}/gear_heirloom_guide.bytes", dir), |r: &crate::tgf::config::ExcelGearHeirloomGuide| r.id.clone()),
            gear_heirloom_guide_rows: load_bytes_vec(&format!("{}/gear_heirloom_guide.bytes", dir)),
            gear_insurance_item: load_bytes_map(&format!("{}/gear_insurance_item.bytes", dir), |r: &crate::tgf::config::ExcelGearInsuranceItem| r.id.clone()),
            gear_insurance_item_rows: load_bytes_vec(&format!("{}/gear_insurance_item.bytes", dir)),
            gear_soul_bound_cost: load_bytes_vec(&format!("{}/gear_soul_bound_cost.bytes", dir)),
            gear_soul_bound_wear_part_random: load_bytes_map(&format!("{}/gear_soul_bound_wear_part_random.bytes", dir), |r: &crate::tgf::config::ExcelGearSoulBoundWearPartRandom| r.group_id.clone()),
            gear_soul_bound_wear_part_random_rows: load_bytes_vec(&format!("{}/gear_soul_bound_wear_part_random.bytes", dir)),
            gear_template: load_bytes_map(&format!("{}/gear_template.bytes", dir), |r: &crate::tgf::config::ExcelGearTemplate| r.item_id.clone()),
            gear_template_rows: load_bytes_vec(&format!("{}/gear_template.bytes", dir)),
            immaculate_trial_grade: load_bytes_map(&format!("{}/immaculate_trial_grade.bytes", dir), |r: &crate::tgf::config::ExcelImmaculateTrialGrade| r.grade_id.clone()),
            immaculate_trial_grade_rows: load_bytes_vec(&format!("{}/immaculate_trial_grade.bytes", dir)),
            immaculate_trial_random_rewards: load_bytes_map(&format!("{}/immaculate_trial_random_rewards.bytes", dir), |r: &crate::tgf::config::ExcelImmaculateTrialRandomRewards| r.random_reward_id.clone()),
            immaculate_trial_random_rewards_rows: load_bytes_vec(&format!("{}/immaculate_trial_random_rewards.bytes", dir)),
            immaculate_trial_rule: load_bytes_map(&format!("{}/immaculate_trial_rule.bytes", dir), |r: &crate::tgf::config::ExcelImmaculateTrialRule| r.activity_rule_id.clone()),
            immaculate_trial_rule_rows: load_bytes_vec(&format!("{}/immaculate_trial_rule.bytes", dir)),
            item_router: load_bytes_vec(&format!("{}/item_router.bytes", dir)),
            item_type: load_bytes_map(&format!("{}/item_type.bytes", dir), |r: &crate::tgf::config::ExcelItemType| r.type_id.clone()),
            item_type_rows: load_bytes_vec(&format!("{}/item_type.bytes", dir)),
            level_type: load_bytes_map(&format!("{}/level_type.bytes", dir), |r: &crate::tgf::config::ExcelLevelType| r.match_making_pool_id.clone()),
            level_type_rows: load_bytes_vec(&format!("{}/level_type.bytes", dir)),
            loot: load_bytes_map(&format!("{}/loot.bytes", dir), |r: &crate::tgf::config::ExcelItemDropItem| r.item_id.clone()),
            loot_rows: load_bytes_vec(&format!("{}/loot.bytes", dir)),
            loot_choose_group_rate: load_bytes_map(&format!("{}/loot_choose_group_rate.bytes", dir), |r: &crate::tgf::config::ExcelLootChooseGroupRate| r.loot_id.clone()),
            loot_choose_group_rate_rows: load_bytes_vec(&format!("{}/loot_choose_group_rate.bytes", dir)),
            loot_disable_loot_id: load_bytes_map(&format!("{}/loot_disable_loot_id.bytes", dir), |r: &crate::tgf::config::ExcelLootDisableLootId| r.loot_id.clone()),
            loot_disable_loot_id_rows: load_bytes_vec(&format!("{}/loot_disable_loot_id.bytes", dir)),
            loot_magic_found: load_bytes_vec(&format!("{}/loot_magic_found.bytes", dir)),
            loot_rate: load_bytes_vec(&format!("{}/loot_rate.bytes", dir)),
            loot_source_map: load_bytes_map(&format!("{}/loot_source_map.bytes", dir), |r: &crate::tgf::config::ExcelLootSourceMap| r.id.clone()),
            loot_source_map_rows: load_bytes_vec(&format!("{}/loot_source_map.bytes", dir)),
            loot_ue_source_choose_group: load_bytes_map(&format!("{}/loot_ue_source_choose_group.bytes", dir), |r: &crate::tgf::config::ExcelLootUeSourceChooseGroup| r.ue_loot_source_id.clone()),
            loot_ue_source_choose_group_rows: load_bytes_vec(&format!("{}/loot_ue_source_choose_group.bytes", dir)),
            loot_unlucky_value: load_bytes_vec(&format!("{}/loot_unlucky_value.bytes", dir)),
            lottery_access_key: load_bytes_map(&format!("{}/lottery_access_key.bytes", dir), |r: &crate::tgf::config::ExcelLotteryAccessKey| r.type_id.clone()),
            lottery_access_key_rows: load_bytes_vec(&format!("{}/lottery_access_key.bytes", dir)),
            lottery_pool: load_bytes_map(&format!("{}/lottery_pool.bytes", dir), |r: &crate::tgf::config::ExcelLotteryPool| r.id.clone()),
            lottery_pool_rows: load_bytes_vec(&format!("{}/lottery_pool.bytes", dir)),
            mail_template: load_bytes_map(&format!("{}/mail_template.bytes", dir), |r: &crate::tgf::config::ExcelMailTemplate| r.type_id.clone()),
            mail_template_rows: load_bytes_vec(&format!("{}/mail_template.bytes", dir)),
            map_detail: load_bytes_map(&format!("{}/map_detail.bytes", dir), |r: &crate::tgf::config::ExcelMapDetail| r.map_id.clone()),
            map_detail_rows: load_bytes_vec(&format!("{}/map_detail.bytes", dir)),
            map_exp: load_bytes_map(&format!("{}/map_exp.bytes", dir), |r: &crate::tgf::config::ExcelMapExp| r.map_id.clone()),
            map_exp_rows: load_bytes_vec(&format!("{}/map_exp.bytes", dir)),
            map_select: load_bytes_map(&format!("{}/map_select.bytes", dir), |r: &crate::tgf::config::ExcelMapSelect| r.map_name_id.clone()),
            map_select_rows: load_bytes_vec(&format!("{}/map_select.bytes", dir)),
            map_select_model_feature: load_bytes_map(&format!("{}/map_select_model_feature.bytes", dir), |r: &crate::tgf::config::ExcelMapSelectModelFeature| r.model_feature_id.clone()),
            map_select_model_feature_rows: load_bytes_vec(&format!("{}/map_select_model_feature.bytes", dir)),
            market_info: load_bytes_vec(&format!("{}/market_info.bytes", dir)),
            market_poor_man: load_bytes_vec(&format!("{}/market_poor_man.bytes", dir)),
            market_popup_video: load_bytes_vec(&format!("{}/market_popup_video.bytes", dir)),
            market_product_item: load_bytes_vec(&format!("{}/market_product_item.bytes", dir)),
            market_refresh_pool: load_bytes_vec(&format!("{}/market_refresh_pool.bytes", dir)),
            market_special_cost: load_bytes_vec(&format!("{}/market_special_cost.bytes", dir)),
            market_tab_info: load_bytes_vec(&format!("{}/market_tab_info.bytes", dir)),
            mithril_force_item: load_bytes_map(&format!("{}/mithril_force_item.bytes", dir), |r: &crate::tgf::config::ExcelMithrilForceItem| r.id.clone()),
            mithril_force_item_rows: load_bytes_vec(&format!("{}/mithril_force_item.bytes", dir)),
            mithril_force_prestige: load_bytes_vec(&format!("{}/mithril_force_prestige.bytes", dir)),
            mithril_force_settlement: load_bytes_vec(&format!("{}/mithril_force_settlement.bytes", dir)),
            mithril_force_settlement_factor: load_bytes_vec(&format!("{}/mithril_force_settlement_factor.bytes", dir)),
            module_unlock_circular: load_bytes_map(&format!("{}/module_unlock_circular.bytes", dir), |r: &crate::tgf::config::ExcelModuleUnlockCircular| r.id.clone()),
            module_unlock_circular_rows: load_bytes_vec(&format!("{}/module_unlock_circular.bytes", dir)),
            module_unlock_type: load_bytes_map(&format!("{}/module_unlock_type.bytes", dir), |r: &crate::tgf::config::ExcelModuleUnlockType| r.module_id.clone()),
            module_unlock_type_rows: load_bytes_vec(&format!("{}/module_unlock_type.bytes", dir)),
            quest_group_list: load_bytes_map(&format!("{}/quest_group_list.bytes", dir), |r: &crate::tgf::config::ExcelQuestGroupList| r.id.clone()),
            quest_group_list_rows: load_bytes_vec(&format!("{}/quest_group_list.bytes", dir)),
            quest_list: load_bytes_map(&format!("{}/quest_list.bytes", dir), |r: &crate::tgf::config::ExcelQuestList| r.id.clone()),
            quest_list_rows: load_bytes_vec(&format!("{}/quest_list.bytes", dir)),
            quest_progress_condition: load_bytes_map(&format!("{}/quest_progress_condition.bytes", dir), |r: &crate::tgf::config::ExcelQuestProgressCondition| r.id.clone()),
            quest_progress_condition_rows: load_bytes_vec(&format!("{}/quest_progress_condition.bytes", dir)),
            quest_progress_type: load_bytes_map(&format!("{}/quest_progress_type.bytes", dir), |r: &crate::tgf::config::ExcelQuestProgressType| r.id.clone()),
            quest_progress_type_rows: load_bytes_vec(&format!("{}/quest_progress_type.bytes", dir)),
            quest_trigger_event_type: load_bytes_vec(&format!("{}/quest_trigger_event_type.bytes", dir)),
            rank_period_reward_pool: load_bytes_map(&format!("{}/rank_period_reward_pool.bytes", dir), |r: &crate::tgf::config::ExcelRankPeriodRewardPool| r.rank_reward_pool_id.clone()),
            rank_period_reward_pool_rows: load_bytes_vec(&format!("{}/rank_period_reward_pool.bytes", dir)),
            rank_report_setting: load_bytes_map(&format!("{}/rank_report_setting.bytes", dir), |r: &crate::tgf::config::ExcelRankReportSetting| r.rank_instance_id.clone()),
            rank_report_setting_rows: load_bytes_vec(&format!("{}/rank_report_setting.bytes", dir)),
            rank_rule: load_bytes_map(&format!("{}/rank_rule.bytes", dir), |r: &crate::tgf::config::ExcelRankRule| r.rank_instance_id.clone()),
            rank_rule_rows: load_bytes_vec(&format!("{}/rank_rule.bytes", dir)),
            role_default_equipment: load_bytes_map(&format!("{}/role_default_equipment.bytes", dir), |r: &crate::tgf::config::ExcelRoleDefaultEquipment| r.config_id.clone()),
            role_default_equipment_rows: load_bytes_vec(&format!("{}/role_default_equipment.bytes", dir)),
            role_face_pinching_limit: load_bytes_vec(&format!("{}/role_face_pinching_limit.bytes", dir)),
            role_head_icon: load_bytes_map(&format!("{}/role_head_icon.bytes", dir), |r: &crate::tgf::config::ExcelRoleHeadIcon| r.type_id.clone()),
            role_head_icon_rows: load_bytes_vec(&format!("{}/role_head_icon.bytes", dir)),
            role_level_exp: load_bytes_vec(&format!("{}/role_level_exp.bytes", dir)),
            role_level_upgrade: load_bytes_map(&format!("{}/role_level_upgrade.bytes", dir), |r: &crate::tgf::config::ExcelRoleLevelUpgrade| r.role_type_id.clone()),
            role_level_upgrade_rows: load_bytes_vec(&format!("{}/role_level_upgrade.bytes", dir)),
            role_skill: load_bytes_map(&format!("{}/role_skill.bytes", dir), |r: &crate::tgf::config::ExcelRoleSkill| r.skill_id.clone()),
            role_skill_rows: load_bytes_vec(&format!("{}/role_skill.bytes", dir)),
            role_skin_type: load_bytes_map(&format!("{}/role_skin_type.bytes", dir), |r: &crate::tgf::config::ExcelRoleSkinType| r.skin_id.clone()),
            role_skin_type_rows: load_bytes_vec(&format!("{}/role_skin_type.bytes", dir)),
            role_type: load_bytes_map(&format!("{}/role_type.bytes", dir), |r: &crate::tgf::config::ExcelRoleType| r.type_id.clone()),
            role_type_rows: load_bytes_vec(&format!("{}/role_type.bytes", dir)),
            season_rule: load_bytes_map(&format!("{}/season_rule.bytes", dir), |r: &crate::tgf::config::ExcelSeasonRule| r.season_id.clone()),
            season_rule_rows: load_bytes_vec(&format!("{}/season_rule.bytes", dir)),
            skill_calculate: load_bytes_map(&format!("{}/skill_calculate.bytes", dir), |r: &crate::tgf::config::ExcelSkillCalculate| r.id.clone()),
            skill_calculate_rows: load_bytes_vec(&format!("{}/skill_calculate.bytes", dir)),
            skill_num_exchange: load_bytes_map(&format!("{}/skill_num_exchange.bytes", dir), |r: &crate::tgf::config::ExcelSkillNumExchange| r.id.clone()),
            skill_num_exchange_rows: load_bytes_vec(&format!("{}/skill_num_exchange.bytes", dir)),
            tiers: load_bytes_map(&format!("{}/tiers.bytes", dir), |r: &crate::tgf::config::ExcelTiers| r.tiers_id.clone()),
            tiers_rows: load_bytes_vec(&format!("{}/tiers.bytes", dir)),
            tiers_kill_plyaer_rule: load_bytes_vec(&format!("{}/tiers_kill_plyaer_rule.bytes", dir)),
            tiers_show_group: load_bytes_map(&format!("{}/tiers_show_group.bytes", dir), |r: &crate::tgf::config::ExcelTiersShowGroup| r.type_id.clone()),
            tiers_show_group_rows: load_bytes_vec(&format!("{}/tiers_show_group.bytes", dir)),
            trade_commodity: load_bytes_map(&format!("{}/trade_commodity.bytes", dir), |r: &crate::tgf::config::ExcelTradeCommodity| r.id.clone()),
            trade_commodity_rows: load_bytes_vec(&format!("{}/trade_commodity.bytes", dir)),
            trade_item_require: load_bytes_map(&format!("{}/trade_item_require.bytes", dir), |r: &crate::tgf::config::ExcelTradeItemRequire| r.id.clone()),
            trade_item_require_rows: load_bytes_vec(&format!("{}/trade_item_require.bytes", dir)),
            trade_market: load_bytes_map(&format!("{}/trade_market.bytes", dir), |r: &crate::tgf::config::ExcelTradeMarket| r.id.clone()),
            trade_market_rows: load_bytes_vec(&format!("{}/trade_market.bytes", dir)),
            trade_standard_price: load_bytes_map(&format!("{}/trade_standard_price.bytes", dir), |r: &crate::tgf::config::ExcelTradeStandardPrice| r.id.clone()),
            trade_standard_price_rows: load_bytes_vec(&format!("{}/trade_standard_price.bytes", dir)),
            trade_valid_price: load_bytes_map(&format!("{}/trade_valid_price.bytes", dir), |r: &crate::tgf::config::ExcelTradeValidPrice| r.id.clone()),
            trade_valid_price_rows: load_bytes_vec(&format!("{}/trade_valid_price.bytes", dir)),
            trade_valid_time: load_bytes_map(&format!("{}/trade_valid_time.bytes", dir), |r: &crate::tgf::config::ExcelTradeValidTime| r.id.clone()),
            trade_valid_time_rows: load_bytes_vec(&format!("{}/trade_valid_time.bytes", dir)),
            uesource_abilityset: load_bytes_map(&format!("{}/UeSource_AbilitySet.bytes", dir), |r: &crate::tgf::config::UeSourceAbilitySet| r.ue_source_id.clone()),
            uesource_abilityset_rows: load_bytes_vec(&format!("{}/UeSource_AbilitySet.bytes", dir)),
            uesource_actorskin: load_bytes_map(&format!("{}/UeSource_ActorSkin.bytes", dir), |r: &crate::tgf::config::UeSourceActorSkin| r.skin_id.clone()),
            uesource_actorskin_rows: load_bytes_vec(&format!("{}/UeSource_ActorSkin.bytes", dir)),
            uesource_character: load_bytes_map(&format!("{}/UeSource_Character.bytes", dir), |r: &crate::tgf::config::UeSourceCharacter| r.role_id.clone()),
            uesource_character_rows: load_bytes_vec(&format!("{}/UeSource_Character.bytes", dir)),
            uesource_equipment: load_bytes_map(&format!("{}/UeSource_Equipment.bytes", dir), |r: &crate::tgf::config::UeSourceEquipment| r.equip_id.clone()),
            uesource_equipment_rows: load_bytes_vec(&format!("{}/UeSource_Equipment.bytes", dir)),
            uesource_equipment_subtype: load_bytes_vec(&format!("{}/UeSource_Equipment_SubType.bytes", dir)),
            uesource_firstlevelattrdataasset: load_bytes_map(&format!("{}/UeSource_FirstLevelAttrDataAsset.bytes", dir), |r: &crate::tgf::config::UeSourceFirstLevelAttrDataAsset| r.id.clone()),
            uesource_firstlevelattrdataasset_rows: load_bytes_vec(&format!("{}/UeSource_FirstLevelAttrDataAsset.bytes", dir)),
            uesource_gameplayability: load_bytes_map(&format!("{}/UeSource_GameplayAbility.bytes", dir), |r: &crate::tgf::config::UeSourceGameplayAbility| r.gameplay_ability_id.clone()),
            uesource_gameplayability_rows: load_bytes_vec(&format!("{}/UeSource_GameplayAbility.bytes", dir)),
            uesource_gameplayeffect: load_bytes_map(&format!("{}/UeSource_GameplayEffect.bytes", dir), |r: &crate::tgf::config::UeSourceGameplayEffect| r.gameplay_effect_id.clone()),
            uesource_gameplayeffect_rows: load_bytes_vec(&format!("{}/UeSource_GameplayEffect.bytes", dir)),
            uesource_itemtype: load_bytes_map(&format!("{}/UeSource_ItemType.bytes", dir), |r: &crate::tgf::config::UeSourceItemType| r.type_id.clone()),
            uesource_itemtype_rows: load_bytes_vec(&format!("{}/UeSource_ItemType.bytes", dir)),
            uesource_loottype: load_bytes_vec(&format!("{}/UeSource_LootType.bytes", dir)),

        }
    }
}
