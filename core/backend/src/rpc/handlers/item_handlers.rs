//! Items: gear (gear_*), item use, stash pages, crafting.
//! The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Sources: there are no dumps/sniffer captures for these RPCs — the format comes from proto (tgf.rs), ExcelConfigjson tables and the client's UE
//! dumps (TGFGame.hpp: FusionService, HeirloomService, InventoryService). The official server sniffer
//! (player_get_info) gives: userCraftingData.recipe = all 8 craft_group recipes, heirloomData.data = one record
//! with limitOption.season 1, a newcomer's stash — 1 page.
//!
//! General:
//! - These RPC responses have no ret_code; on rejection the state does not change, the EnErrorCode goes to the log and RpcHead.error_code
//!   (self.set_rpc_error), the response body is empty.
//! - Currency (item_major_type CURRENCY: 1001 gold, 1037/1038/1039/1052/1053 materials) — common::coin coins,
//!   the rest — items in the grid (inventory_handlers::inv_give/inv_pay). Push — SCPlayerDirtyChgSync:
//!   dirty_postion_action (as for moves: "removed" + full), dirty_coin_data and the domain dirty_*.
//! - Player state: state["item_crafting"] (crafting slots), state["item_crafting_recipes"] (recipes beyond
//!   craft_group), state["item_heirloom"] (DUserHeirloomData), state["item_soul_bound"] {wear_part, role_id, count},
//!   state["inventory_page_count"] (stash pages, see inventory_handlers::stash_page_count).
//! - For player_get_info (account_handlers.rs): item_crafting_data / item_heirloom_data / item_soul_bound_data.

use super::common;
use super::inventory_handlers::{self as inv, ItemChanges, STASH, WEARING};
use super::GatewaySession;
use super::progress_handlers::QuestEvent;
use crate::structures::PlayerModel;
use crate::tgf;
use crate::tgf::EnErrorCode as E;

/// Lower quality: among the unlocked relics there is one that requires `id` in `unlock_condition`
fn relic_is_lower<'a>(guide: impl Iterator<Item = (i32, &'a [i32])>, unlocked: &[i32], id: i32) -> bool {
    guide.into_iter().any(|(other, condition)| condition.contains(&id) && unlocked.contains(&other))
}

/// Relic group gems for an instance with `slots` sockets: gems are stored for the whole group, lower qualities have fewer sockets.
/// Socket numbers from 1 (the `EnableSlots` order in WBP_Heirloom_Panel).
fn relic_gems_for_slots(gem: &tgf::DItemGem, slots: usize) -> tgf::DItemGem {
    let mut trimmed = gem.clone();
    trimmed.slot_data.retain(|s| s.slot_id >= 1 && s.slot_id as usize <= slots);
    trimmed
}

/// Season of the relic/binding data (official server: limitOption.season.seasonId = 1)
const SEASON_ID: i32 = 1;

/// Fallback list of parts (group 1 from gear_soul_bound_wear_part_random) if the table did not load
const SOUL_BOUND_ALL_PARTS: [i32; 10] = [1, 2, 13, 11, 12, 14, 15, 16, 17, 18];

/// Rejection: an EnErrorCode (to the log) and an explanation
struct ItemErr(E, String);

fn fail<T>(code: E, msg: impl Into<String>) -> Result<T, ItemErr> {
    Err(ItemErr(code, msg.into()))
}

trait OrCode<T> {
    fn or_code(self, code: E) -> Result<T, ItemErr>;
}

impl<T> OrCode<T> for Result<T, String> {
    fn or_code(self, code: E) -> Result<T, ItemErr> {
        self.map_err(|e| ItemErr(code, e))
    }
}

/// Accumulated over an action: items/coins and other dirty_* (heirloom, soul_bound, pages, recipes)
#[derive(Default)]
struct Out {
    ch: ItemChanges,
    extra: tgf::ScPlayerDirtyChgSync,
}

fn season() -> Option<tgf::DLimitOption> {
    Some(tgf::DLimitOption { season: Some(tgf::DLimitOptionSeason { season_id: SEASON_ID }) })
}

fn state_get<T: serde::de::DeserializeOwned + Default>(p: &PlayerModel, key: &str) -> T {
    serde_json::from_value(p.state[key].clone()).unwrap_or_default()
}

fn state_set<T: serde::Serialize>(p: &mut PlayerModel, key: &str, v: &T) {
    p.state[key] = serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
}

fn heirloom_get(p: &PlayerModel) -> tgf::DUserHeirloomData {
    let mut d: tgf::DUserHeirloomData = state_get(p, "item_heirloom");
    if d.limit_option.is_none() {
        d.limit_option = season();
    }
    d
}

fn heirloom_set(p: &mut PlayerModel, d: &tgf::DUserHeirloomData, out: &mut Out) {
    state_set(p, "item_heirloom", d);
    out.extra.dirty_heirloom_data = Some(tgf::DUserHeirloomDirty { data: vec![d.clone()] });
}

fn soul_bound_of(p: &PlayerModel) -> tgf::DUserSoulBound {
    let part = p.state["item_soul_bound"]["wear_part"].as_i64().unwrap_or(0) as i32;
    if part == 0 {
        return tgf::DUserSoulBound::default();
    }
    tgf::DUserSoulBound {
        soul_bound_data: vec![tgf::DUserSoulBoundSeason { battle_data: Some(tgf::DBattleSoulBound { bound_wear_part: part }), limit_option: season() }],
    }
}

fn crafting_slots(p: &PlayerModel) -> Vec<tgf::DCraftingSlotData> {
    state_get(p, "item_crafting")
}

/// The slot for the client at `now`: ready units — in completed_count (the client shows "Completed N/M" straight from
/// it). Since 02.11 crafting is instant, as in PVE, and uncompleted_count is always 0 — this is for old time-queued slots.
fn craft_wire(s: &tgf::DCraftingSlotData, now: i64) -> tgf::DCraftingSlotData {
    let fresh = craft_done(s, now);
    tgf::DCraftingSlotData {
        completed_count: s.completed_count + fresh,
        uncompleted_count: s.uncompleted_count - fresh,
        start_time: s.start_time + fresh as i64 * s.time_cost,
        ..s.clone()
    }
}


/// How many units of the slot are ready by time t (seconds): time_cost — seconds per unit
fn craft_done(slot: &tgf::DCraftingSlotData, t: i64) -> i32 {
    if slot.time_cost <= 0 {
        return slot.uncompleted_count;
    }
    ((t - slot.start_time) / slot.time_cost).clamp(0, slot.uncompleted_count as i64) as i32
}

/// Request time: the client's (recived_time/cancel_time) if it is not earlier than ours and ahead by no more than 5 s
fn action_time(client: i64) -> i64 {
    let now = GatewaySession::now_ts();
    if client > 0 {
        now.max(client.min(now + 5))
    } else {
        now
    }
}

/// Simple PRNG (rand is not a dependency): nanoseconds + salt
fn rand_u64(salt: u64) -> u64 {
    let mut x = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) ^ salt.rotate_left(29);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

/// Cost by the variant index (DConfigCostItem.index = choose_cost_id); no variants — the old unlock_cost field
fn choose_cost(costs: &[tgf::DConfigCostItem], legacy: &[tgf::DItemOffset], index: i32) -> Result<Vec<tgf::DItemOffset>, ItemErr> {
    if costs.is_empty() {
        return Ok(legacy.to_vec());
    }
    costs
        .iter()
        .find(|c| c.index == index)
        .or_else(|| if index == 0 { costs.first() } else { None })
        .map(|c| c.items.clone())
        .ok_or_else(|| ItemErr(E::EnErrItemCostConfigIndexNotFound, format!("no cost variant {}", index)))
}

fn offsets(items: &[tgf::DItemBasic]) -> Vec<tgf::DItemOffset> {
    items.iter().map(|b| tgf::DItemOffset { type_id: b.type_id, count: b.count.max(1), expire_offset: 0 }).collect()
}

fn scale(items: &[tgf::DItemOffset], n: i32) -> Vec<tgf::DItemOffset> {
    items.iter().map(|c| tgf::DItemOffset { count: c.count * n, ..c.clone() }).collect()
}

/// Summary of what was granted for the response: currency (guid 0) is summed by type, items — as they are (the last record per guid)
fn merge_rewards(added: &[tgf::DItem]) -> Vec<tgf::DItem> {
    let mut out: Vec<tgf::DItem> = vec![];
    for it in added {
        let g = inv::guid_of(it);
        let t = it.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
        let same = out.iter_mut().find(|o| if g == 0 { inv::guid_of(o) == 0 && o.basic.as_ref().map(|b| b.type_id) == Some(t) } else { inv::guid_of(o) == g });
        match same {
            Some(o) if g == 0 => {
                if let (Some(ob), Some(ib)) = (o.basic.as_mut(), it.basic.as_ref()) {
                    ob.count += ib.count;
                }
            }
            Some(o) => *o = it.clone(),
            None => out.push(it.clone()),
        }
    }
    out
}

impl GatewaySession {
    /// Load the player, perform the action; success — save, update the host loadout, send dirty.
    /// Rejection — a log with the code, the player is not saved, the default response.
    async fn item_action<R: Default>(&self, what: &str, f: impl FnOnce(&mut PlayerModel, &mut Out) -> Result<R, ItemErr>) -> Option<R> {
        let mut p = self.load_me().await;
        let before = p.clone();
        let mut out = Out::default();
        match f(&mut p, &mut out) {
            Ok(_) if super::raid_lock::raid_items_changed(&before, &p).is_some() => {
                // 16.11: the items of a character in a running match are changed only by the raid result (raid_lock.rs) — the player is not saved
                let role = super::raid_lock::raid_items_changed(&before, &p).unwrap_or_default();
                self.raid_lock_refuse(what, role);
                Some(R::default())
            }
            Ok(rsp) => {
                self.save_me(&p).await;
                Self::write_client_loadout_file(&p);
                let base = out.ch.to_dirty(&p);
                let mut dirty = out.extra;
                dirty.dirty_postion_action = base.dirty_postion_action;
                dirty.dirty_coin_data = base.dirty_coin_data;
                println!("[Item] {} {}: ok, items {} coins {}", p.open_id, what, dirty.dirty_postion_action.len(), dirty.dirty_coin_data.len());
                if dirty != tgf::ScPlayerDirtyChgSync::default() {
                    self.push_dirty(dirty);
                }
                Some(rsp)
            }
            Err(ItemErr(code, msg)) => {
                println!("[Item] {} {}: rejected {} ({}) — {}", p.open_id, what, code as i32, code.as_str_name(), msg);
                self.set_rpc_error(code as i32);
                Some(R::default())
            }
        }
    }

    fn item_max_pages(&self) -> i64 {
        self.stash_max_pages() // ext: stash_pages — season max_pages, otherwise const inventory_max_page
    }

    /// Add n stash pages (no more than inventory_max_page); new pages — into dirty_inventory_page
    fn item_add_pages(&self, p: &mut PlayerModel, n: i64, out: &mut Out) -> Result<(), ItemErr> {
        let (have, max) = (inv::stash_page_count(p), self.item_max_pages());
        if have >= max {
            return fail(E::EnErrItemInventoryAddPageFailedReachPageLimit, format!("pages {} of {}", have, max));
        }
        let now = (have + n.max(1)).min(max);
        p.state["inventory_page_count"] = serde_json::json!(now);
        out.extra.dirty_inventory_page.extend((have..now).map(|pg| inv::stash_page(p, pg)));
        Ok(())
    }

    fn item_use_action(&self, type_id: i32) -> Option<tgf::d_item_use_action::ActionType> {
        self.game_data.item_type.get(&type_id)?.use_action.as_ref()?.action_type.clone()
    }

    pub(super) fn item_is_gear(&self, type_id: i32) -> bool {
        self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == tgf::EnItemMajorType::Equipment as i32).unwrap_or(false)
    }

    pub(super) fn item_rarity(&self, type_id: i32) -> i32 {
        let ue_id = self.game_data.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|i| *i != 0).unwrap_or(type_id);
        self.game_data.uesource_itemtype.get(&ue_id).map(|u| u.rarity).unwrap_or(0)
    }

    /// The player's recipes: all from craft_group (as on the official server) + those unlocked by items (use_action recipe_id)
    fn item_recipes(&self, p: &PlayerModel) -> Vec<i32> {
        let mut r: Vec<i32> = self.game_data.craft_group.values().flat_map(|g| g.recipes.iter().copied()).collect();
        r.extend(state_get::<Vec<i32>>(p, "item_crafting_recipes"));
        r.sort_unstable();
        r.dedup();
        r
    }

    /// For player_get_info.user_crafting_data (account_handlers.rs): recipes and crafting slots
    #[allow(dead_code)]
    pub(super) fn item_crafting_data(&self, p: &PlayerModel) -> tgf::DUserCraftingData {
        let now = Self::now_ts();
        tgf::DUserCraftingData { recipe: self.item_recipes(p), crafting: crafting_slots(p).iter().map(|s| craft_wire(s, now)).collect() }
    }

    /// For player_get_info.heirloom_data (account_handlers.rs)
    #[allow(dead_code)]
    pub(super) fn item_heirloom_data(&self, p: &PlayerModel) -> tgf::DUserHeirloom {
        tgf::DUserHeirloom { data: vec![heirloom_get(p)] }
    }

    /// For player_get_info.user_soul_bound_data (account_handlers.rs)
    #[allow(dead_code)]
    pub(super) fn item_soul_bound_data(&self, p: &PlayerModel) -> tgf::DUserSoulBound {
        soul_bound_of(p)
    }

    /// Insurance price (08.11, as in the original with the user's normalization): the item's market price
    /// (`insurance_market_prices`) × the rarity percentage (`season::features().insurance`) / 100. 0 — cannot be insured
    /// (not gear, not on the market, grey/common). An example from the original: the legendary ring "Lament of the Fallen" on the market
    /// 0.8–1.2 million, insurance 166 617 ≈ 20%. The gear_insurance_item table (sp_rate) is a developer mistake, not used.
    pub(super) fn item_insurance_price(&self, type_id: i32, market_price: i64) -> i32 {
        if !self.item_is_gear(type_id) || market_price <= 0 {
            return 0;
        }
        let pct = crate::season::features().insurance.pct(self.item_rarity(type_id)) as i64;
        if pct == 0 {
            return 0;
        }
        (market_price * pct / 100).clamp(1, i32::MAX as i64) as i32
    }

    /// A new bound part: a random one from the group (1 — all parts), not the current one if possible
    fn item_roll_soul_bound(&self, p: &mut PlayerModel, group: i32, role_id: i64, out: &mut Out) {
        let mut parts: Vec<i32> =
            self.game_data.gear_soul_bound_wear_part_random_rows.iter().filter(|r| r.group_id == group).map(|r| r.wear_part).collect();
        if parts.is_empty() {
            parts = SOUL_BOUND_ALL_PARTS.to_vec();
        }
        let cur = p.state["item_soul_bound"]["wear_part"].as_i64().unwrap_or(0) as i32;
        if parts.len() > 1 {
            parts.retain(|w| *w != cur);
        }
        let part = parts[(rand_u64(p.user_id) % parts.len() as u64) as usize];
        let count = p.state["item_soul_bound"]["count"].as_i64().unwrap_or(0) + 1;
        p.state["item_soul_bound"] = serde_json::json!({ "wear_part": part, "role_id": role_id, "count": count });
        out.extra.dirty_soul_bound = Some(soul_bound_of(p));
    }

    /// Use an item (one DItemUseBase). The count — item_basic.count (0 → 1).
    fn item_use_one(&self, p: &mut PlayerModel, u: &tgf::DItemUseBase, out: &mut Out) -> Result<(), ItemErr> {
        use tgf::d_item_use_action::ActionType as A;
        let basic = u.item_basic.clone().unwrap_or_default();
        let Some(it) = inv::find_item_mut(p, basic.guid) else {
            return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", basic.guid));
        };
        let type_id = it.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
        let have = it.basic.as_ref().map(|b| b.count.max(1)).unwrap_or(1);
        let n = basic.count.clamp(1, have);
        let Some(action) = self.item_use_action(type_id) else {
            return fail(E::EnErrItemTypeNotSupportUse, format!("{} has no use_action", type_id));
        };
        let uses = match action {
            A::AddInventoryPage(k) => {
                let k = k.max(1) as i64;
                let free = self.item_max_pages() - inv::stash_page_count(p);
                let uses = (n as i64).min((free + k - 1) / k).max(1) as i32;
                self.item_add_pages(p, k * uses as i64, out)?;
                uses
            }
            A::RecipeId(r) => {
                let mut extra: Vec<i32> = state_get(p, "item_crafting_recipes");
                if !extra.contains(&r) {
                    extra.push(r);
                    state_set(p, "item_crafting_recipes", &extra);
                }
                out.extra.dirty_crafting = Some(tgf::DUserCraftingDirty { dirty_recipe: vec![r] });
                1
            }
            A::SoulBound(group) => {
                self.item_roll_soul_bound(p, group, inv::default_role_guid(p), out);
                1
            }
            // TODO(verify): appearances (facade_*), the random pool and energy are other domains, the grant format is unknown
            A::AddFacade(_) | A::RandomPool(_) | A::AddEnergy(_) => {
                return fail(E::EnErrItemOperationNotSupported, format!("{}: {:?} not supported", type_id, action));
            }
        };
        self.inv_pay(p, &[tgf::DItemOffset { type_id, count: uses, expire_offset: 0 }], &[basic], &mut out.ch).or_code(E::EnErrItemNotEnough)
    }

    pub(super) async fn inventory_page_add_impl(&self, req: tgf::CsInventoryPageAddReq) -> Option<tgf::ScInventoryPageAddRsp> {
        use tgf::d_item_use_action::ActionType as A;
        let cost = req.cost_for_add_page.unwrap_or_default();
        self.item_action("inventory_page_add", |p, out| {
            // paid by an item with use_action add_inventory_page (1032); without it — rejected (-636)
            let Some(A::AddInventoryPage(k)) = self.item_use_action(cost.type_id) else {
                return fail(E::EnErrItemInventoryAddPageFailedThisItemNotSupportToAddPage, format!("{} does not add pages", cost.type_id));
            };
            let n = cost.count.max(1);
            self.inv_pay(p, &[tgf::DItemOffset { type_id: cost.type_id, count: n, expire_offset: 0 }], &[cost.clone()], &mut out.ch)
                .or_code(E::EnErrItemInventoryAddPageFailedAddPageItemNotEnougph)?;
            self.item_add_pages(p, k.max(1) as i64 * n as i64, out)?;
            Ok(tgf::ScInventoryPageAddRsp {})
        })
        .await
    }

    pub(super) async fn use_item_impl(&self, req: tgf::CsItemUseReq) -> Option<tgf::ScItemUseRsp> {
        let data = req.data.unwrap_or_default();
        self.item_action("use_item", |p, out| {
            self.item_use_one(p, &data, out)?;
            Ok(tgf::ScItemUseRsp { got: merge_rewards(&out.ch.added) })
        })
        .await
    }

    /// Batch: all or nothing
    pub(super) async fn use_item_bath_impl(&self, req: tgf::CsBatchItemUseReq) -> Option<tgf::ScBatchItemUseRsp> {
        self.item_action("use_item_bath", |p, out| {
            for u in &req.data {
                self.item_use_one(p, u, out)?;
            }
            Ok(tgf::ScBatchItemUseRsp { got: merge_rewards(&out.ch.added) })
        })
        .await
    }

    /// Crafting start: charge item_cost × count, create a slot (one per recipe). Time — Unix seconds.
    pub(super) async fn crafting_impl(&self, req: tgf::CsCraftingReq) -> Option<tgf::ScCraftingRsp> {
        self.item_action("crafting", |p, out| {
            let Some(recipe) = self.game_data.craft_table.get(&req.recipe_id) else {
                return fail(E::EnErrItemNotFoundInTable, format!("no recipe {}", req.recipe_id));
            };
            if !self.item_recipes(p).contains(&req.recipe_id) {
                return fail(E::EnErrItemOperationNotSupported, format!("recipe {} is not unlocked", req.recipe_id));
            }
            // count_limt — per request, count_max_lmit — total in the recipe queue (99/99 in the table).
            // The recipe is already being crafted — add to its queue (the client does this: session 02.11, it was -4110 "already in progress")
            let limit = if recipe.count_limt > 0 { recipe.count_limt } else { 99 };
            let max = if recipe.count_max_lmit > 0 { recipe.count_max_lmit } else { 99 };
            if req.count <= 0 || req.count > limit {
                return fail(E::EnErrCrafringSlotQueueCraftingNumInvalid, format!("count {} (limit {})", req.count, limit));
            }
            let mut slots = crafting_slots(p);
            let queued = slots.iter().find(|s| s.recipe_id == req.recipe_id).map(|s| s.uncompleted_count + s.completed_count).unwrap_or(0);
            if queued + req.count > max {
                return fail(E::EnErrCrafringSlotQueueCraftingNumInvalid, format!("in queue {} + {} > {}", queued, req.count, max));
            }
            self.inv_pay(p, &scale(&recipe.item_cost, req.count), &req.cost_items, &mut out.ch).or_code(E::EnErrCraftingItemNotEnough)?;
            // As PVE SoloLobby (rpc_dump/PVE.jsonl 02.11 08:18): crafting is instant — count goes straight to completed_count,
            // uncompleted 0, start_time — of the first order in the slot, time_cost from the table; no readiness pushes
            // (dirty — only the charged materials). A repeated order of the same recipe is added to the ready ones.
            let now = Self::now_ts();
            let slot = match slots.iter_mut().find(|s| s.recipe_id == req.recipe_id) {
                Some(s) => {
                    s.completed_count += craft_done(s, now) + req.count;
                    s.uncompleted_count = 0;
                    s.clone()
                }
                None => {
                    let s = tgf::DCraftingSlotData {
                        recipe_id: req.recipe_id,
                        completed_count: req.count,
                        uncompleted_count: 0,
                        start_time: now,
                        time_cost: recipe.time_cost.max(0) as i64,
                    };
                    slots.push(s.clone());
                    s
                }
            };
            state_set(p, "item_crafting", &slots);
            Ok(tgf::ScCraftingRsp { data: Some(craft_wire(&slot, now)) })
        })
        .await
    }

    /// Where to put the crafting result: storage_type (0 → stash) + page/role from the first position of req.items
    fn item_craft_target(p: &PlayerModel, storage_type: i32, items: &[tgf::DItemBasic]) -> Result<tgf::DItemPosition, ItemErr> {
        let st = if storage_type == 0 { 1 } else { storage_type };
        let hint = items.iter().filter_map(|b| b.position.clone()).find(|pos| pos.r#type == st % 100);
        let mut place = inv::storage_place(p, st, hint.as_ref().map(|h| h.guid).unwrap_or(0))
            .ok_or_else(|| ItemErr(E::EnErrCraftingStorageTypeInvalid, format!("storage_type {}", storage_type)))?;
        if let Some(h) = hint.filter(|_| st < 100) {
            place.x = h.x;
            place.y = h.y;
        }
        Ok(place)
    }

    /// Grant the recipe output for `done` units (great success: great_success_rate of 10000 per unit,
    /// TODO(verify) the units; the rate is 0 in all recipes of the table)
    fn item_give_products(&self, p: &mut PlayerModel, recipe: &tgf::config::ExcelCraftingRecipe, done: i32, target: &tgf::DItemPosition, out: &mut Out) -> Result<(), ItemErr> {
        let mut mult = done;
        if recipe.great_success_rate > 0 && recipe.great_success_multiple > 1 {
            for i in 0..done {
                if (rand_u64(i as u64 + 7) % 10000) < recipe.great_success_rate as u64 {
                    mult += recipe.great_success_multiple - 1;
                }
            }
        }
        for prod in &recipe.product {
            self.inv_give(p, prod.type_id, prod.count * mult, target, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
        }
        Ok(())
    }

    /// Collect the ready ones: ready units → output into storage_type; the slot continues with the remainder (start shifts
    /// by the time of the ready ones), an empty slot is deleted
    pub(super) async fn crafting_recived_impl(&self, req: tgf::CsCraftingRecivedReq) -> Option<tgf::ScCraftingRecivedRsp> {
        // +1 s: time in seconds rounded down on both sides — the client may see readiness a fraction of a second earlier
        let t = action_time(req.recived_time) + 1;
        self.item_action("crafting_recived", |p, out| {
            let mut slots = crafting_slots(p);
            let Some(i) = slots.iter().position(|s| s.recipe_id == req.recipe_id) else {
                return fail(E::EnErrCraftingReceivedItemInvalid, format!("no slot for recipe {}", req.recipe_id));
            };
            let Some(recipe) = self.game_data.craft_table.get(&req.recipe_id) else {
                return fail(E::EnErrCraftingConfigChange, format!("recipe {} is not in the table", req.recipe_id));
            };
            let fresh = craft_done(&slots[i], t);
            let done = slots[i].completed_count + fresh;
            if done <= 0 {
                return fail(E::EnErrCraftingUncompleted, format!("recipe {}: ready 0 of {}", req.recipe_id, slots[i].uncompleted_count));
            }
            let target = Self::item_craft_target(p, req.storage_type, &req.items)?;
            self.item_give_products(p, recipe, done, &target, out)?;
            let s = &mut slots[i];
            s.uncompleted_count -= fresh;
            s.completed_count = 0;
            s.start_time += fresh as i64 * s.time_cost;
            if s.uncompleted_count <= 0 {
                slots.remove(i);
            }
            state_set(p, "item_crafting", &slots);
            // as in PVE: the response has only type_id and count, where it lies — in dirty_postion_action
            let item = merge_rewards(&out.ch.added)
                .into_iter()
                .map(|it| {
                    let b = it.basic.unwrap_or_default();
                    tgf::DItem { basic: Some(tgf::DItemBasic { type_id: b.type_id, count: b.count, ..Default::default() }), ..Default::default() }
                })
                .collect();
            Ok(tgf::ScCraftingRecivedRsp { item, recipe_id: req.recipe_id })
        })
        .await
    }

    /// Cancel: the ready ones are granted as output, materials are returned for the unfinished ones; the slot is deleted
    pub(super) async fn crafting_cancel_impl(&self, req: tgf::CsCraftingCancelReq) -> Option<tgf::ScCraftingCancelRsp> {
        let t = action_time(req.cancel_time);
        self.item_action("crafting_cancel", |p, out| {
            let mut slots = crafting_slots(p);
            let Some(i) = slots.iter().position(|s| s.recipe_id == req.recipe_id) else {
                return fail(E::EnErrCraftingReceivedItemInvalid, format!("no slot for recipe {}", req.recipe_id));
            };
            let slot = slots.remove(i);
            let target = Self::item_craft_target(p, req.storage_type, &req.items)?;
            if let Some(recipe) = self.game_data.craft_table.get(&req.recipe_id) {
                let fresh = craft_done(&slot, t);
                let done = slot.completed_count + fresh;
                let left = slot.uncompleted_count - fresh;
                self.item_give_products(p, recipe, done, &target, out)?;
                for c in scale(&recipe.item_cost, left) {
                    self.inv_give(p, c.type_id, c.count, &target, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
                }
            }
            state_set(p, "item_crafting", &slots);
            Ok(tgf::ScCraftingCancelRsp { item: merge_rewards(&out.ch.added), recipe_id: req.recipe_id })
        })
        .await
    }

    /// Repair: loss_durability -= durability_count, cost — the client's cost_item (gear_durability is empty in the tables;
    /// materials repair_material_ids 1052/1037/1038/1039 — currency). TODO(verify): check the cost once there is a table.
    pub(super) async fn gear_repair_impl(&self, req: tgf::CsGearRepairReq) -> Option<tgf::ScGearRepairRsp> {
        self.item_action("gear_repair", |p, out| {
            for r in &req.data {
                let guid = r.gear.as_ref().map(|b| b.guid).unwrap_or(0);
                let Some(it) = inv::find_item_mut(p, guid) else {
                    return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", guid));
                };
                let loss = it.data.as_ref().map(|d| d.loss_durability).unwrap_or(0);
                if r.durability_count <= 0 || loss <= 0 {
                    return fail(E::EnErrGearRepairDurabilityInvalid, format!("guid={}: repair {} with wear {}", guid, r.durability_count, loss));
                }
                self.inv_pay(p, &offsets(&r.cost_item), &r.cost_item, &mut out.ch).or_code(E::EnErrGearRepairCostNotMatch)?;
                let it = inv::find_item_mut(p, guid).unwrap();
                let d = it.data.get_or_insert_with(Default::default);
                d.loss_durability = (loss - r.durability_count).max(0);
                out.ch.dirty.extend(inv::changed(it));
            }
            Ok(tgf::ScGearRepairRsp {})
        })
        .await
    }

    /// Insert a gem (an unlocked gem_guide) into socket slot_id of gear; gem_id in DItemGemSlot — the gem item
    /// (gear_gem_guide.inst_id, like the client's EquippedGemItemMap). gem_guide_id 0 — remove.
    /// TODO(verify): socket shape compatibility (gear_gem_inst.shape_type) is not checked.
    pub(super) async fn gear_inlaid_impl(&self, req: tgf::CsGearInlaidReq) -> Option<tgf::ScGearInlaidRsp> {
        let guid = req.gear.as_ref().map(|b| b.guid).unwrap_or(0);
        self.item_action("gear_inlaid", |p, out| {
            let gem_item = if req.gem_guide_id == 0 {
                0
            } else {
                let Some(g) = self.game_data.gear_gem_guide.get(&req.gem_guide_id) else {
                    return fail(E::EnErrGearGemNotFound, format!("no gem {}", req.gem_guide_id));
                };
                if !heirloom_get(p).unlock_gem_id.contains(&req.gem_guide_id) {
                    return fail(E::EnErrGearGemNotUnlock, format!("gem {} is not unlocked", req.gem_guide_id));
                }
                if g.inst_id != 0 { g.inst_id } else { req.gem_guide_id }
            };
            let Some(it) = inv::find_item_mut(p, guid) else {
                return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", guid));
            };
            let gem = it.data.get_or_insert_with(Default::default).gem_data.get_or_insert_with(Default::default);
            if gem_item != 0 && gem.slot_data.iter().any(|s| s.gem_id == gem_item && s.slot_id != req.slot_id) {
                return fail(E::EnErrGearGemInlaidDuplicates, format!("gem {} is already inserted", gem_item));
            }
            gem.slot_data.retain(|s| s.slot_id != req.slot_id);
            if gem_item != 0 {
                gem.slot_data.push(tgf::DItemGemSlot { slot_id: req.slot_id, gem_id: gem_item, affix_location: req.location });
                gem.slot_data.sort_by_key(|s| s.slot_id);
            }
            out.ch.dirty.extend(inv::changed(it));
            Ok(tgf::ScGearInlaidRsp {})
        })
        .await
    }

    /// Dismantling: the item is deleted, the gear_dismantle.reward (currency) — as coins, items — to the stash
    /// GM `item`: grant an item the same way as a reward (inv_give: a free cell for the item size, filling up
    /// stacks, currency — into coins), with the usual saving and dirty. Ok — where the items ended up.
    /// GM `itemaff`: one item to the stash with the given affixes (`gm_affix_data`). The reply — the place and the affix list.
    pub(super) async fn item_gm_give_affixed(&self, type_id: i32, wishes: &[(i32, i32)], max_all: bool) -> Result<Vec<String>, String> {
        if !self.inv_is_gear(type_id) {
            return Err(format!("{} — not gear", type_id));
        }
        let affix = self.gm_affix_data(type_id, self.inv_gear_wear_part(type_id), Self::inv_affix_seed(type_id), wishes, max_all)?;
        let lines = self.gm_affix_lines(&affix);
        let r = self
            .item_action("gm_item_affix", |p, out| {
                let mut item = self.inv_new_item(p, type_id, 1);
                if let Some(d) = item.data.as_mut() {
                    d.affix_data = affix;
                }
                let stash = tgf::DItemPosition { r#type: STASH, ..Default::default() };
                let placed = self.inv_place_item(p, item, &stash, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
                out.ch.added.push(placed.clone());
                Ok(Some(placed))
            })
            .await
            .flatten();
        let Some(placed) = r else { return Err(format!("could not grant {} (no room in the stash, see the [Item] log)", type_id)) };
        let b = placed.basic.unwrap_or_default();
        let pos = b.position.unwrap_or_default();
        println!("[GM] {}: {} guid {} with affixes {:?}", self.open_id, type_id, b.guid, lines);
        Ok(std::iter::once(format!("{} → stash p.{} ({},{}), guid {}", type_id, pos.guid + 1, pos.x, pos.y, b.guid)).chain(lines).collect())
    }

    pub(super) async fn item_gm_give(&self, type_id: i32, count: i32) -> Result<Vec<String>, String> {
        let r = self
            .item_action("gm_item", |p, out| {
                let stash = tgf::DItemPosition { r#type: STASH, ..Default::default() };
                self.inv_give(p, type_id, count, &stash, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
                Ok(Some(out.ch.added.clone()))
            })
            .await
            .flatten();
        let Some(added) = r else { return Err(format!("could not grant {} x{} (no room or an error, see the [Item] log)", type_id, count)) };
        Ok(added
            .iter()
            .filter_map(|it| it.basic.as_ref())
            .map(|b| match b.position.as_ref() {
                Some(pos) if b.guid != 0 => format!("{} x{} → stash p.{} ({},{})", b.type_id, b.count, pos.guid + 1, pos.x, pos.y),
                _ => format!("{} x{} → currency", b.type_id, b.count),
            })
            .collect())
    }

    pub(super) async fn gear_dismantle_impl(&self, req: tgf::CsGearDismantleReq) -> Option<tgf::ScGearDismantleRsp> {
        self.item_action("gear_dismantle", |p, out| {
            let role = inv::default_role_guid(p) as u64;
            let mut quests = false;
            for b in &req.gear {
                let Some(item) = inv::take(p, b.guid) else {
                    return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", b.guid));
                };
                let type_id = item.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
                let Some(cfg) = self.game_data.gear_dismantle.get(&type_id) else {
                    return fail(E::EnErrItemOperationNotSupported, format!("{} cannot be dismantled", type_id));
                };
                out.ch.dirty.push(inv::removal(&item));
                let stash = tgf::DItemPosition { r#type: STASH, ..Default::default() };
                for r in &cfg.reward {
                    self.inv_give(p, r.type_id, r.count, &stash, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
                }
                // quests (Q1): "dismantle N items of uncommon or higher" — ITEM_CONSUME + EN_ICST_GEAR_DISMANTLE
                let mut ev = QuestEvent::new(tgf::EnQuestProgressType::ItemConsume, 1, role);
                ev.item_type = Some(type_id);
                ev.item_chg = Some(tgf::EnItemChgSubType::EnIcstGearDismantle as i32);
                quests |= self.quest_event(p, &ev);
            }
            if quests {
                self.push_dirty(self.quest_dirty(p));
            }
            Ok(tgf::ScGearDismantleRsp { reward: merge_rewards(&out.ch.added) })
        })
        .await
    }

    /// Soul binding: a random gear part (group 1 gear_soul_bound_wear_part_random), the cost —
    /// the gear_soul_bound_cost row count = current_count + 1 (past the last one — the last one; 1..3 free).
    /// TODO(verify): what current_count is (the attempt number?), whether the binding is per account or per role_id.
    pub(super) async fn gear_soul_bound_impl(&self, req: tgf::CsGearSoulBoundReq) -> Option<tgf::ScGearSoulBoundRsp> {
        self.item_action("gear_soul_bound", |p, out| {
            if req.current_count < 0 {
                return fail(E::EnErrGearSoulBoundCountInvalid, format!("current_count {}", req.current_count));
            }
            let rows = &self.game_data.gear_soul_bound_cost;
            let row = rows.iter().find(|r| r.count == req.current_count + 1).or_else(|| rows.iter().max_by_key(|r| r.count));
            let cost = row.map(|r| r.cost_item.clone()).unwrap_or_default();
            self.inv_pay(p, &cost, &req.cost_item, &mut out.ch).or_code(E::EnErrItemNotEnough)?;
            let role = if req.role_id != 0 { req.role_id } else { inv::default_role_guid(p) };
            self.item_roll_soul_bound(p, 1, role, out);
            Ok(tgf::ScGearSoulBoundRsp {})
        })
        .await
    }

    /// Unlock a relic: unlock_condition conditions (the previous ones unlocked), payment unlock_costs[choose_cost_id]
    pub(super) async fn gear_heirloom_unlock_impl(&self, req: tgf::CsGearHeirloomUnlockReq) -> Option<tgf::ScGearHeirloomUnlockRsp> {
        self.item_action("gear_heirloom_unlock", |p, out| {
            let Some(g) = self.game_data.gear_heirloom_guide.get(&req.heirloom_id) else {
                return fail(E::EnErrGearHeirloomNotFound, format!("no relic {}", req.heirloom_id));
            };
            let mut h = heirloom_get(p);
            if h.unlock_heirloom_id.contains(&req.heirloom_id) {
                return fail(E::EnErrGearHeirloomAlreadyUnlock, format!("{} is already unlocked", req.heirloom_id));
            }
            if let Some(miss) = g.unlock_condition.iter().find(|c| !h.unlock_heirloom_id.contains(c)) {
                return fail(E::EnErrGearHeirloomUnlockConditionNotMatch, format!("first {}", miss));
            }
            let cost = choose_cost(&g.unlock_costs, &g.unlock_cost, req.choose_cost_id)?;
            self.inv_pay(p, &cost, &req.cost_item, &mut out.ch).or_code(E::EnErrItemNotEnough)?;
            h.unlock_heirloom_id.push(req.heirloom_id);
            heirloom_set(p, &h, out);
            Ok(tgf::ScGearHeirloomUnlockRsp { heirloom_id: req.heirloom_id })
        })
        .await
    }

    /// Remove the relic instance lying at new_position. TODO(verify): the meaning of the RPC (reset/destruction of the instance?)
    pub(super) async fn gear_heirloom_unload_impl(&self, req: tgf::CsGearHeirloomUnloadReq) -> Option<tgf::ScGearHeirloomUnloadRsp> {
        let pos = req.new_position.unwrap_or_default();
        self.item_action("gear_heirloom_unload", |p, out| {
            let Some(guid) = inv::occupant(p, &pos) else {
                return fail(E::EnErrItemPositionNotFound, format!("empty at type={} guid={} x={} y={}", pos.r#type, pos.guid, pos.x, pos.y));
            };
            let item = inv::take(p, guid).unwrap();
            let type_id = item.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
            let is_heirloom = matches!(item.create_info.as_ref().and_then(|c| c.create_data.as_ref()), Some(tgf::d_item_create_info::CreateData::Heirloom(_)))
                || self.game_data.gear_heirloom_guide.values().any(|g| g.inst_id == type_id);
            if !is_heirloom {
                return fail(E::EnErrRoleDestroyItemOnlyAvailableForHeirloom, format!("{} is not a relic", type_id));
            }
            out.ch.dirty.push(inv::removal(&item));
            Ok(tgf::ScGearHeirloomUnloadRsp {})
        })
        .await
    }

    /// Create an instance of an unlocked relic (gear_heirloom_guide.inst_id) at position (stash/bag; taken —
    /// the first free cell there) for instantiation_costs[choose_cost_id]. Gems — from the group's heirloom_gem_data.
    pub(super) async fn gear_heirloom_instantiation_impl(&self, req: tgf::CsGearHeirloomInstantiationReq) -> Option<tgf::ScGearHeirloomInstantiationRsp> {
        let want = req.position.clone().unwrap_or_default();
        self.item_action("gear_heirloom_instantiation", |p, out| {
            let Some(g) = self.game_data.gear_heirloom_guide.get(&req.heirloom_id) else {
                return fail(E::EnErrGearHeirloomNotFound, format!("no relic {}", req.heirloom_id));
            };
            let h = heirloom_get(p);
            if !h.unlock_heirloom_id.contains(&req.heirloom_id) {
                return fail(E::EnErrGearHeirloomLock, format!("{} is not unlocked", req.heirloom_id));
            }
            let lower = relic_is_lower(self.game_data.gear_heirloom_guide.values().map(|r| (r.id, r.unlock_condition.as_slice())), &h.unlock_heirloom_id, req.heirloom_id);
            if lower && !crate::season::features().relic_lower_quality {
                return fail(E::EnErrGearHeirloomLock, format!("{}: a higher quality is unlocked, lower ones are disabled", req.heirloom_id));
            }
            let inst = if g.inst_id != 0 { g.inst_id } else { req.heirloom_id };
            let mut owned = false;
            for place in inv::all_places(p) {
                if let Some(list) = inv::container_mut(p, &place) {
                    owned |= list.iter().any(|it| {
                        it.basic.as_ref().map(|b| b.type_id) == Some(inst)
                            && matches!(it.create_info.as_ref().and_then(|c| c.create_data.as_ref()), Some(tgf::d_item_create_info::CreateData::Heirloom(_)))
                    });
                }
            }
            if owned {
                return fail(E::EnErrGearHeirloomSameIdInst, format!("instance {} already exists", inst));
            }
            if want.r#type == WEARING {
                return fail(E::EnErrGearHeirloomCheckInstFailed, "creating directly into worn gear is not supported");
            }
            let Some(mut target) = inv::storage_place(p, if want.r#type == 0 { 1 } else { want.r#type }, want.guid) else {
                return fail(E::EnErrGearHeirloomCheckInstFailed, format!("place type={}", want.r#type));
            };
            target.x = want.x;
            target.y = want.y;
            let cost = choose_cost(&g.instantiation_costs, &[], req.choose_cost_id)?;
            self.inv_pay(p, &cost, &req.cost_item, &mut out.ch).or_code(E::EnErrItemNotEnough)?;
            let mut item = self.inv_new_item(p, inst, 1);
            if let Some(gem) = h.heirloom_gem_data.get(&g.group_id) {
                item.data.get_or_insert_with(Default::default).gem_data = Some(relic_gems_for_slots(gem, g.display_slot.len()));
            }
            item.create_info = Some(tgf::DItemCreateInfo {
                create_data: Some(tgf::d_item_create_info::CreateData::Heirloom(tgf::DItemCreateInfoHeirloom {
                    owner: Some(tgf::DPlayerIdKey { user_id: p.user_id, zone_id: 0 }),
                })),
                ..Default::default()
            });
            let placed = self.inv_place_item(p, item, &target, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
            Ok(tgf::ScGearHeirloomInstantiationRsp { heirloom_id: req.heirloom_id, position: placed.basic.and_then(|b| b.position) })
        })
        .await
    }

    /// Unlock a gem gear_gem_guide for unlock_costs[choose_cost_id] → unlock_gem_id
    pub(super) async fn gear_gem_unlock_impl(&self, req: tgf::CsGearGemUnlockReq) -> Option<tgf::ScGearGemUnlockRsp> {
        self.item_action("gear_gem_unlock", |p, out| {
            let Some(g) = self.game_data.gear_gem_guide.get(&req.gem_id) else {
                return fail(E::EnErrGearGemNotFound, format!("no gem {}", req.gem_id));
            };
            let mut h = heirloom_get(p);
            if h.unlock_gem_id.contains(&req.gem_id) {
                return fail(E::EnErrGearGemAlreadyUnlock, format!("{} is already unlocked", req.gem_id));
            }
            let cost = choose_cost(&g.unlock_costs, &g.unlock_cost, req.choose_cost_id)?;
            self.inv_pay(p, &cost, &req.cost_item, &mut out.ch).or_code(E::EnErrItemNotEnough)?;
            h.unlock_gem_id.push(req.gem_id);
            heirloom_set(p, &h, out);
            Ok(tgf::ScGearGemUnlockRsp { gem_id: req.gem_id })
        })
        .await
    }

    /// A gem into a relic socket (for the whole group heirloom_group_id): heirloom_gem_data[group].slot_data
    pub(super) async fn gear_heirloom_guide_inlaid_gem_impl(&self, req: tgf::CsGearHeirloomGuideInlaidGemReq) -> Option<tgf::ScGearHeirloomGuideInlaidGemRsp> {
        self.item_action("gear_heirloom_guide_inlaid_gem", |p, out| {
            let Some(g) = self.game_data.gear_heirloom_guide.get(&req.heirloom_id) else {
                return fail(E::EnErrGearHeirloomNotFound, format!("no relic {}", req.heirloom_id));
            };
            let mut h = heirloom_get(p);
            if !h.unlock_heirloom_id.contains(&req.heirloom_id) {
                return fail(E::EnErrGearHeirloomLock, format!("{} is not unlocked", req.heirloom_id));
            }
            let gem_item = if req.gem_guide_id == 0 {
                0
            } else {
                let Some(gg) = self.game_data.gear_gem_guide.get(&req.gem_guide_id) else {
                    return fail(E::EnErrGearGemNotFound, format!("no gem {}", req.gem_guide_id));
                };
                if !h.unlock_gem_id.contains(&req.gem_guide_id) {
                    return fail(E::EnErrGearGemNotUnlock, format!("gem {} is not unlocked", req.gem_guide_id));
                }
                if gg.inst_id != 0 { gg.inst_id } else { req.gem_guide_id }
            };
            let gem = h.heirloom_gem_data.entry(g.group_id).or_default();
            if gem_item != 0 && gem.slot_data.iter().any(|s| s.gem_id == gem_item && s.slot_id != req.slot_id) {
                return fail(E::EnErrGearGemInlaidDuplicates, format!("gem {} is already inserted", gem_item));
            }
            gem.slot_data.retain(|s| s.slot_id != req.slot_id);
            if gem_item != 0 {
                gem.slot_data.push(tgf::DItemGemSlot { slot_id: req.slot_id, gem_id: gem_item, affix_location: req.location });
                gem.slot_data.sort_by_key(|s| s.slot_id);
            }
            let gem = gem.clone();
            heirloom_set(p, &h, out);
            Ok(tgf::ScGearHeirloomGuideInlaidGemRsp { gem_data: Some(gem), heirloom_group_id: g.group_id })
        })
        .await
    }

    pub(super) async fn gear_heirloom_guide_inlaid_clear_impl(&self, req: tgf::CsGearHeirloomGuideInlaidClearReq) -> Option<tgf::ScGearHeirloomGuideInlaidClearRsp> {
        self.item_action("gear_heirloom_guide_inlaid_clear", |p, out| {
            let mut h = heirloom_get(p);
            h.heirloom_gem_data.remove(&req.heirloom_group_id);
            heirloom_set(p, &h, out);
            Ok(tgf::ScGearHeirloomGuideInlaidClearRsp {})
        })
        .await
    }

    /// Insure: price item_insurance_price (the client gets it from gear_insurance_price), paid in gold,
    /// into the item — DItemData.insurance {active, value = price, cost_item = gold}
    pub(super) async fn gear_insurance_lock_impl(&self, req: tgf::CsGearInsuranceLockReq) -> Option<tgf::ScGearInsuranceLockRsp> {
        let guid = req.item.as_ref().map(|b| b.guid).unwrap_or(0);
        // the item type — before the action: the market price is read from the trade book (async)
        let type_id = match self.db.get::<PlayerModel>("players", &self.clean_open_id()).await {
            Some(mut p) => inv::find_item_mut(&mut p, guid).and_then(|i| i.basic.as_ref().map(|b| b.type_id)).unwrap_or(0),
            None => 0,
        };
        let market = self.insurance_market_prices(&[type_id]).await.get(&type_id).copied().unwrap_or(0);
        self.item_action("gear_insurance_lock", |p, out| {
            let Some(it) = inv::find_item_mut(p, guid) else {
                return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", guid));
            };
            let type_id = it.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
            if !self.item_is_gear(type_id) {
                return fail(E::EnErrGearInsuranceNotGear, format!("{} is not gear", type_id));
            }
            if it.data.as_ref().and_then(|d| d.insurance.as_ref()).map(|i| i.active).unwrap_or(false) {
                return fail(E::EnErrGearInsuranceRepeated, format!("guid={} is already insured", guid));
            }
            // one insured item per character (as in the original: InventoryContext:GetInsuranceItemNum counts the
            // character's items, the client writes "InsuranceItemNum too much"). 18.11 (owner's item 4): the check was per
            // account — an insured item on one character gave -3845 on every other character
            let pos = it_pos(p, guid);
            let role = item_role(p, pos);
            if let Some(other) = insured_guids(p, role).into_iter().find(|g| *g != guid) {
                return fail(E::EnErrGearInsuranceRepeated, format!("another item is already insured guid={}", other));
            }
            let price = self.item_insurance_price(type_id, market);
            if price <= 0 {
                return fail(E::EnErrGearInsurancePriceZero, format!("{}: no price", type_id));
            }
            if req.price != 0 && req.price != price {
                return fail(E::EnErrGearInsurancePriceNotMatch, format!("client {} ≠ {}", req.price, price));
            }
            self.inv_pay(p, &[tgf::DItemOffset { type_id: common::GOLD, count: price, expire_offset: 0 }], &[], &mut out.ch).or_code(E::EnErrVirtualItemCoinNotEnough)?;
            let it = inv::find_item_mut(p, guid).unwrap();
            it.data.get_or_insert_with(Default::default).insurance =
                Some(tgf::DItemInsurance { active: true, value: price, cost_item: Some(tgf::DItemBasic { type_id: common::GOLD, count: price, ..Default::default() }) });
            out.ch.dirty.extend(inv::changed(it));
            Ok(tgf::ScGearInsuranceLockRsp {})
        })
        .await
    }

    /// Remove insurance: the payment (cost_item) is returned. TODO(verify): whether the official server returns the money.
    pub(super) async fn gear_insurance_unlock_impl(&self, req: tgf::CsGearInsuranceUnLockReq) -> Option<tgf::ScGearInsuranceUnLockRsp> {
        let guid = req.item.as_ref().map(|b| b.guid).unwrap_or(0);
        self.item_action("gear_insurance_unlock", |p, out| {
            let Some(it) = inv::find_item_mut(p, guid) else {
                return fail(E::EnErrItemGuidInvalid, format!("no item guid={}", guid));
            };
            let Some(ins) = it.data.as_mut().and_then(|d| d.insurance.take()).filter(|i| i.active) else {
                return fail(E::EnErrGearInsuranceNotLock, format!("guid={} is not insured", guid));
            };
            out.ch.dirty.extend(inv::changed(it));
            if let Some(c) = ins.cost_item.filter(|c| c.count > 0) {
                let stash = tgf::DItemPosition { r#type: STASH, ..Default::default() };
                self.inv_give(p, c.type_id, c.count, &stash, &mut out.ch).or_code(E::EnErrItemSuitablePositionNotFound)?;
            }
            Ok(tgf::ScGearInsuranceUnLockRsp {})
        })
        .await
    }

    /// Insurance prices by type_id (read only)
    pub(super) async fn gear_insurance_price_impl(&self, req: tgf::CsGearInsurancePriceReq) -> Option<tgf::ScGearInsurancePriceRsp> {
        let market = self.insurance_market_prices(&req.item_id).await;
        Some(tgf::ScGearInsurancePriceRsp {
            data: req
                .item_id
                .iter()
                .map(|&item_id| tgf::DGearInsurancePriceData { item_id, price: self.item_insurance_price(item_id, market.get(&item_id).copied().unwrap_or(0)) })
                .collect(),
        })
    }
}

/// Position of item `guid` on the player (default — not found)
fn it_pos(p: &mut PlayerModel, guid: i64) -> tgf::DItemPosition {
    inv::find_item_mut(p, guid).map(|i| inv::pos_of(i)).unwrap_or_default()
}

/// The character an item belongs to: bags and worn — position guid = role; the account stash — the current character
fn item_role(p: &PlayerModel, pos: tgf::DItemPosition) -> i64 {
    match pos.r#type {
        inv::BACKPACK | inv::WEARING | inv::SMALL_BACKPACK => pos.guid,
        _ => p.hall_role_guid as i64,
    }
}

/// guids of items with active insurance on character `role` (its bags and worn; the shared stash is not counted)
pub(super) fn insured_guids(p: &mut PlayerModel, role: i64) -> Vec<i64> {
    let mut out = vec![];
    for place in inv::all_places(p).into_iter().filter(|pl| pl.r#type != inv::STASH && pl.guid == role) {
        if let Some(list) = inv::container_mut(p, &place) {
            for it in list.iter() {
                if it.data.as_ref().and_then(|d| d.insurance.as_ref()).is_some_and(|i| i.active) {
                    out.push(inv::guid_of(it));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {

    /// 08.11: insurance from the market price × the rarity percentage (legendary 20%: the ring "Lament of the Fallen" 1 million -> 200 000)
    #[tokio::test]
    async fn insurance_price_by_market_and_rarity() {
        let dir = format!("target/test_ins_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "ins".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        assert_eq!(s.item_insurance_price(185015002, 1_000_000), 200_000, "legendary 20%");
        assert_eq!(s.item_insurance_price(185015002, 0), 0, "no market price");
        assert_eq!(s.item_insurance_price(181010000, 5_000), 0, "grey is not insured");
        assert_eq!(s.item_insurance_price(100013, 5_000), 0, "not gear");
        // without deals — price_def × 3: ring 180 000 -> 540 000
        let m = s.insurance_market_prices(&[185015002]).await;
        assert_eq!(m[&185015002], 540_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 18.11 (owner's item 4, -3845 EnErrGearInsuranceRepeated): one insured item per character, not per account —
    /// an insured ring on character A does not block insurance on character B; a second item on the same character — -3845
    #[tokio::test]
    async fn insurance_one_item_per_character() {
        let dir = format!("target/test_ins_role_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "insrole".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let ring = 185015002; // legendary ring, gear: insurance 108 000 without deals
        let worn = |guid: i64, role: u64, slot: i32| tgf::DItem {
            basic: Some(tgf::DItemBasic {
                type_id: ring,
                count: 1,
                guid,
                position: Some(tgf::DItemPosition { r#type: inv::WEARING, guid: role as i64, x: slot, ..Default::default() }),
                ..Default::default()
            }),
            data: Some(Default::default()),
            create_info: None,
        };
        let role = |role_guid: u64, items: Vec<tgf::DItem>| crate::structures::RoleModel {
            role_guid,
            ord: role_guid as i32,
            type_id: 400006,
            role_name: format!("Char{}", role_guid),
            role_level: 1,
            data: tgf::DUserRole {
                deploy_role: Some(tgf::DBattleDeployRole { wearing: Some(tgf::DRoleWearingData { items, ..Default::default() }), ..Default::default() }),
                ..Default::default()
            },
        };
        let mut p = PlayerModel::new("insrole", 10001);
        p.state["gold"] = serde_json::json!(10_000_000);
        p.hall_role_guid = 2;
        p.roles = vec![role(1, vec![worn(101, 1, 9)]), role(2, vec![worn(201, 2, 9), worn(202, 2, 10)])];
        s.db.save("players", "insrole", &p).await;
        let lock = |guid: i64| tgf::CsGearInsuranceLockReq { item: Some(tgf::DItemBasic { guid, ..Default::default() }), price: 0 };
        let err = || s.rpc_error.swap(0, std::sync::atomic::Ordering::Relaxed);
        let insured = |p: &mut PlayerModel, g: i64| {
            inv::find_item_mut(p, g).and_then(|i| i.data.as_ref()).and_then(|d| d.insurance.as_ref()).is_some_and(|i| i.active)
        };

        s.gear_insurance_lock_impl(lock(101)).await;
        assert_eq!(err(), 0, "character 1: first insurance");
        s.gear_insurance_lock_impl(lock(201)).await;
        assert_eq!(err(), 0, "character 2 is not blocked by the insured item of character 1");
        s.gear_insurance_lock_impl(lock(202)).await;
        assert_eq!(err(), E::EnErrGearInsuranceRepeated as i32, "second item on the same character");
        s.gear_insurance_lock_impl(lock(201)).await;
        assert_eq!(err(), E::EnErrGearInsuranceRepeated as i32, "the same item again");

        let mut p = s.db.get::<PlayerModel>("players", "insrole").await.unwrap();
        assert!(insured(&mut p, 101) && insured(&mut p, 201) && !insured(&mut p, 202));
        assert_eq!(p.state["gold"].as_i64(), Some(10_000_000 - 2 * 108_000), "paid twice");
        let _ = std::fs::remove_dir_all(&dir);
    }
    use super::*;

    #[test]
    fn relic_lower_quality_rules() {
        let (blue, purple) = ([10], [20]);
        let guide = || [(10, &[][..]), (20, &blue[..]), (30, &purple[..])].into_iter();
        assert!(relic_is_lower(guide(), &[10, 20], 10), "higher quality unlocked");
        assert!(!relic_is_lower(guide(), &[10, 20], 20), "current quality");
        assert!(!relic_is_lower(guide(), &[10], 10), "the only one unlocked");

        let gem = tgf::DItemGem {
            slot_data: [1, 2, 3].map(|slot_id| tgf::DItemGemSlot { slot_id, ..Default::default() }).to_vec(),
            ..Default::default()
        };
        let ids = |n| relic_gems_for_slots(&gem, n).slot_data.iter().map(|s| s.slot_id).collect::<Vec<_>>();
        assert_eq!(ids(1), [1]);
        assert_eq!(ids(3), [1, 2, 3], "higher quality unchanged");
    }

    #[test]
    fn craft_progress() {
        let s = tgf::DCraftingSlotData { recipe_id: 101, completed_count: 0, uncompleted_count: 5, start_time: 100, time_cost: 10 };
        assert_eq!(craft_done(&s, 99), 0);
        assert_eq!(craft_done(&s, 125), 2);
        assert_eq!(craft_done(&s, 1000), 5);
        assert_eq!(craft_done(&tgf::DCraftingSlotData { time_cost: 0, ..s }, 0), 5);
    }

    #[test]
    fn cost_variants() {
        let costs = vec![
            tgf::DConfigCostItem { index: 1, items: vec![tgf::DItemOffset { type_id: 1052, count: 200, expire_offset: 0 }] },
            tgf::DConfigCostItem { index: 2, items: vec![tgf::DItemOffset { type_id: 1037, count: 50, expire_offset: 0 }] },
        ];
        assert_eq!(choose_cost(&costs, &[], 2).ok().unwrap()[0].type_id, 1037);
        assert_eq!(choose_cost(&costs, &[], 0).ok().unwrap()[0].type_id, 1052);
        assert!(choose_cost(&costs, &[], 3).is_err());
        assert!(choose_cost(&[], &[], 5).ok().unwrap().is_empty());
    }

    #[test]
    fn rewards_merge_currency() {
        let c = |t, n| tgf::DItem { basic: Some(tgf::DItemBasic { type_id: t, count: n, ..Default::default() }), ..Default::default() };
        let m = merge_rewards(&[c(1052, 10), c(1037, 5), c(1052, 10)]);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].basic.as_ref().unwrap().count, 20);
    }

    #[test]
    fn heirloom_state_roundtrip() {
        let mut p = PlayerModel::new("t", 10000001);
        let mut h = heirloom_get(&p);
        assert_eq!(h.limit_option.as_ref().unwrap().season.as_ref().unwrap().season_id, SEASON_ID);
        h.unlock_heirloom_id.push(210110120);
        h.heirloom_gem_data.insert(21011010, tgf::DItemGem { slot_data: vec![tgf::DItemGemSlot { slot_id: 1, gem_id: 240112000, affix_location: 1 }] });
        let mut out = Out::default();
        heirloom_set(&mut p, &h, &mut out);
        assert_eq!(heirloom_get(&p), h);
        assert!(out.extra.dirty_heirloom_data.is_some());
    }
}

#[cfg(test)]
mod craft_tests {
    use super::*;

    #[test]
    fn craft_done_and_wire() {
        let s = tgf::DCraftingSlotData { recipe_id: 102, completed_count: 0, uncompleted_count: 4, start_time: 100, time_cost: 1 };
        assert_eq!(craft_done(&s, 100), 0);
        assert_eq!(craft_done(&s, 102), 2);
        assert_eq!(craft_done(&s, 999), 4);
        let w = craft_wire(&s, 102);
        assert_eq!((w.completed_count, w.uncompleted_count, w.start_time, w.time_cost), (2, 2, 102, 1), "progress is computed by the server");
    }
}
