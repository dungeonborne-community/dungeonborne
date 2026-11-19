//! Player profile and characters: rename/icon/options/birthday, player search (batch_get_*), GM commands,
//! role deletion and transfer, face reshaping, appearance (facade_*).
//! The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Format facts (30.10): there are no dumps/sniffer captures for these RPCs — the format comes from proto (`schema_dump.proto`) and the tables.
//! - Official player_get_info (sniffer): profile = {open_id, user_id, nick_name}, icons 0;
//!   facade_data = {role_wear_info: [{role_id}]} per role, the rest empty.
//! - const.json: user_nickname_length_limit = 12 (ASCII = 1, others = 1.5), user_nickname_preserve_prefix "游客:",
//!   role_lazy_delete_duration = 24h, facade_template_list_size = 20, role_count_limit = 10,
//!   role_reshape_face_pinching_cost_item = "1040,1,0" (not charged — see TODO).
//! - facade.json: id 1400xxx avatars (EN_FACADE_TYPE_AVATAR = 1), 1410xxx frames (2), 1420xxx titles (3),
//!   13xxxxx skins (4, ue_skin_ids usually == [id]; helmet sets 1320xxx have 3 UE skins each).
//! - Stash: x — column (0..10), y — row (0..17), guid — page (inventory_handlers::stash_pages).
//!
//! These RPC responses have no ret_code: on rejection `self.set_rpc_error(EN_ERR_*)` (RpcHead.error_code), a response
//! "without changes" and the reason in the log.
//!
//! Client Lua (tools/research/player/lua_orig, Inventory/WBP_Inventory_SkinPanel, BP_LobbyInventoryMenu,
//! WBP_Store_DetailPanel, UserRole/UserInfoCtl): the client draws role skins from FacadeData.RoleWearInfo
//! (RoleId, WearInfo[].ItemId, HiddenHelmet) — facade ids, it translates them to ue_skin_ids itself; templates — GroupSkins
//! (ItemIds, HiddenHelmet), MaxTemplateSize/Facade_Template_Max_Tips — the client checks the limit too;
//! UserInfoCtl reads NickName and AvatarBoxTypeId (does not read CustomNickName). Role deletion — in C++ (no Lua).
//!
//! Storage (player state):
//! - `player_profile` {nick_name, rename_times, avatar, avatar_box, title, avatar_times, avatar_box_times,
//!   title_times, birthday} — read via `player_profile()` / `player_birthday()`;
//! - `player_private_options` — DPlayerPrivateOptions.client_options bytes (`player_private_options()`);
//! - `player_facade_templates` — DUserFacadeData bytes (only group_skins = skin templates);
//! - `player_facade_owned` — [id] of bought/granted facades beyond the ones unlocked by default (for the shop);
//! - `player_hall_switch_type` — the last hall_switch (1 adventurers' hall, 2 mithril force).
//! Role skins — in the role itself: deploy_role.dress_up.dress_facade_info (UserRoleSkinWearInfo), get_info returns it as well.

use super::common::{self, GOLD};
use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use prost::Message;
use serde_json::json;

const ZONE_ID: u32 = 1;
/// Default icons — as currently in player_get_info/cache_batch_get
const DEFAULT_AVATAR: i32 = 309010001;
const DEFAULT_AVATAR_BOX: i32 = 309020001;
const DEFAULT_BIRTHDAY: &str = "1995-01-01";
/// Unlock all facades with on_sales = true (an emulator without an appearance shop). Off since 03.11: skins — bought for moon
/// stones in the native shop (Sh4, was K11 in docs/WORKAROUNDS.md); only the starting DEFAULT_FACADES are unlocked for everyone.
const UNLOCK_ALL_ON_SALE_FACADES: bool = false;
/// Starting appearances (not sold and not rewards): mithril lantern, flask, mithril cloak, avatars and titles
/// without a localized name. TODO(verify): which appearances a new player had in the original
const DEFAULT_FACADES: [i32; 13] = [
    1300002, 1301001, 1322001, 1400001, 1400002, 1400003, 1400004, 1400005, 1420001, 1420002, 1420003, 1420004, 1420005,
];
const FACADE_AVATAR: i32 = 1;
const FACADE_AVATAR_BORDER: i32 = 2;
const FACADE_TITLE: i32 = 3;
const FACADE_SKIN: i32 = 4;
/// EN_FACADE_SUIT_TYPE_TEMPLATE
const SUIT_TYPE_TEMPLATE: i32 = 2;
/// Version of the facade_get_on_sale_facade list (the table is static)
const ON_SALE_VERSION: i64 = 1;
/// Client cache types — tgf::EnRouterObjectType (it was 2/3 for roles — wrong; RoleBasic = 4, RoleDetail = 5)
const CACHE_USER: u32 = common::CACHE_PLAYER;
const CACHE_ROLE_BASIC: u32 = common::CACHE_ROLE_BASIC;
const CACHE_ROLE_DETAIL: u32 = common::CACHE_ROLE_DETAIL;

// Error codes (schema_dump.proto EnErrorCode)
const ERR_ROLE_NOT_FOUND: i32 = -701;
const ERR_ROLE_DRESS_UP_INVALID: i32 = -745;
const ERR_NICKNAME_LENGTH: i32 = -509;
const ERR_NICKNAME_PREFIX: i32 = -510;
const ERR_NAME_TAKEN: i32 = -534;
const ERR_INVALID_ICON: i32 = -511;
const ERR_BIRTHDAY_FORMAT: i32 = -545;
const ERR_GM_CMD_INVALID: i32 = -501;
const ERR_GM_CMD_PARAM_INVALID: i32 = -503;
const ERR_ITEM_NOT_IN_TABLE: i32 = -605;
const ERR_ITEM_NO_SPACE: i32 = -695;
const ERR_FACADE_NOT_SUPPORT: i32 = -4801;
const ERR_FACADE_NOT_OWNED: i32 = -4802;
const ERR_FACADE_TEMPLATE_NOT_FOUND: i32 = -4810;
const ERR_FACADE_TEMPLATE_QTY: i32 = -4814;
const ERR_FACADE_TEMPLATE_NAME: i32 = -4815;

// ---------- profile ----------

fn prof_str(p: &PlayerModel, key: &str) -> Option<String> {
    p.state["player_profile"][key].as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn prof_i32(p: &PlayerModel, key: &str) -> Option<i32> {
    p.state["player_profile"][key].as_i64().map(|v| v as i32)
}

fn prof_set(p: &mut PlayerModel, key: &str, v: serde_json::Value) {
    if !p.state["player_profile"].is_object() {
        p.state["player_profile"] = json!({});
    }
    p.state["player_profile"][key] = v;
}

/// The player's nickname = the name of the current hall role, as in PVE SoloLobby (rpc_dump/PVE.jsonl 02.11: nick_name changes together with
/// user_hall_setting.role_guid; without roles — "Adventurer"). The client checks the nickname against the selected role: with a constant nickname
/// switching roles from the list worked only to a role with the same name. After that — the own nickname (player_rename), the first role.
pub(super) fn player_nick(p: &PlayerModel) -> String {
    let named = |n: &String| !n.is_empty();
    p.roles
        .iter()
        .find(|r| r.role_guid == p.hall_role_guid)
        .map(|r| r.role_name.clone())
        .filter(named)
        .or_else(|| prof_str(p, "nick_name"))
        .or_else(|| p.roles.iter().min_by_key(|r| r.ord).map(|r| r.role_name.clone()).filter(named))
        .unwrap_or_else(|| "Adventurer".to_string())
}

/// DPlayerProfile from state["player_profile"] — for player_get_info, cache_batch_get (UserCache), friends/guilds
pub(super) fn player_profile(p: &PlayerModel) -> tgf::DPlayerProfile {
    let custom = prof_str(p, "nick_name").unwrap_or_default();
    tgf::DPlayerProfile {
        open_id: p.open_id.clone(),
        user_id: p.user_id,
        nick_name: player_nick(p),
        custom_nick_name: custom,
        sex: 1,
        avatar_icon_type_id: prof_i32(p, "avatar").unwrap_or(DEFAULT_AVATAR),
        avatar_box_icon_type_id: prof_i32(p, "avatar_box").unwrap_or(DEFAULT_AVATAR_BOX),
        title_type_id: prof_i32(p, "title").unwrap_or(0),
        stat_custom_nick_name_times: prof_i32(p, "rename_times").unwrap_or(0),
        stat_avatar_icon_type_id_times: prof_i32(p, "avatar_times").unwrap_or(0),
        stat_avatar_box_icon_type_id_times: prof_i32(p, "avatar_box_times").unwrap_or(0),
        stat_title_type_id_times: prof_i32(p, "title_times").unwrap_or(0),
        ..Default::default()
    }
}

/// Birthday for DPlayerInfo.birthday
pub(super) fn player_birthday(p: &PlayerModel) -> String {
    prof_str(p, "birthday").unwrap_or_else(|| DEFAULT_BIRTHDAY.to_string())
}

/// DPlayerInfo.private_options (player_update_options)
#[allow(dead_code)]
pub(super) fn player_private_options(p: &PlayerModel) -> Option<tgf::DPlayerPrivateOptions> {
    let bytes = serde_json::from_value::<Vec<u8>>(p.state.get("player_private_options")?.clone()).ok()?;
    Some(tgf::DPlayerPrivateOptions { client_options: bytes })
}

/// Nickname length per the const.json rules: ASCII = 1, other characters = 1.5
fn nick_weight(name: &str) -> f64 {
    name.chars().map(|c| if c.is_ascii() { 1.0 } else { 1.5 }).sum()
}

/// Delete roles with an expired lazy_delete_timepoint; if the hall character is deleted — switch to a remaining one.
/// true — something was deleted (needs saving). Call in player_get_info before building the response.
#[allow(dead_code)]
pub(super) fn player_purge_lazy_deleted_roles(p: &mut PlayerModel, now: i64) -> bool {
    let before = p.roles.len();
    p.roles.retain(|r| !(r.data.lazy_delete_timepoint > 0 && r.data.lazy_delete_timepoint <= now));
    if p.roles.len() == before {
        return false;
    }
    fix_hall_role(p);
    true
}

/// hall_role_guid points to a nonexistent role → the first remaining one (or 0)
fn fix_hall_role(p: &mut PlayerModel) -> bool {
    if p.hall_role_guid != 0 && !p.roles.iter().any(|r| r.role_guid == p.hall_role_guid) {
        p.hall_role_guid = p.roles.iter().min_by_key(|r| r.ord).map(|r| r.role_guid).unwrap_or(0);
        return true;
    }
    false
}

/// A free 3x5 block in the stash (pages from the end, so as not to interfere with the player's layout). The server does not know
/// item sizes — the block is free if no item starts in the vicinity of the block.
fn stash_free_block(p: &PlayerModel) -> Option<tgf::DItemPosition> {
    for page in (0..super::inventory_handlers::stash_page_count(p)).rev() { // ext: stash_pages
        for y in [0, 5, 10] {
            for x in [0, 3, 6] {
                let busy = p.stash.element.iter().any(|it| {
                    let pos = it.basic.as_ref().and_then(|b| b.position.clone()).unwrap_or_default();
                    pos.guid == page && pos.x > x - 3 && pos.x < x + 3 && pos.y > y - 5 && pos.y < y + 5
                });
                if !busy {
                    return Some(tgf::DItemPosition { r#type: 1, guid: page, x, y, limit_option: None });
                }
            }
        }
    }
    None
}

fn new_item_guid(_p: &mut PlayerModel) -> i64 {
    crate::guid::next_item()
}

fn load_templates(p: &PlayerModel) -> Vec<tgf::DUserFacadeSkinGroup> {
    p.state
        .get("player_facade_templates")
        .and_then(|v| serde_json::from_value::<Vec<u8>>(v.clone()).ok())
        .and_then(|b| tgf::DUserFacadeData::decode(b.as_slice()).ok())
        .map(|d| d.group_skins)
        .unwrap_or_default()
}

fn save_templates(p: &mut PlayerModel, list: Vec<tgf::DUserFacadeSkinGroup>) {
    let bytes = tgf::DUserFacadeData { group_skins: list, ..Default::default() }.encode_to_vec();
    p.state["player_facade_templates"] = json!(bytes);
}

impl GatewaySession {
    // ---------- domain helpers (public for get_info / friends / guilds / shop) ----------

    /// All DB players (for search by role name / nickname). The DB is small — we filter in Rust.
    pub(super) async fn player_load_all(&self) -> Vec<PlayerModel> {
        match self.db.db.query("SELECT * FROM players").await {
            Ok(mut r) => r.take::<Vec<PlayerModel>>(0).unwrap_or_default(),
            Err(e) => {
                eprintln!("[Player] SELECT players: {}", e);
                Vec::new()
            }
        }
    }

    /// Player by user_id; the current one — from its own record
    async fn player_by_uid(&self, me: &PlayerModel, user_id: u64) -> Option<PlayerModel> {
        if user_id == 0 || user_id == me.user_id {
            Some(me.clone())
        } else {
            self.load_by_user_id(user_id).await
        }
    }

    /// DPlayerBasicData like UserCache in cache_batch_get, but with the real profile
    pub(super) fn player_basic_data(&self, p: &PlayerModel, zone_id: u32) -> tgf::DPlayerBasicData {
        let now = Self::now_ts();
        let active = p.roles.iter().find(|r| r.role_guid == p.hall_role_guid).or_else(|| p.roles.first());
        tgf::DPlayerBasicData {
            user_key: Some(tgf::DPlayerIdKey { user_id: p.user_id, zone_id }),
            account_type: 21,
            account_login_channel: 21,
            account_id: p.user_id,
            login_data_cache: Some(tgf::DLoginBasicDataCache {
                login_time: now,
                business_register_time: now,
                business_login_time: now,
                ..Default::default()
            }),
            social_information: Some(tgf::DPlayerSocialInformation::default()),
            profile: Some(player_profile(p)),
            player_status_cache: Some(tgf::DPlayerStatusCache {
                current_role_type_id: active.map(|r| r.type_id as i32).unwrap_or(0),
                current_role_guid: active.map(|r| r.role_guid as i64).unwrap_or(0),
                current_role_meta: active.and_then(|r| r.data.basic.as_ref()).and_then(|b| b.meta.clone()),
                ..Default::default()
            }),
            player_level: 1,
            user_data_version: 1,
            shared_options: Some(tgf::DPlayerSharedOptions {}),
            ..Default::default()
        }
    }

    /// Which facades the player has: unlocked by default (on_sales) + state["player_facade_owned"]
    pub(super) fn player_facade_owned(&self, p: &PlayerModel) -> Vec<i32> {
        let mut ids: Vec<i32> = if UNLOCK_ALL_ON_SALE_FACADES {
            self.game_data.facade.values().filter(|f| f.on_sales).map(|f| f.id).collect()
        } else {
            DEFAULT_FACADES.iter().copied().filter(|id| self.game_data.facade.contains_key(id)).collect()
        };
        if let Some(arr) = p.state["player_facade_owned"].as_array() {
            ids.extend(arr.iter().filter_map(|v| v.as_i64()).map(|v| v as i32));
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// "Appearance received" push (shop purchase, GM): the updated records + the new collection total
    pub(super) fn facade_granted_dirty(&self, p: &PlayerModel, ids: &[i32]) -> tgf::ScPlayerDirtyChgSync {
        let fd = self.player_facade_data(p);
        let now = Self::now_ts();
        tgf::ScPlayerDirtyChgSync {
            dirty_facade_data: Some(tgf::DUserFacadeDirtyData {
                is_dirty: true,
                total_collectors_value: fd.total_collectors_value,
                updated_datas: ids.iter().map(|&item_id| tgf::DUserFacadeMeta { item_id, expire_timepoint: 0, acquire_time: now }).collect(),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn player_facade_type(&self, id: i32) -> i32 {
        self.game_data.facade.get(&id).map(|f| f.r#type).unwrap_or(0)
    }

    fn player_role_wear_info(r: &crate::structures::RoleModel) -> tgf::DUserFacadeSkinRoleWearInfo {
        let info = r.data.deploy_role.as_ref().and_then(|d| d.dress_up.as_ref()).and_then(|d| d.dress_facade_info.clone()).unwrap_or_default();
        tgf::DUserFacadeSkinRoleWearInfo {
            role_id: r.role_guid as i64,
            wear_info: info.skin_ids.iter().map(|&item_id| tgf::DUserFacadeSkinWearInfo { item_id }).collect(),
            hidden_helmet: info.hidden_helmet,
            item_skins: vec![],
        }
    }

    /// DUserFacadeData for player_get_info (need_user_facade)
    pub(super) fn player_facade_data(&self, p: &PlayerModel) -> tgf::DUserFacadeData {
        let mut d = tgf::DUserFacadeData::default();
        for id in self.player_facade_owned(p) {
            let Some(cfg) = self.game_data.facade.get(&id) else { continue };
            d.total_collectors_value += cfg.worth_value;
            // TODO(verify): acquire_time 0 and expire 0 = forever?
            let meta = tgf::DUserFacadeMeta { item_id: id, expire_timepoint: 0, acquire_time: 0 };
            match cfg.r#type {
                FACADE_AVATAR => d.avatars.push(meta),
                FACADE_AVATAR_BORDER => d.avatar_borders.push(meta),
                FACADE_TITLE => d.titles.push(meta),
                FACADE_SKIN => d.skins.push(meta),
                _ => {}
            }
        }
        d.group_skins = load_templates(p);
        d.template_qty = d.group_skins.len() as i32;
        d.role_wear_info = p.roles.iter().map(Self::player_role_wear_info).collect();
        d
    }

    /// Reset the client's profile cache (UserCache) and, if given, the role's
    fn player_push_cache_expired(&self, user_id: u64, role_guid: Option<u64>) {
        let mut keys = vec![tgf::DCacheKey { type_id: CACHE_USER, zone_id: ZONE_ID, inst_id: user_id }];
        if let Some(g) = role_guid {
            keys.push(tgf::DCacheKey { type_id: CACHE_ROLE_BASIC, zone_id: ZONE_ID, inst_id: g });
            keys.push(tgf::DCacheKey { type_id: CACHE_ROLE_DETAIL, zone_id: ZONE_ID, inst_id: g });
        }
        self.push("tgf.GameClientService.cache_expired", "tgf.SCCacheExpiredSync", tgf::ScCacheExpiredSync { keys });
    }

    /// Set the avatar/frame/title by facade type; None — the id is not an avatar/frame/title
    fn player_apply_avatar(&self, p: &mut PlayerModel, id: i32, fallback_slot: Option<usize>) -> Option<()> {
        let slot = match self.player_facade_type(id) {
            FACADE_AVATAR => 0,
            FACADE_AVATAR_BORDER => 1,
            FACADE_TITLE => 2,
            // an id not from facade.json (e.g. the default 309010001) — by position in the request
            0 => fallback_slot?,
            _ => return None,
        };
        let (key, times) = [("avatar", "avatar_times"), ("avatar_box", "avatar_box_times"), ("title", "title_times")][slot];
        if prof_i32(p, key) != Some(id) {
            prof_set(p, key, json!(id));
            let t = prof_i32(p, times).unwrap_or(0) + 1;
            prof_set(p, times, json!(t));
        }
        Some(())
    }

    fn player_push_avatar_dirty(&self, p: &PlayerModel) {
        let prof = player_profile(p);
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_facade_data: Some(tgf::DUserFacadeDirtyData {
                is_dirty: true,
                updated_avatars: Some(tgf::DUserAvatarDirtyData {
                    type_id: prof.avatar_icon_type_id,
                    box_type_id: prof.avatar_box_icon_type_id,
                    title_type_id: prof.title_type_id,
                }),
                ..Default::default()
            }),
            ..Default::default()
        });
        self.player_push_cache_expired(p.user_id, None);
    }

    // ---------- RPC ----------

    pub(super) async fn player_get_info_by_page_impl(&self, req: tgf::CsPlayerGetInfoByPageReq) -> Option<tgf::ScPlayerGetInfoByPageRsp> {
        // need_type: EN_PGINC_RANDOM_POOL = 17, EN_PGINC_GEAR_INVENTORY = 18. The response — only the page echo.
        // TODO(verify): what data the client expects as a push after the response (stash pages? the reward pool?)
        println!("[Player] get_info_by_page need_type={} page {}x{}", req.need_type, req.page_no, req.page_count);
        Some(tgf::ScPlayerGetInfoByPageRsp {
            page_no: req.page_no,
            page_count: req.page_count,
            page_data: Some(tgf::sc_player_get_info_by_page_rsp::PageData::Tmp(0)),
        })
    }

    pub(super) async fn player_batch_get_basic_info_impl(&self, req: tgf::CsPlayerBatchGetBasicInfoReq) -> Option<tgf::ScPlayerBatchGetBasicInfoRsp> {
        let me = self.load_me().await;
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        // our account_id = user_id (cache_batch_get returns account_id: user_id)
        let keys = req
            .user_keys_with_status
            .iter()
            .filter_map(|k| k.key.as_ref().map(|k| (k.user_id, if k.zone_id != 0 { k.zone_id } else { ZONE_ID })))
            .chain(req.account_ids_with_status.iter().map(|a| (a.account_id, ZONE_ID)));
        for (uid, zone) in keys {
            if !seen.insert(uid) {
                continue;
            }
            match self.player_by_uid(&me, uid).await {
                Some(p) => out.push(self.player_basic_data(&p, zone)),
                None => println!("[Player] batch_get_basic_info: user_id {} not found", uid),
            }
        }
        Some(tgf::ScPlayerBatchGetBasicInfoRsp { user_basic_datas: out, client_tag: req.client_tag })
    }

    pub(super) async fn player_batch_get_id_key_impl(&self, req: tgf::CsPlayerBatchGetIdKeyReq) -> Option<tgf::ScPlayerBatchGetIdKeyRsp> {
        let me = self.load_me().await;
        let mut rsp = tgf::ScPlayerBatchGetIdKeyRsp { tag: req.tag, ..Default::default() };

        for &acc in &req.account_ids {
            if let Some(p) = self.player_by_uid(&me, acc).await {
                rsp.user_keys.push(tgf::DPlayerAccountIdRefer { user_id: p.user_id, zone_id: ZONE_ID, account_id: acc });
            }
        }
        for k in &req.user_key {
            if self.player_by_uid(&me, k.user_id).await.is_some() {
                rsp.user_key.push(k.clone());
            }
        }

        if !req.role_names.is_empty() || !req.role_indexs.is_empty() {
            let all = self.player_load_all().await;
            for n in &req.role_names {
                let found = all.iter().find_map(|p| {
                    p.roles
                        .iter()
                        .find(|r| r.role_name == n.role_name || r.data.basic.as_ref().map(|b| b.role_name == n.role_name).unwrap_or(false))
                        .map(|r| (p.user_id, r.role_guid))
                });
                if let Some((user_id, guid)) = found {
                    rsp.role_names.push(tgf::DRoleNameRefer { user_id, zone_id: ZONE_ID, role_guid: guid as i64, name: n.role_name.clone() });
                }
            }
            // TODO(verify): our create_index is the role number within the account (role_create), not globally unique; take the first
            for k in &req.role_indexs {
                let found = all.iter().find_map(|p| {
                    p.roles.iter().find(|r| {
                        r.data.basic.as_ref().map(|b| b.type_id == k.type_id && b.create_index == k.create_index).unwrap_or(false)
                    }).map(|r| (p.user_id, r.role_guid))
                });
                if let Some((user_id, guid)) = found {
                    rsp.role_indexs.push(tgf::DRoleIndexRefer { user_id, zone_id: ZONE_ID, role_guid: guid as i64, role_index: Some(k.clone()) });
                }
            }
        }
        Some(rsp)
    }

    pub(super) async fn player_rename_impl(&self, req: tgf::CsPlayerRenameReq) -> Option<tgf::ScPlayerRenameRsp> {
        let mut p = self.load_me().await;
        let name = req.name.trim().to_string();
        let old = tgf::ScPlayerRenameRsp { name: player_nick(&p), stat_custom_nick_name_times: prof_i32(&p, "rename_times").unwrap_or(0) };

        let reject = if name.is_empty() || nick_weight(&name) > 12.0 {
            Some((ERR_NICKNAME_LENGTH, "nickname length (limit 12, non-ASCII = 1.5)"))
        } else if name.starts_with("游客:") || name.chars().any(|c| c.is_control()) {
            Some((ERR_NICKNAME_PREFIX, "forbidden prefix/characters"))
        } else if self
            .player_load_all()
            .await
            .iter()
            .any(|o| o.user_id != p.user_id && prof_str(o, "nick_name").map(|n| n.to_lowercase()) == Some(name.to_lowercase()))
        {
            Some((ERR_NAME_TAKEN, "nickname taken"))
        } else {
            None
        };
        if let Some((code, why)) = reject {
            println!("[Player] rename '{}' rejected: {}", name, why);
            self.set_rpc_error(code);
            return Some(old);
        }

        prof_set(&mut p, "nick_name", json!(name));
        let times = old.stat_custom_nick_name_times + 1;
        prof_set(&mut p, "rename_times", json!(times));
        self.save_me(&p).await;
        println!("[Player] {} rename → '{}'", p.open_id, name);
        self.player_push_cache_expired(p.user_id, None);
        Some(tgf::ScPlayerRenameRsp { name, stat_custom_nick_name_times: times })
    }

    pub(super) async fn player_update_icon_impl(&self, req: tgf::CsPlayerUpdateIconReq) -> Option<tgf::ScPlayerUpdateIconRsp> {
        // TODO(verify): icon_type_id order. We take the type from facade.json; ids outside the table — by position [avatar, frame, title]
        let mut p = self.load_me().await;
        let owned = self.player_facade_owned(&p);
        let mut applied = Vec::new();
        for (i, &id) in req.icon_type_id.iter().enumerate() {
            if id == 0 || (self.game_data.facade.contains_key(&id) && !owned.contains(&id)) {
                continue;
            }
            if self.player_apply_avatar(&mut p, id, Some(i).filter(|i| *i < 3)).is_some() {
                applied.push(id);
            }
        }
        if !applied.is_empty() {
            self.save_me(&p).await;
            self.player_push_avatar_dirty(&p);
        } else if req.icon_type_id.iter().any(|&id| id != 0) {
            println!("[Player] update_icon {:?}: no id fits", req.icon_type_id);
            self.set_rpc_error(ERR_INVALID_ICON);
        }
        Some(tgf::ScPlayerUpdateIconRsp { icon_type_id: applied })
    }

    pub(super) async fn player_update_options_impl(&self, req: tgf::CsPlayerUpdateOptionsReq) -> Option<tgf::ScPlayerUpdateOptionsRsp> {
        // DPlayerSharedOptions is empty — nothing to store; private_options — client bytes
        if let Some(opts) = req.update_private_options {
            let mut p = self.load_me().await;
            p.state["player_private_options"] = json!(opts.client_options);
            self.save_me(&p).await;
        }
        Some(tgf::ScPlayerUpdateOptionsRsp {})
    }

    pub(super) async fn player_gm_command_impl(&self, req: tgf::CsPlayerGmCommandReq) -> Option<tgf::ScPlayerGmCommandRsp> {
        let me = self.clean_open_id();
        // account roles (src/auth): gm/admin; without an account — only on a server without login (local tests)
        if !crate::auth::accounts::may_gm(&self.db, &me).await {
            println!("[GM] {}: no gm/admin role — rejected", me);
            // like the other GM rejections: the code in the body, not in the header
            return Some(tgf::ScPlayerGmCommandRsp { result_code: ERR_GM_CMD_INVALID, result_message: "no GM rights".into(), result_list: vec![] });
        }
        Some(self.gm_exec(&req.cmd_args, &me).await)
    }

    /// A GM command on the player of this session without a rights check (the caller checks the rights: the client — above,
    /// the admin API — src/admin_api.rs). `actor` — who executes it (for the log and `banned_by`).
    pub(crate) async fn gm_exec(&self, cmd_args: &[String], actor: &str) -> tgf::ScPlayerGmCommandRsp {
        // The client format is unknown: accept both ["cmd","a","b"] and a single string "cmd a b"
        let args: Vec<String> = cmd_args.iter().flat_map(|a| a.split_whitespace().map(str::to_string).collect::<Vec<_>>()).collect();
        let cmd = args.first().map(|s| s.trim_start_matches(['/', '.']).to_lowercase()).unwrap_or_default();
        let num = |i: usize| args.get(i).and_then(|s| s.parse::<i64>().ok());
        let mut p = self.load_me().await;
        println!("[GM] {} on {}: {:?}", actor, p.open_id, args);

        let result: Result<Vec<String>, String> = match cmd.as_str() {
            "" | "help" | "?" => Ok(vec![
                "gold <n>                 — +n gold (n < 0 — charge)".into(),
                "coin <coin_id> <n>       — +n coins".into(),
                "item <type_id> [count]   — item to the stash".into(),
                "itemaff <type_id> <property>[:%] ... [max] — item to the stash with these affixes (property — id or name, % 0..100, default 100; max — the rest at 100% too)".into(),
                "affixes <type_id>        — which affixes can be on the item (for itemaff)".into(),
                "udp on|off               — match UDP tunnel via the relay (turns on by itself if UDP is cut)".into(),
                "facade <id>              — grant an appearance (avatar/frame/title/skin)".into(),
                "mail [type_id] [count]   — a system mail to self (with an item attached)".into(),
                "exp <n>                  — +n exp to the hall role (level-up as after a raid, dirty_role push)".into(),
                "info                     — profile and roles".into(),
                "rank [add <k> [team]|reset] — the hall role's ranking places / a match with k kills into the history / reset".into(),
                "ban <open_id|name> [reason] — account ban: sessions are dropped, the player is kicked right away".into(),
                "unban <open_id|name>     — lift the ban".into(),
                "wipe <user_id> [reason]  — full account reset (snapshot in player_wipes; user_id — confirmation)".into(),
                "unwipe <snapshot key>    — restore the account and mail from a wipe snapshot".into(),
            ]),
            // Sh4g (05.11): whether the client accepts battle_settle_sync — in the lobby, without a match. The RuClient [RESULTPROBE] probe shows
            // show=true if UMatchService took the result. `settle` — minimal (exp +1, val_changed), `settle gs` — without
            // result, `settle last` — this player's last real raid result
            "settle" => {
                let role = p.hall_role_guid;
                let (level, exp) = p
                    .roles
                    .iter()
                    .find(|r| r.role_guid == role)
                    .and_then(|r| r.data.deploy_role.as_ref())
                    .map(|d| (d.role_level.max(1), d.exp))
                    .unwrap_or((1, 0));
                let mode = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
                let msg = if mode == "last" {
                    super::raid_native::last_settle().lock().unwrap().get(&p.open_id).cloned()
                } else {
                    Some(tgf::ScBattleSettleSync {
                        result: (mode != "gs").then(|| tgf::DBattlePlayerGameOver {
                            user_key: Some(tgf::DPlayerIdKey { user_id: p.user_id, zone_id: 1 }),
                            role_guid: role as i64,
                            ..Default::default()
                        }),
                        gs_result: Some(tgf::DGsUserSettleResult {
                            old_exp: exp,
                            old_level: level,
                            curr_exp: exp + 1,
                            curr_level: level,
                            settle_interface_type: tgf::DSettleInterFaceType::EnSettleInterfaceTypePve as i32,
                            val_changed: true,
                            ..Default::default()
                        }),
                        level_parameter: None,
                    })
                };
                match msg {
                    Some(m) => {
                        let online = crate::lobby::hub::push_to(&p.open_id, "tgf.GameClientService.battle_settle_sync", "tgf.SCBattleSettleSync", &m);
                        Ok(vec![format!("battle_settle_sync ({}) sent, online={}", if mode.is_empty() { "minimal" } else { &mode }, online)])
                    }
                    None => Err("no last result (settle last after a raid)".into()),
                }
            }
            "rank" => self.rank_gm(&mut p, &args).await,
            // Sh4c(4): check "a level-up breaks the lobby" without a match — the same path as raid_finish_exp
            "exp" | "addexp" | "add_exp" => match num(1).map(|v| v as i32) {
                Some(n) if n > 0 => {
                    let role = p.hall_role_guid;
                    match self.progress_add_role_exp(&mut p, role, n) {
                        Some((old, new)) => {
                            self.save_me(&p).await;
                            self.raid_push_role(&p, role);
                            Ok(vec![format!("role {}: +{} exp, level {} → {}", role, n, old, new)])
                        }
                        None => Err(format!("no hall role {} or deploy_role", role)),
                    }
                }
                _ => Err("exp <n>, n > 0".into()),
            },
            "gold" | "addgold" | "add_gold" | "money" => match num(1) {
                Some(n) => match common::add_coin(&mut p, GOLD, n) {
                    Some(now) => {
                        self.save_me(&p).await;
                        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_coin_data: vec![common::coin_dirty(&p, GOLD)], ..Default::default() });
                        Ok(vec![format!("gold = {}", now)])
                    }
                    None => Err("not enough gold".into()),
                },
                None => Err("gold <n>".into()),
            },
            "coin" | "addcoin" | "add_coin" => match (num(1), num(2)) {
                (Some(id), Some(n)) => match common::add_coin(&mut p, id as i32, n) {
                    Some(now) => {
                        self.save_me(&p).await;
                        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_coin_data: vec![common::coin_dirty(&p, id as i32)], ..Default::default() });
                        Ok(vec![format!("coin {} = {}", id, now)])
                    }
                    None => Err("the balance would go negative".into()),
                },
                _ => Err("coin <coin_id> <n>".into()),
            },
            // match UDP tunnel (auth::udp_tunnel): turns on by itself, this command — manual control
            "udp" => match args.get(1).map(|s| s.as_str()) {
                Some(v @ ("on" | "off")) => {
                    let on = v == "on";
                    crate::auth::udp_tunnel::choose(&self.db, &p.open_id, on, "gm", "udp command").await;
                    p.state["udp_tunnel"] = serde_json::json!(on);
                    self.db.save("players", &p.open_id, &p).await;
                    Ok(vec![format!("UDP tunnel {} for {}", v, p.open_id)])
                }
                _ => Err("udp on|off".into()),
            },
            // instance lobby without the launcher (local tests, investigation on prod): the same as the launcher screen buttons
            "lobby" if !crate::lobby::launcher_lobby::enabled() => Err("lobby is off (LAUNCHER_LOBBY)".into()),
            "lobby" => {
                use crate::lobby::launcher_lobby as lobby;
                let open_id = p.open_id.clone();
                let done = match args.get(1).map(|s| s.as_str()) {
                    // join 100 — the "Clotho" tab (lobby::launcher_lobby::CLOTHO_ID)
                    Some("join") => lobby::join_async(&open_id, num(2).unwrap_or(0) as usize).await,
                    Some("size") => lobby::vote(&open_id, num(2).map(|n| n as usize), None),
                    Some("map") => lobby::vote(&open_id, None, num(2)),
                    Some("leave") => {
                        lobby::leave(&open_id);
                        Ok(())
                    }
                    _ => Ok(()),
                };
                match done {
                    Ok(()) => Ok(vec![lobby::snapshot_async(&open_id).await.to_string()]),
                    Err(e) => Err(format!("{} (lobby [join N | size N | map LEVEL | leave])", e.code())),
                }
            }
            "item" | "additem" | "add_item" | "give" => match num(1) {
                Some(type_id) => {
                    let type_id = type_id as i32;
                    let count = num(2).unwrap_or(1).clamp(1, 9999) as i32;
                    if !self.game_data.item_type.contains_key(&type_id) {
                        Err(format!("no item_type {}", type_id))
                    } else {
                        // like a reward: the shared stash layout (item size, stacks), not its own 3x5 block from the end
                        self.item_gm_give(type_id, count).await
                    }
                }
                None => Err("item <type_id> [count]".into()),
            },
            "itemaff" | "item_affix" | "giveaff" => match num(1).map(|v| v as i32) {
                Some(type_id) if self.game_data.item_type.contains_key(&type_id) => {
                    let mut wishes = Vec::new();
                    let mut max_all = false;
                    let mut bad = Vec::new();
                    for a in args.iter().skip(2) {
                        if a.eq_ignore_ascii_case("max") {
                            max_all = true;
                            continue;
                        }
                        let (name, pct) = a.split_once([':', '=']).map_or((a.as_str(), Some(100.0)), |(n, v)| (n, v.trim_end_matches('%').parse::<f64>().ok()));
                        match (self.affix_attr_parse(name), pct) {
                            (Some(aid), Some(pct)) if (0.0..=100.0).contains(&pct) => wishes.push((aid, ((pct * 9999.0 / 100.0).round() as i32).clamp(0, 9999))),
                            _ => bad.push(a.clone()),
                        }
                    }
                    if !bad.is_empty() {
                        Err(format!("did not understand: {} (need <id|name>[:0..100], see affixes {})", bad.join(" "), type_id))
                    } else {
                        self.item_gm_give_affixed(type_id, &wishes, max_all).await
                    }
                }
                Some(t) => Err(format!("no item_type {}", t)),
                None => Err("itemaff <type_id> <property>[:%] ... [max]".into()),
            },
            "affixes" | "affix" => match num(1).map(|v| v as i32) {
                Some(t) => self.gm_affix_options(t, self.inv_gear_wear_part(t)),
                None => Err("affixes <type_id>".into()),
            },
            "facade" | "skin" => match num(1).map(|v| v as i32) {
                Some(id) if self.game_data.facade.contains_key(&id) => {
                    facade_own(&mut p, id);
                    self.save_me(&p).await;
                    self.push_dirty(self.facade_granted_dirty(&p, &[id]));
                    Ok(vec![format!("facade {} granted", id)])
                }
                _ => Err("facade <id from facade.json>".into()),
            },
            // mail check: a letter to self; mail <type_id> [count] — with an item (currency goes to attachments_offset)
            "mail" => match num(1).map(|v| v as i32) {
                Some(t) if !self.game_data.item_type.contains_key(&t) => Err(format!("no item_type {}", t)),
                t => {
                    let items: Vec<tgf::DItem> = t
                        .map(|type_id| {
                            let guid = crate::guid::next_item();
                            let affix_data = if self.game_data.affix_template.contains_key(&type_id) {
                                self.build_item_affix_data_seeded(type_id, 0, guid as u64)
                            } else {
                                vec![]
                            };
                            let count = num(2).unwrap_or(1).clamp(1, 9999) as i32;
                            tgf::DItem {
                                basic: Some(tgf::DItemBasic { type_id, count, guid, expire_timepoint: 0, position: None }),
                                data: Some(tgf::DItemData { affix_data, ..Default::default() }),
                                create_info: None,
                            }
                        })
                        .into_iter()
                        .collect();
                    let n = items.len();
                    let mail = super::progress_handlers::mail_system("Проверка почты", "Письмо от GM-команды /mail.", items);
                    let id = super::progress_handlers::mail_send(&self.db, &self.game_data, &p.open_id, mail).await;
                    Ok(vec![format!("mail {} sent, attachments {}", id, n)])
                }
            },
            // 13.11 returning items from a match: raidrestore <gameover-…bin from restore/> — the list; … apply — by mail with new guids
            // (the item could have gone to the killer with the same guid). One record — once (state.raid_restored).
            "raidrestore" => match args.get(1) {
                None => Err("raidrestore <gameover-<open_id>-3-HHMMSS.bin from restore/> [apply]".into()),
                Some(file) => match super::raid_native::raid_restore_items(&p, file) {
                    Err(e) => Err(e),
                    Ok((role, items)) => {
                        let list: Vec<String> = items
                            .iter()
                            .map(|i| i.basic.as_ref().map_or(String::from("?"), |b| format!("{} x{} (guid {})", b.type_id, b.count, b.guid)))
                            .collect();
                        let done = p.state["raid_restored"].as_array().is_some_and(|a| a.iter().any(|f| f.as_str() == Some(file.as_str())));
                        if args.get(2).map(|s| s.as_str()) != Some("apply") {
                            Ok(std::iter::once(format!("role {}: return {} items{} — raidrestore {} apply", role, items.len(),
                                if done { " (ALREADY RETURNED)" } else { "" }, file)).chain(list).collect())
                        } else if done {
                            Err(format!("{} already returned", file))
                        } else if items.is_empty() {
                            Err("nothing to return".into())
                        } else {
                            let mut mails = vec![];
                            for chunk in items.chunks(10) {
                                let mut part = chunk.to_vec();
                                for it in &mut part {
                                    if let Some(b) = it.basic.as_mut() {
                                        b.guid = crate::guid::next_item();
                                        b.position = None;
                                    }
                                }
                                let mail = super::progress_handlers::mail_system("Возврат вещей", "Вещи из боя, возвращённые администрацией.", part);
                                mails.push(super::progress_handlers::mail_send(&self.db, &self.game_data, &p.open_id, mail).await);
                            }
                            let mut me = self.load_me().await;
                            let mut restored = me.state["raid_restored"].as_array().cloned().unwrap_or_default();
                            restored.push(serde_json::json!(file));
                            me.state["raid_restored"] = serde_json::json!(restored);
                            self.save_me(&me).await;
                            println!("[Restore] {}: from {} returned {} items by mail {:?}", me.open_id, file, items.len(), mails);
                            Ok(std::iter::once(format!("returned {} items by mail {:?}", items.len(), mails)).chain(list).collect())
                        }
                    }
                },
            },
            "info" => Ok(std::iter::once(format!("user_id={} nick='{}' gold={} hall={}", p.user_id, player_nick(&p), common::coin(&p, GOLD), p.hall_role_guid))
                .chain(p.roles.iter().map(|r| format!("role {} '{}' type={} lazy_del={}", r.role_guid, r.role_name, r.type_id, r.data.lazy_delete_timepoint)))
                .collect()),
            "ban" | "unban" => match args.get(1) {
                None => Err(format!("{} <open_id|name> [reason]", cmd)),
                Some(who) => {
                    let reason = args[2..].join(" ");
                    crate::auth::ban::set(&self.db, who, cmd == "ban", &reason, actor).await.map(|a| {
                        vec![format!(
                            "{} {} '{}' ({}){}",
                            if a.banned { "banned" } else { "unbanned" },
                            a.open_id,
                            a.display_name,
                            a.provider,
                            if a.banned && !a.ban_reason.is_empty() { format!(": {}", a.ban_reason) } else { String::new() }
                        )]
                    })
                }
            },
            // 16.11: full account reset for investigations (account_wipe.rs)
            "wipe" => self.gm_wipe(&p, &args, actor).await,
            "unwipe" => self.gm_unwipe(&p, &args, actor).await,
            other => Err(format!("unknown command '{}', see help", other)),
        };

        match result {
            Ok(list) => tgf::ScPlayerGmCommandRsp { result_code: 0, result_message: "ok".into(), result_list: list },
            // EN_ERR_PLAYER_GM_CMD_INVALID / _PARAM_INVALID
            Err(msg) => {
                let code = if msg.starts_with("unknown command") { ERR_GM_CMD_INVALID } else { ERR_GM_CMD_PARAM_INVALID };
                tgf::ScPlayerGmCommandRsp { result_code: code, result_message: msg, result_list: vec![] }
            }
        }
    }

    /// Put a new item into the stash (position `at` or a free block). Affixes — as for the starting equipment.
    fn player_add_stash_item(&self, p: &mut PlayerModel, type_id: i32, count: i32, at: Option<tgf::DItemPosition>) -> Option<tgf::DItem> {
        let position = at.or_else(|| stash_free_block(p))?;
        let guid = new_item_guid(p);
        let affix_data = if self.game_data.affix_template.contains_key(&type_id) {
            self.build_item_affix_data_seeded(type_id, 0, guid as u64)
        } else {
            vec![]
        };
        let item = tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id, count, guid, expire_timepoint: 0, position: Some(position) }),
            data: Some(tgf::DItemData { affix_data, ..Default::default() }),
            create_info: None,
        };
        p.stash.element.push(item.clone());
        Some(item)
    }

    pub(super) async fn player_receive_homeland_reward_impl(&self, req: tgf::CsPlayerReceiveHomelandRewardReq) -> Option<tgf::ScPlayerReceiveHomelandRewardRsp> {
        // Reward of the "homeland" mode: an item into the given stash place, with mail = true officially — by mail
        // (const homeland_mode_reward_mail_template_id = 501049). Mail is another domain, we put it into the stash.
        // TODO(verify): who and how many times may receive it; currently — without checking the reward source.
        if !self.game_data.item_type.contains_key(&req.item_id) {
            println!("[Player] homeland_reward: no item_type {}", req.item_id);
            self.set_rpc_error(ERR_ITEM_NOT_IN_TABLE);
            return Some(tgf::ScPlayerReceiveHomelandRewardRsp {});
        }
        let mut p = self.load_me().await;
        let at = req.position.filter(|pos| {
            pos.r#type == 1
                && (0..super::inventory_handlers::stash_page_count(&p)).contains(&pos.guid) // ext: stash_pages
                && !p.stash.element.iter().any(|it| {
                    let q = it.basic.as_ref().and_then(|b| b.position.clone()).unwrap_or_default();
                    q.guid == pos.guid && q.x == pos.x && q.y == pos.y
                })
        });
        if let Some(item) = self.player_add_stash_item(&mut p, req.item_id, 1, at) {
            self.save_me(&p).await;
            self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_postion_action: vec![item], ..Default::default() });
        } else {
            println!("[Player] homeland_reward: stash full, {} not granted", req.item_id);
            self.set_rpc_error(ERR_ITEM_NO_SPACE);
        }
        Some(tgf::ScPlayerReceiveHomelandRewardRsp {})
    }

    pub(super) async fn player_birthday_update_impl(&self, req: tgf::CsPlayerUpdateBirthDayReq) -> Option<tgf::ScPlayerUpdateBirthDayRsp> {
        let mut p = self.load_me().await;
        let bd = req.birth_day.trim();
        if bd.is_empty() || bd.len() > 32 {
            println!("[Player] birthday '{}' rejected", bd);
            self.set_rpc_error(ERR_BIRTHDAY_FORMAT);
            return Some(tgf::ScPlayerUpdateBirthDayRsp { birth_day: player_birthday(&p) });
        }
        prof_set(&mut p, "birthday", json!(bd));
        self.save_me(&p).await;
        Some(tgf::ScPlayerUpdateBirthDayRsp { birth_day: bd.to_string() })
    }

    pub(super) async fn hall_switch_impl(&self, req: tgf::CsHallSwitchReq) -> Option<tgf::ScHallSwitchRsp> {
        // EnHallSwitchType: 1 — adventurers' hall, 2 — Mithril Force hall (mithril_* — the shop domain).
        // TODO(verify): whether a push is needed (dirty_hall_setting / mithril) after switching the hall
        let mut p = self.load_me().await;
        p.state["player_hall_switch_type"] = json!(req.switch_type);
        self.save_me(&p).await;
        Some(tgf::ScHallSwitchRsp {})
    }

    /// Role deletion: the role's items (worn/backpacks) go together with it, the account stash is not touched.
    /// The official rejections EN_ERR_ROLE_DELETE_FAILED_ONLY_ONE_ROLE / _BACKPACK_NOT_CLEAR / _WEARING_NOT_EMPTY
    /// (-709/-708/-707) are not applied: every role has starting equipment, and the last role can be recreated.
    pub(super) async fn role_delete_impl(&self, req: tgf::CsRoleDeleteReq) -> Option<tgf::ScRoleDeleteRsp> {
        let mut p = self.load_me().await;
        let guid = req.role_guid as u64;
        let before = p.roles.len();
        p.roles.retain(|r| r.role_guid != guid);
        if p.roles.len() == before {
            println!("[Role] delete: no role {}", guid);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
            return Some(tgf::ScRoleDeleteRsp { deleted_role_guid: 0 });
        }
        let hall_changed = fix_hall_role(&mut p);
        self.save_me(&p).await;
        Self::write_client_loadout_file(&p);
        println!("[Role] {} deleted role {} (hall → {})", p.open_id, guid, p.hall_role_guid);
        if hall_changed {
            self.push_dirty(tgf::ScPlayerDirtyChgSync {
                dirty_hall_setting: Some(tgf::DHallSetting { role_guid: p.hall_role_guid as i64 }),
                ..Default::default()
            });
        }
        self.player_push_cache_expired(p.user_id, Some(guid));
        Some(tgf::ScRoleDeleteRsp { deleted_role_guid: req.role_guid })
    }

    /// Lazy deletion: lazy_delete_timepoint = now + role_lazy_delete_duration (24h). The client runs the
    /// timer itself (OnLazyDeleteTimeReached); expired roles are deleted by player_purge_lazy_deleted_roles on login.
    pub(super) async fn role_delete_lazy_impl(&self, req: tgf::CsRoleDeleteLazyReq) -> Option<tgf::ScRoleDeleteLazyRsp> {
        let at = Self::now_ts() + self.player_lazy_delete_secs();
        self.player_set_lazy_delete(req.role_guid, at).await;
        Some(tgf::ScRoleDeleteLazyRsp { deleted_role_guid: req.role_guid })
    }

    pub(super) async fn role_cancel_lazy_delete_impl(&self, req: tgf::CsRoleCancelLazyDeleteReq) -> Option<tgf::ScRoleCancelLazyDeleteRsp> {
        self.player_set_lazy_delete(req.role_guid, 0).await;
        Some(tgf::ScRoleCancelLazyDeleteRsp { deleted_role_guid: req.role_guid })
    }

    /// role_lazy_delete_duration from const.json ("24h" / "30m" / "90s" / a number of seconds), default one day
    fn player_lazy_delete_secs(&self) -> i64 {
        let v = self.game_data.r#const.iter().find(|c| c.key == "role_lazy_delete_duration").map(|c| c.value.trim().to_string()).unwrap_or_default();
        let (n, mul) = match v.chars().last() {
            Some('h') => (&v[..v.len() - 1], 3600),
            Some('m') => (&v[..v.len() - 1], 60),
            Some('s') => (&v[..v.len() - 1], 1),
            Some('d') => (&v[..v.len() - 1], 86400),
            _ => (v.as_str(), 1),
        };
        n.parse::<i64>().map(|n| n * mul).unwrap_or(86400)
    }

    async fn player_set_lazy_delete(&self, role_guid: i64, at: i64) {
        let mut p = self.load_me().await;
        let Some(role) = p.get_role_mut(role_guid as u64) else {
            println!("[Role] lazy_delete: no role {}", role_guid);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
            return;
        };
        role.data.lazy_delete_timepoint = at;
        let data = role.data.clone();
        self.save_me(&p).await;
        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_role: vec![data], ..Default::default() });
    }

    pub(super) async fn role_fetch_user_role_impl(&self, req: tgf::CsRoleFetchUserRoleReq) -> Option<tgf::ScRoleFetchUserRoleRsp> {
        let me = self.load_me().await;
        let guid = req.role_id as u64;
        if let Some(r) = me.roles.iter().find(|r| r.role_guid == guid) {
            return Some(tgf::ScRoleFetchUserRoleRsp { role: Some(r.data.clone()) });
        }
        // Someone else's role: guid = user_id * 100 + n (PlayerModel::add_role)
        let other = self.load_by_user_id(guid / 100).await;
        let role = other.and_then(|p| p.roles.into_iter().find(|r| r.role_guid == guid)).map(|r| r.data);
        if role.is_none() {
            println!("[Role] fetch_user_role: {} not found", guid);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
        }
        Some(tgf::ScRoleFetchUserRoleRsp { role })
    }

    pub(super) async fn role_move_season_impl(&self, req: tgf::CsRoleMoveSeasonReq) -> Option<tgf::ScRoleMoveSeasonRsp> {
        // Role transfer to another season: deploy_role.limit_option = goal_limit_option.
        // TODO(verify): the season role limit (EN_ERR_ROLE_MOVE_SEASON_FAILED_ROLE_COUNT_MEET_LIMIT), what else changes
        let mut p = self.load_me().await;
        let Some(role) = p.get_role_mut(req.role_guid as u64) else {
            println!("[Role] move_season: no role {}", req.role_guid);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
            return Some(tgf::ScRoleMoveSeasonRsp {});
        };
        role.data.deploy_role.get_or_insert_with(Default::default).limit_option = req.goal_limit_option;
        let data = role.data.clone();
        self.save_me(&p).await;
        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_role: vec![data], ..Default::default() });
        Some(tgf::ScRoleMoveSeasonRsp {})
    }

    /// Face check against role_face_pinching_limit (the role's race/gender); an empty list in the table = no restrictions
    fn player_face_allowed(&self, meta: Option<&tgf::DRoleMetaIndex>, f: &tgf::DRoleFacePinching) -> Result<(), String> {
        let Some(meta) = meta else { return Ok(()) };
        let Some(lim) = self
            .game_data
            .role_face_pinching_limit
            .iter()
            .find(|l| l.race == meta.race && (l.gender_type == 0 || l.gender_type == meta.gender))
        else {
            return Ok(());
        };
        let checks: [(&str, i32, &Vec<i32>); 11] = [
            ("face", f.face_type, &lim.face_ids),
            ("skin", f.skin_type, &lim.skincolor_ids),
            ("eye", f.eye_color, &lim.eyecolor_ids),
            ("hair", f.hair_type, &lim.hair_ids),
            ("beard", f.beard_type, &lim.beard_ids),
            ("hair_color", f.hair_color, &lim.haircolor_ids),
            ("tatoo", f.tatoo_type, &lim.tatoo_ids),
            ("tatoo_color", f.tatoo_color, &lim.tatoocolor_ids),
            ("scar", f.scar_type, &lim.scar_type_ids),
            ("makeup", f.makeup_type, &lim.makeup_type_ids),
            ("makeup_color", f.makeup_color, &lim.makeup_color_ids),
        ];
        for (name, v, allowed) in checks {
            if v != 0 && !allowed.is_empty() && !allowed.contains(&v) {
                return Err(format!("{}={} not from {:?}", name, v, allowed));
            }
        }
        Ok(())
    }

    pub(super) async fn role_reshape_face_pinching_impl(&self, req: tgf::CsRoleReshapeFacePinchingReq) -> Option<tgf::ScRoleReshapeFacePinchingRsp> {
        // The price (const role_reshape_face_pinching_cost_item = "1040,1,0", req.cost_items) is not charged.
        // TODO(verify): whether 1040 is a coin or an item, and how the client expects the charge (dirty_coin_data / dirty_remove_guid)
        let mut p = self.load_me().await;
        let Some(new_dress) = req.dress_up else { return Some(tgf::ScRoleReshapeFacePinchingRsp {}) };
        let meta = p.roles.iter().find(|r| r.role_guid == req.role_guid as u64).and_then(|r| r.data.basic.as_ref()).and_then(|b| b.meta.clone());
        if let Some(face) = &new_dress.current_face_pinching {
            if let Err(e) = self.player_face_allowed(meta.as_ref(), face) {
                println!("[Role] reshape_face {} rejected: {}", req.role_guid, e);
                self.set_rpc_error(ERR_ROLE_DRESS_UP_INVALID);
                return Some(tgf::ScRoleReshapeFacePinchingRsp {});
            }
        }
        let Some(role) = p.get_role_mut(req.role_guid as u64) else {
            println!("[Role] reshape_face: no role {}", req.role_guid);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
            return Some(tgf::ScRoleReshapeFacePinchingRsp {});
        };
        let deploy = role.data.deploy_role.get_or_insert_with(Default::default);
        let dress = deploy.dress_up.get_or_insert_with(Default::default);
        // Skins (dress_facade_info) are kept — only the face changes
        let facade_info = dress.dress_facade_info.take();
        *dress = tgf::DRoleDressUp { dress_facade_info: new_dress.dress_facade_info.or(facade_info), ..new_dress };
        if let Some(info) = dress.dress_facade_info.as_mut() {
            info.role_id = req.role_guid;
        }
        let data = role.data.clone();
        self.save_me(&p).await;
        Self::write_client_loadout_file(&p);
        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_role: vec![data], ..Default::default() });
        self.player_push_cache_expired(p.user_id, Some(req.role_guid as u64));
        Some(tgf::ScRoleReshapeFacePinchingRsp {})
    }

    pub(super) async fn facade_update_impl(&self, req: tgf::CsFacadeSetReq) -> Option<tgf::ScFacadeSetRsp> {
        // Set the avatar / frame / title (the type — from facade.json)
        let mut p = self.load_me().await;
        if !self.player_facade_owned(&p).contains(&req.item_id) {
            println!("[Facade] set {}: the player does not have it", req.item_id);
            self.set_rpc_error(ERR_FACADE_NOT_OWNED);
            return Some(tgf::ScFacadeSetRsp { item_id: 0 });
        }
        if self.player_apply_avatar(&mut p, req.item_id, None).is_none() {
            println!("[Facade] set {}: not an avatar/frame/title", req.item_id);
            self.set_rpc_error(ERR_FACADE_NOT_SUPPORT);
            return Some(tgf::ScFacadeSetRsp { item_id: 0 });
        }
        self.save_me(&p).await;
        self.player_push_avatar_dirty(&p);
        Some(tgf::ScFacadeSetRsp { item_id: req.item_id })
    }

    pub(super) async fn facade_update_role_skin_impl(&self, req: tgf::CsFacadeUpdateRoleSkinReq) -> Option<tgf::ScFacadeUpdateRoleSkinRsp> {
        // Role skins: facade id (type SKIN) → deploy_role.dress_up.dress_facade_info.skin_ids.
        // The client (Lua) takes facade ids from RoleWearInfo and translates them to ue_skin_ids itself.
        // TODO(verify): what the DS expects in dress_facade_info.skin_ids (facade id ≠ ue_skin_ids only for helmets 1320xxx)
        let mut p = self.load_me().await;
        let owned = self.player_facade_owned(&p);
        let mut ids: Vec<i32> = Vec::new();
        let mut slots = Vec::new();
        for &id in &req.item_ids {
            let Some(cfg) = self.game_data.facade.get(&id) else { continue };
            if cfg.r#type != FACADE_SKIN || !owned.contains(&id) || ids.contains(&id) {
                println!("[Facade] role_skin: {} skipped (not a skin / the player does not have it)", id);
                continue;
            }
            // One skin per slot (EN_ERR_FACADE_PUT_REPEATED_SLOT): the last one wins
            if let Some(&slot) = cfg.put_third_sorts.first() {
                if let Some(i) = slots.iter().position(|s| *s == slot) {
                    slots.remove(i);
                    ids.remove(i);
                }
                slots.push(slot);
            } else {
                slots.push(0);
            }
            ids.push(id);
        }
        let Some(role) = p.get_role_mut(req.role_id as u64) else {
            println!("[Facade] role_skin: no role {}", req.role_id);
            self.set_rpc_error(ERR_ROLE_NOT_FOUND);
            return Some(tgf::ScFacadeUpdateRoleSkinRsp { role_id: req.role_id, item_ids: vec![], hidden_helmet: req.hidden_helmet });
        };
        let deploy = role.data.deploy_role.get_or_insert_with(Default::default);
        deploy.dress_up.get_or_insert_with(Default::default).dress_facade_info =
            Some(tgf::UserRoleSkinWearInfo { role_id: req.role_id, skin_ids: ids.clone(), hidden_helmet: req.hidden_helmet });
        let data = role.data.clone();
        let wear = Self::player_role_wear_info(role);
        self.save_me(&p).await;
        Self::write_client_loadout_file(&p);
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_role: vec![data],
            dirty_facade_data: Some(tgf::DUserFacadeDirtyData { is_dirty: true, role_wear_info: Some(wear), ..Default::default() }),
            ..Default::default()
        });
        self.player_push_cache_expired(p.user_id, Some(req.role_id as u64));
        Some(tgf::ScFacadeUpdateRoleSkinRsp { role_id: req.role_id, item_ids: ids, hidden_helmet: req.hidden_helmet })
    }

    pub(super) async fn facade_save_skin_template_impl(&self, req: tgf::CsFacadeSaveSkinTemplateReq) -> Option<tgf::ScFacadeSaveSkinTemplateRsp> {
        let mut p = self.load_me().await;
        let mut list = load_templates(&p);
        let limit = self
            .game_data
            .r#const
            .iter()
            .find(|c| c.key == "facade_template_list_size")
            .and_then(|c| c.value.trim().parse::<usize>().ok())
            .unwrap_or(20);
        let name = req.name.trim().to_string();
        // EN_ERR_FACADE_TEMPLATE_QTY_LIMIT / _NAME_LIMIT (the name limit is unknown — we take the nickname limit)
        if list.len() >= limit || name.is_empty() || nick_weight(&name) > 12.0 {
            println!("[Facade] save_template '{}' rejected: {} of {} or name length", name, list.len(), limit);
            self.set_rpc_error(if list.len() >= limit { ERR_FACADE_TEMPLATE_QTY } else { ERR_FACADE_TEMPLATE_NAME });
            return Some(tgf::ScFacadeSaveSkinTemplateRsp { skin_template: None });
        }
        let owned = self.player_facade_owned(&p);
        let now = Self::now_ts();
        let tpl = tgf::DUserFacadeSkinGroup {
            id: format!("{}_{}_{}", p.user_id, now, list.len() + 1),
            created_time: now,
            suit_type: SUIT_TYPE_TEMPLATE,
            name,
            suit_id: 0,
            item_ids: req.item_ids.iter().copied().filter(|id| owned.contains(id) && self.player_facade_type(*id) == FACADE_SKIN).collect(),
            hidden_helmet: req.hidden_helmet,
            expire_timepoint: 0,
        };
        list.push(tpl.clone());
        save_templates(&mut p, list);
        self.save_me(&p).await;
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_facade_data: Some(tgf::DUserFacadeDirtyData { is_dirty: true, update_group_skins: vec![tpl.clone()], ..Default::default() }),
            ..Default::default()
        });
        Some(tgf::ScFacadeSaveSkinTemplateRsp { skin_template: Some(tpl) })
    }

    pub(super) async fn facade_delete_skin_template_impl(&self, req: tgf::CsFacadeDeleteSkinTemplateReq) -> Option<tgf::ScFacadeDeleteSkinTemplateRsp> {
        let mut p = self.load_me().await;
        let mut list = load_templates(&p);
        let before = list.len();
        list.retain(|t| t.id != req.id);
        if list.len() == before {
            println!("[Facade] delete_template {}: not found", req.id);
            self.set_rpc_error(ERR_FACADE_TEMPLATE_NOT_FOUND);
            return Some(tgf::ScFacadeDeleteSkinTemplateRsp { id: String::new() });
        }
        save_templates(&mut p, list);
        self.save_me(&p).await;
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_facade_data: Some(tgf::DUserFacadeDirtyData { is_dirty: true, delete_group_id: vec![req.id.clone()], ..Default::default() }),
            ..Default::default()
        });
        Some(tgf::ScFacadeDeleteSkinTemplateRsp { id: req.id })
    }

    pub(super) async fn facade_get_on_sale_facade_impl(&self, req: tgf::CsFacadeGetOnSaleFacadeReq) -> Option<tgf::ScFacadeGetOnSaleFacadeRsp> {
        // Everything with on_sales = true from facade.json, by type and priority. The table is static — the version is a constant.
        let mut rsp = tgf::ScFacadeGetOnSaleFacadeRsp { version: ON_SALE_VERSION, ..Default::default() };
        if req.last_version == ON_SALE_VERSION {
            // TODO(verify): with a matching version the client probably keeps its cache — we return the full list anyway
        }
        let mut rows: Vec<_> = self.game_data.facade.values().filter(|f| f.on_sales).collect();
        rows.sort_by_key(|f| (f.r#type, f.priority, f.id));
        for f in rows {
            match f.r#type {
                FACADE_AVATAR => rsp.avatars.push(f.id),
                FACADE_AVATAR_BORDER => rsp.avatar_borders.push(f.id),
                FACADE_TITLE => rsp.titles.push(f.id),
                FACADE_SKIN => rsp.skins.push(f.id),
                _ => {}
            }
        }
        Some(rsp)
    }
}

/// Add an appearance to the bought/granted ones (state.player_facade_owned). true — new.
pub(super) fn facade_own(p: &mut PlayerModel, id: i32) -> bool {
    let mut owned: Vec<i64> = p.state["player_facade_owned"].as_array().map(|a| a.iter().filter_map(|v| v.as_i64()).collect()).unwrap_or_default();
    if owned.contains(&(id as i64)) {
        return false;
    }
    owned.push(id as i64);
    p.state["player_facade_owned"] = json!(owned);
    true
}
