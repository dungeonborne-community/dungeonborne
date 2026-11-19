//! Buying stash pages (owner's item 1, 18.11).
//!
//! As in the game: the "+" tab of the stash (`BP_InventoryPageAddTab`) without the item "Exquisite box" (1032,
//! `use_action.add_inventory_page = 1`, `is_auto_use`) opens a quick buy (`QuickBuyProductService`: 1032 → product 720001,
//! AutoUse) → a regular `market_buy_product(720001, 1)`. The product lives in tab 2000 of market 103 ("special"), the client
//! takes its price from the user's tab data. The box is used at once: the page is added, nothing is put into the stash.
//!
//! Owner's rules (`[features.stash]`, season.rs `Stash`): a new account gets `base_pages`, each next page costs
//! `page_price` gold (0 — the game's ladder market_special_cost), up to `max_pages` (0 — const inventory_max_page).
//! Accounts created before keep their pages (state without the field — 8).

use super::inventory_handlers as inv;
use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;

/// Tab of the page product (market 103 "special"); the client needs it in the user's market tabs to know the price
pub(super) const PAGE_TAB: i32 = 2000;

impl GatewaySession {
    /// Max stash pages: season `max_pages`, otherwise const inventory_max_page (8)
    pub(super) fn stash_max_pages(&self) -> i64 {
        let season = crate::season::features().stash.max_pages;
        if season > 0 {
            return season;
        }
        self.game_data.r#const.iter().find(|c| c.key == "inventory_max_page").and_then(|c| c.value.trim().parse().ok()).unwrap_or(inv::STASH_PAGES)
    }

    /// Pages added by one unit of an item that is used at once (auto-use + add_inventory_page); None — a regular item
    pub(super) fn stash_auto_pages(&self, type_id: i32) -> Option<i64> {
        let ua = self.game_data.item_type.get(&type_id)?.use_action.as_ref().filter(|u| u.is_auto_use)?;
        match ua.action_type.as_ref()? {
            tgf::d_item_use_action::ActionType::AddInventoryPage(k) => Some((*k).max(1) as i64),
            _ => None,
        }
    }

    /// Pages per purchase of a product (sum over its rewards); 0 — not a page product
    fn stash_product_pages(&self, product_id: i32) -> i64 {
        self.game_data
            .market_product_item
            .iter()
            .filter_map(|r| r.product.as_ref())
            .filter(|p| p.product_id == product_id)
            .flat_map(|p| p.reward_items.iter())
            .filter_map(|r| self.stash_auto_pages(r.type_id).map(|k| k * r.count.max(1) as i64))
            .sum()
    }

    /// The page product in the tab: available = how many pages are left to the max, price — the season's `page_price` gold
    pub(super) fn stash_tab_adjust(&self, p: &PlayerModel, tab: &mut tgf::DUserMarketTabData) {
        let (have, max, price) = (inv::stash_page_count(p), self.stash_max_pages(), crate::season::features().stash.page_price);
        for a in tab.available_products.iter_mut() {
            let per = self.stash_product_pages(a.product_id);
            if per <= 0 {
                continue;
            }
            let left = ((max - have).max(0) / per) as i32;
            a.available_count = left;
            a.total_available_count = a.total_available_count.max(left);
            if price > 0 {
                let price = price.min(i32::MAX as i64) as i32;
                a.cost_options = vec![tgf::MarketNormalOptionCost {
                    cost_items: vec![tgf::MarketNormalCost { cost_type: super::common::GOLD, cost_price: price, origin_price: price }],
                }];
            }
        }
    }

    /// Add `n` pages (no more than the max): the new pages for dirty_inventory_page; Err — the max is reached, nothing changed
    pub(super) fn stash_add_pages(&self, p: &mut PlayerModel, n: i64) -> Result<Vec<tgf::DUserInventoryPage>, String> {
        let (have, max) = (inv::stash_page_count(p), self.stash_max_pages());
        if have + n.max(1) > max {
            return Err(format!("stash pages {} + {} > max {}", have, n, max));
        }
        let now = have + n.max(1);
        p.state["inventory_page_count"] = serde_json::json!(now);
        Ok((have..now).map(|pg| inv::stash_page(p, pg)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 18.11 (owner's item 1): the quick buy of the page product 720001 adds a page right away (the box 1032 is not put
    /// into the stash), takes the page price, the page is saved; at the max — refused without charging; the tab shows how
    /// many pages are left. Prices/base here are passed explicitly: tests run without a season (default — 8 pages).
    #[tokio::test]
    async fn buy_stash_page_adds_page_and_stops_at_max() {
        let dir = format!("target/test_stash_pages_db_{}", crate::auth::sessions::random_hex(4));
        let s = GatewaySession {
            db: crate::db::GameDb::init(&dir).await,
            game_data: std::sync::Arc::new(crate::game_data::GameData::load_all("ExcelConfig")),
            open_id: "pages".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        assert_eq!(s.stash_auto_pages(1032), Some(1), "Exquisite box: +1 page, auto-use");
        assert_eq!(s.stash_auto_pages(100045), None, "arrows — a regular item");
        assert_eq!(s.stash_max_pages(), 8, "const inventory_max_page");
        assert_eq!(s.stash_product_pages(720001), 1);

        // a new account of the season "base 3": 3 pages, gold for two pages and a bit
        let mut p = PlayerModel::new("pages", 10001);
        p.state["inventory_page_count"] = serde_json::json!(3);
        p.state["gold"] = serde_json::json!(1_000_000);
        // without a season — the game's ladder: 5000 / 10000 / 25000 gold, then Moonstone (1012)
        p.state["coins"] = serde_json::json!({ "1012": 10_000 });
        s.db.save("players", "pages", &p).await;
        let buy = || tgf::CsMarketBuyProductReq { product_id: 720001, product_cnt: 1, market_type: 101, ..Default::default() };
        let err = || s.rpc_error.swap(0, std::sync::atomic::Ordering::Relaxed);
        let load = || async { s.db.get::<PlayerModel>("players", "pages").await.unwrap() };

        for want in 4..=8 {
            let gold = load().await.state["gold"].as_i64().unwrap();
            s.market_buy_product_impl(buy()).await;
            assert_eq!(err(), 0, "page {}", want);
            let p = load().await;
            assert_eq!(inv::stash_page_count(&p), want, "page added and saved");
            assert!(p.stash.element.iter().all(|i| i.basic.as_ref().unwrap().type_id != 1032), "the box is not put into the stash");
            let spent = gold - p.state["gold"].as_i64().unwrap() + (10_000 - p.state["coins"]["1012"].as_i64().unwrap_or(10_000));
            assert!(spent > 0, "price charged");
        }
        // at the max: refused, nothing charged
        let gold = load().await.state["gold"].as_i64().unwrap();
        s.market_buy_product_impl(buy()).await;
        assert_ne!(err(), 0, "beyond the max — refused");
        let p = load().await;
        assert_eq!(inv::stash_page_count(&p), 8);
        assert_eq!(p.state["gold"].as_i64().unwrap(), gold, "nothing charged at the max");

        // the tab shows how many are left and the price from the season (here — set by hand)
        let mut tab = tgf::DUserMarketTabData {
            available_products: vec![tgf::DUserMarketTabAvailableProduct { product_id: 720001, available_count: 7, ..Default::default() }],
            ..Default::default()
        };
        let mut p3 = PlayerModel::new("x", 1);
        p3.state["inventory_page_count"] = serde_json::json!(3);
        s.stash_tab_adjust(&p3, &mut tab);
        assert_eq!(tab.available_products[0].available_count, 5, "3 of 8 — 5 left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A new account gets the season's base pages; places for grants never go beyond the player's pages
    #[test]
    fn new_account_pages_follow_season() {
        let p = PlayerModel::new("n", 1);
        assert_eq!(inv::stash_page_count(&p), crate::season::features().stash.base_pages);
    }
}
