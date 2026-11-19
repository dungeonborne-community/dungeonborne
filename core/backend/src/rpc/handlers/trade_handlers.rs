//! Trading post (market 1, homogeneous goods: sell/buy orders) and auction (market 2: lot + bids).
//! Handlers in handlers.rs only delegate here (`handle_<rpc>` → `<rpc>_impl`).
//!
//! Format facts (PVE reference / official server sniffer, 30.10): trade_order_history{market_id 2, then 1} and
//! trade_auction_fetch_order_info{} on login, trade_product_price_basic_info{keys, order_type 11} — all responses empty
//! (empty market). The rest — from the proto (schema_dump.proto) and client UE dumps (TradeHomogenService, TradeSearchService):
//! - `order_type` in price requests = oneof field number in DTradeOrderTicketMeta: 11 sale, 12 purchase, 13 auction.
//! - Product = a trade_commodity row (id = trade_product_id, item_id = item type); ids repeat across markets, so
//!   we read the table ourselves keyed by (market_id, id) — GameData.trade_commodity keeps only one row per id.
//! - Errors: trade_* responses have no ret_code (except purchase.result_code) → on rejection an empty response (ticket_id = 0),
//!   the EN_ERR_TRADE_* code — into RpcHead.error_code (set_rpc_error) and the log.
//!
//! Model: one shared order book `trade_book:main` (prost bytes of TradeBook). Each order holds an escrow (items taken from
//! the seller / remaining gold of a purchase) and the owner's DUserTradeConfirmData: everything the player is to receive
//! (goods, proceeds, refund) lies in pending_receive until they claim it via trade_receive_confirmation_product.
//! So a deal changes only the book (other players' PlayerModel is not touched), and gold/items move only for the player of
//! one's own session: first the player is saved, then the book; if the book is not written, the player rolls back to a copy (no losses and
//! no double charges). All book changes — under a global mutex. Expirations and auction results —
//! lazily, on any modifying request (tick). Owners of changed orders get
//! SCPlayerDirtyChgSync.dirty_trade_market (dirty_orders + dirty_confimation[_full], delete_*).

use super::common::{self, GOLD};
use super::GatewaySession;
use super::progress_handlers::QuestEvent;
use crate::google::protobuf::Timestamp;
use crate::lobby::hub;
use crate::structures::PlayerModel;
use crate::tgf;
use prost::Message;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;
use tgf::d_trade_order_ticket_meta::Type as Kind;
use tgf::d_trade_pending_confirmation::Data as Pend;

// Order kind: oneof field numbers of DTradeOrderTicketMeta.type; BID — our internal one (AuctionOrder with a bid)
const SALES: i32 = 11;
const PURCHASE: i32 = 12;
const AUCTION: i32 = 13;
const BID: i32 = 14;

// EnTradeOrderTicketState
const ST_TRADING: i32 = 12;
const ST_CONFIRMATION: i32 = 21;
const ST_EXPIRATION: i32 = 22;
const ST_CANCEL: i32 = 23;

// EnTradeConfirmationType
const CT_MATCH: i32 = 1;
const CT_FAILED_RETURN: i32 = 4; // EN_TRADE_CONFIRMATION_TYPE_FAILED_RETURN — refund of a purchase deposit

// tgf.config.EnTradeMarketType
const MARKET_AUCTION: i32 = 1;
const MARKET_HOMOGENEOUS: i32 = 2;

// EnTradeOrderTicketLabelTagType / AttributeType
const TAG_GEAR_SUB_TYPE: i32 = 1001;
const TAG_ROLE_CAREER: i32 = 1002;
const TAG_AFFIX_ATTRIBUTE: i32 = 1003;
const TAG_WEARING_PART: i32 = 1004;
const TAG_AFFIX_ATTRIBUTE_FIXED: i32 = 1008;
const TAG_CORRUPTION: i32 = 1006;
const TAG_AFFIX_GEAR_SET: i32 = 8001;
const ATTR_RARITY: i32 = 1;

// EnErrorCode (schema_dump.proto)
const E_COIN_NOT_ENOUGH: i32 = -681;
const E_PRODUCT_NOT_FOUND: i32 = -4204;
const E_NOT_ON_SHELF: i32 = -4205;
const E_MARKET_NOT_FOUND: i32 = -4206;
const E_USER_SALE_LIMIT: i32 = -4209;
const E_PRICE_INVALID: i32 = -4210;
const E_SALE_ITEM_EMPTY: i32 = -4211;
const E_SALE_ITEM_INVALID: i32 = -4212;
const E_UNABLE_TO_CANCEL: i32 = -4214;
const E_CONFIRMATION_NOT_FOUND: i32 = -4217;
const E_TICKET_UNAVAILABLE: i32 = -4221;
const E_PURCHASE_COUNT_LIMIT: i32 = -4224;
const E_MARKET_TYPE_NOT_MATCH: i32 = -4232;
const E_AUCTION_BID_SELF: i32 = -4236;
const E_TICKET_FINISHED: i32 = -4237;
const E_ORDER_NOT_FINISH: i32 = -4239;
const E_AUCTION_ALREADY_BID: i32 = -4240;
const E_CURRENT_PRICE_BID: i32 = -4242;
/// EN_ERR_TRADE_AUCTION_PRICE_LIMIT — auction lot price out of bounds (12.11)
const E_AUCTION_PRICE_LIMIT: i32 = -4261;
const E_USER_PURCHASE_LIMIT: i32 = -4243;
const E_LOSS_DURABILITY: i32 = -4250;
const E_QUICK_NOT_MATCHED: i32 = -4254;
const E_DB_SAVE_FAILED: i32 = -4256;
const E_CANCEL_WITH_BIDDER: i32 = -4259;
const E_NOT_ALLOW_SELL_WEARING: i32 = -4265;

// Item places (as inventory_handlers): 1 stash (guid = page), 2 backpack, 3 worn, 4 small backpack
const POS_STASH: i32 = 1;
const POS_BACKPACK: i32 = 2;
const POS_WEARING: i32 = 3;
const POS_SMALL_BACKPACK: i32 = 4;
/// Stash page: x ∈ [0,17), y ∈ [0,10); an item occupies row_size in x and column_size in y
/// (UeSource_ItemType.position_cfg; checked on PVE player_get_info stashes — only this gives no overlaps).
const STASH_W: i32 = 17;
const STASH_H: i32 = 10;

/// Auction/search result page size if the client did not set count. TODO(verify): the real page size.
const PAGE: usize = 20;
/// How many last deals to keep (price history)
const MAX_DEALS: usize = 5000;
const BOOK_TABLE: &str = "trade_book";
const BOOK_ID: &str = "main";
const WEEK: i64 = 7 * 86400;
/// System seller (`trade_seed_system_lots`): no player has this user_id
const SYSTEM_USER: u64 = 1;
const SYSTEM_OPEN_ID: &str = "SYSTEM";
/// Lots of each item on the auction: one is bought — the next pass tops it up.
/// `TRADE_SYSTEM_AUCTION_LOTS` (playtest 07.11 — 25), default 2
fn system_lots() -> usize {
    std::env::var("TRADE_SYSTEM_AUCTION_LOTS").ok().and_then(|v| v.trim().parse().ok()).filter(|n: &usize| *n > 0).unwrap_or(2)
}
/// Stock of unidentified items of one product on Buyout Trades; sold out — the next pass lists them again.
/// `TRADE_SYSTEM_BUYOUT_STOCK` (playtest 07.11 — 25), default 3
fn system_stock() -> i32 {
    std::env::var("TRADE_SYSTEM_BUYOUT_STOCK").ok().and_then(|v| v.trim().parse().ok()).filter(|n: &i32| *n > 0).unwrap_or(3)
}

pub fn system_buyout() -> bool {
    std::env::var("TRADE_SYSTEM_BUYOUT").is_ok_and(|v| v == "1")
}

pub fn system_auction() -> bool {
    std::env::var("TRADE_SYSTEM_AUCTION").is_ok_and(|v| v == "1")
}

// ==========================================
// Stored model (prost → bytes in SurrealDB: SurrealDB cannot digest HashMap in protobuf structs)
// ==========================================

#[derive(Clone, PartialEq, Message)]
struct TradeOrder {
    #[prost(message, optional, tag = "1")]
    meta: Option<tgf::DTradeOrderTicketMeta>,
    #[prost(string, tag = "2")]
    owner_open_id: String,
    #[prost(uint64, tag = "3")]
    owner_user_id: u64,
    /// Escrow: seller's items not yet gone to buyers
    #[prost(message, repeated, tag = "4")]
    escrow: Vec<tgf::DItem>,
    /// What the owner is to receive (pending_receive) and what has already been claimed
    #[prost(message, optional, tag = "5")]
    confirm: Option<tgf::DUserTradeConfirmData>,
    /// Label for search (type, subtype, rarity, affixes)
    #[prost(message, optional, tag = "6")]
    label: Option<tgf::DTradeOrderTicketLabel>,
    /// For a bid — the auction id
    #[prost(int64, tag = "7")]
    auction_id: i64,
    /// Tax on proceeds, % (trade_commodity.tax_percentage at listing time)
    #[prost(int32, tag = "8")]
    tax: i32,
    /// Lot snapshot (auction and bids) — for the card when the escrow is already empty
    #[prost(message, optional, tag = "9")]
    item: Option<tgf::DItem>,
}

#[derive(Clone, PartialEq, Message)]
struct TradeDeal {
    #[prost(int32, tag = "1")]
    market: i32,
    #[prost(int32, tag = "2")]
    product: i32,
    #[prost(int64, tag = "3")]
    unit_price: i64,
    #[prost(int32, tag = "4")]
    count: i32,
    #[prost(int64, tag = "5")]
    ts: i64,
}

#[derive(Clone, PartialEq, Message)]
struct TradeBook {
    #[prost(int64, tag = "1")]
    next_id: i64,
    #[prost(message, repeated, tag = "2")]
    orders: Vec<TradeOrder>,
    #[prost(message, repeated, tag = "3")]
    deals: Vec<TradeDeal>,
    #[prost(int64, tag = "4")]
    version: i64,
}

mod book_bytes {
    use serde::{de::Visitor, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_bytes(v)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
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
            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Vec<u8>, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element()? {
                    out.push(b);
                }
                Ok(out)
            }
        }
        d.deserialize_any(V)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct BookDoc {
    #[serde(with = "book_bytes")]
    data: Vec<u8>,
}

fn ts(sec: i64) -> Option<Timestamp> {
    Some(Timestamp { seconds: sec, nanos: 0 })
}

fn user_key(uid: u64) -> tgf::DPlayerIdKey {
    tgf::DPlayerIdKey { user_id: uid, zone_id: 1 }
}

fn gold_item(n: i64) -> tgf::DItem {
    tgf::DItem {
        basic: Some(tgf::DItemBasic { type_id: GOLD, count: n.clamp(0, i32::MAX as i64) as i32, ..Default::default() }),
        ..Default::default()
    }
}

fn type_of(it: &tgf::DItem) -> i32 {
    it.basic.as_ref().map_or(0, |b| b.type_id)
}

fn count_of(it: &tgf::DItem) -> i32 {
    it.basic.as_ref().map_or(1, |b| b.count.max(1))
}

fn after_tax(gross: i64, tax: i32) -> i64 {
    gross - gross * tax.clamp(0, 100) as i64 / 100
}

fn conf_data(key: i64, ctype: i32, item: tgf::DItem, unit: i64, count: i32, total: i64, final_price: i64, now: i64) -> Pend {
    Pend::ConfirmationData(tgf::DTradeTicketConfirmation {
        confirmation: Some(tgf::DTradeConfirmation {
            confirmation_key: key,
            confirmation_type: ctype,
            confirm_timepoint: ts(now),
            item_data: Some(item),
            unit_price: unit,
            total_price: total,
            final_price,
            trade_count: count,
            ..Default::default()
        }),
    })
}

fn refund_data(key: i64, items: Vec<tgf::DItem>) -> Pend {
    Pend::RefundData(tgf::DTradeTicketRefund { confirmation_key: key, refund_items: items })
}

fn pend_items(p: &tgf::DTradePendingConfirmation) -> Vec<tgf::DItem> {
    match &p.data {
        Some(Pend::ConfirmationData(c)) => c.confirmation.as_ref().and_then(|c| c.item_data.clone()).into_iter().collect(),
        Some(Pend::RefundData(r)) => r.refund_items.clone(),
        None => vec![],
    }
}

fn new_meta(id: i64, key: tgf::DTradeMarketProductKey, now: i64, end: i64, kind: Kind) -> tgf::DTradeOrderTicketMeta {
    tgf::DTradeOrderTicketMeta {
        trade_order_ticket_id: id,
        trade_product_key: Some(key),
        ticket_publish_timestamp: ts(now),
        ticket_state: ST_TRADING,
        valid_begin_timestamp: ts(now),
        valid_end_timestamp: ts(end),
        r#type: Some(kind),
        ..Default::default()
    }
}

/// Take n pieces out of `escrow` (a stack larger than needed is split)
fn pull_units(escrow: &mut Vec<tgf::DItem>, mut n: i32) -> Vec<tgf::DItem> {
    let mut out = Vec::new();
    while n > 0 && !escrow.is_empty() {
        let c = count_of(&escrow[0]);
        if c <= n {
            n -= c;
            out.push(escrow.remove(0));
        } else {
            let mut part = escrow[0].clone();
            if let Some(b) = part.basic.as_mut() {
                b.count = n;
            }
            if let Some(b) = escrow[0].basic.as_mut() {
                b.count = c - n;
            }
            out.push(part);
            n = 0;
        }
    }
    out
}

impl TradeOrder {
    fn id(&self) -> i64 {
        self.meta.as_ref().map_or(0, |m| m.trade_order_ticket_id)
    }
    fn state(&self) -> i32 {
        self.meta.as_ref().map_or(0, |m| m.ticket_state)
    }
    fn set_state(&mut self, s: i32) {
        self.meta.get_or_insert_with(Default::default).ticket_state = s;
    }
    fn product_key(&self) -> tgf::DTradeMarketProductKey {
        self.meta.as_ref().and_then(|m| m.trade_product_key.clone()).unwrap_or_default()
    }
    fn key(&self) -> (i32, i32) {
        let k = self.product_key();
        (k.trade_market_id, k.trade_product_id)
    }
    fn end_ts(&self) -> i64 {
        self.meta.as_ref().and_then(|m| m.valid_end_timestamp.as_ref()).map_or(0, |t| t.seconds)
    }
    fn kind(&self) -> i32 {
        match self.meta.as_ref().and_then(|m| m.r#type.as_ref()) {
            Some(Kind::SalesOrder(_)) => SALES,
            Some(Kind::PurchaseOrder(_)) => PURCHASE,
            Some(Kind::AuctionOrder(_)) if self.auction_id != 0 => BID,
            Some(Kind::AuctionOrder(_)) => AUCTION,
            None => 0,
        }
    }
    /// Active: trading and not expired
    fn live(&self, now: i64) -> bool {
        self.state() == ST_TRADING && (self.end_ts() == 0 || self.end_ts() > now)
    }
    fn has_pending(&self) -> bool {
        self.confirm.as_ref().map_or(false, |c| !c.pending_receive.is_empty())
    }
    fn add_pending(&mut self, key: i64, data: Pend) {
        let id = self.id();
        let c = self.confirm.get_or_insert_with(|| tgf::DUserTradeConfirmData { trade_order_ticket_id: id, ..Default::default() });
        c.pending_receive.insert(key, tgf::DTradePendingConfirmation { trade_order_ticket_id: id, data: Some(data) });
    }
    fn sales(&self) -> Option<&tgf::DTradeSalesOrderMeta> {
        match self.meta.as_ref()?.r#type.as_ref()? {
            Kind::SalesOrder(s) => Some(s),
            _ => None,
        }
    }
    fn sales_mut(&mut self) -> Option<&mut tgf::DTradeSalesOrderMeta> {
        match self.meta.as_mut()?.r#type.as_mut()? {
            Kind::SalesOrder(s) => Some(s),
            _ => None,
        }
    }
    fn purchase(&self) -> Option<&tgf::DTradePurchaseOrderMeta> {
        match self.meta.as_ref()?.r#type.as_ref()? {
            Kind::PurchaseOrder(p) => Some(p),
            _ => None,
        }
    }
    fn purchase_mut(&mut self) -> Option<&mut tgf::DTradePurchaseOrderMeta> {
        match self.meta.as_mut()?.r#type.as_mut()? {
            Kind::PurchaseOrder(p) => Some(p),
            _ => None,
        }
    }
    fn auction(&self) -> Option<&tgf::DTradeAuctionMeta> {
        match self.meta.as_ref()?.r#type.as_ref()? {
            Kind::AuctionOrder(a) => Some(a),
            _ => None,
        }
    }
    fn auction_mut(&mut self) -> Option<&mut tgf::DTradeAuctionMeta> {
        match self.meta.as_mut()?.r#type.as_mut()? {
            Kind::AuctionOrder(a) => Some(a),
            _ => None,
        }
    }
    /// Current lot price (auction) / price per piece (sale, purchase)
    fn price(&self) -> i64 {
        if let Some(s) = self.sales() {
            return s.unit_price as i64;
        }
        if let Some(p) = self.purchase() {
            return p.current_unit_price as i64;
        }
        self.auction().and_then(|a| a.auction_order_meta.as_ref()).map_or(0, |m| m.current_price as i64)
    }
    fn top_bid(&self) -> Option<tgf::DTradeBidOrderMeta> {
        self.auction()?.bid_order_meta.clone().filter(|b| b.bid_trade_order_ticket_id != 0)
    }
}

impl TradeBook {
    fn alloc(&mut self) -> i64 {
        self.next_id = self.next_id.max(1_000_000) + 1;
        self.next_id
    }
    fn idx(&self, id: i64) -> Option<usize> {
        self.orders.iter().position(|o| o.id() == id)
    }
    fn deal(&mut self, market: i32, product: i32, unit_price: i64, count: i32, now: i64) {
        self.deals.push(TradeDeal { market, product, unit_price, count, ts: now });
        if self.deals.len() > MAX_DEALS {
            let cut = self.deals.len() - MAX_DEALS;
            self.deals.drain(0..cut);
        }
    }
    /// Active sales of a product by ascending price (on equal price — older first), except own
    fn sales_for(&self, market: i32, product: i32, max_price: i64, exclude_uid: u64, now: i64) -> Vec<usize> {
        let mut v: Vec<(i64, i64, usize)> = self
            .orders
            .iter()
            .enumerate()
            .filter(|(_, o)| o.kind() == SALES && o.live(now) && o.key() == (market, product) && o.owner_user_id != exclude_uid)
            .filter(|(_, o)| o.sales().map_or(false, |s| s.left_item_count > 0) && o.price() <= max_price)
            .map(|(i, o)| (o.price(), o.id(), i))
            .collect();
        v.sort();
        v.into_iter().map(|x| x.2).collect()
    }
    /// Active purchases of a product by descending price (on equal price — older first), except own
    fn purchases_for(&self, market: i32, product: i32, min_price: i64, exclude_uid: u64, now: i64) -> Vec<usize> {
        let mut v: Vec<(i64, i64, usize)> = self
            .orders
            .iter()
            .enumerate()
            .filter(|(_, o)| o.kind() == PURCHASE && o.live(now) && o.key() == (market, product) && o.owner_user_id != exclude_uid)
            .filter(|(_, o)| o.purchase().map_or(false, |p| p.left_item_count > 0) && o.price() >= min_price)
            .map(|(i, o)| (-o.price(), o.id(), i))
            .collect();
        v.sort();
        v.into_iter().map(|x| x.2).collect()
    }
    fn deals_of(&self, market: i32, product: i32, since: i64) -> impl Iterator<Item = &TradeDeal> {
        self.deals.iter().filter(move |d| d.market == market && d.product == product && d.ts >= since)
    }
}

/// Listing price bounds (12.11): SP — the average deal price over a week (`avg`, if the product recalculates SP —
/// is_calculate_sp), but not below price_def × lower/100 (so cheap sales to oneself cannot knock down the bound); otherwise
/// price_def. min = max(valid_lower_limit, ⌈SP × lower / 100⌉), max = ⌊SP × upper / 100⌋ (upper 0 — no upper bound).
fn price_bounds(avg: Option<i64>, def: i64, lower: i64, upper: i64, valid_min: i64) -> (i64, Option<i64>) {
    let def = def.max(0);
    let lower = lower.max(0);
    let sp = match avg {
        Some(a) => a.max(def * lower / 100),
        None => def,
    };
    let min = if lower > 0 && sp > 0 { (sp * lower + 99) / 100 } else { 0 }.max(valid_min.max(1));
    let max = (upper > 0 && sp > 0).then(|| (sp * upper / 100).max(min));
    (min, max)
}

/// Which orders changed (push to owners) and which were deleted
#[derive(Default)]
struct Fx {
    touched: BTreeSet<i64>,
    deleted: Vec<(String, i64)>,
    removed_conf: Vec<(String, i64)>,
}

// ==========================================
// Pure book logic (no DB; covered by tests below)
// ==========================================

/// Purchase: take up to `need` pieces from sales at price ≤ max_price (cheapest first). The buyer pays the seller's price
/// (the difference from max_price is not charged at all). Returns (bought, gold spent).
/// `reroll` — applied to each bought item before it is written to the buyer (12.11: affixes are rolled anew, trade_reroll).
fn fill_buy(book: &mut TradeBook, buyer: &mut TradeOrder, max_price: i64, mut need: i32, now: i64, fx: &mut Fx, reroll: &dyn Fn(&mut tgf::DItem)) -> (i32, i64) {
    let (market, product) = buyer.key();
    let (mut filled, mut spent) = (0, 0i64);
    for i in book.sales_for(market, product, max_price, buyer.owner_user_id, now) {
        if need <= 0 {
            break;
        }
        let (unit, left, tax) = {
            let o = &book.orders[i];
            (o.price(), o.sales().map_or(0, |s| s.left_item_count), o.tax)
        };
        let q = need.min(left);
        if q <= 0 {
            continue;
        }
        let mut items = pull_units(&mut book.orders[i].escrow, q);
        items.iter_mut().for_each(|it| reroll(it));
        let q: i32 = items.iter().map(count_of).sum();
        if q <= 0 {
            continue;
        }
        let gross = unit * q as i64;
        let net = after_tax(gross, tax);
        let seller_key = book.alloc();
        let keys: Vec<i64> = items.iter().map(|_| book.alloc()).collect();
        {
            let o = &mut book.orders[i];
            let rest = o.sales_mut().map(|s| {
                s.left_item_count -= q;
                s.left_item_count
            });
            o.add_pending(seller_key, conf_data(seller_key, CT_MATCH, gold_item(net), unit, q, gross, net, now));
            if rest.unwrap_or(0) <= 0 || o.escrow.is_empty() {
                o.set_state(ST_CONFIRMATION);
            }
            fx.touched.insert(o.id());
        }
        for (k, it) in keys.into_iter().zip(items) {
            let c = count_of(&it);
            buyer.add_pending(k, conf_data(k, CT_MATCH, it, unit, c, unit * c as i64, unit * c as i64, now));
        }
        book.deal(market, product, unit, q, now);
        need -= q;
        filled += q;
        spent += gross;
    }
    (filled, spent)
}

/// Sale: give items from the seller's escrow to purchases with price ≥ min_price (most expensive first), at the buyer's price.
/// Returns the number of pieces sold. `reroll` — as in fill_buy.
fn fill_sell(book: &mut TradeBook, seller: &mut TradeOrder, min_price: i64, now: i64, fx: &mut Fx, reroll: &dyn Fn(&mut tgf::DItem)) -> i32 {
    let (market, product) = seller.key();
    let mut sold = 0;
    for i in book.purchases_for(market, product, min_price, seller.owner_user_id, now) {
        let avail: i32 = seller.escrow.iter().map(count_of).sum();
        if avail <= 0 {
            break;
        }
        let (unit, left) = {
            let o = &book.orders[i];
            (o.price(), o.purchase().map_or(0, |p| p.left_item_count))
        };
        let q = left.min(avail);
        if q <= 0 {
            continue;
        }
        let mut items = pull_units(&mut seller.escrow, q);
        items.iter_mut().for_each(|it| reroll(it));
        let gross = unit * q as i64;
        let keys: Vec<i64> = items.iter().map(|_| book.alloc()).collect();
        let refund_key = book.alloc();
        {
            let o = &mut book.orders[i];
            let (rest, rest_gold) = o.purchase_mut().map_or((0, 0), |p| {
                p.left_item_count -= q;
                p.left_price = (p.left_price as i64 - gross).max(0) as i32;
                (p.left_item_count, p.left_price)
            });
            for (k, it) in keys.into_iter().zip(items) {
                let c = count_of(&it);
                o.add_pending(k, conf_data(k, CT_MATCH, it, unit, c, unit * c as i64, unit * c as i64, now));
            }
            if rest <= 0 {
                if rest_gold > 0 {
                    // remainder of the purchase escrow (should not remain, but we do not lose it)
                    o.add_pending(refund_key, refund_data(refund_key, vec![gold_item(rest_gold as i64)]));
                    if let Some(p) = o.purchase_mut() {
                        p.left_price = 0;
                    }
                }
                o.set_state(ST_CONFIRMATION);
            }
            fx.touched.insert(o.id());
        }
        let k = book.alloc();
        let net = after_tax(gross, seller.tax);
        seller.add_pending(k, conf_data(k, CT_MATCH, gold_item(net), unit, q, gross, net, now));
        if let Some(s) = seller.sales_mut() {
            s.left_item_count -= q;
        }
        book.deal(market, product, unit, q, now);
        sold += q;
    }
    if seller.sales().map_or(false, |s| s.left_item_count <= 0) || seller.escrow.is_empty() {
        seller.set_state(ST_CONFIRMATION);
    }
    sold
}

/// 18.11 (owner's item 9, "Buyout does not give the set bonus"): the Buyout reroll (trade_reroll) keeps the seller's gear set.
/// The game warns about exactly this on listing (Trade_ProductSale_Tips11): a set item sold as unidentified stays a set item,
/// only the bonus "values" change. Our reroll took sets from the template pool — purple ones have none there (sets come only
/// from bosses/elites), blue ones ~15%: the set was lost. Now: the rolled properties stay, the rolled sets are dropped, the seller's
/// sets stay, their level (bonus value, random_level 0..9999) is rolled anew by `level(set id)` — as in the game text
/// (owner's decision 18.11: "reroll"). A set-only MINOR entry replaces one rolled MINOR entry, so the number of properties
/// does not grow. No set at the seller — the roll as is.
fn keep_gear_sets(orig: &[tgf::DBattleAffixRandomData], rolled: Vec<tgf::DBattleAffixRandomData>, level: &dyn Fn(i32) -> i32) -> Vec<tgf::DBattleAffixRandomData> {
    if orig.iter().all(|a| a.gear_set_random.is_empty()) {
        return rolled;
    }
    let sets_of = |o: &tgf::DBattleAffixRandomData| {
        let mut g = o.gear_set_random.clone();
        for (gid, c) in g.iter_mut() {
            c.random_data.iter_mut().for_each(|r| r.random_level = level(*gid));
        }
        g
    };
    let mut out: Vec<tgf::DBattleAffixRandomData> = rolled
        .into_iter()
        .map(|mut a| {
            a.gear_set_random.clear();
            a
        })
        .filter(|a| !a.attribute_random.is_empty() || !a.ability_set_random.is_empty() || !a.gem_slot_random.is_empty())
        .collect();
    for o in orig.iter().filter(|o| !o.gear_set_random.is_empty()) {
        let set_only = o.attribute_random.is_empty() && o.ability_set_random.is_empty() && o.gem_slot_random.is_empty();
        if !set_only {
            // MAJOR (attributes + sets in one entry): the sets go into the rolled entry of the same slot
            if let Some(a) = out.iter_mut().find(|a| a.affix_type == o.affix_type && a.location == o.location) {
                a.gear_set_random = sets_of(o);
                continue;
            }
        } else if let Some(i) = out.iter().rposition(|a| a.affix_type == o.affix_type) {
            out.remove(i);
        }
        out.push(tgf::DBattleAffixRandomData {
            location: o.location,
            affix_type: o.affix_type,
            gear_set_random: sets_of(o),
            ..Default::default()
        });
    }
    out
}

/// Close an order (cancel/expiration): everything from the escrow and the remaining purchase gold — to the owner as a refund.
/// 07.11: purchase deposit (gold) — not `refund_data`, but a confirmation with gold of type FailedReturn, like seller proceeds:
/// the client does not show `refund_data` with gold (stand and prod 07.11: the refund was never claimed, players pressed
/// "Cancel" again and got -4237). The remaining amount to buy is zeroed — the game does not offer to cancel a closed order.
fn close_refund(book: &mut TradeBook, i: usize, state: i32, now: i64, fx: &mut Fx) {
    let (gold_key, items_key) = (book.alloc(), book.alloc());
    let o = &mut book.orders[i];
    let items = std::mem::take(&mut o.escrow);
    let mut gold = (0i64, 0i32);
    if let Some(p) = o.purchase_mut() {
        gold = (p.left_price as i64, p.left_item_count);
        p.left_price = 0;
        p.left_item_count = 0;
    }
    let (amount, count) = gold;
    if amount > 0 {
        let unit = if count > 0 { amount / count as i64 } else { amount };
        o.add_pending(gold_key, conf_data(gold_key, CT_FAILED_RETURN, gold_item(amount), unit, count.max(1), amount, amount, now));
    }
    if !items.is_empty() {
        o.add_pending(items_key, refund_data(items_key, items));
    }
    o.set_state(state);
    fx.touched.insert(o.id());
}

/// Auction result: lot — to the top bidder, proceeds (minus tax) — to the seller; no bids — lot back to the seller
fn settle_auction(book: &mut TradeBook, i: usize, now: i64, fx: &mut Fx) {
    let (aid, (market, product), tax, top) = {
        let o = &book.orders[i];
        (o.id(), o.key(), o.tax, o.top_bid())
    };
    let Some((top, j)) = top.and_then(|b| book.idx(b.bid_trade_order_ticket_id).map(|j| (b, j))) else {
        close_refund(book, i, ST_EXPIRATION, now, fx);
        return;
    };
    let gross = top.price as i64;
    let net = after_tax(gross, tax);
    let items = std::mem::take(&mut book.orders[i].escrow);
    let keys: Vec<i64> = items.iter().map(|_| book.alloc()).collect();
    let seller_key = book.alloc();
    {
        let w = &mut book.orders[j];
        for (k, it) in keys.into_iter().zip(items) {
            let c = count_of(&it);
            w.add_pending(k, conf_data(k, CT_MATCH, it, gross, c, gross, gross, now));
        }
        w.set_state(ST_CONFIRMATION);
        fx.touched.insert(w.id());
    }
    {
        let o = &mut book.orders[i];
        o.add_pending(seller_key, conf_data(seller_key, CT_MATCH, gold_item(net), gross, 1, gross, net, now));
        o.set_state(ST_CONFIRMATION);
        fx.touched.insert(aid);
    }
    for o in book.orders.iter_mut().filter(|o| o.auction_id == aid && o.id() != top.bid_trade_order_ticket_id) {
        if o.state() == ST_TRADING {
            o.set_state(ST_EXPIRATION); // outbid bids: gold was already returned when outbid
            fx.touched.insert(o.id());
        }
    }
    book.deal(market, product, gross, 1, now);
}

/// 07.11: purchases closed before the fix — the deposit refund lies in `refund_data` with gold (the client does not show it), the remaining
/// amount to buy is not zeroed (the game offers "Cancel" → -4237). Converted into a FailedReturn confirmation; a repeat changes nothing.
fn migrate_purchase_refunds(book: &mut TradeBook, now: i64, fx: &mut Fx) {
    for o in book.orders.iter_mut() {
        if o.kind() != PURCHASE || o.state() == ST_TRADING {
            continue;
        }
        let id = o.id();
        let mut changed = false;
        if let Some(p) = o.purchase_mut() {
            if p.left_item_count != 0 || p.left_price != 0 {
                p.left_item_count = 0;
                p.left_price = 0;
                changed = true;
            }
        }
        if let Some(c) = o.confirm.as_mut() {
            for (k, v) in c.pending_receive.iter_mut() {
                let gold = match &v.data {
                    Some(Pend::RefundData(r)) if !r.refund_items.is_empty() && r.refund_items.iter().all(|it| type_of(it) == GOLD) => {
                        r.refund_items.iter().map(|it| count_of(it) as i64).sum::<i64>()
                    }
                    _ => continue,
                };
                v.data = Some(conf_data(*k, CT_FAILED_RETURN, gold_item(gold), gold, 1, gold, gold, now));
                changed = true;
            }
        }
        if changed {
            fx.touched.insert(id);
        }
    }
}

/// Lazy expiration handling: expired sales/purchases — refund, expired auctions — result
fn tick(book: &mut TradeBook, now: i64, fx: &mut Fx) {
    migrate_purchase_refunds(book, now, fx);
    for i in 0..book.orders.len() {
        let o = &book.orders[i];
        if o.state() != ST_TRADING || o.end_ts() == 0 || o.end_ts() > now {
            continue;
        }
        match o.kind() {
            SALES | PURCHASE => close_refund(book, i, ST_EXPIRATION, now, fx),
            AUCTION => settle_auction(book, i, now, fx),
            _ => {}
        }
    }
}

/// Auction bid. The previous leader immediately gets the gold back (FailedReturn refund). Price ≥ buyout — immediate deal.
/// Returns (bid id, gold to charge).
fn place_bid(book: &mut TradeBook, auction_id: i64, bidder: (u64, &str), price: i32, pct: i32, now: i64, fx: &mut Fx) -> Result<(i64, i32), i32> {
    let i = book.idx(auction_id).ok_or(E_TICKET_UNAVAILABLE)?;
    let o = &book.orders[i];
    if o.kind() != AUCTION {
        return Err(E_TICKET_UNAVAILABLE);
    }
    if !o.live(now) {
        return Err(E_TICKET_FINISHED);
    }
    if o.owner_user_id == bidder.0 {
        return Err(E_AUCTION_BID_SELF);
    }
    let am = o.auction().and_then(|a| a.auction_order_meta.clone()).unwrap_or_default();
    let top = o.top_bid();
    if top.as_ref().and_then(|b| b.user_key.as_ref()).map(|k| k.user_id) == Some(bidder.0) {
        return Err(E_AUCTION_ALREADY_BID);
    }
    let min = match &top {
        None => am.origin_price as i64,
        Some(_) => am.current_price as i64 + (am.current_price as i64 * pct.max(0) as i64 / 100).max(1),
    };
    let price = if am.fixed_price > 0 { price.min(am.fixed_price) } else { price };
    let buyout = am.fixed_price > 0 && price >= am.fixed_price;
    if price <= 0 || ((price as i64) < min && !buyout) {
        return Err(E_CURRENT_PRICE_BID);
    }
    let (key, end, item, label) = (o.product_key(), o.end_ts(), o.item.clone(), o.label.clone());

    let bid_id = book.alloc();
    let bid_meta = tgf::DTradeBidOrderMeta { bid_trade_order_ticket_id: bid_id, user_key: Some(user_key(bidder.0)), price };
    if let Some(prev) = top {
        if let Some(j) = book.idx(prev.bid_trade_order_ticket_id) {
            let k = book.alloc();
            let w = &mut book.orders[j];
            let mut back = gold_item(prev.price as i64);
            if let Some(b) = back.basic.as_mut() {
                b.count = prev.price;
            }
            w.add_pending(k, refund_data(k, vec![back]));
            fx.touched.insert(w.id());
        }
    }
    let new_am = tgf::DTradeAuctionOrderMeta { current_price: price, ..am };
    if let Some(a) = book.orders[i].auction_mut() {
        a.auction_order_meta = Some(new_am.clone());
        a.bid_order_meta = Some(bid_meta.clone());
    }
    fx.touched.insert(auction_id);
    for o in book.orders.iter_mut().filter(|o| o.auction_id == auction_id) {
        if let Some(a) = o.auction_mut() {
            a.auction_order_meta = Some(new_am.clone());
        }
        fx.touched.insert(o.id());
    }
    book.orders.push(TradeOrder {
        meta: Some(new_meta(
            bid_id,
            key,
            now,
            end,
            Kind::AuctionOrder(tgf::DTradeAuctionMeta { auction_order_meta: Some(new_am), bid_order_meta: Some(bid_meta) }),
        )),
        owner_open_id: bidder.1.to_string(),
        owner_user_id: bidder.0,
        auction_id,
        item,
        label,
        ..Default::default()
    });
    fx.touched.insert(bid_id);
    if buyout {
        settle_auction(book, i, now, fx);
    }
    Ok((bid_id, price))
}

// ==========================================
// Label search
// ==========================================

/// Label match against a rule: type/subtype, tags (OR within a tag, AND between tags), rarity, exclusions.
/// Tags we do not have in the label (some professions are not marked by the tables) do not filter out: TODO(verify).
fn label_match(
    l: &tgf::DTradeOrderTicketLabel,
    major: i32,
    minor: i32,
    tags: &[tgf::DTradeOrderTicketSearchTag],
    attrs: &[tgf::DTradeOrderTicketSearchAttribute],
    exclude: &[tgf::DTradeOrderTicketSearchTag],
) -> bool {
    if major != 0 && l.major_index_type != major {
        return false;
    }
    if minor != 0 && l.minor_index_type != minor {
        return false;
    }
    // profession (1002) is not strict: an item without a label (ring, amulet) suits all classes
    let strict = |t: i32| matches!(t, TAG_GEAR_SUB_TYPE | TAG_AFFIX_ATTRIBUTE | TAG_AFFIX_ATTRIBUTE_FIXED | TAG_WEARING_PART | TAG_CORRUPTION | TAG_AFFIX_GEAR_SET);
    for t in tags.iter().filter(|t| !t.values.is_empty()) {
        match l.tags.iter().find(|x| x.r#type == t.r#type) {
            Some(x) if !x.values.iter().any(|v| t.values.contains(v)) => return false,
            None if strict(t.r#type) => return false,
            _ => {}
        }
    }
    for t in exclude {
        if l.tags.iter().any(|x| x.r#type == t.r#type && x.values.iter().any(|v| t.values.contains(v))) {
            return false;
        }
    }
    for a in attrs.iter().filter(|a| a.r#type == ATTR_RARITY) {
        let r = l.attributes.iter().find(|x| x.r#type == ATTR_RARITY).map_or(0, |x| x.values);
        if r < a.min_value || (a.max_value > 0 && r > a.max_value) {
            return false;
        }
    }
    true
}

fn rarity_of(l: &tgf::DTradeOrderTicketLabel) -> i32 {
    l.attributes.iter().find(|x| x.r#type == ATTR_RARITY).map_or(0, |x| x.values)
}

/// Auctions matching a search rule (market, labels, price), sorted: sort_by 1 — by price, otherwise newest first
fn auction_search<'a>(book: &'a TradeBook, rule: &tgf::DTradeOrderTicketSearchRule, sort_by: i32, now: i64) -> Vec<&'a TradeOrder> {
    let mut v: Vec<&TradeOrder> = book
        .orders
        .iter()
        .filter(|o| o.kind() == AUCTION && o.live(now))
        .filter(|o| rule.trade_market_id == 0 || o.key().0 == rule.trade_market_id)
        .filter(|o| rule.min_price <= 0 || o.price() >= rule.min_price as i64)
        .filter(|o| rule.max_price <= 0 || o.price() <= rule.max_price as i64)
        .filter(|o| {
            let l = o.label.clone().unwrap_or_default();
            label_match(&l, rule.major_index_type, rule.minor_index_type, &rule.tags, &rule.attributes, &rule.exclude_tags)
        })
        .collect();
    if sort_by == 1 {
        v.sort_by_key(|o| (o.price(), o.id()));
    } else {
        v.sort_by_key(|o| -o.id());
    }
    if let Some(b) = rule.bound_exclude.as_ref().filter(|b| b.trade_order_ticket_id != 0) {
        if let Some(pos) = v.iter().position(|o| o.id() == b.trade_order_ticket_id) {
            v.drain(0..=pos);
        }
    }
    v
}

fn page_of(v: Vec<&TradeOrder>, count: i32) -> (Vec<tgf::DTradeOrderTicketMeta>, bool) {
    let n = if count > 0 { count as usize } else { PAGE };
    let more = v.len() > n;
    (v.into_iter().take(n).filter_map(|o| o.meta.clone()).collect(), more)
}

static COMMODITIES: OnceLock<BTreeMap<(i32, i32), tgf::config::ExcelTradeCommodity>> = OnceLock::new();
static TRADE_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

/// trade_commodity keyed by (market_id, id): we read .bytes ourselves (in GameData duplicate ids across markets are overwritten)
fn commodities() -> &'static BTreeMap<(i32, i32), tgf::config::ExcelTradeCommodity> {
    COMMODITIES.get_or_init(|| {
        crate::game_data::load_bytes_vec::<tgf::config::ExcelTradeCommodity>("ExcelConfig/trade_commodity.bytes")
            .into_iter()
            .map(|c| ((c.market_id, c.id), c))
            .collect()
    })
}

/// 09.11: gear classes and slots for auction labels (UeSource_Equipment + UeSource_Equipment_SubType).
/// Key — the item's ue_source id. Weapons: classes from weapon_configs.career_restrict, slots by weapon_equip_type
/// (right/left hand of both pairs; can_equip_double — left too); armor and jewelry: classes by subtype
/// (SubType.career_config), slots — gear_configs.available_equip_slot. An empty class list — suits everyone.
#[derive(Default, Clone, Debug, PartialEq)]
struct GearFit {
    careers: Vec<i32>,
    parts: Vec<i32>,
}

static GEAR_FIT: OnceLock<HashMap<i32, GearFit>> = OnceLock::new();

fn gear_fit_table() -> &'static HashMap<i32, GearFit> {
    GEAR_FIT.get_or_init(|| {
        let subs = crate::game_data::load_bytes_vec::<tgf::config::UeSourceEquipmentSubType>("ExcelConfig/UeSource_Equipment_SubType.bytes");
        let equip = crate::game_data::load_bytes_vec::<tgf::config::UeSourceEquipment>("ExcelConfig/UeSource_Equipment.bytes");
        let t = gear_fit_build(&subs, &equip);
        println!("[Trade] gear classes and slots for search: {} items", t.len());
        t
    })
}

fn gear_fit_build(subs: &[tgf::config::UeSourceEquipmentSubType], equip: &[tgf::config::UeSourceEquipment]) -> HashMap<i32, GearFit> {
    use tgf::EnItemWearingPart as W;
    let by_sub: HashMap<i32, Vec<i32>> =
        subs.iter().flat_map(|s| s.career_config.iter()).map(|c| (c.equip_subtype, c.career_restrict.clone())).collect();
    let clean = |v: &mut Vec<i32>| {
        v.retain(|c| *c != 0);
        v.sort_unstable();
        v.dedup();
    };
    equip
        .iter()
        .map(|e| {
            let mut f = GearFit::default();
            let weapons: Vec<_> = e.weapon_configs.iter().filter(|w| w.is_weapon).collect();
            for w in &weapons {
                f.careers.extend(w.career_restrict.iter().copied());
                let right = [W::TypePair1rightweapon as i32, W::TypePair2rightweapon as i32];
                let left = [W::TypePair1leftweapon as i32, W::TypePair2leftweapon as i32];
                match tgf::EnWeaponEquipType::try_from(w.weapon_equip_type).unwrap_or(tgf::EnWeaponEquipType::None) {
                    tgf::EnWeaponEquipType::Onlyright | tgf::EnWeaponEquipType::Bothhand => {
                        f.parts.extend(right);
                        if w.can_equip_double {
                            f.parts.extend(left);
                        }
                    }
                    tgf::EnWeaponEquipType::Onlyleft => f.parts.extend(left),
                    _ => {}
                }
            }
            if weapons.is_empty() {
                f.careers.extend(by_sub.get(&e.equip_subtype).into_iter().flatten().copied());
            }
            for g in e.gear_configs.iter().filter(|g| g.is_gear) {
                f.careers.extend(g.career_restrict.iter().copied());
                f.parts.extend(g.available_equip_slot.iter().copied().filter(|p| (1..=40).contains(p)));
            }
            clean(&mut f.careers);
            clean(&mut f.parts);
            (e.equip_id, f)
        })
        .collect()
}

fn trade_lock() -> &'static tokio::sync::Mutex<()> {
    TRADE_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

/// Player item lists (except worn) with the place type and a "role container" flag
fn item_lists(p: &mut PlayerModel) -> Vec<(i32, &mut Vec<tgf::DItem>)> {
    let PlayerModel { stash, roles, .. } = p;
    let mut out: Vec<(i32, &mut Vec<tgf::DItem>)> = vec![(POS_STASH, &mut stash.element)];
    for r in roles.iter_mut() {
        if let Some(d) = r.data.deploy_role.as_mut() {
            let tgf::DBattleDeployRole { backpack, small_backpack, wearing, .. } = d;
            if let Some(b) = backpack.as_mut() {
                out.push((POS_BACKPACK, &mut b.items));
            }
            if let Some(b) = small_backpack.as_mut() {
                out.push((POS_SMALL_BACKPACK, &mut b.items));
            }
            if let Some(w) = wearing.as_mut() {
                out.push((POS_WEARING, &mut w.items));
            }
        }
    }
    out
}

/// An item taken for an order
struct Taken {
    item: tgf::DItem,
    /// for dirty_postion_action: "removed from the old place" or the remainder of a split stack
    dirty: tgf::DItem,
    removed_guid: Option<i64>,
    from_role: bool,
}

/// Take item `b` (guid, count) from the player for escrow. Worn and worn-out items are not sold.
fn take_for_trade(p: &mut PlayerModel, b: &tgf::DItemBasic, want_type: i32) -> Result<Taken, i32> {
    for (place, list) in item_lists(p) {
        let Some(i) = list.iter().position(|it| it.basic.as_ref().map_or(false, |x| x.guid == b.guid)) else { continue };
        if place == POS_WEARING {
            return Err(E_NOT_ALLOW_SELL_WEARING);
        }
        if want_type != 0 && type_of(&list[i]) != want_type {
            return Err(E_SALE_ITEM_INVALID);
        }
        if list[i].data.as_ref().map_or(false, |d| d.loss_durability > 0) {
            return Err(E_LOSS_DURABILITY); // TODO(verify): the official server requires repair before selling
        }
        let have = count_of(&list[i]);
        let want = if b.count > 0 { b.count } else { have };
        if want > have {
            return Err(E_SALE_ITEM_INVALID);
        }
        if want < have {
            let mut part = list[i].clone();
            if let Some(x) = part.basic.as_mut() {
                x.count = want;
            }
            if let Some(x) = list[i].basic.as_mut() {
                x.count = have - want;
            }
            return Ok(Taken { item: part, dirty: list[i].clone(), removed_guid: None, from_role: place != POS_STASH });
        }
        let item = list.remove(i);
        let ob = item.basic.clone().unwrap_or_default();
        let dirty = tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id: ob.type_id, guid: ob.guid, position: ob.position.clone(), ..Default::default() }),
            ..Default::default()
        };
        return Ok(Taken { item, dirty, removed_guid: Some(ob.guid), from_role: place != POS_STASH });
    }
    Err(E_SALE_ITEM_INVALID)
}

impl GatewaySession {
    // ---------- internals ----------

    fn trade_commodity_of(&self, market: i32, product: i32) -> Option<tgf::config::ExcelTradeCommodity> {
        commodities()
            .get(&(market, product))
            .cloned()
            .or_else(|| self.game_data.trade_commodity.get(&product).filter(|c| c.market_id == market).cloned())
    }

    /// Product and market for listing: present in the tables, on the shelf, market of the right type
    fn trade_check_product(&self, key: &tgf::DTradeMarketProductKey, market_type: i32) -> Result<(tgf::config::ExcelTradeCommodity, tgf::config::ExcelTradeMarket), i32> {
        let market = self.game_data.trade_market.get(&key.trade_market_id).cloned().ok_or(E_MARKET_NOT_FOUND)?;
        if market.market_type != market_type {
            return Err(E_MARKET_TYPE_NOT_MATCH);
        }
        let c = self.trade_commodity_of(key.trade_market_id, key.trade_product_id).ok_or(E_PRODUCT_NOT_FOUND)?;
        if !c.is_on_shelf {
            return Err(E_NOT_ON_SHELF);
        }
        Ok((c, market))
    }

    fn trade_currency(market: &tgf::config::ExcelTradeMarket) -> i32 {
        if market.currency_type != 0 { market.currency_type } else { GOLD }
    }

    /// Order duration: trade_valid_time[order_valid_time] (po — purchase, so — sale/auction)
    fn trade_valid_secs(&self, c: &tgf::config::ExcelTradeCommodity, purchase: bool) -> i64 {
        self.game_data
            .trade_valid_time
            .get(&c.order_valid_time)
            .and_then(|v| if purchase { v.po_valid_time.as_ref() } else { v.so_valid_time.as_ref() })
            .map(|d| d.seconds)
            .filter(|s| *s > 0)
            .unwrap_or(if purchase { 3600 } else { 86400 })
    }

    fn trade_is_coin(&self, type_id: i32) -> bool {
        type_id == GOLD || self.game_data.item_type.get(&type_id).map_or(false, |t| t.item_major_type == 1)
    }

    /// Gear subtype for the Item Type filter. `slot_type` matches EnGearEquipSubType numbers; for rings, amulets
    /// and some weapons it is empty, then by class name. Armor also goes as a common type (COMMON_CHEST…): which one the client sends is not seen yet.
    fn trade_gear_sub_types(&self, type_id: i32) -> Vec<i32> {
        let Some(u) = self.game_data.uesource_itemtype.get(&type_id) else { return vec![] };
        let name = u.ue_path.rsplit('/').next().unwrap_or_default();
        match u.slot_type {
            n @ 1..=25 if (1..=5).contains(&(n % 10)) => vec![n, 200 + n % 10],
            0 if name.contains("Ring") => vec![31],
            0 if name.contains("Necklace") => vec![32],
            0 if name.contains("LongSword") => vec![64],
            0 => vec![],
            n => vec![n],
        }
    }

    /// Item label for search: type/subtype (item_type), gear subtype, profession, affixes,
    /// corruption, set; attribute — rarity (UeSource_ItemType.rarity)
    fn trade_label(&self, type_id: i32, item: Option<&tgf::DItem>) -> tgf::DTradeOrderTicketLabel {
        let mut l = tgf::DTradeOrderTicketLabel::default();
        // 09.11: classes and slots — from the gear tables (previously only special_career_require, for regular gear it is
        // empty — the class filter let everything through)
        let ue = self.game_data.item_type.get(&type_id).map(|t| t.ue_source_type_id).filter(|i| *i != 0).unwrap_or(type_id);
        let fit = gear_fit_table().get(&ue).or_else(|| gear_fit_table().get(&type_id)).cloned().unwrap_or_default();
        if let Some(t) = self.game_data.item_type.get(&type_id) {
            l.major_index_type = t.item_major_type;
            l.minor_index_type = t.item_minor_type;
            let careers = if !t.special_career_require.is_empty() { t.special_career_require.clone() } else { fit.careers.clone() };
            if !careers.is_empty() {
                l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_ROLE_CAREER, values: careers });
            }
        }
        if !fit.parts.is_empty() {
            l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_WEARING_PART, values: fit.parts.clone() });
        }
        let sub_types = self.trade_gear_sub_types(type_id);
        if !sub_types.is_empty() {
            l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_GEAR_SUB_TYPE, values: sub_types });
        }
        if let Some(d) = item.and_then(|i| i.data.as_ref()) {
            // 09.11: base properties (affix_type BASIC) — tag "fixed affix" (1008), the rest — 1003
            let basic = |a: &&tgf::DBattleAffixRandomData| a.affix_type == tgf::EnBattleAffixType::Basic as i32;
            let fixed: BTreeSet<i32> = d.affix_data.iter().filter(basic).flat_map(|a| a.attribute_random.keys().copied()).collect();
            let attrs: BTreeSet<i32> = d.affix_data.iter().filter(|a| !basic(a)).flat_map(|a| a.attribute_random.keys().copied()).collect();
            let sets: BTreeSet<i32> = d.affix_data.iter().flat_map(|a| a.gear_set_random.keys().copied()).collect();
            if !attrs.is_empty() {
                l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_AFFIX_ATTRIBUTE, values: attrs.into_iter().collect() });
            }
            if !fixed.is_empty() {
                l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_AFFIX_ATTRIBUTE_FIXED, values: fixed.into_iter().collect() });
            }
            if !sets.is_empty() {
                l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_AFFIX_GEAR_SET, values: sets.into_iter().collect() });
            }
            if d.corruption_id != 0 {
                l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_CORRUPTION, values: vec![d.corruption_id] });
            }
        }
        let rarity = self.game_data.uesource_itemtype.get(&type_id).map_or(0, |u| u.rarity);
        l.attributes.push(tgf::DTradeOrderTicketLabelAttribute { r#type: ATTR_RARITY, values: rarity });
        l
    }

    /// Item size in the stash: (by x, by y)
    fn trade_footprint(&self, type_id: i32) -> (i32, i32) {
        // common shape (via item_type.ue_source_type_id, as in the other domains)
        let s = self.inv_item_shape(type_id);
        (s.w, s.h)
    }

    /// Free stash space for an item; if goal fits (stash, in bounds, cells free) — take it
    fn trade_stash_place(&self, p: &PlayerModel, type_id: i32, goal: Option<&tgf::DItemPosition>) -> Option<tgf::DItemPosition> {
        let mut busy = HashSet::new();
        for it in &p.stash.element {
            let Some(pos) = it.basic.as_ref().and_then(|b| b.position.as_ref()) else { continue };
            let (w, h) = self.trade_footprint(type_of(it));
            for dx in 0..w {
                for dy in 0..h {
                    busy.insert((pos.guid, pos.x + dx, pos.y + dy));
                }
            }
        }
        let (w, h) = self.trade_footprint(type_id);
        let fits = |page: i64, x: i32, y: i32| {
            x >= 0 && y >= 0 && x + w <= STASH_W && y + h <= STASH_H && (0..w).all(|dx| (0..h).all(|dy| !busy.contains(&(page, x + dx, y + dy))))
        };
        let pages = super::inventory_handlers::stash_page_count(p); // ext: stash_pages — the player's pages, not 8
        if let Some(g) = goal.filter(|g| g.r#type == POS_STASH && g.guid >= 0 && g.guid < pages) {
            if fits(g.guid, g.x, g.y) {
                return Some(tgf::DItemPosition { r#type: POS_STASH, guid: g.guid, x: g.x, y: g.y, limit_option: None });
            }
        }
        for page in 0..pages {
            for y in 0..STASH_H {
                for x in 0..STASH_W {
                    if fits(page, x, y) {
                        return Some(tgf::DItemPosition { r#type: POS_STASH, guid: page, x, y, limit_option: None });
                    }
                }
            }
        }
        None
    }

    async fn trade_load_book(&self) -> TradeBook {
        let mut book = self
            .db
            .get::<BookDoc>(BOOK_TABLE, BOOK_ID)
            .await
            .and_then(|d| TradeBook::decode(d.data.as_slice()).ok())
            .unwrap_or_default();
        // 09.11: auction lot labels — rebuilt from the item (lots listed before classes/slots/affixes were labeled)
        for o in book.orders.iter_mut().filter(|o| o.kind() == AUCTION) {
            if let Some(item) = o.item.clone() {
                o.label = Some(self.trade_label(type_of(&item), Some(&item)));
            }
        }
        book
    }

    async fn trade_save_book(&self, book: &mut TradeBook) -> bool {
        book.version += 1;
        let doc = BookDoc { data: book.encode_to_vec() };
        match self.db.db.upsert::<Option<BookDoc>>((BOOK_TABLE, BOOK_ID)).content(doc).await {
            Ok(_) => true,
            Err(e) => {
                eprintln!("[Trade] failed to save the order book: {}", e);
                false
            }
        }
    }

    /// Save the player (if changed), then the book; if the book is not written — the player goes back to the copy
    async fn trade_commit(&self, book: &mut TradeBook, me: Option<(&PlayerModel, &PlayerModel)>) -> bool {
        if let Some((p, _)) = me {
            // 16.11: the character's item is in a match — the player is not saved, we do not write the book (otherwise an order with an item they still have)
            if !self.save_me(p).await {
                return false;
            }
        }
        if self.trade_save_book(book).await {
            return true;
        }
        if let Some((_, backup)) = me {
            self.save_me(backup).await;
        }
        false
    }

    /// dirty_trade_market pushes to owners of changed orders; to self — together with `me` (coins, items) after the response
    fn trade_flush(&self, book: &TradeBook, fx: Fx, mut me: tgf::ScPlayerDirtyChgSync) {
        let my = self.clean_open_id();
        let mut per: BTreeMap<String, tgf::DUserTradeMarketDirty> = BTreeMap::new();
        for id in &fx.touched {
            let Some(o) = book.orders.iter().find(|o| o.id() == *id) else { continue };
            let d = per.entry(o.owner_open_id.clone()).or_default();
            d.dirty_orders.extend(o.meta.clone());
            let c = o.confirm.clone().unwrap_or(tgf::DUserTradeConfirmData { trade_order_ticket_id: *id, ..Default::default() });
            d.dirty_confimation.extend(c.pending_receive.values().cloned());
            d.dirty_confimation_full.push(c);
        }
        for (owner, id) in fx.deleted {
            per.entry(owner).or_default().delete_orders.push(id);
        }
        for (owner, key) in fx.removed_conf {
            per.entry(owner).or_default().delete_confimation.push(key);
        }
        for (owner, d) in per {
            if owner == my {
                me.dirty_trade_market = Some(d);
            } else {
                let msg = tgf::ScPlayerDirtyChgSync { dirty_trade_market: Some(d), ..Default::default() };
                common::push_to(&owner, hub::DIRTY_METHOD, hub::DIRTY_TYPE, &msg);
            }
        }
        if me != tgf::ScPlayerDirtyChgSync::default() {
            self.push_dirty(me);
        }
    }

    /// Expirations only (history, cancel etc.): tick + save if something expired
    /// 16.11 (wipe): remove all of the player's orders from the market. Their auctions with bids are first closed by a result (lot — to the top
    /// bidder), so other players' bid gold does not hang; their bids on other lots disappear — the lot goes to the next bidder or
    /// returns to the seller (settle_auction). Returns the removed orders (protobuf) for the snapshot; if the book is not written — Err.
    pub(super) async fn trade_wipe_user(&self, user_id: u64) -> Result<Vec<Vec<u8>>, String> {
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let bidded: Vec<usize> = (0..book.orders.len())
            .filter(|&i| {
                let o = &book.orders[i];
                o.owner_user_id == user_id && o.kind() == AUCTION && o.live(now) && o.top_bid().is_some()
            })
            .collect();
        for i in bidded {
            settle_auction(&mut book, i, now, &mut fx);
        }
        let (mine, rest): (Vec<TradeOrder>, Vec<TradeOrder>) = std::mem::take(&mut book.orders).into_iter().partition(|o| o.owner_user_id == user_id);
        book.orders = rest;
        for o in &mine {
            fx.touched.remove(&o.id());
        }
        if (!mine.is_empty() || !fx.touched.is_empty()) && !self.trade_save_book(&mut book).await {
            return Err("auction book not written — reset cancelled".into());
        }
        self.trade_flush(&book, fx, Default::default());
        Ok(mine.iter().map(prost::Message::encode_to_vec).collect())
    }

    async fn trade_book_ticked(&self, fx: &mut Fx) -> TradeBook {
        let mut book = self.trade_load_book().await;
        tick(&mut book, Self::now_ts(), fx);
        book
    }

    /// Market price of an item for insurance: price_def × `insurance.market_mult` (first the Trade row, market 1, then
    /// auction). Not on the market — 0. Result: type_id -> price. 13.11 (user: "abolish the dynamic market price
    /// system"): the average deal price over a week is no longer used — the price is constant, from the tables.
    pub(super) async fn insurance_market_prices(&self, type_ids: &[i32]) -> HashMap<i32, i64> {
        let mult = crate::season::features().insurance.market_mult;
        let mut out = HashMap::new();
        for &t in type_ids {
            let mut by_market: Vec<(i32, i64)> = commodities().iter().filter(|(_, c)| c.item_id == t).map(|((m, _), c)| (*m, c.price_def as i64)).collect();
            by_market.sort_by_key(|(m, _)| *m);
            let price = by_market.iter().find(|(_, d)| *d > 0).map_or(0, |(_, def)| (*def as f64 * mult).round() as i64);
            out.insert(t, price);
        }
        out
    }

    /// Standard product price (SP: the client shows it, listing bounds derive from it) — price_def of the trade table.
    /// 13.11 (user: "abolish the dynamic market price system", bounds x0.5–x4 of the table base price):
    /// previously — the average deal price over a week, and the bounds followed the deals.
    fn trade_standard_price(&self, _book: &TradeBook, market: i32, product: i32, _now: i64) -> i64 {
        self.trade_commodity_of(market, product).map_or(0, |c| c.price_def as i64)
    }

    /// 12.11 (user: "a minimum price check in both auctions"; tables — the tables session): listing price
    /// bounds from trade_commodity.price_lower/upper_limit (% of the standard price). Without the check a player could list at any price.
    /// 13.11: always from price_def (is_calculate_sp is ignored), season playtest — 50–400% on both markets.
    fn trade_price_bounds(&self, _book: &TradeBook, _key: &tgf::DTradeMarketProductKey, c: &tgf::config::ExcelTradeCommodity, market: &tgf::config::ExcelTradeMarket, _now: i64) -> (i64, Option<i64>) {
        price_bounds(None, c.price_def as i64, c.price_lower_limit as i64, c.price_upper_limit as i64, market.valid_lower_limit)
    }

    fn trade_update_interval(&self, market: i32) -> i64 {
        let pool = self.game_data.trade_market.get(&market).map_or(1, |m| m.calculate_pool_id.max(1));
        self.game_data
            .trade_standard_price
            .get(&pool)
            .and_then(|s| s.update_interval.as_ref())
            .map(|d| d.seconds)
            .filter(|s| *s > 0)
            .unwrap_or(7200)
    }

    /// Active orders of a product of the needed kind (sales/purchases — by order_type, on the auction — lots)
    fn trade_live_of<'a>(book: &'a TradeBook, market: i32, product: i32, order_type: i32, now: i64) -> Vec<&'a TradeOrder> {
        book.orders
            .iter()
            .filter(|o| o.live(now) && o.key() == (market, product))
            .filter(|o| match order_type {
                PURCHASE => o.kind() == PURCHASE,
                AUCTION => o.kind() == AUCTION,
                _ => o.kind() == SALES || o.kind() == AUCTION,
            })
            .collect()
    }

    /// The price fits within the product's current bounds (`trade_price_bounds`). 13.11 (user: "if someone before our fixes
    /// sold an item for 1 gold, the item type is broken and cannot be sold"; LOCAL analysis: the client steps the price on a grid from
    /// 3 gold, at a price of 1–2 the window sets 0 and the server rejects): deals and orders under the old rules outside the bounds
    /// are not counted in market responses — neither in the "lowest price" nor in history. No product — no check.
    fn trade_price_ok(&self, market: i32, product: i32) -> impl Fn(i64) -> bool {
        let bounds = self.trade_commodity_of(market, product).zip(self.game_data.trade_market.get(&market).cloned()).map(|(c, m)| {
            price_bounds(None, c.price_def as i64, c.price_lower_limit as i64, c.price_upper_limit as i64, m.valid_lower_limit)
        });
        move |p: i64| bounds.map_or(true, |(lo, hi)| p >= lo && hi.map_or(true, |h| p <= h))
    }

    fn trade_basic_info(&self, book: &TradeBook, key: &tgf::DTradeMarketProductKey, order_type: i32, now: i64) -> tgf::DTradeMarketProductBasicIndexInfo {
        let (market, product) = (key.trade_market_id, key.trade_product_id);
        let ok = self.trade_price_ok(market, product);
        let live: Vec<i64> = Self::trade_live_of(book, market, product, order_type, now).iter().map(|o| o.price()).filter(|p| ok(*p)).collect();
        let lowest = if order_type == PURCHASE { live.iter().copied().max().unwrap_or(0) } else { live.iter().copied().min().unwrap_or(0) };
        let iv = self.trade_update_interval(market);
        tgf::DTradeMarketProductBasicIndexInfo {
            trade_product_key: Some(key.clone()),
            lowest_price: lowest,
            standard_price: self.trade_standard_price(book, market, product, now),
            next_update_standard_price_time: ts((now / iv + 1) * iv),
            summary_version: book.version,
            timestamp: ts(now),
        }
    }

    /// Distribution of listed goods by price (ascending, at most 10 steps)
    fn trade_on_sale(book: &TradeBook, market: i32, product: i32, now: i64) -> Vec<tgf::DTradeMarketProductOnSaleDistribution> {
        let mut by: BTreeMap<i64, (i32, i32)> = BTreeMap::new();
        for o in Self::trade_live_of(book, market, product, SALES, now) {
            let e = by.entry(o.price()).or_default();
            e.0 += 1;
            e.1 += o.escrow.iter().map(count_of).sum::<i32>();
        }
        let more = by.len() > 10;
        by.into_iter()
            .take(10)
            .map(|(price, (orders, items))| tgf::DTradeMarketProductOnSaleDistribution {
                lowest_price: price,
                highest_price: price,
                order_ticket_count: orders,
                item_count: items,
                hide_more: more,
            })
            .collect()
    }

    /// Share of listed goods not more expensive than the price: cumulative % of pieces by price step. TODO(verify): meaning of percentage
    fn trade_percentage(book: &TradeBook, market: i32, product: i32, now: i64) -> Vec<tgf::DTradeProductPricePercentage> {
        let dist = Self::trade_on_sale(book, market, product, now);
        let total: i64 = dist.iter().map(|d| d.item_count as i64).sum();
        let mut acc = 0i64;
        dist.iter()
            .filter(|_| total > 0)
            .map(|d| {
                acc += d.item_count as i64;
                tgf::DTradeProductPricePercentage { percentage: acc * 100 / total, min_price: d.lowest_price }
            })
            .collect()
    }

    /// Price history: deals over a week by standard price recalculation intervals
    fn trade_metric_points(&self, book: &TradeBook, market: i32, product: i32, now: i64) -> Vec<tgf::DTradeMarketProductMetricPoint> {
        let iv = self.trade_update_interval(market);
        let mut by: BTreeMap<i64, (i64, i64, i64, i64)> = BTreeMap::new();
        let ok = self.trade_price_ok(market, product);
        for d in book.deals_of(market, product, now - WEEK).filter(|d| ok(d.unit_price)) {
            let e = by.entry(d.ts / iv * iv).or_insert((i64::MAX, 0, 0, 0));
            e.0 = e.0.min(d.unit_price);
            e.1 = e.1.max(d.unit_price);
            e.2 += d.unit_price * d.count as i64;
            e.3 += d.count as i64;
        }
        by.into_iter()
            .map(|(t, (lo, hi, sum, n))| tgf::DTradeMarketProductMetricPoint {
                timestamp: ts(t),
                lowest_price: lo,
                standard_price: if n > 0 { sum / n } else { 0 },
                highest_price: hi,
            })
            .collect()
    }

    /// Homogeneous market product card for search/recommendations
    fn trade_product_index(&self, book: &TradeBook, c: &tgf::config::ExcelTradeCommodity, order_type: i32, now: i64) -> (tgf::DTradeIndexRecommendIndexInfo, tgf::DTradeOrderTicketLabel) {
        let key = tgf::DTradeMarketProductKey { area_code: 0, trade_market_id: c.market_id, trade_product_id: c.id };
        let basic = self.trade_basic_info(book, &key, order_type, now);
        let label = self.trade_label(c.item_id, None);
        let has_order = !Self::trade_live_of(book, c.market_id, c.id, order_type, now).is_empty();
        let sale_num: i64 = book.deals_of(c.market_id, c.id, now - WEEK).map(|d| d.count as i64).sum();
        let bound = tgf::DTradeIndexSearchBound {
            recommend_score: sale_num,
            trade_product_id: c.id,
            sale_num,
            rarity: rarity_of(&label),
            search_order: c.id as i64,
            lowest_price: basic.lowest_price,
            has_order,
        };
        (tgf::DTradeIndexRecommendIndexInfo { basic_info: Some(basic), extra_info: Some(bound) }, label)
    }

    /// Market products matching a rule: those with orders first, then by id; bound — continue from the one after it
    fn trade_product_search(
        &self,
        book: &TradeBook,
        market: i32,
        order_type: i32,
        rule: Option<&tgf::DTradeIndexSearchBasicRule>,
        extra: impl Fn(&tgf::DTradeOrderTicketLabel) -> bool,
        now: i64,
    ) -> Vec<tgf::DTradeIndexRecommendIndexInfo> {
        let pr = rule.and_then(|r| r.product_rule.clone()).unwrap_or_default();
        let mut v: Vec<tgf::DTradeIndexRecommendIndexInfo> = commodities()
            .values()
            .filter(|c| c.market_id == market && c.is_on_shelf)
            .filter_map(|c| {
                let (info, label) = self.trade_product_index(book, c, order_type, now);
                let lowest = info.basic_info.as_ref().map_or(0, |b| b.lowest_price);
                let has = info.extra_info.as_ref().map_or(false, |b| b.has_order);
                if (pr.min_price > 0 || pr.max_price > 0) && !has {
                    return None;
                }
                if (pr.min_price > 0 && lowest < pr.min_price as i64) || (pr.max_price > 0 && lowest > pr.max_price as i64) {
                    return None;
                }
                (label_match(&label, pr.major_index_type, pr.minor_index_type, &pr.tags, &pr.attributes, &pr.exclude_tags) && extra(&label)).then_some(info)
            })
            .collect();
        let sort_key = |i: &tgf::DTradeIndexRecommendIndexInfo| {
            let b = i.extra_info.clone().unwrap_or_default();
            (!b.has_order, b.search_order)
        };
        v.sort_by_key(sort_key);
        if let Some(b) = rule.and_then(|r| r.bound.as_ref()).filter(|b| b.trade_product_id != 0) {
            if let Some(pos) = v.iter().position(|i| i.extra_info.as_ref().map_or(0, |x| x.trade_product_id) == b.trade_product_id) {
                v.drain(0..=pos);
            }
        }
        v
    }

    fn trade_auction_detail(&self, book: &TradeBook, o: &TradeOrder) -> tgf::DTradeAuctionOrderInfo {
        // a bid — show its auction
        let a = if o.kind() == BID { book.orders.iter().find(|x| x.id() == o.auction_id).unwrap_or(o) } else { o };
        tgf::DTradeAuctionOrderInfo {
            order: Some(tgf::DTradeOrderTicketDetail {
                meta: a.meta.clone(),
                item_data: a.escrow.first().cloned().or_else(|| a.item.clone()),
                label: a.label.clone(),
                publisher: Some(user_key(a.owner_user_id)),
                data_version: book.version,
                ..Default::default()
            }),
        }
    }

    // ---------- prices ----------

    pub(super) async fn trade_product_price_basic_info_impl(&self, req: tgf::CsTradeProductPriceBasicInfoReq) -> Option<tgf::ScTradeProductPriceBasicInfoRsp> {
        let book = self.trade_load_book().await;
        let now = Self::now_ts();
        let info = req.trade_product_key.iter().map(|k| self.trade_basic_info(&book, k, req.order_type, now)).collect();
        Some(tgf::ScTradeProductPriceBasicInfoRsp { info, order_type: req.order_type })
    }

    pub(super) async fn trade_product_price_info_impl(&self, req: tgf::CsTradeProductPriceInfoReq) -> Option<tgf::ScTradeProductPriceInfoRsp> {
        let Some(key) = req.trade_product_key.clone() else {
            return Some(tgf::ScTradeProductPriceInfoRsp { info: None, order_type: req.order_type });
        };
        let book = self.trade_load_book().await;
        let now = Self::now_ts();
        let (m, p) = (key.trade_market_id, key.trade_product_id);
        let info = tgf::DTradeMarketProductIndexInfo {
            trade_product_key: Some(key.clone()),
            metric_point: self.trade_metric_points(&book, m, p, now),
            on_sale: Self::trade_on_sale(&book, m, p, now),
            percentage: Self::trade_percentage(&book, m, p, now),
            basic_info: Some(self.trade_basic_info(&book, &key, req.order_type, now)),
        };
        Some(tgf::ScTradeProductPriceInfoRsp { info: Some(info), order_type: req.order_type })
    }

    pub(super) async fn trade_product_percentage_info_impl(&self, req: tgf::CsTradeProductPercentageInfoReq) -> Option<tgf::ScTradeProductPercentageInfoRsp> {
        let Some(key) = req.trade_product_key.clone() else {
            return Some(tgf::ScTradeProductPercentageInfoRsp { info: None });
        };
        let book = self.trade_load_book().await;
        let percentage = Self::trade_percentage(&book, key.trade_market_id, key.trade_product_id, Self::now_ts());
        Some(tgf::ScTradeProductPercentageInfoRsp { info: Some(tgf::DTradeMarketProductPercentageInfo { trade_product_key: Some(key), percentage }) })
    }

    // ---------- listing ----------

    /// Purchase order (homogeneous market). is_fixed — "buy now": only what matches immediately, the remainder is not
    /// listed (nothing matched — EN_ERR_TRADE_MATCHMAKINT_QUICK_NOT_MATCHED). Otherwise the remainder stays as an order,
    /// the gold for it in escrow. Charged: actual cost of the deals + price × remainder.
    pub(super) async fn trade_purchase_order_publish_impl(&self, req: tgf::CsTradePurchaseOrderPublishReq) -> Option<tgf::ScTradePurchaseOrderPublishRsp> {
        let fail = |code: i32| {
            println!("[Trade] purchase_order_publish: rejected {}", code);
            Some(tgf::ScTradePurchaseOrderPublishRsp { result_code: code, is_fixed: req.is_fixed, ..Default::default() })
        };
        let key = req.trade_product_key.clone().unwrap_or_default();
        let (comm, market) = match self.trade_check_product(&key, MARKET_HOMOGENEOUS) {
            Ok(x) => x,
            Err(e) => return fail(e),
        };
        if req.count <= 0 || req.price <= 0 || (req.price as i64) < market.valid_lower_limit {
            return fail(E_PRICE_INVALID);
        }
        if comm.purchase_count_limit > 0 && req.count > comm.purchase_count_limit {
            return fail(E_PURCHASE_COUNT_LIMIT);
        }
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let mut p = self.load_me().await;
        let backup = p.clone();
        let (lo, hi) = self.trade_price_bounds(&book, &key, &comm, &market, now);
        if (req.price as i64) < lo || hi.is_some_and(|h| req.price as i64 > h) {
            println!("[Trade] purchase_order_publish: price {} out of bounds {}..{:?}", req.price, lo, hi);
            return fail(E_PRICE_INVALID);
        }
        let mine = book.orders.iter().filter(|o| o.owner_user_id == p.user_id && o.kind() == PURCHASE && o.live(now)).count();
        if !req.is_fixed && market.po_amount_limit > 0 && mine >= market.po_amount_limit as usize {
            return fail(E_USER_PURCHASE_LIMIT);
        }
        let currency = Self::trade_currency(&market);
        let max_cost = req.price as i64 * req.count as i64;
        if common::coin(&p, currency) < max_cost {
            return fail(E_COIN_NOT_ENOUGH);
        }
        let id = book.alloc();
        let pm = tgf::DTradePurchaseOrderMeta {
            origin_item_count: req.count,
            left_item_count: req.count,
            current_unit_price: req.price,
            left_price: 0,
            is_fixed: req.is_fixed,
            origin_price: req.price,
            user_key: Some(user_key(p.user_id)),
        };
        let end = now + self.trade_valid_secs(&comm, true);
        let mut order = TradeOrder {
            meta: Some(new_meta(id, key.clone(), now, end, Kind::PurchaseOrder(pm))),
            owner_open_id: self.clean_open_id(),
            owner_user_id: p.user_id,
            tax: comm.tax_percentage,
            ..Default::default()
        };
        let (filled, spent) = fill_buy(&mut book, &mut order, req.price as i64, req.count, now, &mut fx, &|it| self.trade_reroll(it));
        if req.is_fixed && filled == 0 {
            // nothing bought: save only the tick results
            if !fx.touched.is_empty() && self.trade_save_book(&mut book).await {
                self.trade_flush(&book, fx, Default::default());
            }
            return fail(E_QUICK_NOT_MATCHED);
        }
        let rest = req.count - filled;
        let standing = if req.is_fixed { 0 } else { rest };
        let charge = spent + req.price as i64 * standing as i64;
        if common::add_coin(&mut p, currency, -charge).is_none() {
            return fail(E_COIN_NOT_ENOUGH);
        }
        if let Some(pm) = order.purchase_mut() {
            pm.left_item_count = standing;
            pm.left_price = (req.price as i64 * standing as i64) as i32;
        }
        order.set_state(if standing > 0 { ST_TRADING } else { ST_CONFIRMATION });
        book.orders.push(order);
        fx.touched.insert(id);
        if !self.trade_commit(&mut book, Some((&p, &backup))).await {
            return fail(E_DB_SAVE_FAILED);
        }
        println!("[Trade] {} purchase #{} product {}: {}×{} — immediately {} pcs for {}, charged {}", p.open_id, id, key.trade_product_id, req.count, req.price, filled, spent, charge);
        self.trade_flush(&book, fx, tgf::ScPlayerDirtyChgSync { dirty_coin_data: vec![common::coin_dirty(&p, currency)], ..Default::default() });
        Some(tgf::ScTradePurchaseOrderPublishRsp {
            trade_order_ticket_id: id,
            result_code: 0,
            matchmaking_info: Some(tgf::DTradeOrderMatchMakingBasicInfo {
                origin_price: req.price,
                origin_count: req.count,
                confirmation_price: spent.min(i32::MAX as i64) as i32, // TODO(verify): total or price per piece
                confirmation_count: filled,
            }),
            is_fixed: req.is_fixed,
        })
    }

    /// Take items `basics` from the player into escrow; returns (items, dirty for the push)
    fn trade_escrow(&self, p: &mut PlayerModel, basics: &[tgf::DItemBasic], item_id: i32) -> Result<(Vec<tgf::DItem>, tgf::ScPlayerDirtyChgSync, bool), i32> {
        if basics.is_empty() {
            return Err(E_SALE_ITEM_EMPTY);
        }
        // crafted items (relics) cannot be sold to players on the market (user decision 07.11)
        if self.is_crafted(item_id) {
            println!("[Trade] {}: product {} is crafted — cannot be listed", p.open_id, item_id);
            return Err(E_SALE_ITEM_INVALID);
        }
        let mut items = vec![];
        let mut dirty = tgf::ScPlayerDirtyChgSync::default();
        let mut from_role = false;
        for b in basics {
            let t = take_for_trade(p, b, item_id)?;
            if self.is_crafted(type_of(&t.item)) {
                println!("[Trade] {}: item {} is crafted — cannot be listed", p.open_id, type_of(&t.item));
                return Err(E_SALE_ITEM_INVALID); // the player is not saved — taken items are not lost
            }
            items.push(t.item);
            dirty.dirty_postion_action.push(t.dirty);
            dirty.dirty_remove_guid.extend(t.removed_guid); // TODO(verify): which of the two fields the client applies
            from_role |= t.from_role;
        }
        Ok((items, dirty, from_role))
    }

    pub(super) async fn trade_sales_order_publish_impl(&self, req: tgf::CsTradeSalesOrderPublishReq) -> Option<tgf::ScTradeSalesOrderPublishRsp> {
        let fail = |code: i32| {
            println!("[Trade] sales_order_publish: rejected {}", code);
            self.set_rpc_error(code);
            Some(tgf::ScTradeSalesOrderPublishRsp { trade_order_ticket_id: 0 })
        };
        let key = req.trade_product_key.clone().unwrap_or_default();
        let (comm, market) = match self.trade_check_product(&key, MARKET_HOMOGENEOUS) {
            Ok(x) => x,
            Err(e) => return fail(e),
        };
        if req.price <= 0 || (req.price as i64) < market.valid_lower_limit {
            return fail(E_PRICE_INVALID);
        }
        // 12.11: price_lower/upper_limit bounds (% of SP) — trade_price_bounds; the trade_valid_price step is not checked
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let (lo, hi) = self.trade_price_bounds(&book, &key, &comm, &market, now);
        if (req.price as i64) < lo || hi.is_some_and(|h| req.price as i64 > h) {
            println!("[Trade] sales_order_publish: price {} out of bounds {}..{:?}", req.price, lo, hi);
            return fail(E_PRICE_INVALID);
        }
        let mut p = self.load_me().await;
        let backup = p.clone();
        let mine = book.orders.iter().filter(|o| o.owner_user_id == p.user_id && o.kind() == SALES && o.live(now)).count();
        if market.so_amount_limit > 0 && mine >= market.so_amount_limit as usize {
            return fail(E_USER_SALE_LIMIT);
        }
        let (items, dirty, from_role) = match self.trade_escrow(&mut p, &req.item_basics, comm.item_id) {
            Ok(x) => x,
            Err(e) => return fail(e),
        };
        let total: i32 = items.iter().map(count_of).sum();
        let id = book.alloc();
        let sm = tgf::DTradeSalesOrderMeta {
            origin_item_count: total,
            left_item_count: total,
            unit_price: req.price,
            unit_count: 1,
            user_key: Some(user_key(p.user_id)),
        };
        let end = now + self.trade_valid_secs(&comm, false);
        let mut order = TradeOrder {
            meta: Some(new_meta(id, key.clone(), now, end, Kind::SalesOrder(sm))),
            owner_open_id: self.clean_open_id(),
            owner_user_id: p.user_id,
            escrow: items,
            tax: comm.tax_percentage,
            ..Default::default()
        };
        let sold = fill_sell(&mut book, &mut order, req.price as i64, now, &mut fx, &|it| self.trade_reroll(it));
        book.orders.push(order);
        fx.touched.insert(id);
        if !self.trade_commit(&mut book, Some((&p, &backup))).await {
            return fail(E_DB_SAVE_FAILED);
        }
        if from_role {
            Self::write_client_loadout_file(&p);
        }
        println!("[Trade] {} sale #{} product {}: {} pcs at {}, sold immediately {}", p.open_id, id, key.trade_product_id, total, req.price, sold);
        self.trade_flush(&book, fx, dirty);
        Some(tgf::ScTradeSalesOrderPublishRsp { trade_order_ticket_id: id })
    }

    pub(super) async fn trade_bid_order_publish_impl(&self, req: tgf::CsTradeBidOrderPublishReq) -> Option<tgf::ScTradeBidOrderPublishRsp> {
        let fail = |code: i32| {
            println!("[Trade] bid_order_publish: rejected {}", code);
            self.set_rpc_error(code);
            Some(tgf::ScTradeBidOrderPublishRsp::default())
        };
        let ak = req.auction_ticket_key.clone().unwrap_or_default();
        let market_id = ak.trade_product_key.as_ref().map_or(0, |k| k.trade_market_id);
        let market = self.game_data.trade_market.get(&market_id).cloned().unwrap_or_default();
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let mut p = self.load_me().await;
        let backup = p.clone();
        // currency — the lot's market (if the key has no market, take the market of the order itself)
        let market = match book.idx(ak.trade_order_ticket_id).map(|i| book.orders[i].key().0) {
            Some(m) if m != market_id => self.game_data.trade_market.get(&m).cloned().unwrap_or(market),
            _ => market,
        };
        let currency = Self::trade_currency(&market);
        if common::coin(&p, currency) < req.price as i64 {
            return fail(E_COIN_NOT_ENOUGH);
        }
        let open_id = self.clean_open_id();
        let (bid_id, charged) = match place_bid(&mut book, ak.trade_order_ticket_id, (p.user_id, &open_id), req.price, market.bid_price_percentage, now, &mut fx) {
            Ok(x) => x,
            Err(e) => {
                if !fx.touched.is_empty() && self.trade_save_book(&mut book).await {
                    self.trade_flush(&book, fx, Default::default());
                }
                return fail(e);
            }
        };
        if common::add_coin(&mut p, currency, -(charged as i64)).is_none() {
            return fail(E_COIN_NOT_ENOUGH);
        }
        if !self.trade_commit(&mut book, Some((&p, &backup))).await {
            return fail(E_DB_SAVE_FAILED);
        }
        println!("[Trade] {} bid #{} on auction #{}: {}", p.open_id, bid_id, ak.trade_order_ticket_id, charged);
        let auction_data = book.idx(ak.trade_order_ticket_id).and_then(|i| book.orders[i].meta.clone());
        self.trade_flush(&book, fx, tgf::ScPlayerDirtyChgSync { dirty_coin_data: vec![common::coin_dirty(&p, currency)], ..Default::default() });
        Some(tgf::ScTradeBidOrderPublishRsp { trade_order_ticket_id: bid_id, auction_data })
    }

    /// Auction lot: one item, starting price, buyout (0 — none). Listing fee — the request's publish_cost
    /// (coins), otherwise trade_commodity.publish_cost in the publish_cost_currency_type currency (0 → gold).
    pub(super) async fn trade_auction_order_publish_impl(&self, req: tgf::CsTradeAuctionOrderPublishReq) -> Option<tgf::ScTradeAuctionOrderPublishRsp> {
        let fail = |code: i32| {
            println!("[Trade] auction_order_publish: rejected {}", code);
            self.set_rpc_error(code);
            Some(tgf::ScTradeAuctionOrderPublishRsp { trade_order_ticket_id: 0 })
        };
        let key = req.trade_product_key.clone().unwrap_or_default();
        let (comm, market) = match self.trade_check_product(&key, MARKET_AUCTION) {
            Ok(x) => x,
            Err(e) => return fail(e),
        };
        if req.price <= 0 || (req.price as i64) < market.valid_lower_limit || (req.fixed_price != 0 && req.fixed_price < req.price) {
            return fail(E_PRICE_INVALID);
        }
        if req.item_basics.len() != 1 {
            return fail(E_SALE_ITEM_INVALID);
        }
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let mut p = self.load_me().await;
        let backup = p.clone();
        let (lo, hi) = self.trade_price_bounds(&book, &key, &comm, &market, now);
        let fixed = req.fixed_price as i64;
        if (req.price as i64) < lo || hi.is_some_and(|h| req.price as i64 > h) || (fixed != 0 && hi.is_some_and(|h| fixed > h)) {
            println!("[Trade] auction_order_publish: start {} / buyout {} out of bounds {}..{:?}", req.price, fixed, lo, hi);
            return fail(E_AUCTION_PRICE_LIMIT);
        }
        let mine = book.orders.iter().filter(|o| o.owner_user_id == p.user_id && o.kind() == AUCTION && o.live(now)).count();
        if market.so_amount_limit > 0 && mine >= market.so_amount_limit as usize {
            return fail(E_USER_SALE_LIMIT);
        }
        // listing fee
        let mut costs: Vec<(i32, i64)> = req
            .publish_cost
            .iter()
            .filter(|b| b.count > 0 && self.trade_is_coin(b.type_id))
            .map(|b| (b.type_id, b.count as i64))
            .collect();
        if costs.is_empty() && comm.publish_cost > 0 {
            let cur = if market.publish_cost_currency_type != 0 { market.publish_cost_currency_type } else { GOLD };
            costs.push((cur, comm.publish_cost as i64));
        }
        for (t, n) in &costs {
            if common::add_coin(&mut p, *t, -*n).is_none() {
                return fail(E_COIN_NOT_ENOUGH);
            }
        }
        let (items, mut dirty, from_role) = match self.trade_escrow(&mut p, &req.item_basics, comm.item_id) {
            Ok(x) => x,
            Err(e) => return fail(e),
        };
        let item = items[0].clone();
        let id = book.alloc();
        let am = tgf::DTradeAuctionOrderMeta {
            auction_trade_order_ticket_id: id,
            user_key: Some(user_key(p.user_id)),
            origin_price: req.price,
            current_price: req.price,
            fixed_price: req.fixed_price,
        };
        let label = self.trade_label(type_of(&item), Some(&item));
        let end = now + self.trade_valid_secs(&comm, false);
        let mut meta = new_meta(id, key.clone(), now, end, Kind::AuctionOrder(tgf::DTradeAuctionMeta { auction_order_meta: Some(am), bid_order_meta: None }));
        meta.label_cache_rarity = rarity_of(&label);
        let order = TradeOrder {
            meta: Some(meta),
            owner_open_id: self.clean_open_id(),
            owner_user_id: p.user_id,
            escrow: items,
            confirm: Some(tgf::DUserTradeConfirmData {
                trade_order_ticket_id: id,
                publish_cost_item: costs.first().map(|(t, n)| tgf::DItem {
                    basic: Some(tgf::DItemBasic { type_id: *t, count: *n as i32, ..Default::default() }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            label: Some(label),
            tax: comm.tax_percentage,
            item: Some(item),
            ..Default::default()
        };
        book.orders.push(order);
        fx.touched.insert(id);
        if !self.trade_commit(&mut book, Some((&p, &backup))).await {
            return fail(E_DB_SAVE_FAILED);
        }
        if from_role {
            Self::write_client_loadout_file(&p);
        }
        println!("[Trade] {} auction #{} product {}: start {}, buyout {}", p.open_id, id, key.trade_product_id, req.price, req.fixed_price);
        let mut coins: Vec<i32> = costs.iter().map(|c| c.0).collect();
        coins.dedup();
        dirty.dirty_coin_data = coins.into_iter().map(|t| common::coin_dirty(&p, t)).collect();
        self.trade_flush(&book, fx, dirty);
        Some(tgf::ScTradeAuctionOrderPublishRsp { trade_order_ticket_id: id })
    }

    /// 12.11 (user decision): on Buyout Trades (market 2, buy/sell orders) gear affixes are rolled
    /// anew at the moment of the deal — the buyer gets not the item the seller listed, but a new roll of the same item
    /// (like a new item: inv_gear_affixes, season rules). Not gear — unchanged. The auction (market 1) is not touched:
    /// affixes are visible there before buying. Seed — time and item guid (several items of one deal — different rolls).
    fn trade_reroll(&self, it: &mut tgf::DItem) {
        let (t, guid) = it.basic.as_ref().map_or((0, 0), |b| (b.type_id, b.guid));
        if !self.inv_is_gear(t) {
            return;
        }
        let weapon = self.game_data.item_type.get(&t).is_some_and(|x| x.item_minor_type == tgf::EnItemMinorType::EquipmentWeapon as i32);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(1);
        let seed = nanos ^ (guid as u64).rotate_left(29) ^ (t as u64).rotate_left(23);
        let affixes = self.build_item_affix_data_seeded(t, if weapon { 1 } else { 0 }, seed);
        if !affixes.is_empty() {
            let d = it.data.get_or_insert_with(Default::default);
            let wp = if weapon { 1 } else { 0 };
            d.affix_data = keep_gear_sets(&d.affix_data, affixes, &|gid| Self::affix_roll(t, wp, (gid as u64) ^ seed));
        }
    }

    /// System sellers: the market is not empty while there are few players. Flags are independent (docs/WORKAROUNDS.md K22):
    /// `TRADE_SYSTEM_BUYOUT=1` — Buyout Trades (market 1): stock of unidentified items at `price_def`, bought with a regular order;
    /// `TRADE_SYSTEM_AUCTION=1` — auction (market 2): lots with buyout at `price_def`.
    /// Stock is topped up on every pass. With the flag off its lots without bids are removed. Returns how many were listed.
    pub async fn trade_seed_system_lots(&self) -> usize {
        let (buyout_on, auction_on) = (system_buyout(), system_auction());
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let before = book.orders.len();
        book.orders.retain(|o| {
            if o.owner_user_id != SYSTEM_USER {
                return true;
            }
            // system sellers do not list crafted items (relics) (user decision 07.11); lots without bids are removed
            let crafted = o.item.as_ref().or(o.escrow.first()).is_some_and(|i| self.is_crafted(type_of(i)));
            // products taken off the shelf (season playtest 11.11: items without their own effect, ice staves) — their lots are removed too
            let (market, product) = o.key();
            let shelved = !self.trade_commodity_of(market, product).is_some_and(|c| c.is_on_shelf);
            let crafted = crafted || shelved;
            let off = (o.kind() == AUCTION && (!auction_on || crafted) && o.top_bid().is_none()) || (o.kind() == SALES && (!buyout_on || crafted));
            !off && (o.live(now) || o.end_ts() > now - 86400)
        });
        let removed = before - book.orders.len();
        // the lot label is stored from listing time: after a relabeling the search would see the old one
        let mut relabeled = 0;
        for o in book.orders.iter_mut().filter(|o| o.owner_user_id == SYSTEM_USER && o.kind() == AUCTION) {
            let Some(item) = o.item.as_ref() else { continue };
            let label = self.trade_label(type_of(item), Some(item));
            if o.label.as_ref() != Some(&label) {
                o.label = Some(label);
                relabeled += 1;
            }
        }
        let mut tmp = PlayerModel::new(SYSTEM_OPEN_ID, SYSTEM_USER);
        let mut added = 0;
        for ((market_id, product), comm) in commodities() {
            let Some(market) = self.game_data.trade_market.get(market_id) else { continue };
            if !comm.is_on_shelf || comm.price_def <= 0 || self.is_crafted(comm.item_id) {
                continue;
            }
            let kind = match market.market_type {
                MARKET_AUCTION if auction_on => AUCTION,
                MARKET_HOMOGENEOUS if buyout_on => SALES,
                _ => continue,
            };
            let have = book
                .orders
                .iter()
                .filter(|o| o.owner_user_id == SYSTEM_USER && o.kind() == kind && o.live(now) && o.key() == (*market_id, *product))
                .count();
            let key = tgf::DTradeMarketProductKey { trade_market_id: *market_id, trade_product_id: *product, ..Default::default() };
            let end = now + self.trade_valid_secs(comm, false);
            if kind == SALES {
                if have > 0 {
                    continue;
                }
                let stock = system_stock();
                let escrow: Vec<tgf::DItem> = (0..stock).map(|_| self.inv_new_item(&mut tmp, comm.item_id, 1)).collect();
                let id = book.alloc();
                let sm = tgf::DTradeSalesOrderMeta {
                    origin_item_count: stock,
                    left_item_count: stock,
                    unit_price: comm.price_def,
                    unit_count: 1,
                    user_key: Some(user_key(SYSTEM_USER)),
                };
                book.orders.push(TradeOrder {
                    meta: Some(new_meta(id, key, now, end, Kind::SalesOrder(sm))),
                    owner_open_id: SYSTEM_OPEN_ID.to_string(),
                    owner_user_id: SYSTEM_USER,
                    escrow,
                    tax: 0,
                    ..Default::default()
                });
                added += 1;
                continue;
            }
            for _ in have..system_lots() {
                let item = self.inv_new_item(&mut tmp, comm.item_id, 1);
                let id = book.alloc();
                let am = tgf::DTradeAuctionOrderMeta {
                    auction_trade_order_ticket_id: id,
                    user_key: Some(user_key(SYSTEM_USER)),
                    origin_price: comm.price_def,
                    current_price: comm.price_def,
                    fixed_price: comm.price_def,
                };
                let label = self.trade_label(comm.item_id, Some(&item));
                let mut meta = new_meta(id, key.clone(), now, end, Kind::AuctionOrder(tgf::DTradeAuctionMeta { auction_order_meta: Some(am), bid_order_meta: None }));
                meta.label_cache_rarity = rarity_of(&label);
                book.orders.push(TradeOrder {
                    meta: Some(meta),
                    owner_open_id: SYSTEM_OPEN_ID.to_string(),
                    owner_user_id: SYSTEM_USER,
                    escrow: vec![item.clone()],
                    confirm: Some(tgf::DUserTradeConfirmData { trade_order_ticket_id: id, ..Default::default() }),
                    label: Some(label),
                    tax: 0,
                    item: Some(item),
                    ..Default::default()
                });
                added += 1;
            }
        }
        if added > 0 || removed > 0 || relabeled > 0 || !fx.touched.is_empty() {
            if self.trade_save_book(&mut book).await {
                println!(
                    "[Trade] system sellers (buyout {}, auction {}): listed {}, removed {}, new labels {}, orders in book {}, {} KB",
                    buyout_on, auction_on, added, removed, relabeled, book.orders.len(), book.encoded_len() / 1024
                );
            }
            self.trade_flush(&book, fx, Default::default());
        }
        added
    }

    // ---------- auction: browsing ----------

    /// Lot cards by keys. An empty request (the client sends it on login) — an empty response, as on the official server.
    pub(super) async fn trade_auction_fetch_order_info_impl(&self, req: tgf::CsTradeAuctionFetchOrderInfoReq) -> Option<tgf::ScTradeAuctionFetchOrderInfoRsp> {
        if req.auction_ticket_key.is_empty() {
            return Some(Default::default());
        }
        let book = self.trade_load_book().await;
        let auction_order_info = req
            .auction_ticket_key
            .iter()
            .filter_map(|k| book.orders.iter().find(|o| o.id() == k.trade_order_ticket_id))
            .filter(|o| o.kind() == AUCTION || o.kind() == BID)
            .map(|o| self.trade_auction_detail(&book, o))
            .collect();
        Some(tgf::ScTradeAuctionFetchOrderInfoRsp { auction_order_info })
    }

    pub(super) async fn trade_auction_index_impl(&self, req: tgf::CsTradeAuctionIndexReq) -> Option<tgf::ScTradeAuctionIndexRsp> {
        let book = self.trade_load_book().await;
        let rule = req.search_rule.clone().unwrap_or_default();
        let found = auction_search(&book, &rule, req.sort_by, Self::now_ts());
        let (orders, next_page) = page_of(found, 0);
        Some(tgf::ScTradeAuctionIndexRsp { orders, next_page })
    }

    // ---------- my orders ----------

    /// Cancel: sale/purchase/auction without bids → everything from the escrow as a refund (claimed via receive)
    pub(super) async fn trade_order_cancel_impl(&self, req: tgf::CsTradeOrderCancelReq) -> Option<tgf::ScTradeOrderCancelRsp> {
        let _g = trade_lock().lock().await;
        let now = Self::now_ts();
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let me = self.load_me().await.user_id;
        let res = match book.idx(req.trade_order_ticket_id) {
            None => Err(E_TICKET_UNAVAILABLE),
            Some(i) if book.orders[i].owner_user_id != me => Err(E_TICKET_UNAVAILABLE),
            Some(i) if !book.orders[i].live(now) => Err(E_TICKET_FINISHED),
            Some(i) => match book.orders[i].kind() {
                SALES | PURCHASE => Ok(i),
                AUCTION if book.orders[i].top_bid().is_some() => Err(E_CANCEL_WITH_BIDDER),
                AUCTION => Ok(i),
                _ => Err(E_UNABLE_TO_CANCEL),
            },
        };
        match res {
            Ok(i) => close_refund(&mut book, i, ST_CANCEL, now, &mut fx),
            Err(e) => {
                println!("[Trade] order_cancel #{}: rejected {}", req.trade_order_ticket_id, e);
                self.set_rpc_error(e);
            }
        }
        if !fx.touched.is_empty() && self.trade_save_book(&mut book).await {
            self.trade_flush(&book, fx, Default::default());
        }
        Some(tgf::ScTradeOrderCancelRsp {})
    }

    /// My market orders (including bids) with what is waiting to be received
    pub(super) async fn trade_order_history_impl(&self, req: tgf::CsTradeOrderHistoryReq) -> Option<tgf::ScTradeOrderHistoryRsp> {
        let _g = trade_lock().lock().await;
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        if !fx.touched.is_empty() && self.trade_save_book(&mut book).await {
            self.trade_flush(&book, fx, Default::default());
        }
        let me = self.load_me().await.user_id;
        let meta_confirmation = book
            .orders
            .iter()
            .filter(|o| o.owner_user_id == me && (req.market_id == 0 || o.key().0 == req.market_id))
            .map(|o| tgf::DTradeMetaConfirmation {
                meta: o.meta.clone(),
                confirmation: Some(o.confirm.clone().unwrap_or(tgf::DUserTradeConfirmData { trade_order_ticket_id: o.id(), ..Default::default() })),
            })
            .collect();
        Some(tgf::ScTradeOrderHistoryRsp { meta_confirmation })
    }

    /// Remove finished orders from history (if everything is received)
    pub(super) async fn trade_order_delete_impl(&self, req: tgf::CsTradeOrderDeleteReq) -> Option<tgf::ScTradeOrderDeleteRsp> {
        let _g = trade_lock().lock().await;
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let me = self.load_me().await.user_id;
        let my = self.clean_open_id();
        for id in &req.trade_order_ticket_id {
            match book.idx(*id) {
                Some(i) if book.orders[i].owner_user_id == me && book.orders[i].state() != ST_TRADING && !book.orders[i].has_pending() => {
                    book.orders.remove(i);
                    fx.touched.remove(id);
                    fx.deleted.push((my.clone(), *id));
                }
                Some(i) if book.orders[i].owner_user_id == me => {
                    println!("[Trade] order_delete #{}: rejected {}", id, E_ORDER_NOT_FINISH);
                    self.set_rpc_error(E_ORDER_NOT_FINISH);
                }
                _ => {
                    println!("[Trade] order_delete #{}: rejected {}", id, E_TICKET_UNAVAILABLE);
                    self.set_rpc_error(E_TICKET_UNAVAILABLE);
                }
            }
        }
        if (!fx.touched.is_empty() || !fx.deleted.is_empty()) && self.trade_save_book(&mut book).await {
            self.trade_flush(&book, fx, Default::default());
        }
        Some(tgf::ScTradeOrderDeleteRsp {})
    }

    /// Claim goods/proceeds/refund: confirmation_id — a pending_receive key (or an order id — then everything for it).
    /// Coins — to the balance, items — to the stash (goal_position if the cells are free, otherwise the first free place).
    /// Did not fit — this entry keeps waiting.
    pub(super) async fn trade_receive_confirmation_product_impl(&self, req: tgf::CsTradeReceiveConfirmationProductReq) -> Option<tgf::ScTradeReceiveConfirmationProductRsp> {
        let _g = trade_lock().lock().await;
        let mut fx = Fx::default();
        let mut book = self.trade_book_ticked(&mut fx).await;
        let mut p = self.load_me().await;
        let backup = p.clone();
        let my = self.clean_open_id();
        let mut coins: BTreeSet<i32> = BTreeSet::new();
        let mut dirty = tgf::ScPlayerDirtyChgSync::default();
        let mut quests = false;
        for r in &req.receive_confirmation_product {
            // (order, keys)
            let mut targets: Vec<(usize, i64)> = vec![];
            for (i, o) in book.orders.iter().enumerate().filter(|(_, o)| o.owner_user_id == p.user_id) {
                let Some(c) = o.confirm.as_ref() else { continue };
                if c.pending_receive.contains_key(&r.confirmation_id) {
                    targets = vec![(i, r.confirmation_id)];
                    break;
                }
                if o.id() == r.confirmation_id {
                    targets = c.pending_receive.keys().map(|k| (i, *k)).collect();
                }
            }
            if targets.is_empty() {
                println!("[Trade] receive #{}: rejected {}", r.confirmation_id, E_CONFIRMATION_NOT_FOUND);
                self.set_rpc_error(E_CONFIRMATION_NOT_FOUND);
            }
            for (i, k) in targets {
                let Some(pend) = book.orders[i].confirm.as_ref().and_then(|c| c.pending_receive.get(&k)).cloned() else { continue };
                let snapshot = (p.clone(), dirty.dirty_postion_action.len(), coins.clone(), quests);
                let mut ok = true;
                let mut goal = r.goal_position.as_ref();
                for mut it in pend_items(&pend) {
                    let t = type_of(&it);
                    if self.trade_is_coin(t) {
                        ok &= common::add_coin(&mut p, t, count_of(&it) as i64).is_some();
                        coins.insert(t);
                        continue;
                    }
                    let Some(pos) = self.trade_stash_place(&p, t, goal.take()) else {
                        println!("[Trade] receive #{}: no stash space for {}", k, t);
                        ok = false;
                        break;
                    };
                    let b = it.basic.get_or_insert_with(Default::default);
                    b.guid = crate::guid::next_item();
                    b.position = Some(pos);
                    // quests (Q1): "buy N items on the trading post" (ignore_entity_args — coins)
                    let mut ev = QuestEvent::new(tgf::EnQuestProgressType::TradeBuyItem, count_of(&it) as i64, p.hall_role_guid);
                    ev.item_type = Some(t);
                    quests |= self.quest_event(&mut p, &ev);
                    p.stash.element.push(it.clone());
                    dirty.dirty_postion_action.push(it);
                }
                if !ok {
                    p = snapshot.0;
                    dirty.dirty_postion_action.truncate(snapshot.1);
                    coins = snapshot.2;
                    quests = snapshot.3;
                    continue;
                }
                let o = &mut book.orders[i];
                if let Some(c) = o.confirm.as_mut() {
                    c.pending_receive.remove(&k);
                    c.receive_all_confirmation_key.push(k);
                }
                fx.touched.insert(o.id());
                fx.removed_conf.push((my.clone(), k));
            }
        }
        if fx.removed_conf.is_empty() {
            if !fx.touched.is_empty() && self.trade_save_book(&mut book).await {
                self.trade_flush(&book, fx, Default::default());
            }
            return Some(tgf::ScTradeReceiveConfirmationProductRsp {});
        }
        if !self.trade_commit(&mut book, Some((&p, &backup))).await {
            println!("[Trade] receive: rejected {}", E_DB_SAVE_FAILED);
            self.set_rpc_error(E_DB_SAVE_FAILED);
            return Some(tgf::ScTradeReceiveConfirmationProductRsp {});
        }
        println!("[Trade] {} entries received: {}", p.open_id, fx.removed_conf.len());
        dirty.dirty_coin_data = coins.into_iter().map(|t| common::coin_dirty(&p, t)).collect();
        if quests {
            dirty.dirty_quest_snapshot = Some(self.progress_quests_snapshot(&p));
        }
        self.trade_flush(&book, fx, dirty);
        Some(tgf::ScTradeReceiveConfirmationProductRsp {})
    }

    // ---------- recommendations and search ----------

    /// Auction selections. sp_score — expensive, publish_new and best_selling_* — new (there are no demand stats per lot),
    /// batch — by its own rules with echo of sequence/private_data. TODO(verify): rarity_wearing_part — no
    /// "body part ↔ item" table, empty groups are returned.
    pub(super) async fn trade_auction_recommend_order_ticket_impl(&self, req: tgf::CsTradeAuctionRecommendOrderTicketReq) -> Option<tgf::ScTradeAuctionRecommendOrderTicketRsp> {
        let book = self.trade_load_book().await;
        let now = Self::now_ts();
        let market = req.trade_market_id;
        let basic = |r: &Option<tgf::DTradeRecommendOrderTicketBasicRule>, sort_by: i32| {
            let r = r.as_ref()?;
            let mut rule = r.filter_rule.clone().unwrap_or_default();
            if rule.trade_market_id == 0 {
                rule.trade_market_id = market;
            }
            let mut found = auction_search(&book, &rule, sort_by, now);
            if sort_by == 2 {
                found.sort_by_key(|o| -o.price());
            }
            let (order_tickets, has_more) = page_of(found, r.count);
            Some(tgf::DTradeRecommendOrderTicketBasicResult { order_tickets, has_more })
        };
        // 09.11: main page selections "by slot and rarity" — lots with this slot (label 1004) in the rarity range
        let rarity_wearing_part_result = req
            .rarity_wearing_part_rule
            .iter()
            .flat_map(|r| {
                let parts = if r.wearing_parts.is_empty() { vec![0] } else { r.wearing_parts.clone() };
                let mut rule = r.filter_rule.clone().unwrap_or_default();
                if rule.trade_market_id == 0 {
                    rule.trade_market_id = market;
                }
                let base = auction_search(&book, &rule, 0, now);
                parts
                    .into_iter()
                    .map(|wp| {
                        let found: Vec<&TradeOrder> = base
                            .iter()
                            .copied()
                            .filter(|o| {
                                let l = o.label.clone().unwrap_or_default();
                                let rar = rarity_of(&l);
                                rar >= r.min_rarity
                                    && (r.max_rarity <= 0 || rar <= r.max_rarity)
                                    && (wp == 0 || l.tags.iter().any(|t| t.r#type == TAG_WEARING_PART && t.values.contains(&wp)))
                            })
                            .collect();
                        let (order_tickets, has_more) = page_of(found, r.count);
                        tgf::DTradeRecommendOrderTicketRarityWearingPartResult {
                            min_rarity: r.min_rarity,
                            max_rarity: r.max_rarity,
                            wearing_part: wp,
                            order_tickets,
                            has_more,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        let batch_result = req
            .batch_rule
            .iter()
            .map(|b| tgf::DTradeRecommendOrderTicketBatchResult { result: basic(&b.rule, 0), sequence: b.sequence, private_data: b.private_data.clone() })
            .collect();
        let result = tgf::DTradeRecommendOrderTicketResult {
            area_code: 0,
            order_ticket_type: AUCTION,
            trade_market_id: market,
            sp_score_result: basic(&req.sp_score_rule, 2),
            publish_new_result: basic(&req.publish_new_rule, 0),
            best_selling_amount_result: basic(&req.best_selling_amount_rule, 0),
            best_selling_price_increase_result: basic(&req.best_selling_price_increase_rule, 0),
            rarity_wearing_part_result,
            batch_result,
        };
        Some(tgf::ScTradeAuctionRecommendOrderTicketRsp { result: Some(result) })
    }

    /// Homogeneous market selections: hot_sale — products with orders by weekly deal volume; by weapon subtype and
    /// rarity — via gear_base_item. TODO(verify): rarity_wearing_part — no "body part ↔ item" table.
    pub(super) async fn trade_product_recommend_order_ticket_impl(&self, req: tgf::CsTradeProductRecommendOrderTicketReq) -> Option<tgf::ScTradeProductRecommendOrderTicketRsp> {
        let book = self.trade_load_book().await;
        let now = Self::now_ts();
        let (market, ot) = (req.trade_market_id, if req.order_type != 0 { req.order_type } else { SALES });
        let take = |v: Vec<tgf::DTradeIndexRecommendIndexInfo>, count: i32| {
            let n = if count > 0 { count as usize } else { PAGE };
            let more = v.len() > n;
            (v.into_iter().take(n).collect::<Vec<_>>(), more)
        };
        let mut result = tgf::DTradeRecommendProductResult { trade_market_id: market, ..Default::default() };
        if let Some(r) = req.hot_sale_rule.as_ref() {
            let mut v: Vec<_> = self
                .trade_product_search(&book, market, ot, r.filter_rule.as_ref(), |_| true, now)
                .into_iter()
                .filter(|i| i.extra_info.as_ref().map_or(false, |b| b.has_order))
                .collect();
            v.sort_by_key(|i| -i.extra_info.as_ref().map_or(0, |b| b.sale_num));
            let (v, more) = take(v, r.count);
            result.hot_sale_result = v.into_iter().map(|i| tgf::DTradeIndexRecommendHotSaleResult { info: Some(i) }).collect();
            result.hot_sale_more_info = more;
        }
        for r in req.rarity_wearing_part_rule.iter().chain(req.batch_rarity_wearing_part_rule.iter()) {
            let parts = if r.wearing_type.is_empty() { vec![0] } else { r.wearing_type.clone() };
            for wt in parts {
                result.rarity_wearing_part_result.push(tgf::DTradeIndexRecommendRarityWearingTypeResult {
                    info: vec![],
                    min_rarity: r.min_rarity,
                    max_rarity: r.max_rarity,
                    wearing_type: wt,
                    more_info: false,
                });
            }
        }
        for r in &req.batch_rarity_gear_sub_type_rule {
            let subs = if r.gear_sub_type.is_empty() { vec![0] } else { r.gear_sub_type.clone() };
            for st in subs {
                let fit = |l: &tgf::DTradeOrderTicketLabel| {
                    let rar = rarity_of(l);
                    rar >= r.min_rarity
                        && (r.max_rarity <= 0 || rar <= r.max_rarity)
                        && (st == 0 || l.tags.iter().any(|t| t.r#type == TAG_GEAR_SUB_TYPE && t.values.contains(&st)))
                };
                let (info, more_info) = take(self.trade_product_search(&book, market, ot, r.filter_rule.as_ref(), fit, now), r.count);
                result.rarity_gear_sub_type_result.push(tgf::DTradeIndexRecommendRarityGearSubTypeResult {
                    info,
                    min_rarity: r.min_rarity,
                    max_rarity: r.max_rarity,
                    gear_sub_type: st,
                    more_info,
                });
            }
        }
        Some(tgf::ScTradeProductRecommendOrderTicketRsp { result: Some(result) })
    }

    /// Homogeneous market product search: all products on the shelf matching the rule (those with orders first), page count
    pub(super) async fn trade_product_search_order_ticket_impl(&self, req: tgf::CsTradeProductSearchOrderTicketReq) -> Option<tgf::ScTradeProductSearchOrderTicketRsp> {
        let book = self.trade_load_book().await;
        let now = Self::now_ts();
        let (market, ot) = (req.trade_market_id, if req.order_type != 0 { req.order_type } else { SALES });
        let rule = req.custom_search_rule.clone().unwrap_or_default();
        let found = self.trade_product_search(&book, market, ot, rule.filter_rule.as_ref(), |_| true, now);
        let n = if rule.count > 0 { rule.count as usize } else { PAGE };
        let more = found.len() > n;
        let result = tgf::DTradeRecommendProductResult {
            trade_market_id: market,
            custom_search_result: found.into_iter().take(n).map(|i| tgf::DTradeIndexCustomSearchResult { info: Some(i) }).collect(),
            custom_search_more_info: more,
            ..Default::default()
        };
        Some(tgf::ScTradeProductSearchOrderTicketRsp { result: Some(result) })
    }

    /// Set anchor prices by profession/rarity. There are no set stats — groups without prices. TODO(verify)
    pub(super) async fn trade_career_rarity_affix_gear_set_price_impl(&self, req: tgf::CsTradeCareerRarityAffixGearsetPriceReq) -> Option<tgf::ScTradeCareerRarityAffixGearsetPriceRsp> {
        let result = tgf::DTradeCareerRarityAffixGearsetPriceResult {
            area_code: 0,
            trade_market_id: req.trade_market_id,
            career_rarity_affix_gear_set_anchor_price_result: req
                .batch_career_rarity_price_rule
                .iter()
                .map(|r| tgf::DTradeCareerRarityAffixGearSetAnchorPriceResult { career: r.career, rarity: r.rarity, affix_gear_set_price_result: vec![] })
                .collect(),
        };
        Some(tgf::ScTradeCareerRarityAffixGearsetPriceRsp { result: Some(result) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn price_bounds_from_commodity_limits() {
        // green, price_def 900: Buyout 50–200% → 450..1800; auction from 10%, upper 500% → 90..4500
        assert_eq!(price_bounds(None, 900, 50, 200, 1), (450, Some(1800)));
        assert_eq!(price_bounds(None, 900, 10, 500, 1), (90, Some(4500)));
        // SP from deals is higher — bounds from it; below the floor (def × lower) — the floor
        assert_eq!(price_bounds(Some(2000), 900, 50, 200, 1), (1000, Some(4000)));
        assert_eq!(price_bounds(Some(10), 900, 50, 200, 1), (225, Some(900)), "SP not below 450, min = 50% of 450");
        // without bounds — only valid_lower_limit; the minimum is rounded up
        assert_eq!(price_bounds(None, 900, 0, 0, 1), (1, None));
        assert_eq!(price_bounds(None, 3, 50, 200, 1), (2, Some(6)));
    }

    /// 13.11: old prices out of bounds (a deal or order for 1 gold) do not get into market responses
    #[tokio::test]
    async fn old_prices_outside_bounds_are_ignored() {
        let dir = format!("target/test_trade_ok_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "ok".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        // green sword 181012000, product 1810120000: price_def 900, bounds 50–400% -> 450..3600
        let c = s.trade_commodity_of(1, 1810120000).expect("product in the table");
        let (lo, hi) = (c.price_def as i64 * c.price_lower_limit as i64 / 100, c.price_def as i64 * c.price_upper_limit as i64 / 100);
        let ok = s.trade_price_ok(1, 1810120000);
        assert!(!ok(1) && !ok(2), "a deal for 1–2 gold under the old rules");
        assert!(ok(lo) && ok(c.price_def as i64) && ok(hi));
        assert!(!ok(hi + 1));
        assert!(s.trade_price_ok(1, 999_999_999)(1), "no product — no check");
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn key(m: i32, p: i32) -> tgf::DTradeMarketProductKey {
        tgf::DTradeMarketProductKey { area_code: 0, trade_market_id: m, trade_product_id: p }
    }

    /// 18.11 (owner's item 5): lots on Buyout and the auction live 7 days — trade_commodity.order_valid_time = 1
    /// (trade_valid_time row 1: so 604800 s); buy orders stay 1 hour (po 3600 s). The client shows the same row of its pak
    /// (TradeUtils GetTradeValidTime). Before: Buyout 1 day (row 2), auction 12 hours (row 5)
    #[tokio::test]
    async fn trade_lots_live_seven_days() {
        let dir = format!("target/test_trade_7d_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "t7d".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let comms = commodities();
        assert!(comms.len() > 300, "commodities: {}", comms.len());
        for c in comms.values() {
            assert_eq!(s.trade_valid_secs(c, false), WEEK, "lot {} market {}: 7 days", c.id, c.market_id);
            assert_eq!(s.trade_valid_secs(c, true), 3600, "buy order {} market {}: 1 hour", c.id, c.market_id);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 07.11: a player cannot list crafted items (relics) either on Buyout or on the auction (shared trade_escrow)
    #[tokio::test]
    async fn crafted_items_cannot_be_listed() {
        let dir = format!("target/test_craft_trade_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "craft".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let mut p = PlayerModel::new("craft", 19_100_001);
        p.stash.element.push(item(501, 220110180, 1)); // pink relic sword
        p.stash.element.push(item(502, 181040000, 1)); // regular weapon
        let b = |g: i64| vec![tgf::DItemBasic { guid: g, count: 1, ..Default::default() }];
        assert_eq!(s.trade_escrow(&mut p, &b(501), 220110180).err(), Some(E_SALE_ITEM_INVALID));
        // the product is not crafted, but a relic is slipped into the order — also rejected (the type of the taken item is checked)
        assert_eq!(s.trade_escrow(&mut p, &b(501), 0).err(), Some(E_SALE_ITEM_INVALID));
        assert!(s.trade_escrow(&mut p, &b(502), 181040000).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 18.11 (owner's item 9): an item bought on Buyout keeps the seller's gear set — properties are rolled anew (12.11),
    /// the set is not lost. Purple ring 185014000: its template has no set pool, before the fix the set disappeared on every deal
    #[tokio::test]
    async fn buyout_reroll_keeps_gear_set() {
        let dir = format!("target/test_buyout_set_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "buyset".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let ring = 185014000;
        let sets = |d: &[tgf::DBattleAffixRandomData]| -> Vec<(i32, i32)> {
            let mut v: Vec<(i32, i32)> = d.iter().flat_map(|a| a.gear_set_random.iter().map(|(k, c)| (*k, c.random_data[0].random_level))).collect();
            v.sort_unstable();
            v
        };
        let minor = |d: &[tgf::DBattleAffixRandomData]| d.iter().filter(|a| a.affix_type == 20).count();
        // the seller's ring: a normal roll + "Midas" (11009) in a separate MINOR entry instead of one property, as from a boss
        let mut seller = s.build_item_affix_data_seeded(ring, 0, 12345);
        assert!(sets(&seller).is_empty(), "the purple template has no sets");
        let n_minor = minor(&seller);
        let i = seller.iter().rposition(|a| a.affix_type == 20).expect("MINOR slot");
        let loc = seller[i].location;
        seller[i] = tgf::DBattleAffixRandomData {
            location: loc,
            affix_type: 20,
            gear_set_random: [(11009, tgf::DBattleAffixGearSetRandomCollectData {
                random_data: vec![tgf::DBattleAffixGearSetRandomData { random_level: 4321, group_index: 1 }],
            })]
            .into(),
            ..Default::default()
        };
        let mut it = item(901, ring, 1);
        it.data = Some(tgf::DItemData { affix_data: seller.clone(), ..Default::default() });
        // owner's decision 18.11: the set stays, its value (level) is rolled anew
        let mut levels = std::collections::HashSet::new();
        for _ in 0..20 {
            let mut bought = it.clone();
            s.trade_reroll(&mut bought);
            let d = &bought.data.as_ref().unwrap().affix_data;
            let got = sets(d);
            assert_eq!(got.iter().map(|g| g.0).collect::<Vec<_>>(), vec![11009], "the seller's set is kept");
            assert!((0..=9999).contains(&got[0].1), "level in range: {}", got[0].1);
            levels.insert(got[0].1);
            assert_eq!(minor(d), n_minor, "the number of MINOR properties did not grow");
        }
        assert!(levels.len() > 1, "the set value is rolled anew: {:?}", levels);
        // without a set at the seller — the roll as is (a set may appear only from the template pool)
        let plain = s.build_item_affix_data_seeded(ring, 0, 777);
        let mut it2 = item(902, ring, 1);
        it2.data = Some(tgf::DItemData { affix_data: plain, ..Default::default() });
        s.trade_reroll(&mut it2);
        assert!(sets(&it2.data.unwrap().affix_data).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keep_gear_sets_major_entry() {
        let set = |id: i32, lvl: i32| -> std::collections::HashMap<i32, tgf::DBattleAffixGearSetRandomCollectData> {
            [(id, tgf::DBattleAffixGearSetRandomCollectData { random_data: vec![tgf::DBattleAffixGearSetRandomData { random_level: lvl, group_index: 1 }] })].into()
        };
        let attr = |id: i32| -> std::collections::HashMap<i32, tgf::DBattleAffixAttributeRandomValueCollectData> { [(id, Default::default())].into() };
        let e = |t: i32, a, g| tgf::DBattleAffixRandomData { location: 1, affix_type: t, attribute_random: a, gear_set_random: g, ..Default::default() };
        // MAJOR with attributes and a set at the seller; the roll has a different set — the seller's one is kept
        let orig = vec![e(10, attr(1), set(11005, 100))];
        let rolled = vec![e(10, attr(2), set(11003, 9000)), e(20, attr(3), Default::default())];
        let got = keep_gear_sets(&orig, rolled.clone(), &|_| 777);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].attribute_random.keys().copied().collect::<Vec<_>>(), vec![2], "rolled attributes stay");
        assert_eq!(got[0].gear_set_random, set(11005, 777), "the seller's set with a new level");
        // the seller without sets — unchanged
        assert_eq!(keep_gear_sets(&[e(10, attr(1), Default::default())], rolled.clone(), &|_| 777), rolled);
    }
    fn item(guid: i64, type_id: i32, count: i32) -> tgf::DItem {
        tgf::DItem { basic: Some(tgf::DItemBasic { type_id, count, guid, ..Default::default() }), ..Default::default() }
    }
    fn sale(book: &mut TradeBook, uid: u64, price: i32, items: Vec<tgf::DItem>) -> i64 {
        let id = book.alloc();
        let n = items.iter().map(count_of).sum();
        book.orders.push(TradeOrder {
            meta: Some(new_meta(id, key(1, 10), 100, 1000, Kind::SalesOrder(tgf::DTradeSalesOrderMeta { origin_item_count: n, left_item_count: n, unit_price: price, unit_count: 1, user_key: None }))),
            owner_open_id: format!("u{}", uid),
            owner_user_id: uid,
            escrow: items,
            tax: 25,
            ..Default::default()
        });
        id
    }
    fn buy_order(book: &mut TradeBook, uid: u64, price: i32, count: i32) -> TradeOrder {
        let id = book.alloc();
        TradeOrder {
            meta: Some(new_meta(id, key(1, 10), 100, 1000, Kind::PurchaseOrder(tgf::DTradePurchaseOrderMeta { origin_item_count: count, left_item_count: count, current_unit_price: price, origin_price: price, ..Default::default() }))),
            owner_open_id: format!("u{}", uid),
            owner_user_id: uid,
            tax: 25,
            ..Default::default()
        }
    }
    fn pending_items(o: &TradeOrder) -> Vec<tgf::DItem> {
        o.confirm.as_ref().map(|c| c.pending_receive.values().flat_map(pend_items).collect()).unwrap_or_default()
    }
    fn gold_of(o: &TradeOrder) -> i64 {
        pending_items(o).iter().filter(|i| type_of(i) == GOLD).map(|i| count_of(i) as i64).sum()
    }

    #[test]
    fn buy_takes_cheapest_and_pays_seller_after_tax() {
        let mut book = TradeBook::default();
        let expensive = sale(&mut book, 1, 200, vec![item(1, 500, 5)]);
        let cheap = sale(&mut book, 2, 100, vec![item(2, 500, 3)]);
        let mut fx = Fx::default();
        let mut buyer = buy_order(&mut book, 3, 250, 4);
        let (filled, spent) = fill_buy(&mut book, &mut buyer, 250, 4, 200, &mut fx, &|it| {
            // reroll: a marker in affix_data — on every bought item
            it.data.get_or_insert_with(Default::default).affix_data = vec![tgf::DBattleAffixRandomData { affix_type: 99, ..Default::default() }];
        });
        assert_eq!((filled, spent), (4, 3 * 100 + 200));
        let items: i32 = pending_items(&buyer).iter().map(count_of).sum();
        assert_eq!(items, 4);
        assert!(pending_items(&buyer).iter().all(|i| i.data.as_ref().is_some_and(|d| d.affix_data[0].affix_type == 99)), "affixes rerolled");
        let e_left = &book.orders[book.idx(expensive).unwrap()].escrow;
        assert!(e_left.iter().all(|i| i.data.as_ref().map_or(true, |d| d.affix_data.is_empty())), "those left on the shelf keep the previous ones");
        let c = &book.orders[book.idx(cheap).unwrap()];
        assert_eq!(c.state(), ST_CONFIRMATION);
        assert_eq!(gold_of(c), 225); // 300 − 25%
        let e = &book.orders[book.idx(expensive).unwrap()];
        assert_eq!(e.state(), ST_TRADING);
        assert_eq!(e.escrow.iter().map(count_of).sum::<i32>(), 4);
        assert_eq!(gold_of(e), 150);
    }

    #[test]
    fn own_and_too_expensive_sales_are_skipped() {
        let mut book = TradeBook::default();
        sale(&mut book, 3, 50, vec![item(1, 500, 1)]); // own
        sale(&mut book, 1, 300, vec![item(2, 500, 1)]); // more expensive
        let mut fx = Fx::default();
        let mut buyer = buy_order(&mut book, 3, 250, 1);
        assert_eq!(fill_buy(&mut book, &mut buyer, 250, 1, 200, &mut fx, &|_| {}), (0, 0));
        assert!(fx.touched.is_empty());
    }

    #[test]
    fn sell_fills_standing_purchase_and_cancel_refunds_rest() {
        let mut book = TradeBook::default();
        let mut b = buy_order(&mut book, 3, 120, 2);
        if let Some(p) = b.purchase_mut() {
            p.left_price = 240;
        }
        let bid = b.id();
        book.orders.push(b);
        let mut fx = Fx::default();
        let id = book.alloc();
        let mut seller = TradeOrder {
            meta: Some(new_meta(id, key(1, 10), 100, 1000, Kind::SalesOrder(tgf::DTradeSalesOrderMeta { origin_item_count: 3, left_item_count: 3, unit_price: 100, unit_count: 1, user_key: None }))),
            owner_user_id: 1,
            escrow: vec![item(7, 500, 3)],
            tax: 25,
            ..Default::default()
        };
        assert_eq!(fill_sell(&mut book, &mut seller, 100, 200, &mut fx, &|_| {}), 2);
        assert_eq!(gold_of(&seller), 180); // 240 − 25%
        assert_eq!(seller.escrow.iter().map(count_of).sum::<i32>(), 1);
        let bo = &book.orders[book.idx(bid).unwrap()];
        assert_eq!(bo.state(), ST_CONFIRMATION);
        assert_eq!(pending_items(bo).iter().map(count_of).sum::<i32>(), 2);
        assert_eq!(bo.purchase().unwrap().left_price, 0);
        // cancelling a sale — the item remainder is returned
        book.orders.push(seller);
        let i = book.idx(id).unwrap();
        close_refund(&mut book, i, ST_CANCEL, 300, &mut fx);
        let s = &book.orders[i];
        assert_eq!(s.state(), ST_CANCEL);
        assert!(s.escrow.is_empty());
        assert_eq!(pending_items(s).iter().filter(|i| type_of(i) == 500).map(count_of).sum::<i32>(), 1);
    }

    /// 07.11: cancelling a standing purchase — the gold deposit comes as a FailedReturn confirmation (the client shows it and
    /// claims it, like seller proceeds), the remaining amount to buy is zeroed
    #[test]
    fn purchase_cancel_refunds_escrow_as_confirmation() {
        let mut book = TradeBook::default();
        let mut b = buy_order(&mut book, 3, 8000, 3);
        if let Some(p) = b.purchase_mut() {
            p.left_price = 24000;
        }
        let id = b.id();
        book.orders.push(b);
        let mut fx = Fx::default();
        let i = book.idx(id).unwrap();
        close_refund(&mut book, i, ST_CANCEL, 300, &mut fx);
        let o = &book.orders[i];
        assert_eq!(o.state(), ST_CANCEL);
        assert_eq!(o.purchase().unwrap().left_item_count, 0);
        assert_eq!(o.purchase().unwrap().left_price, 0);
        assert_eq!(gold_of(o), 24000);
        let pend: Vec<_> = o.confirm.as_ref().unwrap().pending_receive.values().cloned().collect();
        assert_eq!(pend.len(), 1);
        match &pend[0].data {
            Some(Pend::ConfirmationData(c)) => {
                let c = c.confirmation.as_ref().unwrap();
                assert_eq!(c.confirmation_type, CT_FAILED_RETURN);
                assert_eq!((c.unit_price, c.trade_count, c.total_price), (8000, 3, 24000));
            }
            other => panic!("expected a confirmation, not {:?}", other.is_some()),
        }
        assert!(fx.touched.contains(&id));
    }

    #[test]
    fn auction_outbid_refund_and_settle_on_expiry() {
        let mut book = TradeBook::default();
        let id = book.alloc();
        book.orders.push(TradeOrder {
            meta: Some(new_meta(
                id,
                key(2, 20),
                100,
                1000,
                Kind::AuctionOrder(tgf::DTradeAuctionMeta {
                    auction_order_meta: Some(tgf::DTradeAuctionOrderMeta { auction_trade_order_ticket_id: id, origin_price: 100, current_price: 100, fixed_price: 1000, user_key: None }),
                    bid_order_meta: None,
                }),
            )),
            owner_user_id: 1,
            escrow: vec![item(9, 600, 1)],
            tax: 25,
            ..Default::default()
        });
        let mut fx = Fx::default();
        assert_eq!(place_bid(&mut book, id, (1, "u1"), 200, 5, 200, &mut fx), Err(E_AUCTION_BID_SELF));
        assert_eq!(place_bid(&mut book, id, (2, "u2"), 99, 5, 200, &mut fx), Err(E_CURRENT_PRICE_BID));
        let (b1, c1) = place_bid(&mut book, id, (2, "u2"), 100, 5, 200, &mut fx).unwrap();
        assert_eq!(c1, 100);
        assert_eq!(place_bid(&mut book, id, (2, "u2"), 300, 5, 200, &mut fx), Err(E_AUCTION_ALREADY_BID));
        assert_eq!(place_bid(&mut book, id, (3, "u3"), 104, 5, 200, &mut fx), Err(E_CURRENT_PRICE_BID));
        let (b2, _) = place_bid(&mut book, id, (3, "u3"), 105, 5, 200, &mut fx).unwrap();
        assert_eq!(gold_of(&book.orders[book.idx(b1).unwrap()]), 100); // the outbid one gets the gold back
        tick(&mut book, 1000, &mut fx);
        let a = &book.orders[book.idx(id).unwrap()];
        assert_eq!(a.state(), ST_CONFIRMATION);
        assert_eq!(gold_of(a), 79); // 105 − 25%
        let w = &book.orders[book.idx(b2).unwrap()];
        assert_eq!(w.state(), ST_CONFIRMATION);
        assert_eq!(pending_items(w).iter().filter(|i| type_of(i) == 600).count(), 1);
        assert_eq!(book.orders[book.idx(b1).unwrap()].state(), ST_EXPIRATION);
    }

    #[test]
    fn buyout_settles_immediately_and_no_bids_returns_lot() {
        let mut book = TradeBook::default();
        let mk = |book: &mut TradeBook| {
            let id = book.alloc();
            book.orders.push(TradeOrder {
                meta: Some(new_meta(
                    id,
                    key(2, 20),
                    100,
                    1000,
                    Kind::AuctionOrder(tgf::DTradeAuctionMeta {
                        auction_order_meta: Some(tgf::DTradeAuctionOrderMeta { origin_price: 100, current_price: 100, fixed_price: 500, ..Default::default() }),
                        bid_order_meta: None,
                    }),
                )),
                owner_user_id: 1,
                escrow: vec![item(9, 600, 1)],
                ..Default::default()
            });
            id
        };
        let a = mk(&mut book);
        let mut fx = Fx::default();
        let (_, charged) = place_bid(&mut book, a, (2, "u2"), 9999, 5, 200, &mut fx).unwrap();
        assert_eq!(charged, 500); // not more than the buyout price
        assert_eq!(book.orders[book.idx(a).unwrap()].state(), ST_CONFIRMATION);
        let b = mk(&mut book);
        tick(&mut book, 1000, &mut fx);
        let o = &book.orders[book.idx(b).unwrap()];
        assert_eq!(o.state(), ST_EXPIRATION);
        assert_eq!(pending_items(o).len(), 1);
    }

    #[test]
    fn book_roundtrip_and_pull_units_split() {
        let mut esc = vec![item(1, 5, 3), item(2, 5, 4)];
        let got = pull_units(&mut esc, 5);
        assert_eq!(got.iter().map(count_of).collect::<Vec<_>>(), vec![3, 2]);
        assert_eq!(esc.iter().map(count_of).collect::<Vec<_>>(), vec![2]);
        let mut book = TradeBook::default();
        sale(&mut book, 1, 10, esc);
        let back = TradeBook::decode(book.encode_to_vec().as_slice()).unwrap();
        assert_eq!(back, book);
    }

    #[test]
    fn search_filters_by_label_and_pages() {
        let mut l = tgf::DTradeOrderTicketLabel { major_index_type: 4, minor_index_type: 41, ..Default::default() };
        l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_GEAR_SUB_TYPE, values: vec![1] });
        l.attributes.push(tgf::DTradeOrderTicketLabelAttribute { r#type: ATTR_RARITY, values: 3 });
        let tag = |t, v: Vec<i32>| tgf::DTradeOrderTicketSearchTag { r#type: t, values: v };
        let rar = |a, b| tgf::DTradeOrderTicketSearchAttribute { r#type: ATTR_RARITY, min_value: a, max_value: b };
        assert!(label_match(&l, 4, 0, &[tag(TAG_GEAR_SUB_TYPE, vec![1, 2])], &[rar(2, 4)], &[]));
        assert!(!label_match(&l, 5, 0, &[], &[], &[]));
        assert!(!label_match(&l, 0, 0, &[tag(TAG_GEAR_SUB_TYPE, vec![2])], &[], &[]));
        assert!(!label_match(&l, 0, 0, &[], &[rar(4, 0)], &[]));
        assert!(!label_match(&l, 0, 0, &[tag(TAG_AFFIX_ATTRIBUTE, vec![7])], &[], &[])); // no such affix
        assert!(label_match(&l, 0, 0, &[tag(TAG_ROLE_CAREER, vec![2])], &[], &[])); // profession not labeled — not filtered out
        assert!(!label_match(&tgf::DTradeOrderTicketLabel::default(), 0, 0, &[tag(TAG_GEAR_SUB_TYPE, vec![65])], &[], &[])); // without a subtype it does not match the filter
        assert!(!label_match(&l, 0, 0, &[], &[], &[tag(TAG_GEAR_SUB_TYPE, vec![1])]));
        // 09.11: class labeled — another class is filtered out; fixed affix and slot — strict
        l.tags.push(tgf::DTradeOrderTicketLabelTag { r#type: TAG_ROLE_CAREER, values: vec![4, 5] });
        assert!(label_match(&l, 0, 0, &[tag(TAG_ROLE_CAREER, vec![5])], &[], &[]));
        assert!(!label_match(&l, 0, 0, &[tag(TAG_ROLE_CAREER, vec![2])], &[], &[]));
        assert!(!label_match(&l, 0, 0, &[tag(TAG_AFFIX_ATTRIBUTE_FIXED, vec![11002])], &[], &[]));
        assert!(!label_match(&l, 0, 0, &[tag(TAG_WEARING_PART, vec![13])], &[], &[]));
    }

    /// 09.11: classes and slots from the gear tables: weapons by weapon_configs, armor by subtype, rings — for everyone
    #[test]
    fn gear_fit_from_tables() {
        let subs = crate::game_data::load_bytes_vec::<tgf::config::UeSourceEquipmentSubType>("ExcelConfig/UeSource_Equipment_SubType.bytes");
        let equip = crate::game_data::load_bytes_vec::<tgf::config::UeSourceEquipment>("ExcelConfig/UeSource_Equipment.bytes");
        if equip.is_empty() {
            return; // no ExcelConfig nearby (build without data)
        }
        let t = gear_fit_build(&subs, &equip);
        use tgf::EnGearEquipSubType as S;
        let of = |sub: S| equip.iter().find(|e| e.equip_subtype == sub as i32).map(|e| t[&e.equip_id].clone()).unwrap();
        let cloth = of(S::EnGearEquipSubtypeClothChest);
        assert!(cloth.careers.contains(&(tgf::EnRoleCareerType::EnRoleCareerWizard as i32)), "{:?}", cloth);
        assert!(!cloth.careers.contains(&(tgf::EnRoleCareerType::EnRoleCareerFighter as i32)), "{:?}", cloth);
        assert_eq!(cloth.parts, vec![tgf::EnItemWearingPart::TypeUpperarmor as i32]);
        let ring = of(S::EnGearEquipSubtypeRing);
        assert!(ring.careers.is_empty(), "ring — for everyone: {:?}", ring);
        let shield = of(S::EnGearEquipSubtypeShield);
        assert!(!shield.careers.is_empty() && shield.parts.contains(&(tgf::EnItemWearingPart::TypePair1leftweapon as i32)), "{:?}", shield);
    }
}

// ---- one-time migration of item guids (crate::guid::migrate) ----
/// Order book: guids in metadata, escrow, confirmations and lot snapshots
pub(super) async fn trade_migrate_guids(db: &crate::db::GameDb, map: &mut crate::guid::GuidMap) -> usize {
    let Some(mut book) = db.get::<BookDoc>(BOOK_TABLE, BOOK_ID).await.and_then(|d| TradeBook::decode(d.data.as_slice()).ok()) else {
        return 0;
    };
    let mut n = 0;
    for o in book.orders.iter_mut() {
        if let Some(m) = o.meta.as_mut() {
            n += crate::guid::remap_message(m, map);
        }
        for it in o.escrow.iter_mut() {
            n += crate::guid::remap_message(it, map);
        }
        if let Some(c) = o.confirm.as_mut() {
            n += crate::guid::remap_message(c, map);
        }
        if let Some(it) = o.item.as_mut() {
            n += crate::guid::remap_message(it, map);
        }
    }
    if n > 0 {
        let doc = BookDoc { data: book.encode_to_vec() };
        if let Err(e) = db.db.upsert::<Option<BookDoc>>((BOOK_TABLE, BOOK_ID)).content(doc).await {
            eprintln!("[Trade] guid migration: book not saved: {}", e);
        }
    }
    n
}
