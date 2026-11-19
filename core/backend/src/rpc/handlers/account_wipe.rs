//! 16.11 Full account reset (GM `wipe`) — for investigations (dupes, cheats). The account becomes as new:
//! no characters, stash, gold, coins, quests, appearances, mail, auction orders or season ranking places.
//! Kept: login binding (Discord, open_id, user_id), ban, guild, friends, client settings, role guid counter.
//!
//! Before the reset everything is saved to the `player_wipes` table (key `<open_id>_<unix>`): the whole player record, the mailbox,
//! the removed market orders. `unwipe <key>` restores the player record and mail from the snapshot (if the decision is reversed);
//! orders and ranking places are not restored — their contents are in the snapshot.
//!
//! Protection against mistakes: the second word must be the player's user_id (`wipe 11000018 reason`) — the admin API without open_id takes
//! the only online player. Player in a match (room still alive) — rejected: the raid result would write the items back.

use super::GatewaySession;
use crate::structures::PlayerModel;
use serde::{Deserialize, Serialize};

const WIPES: &str = "player_wipes";
/// What in state survives the reset: guild (the guild also stores the membership), UDP tunnel, client settings
const KEEP_STATE: [&str; 4] = ["guild", "udp_tunnel", "player_setting_bytes", "player_private_options"];

#[derive(Serialize, Deserialize, Clone)]
pub(super) struct WipeSnapshot {
    pub at: i64,
    pub by: String,
    pub reason: String,
    pub player: PlayerModel,
    pub mail_next_id: i64,
    /// tgf::ScMailGetAllRsp (protobuf)
    pub mail: Vec<u8>,
    /// Removed market orders (protobuf TradeOrder from trade_handlers)
    pub trade_orders: Vec<Vec<u8>>,
}

/// A fresh account record in place of `p`: as for a new player, plus what the reset does not touch
pub(super) fn wiped(p: &PlayerModel) -> PlayerModel {
    let mut n = PlayerModel::new(&p.open_id, p.user_id);
    n.next_guid = p.next_guid; // guids of new roles do not repeat old ones (ranking, insurance)
    for k in KEEP_STATE {
        if let Some(v) = p.state.get(k).filter(|v| !v.is_null()) {
            n.state[k] = v.clone();
        }
    }
    n
}

fn gold_of(p: &PlayerModel) -> i64 {
    p.state["gold"].as_i64().unwrap_or(0)
}

fn items_of(p: &PlayerModel) -> usize {
    let on_roles: usize = p
        .roles
        .iter()
        .filter_map(|r| r.data.deploy_role.as_ref())
        .map(|d| {
            d.wearing.as_ref().map_or(0, |w| w.items.len())
                + d.backpack.as_ref().map_or(0, |b| b.items.len())
                + d.small_backpack.as_ref().map_or(0, |b| b.items.len())
        })
        .sum();
    on_roles + p.stash.element.len()
}

impl GatewaySession {
    /// `wipe <user_id> [reason]` on the player of session `p`
    pub(super) async fn gm_wipe(&self, p: &PlayerModel, args: &[String], actor: &str) -> Result<Vec<String>, String> {
        let usage = format!("wipe <user_id> [reason] — to confirm, the user_id of this player: {} ({})", p.user_id, p.open_id);
        if args.get(1).and_then(|s| s.parse::<u64>().ok()) != Some(p.user_id) {
            return Err(usage);
        }
        if let Some(room) = crate::lobby::matchmaking::room_of_player(&p.open_id) {
            return Err(format!("player is in a match (room {}) — wait for the raid result", room));
        }
        let reason = args[2..].join(" ");
        let kicked = crate::auth::registry::kick(&p.open_id);
        // snapshot before any changes except the market: orders are removed under the book lock and put into the snapshot right away
        let (mail_next_id, mail, mails) = super::progress_handlers::mail_snapshot(&self.db, &p.open_id).await;
        let trade_orders = self.trade_wipe_user(p.user_id).await?;
        let at = Self::now_ts();
        let key = format!("{}_{}", p.open_id, at);
        let snap = WipeSnapshot {
            at,
            by: actor.to_string(),
            reason: reason.clone(),
            player: p.clone(),
            mail_next_id,
            mail,
            trade_orders,
        };
        self.db.save(WIPES, &key, &snap).await;
        if self.db.get::<WipeSnapshot>(WIPES, &key).await.is_none() {
            return Err(format!("snapshot {} was not written — account untouched (market orders already removed: {})", key, snap.trade_orders.len()));
        }

        let fresh = wiped(p);
        super::progress_handlers::mail_replace(&self.db, &p.open_id, mail_next_id, &[]).await?;
        self.db.save("players", &p.open_id, &fresh).await;
        let roles: Vec<u64> = p.roles.iter().map(|r| r.role_guid).collect();
        let boards = self.rank_forget(p.user_id, &roles).await;
        let names: Vec<&str> = p.roles.iter().map(|r| r.role_name.as_str()).collect();
        println!(
            "[Wipe] {} reset {} (user_id {}): characters {} {:?}, items {}, gold {}, mails {}, orders {}; snapshot {}; reason: {}",
            actor,
            p.open_id,
            p.user_id,
            p.roles.len(),
            names,
            items_of(p),
            gold_of(p),
            mails,
            snap.trade_orders.len(),
            key,
            reason
        );
        Ok(vec![
            format!("account {} (user_id {}) reset, sessions closed: {}", p.open_id, p.user_id, kicked),
            format!("removed: characters {} {:?}, items {}, gold {}, mails {}, market orders {}", p.roles.len(), names, items_of(p), gold_of(p), mails, snap.trade_orders.len()),
            format!("removed from ranking tables: {}", boards),
            format!("snapshot for the investigation: {} {} (restore: unwipe {})", WIPES, key, key),
        ])
    }

    /// `unwipe <snapshot key>` — restore the player record and mail from the snapshot (the session player must match the snapshot)
    pub(super) async fn gm_unwipe(&self, p: &PlayerModel, args: &[String], actor: &str) -> Result<Vec<String>, String> {
        let Some(key) = args.get(1) else { return Err("unwipe <snapshot key from wipe>".into()) };
        let Some(snap) = self.db.get::<WipeSnapshot>(WIPES, key).await else { return Err(format!("no snapshot {}", key)) };
        if snap.player.open_id != p.open_id {
            return Err(format!("snapshot {} belongs to player {}, but the command targets {}", key, snap.player.open_id, p.open_id));
        }
        if let Some(room) = crate::lobby::matchmaking::room_of_player(&p.open_id) {
            return Err(format!("player is in a match (room {}) — wait for the raid result", room));
        }
        let kicked = crate::auth::registry::kick(&p.open_id);
        super::progress_handlers::mail_replace(&self.db, &p.open_id, snap.mail_next_id, &snap.mail).await?;
        // guild, ban and settings — the current ones (they may have changed after the reset)
        let mut back = snap.player.clone();
        for k in KEEP_STATE {
            match p.state.get(k).filter(|v| !v.is_null()) {
                Some(v) => back.state[k] = v.clone(),
                None => {
                    let _ = back.state.as_object_mut().map(|o| o.remove(k));
                }
            }
        }
        self.db.save("players", &p.open_id, &back).await;
        println!("[Wipe] {} restored {} from snapshot {} (reset by {} at {})", actor, p.open_id, key, snap.by, snap.at);
        Ok(vec![
            format!("account {} restored from {}: characters {}, items {}, gold {}; sessions closed: {}", p.open_id, key, back.roles.len(), items_of(&back), gold_of(&back), kicked),
            format!("not restored: market orders ({}) and ranking places — the order contents are in the snapshot", snap.trade_orders.len()),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::RoleModel;
    use crate::tgf;
    use serde_json::json;

    #[test]
    fn wiped_is_new_account_with_kept_links() {
        let mut p = PlayerModel::new("76561197960265999", 11000099);
        p.next_guid = 7;
        p.hall_role_guid = 1100009901;
        p.roles.push(RoleModel { role_guid: 1100009901, ord: 0, type_id: 400004, role_name: "Dupe".into(), role_level: 20, data: tgf::DUserRole::default() });
        p.stash.element.push(Default::default());
        p.state["gold"] = json!(999_999);
        p.state["coins"] = json!({ "2001": 50 });
        p.state["guild"] = json!({ "guild_id": 5 });
        p.state["player_setting_bytes"] = json!("abc");
        p.state["raid_entry"] = json!({ "applied": false });

        let n = wiped(&p);
        assert_eq!((n.open_id.as_str(), n.user_id, n.next_guid, n.hall_role_guid), ("76561197960265999", 11000099, 7, 0));
        assert!(n.roles.is_empty() && n.stash.element.is_empty());
        assert_eq!(n.state["gold"], PlayerModel::new("x", 1).state["gold"]);
        assert!(n.state.get("coins").is_none() && n.state.get("raid_entry").is_none());
        assert_eq!(n.state["guild"], json!({ "guild_id": 5 }));
        assert_eq!(n.state["player_setting_bytes"], json!("abc"));
        assert_eq!(items_of(&p), 1);
    }
}
