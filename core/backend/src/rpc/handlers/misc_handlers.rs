//! Service RPCs: pings, telemetry, anti-cheat (ACE/hope), logs, payments (pay_*), DLC.
//! The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Format facts:
//! - ping/business_ping: SCPongRsp and SCBusinessPongRsp are empty messages (no time fields); the official server
//!   answers a ping with an empty body (sniffer, rpc_dump PVE_battle_0720).
//! - client_action_log / network_latency_report / cache_unwatch: the responses in rpc_dump PVE are empty — we only acknowledge.
//! - pay_*: there are no real payments. pay_get_all (below) returns pay_data = None, i.e. a diamond balance of 0 —
//!   all pay_* are consistent with this: 0 diamonds, bills not applied, real-money purchases are not created.
//!   pay_* responses have no ret_code, so a rejection = an empty response.
//! - ACE (player_ace_packet) and hope_report_execute — anti-cheat: acknowledgement only, packets are not parsed.
//! - DLC: Steam DLC (app_id) from dlc_content; the package contents are dlc_packet rows (several per app_id, so
//!   we read .bytes as a list: `game_data.dlc_packet` is a HashMap by app_id and keeps only the last row).
//!   Flow: refresh_dlc_info (the client reports the ids of its DLCs) → push SCNewDLCPacketSync with the reward → the client
//!   sends dlc_receive_packet (DItemBasic items, possibly with stash positions) → granted once.

use super::{inventory_handlers, GatewaySession};
use crate::structures::PlayerModel;
use crate::tgf;
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

/// The "account stash" place in DItemPosition.type (as in inventory_handlers)
const STASH: i32 = 1;

/// All dlc_packet.bytes rows (not a HashMap — one app_id has many rows)
fn dlc_packet_rows() -> &'static Vec<tgf::config::ExcelDlcPacket> {
    static ROWS: OnceLock<Vec<tgf::config::ExcelDlcPacket>> = OnceLock::new();
    ROWS.get_or_init(|| crate::game_data::load_bytes_vec("ExcelConfig/dlc_packet.bytes"))
}

/// DLC package items from the table
fn dlc_packet_items(app_id: u32) -> Vec<&'static tgf::config::ExcelDlcPacket> {
    dlc_packet_rows().iter().filter(|r| r.app_id as u32 == app_id).collect()
}

/// (open_id, app_id) already shown SCNewDLCPacketSync in this process — so that the timed refresh does not spam
fn dlc_offered() -> &'static Mutex<HashSet<(String, u32)>> {
    static OFFERED: OnceLock<Mutex<HashSet<(String, u32)>>> = OnceLock::new();
    OFFERED.get_or_init(|| Mutex::new(HashSet::new()))
}

fn dlc_already_received(p: &PlayerModel, app_id: u32) -> bool {
    !p.state["dlc_received"][app_id.to_string()].is_null()
}

impl GatewaySession {
    // ---------------------------------------------------------------- pings, logs, telemetry

    pub(super) async fn client_action_log_impl(&self, _req: tgf::CsCommonLoggingReq) -> Option<tgf::ScCommonLoggingRsp> {
        Some(tgf::ScCommonLoggingRsp {})
    }

    pub(super) async fn ping_impl(&self, _req: tgf::CsPingReq) -> Option<tgf::ScPongRsp> {
        Some(tgf::ScPongRsp {})
    }

    pub(super) async fn business_ping_impl(&self, _req: tgf::CsBusinessPingReq) -> Option<tgf::ScBusinessPongRsp> {
        Some(tgf::ScBusinessPongRsp {})
    }

    pub(super) async fn network_latency_report_impl(&self, _req: tgf::CsNetworkLatencyReportReq) -> Option<tgf::ScNetworkLatencyReportRsp> {
        Some(tgf::ScNetworkLatencyReportRsp {})
    }

    /// The client takes long to load on the DS — useful to see in the log when debugging dedicated servers
    pub(super) async fn network_block_loading_impl(&self, req: tgf::CsNetworkBlockLoadingReq) -> Option<tgf::ScNetworkBlockLoadingRsp> {
        println!(
            "[Net] {} block_loading: battle={} ds={} region={} cnt={}",
            self.clean_open_id(),
            req.battle_id,
            req.ds_address,
            req.ds_region,
            req.loading_cnt
        );
        // the server is alive, but the client did not load: direct UDP seems to be cut on the way. Release it from the room: the match
        // has not started for it, items are untouched. 09.11 (user's decision): the server no longer enables the UDP tunnel itself —
        // only the player in the launcher (settings or a launcher hint) and GM `udp on|off` (auth::udp_tunnel)
        let open_id = self.clean_open_id();
        let in_room = crate::lobby::matchmaking::room_of_player(&open_id).as_deref() == Some(req.battle_id.as_str());
        let alive = crate::lobby::allocator::has_heartbeat(&req.ds_address);
        if crate::auth::udp_tunnel::direct_load_failed(&open_id, &req.ds_address, alive, in_room) {
            let (marked, _) = crate::lobby::matchmaking::members_absent(&req.battle_id, std::slice::from_ref(&open_id), false);
            for o in &marked {
                crate::rpc::handlers::room_finish_push(o, &req.battle_id, None, None);
            }
            println!("[Net] {} cannot connect to {}: released from room {} (not enabling the UDP tunnel — the player's choice)", open_id, req.ds_address, req.battle_id);
        }
        Some(tgf::ScNetworkBlockLoadingRsp {})
    }

    /// Platform account data (incl. the access token) — not stored, only acknowledged
    pub(super) async fn update_account_data_impl(&self, _req: tgf::CsUpdateAccountDataReq) -> Option<tgf::ScUpdateAccountDataRsp> {
        Some(tgf::ScUpdateAccountDataRsp {})
    }

    pub(super) async fn telemetry_report_impl(&self, _req: tgf::CsTelemetryReportReq) -> Option<tgf::ScTelemetryReportRsp> {
        Some(tgf::ScTelemetryReportRsp {})
    }

    // ---------------------------------------------------------------- anti-cheat (acknowledgement only)

    pub(super) async fn player_ace_packet_impl(&self, _req: tgf::CsPlayerAcePacketReq) -> Option<tgf::ScPlayerAcePacketRsp> {
        Some(tgf::ScPlayerAcePacketRsp {})
    }

    pub(super) async fn hope_report_execute_impl(&self, _req: tgf::CsHopeReportExecuteReq) -> Option<tgf::ScHopeReportExecuteRsp> {
        Some(tgf::ScHopeReportExecuteRsp {})
    }

    // ---------------------------------------------------------------- other

    /// Redirect to Apollo/iTop (platform friend list, ACVM report) — there is no such service, return an empty list
    pub(super) async fn player_uri_redirect_impl(&self, req: tgf::CsPlayerRedirectReq) -> Option<tgf::ScPlayerRedirectRsp> {
        let data = match req.r#type {
            // EN_REDIRECT_APOLLO_ITOP_FRIEND_LIST
            1 => Some(tgf::RedirectResData {
                ret: 0,
                msg: String::new(),
                data: Some(tgf::redirect_res_data::Data::FriendList(tgf::ITopFriendFriendList::default())),
            }),
            _ => Some(tgf::RedirectResData::default()),
        };
        Some(tgf::ScPlayerRedirectRsp { ret: 0, msg: String::new(), data })
    }

    pub(super) async fn cache_unwatch_impl(&self, _req: tgf::CsCacheUnwatchReq) -> Option<tgf::ScCacheUnwatchRsp> {
        Some(tgf::ScCacheUnwatchRsp {})
    }

    // ---------------------------------------------------------------- payments (none: balance 0, as in pay_get_all)

    pub(super) async fn pay_update_diamond_impl(&self, req: tgf::CsPayUpdateDiamondReq) -> Option<tgf::ScPayUpdateDiamondRsp> {
        println!("[Pay] {} update_diamond bill={:?}: no payments, balance 0", self.clean_open_id(), req.bill_no);
        Some(tgf::ScPayUpdateDiamondRsp { diamond_cnt: 0, free_diamond_cnt: 0, is_bill_applied: false })
    }

    /// Real-money purchase: no order (Midas token/URL) is created — empty payment fields
    // TODO(verify): how the client reacts to an empty midas_token (error/nothing) — real-money purchase buttons
    pub(super) async fn pay_direct_buy_impl(&self, req: tgf::CsPayDirectBuyProductReq) -> Option<tgf::ScPayDirectBuyProductRsp> {
        println!("[Pay] {} direct_buy product={} ({}): rejected — no payments", self.clean_open_id(), req.product_id, req.product_ident);
        Some(tgf::ScPayDirectBuyProductRsp { product_id: req.product_id, product_ident: req.product_ident, ..Default::default() })
    }

    pub(super) async fn pay_check_bill_applied_impl(&self, _req: tgf::CsPayCheckBillAppliedReq) -> Option<tgf::ScPayCheckBillAppliedRsp> {
        Some(tgf::ScPayCheckBillAppliedRsp { is_bill_applied: false })
    }

    /// Payment channel authorization — not stored
    pub(super) async fn pay_update_authorization_data_impl(&self, _req: tgf::CsPayUpdateAuthorizationDataReq) -> Option<tgf::ScPayUpdateAuthorizationDataRsp> {
        Some(tgf::ScPayUpdateAuthorizationDataRsp {})
    }

    pub(super) async fn pay_cancel_impl(&self, req: tgf::CsPayCancelReq) -> Option<tgf::ScPayCancelRsp> {
        println!("[Pay] {} cancel bill={:?} amt={}", self.clean_open_id(), req.bill_no, req.amt);
        Some(tgf::ScPayCancelRsp { raw_json: "{}".to_string() })
    }

    /// Exchange of paid diamonds for free ones: the diamond balance is always 0 (pay_data = None), there is no rate table — nothing
    pub(super) async fn pay_exchange_free_diamond_impl(&self, req: tgf::CsPayExchangeFreeDiamondReq) -> Option<tgf::ScPayExchangeFreeDiamondRsp> {
        println!("[Pay] {} exchange_free_diamond x{}: rejected — no diamonds", self.clean_open_id(), req.exchange_count);
        Some(tgf::ScPayExchangeFreeDiamondRsp {})
    }

    pub(super) async fn pay_submit_pending_flow_impl(&self, _req: tgf::CsPaySubmitPendingFlowReq) -> Option<tgf::ScPaySubmitPendingFlowRsp> {
        Some(tgf::ScPaySubmitPendingFlowRsp { has_pending: false })
    }

    // ---------------------------------------------------------------- DLC

    /// The client reports the ids of its Steam DLCs. There is no way to check ownership via the Steam WebAPI — we trust the client for
    /// DLCs from dlc_content. For packages not yet received we push SCNewDLCPacketSync after the response (once per process).
    pub(super) async fn refresh_dlc_info_impl(&self, req: tgf::CsRefreshDlcInfoReq) -> Option<tgf::ScRefreshDlcInfoRsp> {
        let open_id = self.clean_open_id();
        let player = self.db.get::<PlayerModel>("players", &open_id).await;
        let mut list = Vec::new();
        for &app_id in &req.dlc_app_id_list {
            let effective = self.game_data.dlc_content.contains_key(&(app_id as i32));
            list.push(tgf::sc_refresh_dlc_info_rsp::OneDlcInfo { dlc_app_id: app_id, effective });
            let Some(p) = player.as_ref() else { continue };
            if !effective || dlc_already_received(p, app_id) {
                continue;
            }
            if !dlc_offered().lock().unwrap().insert((open_id.clone(), app_id)) {
                continue;
            }
            let items: Vec<tgf::DItemOffset> = dlc_packet_items(app_id).iter().filter_map(|r| r.item.clone()).collect();
            println!("[DLC] {}: offered package {} ({} items)", open_id, app_id, items.len());
            self.push("tgf.GameClientService.dlc_packet_sync", "tgf.SCNewDLCPacketSync", tgf::ScNewDlcPacketSync { dlc_app_id: app_id, items });
        }
        Some(tgf::ScRefreshDlcInfoRsp { dlc_info_list: list })
    }

    /// Receiving a DLC package: once (state["dlc_received"][app_id] = ts), only items from dlc_packet,
    /// count not above the table's. item_type items go to the stash (position from the request, otherwise a free cell
    /// of page 7). Rows without item_type (appearances 13xxxxx, is_recycle) belong to the appearance domain and are not granted here.
    pub(super) async fn dlc_receive_packet_impl(&self, req: tgf::CsReceiveDlcPacketReq) -> Option<tgf::ScReceiveDlcPacketRsp> {
        let app_id = req.dlc_app_id;
        let empty = tgf::ScReceiveDlcPacketRsp { dlc_app_id: app_id, items: vec![] };
        let open_id = self.clean_open_id();
        let rows = dlc_packet_items(app_id);
        if rows.is_empty() || !self.game_data.dlc_content.contains_key(&(app_id as i32)) {
            println!("[DLC] {}: unknown package {} — rejected", open_id, app_id);
            return Some(empty);
        }
        let mut player = self.db.get::<PlayerModel>("players", &open_id).await?;
        if dlc_already_received(&player, app_id) {
            println!("[DLC] {}: package {} already received (EN_ERR_DLC_PACKET_RECEIVED)", open_id, app_id);
            return Some(empty);
        }

        let now = Self::now_ts();
        let mut ch = inventory_handlers::ItemChanges::default();
        let (mut granted, mut skipped) = (Vec::new(), Vec::new());
        for row in rows {
            let Some(off) = row.item.clone() else { continue };
            if !self.game_data.item_type.contains_key(&off.type_id) {
                skipped.push(off.type_id);
                continue;
            }
            // the position the client chose (if it sent one and it is the stash), otherwise the first free stash cell
            let target = req
                .items
                .iter()
                .find(|b| b.type_id == off.type_id)
                .and_then(|b| b.position.clone())
                .filter(|pos| pos.r#type == STASH)
                .unwrap_or(tgf::DItemPosition { r#type: STASH, x: -1, y: -1, ..Default::default() });
            let mut item = self.inv_new_item(&mut player, off.type_id, off.count.max(1));
            if let Some(b) = item.basic.as_mut() {
                b.expire_timepoint = if off.expire_offset > 0 { now + off.expire_offset } else { 0 };
            }
            if let Err(e) = self.inv_place_item(&mut player, item, &target, &mut ch) {
                println!("[DLC] {}: {}", open_id, e);
                skipped.push(off.type_id);
                continue;
            }
            granted.push(off);
        }
        let dirty = ch.dirty;
        if !player.state["dlc_received"].is_object() {
            player.state["dlc_received"] = serde_json::json!({});
        }
        player.state["dlc_received"][app_id.to_string()] = serde_json::json!(now);
        self.db.save("players", &open_id, &player).await;
        println!(
            "[DLC] {}: package {} received, to the stash {} items{}",
            open_id,
            app_id,
            granted.len(),
            if skipped.is_empty() { String::new() } else { format!("; not granted (appearances/no room): {:?}", skipped) }
        );
        // TODO(verify): appearances from the package (1322001, 1300002, 1321000) — grant via the appearance domain once it exists
        if !dirty.is_empty() {
            self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_postion_action: dirty, ..Default::default() });
        }
        Some(tgf::ScReceiveDlcPacketRsp { dlc_app_id: app_id, items: granted })
    }
}


// ---- moved from handlers.rs ----

impl GatewaySession {
    /// 1. Balance request (return nothing, as in the old code)
    pub(super) async fn pay_get_all_impl(
        &self,
        _req: crate::tgf::CsPayGetAllReq,
    ) -> Option<crate::tgf::ScPayGetAllRsp> {
        Some(crate::tgf::ScPayGetAllRsp {
            pay_data: None, // Strictly None!
        })
    }
}
