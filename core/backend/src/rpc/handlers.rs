use crate::db::GameDb;
use crate::game_data::{GameData, RpcHandlers};
use crate::structures::PlayerModel;
use crate::tgf;
use prost::Message;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use std::sync::OnceLock;

mod common;
mod account_handlers;
mod account_wipe;
mod affix;
mod trial;
pub use trial::{trial_ensure_open, trial_me, TrialMe, TRIAL_LEVEL};
mod arena_settle;
pub use arena_settle::{arena_strip, ArenaPlayerStat};
mod battle_handlers;
mod ds_handlers;
mod guild_handlers;
mod inventory_handlers;
mod item_handlers;
mod lobby_handlers;
pub(crate) use lobby_handlers::room_finish_push;
pub(crate) use lobby_handlers::room_sent_reset;
pub(crate) use social_handlers::presence_offline;
mod misc_handlers;
mod player_handlers;
mod progress_handlers;
mod raid_handlers;
mod raid_lock;
mod raid_native;
mod relic_limit;
mod relic_reset;
mod rank;
mod stats;
mod stash_pages; // ext: stash_pages
pub use raid_native::NativeReject;
pub use trade_handlers::{system_auction, system_buyout};
mod role_handlers;
mod room_handlers;
mod shop_handlers;
mod settings;
mod social_handlers;
mod trade_handlers;
mod toast; // ext: toast
pub use toast::{toast_online, toast_to}; // ext: toast

/// Context of the current request
pub struct GatewaySession {
    pub db: GameDb,
    pub game_data: Arc<GameData>,
    pub open_id: String,
    pub peer_ip: String,
    pub after_pushes: Mutex<Vec<(String, String, Vec<u8>)>>,
    /// Error code for the response's RpcHead.error_code (0 — success); set via set_rpc_error
    pub rpc_error: std::sync::atomic::AtomicI32,
}

impl GatewaySession {
    pub fn push<T: Message>(&self, method: &str, type_url: &str, msg: T) {
        self.after_pushes.lock().unwrap().push((
            method.to_string(),
            type_url.to_string(),
            msg.encode_to_vec(),
        ));
    }

    pub fn clean_open_id(&self) -> String {
        let cleaned: String = self.open_id.chars().filter(|c| c.is_alphanumeric()).collect();
        if cleaned.is_empty() {
            "76561198000000901".to_string()
        } else {
            cleaned
        }
    }

    pub fn now_ts() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
    }
}

/// One-time migration of item guids in domain tables (called by crate::guid::migrate after players):
/// trade book, mail (by open_id), gifts (by user_id). Returns the number of replacements.
pub async fn migrate_item_guids(db: &GameDb, players: &[(String, u64)], map: &mut crate::guid::GuidMap) -> usize {
    let open_ids: Vec<String> = players.iter().map(|(o, _)| o.clone()).collect();
    let user_ids: Vec<u64> = players.iter().map(|(_, u)| *u).collect();
    trade_handlers::trade_migrate_guids(db, map).await
        + progress_handlers::mail_migrate_guids(db, &open_ids, map).await
        + social_handlers::gift_migrate_guids(db, &user_ids, map).await
}

/// One-time reset of 12.11: relic resources, relics, gems, recipes (relic_reset.rs). Call before accepting clients.
/// 12.11 16:12: removed from main.rs — it ran a second time (see relic_reset::run, mark).
#[allow(dead_code)]
pub async fn relic_reset(db: &GameDb, game_data: &GameData) {
    // like raid_handlers::is_crafted: inst_id from the guide + variants with a UE resource from the Heirloom folder (220110141 etc.);
    // only 220xxxxxx gear — the pass and relic shards (100076, 100084…100087, treasures) are not touched
    let ids = game_data
        .gear_heirloom_guide
        .values()
        .map(|g| if g.inst_id != 0 { g.inst_id } else { g.id })
        .chain(game_data.uesource_itemtype.iter().filter(|(id, u)| u.ue_path.contains("Heirloom") && **id / 10_000_000 == 22).map(|(id, _)| *id))
        .collect();
    relic_reset::run(db, &ids).await;
}

/// All handlers only delegate to the domain files src/rpc/handlers/<domain>.rs.
/// Methods with `req: Empty` (*_sync etc.) are server→client pushes and stay the trait's default implementation.
#[async_trait::async_trait]
impl RpcHandlers for GatewaySession {
    // ---- login, profile, cache (account_handlers.rs) ----
    async fn handle_pre_login(&self, req: tgf::CsPreLoginReq) -> Option<tgf::ScPreLoginRsp> {
        self.pre_login_impl(req).await
    }
    async fn handle_login(&self, req: tgf::CsLoginReq) -> Option<tgf::ScLoginRsp> {
        self.login_impl(req).await
    }
    async fn handle_player_get_info(
        &self,
        req: tgf::CsPlayerGetInfoReq,
    ) -> Option<tgf::ScPlayerGetInfoRsp> {
        self.player_get_info_impl(req).await
    }
    async fn handle_player_setting_data(
        &self,
        req: tgf::CsPlayerSettingDataReq,
    ) -> Option<tgf::ScPlayerSettingDataRsp> {
        self.player_setting_data_impl(req).await
    }
    async fn handle_cache_batch_get(
        &self,
        req: tgf::CsCacheBatchGetReq,
    ) -> Option<tgf::ScCacheBatchGetRsp> {
        self.cache_batch_get_impl(req).await
    }
    async fn handle_cache_expired(
        &self,
        req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScCacheExpiredSync> {
        self.cache_expired_impl(req).await
    }
    async fn handle_player_all_cache_expired(
        &self,
        req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScAllCacheExpiredSync> {
        self.player_all_cache_expired_impl(req).await
    }
    async fn handle_rank_cache_expired_sync(
        &self,
        req: crate::google::protobuf::Empty,
    ) -> Option<tgf::ScRankCacheExpiredSync> {
        self.rank_cache_expired_sync_impl(req).await
    }

    // ---- character creation, hall (role_handlers.rs) ----
    async fn handle_role_create(&self, req: tgf::CsRoleCreateReq) -> Option<tgf::ScRoleCreateRsp> {
        self.role_create_impl(req).await
    }
    async fn handle_hall_set_role(
        &self,
        req: tgf::CsHallSetRoleReq,
    ) -> Option<tgf::ScHallSetRoleRsp> {
        self.hall_set_role_impl(req).await
    }
    async fn handle_role_select_skill(
        &self,
        req: tgf::CsRoleSelectSkillReq,
    ) -> Option<tgf::ScRoleSelectSkillRsp> {
        self.role_select_skill_impl(req).await
    }

    // ---- player profile and characters (player_handlers.rs) ----
    async fn handle_player_get_info_by_page(&self, req: crate::tgf::CsPlayerGetInfoByPageReq) -> Option<crate::tgf::ScPlayerGetInfoByPageRsp> {
        self.player_get_info_by_page_impl(req).await
    }
    async fn handle_player_batch_get_basic_info(&self, req: crate::tgf::CsPlayerBatchGetBasicInfoReq) -> Option<crate::tgf::ScPlayerBatchGetBasicInfoRsp> {
        self.player_batch_get_basic_info_impl(req).await
    }
    async fn handle_player_batch_get_id_key(&self, req: crate::tgf::CsPlayerBatchGetIdKeyReq) -> Option<crate::tgf::ScPlayerBatchGetIdKeyRsp> {
        self.player_batch_get_id_key_impl(req).await
    }
    async fn handle_player_rename(&self, req: crate::tgf::CsPlayerRenameReq) -> Option<crate::tgf::ScPlayerRenameRsp> {
        self.player_rename_impl(req).await
    }
    async fn handle_player_update_icon(&self, req: crate::tgf::CsPlayerUpdateIconReq) -> Option<crate::tgf::ScPlayerUpdateIconRsp> {
        self.player_update_icon_impl(req).await
    }
    async fn handle_player_update_options(&self, req: crate::tgf::CsPlayerUpdateOptionsReq) -> Option<crate::tgf::ScPlayerUpdateOptionsRsp> {
        self.player_update_options_impl(req).await
    }
    async fn handle_player_gm_command(&self, req: crate::tgf::CsPlayerGmCommandReq) -> Option<crate::tgf::ScPlayerGmCommandRsp> {
        self.player_gm_command_impl(req).await
    }
    async fn handle_player_receive_homeland_reward(&self, req: crate::tgf::CsPlayerReceiveHomelandRewardReq) -> Option<crate::tgf::ScPlayerReceiveHomelandRewardRsp> {
        self.player_receive_homeland_reward_impl(req).await
    }
    async fn handle_player_birthday_update(&self, req: crate::tgf::CsPlayerUpdateBirthDayReq) -> Option<crate::tgf::ScPlayerUpdateBirthDayRsp> {
        self.player_birthday_update_impl(req).await
    }
    async fn handle_hall_switch(&self, req: crate::tgf::CsHallSwitchReq) -> Option<crate::tgf::ScHallSwitchRsp> {
        self.hall_switch_impl(req).await
    }
    async fn handle_role_delete(&self, req: crate::tgf::CsRoleDeleteReq) -> Option<crate::tgf::ScRoleDeleteRsp> {
        self.role_delete_impl(req).await
    }
    async fn handle_role_delete_lazy(&self, req: crate::tgf::CsRoleDeleteLazyReq) -> Option<crate::tgf::ScRoleDeleteLazyRsp> {
        self.role_delete_lazy_impl(req).await
    }
    async fn handle_role_cancel_lazy_delete(&self, req: crate::tgf::CsRoleCancelLazyDeleteReq) -> Option<crate::tgf::ScRoleCancelLazyDeleteRsp> {
        self.role_cancel_lazy_delete_impl(req).await
    }
    async fn handle_role_fetch_user_role(&self, req: crate::tgf::CsRoleFetchUserRoleReq) -> Option<crate::tgf::ScRoleFetchUserRoleRsp> {
        self.role_fetch_user_role_impl(req).await
    }
    async fn handle_role_move_season(&self, req: crate::tgf::CsRoleMoveSeasonReq) -> Option<crate::tgf::ScRoleMoveSeasonRsp> {
        self.role_move_season_impl(req).await
    }
    async fn handle_role_reshape_face_pinching(&self, req: crate::tgf::CsRoleReshapeFacePinchingReq) -> Option<crate::tgf::ScRoleReshapeFacePinchingRsp> {
        self.role_reshape_face_pinching_impl(req).await
    }
    async fn handle_facade_update(&self, req: crate::tgf::CsFacadeSetReq) -> Option<crate::tgf::ScFacadeSetRsp> {
        self.facade_update_impl(req).await
    }
    async fn handle_facade_update_role_skin(&self, req: crate::tgf::CsFacadeUpdateRoleSkinReq) -> Option<crate::tgf::ScFacadeUpdateRoleSkinRsp> {
        self.facade_update_role_skin_impl(req).await
    }
    async fn handle_facade_save_skin_template(&self, req: crate::tgf::CsFacadeSaveSkinTemplateReq) -> Option<crate::tgf::ScFacadeSaveSkinTemplateRsp> {
        self.facade_save_skin_template_impl(req).await
    }
    async fn handle_facade_delete_skin_template(&self, req: crate::tgf::CsFacadeDeleteSkinTemplateReq) -> Option<crate::tgf::ScFacadeDeleteSkinTemplateRsp> {
        self.facade_delete_skin_template_impl(req).await
    }
    async fn handle_facade_get_on_sale_facade(&self, req: crate::tgf::CsFacadeGetOnSaleFacadeReq) -> Option<crate::tgf::ScFacadeGetOnSaleFacadeRsp> {
        self.facade_get_on_sale_facade_impl(req).await
    }

    // ---- items in the lobby (inventory_handlers.rs) ----
    async fn handle_role_wear(&self, req: crate::tgf::CsRoleWearReq) -> Option<crate::tgf::ScRoleWearRsp> {
        self.role_wear_impl(req).await
    }
    async fn handle_role_take_off(&self, req: crate::tgf::CsRoleTakeOffReq) -> Option<crate::tgf::ScRoleTakeOffRsp> {
        self.role_take_off_impl(req).await
    }
    async fn handle_role_weapon_set_swap(&self, req: crate::tgf::CsRoleWeaponSetSwapReq) -> Option<crate::tgf::ScRoleWeaponSetSwapRsp> {
        self.role_weapon_set_swap_impl(req).await
    }
    async fn handle_inventory_item_move(&self, req: crate::tgf::CsInventoryItemMoveReq) -> Option<crate::tgf::ScInventoryItemMoveRsp> {
        self.inventory_item_move_impl(req).await
    }
    async fn handle_inventory_item_swap(&self, req: crate::tgf::CsInventoryItemSwapReq) -> Option<crate::tgf::ScInventoryItemSwapRsp> {
        self.inventory_item_swap_impl(req).await
    }

    // ---- gear, crafting, item use (item_handlers.rs) ----
    async fn handle_inventory_page_add(&self, req: crate::tgf::CsInventoryPageAddReq) -> Option<crate::tgf::ScInventoryPageAddRsp> {
        self.inventory_page_add_impl(req).await
    }
    async fn handle_use_item(&self, req: crate::tgf::CsItemUseReq) -> Option<crate::tgf::ScItemUseRsp> {
        self.use_item_impl(req).await
    }
    async fn handle_use_item_bath(&self, req: crate::tgf::CsBatchItemUseReq) -> Option<crate::tgf::ScBatchItemUseRsp> {
        self.use_item_bath_impl(req).await
    }
    async fn handle_crafting(&self, req: crate::tgf::CsCraftingReq) -> Option<crate::tgf::ScCraftingRsp> {
        self.crafting_impl(req).await
    }
    async fn handle_crafting_cancel(&self, req: crate::tgf::CsCraftingCancelReq) -> Option<crate::tgf::ScCraftingCancelRsp> {
        self.crafting_cancel_impl(req).await
    }
    async fn handle_crafting_recived(&self, req: crate::tgf::CsCraftingRecivedReq) -> Option<crate::tgf::ScCraftingRecivedRsp> {
        self.crafting_recived_impl(req).await
    }
    async fn handle_gear_repair(&self, req: crate::tgf::CsGearRepairReq) -> Option<crate::tgf::ScGearRepairRsp> {
        self.gear_repair_impl(req).await
    }
    async fn handle_gear_inlaid(&self, req: crate::tgf::CsGearInlaidReq) -> Option<crate::tgf::ScGearInlaidRsp> {
        self.gear_inlaid_impl(req).await
    }
    async fn handle_gear_dismantle(&self, req: crate::tgf::CsGearDismantleReq) -> Option<crate::tgf::ScGearDismantleRsp> {
        self.gear_dismantle_impl(req).await
    }
    async fn handle_gear_soul_bound(&self, req: crate::tgf::CsGearSoulBoundReq) -> Option<crate::tgf::ScGearSoulBoundRsp> {
        self.gear_soul_bound_impl(req).await
    }
    async fn handle_gear_heirloom_unlock(&self, req: crate::tgf::CsGearHeirloomUnlockReq) -> Option<crate::tgf::ScGearHeirloomUnlockRsp> {
        self.gear_heirloom_unlock_impl(req).await
    }
    async fn handle_gear_heirloom_unload(&self, req: crate::tgf::CsGearHeirloomUnloadReq) -> Option<crate::tgf::ScGearHeirloomUnloadRsp> {
        self.gear_heirloom_unload_impl(req).await
    }
    async fn handle_gear_heirloom_instantiation(&self, req: crate::tgf::CsGearHeirloomInstantiationReq) -> Option<crate::tgf::ScGearHeirloomInstantiationRsp> {
        self.gear_heirloom_instantiation_impl(req).await
    }
    async fn handle_gear_gem_unlock(&self, req: crate::tgf::CsGearGemUnlockReq) -> Option<crate::tgf::ScGearGemUnlockRsp> {
        self.gear_gem_unlock_impl(req).await
    }
    async fn handle_gear_heirloom_guide_inlaid_gem(&self, req: crate::tgf::CsGearHeirloomGuideInlaidGemReq) -> Option<crate::tgf::ScGearHeirloomGuideInlaidGemRsp> {
        self.gear_heirloom_guide_inlaid_gem_impl(req).await
    }
    async fn handle_gear_heirloom_guide_inlaid_clear(&self, req: crate::tgf::CsGearHeirloomGuideInlaidClearReq) -> Option<crate::tgf::ScGearHeirloomGuideInlaidClearRsp> {
        self.gear_heirloom_guide_inlaid_clear_impl(req).await
    }
    async fn handle_gear_insurance_lock(&self, req: crate::tgf::CsGearInsuranceLockReq) -> Option<crate::tgf::ScGearInsuranceLockRsp> {
        self.gear_insurance_lock_impl(req).await
    }
    async fn handle_gear_insurance_unlock(&self, req: crate::tgf::CsGearInsuranceUnLockReq) -> Option<crate::tgf::ScGearInsuranceUnLockRsp> {
        self.gear_insurance_unlock_impl(req).await
    }
    async fn handle_gear_insurance_price(&self, req: crate::tgf::CsGearInsurancePriceReq) -> Option<crate::tgf::ScGearInsurancePriceRsp> {
        self.gear_insurance_price_impl(req).await
    }

    // ---- level selection, matchmaking, battle room (battle_handlers.rs) ----
    async fn handle_battle_fetch_battle_user_data(
        &self,
        req: crate::tgf::CsBattleFetchBattleUserReq,
    ) -> Option<crate::tgf::ScBattleFetchBattleUserRsp> {
        self.battle_fetch_battle_user_data_impl(req).await
    }
    async fn handle_level_select(
        &self,
        req: crate::tgf::CsLevelCompetitionSelectReq,
    ) -> Option<crate::tgf::ScLevelCompetitionSelectRsp> {
        self.level_select_impl(req).await
    }
    async fn handle_battle_matching_pre_search(
        &self,
        req: crate::tgf::CsBattleMatchingPreSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingPreSearchRsp> {
        self.battle_matching_pre_search_impl(req).await
    }
    async fn handle_battle_matching_cancel_pre_search(
        &self,
        req: crate::tgf::CsBattleMatchingCancelPreSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingCancelPreSearchRsp> {
        self.battle_matching_cancel_pre_search_impl(req).await
    }
    async fn handle_battle_matching_search(
        &self,
        req: crate::tgf::CsBattleMatchingSearchReq,
    ) -> Option<crate::tgf::ScBattleMatchingSearchRsp> {
        self.battle_matching_search_impl(req).await
    }
    async fn handle_battle_matching_heartbeat(
        &self,
        req: crate::tgf::CsBattleMatchingHeartbeatReq,
    ) -> Option<crate::tgf::ScBattleMatchingHeartbeatRsp> {
        self.battle_matching_heartbeat_impl(req).await
    }
    async fn handle_battle_matching_exit(
        &self,
        req: crate::tgf::CsBattleMatchingExitReq,
    ) -> Option<crate::tgf::ScBattleMatchingExitRsp> {
        self.battle_matching_exit_impl(req).await
    }
    async fn handle_battle_matching_confirm_refuse(
        &self,
        req: crate::tgf::CsBattleMatchingConfirmRefuseReq,
    ) -> Option<crate::tgf::ScBattleMatchingConfirmRefuseRsp> {
        self.battle_matching_confirm_refuse_impl(req).await
    }
    async fn handle_battle_room_heartbeat(
        &self,
        req: crate::tgf::CsBattleRoomHeartbeatReq,
    ) -> Option<crate::tgf::ScBattleRoomHeartbeatRsp> {
        self.battle_room_heartbeat_impl(req).await
    }
    async fn handle_battle_room_player_act(
        &self,
        req: crate::tgf::CsBattleRoomPlayerActReq,
    ) -> Option<crate::tgf::ScBattleRoomPlayerActRsp> {
        self.battle_room_player_act_impl(req).await
    }
    async fn handle_create_game_session_result_event(
        &self,
        req: crate::tgf::CreateGameSessionResultEventReq,
    ) -> Option<crate::google::protobuf::Empty> {
        self.create_game_session_result_event_impl(req).await
    }

    // ---- party (lobby_handlers.rs) ----
    async fn handle_team_send_invitation(&self, req: crate::tgf::CsTeamSendInvitationReq) -> Option<crate::tgf::ScTeamSendInvitationRsp> {
        self.team_send_invitation_impl(req).await
    }
    async fn handle_team_approve_invitation(&self, req: crate::tgf::CsTeamApproveInvitationReq) -> Option<crate::tgf::ScTeamApproveInvitationRsp> {
        self.team_approve_invitation_impl(req).await
    }
    async fn handle_team_reject_invitation(&self, req: crate::tgf::CsTeamRejectInvitationReq) -> Option<crate::tgf::ScTeamRejectInvitationRsp> {
        self.team_reject_invitation_impl(req).await
    }
    async fn handle_team_exit(&self, req: crate::tgf::CsTeamExitReq) -> Option<crate::tgf::ScTeamExitRsp> {
        self.team_exit_impl(req).await
    }
    async fn handle_team_remove_member(&self, req: crate::tgf::CsTeamRemoveMemberReq) -> Option<crate::tgf::ScTeamRemoveMemberRsp> {
        self.team_remove_member_impl(req).await
    }
    async fn handle_team_transfer_captain(&self, req: crate::tgf::CsTeamTransferCaptainReq) -> Option<crate::tgf::ScTeamTransferCaptainRsp> {
        self.team_transfer_captain_impl(req).await
    }
    async fn handle_team_send_battle_confirm(&self, req: crate::tgf::CsTeamSendBattleConfirmReq) -> Option<crate::tgf::ScTeamSendBattleConfirmRsp> {
        self.team_send_battle_confirm_impl(req).await
    }
    async fn handle_team_send_battle_unconfirm(&self, req: crate::tgf::CsTeamSendBattleUnconfirmReq) -> Option<crate::tgf::ScTeamSendBattleUnconfirmRsp> {
        self.team_send_battle_unconfirm_impl(req).await
    }

    // ---- raid results (report_action) (raid_handlers.rs) ----
    async fn handle_report_action(
        &self,
        req: crate::tgf::CsReportActionReq,
    ) -> Option<crate::tgf::ScReportActionRsp> {
        self.report_action_impl(req).await
    }

    // ---- DS service (ds_handlers.rs) ----
    async fn handle_init_player(
        &self,
        req: crate::tgf::DsInitPlayerListReq,
    ) -> Option<crate::tgf::DsInitPlayerListRsp> {
        self.init_player_impl(req).await
    }
    async fn handle_heartbeat(
        &self,
        req: crate::tgf::DsHeartbeatReq,
    ) -> Option<crate::tgf::DsHeartbeatRsp> {
        self.heartbeat_impl(req).await
    }
    async fn handle_query_player_game_status(
        &self,
        req: crate::tgf::DsQueryPlayerGameStatusReq,
    ) -> Option<crate::tgf::DsQueryPlayerGameStatusRsp> {
        self.query_player_game_status_impl(req).await
    }
    async fn handle_player_self_quit(
        &self,
        req: crate::tgf::DsPlayerSelfQuitReq,
    ) -> Option<crate::tgf::DsPlayerSelfQuitRsp> {
        self.player_self_quit_impl(req).await
    }
    async fn handle_player_kickout(
        &self,
        req: crate::tgf::DsPlayerKickOutReq,
    ) -> Option<crate::tgf::DsPlayerKickOutRsp> {
        self.player_kickout_impl(req).await
    }
    async fn handle_player_kickout_off(
        &self,
        req: crate::tgf::DsPlayerKickOutOffReq,
    ) -> Option<crate::tgf::DsPlayerKickOutOffRsp> {
        self.player_kickout_off_impl(req).await
    }
    async fn handle_player_game_over(
        &self,
        req: crate::tgf::DsPlayerGameOverReq,
    ) -> Option<crate::tgf::DsPlayerGameOverRsp> {
        self.player_game_over_impl(req).await
    }
    async fn handle_hello_world(&self, req: crate::tgf::DsHelloWorldReq) -> Option<crate::tgf::DsHelloWorldRsp> {
        self.hello_world_impl(req).await
    }
    async fn handle_ds_end(&self, req: crate::tgf::DsDsEndReq) -> Option<crate::tgf::DsDsEndRsp> {
        self.ds_end_impl(req).await
    }
    async fn handle_player_online_heartbeat(&self, req: crate::tgf::DsPlayerOnlineHeartbeatReq) -> Option<crate::tgf::DsPlayerOnlineHeartbeatRsp> {
        self.player_online_heartbeat_impl(req).await
    }
    async fn handle_player_online_report(&self, req: crate::tgf::DsPlayerOnlineReportReq) -> Option<crate::tgf::DsPlayerOnlineReportRsp> {
        self.player_online_report_impl(req).await
    }

    // ---- mail, quests, rankings, levels (progress_handlers.rs) ----
    async fn handle_mail_get_all(&self, req: crate::tgf::CsMailGetAllReq) -> Option<crate::tgf::ScMailGetAllRsp> {
        self.mail_get_all_impl(req).await
    }
    async fn handle_mail_read(&self, req: crate::tgf::CsMailReadReq) -> Option<crate::tgf::ScMailReadRsp> {
        self.mail_read_impl(req).await
    }
    async fn handle_mail_read_all(&self, req: crate::tgf::CsMailReadAllReq) -> Option<crate::tgf::ScMailReadAllRsp> {
        self.mail_read_all_impl(req).await
    }
    async fn handle_mail_receive_attachment(&self, req: crate::tgf::CsMailReceiveAttachmentsReq) -> Option<crate::tgf::ScMailReceiveAttachmentsRsp> {
        self.mail_receive_attachment_impl(req).await
    }
    async fn handle_mail_receive_attachment_all(&self, req: crate::tgf::CsMailReceiveAttachmentsAllReq) -> Option<crate::tgf::ScMailReceiveAttachmentsAllRsp> {
        self.mail_receive_attachment_all_impl(req).await
    }
    async fn handle_mail_receive_attachment_partly(&self, req: crate::tgf::CsMailReceiveAttachmentsPartlyReq) -> Option<crate::tgf::ScMailReceiveAttachmentsPartlyRsp> {
        self.mail_receive_attachment_partly_impl(req).await
    }
    async fn handle_battle_record_list_get(&self, req: crate::tgf::CsBattleRecordListGetReq) -> Option<crate::tgf::ScBattleRecordListGetRsp> {
        self.battle_record_list_get_impl(req).await
    }
    async fn handle_battle_record_data_get(&self, req: crate::tgf::CsBattleRecordDataGetReq) -> Option<crate::tgf::ScBattleRecordDataGetRsp> {
        self.battle_record_data_get_impl(req).await
    }
    async fn handle_battle_record_data_set(&self, req: crate::tgf::CsBattleRecordDataSetReq) -> Option<crate::tgf::ScBattleRecordDataSetRsp> {
        self.battle_record_data_set_impl(req).await
    }
    async fn handle_battle_update_setting(&self, req: crate::tgf::CsBattleUpdateSettingReq) -> Option<crate::tgf::ScBattleRoomPlayerActRsp> {
        self.battle_update_setting_impl(req).await
    }
    async fn handle_level_prestart(&self, req: crate::tgf::CsLevelPrestartReq) -> Option<crate::tgf::ScLevelPrestartRsp> {
        self.level_prestart_impl(req).await
    }
    async fn handle_level_start(&self, req: crate::tgf::CsLevelStartReq) -> Option<crate::tgf::ScLevelStartRsp> {
        self.level_start_impl(req).await
    }
    async fn handle_level_finish(&self, req: crate::tgf::CsLevelFinishReq) -> Option<crate::tgf::ScLevelFinishRsp> {
        self.level_finish_impl(req).await
    }
    async fn handle_level_challenge(&self, req: crate::tgf::CsLevelChallengeReq) -> Option<crate::tgf::ScLevelChallengeRsp> {
        self.level_challenge_impl(req).await
    }
    async fn handle_map_move(&self, req: crate::tgf::CsMapMoveReq) -> Option<crate::tgf::ScMapMoveRsp> {
        self.map_move_impl(req).await
    }
    async fn handle_quest_rewards_receive(&self, req: crate::tgf::CsQuestRewardsReceiveReq) -> Option<crate::tgf::ScQuestRewardsReceiveRsp> {
        self.quest_rewards_receive_impl(req).await
    }
    async fn handle_quest_special_progress_report(&self, req: crate::tgf::CsQuestSpecialProgressReportReq) -> Option<crate::tgf::ScQuestSpecialProgressReportRsp> {
        self.quest_special_progress_report_impl(req).await
    }
    async fn handle_quest_item_submit(&self, req: crate::tgf::CsQuestItemSubmitReq) -> Option<crate::tgf::ScQuestItemSubmitRsp> {
        self.quest_item_submit_impl(req).await
    }
    async fn handle_rank_get_specify_rank(&self, req: crate::tgf::CsRankGetSpecifyRankReq) -> Option<crate::tgf::ScRankGetSpecifyRankRsp> {
        self.rank_get_specify_rank_impl(req).await
    }
    async fn handle_rank_get_self_rank(&self, req: crate::tgf::CsRankGetSelfRankReq) -> Option<crate::tgf::ScRankGetSelfRankRsp> {
        self.rank_get_self_rank_impl(req).await
    }
    async fn handle_lottery_draw(&self, req: crate::tgf::CsLotteryDrawReq) -> Option<crate::tgf::ScLotteryDrawRsp> {
        self.lottery_draw_impl(req).await
    }

    // ---- shop, activities, mithril (shop_handlers.rs) ----
    async fn handle_market_get_product_list(&self, req: crate::tgf::CsMarketGetProductListReq) -> Option<crate::tgf::ScMarketGetProductListRsp> {
        self.market_get_product_list_impl(req).await
    }
    async fn handle_market_buy_product(&self, req: crate::tgf::CsMarketBuyProductReq) -> Option<crate::tgf::ScMarketBuyProductRsp> {
        self.market_buy_product_impl(req).await
    }
    async fn handle_market_sell_items(&self, req: crate::tgf::CsMarketSellItemsReq) -> Option<crate::tgf::ScMarketSellItemsRsp> {
        self.market_sell_items_impl(req).await
    }
    async fn handle_market_buy_sold_item(&self, req: crate::tgf::CsMarketBuySoldItemReq) -> Option<crate::tgf::ScMarketBuySoldItemRsp> {
        self.market_buy_sold_item_impl(req).await
    }
    async fn handle_market_reset_refresh_effect(&self, req: crate::tgf::CsMarketResetRefreshEffectReq) -> Option<crate::tgf::ScMarketResetRefreshEffectRsp> {
        self.market_reset_refresh_effect_impl(req).await
    }
    async fn handle_market_video_popup(&self, req: crate::tgf::CsMarketVideoPopUpReq) -> Option<crate::tgf::ScMarketVideoPopUpRsp> {
        self.market_video_popup_impl(req).await
    }
    async fn handle_activity_center_list(&self, req: crate::tgf::CsActivityCenterListReq) -> Option<crate::tgf::ScActivityCenterListRsp> {
        self.activity_center_list_impl(req).await
    }
    async fn handle_activity_center_random_rewards(&self, req: crate::tgf::CsActivityRandomRewardReq) -> Option<crate::tgf::ScActivityRandomRewardRsp> {
        self.activity_center_random_rewards_impl(req).await
    }
    async fn handle_activity_center_random_reward_selected(&self, req: crate::tgf::CsActivityRandomRewardSelectdReq) -> Option<crate::tgf::ScActivityRandomRewardSelectdRsp> {
        self.activity_center_random_reward_selected_impl(req).await
    }
    async fn handle_activity_center_buy_passport(&self, req: crate::tgf::CsActivityCenterBuyPassportReq) -> Option<crate::tgf::ScActivityCenterBuyPassportRsp> {
        self.activity_center_buy_passport_impl(req).await
    }
    async fn handle_activity_center_passport_receive_rewards(&self, req: crate::tgf::CsActivityCenterPassportReceiveRewardsReq) -> Option<crate::tgf::ScActivityCenterPassportReceiveRewardsRsp> {
        self.activity_center_passport_receive_rewards_impl(req).await
    }
    async fn handle_activity_center_passport_receive_cycle_rewards(&self, req: crate::tgf::CsActivityCenterPassportReceiveCycleRewardsReq) -> Option<crate::tgf::ScActivityCenterPassportReceiveCycleRewardsRsp> {
        self.activity_center_passport_receive_cycle_rewards_impl(req).await
    }
    async fn handle_activity_center_passport_refresh_quest(&self, req: crate::tgf::CsActivityCenterPassportRefreshQuestReq) -> Option<crate::tgf::ScActivityCenterPassportRefreshQuestRsp> {
        self.activity_center_passport_refresh_quest_impl(req).await
    }
    async fn handle_activity_center_trial_open(&self, req: crate::tgf::CsActivityCenteTrialOpenReq) -> Option<crate::tgf::ScActivityCenterTrialOpenRsp> {
        self.activity_center_trial_open_impl(req).await
    }
    async fn handle_activity_center_trial_self_quit(&self, req: crate::tgf::CsActivityCenterTrialSelfQuitReq) -> Option<crate::tgf::ScActivityCenterTrialSelfQuitRsp> {
        self.activity_center_trial_self_quit_impl(req).await
    }
    async fn handle_activity_center_trial_receive_reward(&self, req: crate::tgf::CsActivityCenterTrialReceiveRewardReq) -> Option<crate::tgf::ScActivityCenterTrialReceiveRewardRsp> {
        self.activity_center_trial_receive_reward_impl(req).await
    }
    async fn handle_activity_center_trial_history_record(&self, req: crate::tgf::CsActivityCenterTrialHistoryRecordReq) -> Option<crate::tgf::ScActivityCenterTrialHistoryRecordRsp> {
        self.activity_center_trial_history_record_impl(req).await
    }
    async fn handle_mithril_force_role_init(&self, req: crate::tgf::CsMithrilForceRoleInitReq) -> Option<crate::tgf::ScMithrilForceRoleInitRsp> {
        self.mithril_force_role_init_impl(req).await
    }
    async fn handle_mithril_force_role_select(&self, req: crate::tgf::CsMithrilForceRoleSelectReq) -> Option<crate::tgf::ScMithrilForceRoleSelectRsp> {
        self.mithril_force_role_select_impl(req).await
    }
    async fn handle_mithril_force_receive_reward(&self, req: crate::tgf::CsMithrilForceReceiveRewardReq) -> Option<crate::tgf::ScMithrilForceReceiveRewardRsp> {
        self.mithril_force_receive_reward_impl(req).await
    }
    async fn handle_mithril_role_wear(&self, req: crate::tgf::CsMithrilRoleWearReq) -> Option<crate::tgf::ScMithrilRoleWearRsp> {
        self.mithril_role_wear_impl(req).await
    }

    // ---- friends, chat (social_handlers.rs) ----
    async fn handle_friend_get_all(&self, req: crate::tgf::CsFriendGetAllReq) -> Option<crate::tgf::ScFriendGetAllRsp> {
        self.friend_get_all_impl(req).await
    }
    async fn handle_friend_invite(&self, req: crate::tgf::CsFriendInviteReq) -> Option<crate::tgf::ScFriendInviteRsp> {
        self.friend_invite_impl(req).await
    }
    async fn handle_friend_accept_invite(&self, req: crate::tgf::CsFriendAcceptInviteReq) -> Option<crate::tgf::ScFriendAcceptInviteRsp> {
        self.friend_accept_invite_impl(req).await
    }
    async fn handle_friend_reject_invite(&self, req: crate::tgf::CsFriendRejectInviteReq) -> Option<crate::tgf::ScFriendRejectInviteRsp> {
        self.friend_reject_invite_impl(req).await
    }
    async fn handle_friend_remove(&self, req: crate::tgf::CsFriendRemoveReq) -> Option<crate::tgf::ScFriendRemoveRsp> {
        self.friend_remove_impl(req).await
    }
    async fn handle_friend_send_gift(&self, req: crate::tgf::CsFriendSendGiftReq) -> Option<crate::tgf::ScFriendSendGiftRsp> {
        self.friend_send_gift_impl(req).await
    }
    async fn handle_friend_receive_gift(&self, req: crate::tgf::CsFriendReceiveGiftReq) -> Option<crate::tgf::ScFriendReceiveGiftRsp> {
        self.friend_receive_gift_impl(req).await
    }
    async fn handle_friend_get_suggest(&self, req: crate::tgf::CsFriendGetSuggestReq) -> Option<crate::tgf::ScFriendGetSuggestRsp> {
        self.friend_get_suggest_impl(req).await
    }
    async fn handle_friend_accept_all_invite(&self, req: crate::tgf::CsFriendAcceptAllInviteReq) -> Option<crate::tgf::ScFriendAcceptAllInviteRsp> {
        self.friend_accept_all_invite_impl(req).await
    }
    async fn handle_friend_reject_all_invite(&self, req: crate::tgf::CsFriendRejectAllInviteReq) -> Option<crate::tgf::ScFriendRejectAllInviteRsp> {
        self.friend_reject_all_invite_impl(req).await
    }
    async fn handle_chat_get_all_channels(&self, req: crate::tgf::CsChatGetAllChannelsReq) -> Option<crate::tgf::ScChatGetAllChannelsRsp> {
        self.chat_get_all_channels_impl(req).await
    }
    async fn handle_chat_get_channel_snapshot(&self, req: crate::tgf::CsChatGetChannelSnapshotReq) -> Option<crate::tgf::ScChatGetChannelSnapshotRsp> {
        self.chat_get_channel_snapshot_impl(req).await
    }
    async fn handle_chat_channel_heartbeat(&self, req: crate::tgf::CsChatChannelHeartbeatReq) -> Option<crate::tgf::ScChatChannelHeartbeatRsp> {
        self.chat_channel_heartbeat_impl(req).await
    }
    async fn handle_chat_send_message(&self, req: crate::tgf::CsChatSendMessageReq) -> Option<crate::tgf::ScChatSendMessageRsp> {
        self.chat_send_message_impl(req).await
    }
    async fn handle_chat_update_panel(&self, req: crate::tgf::CsChatUpdatePanelReq) -> Option<crate::tgf::ScChatUpdatePanelRsp> {
        self.chat_update_panel_impl(req).await
    }

    // ---- guilds (guild_handlers.rs) ----
    async fn handle_guild_create(&self, req: crate::tgf::CsGuildCreateReq) -> Option<crate::tgf::ScGuildCreateRsp> {
        self.guild_create_impl(req).await
    }
    async fn handle_guild_exit(&self, req: crate::tgf::CsGuildExitReq) -> Option<crate::tgf::ScGuildExitRsp> {
        self.guild_exit_impl(req).await
    }
    async fn handle_guild_pay_off(&self, req: crate::tgf::CsGuildPayOffReq) -> Option<crate::tgf::ScGuildPayOffRsp> {
        self.guild_pay_off_impl(req).await
    }
    async fn handle_guild_get_all(&self, req: crate::tgf::CsGuildGetAllReq) -> Option<crate::tgf::ScGuildGetAllRsp> {
        self.guild_get_all_impl(req).await
    }
    async fn handle_guild_batch_get(&self, req: crate::tgf::CsGuildBatchGetReq) -> Option<crate::tgf::ScGuildBatchGetRsp> {
        self.guild_batch_get_impl(req).await
    }
    async fn handle_guild_suggest(&self, req: crate::tgf::CsGuildSuggestReq) -> Option<crate::tgf::ScGuildSuggestRsp> {
        self.guild_suggest_impl(req).await
    }
    async fn handle_guild_set_role(&self, req: crate::tgf::CsGuildSetRoleReq) -> Option<crate::tgf::ScGuildSetRoleRsp> {
        self.guild_set_role_impl(req).await
    }
    async fn handle_guild_rename(&self, req: crate::tgf::CsGuildRenameReq) -> Option<crate::tgf::ScGuildRenameRsp> {
        self.guild_rename_impl(req).await
    }
    async fn handle_guild_set_configure(&self, req: crate::tgf::CsGuildSetConfigureReq) -> Option<crate::tgf::ScGuildSetConfigureRsp> {
        self.guild_set_configure_impl(req).await
    }
    async fn handle_guild_send_apply(&self, req: crate::tgf::CsGuildSendApplyReq) -> Option<crate::tgf::ScGuildSendApplyRsp> {
        self.guild_send_apply_impl(req).await
    }
    async fn handle_guild_approve_apply(&self, req: crate::tgf::CsGuildApproveApplyReq) -> Option<crate::tgf::ScGuildApproveApplyRsp> {
        self.guild_approve_apply_impl(req).await
    }
    async fn handle_guild_reject_apply(&self, req: crate::tgf::CsGuildRejectApplyReq) -> Option<crate::tgf::ScGuildRejectApplyRsp> {
        self.guild_reject_apply_impl(req).await
    }
    async fn handle_guild_send_invitation(&self, req: crate::tgf::CsGuildSendInvitationReq) -> Option<crate::tgf::ScGuildSendInvitationRsp> {
        self.guild_send_invitation_impl(req).await
    }
    async fn handle_guild_approve_invitation(&self, req: crate::tgf::CsGuildApproveInvitationReq) -> Option<crate::tgf::ScGuildApproveInvitationRsp> {
        self.guild_approve_invitation_impl(req).await
    }
    async fn handle_guild_reject_invitation(&self, req: crate::tgf::CsGuildRejectInvitationReq) -> Option<crate::tgf::ScGuildRejectInvitationRsp> {
        self.guild_reject_invitation_impl(req).await
    }
    async fn handle_guild_revoke_invitation(&self, req: crate::tgf::CsGuildRevokeInvitationReq) -> Option<crate::tgf::ScGuildRevokeInvitationRsp> {
        self.guild_revoke_invitation_impl(req).await
    }
    async fn handle_guild_remove_member(&self, req: crate::tgf::CsGuildRemoveMemberReq) -> Option<crate::tgf::ScGuildRemoveMemberRsp> {
        self.guild_remove_member_impl(req).await
    }
    async fn handle_guild_add_impeachment(&self, req: crate::tgf::CsGuildAddImpeachmentReq) -> Option<crate::tgf::ScGuildAddImpeachmentRsp> {
        self.guild_add_impeachment_impl(req).await
    }
    async fn handle_guild_vote_impeachment(&self, req: crate::tgf::CsGuildVoteImpeachmentReq) -> Option<crate::tgf::ScGuildVoteImpeachmentRsp> {
        self.guild_vote_impeachment_impl(req).await
    }

    // ---- rooms, party search (room_handlers.rs) ----
    async fn handle_team_match_exit(
        &self,
        req: crate::tgf::CsTeamMatchExitReq,
    ) -> Option<crate::tgf::ScTeamMatchExitRsp> {
        self.team_match_exit_impl(req).await
    }
    async fn handle_team_apply_join_team(&self, req: crate::tgf::CsTeamApplyJoinTeamReq) -> Option<crate::tgf::ScTeamApplyJoinTeamRsp> {
        self.team_apply_join_team_impl(req).await
    }
    async fn handle_team_accept_apply(&self, req: crate::tgf::CsTeamAcceptApplyReq) -> Option<crate::tgf::ScTeamAcceptApplyRsp> {
        self.team_accept_apply_impl(req).await
    }
    async fn handle_team_change_team_finder(&self, req: crate::tgf::CsTeamChangeTeamFinderReq) -> Option<crate::tgf::ScTeamChangeTeamFinderRsp> {
        self.team_change_team_finder_impl(req).await
    }
    async fn handle_team_reject_apply(&self, req: crate::tgf::CsTeamRejectApplyReq) -> Option<crate::tgf::ScTeamRejectApplyRsp> {
        self.team_reject_apply_impl(req).await
    }
    async fn handle_team_match_search(&self, req: crate::tgf::CsTeamSearchReq) -> Option<crate::tgf::ScTeamSearchRsp> {
        self.team_match_search_impl(req).await
    }
    async fn handle_team_match_check(&self, req: crate::tgf::CsTeamCheckReq) -> Option<crate::tgf::ScTeamCheckRsp> {
        self.team_match_check_impl(req).await
    }
    async fn handle_room_create(&self, req: crate::tgf::CsRoomCreateReq) -> Option<crate::tgf::ScRoomCreateRsp> {
        self.room_create_impl(req).await
    }
    async fn handle_room_enter(&self, req: crate::tgf::CsRoomEnterReq) -> Option<crate::tgf::ScRoomEnterRsp> {
        self.room_enter_impl(req).await
    }
    async fn handle_room_leave(&self, req: crate::tgf::CsRoomLeaveReq) -> Option<crate::tgf::ScRoomLeaveRsp> {
        self.room_leave_impl(req).await
    }
    async fn handle_room_disband(&self, req: crate::tgf::CsRoomDisbandReq) -> Option<crate::tgf::ScRoomDisbandRsp> {
        self.room_disband_impl(req).await
    }
    async fn handle_room_change_myself_pos(&self, req: crate::tgf::CsRoomChangeMySelfPosReq) -> Option<crate::tgf::ScRoomChangeMySelfPosRsp> {
        self.room_change_myself_pos_impl(req).await
    }
    async fn handle_room_change_member_pos(&self, req: crate::tgf::CsRoomChangeMemberPosReq) -> Option<crate::tgf::ScRoomChangeMemberPosRsp> {
        self.room_change_member_pos_impl(req).await
    }
    async fn handle_room_select_battle_map(&self, req: crate::tgf::CsRoomSelectBattleMapReq) -> Option<crate::tgf::ScRoomSelectBattleMapRsp> {
        self.room_select_battle_map_impl(req).await
    }
    async fn handle_room_select_battle_type(&self, req: crate::tgf::CsRoomSelectBattleTypeReq) -> Option<crate::tgf::ScRoomSelectBattleTypeRsp> {
        self.room_select_battle_type_impl(req).await
    }
    async fn handle_room_get_ready(&self, req: crate::tgf::CsRoomGetReadyReq) -> Option<crate::tgf::ScRoomGetReadyRsp> {
        self.room_get_ready_impl(req).await
    }
    async fn handle_room_cancel_ready(&self, req: crate::tgf::CsRoomCancelReadyReq) -> Option<crate::tgf::ScRoomCancelReadyRsp> {
        self.room_cancel_ready_impl(req).await
    }
    async fn handle_room_start_battle(&self, req: crate::tgf::CsRoomStartBattleReq) -> Option<crate::tgf::ScRoomStartBattleRsp> {
        self.room_start_battle_impl(req).await
    }
    async fn handle_room_name_change(&self, req: crate::tgf::CsRoomNameChangeReq) -> Option<crate::tgf::ScRoomNameChangeRsp> {
        self.room_name_change_impl(req).await
    }
    async fn handle_room_keyword_change(&self, req: crate::tgf::CsRoomKeywordChangeReq) -> Option<crate::tgf::ScRoomKeywordChangeRsp> {
        self.room_keyword_change_impl(req).await
    }
    async fn handle_room_transfer_owner(&self, req: crate::tgf::CsRoomTransferOwnerReq) -> Option<crate::tgf::ScRoomTransferOwnerRsp> {
        self.room_transfer_owner_impl(req).await
    }
    async fn handle_room_kickoff_member(&self, req: crate::tgf::CsRoomKickOffMemberReq) -> Option<crate::tgf::ScRoomKickOffMemberRsp> {
        self.room_kickoff_member_impl(req).await
    }
    async fn handle_room_suggest(&self, req: crate::tgf::CsRoomSuggestReq) -> Option<crate::tgf::ScRoomSuggestRsp> {
        self.room_suggest_impl(req).await
    }
    async fn handle_room_batch_get(&self, req: crate::tgf::CsRoomBatchGetReq) -> Option<crate::tgf::ScRoomBatchGetRsp> {
        self.room_batch_get_impl(req).await
    }
    async fn handle_room_accept_invite(&self, req: crate::tgf::CsRoomInviteAcceptReq) -> Option<crate::tgf::ScRoomInviteAcceptRsp> {
        self.room_accept_invite_impl(req).await
    }
    async fn handle_room_send_invite(&self, req: crate::tgf::CsRoomInviteSendReq) -> Option<crate::tgf::ScRoomInviteSendRsp> {
        self.room_send_invite_impl(req).await
    }
    async fn handle_room_reject_invite(&self, req: crate::tgf::CsRoomInviteRejectReq) -> Option<crate::tgf::ScRoomInviteRejectRsp> {
        self.room_reject_invite_impl(req).await
    }
    async fn handle_room_ignore_room_all_invite(&self, req: crate::tgf::CsRoomIgnoreRoomAllInviteReq) -> Option<crate::tgf::ScRoomIgnoreRoomAllInviteRsp> {
        self.room_ignore_room_all_invite_impl(req).await
    }
    async fn handle_room_ignore_all_user_invite(&self, req: crate::tgf::CsRoomIgnoreAllUserInviteReq) -> Option<crate::tgf::ScRoomIgnoreAllUserInviteRsp> {
        self.room_ignore_all_user_invite_impl(req).await
    }

    // ---- trade, auction (trade_handlers.rs) ----
    async fn handle_trade_product_price_basic_info(&self, req: crate::tgf::CsTradeProductPriceBasicInfoReq) -> Option<crate::tgf::ScTradeProductPriceBasicInfoRsp> {
        self.trade_product_price_basic_info_impl(req).await
    }
    async fn handle_trade_product_price_info(&self, req: crate::tgf::CsTradeProductPriceInfoReq) -> Option<crate::tgf::ScTradeProductPriceInfoRsp> {
        self.trade_product_price_info_impl(req).await
    }
    async fn handle_trade_product_percentage_info(&self, req: crate::tgf::CsTradeProductPercentageInfoReq) -> Option<crate::tgf::ScTradeProductPercentageInfoRsp> {
        self.trade_product_percentage_info_impl(req).await
    }
    async fn handle_trade_purchase_order_publish(&self, req: crate::tgf::CsTradePurchaseOrderPublishReq) -> Option<crate::tgf::ScTradePurchaseOrderPublishRsp> {
        self.trade_purchase_order_publish_impl(req).await
    }
    async fn handle_trade_sales_order_publish(&self, req: crate::tgf::CsTradeSalesOrderPublishReq) -> Option<crate::tgf::ScTradeSalesOrderPublishRsp> {
        self.trade_sales_order_publish_impl(req).await
    }
    async fn handle_trade_bid_order_publish(&self, req: crate::tgf::CsTradeBidOrderPublishReq) -> Option<crate::tgf::ScTradeBidOrderPublishRsp> {
        self.trade_bid_order_publish_impl(req).await
    }
    async fn handle_trade_auction_order_publish(&self, req: crate::tgf::CsTradeAuctionOrderPublishReq) -> Option<crate::tgf::ScTradeAuctionOrderPublishRsp> {
        self.trade_auction_order_publish_impl(req).await
    }
    async fn handle_trade_auction_fetch_order_info(&self, req: crate::tgf::CsTradeAuctionFetchOrderInfoReq) -> Option<crate::tgf::ScTradeAuctionFetchOrderInfoRsp> {
        self.trade_auction_fetch_order_info_impl(req).await
    }
    async fn handle_trade_auction_index(&self, req: crate::tgf::CsTradeAuctionIndexReq) -> Option<crate::tgf::ScTradeAuctionIndexRsp> {
        self.trade_auction_index_impl(req).await
    }
    async fn handle_trade_order_cancel(&self, req: crate::tgf::CsTradeOrderCancelReq) -> Option<crate::tgf::ScTradeOrderCancelRsp> {
        self.trade_order_cancel_impl(req).await
    }
    async fn handle_trade_order_history(&self, req: crate::tgf::CsTradeOrderHistoryReq) -> Option<crate::tgf::ScTradeOrderHistoryRsp> {
        self.trade_order_history_impl(req).await
    }
    async fn handle_trade_order_delete(&self, req: crate::tgf::CsTradeOrderDeleteReq) -> Option<crate::tgf::ScTradeOrderDeleteRsp> {
        self.trade_order_delete_impl(req).await
    }
    async fn handle_trade_receive_confirmation_product(&self, req: crate::tgf::CsTradeReceiveConfirmationProductReq) -> Option<crate::tgf::ScTradeReceiveConfirmationProductRsp> {
        self.trade_receive_confirmation_product_impl(req).await
    }
    async fn handle_trade_auction_recommend_order_ticket(&self, req: crate::tgf::CsTradeAuctionRecommendOrderTicketReq) -> Option<crate::tgf::ScTradeAuctionRecommendOrderTicketRsp> {
        self.trade_auction_recommend_order_ticket_impl(req).await
    }
    async fn handle_trade_product_recommend_order_ticket(&self, req: crate::tgf::CsTradeProductRecommendOrderTicketReq) -> Option<crate::tgf::ScTradeProductRecommendOrderTicketRsp> {
        self.trade_product_recommend_order_ticket_impl(req).await
    }
    async fn handle_trade_product_search_order_ticket(&self, req: crate::tgf::CsTradeProductSearchOrderTicketReq) -> Option<crate::tgf::ScTradeProductSearchOrderTicketRsp> {
        self.trade_product_search_order_ticket_impl(req).await
    }
    async fn handle_trade_career_rarity_affix_gear_set_price(&self, req: crate::tgf::CsTradeCareerRarityAffixGearsetPriceReq) -> Option<crate::tgf::ScTradeCareerRarityAffixGearsetPriceRsp> {
        self.trade_career_rarity_affix_gear_set_price_impl(req).await
    }

    // ---- service, payments, DLC (misc_handlers.rs) ----
    async fn handle_pay_get_all(
        &self,
        req: crate::tgf::CsPayGetAllReq,
    ) -> Option<crate::tgf::ScPayGetAllRsp> {
        self.pay_get_all_impl(req).await
    }
    async fn handle_client_action_log(&self, req: crate::tgf::CsCommonLoggingReq) -> Option<crate::tgf::ScCommonLoggingRsp> {
        self.client_action_log_impl(req).await
    }
    async fn handle_ping(&self, req: crate::tgf::CsPingReq) -> Option<crate::tgf::ScPongRsp> {
        self.ping_impl(req).await
    }
    async fn handle_business_ping(&self, req: crate::tgf::CsBusinessPingReq) -> Option<crate::tgf::ScBusinessPongRsp> {
        self.business_ping_impl(req).await
    }
    async fn handle_network_latency_report(&self, req: crate::tgf::CsNetworkLatencyReportReq) -> Option<crate::tgf::ScNetworkLatencyReportRsp> {
        self.network_latency_report_impl(req).await
    }
    async fn handle_network_block_loading(&self, req: crate::tgf::CsNetworkBlockLoadingReq) -> Option<crate::tgf::ScNetworkBlockLoadingRsp> {
        self.network_block_loading_impl(req).await
    }
    async fn handle_update_account_data(&self, req: crate::tgf::CsUpdateAccountDataReq) -> Option<crate::tgf::ScUpdateAccountDataRsp> {
        self.update_account_data_impl(req).await
    }
    async fn handle_telemetry_report(&self, req: crate::tgf::CsTelemetryReportReq) -> Option<crate::tgf::ScTelemetryReportRsp> {
        self.telemetry_report_impl(req).await
    }
    async fn handle_player_ace_packet(&self, req: crate::tgf::CsPlayerAcePacketReq) -> Option<crate::tgf::ScPlayerAcePacketRsp> {
        self.player_ace_packet_impl(req).await
    }
    async fn handle_player_uri_redirect(&self, req: crate::tgf::CsPlayerRedirectReq) -> Option<crate::tgf::ScPlayerRedirectRsp> {
        self.player_uri_redirect_impl(req).await
    }
    async fn handle_cache_unwatch(&self, req: crate::tgf::CsCacheUnwatchReq) -> Option<crate::tgf::ScCacheUnwatchRsp> {
        self.cache_unwatch_impl(req).await
    }
    async fn handle_pay_update_diamond(&self, req: crate::tgf::CsPayUpdateDiamondReq) -> Option<crate::tgf::ScPayUpdateDiamondRsp> {
        self.pay_update_diamond_impl(req).await
    }
    async fn handle_pay_direct_buy(&self, req: crate::tgf::CsPayDirectBuyProductReq) -> Option<crate::tgf::ScPayDirectBuyProductRsp> {
        self.pay_direct_buy_impl(req).await
    }
    async fn handle_pay_check_bill_applied(&self, req: crate::tgf::CsPayCheckBillAppliedReq) -> Option<crate::tgf::ScPayCheckBillAppliedRsp> {
        self.pay_check_bill_applied_impl(req).await
    }
    async fn handle_pay_update_authorization_data(&self, req: crate::tgf::CsPayUpdateAuthorizationDataReq) -> Option<crate::tgf::ScPayUpdateAuthorizationDataRsp> {
        self.pay_update_authorization_data_impl(req).await
    }
    async fn handle_pay_cancel(&self, req: crate::tgf::CsPayCancelReq) -> Option<crate::tgf::ScPayCancelRsp> {
        self.pay_cancel_impl(req).await
    }
    async fn handle_pay_exchange_free_diamond(&self, req: crate::tgf::CsPayExchangeFreeDiamondReq) -> Option<crate::tgf::ScPayExchangeFreeDiamondRsp> {
        self.pay_exchange_free_diamond_impl(req).await
    }
    async fn handle_pay_submit_pending_flow(&self, req: crate::tgf::CsPaySubmitPendingFlowReq) -> Option<crate::tgf::ScPaySubmitPendingFlowRsp> {
        self.pay_submit_pending_flow_impl(req).await
    }
    async fn handle_hope_report_execute(&self, req: crate::tgf::CsHopeReportExecuteReq) -> Option<crate::tgf::ScHopeReportExecuteRsp> {
        self.hope_report_execute_impl(req).await
    }
    async fn handle_dlc_receive_packet(&self, req: crate::tgf::CsReceiveDlcPacketReq) -> Option<crate::tgf::ScReceiveDlcPacketRsp> {
        self.dlc_receive_packet_impl(req).await
    }
    async fn handle_refresh_dlc_info(&self, req: crate::tgf::CsRefreshDlcInfoReq) -> Option<crate::tgf::ScRefreshDlcInfoRsp> {
        self.refresh_dlc_info_impl(req).await
    }
}
