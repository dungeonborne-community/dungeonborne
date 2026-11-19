//! Raid results from the client (report_action: LocalSettle/PrivateRaid/FETCH_LOADOUT): loot extraction, raid experience, loadouts for the listen host, p2p room.
//! Moved from handlers.rs (31.10): handlers.rs only delegates (`handle_<rpc>` → `<rpc>_impl`).

use super::*;

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ActiveP2PHost {
    pub host_open_id: String,
    pub host_ip: String,
    pub level_id: i32,
    pub timestamp: i64,
    pub players_joined: i32,
    pub client_open_ids: Vec<String>,
}

static ACTIVE_P2P_HOST: OnceLock<Mutex<Option<ActiveP2PHost>>> = OnceLock::new();

pub(super) fn p2p_room() -> &'static Mutex<Option<ActiveP2PHost>> {
    ACTIVE_P2P_HOST.get_or_init(|| Mutex::new(None))
}

impl GatewaySession {
    pub(super) fn encode_hex(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(&mut s, "{:02x}", b);
        }
        s
    }

    /// Serializes the player's active character (metadata, skills, equipment with affixes and the backpack) for the Listen server
    pub fn encode_player_loadout(player: &PlayerModel) -> Option<String> {
        let role = player
            .roles
            .iter()
            .find(|r| r.role_guid == player.hall_role_guid)
            .or_else(|| player.roles.first())?;

        let deploy = role.data.deploy_role.as_ref()?;
        let basic = deploy.basic.as_ref().or(role.data.basic.as_ref())?;
        let meta = basic.meta.as_ref();

        let type_id = basic.type_id;
        let race = meta.map(|m| m.race).unwrap_or(1);
        let career = meta.map(|m| m.career).unwrap_or(1);
        let gender = meta.map(|m| m.gender).unwrap_or(1);
        let level = deploy.role_level.max(1);

        let skills_str = deploy
            .role_skill
            .as_ref()
            .map(|s| {
                let mut ids: Vec<String> = s
                    .role_active_skill
                    .iter()
                    .filter(|k| k.skill_id > 0)
                    .map(|k| k.skill_id.to_string())
                    .collect();
                for &pid in &s.role_passive_skill {
                    if pid > 0 {
                        ids.push(pid.to_string());
                    }
                }
                ids.join(",")
            })
            .unwrap_or_default();

        let mut items_parts = Vec::new();

        if let Some(wearing) = &deploy.wearing {
            for item in &wearing.items {
                if let Some(b) = &item.basic {
                    let slot = b.position.as_ref().map(|p| p.x).unwrap_or(0);
                    let count = b.count.max(1);
                    let hex = Self::encode_hex(&item.encode_to_vec());
                    if slot > 0 && b.type_id > 0 {
                        items_parts.push(format!("w,{},{},{},{}", slot, b.type_id, count, hex));
                    }
                }
            }
        }

        if let Some(backpack) = &deploy.backpack {
            for item in &backpack.items {
                if let Some(b) = &item.basic {
                    let x = b.position.as_ref().map(|p| p.x).unwrap_or(0);
                    let y = b.position.as_ref().map(|p| p.y).unwrap_or(0);
                    let count = b.count.max(1);
                    let hex = Self::encode_hex(&item.encode_to_vec());
                    if b.type_id > 0 {
                        items_parts.push(format!("b,0,{},{},{},{},{}", x, y, b.type_id, count, hex));
                    }
                }
            }
        }

        // The 11th field — the role's base attributes (role_affix.attribute: id → value, e.g. 1001 strength, 2001 MaxHealthBase).
        // In a local match the client applies them to itself; on the server ("listen as dedicated") LocalDev sets them on other
        // pawns from this field — on 31.10 without it the joined players had stats of 30 and default HP.
        let mut attrs: Vec<(i32, f32)> = deploy
            .role_affix
            .as_ref()
            .map(|a| a.attribute.iter().map(|(k, v)| (*k, *v)).collect())
            .unwrap_or_default();
        attrs.sort_by_key(|(k, _)| *k);
        let attrs_str = attrs.iter().map(|(k, v)| format!("{}:{}", k, v)).collect::<Vec<_>>().join(",");

        Some(format!(
            "L1|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            player.user_id,
            role.role_guid,
            type_id,
            race,
            career,
            gender,
            level,
            skills_str,
            items_parts.join(";"),
            attrs_str
        ))
    }

    pub(super) fn decode_hex(s: &str) -> Option<Vec<u8>> {
        if s.len() % 2 != 0 {
            return None;
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
            .collect()
    }

    /// Parses an inventory snapshot string from the match:
    /// "w,<slot>,<count>,<hex>;b,<bag_kind>,<x>,<y>,<count>,<hex>"
    pub fn parse_raid_items(
        snapshot_body: &str,
        role_guid: i64,
    ) -> (Vec<tgf::DItem>, Vec<tgf::DItem>, Vec<tgf::DItem>) {
        let mut wearing = Vec::new();
        let mut backpack = Vec::new();
        let mut small_backpack = Vec::new();

        for entry in snapshot_body.split(';') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let parts: Vec<&str> = entry.split(',').collect();
            if parts[0] == "w" && parts.len() >= 4 {
                let slot: i32 = parts[1].parse().unwrap_or(0);
                let count: i32 = parts[2].parse().unwrap_or(1).max(1);
                if let Some(bytes) = Self::decode_hex(parts[3]) {
                    if let Ok(mut item) = tgf::DItem::decode(bytes.as_slice()) {
                        let basic = item.basic.get_or_insert_with(tgf::DItemBasic::default);
                        if basic.guid == 0 || crate::guid::too_big(basic.guid) {
                            basic.guid = crate::guid::next_item();
                        }
                        basic.count = count;
                        basic.position = Some(tgf::DItemPosition {
                            r#type: 3, // RoleWearing
                            guid: role_guid,
                            x: slot,
                            y: 0,
                            limit_option: None,
                        });
                        wearing.push(item);
                    }
                }
            } else if parts[0] == "b" && parts.len() >= 6 {
                let bag_kind: i32 = parts[1].parse().unwrap_or(0);
                let x: i32 = parts[2].parse().unwrap_or(0);
                let y: i32 = parts[3].parse().unwrap_or(0);
                let count: i32 = parts[4].parse().unwrap_or(1).max(1);
                if let Some(bytes) = Self::decode_hex(parts[5]) {
                    if let Ok(mut item) = tgf::DItem::decode(bytes.as_slice()) {
                        let basic = item.basic.get_or_insert_with(tgf::DItemBasic::default);
                        if basic.guid == 0 || crate::guid::too_big(basic.guid) {
                            basic.guid = crate::guid::next_item();
                        }
                        basic.count = count;
                        let pos_type = if bag_kind == 1 { 4 } else { 2 }; // 2 = Backpack, 4 = SmallBackpack
                        basic.position = Some(tgf::DItemPosition {
                            r#type: pos_type,
                            guid: role_guid,
                            x,
                            y,
                            limit_option: None,
                        });
                        if bag_kind == 1 {
                            small_backpack.push(item);
                        } else {
                            backpack.push(item);
                        }
                    }
                }
            }
        }

        (wearing, backpack, small_backpack)
    }

    /// The item is equipment (item_type.item_major_type = Equipment)
    pub(super) fn is_equipment(&self, type_id: i32) -> bool {
        self.game_data.item_type.get(&type_id).map(|t| t.item_major_type == tgf::EnItemMajorType::Equipment as i32).unwrap_or(false)
    }

    /// Vendor sell price of an item: item_type.system_repurchase_price — the same number the client shows in the tooltip.
    /// 0 — no price. 13.11: the trade price (trade_commodity.price_def) is no longer substituted — it was taken from the first
    /// matching row of any market (189010000 went to the vendor for 400 000 at the auction price), and the client did not show it.
    /// Equipment buyback prices — 50% of the standard Buyout Trades price (user's decision 13.11) — in the season item_type.
    pub(super) fn item_sell_price(&self, type_id: i32) -> i64 {
        self.game_data.item_type.get(&type_id).map(|t| t.system_repurchase_price as i64).filter(|p| *p > 0).unwrap_or(0)
    }

    /// A crafted item is a relic (Heirloom): the result of gear_heirloom_guide.inst_id or a UE resource from the Heirloom folder
    /// (variants like 220110141 are not listed in the guide)
    pub(super) fn is_crafted(&self, type_id: i32) -> bool {
        self.game_data.gear_heirloom_guide.values().any(|g| g.inst_id == type_id)
            || self.game_data.uesource_itemtype.get(&type_id).map(|u| u.ue_path.contains("Heirloom")).unwrap_or(false)
    }

    /// Vendor sell price (07.11, user's decision): crafted items — 0, the rest — `item_sell_price`.
    /// Before, relics (system_repurchase_price 0) took the trade price price_def: pink (EXCEPTIONAL) — 40000.
    pub(super) fn merchant_sell_price(&self, type_id: i32) -> i64 {
        if self.is_crafted(type_id) {
            0
        } else {
            self.item_sell_price(type_id)
        }
    }

    /// Raid exit: the last inventory snapshot becomes the role's inventory. In Dead Money mode the extracted loot
    /// is sold for gold: items in the bags (backpack and small) whose guids the role did not have before the raid (guid 0 / foreign); own
    /// items and all worn gear (even found in the raid) stay in place. Without the mode nothing is sold.
    /// Loot without a price stays in the inventory. Gold — in state.gold, the result — in state.raid_last_sale and state.raid_sales.
    /// Returns (items sold, gold received).
    pub(super) fn settle_extraction(&self, player: &mut PlayerModel, role_guid: u64, snapshot: &str, tag: &str) -> (usize, i64) {
        let (wearing, backpack, small_bp) = Self::parse_raid_items(snapshot, role_guid as i64);
        self.settle_extraction_items(player, role_guid, wearing, backpack, small_bp, tag)
    }

    /// The same for ready items (DS native result: `raid_apply_native_result`)
    pub(super) fn settle_extraction_items(
        &self,
        player: &mut PlayerModel,
        role_guid: u64,
        wearing: Vec<tgf::DItem>,
        backpack: Vec<tgf::DItem>,
        small_bp: Vec<tgf::DItem>,
        tag: &str,
    ) -> (usize, i64) {
        let owned: std::collections::HashSet<i64> = player
            .roles
            .iter()
            .find(|r| r.role_guid == role_guid)
            .and_then(|r| r.data.deploy_role.as_ref())
            .map(|d| {
                let lists = [d.wearing.as_ref().map(|w| &w.items), d.backpack.as_ref().map(|b| &b.items), d.small_backpack.as_ref().map(|b| &b.items)];
                lists.into_iter().flatten().flatten().filter_map(|i| i.basic.as_ref()).map(|b| b.guid).filter(|g| *g != 0).collect()
            })
            .unwrap_or_default();

        let (mut sold, mut gold, mut lines, mut unpriced) = (0usize, 0i64, Vec::new(), Vec::new());
        // auto-sell is a season flag (seasons/, features.auto_sell_extracted): without it everything extracted stays with the player
        let auto_sale = crate::season::features().auto_sell_extracted;
        let mut sell = |items: Vec<tgf::DItem>| -> Vec<tgf::DItem> {
            items
                .into_iter()
                .filter(|item| {
                    let Some(b) = item.basic.as_ref() else { return true };
                    if !auto_sale || owned.contains(&b.guid) {
                        return true; // auto-sell is off or it is an own item
                    }
                    let price = self.item_sell_price(b.type_id);
                    if price <= 0 {
                        unpriced.push(b.type_id);
                        return true;
                    }
                    let count = b.count.max(1) as i64;
                    sold += 1;
                    gold += price * count;
                    lines.push(serde_json::json!({ "type_id": b.type_id, "count": count, "price": price }));
                    false
                })
                .collect()
        };
        // worn gear is not sold (incl. gear found and worn in the raid) — only the bags
        let (backpack, small_bp) = (sell(backpack), sell(small_bp));

        if let Some(role) = player.get_role_mut(role_guid) {
            if let Some(deploy) = &mut role.data.deploy_role {
                deploy.wearing = Some(tgf::DRoleWearingData { items: wearing });
                deploy.backpack = Some(tgf::DRoleBackpackData { items: backpack, row_size: 0, column_size: 0 });
                if !small_bp.is_empty() {
                    deploy.small_backpack = Some(tgf::DRoleBackpackData { items: small_bp, row_size: 2, column_size: 5 });
                }
            }
        }
        // 09.11: backpack width by race
        self.apply_bag_sizes(player);
        // 12.11: a second relic of the same kind worn in the raid (the server mod did not take it off) — to the backpack or the stash
        let off = super::relic_limit::take_off_extra(player, role_guid, &|t| self.relic_kind(t), &|t| self.inv_item_shape(t));
        if !off.is_empty() {
            println!("[RelicLimit] {}: extra relics taken off after the raid {:?}", player.open_id, off);
        }
        let before = common::coin(player, common::GOLD);
        player.state["gold"] = serde_json::json!(before + gold);
        let sale = serde_json::json!({
            "at": Self::now_ts(), "tag": tag, "role_guid": role_guid, "sold": sold, "gold": gold,
            "gold_before": before, "items": lines, "unpriced": unpriced,
        });
        let mut history = player.state["raid_sales"].as_array().cloned().unwrap_or_default();
        history.push(sale.clone());
        let keep = history.len().saturating_sub(20);
        player.state["raid_sales"] = serde_json::Value::Array(history.split_off(keep));
        player.state["raid_last_sale"] = sale;
        // snapshot applied — a repeated exit status (2 → 3) will not sell the loot a second time
        player.state["raid_last_snapshot"] = serde_json::json!("");
        println!(
            "[RaidSale] {} ({}): sold {} items for {} gold ({} -> {}){}",
            player.open_id,
            tag,
            sold,
            gold,
            before,
            before + gold,
            if unpriced.is_empty() { String::new() } else { format!("; no price, kept: {:?}", unpriced) }
        );
        (sold, gold)
    }

    /// Raid start for experience (map_exp by time in the raid): the first match snapshot after the previous result
    pub(super) fn raid_mark_start(player: &mut PlayerModel) {
        if player.state["raid_started_at"].as_i64().unwrap_or(0) == 0 {
            player.state["raid_started_at"] = serde_json::json!(Self::now_ts());
        }
    }

    /// Raid result (extraction or death): experience by time in the raid (progress_raid_exp), level-up, dirty_role push.
    /// true — the player was changed (save).
    pub(super) fn raid_finish_exp(&self, player: &mut PlayerModel, role_guid: u64, tag: &str) -> bool {
        let started = player.state["raid_started_at"].as_i64().unwrap_or(0);
        if started == 0 || role_guid == 0 {
            return false;
        }
        player.state["raid_started_at"] = serde_json::json!(0);
        let alive = Self::now_ts() - started;
        let (_, map_id) = self.build_level_and_map_params(&Self::get_saved_level_select(player));
        // potion experience for this match: once, the field is set by raid_apply_native_result_in
        let bonus = player.state["raid_bonus_exp"].as_i64().unwrap_or(0) as i32;
        player.state["raid_bonus_exp"] = serde_json::json!(0);
        let exp = self.progress_raid_exp(map_id, alive) + bonus;
        player.state["raid_last_exp"] = serde_json::json!(exp); // for the results screen (raid_native::raid_push_settle)
        if let Some((old, new)) = self.progress_add_role_exp(player, role_guid, exp) {
            println!("[Exp] {} {}: map {}, {} s in raid → +{} exp (potions {}), level {} → {}", player.open_id, tag, map_id, alive, exp, bonus, old, new);
            self.raid_push_role(player, role_guid);
        }
        true
    }

    /// dirty_role to the player (it may not be the current session — e.g. the listen-server host reports for others)
    pub(super) fn raid_push_role(&self, player: &PlayerModel, role_guid: u64) {
        if let Some(r) = player.roles.iter().find(|r| r.role_guid == role_guid) {
            // + gold: the raid result changes it (loot auto-sell), and the counter in the main menu is updated only
            // by dirty_coin_data — without it, before 05.11 the menu showed the old gold, the stash (player_get_info) — the new one
            let msg = tgf::ScPlayerDirtyChgSync {
                dirty_role: vec![r.data.clone()],
                dirty_coin_data: vec![super::common::coin_dirty(player, super::common::GOLD)],
                ..Default::default()
            };
            crate::lobby::hub::push_to(&player.open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &msg);
        }
    }

    /// Saves the player's serialized loadout into Saved/RebuildProbe/client-loadout-<user_id>.txt — LocalDev
    /// on the host initializes the joined player from it (PlayerState.GetUserId). client-loadout.txt — the last written one,
    /// for the old LocalDev (because of it all joined players got the class of whoever wrote last).
    pub fn write_client_loadout_file(player: &PlayerModel) {
        if let Some(loadout_str) = Self::encode_player_loadout(player) {
            if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
                let dir = std::path::Path::new(&local_app_data)
                    .join("Dungeonborne")
                    .join("Saved")
                    .join("RebuildProbe");
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join(format!("client-loadout-{}.txt", player.user_id)), &loadout_str);
                let _ = std::fs::write(dir.join("client-loadout.txt"), &loadout_str);
            }
        }
    }

    /// Accepts inventory/loot/experience snapshots from the match (LocalSettle.lua and PrivateRaid.lua)
    pub(super) async fn report_action_impl(
        &self,
        req: crate::tgf::CsReportActionReq,
    ) -> Option<crate::tgf::ScReportActionRsp> {
        let open_id = self.clean_open_id();

        if let Some(body) = req.body {
            if let Some(biz) = body.business_data {
                let desc = biz.report_desc.as_str();
                let content = biz.report_content.as_str();

                // 1. LocalSettle.lua protocol:
                // "v1|<seq>|<status>|<items>" (single) OR "v2|<user_id_or_open_id>|<seq>|<status>|<items>" (server on behalf of any player)
                // status: 0 = in a match (save the snapshot), 1 = died (clear items), 2 = escaped through a portal, 3 = left
                if biz.report_scene == 97001 || desc == "LOCALSETTLE" {
                    let (target_key, seq, status, snapshot_str) = if content.starts_with("v2|") {
                        let p: Vec<&str> = content.splitn(5, '|').collect();
                        if p.len() >= 5 {
                            (p[1].to_string(), p[2].parse().unwrap_or(0), p[3].parse().unwrap_or(0), p[4])
                        } else {
                            (open_id.clone(), 0, -1, "")
                        }
                    } else {
                        let p: Vec<&str> = content.splitn(4, '|').collect();
                        if p.len() >= 4 && p[0] == "v1" {
                            (open_id.clone(), p[1].parse().unwrap_or(0), p[2].parse().unwrap_or(0), p[3])
                        } else {
                            (open_id.clone(), 0, -1, "")
                        }
                    };

                    if status >= 0 {
                        let mut player = if let Ok(uid) = target_key.parse::<u64>() {
                            if let Some(by_uid) = self.db.find_by_user_id::<PlayerModel>(uid).await {
                                by_uid
                            } else {
                                self.db.get::<PlayerModel>("players", &target_key).await
                                    .unwrap_or_else(|| PlayerModel::new(&open_id, 10001))
                            }
                        } else {
                            self.db.get::<PlayerModel>("players", &target_key).await
                                .unwrap_or_else(|| PlayerModel::new(&open_id, 10001))
                        };
                        let player_open_id = player.open_id.clone();

                        let role_guid = if player.hall_role_guid != 0 {
                            player.hall_role_guid
                        } else {
                            player.roles.first().map(|r| r.role_guid).unwrap_or(0)
                        };

                        if status == 0 && !snapshot_str.is_empty() {
                            player.state["raid_last_snapshot"] = serde_json::json!(snapshot_str);
                            player.state["raid_last_seq"] = serde_json::json!(seq);
                            Self::raid_mark_start(&mut player);
                            self.db.save("players", &player_open_id, &player).await;
                        } else if status == 2 || status == 3 {
                            // Successful match exit — apply the last non-empty snapshot to the character!
                            let saved_snap = player.state["raid_last_snapshot"]
                                .as_str()
                                .unwrap_or("")
                                .to_string();
                            if !saved_snap.is_empty() && role_guid != 0 {
                                // Successful exit: own items — back in place, extracted loot — into gold
                                self.settle_extraction(&mut player, role_guid, &saved_snap, &format!("LocalSettle status={}", status));
                                println!(
                                    "[LocalSettle] Player {} (user_id={}) successfully extracted loot from the raid (status={})!",
                                    player_open_id, player.user_id, status
                                );
                                self.db.save("players", &player_open_id, &player).await;
                            }
                            if self.raid_finish_exp(&mut player, role_guid, &format!("LocalSettle status={}", status)) {
                                self.db.save("players", &player_open_id, &player).await;
                            }
                        } else if status == 1 {
                            println!(
                                "[LocalSettle] Player {} (user_id={}) died in the raid.",
                                player_open_id, player.user_id
                            );
                            if self.raid_finish_exp(&mut player, role_guid, "LocalSettle death") {
                                self.db.save("players", &player_open_id, &player).await;
                            }
                        }
                    }
                }
                // 2. PrivateRaid.lua protocol: "rb1|<token>|<seq>|<state>|<body>"
                else if biz.report_scene == 97042 || desc == "REBUILD_LOCAL_RAID_V1" {
                    let parts: Vec<&str> = content.splitn(5, '|').collect();
                    if parts.len() >= 5 && parts[0] == "rb1" {
                        let token = parts[1];
                        let state = parts[3];
                        let payload = parts[4];

                        let mut player = self
                            .db
                            .get::<PlayerModel>("players", &open_id)
                            .await
                            .unwrap_or_else(|| PlayerModel::new(&open_id, 10001));

                        let role_guid = if player.hall_role_guid != 0 {
                            player.hall_role_guid
                        } else {
                            player.roles.first().map(|r| r.role_guid).unwrap_or(0)
                        };

                        if state == "snapshot" && !payload.is_empty() {
                            player.state["raid_last_snapshot"] = serde_json::json!(payload);
                            if player.state["raid_token"].as_str() != Some(token) {
                                player.state["raid_started_at"] = serde_json::json!(0); // new raid
                            }
                            player.state["raid_token"] = serde_json::json!(token);
                            Self::raid_mark_start(&mut player);
                            self.db.save("players", &open_id, &player).await;
                        } else if state == "extracted" || state == "left" {
                            let already_done = player.state["raid_done"].as_str().unwrap_or("");
                            if already_done != token {
                                player.state["raid_done"] = serde_json::json!(token);
                                let saved_snap = player.state["raid_last_snapshot"]
                                    .as_str()
                                    .unwrap_or("")
                                    .to_string();
                                if !saved_snap.is_empty() && role_guid != 0 {
                                    // Successful exit: own items — back in place, extracted loot — into gold
                                    self.settle_extraction(&mut player, role_guid, &saved_snap, &format!("PrivateRaid {}", state));
                                    println!(
                                        "[PrivateRaid] Player {} extracted loot from the raid ({})!",
                                        open_id, state
                                    );
                                }
                                self.raid_finish_exp(&mut player, role_guid, &format!("PrivateRaid {}", state));
                                self.db.save("players", &open_id, &player).await;
                            }
                        } else if state == "dead" {
                            let already_done = player.state["raid_done"].as_str().unwrap_or("");
                            if already_done != token {
                                player.state["raid_done"] = serde_json::json!(token);
                                self.raid_finish_exp(&mut player, role_guid, "PrivateRaid dead");
                                self.db.save("players", &open_id, &player).await;
                            }
                        }
                    }
                }
                // 3. Loadout request of a joined player from the Host: report_scene == 97002 or desc == "FETCH_LOADOUT"
                else if biz.report_scene == 97002 || desc == "FETCH_LOADOUT" {
                    // content contains user_id, open_id or the client's sequence number ("1", "2")
                    let req_key = content.trim();
                    let mut target_open_id = None;

                    if let Ok(client_idx) = req_key.parse::<usize>() {
                        if client_idx >= 1 && client_idx <= 4 {
                            let guard = p2p_room().lock().unwrap();
                            if let Some(room) = guard.as_ref() {
                                if let Some(cid) = room.client_open_ids.get(client_idx - 1) {
                                    target_open_id = Some(cid.clone());
                                }
                            }
                        }
                    }

                    let target_player = if let Some(ref cid) = target_open_id {
                        self.db.get::<PlayerModel>("players", cid).await
                    } else if let Ok(uid) = req_key.parse::<u64>() {
                        self.db.find_by_user_id::<PlayerModel>(uid).await
                    } else if !req_key.is_empty() {
                        self.db.get::<PlayerModel>("players", req_key).await
                    } else {
                        None
                    };

                    // If the client connected directly via the console `open 127.0.0.1:7777` without matchmaking,
                    // look for any other player in the DB (other than the Host)
                    let resolved_player = if target_player.is_some() {
                        target_player
                    } else {
                        self.db.get::<PlayerModel>("players", &open_id).await
                    };

                    if let Some(tp) = resolved_player {
                        if let Some(loadout_str) = Self::encode_player_loadout(&tp) {
                            println!(
                                "[Loadout] Sent the loadout of player open_id={} (user_id={}) to the Host!",
                                tp.open_id, tp.user_id
                            );
                            return Some(crate::tgf::ScReportActionRsp {
                                err_code: 0,
                                err_msg: loadout_str,
                            });
                        }
                    }
                }
            }
        }

        Some(crate::tgf::ScReportActionRsp {
            err_code: 0,
            err_msg: "OK".to_string(),
        })
    }
}
