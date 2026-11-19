//! Shared helpers of the domain handlers (*_handlers.rs): current session player, saving, pushes to self and others.
//! A push's type_url is the full protobuf message name (as in route_rpc: "tgf.SCPlayerDirtyChgSync"), not the Rust type name.

use super::GatewaySession;
use crate::lobby::hub;
use crate::structures::PlayerModel;
use crate::tgf;
use prost::Message;

#[allow(dead_code)]
impl GatewaySession {
    /// Current session player; if there is no record in the DB — a new one (as in lobby_handlers::current_player)
    pub(super) async fn load_me(&self) -> PlayerModel {
        let open_id = self.clean_open_id();
        self.db
            .get::<PlayerModel>("players", &open_id)
            .await
            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001))
    }

    /// Save the current session player. false — not saved: the action changes items of a character currently in a match
    /// (16.11, raid_lock.rs; the copy in the database is "before"). A caller that writes something else (auction book) must stop.
    pub(super) async fn save_me(&self, player: &PlayerModel) -> bool {
        let open_id = self.clean_open_id();
        if let Some(cur) = self.db.get::<PlayerModel>("players", &open_id).await {
            if let Some(role) = super::raid_lock::raid_items_changed(&cur, player) {
                self.raid_lock_refuse("save", role);
                return false;
            }
        }
        self.db.save("players", &open_id, player).await;
        true
    }

    /// Player by internal user_id (friends, guilds, rooms)
    pub(super) async fn load_by_user_id(&self, user_id: u64) -> Option<PlayerModel> {
        self.db.find_by_user_id::<PlayerModel>(user_id).await
    }

    /// Error code into the response header (RpcHead.error_code) — for RPCs without ret_code in the body.
    /// Codes — ExcelConfigjson/error_code.json. TODO(verify): how the client shows such an error.
    pub(super) fn set_rpc_error(&self, code: i32) {
        self.rpc_error.store(code, std::sync::atomic::Ordering::Relaxed);
    }

    /// M5: match access by account roles (`[auth] battle_roles`, `auth::accounts::may_battle`) for all participants
    /// (party, room team). Rejected — true: code into the response header and a window with the rejection text to the initiator.
    /// TODO(verify): which text the client itself shows for -1028 next to the window.
    pub(super) async fn battle_access_denied(&self, open_ids: &[String]) -> bool {
        if !crate::lobby::match_checks::server().battle_access {
            return false;
        }
        let me = self.clean_open_id();
        let mut denied = Vec::new();
        for id in open_ids {
            if !crate::auth::accounts::may_battle(&self.db, id).await {
                denied.push(id.as_str());
            }
        }
        if denied.is_empty() {
            return false;
        }
        let (_, msg) = crate::auth::battle_policy();
        let content = if denied.contains(&me.as_str()) { msg.to_string() } else { format!("У напарника нет доступа к бою. {}", msg) };
        println!("[Auth] match: rejected {} (no role from battle_roles: {:?})", me, denied);
        self.set_rpc_error(tgf::EnErrorCode::EnErrMatchingThisActiveLevelNotOpen as i32);
        self.push(
            "tgf.GameClientService.modal_message",
            "tgf.SCModalMessageSync",
            tgf::ScModalMessageSync { title: "Доступ к бою".into(), content },
        );
        true
    }

    /// Reset of the client's role and player cache — as PVE SoloLobby after role_create / hall_set_role (rpc_dump/PVE.jsonl
    /// 02.11): two SCCacheExpiredSync pushes, first {EN_ROT_ROLE_BASIC, role}, then {EN_ROT_PLAYER, player}.
    /// Without them the client does not update the current role: the character list window does not close, the hall stays empty.
    pub(super) fn push_role_cache_expired(&self, user_id: u64, role_guid: u64) {
        for (type_id, inst_id) in [(CACHE_ROLE_BASIC, role_guid), (CACHE_PLAYER, user_id)] {
            self.push(
                CACHE_EXPIRED_METHOD,
                CACHE_EXPIRED_TYPE,
                tgf::ScCacheExpiredSync { keys: vec![tgf::DCacheKey { type_id, zone_id: CACHE_ZONE, inst_id }] },
            );
        }
    }

    /// SCPlayerDirtyChgSync push to the current player after the response
    pub(super) fn push_dirty(&self, mut dirty: tgf::ScPlayerDirtyChgSync) {
        split_virtual_coins(&mut dirty);
        self.push(hub::DIRTY_METHOD, hub::DIRTY_TYPE, dirty);
    }
}

/// Coin for the client: it reads only these ids from coins_data (FUN_142682480 in the exe). The other currencies of the table
/// (materials 1037, 1038, 1039, 1052, 1053, trial token 1500001) are its "virtual items". In state["coins"] they
/// are stored as coins, only the transfer differs.
pub(super) fn is_client_coin(coin_type_id: i32) -> bool {
    (1000..1031).contains(&coin_type_id) || (2000..30000).contains(&coin_type_id)
}

/// Client virtual item: a regular DItem with position.x < 0, the client puts it into a map by type_id (FUN_1459aee2b)
/// and replaces the count. So a zero balance is sent the same way.
pub(super) fn virtual_item(type_id: i32, count: i64) -> tgf::DItem {
    tgf::DItem {
        basic: Some(tgf::DItemBasic {
            type_id,
            count: count.clamp(0, i32::MAX as i64) as i32,
            position: Some(tgf::DItemPosition { r#type: tgf::EnItemStorageType::Inventory as i32, guid: 0, x: -1, y: -1, limit_option: None }),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Virtual currencies from dirty_coin_data — into dirty_postion_action
pub(super) fn split_virtual_coins(dirty: &mut tgf::ScPlayerDirtyChgSync) {
    let coins = std::mem::take(&mut dirty.dirty_coin_data);
    for c in coins {
        if is_client_coin(c.coin_type_id) {
            dirty.dirty_coin_data.push(c);
        } else {
            dirty.dirty_postion_action.push(virtual_item(c.coin_type_id, c.count));
        }
    }
}

#[cfg(test)]
mod virtual_coin_tests {
    use super::*;

    #[test]
    fn materials_go_as_virtual_items() {
        assert!(is_client_coin(GOLD) && is_client_coin(1012) && is_client_coin(1030) && is_client_coin(2000));
        for id in [1031, 1037, 1038, 1039, 1052, 1053, 1500001] {
            assert!(!is_client_coin(id), "{}", id);
        }
        let mut dirty = tgf::ScPlayerDirtyChgSync {
            dirty_coin_data: vec![
                tgf::DCoin { coin_type_id: GOLD, count: 5, limit_option: None },
                tgf::DCoin { coin_type_id: 1052, count: 0, limit_option: None },
            ],
            ..Default::default()
        };
        split_virtual_coins(&mut dirty);
        assert_eq!(dirty.dirty_coin_data.len(), 1);
        let basic = dirty.dirty_postion_action[0].basic.as_ref().unwrap();
        assert_eq!((basic.type_id, basic.count), (1052, 0));
        assert!(basic.position.as_ref().unwrap().x < 0);
    }
}

/// Client cache types (DCacheKey.type_id = tgf::EnRouterObjectType)
pub const CACHE_PLAYER: u32 = tgf::EnRouterObjectType::EnRotPlayer as u32; // 1
pub const CACHE_ROLE_BASIC: u32 = tgf::EnRouterObjectType::EnRotRoleBasic as u32; // 4
pub const CACHE_ROLE_DETAIL: u32 = tgf::EnRouterObjectType::EnRotRoleDetail as u32; // 5
/// zone_id in cache keys — as the player's user_key (account_handlers: zone 1)
pub const CACHE_ZONE: u32 = 1;
pub const CACHE_EXPIRED_METHOD: &str = "tgf.GameClientService.cache_expired";
pub const CACHE_EXPIRED_TYPE: &str = "tgf.SCCacheExpiredSync";

/// Gold (DCoin 1001) — the same value player_get_info sends in coins_data
pub const GOLD: i32 = 1001;

/// Environment flag: `1` on, `0` off, missing or another value — `default`
pub fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name).as_deref() {
        Ok("1") => true,
        Ok("0") => false,
        _ => default,
    }
}

pub use crate::structures::START_GOLD;

/// Coin balance: gold in state["gold"] (default START_GOLD, as in player_get_info), the rest — state["coins"][id]
#[allow(dead_code)]
pub(super) fn coin(p: &PlayerModel, coin_type_id: i32) -> i64 {
    if coin_type_id == GOLD {
        p.state["gold"].as_i64().unwrap_or_else(crate::structures::start_gold)
    } else {
        p.state["coins"][coin_type_id.to_string()].as_i64().unwrap_or(0)
    }
}

/// Change the balance by delta; None (no change) if it goes negative. Returns the new balance.
#[allow(dead_code)]
pub(super) fn add_coin(p: &mut PlayerModel, coin_type_id: i32, delta: i64) -> Option<i64> {
    let now = coin(p, coin_type_id).checked_add(delta).filter(|v| *v >= 0)?;
    if coin_type_id == GOLD {
        p.state["gold"] = serde_json::json!(now);
    } else {
        if !p.state["coins"].is_object() {
            p.state["coins"] = serde_json::json!({});
        }
        p.state["coins"][coin_type_id.to_string()] = serde_json::json!(now);
    }
    Some(now)
}

/// Record for ScPlayerDirtyChgSync.dirty_coin_data with the current balance
#[allow(dead_code)]
pub(super) fn coin_dirty(p: &PlayerModel, coin_type_id: i32) -> tgf::DCoin {
    tgf::DCoin { coin_type_id, count: coin(p, coin_type_id), limit_option: None }
}

/// Push to another player (by open_id) if online. false — offline.
#[allow(dead_code)]
pub(super) fn push_to<T: Message>(open_id: &str, method: &str, type_url: &str, msg: &T) -> bool {
    hub::push_to(open_id, method, type_url, msg)
}
