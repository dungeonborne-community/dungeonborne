//! Items in the lobby: equip / unequip / move / swap / switch weapon set.
//! The handlers in handlers.rs only delegate here. The response and push format — as in PVE SoloLobby (rpc_dump 30.10):
//! after an action SCPlayerDirtyChgSync.dirty_postion_action — for every moved item a "removed from the old
//! place" record (only basic with the old position), then the full item at the new place.
//!
//! Places (DItemPosition.type): 1 account stash (guid = page, x, y), 2 backpack, 4 small backpack (guid = role, x, y),
//! 3 worn (guid = role, x = slot). Item sizes in cells and slot compatibility are checked by the client.
//!
//! Below, inv_* are shared helpers for other domains (item_handlers): item shape, free cell search,
//! granting (inv_give/inv_place_item) and charging (inv_pay) with currency via common::add_coin. Moves do not
//! use them.

use super::common;
use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;

pub(super) const STASH: i32 = 1;
pub(super) const BACKPACK: i32 = 2;
pub(super) const WEARING: i32 = 3;
pub(super) const SMALL_BACKPACK: i32 = 4;
/// default stash pages (as in PVE; = const inventory_max_page). A custom number — state["inventory_page_count"]
pub const STASH_PAGES: i64 = 8;
/// stash page size: x 0..17 (row_size), y 0..10 (column_size)
pub(super) const STASH_W: i32 = 17;
pub(super) const STASH_H: i32 = 10;

pub(super) fn guid_of(item: &tgf::DItem) -> i64 {
    item.basic.as_ref().map(|b| b.guid).unwrap_or(0)
}

pub(super) fn pos_of(item: &tgf::DItem) -> tgf::DItemPosition {
    item.basic.as_ref().and_then(|b| b.position.clone()).unwrap_or_default()
}

pub(super) fn same_place(a: &tgf::DItemPosition, b: &tgf::DItemPosition) -> bool {
    a.r#type == b.r#type && a.guid == b.guid && a.x == b.x && (a.r#type == WEARING || a.y == b.y)
}

/// The item list of place `pos` (stash or a role container) if the player has such a place
pub(super) fn container_mut<'a>(p: &'a mut PlayerModel, pos: &tgf::DItemPosition) -> Option<&'a mut Vec<tgf::DItem>> {
    match pos.r#type {
        STASH => Some(&mut p.stash.element),
        BACKPACK | WEARING | SMALL_BACKPACK => {
            let role = p.roles.iter_mut().find(|r| r.role_guid as i64 == pos.guid)?;
            let d = role.data.deploy_role.get_or_insert_with(Default::default);
            Some(match pos.r#type {
                BACKPACK => &mut d.backpack.get_or_insert_with(|| tgf::DRoleBackpackData { items: vec![], row_size: 7, column_size: 10 }).items,
                SMALL_BACKPACK => &mut d.small_backpack.get_or_insert_with(|| tgf::DRoleBackpackData { items: vec![], row_size: 2, column_size: 5 }).items,
                _ => &mut d.wearing.get_or_insert_with(Default::default).items,
            })
        }
        _ => None,
    }
}

/// All places of the player where items can lie
pub(super) fn all_places(p: &PlayerModel) -> Vec<tgf::DItemPosition> {
    let mut out = vec![tgf::DItemPosition { r#type: STASH, ..Default::default() }];
    for r in &p.roles {
        for t in [BACKPACK, WEARING, SMALL_BACKPACK] {
            out.push(tgf::DItemPosition { r#type: t, guid: r.role_guid as i64, ..Default::default() });
        }
    }
    out
}

/// Item repair (05.11): items from a raid result (the DS returns picked-up consumables without a guid) have guid = 0 and
/// the position has no role guid. The client sends such items in inventory_item_move with guid 0, and `take` took the first
/// item with guid 0 of any role — someone else's item was moved, the client and the backend diverged (items "appear").
/// We assign a guid from the shared counter (0 and > 2^31) and set the type/guid of its own place on the position. Returns the number of fixes.
pub(crate) fn repair_items(p: &mut PlayerModel) -> usize {
    let mut fixed = 0;
    for place in all_places(p) {
        let Some(list) = container_mut(p, &place) else { continue };
        for it in list.iter_mut() {
            let Some(b) = it.basic.as_mut() else { continue };
            let mut touched = false;
            if b.guid == 0 || crate::guid::too_big(b.guid) {
                b.guid = crate::guid::next_item();
                touched = true;
            }
            let pos = b.position.get_or_insert_with(Default::default);
            if pos.r#type != place.r#type && place.r#type != STASH {
                pos.r#type = place.r#type;
                touched = true;
            }
            if place.r#type != STASH && pos.guid != place.guid {
                pos.guid = place.guid; // bags/worn: position guid = role (for the stash it is the page number — not touched)
                touched = true;
            }
            fixed += touched as usize;
        }
    }
    fixed
}

/// Take an item by guid from anywhere
pub(super) fn take(p: &mut PlayerModel, guid: i64) -> Option<tgf::DItem> {
    for place in all_places(p) {
        if let Some(list) = container_mut(p, &place) {
            if let Some(i) = list.iter().position(|it| guid_of(it) == guid) {
                return Some(list.remove(i));
            }
        }
    }
    None
}

/// guid of the item lying exactly at `pos`
pub(super) fn occupant(p: &mut PlayerModel, pos: &tgf::DItemPosition) -> Option<i64> {
    container_mut(p, pos)?.iter().find(|it| same_place(&pos_of(it), pos)).map(guid_of)
}

pub(super) fn put(p: &mut PlayerModel, mut item: tgf::DItem, pos: &tgf::DItemPosition) -> Result<tgf::DItem, String> {
    let list = container_mut(p, pos).ok_or_else(|| format!("no place type={} guid={}", pos.r#type, pos.guid))?;
    if let Some(b) = item.basic.as_mut() {
        b.position = Some(pos.clone());
    }
    list.push(item.clone());
    Ok(item)
}

/// Move result: the item at the new place, the displaced one (if any) and the records for dirty_postion_action
struct Moved {
    item: tgf::DItem,
    displaced: Option<tgf::DItem>,
    dirty: Vec<tgf::DItem>,
}

/// 08.11 (stack bug): dirty_postion_action records — one "removed" (the place before the action) and one final per
/// affected item, in the order of first mention. Before, all intermediate steps went out (top-up → move of the remainder
/// of the same guid: "removed / 2 pcs in the backpack / removed / 2 pcs in the inventory"), the client parsed them differently and lost where
/// the stack lies. An unchanged item is not written; records without a guid stay as they are.
pub(super) fn compact_dirty(before: &mut PlayerModel, after: &mut PlayerModel, dirty: Vec<tgf::DItem>) -> Vec<tgf::DItem> {
    let mut order: Vec<i64> = Vec::new();
    let mut no_guid = Vec::new();
    for d in &dirty {
        match guid_of(d) {
            0 => no_guid.push(d.clone()),
            g if !order.contains(&g) => order.push(g),
            _ => {}
        }
    }
    let (mut removals, mut finals) = (Vec::new(), Vec::new());
    for g in order {
        let orig = find_item_mut(before, g).map(|i| i.clone());
        let fin = find_item_mut(after, g).map(|i| i.clone());
        if orig.is_none() && fin.is_none() {
            // not in the player's places (find_item_mut does not see it) — send as it was
            finals.extend(dirty.iter().filter(|d| guid_of(d) == g).cloned());
            continue;
        }
        if orig == fin {
            continue;
        }
        if let Some(o) = &orig {
            removals.push(removal(o));
        }
        finals.extend(fin);
    }
    removals.extend(finals);
    removals.extend(no_guid);
    removals
}

pub(super) fn removal(item: &tgf::DItem) -> tgf::DItem {
    let b = item.basic.as_ref();
    tgf::DItem {
        basic: Some(tgf::DItemBasic {
            type_id: b.map(|b| b.type_id).unwrap_or(0),
            guid: b.map(|b| b.guid).unwrap_or(0),
            position: b.and_then(|b| b.position.clone()),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Move item `guid` to `goal`. An occupied place is freed: the previous item goes to `swap_to`
/// (if not given — to where ours was taken from). Whether the cell is free is checked by the client.
/// Error — the player is rolled back as it was (the item is not lost).
fn move_item(p: &mut PlayerModel, guid: i64, goal: &tgf::DItemPosition, swap_to: Option<&tgf::DItemPosition>) -> Result<Moved, String> {
    let backup = p.clone();
    let res = move_item_unchecked(p, guid, goal, swap_to);
    if res.is_err() {
        *p = backup;
    }
    res
}

/// inventory_item_swap (07.11): the client puts our item `a` on `goal` over `b` — not necessarily into its top-left
/// corner (a 3×1 sword from x3 to x1 while another lies at x0) — and chooses itself where `b` goes
/// (`b_to` = specify_swapped_item_new_position; not given — to `a`'s old place). Before, `b` was looked up only exactly
/// at goal: not found — `a` was put over `b`, `b` stayed in place, and only `a` got into the client push
/// (the client drew swords inside each other). The place is checked by the client; the server executes and reports both items.
fn swap_items(p: &mut PlayerModel, a: i64, b: i64, goal: &tgf::DItemPosition, b_to: Option<&tgf::DItemPosition>) -> Result<Moved, String> {
    if b == 0 || b == a {
        return move_item(p, a, goal, b_to);
    }
    if a == 0 {
        return Err("item without a guid (0) — rejected".into());
    }
    let backup = p.clone();
    let res = (|| {
        container_mut(p, goal).ok_or_else(|| format!("no place type={} guid={}", goal.r#type, goal.guid))?;
        let ia = take(p, a).ok_or_else(|| format!("the player has no item guid={}", a))?;
        let ib = take(p, b).ok_or_else(|| format!("the player has no item guid={} (the swap partner)", b))?;
        let to = b_to.filter(|s| s.r#type != 0).cloned().unwrap_or_else(|| pos_of(&ia));
        let mut dirty = vec![removal(&ia), removal(&ib)];
        let item = put(p, ia, goal)?;
        let displaced = put(p, ib, &to)?;
        dirty.push(item.clone());
        dirty.push(displaced.clone());
        Ok(Moved { item, displaced: Some(displaced), dirty })
    })();
    if res.is_err() {
        *p = backup;
    }
    res
}

fn move_item_unchecked(p: &mut PlayerModel, guid: i64, goal: &tgf::DItemPosition, swap_to: Option<&tgf::DItemPosition>) -> Result<Moved, String> {
    container_mut(p, goal).ok_or_else(|| format!("no place type={} guid={}", goal.r#type, goal.guid))?;
    if guid == 0 {
        // without a guid the item cannot be identified — before, the first one with guid 0 of any role was taken (someone else's)
        return Err("item without a guid (0) — rejected, repair_items is needed".into());
    }
    let item = take(p, guid).ok_or_else(|| format!("the player has no item guid={}", guid))?;
    let from = pos_of(&item);
    let mut dirty = vec![removal(&item)];
    let mut displaced = None;
    if let Some(other) = occupant(p, goal).filter(|g| *g != guid) {
        let other_item = take(p, other).unwrap();
        dirty.push(removal(&other_item));
        let to = swap_to.filter(|s| s.r#type != 0).cloned().unwrap_or(from);
        displaced = Some(put(p, other_item, &to)?);
    }
    let item = put(p, item, goal)?;
    dirty.push(item.clone());
    dirty.extend(displaced.clone());
    Ok(Moved { item, displaced, dirty })
}

/// inventory_item_move with a count (05.11, like the client: stacks are merged and split). `n` — how many pieces
/// to move (0 or ≥ the stack — all of it). On goal the same item in another stack — top it up to the stack limit (`shape.stack`);
/// part of a stack onto an empty cell — split off as a new stack with a new guid; otherwise — move it whole (`move_item`, swap).
/// Before, it was always moved whole with a swap: the client merged/split, the server swapped — the stacks diverged.
/// 08.11 (stack bug): the actions of one inventory_item_move are one player gesture (a stack onto a non-full stack:
/// "top up part" + "the remainder next to it"). All or nothing: one fails — the player is as before the request. Before, the second
/// was applied without the first, the stack was split in half, the client and the server diverged.
pub(super) fn apply_moves(p: &mut PlayerModel, actions: &[tgf::DUserInvetoryItemMoveAction], shape_of: &dyn Fn(i32) -> ItemShape) -> Result<Vec<tgf::DItem>, String> {
    let start = p.clone();
    let mut dirty = vec![];
    for a in actions {
        let guid = a.item_basic.as_ref().map(|b| b.guid).unwrap_or(0);
        let count = a.item_basic.as_ref().map(|b| b.count).unwrap_or(0);
        let goal = a.goal_position.clone().unwrap_or_default();
        match move_count(p, guid, count, &goal, shape_of) {
            Ok(d) => dirty.extend(d),
            Err(e) => {
                *p = start;
                return Err(format!("guid={}: {}", guid, e));
            }
        }
    }
    Ok(dirty)
}

fn move_count(p: &mut PlayerModel, guid: i64, n: i32, goal: &tgf::DItemPosition, shape_of: &dyn Fn(i32) -> ItemShape) -> Result<Vec<tgf::DItem>, String> {
    if guid == 0 {
        return Err("item without a guid (0) — rejected".into());
    }
    let src = find_item_mut(p, guid).ok_or_else(|| format!("the player has no item guid={}", guid))?.clone();
    let sb = src.basic.clone().unwrap_or_default();
    let have = sb.count.max(1);
    let n = if n <= 0 || n >= have { have } else { n };
    let stack = shape_of(sb.type_id).stack;
    let occ = occupant(p, goal).filter(|g| *g != guid);
    let occ_item = occ.and_then(|g| find_item_mut(p, g).map(|i| i.clone()));
    let mut dirty = vec![];
    // 1) merge into the same kind of stack
    if let Some(o) = occ_item.filter(|o| o.basic.as_ref().map(|b| b.type_id) == Some(sb.type_id) && stack > 1) {
        let og = guid_of(&o);
        let room = stack - o.basic.as_ref().map(|b| b.count.max(1)).unwrap_or(1);
        let add = n.min(room);
        if add <= 0 {
            // 08.11: a whole stack onto a full stack of the same type — swap places, as with any other item;
            // part of a stack onto a full one — rejected
            if n >= have {
                return Ok(move_item(p, guid, goal, None)?.dirty);
            }
            return Err(format!("stack guid={} is full ({}/{})", og, stack, stack));
        }
        let target = find_item_mut(p, og).unwrap();
        let before = removal(target);
        target.basic.as_mut().unwrap().count += add;
        dirty.push(before);
        dirty.push(target.clone());
        if add >= have {
            let gone = take(p, guid).unwrap();
            dirty.insert(0, removal(&gone));
        } else {
            let s = find_item_mut(p, guid).unwrap();
            let before = removal(s);
            s.basic.as_mut().unwrap().count = have - add;
            dirty.insert(0, s.clone());
            dirty.insert(0, before);
        }
        return Ok(dirty);
    }
    // 2) split part of the stack onto a free cell
    if n < have {
        let s = shape_of(sb.type_id);
        if occ.is_some() || !area_free(p, goal, s.w, s.h, 0, shape_of) {
            return Err(format!("the cell for part of stack guid={} is occupied", guid));
        }
        let left = find_item_mut(p, guid).unwrap();
        let before = removal(left);
        left.basic.as_mut().unwrap().count = have - n;
        dirty.push(before);
        dirty.push(left.clone());
        let mut part = src.clone();
        if let Some(b) = part.basic.as_mut() {
            b.guid = crate::guid::next_item();
            b.count = n;
        }
        dirty.push(put(p, part, goal)?);
        return Ok(dirty);
    }
    // 3) a whole stack / a single item — as before (an occupied place — swap)
    Ok(move_item(p, guid, goal, None)?.dirty)
}

/// The left hand of the same set for the right one: 1 → 2, 3 → 4 (weapon sets 1/2 and 3/4)
fn off_hand_of(slot: i32) -> Option<i32> {
    match slot {
        1 => Some(2),
        3 => Some(4),
        _ => None,
    }
}

/// A two-hander into the right hand (the client sent secondary_weapon_position): the item from the left hand of the same set — to `dest`
/// (if the client gave a cell and it is free) or to the first free cell of the same place. Left hand empty — None.
fn take_off_off_hand(
    p: &mut PlayerModel,
    role_guid: i64,
    main_slot: i32,
    dest: &tgf::DItemPosition,
    shape_of: &dyn Fn(i32) -> ItemShape,
) -> Result<Option<Moved>, String> {
    let Some(off) = off_hand_of(main_slot) else { return Ok(None) };
    let off_pos = tgf::DItemPosition { r#type: WEARING, guid: role_guid, x: off, y: 0, limit_option: None };
    let Some(g) = occupant(p, &off_pos) else { return Ok(None) };
    let type_id = container_mut(p, &off_pos)
        .and_then(|l| l.iter().find(|i| guid_of(i) == g).and_then(|i| i.basic.as_ref().map(|b| b.type_id)))
        .unwrap_or(0);
    let s = shape_of(type_id);
    let to = if area_free(p, dest, s.w, s.h, g, shape_of) {
        dest.clone()
    } else {
        free_cell(p, dest, s.w, s.h, shape_of).ok_or_else(|| format!("nowhere to put the left hand ({} {}x{})", type_id, s.w, s.h))?
    };
    move_item(p, g, &to, None).map(Some)
}

/// The weapon pair for a set slot: 1 ↔ 3 (right hand), 2 ↔ 4 (left)
fn other_set_slot(slot: i32) -> i32 {
    match slot {
        1 => 3,
        3 => 1,
        2 => 4,
        4 => 2,
        s => s,
    }
}

/// Stash for player_get_info: stash_page_count pages, each with one item group of its page
pub fn stash_pages(p: &PlayerModel) -> Vec<tgf::DUserInventoryPage> {
    (0..stash_page_count(p)).map(|page| stash_page(p, page)).collect()
}

/// Number of stash pages: state["inventory_page_count"] (inventory_page_add / item add_inventory_page), otherwise STASH_PAGES
pub(super) fn stash_page_count(p: &PlayerModel) -> i64 {
    p.state["inventory_page_count"].as_i64().filter(|n| *n > 0).unwrap_or(STASH_PAGES)
}

/// One stash page (for player_get_info and dirty_inventory_page)
pub(super) fn stash_page(p: &PlayerModel, page: i64) -> tgf::DUserInventoryPage {
    tgf::DUserInventoryPage {
        item_group: vec![tgf::DItemGroup { element: p.stash.element.iter().filter(|it| pos_of(it).guid == page).cloned().collect() }],
        row_size: STASH_W,
        column_size: STASH_H,
        page_id: page,
        limit_option: None,
    }
}

// ----- shared item helpers (item_handlers etc.) -----

/// Item shape in the grid (UeSource_ItemType.position_cfg): w — along x (row_size), h — along y (column_size),
/// stack — accumulation_limit. The axis was checked against the PVE stash (rpc_dump PVE_battle_0720: 56 items without overlaps).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ItemShape {
    pub w: i32,
    pub h: i32,
    pub stack: i32,
}

impl Default for ItemShape {
    fn default() -> Self {
        Self { w: 1, h: 1, stack: 1 }
    }
}

/// An item by guid anywhere on the player (stash, bags, worn) — for in-place edits
pub(super) fn find_item_mut(p: &mut PlayerModel, guid: i64) -> Option<&mut tgf::DItem> {
    for place in all_places(p) {
        let found = container_mut(p, &place).map(|l| l.iter().any(|it| guid_of(it) == guid)).unwrap_or(false);
        if found {
            return container_mut(p, &place)?.iter_mut().find(|it| guid_of(it) == guid);
        }
    }
    None
}

/// dirty_postion_action records for an item changed in place (count, durability, gems): "removed" + full
pub(super) fn changed(item: &tgf::DItem) -> [tgf::DItem; 2] {
    [removal(item), item.clone()]
}

/// Grid size of a place (width along x, height along y); None — no such place or it has no grid (worn)
pub(super) fn grid_of(p: &PlayerModel, place: &tgf::DItemPosition) -> Option<(i32, i32)> {
    match place.r#type {
        STASH => (place.guid >= 0 && place.guid < stash_page_count(p)).then_some((STASH_W, STASH_H)),
        BACKPACK | SMALL_BACKPACK => {
            let role = p.roles.iter().find(|r| r.role_guid as i64 == place.guid)?;
            let d = role.data.deploy_role.as_ref();
            let bag = if place.r#type == BACKPACK { d.and_then(|d| d.backpack.as_ref()) } else { d.and_then(|d| d.small_backpack.as_ref()) };
            let (dw, dh) = if place.r#type == BACKPACK { (7, 10) } else { (2, 5) };
            Some(bag.map(|b| (if b.row_size > 0 { b.row_size } else { dw }, if b.column_size > 0 { b.column_size } else { dh })).unwrap_or((dw, dh)))
        }
        _ => None,
    }
}

/// Whether the w×h area with the top-left corner at pos is free (item `ignore` is not counted)
pub(super) fn area_free(p: &mut PlayerModel, pos: &tgf::DItemPosition, w: i32, h: i32, ignore: i64, shape_of: &dyn Fn(i32) -> ItemShape) -> bool {
    let Some((gw, gh)) = grid_of(p, pos) else { return false };
    if pos.x < 0 || pos.y < 0 || pos.x + w > gw || pos.y + h > gh {
        return false;
    }
    let Some(list) = container_mut(p, pos) else { return false };
    !list.iter().any(|it| {
        let ip = pos_of(it);
        if guid_of(it) == ignore || (pos.r#type == STASH && ip.guid != pos.guid) {
            return false;
        }
        let s = shape_of(it.basic.as_ref().map(|b| b.type_id).unwrap_or(0));
        ip.x < pos.x + w && pos.x < ip.x + s.w && ip.y < pos.y + h && pos.y < ip.y + s.h
    })
}

/// The first free cell for w×h in place `place` (type + page/role guid). Stash: first page place.guid,
/// then the others. Traversal by rows (y), within a row — by x.
pub(super) fn free_cell(p: &mut PlayerModel, place: &tgf::DItemPosition, w: i32, h: i32, shape_of: &dyn Fn(i32) -> ItemShape) -> Option<tgf::DItemPosition> {
    let mut candidates = vec![place.guid];
    if place.r#type == STASH {
        candidates.extend((0..stash_page_count(p)).filter(|g| *g != place.guid));
    }
    for g in candidates {
        let base = tgf::DItemPosition { r#type: place.r#type, guid: g, x: 0, y: 0, limit_option: None };
        let Some((gw, gh)) = grid_of(p, &base) else { continue };
        for y in 0..=(gh - h) {
            for x in 0..=(gw - w) {
                let pos = tgf::DItemPosition { x, y, ..base.clone() };
                if area_free(p, &pos, w, h, 0, shape_of) {
                    return Some(pos);
                }
            }
        }
    }
    None
}

impl GatewaySession {
    /// Load the player, perform the action; on success — save, update the loadout for the host and send
    /// dirty_postion_action. An error is logged and not saved.
    async fn inventory_action<R>(&self, what: &str, f: impl FnOnce(&mut PlayerModel) -> Result<(R, Vec<tgf::DItem>), String>) -> Option<R> {
        let open_id = self.clean_open_id();
        let mut player = self.db.get::<PlayerModel>("players", &open_id).await?;
        let mut before = player.clone();
        match f(&mut player) {
            Ok(_) if super::raid_lock::raid_items_changed(&before, &player).is_some() => {
                // 16.11: the items of a character in a running match are changed only by the raid result (raid_lock.rs) — the player is not saved
                let role = super::raid_lock::raid_items_changed(&before, &player).unwrap_or_default();
                self.raid_lock_refuse(what, role);
                None
            }
            Ok(_) if super::relic_limit::limit_broken(&before, &player, &|t| self.relic_kind(t)).is_some() => {
                // 12.11: at most one relic of a kind on a character (relic_limit.rs) — the action is cancelled, the player is not saved
                let why = super::relic_limit::limit_broken(&before, &player, &|t| self.relic_kind(t)).unwrap_or_default();
                println!("[Inventory] {} {}: rejected — {}", open_id, what, why);
                self.set_rpc_error(tgf::EnErrorCode::EnErrGearHeirloomWearingExist as i32);
                None
            }
            Ok((rsp, dirty)) => {
                let dirty = compact_dirty(&mut before, &mut player, dirty);
                self.db.save("players", &open_id, &player).await;
                Self::write_client_loadout_file(&player);
                println!("[Inventory] {} {}: {} items updated", open_id, what, dirty.len());
                self.push(
                    "tgf.GameClientService.player_dirty_change",
                    "tgf.SCPlayerDirtyChgSync",
                    tgf::ScPlayerDirtyChgSync { dirty_postion_action: dirty, ..Default::default() },
                );
                Some(rsp)
            }
            Err(e) => {
                println!("[Inventory] {} {}: rejected — {}", open_id, what, e);
                None
            }
        }
    }

    pub(super) async fn inv_role_wear(&self, req: tgf::CsRoleWearReq) -> tgf::ScRoleWearRsp {
        let guid = req.item_basic.as_ref().map(|b| b.guid).unwrap_or(0);
        let goal = req.goal_position.clone().unwrap_or_default();
        let secondary = req.secondary_weapon_position.clone().filter(|s| s.r#type != 0);
        let shape_of = |t: i32| self.inv_item_shape(t);
        self.inventory_action("role_wear", |p| {
            let backup = p.clone();
            let m = move_item(p, guid, &goal, req.swap_position.as_ref())?;
            let mut dirty = m.dirty;
            // a two-hander (the client sends secondary_weapon_position only for it): the left hand of the set — to the bag/stash
            let mut off = None;
            if let (Some(dest), WEARING) = (secondary.as_ref(), goal.r#type) {
                match take_off_off_hand(p, goal.guid, goal.x, dest, &shape_of) {
                    Ok(Some(o)) => {
                        dirty.extend(o.dirty);
                        off = Some(o.item);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        *p = backup;
                        return Err(e);
                    }
                }
            }
            Ok((tgf::ScRoleWearRsp { worn_item: Some(m.item), taken_off_item: m.displaced, taken_off_secondary_weapon: off }, dirty))
        })
        .await
        .unwrap_or_default()
    }

    pub(super) async fn inv_role_take_off(&self, req: tgf::CsRoleTakeOffReq) -> tgf::ScRoleTakeOffRsp {
        let guid = req.item_basic.as_ref().map(|b| b.guid).unwrap_or(0);
        let goal = req.storage_position.clone().unwrap_or_default();
        let secondary = req.secondary_weapon_position.clone().filter(|s| s.r#type != 0);
        let shape_of = |t: i32| self.inv_item_shape(t);
        self.inventory_action("role_take_off", |p| {
            let from = find_item_mut(p, guid).map(|i| pos_of(i)).unwrap_or_default();
            let backup = p.clone();
            let m = move_item(p, guid, &goal, None)?;
            let mut dirty = m.dirty;
            // the client asks to take off the left hand too (secondary_weapon_position) — take off the item of the paired slot
            let mut off = None;
            if let (Some(dest), WEARING) = (secondary.as_ref(), from.r#type) {
                match take_off_off_hand(p, from.guid, from.x, dest, &shape_of) {
                    Ok(Some(o)) => {
                        dirty.extend(o.dirty);
                        off = Some(o.item);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        *p = backup;
                        return Err(e);
                    }
                }
            }
            Ok((tgf::ScRoleTakeOffRsp { taken_off_item: Some(m.item), taken_off_secondary_weapon: off }, dirty))
        })
        .await
        .unwrap_or_default()
    }

    pub(super) async fn inv_item_move(&self, req: tgf::CsInventoryItemMoveReq) -> tgf::ScInventoryItemMoveRsp {
        let actions = req.move_actions.clone();
        let shape_of = |t: i32| self.inv_item_shape(t);
        let rsp = self
            .inventory_action("inventory_item_move", |p| {
                match apply_moves(p, &actions, &shape_of) {
                    Ok(dirty) => Ok((tgf::ScInventoryItemMoveRsp { failed_actions: vec![], succ_actions: actions.clone() }, dirty)),
                    Err(e) => {
                        println!("[Inventory] inventory_item_move: {} — request ({} actions) cancelled as a whole", e, actions.len());
                        Ok((tgf::ScInventoryItemMoveRsp { failed_actions: actions.clone(), succ_actions: vec![] }, vec![]))
                    }
                }
            })
            .await;
        rsp.unwrap_or_else(|| tgf::ScInventoryItemMoveRsp { failed_actions: req.move_actions, succ_actions: vec![] })
    }

    /// Swap: our item — onto goal, the swap partner (swapped_item_basic) — onto
    /// specify_swapped_item_new_position (not given — to our old place). Rejection — error -620, the player does not change.
    pub(super) async fn inv_item_swap(&self, req: tgf::CsInventoryItemSwapReq) -> tgf::ScInventoryItemSwapRsp {
        let guid = req.item_basic.as_ref().map(|b| b.guid).unwrap_or(0);
        let other = req.swapped_item_basic.as_ref().map(|b| b.guid).unwrap_or(0);
        let goal = req
            .goal_position
            .clone()
            .or_else(|| req.swapped_item_basic.as_ref().and_then(|b| b.position.clone()))
            .unwrap_or_default();
        let b_to = req.specify_swapped_item_new_position.clone();
        let ok = self
            .inventory_action("inventory_item_swap", |p| {
                let m = swap_items(p, guid, other, &goal, b_to.as_ref())?;
                Ok(((), m.dirty))
            })
            .await;
        if ok.is_none() {
            self.set_rpc_error(tgf::EnErrorCode::EnErrItemSwapPositionCheckFailed as i32);
        }
        tgf::ScInventoryItemSwapRsp {}
    }

    /// Weapon set switch: ori <-> goal, and the second hand ori_secondary <-> goal_secondary (not sent — the paired slot
    /// of the other set: 2 ↔ 4).
    pub(super) async fn inv_weapon_set_swap(&self, req: tgf::CsRoleWeaponSetSwapReq) -> tgf::ScRoleWeaponSetSwapRsp {
        let ori = req.ori_position.clone().unwrap_or_default();
        let goal = req.goal_position.clone().unwrap_or_default();
        let secondary = req.ori_secondary_weapon_position.clone().filter(|s| s.r#type == WEARING).map(|sec| {
            let sec_goal = req
                .goal_secondary_weapon_position
                .clone()
                .filter(|g| g.r#type == WEARING)
                .unwrap_or_else(|| tgf::DItemPosition { x: other_set_slot(sec.x), ..sec.clone() });
            (sec, sec_goal)
        });
        self.inventory_action("role_weapon_set_swap", |p| {
            let mut dirty = vec![];
            for (a, b) in std::iter::once((ori.clone(), goal.clone())).chain(secondary) {
                if let Some(g) = occupant(p, &a) {
                    dirty.extend(move_item(p, g, &b, Some(&a))?.dirty);
                } else if let Some(g) = occupant(p, &b) {
                    dirty.extend(move_item(p, g, &a, None)?.dirty); // a is empty — just move whatever occupies b
                }
            }
            Ok(((), dirty))
        })
        .await;
        tgf::ScRoleWeaponSetSwapRsp {}
    }
}

/// Accumulated changes for ScPlayerDirtyChgSync: dirty_postion_action records, touched coins and what was granted
/// (for the response fields got/item/reward)
#[derive(Default, Debug)]
pub(super) struct ItemChanges {
    pub dirty: Vec<tgf::DItem>,
    pub coins: Vec<i32>,
    pub added: Vec<tgf::DItem>,
}

impl ItemChanges {
    fn coin(&mut self, coin_type_id: i32) {
        if !self.coins.contains(&coin_type_id) {
            self.coins.push(coin_type_id);
        }
    }

    /// Push message: dirty_postion_action + dirty_coin_data with the current balances
    pub fn to_dirty(&self, p: &PlayerModel) -> tgf::ScPlayerDirtyChgSync {
        tgf::ScPlayerDirtyChgSync {
            dirty_postion_action: self.dirty.clone(),
            dirty_coin_data: self.coins.iter().map(|c| common::coin_dirty(p, *c)).collect(),
            ..Default::default()
        }
    }
}

/// Default role (standing in the hall, otherwise the first) — for bags when granting items
pub(super) fn default_role_guid(p: &PlayerModel) -> i64 {
    if p.hall_role_guid != 0 {
        p.hall_role_guid as i64
    } else {
        p.roles.first().map(|r| r.role_guid as i64).unwrap_or(0)
    }
}

/// Place by the client's EnItemStorageType: 1/101 — stash (guid = page), 2/102 — backpack, 4 — small backpack
/// (guid = role; 0 — the default role)
pub(super) fn storage_place(p: &PlayerModel, storage_type: i32, guid: i64) -> Option<tgf::DItemPosition> {
    let (t, g) = match storage_type {
        1 | 101 => (STASH, guid.max(0)),
        2 | 102 => (BACKPACK, if guid != 0 { guid } else { default_role_guid(p) }),
        4 => (SMALL_BACKPACK, if guid != 0 { guid } else { default_role_guid(p) }),
        _ => return None,
    };
    Some(tgf::DItemPosition { r#type: t, guid: g, x: 0, y: 0, limit_option: None })
}

impl GatewaySession {
    /// Item shape: item_type.ue_source_type_id (or the id itself) → UeSource_ItemType.position_cfg
    pub(super) fn inv_item_shape(&self, type_id: i32) -> ItemShape {
        let ue_id = self.game_data.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|i| *i != 0).unwrap_or(type_id);
        self.game_data
            .uesource_itemtype
            .get(&ue_id)
            .and_then(|u| u.position_cfg.as_ref())
            .map(|c| ItemShape { w: c.row_size.max(1), h: c.column_size.max(1), stack: c.accumulation_limit.max(1) })
            .unwrap_or_default()
    }

    /// Currency (EN_ITEM_MAJOR_TYPE_CURRENCY: gold 1001, materials 1037/1038/1039/1052/1053...) — stored as a coin
    /// (common::coin), not as an item
    pub(super) fn inv_is_currency(&self, type_id: i32) -> bool {
        type_id == common::GOLD || self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == tgf::EnItemMajorType::Currency as i32).unwrap_or(false)
    }

    /// Whether type_id is equipment (EN_ITEM_MAJOR_TYPE_EQUIPMENT)
    pub(super) fn inv_is_gear(&self, type_id: i32) -> bool {
        self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == tgf::EnItemMajorType::Equipment as i32).unwrap_or(false)
    }

    /// Affixes of new equipment (a weapon — as in hand, slot 1); not equipment — empty. Salt — the time.
    pub(super) fn inv_gear_affixes(&self, type_id: i32) -> Vec<tgf::DBattleAffixRandomData> {
        if !self.inv_is_gear(type_id) {
            return vec![];
        }
        self.build_item_affix_data_seeded(type_id, self.inv_gear_wear_part(type_id), Self::inv_affix_seed(type_id))
    }

    /// wear_part for the affix roll: weapon 1, the rest 0
    pub(super) fn inv_gear_wear_part(&self, type_id: i32) -> i32 {
        let weapon = self.game_data.item_type.get(&type_id).map(|t| t.item_minor_type == tgf::EnItemMinorType::EquipmentWeapon as i32).unwrap_or(false);
        if weapon { 1 } else { 0 }
    }

    /// Roll salt of a new item: time in ns and type_id
    pub(super) fn inv_affix_seed(type_id: i32) -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1) ^ (type_id as u64).rotate_left(23)
    }

    /// Repair on login (05.11): equipment without affixes (the starting weapon in the backpack before 05.11 — 0 damage) gets them.
    pub(crate) fn inv_repair_gear_affixes(&self, p: &mut PlayerModel) -> usize {
        let mut fixed = 0;
        for place in all_places(p) {
            let Some(list) = container_mut(p, &place) else { continue };
            for it in list.iter_mut() {
                let type_id = it.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
                let empty = it.data.as_ref().map(|d| d.affix_data.is_empty()).unwrap_or(true);
                if empty && self.inv_is_gear(type_id) {
                    let affixes = self.inv_gear_affixes(type_id);
                    if !affixes.is_empty() {
                        it.data.get_or_insert_with(Default::default).affix_data = affixes;
                        fixed += 1;
                    }
                }
            }
        }
        fixed
    }

    /// A new item with its own guid (shared counter crate::guid, < 2^31). Equipment — with affixes (build_item_affix_data_seeded).
    pub(super) fn inv_new_item(&self, _p: &mut PlayerModel, type_id: i32, count: i32) -> tgf::DItem {
        let guid = crate::guid::next_item();
        let data = tgf::DItemData { affix_data: self.inv_gear_affixes(type_id), ..Default::default() };
        tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id, count: count.max(1), guid, expire_timepoint: 0, position: None }),
            data: Some(data),
            create_info: None,
        }
    }

    /// Put a ready item into place `target` (type + page/role guid). If target has x/y set and the area
    /// is free — there, otherwise the first free cell. No room — Err.
    pub(super) fn inv_place_item(&self, p: &mut PlayerModel, mut item: tgf::DItem, target: &tgf::DItemPosition, ch: &mut ItemChanges) -> Result<tgf::DItem, String> {
        let type_id = item.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
        let s = self.inv_item_shape(type_id);
        let shape_of = |t: i32| self.inv_item_shape(t);
        let pos = if area_free(p, target, s.w, s.h, 0, &shape_of) {
            target.clone()
        } else {
            free_cell(p, target, s.w, s.h, &shape_of).ok_or_else(|| format!("no room {}x{} for {} in type={} guid={}", s.w, s.h, type_id, target.r#type, target.guid))?
        };
        if let Some(b) = item.basic.as_mut() {
            b.position = Some(pos.clone());
        }
        container_mut(p, &pos).ok_or("no place")?.push(item.clone());
        ch.dirty.push(item.clone());
        Ok(item)
    }

    /// Grant type_id × count: currency — as a coin; items — first top up the non-full stacks of place `target`,
    /// the remainder — as new stacks (the first one — at target.x/y if free). No room — Err (do not save the player).
    pub(super) fn inv_give(&self, p: &mut PlayerModel, type_id: i32, count: i32, target: &tgf::DItemPosition, ch: &mut ItemChanges) -> Result<(), String> {
        if count <= 0 || type_id == 0 {
            return Ok(());
        }
        if self.inv_is_currency(type_id) {
            common::add_coin(p, type_id, count as i64).ok_or("coin overflow")?;
            ch.coin(type_id);
            ch.added.push(tgf::DItem { basic: Some(tgf::DItemBasic { type_id, count, ..Default::default() }), ..Default::default() });
            return Ok(());
        }
        let stack = self.inv_item_shape(type_id).stack;
        let mut left = count;
        if stack > 1 {
            let same_page = |it: &tgf::DItem| target.r#type != STASH || pos_of(it).guid == target.guid;
            if let Some(list) = container_mut(p, target) {
                for it in list.iter_mut().filter(|it| same_page(it)) {
                    let Some(b) = it.basic.as_mut() else { continue };
                    if b.type_id != type_id || b.count >= stack || left == 0 {
                        continue;
                    }
                    let add = left.min(stack - b.count);
                    let before = removal(it);
                    if let Some(b) = it.basic.as_mut() {
                        b.count += add;
                    }
                    left -= add;
                    ch.dirty.push(before);
                    ch.dirty.push(it.clone());
                    ch.added.push(it.clone());
                }
            }
        }
        let mut first = true;
        while left > 0 {
            let n = left.min(stack);
            let item = self.inv_new_item(p, type_id, n);
            let at = if first { target.clone() } else { tgf::DItemPosition { x: -1, y: -1, ..target.clone() } };
            first = false;
            let placed = self.inv_place_item(p, item, &at, ch)?;
            ch.added.push(placed);
            left -= n;
        }
        Ok(())
    }

    /// Charge a cost: currency — as a coin, items — from the player's stacks (first the guids from `hint` — the client sent them,
    /// then any stacks: the stash, then the bags). Not enough — Err (the player may have partially changed — do not save).
    pub(super) fn inv_pay(&self, p: &mut PlayerModel, costs: &[tgf::DItemOffset], hint: &[tgf::DItemBasic], ch: &mut ItemChanges) -> Result<(), String> {
        let mut need: Vec<(i32, i64)> = vec![];
        for c in costs.iter().filter(|c| c.type_id != 0 && c.count > 0) {
            match need.iter_mut().find(|(t, _)| *t == c.type_id) {
                Some(e) => e.1 += c.count as i64,
                None => need.push((c.type_id, c.count as i64)),
            }
        }
        for (type_id, count) in need {
            if self.inv_is_currency(type_id) {
                common::add_coin(p, type_id, -count).ok_or_else(|| format!("not enough coin {}: need {}, have {}", type_id, count, common::coin(p, type_id)))?;
                ch.coin(type_id);
                continue;
            }
            // candidates: the guids from hint, then all stacks of this type
            let mut order: Vec<i64> = hint.iter().filter(|b| b.type_id == type_id || b.type_id == 0).map(|b| b.guid).collect();
            for place in all_places(p) {
                if let Some(list) = container_mut(p, &place) {
                    order.extend(list.iter().filter(|it| it.basic.as_ref().map(|b| b.type_id) == Some(type_id)).map(guid_of));
                }
            }
            let mut left = count;
            let mut seen = vec![];
            for g in order {
                if left == 0 || seen.contains(&g) {
                    continue;
                }
                seen.push(g);
                let Some(it) = find_item_mut(p, g) else { continue };
                let have = it.basic.as_ref().filter(|b| b.type_id == type_id).map(|b| b.count.max(1) as i64).unwrap_or(0);
                if have == 0 {
                    continue;
                }
                let take_n = left.min(have);
                left -= take_n;
                if take_n == have {
                    let gone = take(p, g).unwrap();
                    ch.dirty.push(removal(&gone));
                } else {
                    let before = removal(it);
                    if let Some(b) = it.basic.as_mut() {
                        b.count -= take_n as i32;
                    }
                    ch.dirty.push(before);
                    ch.dirty.push(it.clone());
                }
            }
            if left > 0 {
                return Err(format!("not enough item {}: need {}, short by {}", type_id, count, left));
            }
        }
        Ok(())
    }
}

/// 09.11 (user's decision): backpack width per the game tables (const.json): role_backpack_default_row_size (6) ×
/// role_backpack_defualt_column_size (10), humans + role_backpack_race_add_row_size (1). Before, 7×10 for everyone.
/// The small backpack is not touched (user's decision).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct BagSize {
    pub backpack: (i32, i32),
}

/// Bring the character's backpack to the size: items outside the grid — to a free spot in the backpack, otherwise the stash. What cannot
/// be placed anywhere — stays where it was (not lost). Returns (changed, moved).
pub(super) fn fit_bags(p: &mut PlayerModel, role_guid: i64, size: BagSize, shape_of: &dyn Fn(i32) -> ItemShape) -> (bool, usize) {
    let Some(d) = p.roles.iter_mut().find(|r| r.role_guid as i64 == role_guid).and_then(|r| r.data.deploy_role.as_mut()) else {
        return (false, 0);
    };
    let mut changed = false;
    let bag = d.backpack.get_or_insert_with(Default::default);
    if (bag.row_size, bag.column_size) != size.backpack {
        (bag.row_size, bag.column_size) = size.backpack;
        changed = true;
    }
    let (gw, gh) = size.backpack;
    let homeless: Vec<i64> = bag
        .items
        .iter()
        .filter(|it| {
            let pos = pos_of(it);
            let s = shape_of(it.basic.as_ref().map_or(0, |b| b.type_id));
            pos.x < 0 || pos.y < 0 || pos.x + s.w > gw || pos.y + s.h > gh
        })
        .map(guid_of)
        .collect();
    let mut moved = 0;
    for guid in homeless {
        let Some(item) = take(p, guid) else { continue };
        let from = pos_of(&item);
        let s = shape_of(item.basic.as_ref().map_or(0, |b| b.type_id));
        let bag = tgf::DItemPosition { r#type: BACKPACK, guid: role_guid, ..Default::default() };
        let stash = tgf::DItemPosition { r#type: STASH, guid: 0, ..Default::default() };
        match free_cell(p, &bag, s.w, s.h, shape_of).or_else(|| free_cell(p, &stash, s.w, s.h, shape_of)) {
            Some(to) => {
                let _ = put(p, item, &to);
                moved += 1;
                changed = true;
            }
            None => {
                let _ = put(p, item, &from); // nowhere — leave it in place
            }
        }
    }
    (changed, moved)
}

/// 14.11 (user: "this is a class equipment slot! make an exception for all swords outside the main weapon slots",
/// "and the check for swords must be removed in the inventory too"): the sword master's class slots — `EnItemWearingPart`
/// AdditionalWeapon1…9 (101–109, `position.x` of the worn item). The map rarity limit does not check items in them at all.
pub(super) fn in_class_weapon_slot(item: &tgf::DItem) -> bool {
    use tgf::EnItemWearingPart as W;
    let x = item.basic.as_ref().and_then(|b| b.position.as_ref()).map_or(0, |p| p.x);
    (W::TypeAdditionalWeapon1 as i32..=W::TypeAdditionalWeapon9 as i32).contains(&x)
}

/// 09.11 (user's decision): gear outside the map rarity limit — to the stash before handing the match data to the server
/// (after "Start" items can still be moved). Worn: rarity not in the `allowed` list; backpack and small backpack: above
/// the highest allowed one. `rarity_of` — Some(rarity) for gear, None — not gear; rarity 0 is not
/// counted. No room in the stash — the item stays (not lost). Returns the type_ids moved.
/// 14.11: the sword master's class slots (`in_class_weapon_slot`) and swords in the bags (`is_sword`) are not checked; a sword in
/// the main weapon slots (1–4) is checked as before.
pub(super) fn stash_over_limit(
    p: &mut PlayerModel,
    role_guid: i64,
    allowed: &[i32],
    rarity_of: &dyn Fn(i32) -> Option<i32>,
    is_sword: &dyn Fn(i32) -> bool,
    shape_of: &dyn Fn(i32) -> ItemShape,
) -> Vec<i32> {
    let Some(d) = p.roles.iter().find(|r| r.role_guid as i64 == role_guid).and_then(|r| r.data.deploy_role.as_ref()) else {
        return vec![];
    };
    let top = allowed.iter().copied().max().unwrap_or(i32::MAX);
    let type_of = |it: &tgf::DItem| it.basic.as_ref().map_or(0, |b| b.type_id);
    let mut bad: Vec<i64> = vec![];
    for (bag, worn) in [(&d.wearing.as_ref().map(|w| &w.items), true), (&d.backpack.as_ref().map(|b| &b.items), false), (&d.small_backpack.as_ref().map(|b| &b.items), false)] {
        for it in bag.into_iter().flat_map(|v| v.iter()) {
            if (worn && in_class_weapon_slot(it)) || (!worn && is_sword(type_of(it))) {
                continue;
            }
            let Some(r) = rarity_of(type_of(it)).filter(|r| *r != 0) else { continue };
            if (worn && !allowed.contains(&r)) || (!worn && r > top) {
                bad.push(guid_of(it));
            }
        }
    }
    let mut moved = vec![];
    for guid in bad {
        let Some(item) = take(p, guid) else { continue };
        let from = pos_of(&item);
        let t = type_of(&item);
        let s = shape_of(t);
        let stash = tgf::DItemPosition { r#type: STASH, guid: 0, ..Default::default() };
        match free_cell(p, &stash, s.w, s.h, shape_of) {
            Some(to) => {
                let _ = put(p, item, &to);
                moved.push(t);
            }
            None => {
                let _ = put(p, item, &from);
            }
        }
    }
    moved
}

impl GatewaySession {
    /// Before handing out match data: gear outside the limit of map `level_id` — to the stash (`stash_over_limit`)
    pub fn raid_gear_gate(&self, p: &mut PlayerModel, level_id: i64) -> Vec<i32> {
        let Some(allowed) = crate::lobby::match_checks::gear_rule(level_id) else { return vec![] };
        let rarity_of = |t: i32| self.item_is_gear(t).then(|| self.item_rarity(t));
        let is_sword = |t: i32| self.item_is_sword(t);
        let shape_of = |t: i32| self.inv_item_shape(t);
        let role = p.hall_role_guid as i64;
        stash_over_limit(p, role, &allowed, &rarity_of, &is_sword, &shape_of)
    }

    /// 14.11: a sword or a longsword — UeSource_ItemType `slot_type` (EnSkinSlotType Sword / LongSword: 20 + 15 rows) or
    /// the class name `ID_*Sword*` (3 special swords have an empty `slot_type`: ID_Ex_Sword_…, ID_Ex_LongSword_…)
    pub(super) fn item_is_sword(&self, type_id: i32) -> bool {
        use tgf::EnSkinSlotType as S;
        let ue = self.game_data.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|i| *i != 0).unwrap_or(type_id);
        let Some(u) = self.game_data.uesource_itemtype.get(&ue) else { return false };
        let name = u.ue_path.rsplit('/').next().unwrap_or_default();
        u.slot_type == S::Sword as i32 || u.slot_type == S::LongSword as i32 || name.contains("Sword")
    }
}

impl GatewaySession {
    fn bag_const(&self, key: &str, default: i32) -> i32 {
        self.game_data.r#const.iter().find(|c| c.key == key).and_then(|c| c.value.trim().parse().ok()).unwrap_or(default)
    }

    /// Backpack size for the character type (race — role_type.meta.race, 1 — humans)
    pub(super) fn role_bag_size(&self, role_type: i32) -> BagSize {
        let human = self.game_data.role_type.get(&role_type).and_then(|r| r.meta.as_ref()).is_some_and(|m| m.race == tgf::EnRoleRaceType::EnRoleRaceHuman as i32);
        let rows = self.bag_const("role_backpack_default_row_size", 6) + if human { self.bag_const("role_backpack_race_add_row_size", 1) } else { 0 };
        let cols = self.bag_const("role_backpack_defualt_column_size", 10);
        BagSize { backpack: (rows, cols) }
    }

    /// Backpacks of all the player's characters — to the table sizes; true — something changed
    pub(super) fn apply_bag_sizes(&self, p: &mut PlayerModel) -> bool {
        let shape_of = |t: i32| self.inv_item_shape(t);
        let roles: Vec<(i64, i32)> = p.roles.iter().map(|r| (r.role_guid as i64, r.type_id as i32)).collect();
        let mut any = false;
        for (guid, ty) in roles {
            let (changed, moved) = fit_bags(p, guid, self.role_bag_size(ty), &shape_of);
            if moved > 0 {
                println!("[Inventory] {} role {}: backpack to race size, items moved {}", p.open_id, guid, moved);
            }
            any |= changed;
        }
        any
    }

    /// Character class for statistics: role_type.meta.career
    pub(super) fn role_career(&self, role_type: i32) -> i32 {
        self.game_data.role_type.get(&role_type).and_then(|r| r.meta.as_ref()).map_or(0, |m| m.career)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::PlayerModel;

    /// 09.11: gear above the limit — to the stash: worn not from the list, in the backpack — above the upper bound
    #[test]
    fn over_limit_gear_goes_to_stash() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 1 };
        let no_sword = |_t: i32| false;
        // type_id = rarity * 1000 (nominally), 9 and the player() set items — not gear
        let rarity = |t: i32| (t != 9 && t < 10000).then_some(t / 1000);
        let d = p.roles[0].data.deploy_role.as_mut().unwrap();
        d.backpack.as_mut().unwrap().items.push(item(31, 4000, at(BACKPACK, r, 0, 5))); // blue in the backpack
        d.backpack.as_mut().unwrap().items.push(item(32, 3000, at(BACKPACK, r, 1, 5))); // green in the backpack
        d.backpack.as_mut().unwrap().items.push(item(33, 9, at(BACKPACK, r, 2, 5))); // not gear
        let green = [0, 1, 2, 3];
        assert_eq!(stash_over_limit(&mut p, r, &green, &rarity, &no_sword, &shape), vec![4000]);
        assert_eq!(where_is(&mut p, 31).r#type, STASH);
        assert_eq!(where_is(&mut p, 32).r#type, BACKPACK);
        assert_eq!(where_is(&mut p, 33).r#type, BACKPACK);
        // Hard: anything goes in the backpack
        let d = p.roles[0].data.deploy_role.as_mut().unwrap();
        d.backpack.as_mut().unwrap().items.push(item(34, 2000, at(BACKPACK, r, 3, 5)));
        assert!(stash_over_limit(&mut p, r, &[5, 6, 7, 8], &rarity, &no_sword, &shape).is_empty());
    }

    /// 14.11: the sword master's class slots (101–109) and swords in the bags are not checked by the rarity limit; grey in the second weapon
    /// pair (a main slot) is taken off on Hard, a blue non-sword in the backpack on a "grey-green" map — too
    #[test]
    fn swords_outside_main_weapon_slots_are_not_checked() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 1 };
        let rarity = |t: i32| (t != 9 && t < 10000).then_some(t / 1000);
        // swords — type_id with the last digit 7
        let is_sword = |t: i32| t % 10 == 7;
        let d = p.roles[0].data.deploy_role.as_mut().unwrap();
        let w = &mut d.wearing.as_mut().unwrap().items;
        w.push(item(40, 2007, at(WEARING, r, 101, 0))); // grey sword in a class slot
        w.push(item(41, 4007, at(WEARING, r, 109, 0))); // blue sword in a class slot
        w.push(item(42, 2007, at(WEARING, r, 3, 0))); // grey sword in the second weapon pair — a main slot
        let bag = &mut d.backpack.as_mut().unwrap().items;
        bag.push(item(43, 4007, at(BACKPACK, r, 0, 5))); // blue sword in the backpack
        bag.push(item(44, 4001, at(BACKPACK, r, 1, 5))); // blue non-sword in the backpack
        assert!(in_class_weapon_slot(&item(0, 1, at(WEARING, r, 105, 0))) && !in_class_weapon_slot(&item(0, 1, at(WEARING, r, 4, 0))));
        assert_eq!(stash_over_limit(&mut p, r, &[5, 6, 7, 8], &rarity, &is_sword, &shape), vec![2007]);
        assert_eq!(where_is(&mut p, 42).r#type, STASH);
        assert_eq!(stash_over_limit(&mut p, r, &[0, 1, 2, 3], &rarity, &is_sword, &shape), vec![4001]);
        for g in [40, 41] {
            assert_eq!(where_is(&mut p, g).r#type, WEARING);
        }
        assert_eq!(where_is(&mut p, 43).r#type, BACKPACK);
        assert_eq!(where_is(&mut p, 44).r#type, STASH);
    }

    /// 09.11: a non-human — backpack 6×10, an item from the 7th column moves inside; a repeated call changes nothing
    #[test]
    fn backpack_shrinks_to_race_size() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 1 };
        let d = p.roles[0].data.deploy_role.as_mut().unwrap();
        d.backpack.as_mut().unwrap().items.push(item(20, 181011000, at(BACKPACK, r, 6, 0)));
        let elf = BagSize { backpack: (6, 10) };
        assert_eq!(fit_bags(&mut p, r, elf, &shape), (true, 1));
        let x = where_is(&mut p, 20);
        assert!(x.r#type == BACKPACK && x.x < 6, "from the 7th column — inside 6×10: {:?}", x);
        assert_eq!(p.roles[0].data.deploy_role.as_ref().unwrap().backpack.as_ref().unwrap().row_size, 6);
        assert_eq!(fit_bags(&mut p, r, elf, &shape), (false, 0));
        // a human — 7×10, nothing to move
        assert_eq!(fit_bags(&mut p, r, BagSize { backpack: (7, 10) }, &shape), (true, 0));
    }

    fn item(guid: i64, type_id: i32, pos: tgf::DItemPosition) -> tgf::DItem {
        tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id, count: 1, guid, position: Some(pos), ..Default::default() }),
            ..Default::default()
        }
    }
    fn at(t: i32, guid: i64, x: i32, y: i32) -> tgf::DItemPosition {
        tgf::DItemPosition { r#type: t, guid, x, y, ..Default::default() }
    }
    fn player() -> PlayerModel {
        let mut p = PlayerModel::new("t", 10000001);
        p.add_role(400002, "r".into(), tgf::DUserRole::default());
        let r = p.roles[0].role_guid as i64;
        let d = p.roles[0].data.deploy_role.get_or_insert_with(Default::default);
        d.wearing = Some(tgf::DRoleWearingData { items: vec![item(1, 181010000, at(WEARING, r, 1, 0))] });
        d.backpack = Some(tgf::DRoleBackpackData { items: vec![item(2, 181011000, at(BACKPACK, r, 3, 2))], row_size: 7, column_size: 10 });
        p.stash.element.push(item(3, 100013, at(STASH, 1, 5, 6)));
        p
    }
    fn where_is(p: &mut PlayerModel, guid: i64) -> tgf::DItemPosition {
        for place in all_places(p) {
            if let Some(it) = container_mut(p, &place).unwrap().iter().find(|i| guid_of(i) == guid) {
                return pos_of(it);
            }
        }
        panic!("guid {} is gone", guid)
    }

    #[test]
    fn wear_swaps_occupant_to_old_place() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let m = move_item(&mut p, 2, &at(WEARING, r, 1, 0), None).unwrap();
        assert_eq!(where_is(&mut p, 2), at(WEARING, r, 1, 0));
        assert_eq!(where_is(&mut p, 1), at(BACKPACK, r, 3, 2)); // the sword went to the place of the new one
        assert_eq!(m.dirty.len(), 4); // 2 "removed" + 2 full
        assert!(m.displaced.is_some());
    }

    #[test]
    fn swap_with_offset_goal_moves_other_to_specified_cell() {
        // rpc_dump 07.11 19:30:24 (seq 397): 3×1 swords. A (x2,y7) is put onto x1 over B (x0,y7), B → x4.
        // Before, B was looked up only exactly at x1 — not found, A was put over B, B stayed at x0, the push had only A.
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let bag = &mut p.roles[0].data.deploy_role.as_mut().unwrap().backpack.as_mut().unwrap().items;
        bag.clear();
        bag.push(item(10, 181010000, at(BACKPACK, r, 0, 7)));
        bag.push(item(11, 181010000, at(BACKPACK, r, 3, 7)));
        let m = swap_items(&mut p, 11, 10, &at(BACKPACK, r, 1, 7), Some(&at(BACKPACK, r, 4, 7))).unwrap();
        assert_eq!(where_is(&mut p, 11), at(BACKPACK, r, 1, 7));
        assert_eq!(where_is(&mut p, 10), at(BACKPACK, r, 4, 7));
        // both in the push: "removed" A and B, then full A and B at the new places
        assert_eq!(m.dirty.len(), 4);
        assert_eq!(pos_of(&m.dirty[3]), at(BACKPACK, r, 4, 7));
        // no cell given for B — B goes to A's old place
        swap_items(&mut p, 10, 11, &at(BACKPACK, r, 1, 7), None).unwrap();
        assert_eq!((where_is(&mut p, 10), where_is(&mut p, 11)), (at(BACKPACK, r, 1, 7), at(BACKPACK, r, 4, 7)));
        // the player does not have B — rejected, nothing touched
        assert!(swap_items(&mut p, 10, 999, &at(BACKPACK, r, 0, 0), None).is_err());
        assert_eq!(where_is(&mut p, 10), at(BACKPACK, r, 1, 7));
    }

    #[test]
    fn stash_and_back() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        move_item(&mut p, 3, &at(BACKPACK, r, 0, 0), None).unwrap();
        assert!(p.stash.element.is_empty());
        move_item(&mut p, 3, &at(STASH, 2, 1, 1), None).unwrap();
        assert_eq!(where_is(&mut p, 3), at(STASH, 2, 1, 1));
        assert_eq!(stash_pages(&p)[2].item_group[0].element.len(), 1);
    }

    #[test]
    fn failed_move_rolls_back() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        // the displaced item has nowhere to go (no role 999) — nothing must be lost
        assert!(move_item(&mut p, 2, &at(WEARING, r, 1, 0), Some(&at(BACKPACK, 999, 0, 0))).is_err());
        assert_eq!(where_is(&mut p, 1), at(WEARING, r, 1, 0));
        assert_eq!(where_is(&mut p, 2), at(BACKPACK, r, 3, 2));
        assert!(move_item(&mut p, 42, &at(STASH, 0, 0, 0), None).is_err());
    }

    #[test]
    fn free_cell_skips_occupied() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        // item 3 (100013) lies on page 1 at (5,6); all shapes 2×1
        let shape = |_t: i32| ItemShape { w: 2, h: 1, stack: 1 };
        assert!(!area_free(&mut p, &at(STASH, 1, 4, 6), 2, 1, 0, &shape));
        assert!(area_free(&mut p, &at(STASH, 1, 4, 6), 2, 1, 3, &shape)); // does not block itself
        assert!(!area_free(&mut p, &at(STASH, 1, 16, 0), 2, 1, 0, &shape)); // past the edge 17
        assert_eq!(free_cell(&mut p, &at(STASH, 1, 0, 0), 2, 1, &shape), Some(at(STASH, 1, 0, 0)));
        // backpack 7×10: (3,2) is taken by item 2 (2×1) → (2,2) and (3,2) are taken
        assert!(!area_free(&mut p, &at(BACKPACK, r, 2, 2), 2, 1, 0, &shape));
        assert!(grid_of(&p, &at(WEARING, r, 0, 0)).is_none());
        assert!(grid_of(&p, &at(STASH, STASH_PAGES, 0, 0)).is_none());
    }

    #[test]
    fn repair_gives_guid_and_role_position() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        // as from a DS result: a consumable without a guid, the position has no role
        let d = p.roles[0].data.deploy_role.as_mut().unwrap();
        d.backpack.as_mut().unwrap().items.push(item(0, 100047, at(BACKPACK, 0, 0, 8)));
        assert!(move_item(&mut p, 0, &at(STASH, 0, 0, 0), None).is_err()); // guid 0 is not moved
        assert_eq!(repair_items(&mut p), 1);
        let it = p.roles[0].data.deploy_role.as_ref().unwrap().backpack.as_ref().unwrap().items.iter().find(|i| i.basic.as_ref().unwrap().type_id == 100047).unwrap().clone();
        let g = guid_of(&it);
        assert!(g != 0 && !crate::guid::too_big(g));
        assert_eq!(pos_of(&it), at(BACKPACK, r, 0, 8));
        assert_eq!(repair_items(&mut p), 0); // again — nothing
        move_item(&mut p, g, &at(STASH, 0, 0, 0), None).unwrap();
        assert_eq!(where_is(&mut p, g), at(STASH, 0, 0, 0));
    }

    fn count_of(p: &mut PlayerModel, guid: i64) -> i32 {
        for place in all_places(p) {
            if let Some(it) = container_mut(p, &place).unwrap().iter().find(|i| guid_of(i) == guid) {
                return it.basic.as_ref().unwrap().count;
            }
        }
        0
    }

    #[test]
    fn stacks_merge_and_split_like_client() {
        // the case of 05.11 13:23:25: stack A ×3 (backpack y7), in the stash at y2 stack B ×1 of the same type, limit 3.
        // Client: 2 of A → onto B (B=3), the remaining 1 of A → onto the empty y8.
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        let mut a = item(54, 100016, at(BACKPACK, r, 0, 7));
        a.basic.as_mut().unwrap().count = 3;
        p.roles[0].data.deploy_role.as_mut().unwrap().backpack.as_mut().unwrap().items.push(a);
        p.stash.element.push(item(70, 100016, at(STASH, 0, 0, 2)));
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 3 };
        move_count(&mut p, 54, 2, &at(STASH, 0, 0, 2), &shape).unwrap();
        assert_eq!((count_of(&mut p, 70), count_of(&mut p, 54)), (3, 1));
        assert_eq!(where_is(&mut p, 70), at(STASH, 0, 0, 2));
        assert_eq!(where_is(&mut p, 54), at(BACKPACK, r, 0, 7));
        // the remainder whole onto an empty cell — the same stack is moved (guid 54)
        move_count(&mut p, 54, 1, &at(STASH, 0, 0, 8), &shape).unwrap();
        assert_eq!(where_is(&mut p, 54), at(STASH, 0, 0, 8));
        // part is not put into a full stack; a whole stack onto a full one — swap places (08.11), then back
        let mut half = item(55, 100016, at(STASH, 0, 5, 5));
        half.basic.as_mut().unwrap().count = 2;
        p.stash.element.push(half);
        assert!(move_count(&mut p, 55, 1, &at(STASH, 0, 0, 2), &shape).is_err());
        move_count(&mut p, 54, 1, &at(STASH, 0, 0, 2), &shape).unwrap();
        assert_eq!((where_is(&mut p, 54), where_is(&mut p, 70)), (at(STASH, 0, 0, 2), at(STASH, 0, 0, 8)));
        move_count(&mut p, 54, 1, &at(STASH, 0, 0, 8), &shape).unwrap();
        assert_eq!((where_is(&mut p, 54), where_is(&mut p, 70)), (at(STASH, 0, 0, 8), at(STASH, 0, 0, 2)));
        assert_eq!((count_of(&mut p, 54), count_of(&mut p, 70)), (1, 3));
        take(&mut p, 55);
        // split: 1 of B (×3) onto an empty cell — a new stack ×1 with a new guid, B=2
        let d = move_count(&mut p, 70, 1, &at(STASH, 0, 3, 3), &shape).unwrap();
        assert_eq!(count_of(&mut p, 70), 2);
        let new_guid = guid_of(d.last().unwrap());
        assert!(new_guid != 70 && new_guid != 0);
        assert_eq!((where_is(&mut p, new_guid), count_of(&mut p, new_guid)), (at(STASH, 0, 3, 3), 1));
        // the total number of pieces is kept: 2 + 1 + 1 (A at y8)
        assert_eq!(count_of(&mut p, 70) + count_of(&mut p, new_guid) + count_of(&mut p, 54), 4);
    }

    fn mv(guid: i64, count: i32, goal: tgf::DItemPosition) -> tgf::DUserInvetoryItemMoveAction {
        tgf::DUserInvetoryItemMoveAction {
            item_basic: Some(tgf::DItemBasic { guid, count, ..Default::default() }),
            goal_position: Some(goal),
            ..Default::default()
        }
    }
    fn stack(p: &mut PlayerModel, guid: i64, count: i32, pos: tgf::DItemPosition) {
        let mut it = item(guid, 100016, pos.clone());
        it.basic.as_mut().unwrap().count = count;
        match pos.r#type {
            STASH => p.stash.element.push(it),
            _ => p.roles[0].data.deploy_role.as_mut().unwrap().backpack.as_mut().unwrap().items.push(it),
        }
    }

    #[test]
    fn stack_request_is_all_or_nothing() {
        // rpc_dump 08.11 seq 374: in the inventory at (9,7) stack 611 ×3 (full), 580 ×3 is dragged onto it from the backpack.
        // The client sends [1 of 580 → (9,7), 2 of 580 → (9,8)]. The first — "full", the second used to pass:
        // 580 was split, the client considered it whole in the backpack. Now the whole request is rejected, the player is untouched.
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 3 };
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        stack(&mut p, 611, 3, at(STASH, 0, 9, 7));
        stack(&mut p, 580, 3, at(BACKPACK, r, 0, 0));
        let before = p.clone();
        assert!(apply_moves(&mut p, &[mv(580, 1, at(STASH, 0, 9, 7)), mv(580, 2, at(STASH, 0, 9, 8))], &shape).is_err());
        assert_eq!(p.stash.element, before.stash.element);
        assert_eq!((where_is(&mut p, 580), count_of(&mut p, 580)), (at(BACKPACK, r, 0, 0), 3));
        // the same gesture onto a non-full 611 ×2 (seq 371): 1 is topped up, the remainder 2 as the same stack (580) — to (9,8)
        take(&mut p, 611);
        stack(&mut p, 611, 2, at(STASH, 0, 9, 7));
        let mut start = p.clone();
        let dirty = apply_moves(&mut p, &[mv(580, 1, at(STASH, 0, 9, 7)), mv(580, 2, at(STASH, 0, 9, 8))], &shape).unwrap();
        assert_eq!((count_of(&mut p, 611), count_of(&mut p, 580)), (3, 2));
        assert_eq!(where_is(&mut p, 580), at(STASH, 0, 9, 8));
        // compaction: for 580 — one "removed" from the backpack and one final; for 611 — "removed" and a final ×3
        let c = compact_dirty(&mut start, &mut p, dirty);
        let of = |g: i64| c.iter().filter(|d| guid_of(d) == g).cloned().collect::<Vec<_>>();
        let (d580, d611) = (of(580), of(611));
        assert_eq!(d580.len(), 2);
        assert_eq!((pos_of(&d580[0]), d580[0].basic.as_ref().unwrap().count), (at(BACKPACK, r, 0, 0), 0));
        assert_eq!((pos_of(&d580[1]), d580[1].basic.as_ref().unwrap().count), (at(STASH, 0, 9, 8), 2));
        assert_eq!(d611.len(), 2);
        assert_eq!(d611[1].basic.as_ref().unwrap().count, 3);
        // all "removed" before all finals
        assert!(c.iter().take(2).all(|d| d.basic.as_ref().unwrap().count == 0));
        // a whole stack onto a full one — swap; an empty request and "nothing changed" — empty dirty
        let mut start = p.clone();
        let dirty = apply_moves(&mut p, &[mv(580, 2, at(STASH, 0, 9, 7))], &shape).unwrap();
        assert_eq!((where_is(&mut p, 580), where_is(&mut p, 611)), (at(STASH, 0, 9, 7), at(STASH, 0, 9, 8)));
        assert_eq!(compact_dirty(&mut start, &mut p, dirty).len(), 4);
        let mut same = p.clone();
        let rec = removal(&same.stash.element[0]);
        assert!(compact_dirty(&mut same, &mut p, vec![rec]).is_empty());
    }

    #[test]
    fn two_hander_takes_off_off_hand() {
        let mut p = player();
        let r = p.roles[0].role_guid as i64;
        // the left hand (slot 2) — a relic; a two-hander into slot 1 → the relic to the backpack at the given cell
        p.roles[0].data.deploy_role.as_mut().unwrap().wearing.as_mut().unwrap().items.push(item(7, 185010000, at(WEARING, r, 2, 0)));
        let shape = |_t: i32| ItemShape { w: 1, h: 1, stack: 1 };
        let m = take_off_off_hand(&mut p, r, 1, &at(BACKPACK, r, 0, 5), &shape).unwrap().unwrap();
        assert_eq!(where_is(&mut p, 7), at(BACKPACK, r, 0, 5));
        assert_eq!(guid_of(&m.item), 7);
        // the cell is taken (item 2 lies at (3,2), shape 1×1) → the first free one
        p.roles[0].data.deploy_role.as_mut().unwrap().wearing.as_mut().unwrap().items.push(item(8, 185010000, at(WEARING, r, 4, 0)));
        take_off_off_hand(&mut p, r, 3, &at(BACKPACK, r, 3, 2), &shape).unwrap().unwrap();
        assert_eq!(where_is(&mut p, 8), at(BACKPACK, r, 0, 0));
        // the left hand is empty / not a hand slot — nothing
        assert!(take_off_off_hand(&mut p, r, 1, &at(BACKPACK, r, 0, 0), &shape).unwrap().is_none());
        assert!(take_off_off_hand(&mut p, r, 14, &at(BACKPACK, r, 0, 0), &shape).unwrap().is_none());
    }

    #[test]
    fn weapon_set_slots_pair() {
        assert_eq!((other_set_slot(1), other_set_slot(2), other_set_slot(3), other_set_slot(4)), (3, 4, 1, 2));
    }
}

// ---- RPC wrappers (handle_<rpc> in handlers.rs delegates here) ----
impl GatewaySession {
    pub(super) async fn role_wear_impl(&self, req: crate::tgf::CsRoleWearReq) -> Option<crate::tgf::ScRoleWearRsp> {
        Some(self.inv_role_wear(req).await)
    }

    pub(super) async fn role_take_off_impl(&self, req: crate::tgf::CsRoleTakeOffReq) -> Option<crate::tgf::ScRoleTakeOffRsp> {
        Some(self.inv_role_take_off(req).await)
    }

    pub(super) async fn role_weapon_set_swap_impl(&self, req: crate::tgf::CsRoleWeaponSetSwapReq) -> Option<crate::tgf::ScRoleWeaponSetSwapRsp> {
        Some(self.inv_weapon_set_swap(req).await)
    }

    pub(super) async fn inventory_item_move_impl(&self, req: crate::tgf::CsInventoryItemMoveReq) -> Option<crate::tgf::ScInventoryItemMoveRsp> {
        Some(self.inv_item_move(req).await)
    }

    pub(super) async fn inventory_item_swap_impl(&self, req: crate::tgf::CsInventoryItemSwapReq) -> Option<crate::tgf::ScInventoryItemSwapRsp> {
        Some(self.inv_item_swap(req).await)
    }
}
