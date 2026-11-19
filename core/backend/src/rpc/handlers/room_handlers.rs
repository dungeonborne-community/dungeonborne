//! Private rooms (room_*) and party search (team_* beyond lobby_handlers.rs).
//! The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`). State — crate::lobby::{room, team}.
//!
//! Format facts (there are no traffic dumps for room_* or for these team_* — only proto and UE headers):
//! - all room_* responses are empty and without ret_code (except room_enter → is_invite, suggest → room_keys, batch_get → rooms).
//!   Error: an EnErrorCode in RpcHead.error_code (set_rpc_error) + log + the room snapshot sent again to the initiator.
//! - Client Lua (tools/research/room/lua_orig, PVP/*): WBP_PVP_RoomWIndow.SetPlayers walks TeamMembers[i] ->
//!   PlayerSlots[i] and checks UserKey:GetIsValid() — empty slots in team_member are expected; slots — Team_1 and
//!   Team_2 with 3 each (Leader/Member1/Member2). Room list: a non-empty RoomPassword -> a lock and a password window
//!   (the server checks the password). Text search: a number -> room_key, otherwise room_name in room_batch_get.
//!   Maps/modes — DataTable PVPMapConfigDataTable/PVPModeConfigDataTable (MapId/ModeId, values unknown).
//! - room state — push ScPlayerDirtyChgSync { dirty_private_room_snapshot, dirty_private_room_action }
//!   to all members (see lobby::room). Invitation — only action invite_notify_action to the recipient.
//! - room_start_battle: lobby::matchmaking::start_direct (a server from the allocator right away) → the matchmaker tick sends
//!   battle_matching_log_sync (status 4 + room_key) → battle_room_heartbeat returns DsInitFinish, as for a match
//!   from the queue. Room members additionally get start_battle_action. Return to preparation — room::tick,
//!   when the battle room was closed by ds_end (misc_handlers) or the timeout expired.
//! - team finder (UTeamService: SetFinderStatus, SearchTeam(MinRoleLevel, RoleTypeList, ModeTypeList,
//!   ServiceTypeList), timer OnTeamSearchTimer, UTeamInvite.bIsApplyJoin): applications — DTeamStorage.pending_applies,
//!   status — dirty_team_finder_status, search — dirty_team_match_data + dirty_team_match_action (Matched{team_key}).
//!   team_match_search/check is a PARTY search (not a match search): DTeamMatchEventLog.matched carries only team_key.
//!   team_match_check is treated as the search heartbeat (every call tries to match a party).
//!   team_match_exit cancels the search (team::match_cancel); otherwise — by the TEAM_MATCH_STALE_SECS timeout or on disconnect.

use super::GatewaySession;
use crate::lobby::{hub, room, team};
use crate::structures::PlayerModel;
use crate::tgf;

/// Full deploy_role of the active character (DRoomMember.deploy_role)
fn room_deploy(player: &PlayerModel) -> Option<tgf::DBattleDeployRole> {
    player
        .roles
        .iter()
        .find(|r| r.role_guid == player.hall_role_guid)
        .or_else(|| player.roles.first())
        .and_then(|r| r.data.deploy_role.clone())
}

/// Player profile for party search filters
fn team_profile(player: &PlayerModel) -> team::Profile {
    let role = super::lobby_handlers::team_deploy_role(player);
    let lvl = GatewaySession::get_saved_level_select(player);
    team::Profile {
        role_type_id: role.as_ref().map(|r| r.role_type_id).unwrap_or_default(),
        role_level: role.as_ref().map(|r| r.role_level).unwrap_or_default(),
        level_type: lvl.level_type,
        region: lvl.region,
    }
}

fn search_target(t: Option<tgf::DTeamSearchTarget>) -> team::SearchTarget {
    let t = t.unwrap_or_default();
    team::SearchTarget { type_ids: t.type_id, level_types: t.level_type, min_level: t.min_level, regions: t.region }
}

/// lobby::team errors -> EnErrorCode (for RpcHead.error_code)
fn team_err_code(e: &str) -> i32 {
    match e {
        "no_team" | "gone" => -3224,     // EN_ERR_TEAM_NOT_FOUND
        "already_member" => -3206,        // EN_ERR_TEAM_ALREADY_MEMBER
        "full" => -3212,                  // EN_ERR_TEAM_MEMBER_COUNT_EXCEEDED
        "not_captain" => -3207,           // EN_ERR_TEAM_ACCESS_DENY
        "no_apply" => -3230,              // EN_ERR_TEAM_APPLY_HAS_EXPIRED
        _ => -3226,                       // EN_ERR_TEAM_JOIN_TEAM_FAILED
    }
}

fn room_id_of(k: &Option<tgf::DRoomKey>) -> i64 {
    k.as_ref().map(|k| k.room_id).unwrap_or_default()
}

impl GatewaySession {
    // ---------------------------------------------------------------- room helpers

    /// Delivery of room pushes: to self — after the response (self.push), to others — via the hub.
    fn room_deliver(&self, out: room::Outbox) {
        let me = self.clean_open_id();
        for (open_id, msg) in out {
            if open_id == me {
                self.push(hub::DIRTY_METHOD, hub::DIRTY_TYPE, msg);
            } else if !hub::push_to(&open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg) {
                println!("[Room] {} offline, room push not delivered", open_id);
            }
        }
    }

    /// Result of a room operation: broadcast or (error) log + resync of the initiator.
    fn room_apply(&self, what: &str, user_id: u64, res: Result<room::Outbox, i32>) {
        match res {
            Ok(out) => self.room_deliver(out),
            Err(code) => {
                println!("[Room] {} from {}: error {}", what, user_id, code);
                self.set_rpc_error(code);
                self.room_deliver(room::my_snapshot(user_id, &self.clean_open_id()));
            }
        }
    }

    /// The party leader created or entered a room — online party members follow into free slots
    /// (before that the client asks for the "TeamMemberEnterRoom" confirmation). They do not need the room password.
    /// TODO(verify): whether the official server sends a separate team_member_notify_action.
    async fn room_bring_team(&self, captain: u64) {
        let Some(t) = team::team_of(captain) else { return };
        if t.captain != captain {
            return;
        }
        let Some(r) = room::room_of(captain) else { return };
        for m in t.members.iter().filter(|m| m.user_id != captain && hub::is_online(&m.open_id)) {
            let Some(p) = self.load_by_user_id(m.user_id).await else { continue };
            match room::enter(m.user_id, &m.open_id, room_deploy(&p), r.room_id, &r.keyword, false, None) {
                Ok(out) => self.room_deliver(out),
                Err(code) => println!("[Room] party member {} did not enter room {}: error {}", m.user_id, r.room_id, code),
            }
        }
    }

    // ---------------------------------------------------------------- party search helpers

    /// A match was made: the search participants get Matched{team_key} and a team_match_data reset, everyone — party snapshots.
    async fn team_deliver_match(&self, res: team::MatchResult) {
        let team_key = tgf::DTeamKey { team_id: res.team.team_id };
        println!("[TeamMatch] party {} assembled by search: {:?}", res.team.team_id, res.matched);
        for &uid in &res.matched {
            let Some(player) = self.load_by_user_id(uid).await else { continue };
            let still_searching = team::match_status(uid);
            let msg = tgf::ScPlayerDirtyChgSync {
                dirty_team_match_action: vec![tgf::DTeamMatchEventLog {
                    event: Some(tgf::d_team_match_event_log::Event::Matched(tgf::DTeamMatched {
                        team_key: Some(team_key.clone()),
                    })),
                }],
                dirty_team_match_data: Some(
                    still_searching
                        .map(|s| tgf::DTeamMatchData {
                            matching_id: s.matching_id,
                            team_matchsvr_id: 1,
                            start_matching_timepoint: s.start_ts,
                        })
                        .unwrap_or_default(),
                ),
                ..Default::default()
            };
            hub::push_to(&player.open_id, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
        }
        let mut ids = Self::team_user_ids(&res.team);
        for t in &res.old_teams {
            ids.extend(Self::team_user_ids(t));
        }
        self.push_team_snapshots(&ids).await;
    }

    // ---------------------------------------------------------------- team finder / party search

    pub(super) async fn team_apply_join_team_impl(&self, req: tgf::CsTeamApplyJoinTeamReq) -> Option<tgf::ScTeamApplyJoinTeamRsp> {
        let me = self.load_me().await;
        let team_id = req.team_key.map(|k| k.team_id).unwrap_or_default();
        match team::apply(me.user_id, &self.clean_open_id(), team_id, req.apply_type) {
            Ok(t) => {
                println!("[Team] {} applied to party {} (type={})", me.user_id, team_id, req.apply_type);
                // The party sees the application in pending_applies; the applicant's snapshot does not change
                self.push_team_snapshots(&Self::team_user_ids(&t)).await;
            }
            Err(e) => {
                println!("[Team] application of {} to party {} rejected: {}", me.user_id, team_id, e);
                self.set_rpc_error(team_err_code(e));
                self.push_team_snapshots(&[me.user_id]).await;
            }
        }
        Some(Default::default())
    }

    pub(super) async fn team_accept_apply_impl(&self, req: tgf::CsTeamAcceptApplyReq) -> Option<tgf::ScTeamAcceptApplyRsp> {
        let me = self.load_me().await;
        let applicant = req.user_key.map(|k| k.user_id).unwrap_or_default();
        let team_id = req.team_key.map(|k| k.team_id).unwrap_or_default();
        match team::accept_apply(me.user_id, applicant, team_id) {
            Ok((t, old)) => {
                println!("[Team] {} accepted the application of {} to party {}", me.user_id, applicant, t.team_id);
                let mut ids = Self::team_user_ids(&t);
                if let Some(old) = old {
                    ids.extend(Self::team_user_ids(&old));
                }
                self.push_team_snapshots(&ids).await;
            }
            Err(e) => {
                println!("[Team] {} did not accept the application of {}: {}", me.user_id, applicant, e);
                self.set_rpc_error(team_err_code(e));
                self.push_team_snapshots(&[me.user_id]).await;
            }
        }
        Some(Default::default())
    }

    pub(super) async fn team_reject_apply_impl(&self, req: tgf::CsTeamRejectApplyReq) -> Option<tgf::ScTeamRejectApplyRsp> {
        let me = self.load_me().await;
        let applicant = req.user_key.map(|k| k.user_id).unwrap_or_default();
        let team_id = req.team_key.map(|k| k.team_id).unwrap_or_default();
        match team::reject_apply(me.user_id, applicant, team_id) {
            Ok(t) => {
                let mut ids = Self::team_user_ids(&t);
                ids.push(applicant);
                self.push_team_snapshots(&ids).await;
            }
            Err(e) => {
                println!("[Team] {} did not reject the application of {}: {}", me.user_id, applicant, e);
                self.set_rpc_error(team_err_code(e));
            }
        }
        Some(Default::default())
    }

    pub(super) async fn team_change_team_finder_impl(&self, req: tgf::CsTeamChangeTeamFinderReq) -> Option<tgf::ScTeamChangeTeamFinderRsp> {
        let me = self.load_me().await;
        team::set_finder(me.user_id, req.finder_status.then(|| team_profile(&me)));
        println!("[Team] {} finder_status={}", me.user_id, req.finder_status);
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_team_finder_status: Some(tgf::DUserTeamFinderData { finder_status: req.finder_status }),
            ..Default::default()
        });
        Some(tgf::ScTeamChangeTeamFinderRsp { finder_status: req.finder_status })
    }

    pub(super) async fn team_match_search_impl(&self, req: tgf::CsTeamSearchReq) -> Option<tgf::ScTeamSearchRsp> {
        let me = self.load_me().await;
        let open_id = self.clean_open_id();
        team::touch_open_id(me.user_id, &open_id);
        let target = search_target(req.search_target);
        println!("[TeamMatch] {} is looking for a party: {:?}", me.user_id, target);
        let matching_id = team::match_search(me.user_id, &open_id, target, team_profile(&me));
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_team_match_data: Some(tgf::DTeamMatchData {
                matching_id: matching_id.clone(),
                team_matchsvr_id: 1,
                start_matching_timepoint: Self::now_ts(),
            }),
            ..Default::default()
        });
        // Try to match right away (then — on every team_match_check)
        if let Some(res) = team::match_check(me.user_id) {
            self.team_deliver_match(res).await;
        }
        Some(tgf::ScTeamSearchRsp { matching_id })
    }

    pub(super) async fn team_match_check_impl(&self, _req: tgf::CsTeamCheckReq) -> Option<tgf::ScTeamCheckRsp> {
        let me = self.load_me().await;
        if let Some(res) = team::match_check(me.user_id) {
            self.team_deliver_match(res).await;
        }
        Some(Default::default())
    }

    // ---------------------------------------------------------------- rooms: members

    pub(super) async fn room_create_impl(&self, req: tgf::CsRoomCreateReq) -> Option<tgf::ScRoomCreateRsp> {
        let me = self.load_me().await;
        let res = room::create(me.user_id, &self.clean_open_id(), room_deploy(&me), req.room_name.trim(), &req.room_keyword);
        let ok = res.is_ok();
        self.room_apply("room_create", me.user_id, res);
        if ok {
            self.room_bring_team(me.user_id).await;
        }
        Some(Default::default())
    }

    pub(super) async fn room_enter_impl(&self, req: tgf::CsRoomEnterReq) -> Option<tgf::ScRoomEnterRsp> {
        let me = self.load_me().await;
        let res = room::enter(
            me.user_id,
            &self.clean_open_id(),
            room_deploy(&me),
            room_id_of(&req.room_key),
            &req.room_keyword,
            req.is_invite,
            None,
        );
        let ok = res.is_ok();
        self.room_apply("room_enter", me.user_id, res);
        if ok {
            self.room_bring_team(me.user_id).await;
        }
        Some(tgf::ScRoomEnterRsp { is_invite: req.is_invite })
    }

    pub(super) async fn room_leave_impl(&self, _req: tgf::CsRoomLeaveReq) -> Option<tgf::ScRoomLeaveRsp> {
        let me = self.load_me().await;
        self.room_apply("room_leave", me.user_id, room::leave(me.user_id));
        Some(Default::default())
    }

    pub(super) async fn room_disband_impl(&self, _req: tgf::CsRoomDisbandReq) -> Option<tgf::ScRoomDisbandRsp> {
        let me = self.load_me().await;
        self.room_apply("room_disband", me.user_id, room::disband(me.user_id));
        Some(Default::default())
    }

    pub(super) async fn room_change_myself_pos_impl(&self, req: tgf::CsRoomChangeMySelfPosReq) -> Option<tgf::ScRoomChangeMySelfPosRsp> {
        let me = self.load_me().await;
        let res = match room::Pos::from_req(req.new_target_pos.as_ref(), req.target_pos) {
            Some(to) => room::move_member(me.user_id, me.user_id, to),
            None => Err(room::ERR_POS_PARAM_INVALID),
        };
        self.room_apply("room_change_myself_pos", me.user_id, res);
        Some(Default::default())
    }

    pub(super) async fn room_change_member_pos_impl(&self, req: tgf::CsRoomChangeMemberPosReq) -> Option<tgf::ScRoomChangeMemberPosRsp> {
        let me = self.load_me().await;
        let target = req.member.map(|k| k.user_id).unwrap_or_default();
        // 18.11: the room's kick button sends {WATCH, -1} for another member — kick out of the room (room::change_member_pos)
        let res = room::change_member_pos(me.user_id, target, req.new_target_pos.as_ref(), req.target);
        self.room_apply("room_change_member_pos", me.user_id, res);
        Some(Default::default())
    }

    pub(super) async fn room_transfer_owner_impl(&self, req: tgf::CsRoomTransferOwnerReq) -> Option<tgf::ScRoomTransferOwnerRsp> {
        let me = self.load_me().await;
        let target = req.new_owner.map(|k| k.user_id).unwrap_or_default();
        self.room_apply("room_transfer_owner", me.user_id, room::transfer_owner(me.user_id, target));
        Some(Default::default())
    }

    pub(super) async fn room_kickoff_member_impl(&self, req: tgf::CsRoomKickOffMemberReq) -> Option<tgf::ScRoomKickOffMemberRsp> {
        let me = self.load_me().await;
        let target = req.member.map(|k| k.user_id).unwrap_or_default();
        self.room_apply("room_kickoff_member", me.user_id, room::kick(me.user_id, target));
        Some(Default::default())
    }

    // ---------------------------------------------------------------- rooms: settings and readiness

    pub(super) async fn room_select_battle_map_impl(&self, req: tgf::CsRoomSelectBattleMapReq) -> Option<tgf::ScRoomSelectBattleMapRsp> {
        let me = self.load_me().await;
        self.room_apply("room_select_battle_map", me.user_id, room::select_map(me.user_id, req.map_id));
        Some(Default::default())
    }

    pub(super) async fn room_select_battle_type_impl(&self, req: tgf::CsRoomSelectBattleTypeReq) -> Option<tgf::ScRoomSelectBattleTypeRsp> {
        let me = self.load_me().await;
        self.room_apply("room_select_battle_type", me.user_id, room::select_battle_type(me.user_id, req.battle_type));
        Some(Default::default())
    }

    pub(super) async fn room_name_change_impl(&self, req: tgf::CsRoomNameChangeReq) -> Option<tgf::ScRoomNameChangeRsp> {
        let me = self.load_me().await;
        self.room_apply("room_name_change", me.user_id, room::rename(me.user_id, req.name.trim()));
        Some(Default::default())
    }

    pub(super) async fn room_keyword_change_impl(&self, req: tgf::CsRoomKeywordChangeReq) -> Option<tgf::ScRoomKeywordChangeRsp> {
        let me = self.load_me().await;
        // The proto field is called keyworld (a typo in the original)
        self.room_apply("room_keyword_change", me.user_id, room::change_keyword(me.user_id, &req.keyworld));
        Some(Default::default())
    }

    pub(super) async fn room_get_ready_impl(&self, _req: tgf::CsRoomGetReadyReq) -> Option<tgf::ScRoomGetReadyRsp> {
        let me = self.load_me().await;
        // Readiness fixes the current character (it could have changed after entering the room)
        let preview = self.build_battle_user_data(&me).deploy_preview;
        let res = room::set_ready(me.user_id, true, room_deploy(&me), preview);
        self.room_apply("room_get_ready", me.user_id, res);
        Some(Default::default())
    }

    pub(super) async fn room_cancel_ready_impl(&self, _req: tgf::CsRoomCancelReadyReq) -> Option<tgf::ScRoomCancelReadyRsp> {
        let me = self.load_me().await;
        self.room_apply("room_cancel_ready", me.user_id, room::set_ready(me.user_id, false, None, None));
        Some(Default::default())
    }

    pub(super) async fn room_start_battle_impl(&self, req: tgf::CsRoomStartBattleReq) -> Option<tgf::ScRoomStartBattleRsp> {
        let me = self.load_me().await;
        let res = match room::check_start(me.user_id) {
            Ok(r) => {
                // custom game: PVP arena by the room window's map/mode, teams — by seats (05.11)
                let arena = room::pvp_arena(r.battle_map, r.battle_type);
                let level_id = arena.map(|a| a.0).unwrap_or_else(|| room::level_for_map(r.battle_map));
                let fighters = room::fighter_teams(&r);
                for (u, t) in &fighters {
                    crate::lobby::team::set_room_battle_team(*u, *t);
                }
                let mut lvl = Self::get_saved_level_select(&me);
                lvl.level_id = level_id as i32;
                lvl.level_type = if level_id >= 50_000_000 { 2002 } else { 2001 };
                if !req.region.is_empty() {
                    lvl.region = req.region.clone();
                }
                let (_, map_id) = self.build_level_and_map_params(&lvl);
                let mut participators = Vec::new();
                let mut ids = Vec::new();
                for m in r.team_members() {
                    match self.load_by_user_id(m.user_id).await {
                        Some(p) => {
                            participators.push(self.build_battle_user_data(&p));
                            ids.push(p.open_id.clone());
                        }
                        None => println!("[Room] start {}: no player {} in DB", r.room_id, m.user_id),
                    }
                }
                if self.battle_access_denied(&ids).await {
                    for (u, _) in &fighters {
                        crate::lobby::team::clear_room_battle_team(*u);
                    }
                    self.room_deliver(room::my_snapshot(me.user_id, &self.clean_open_id()));
                    return Some(Default::default());
                }
                println!("[Room] start {}: arena {:?}, level {}, teams {:?}", r.room_id, arena, level_id, fighters);
                room::start_battle(me.user_id, room::StartInfo { level_id, map_id, participators, arena: arena.map(|a| a.1), fighters })
            }
            Err(e) => Err(e),
        };
        self.room_apply("room_start_battle", me.user_id, res);
        Some(Default::default())
    }

    // ---------------------------------------------------------------- room list

    pub(super) async fn room_suggest_impl(&self, req: tgf::CsRoomSuggestReq) -> Option<tgf::ScRoomSuggestRsp> {
        Some(tgf::ScRoomSuggestRsp { room_keys: room::suggest(req.index) })
    }

    pub(super) async fn room_batch_get_impl(&self, req: tgf::CsRoomBatchGetReq) -> Option<tgf::ScRoomBatchGetRsp> {
        let me = self.load_me().await;
        let rooms = room::batch_get(me.user_id, &req.room_key, &req.room_name, req.search);
        println!(
            "[Room] batch_get keys={} names={:?} search={} -> {}",
            req.room_key.len(),
            req.room_name,
            req.search,
            rooms.len()
        );
        Some(tgf::ScRoomBatchGetRsp { rooms })
    }

    // ---------------------------------------------------------------- invitations

    pub(super) async fn room_send_invite_impl(&self, req: tgf::CsRoomInviteSendReq) -> Option<tgf::ScRoomInviteSendRsp> {
        let me = self.load_me().await;
        let target = req.target_user.map(|k| k.user_id).unwrap_or_default();
        let res = match self.load_by_user_id(target).await {
            Some(t) => room::invite(me.user_id, target, &t.open_id),
            None => Err(room::ERR_MEMBER_NOT_ONLINE),
        };
        // The error goes to the initiator only as a log (no need to resend their room, but no harm either)
        self.room_apply("room_send_invite", me.user_id, res);
        Some(Default::default())
    }

    pub(super) async fn room_accept_invite_impl(&self, req: tgf::CsRoomInviteAcceptReq) -> Option<tgf::ScRoomInviteAcceptRsp> {
        let me = self.load_me().await;
        let sender = req.send_user.map(|k| k.user_id);
        let res = room::enter(
            me.user_id,
            &self.clean_open_id(),
            room_deploy(&me),
            room_id_of(&req.room_key),
            "",
            true,
            sender,
        );
        let ok = res.is_ok();
        self.room_apply("room_accept_invite", me.user_id, res);
        if ok {
            self.room_bring_team(me.user_id).await;
        }
        Some(Default::default())
    }

    pub(super) async fn room_reject_invite_impl(&self, req: tgf::CsRoomInviteRejectReq) -> Option<tgf::ScRoomInviteRejectRsp> {
        let me = self.load_me().await;
        let sender = req.send_user.map(|k| k.user_id).unwrap_or_default();
        let room_id = room_id_of(&req.room_key);
        // proto has no rejection notification for the sender — just remove the invitation
        if room::reject_invite(me.user_id, sender, room_id).is_none() {
            println!("[Room] {}: no invitation to room {} from {}", me.user_id, room_id, sender);
            self.set_rpc_error(room::ERR_INVITE_NOT_EXIST);
        }
        Some(Default::default())
    }

    pub(super) async fn room_ignore_room_all_invite_impl(&self, req: tgf::CsRoomIgnoreRoomAllInviteReq) -> Option<tgf::ScRoomIgnoreRoomAllInviteRsp> {
        let me = self.load_me().await;
        room::ignore_room(me.user_id, room_id_of(&req.room_key));
        Some(Default::default())
    }

    pub(super) async fn room_ignore_all_user_invite_impl(&self, _req: tgf::CsRoomIgnoreAllUserInviteReq) -> Option<tgf::ScRoomIgnoreAllUserInviteRsp> {
        let me = self.load_me().await;
        room::ignore_all(me.user_id);
        Some(Default::default())
    }
}


// ---- moved from handlers.rs ----

impl GatewaySession {
    /// 4. CANCEL PARTY SEARCH
    pub(super) async fn team_match_exit_impl(
        &self,
        _req: crate::tgf::CsTeamMatchExitReq,
    ) -> Option<crate::tgf::ScTeamMatchExitRsp> {
        println!("[Matchmaker] Player {} cancelled the party search", self.open_id);
        let me = self.load_me().await;
        if crate::lobby::team::match_cancel(me.user_id).is_some() {
            // search reset — an empty team_match_data (as after a match in room_handlers)
            self.push_dirty(tgf::ScPlayerDirtyChgSync {
                dirty_team_match_data: Some(tgf::DTeamMatchData::default()),
                ..Default::default()
            });
        }
        Some(crate::tgf::ScTeamMatchExitRsp {
            ..Default::default()
        })
    }
}
