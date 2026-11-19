//! Native raid result from the game server (dbnative → `POST /ds/player_result`, src/ds/agent_link.rs).
//!
//! The server is the authority in a raid: the native `UBattleDsUtility` chain (death, portal extraction, leaving) takes
//! the inventory off the pawn into the player record `DBattleUserData`; dbnative serializes it at status 3 and sends it here. Record codes are
//! native enums: status = `EnDSPlayerGameStatusType` (2 JOIN, 3 GAMEOVER), reason =
//! `EnDSPlayerActorStatusType` (1 SELF_QUIT, 2 DEAD, 3 ESCAPE, 4 ALIVE, 11–17 server/lobby failures).
//!
//! Exactly once: when issuing battle data (`GET /ds/battle_user_data`) the backend writes `state.raid_entry` on the player
//! {token, role_guid, issued_at, guids, applied}; the token goes into `ds_player_key.ds_token`. The result applies only
//! to an unapplied entry of the same role (and the same token, if the server returned it); a repeat → 409.
//!
//! Decision:
//! - ESCAPE / ALIVE — extraction: a teammate's gear goes back to them by mail (`raid_team_returns`), role inventory = result,
//!   with the season flag `auto_sell_extracted` extracted loot from bags is sold (`settle_extraction_items`);
//! - DEAD / SELF_QUIT — death: worn gear and backpack are lost, the small backpack stays (as the server returned it);
//! - failures (11–17, other) — items as before the raid, result marked applied.
//! Raid exp — `raid_finish_exp` in both outcomes. Duplicates (a guid that already lies in the stash or on another role
//! and was not issued to the raid) are dropped.

use super::*;
use super::progress_handlers::QuestEvent;
use serde_json::json;
use std::collections::{HashMap, HashSet};

pub const GAME_STATUS_GAMEOVER: i32 = tgf::EnDsPlayerGameStatusType::Gameover as i32;
const ACTOR_SELF_QUIT: i32 = tgf::EnDsPlayerActorStatusType::SelfQuit as i32;
const ACTOR_DEAD: i32 = tgf::EnDsPlayerActorStatusType::Dead as i32;
const ACTOR_ESCAPE: i32 = tgf::EnDsPlayerActorStatusType::Escape as i32;
const ACTOR_ALIVE: i32 = tgf::EnDsPlayerActorStatusType::Alive as i32;
/// 09.11: "kicked, game over immediately" — native "Hand of Justice" (in-match ban, GA_TGF_Hero_Self_Destruct); treated as death
const ACTOR_KICKED: i32 = tgf::EnDsPlayerActorStatusType::KickOffAndGameOverAtOnce as i32;

/// Apply rejection: HTTP code and reason (dbnative response)
pub struct NativeReject(pub u16, pub String);

fn reject(code: u16, why: impl Into<String>) -> NativeReject {
    NativeReject(code, why.into())
}

/// Paid insurance of an item that died together with its owner (08.11): the price of the "Insurance Premium" for the killer.
/// Table INSURANCE_PAID, key — guid of the insured item.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub(super) struct InsurancePaid {
    pub value: i64,
    pub owner: String,
    pub at: i64,
}
pub(super) const INSURANCE_PAID: &str = "insurance_paid";

fn items_of(list: &Option<impl HasItems>) -> Vec<tgf::DItem> {
    list.as_ref().map(|l| l.items()).unwrap_or_default()
}

trait HasItems {
    fn items(&self) -> Vec<tgf::DItem>;
}
impl HasItems for tgf::DRoleWearingData {
    fn items(&self) -> Vec<tgf::DItem> {
        self.items.clone()
    }
}
impl HasItems for tgf::DRoleBackpackData {
    fn items(&self) -> Vec<tgf::DItem> {
        self.items.clone()
    }
}

fn guid_of(i: &tgf::DItem) -> i64 {
    i.basic.as_ref().map(|b| b.guid).unwrap_or(0)
}

/// Returning items from a match (13.11, user: "give him his items back and make a command"): battle server record at raid end
/// (`gameover-<open_id>-3-HHMMSS.bin` from Robot `ue4ss\Mods\DungeonborneRuClient\native\`, put into `RESTORE_DIR`,
/// default `restore/`). What to return: worn gear and backpack of the record (the small backpack stays on death anyway), except
/// insured items (insurance returns them itself) and items the player has now. Ok(record role, items).
pub(super) fn raid_restore_items(p: &PlayerModel, file: &str) -> Result<(u64, Vec<tgf::DItem>), String> {
    use prost::Message;
    let ok_name = !file.is_empty() && file.ends_with(".bin") && file.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) && !file.contains("..");
    if !ok_name {
        return Err(format!("file name {:?}: only letters, digits, . _ -, ending in .bin", file));
    }
    let dir = std::env::var("RESTORE_DIR").unwrap_or_else(|_| "restore".into());
    let bytes = std::fs::read(std::path::Path::new(&dir).join(file)).map_err(|e| format!("{}/{}: {}", dir, file, e))?;
    let data = tgf::DBattleUserData::decode(&bytes[..]).map_err(|e| format!("cannot parse record: {}", e))?;
    let deploy = data.deploy_preview.as_ref().and_then(|d| d.deploy_role.as_ref()).ok_or("record has no deploy_role")?;
    let role_guid = deploy.basic.as_ref().map(|b| b.role_guid as u64).unwrap_or(0);
    if !p.roles.iter().any(|r| r.role_guid == role_guid) {
        return Err(format!("record role {} is not a role of player {}", role_guid, p.open_id));
    }
    let mut have: HashSet<i64> = p.stash.element.iter().map(guid_of).collect();
    for r in &p.roles {
        have.extend(role_guids(p, r.role_guid));
    }
    let insured = |i: &tgf::DItem| i.data.as_ref().and_then(|x| x.insurance.as_ref()).is_some_and(|x| x.active);
    let items = [items_of(&deploy.wearing), items_of(&deploy.backpack)]
        .concat()
        .into_iter()
        .filter(|i| guid_of(i) != 0 && !have.contains(&guid_of(i)) && !insured(i))
        .collect();
    Ok((role_guid, items))
}

/// Late result after leaving the hall (13.11): items issued to the raid that are not among those the player still has
fn late_gone(issued: &HashSet<i64>, kept: &HashSet<i64>) -> HashSet<i64> {
    issued.iter().copied().filter(|g| *g != 0 && !kept.contains(g)).collect()
}

/// Return mails: for how many matches to keep the record of a player's gear. A teammate could start a new raid before the extractor
const RAID_GEAR_KEEP: usize = 5;
const MAIL_TEAMMATE_RETURN: i32 = 501035;
const MAIL_OFFHAND_RETURN: i32 = 501036;
const PAIR_RIGHT_WEAPONS: [i32; 2] = [tgf::EnItemWearingPart::TypePair1rightweapon as i32, tgf::EnItemWearingPart::TypePair2rightweapon as i32];

fn owner_ids(item: &tgf::DItem) -> Vec<u64> {
    item.create_info.as_ref().and_then(|c| c.ownership.as_ref()).map(|o| o.user_keys.iter().map(|k| k.user_id).collect()).unwrap_or_default()
}

fn set_owners(item: &mut tgf::DItem, ids: &[u64]) {
    let keys = ids.iter().map(|&user_id| tgf::DPlayerIdKey { user_id, zone_id: 1 }).collect();
    item.create_info.get_or_insert_with(Default::default).ownership = Some(tgf::DItemOwnership { user_keys: keys });
}

/// Match record: team and gear on entry. Re-issuing in the same match does not change the record
fn raid_gear_record(player: &mut PlayerModel, role_guid: u64, battle: &str, team: &[String]) {
    let mut list = player.state["raid_gear"].as_array().cloned().unwrap_or_default();
    if list.iter().any(|r| r["battle"] == json!(battle)) {
        return;
    }
    let deploy = player.roles.iter().find(|r| r.role_guid == role_guid).and_then(|r| r.data.deploy_role.as_ref());
    let items: Vec<tgf::DItem> = deploy.map(|d| [items_of(&d.wearing), items_of(&d.backpack), items_of(&d.small_backpack)].concat()).unwrap_or_default();
    let gear: Vec<serde_json::Value> = items.iter().filter(|i| guid_of(i) != 0).map(|i| json!({ "g": guid_of(i), "o": owner_ids(i) })).collect();
    list.push(json!({ "battle": battle, "team": team, "gear": gear }));
    let drop = list.len().saturating_sub(RAID_GEAR_KEEP);
    player.state["raid_gear"] = json!(list.split_off(drop));
}

/// Recorded match gear: guid → owners of the item on entry
fn raid_gear_of(player: &PlayerModel, battle: &str) -> Option<(Vec<String>, HashMap<i64, Vec<u64>>)> {
    let list = player.state["raid_gear"].as_array()?;
    let rec = list.iter().find(|r| r["battle"] == json!(battle))?;
    let team = rec["team"].as_array()?.iter().filter_map(|t| t.as_str().map(str::to_string)).collect();
    let gear = rec["gear"]
        .as_array()?
        .iter()
        .filter_map(|g| Some((g["g"].as_i64()?, g["o"].as_array()?.iter().filter_map(|u| u.as_u64()).collect())))
        .collect();
    Some((team, gear))
}

/// What to return after extraction
#[derive(Default)]
struct TeamReturns {
    /// teammate's open_id and their gear
    to_mates: Vec<(String, Vec<tgf::DItem>)>,
    /// own off-hand weapon: the main one went back to the teammate
    off_hand: Vec<tgf::DItem>,
}

impl TeamReturns {
    fn give_back(&self) -> Vec<tgf::DItem> {
        self.to_mates.iter().flat_map(|(_, items)| items.iter().cloned()).collect()
    }
}

/// guids of all role items (worn, backpack, small backpack)
fn role_guids(p: &PlayerModel, role_guid: u64) -> HashSet<i64> {
    p.roles
        .iter()
        .find(|r| r.role_guid == role_guid)
        .and_then(|r| r.data.deploy_role.as_ref())
        .map(|d| {
            let mut v = items_of(&d.wearing);
            v.extend(items_of(&d.backpack));
            v.extend(items_of(&d.small_backpack));
            v.iter().map(guid_of).filter(|g| *g != 0).collect()
        })
        .unwrap_or_default()
}

impl GatewaySession {
    /// Insured role items before the raid (worn and backpack): (place, item)
    fn raid_take_insured(&self, p: &PlayerModel, role_guid: u64) -> Vec<(i32, tgf::DItem)> {
        let Some(d) = p.roles.iter().find(|r| r.role_guid == role_guid).and_then(|r| r.data.deploy_role.as_ref()) else { return vec![] };
        let active = |i: &tgf::DItem| i.data.as_ref().and_then(|x| x.insurance.as_ref()).is_some_and(|x| x.active);
        let mut out = vec![];
        for i in items_of(&d.wearing).into_iter().filter(active) {
            out.push((super::inventory_handlers::WEARING, i));
        }
        for i in items_of(&d.backpack).into_iter().filter(active) {
            out.push((super::inventory_handlers::BACKPACK, i));
        }
        out
    }

    /// Return insured items after death: worn — into their slot (the grey kit item from that slot is removed),
    /// from the backpack — to the previous cell or the first free one (backpack, then stash). Insurance is removed.
    /// Returns (guid, amount paid) — for the premium price.
    fn raid_restore_insured(&self, p: &mut PlayerModel, role_guid: u64, items: Vec<(i32, tgf::DItem)>) -> Vec<(i64, i32)> {
        use super::inventory_handlers as inv;
        let shape_of = |t: i32| self.inv_item_shape(t);
        let mut kept = vec![];
        for (place, mut item) in items {
            let value = item.data.as_ref().and_then(|d| d.insurance.as_ref()).map_or(0, |i| i.value);
            if let Some(d) = item.data.as_mut() {
                d.insurance = None;
            }
            let guid = guid_of(&item);
            let pos = inv::pos_of(&item);
            let mut target = tgf::DItemPosition { r#type: place, guid: role_guid as i64, x: pos.x, y: pos.y, limit_option: None };
            if place == inv::WEARING {
                if let Some(list) = inv::container_mut(p, &target) {
                    list.retain(|i| !inv::same_place(&inv::pos_of(i), &target));
                }
            } else {
                let s = shape_of(item.basic.as_ref().map_or(0, |b| b.type_id));
                if !inv::area_free(p, &target, s.w, s.h, 0, &shape_of) {
                    let stash = tgf::DItemPosition { r#type: inv::STASH, ..Default::default() };
                    match inv::free_cell(p, &target, s.w, s.h, &shape_of).or_else(|| inv::free_cell(p, &stash, s.w, s.h, &shape_of)) {
                        Some(free) => target = free,
                        None => {
                            println!("[Insurance] guid={} nowhere to return — item lost", guid);
                            continue;
                        }
                    }
                }
            }
            if let Some(b) = item.basic.as_mut() {
                b.position = Some(target.clone());
            }
            if let Some(list) = inv::container_mut(p, &target) {
                list.push(item);
                kept.push((guid, value));
            }
        }
        kept
    }

    /// The player's result in this server session is already applied — re-entering the same raid (dbnative `&session=`).
    pub fn raid_finished_in_session(player: &PlayerModel, session: &str) -> bool {
        let e = &player.state["raid_entry"];
        !session.is_empty() && e["applied"] == json!(true) && e["session"].as_str() == Some(session)
    }

    /// Issuing battle data to the server: an "issued to raid" entry (one unapplied per role). Returns the token for
    /// `ds_player_key.ds_token`. Re-issuing the same role before the result (reconnect) — the same token.
    pub fn raid_entry_issue(player: &mut PlayerModel, role_guid: u64, session: &str, room_token: Option<&str>) -> String {
        let e = &player.state["raid_entry"];
        if e["applied"] == json!(false) && e["role_guid"].as_u64() == Some(role_guid) {
            if let Some(t) = e["token"].as_str().map(str::to_string).filter(|t| room_token.map_or(true, |r| r == t)) {
                // another server session — the previous raid ended without a result (server crashed, kill without exit): raid time
                // is counted anew, otherwise exp would accrue for the whole gap between raids
                if e["session"].as_str().unwrap_or("") != session {
                    player.state["raid_started_at"] = json!(Self::now_ts());
                }
                player.state["raid_entry"]["session"] = json!(session);
                return t;
            }
        }
        // room token (ds_init_finish → client address → check in battle_user_data) — the same in the match record and in the result
        let token = room_token.map(str::to_string).unwrap_or_else(|| format!("{:016x}", rand::random::<u64>()));
        let guids: Vec<i64> = role_guids(player, role_guid).into_iter().collect();
        player.state["raid_entry"] = json!({
            "token": token, "role_guid": role_guid, "issued_at": Self::now_ts(), "guids": guids, "applied": false,
            "session": session,
        });
        if let Some((battle, team)) = crate::lobby::matchmaking::battle_mates(&player.open_id) {
            raid_gear_record(player, role_guid, &battle, &team);
            player.state["raid_entry"]["battle"] = json!(battle);
            // team in the match (party) — for the rating: solo search (data_type 3) or team (4)
            player.state["raid_entry"]["team_size"] = json!(team.len() + 1);
        }
        // new entry — new raid: time from this issue. raid_mark_start kept the old time of the previous issue without a result
        // (04.11 08:25 room closed "did not come to the hall" → at 08:34 leaving the hall after 20 s counted as 559 s, lvl 8→9)
        player.state["raid_started_at"] = json!(Self::now_ts());
        token
    }

    /// Apply the raid result of player `open_id` (`DBattleUserData` record from the server). Ok — summary for the response.
    pub async fn raid_apply_native_result(
        &self,
        open_id: &str,
        game_status: i32,
        actor_status: i32,
        session: &str,
        data: tgf::DBattleUserData,
    ) -> Result<serde_json::Value, NativeReject> {
        self.raid_apply_native_result_in(open_id, game_status, actor_status, session, data, false, 0).await
    }

    /// `hall` — the mod marked the result as leaving the waiting hall (`scene=waiting_room`, natively finish_scenes = 1
    /// WAITING_ROOM: PlayerGameFinish does not take items). Without the mark the hall is detected by `battle_started = false`.
    /// `bonus_exp` — potion exp for the match (`raid_finish_exp` adds it only on extracting alive, 10.11).
    pub async fn raid_apply_native_result_in(
        &self,
        open_id: &str,
        game_status: i32,
        actor_status: i32,
        session: &str,
        data: tgf::DBattleUserData,
        hall: bool,
        bonus_exp: i32,
    ) -> Result<serde_json::Value, NativeReject> {
        if game_status != GAME_STATUS_GAMEOVER {
            return Err(reject(400, format!("status {} is not GAMEOVER (3)", game_status)));
        }
        let mut player = self.db.get::<PlayerModel>("players", open_id).await.ok_or_else(|| reject(404, "no such player"))?;
        let entry = player.state["raid_entry"].clone();
        if entry.is_null() {
            return Err(reject(409, "no raid issue (battle_user_data) — nothing to apply the result to"));
        }
        if entry["applied"] != json!(false) {
            // 13.11 (dupe): raid closed by leaving the hall, but the player still played the match — remove what was given away and lost
            if !hall && entry["result"]["applied"] == json!("hall_leave") {
                return self.raid_late_after_hall(&mut player, open_id, actor_status, session, &data, &entry).await;
            }
            return Err(reject(409, format!("result already applied: {}", entry["result"])));
        }
        let deploy = data.deploy_preview.as_ref().and_then(|d| d.deploy_role.as_ref()).ok_or_else(|| reject(400, "record has no deploy_role"))?;
        // raid start time for stats (raid_finish_exp resets it)
        let raid_started = player.state["raid_started_at"].as_i64().filter(|t| *t > 0).unwrap_or_else(Self::now_ts);
        let role_guid = deploy.basic.as_ref().map(|b| b.role_guid as u64).unwrap_or(0);
        if Some(role_guid) != entry["role_guid"].as_u64() {
            return Err(reject(409, format!("result role {} ≠ issued {}", role_guid, entry["role_guid"])));
        }
        let issued_in = entry["session"].as_str().unwrap_or("");
        if !session.is_empty() && !issued_in.is_empty() && session != issued_in {
            return Err(reject(409, format!("result from session {} ≠ issue session {}", session, issued_in)));
        }
        let token = data.ds_player_key.as_ref().map(|k| k.ds_token.as_str()).unwrap_or("");
        if !token.is_empty() && Some(token) != entry["token"].as_str() {
            return Err(reject(409, "result token does not match the issued one"));
        }

        // leaving the waiting hall (the room match has not started yet): as in the original — no losses, exp, quests or result.
        // Items in the database are untouched (the result was not applied yet), we close the raid entry and remove the player from the room.
        let room = crate::lobby::matchmaking::room_of_player(open_id);
        // custom game (private room match, battle_id "pr…", 05.11): no loot and no losses — items and exp
        // are not touched (same path as leaving the hall). PVP result window — TODO.
        // 11.11: and any arena (DM, "Trial of Clotho" — match "clotho…"): "Loot Protection", only worn gear went into the match
        let custom = room.as_deref().is_some_and(super::raid_lock::keeps_items);
        if hall || custom || room.as_deref().map_or(false, |b| !crate::lobby::allocator::battle_started(b)) {
            return Ok(self.raid_hall_leave(&mut player, open_id, room.as_deref(), actor_status, hall).await);
        }

        // duplicates: an item that is already in the stash or on another role and was not issued to the raid
        let issued: HashSet<i64> = entry["guids"].as_array().map(|a| a.iter().filter_map(|g| g.as_i64()).collect()).unwrap_or_default();
        let mut elsewhere: HashSet<i64> = player.stash.element.iter().map(guid_of).collect();
        for r in player.roles.iter().filter(|r| r.role_guid != role_guid) {
            elsewhere.extend(role_guids(&player, r.role_guid));
        }
        let mut dropped = 0;
        let mut clean = |items: Vec<tgf::DItem>| -> Vec<tgf::DItem> {
            items
                .into_iter()
                .filter(|i| {
                    let g = guid_of(i);
                    let dup = g != 0 && elsewhere.contains(&g) && !issued.contains(&g);
                    dropped += dup as usize;
                    !dup
                })
                .collect()
        };
        let mut wearing = clean(items_of(&deploy.wearing));
        let mut backpack = clean(items_of(&deploy.backpack));
        let mut small = clean(items_of(&deploy.small_backpack));
        let mut returns = TeamReturns::default();
        if matches!(actor_status, ACTOR_ESCAPE | ACTOR_ALIVE) && common::env_flag("RAID_RETURN_TEAM_GEAR", true) {
            let battle = entry["battle"].as_str().map(str::to_string).or_else(|| room.clone());
            if let Some(battle) = battle {
                returns = self.raid_team_returns(&player, &battle, &issued, &mut wearing, &mut backpack, &mut small).await;
            }
        }

        let tag = format!("native actor={}", actor_status);
        // Sh4f: dirty_role, quests, mails, battle_settle_sync — after the client returns to the lobby (hub::take_deferred)
        let defer = crate::lobby::hub::defer_enabled();
        if defer {
            crate::lobby::hub::set_defer(open_id, true);
        }
        let before = role_exp_of(&player, role_guid);
        // 10.11: ascension potion — "exp after a successful escape" (Item_Description_100070): only on extracting alive;
        // on death, self-quit and kick the potion exp is not granted (06.11–10.11 it was granted in both cases)
        let escaped = matches!(actor_status, ACTOR_ESCAPE | ACTOR_ALIVE);
        player.state["raid_bonus_exp"] = json!(if escaped { bonus_exp } else { 0 });
        if !escaped && bonus_exp > 0 {
            println!("[Exp] {} {}: potion exp {} not granted — not an extraction", open_id, tag, bonus_exp);
        }
        let summary = match actor_status {
            ACTOR_ESCAPE | ACTOR_ALIVE => {
                let (sold, gold) = self.settle_extraction_items(&mut player, role_guid, wearing, backpack, small, &tag);
                self.raid_finish_exp(&mut player, role_guid, &tag);
                json!({ "applied": "extraction", "sold": sold, "gold": gold })
            }
            ACTOR_DEAD | ACTOR_SELF_QUIT | ACTOR_KICKED => {
                let lost = items_of(&deploy.wearing).len() + items_of(&deploy.backpack).len();
                // insurance (08.11, as in the original): an insured item from the pre-raid inventory stays with the owner,
                // insurance expires; the battle server itself gives the killer the "Insurance Premium" (its price — insurance_paid)
                let insured = self.raid_take_insured(&player, role_guid);
                if let Some(d) = player.get_role_mut(role_guid).and_then(|r| r.data.deploy_role.as_mut()) {
                    let (rows, cols) = d.backpack.as_ref().map(|b| (b.row_size, b.column_size)).unwrap_or((7, 10));
                    d.wearing = Some(tgf::DRoleWearingData { items: vec![] });
                    d.backpack = Some(tgf::DRoleBackpackData { items: vec![], row_size: rows, column_size: cols });
                    let (srows, scols) = d.small_backpack.as_ref().map(|b| (b.row_size, b.column_size)).unwrap_or((2, 5));
                    d.small_backpack = Some(tgf::DRoleBackpackData { items: small.clone(), row_size: srows, column_size: scols });
                }
                // 08.11 user decision: after death — the class's grey starter kit (worn + backpack)
                let role_type = player.get_role_mut(role_guid).map(|r| r.type_id as i32).unwrap_or(0);
                let (kit_wear, kit_bag) = self.generate_starter_items(role_type, role_guid as i64);
                if let Some(d) = player.get_role_mut(role_guid).and_then(|r| r.data.deploy_role.as_mut()) {
                    if let Some(w) = d.wearing.as_mut() {
                        w.items = kit_wear;
                    }
                    if let Some(b) = d.backpack.as_mut() {
                        b.items = kit_bag;
                    }
                }
                let kept = self.raid_restore_insured(&mut player, role_guid, insured.clone());
                for (guid, value) in &kept {
                    let rec = InsurancePaid { value: *value as i64, owner: open_id.to_string(), at: Self::now_ts() };
                    self.db.save(INSURANCE_PAID, &guid.to_string(), &rec).await;
                }
                if !kept.is_empty() {
                    println!("[Insurance] {}: death, insured items stayed with the owner: {:?}", open_id, kept);
                }
                self.raid_finish_exp(&mut player, role_guid, &tag);
                json!({ "applied": match actor_status { ACTOR_DEAD => "death", ACTOR_KICKED => "ban_kick", _ => "self_quit" },
                        "lost_items": lost, "small_backpack_kept": small.len() })
            }
            other => json!({ "applied": "kept", "actor_status": other }),
        };
        let mut summary = summary;
        summary["dropped_duplicates"] = json!(dropped);
        let give_back = returns.give_back();
        if !give_back.is_empty() || !returns.off_hand.is_empty() {
            summary["returned_to_team"] = json!(give_back.len());
            summary["off_hand_mailed"] = json!(returns.off_hand.len());
        }
        // quests (Q1): participation and (extraction) victory; level — the last choice in the lobby, as for exp (B3a)
        let lvl = Self::get_saved_level_select(&player);
        // 09.11 player stats (stats.rs): games, extractions/deaths, player kills, raid time
        let raid_kills = super::rank::raid_kills(&player.state) as i64;
        super::stats::after_raid(
            &mut player.state,
            role_guid,
            lvl.level_id as i64,
            summary["applied"].as_str().unwrap_or(""),
            raid_kills,
            Self::now_ts() - raid_started,
        );
        let mut quests_changed = false;
        let mut kinds = vec![tgf::EnQuestProgressType::BattleParticipate];
        if matches!(actor_status, ACTOR_ESCAPE | ACTOR_ALIVE) {
            kinds.push(tgf::EnQuestProgressType::BattleVictory);
        }
        for kind in kinds {
            let mut ev = QuestEvent::new(kind, 1, role_guid);
            ev.level_type = Some(lvl.level_type as i64);
            ev.level_id = Some(lvl.level_id as i64);
            quests_changed |= self.quest_event(&mut player, &ev);
        }
        // rating "dungeon kills" (rank.rs): match into the role history, tables are updated immediately; the player — in the common save below
        if matches!(actor_status, ACTOR_ESCAPE | ACTOR_ALIVE | ACTOR_DEAD | ACTOR_SELF_QUIT | ACTOR_KICKED) {
            let team = player.state["raid_entry"]["team_size"].as_u64().unwrap_or(1) > 1;
            let rank_kills = super::rank::raid_kills(&player.state);
            let boards = self.rank_after_raid(&mut player, role_guid, lvl.level_id, team, rank_kills).await;
            summary["rank_boards"] = json!(boards);
            summary["rank_kills"] = json!(rank_kills);
        }
        // reward for player kills per raid (season flag kill_reward) — by mail; the counter is reset in the same save
        let kills = if crate::season::features().kill_reward.is_some() { take_kills(&mut player.state) } else { 0 };
        // items from the DS result without a guid (picked-up consumables) — guid and role position, otherwise inventory_item_move breaks
        let repaired = super::inventory_handlers::repair_items(&mut player);
        if repaired > 0 {
            summary["repaired_items"] = json!(repaired);
        }
        player.state["raid_entry"]["applied"] = json!(true);
        player.state["raid_entry"]["applied_at"] = json!(Self::now_ts());
        player.state["raid_entry"]["result"] = summary.clone();
        player.state["raid_last_snapshot"] = json!("");
        self.db.save("players", open_id, &player).await;
        Self::write_client_loadout_file(&player);
        self.raid_push_role(&player, role_guid);
        if kills > 0 {
            self.kill_reward_mail(&player, kills).await;
        }
        self.raid_send_returns(&player, role_guid, returns).await;
        if quests_changed {
            crate::lobby::hub::push_to(open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &self.quest_dirty(&player));
        }
        let gold = summary["gold"].as_i64().unwrap_or(0) as i32;
        let settle = self.raid_push_settle(&player, role_guid, actor_status, deploy, token, before, gold, give_back);
        // Sh4g (RAID_DEFER_ROOM): room end — also deferred, after the result (order in the lobby: result, then room)
        let defer_room = crate::lobby::hub::defer_room_enabled();
        if defer && !defer_room {
            crate::lobby::hub::set_defer(open_id, false);
        } else if !defer && defer_room {
            crate::lobby::hub::set_defer(open_id, true);
        }
        // the match is over for this player: room events 5–6, otherwise the client will go back to the server (M3)
        if let Some(b) = crate::lobby::matchmaking::room_of_player(open_id) {
            super::lobby_handlers::room_finish_push(open_id, &b, settle.result, settle.level_parameter);
        }
        if defer || defer_room {
            crate::lobby::hub::set_defer(open_id, false);
            println!("[RaidNative] {}: deferred until lobby: {}{}", open_id, if defer { "result (RAID_DEFER_SETTLE) " } else { "" },
                if defer_room { "room end (RAID_DEFER_ROOM)" } else { "" });
        }
        println!("[RaidNative] {} role {}: {}", open_id, role_guid, summary);
        // match room: results from everyone — close it and free the server (M3)
        let mut summary = summary;
        if let Some(closed) = crate::lobby::matchmaking::member_result(open_id) {
            summary["room_closed"] = json!(closed);
        }
        Ok(summary)
    }

    /// Raid extraction: the teammate's gear is taken away, the rest of the gear gets owner markers.
    /// An item stays with the extractor if they are among the item's owners (found together in a team raid).
    /// Own gear — markers as on entry. Found and looted from an enemy — own team marker, solo without a marker.
    async fn raid_team_returns(
        &self,
        player: &PlayerModel,
        battle: &str,
        issued: &HashSet<i64>,
        wearing: &mut Vec<tgf::DItem>,
        backpack: &mut Vec<tgf::DItem>,
        small: &mut Vec<tgf::DItem>,
    ) -> TeamReturns {
        let Some((team, own_gear)) = raid_gear_of(player, battle) else {
            return TeamReturns::default();
        };
        let mut mates_gear: HashMap<i64, (String, u64, Vec<u64>)> = HashMap::new();
        let mut team_ids = vec![player.user_id];
        for mate in &team {
            let Some(m) = self.db.get::<PlayerModel>("players", mate).await else { continue };
            team_ids.push(m.user_id);
            if let Some((_, gear)) = raid_gear_of(&m, battle) {
                for (guid, owners) in gear {
                    mates_gear.insert(guid, (mate.clone(), m.user_id, owners));
                }
            }
        }
        let mut by_mate: HashMap<String, Vec<tgf::DItem>> = HashMap::new();
        let mut freed_right: Vec<i32> = Vec::new();
        let is_gear = |i: &tgf::DItem| self.is_equipment(i.basic.as_ref().map(|b| b.type_id).unwrap_or(0));
        for (list, worn) in [(&mut *wearing, true), (&mut *backpack, false), (&mut *small, false)] {
            let mut keep = Vec::with_capacity(list.len());
            for mut item in std::mem::take(list) {
                let guid = guid_of(&item);
                if !is_gear(&item) {
                    keep.push(item);
                    continue;
                }
                if issued.contains(&guid) {
                    if let Some(owners) = own_gear.get(&guid).filter(|o| !o.is_empty()) {
                        set_owners(&mut item, owners);
                    }
                    keep.push(item);
                    continue;
                }
                match mates_gear.get(&guid) {
                    Some((_, _, owners)) if owners.contains(&player.user_id) => {
                        set_owners(&mut item, owners);
                        keep.push(item);
                    }
                    Some((mate, mate_id, owners)) => {
                        if worn {
                            let part = item.basic.as_ref().and_then(|b| b.position.as_ref()).map(|p| p.x).unwrap_or(0);
                            if PAIR_RIGHT_WEAPONS.contains(&part) {
                                freed_right.push(part);
                            }
                        }
                        set_owners(&mut item, if owners.is_empty() { std::slice::from_ref(mate_id) } else { owners });
                        by_mate.entry(mate.clone()).or_default().push(item);
                    }
                    None => {
                        if team.is_empty() {
                            if let Some(c) = item.create_info.as_mut() {
                                c.ownership = None;
                            }
                        } else {
                            set_owners(&mut item, &team_ids);
                        }
                        keep.push(item);
                    }
                }
            }
            *list = keep;
        }
        // the main weapon went to the teammate: the off-hand of the same pair comes to the extractor by mail
        let (off_hand, rest): (Vec<_>, Vec<_>) = std::mem::take(wearing).into_iter().partition(|i| {
            let part = i.basic.as_ref().and_then(|b| b.position.as_ref()).map(|p| p.x).unwrap_or(0);
            freed_right.iter().any(|r| part == r + 1)
        });
        *wearing = rest;
        TeamReturns { to_mates: by_mate.into_iter().collect(), off_hand }
    }

    /// Return mails: teammates get their gear, the extractor — their off-hand
    async fn raid_send_returns(&self, player: &PlayerModel, role_guid: u64, returns: TeamReturns) {
        let name = player.roles.iter().find(|r| r.role_guid == role_guid).map(|r| r.role_name.clone()).unwrap_or_default();
        let mut send = Vec::new();
        for (mate, items) in returns.to_mates {
            for part in items.chunks(super::progress_handlers::MAIL_MAX_ITEMS) {
                send.push((mate.clone(), MAIL_TEAMMATE_RETURN, part.to_vec()));
            }
        }
        for part in returns.off_hand.chunks(super::progress_handlers::MAIL_MAX_ITEMS) {
            send.push((player.open_id.clone(), MAIL_OFFHAND_RETURN, part.to_vec()));
        }
        for (to, template, items) in send {
            let count = items.len();
            let ext = [("TEAMMATE_NAME", name.clone())];
            match super::progress_handlers::mail_from_template(&self.game_data, template, items, &ext) {
                Some(mut mail) => {
                    mail.channel = tgf::EnMailChannelType::EnMailChannelBattleBringInRecover as i32;
                    super::progress_handlers::mail_send(&self.db, &self.game_data, &to, mail).await;
                    println!("[RaidReturn] {} → {}: template {}, items {}", player.open_id, to, template, count);
                }
                None => println!("[RaidReturn] no mail template {}: {} items for {} lost", template, count, to),
            }
        }
    }

    /// 13.11 (dupe, safeguard for the server mod's HallGuard): the result of a real raid arrived when the raid was already closed by leaving
    /// the hall (`hall_leave`: items at home, the result is not applied). This happened on a connection drop at the end of the hall timer: the battle
    /// server sent "left the hall", and the reconnected player got a pawn and went into the raid with the same items. Loot found in such a
    /// raid is not given; from home we remove items issued to the raid that are not in the result (given away, dropped, lost): on
    /// extracting alive — everything not on the character; on death — everything except the small backpack and insured items.
    async fn raid_late_after_hall(
        &self,
        player: &mut PlayerModel,
        open_id: &str,
        actor_status: i32,
        session: &str,
        data: &tgf::DBattleUserData,
        entry: &serde_json::Value,
    ) -> Result<serde_json::Value, NativeReject> {
        let deploy = data.deploy_preview.as_ref().and_then(|d| d.deploy_role.as_ref()).ok_or_else(|| reject(400, "record has no deploy_role"))?;
        let role_guid = deploy.basic.as_ref().map(|b| b.role_guid as u64).unwrap_or(0);
        if Some(role_guid) != entry["role_guid"].as_u64() {
            return Err(reject(409, format!("result role {} ≠ issued {}", role_guid, entry["role_guid"])));
        }
        let issued_in = entry["session"].as_str().unwrap_or("");
        if !session.is_empty() && !issued_in.is_empty() && session != issued_in {
            return Err(reject(409, format!("result from session {} ≠ issue session {}", session, issued_in)));
        }
        let token = data.ds_player_key.as_ref().map(|k| k.ds_token.as_str()).unwrap_or("");
        if !token.is_empty() && Some(token) != entry["token"].as_str() {
            return Err(reject(409, "result token does not match the issued one"));
        }
        if entry["late_result"].is_object() {
            return Err(reject(409, format!("late result already processed: {}", entry["late_result"])));
        }
        let issued: HashSet<i64> = entry["guids"].as_array().map(|a| a.iter().filter_map(|g| g.as_i64()).collect()).unwrap_or_default();
        let escaped = matches!(actor_status, ACTOR_ESCAPE | ACTOR_ALIVE);
        let mut kept: HashSet<i64> = items_of(&deploy.small_backpack).iter().map(guid_of).collect();
        if escaped {
            kept.extend(items_of(&deploy.wearing).iter().map(guid_of));
            kept.extend(items_of(&deploy.backpack).iter().map(guid_of));
        } else {
            kept.extend(self.raid_take_insured(player, role_guid).iter().map(|(_, i)| guid_of(i)));
        }
        let gone = late_gone(&issued, &kept);
        let mut removed = vec![];
        for place in super::inventory_handlers::all_places(player) {
            if let Some(list) = super::inventory_handlers::container_mut(player, &place) {
                list.retain(|i| {
                    let g = guid_of(i);
                    let hit = gone.contains(&g);
                    if hit {
                        removed.push(json!({ "guid": g, "type_id": i.basic.as_ref().map_or(0, |b| b.type_id) }));
                    }
                    !hit
                });
            }
        }
        let summary = json!({ "applied": "late_after_hall", "actor_status": actor_status, "issued": issued.len(),
                              "removed": removed.len(), "removed_items": removed });
        player.state["raid_entry"]["late_result"] = summary.clone();
        self.db.save("players", open_id, player).await;
        println!("[RaidNative] ALERT {}: raid result after leaving the hall (dupe?) — found loot not given, removed from home {}: {}",
            open_id, summary["removed"], summary);
        Ok(summary)
    }

    /// Result of a player who left the waiting hall (B12): nothing is applied. The raid entry is "applied" (a repeated result
    /// 409), raid time is reset; the client gets "match over" (events 5–6, close the search), no battle_settle_sync;
    /// removed from the room (an empty one closes — `room_closed`, the mod resets the server).
    async fn raid_hall_leave(&self, player: &mut PlayerModel, open_id: &str, battle_id: Option<&str>, actor_status: i32, marked: bool) -> serde_json::Value {
        let mut summary = json!({ "applied": "hall_leave", "actor_status": actor_status, "scene_marked": marked });
        player.state["raid_entry"]["applied"] = json!(true);
        player.state["raid_entry"]["applied_at"] = json!(Self::now_ts());
        player.state["raid_entry"]["result"] = summary.clone();
        player.state["raid_started_at"] = json!(0);
        player.state["raid_last_snapshot"] = json!("");
        self.db.save("players", open_id, player).await;
        if let Some(battle_id) = battle_id {
            super::lobby_handlers::room_finish_push(open_id, battle_id, None, None);
            crate::lobby::matchmaking::leave_room(open_id, battle_id);
            if crate::lobby::matchmaking::room(battle_id).is_none() {
                summary["room_closed"] = json!(battle_id);
            }
        }
        println!("[RaidNative] {}: left the waiting hall of room {} — no losses or exp: {}", open_id, battle_id.unwrap_or("?"), summary);
        summary
    }

    /// Player-on-player kill from the server (`POST /ds/kill`, Sh4): the killer gets +1 to raid kills for the rating
    /// (raid_entry.kills, always) and, with the season flag kill_reward, to unclaimed kills — reward by mail at their result
    /// (`kill_reward_mail`). Not counted: self, one of the two is not a player, both in the same match team
    /// (`team::battle_team_id`). Ok(unclaimed with reward / raid kills without it) / Err(reason).
    pub async fn register_kill(&self, killer: &str, victim: &str) -> Result<i64, NativeReject> {
        self.count_kill(killer, victim, crate::season::features().kill_reward.is_some()).await
    }

    /// Quest progress from the battle server (`POST /ds/quest_progress`, 08.11): raid increments — monsters killed
    /// (KILL_TYPE, target 11), chests opened (LOOT, 41), damage to players (PLAYER_CAUSED_TOTAL_DAMAGE, 1). Only while
    /// the player's raid is open (raid_entry.applied = false); role — the raid role, level — the lobby choice (as for the raid result).
    /// The quest snapshot goes to the client immediately (it stays connected to the backend in a raid too). Ok(whether quests changed) / Err(reason).
    pub async fn quest_progress_from_ds(&self, open_id: &str, monsters: i64, chests: i64, damage: i64) -> Result<bool, NativeReject> {
        let mut p = self.db.get::<PlayerModel>("players", open_id).await.ok_or_else(|| reject(404, "no such player"))?;
        if p.state["raid_entry"]["applied"] != json!(false) {
            return Err(reject(409, "player's raid is closed"));
        }
        let role_guid = p.state["raid_entry"]["role_guid"].as_u64().unwrap_or(p.hall_role_guid);
        let lvl = Self::get_saved_level_select(&p);
        let mut changed = false;
        for (kind, value, entity) in [
            (tgf::EnQuestProgressType::KillType, monsters, 11),
            (tgf::EnQuestProgressType::Loot, chests, 41),
            (tgf::EnQuestProgressType::PlayerCausedTotalDamage, damage, 1),
        ] {
            if value <= 0 {
                continue;
            }
            let mut ev = QuestEvent::new(kind, value, role_guid);
            ev.level_type = Some(lvl.level_type as i64);
            ev.level_id = Some(lvl.level_id as i64);
            ev.entity = Some(entity);
            changed |= self.quest_event(&mut p, &ev);
        }
        // 09.11 player stats: monsters, chests, damage to players
        for (ty, value) in [(super::stats::KILL_MONSTER, monsters), (super::stats::OPEN_CHEST, chests), (super::stats::DAMAGE_TO_PLAYER, damage)] {
            super::stats::add(&mut p.state, role_guid, lvl.level_id as i64, ty, value);
        }
        if changed || monsters + chests + damage > 0 {
            self.db.save("players", open_id, &p).await;
        }
        if changed {
            crate::lobby::hub::push_to(open_id, crate::lobby::hub::DIRTY_METHOD, crate::lobby::hub::DIRTY_TYPE, &self.quest_dirty(&p));
        }
        println!("[Quest] {} from battle server: monsters {}, chests {}, damage to players {} — quests {}", open_id, monsters, chests, damage,
            if changed { "changed" } else { "unchanged" });
        Ok(changed)
    }

    /// Kill accounting; `reward` — season flag kill_reward (register_kill and the test)
    async fn count_kill(&self, killer: &str, victim: &str, reward: bool) -> Result<i64, NativeReject> {
        if killer.is_empty() || killer == victim {
            return Err(reject(400, "killer not specified or killed self"));
        }
        let mut k = self.db.get::<PlayerModel>("players", killer).await.ok_or_else(|| reject(404, "no such killer"))?;
        let v = self.db.get::<PlayerModel>("players", victim).await.ok_or_else(|| reject(404, "no such victim"))?;
        if crate::lobby::team::battle_team_id(k.user_id) == crate::lobby::team::battle_team_id(v.user_id) {
            return Err(reject(409, "friendly (same team)"));
        }
        // rating: only while the killer's raid is not closed (result not applied yet)
        let in_raid = k.state["raid_entry"]["applied"] == json!(false);
        let raid_n = if in_raid { super::rank::raid_kill_add(&mut k.state) } else { 0 };
        if !reward {
            self.db.save("players", killer, &k).await;
            println!("[Kill] {} killed {} — raid kills {}{}", killer, victim, raid_n, if in_raid { "" } else { " (outside a raid — not rated)" });
            return Ok(raid_n as i64);
        }
        let n = add_kill(&mut k.state);
        // the killer is already out of the raid (result applied: extracted before the server reported the death) — mail right away
        let out_of_raid = k.state["raid_entry"]["applied"] == json!(true);
        let send_now = if out_of_raid { take_kills(&mut k.state) } else { 0 };
        self.db.save("players", killer, &k).await;
        println!("[KillReward] {} killed {} — unclaimed kills {}{}", killer, victim, n, if send_now > 0 { ", outside a raid — mailed right away" } else { "" });
        if send_now > 0 {
            self.kill_reward_mail(&k, send_now).await;
        }
        Ok(n)
    }

    /// "Kill reward" mail: per_kill × kills of the kill_reward currency (season flag) attached
    async fn kill_reward_mail(&self, player: &PlayerModel, kills: i64) {
        let Some(r) = crate::season::features().kill_reward.clone() else { return };
        let amount = (kills * r.per_kill).clamp(1, i32::MAX as i64) as i32;
        let coin = tgf::DItem { basic: Some(tgf::DItemBasic { type_id: r.currency, count: amount, ..Default::default() }), ..Default::default() };
        let mail = super::progress_handlers::mail_system(
            "Награда за убийства",
            &format!("Убито игроков за рейд: {}. Награда: {}.", kills, amount),
            vec![coin],
        );
        super::progress_handlers::mail_send(&self.db, &self.game_data, &player.open_id, mail).await;
    }

    /// Results screen (`battle_settle_sync`) — as in the dedicated server's `player_game_over`: what the server took off the pawn,
    /// exp before/after and gold for the sold extracted loot. At this moment the player is still in the raid, the client keeps
    /// the connection to the backend and shows the results on returning to the lobby.
    fn raid_push_settle(
        &self,
        player: &PlayerModel,
        role_guid: u64,
        actor_status: i32,
        deploy: &tgf::DBattleDeployRole,
        token: &str,
        (old_level, old_exp): (i32, i32),
        gold: i32,
        give_back_items: Vec<tgf::DItem>,
    ) -> tgf::ScBattleSettleSync {
        let (level, exp) = role_exp_of(player, role_guid);
        let (level_param, map_id) = self.build_level_and_map_params(&Self::get_saved_level_select(player));
        let add_exp = player.state["raid_last_exp"].as_i64().unwrap_or(0) as i32;
        let msg = tgf::ScBattleSettleSync {
            result: Some(tgf::DBattlePlayerGameOver {
                user_key: Some(tgf::DPlayerIdKey { user_id: player.user_id, zone_id: 1 }),
                role_guid: role_guid as i64,
                map_data: Some(tgf::DBattleMapData { map_id }),
                ds_result: Some(tgf::DDsUserBattleResult {
                    ds_user_key: player.open_id.clone(),
                    game_type: GAME_STATUS_GAMEOVER,
                    actor_type: actor_status,
                    player_wearing: deploy.wearing.clone(),
                    player_backpack: deploy.backpack.clone(),
                    player_small_backpack: deploy.small_backpack.clone(),
                    batle_info: Some(tgf::DDsUserBattleInfo { add_exp, ..Default::default() }),
                    token: token.to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            gs_result: Some(tgf::DGsUserSettleResult {
                old_exp,
                old_level,
                curr_exp: exp,
                curr_level: level,
                // map_detail.settle_interface_type: PVE for dungeons (the table is not loaded into game_data)
                settle_interface_type: tgf::DSettleInterFaceType::EnSettleInterfaceTypePve as i32,
                gain_gold_out_of_portal: gold,
                give_back_items,
                val_changed: true,
                ..Default::default()
            }),
            level_parameter: Some(level_param),
        };
        crate::lobby::hub::push_to(&player.open_id, "tgf.GameClientService.battle_settle_sync", "tgf.SCBattleSettleSync", &msg);
        println!("[RaidNative] {}: battle_settle_sync (level {}→{}, exp +{}, gold {})", player.open_id, old_level, level, add_exp, gold);
        last_settle().lock().unwrap().insert(player.open_id.clone(), msg.clone());
        msg
    }
}

/// The player's last battle_settle_sync — for GM `settle last` (Sh4g, 05.11: does the client accept our result in the lobby)
pub(crate) fn last_settle() -> &'static std::sync::Mutex<std::collections::HashMap<String, tgf::ScBattleSettleSync>> {
    static S: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, tgf::ScBattleSettleSync>>> =
        std::sync::OnceLock::new();
    S.get_or_init(Default::default)
}

/// The player's unclaimed raid kills (state.kills_pending) — mailed at their result
fn kills_pending(state: &serde_json::Value) -> i64 {
    state["kills_pending"].as_i64().unwrap_or(0)
}

fn add_kill(state: &mut serde_json::Value) -> i64 {
    let n = kills_pending(state) + 1;
    state["kills_pending"] = json!(n);
    n
}

fn take_kills(state: &mut serde_json::Value) -> i64 {
    let n = kills_pending(state);
    state["kills_pending"] = json!(0);
    n
}

/// (level, exp) of a role
fn role_exp_of(player: &PlayerModel, role_guid: u64) -> (i32, i32) {
    player
        .roles
        .iter()
        .find(|r| r.role_guid == role_guid)
        .and_then(|r| r.data.deploy_role.as_ref())
        .map(|d| (d.role_level.max(1), d.exp))
        .unwrap_or((1, 0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::RoleModel;

    /// Sh4: a player-on-player kill accumulates on the killer; self and same-team kills do not count
    #[tokio::test]
    async fn kills_counted_for_killer() {
        let dir = format!("target/test_kill_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        let s = GatewaySession {
            db: db.clone(),
            game_data: std::sync::Arc::new(GameData::load_all("ExcelConfig")),
            open_id: "k".into(),
            peer_ip: "test".into(),
            after_pushes: Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        db.save("players", "kill-a", &PlayerModel::new("kill-a", 19_000_001)).await;
        db.save("players", "kill-b", &PlayerModel::new("kill-b", 19_000_002)).await;
        assert_eq!(s.count_kill("kill-a", "kill-b", true).await.ok(), Some(1));
        assert_eq!(s.count_kill("kill-a", "kill-b", true).await.ok(), Some(2));
        assert!(s.count_kill("kill-a", "kill-a", true).await.is_err(), "self");
        assert!(s.count_kill("kill-a", "nobody", true).await.is_err(), "no victim");
        let mut a = db.get::<PlayerModel>("players", "kill-a").await.unwrap();
        assert_eq!(take_kills(&mut a.state), 2);
        assert_eq!(kills_pending(&a.state), 0);
        // rating: without the reward flag raid kills are still counted (raid_entry.kills), no reward
        a.state["raid_entry"] = json!({ "applied": false });
        db.save("players", "kill-a", &a).await;
        assert_eq!(s.count_kill("kill-a", "kill-b", false).await.ok(), Some(1));
        assert_eq!(s.count_kill("kill-a", "kill-b", true).await.ok(), Some(1), "with reward — unclaimed too");
        let a = db.get::<PlayerModel>("players", "kill-a").await.unwrap();
        assert_eq!(super::super::rank::raid_kills(&a.state), 2);
        assert_eq!(kills_pending(&a.state), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Extraction with a teammate's items: gear goes by mail, a shared find stays, found items get the team marker
    #[tokio::test]
    async fn team_gear_returns_by_mail() {
        let dir = format!("target/test_return_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        let game_data = std::sync::Arc::new(GameData::load_all("ExcelConfig"));
        let gear_id = *game_data.item_type.iter().find(|(_, t)| t.item_major_type == tgf::EnItemMajorType::Equipment as i32).unwrap().0;
        let s = GatewaySession {
            db: db.clone(),
            game_data,
            open_id: "ret-me".into(),
            peer_ip: "test".into(),
            after_pushes: Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let item = |guid: i64, x: i32| tgf::DItem {
            basic: Some(tgf::DItemBasic { type_id: gear_id, count: 1, guid, position: Some(tgf::DItemPosition { x, ..Default::default() }), ..Default::default() }),
            ..Default::default()
        };
        // 201 own off-hand, 101 teammate's main weapon, 102 shared find, 103 looted from an enemy
        let mut wearing = vec![item(101, 1), item(201, 2)];
        let mut backpack = vec![item(102, 0), item(103, 1)];
        let mut small = vec![];
        let mut me = PlayerModel::new("ret-me", 19_100_001);
        raid_gear_record(&mut me, 1, "b1", &["ret-mate".to_string()]);
        me.state["raid_gear"][0]["gear"] = json!([{ "g": 201, "o": [] }]);
        let mut mate = PlayerModel::new("ret-mate", 19_100_002);
        mate.state["raid_gear"] = json!([{ "battle": "b1", "team": ["ret-me"], "gear": [
            { "g": 101, "o": [] }, { "g": 102, "o": [19_100_001, 19_100_002] }] }]);
        db.save("players", "ret-mate", &mate).await;

        let issued: HashSet<i64> = [201].into_iter().collect();
        let r = s.raid_team_returns(&me, "b1", &issued, &mut wearing, &mut backpack, &mut small).await;

        assert!(wearing.is_empty(), "the off-hand left together with the main weapon");
        assert_eq!(r.off_hand.iter().map(guid_of).collect::<Vec<_>>(), vec![201]);
        assert_eq!(r.to_mates.len(), 1);
        assert_eq!(r.to_mates[0].0, "ret-mate");
        assert_eq!(r.to_mates[0].1.iter().map(guid_of).collect::<Vec<_>>(), vec![101]);
        assert_eq!(owner_ids(&r.to_mates[0].1[0]), vec![19_100_002], "returned item marked only by its owner");
        assert_eq!(backpack.iter().map(guid_of).collect::<Vec<_>>(), vec![102, 103]);
        assert_eq!(owner_ids(&backpack[1]), vec![19_100_001, 19_100_002], "found item — team marker");

        // solo: an item looted from an enemy stays without the old marker
        let mut enemy_item = item(301, 0);
        set_owners(&mut enemy_item, &[777]);
        let mut solo = PlayerModel::new("ret-solo", 19_100_003);
        raid_gear_record(&mut solo, 1, "b2", &[]);
        let (mut w, mut b, mut sm) = (vec![], vec![enemy_item], vec![]);
        let r = s.raid_team_returns(&solo, "b2", &HashSet::new(), &mut w, &mut b, &mut sm).await;
        assert!(r.to_mates.is_empty());
        assert_eq!(b.len(), 1, "gear from an enemy is extracted");
        assert!(owner_ids(&b[0]).is_empty());
        drop(s);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Gear record: re-issue does not change it, old matches are evicted
    #[test]
    fn gear_record_keeps_last_battles() {
        let mut p = PlayerModel::new("rec", 1);
        raid_gear_record(&mut p, 1, "b0", &["m".to_string()]);
        raid_gear_record(&mut p, 1, "b0", &[]);
        assert_eq!(raid_gear_of(&p, "b0").unwrap().0, vec!["m".to_string()]);
        for i in 1..=RAID_GEAR_KEEP {
            raid_gear_record(&mut p, 1, &format!("b{}", i), &[]);
        }
        assert!(raid_gear_of(&p, "b0").is_none());
        assert!(raid_gear_of(&p, &format!("b{}", RAID_GEAR_KEEP)).is_some());
    }

    /// 13.11 returning items from a match: battle server record -> worn and backpack, without those already owned; another role and a bad name — rejected
    #[test]
    fn raid_restore_items_from_gameover_record() {
        std::env::set_var("RESTORE_DIR", concat!(env!("CARGO_MANIFEST_DIR"), "/src/rpc/handlers/testdata"));
        let rec = tgf::DBattleUserData::decode(&include_bytes!("testdata/gameover_host_091057.bin")[..]).unwrap();
        let deploy = rec.deploy_preview.clone().unwrap().deploy_role.unwrap();
        let role_guid = deploy.basic.as_ref().unwrap().role_guid as u64;
        let all = items_of(&deploy.wearing).len() + items_of(&deploy.backpack).len();
        let mut p = PlayerModel::new("restore-test", 10949022);
        p.roles.push(RoleModel { role_guid, ord: 1, type_id: 400006, role_name: "Char5".into(), role_level: 1, data: Default::default() });
        let (role, items) = raid_restore_items(&p, "gameover_host_091057.bin").unwrap();
        assert_eq!(role, role_guid);
        assert_eq!(items.len(), all, "the player has nothing from the record — everything to return");
        // one item is already in the player's stash — it is not returned
        p.stash.element.push(items[0].clone());
        assert_eq!(raid_restore_items(&p, "gameover_host_091057.bin").unwrap().1.len(), all - 1);
        assert!(raid_restore_items(&p, "../secret.bin").is_err());
        assert!(raid_restore_items(&PlayerModel::new("other", 1), "gameover_host_091057.bin").is_err(), "another role");
    }

    /// 13.11 (dupe): leaving the hall, then a raid result after all — found loot is not given, items given away in the raid are removed from home
    #[tokio::test]
    async fn late_result_after_hall_leave_removes_given_away() {
        let rec = tgf::DBattleUserData::decode(&include_bytes!("testdata/gameover_host_091057.bin")[..]).unwrap();
        let deploy = rec.deploy_preview.clone().unwrap().deploy_role.unwrap();
        let role_guid = deploy.basic.as_ref().unwrap().role_guid as u64;
        let dir = format!("target/test_late_hall_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        let s = GatewaySession {
            db: db.clone(),
            game_data: std::sync::Arc::new(GameData::load_all("ExcelConfig")),
            open_id: "late-hall".into(),
            peer_ip: "test".into(),
            after_pushes: Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let mut p = PlayerModel::new("late-hall", 10949021);
        p.hall_role_guid = role_guid;
        p.roles.push(RoleModel {
            role_guid,
            ord: 1,
            type_id: 400006,
            role_name: "Char5".into(),
            role_level: 1,
            data: tgf::DUserRole { deploy_role: Some(deploy.clone()), ..Default::default() },
        });
        GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None);
        db.save("players", "late-hall", &p).await;
        let home_before = items_of(&deploy.wearing).len();
        assert!(home_before >= 2, "at least two worn items in the snapshot");

        // drop at the end of the hall timer: result "left the hall" — items at home
        let r = s.raid_apply_native_result_in("late-hall", 3, 1, "s1", rec.clone(), true, 0).await.ok().unwrap();
        assert_eq!(r["applied"], "hall_leave");

        // the player still entered the raid and gave away the first worn item: it is not in the extraction result
        let mut late = rec.clone();
        let given = {
            let w = late.deploy_preview.as_mut().unwrap().deploy_role.as_mut().unwrap().wearing.as_mut().unwrap();
            guid_of(&w.items.remove(0))
        };
        let r = s.raid_apply_native_result("late-hall", 3, ACTOR_ESCAPE, "s1", late.clone()).await.ok().unwrap();
        assert_eq!(r["applied"], "late_after_hall");
        assert_eq!(r["removed"], 1);
        let p = db.get::<PlayerModel>("players", "late-hall").await.unwrap();
        let w = p.roles[0].data.deploy_role.clone().unwrap().wearing.unwrap().items;
        assert!(w.iter().all(|i| guid_of(i) != given), "the given-away item is removed from home");
        assert_eq!(w.len(), home_before - 1, "the rest of the worn gear is in place");
        assert_eq!(s.raid_apply_native_result("late-hall", 3, ACTOR_ESCAPE, "s1", late).await.err().map(|e| e.0), Some(409), "once");
        drop(s);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A real server snapshot 02.11 09:10:57 (host record, role Char5 1094902005): death, extraction, repeat, foreign token
    #[tokio::test]
    async fn native_result_applies_once() {
        let rec = tgf::DBattleUserData::decode(&include_bytes!("testdata/gameover_host_091057.bin")[..]).unwrap();
        let deploy = rec.deploy_preview.clone().unwrap().deploy_role.unwrap();
        let role_guid = deploy.basic.as_ref().unwrap().role_guid as u64;
        let worn = deploy.wearing.as_ref().map(|w| w.items.len()).unwrap_or(0);
        assert!(worn > 0, "the snapshot has worn items");

        let dir = format!("target/test_native_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        let game_data = std::sync::Arc::new(GameData::load_all("ExcelConfig"));
        let s = GatewaySession {
            db: db.clone(),
            game_data,
            open_id: "native-test".into(),
            peer_ip: "test".into(),
            after_pushes: Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let fresh = || {
            let mut p = PlayerModel::new("native-test", 10000902);
            p.hall_role_guid = role_guid;
            p.roles.push(RoleModel {
                role_guid,
                ord: 1,
                type_id: 400006,
                role_name: "Char5".into(),
                role_level: 1,
                data: tgf::DUserRole { deploy_role: Some(deploy.clone()), ..Default::default() },
            });
            p
        };

        // without an issue — nothing to apply to
        db.save("players", "native-test", &fresh()).await;
        assert_eq!(s.raid_apply_native_result("native-test", 3, 2, "s1", rec.clone()).await.err().map(|e| e.0), Some(409));

        // death: worn and backpack lost, small backpack stayed, repeat → 409
        let mut p = fresh();
        let token = GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None);
        assert_eq!(GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None), token, "reconnect — same token");
        db.save("players", "native-test", &p).await;
        let r = s.raid_apply_native_result("native-test", 3, 2, "s1", rec.clone()).await.ok().unwrap();
        assert_eq!(r["applied"], "death");
        let p = db.get::<PlayerModel>("players", "native-test").await.unwrap();
        let d = p.roles[0].data.deploy_role.clone().unwrap();
        // 08.11: instead of the lost items — the class's grey starter kit
        let (kit_wear, kit_bag) = s.generate_starter_items(p.roles[0].type_id as i32, role_guid as i64);
        let types = |v: &[tgf::DItem]| v.iter().map(|i| i.basic.as_ref().map_or(0, |b| b.type_id)).collect::<Vec<_>>();
        assert_eq!(types(&d.wearing.unwrap().items), types(&kit_wear));
        assert_eq!(types(&d.backpack.unwrap().items), types(&kit_bag));
        assert_eq!(s.raid_apply_native_result("native-test", 3, 2, "s1", rec.clone()).await.err().map(|e| e.0), Some(409), "exactly once");
        let p2 = db.get::<PlayerModel>("players", "native-test").await.unwrap();
        assert!(GatewaySession::raid_finished_in_session(&p2, "s1"), "re-entering the same session");
        assert!(!GatewaySession::raid_finished_in_session(&p2, "s2"), "new raid — allowed");

        // 09.11: "Hand of Justice" (status 16 — kicked, game over immediately) — as death: grey kit instead of items
        let mut p = fresh();
        GatewaySession::raid_entry_issue(&mut p, role_guid, "s9", None);
        db.save("players", "native-test", &p).await;
        let r = s.raid_apply_native_result("native-test", 3, 16, "s9", rec.clone()).await.ok().unwrap();
        assert_eq!(r["applied"], "ban_kick");
        let d = db.get::<PlayerModel>("players", "native-test").await.unwrap().roles[0].data.deploy_role.clone().unwrap();
        assert_eq!(types(&d.wearing.unwrap().items), types(&kit_wear), "a banned player loses worn gear");

        // insurance (08.11): an insured worn item stays with the owner after death, insurance expires,
        // the amount paid is recorded for the premium price
        let mut p = fresh();
        let ins = {
            let w = p.roles[0].data.deploy_role.as_mut().unwrap().wearing.as_mut().unwrap();
            let it = &mut w.items[0];
            it.data.get_or_insert_with(Default::default).insurance = Some(tgf::DItemInsurance { active: true, value: 777, cost_item: None });
            (guid_of(it), inventory_handlers::pos_of(it))
        };
        GatewaySession::raid_entry_issue(&mut p, role_guid, "s7", None);
        db.save("players", "native-test", &p).await;
        let r = s.raid_apply_native_result("native-test", 3, 2, "s7", rec.clone()).await.ok().unwrap();
        assert_eq!(r["applied"], "death");
        let p = db.get::<PlayerModel>("players", "native-test").await.unwrap();
        let w = p.roles[0].data.deploy_role.clone().unwrap().wearing.unwrap().items;
        let kept = w.iter().find(|i| guid_of(i) == ins.0).expect("insured item stayed");
        assert!(kept.data.as_ref().and_then(|d| d.insurance.as_ref()).is_none(), "insurance cleared");
        assert_eq!(inventory_handlers::pos_of(kept).x, ins.1.x, "in its slot");
        assert_eq!(w.iter().filter(|i| inventory_handlers::pos_of(i).x == ins.1.x).count(), 1, "the kit item from this slot removed");
        let paid = db.get::<InsurancePaid>(INSURANCE_PAID, &ins.0.to_string()).await.unwrap();
        assert_eq!(paid.value, 777);

        // ascension potion (10.11): exp only on extracting alive; on death — none; the field is reset after granting
        for (actor, potion) in [(3, true), (2, false)] {
            let mut p = fresh();
            GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None);
            db.save("players", "native-test", &p).await;
            s.raid_apply_native_result_in("native-test", 3, actor, "s1", rec.clone(), false, 1500).await.ok().unwrap();
            let p = db.get::<PlayerModel>("players", "native-test").await.unwrap();
            let got = p.state["raid_last_exp"].as_i64().unwrap_or(0);
            if potion {
                assert!(got >= 1500, "extraction: potion exp in the result ({})", got);
            } else {
                assert!(got < 1500, "death: no potion exp ({})", got);
            }
            assert_eq!(p.state["raid_bonus_exp"], 0, "outcome {}: the potion counts once", actor);
        }

        // extraction: worn gear in place (own items are not sold)
        let mut p = fresh();
        GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None);
        db.save("players", "native-test", &p).await;
        let r = s.raid_apply_native_result("native-test", 3, 3, "s1", rec.clone()).await.ok().unwrap();
        assert_eq!(r["applied"], "extraction");
        let p = db.get::<PlayerModel>("players", "native-test").await.unwrap();
        assert_eq!(p.roles[0].data.deploy_role.clone().unwrap().wearing.unwrap().items.len(), worn);

        // not GAMEOVER and a foreign token — rejected
        let mut p = fresh();
        GatewaySession::raid_entry_issue(&mut p, role_guid, "s1", None);
        db.save("players", "native-test", &p).await;
        assert_eq!(s.raid_apply_native_result("native-test", 2, 4, "s1", rec.clone()).await.err().map(|e| e.0), Some(400));
        let mut other = rec.clone();
        other.ds_player_key.get_or_insert_with(Default::default).ds_token = "foreign".into();
        assert_eq!(s.raid_apply_native_result("native-test", 3, 3, "s1", other).await.err().map(|e| e.0), Some(409));
        drop(s);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
