//! Progress: mail, quests, lottery, levels/map, match records and match settings.
//! The handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Mail — DB table `mail_box` (id = open_id): the whole box as one record, mails as protobuf bytes (DMailContent).
//! Mail format — as in the official server sniffer (mail_get_all, major_type 3): sender.profile.nick_name,
//! delivery/show/start/expired_time in seconds (expiry 30 days = const mail_default_expire), attachments in
//! attachments_detail (index in order, the full DItem with affix_data), status/minor_type/flags = 0 did not come there.
//! On login the client asks for major_type 3 (SYSTEM_LOGIC) and 4 (SYSTEM_SECURITY) — other tabs are not visible.
//! status — the EnMailStatusType bit mask: 1 read, 2 attachments taken, 4 deleted.
//! Other domains send mail via [`mail_send`] (+ [`mail_new`] / [`mail_system`] / [`mail_from_template`]).
//! Client Lua (tools/research/progress/lua_orig/.../Script/Mail): the window tabs — only 3 and 4; attachments of two kinds
//! with a shared index and received_items — attachments_offset (type+count, "virtual", ReceiveVirtualAttachments →
//! mail_receive_attachment by mail_ids) and attachments_detail (full DItems: WBP_Mail_Inventory_Panel, the player
//! arranges them into the stash/backpack → mail_receive_attachment_partly with places in DMailReciveKey.items).
//!
//! Stash: position type=1, guid = page (0..STASH_PAGES), x along the page's row_size (17), y along column_size (10).
//! An item takes position_cfg.row_size cells along x and column_size along y (UeSource_ItemType via
//! item_type.ue_source_type_id) — checked against the stash layout in rpc_dump PVE_battle_0720 (only this way without overlaps).
//!
//! Quests — state["progress_quests"][id] = {status, values[], created, completed, received}; the snapshot for the client —
//! [`GatewaySession::progress_quests_snapshot`] (PVE player_get_info.user_quest format: processing_quests by progress type id
//! → quest_progress_list by quest_id). Rankings — rank.rs.
//! level_*/map_move are not called in the current match flow (level_select → battle_matching_* → DS): they are not in rpc_dump PVE/OUR;
//! the sniffer has only level_finish (exit_reason FORCE_SELF_QUIT mid-match), the response — an echo of level_data.
//! So they change nothing in the match: echo + a record in state.

use super::common::{self, GOLD};
use super::inventory_handlers as inv;
use super::GatewaySession;
use crate::db::GameDb;
use crate::game_data::GameData;
use crate::structures::PlayerModel;
use crate::tgf;
use serde::{Deserialize, Serialize};
use serde_json::json;

const STASH: i32 = 1;
/// stash page size (as stash_pages in inventory_handlers): x < 17, y < 10
const STASH_X: i32 = 17;
const STASH_Y: i32 = 10;

const MAIL_TABLE: &str = "mail_box";
const MAIL_CHANGE_METHOD: &str = "tgf.GameClientService.mail_change_sync";
const MAIL_CHANGE_TYPE: &str = "tgf.SCMailChangeSync";
const MAIL_ST_READ: i32 = 1;
const MAIL_ST_TAKEN: i32 = 2;
const MAIL_ST_REMOVED: i32 = 4;
const MAIL_FLAG_KEEP: i32 = 1;
/// EN_MAIL_MT_SYSTEM_LOGIC — the tab where the official server puts system mails
pub(super) const MAIL_MT_SYSTEM: i32 = 3;
/// const.json: mail_default_expire = 30d, user_mail_max_count_per_major_type = 100, mail_detail_item_max_size = 15
const MAIL_EXPIRE_S: i64 = 30 * 24 * 3600;
const MAIL_MAX_PER_TYPE: usize = 100;
pub(super) const MAIL_MAX_ITEMS: usize = 15;
/// const.json: i18n_system_admin
const MAIL_SYSTEM_SENDER: &str = "System";
/// EN_MAIL_MT_FRIEND_GIFT — a friend's gift. The client's mail window (Lua WBP_MailHeaderList) shows only tabs
/// 3 and 4 — a mail of this type is not visible there. TODO(verify): where the client shows gifts
#[allow(dead_code)]
pub(super) const MAIL_MT_FRIEND_GIFT: i32 = 21;
/// DMailOperationResult.result (EnErrorCode)
const ERR_MAIL_NOT_FOUND: i32 = tgf::EnErrorCode::EnErrMailNotFound as i32;
const ERR_MAIL_ALREADY_RECEIVED: i32 = tgf::EnErrorCode::EnErrMailAttachmentsAlreadyReceived as i32;
const ERR_MAIL_PARAMS: i32 = tgf::EnErrorCode::EnErrMailReceiveMailAttachmentsParamsInvalid as i32;

const BATTLE_RECORDS_MAX: usize = 50;

/// All "progress" records in shared tables (mail, rankings) — read-modify-write under one lock,
/// otherwise a mail from another domain may get lost while attachments are being received at the same time.
static PROGRESS_DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// A protobuf message as bytes in SurrealDB (like prost_bytes in structures.rs)
mod pbytes {
    use prost::Message;
    use serde::{de::Visitor, Deserializer, Serializer};

    pub fn serialize<T: Message, S: Serializer>(v: &T, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(&v.encode_to_vec())
    }

    pub fn deserialize<'de, T: Message + Default, D: Deserializer<'de>>(d: D) -> Result<T, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Vec<u8>, E> {
                Ok(v.to_vec())
            }
            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Vec<u8>, E> {
                Ok(v)
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Vec<u8>, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = a.next_element::<u8>()? {
                    out.push(b);
                }
                Ok(out)
            }
        }
        let bytes = d.deserialize_any(V)?;
        T::decode(bytes.as_slice()).map_err(serde::de::Error::custom)
    }
}

// =====================================================================================================
// Granting items into the stash (mail attachments, quest rewards)
// =====================================================================================================

/// (cells along x, cells along y, stack maximum)
type Footprint = (i32, i32, i32);

fn footprint(gd: &GameData, type_id: i32) -> Footprint {
    let ue_id = gd.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|id| *id != 0).unwrap_or(type_id);
    match gd.uesource_itemtype.get(&ue_id).and_then(|u| u.position_cfg.as_ref()) {
        Some(c) => (c.row_size.max(1), c.column_size.max(1), if c.accumulation_limit > 0 { c.accumulation_limit } else { i32::MAX }),
        // like inventory_handlers::ItemShape::default — without a record we do not stack
        None => (1, 1, 1),
    }
}

fn is_currency(gd: &GameData, type_id: i32) -> bool {
    type_id == GOLD || gd.item_type.get(&type_id).map(|t| t.item_major_type == tgf::EnItemMajorType::Currency as i32).unwrap_or(false)
}

/// Stash cell occupancy [page][x][y]
fn stash_grid(p: &PlayerModel, fp: &dyn Fn(i32) -> Footprint) -> Vec<Vec<[bool; STASH_Y as usize]>> {
    let pages = super::inventory_handlers::stash_page_count(p); // ext: stash_pages — the player's pages, not 8
    let mut taken = vec![vec![[false; STASH_Y as usize]; STASH_X as usize]; pages as usize];
    for it in &p.stash.element {
        let Some(b) = it.basic.as_ref() else { continue };
        let Some(pos) = b.position.as_ref().filter(|pos| pos.r#type == STASH && (0..pages).contains(&pos.guid)) else { continue };
        let (iw, ih, _) = fp(b.type_id);
        for x in pos.x.max(0)..(pos.x + iw).min(STASH_X) {
            for y in pos.y.max(0)..(pos.y + ih).min(STASH_Y) {
                taken[pos.guid as usize][x as usize][y as usize] = true;
            }
        }
    }
    taken
}

/// Whether a w×h item fits into the stash at `pos` (within the page and onto free cells)
fn stash_fits(grid: &[Vec<[bool; STASH_Y as usize]>], pos: &tgf::DItemPosition, w: i32, h: i32) -> bool {
    pos.r#type == STASH
        && (0..grid.len() as i64).contains(&pos.guid)
        && pos.x >= 0
        && pos.y >= 0
        && pos.x + w <= STASH_X
        && pos.y + h <= STASH_Y
        && (pos.x..pos.x + w).all(|x| (pos.y..pos.y + h).all(|y| !grid[pos.guid as usize][x as usize][y as usize]))
}

/// A free stash spot for a w×h item: pages in order, within a page — by x, then by y
fn free_stash_cell(p: &PlayerModel, fp: &dyn Fn(i32) -> Footprint, w: i32, h: i32) -> Option<tgf::DItemPosition> {
    let grid = stash_grid(p, fp);
    for page in 0..grid.len() as i64 {
        for x in 0..=(STASH_X - w) {
            for y in 0..=(STASH_Y - h) {
                let pos = tgf::DItemPosition { r#type: STASH, guid: page, x, y, ..Default::default() };
                if stash_fits(&grid, &pos, w, h) {
                    return Some(pos);
                }
            }
        }
    }
    None
}

/// The backpack (2) / small backpack (4) of role guid — if the role already has it. The cells are checked by grant_items: on "take
/// into the backpack" the client sends a place without x/y (0,0), and the item was put over an occupied cell — the inventory broke (03.11 13:26).
fn role_bag<'a>(p: &'a mut PlayerModel, pos: &tgf::DItemPosition) -> Option<&'a mut Vec<tgf::DItem>> {
    let d = p.roles.iter_mut().find(|r| r.role_guid as i64 == pos.guid)?.data.deploy_role.as_mut()?;
    match pos.r#type {
        2 => d.backpack.as_mut().map(|b| &mut b.items),
        4 => d.small_backpack.as_mut().map(|b| &mut b.items),
        _ => None,
    }
}

fn has_guid(p: &PlayerModel, guid: i64) -> bool {
    let hit = |l: &Vec<tgf::DItem>| l.iter().any(|it| it.basic.as_ref().map(|b| b.guid) == Some(guid));
    hit(&p.stash.element)
        || p.roles.iter().filter_map(|r| r.data.deploy_role.as_ref()).any(|d| {
            d.backpack.as_ref().map(|b| hit(&b.items)).unwrap_or(false)
                || d.small_backpack.as_ref().map(|b| hit(&b.items)).unwrap_or(false)
                || d.wearing.as_ref().map(|b| hit(&b.items)).unwrap_or(false)
        })
}

/// What was granted: items (full, at new places — for dirty_postion_action) and coin ids
#[derive(Default)]
struct Granted {
    items: Vec<tgf::DItem>,
    coins: Vec<i32>,
}

impl Granted {
    fn dirty(&self, p: &PlayerModel) -> tgf::ScPlayerDirtyChgSync {
        let mut coins = self.coins.clone();
        coins.sort();
        coins.dedup();
        tgf::ScPlayerDirtyChgSync {
            dirty_postion_action: self.items.clone(),
            dirty_coin_data: coins.into_iter().map(|c| common::coin_dirty(p, c)).collect(),
            ..Default::default()
        }
    }
    fn is_empty(&self) -> bool {
        self.items.is_empty() && self.coins.is_empty()
    }
    fn extend(&mut self, o: Granted) {
        self.items.extend(o.items);
        self.coins.extend(o.coins);
    }
}

/// Grant error: an EnErrorCode + text for the log
type GrantErr = (i32, String);
const ERR_NO_SPACE: i32 = tgf::EnErrorCode::EnErrItemSuitablePositionNotFound as i32;

/// Put items to the player: currency — into coins, the rest — to the place the client chose (`Some(pos)`: the stash —
/// if the cells are free, the role's backpack/small backpack), otherwise to a free stash spot. The item guid is kept if
/// it is not 0 and the player has no such guid, otherwise a new one; stacks larger than accumulation_limit are split.
/// All or nothing: did not fit — the player does not change. `fp` — item sizes, `currency` — whether the type is a currency.
fn grant_items(p: &mut PlayerModel, items: Vec<(tgf::DItem, Option<tgf::DItemPosition>)>, fp: &dyn Fn(i32) -> Footprint, currency: &dyn Fn(i32) -> bool) -> Result<Granted, GrantErr> {
    let backup = p.clone();
    let res = grant_items_unchecked(p, items, fp, currency);
    if res.is_err() {
        *p = backup;
    }
    res
}

fn grant_items_unchecked(p: &mut PlayerModel, items: Vec<(tgf::DItem, Option<tgf::DItemPosition>)>, fp: &dyn Fn(i32) -> Footprint, currency: &dyn Fn(i32) -> bool) -> Result<Granted, GrantErr> {
    let mut out = Granted::default();
    for (item, want) in items {
        let Some(basic) = item.basic.clone() else { continue };
        let count = basic.count.max(1);
        if currency(basic.type_id) {
            common::add_coin(p, basic.type_id, count as i64).ok_or((tgf::EnErrorCode::EnErrUnknown as i32, format!("coin {} was not added", basic.type_id)))?;
            out.coins.push(basic.type_id);
            continue;
        }
        let (w, h, limit) = fp(basic.type_id);
        let mut left = count;
        let mut want = want.filter(|w| w.r#type != 0).map(|w| tgf::DItemPosition { limit_option: None, ..w });
        let shape = |t: i32| {
            let (w, h, stack) = fp(t);
            inv::ItemShape { w, h, stack }
        };
        // keep the attachment's own guid if it fits into the client's 32 bits and is not taken
        let mut keep_guid = Some(basic.guid).filter(|g| *g != 0 && !crate::guid::too_big(*g) && !has_guid(p, *g));
        while left > 0 {
            let n = left.min(limit);
            left -= n;
            let guid = keep_guid.take().unwrap_or_else(|| crate::guid::next_item());
            let mut it = item.clone();
            let mut b = tgf::DItemBasic { count: n, guid, ..basic.clone() };
            // the client's place — only for the first stack
            // the role's backpack: the client's cell if free, otherwise the first free one there; no room — to the stash
            let wanted = want.take();
            let in_bag = wanted.clone().filter(|pos| pos.r#type != STASH && role_bag(p, pos).is_some()).and_then(|pos| {
                if inv::area_free(p, &pos, w, h, 0, &shape) {
                    Some(pos)
                } else {
                    inv::free_cell(p, &pos, w, h, &shape)
                }
            });
            match in_bag {
                Some(pos) => {
                    b.position = Some(pos.clone());
                    it.basic = Some(b);
                    role_bag(p, &pos).unwrap().push(it.clone());
                }
                None => {
                    let other = wanted.filter(|pos| pos.r#type == STASH);
                    let pos = other
                        .filter(|pos| stash_fits(&stash_grid(p, fp), pos, w, h))
                        .or_else(|| free_stash_cell(p, fp, w, h))
                        .ok_or((ERR_NO_SPACE, format!("no room in the stash for {} ({}x{})", basic.type_id, w, h)))?;
                    b.position = Some(pos);
                    it.basic = Some(b);
                    p.stash.element.push(it.clone());
                }
            }
            out.items.push(it);
        }
    }
    Ok(out)
}

fn offset_item(o: &tgf::DItemOffset, now: i64) -> tgf::DItem {
    tgf::DItem {
        basic: Some(tgf::DItemBasic {
            type_id: o.type_id,
            count: o.count.max(1),
            expire_timepoint: if o.expire_offset > 0 { now + o.expire_offset } else { 0 },
            ..Default::default()
        }),
        ..Default::default()
    }
}

// =====================================================================================================
// Mail: storage and sending (API for other domains)
// =====================================================================================================

#[derive(Serialize, Deserialize, Clone, Default)]
struct MailBox {
    next_id: i64,
    #[serde(with = "pbytes")]
    list: tgf::ScMailGetAllRsp,
}

async fn mail_load(db: &GameDb, open_id: &str) -> MailBox {
    db.get::<MailBox>(MAIL_TABLE, open_id).await.unwrap_or_default()
}

async fn mail_store(db: &GameDb, open_id: &str, b: &MailBox) {
    db.save(MAIL_TABLE, open_id, b).await;
}

/// 16.11 (wipe): the player's box for the snapshot — (next_id, mails as protobuf, number of mails); the box does not change
pub(super) async fn mail_snapshot(db: &GameDb, open_id: &str) -> (i64, Vec<u8>, usize) {
    let b = mail_load(db, open_id).await;
    (b.next_id, prost::Message::encode_to_vec(&b.list), b.list.mails.len())
}

/// 16.11 (wipe / unwipe): write the whole box; an empty `list` — a box without mails (next_id is kept)
pub(super) async fn mail_replace(db: &GameDb, open_id: &str, next_id: i64, list: &[u8]) -> Result<(), String> {
    let list = <tgf::ScMailGetAllRsp as prost::Message>::decode(list).map_err(|e| format!("box in the snapshot: {}", e))?;
    mail_store(db, open_id, &MailBox { next_id, list }).await;
    Ok(())
}

fn mail_record(m: &tgf::DMailContent) -> tgf::DMailRecord {
    tgf::DMailRecord {
        mail_id: m.mail_id,
        major_type: m.major_type,
        minor_type: m.minor_type,
        status: m.status,
        delivery_time: m.delivery_time,
        show_time: m.show_time,
        start_time: m.start_time,
        expired_time: m.expired_time,
        remove_time: m.remove_time,
        resolve_expired_time: m.resolve_expired_time,
        limit_option: m.limit_option.clone(),
        received_items: m.received_items.clone(),
        is_global_mail: false,
        fetch_error_count: 0,
    }
}

fn mail_visible(m: &tgf::DMailContent, now: i64) -> bool {
    m.status & MAIL_ST_REMOVED == 0 && (m.expired_time == 0 || m.expired_time > now) && m.show_time <= now
}

/// Indices of attachments (attachments_offset and attachments_detail — a shared index count) that are not taken yet
fn mail_pending(m: &tgf::DMailContent) -> Vec<i32> {
    m.attachments_offset.iter().map(|a| a.index).chain(m.attachments_detail.iter().map(|a| a.index)).filter(|i| !m.received_items.contains(i)).collect()
}

/// The item of attachment `index`: from attachments_detail as is, from attachments_offset — a new one by type/count
fn mail_item_at(m: &tgf::DMailContent, index: i32, now: i64) -> Option<tgf::DItem> {
    m.attachments_detail
        .iter()
        .find(|a| a.index == index)
        .and_then(|a| a.item.clone())
        .or_else(|| m.attachments_offset.iter().find(|a| a.index == index).and_then(|a| a.item.as_ref()).map(|o| offset_item(o, now)))
}

/// Sender "System" (const.json i18n_system_admin), as in the sniffer
pub(super) fn mail_system_sender() -> tgf::DMailUserInfo {
    tgf::DMailUserInfo { profile: Some(tgf::DPlayerProfile { nick_name: MAIL_SYSTEM_SENDER.to_string(), ..Default::default() }), ..Default::default() }
}

/// A mail of tab `major_type` (EnMailMajorType: 3 system, 4 security, 21 friend gift...) from `sender`.
/// Attachments — any DItems (mail_send moves currency to attachments_offset); the mail is kept after the attachments are received.
pub(super) fn mail_new(major_type: i32, sender: tgf::DMailUserInfo, title: &str, content: &str, items: Vec<tgf::DItem>) -> tgf::DMailContent {
    tgf::DMailContent {
        major_type,
        title: title.to_string(),
        content: content.to_string(),
        sender: Some(sender),
        flags: MAIL_FLAG_KEEP,
        attachments_detail: items.into_iter().map(|it| tgf::DMailItem { index: 0, item: Some(it) }).collect(),
        ..Default::default()
    }
}

/// A system mail (sender "System", major_type 3)
pub(super) fn mail_system(title: &str, content: &str, items: Vec<tgf::DItem>) -> tgf::DMailContent {
    mail_new(MAIL_MT_SYSTEM, mail_system_sender(), title, content, items)
}

/// A mail from a mail_template (title/text — client localization keys), the template's attachments + `extra`.
/// `ext` — {{extension(KEY)}} substitutions in the template text.
pub(super) fn mail_from_template(gd: &GameData, template_id: i32, extra: Vec<tgf::DItem>, ext: &[(&str, String)]) -> Option<tgf::DMailContent> {
    let cfg = gd.mail_template.get(&template_id)?.config.clone()?;
    let now = GatewaySession::now_ts();
    let mut m = mail_system(&cfg.title, &cfg.content, extra);
    m.attachments_offset = cfg.attachments.iter().map(|o| tgf::DMailItemOffset { index: 0, item: Some(o.clone()) }).collect();
    m.major_type = if cfg.major_type != 0 { cfg.major_type } else { MAIL_MT_SYSTEM };
    m.minor_type = cfg.minor_type;
    m.flags = if cfg.keep_after_take_attachments { MAIL_FLAG_KEEP } else { 0 };
    m.mail_template_id = template_id;
    m.mail_template_extensions = ext.iter().map(|(k, v)| (k.to_string(), v.clone())).collect();
    let ts = |t: &Option<crate::google::protobuf::Timestamp>| t.as_ref().map(|t| t.seconds).unwrap_or(0);
    m.show_time = ts(&cfg.show_time).min(now);
    m.start_time = ts(&cfg.start_time).min(now);
    m.expired_time = match (ts(&cfg.expired_time), cfg.expired_duration_s) {
        (e, _) if e > now => e,
        (_, d) if d > 0 => now + d as i64,
        _ => 0, // mail_send will fill it (30 days)
    };
    Some(m)
}

/// Attachments into the client's view: currency from detail → attachments_offset; a shared index count (first offset, then
/// detail); at most 15 in total; items without a guid get a mail guid (the client refers to it in DMailReciveKey.items,
/// the guid is kept on receipt).
fn mail_normalize(gd: &GameData, m: &mut tgf::DMailContent) {
    let mut offsets: Vec<tgf::DItemOffset> = m.attachments_offset.drain(..).filter_map(|a| a.item).collect();
    let mut details = Vec::new();
    for it in m.attachments_detail.drain(..).filter_map(|a| a.item) {
        let b = it.basic.clone().unwrap_or_default();
        if is_currency(gd, b.type_id) {
            offsets.push(tgf::DItemOffset { type_id: b.type_id, count: b.count.max(1), expire_offset: 0 });
        } else {
            details.push(it);
        }
    }
    offsets.truncate(MAIL_MAX_ITEMS);
    details.truncate(MAIL_MAX_ITEMS - offsets.len());
    let k = offsets.len() as i32;
    m.attachments_offset = offsets.into_iter().enumerate().map(|(i, o)| tgf::DMailItemOffset { index: i as i32, item: Some(o) }).collect();
    m.attachments_detail = details
        .into_iter()
        .enumerate()
        .map(|(i, mut it)| {
            let index = k + i as i32;
            if let Some(b) = it.basic.as_mut() {
                // attachment guid — from the shared counter (the client refers to it in the receive key)
                if b.guid == 0 || crate::guid::too_big(b.guid) {
                    b.guid = crate::guid::next_item();
                }
                b.position = None;
            }
            tgf::DMailItem { index, item: Some(it) }
        })
        .collect();
}

/// Send a mail to player `open_id` (may be offline). Fills mail_id, empty times (expiry 30 days), arranges
/// the attachments (mail_normalize). An online player gets a mail_change_sync push with the new mail right away. Returns the mail_id.
pub(super) async fn mail_send(db: &GameDb, gd: &GameData, open_id: &str, mut mail: tgf::DMailContent) -> i64 {
    let now = GatewaySession::now_ts();
    {
        let _g = PROGRESS_DB_LOCK.lock().await;
        let mut b = mail_load(db, open_id).await;
        b.next_id = b.next_id.max(b.list.mails.iter().map(|m| m.mail_id).max().unwrap_or(0)) + 1;
        mail.mail_id = b.next_id;
        if mail.major_type == 0 {
            mail.major_type = MAIL_MT_SYSTEM;
        }
        mail.status = 0;
        mail.received_items.clear();
        mail.delivery_time = if mail.delivery_time > 0 { mail.delivery_time } else { now };
        for t in [&mut mail.show_time, &mut mail.start_time] {
            if *t == 0 {
                *t = mail.delivery_time;
            }
        }
        if mail.expired_time == 0 {
            mail.expired_time = mail.delivery_time + MAIL_EXPIRE_S;
        }
        mail_normalize(gd, &mut mail);
        // limit of 100 mails per tab: push out the oldest ones
        let same: Vec<i64> = b.list.mails.iter().filter(|m| m.major_type == mail.major_type).map(|m| m.mail_id).collect();
        if same.len() >= MAIL_MAX_PER_TYPE {
            let drop: Vec<i64> = same[..same.len() + 1 - MAIL_MAX_PER_TYPE].to_vec();
            b.list.mails.retain(|m| !drop.contains(&m.mail_id));
        }
        b.list.mails.push(mail.clone());
        mail_store(db, open_id, &b).await;
    }
    let online = common::push_to(open_id, MAIL_CHANGE_METHOD, MAIL_CHANGE_TYPE, &tgf::ScMailChangeSync { mails: vec![], add_mails: vec![mail.clone()] });
    println!(
        "[Mail] -> {}: mail {} (type {}) \"{}\", attachments {}+{}{}",
        open_id,
        mail.mail_id,
        mail.major_type,
        mail.title,
        mail.attachments_offset.len(),
        mail.attachments_detail.len(),
        if online { " (online, push)" } else { "" }
    );
    mail.mail_id
}

/// Which attachments to take: None — all remaining; Some — (index, the place chosen by the client)
type MailWant = Option<Vec<(i32, Option<tgf::DItemPosition>)>>;

/// A quest progress event (Q1) and what is known about it for condition_ids
#[derive(Clone, Debug)]
pub(super) struct QuestEvent {
    pub kind: tgf::EnQuestProgressType,
    pub value: i64,
    pub role_guid: Option<u64>,
    pub level_type: Option<i64>,
    pub level_id: Option<i64>,
    pub item_type: Option<i32>,
    pub item_chg: Option<i32>,
    /// Target type for quest_list.progress.entity_args (08.11: 11 — monsters KILL_TYPE, 41 — chests LOOT, 1 — players
    /// PLAYER_CAUSED_TOTAL_DAMAGE); None — do not check
    pub entity: Option<i32>,
}

impl QuestEvent {
    pub fn new(kind: tgf::EnQuestProgressType, value: i64, role_guid: u64) -> Self {
        QuestEvent { kind, value, role_guid: Some(role_guid).filter(|g| *g != 0), level_type: None, level_id: None, item_type: None, item_chg: None, entity: None }
    }
}

impl GatewaySession {
    /// Apply an operation to mails `ids` (None — all visible mails of tab major/minor; 0 = any).
    /// `take` — take the attachments (otherwise only read). Loads the box and the player, saves, sends dirty.
    async fn mail_apply(&self, ids: Option<Vec<(i64, MailWant)>>, filter: (i32, i32), take: bool, need_remove: bool) -> Vec<tgf::DMailOperationResult> {
        let open_id = self.clean_open_id();
        let now = Self::now_ts();
        let gd = self.game_data.clone();
        let fp = |t: i32| footprint(&gd, t);
        let cur = |t: i32| is_currency(&gd, t);
        let _g = PROGRESS_DB_LOCK.lock().await;
        let mut b = mail_load(&self.db, &open_id).await;
        let mut player = self.load_me().await;
        let targets: Vec<(i64, MailWant)> = ids.unwrap_or_else(|| {
            b.list
                .mails
                .iter()
                .filter(|m| mail_visible(m, now) && (filter.0 == 0 || m.major_type == filter.0) && (filter.1 == 0 || m.minor_type == filter.1))
                .filter(|m| !take || !mail_pending(m).is_empty())
                .map(|m| (m.mail_id, None))
                .collect()
        });
        let mut results = Vec::new();
        let mut granted = Granted::default();
        let mut changed: Vec<tgf::DMailRecord> = Vec::new();
        for (id, want) in targets {
            let Some(m) = b.list.mails.iter_mut().find(|m| m.mail_id == id && mail_visible(m, now)) else {
                println!("[Mail] {} mail {}: no such mail", open_id, id);
                results.push(tgf::DMailOperationResult { result: ERR_MAIL_NOT_FOUND, record: Some(tgf::DMailRecord { mail_id: id, ..Default::default() }) });
                continue;
            };
            let mut result = 0;
            if take {
                let pending = mail_pending(m);
                let picked: Vec<(i32, Option<tgf::DItemPosition>)> = match want {
                    None => pending.iter().map(|i| (*i, None)).collect(),
                    Some(w) => w.into_iter().filter(|(i, _)| pending.contains(i)).collect(),
                };
                if picked.is_empty() {
                    result = if pending.is_empty() { ERR_MAIL_ALREADY_RECEIVED } else { ERR_MAIL_PARAMS };
                } else {
                    let items = picked.iter().filter_map(|(i, pos)| Some((mail_item_at(m, *i, now)?, pos.clone()))).collect();
                    match grant_items(&mut player, items, &fp, &cur) {
                        Ok(g) => {
                            m.received_items.extend(picked.iter().map(|(i, _)| *i));
                            granted.extend(g);
                        }
                        Err((code, e)) => {
                            println!("[Mail] {} mail {}: attachments not granted — {}", open_id, id, e);
                            result = code;
                        }
                    }
                }
                if mail_pending(m).is_empty() {
                    m.status |= MAIL_ST_TAKEN;
                }
            }
            m.status |= MAIL_ST_READ;
            let had_items = !m.attachments_detail.is_empty() || !m.attachments_offset.is_empty();
            if mail_pending(m).is_empty() && (need_remove || (take && had_items && m.flags & MAIL_FLAG_KEEP == 0)) {
                m.status |= MAIL_ST_REMOVED;
                m.remove_time = now;
            }
            results.push(tgf::DMailOperationResult { result, record: Some(mail_record(m)) });
            changed.push(mail_record(m));
        }
        // deleted and expired mails are not kept in the box
        b.list.mails.retain(|m| m.status & MAIL_ST_REMOVED == 0 && (m.expired_time == 0 || m.expired_time > now));
        mail_store(&self.db, &open_id, &b).await;
        if !granted.is_empty() {
            self.save_me(&player).await;
            Self::write_client_loadout_file(&player);
            println!("[Mail] {}: received from attachments {} items, coins {}", open_id, granted.items.len(), granted.coins.len());
            self.push_dirty(granted.dirty(&player));
        }
        // the client's mail list — from mail_get_all and mail_change_sync; without a push a deleted mail stayed in the list, and
        // the attachments of a mail with received attachments could be "taken" again (03.11 13:26). mails — DMailRecords of changed
        // mails (status: read / taken / deleted, remove_time, received_items), add_mails — new ones.
        // TODO(verify): whether the client removes a mail with status REMOVED in mails from the list (there is no mail_change_sync reference)
        if !changed.is_empty() {
            self.push(MAIL_CHANGE_METHOD, MAIL_CHANGE_TYPE, tgf::ScMailChangeSync { mails: changed, add_mails: vec![] });
        }
        results
    }

    pub(super) async fn mail_get_all_impl(&self, req: tgf::CsMailGetAllReq) -> Option<tgf::ScMailGetAllRsp> {
        let now = Self::now_ts();
        let b = mail_load(&self.db, &self.clean_open_id()).await;
        let mut mails: Vec<tgf::DMailContent> = b.list.mails.into_iter().filter(|m| mail_visible(m, now) && m.major_type == req.major_type).collect();
        mails.sort_by(|a, b| b.delivery_time.cmp(&a.delivery_time).then(b.mail_id.cmp(&a.mail_id)));
        let total = mails.len() as i32;
        // the official server responds without page_*; paged — only if the client asked for it itself
        // TODO(verify): page_no from 0 or from 1 — in the dumps the client always sends only major_type
        if req.page_count > 0 {
            let skip = (req.page_no.max(1) - 1) as usize * req.page_count as usize;
            mails = mails.into_iter().skip(skip).take(req.page_count as usize).collect();
        }
        Some(tgf::ScMailGetAllRsp { mails, page_count: req.page_count, page_no: req.page_no, mail_count: total, major_type: req.major_type })
    }

    /// Lua MailService: MarkAsRead(ids) and DeleteMails(ids) (need_remove). The client does not allow deleting mails with
    /// untaken attachments (MailInvalidDeletion) — we keep them too.
    pub(super) async fn mail_read_impl(&self, req: tgf::CsMailReadReq) -> Option<tgf::ScMailReadRsp> {
        let ids = req.mail_ids.iter().map(|id| (*id, None)).collect();
        Some(tgf::ScMailReadRsp { mails: self.mail_apply(Some(ids), (0, 0), false, req.need_remove).await })
    }

    pub(super) async fn mail_read_all_impl(&self, req: tgf::CsMailReadAllReq) -> Option<tgf::ScMailReadAllRsp> {
        Some(tgf::ScMailReadAllRsp { mails: self.mail_apply(None, (req.major_type, req.minor_type), false, req.need_remove).await })
    }

    /// Lua: ReceiveVirtualAttachments(MailIds, NeedRemove) — without places, we arrange them ourselves
    pub(super) async fn mail_receive_attachment_impl(&self, req: tgf::CsMailReceiveAttachmentsReq) -> Option<tgf::ScMailReceiveAttachmentsRsp> {
        let mut ids: Vec<(i64, MailWant)> = req.mail_ids.iter().map(|id| (*id, None)).collect();
        ids.extend(self.mail_keys(&req.mail_key).await);
        Some(tgf::ScMailReceiveAttachmentsRsp { mails: self.mail_apply(Some(ids), (0, 0), true, req.need_remove).await })
    }

    pub(super) async fn mail_receive_attachment_all_impl(&self, req: tgf::CsMailReceiveAttachmentsAllReq) -> Option<tgf::ScMailReceiveAttachmentsAllRsp> {
        Some(tgf::ScMailReceiveAttachmentsAllRsp { mails: self.mail_apply(None, (req.major_type, req.minor_type), true, req.need_remove).await })
    }

    /// Lua WBP_Mail_Inventory_Panel: the player drags items from the mail's "virtual" inventory into the stash /
    /// the backpack, then ReceiveAttachments → keys with the item guids and the chosen places
    pub(super) async fn mail_receive_attachment_partly_impl(&self, req: tgf::CsMailReceiveAttachmentsPartlyReq) -> Option<tgf::ScMailReceiveAttachmentsPartlyRsp> {
        let ids = self.mail_keys(&req.mail_key).await;
        Some(tgf::ScMailReceiveAttachmentsPartlyRsp { mails: self.mail_apply(Some(ids), (0, 0), true, req.need_remove).await })
    }

    /// DMailReciveKey → (mail_id, [(index, place)]). items[i] — an attachment item (by guid, otherwise by type_id) with the place
    /// where the player put it; indexs — the indices (if there are as many of them as items — paired with items[i]).
    /// An empty key — all attachments. TODO(verify): the exact items ↔ indexs relation — from a live client request.
    async fn mail_keys(&self, keys: &[tgf::DMailReciveKey]) -> Vec<(i64, MailWant)> {
        if keys.is_empty() {
            return vec![];
        }
        let b = mail_load(&self.db, &self.clean_open_id()).await;
        keys.iter()
            .map(|k| {
                let m = b.list.mails.iter().find(|m| m.mail_id == k.mail_id);
                let mut want: Vec<(i32, Option<tgf::DItemPosition>)> = Vec::new();
                for (n, basic) in k.items.iter().enumerate() {
                    let by_guid = m.and_then(|m| {
                        m.attachments_detail
                            .iter()
                            .filter(|a| !want.iter().any(|(i, _)| *i == a.index))
                            .find(|a| {
                                a.item
                                    .as_ref()
                                    .and_then(|i| i.basic.as_ref())
                                    .map(|b| if basic.guid != 0 { b.guid == basic.guid } else { b.type_id == basic.type_id })
                                    .unwrap_or(false)
                            })
                            .map(|a| a.index)
                    });
                    let paired = (k.indexs.len() == k.items.len()).then(|| k.indexs[n]);
                    if let Some(i) = by_guid.or(paired) {
                        want.push((i, basic.position.clone()));
                    }
                }
                for i in &k.indexs {
                    if !want.iter().any(|(w, _)| w == i) {
                        want.push((*i, None));
                    }
                }
                (k.mail_id, if want.is_empty() { None } else { Some(want) })
            })
            .collect()
    }

    // =================================================================================================
    // Match records, match settings (state)
    // =================================================================================================

    pub(super) async fn battle_record_list_get_impl(&self, req: tgf::CsBattleRecordListGetReq) -> Option<tgf::ScBattleRecordListGetRsp> {
        let p = self.load_me().await;
        let mut keys: Vec<tgf::DBattleRecordKey> = p.state["progress_battle_records"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| tgf::DBattleRecordKey {
                        battle_id: r["battle_id"].as_str().unwrap_or_default().to_string(),
                        timepoint: r["timepoint"].as_i64().unwrap_or(0),
                        is_private_playback: r["private"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default();
        keys.sort_by(|a, b| b.timepoint.cmp(&a.timepoint));
        if req.count > 0 {
            keys.truncate(req.count as usize);
        }
        Some(tgf::ScBattleRecordListGetRsp { battle_record_keys: keys })
    }

    pub(super) async fn battle_record_data_get_impl(&self, req: tgf::CsBattleRecordDataGetReq) -> Option<tgf::ScBattleRecordDataGetRsp> {
        let p = self.load_me().await;
        let id = req.battle_record_key.as_ref().map(|k| k.battle_id.clone()).unwrap_or_default();
        let rec = p.state["progress_battle_records"].as_array().and_then(|a| a.iter().find(|r| r["battle_id"].as_str() == Some(id.as_str())).cloned());
        Some(tgf::ScBattleRecordDataGetRsp {
            battle_record_key: req.battle_record_key,
            battle_record_data: rec.map(|r| tgf::DBattleRecordData { timepoint: r["data_timepoint"].as_i64().unwrap_or(0) }),
        })
    }

    pub(super) async fn battle_record_data_set_impl(&self, req: tgf::CsBattleRecordDataSetReq) -> Option<tgf::ScBattleRecordDataSetRsp> {
        let Some(key) = req.battle_record_key.clone().filter(|k| !k.battle_id.is_empty()) else {
            return Some(tgf::ScBattleRecordDataSetRsp { battle_record_key: req.battle_record_key });
        };
        let mut p = self.load_me().await;
        progress_battle_record_put(&mut p, &key, req.battle_record_data.as_ref().map(|d| d.timepoint).unwrap_or(0));
        self.save_me(&p).await;
        Some(tgf::ScBattleRecordDataSetRsp { battle_record_key: Some(key) })
    }

    /// In-match control scheme — saved; substituted into deploy_preview/dirty_level_current (battle_handlers.rs, role_handlers.rs)
    /// via [`progress_battle_options`] (currently input_scheme_id: 0 there).
    pub(super) async fn battle_update_setting_impl(&self, req: tgf::CsBattleUpdateSettingReq) -> Option<tgf::ScBattleRoomPlayerActRsp> {
        if let Some(s) = req.battle_setting {
            let mut p = self.load_me().await;
            p.state["progress_battle_setting"] = json!({ "input_scheme_id": s.input_scheme_id });
            self.save_me(&p).await;
        }
        Some(tgf::ScBattleRoomPlayerActRsp {})
    }

    // =================================================================================================
    // Levels / map: not called in the current flow, the match is not touched (see the module)
    // =================================================================================================

    pub(super) async fn level_prestart_impl(&self, req: tgf::CsLevelPrestartReq) -> Option<tgf::ScLevelPrestartRsp> {
        // TODO(verify): if the client starts sending it — what it expects in the response (deploy_response?); for now an echo, as level_finish
        println!("[Level] {} level_prestart {:?}", self.clean_open_id(), req.level_data.as_ref().and_then(|l| l.level_parameter.as_ref()).map(|l| (l.level_type, l.level_id)));
        Some(tgf::ScLevelPrestartRsp { level_data: req.level_data })
    }

    pub(super) async fn level_start_impl(&self, req: tgf::CsLevelStartReq) -> Option<tgf::ScLevelStartRsp> {
        // TODO(verify): as level_prestart
        println!("[Level] {} level_start {:?}", self.clean_open_id(), req.level_data.as_ref().and_then(|l| l.level_parameter.as_ref()).map(|l| (l.level_type, l.level_id)));
        Some(tgf::ScLevelStartRsp { level_data: req.level_data })
    }

    /// Sniffer: C2S {level_data{level_parameter{}}, exit_reason 12 (FORCE_SELF_QUIT)} → S2C {level_data{level_parameter{}}}.
    /// Leaving the match is handled by battle_room_* / DS — here only an echo.
    pub(super) async fn level_finish_impl(&self, req: tgf::CsLevelFinishReq) -> Option<tgf::ScLevelFinishRsp> {
        println!("[Level] {} level_finish exit_reason={}", self.clean_open_id(), req.exit_reason);
        Some(tgf::ScLevelFinishRsp { level_data: Some(req.level_data.unwrap_or_default()) })
    }

    /// Challenger Mode ("challenge mode") is on for the player (`level_challenge`, state.progress_level_challenger)
    pub(super) fn challenger_on(p: &PlayerModel) -> bool {
        p.state["progress_level_challenger"].as_i64() == Some(tgf::EnChallengerModelType::Open as i64)
    }

    /// Challenger Mode choice for the client (DUserLevelData / DLevelData.current_select_challenge)
    pub(super) fn challenger_model(p: &PlayerModel) -> tgf::DChallengerModel {
        let t = if Self::challenger_on(p) { tgf::EnChallengerModelType::Open } else { tgf::EnChallengerModelType::NotOpen };
        tgf::DChallengerModel { challenger_type: t as i32 }
    }

    /// 08.11: the "challenge mode" toggle in the map selection window. Before, the choice was only saved — the client sent the request again and
    /// again, and the checkbox did not change (it waits for the new state). Now also a current_select_challenge push; in the search
    /// (`lobby_matching_search` → matchmaking::queue_size) solo and duo go into rooms of three.
    /// TODO(verify): which field UMatchService.GetSelectedChallengerType reads — check on the stand.
    pub(super) async fn level_challenge_impl(&self, req: tgf::CsLevelChallengeReq) -> Option<tgf::ScLevelChallengeRsp> {
        if let Some(c) = req.challenger {
            let mut p = self.load_me().await;
            p.state["progress_level_challenger"] = json!(c.challenger_type);
            self.save_me(&p).await;
            println!("[Level] {} Challenger Mode: {}", p.open_id, if Self::challenger_on(&p) { "on" } else { "off" });
            self.push_dirty(tgf::ScPlayerDirtyChgSync {
                dirty_level_current: Some(tgf::DLevelData { current_select_challenge: Some(Self::challenger_model(&p)), ..Default::default() }),
                ..Default::default()
            });
            // in a party the mode is shared — the leader's choice (DTeamStorage.challenger_model): a snapshot to all members
            if let Some(t) = crate::lobby::team::team_of(p.user_id).filter(|t| t.captain == p.user_id) {
                self.push_team_snapshots(&Self::team_user_ids(&t)).await;
                // a member's client changes the checkbox from the party action in the team chat (a snapshot is not enough — checked 08.11)
                let action = tgf::d_chat_message_team_action::Action::ChallengerModelSwitch(Self::challenger_model(&p));
                self.chat_push_team_action(&t, &p, action);
            }
        }
        Some(tgf::ScLevelChallengeRsp { result_code: 0 })
    }

    pub(super) async fn map_move_impl(&self, req: tgf::CsMapMoveReq) -> Option<tgf::ScMapMoveRsp> {
        // TODO(verify): whether a dirty_map_current push is needed — there are no calls in the dumps
        if let Some(m) = req.map_data.as_ref() {
            let mut p = self.load_me().await;
            p.state["progress_map_current"] = json!({ "x": m.x, "y": m.y });
            self.save_me(&p).await;
        }
        Some(tgf::ScMapMoveRsp { map_data: req.map_data })
    }

    // =================================================================================================
    // Quests
    // =================================================================================================

    /// Quest snapshot for player_get_info.user_quest / dirty_quest_snapshot (PVE format)
    pub(super) fn progress_quests_snapshot(&self, p: &PlayerModel) -> tgf::DUserQuestsData {
        let mut out = tgf::DUserQuestsData { last_event_id: p.state["progress_quest_event"].as_i64().unwrap_or(0), ..Default::default() };
        let Some(qs) = p.state["progress_quests"].as_object() else { return out };
        for (id, q) in qs {
            let Ok(id) = id.parse::<i32>() else { continue };
            let status = q["status"].as_i64().unwrap_or(0) as i32;
            let created = q["created"].as_i64().unwrap_or(0);
            if let Some(t) = q["completed"].as_i64() {
                out.completed_quests.insert(id, tgf::DUserQuestCompletedData { quest_id: id, timepoint: t });
            }
            if let Some(t) = q["received"].as_i64() {
                out.received_quests.insert(id, tgf::DUserQuestReceivedData { quest_id: id, timepoint: t });
            }
            // 08.11: a quest with a received reward stays in the list with status RECEIVE — the client (UTaskService.MapTaskData,
            // BattlePassTaskService) shows it as "received". Before, it dropped out of the snapshot, the client kept the old
            // COMPLETE record, and the "claim" button did not disappear although the reward was granted.
            let Some(cfg) = self.game_data.quest_list.get(&id) else { continue };
            for (i, pr) in cfg.progress.iter().enumerate() {
                let value = q["values"][i].as_i64().unwrap_or(0);
                out.processing_quests
                    .entry(pr.type_id)
                    .or_default()
                    .quest_progress_list
                    .insert(id, tgf::DUserQuestData { quest_id: id, value, status, unique_count: Default::default(), created_time: created });
            }
        }
        out
    }

    /// Add a quest to the player as "in progress" (if not there yet). true — added.
    pub(super) fn progress_quest_ensure(&self, p: &mut PlayerModel, quest_id: i32) -> bool {
        let Some(cfg) = self.game_data.quest_list.get(&quest_id) else { return false };
        if !p.state["progress_quests"].is_object() {
            p.state["progress_quests"] = json!({});
        }
        let key = quest_id.to_string();
        if p.state["progress_quests"].get(&key).is_some() {
            return false;
        }
        p.state["progress_quests"][key] = json!({
            "status": tgf::EnQuestStatus::Processing as i32,
            "values": vec![0i64; cfg.progress.len()],
            "created": Self::now_ts(),
        });
        true
    }

    /// Quest progress: add `value` to all of the player's "in progress" quests with progress of type `progress_type`
    /// (count_type SINGLE — take the maximum). For the battle/trade domains. Returns the quests that became completed.
    #[allow(dead_code)]
    pub(super) fn progress_quest_add(&self, p: &mut PlayerModel, progress_type: i32, value: i64) -> Vec<i32> {
        let ids: Vec<i32> = p.state["progress_quests"].as_object().map(|o| o.keys().filter_map(|k| k.parse().ok()).collect()).unwrap_or_default();
        ids.into_iter().filter(|id| self.quest_bump(p, *id, |t, _| t == progress_type, value)).collect()
    }

    /// Add progress to quest `id` by the progress records matching `pick(type_id, index)`. true — the quest became completed.
    fn quest_bump(&self, p: &mut PlayerModel, id: i32, pick: impl Fn(i32, usize) -> bool, value: i64) -> bool {
        let Some(cfg) = self.game_data.quest_list.get(&id) else { return false };
        let q = &mut p.state["progress_quests"][id.to_string()];
        if q["status"].as_i64() != Some(tgf::EnQuestStatus::Processing as i64) {
            return false;
        }
        let mut values: Vec<i64> = (0..cfg.progress.len()).map(|i| q["values"][i].as_i64().unwrap_or(0)).collect();
        let mut touched = false;
        for (i, pr) in cfg.progress.iter().enumerate() {
            if pick(pr.type_id, i) {
                values[i] = if pr.count_type == tgf::EnQuestProgressCountType::AddUp as i32 { values[i] + value } else { values[i].max(value) };
                touched = true;
            }
        }
        if !touched {
            return false;
        }
        q["values"] = json!(values);
        let done = cfg.progress.iter().enumerate().all(|(i, pr)| values[i] >= pr.value.max(1));
        if done {
            q["status"] = json!(tgf::EnQuestStatus::Complete as i32);
            q["completed"] = json!(Self::now_ts());
        }
        done
    }

    /// Grant the reward of a (completed) quest and mark it "received". AUTO_MAIL — the reward goes by mail (Granted is empty).
    /// `positions` — items_position of a manual receipt: on an EnErrorCode (QuestRewardReceiveManualFailed*CountNotMatching /
    /// TypeNotMatching) the client sends a record with a place for every reward item; a place matching by type_id
    /// is used, the rest we arrange ourselves. TODO(verify): the order/format from a live request.
    async fn quest_give_reward(&self, p: &mut PlayerModel, id: i32, positions: &[tgf::DItemBasic]) -> Result<Granted, GrantErr> {
        let cfg = self.game_data.quest_list.get(&id).ok_or((tgf::EnErrorCode::EnErrQuestConfigNull as i32, "no quest in quest_list".to_string()))?;
        let now = Self::now_ts();
        let reward = cfg.rewards.clone().unwrap_or_default();
        let items: Vec<tgf::DItem> = reward.items.iter().map(|o| offset_item(o, now)).collect();
        let granted = if reward.give_out_type == tgf::EnItemRewardGiveOutType::AutoMail as i32 {
            let mail = mail_from_template(&self.game_data, reward.mail_template_id, items.clone(), &[])
                .unwrap_or_else(|| mail_system(&cfg.name, &cfg.name, items));
            mail_send(&self.db, &self.game_data, &p.open_id, mail).await;
            Granted::default()
        } else {
            let mut free: Vec<&tgf::DItemBasic> = positions.iter().filter(|b| b.position.is_some()).collect();
            let placed = items
                .into_iter()
                .map(|it| {
                    let t = it.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
                    let pos = free.iter().position(|b| b.type_id == t).map(|i| free.remove(i).position.clone().unwrap());
                    (it, pos)
                })
                .collect();
            let gd = self.game_data.clone();
            grant_items(p, placed, &|t| footprint(&gd, t), &|t| is_currency(&gd, t))?
        };
        let q = &mut p.state["progress_quests"][id.to_string()];
        q["status"] = json!(tgf::EnQuestStatus::Receive as i32);
        q["received"] = json!(now);
        Ok(granted)
    }

    /// Remove a player's quest (a daily one not from the current pass set). true — it existed.
    pub(super) fn progress_quest_drop(&self, p: &mut PlayerModel, quest_id: i32) -> bool {
        p.state["progress_quests"].as_object_mut().map_or(false, |o| o.remove(&quest_id.to_string()).is_some())
    }

    /// The quest was created before `since` (the start of the current day/week) — remove it so that `progress_quest_ensure` creates it again
    pub(super) fn progress_quest_drop_stale(&self, p: &mut PlayerModel, quest_id: i32, since: i64) -> bool {
        let created = p.state["progress_quests"][quest_id.to_string()]["created"].as_i64();
        created.map_or(false, |c| c < since) && self.progress_quest_drop(p, quest_id)
    }

    /// Progress event (Q1): to all "in progress" quests with progress `ev.kind` whose
    /// condition_ids (quest_progress_condition) are all met and whose item type is not in ignore_entity_args, add `ev.value`.
    /// Conditions the backend does not know (kills, damage, scene…) are not met — such a quest does not grow.
    /// true — the progress changed (snapshot — [`Self::quest_dirty`]).
    pub(super) fn quest_event(&self, p: &mut PlayerModel, ev: &QuestEvent) -> bool {
        let ids: Vec<i32> = p.state["progress_quests"].as_object().map(|o| o.keys().filter_map(|k| k.parse().ok()).collect()).unwrap_or_default();
        let mut changed = false;
        for id in ids {
            if p.state["progress_quests"][id.to_string()]["status"].as_i64() != Some(tgf::EnQuestStatus::Processing as i64) {
                continue;
            }
            let Some(cfg) = self.game_data.quest_list.get(&id) else { continue };
            let hit: Vec<usize> = cfg
                .progress
                .iter()
                .enumerate()
                .filter(|(_, pr)| {
                    pr.type_id == ev.kind as i32
                        && ev.item_type.map_or(true, |t| !pr.ignore_entity_args.contains(&t))
                        && ev.entity.map_or(true, |e| pr.entity_args.is_empty() || pr.entity_args.contains(&e))
                        && pr.condition_ids.iter().all(|c| self.quest_condition_ok(p, *c, ev))
                })
                .map(|(i, _)| i)
                .collect();
            if hit.is_empty() {
                continue;
            }
            if self.quest_bump(p, id, |_, i| hit.contains(&i), ev.value) {
                println!("[Quest] {} quest {} completed ({:?})", p.open_id, id, ev.kind);
            }
            changed = true;
        }
        if changed {
            let n = p.state["progress_quest_event"].as_i64().unwrap_or(0) + 1;
            p.state["progress_quest_event"] = json!(n);
        }
        changed
    }

    fn quest_condition_ok(&self, p: &PlayerModel, id: i64, ev: &QuestEvent) -> bool {
        use tgf::quest_progress_condition_data::Limit as L;
        let Some(limit) = self.game_data.quest_progress_condition.get(&(id as i32)).and_then(|c| c.value.as_ref()).and_then(|v| v.limit.as_ref()) else {
            return false;
        };
        let role = ev.role_guid.and_then(|g| p.roles.iter().find(|r| r.role_guid == g));
        let deploy = role.and_then(|r| r.data.deploy_role.as_ref());
        let meta = deploy.and_then(|d| d.basic.as_ref()).or(role.and_then(|r| r.data.basic.as_ref())).and_then(|b| b.meta.as_ref());
        match limit {
            L::BattleWhichLevelType(v) => ev.level_type == Some(*v),
            L::BattleWhichLevelId(v) => ev.level_id == Some(*v),
            L::RoleWhichCareer(v) => meta.map_or(false, |m| m.career == *v),
            L::RoleWhichRace(v) => meta.map_or(false, |m| m.race == *v),
            L::RoleWhichGender(v) => meta.map_or(false, |m| m.gender == *v),
            L::RoleWhichForce(v) => meta.map_or(false, |m| m.force == *v),
            L::RoleHistoryMaxLevelLess(v) => deploy.map_or(false, |d| (d.history_role_max_level.max(d.role_level) as i64) < *v),
            L::ItemChgSubType(s) => ev.item_chg.map_or(false, |c| s.values.contains(&c)),
            L::ItemRaity(s) => ev.item_type.map_or(false, |t| s.values.contains(&self.item_rarity(t))),
            _ => false,
        }
    }

    /// Quest snapshot push (after [`Self::quest_event`] / pass synchronization)
    pub(super) fn quest_dirty(&self, p: &PlayerModel) -> tgf::ScPlayerDirtyChgSync {
        tgf::ScPlayerDirtyChgSync { dirty_quest_snapshot: Some(self.progress_quests_snapshot(p)), ..Default::default() }
    }

    /// Push after quests change: the full quest snapshot + granted items/coins + "reward received" events
    /// (`received` — quests whose reward was just granted). 08.11: with only a snapshot the client did not remove the "claim" button
    /// (neither when the quest dropped out of the snapshot nor with status RECEIVE) — DUserQuestEvent::Received is needed.
    fn quest_push(&self, p: &mut PlayerModel, granted: &Granted, received: &[i32]) {
        let mut ev = p.state["progress_quest_event"].as_i64().unwrap_or(0);
        let now = Self::now_ts();
        let mut events = Vec::new();
        for &id in received {
            ev += 1;
            let timepoint = p.state["progress_quests"][id.to_string()]["received"].as_i64().unwrap_or(now);
            events.push(tgf::DUserQuestEvent {
                event_id: ev,
                event: Some(tgf::d_user_quest_event::Event::Received(tgf::DUserQuestReceivedData { quest_id: id, timepoint })),
            });
        }
        if events.is_empty() {
            ev += 1;
        }
        p.state["progress_quest_event"] = json!(ev);
        let mut dirty = granted.dirty(p);
        dirty.dirty_quest_events = events;
        dirty.dirty_quest_snapshot = Some(self.progress_quests_snapshot(p));
        self.push_dirty(dirty);
    }

    pub(super) async fn quest_rewards_receive_impl(&self, req: tgf::CsQuestRewardsReceiveReq) -> Option<tgf::ScQuestRewardsReceiveRsp> {
        let mut p = self.load_me().await;
        let status = p.state["progress_quests"][req.quest_id.to_string()]["status"].as_i64();
        if status != Some(tgf::EnQuestStatus::Complete as i64) {
            println!("[Quest] {} rewards_receive {}: quest not completed (status {:?})", p.open_id, req.quest_id, status);
            self.set_rpc_error(if status == Some(tgf::EnQuestStatus::Receive as i64) {
                tgf::EnErrorCode::EnErrQuestReceiveRewardFailedHasAlreadyReceived as i32
            } else {
                tgf::EnErrorCode::EnErrQuestReceiveRewardFailedNotComplete as i32
            });
            return Some(tgf::ScQuestRewardsReceiveRsp {});
        }
        match self.quest_give_reward(&mut p, req.quest_id, &req.items_position).await {
            Ok(g) => {
                println!("[Quest] {} quest {} reward: {} items, coins {}", p.open_id, req.quest_id, g.items.len(), g.coins.len());
                self.quest_push(&mut p, &g, &[req.quest_id]);
                self.save_me(&p).await;
                Self::write_client_loadout_file(&p);
            }
            Err((code, e)) => {
                println!("[Quest] {} quest {} reward not granted — {}", p.open_id, req.quest_id, e);
                self.set_rpc_error(code);
            }
        }
        Some(tgf::ScQuestRewardsReceiveRsp {})
    }

    /// The client reports the progress of NEWBIE_SPECIAL_PROGRESS_FROM_CLIENT quests itself (tutorial). We create the quest if
    /// it does not exist yet; a completed one with an AUTO_INVENTORY/AUTO_MAIL reward is granted right away.
    pub(super) async fn quest_special_progress_report_impl(&self, req: tgf::CsQuestSpecialProgressReportReq) -> Option<tgf::ScQuestSpecialProgressReportRsp> {
        let special = tgf::EnQuestProgressType::NewbieSpecialProgressFromClient as i32;
        let mut p = self.load_me().await;
        let mut changed = false;
        let mut granted = Granted::default();
        let mut received: Vec<i32> = Vec::new();
        for r in &req.quests_progress {
            let Some(cfg) = self.game_data.quest_list.get(&r.quest_id) else { continue };
            if !cfg.progress.iter().any(|pr| pr.type_id == special) {
                println!("[Quest] {} special_progress {}: the quest has no client progress", p.open_id, r.quest_id);
                continue;
            }
            changed |= self.progress_quest_ensure(&mut p, r.quest_id);
            let before = p.state["progress_quests"][r.quest_id.to_string()].clone();
            let done = self.quest_bump(&mut p, r.quest_id, |t, _| t == special, r.value);
            changed |= before != p.state["progress_quests"][r.quest_id.to_string()];
            let give_out = cfg.rewards.as_ref().map(|r| r.give_out_type).unwrap_or(0);
            if done && give_out != tgf::EnItemRewardGiveOutType::Manual as i32 {
                match self.quest_give_reward(&mut p, r.quest_id, &[]).await {
                    Ok(g) => {
                        granted.extend(g);
                        received.push(r.quest_id);
                    }
                    Err((_, e)) => println!("[Quest] {} quest {} auto-reward not granted — {}", p.open_id, r.quest_id, e),
                }
            }
        }
        if changed {
            self.quest_push(&mut p, &granted, &received);
            self.save_me(&p).await;
        }
        Some(tgf::ScQuestSpecialProgressReportRsp {})
    }

    /// Submit items into an ITEM_SUBMIT progress quest: the items (by guid) are removed from the stash/backpacks, progress += their
    /// count. TODO(verify): the condition_ids conditions (which type_ids qualify) are not checked.
    pub(super) async fn quest_item_submit_impl(&self, req: tgf::CsQuestItemSubmitReq) -> Option<tgf::ScQuestItemSubmitRsp> {
        let submit = tgf::EnQuestProgressType::ItemSubmit as i32;
        let mut p = self.load_me().await;
        let ok = self.game_data.quest_list.get(&req.quest_id).map(|c| c.progress.iter().any(|pr| pr.type_id == submit)).unwrap_or(false);
        if !ok {
            println!("[Quest] {} item_submit {}: no item submission quest", p.open_id, req.quest_id);
            self.set_rpc_error(tgf::EnErrorCode::EnErrQuestConfigNull as i32);
            return Some(tgf::ScQuestItemSubmitRsp {});
        }
        if req.items_position.is_empty() {
            self.set_rpc_error(tgf::EnErrorCode::EnErrQuestSubmitFailedSubmitListEmpty as i32);
            return Some(tgf::ScQuestItemSubmitRsp {});
        }
        self.progress_quest_ensure(&mut p, req.quest_id);
        let mut removed = Vec::new();
        let mut changed_items = Vec::new();
        let mut total = 0i64;
        for b in &req.items_position {
            if let Some((gone, rest)) = take_item_count(&mut p, b.guid, b.count.max(1)) {
                total += gone as i64;
                match rest {
                    Some(it) => changed_items.push(it),
                    None => removed.push(b.guid),
                }
            }
        }
        if total > 0 {
            self.quest_bump(&mut p, req.quest_id, |t, _| t == submit, total);
            let ev = p.state["progress_quest_event"].as_i64().unwrap_or(0) + 1;
            p.state["progress_quest_event"] = json!(ev);
            self.save_me(&p).await;
            Self::write_client_loadout_file(&p);
            println!("[Quest] {} item_submit {}: submitted {} pcs", p.open_id, req.quest_id, total);
            // TODO(verify): removed items — dirty_remove_guid (how PVE removes them was not seen)
            self.push_dirty(tgf::ScPlayerDirtyChgSync {
                dirty_remove_guid: removed,
                dirty_postion_action: changed_items,
                dirty_quest_snapshot: Some(self.progress_quests_snapshot(&p)),
                ..Default::default()
            });
        }
        Some(tgf::ScQuestItemSubmitRsp {})
    }

    // =================================================================================================
    // Lottery
    // =================================================================================================

    /// lottery_pool: one test pool (id 1, key 100002, random_pool_id 620001 — no such pool in any table),
    /// the response is a placeholder oneof Tmp(i32), the drop cannot be put there; no lottery UI was found in the client.
    /// So nothing is charged or granted. TODO(verify): if the client starts sending it — see what it expects.
    pub(super) async fn lottery_draw_impl(&self, req: tgf::CsLotteryDrawReq) -> Option<tgf::ScLotteryDrawRsp> {
        let pool = self.game_data.lottery_pool.get(&req.lottery_pool_id);
        println!(
            "[Lottery] {} draw pool={} x{}: {}",
            self.clean_open_id(),
            req.lottery_pool_id,
            req.draw_times,
            if pool.is_some() { "pool exists, no random reward pool — empty" } else { "no such pool" }
        );
        if pool.is_none() {
            self.set_rpc_error(tgf::EnErrorCode::EnErrLotteryDrawFailedDrawPoolIdNotFound as i32);
        }
        Some(tgf::ScLotteryDrawRsp { lottery_list: Some(tgf::DLotteryList { lottery: Some(tgf::d_lottery_list::Lottery::Tmp(0)) }) })
    }
}

/// Save a match record into state["progress_battle_records"] (a new one — at the end, at most 50). For the battle domain.
pub(super) fn progress_battle_record_put(p: &mut PlayerModel, key: &tgf::DBattleRecordKey, data_timepoint: i64) {
    if !p.state["progress_battle_records"].is_array() {
        p.state["progress_battle_records"] = json!([]);
    }
    let list = p.state["progress_battle_records"].as_array_mut().unwrap();
    list.retain(|r| r["battle_id"].as_str() != Some(key.battle_id.as_str()));
    list.push(json!({
        "battle_id": key.battle_id,
        "timepoint": if key.timepoint > 0 { key.timepoint } else { GatewaySession::now_ts() },
        "private": key.is_private_playback,
        "data_timepoint": data_timepoint,
    }));
    let over = list.len().saturating_sub(BATTLE_RECORDS_MAX);
    list.drain(..over);
}

/// Control scheme saved by battle_update_setting
#[allow(dead_code)]
pub(super) fn progress_battle_options(p: &PlayerModel) -> tgf::DBattleOptions {
    tgf::DBattleOptions { input_scheme_id: p.state["progress_battle_setting"]["input_scheme_id"].as_i64().unwrap_or(0) as i32 }
}

/// Take `count` pieces of item guid from the stash or the backpacks/worn. Returns (how many were taken,
/// the stack remainder — None if the item went whole).
fn take_item_count(p: &mut PlayerModel, guid: i64, count: i32) -> Option<(i32, Option<tgf::DItem>)> {
    let mut lists: Vec<&mut Vec<tgf::DItem>> = vec![&mut p.stash.element];
    for r in p.roles.iter_mut() {
        if let Some(d) = r.data.deploy_role.as_mut() {
            lists.extend(d.backpack.as_mut().map(|b| &mut b.items));
            lists.extend(d.small_backpack.as_mut().map(|b| &mut b.items));
            lists.extend(d.wearing.as_mut().map(|b| &mut b.items));
        }
    }
    for list in lists {
        let Some(i) = list.iter().position(|it| it.basic.as_ref().map(|b| b.guid) == Some(guid)) else { continue };
        let have = list[i].basic.as_ref().map(|b| b.count.max(1)).unwrap_or(1);
        if have > count {
            if let Some(b) = list[i].basic.as_mut() {
                b.count = have - count;
            }
            return Some((count, Some(list[i].clone())));
        }
        list.remove(i);
        return Some((have, None));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(page: i64, x: i32, y: i32) -> tgf::DItemPosition {
        tgf::DItemPosition { r#type: STASH, guid: page, x, y, ..Default::default() }
    }
    fn item(type_id: i32, count: i32) -> tgf::DItem {
        tgf::DItem { basic: Some(tgf::DItemBasic { type_id, count, ..Default::default() }), ..Default::default() }
    }
    /// 1 — a 3×1 sword, 2 — a 1×1 potion with stack 3, 1001 — gold
    fn fp(t: i32) -> Footprint {
        match t {
            1 => (3, 1, 1),
            2 => (1, 1, 3),
            _ => (1, 1, i32::MAX),
        }
    }
    fn cur(t: i32) -> bool {
        t == GOLD
    }
    fn auto(v: Vec<tgf::DItem>) -> Vec<(tgf::DItem, Option<tgf::DItemPosition>)> {
        v.into_iter().map(|i| (i, None)).collect()
    }

    #[test]
    fn stash_cells_respect_footprint() {
        let mut p = PlayerModel::new("t", 10000001);
        let g = grant_items(&mut p, auto(vec![item(1, 1), item(1, 1)]), &fp, &cur).unwrap();
        assert_eq!(g.items[0].basic.as_ref().unwrap().position, Some(at(0, 0, 0)));
        assert_eq!(g.items[1].basic.as_ref().unwrap().position, Some(at(0, 0, 1))); // 0..3 along x is taken — the next one along y
        let g = grant_items(&mut p, auto(vec![item(2, 7), item(GOLD, 100)]), &fp, &cur).unwrap();
        assert_eq!(g.items.len(), 3); // 3 + 3 + 1
        assert_eq!(g.coins, vec![GOLD]);
        assert_eq!(common::coin(&p, GOLD), common::START_GOLD + 100);
        let guids: Vec<i64> = p.stash.element.iter().map(|i| i.basic.as_ref().unwrap().guid).collect();
        assert!(guids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn grant_without_space_changes_nothing() {
        let mut p = PlayerModel::new("t", 10000001);
        let big = |_t: i32| -> Footprint { (STASH_X, STASH_Y, 1) };
        let pages = super::super::inventory_handlers::stash_page_count(&p) as usize;
        assert!(grant_items(&mut p, auto(vec![item(5, pages as i32)]), &big, &cur).is_ok());
        let before = p.stash.element.len();
        assert!(grant_items(&mut p, auto(vec![item(GOLD, 5), item(5, 1)]), &big, &cur).is_err());
        assert_eq!(p.stash.element.len(), before);
        assert_eq!(common::coin(&p, GOLD), common::START_GOLD);
    }

    #[test]
    fn client_position_and_guid_kept() {
        let mut p = PlayerModel::new("t", 10000001);
        let mut sword = item(1, 1);
        sword.basic.as_mut().unwrap().guid = 123_456_789;
        let g = grant_items(&mut p, vec![(sword.clone(), Some(at(2, 5, 4)))], &fp, &cur).unwrap();
        assert_eq!(g.items[0].basic.as_ref().unwrap().position, Some(at(2, 5, 4)));
        assert_eq!(g.items[0].basic.as_ref().unwrap().guid, 123_456_789);
        // the same place is taken, the same guid already exists — an automatic place and a new guid
        let g = grant_items(&mut p, vec![(sword, Some(at(2, 5, 4)))], &fp, &cur).unwrap();
        assert_eq!(g.items[0].basic.as_ref().unwrap().position, Some(at(0, 0, 0)));
        assert_ne!(g.items[0].basic.as_ref().unwrap().guid, 123_456_789);
        // a guid larger than i32::MAX (the old scheme) cannot be held by the client — always a new one
        let mut old = item(1, 1);
        old.basic.as_mut().unwrap().guid = 900_000_000_701;
        let g = grant_items(&mut p, vec![(old, None)], &fp, &cur).unwrap();
        assert!(!crate::guid::too_big(g.items[0].basic.as_ref().unwrap().guid));
    }

    /// "Take into the backpack": the client sends a place without x/y — do not cover an occupied cell (03.11 13:26 broke the inventory)
    #[test]
    fn bag_cell_must_be_free() {
        let role_guid = 1_000_000_101u64;
        let bag = |x: i32, y: i32| tgf::DItemPosition { r#type: 2, guid: role_guid as i64, x, y, ..Default::default() };
        let mut p = PlayerModel::new("t", 10000001);
        let mut potion = item(2, 1);
        potion.basic.as_mut().unwrap().position = Some(bag(0, 0));
        potion.basic.as_mut().unwrap().guid = 555;
        let backpack = tgf::DRoleBackpackData { items: vec![potion], row_size: 7, column_size: 10 };
        p.roles.push(crate::structures::RoleModel {
            role_guid,
            ord: 1,
            type_id: 400002,
            role_name: "r".into(),
            role_level: 1,
            data: tgf::DUserRole { deploy_role: Some(tgf::DBattleDeployRole { backpack: Some(backpack), ..Default::default() }), ..Default::default() },
        });
        let g = grant_items(&mut p, vec![(item(1, 1), Some(bag(0, 0)))], &fp, &cur).unwrap();
        let pos = g.items[0].basic.as_ref().unwrap().position.clone().unwrap();
        assert_eq!(pos.r#type, 2, "into the backpack, since there is room");
        assert_ne!((pos.x, pos.y), (0, 0), "not over the potion at (0,0)");
        assert!(pos.limit_option.is_none());
    }

    #[test]
    fn mail_pending_and_record() {
        let mut m = mail_system("t", "c", vec![item(1, 1)]);
        m.attachments_offset.push(tgf::DMailItemOffset { index: 1, item: Some(tgf::DItemOffset { type_id: GOLD, count: 5, expire_offset: 0 }) });
        m.mail_id = 7;
        assert_eq!(mail_pending(&m), vec![1, 0]);
        assert_eq!(mail_item_at(&m, 1, 0).and_then(|i| i.basic).map(|b| (b.type_id, b.count)), Some((GOLD, 5)));
        m.received_items.push(0);
        assert_eq!(mail_pending(&m), vec![1]);
        assert_eq!(mail_record(&m).mail_id, 7);
        assert!(mail_visible(&m, 0));
        m.status |= MAIL_ST_REMOVED;
        assert!(!mail_visible(&m, 0));
    }

    #[test]
    fn battle_records_capped() {
        let mut p = PlayerModel::new("t", 1);
        for i in 0..60 {
            progress_battle_record_put(&mut p, &tgf::DBattleRecordKey { battle_id: format!("b{}", i), timepoint: i + 1, is_private_playback: false }, 0);
        }
        progress_battle_record_put(&mut p, &tgf::DBattleRecordKey { battle_id: "b59".into(), timepoint: 100, is_private_playback: false }, 5);
        let l = p.state["progress_battle_records"].as_array().unwrap();
        assert_eq!(l.len(), BATTLE_RECORDS_MAX);
        assert_eq!(l.last().unwrap()["data_timepoint"], 5);
    }
}

// =====================================================================================================
// Character experience and level (map_exp + role_level_exp)
// =====================================================================================================

impl GatewaySession {
    /// Raid experience: map_exp — steps by time in the raid (alive_time, s) for the map; no rows for the map — map 1.
    /// TODO(verify): team_bonus (0 in all rows) and whether experience is granted on death the same as on extraction.
    pub(super) fn progress_raid_exp(&self, map_id: i32, alive_secs: i64) -> i32 {
        let rows = &self.game_data.map_exp_rows;
        let pick = |m: i32| rows.iter().filter(|r| r.map_id == m && r.alive_time <= alive_secs).max_by_key(|r| r.alive_time).map(|r| r.exp);
        let has_map = rows.iter().any(|r| r.map_id == map_id);
        pick(if has_map { map_id } else { 1 }).unwrap_or(0).max(0)
    }

    /// Add experience to the role and raise the level by role_level_exp (max_exp of level L — experience to L+1). The cap — the highest
    /// role_level_upgrade level of this class. On a level change the base attributes are recomputed.
    /// Returns the (old, new) level; None — no role or add <= 0.
    pub(super) fn progress_add_role_exp(&self, p: &mut PlayerModel, role_guid: u64, add: i32) -> Option<(i32, i32)> {
        if add <= 0 {
            return None;
        }
        let gd = &self.game_data;
        let role = p.get_role_mut(role_guid)?;
        let type_id = role.type_id as i32;
        let cap = gd
            .role_level_upgrade_rows
            .iter()
            .filter(|r| r.role_type_id == type_id)
            .map(|r| r.role_level)
            .max()
            .or_else(|| gd.role_level_exp.iter().map(|r| r.level).max())
            .unwrap_or(1);
        let deploy = role.data.deploy_role.as_mut()?;
        let old = deploy.role_level.max(1);
        let mut level = old;
        deploy.exp += add;
        while level < cap {
            let need = gd.role_level_exp.iter().find(|r| r.level == level).map(|r| r.max_exp).unwrap_or(0);
            if need <= 0 || deploy.exp < need {
                break;
            }
            deploy.exp -= need;
            level += 1;
        }
        deploy.role_level = level;
        deploy.history_role_max_level = deploy.history_role_max_level.max(level);
        if level != old {
            deploy.role_affix.get_or_insert_with(Default::default).attribute = self.build_role_base_attributes(type_id, level);
        }
        role.role_level = level as u32;
        Some((old, level))
    }
}

// ---- one-time migration of item guids (crate::guid::migrate) ----
/// Players' mailboxes (key — open_id): attachment guids
pub(super) async fn mail_migrate_guids(db: &GameDb, open_ids: &[String], map: &mut crate::guid::GuidMap) -> usize {
    let mut total = 0;
    for open_id in open_ids {
        let Some(mut b) = db.get::<MailBox>(MAIL_TABLE, open_id).await else { continue };
        let n = crate::guid::remap_message(&mut b.list, map);
        if n > 0 {
            mail_store(db, open_id, &b).await;
            total += n;
        }
    }
    total
}

#[cfg(test)]
mod quest_tests {
    use super::*;
    use tgf::EnQuestProgressType as K;

    fn status(p: &PlayerModel, id: i32) -> (Option<i64>, i64) {
        let q = &p.state["progress_quests"][id.to_string()];
        (q["status"].as_i64(), q["values"][0].as_i64().unwrap_or(-1))
    }

    /// Q1: pass 16518 quests in the quest domain (as in PVE: 3 daily + 10 weekly) and backend events
    #[tokio::test]
    async fn passport_quests_sync_and_events() {
        let dir = format!("target/test_quest_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(GameData::load_all("ExcelConfig")),
            open_id: "quest-test".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let mut p = PlayerModel::new("quest-test", 10949021);
        assert!(s.act_sync_quests(&mut p));
        let qs = p.state["progress_quests"].as_object().unwrap().clone();
        let weekly: Vec<i32> = (1651818..=1651827).collect();
        assert!(weekly.iter().all(|id| qs.contains_key(&id.to_string())), "all weekly ones");
        let daily = qs.keys().filter_map(|k| k.parse::<i32>().ok()).filter(|id| (1651801..=1651817).contains(id)).count();
        assert_eq!(daily, 3, "3 daily ones from the pool");
        assert!(!s.act_sync_quests(&mut p), "a repeat changes nothing");
        let snap = s.progress_quests_snapshot(&p);
        assert!(snap.processing_quests.contains_key(&(K::TradeBuyItem as i32)));

        // marketplace: 5 pieces — the first quest is completed, the second 5/10; coins are not counted
        let mut ev = QuestEvent::new(K::TradeBuyItem, 5, 0);
        ev.item_type = Some(100060);
        assert!(s.quest_event(&mut p, &ev));
        assert_eq!(status(&p, 1651821), (Some(tgf::EnQuestStatus::Complete as i64), 5));
        assert_eq!(status(&p, 1651822).1, 5);
        ev.item_type = Some(1001);
        assert!(!s.quest_event(&mut p, &ev), "gold — ignore_entity_args");

        // dismantling an uncommon item — ITEM_CONSUME with the "dismantle" and rarity conditions
        let uncommon = s.game_data.item_type.keys().copied().find(|t| s.item_rarity(*t) == tgf::EnItemRarity::Uncommon as i32).unwrap();
        let mut ev = QuestEvent::new(K::ItemConsume, 1, 0);
        ev.item_type = Some(uncommon);
        ev.item_chg = Some(tgf::EnItemChgSubType::EnIcstGearDismantle as i32);
        assert!(s.quest_event(&mut p, &ev));
        assert_eq!(status(&p, 1651824).1, 1);
        ev.item_chg = None;
        assert!(!s.quest_event(&mut p, &ev), "without dismantling — the condition is not met");

        // victory: 1651827 — PVP only (level_type 2002)
        let mut ev = QuestEvent::new(K::BattleVictory, 1, 0);
        ev.level_type = Some(2001);
        s.quest_event(&mut p, &ev);
        assert_eq!(status(&p, 1651827).1, 0);
        ev.level_type = Some(2002);
        assert!(s.quest_event(&mut p, &ev));
        assert_eq!(status(&p, 1651827).0, Some(tgf::EnQuestStatus::Complete as i64));

        // 08.11: monsters / chests / damage to players from the battle server — only for their own entity_args (11 / 41 / 1)
        let ds = |kind, value, entity| {
            let mut ev = QuestEvent::new(kind, value, 0);
            ev.level_type = Some(2001);
            ev.entity = Some(entity);
            ev
        };
        assert!(s.quest_event(&mut p, &ds(K::KillType, 7, 11)));
        assert_eq!(status(&p, 1651818).1, 7, "kill 100 monsters");
        assert!(!s.quest_event(&mut p, &ds(K::KillType, 7, 41)), "wrong target");
        assert!(s.quest_event(&mut p, &ds(K::Loot, 3, 41)));
        assert_eq!(status(&p, 1651819).1, 3, "open 40 chests");
        assert!(s.quest_event(&mut p, &ds(K::PlayerCausedTotalDamage, 500, 1)));
        assert_eq!(status(&p, 1651820).1, 500, "3000 damage to players");
        let mut pvp = ds(K::KillType, 7, 11);
        pvp.level_type = Some(2002);
        assert!(!s.quest_event(&mut p, &pvp), "condition 102001 — adventure only");

        // reward received — the quest stays in the snapshot with status RECEIVE (otherwise the client keeps "claim")
        s.quest_give_reward(&mut p, 1651821, &[]).await.unwrap();
        let snap = s.progress_quests_snapshot(&p);
        let q = &snap.processing_quests[&(K::TradeBuyItem as i32)].quest_progress_list[&1651821];
        assert_eq!(q.status, tgf::EnQuestStatus::Receive as i32);
        assert!(snap.received_quests.contains_key(&1651821));

        // new week: the weekly one is created again
        p.state["progress_quests"]["1651821"]["created"] = json!(0);
        assert!(s.act_sync_quests(&mut p));
        assert_eq!(status(&p, 1651821), (Some(tgf::EnQuestStatus::Processing as i64), 0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
