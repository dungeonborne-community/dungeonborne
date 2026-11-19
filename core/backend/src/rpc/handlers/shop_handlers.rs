//! NPC shop (market_*), activity center (activity_center_*), Mithril force (mithril_*).
//! Handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Shop — PVE SoloLobby format (rpc_dump/PVE_lobby_0630, player_get_info.user_market_tabs):
//! - merchant tabs = market_info 101 with products (the empty 4444 is not sent); product DUserMarketTabAvailableProduct:
//!   available_count = total_available_count = refresh_count, cost_options from the config, last/next as for the tab,
//!   identified → identified_items: a ready DItem (player guid, affixes) — exactly what the buyer gets.
//! - a tab where all products have a buy_start/end window and none is active (1009/1010/1011) — window rotation
//!   by day: window # ((now − tab.start_timepoint) / 86400) % window_count (verified: on day 1273 PVE 1009 → window 13
//!   of 20, 1010/1011 → 1 of 4; the old emulator's "period 1266" → 6 and 2). Other products with a window — only inside the window.
//! - products with refresh_pool_id (1014/1015) — min(tab.random_count, pool.market_location_count) random by random_weight.
//! - last = start of the day (rotation) or of the period, next = the next period boundary from start_timepoint (as in PVE).
//!   The tab is rebuilt when the day (rotation) or the period changes.
//! State: market_tabs[tab] = {gen, data}; market_sold (buyback, DUserMarketSoldItem), market_sold_ver;
//! market_popup_videos[id] = time; market_play_refresh_effect.
//!
//! Place grid: x — by the place's row_size (stash 17, backpack 7, small 2), y — by column_size (10/10/5); an item occupies
//! position_cfg.row_size in x and column_size in y (UeSource_ItemType; verified on the PVE stash — 56 items with no overlaps).
//! New/changed items — dirty_postion_action (full item), removed ones — a "basic only with the old place" entry
//! (as inventory_handlers).
//!
//! Shop and activity responses have no ret_code: rejection = empty response without changes and pushes + set_rpc_error(code)
//! (RpcHead.error_code), the reason — to the log.
//!
//! Client Lua (tools/research/shop/lua_orig, BattlePassRewardService): passport reward — RewardId = basic_rewards_id
//! (free) or high_rewards_id (premium), ReceivedRewardsId — a set of these ids; daily quests are looked up by
//! PassportData.DailyQuests.QuestId, progress/status — from TaskService (quest domain).
//!
//! Activities — the official server format (sniffer) and PVE: activity_center_list returns only passport 16518
//! (start 1733097600, end/expired 2111702400 — extended by the server, 2026-12-01 in the config), join_time, daily_join_times 1,
//! next_daily_reset_time (00:00 UTC), next_weekly_reset_time (Monday 00:00 UTC), passport_data: 3 daily quests
//! from quest_list.random_pool_id = daily_quest_cfg.pool_id, status PROCESSING, idx 1..3. State: activity[id] = DUserActivityData.

use super::common;
use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use crate::tgf::d_user_activity_data::CustomData;
use serde_json::json;
use std::collections::{BTreeMap, HashMap, HashSet};

const STASH: i32 = 1;
const BACKPACK: i32 = 2;
const WEARING: i32 = 3;
const SMALL_BACKPACK: i32 = 4;
const STASH_NO_XY: i32 = 101;
const BACKPACK_NO_XY: i32 = 102;

const MERCHANT_MARKET: i32 = 101;
/// "Insurance Premium" by rarity (item_type 2100010–2100017, = ids of gear_insurance_item rows)
const INSURANCE_PREMIUM_FIRST: i32 = 2100010;
const INSURANCE_PREMIUM_LAST: i32 = 2100017;

/// Price of the "Insurance Premium" from the owner's payment, as on the battle server (0x142ec4130): payment × (1000 − ratio) / 1000, rounded down
fn insurance_premium_sale(paid: i64, ratio: i64) -> i64 {
    paid.max(0) * (1000 - ratio.clamp(0, 1000)) / 1000
}
const DAY: i64 = 86_400;
/// how many recent sales to keep for buyback
const SOLD_KEEP: usize = 20;
const MAJOR_CURRENCY: i32 = 1;
const MAJOR_EQUIPMENT: i32 = 4;

// error codes (schema_dump.proto EnErrorCode) — into RpcHead.error_code via set_rpc_error
const ERR_TAB_NOT_FOUND: i32 = -1501;
const ERR_PRODUCT_NOT_FOUND: i32 = -1502;
const ERR_SELL_OUT: i32 = -1514;
const ERR_STOCK: i32 = -1515;
const ERR_NOT_IN_AVAILABLE: i32 = -1516;
const ERR_BILL_NO: i32 = -1523;
const ERR_SOLD_NOT_FOUND: i32 = -1543;
const ERR_VIDEO_NOT_FOUND: i32 = -1557;
const ERR_COIN: i32 = -681; // EN_ERR_VIRTUAL_ITEM_COIN_NOT_ENOUGH
const ERR_ITEM_MAX: i32 = -607;
const ERR_ITEM_GUID: i32 = -608;
const ERR_NO_PLACE: i32 = -611; // EN_ERR_ITEM_POSITION_NOT_FOUND
const ERR_PAGE_LIMIT: i32 = tgf::EnErrorCode::EnErrItemInventoryAddPageFailedReachPageLimit as i32; // ext: stash_pages
const ERR_ACT_PAID: i32 = -4703;
const ERR_ACT_EXCEL: i32 = -4704;
const ERR_ACT_REPEATED: i32 = -4705;
const ERR_ACT_INVALID_REWARDS: i32 = -4706;
const ERR_ACT_NOT_SUPPORT: i32 = -4708;
const ERR_ACT_NOT_FOUND: i32 = -4709;
const ERR_ACT_QUEST_NOT_IN_DAILY: i32 = -4715;
const ERR_ACT_QUEST_REFRESH: i32 = -4716;
const ERR_ACT_REWARD_COND: i32 = -4717;
const ERR_ACT_REWARD_NOT_MATCH: i32 = -4718;
const ERR_ACT_TYPE: i32 = -4719;
const ERR_MITHRIL_ITEM_INVALID: i32 = -5403;

/// Rejection: code for RpcHead.error_code + reason for the log
struct Fail {
    code: i32,
    msg: String,
}
impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.msg, self.code)
    }
}
fn fail(code: i32, msg: impl Into<String>) -> Fail {
    Fail { code, msg: msg.into() }
}
trait OrFail<T> {
    fn or_fail(self, code: i32, msg: &str) -> Result<T, Fail>;
}
impl<T> OrFail<T> for Option<T> {
    fn or_fail(self, code: i32, msg: &str) -> Result<T, Fail> {
        self.ok_or_else(|| fail(code, msg))
    }
}

const QUEST_PROCESSING: i32 = 1;
const ACT_RANDOM_ITEM: i32 = 3;
const ACT_STEAM: i32 = 8;
const ACT_PASSPORT: i32 = 9;
/// "Trial of Clotho" (16600), docs/CLOTHO_TRIAL_PLAN.md
const ACT_TRIAL: i32 = 10;
const ERR_TRIAL_RULE: i32 = tgf::EnErrorCode::EnErrActivityCenterInvalidTrialRule as i32;
const ERR_TRIAL_ALREADY_OPEN: i32 = tgf::EnErrorCode::EnErrActivityCenterInvalidTrialAlreadyOpen as i32;
const ERR_TRIAL_GRADE: i32 = tgf::EnErrorCode::EnErrActivityCenterInvalidTrialGrade as i32;
const ERR_TRIAL_ALREADY_END: i32 = tgf::EnErrorCode::EnErrActivityCenterInvalidTrialAlreadyEnd as i32;
/// the passport the official server returns, and its dates from there (end 2026-12-01 in the config)
const OFFICIAL_PASSPORT: i32 = 16518;
const OFFICIAL_PASSPORT_TIMES: (i64, i64, i64) = (1_733_097_600, 2_111_702_400, 2_111_702_400);
const FORCE_MITHRIL: i32 = 2;

// ---------- small helpers ----------

fn secs(t: &Option<crate::google::protobuf::Timestamp>) -> i64 {
    t.as_ref().map(|t| t.seconds).unwrap_or(0)
}

/// Next 00:00 UTC and Monday 00:00 UTC (1970-01-01 is a Thursday)
fn next_day(now: i64) -> i64 {
    (now.div_euclid(DAY) + 1) * DAY
}
fn next_monday(now: i64) -> i64 {
    let d = now.div_euclid(DAY);
    (d - (d + 3).rem_euclid(7) + 7) * DAY
}

/// splitmix64; seed — time (no rand crate)
struct Rng(u64);
impl Rng {
    fn new() -> Self {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        Rng(n ^ 0xD1B5_4A32_D192_ED03)
    }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }
}

/// Weighted choice of `count` indices without repeats (weight ≤ 0 counts as 1)
fn pick_weighted(weights: &[i64], count: usize, rng: &mut Rng) -> Vec<usize> {
    let mut left: Vec<usize> = (0..weights.len()).collect();
    let mut out = vec![];
    while out.len() < count && !left.is_empty() {
        let total: i64 = left.iter().map(|&i| weights[i].max(1)).sum();
        let mut r = rng.below(total as u64) as i64;
        let pos = left
            .iter()
            .position(|&i| {
                let w = weights[i].max(1);
                if r < w { true } else { r -= w; false }
            })
            .unwrap_or(0);
        out.push(left.remove(pos));
    }
    out
}

fn guid_of(item: &tgf::DItem) -> i64 {
    item.basic.as_ref().map(|b| b.guid).unwrap_or(0)
}
fn pos_of(item: &tgf::DItem) -> tgf::DItemPosition {
    item.basic.as_ref().and_then(|b| b.position.clone()).unwrap_or_default()
}

/// dirty_postion_action entry "item removed from its place" (as inventory_handlers)
fn removal(item: &tgf::DItem) -> tgf::DItem {
    let b = item.basic.clone().unwrap_or_default();
    tgf::DItem {
        basic: Some(tgf::DItemBasic { type_id: b.type_id, guid: b.guid, position: b.position, ..Default::default() }),
        ..Default::default()
    }
}

fn alloc_guid(_p: &mut PlayerModel) -> i64 {
    crate::guid::next_item()
}

fn coins_dirty(p: &PlayerModel, ids: &[i32]) -> Vec<tgf::DCoin> {
    let mut seen = HashSet::new();
    ids.iter().filter(|id| seen.insert(**id)).map(|id| common::coin_dirty(p, *id)).collect()
}

/// Items of a place and its grid size (by x, by y). Worn — no grid (0, 0).
fn place_list(p: &mut PlayerModel, t: i32, guid: i64) -> Option<(&mut Vec<tgf::DItem>, i32, i32)> {
    if t == STASH {
        return Some((&mut p.stash.element, 17, 10));
    }
    let role = p.roles.iter_mut().find(|r| r.role_guid as i64 == guid)?;
    let d = role.data.deploy_role.get_or_insert_with(Default::default);
    match t {
        BACKPACK => {
            let b = d.backpack.get_or_insert_with(|| tgf::DRoleBackpackData { items: vec![], row_size: 7, column_size: 10 });
            let (w, h) = (b.row_size.max(1), b.column_size.max(1));
            Some((&mut b.items, w, h))
        }
        SMALL_BACKPACK => {
            let b = d.small_backpack.get_or_insert_with(|| tgf::DRoleBackpackData { items: vec![], row_size: 2, column_size: 5 });
            let (w, h) = (b.row_size.max(1), b.column_size.max(1));
            Some((&mut b.items, w, h))
        }
        WEARING => Some((&mut d.wearing.get_or_insert_with(Default::default).items, 0, 0)),
        _ => None,
    }
}

/// Take an item by guid from anywhere (stash, bags, worn)
fn take_item(p: &mut PlayerModel, guid: i64) -> Option<tgf::DItem> {
    let mut places = vec![(STASH, 0)];
    for r in &p.roles {
        for t in [BACKPACK, SMALL_BACKPACK, WEARING] {
            places.push((t, r.role_guid as i64));
        }
    }
    for (t, g) in places {
        if let Some((list, _, _)) = place_list(p, t, g) {
            if let Some(i) = list.iter().position(|it| guid_of(it) == guid) {
                return Some(list.remove(i));
            }
        }
    }
    None
}

// ---------- items: shape, creation, placement ----------

impl GatewaySession {
    /// Item shape: (by x, by y, stack limit) from UeSource_ItemType.position_cfg; no entry — 1×1, no stacking
    fn shop_item_shape(&self, type_id: i32) -> (i32, i32, i32) {
        let ue = self.game_data.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|v| *v > 0).unwrap_or(type_id);
        let pc = self.game_data.uesource_itemtype.get(&ue).and_then(|u| u.position_cfg.clone()).unwrap_or_default();
        (pc.row_size.max(1), pc.column_size.max(1), pc.accumulation_limit.max(1))
    }

    fn shop_is_currency(&self, type_id: i32) -> bool {
        self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == MAJOR_CURRENCY).unwrap_or(false)
    }

    /// A new player item: own guid, gear gets affixes (random roll, like build_item_affix_data_seeded)
    fn shop_new_item(&self, p: &mut PlayerModel, type_id: i32, count: i32, rng: &mut Rng) -> tgf::DItem {
        let guid = alloc_guid(p);
        let equip = self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == MAJOR_EQUIPMENT).unwrap_or(false);
        let data = if equip {
            // the template needs wear_part only when there is no affix_template: weapon (181xxxxxx) → slot 1
            let wear_part = if type_id / 1_000_000 == 181 { 1 } else { 0 };
            tgf::DItemData { affix_data: self.build_item_affix_data_seeded(type_id, wear_part, rng.next() | 1), ..Default::default() }
        } else {
            tgf::DItemData::default()
        };
        tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id, count: count.max(1), guid, ..Default::default() }),
            data: Some(data),
            create_info: None,
        }
    }

    /// A free cell for an item of shape `shape` in place (t, guid); `want` — check it first
    fn shop_free_cell(&self, p: &mut PlayerModel, t: i32, guid: i64, shape: (i32, i32), want: Option<(i32, i32)>) -> Option<(i32, i32)> {
        let shapes: Vec<(tgf::DItemPosition, i32)> = {
            let (list, _, _) = place_list(p, t, guid)?;
            list.iter().map(|it| (pos_of(it), it.basic.as_ref().map(|b| b.type_id).unwrap_or(0))).collect()
        };
        let (_, gw, gh) = place_list(p, t, guid)?;
        if gw == 0 || shape.0 > gw || shape.1 > gh {
            return None;
        }
        let mut occ = vec![false; (gw * gh) as usize];
        for (pos, type_id) in shapes {
            if t == STASH && pos.guid != guid {
                continue;
            }
            let (w, h, _) = self.shop_item_shape(type_id);
            for x in pos.x.max(0)..(pos.x + w).min(gw) {
                for y in pos.y.max(0)..(pos.y + h).min(gh) {
                    occ[(x * gh + y) as usize] = true;
                }
            }
        }
        let fits = |x: i32, y: i32| {
            x >= 0 && y >= 0 && x + shape.0 <= gw && y + shape.1 <= gh && (x..x + shape.0).all(|i| (y..y + shape.1).all(|j| !occ[(i * gh + j) as usize]))
        };
        if let Some((x, y)) = want.filter(|(x, y)| fits(*x, *y)) {
            return Some((x, y));
        }
        (0..gw).flat_map(|x| (0..gh).map(move |y| (x, y))).find(|(x, y)| fits(*x, *y))
    }

    /// Where to put on client request: (type, guid, exact cell). Backpack → then small backpack; stash is always last.
    fn shop_places(dest: &tgf::DItemPosition, pages: i64) -> Vec<(i32, i64, Option<(i32, i32)>)> {
        let xy = Some((dest.x, dest.y));
        let mut out = match dest.r#type {
            STASH => vec![(STASH, dest.guid, xy)],
            STASH_NO_XY => vec![(STASH, dest.guid, None)],
            BACKPACK => vec![(BACKPACK, dest.guid, xy), (SMALL_BACKPACK, dest.guid, None)],
            BACKPACK_NO_XY | WEARING => vec![(BACKPACK, dest.guid, None), (SMALL_BACKPACK, dest.guid, None)],
            SMALL_BACKPACK => vec![(SMALL_BACKPACK, dest.guid, xy), (BACKPACK, dest.guid, None)],
            _ => vec![],
        };
        // ext: stash_pages — only the player's pages (was the constant 8)
        for page in 0..pages {
            if !out.iter().any(|(t, g, _)| *t == STASH && *g == page) {
                out.push((STASH, page, None));
            }
        }
        out
    }

    /// Put an item: a stack is topped up into identical incomplete ones, the remainder as new stacks into free cells.
    /// Returns full items for dirty_postion_action. Error — no space (the player may have changed: the caller rolls back).
    fn shop_put(&self, p: &mut PlayerModel, item: tgf::DItem, dest: &tgf::DItemPosition) -> Result<Vec<tgf::DItem>, Fail> {
        let basic = item.basic.clone().unwrap_or_default();
        let (w, h, stack) = self.shop_item_shape(basic.type_id);
        let places = Self::shop_places(dest, super::inventory_handlers::stash_page_count(p));
        let mut left = basic.count.max(1);
        let mut dirty = vec![];
        if stack > 1 {
            for (t, g, _) in &places {
                let Some((list, _, _)) = place_list(p, *t, *g) else { continue };
                for it in list.iter_mut() {
                    let page_ok = *t != STASH || pos_of(it).guid == *g;
                    let before = removal(it);
                    let Some(b) = it.basic.as_mut().filter(|b| page_ok && b.type_id == basic.type_id && b.count < stack) else { continue };
                    let add = left.min(stack - b.count);
                    b.count += add;
                    left -= add;
                    // stack changed in place: "removed" + full (as inventory_handlers::changed)
                    dirty.push(before);
                    dirty.push(it.clone());
                    if left == 0 {
                        return Ok(dirty);
                    }
                }
            }
        }
        let mut first = true;
        while left > 0 {
            let n = left.min(stack);
            let mut piece = item.clone();
            let guid = if first && basic.guid != 0 { basic.guid } else { alloc_guid(p) };
            first = false;
            let spot = places.iter().find_map(|(t, g, want)| self.shop_free_cell(p, *t, *g, (w, h), *want).map(|(x, y)| (*t, *g, x, y)));
            let Some((t, g, x, y)) = spot else {
                return Err(fail(ERR_NO_PLACE, format!("no space for {}×{} (type_id {})", w, h, basic.type_id)));
            };
            let b = piece.basic.get_or_insert_with(Default::default);
            b.guid = guid;
            b.count = n;
            b.position = Some(tgf::DItemPosition { r#type: t, guid: g, x, y, ..Default::default() });
            place_list(p, t, g).or_fail(ERR_NO_PLACE, "place disappeared")?.0.push(piece.clone());
            dirty.push(piece);
            left -= n;
        }
        Ok(dirty)
    }

    /// Grant a reward: currency — to the account (coins), the rest — as new items into dest (dirty)
    fn shop_grant(
        &self,
        p: &mut PlayerModel,
        type_id: i32,
        count: i32,
        dest: &tgf::DItemPosition,
        rng: &mut Rng,
        coins: &mut Vec<i32>,
        dirty: &mut Vec<tgf::DItem>,
    ) -> Result<(), Fail> {
        if count <= 0 || type_id <= 0 {
            return Ok(());
        }
        // appearance (skin, avatar, title) — unlocked, not placed as an item (push — market_buy_product_impl)
        if self.game_data.facade.contains_key(&type_id) {
            super::player_handlers::facade_own(p, type_id);
            return Ok(());
        }
        if self.shop_is_currency(type_id) {
            common::add_coin(p, type_id, count as i64).or_fail(ERR_ITEM_MAX, "currency overflow")?;
            coins.push(type_id);
            return Ok(());
        }
        let (_, _, stack) = self.shop_item_shape(type_id);
        if stack > 1 {
            // stackable: one "template", shop_put splits it into stacks itself
            let mut it = self.shop_new_item(p, type_id, 1, rng);
            it.basic.as_mut().unwrap().count = count;
            dirty.extend(self.shop_put(p, it, dest)?);
        } else {
            for _ in 0..count {
                let it = self.shop_new_item(p, type_id, 1, rng);
                dirty.extend(self.shop_put(p, it, dest)?);
            }
        }
        Ok(())
    }

    /// Activity rewards: place — from the client's items_position (by type_id), otherwise the stash
    fn shop_grant_rewards(
        &self,
        p: &mut PlayerModel,
        rewards: &[tgf::DItemOffset],
        positions: &[tgf::DItemBasic],
        rng: &mut Rng,
    ) -> Result<(Vec<tgf::DItemBasic>, Vec<i32>, Vec<tgf::DItem>), Fail> {
        let (mut coins, mut dirty) = (vec![], vec![]);
        let mut used = vec![false; positions.len()];
        for r in rewards {
            let i = positions.iter().enumerate().position(|(i, b)| !used[i] && b.type_id == r.type_id && b.position.is_some());
            let dest = match i {
                Some(i) => {
                    used[i] = true;
                    positions[i].position.clone().unwrap()
                }
                None => tgf::DItemPosition { r#type: STASH_NO_XY, ..Default::default() },
            };
            self.shop_grant(p, r.type_id, r.count, &dest, rng, &mut coins, &mut dirty)?;
        }
        let items = rewards.iter().map(|r| tgf::DItemBasic { type_id: r.type_id, count: r.count, ..Default::default() }).collect();
        Ok((items, coins, dirty))
    }
}

// ---------- shop: tabs ----------

type Product = tgf::MarketProductItem;

fn window(p: &Product) -> (i64, i64) {
    (secs(&p.buy_start_timepoint), secs(&p.buy_end_timepoint))
}
fn on_window(p: &Product, now: i64) -> bool {
    let (s, e) = window(p);
    (s == 0 || now >= s) && (e == 0 || now < e)
}
/// Tab config signature: product ids and their prices (cost_options) — changes after balance build
fn tab_signature(products: &[&Product]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    let mut ids: Vec<i32> = products.iter().map(|p| p.product_id).collect();
    ids.sort_unstable();
    for p in products {
        p.product_id.hash(&mut h);
        for c in p.cost_options.iter().flat_map(|o| o.cost_items.iter()) {
            (c.cost_type, c.cost_price, c.origin_price).hash(&mut h);
        }
    }
    ids.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// All products have windows and none is running — a rotation tab (1009/1010/1011)
fn is_rotating(products: &[&Product], now: i64) -> bool {
    !products.is_empty() && products.iter().all(|p| window(p) != (0, 0)) && !products.iter().any(|p| on_window(p, now))
}

/// Tab generation, last, next, day number from start_timepoint
fn tab_cycle(cfg: &tgf::MarketTabInfo, now: i64, rotating: bool) -> (i64, i64, i64, i64) {
    let start = secs(&cfg.start_timepoint);
    let day_idx = (now - start).max(0) / DAY;
    if cfg.period <= 0 {
        return (if rotating { day_idx } else { 0 }, 0, 0, day_idx);
    }
    let pidx = (now - start).max(0) / cfg.period;
    let pstart = start + pidx * cfg.period;
    let next = pstart + cfg.period;
    if cfg.period >= DAY {
        // as in PVE: last — start of the day, next — period boundary
        (if rotating { day_idx } else { pidx }, start + day_idx * DAY, next, day_idx)
    } else {
        (pidx, pstart, next, day_idx)
    }
}

impl GatewaySession {
    fn market_tab_cfg(&self, tab: i32) -> Option<&tgf::MarketTabInfo> {
        self.game_data.market_tab_info.iter().find(|t| t.market_tab_id == tab).and_then(|t| t.data.as_ref())
    }

    fn market_product(&self, id: i32) -> Option<&Product> {
        self.game_data.market_product_item.iter().find(|r| r.product_id == id).and_then(|r| r.product.as_ref())
    }

    /// Tab products on sale (is_disabled_auto_refresh — the "pauper" discount 710001, PVE does not have it in the list)
    fn market_tab_products(&self, tab: i32) -> Vec<&Product> {
        self.game_data
            .market_product_item
            .iter()
            .filter(|r| r.market_tab_id == tab)
            .filter_map(|r| r.product.as_ref())
            .filter(|p| p.is_on_sale && !p.is_disabled_auto_refresh)
            .collect()
    }

    /// Merchant tabs (market_info 101) that have products — like user_market_tabs in PVE
    fn market_merchant_tabs(&self) -> Vec<i32> {
        let Some(m) = self.game_data.market_info.iter().find(|m| m.market_id == MERCHANT_MARKET).and_then(|m| m.data.as_ref()) else {
            return vec![];
        };
        m.market_tab_ids.iter().chain(&m.shared_market_tab_ids).copied().filter(|t| !self.market_tab_products(*t).is_empty()).collect()
    }

    /// Product price for the `used`-th purchase: is_special_cost — the market_special_cost ladder (sort_idx = used + 1).
    /// Discount (cost_price < origin_price) — only inside the discount_start/end_timepoint window: outside it the price = origin_price,
    /// as the client shows (03.11: 40 stones shown, 36 charged — the 2024 discount ended long ago).
    fn market_cost(&self, prod: &Product, used: i32) -> Vec<tgf::MarketNormalOptionCost> {
        if prod.is_special_cost {
            let rows: Vec<_> = self.game_data.market_special_cost.iter().filter(|r| r.product_id == prod.product_id).collect();
            let row = rows.iter().find(|r| r.sort_idx == used + 1).or_else(|| rows.iter().max_by_key(|r| r.sort_idx));
            if let Some(d) = row.and_then(|r| r.data.as_ref()) {
                return d.cost_options.clone();
            }
        }
        let mut options = prod.cost_options.clone();
        let (start, end) = (secs(&prod.discount_start_timepoint), secs(&prod.discount_end_timepoint));
        let now = Self::now_ts();
        let discounted = (start > 0 || end > 0) && now >= start && (end == 0 || now < end);
        if (start > 0 || end > 0) && !discounted {
            for c in options.iter_mut().flat_map(|o| o.cost_items.iter_mut()) {
                if c.origin_price > 0 {
                    c.cost_price = c.origin_price;
                }
            }
        }
        options
    }

    /// Tab contents for the current generation
    fn market_pick_products<'a>(&self, cfg: &tgf::MarketTabInfo, products: &[&'a Product], now: i64, day_idx: i64, rotating: bool, rng: &mut Rng) -> Vec<&'a Product> {
        if rotating {
            let mut starts: Vec<i64> = products.iter().map(|p| window(p).0).collect();
            starts.sort_unstable();
            starts.dedup();
            let pick = starts[(day_idx as usize) % starts.len()];
            return products.iter().copied().filter(|p| window(p).0 == pick).collect();
        }
        let live: Vec<&Product> = products.iter().copied().filter(|p| on_window(p, now)).collect();
        let mut keep: HashSet<i32> = live.iter().filter(|p| p.refresh_pool_id == 0).map(|p| p.product_id).collect();
        let mut pools: BTreeMap<i32, Vec<&Product>> = BTreeMap::new();
        for p in live.iter().filter(|p| p.refresh_pool_id != 0) {
            pools.entry(p.refresh_pool_id).or_default().push(p);
        }
        for (pool_id, list) in pools {
            let loc = self.game_data.market_refresh_pool.iter().find(|r| r.market_refresh_pool_id == pool_id).map(|r| r.market_location_count).unwrap_or(0);
            let want = match (cfg.random_count, loc) {
                (0, 0) => list.len(),
                (0, l) => l as usize,
                (r, 0) => r as usize,
                (r, l) => r.min(l) as usize,
            };
            let weights: Vec<i64> = list.iter().map(|p| p.random_weight as i64).collect();
            keep.extend(pick_weighted(&weights, want, rng).into_iter().map(|i| list[i].product_id));
        }
        live.into_iter().filter(|p| keep.contains(&p.product_id)).collect()
    }

    /// Player's tab: from state while the generation is unchanged, otherwise rebuilt. bool — state changed.
    fn market_tab(&self, p: &mut PlayerModel, tab_id: i32, now: i64, rng: &mut Rng) -> Option<(tgf::DUserMarketTabData, bool)> {
        let mut cfg = self.market_tab_cfg(tab_id)?.clone();
        let products = self.market_tab_products(tab_id);
        let rotating = is_rotating(&products, now);
        // season: the merchant refreshes more often than the config (playtest 07.11 — once an hour). Only merchant tabs with a period;
        // rotation (1009–1011) then changes products every period, not once a day
        let refresh = crate::season::features().merchant_refresh_secs;
        let fast = refresh > 0 && cfg.period > 0 && self.market_merchant_tabs().contains(&tab_id);
        if fast {
            cfg.period = refresh;
        }
        let (gen, last, next, day_idx) = tab_cycle(&cfg, now, rotating);
        let day_idx = if fast { gen } else { day_idx };
        let cfg = &cfg;
        let key = tab_id.to_string();
        // tab config signature (products and prices): tabs without a period (shop 3001–3005) were otherwise never rebuilt
        // — after balance build the player kept old prices ("not enough" rejection, 03.11) and had no new products
        let sig = tab_signature(&products);
        let saved = &p.state["market_tabs"][key.as_str()];
        let old: Option<tgf::DUserMarketTabData> = serde_json::from_value(saved["data"].clone()).ok();
        // no signature (tab built before 03.11) — rebuild too: unknown which config it came from
        if saved["gen"].as_i64() == Some(gen) && saved["sig"].as_str() == Some(sig.as_str()) {
            if let Some(d) = old {
                return Some((d, false));
            }
        }
        // same generation, only the config changed — carry over the player's purchases (used/available)
        let keep_counts = if saved["gen"].as_i64() == Some(gen) { old } else { None };
        let chosen = self.market_pick_products(cfg, &products, now, day_idx, rotating, rng);
        let mut available = vec![];
        for prod in chosen {
            let n = if prod.refresh_count > 0 { prod.refresh_count } else { 999 };
            let identified_items = if prod.identified {
                prod.reward_items.iter().map(|r| self.shop_new_item(p, r.type_id, r.count, rng)).collect()
            } else {
                vec![]
            };
            let prev = keep_counts.as_ref().and_then(|d| d.available_products.iter().find(|a| a.product_id == prod.product_id));
            let used = prev.map(|a| a.used_count).unwrap_or(0);
            available.push(tgf::DUserMarketTabAvailableProduct {
                product_id: prod.product_id,
                available_count: prev.map(|a| a.available_count).unwrap_or(n),
                total_available_count: n,
                cost_options: self.market_cost(prod, used),
                used_count: used,
                next_auto_refresh_reset_time: next,
                last_auto_refresh_reset_time: last,
                identified: prod.identified,
                identified_items,
            });
        }
        let data = tgf::DUserMarketTabData {
            market_tab_id: tab_id,
            next_auto_refresh_reset_time: next,
            last_auto_refresh_reset_time: last,
            available_products: available,
            ..Default::default()
        };
        if !p.state["market_tabs"].is_object() {
            p.state["market_tabs"] = json!({});
        }
        p.state["market_tabs"][key.as_str()] = json!({ "gen": gen, "sig": sig, "data": serde_json::to_value(&data).unwrap_or_default() });
        Some((data, true))
    }

    fn market_store_tab(p: &mut PlayerModel, tab: &tgf::DUserMarketTabData) {
        let key = tab.market_tab_id.to_string();
        p.state["market_tabs"][key.as_str()]["data"] = serde_json::to_value(tab).unwrap_or_default();
    }

    /// For player_get_info (need_user_market_tabs): merchant tabs; loads and saves the player itself —
    /// call after the last save_me in player_get_info.
    #[allow(dead_code)]
    pub(super) async fn market_user_tabs(&self) -> Vec<tgf::DUserMarketTabData> {
        let mut p = self.load_me().await;
        let (now, mut rng, mut changed) = (Self::now_ts(), Rng::new(), false);
        let tabs = self
            .market_merchant_tabs()
            .into_iter()
            .chain([super::stash_pages::PAGE_TAB]) // ext: stash_pages — the page product's tab (market 103)
            .filter_map(|id| self.market_tab(&mut p, id, now, &mut rng))
            .map(|(d, c)| {
                changed |= c;
                d
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|mut d| {
                self.stash_tab_adjust(&p, &mut d); // ext: stash_pages
                d
            })
            .collect();
        if changed {
            self.save_me(&p).await;
        }
        tabs
    }
}

/// For player_get_info (need_user_sold_items): buyback list
#[allow(dead_code)]
pub(super) fn market_sold_list(p: &PlayerModel) -> tgf::DUserMarketSoldList {
    tgf::DUserMarketSoldList { sold_items: serde_json::from_value(p.state["market_sold"].clone()).unwrap_or_default() }
}

/// For player_get_info (need_market_video_popup): watched videos
#[allow(dead_code)]
pub(super) fn market_popup_list(p: &PlayerModel) -> Vec<tgf::DVideoPopupData> {
    p.state["market_popup_videos"]
        .as_object()
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some(tgf::DVideoPopupData { popup_video_id: k.parse().ok()?, time_point: v.as_i64().unwrap_or(0) }))
                .collect()
        })
        .unwrap_or_default()
}

fn set_sold_list(p: &mut PlayerModel, list: &tgf::DUserMarketSoldList) {
    p.state["market_sold"] = serde_json::to_value(&list.sold_items).unwrap_or_default();
}

// ---------- shop: buying / selling ----------

impl GatewaySession {
    fn market_buy(&self, p: &mut PlayerModel, req: &tgf::CsMarketBuyProductReq, rng: &mut Rng) -> Result<tgf::ScPlayerDirtyChgSync, Fail> {
        if !req.bill_no.is_empty() || req.market_type == 255 {
            // TODO(verify): purchases for real money (midas / direct buy) — not supported
            return Err(fail(ERR_BILL_NO, "purchases for money are not supported"));
        }
        let cnt = req.product_cnt.max(1);
        let prod = self.market_product(req.product_id).or_fail(ERR_PRODUCT_NOT_FOUND, "product not in market_product_item")?;
        let now = Self::now_ts();
        let (mut tab, _) = self.market_tab(p, prod.market_tab_id, now, rng).or_fail(ERR_TAB_NOT_FOUND, "no tab")?;
        self.stash_tab_adjust(p, &mut tab); // ext: stash_pages
        let idx = tab.available_products.iter().position(|a| a.product_id == req.product_id).or_fail(ERR_NOT_IN_AVAILABLE, "product not in the tab's current list")?;
        let ap = tab.available_products[idx].clone();
        if ap.available_count < cnt {
            return Err(fail(if ap.available_count == 0 { ERR_SELL_OUT } else { ERR_STOCK }, format!("{} left of {}", ap.available_count, cnt)));
        }
        // payment option: by the client_cost currency type (SendMarketBuyProductWithCostReq), otherwise by index
        let want_type = req.client_cost.as_ref().map(|c| c.type_id).unwrap_or(0);
        let option = ap
            .cost_options
            .iter()
            .find(|o| want_type != 0 && o.cost_items.iter().any(|c| c.cost_type == want_type))
            .or_else(|| ap.cost_options.get(req.optional_costs_index.max(0) as usize))
            .or_else(|| ap.cost_options.first())
            .cloned()
            .unwrap_or_default();

        let backup = p.clone();
        let mut pages = vec![]; // ext: stash_pages
        let res = (|| -> Result<(Vec<i32>, Vec<tgf::DItem>), Fail> {
            let (mut coins, mut dirty) = (vec![], vec![]);
            for c in &option.cost_items {
                let total = c.cost_price as i64 * cnt as i64;
                if total > 0 {
                    common::add_coin(p, c.cost_type, -total).ok_or_else(|| fail(ERR_COIN, format!("not enough currency {} ({} needed)", c.cost_type, total)))?;
                    coins.push(c.cost_type);
                }
            }
            let dest = tgf::DItemPosition { r#type: req.item_storage_type, guid: req.item_storage_guid, ..Default::default() };
            for k in 0..cnt {
                if ap.identified && !ap.identified_items.is_empty() {
                    for it in &ap.identified_items {
                        let mut it = it.clone();
                        // the first purchase gets the shown guid, further copies — new ones
                        if ap.used_count + k > 0 {
                            it.basic.get_or_insert_with(Default::default).guid = alloc_guid(p);
                        }
                        if self.shop_is_currency(it.basic.as_ref().map(|b| b.type_id).unwrap_or(0)) {
                            continue;
                        }
                        dirty.extend(self.shop_put(p, it, &dest)?);
                    }
                } else {
                    for r in &prod.reward_items {
                        // ext: stash_pages — an auto-used page box: the pages are added at once, the item is not placed
                        if let Some(k) = self.stash_auto_pages(r.type_id) {
                            pages.extend(self.stash_add_pages(p, k * r.count.max(1) as i64).map_err(|e| fail(ERR_PAGE_LIMIT, e))?);
                            continue;
                        }
                        self.shop_grant(p, r.type_id, r.count, &dest, rng, &mut coins, &mut dirty)?;
                    }
                }
            }
            // TODO(verify): auto_use_reward_items (704014 → 100023) — the item is applied immediately; currently placed as is
            Ok((coins, dirty))
        })();
        let (coins, dirty) = match res {
            Ok(v) => v,
            Err(e) => {
                *p = backup;
                return Err(e);
            }
        };
        // 18.11 (owner's item 8, arrows by quantity): N purchases topped up the same stack N times — the push carried every
        // intermediate step of one guid and the client lost the stack (as with moves, FIX_LIST 08.11). One "removed" + one final per guid
        let mut backup = backup;
        let dirty = super::inventory_handlers::compact_dirty(&mut backup, p, dirty);
        let a = &mut tab.available_products[idx];
        a.available_count -= cnt;
        a.used_count += cnt;
        if prod.is_special_cost {
            a.cost_options = self.market_cost(prod, a.used_count);
        }
        if a.available_count == 0 && !tab.already_buy_products.contains(&req.product_id) {
            tab.already_buy_products.push(req.product_id);
        }
        self.stash_tab_adjust(p, &mut tab); // ext: stash_pages
        Self::market_store_tab(p, &tab);
        Ok(tgf::ScPlayerDirtyChgSync {
            dirty_inventory_page: pages, // ext: stash_pages
            dirty_postion_action: dirty,
            dirty_coin_data: coins_dirty(p, &coins),
            dirty_market_tabs: vec![tab],
            ..Default::default()
        })
    }

    fn market_sell(&self, p: &mut PlayerModel, basics: &[tgf::DItemBasic], premiums: &HashMap<i64, i64>) -> tgf::ScPlayerDirtyChgSync {
        let now = Self::now_ts();
        let mut sold = market_sold_list(p);
        let mut ver = p.state["market_sold_ver"].as_i64().unwrap_or(0);
        let (mut gold, mut dirty) = (0i64, vec![]);
        for want in basics {
            let Some(mut item) = take_item(p, want.guid) else {
                println!("[Market] sale: no item guid={}", want.guid);
                continue;
            };
            let b = item.basic.clone().unwrap_or_default();
            // "Insurance Premium" (08.11): price — the insurance paid by the owner; cannot be bought back
            if let Some(&premium) = premiums.get(&b.guid) {
                let have = b.count.max(1);
                dirty.push(removal(&item));
                gold += premium * have as i64;
                println!("[Insurance] {}: premium guid={} sold for {}", p.open_id, b.guid, premium * have as i64);
                continue;
            }
            let crafted = self.is_crafted(b.type_id);
            let mut price = self.merchant_sell_price(b.type_id);
            if price <= 0 && !crafted {
                price = item.data.as_ref().map(|d| d.sale_price as i64).unwrap_or(0);
            }
            if crafted {
                println!("[Market] sale: type_id {} is crafted — for 0", b.type_id);
            } else if price <= 0 {
                // no price (grey starter weapon, bolts: system_repurchase_price 0) — sold for 0. Previously
                // it was kept and answered -608 "invalid guid": the client showed an error and the item did not go (05.11)
                println!("[Market] sale: type_id {} has no price — for 0", b.type_id);
                price = 0;
            }
            let have = b.count.max(1);
            let n = if want.count > 0 { want.count.min(have) } else { have };
            let mut sold_item = item.clone();
            if n < have {
                // part of a stack: the remainder stays in place, the sold part — with a new guid
                item.basic.as_mut().unwrap().count = have - n;
                dirty.push(removal(&item));
                dirty.push(item.clone());
                let pos = pos_of(&item);
                place_list(p, pos.r#type, pos.guid).map(|(l, _, _)| l.push(item));
                sold_item.basic.as_mut().unwrap().guid = alloc_guid(p);
            } else {
                dirty.push(removal(&item));
            }
            let sb = sold_item.basic.as_mut().unwrap();
            sb.count = n;
            sb.position = None;
            gold += price * n as i64;
            ver += 1;
            sold.sold_items.push(tgf::DUserMarketSoldItem { item_data: Some(sold_item), last_sold_timepoint: now, sold_version: ver });
        }
        let skip = sold.sold_items.len().saturating_sub(SOLD_KEEP);
        sold.sold_items.drain(..skip);
        common::add_coin(p, common::GOLD, gold);
        p.state["market_sold_ver"] = json!(ver);
        set_sold_list(p, &sold);
        tgf::ScPlayerDirtyChgSync {
            dirty_postion_action: dirty,
            dirty_coin_data: coins_dirty(p, &[common::GOLD]),
            dirty_sold_items_list: Some(sold),
            ..Default::default()
        }
    }

    fn market_buy_back(&self, p: &mut PlayerModel, req: &tgf::CsMarketBuySoldItemReq) -> Result<tgf::ScPlayerDirtyChgSync, Fail> {
        let mut sold = market_sold_list(p);
        let idx = sold
            .sold_items
            .iter()
            .position(|s| (req.sold_version != 0 && s.sold_version == req.sold_version) || (req.sold_version == 0 && s.item_data.as_ref().map(guid_of) == Some(req.item_guid)))
            .or_fail(ERR_SOLD_NOT_FOUND, "no such item in the buyback list")?;
        let mut item = sold.sold_items[idx].item_data.clone().unwrap_or_default();
        let have = item.basic.as_ref().map(|b| b.count.max(1)).unwrap_or(1);
        let n = if req.count > 0 { req.count.min(have) } else { have };
        let type_id = item.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
        // TODO(verify): buyback price = sale price (as sold)
        let mut price = self.merchant_sell_price(type_id);
        if price <= 0 && !self.is_crafted(type_id) {
            price = item.data.as_ref().map(|d| d.sale_price as i64).unwrap_or(0);
        }
        let backup = p.clone();
        let res = (|| -> Result<Vec<tgf::DItem>, Fail> {
            common::add_coin(p, common::GOLD, -(price * n as i64)).or_fail(ERR_COIN, "not enough gold")?;
            let mut piece = item.clone();
            let b = piece.basic.get_or_insert_with(Default::default);
            b.count = n;
            if n < have {
                b.guid = alloc_guid(p);
            }
            let dest = tgf::DItemPosition { r#type: req.item_storage_type, guid: req.item_storage_guid, ..Default::default() };
            self.shop_put(p, piece, &dest)
        })();
        let dirty = match res {
            Ok(d) => d,
            Err(e) => {
                *p = backup;
                return Err(e);
            }
        };
        if n < have {
            item.basic.as_mut().unwrap().count = have - n;
            sold.sold_items[idx].item_data = Some(item);
        } else {
            sold.sold_items.remove(idx);
        }
        set_sold_list(p, &sold);
        Ok(tgf::ScPlayerDirtyChgSync {
            dirty_postion_action: dirty,
            dirty_coin_data: coins_dirty(p, &[common::GOLD]),
            dirty_sold_items_list: Some(sold),
            ..Default::default()
        })
    }
}

// ---------- activities ----------

impl GatewaySession {
    fn act_cfg(&self, id: i32) -> Option<&tgf::config::ExcelActivityCenter> {
        self.game_data.activity_center.get(&id)
    }

    fn act_times(cfg: &tgf::config::ExcelActivityCenter) -> (i64, i64, i64) {
        if cfg.id == OFFICIAL_PASSPORT {
            return OFFICIAL_PASSPORT_TIMES;
        }
        let end = secs(&cfg.end_time);
        let exp = secs(&cfg.expired_time);
        (secs(&cfg.start_time), end, if exp > 0 { exp } else { end })
    }

    /// Whether an activity is visible: on, not hidden, not Steam achievements, running (until expired)
    fn act_visible(cfg: &tgf::config::ExcelActivityCenter, now: i64) -> bool {
        let (s, _, x) = Self::act_times(cfg);
        cfg.on && !cfg.client_hidden && cfg.r#type != ACT_STEAM && now >= s && (x == 0 || now < x)
    }

    /// Passport daily quests: `n` random from the pool (quest_list.random_pool_id), except `exclude`
    fn act_roll_quests(&self, cfg: &tgf::config::ExcelActivityCenter, n: usize, exclude: &[i32], rng: &mut Rng) -> Vec<i32> {
        let pool = cfg.daily_quest_cfg.as_ref().map(|c| c.pool_id).unwrap_or(cfg.id);
        let mut cands: Vec<_> = self
            .game_data
            .quest_list
            .values()
            .filter(|q| q.random_pool_id == pool && (q.activity_id == cfg.id || q.activity_id == 0) && !exclude.contains(&q.id))
            .map(|q| (q.id, q.random_pool_weight as i64))
            .collect();
        cands.sort_unstable();
        let weights: Vec<i64> = cands.iter().map(|c| c.1).collect();
        pick_weighted(&weights, n, rng).into_iter().map(|i| cands[i].0).collect()
    }

    /// Player activity data (state["activity"][id]), with a day reset. bool — changed.
    fn act_data(&self, p: &mut PlayerModel, cfg: &tgf::config::ExcelActivityCenter, now: i64, rng: &mut Rng) -> (tgf::DUserActivityData, bool) {
        let key = cfg.id.to_string();
        let before: tgf::DUserActivityData = serde_json::from_value(p.state["activity"][key.as_str()].clone()).unwrap_or_default();
        let mut d = before.clone();
        let (s, e, x) = Self::act_times(cfg);
        if d.id == 0 {
            d.join_time = now;
        }
        d.id = cfg.id;
        d.r#type = cfg.r#type;
        d.start_time = s;
        d.end_time = e;
        d.expired_time = x;
        let new_day = now >= d.next_daily_reset_time;
        d.next_daily_reset_time = next_day(now);
        d.next_weekly_reset_time = next_monday(now);
        if new_day {
            d.daily_join_times = 1;
        }
        if cfg.r#type == ACT_PASSPORT {
            let mut pp = match d.custom_data.take() {
                Some(CustomData::PassportData(pp)) => pp,
                _ => Default::default(),
            };
            if new_day || pp.daily_quests.is_empty() {
                let n = cfg.daily_quest_cfg.as_ref().map(|c| c.init_cnt.max(c.max_cnt)).filter(|n| *n > 0).unwrap_or(3) as usize;
                pp.daily_quests = self
                    .act_roll_quests(cfg, n, &[], rng)
                    .into_iter()
                    .enumerate()
                    .map(|(i, quest_id)| tgf::DUserActivityPpDailyQuestInfo { quest_id, status: QUEST_PROCESSING, idx: i as i32 + 1 })
                    .collect();
                pp.daily_quest_idx_max = pp.daily_quests.len() as i32;
                pp.daily_quests_last_refresh_time = now;
            }
            d.custom_data = Some(CustomData::PassportData(pp));
        }
        if cfg.r#type == ACT_TRIAL {
            let mut td = match d.custom_data.take() {
                Some(CustomData::TrialData(td)) => td,
                _ => Default::default(),
            };
            td.rule_id = cfg.activity_rule;
            d.custom_data = Some(CustomData::TrialData(td));
            d.custom_data_init = true;
        }
        let changed = d != before;
        if changed {
            Self::act_store(p, &d);
        }
        (d, changed)
    }

    /// Q1: passport quests — into the quest domain (state.progress_quests): the client takes tasks and progress from user_quest
    /// (TaskService), finds the daily ones via PassportData.DailyQuests. As in PVE (player_get_info 02.11): 3 daily from the pool
    /// + all activity quests with progress_reset_period (weekly). A daily quest not in the current set is removed, daily ones
    /// are created anew at the start of the day (next_daily_reset_time), weekly ones — at the start of the period
    /// (start_day_reset_timepoint + k·period_days). true — something changed (the player must be saved).
    pub(super) fn act_sync_quests(&self, p: &mut PlayerModel) -> bool {
        let (now, mut rng, mut changed) = (Self::now_ts(), Rng::new(), false);
        let mut cfgs: Vec<_> = self.game_data.activity_center.values().filter(|c| c.r#type == ACT_PASSPORT && Self::act_visible(c, now)).collect();
        cfgs.sort_by_key(|c| c.id);
        for cfg in cfgs {
            let (d, c) = self.act_data(p, cfg, now, &mut rng);
            changed |= c;
            let daily: Vec<i32> = match &d.custom_data {
                Some(CustomData::PassportData(pp)) => pp.daily_quests.iter().map(|q| q.quest_id).collect(),
                _ => vec![],
            };
            let day_start = d.next_daily_reset_time - 86_400;
            let pool = cfg.daily_quest_cfg.as_ref().map(|c| c.pool_id).unwrap_or(cfg.id);
            let mut quests: Vec<_> = self.game_data.quest_list.values().filter(|q| q.activity_id == cfg.id && q.on).collect();
            quests.sort_by_key(|q| q.id);
            for q in quests {
                if pool != 0 && q.random_pool_id == pool {
                    if daily.contains(&q.id) {
                        changed |= self.progress_quest_drop_stale(p, q.id, day_start);
                        changed |= self.progress_quest_ensure(p, q.id);
                    } else {
                        changed |= self.progress_quest_drop(p, q.id);
                    }
                } else if let Some(r) = q.progress_reset_period.as_ref().filter(|r| r.period_days > 0) {
                    changed |= self.progress_quest_drop_stale(p, q.id, period_start(r, now));
                    changed |= self.progress_quest_ensure(p, q.id);
                }
            }
        }
        changed
    }

    fn act_store(p: &mut PlayerModel, d: &tgf::DUserActivityData) {
        if !p.state["activity"].is_object() {
            p.state["activity"] = json!({});
        }
        p.state["activity"][d.id.to_string().as_str()] = serde_json::to_value(d).unwrap_or_default();
    }

    /// All visible activities of the player; saves the player on change
    pub(super) async fn act_user_list(&self) -> tgf::DUserActivityDataList {
        let mut p = self.load_me().await;
        let (now, mut rng, mut changed) = (Self::now_ts(), Rng::new(), false);
        let mut cfgs: Vec<_> = self.game_data.activity_center.values().filter(|c| Self::act_visible(c, now)).collect();
        cfgs.sort_by_key(|c| (c.priority, c.id));
        let datas = cfgs
            .into_iter()
            .map(|cfg| {
                let (d, c) = self.act_data(&mut p, cfg, now, &mut rng);
                changed |= c;
                d
            })
            .collect();
        if changed {
            self.save_me(&p).await;
        }
        tgf::DUserActivityDataList { datas }
    }

    /// Load the player and activity `id`, apply `f`; on success — save and push dirty_activity_data (+ coins/items)
    async fn act_action<R: Default>(
        &self,
        what: &str,
        id: i32,
        f: impl FnOnce(&Self, &mut PlayerModel, &mut tgf::DUserActivityData, &tgf::config::ExcelActivityCenter, &mut Rng) -> Result<(R, Vec<i32>, Vec<tgf::DItem>), Fail>,
    ) -> R {
        let Some(cfg) = self.act_cfg(id) else {
            println!("[Activity] {} {}: not in activity_center", what, id);
            self.set_rpc_error(ERR_ACT_NOT_FOUND);
            return R::default();
        };
        let mut p = self.load_me().await;
        let (now, mut rng) = (Self::now_ts(), Rng::new());
        let (mut d, _) = self.act_data(&mut p, cfg, now, &mut rng);
        // on rejection the player is not saved — partial changes are lost with it
        let mut before = p.clone();
        match f(self, &mut p, &mut d, cfg, &mut rng) {
            Ok((rsp, coins, items)) => {
                // rewards of one stackable type top up the same stack — one entry per guid (as market_buy, 18.11)
                let items = super::inventory_handlers::compact_dirty(&mut before, &mut p, items);
                Self::act_store(&mut p, &d);
                self.save_me(&p).await;
                self.push_dirty(tgf::ScPlayerDirtyChgSync {
                    dirty_activity_data: Some(tgf::DUserActivityDataList { datas: vec![d] }),
                    dirty_coin_data: coins_dirty(&p, &coins),
                    dirty_postion_action: items,
                    ..Default::default()
                });
                rsp
            }
            Err(e) => {
                println!("[Activity] {} {}: rejected — {}", what, id, e);
                self.set_rpc_error(e.code);
                R::default()
            }
        }
    }
}

/// Start of the current quest reset period: start_day_reset_timepoint + k·period_days (≤ now)
fn period_start(r: &tgf::config::DQuestProgressResetInfo, now: i64) -> i64 {
    let start = secs(&r.start_day_reset_timepoint);
    let len = r.period_days as i64 * 86_400;
    if len <= 0 || now < start {
        return start;
    }
    start + (now - start) / len * len
}

fn trial_mut(d: &mut tgf::DUserActivityData) -> Result<&mut tgf::DUserActivityTrialData, Fail> {
    match d.custom_data.as_mut() {
        Some(CustomData::TrialData(td)) => Ok(td),
        _ => Err(fail(ERR_ACT_TYPE, "this is not a trial")),
    }
}

/// A finished run into the player's history (state.trial_history, the last HISTORY_KEEP)
fn trial_history_push(p: &mut PlayerModel, game: &tgf::DTrialRoundData) {
    let mut list: Vec<tgf::DTrialRoundData> = serde_json::from_value(p.state["trial_history"].clone()).unwrap_or_default();
    list.insert(0, game.clone());
    list.truncate(super::trial::HISTORY_KEEP);
    p.state["trial_history"] = serde_json::to_value(&list).unwrap_or_default();
}

fn passport_mut(d: &mut tgf::DUserActivityData) -> Result<&mut tgf::DUserActivityPassprotsData, Fail> {
    match d.custom_data.as_mut() {
        Some(CustomData::PassportData(pp)) => Ok(pp),
        _ => Err(fail(ERR_ACT_TYPE, "this is not a passport")),
    }
}

// ---------- handlers ----------

impl GatewaySession {
    pub(super) async fn market_get_product_list_impl(&self, req: tgf::CsMarketGetProductListReq) -> Option<tgf::ScMarketGetProductListRsp> {
        let mut p = self.load_me().await;
        let (now, mut rng, mut changed) = (Self::now_ts(), Rng::new(), false);
        let ids = if req.batch_market_tab_ids.is_empty() { self.market_merchant_tabs() } else { req.batch_market_tab_ids };
        let batch_data = ids
            .into_iter()
            .map(|id| match self.market_tab(&mut p, id, now, &mut rng) {
                Some((d, c)) => {
                    changed |= c;
                    tgf::BatchMarketGetProduct { error_code: 0, market_tab_id: id, market_tab_data: Some(d) }
                }
                None => tgf::BatchMarketGetProduct { error_code: ERR_TAB_NOT_FOUND, market_tab_id: id, market_tab_data: None },
            })
            .collect();
        if changed {
            self.save_me(&p).await;
        }
        Some(tgf::ScMarketGetProductListRsp { batch_data })
    }

    pub(super) async fn market_buy_product_impl(&self, req: tgf::CsMarketBuyProductReq) -> Option<tgf::ScMarketBuyProductRsp> {
        let mut p = self.load_me().await;
        let owned_before = self.player_facade_owned(&p);
        match self.market_buy(&mut p, &req, &mut Rng::new()) {
            Ok(dirty) => {
                self.save_me(&p).await;
                Self::write_client_loadout_file(&p);
                println!("[Market] {} bought {}×{}", p.open_id, req.product_id, req.product_cnt.max(1));
                self.push_dirty(dirty);
                let new_facades: Vec<i32> = self.player_facade_owned(&p).into_iter().filter(|id| !owned_before.contains(id)).collect();
                if !new_facades.is_empty() {
                    println!("[Market] {}: appearances unlocked {:?}", p.open_id, new_facades);
                    self.push_dirty(self.facade_granted_dirty(&p, &new_facades));
                }
            }
            Err(e) => {
                println!("[Market] {} purchase {}: rejected — {}", p.open_id, req.product_id, e);
                self.set_rpc_error(e.code);
            }
        }
        Some(tgf::ScMarketBuyProductRsp { product_id: req.product_id, ..Default::default() })
    }

    /// Prices of "Insurance Premiums" (2100010–2100017) among items being sold: by create_info.insurance.item_guid — the amount the
    /// owner paid (insurance_paid, written by the death result); no entry — insurance of the original item at the current market.
    /// The merchant pays the premium's own `sale_price` — set by the battle server: payment × (1000 − dice_attenuate_ratio) / 1000,
    /// rounded down (native 0x142ec4130, docs/INSURANCE.md "Premium price"); no field — computed the same way from the payment.
    /// const `dice_attenuate_ratio` (‰, how much is cut from the premium; 150 in the table), as native: 0..1000
    fn dice_attenuate_ratio(&self) -> i64 {
        self.game_data.r#const.iter().find(|c| c.key == "dice_attenuate_ratio").and_then(|c| c.value.trim().parse::<i64>().ok()).unwrap_or(150).clamp(0, 1000)
    }

    async fn insurance_premium_prices(&self, p: &mut PlayerModel, basics: &[tgf::DItemBasic]) -> HashMap<i64, i64> {
        use tgf::d_item_create_info::CreateData;
        let mut out = HashMap::new();
        for want in basics {
            let Some(item) = super::inventory_handlers::find_item_mut(p, want.guid).map(|i| i.clone()) else { continue };
            let t = item.basic.as_ref().map_or(0, |b| b.type_id);
            if !(INSURANCE_PREMIUM_FIRST..=INSURANCE_PREMIUM_LAST).contains(&t) {
                continue;
            }
            let origin = item.create_info.as_ref().and_then(|c| match &c.create_data {
                Some(CreateData::Insurance(i)) => Some((i.item_guid, i.item_id)),
                _ => None,
            });
            let paid = match origin {
                Some((g, _)) if g != 0 => self
                    .db
                    .get::<super::raid_native::InsurancePaid>(super::raid_native::INSURANCE_PAID, &g.to_string())
                    .await
                    .map(|r| r.value),
                _ => None,
            };
            let own = item.data.as_ref().map_or(0, |d| d.sale_price as i64);
            let premium = match (paid, origin) {
                (Some(v), _) if v > 0 => v,
                (_, Some((_, id))) => {
                    let m = self.insurance_market_prices(&[id]).await.get(&id).copied().unwrap_or(0);
                    self.item_insurance_price(id, m) as i64
                }
                _ => 0,
            };
            let price = if own > 0 { own } else { insurance_premium_sale(premium, self.dice_attenuate_ratio()) };
            println!("[Insurance] premium guid={} original {:?}: payment {} ({}), sale_price {}, to merchant {}", want.guid, origin, premium,
                if paid.is_some() { "paid by owner" } else { "by market" }, own, price);
            out.insert(want.guid, price);
        }
        out
    }

    pub(super) async fn market_sell_items_impl(&self, req: tgf::CsMarketSellItemsReq) -> Option<tgf::ScMarketSellItemsRsp> {
        let mut p = self.load_me().await;
        let premiums = self.insurance_premium_prices(&mut p, &req.sold_item_basics).await;
        let dirty = self.market_sell(&mut p, &req.sold_item_basics, &premiums);
        if dirty.dirty_postion_action.is_empty() {
            // nothing sold (no items / no price)
            self.set_rpc_error(ERR_ITEM_GUID);
            return Some(tgf::ScMarketSellItemsRsp {});
        }
        self.save_me(&p).await;
        Self::write_client_loadout_file(&p);
        println!("[Market] {} sold {} items, gold {}", p.open_id, dirty.dirty_postion_action.len(), common::coin(&p, common::GOLD));
        self.push_dirty(dirty);
        Some(tgf::ScMarketSellItemsRsp {})
    }

    pub(super) async fn market_buy_sold_item_impl(&self, req: tgf::CsMarketBuySoldItemReq) -> Option<tgf::ScMarketBuySoldItemRsp> {
        let mut p = self.load_me().await;
        match self.market_buy_back(&mut p, &req) {
            Ok(dirty) => {
                self.save_me(&p).await;
                Self::write_client_loadout_file(&p);
                self.push_dirty(dirty);
            }
            Err(e) => {
                println!("[Market] {} buyback guid={}: rejected — {}", p.open_id, req.item_guid, e);
                self.set_rpc_error(e.code);
            }
        }
        Some(tgf::ScMarketBuySoldItemRsp {})
    }

    /// The client played the tab refresh animation — the user_market_data.play_refresh_effect flag is reset
    pub(super) async fn market_reset_refresh_effect_impl(&self, _req: tgf::CsMarketResetRefreshEffectReq) -> Option<tgf::ScMarketResetRefreshEffectRsp> {
        let mut p = self.load_me().await;
        p.state["market_play_refresh_effect"] = json!(false);
        self.save_me(&p).await;
        self.push_dirty(tgf::ScPlayerDirtyChgSync {
            dirty_market_data: Some(tgf::DUserMarketData { play_refresh_effect: false }),
            ..Default::default()
        });
        Some(tgf::ScMarketResetRefreshEffectRsp {})
    }

    /// Shop video shown: remember the time, return all shown ones
    pub(super) async fn market_video_popup_impl(&self, req: tgf::CsMarketVideoPopUpReq) -> Option<tgf::ScMarketVideoPopUpRsp> {
        let mut p = self.load_me().await;
        let id = req.popup_video_id as i32;
        if self.game_data.market_popup_video.iter().any(|v| v.popup_video_id == id) {
            if !p.state["market_popup_videos"].is_object() {
                p.state["market_popup_videos"] = json!({});
            }
            p.state["market_popup_videos"][id.to_string().as_str()] = json!(Self::now_ts());
            self.save_me(&p).await;
        } else {
            println!("[Market] video {} not found in market_popup_video", id);
            self.set_rpc_error(ERR_VIDEO_NOT_FOUND);
        }
        Some(tgf::ScMarketVideoPopUpRsp { popup_videos: market_popup_list(&p) })
    }

    pub(super) async fn activity_center_list_impl(&self, _req: tgf::CsActivityCenterListReq) -> Option<tgf::ScActivityCenterListRsp> {
        Some(tgf::ScActivityCenterListRsp { list: Some(self.act_user_list().await) })
    }

    /// Random reward (EN_ACTIVITY_CENTER_RANDOM_ITEM, 16502 — ended in 2024 in the config): roll items from the pool
    /// by the hall role's class; result — DUserActivityRandomData.unlocked_rewards.
    pub(super) async fn activity_center_random_rewards_impl(&self, req: tgf::CsActivityRandomRewardReq) -> Option<tgf::ScActivityRandomRewardRsp> {
        let items = self
            .act_action("random_rewards", req.activity_id, |s, p, d, cfg, rng| {
                if cfg.r#type != ACT_RANDOM_ITEM {
                    return Err(fail(ERR_ACT_TYPE, "not RANDOM_ITEM"));
                }
                let rd = match d.custom_data.take() {
                    Some(CustomData::RandomData(rd)) => rd,
                    _ => Default::default(),
                };
                if !rd.unlocked_rewards.is_empty() && !req.paid_refresh {
                    let items = rd.unlocked_rewards.clone();
                    d.custom_data = Some(CustomData::RandomData(rd));
                    return Ok((items, vec![], vec![]));
                }
                // TODO(verify): career_type_id = role_type − 400000 (400001 → 1); paid roll — manual_refresh_costs
                let hall = if p.hall_role_guid != 0 { p.hall_role_guid } else { p.roles.first().map(|r| r.role_guid).unwrap_or(0) };
                let career = p.roles.iter().find(|r| r.role_guid == hall).map(|r| r.type_id as i32 - 400_000).unwrap_or(1);
                let mut rows: Vec<_> = s.game_data.activity_center_random_rewards.values().filter(|r| r.activity_id == cfg.id).collect();
                rows.sort_by_key(|r| (r.career_type_id != career, r.id));
                let row = rows.first().or_fail(ERR_ACT_EXCEL, "no activity_center_random_rewards")?;
                let pool = s.game_data.activity_center_random_rewards_pool.get(&row.item_randoom_pool_id).or_fail(ERR_ACT_EXCEL, "no pool")?;
                let mut coins = vec![];
                if req.paid_refresh {
                    for kv in &cfg.manual_refresh_costs {
                        common::add_coin(p, kv.k, -(kv.v as i64)).or_fail(ERR_ACT_PAID, "not enough for a paid roll")?;
                        coins.push(kv.k);
                    }
                }
                let cnt_w: Vec<i64> = row.item_cnt_pool.iter().map(|kv| kv.v as i64).collect();
                let n = pick_weighted(&cnt_w, 1, rng).first().map(|i| row.item_cnt_pool[*i].k).unwrap_or(1).max(1) as usize;
                let w: Vec<i64> = pool.random_item.iter().map(|t| t.c as i64).collect();
                let items: Vec<tgf::DItemBasic> = pick_weighted(&w, n, rng)
                    .into_iter()
                    .map(|i| tgf::DItemBasic { type_id: pool.random_item[i].a, count: pool.random_item[i].b.max(1), ..Default::default() })
                    .collect();
                d.custom_data = Some(CustomData::RandomData(tgf::DUserActivityRandomData {
                    unlocked_rewards: items.clone(),
                    selecd_max: row.selected_max.max(1),
                    selecd_type_ids: vec![],
                }));
                Ok((items, coins, vec![]))
            })
            .await;
        Some(tgf::ScActivityRandomRewardRsp { items })
    }

    /// Choice from the roll: grant the chosen ones (no more than selecd_max) where the client put them
    pub(super) async fn activity_center_random_reward_selected_impl(&self, req: tgf::CsActivityRandomRewardSelectdReq) -> Option<tgf::ScActivityRandomRewardSelectdRsp> {
        self.act_action("random_reward_selected", req.activity_id, |s, p, d, _cfg, rng| {
            let Some(CustomData::RandomData(rd)) = d.custom_data.as_mut() else { return Err(fail(ERR_ACT_INVALID_REWARDS, "no roll")) };
            let room = (rd.selecd_max.max(1) as usize).saturating_sub(rd.selecd_type_ids.len());
            let picked: Vec<tgf::DItemOffset> = req
                .items_position
                .iter()
                .filter(|b| rd.unlocked_rewards.iter().any(|u| u.type_id == b.type_id) && !rd.selecd_type_ids.contains(&b.type_id))
                .take(room)
                .map(|b| {
                    let count = rd.unlocked_rewards.iter().find(|u| u.type_id == b.type_id).map(|u| u.count).unwrap_or(1);
                    tgf::DItemOffset { type_id: b.type_id, count, expire_offset: 0 }
                })
                .collect();
            if picked.is_empty() {
                return Err(fail(ERR_ACT_INVALID_REWARDS, "nothing to grant"));
            }
            rd.selecd_type_ids.extend(picked.iter().map(|r| r.type_id));
            let (_, coins, items) = s.shop_grant_rewards(p, &picked, &req.items_position, rng)?;
            Ok(((), coins, items))
        })
        .await;
        Some(tgf::ScActivityRandomRewardSelectdRsp {})
    }

    /// Buying the passport premium branch. TODO(verify): price (DLC / real money) unknown — free
    pub(super) async fn activity_center_buy_passport_impl(&self, req: tgf::CsActivityCenterBuyPassportReq) -> Option<tgf::ScActivityCenterBuyPassportRsp> {
        self.act_action("buy_passport", req.activity_id, |_, _, d, _, _| {
            let pp = passport_mut(d)?;
            if pp.unlocked_high_rewards {
                return Err(fail(ERR_ACT_NOT_SUPPORT, "already bought"));
            }
            pp.unlocked_high_rewards = true;
            Ok(((), vec![], vec![]))
        })
        .await;
        Some(tgf::ScActivityCenterBuyPassportRsp {})
    }

    /// Passport level reward: an activity_center_passprots_rewards row with basic_rewards_id or high_rewards_id = reward_id,
    /// available at exp ≥ exp_begin; high — only after purchase. 16518 has no rows (only 16515 does).
    /// TODO(verify): source of passport exp (exp_item_type) — currently exp is always 0
    pub(super) async fn activity_center_passport_receive_rewards_impl(&self, req: tgf::CsActivityCenterPassportReceiveRewardsReq) -> Option<tgf::ScActivityCenterPassportReceiveRewardsRsp> {
        let items = self
            .act_action("passport_receive_rewards", req.activity_id, |s, p, d, cfg, rng| {
                let pp = passport_mut(d)?;
                let row = s
                    .game_data
                    .activity_center_passprots_rewards
                    .values()
                    .find(|r| r.activity_id == cfg.id && req.reward_id != 0 && (r.basic_rewards_id == req.reward_id || r.high_rewards_id == req.reward_id))
                    .or_fail(ERR_ACT_REWARD_NOT_MATCH, "no such reward")?;
                if pp.received_rewards_id.contains(&req.reward_id) {
                    return Err(fail(ERR_ACT_REPEATED, "already received"));
                }
                if pp.exp < row.exp_begin {
                    return Err(fail(ERR_ACT_REWARD_COND, format!("exp {} < {}", pp.exp, row.exp_begin)));
                }
                // as in BattlePassRewardService.lua: basic_rewards_id — free, high_rewards_id — after purchase
                let rewards = if row.high_rewards_id == req.reward_id {
                    if !pp.unlocked_high_rewards {
                        return Err(fail(ERR_ACT_REWARD_COND, "premium branch not bought"));
                    }
                    row.high_rewards.clone()
                } else {
                    row.basic_rewards.clone()
                };
                pp.received_rewards_id.push(req.reward_id);
                s.shop_grant_rewards(p, &rewards, &req.items_position, rng)
            })
            .await;
        Some(tgf::ScActivityCenterPassportReceiveRewardsRsp { items })
    }

    /// Cyclic rewards after the max level (grade_max_exp.cycle_exp). TODO(verify): the cycle reward contents
    /// were not found in the tables — nothing is granted
    pub(super) async fn activity_center_passport_receive_cycle_rewards_impl(&self, req: tgf::CsActivityCenterPassportReceiveCycleRewardsReq) -> Option<tgf::ScActivityCenterPassportReceiveCycleRewardsRsp> {
        println!("[Activity] passport_receive_cycle_rewards {} grade_qty={}: not implemented", req.activity_id, req.grade_qty);
        self.set_rpc_error(ERR_ACT_NOT_SUPPORT);
        Some(tgf::ScActivityCenterPassportReceiveCycleRewardsRsp { items: vec![] })
    }

    /// Replacing a passport daily quest with another one from the pool (price — manual_refresh_costs, empty for 16518)
    pub(super) async fn activity_center_passport_refresh_quest_impl(&self, req: tgf::CsActivityCenterPassportRefreshQuestReq) -> Option<tgf::ScActivityCenterPassportRefreshQuestRsp> {
        let quest_id = self
            .act_action("passport_refresh_quest", req.activity_id, |s, p, d, cfg, rng| {
                let pp = passport_mut(d)?;
                let current: Vec<i32> = pp.daily_quests.iter().map(|q| q.quest_id).collect();
                let slot = pp.daily_quests.iter_mut().find(|q| q.quest_id == req.quest_id).or_fail(ERR_ACT_QUEST_NOT_IN_DAILY, "no such quest among the daily ones")?;
                let new_id = *s.act_roll_quests(cfg, 1, &current, rng).first().or_fail(ERR_ACT_QUEST_REFRESH, "pool is empty")?;
                let mut coins = vec![];
                for kv in &cfg.manual_refresh_costs {
                    common::add_coin(p, kv.k, -(kv.v as i64)).or_fail(ERR_ACT_PAID, "not enough for a replacement")?;
                    coins.push(kv.k);
                }
                slot.quest_id = new_id;
                slot.status = QUEST_PROCESSING;
                // quest domain: remove the old one, create the new one (Q1)
                s.progress_quest_drop(p, req.quest_id);
                s.progress_quest_drop(p, new_id);
                s.progress_quest_ensure(p, new_id);
                s.push_dirty(s.quest_dirty(p));
                Ok((new_id, coins, vec![]))
            })
            .await;
        Some(tgf::ScActivityCenterPassportRefreshQuestRsp { quest_id })
    }

    // "Trial of Clotho" (EN_ACTIVITY_CENTER_IMMACULATE_TRIAL, 16600; docs/CLOTHO_TRIAL_PLAN.md): the event is extended in
    // the balance pak, the run — trial.rs. Stone 1033 (immaculate_trial_rule.cost_itmes) opens a run, matches are recorded by the result
    // of the arena of level 50000006 (trial_record_match), reward — by mails 501030 (first) and 501031 (by grade).

    fn trial_rule(&self, rule_id: i32) -> Option<&tgf::config::ExcelImmaculateTrialRule> {
        self.game_data
            .immaculate_trial_rule
            .get(&rule_id)
            .or_else(|| self.game_data.immaculate_trial_rule_rows.iter().find(|r| r.activity_rule_id == rule_id))
    }

    pub(super) async fn activity_center_trial_open_impl(&self, req: tgf::CsActivityCenteTrialOpenReq) -> Option<tgf::ScActivityCenterTrialOpenRsp> {
        self.act_action("trial_open", req.activity_id, |s, p, d, cfg, _rng| {
            if cfg.r#type != ACT_TRIAL {
                return Err(fail(ERR_ACT_TYPE, "not a trial"));
            }
            let rule = s.trial_rule(cfg.activity_rule).or_fail(ERR_TRIAL_RULE, "no immaculate_trial_rule")?.clone();
            let td = trial_mut(d)?;
            if td.current_game.as_ref().is_some_and(|g| g.status != super::trial::STATUS_NONE) {
                return Err(fail(ERR_TRIAL_ALREADY_OPEN, "run already open or awaiting reward"));
            }
            let mut ch = super::inventory_handlers::ItemChanges::default();
            s.inv_pay(p, &rule.cost_itmes, &req.items, &mut ch).map_err(|e| fail(ERR_ACT_PAID, e))?;
            super::trial::open(td);
            println!("[Trial] {}: run #{} opened", p.open_id, td.open_times);
            Ok(((), ch.coins, ch.dirty))
        })
        .await;
        Some(tgf::ScActivityCenterTrialOpenRsp {})
    }

    pub(super) async fn activity_center_trial_self_quit_impl(&self, req: tgf::CsActivityCenterTrialSelfQuitReq) -> Option<tgf::ScActivityCenterTrialSelfQuitRsp> {
        self.act_action("trial_self_quit", req.activity_id, |s, p, d, cfg, _rng| {
            let rule = s.trial_rule(cfg.activity_rule).or_fail(ERR_TRIAL_RULE, "no immaculate_trial_rule")?.clone();
            let td = trial_mut(d)?;
            if !td.current_game.as_ref().is_some_and(|g| g.status == super::trial::STATUS_OPEN) {
                return Err(fail(ERR_TRIAL_ALREADY_END, "run is not in progress"));
            }
            super::trial::close(td, &s.game_data.immaculate_trial_grade_rows, &rule, true);
            println!("[Trial] {}: gave up, grade {:?}", p.open_id, td.current_game.as_ref().map(|g| g.grade_id));
            Ok(((), vec![], vec![]))
        })
        .await;
        Some(tgf::ScActivityCenterTrialSelfQuitRsp {})
    }

    /// Reward of a closed run: first-time (once per grade) and by grade — by mail; the run goes into history, status "none"
    pub(super) async fn activity_center_trial_receive_reward_impl(&self, req: tgf::CsActivityCenterTrialReceiveRewardReq) -> Option<tgf::ScActivityCenterTrialReceiveRewardRsp> {
        let got: (Vec<tgf::DItemOffset>, Vec<tgf::DItemOffset>, i32, i32) = self
            .act_action("trial_receive_reward", req.activity_id, |s, p, d, cfg, _rng| {
                let rule = s.trial_rule(cfg.activity_rule).or_fail(ERR_TRIAL_RULE, "no immaculate_trial_rule")?.clone();
                let td = trial_mut(d)?;
                let game = td
                    .current_game
                    .clone()
                    .filter(|g| g.status == super::trial::STATUS_REWARD)
                    .or_fail(ERR_ACT_REWARD_COND, "no reward: run not closed")?;
                let grade = super::trial::grade_for(&s.game_data.immaculate_trial_grade_rows, rule.activity_rule_id, game.win_count, game.failed_count)
                    .or_fail(ERR_TRIAL_GRADE, "no grade for the result")?
                    .clone();
                let (first, random) = super::trial::rewards(td, &grade);
                if !first.is_empty() {
                    td.received_first_reward_ids.push(grade.grade_id);
                }
                trial_history_push(p, &game);
                td.current_game = Some(tgf::DTrialRoundData { status: super::trial::STATUS_NONE, grade_id: grade.grade_id, ..Default::default() });
                println!(
                    "[Trial] {}: grade reward {} ({}/{}): first-time {}, by grade {}",
                    p.open_id, grade.grade_id, game.win_count, game.failed_count, first.len(), random.len()
                );
                Ok(((first, random, rule.first_reward_mail_id, rule.random_reward_mail_id), vec![], vec![]))
            })
            .await;
        let (first, random, first_mail, random_mail) = got;
        let as_items = |v: &[tgf::DItemOffset]| -> Vec<tgf::DItem> {
            v.iter()
                .map(|o| tgf::DItem { basic: Some(tgf::DItemBasic { type_id: o.type_id, count: o.count.max(1), ..Default::default() }), ..Default::default() })
                .collect()
        };
        for (template, list) in [(first_mail, &first), (random_mail, &random)] {
            if list.is_empty() {
                continue;
            }
            match super::progress_handlers::mail_from_template(&self.game_data, template, as_items(list), &[]) {
                Some(mail) => {
                    super::progress_handlers::mail_send(&self.db, &self.game_data, &self.open_id, mail).await;
                }
                None => println!("[Trial] {}: no mail template {} — reward {:?} not sent", self.open_id, template, list),
            }
        }
        Some(tgf::ScActivityCenterTrialReceiveRewardRsp {
            rewards: Some(tgf::DTrialRewardsData {
                first_item_rewards: as_items(&first),
                random_item_rewards: as_items(&random),
                first_rewards: first,
                random_rewards: random,
            }),
        })
    }

    pub(super) async fn activity_center_trial_history_record_impl(&self, _req: tgf::CsActivityCenterTrialHistoryRecordReq) -> Option<tgf::ScActivityCenterTrialHistoryRecordRsp> {
        let p = self.load_me().await;
        let records: Vec<tgf::DTrialRoundData> = serde_json::from_value(p.state["trial_history"].clone()).unwrap_or_default();
        Some(tgf::ScActivityCenterTrialHistoryRecordRsp { records })
    }

    /// The player's run state (the launcher's "Clotho" tab): None — no event. Read only, no saving.
    pub(super) async fn trial_state(&self, open_id: &str) -> Option<super::trial::TrialMe> {
        let cfg = self.game_data.activity_center.values().find(|c| c.r#type == ACT_TRIAL && c.on).cloned()?;
        let rule = self.trial_rule(cfg.activity_rule).cloned()?;
        let mut p = self.db.get::<PlayerModel>("players", open_id).await?;
        let (d, _) = self.act_data(&mut p, &cfg, Self::now_ts(), &mut Rng::new());
        let Some(CustomData::TrialData(td)) = d.custom_data else { return None };
        let g = td.current_game.unwrap_or_default();
        let tokens = if g.status == super::trial::STATUS_OPEN { (rule.failed_count - g.failed_count).max(0) } else { 0 };
        Some(super::trial::TrialMe { activity_id: cfg.id, status: g.status, wins: g.win_count, fails: g.failed_count, tokens })
    }

    /// Trial match (result of the arena of level 50000006): a win or loss into the run of player `open_id`; push of event data and
    /// `activity_center_trial_record_sync`. No run, it is closed or the match is already recorded — nothing (false).
    pub(super) async fn trial_record_match(&self, open_id: &str, battle_id: &str, won: bool) -> bool {
        let Some(cfg) = self.game_data.activity_center.values().find(|c| c.r#type == ACT_TRIAL && c.on).cloned() else { return false };
        let Some(rule) = self.trial_rule(cfg.activity_rule).cloned() else { return false };
        let Some(mut p) = self.db.get::<PlayerModel>("players", open_id).await else { return false };
        let (now, mut rng) = (Self::now_ts(), Rng::new());
        let (mut d, _) = self.act_data(&mut p, &cfg, now, &mut rng);
        let Some(CustomData::TrialData(td)) = d.custom_data.as_mut() else { return false };
        let Some(closed) = super::trial::record_match(td, &self.game_data.immaculate_trial_grade_rows, &rule, battle_id, won) else {
            println!("[Trial] {}: match {} not recorded — run not in progress or already recorded", open_id, battle_id);
            return false;
        };
        let record = td.current_game.clone();
        let line = record.as_ref().map(|g| format!("{}/{}", g.win_count, g.failed_count)).unwrap_or_default();
        Self::act_store(&mut p, &d);
        self.db.save("players", open_id, &p).await;
        crate::lobby::hub::push_to(
            open_id,
            crate::lobby::hub::DIRTY_METHOD,
            crate::lobby::hub::DIRTY_TYPE,
            &tgf::ScPlayerDirtyChgSync { dirty_activity_data: Some(tgf::DUserActivityDataList { datas: vec![d] }), ..Default::default() },
        );
        crate::lobby::hub::push_to(
            open_id,
            "tgf.GameClientService.activity_center_trial_record_sync",
            "tgf.SCActivityCenterTrialRecordSync",
            &tgf::ScActivityCenterTrialRecordSync { record },
        );
        println!("[Trial] {}: match {} — {}, run {}{}", open_id, battle_id, if won { "win" } else { "loss" }, line, if closed { ", closed" } else { "" });
        true
    }

    // Mithril force: roles 400101.. (role_type.meta.force = MITHRIL_FORCE), items mithril_force_item, prestige 0
    // (player_get_info: mithril_force_data.prestige = 0). The mode is not used — minimal responses.

    /// Mithril force role of the needed class: if the player already has it — its guid. TODO(verify): role creation (role_create
    /// with role_type 4001xx and starter gear) — currently 0 if there is no such role
    pub(super) async fn mithril_force_role_init_impl(&self, req: tgf::CsMithrilForceRoleInitReq) -> Option<tgf::ScMithrilForceRoleInitRsp> {
        let p = self.load_me().await;
        let types: Vec<i32> = self
            .game_data
            .role_type
            .values()
            .filter(|t| t.meta.as_ref().map(|m| m.force == FORCE_MITHRIL && m.career == req.career).unwrap_or(false))
            .map(|t| t.type_id)
            .collect();
        let role_guid = p.roles.iter().find(|r| types.contains(&(r.type_id as i32))).map(|r| r.role_guid as i64).unwrap_or(0);
        if role_guid == 0 {
            println!("[Mithril] role_init career={}: no role (types {:?}), creation not implemented", req.career, types);
        }
        Some(tgf::ScMithrilForceRoleInitRsp { role_guid })
    }

    pub(super) async fn mithril_force_role_select_impl(&self, _req: tgf::CsMithrilForceRoleSelectReq) -> Option<tgf::ScMithrilForceRoleSelectRsp> {
        Some(tgf::ScMithrilForceRoleSelectRsp {})
    }

    /// Prestige rewards: prestige is always 0 — nothing to grant
    pub(super) async fn mithril_force_receive_reward_impl(&self, _req: tgf::CsMithrilForceReceiveRewardReq) -> Option<tgf::ScMithrilForceReceiveRewardRsp> {
        Some(tgf::ScMithrilForceReceiveRewardRsp {})
    }

    /// Equip an issued Mithril force item (type_id from mithril_force_item with unlock_level ≤ 1) into slot goal_position:
    /// a new item is created; a previous mithril item in the slot is deleted, a regular one — rejection.
    pub(super) async fn mithril_role_wear_impl(&self, req: tgf::CsMithrilRoleWearReq) -> Option<tgf::ScMithrilRoleWearRsp> {
        let goal = req.goal_position.clone().unwrap_or_default();
        let ok = self.game_data.mithril_force_item.get(&req.type_id).map(|m| m.unlock_level <= 1).unwrap_or(false);
        if !ok || goal.r#type != WEARING {
            println!("[Mithril] role_wear {}: not a mithril item or not a slot (type {})", req.type_id, goal.r#type);
            self.set_rpc_error(ERR_MITHRIL_ITEM_INVALID);
            return Some(tgf::ScMithrilRoleWearRsp { worn_item: None });
        }
        let mut p = self.load_me().await;
        let mut rng = Rng::new();
        let mut item = self.shop_new_item(&mut p, req.type_id, req.count.max(1), &mut rng);
        item.basic.as_mut().unwrap().position = Some(goal.clone());
        let mithril = &self.game_data.mithril_force_item;
        let Some((list, _, _)) = place_list(&mut p, WEARING, goal.guid) else {
            println!("[Mithril] role_wear: no role {}", goal.guid);
            self.set_rpc_error(ERR_MITHRIL_ITEM_INVALID);
            return Some(tgf::ScMithrilRoleWearRsp { worn_item: None });
        };
        let mut dirty = vec![];
        if let Some(i) = list.iter().position(|it| pos_of(it).x == goal.x) {
            let type_id = list[i].basic.as_ref().map(|b| b.type_id).unwrap_or(0);
            if !mithril.contains_key(&type_id) {
                println!("[Mithril] role_wear: slot {} occupied by a regular item {}", goal.x, type_id);
                self.set_rpc_error(ERR_MITHRIL_ITEM_INVALID);
                return Some(tgf::ScMithrilRoleWearRsp { worn_item: None });
            }
            dirty.push(removal(&list.remove(i)));
        }
        list.push(item.clone());
        dirty.push(item.clone());
        self.save_me(&p).await;
        Self::write_client_loadout_file(&p);
        self.push_dirty(tgf::ScPlayerDirtyChgSync { dirty_postion_action: dirty, ..Default::default() });
        Some(tgf::ScMithrilRoleWearRsp { worn_item: Some(item) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 08.11: premium to merchant = payment × (1000 − dice_attenuate_ratio) / 1000 rounded down; 150 in the table → 85%:
    /// stand 108 000 → 91 800 (as the game's tooltip), original 196 020 → 166 617
    #[test]
    fn insurance_premium_sale_price() {
        let gd = crate::game_data::GameData::load_all("ExcelConfig");
        let r = gd.r#const.iter().find(|c| c.key == "dice_attenuate_ratio").map(|c| c.value.trim().to_string());
        assert_eq!(r.as_deref(), Some("150"));
        assert_eq!(insurance_premium_sale(108_000, 150), 91_800);
        assert_eq!(insurance_premium_sale(196_020, 150), 166_617);
        assert_eq!(insurance_premium_sale(0, 150), 0);
        assert_eq!(insurance_premium_sale(108_000, 0), 108_000);
        assert_eq!(insurance_premium_sale(108_000, 5000), 0);
    }

    /// 07.11: crafted items (Heirloom relics) go to the merchant for 0; pink ones used to go for price_def 40000
    #[tokio::test]
    async fn crafted_items_sell_for_zero() {
        let dir = format!("target/test_craft_sell_db_{}", crate::auth::sessions::random_hex(4));
        let db = crate::db::GameDb::init(&dir).await;
        let s = GatewaySession {
            db,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "craft".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let gd = s.game_data.clone();
        let heir: Vec<i32> = gd.item_type.keys().copied().filter(|t| t / 10_000_000 == 22).collect();
        assert!(heir.len() > 100, "relics in data: {}", heir.len());
        for t in &heir {
            assert!(s.is_crafted(*t), "{} not recognized as crafted", t);
            assert_eq!(s.merchant_sell_price(*t), 0, "{}", t);
        }
        // 13.11: the trade price is no longer substituted — relics have no buyback at all
        assert!(heir.iter().all(|t| s.item_sell_price(*t) == 0), "relics without system_repurchase_price — 0");
        // price — exactly system_repurchase_price from the table, the auction price (400 000) is not substituted; the seasonal item_type
        // (718ddda) gives 189010000 200 000 = 50% Buyout, base — 0
        for t in [189010000, 189010001, 181040000] {
            let table = gd.item_type.get(&t).map_or(0, |r| r.system_repurchase_price as i64);
            assert_eq!(s.item_sell_price(t), table.max(0), "{}", t);
        }
        // regular gear — the previous price
        let sword = 181040000;
        assert!(!s.is_crafted(sword));
        assert_eq!(s.merchant_sell_price(sword), s.item_sell_price(sword));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reset_boundaries() {
        // from the sniffer: join 1793052153 (Mon 22:02) → next_daily 1793059200, next_weekly 1793577600
        assert_eq!(next_day(1_793_052_153), 1_793_059_200);
        assert_eq!(next_monday(1_793_052_153), 1_793_577_600);
        assert_eq!(next_monday(1_793_310_465), 1_793_577_600);
    }

    #[test]
    fn rotation_day_index_matches_pve() {
        // PVE: now ≈ 2026-10-30, 1009 start 2023-05-06 → day 1273 → window 13 of 20, 1 of 4
        let cfg = tgf::MarketTabInfo {
            period: 604_800,
            start_timepoint: Some(crate::google::protobuf::Timestamp { seconds: 1_683_331_200, nanos: 0 }),
            ..Default::default()
        };
        let (gen, last, next, day) = tab_cycle(&cfg, 1_793_330_000, true);
        assert_eq!(day, 1273);
        assert_eq!((day % 20, day % 4), (13, 1));
        assert_eq!(gen, day);
        assert_eq!(last, 1_793_318_400);
        assert_eq!(next, 1_793_404_800);
    }

    /// 18.11 (owner's item 8): buying arrows "by quantity" (product_cnt = N, the product is 1 arrow, stack 15, 2×1).
    /// Each arrow was put separately: the push carried every intermediate step of one stack ("removed / 2 pcs / removed /
    /// 3 pcs …") — the same thing that broke stacks on moving (FIX_LIST 08.11, compact_dirty). Now: per guid at most one
    /// "removed" and one final entry, the final entries match the saved player; stacks ≤ 15, cells do not overlap, nothing lost.
    #[tokio::test]
    async fn buy_arrows_by_count_stacks_and_one_entry_per_guid() {
        let dir = format!("target/test_arrows_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "arrows".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        const ARROW: i32 = 100045;
        assert_eq!(s.shop_item_shape(ARROW), (2, 1, 15), "arrow: 2×1, stack 15");
        let role_guid = 7u64;
        let mut p = PlayerModel::new("arrows", 10001);
        p.state["gold"] = json!(1_000_000);
        // 7 arrows already in the backpack (an incomplete stack) and a sword next to it
        let item = |type_id: i32, count: i32, guid: i64, x: i32, y: i32| tgf::DItem {
            basic: Some(tgf::DItemBasic {
                type_id,
                count,
                guid,
                position: Some(tgf::DItemPosition { r#type: BACKPACK, guid: role_guid as i64, x, y, ..Default::default() }),
                ..Default::default()
            }),
            data: Some(Default::default()),
            create_info: None,
        };
        p.roles.push(crate::structures::RoleModel {
            role_guid,
            ord: 1,
            type_id: 400006,
            role_name: "Archer".into(),
            role_level: 1,
            data: tgf::DUserRole {
                deploy_role: Some(tgf::DBattleDeployRole {
                    backpack: Some(tgf::DRoleBackpackData { items: vec![item(ARROW, 7, 501, 0, 0), item(181040000, 1, 502, 2, 0)], row_size: 7, column_size: 10 }),
                    ..Default::default()
                }),
                ..Default::default()
            },
        });
        // the merchant tab where arrows are on sale now
        let mut rng = Rng(7);
        let now = GatewaySession::now_ts();
        let (prod, _) = [(1004, 704013), (1016, 705004)]
            .into_iter()
            .find(|(tab, prod)| s.market_tab(&mut p, *tab, now, &mut rng).is_some_and(|(t, _)| t.available_products.iter().any(|a| a.product_id == *prod)))
            .map(|(tab, prod)| (prod, tab))
            .expect("arrows are not on sale in tabs 1004/1016");
        let gold0 = p.state["gold"].as_i64().unwrap();
        let req = tgf::CsMarketBuyProductReq {
            product_id: prod,
            product_cnt: 40,
            item_storage_type: BACKPACK,
            item_storage_guid: role_guid as i64,
            market_type: MERCHANT_MARKET,
            ..Default::default()
        };
        let dirty = s.market_buy(&mut p, &req, &mut rng).map_err(|e| e.to_string()).expect("purchase").dirty_postion_action;
        assert_eq!(p.state["gold"].as_i64().unwrap(), gold0 - 40 * 25, "40 × 25 gold");

        // the player: 47 arrows, stacks ≤ 15, no overlapping cells
        let bag = p.roles[0].data.deploy_role.as_ref().unwrap().backpack.as_ref().unwrap().items.clone();
        let arrows: Vec<&tgf::DItem> = bag.iter().filter(|i| i.basic.as_ref().unwrap().type_id == ARROW).collect();
        assert_eq!(arrows.iter().map(|i| i.basic.as_ref().unwrap().count).sum::<i32>(), 47, "nothing lost");
        assert!(arrows.iter().all(|i| (1..=15).contains(&i.basic.as_ref().unwrap().count)), "stack limit 15");
        assert_eq!(arrows.len(), 4, "15 + 15 + 15 + 2");
        let mut cells = HashSet::new();
        for it in &bag {
            let b = it.basic.as_ref().unwrap();
            let pos = b.position.clone().unwrap();
            let (w, h, _) = s.shop_item_shape(b.type_id);
            for x in pos.x..pos.x + w {
                for y in pos.y..pos.y + h {
                    assert!(x < 7 && y < 10, "outside the backpack: guid {}", b.guid);
                    assert!(cells.insert((x, y)), "cell ({}, {}) is taken twice (guid {})", x, y, b.guid);
                }
            }
        }

        // the push: per guid at most one "removed" (count 0) and one final; the final one = the saved item
        let mut by_guid: HashMap<i64, Vec<&tgf::DItem>> = HashMap::new();
        for d in &dirty {
            by_guid.entry(guid_of(d)).or_default().push(d);
        }
        for (g, recs) in &by_guid {
            let finals: Vec<_> = recs.iter().filter(|d| d.basic.as_ref().unwrap().count > 0).collect();
            let removals = recs.len() - finals.len();
            assert!(removals <= 1 && finals.len() == 1, "guid {}: {} removals, {} final entries in one push", g, removals, finals.len());
            let saved = bag.iter().find(|i| guid_of(i) == *g).expect("final entry of an item the player has");
            assert_eq!(*finals[0], saved, "guid {}: push ≠ saved", g);
        }
        assert_eq!(by_guid.len(), 4, "the old stack + 3 new ones; the sword is untouched");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn weighted_pick_no_repeats() {
        let mut rng = Rng(42);
        let got = pick_weighted(&[1, 100, 1, 0], 3, &mut rng);
        assert_eq!(got.len(), 3);
        let set: HashSet<_> = got.iter().collect();
        assert_eq!(set.len(), 3);
    }
}
