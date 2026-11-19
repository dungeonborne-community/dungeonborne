//! 16.11 (dupe via the menu): while a character's raid is not closed by a result and the match in its room is running, that character's items
//! (worn, backpack, small backpack) cannot be changed from the lobby. Otherwise a player who crashed or restarted the game mid-raid
//! moved gear to the stash, sold or insured it while someone from the party was alive; the death result then cleared already
//! empty slots (raid_native: only worn gear and the backpack are cleared) — the item stayed with him and also lay on the corpse in the raid.
//! Stash, other characters, trade and the rest of the lobby stay available. Matches without losses (own game "pr…", arenas)
//! and the waiting hall are not locked: their result does not touch items in the database.
//! - inventory_action / item_action: compare before and after the action, reject with -626 and a window with the reason;
//! - save_me (common.rs): the same against the copy in the database — for other paths (selling, auction, quests, mail).

use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use serde_json::json;

/// Reject code: EN_ERR_ITEM_TYPE_IN_BATTLE_NOT_ALLOW_CHANGE_ITEM (the client does not show it itself — we send the window)
const ERR_IN_BATTLE: tgf::EnErrorCode = tgf::EnErrorCode::EnErrItemTypeInBattleNotAllowChangeItem;

/// Character in an open raid: the raid loadout is not applied, the player's room is alive, its match has started and the result takes items away.
pub(super) fn raid_locked_role(p: &PlayerModel) -> Option<u64> {
    let e = &p.state["raid_entry"];
    if e["applied"] != json!(false) {
        return None;
    }
    let role = e["role_guid"].as_u64()?;
    let room = crate::lobby::matchmaking::room_of_player(&p.open_id)?;
    (!keeps_items(&room) && crate::lobby::allocator::battle_started(&room)).then_some(role)
}

/// Match without losses: own game ("pr…") or arena — the result does not touch items in the database (raid_native → raid_hall_leave)
pub(super) fn keeps_items(battle_id: &str) -> bool {
    battle_id.starts_with("pr") || crate::lobby::matchmaking::room(battle_id).is_some_and(|r| r.arena_map_id.is_some())
}

type RaidItems = (Option<tgf::DRoleWearingData>, Option<tgf::DRoleBackpackData>, Option<tgf::DRoleBackpackData>);

/// Character items that go into the raid and are removed by the result: worn, backpack, small backpack
fn raid_items(p: &PlayerModel, role: u64) -> Option<Option<RaidItems>> {
    let r = p.roles.iter().find(|r| r.role_guid == role)?;
    Some(r.data.deploy_role.as_ref().map(|d| (d.wearing.clone(), d.backpack.clone(), d.small_backpack.clone())))
}

/// Items of character `role` differ (move, take off, sale, insurance, character deletion)
pub(super) fn raid_items_differ(before: &PlayerModel, after: &PlayerModel, role: u64) -> bool {
    raid_items(before, role) != raid_items(after, role)
}

/// The action changes items of a character currently in a raid: Some(role)
pub(super) fn raid_items_changed(before: &PlayerModel, after: &PlayerModel) -> Option<u64> {
    let role = raid_locked_role(before)?;
    raid_items_differ(before, after, role).then_some(role)
}

impl GatewaySession {
    /// Reject an action on items of a character in a raid: log, code in the response header, a window with the reason
    pub(super) fn raid_lock_refuse(&self, what: &str, role: u64) {
        println!("[RaidLock] {} {}: rejected — character {} is in a match, only the raid result changes its items", self.clean_open_id(), what, role);
        self.set_rpc_error(ERR_IN_BATTLE as i32);
        self.push(
            "tgf.GameClientService.modal_message",
            "tgf.SCModalMessageSync",
            tgf::ScModalMessageSync {
                title: "Персонаж в бою".into(),
                content: "Бой твоей команды ещё идёт: снаряжение и рюкзак этого персонажа изменятся только после итога боя. \
                          Склад и другие персонажи доступны."
                    .into(),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::RoleModel;

    fn item(guid: i64) -> tgf::DItem {
        tgf::DItem { basic: Some(tgf::DItemBasic { guid, type_id: 100001, count: 1, ..Default::default() }), ..Default::default() }
    }

    fn player(role: u64, wear: Vec<tgf::DItem>, bag: Vec<tgf::DItem>) -> PlayerModel {
        let mut p = PlayerModel::new("76561197960265999", 10001);
        let mut data = tgf::DUserRole::default();
        data.deploy_role = Some(tgf::DBattleDeployRole {
            wearing: Some(tgf::DRoleWearingData { items: wear }),
            backpack: Some(tgf::DRoleBackpackData { items: bag, row_size: 7, column_size: 10 }),
            ..Default::default()
        });
        p.roles.push(RoleModel { role_guid: role, ord: 0, type_id: 400004, role_name: "t".into(), role_level: 1, data });
        p
    }

    #[test]
    fn raid_items_differ_on_any_change_of_the_raid_role() {
        let before = player(7, vec![item(1)], vec![item(2)]);
        assert!(!raid_items_differ(&before, &before.clone(), 7));
        // took worn gear off into the backpack
        let after = player(7, vec![], vec![item(2), item(1)]);
        assert!(raid_items_differ(&before, &after, 7));
        // removed from the backpack (to the stash / sold)
        let after = player(7, vec![item(1)], vec![]);
        assert!(raid_items_differ(&before, &after, 7));
        // insured an item in the backpack
        let mut after = before.clone();
        let it = &mut after.roles[0].data.deploy_role.as_mut().unwrap().backpack.as_mut().unwrap().items[0];
        it.data = Some(tgf::DItemData { insurance: Some(Default::default()), ..Default::default() });
        assert!(raid_items_differ(&before, &after, 7));
        // deleted the character
        let mut after = before.clone();
        after.roles.clear();
        assert!(raid_items_differ(&before, &after, 7));
        // stash and another character — not affected
        let mut after = before.clone();
        after.stash.element.push(Default::default());
        assert!(!raid_items_differ(&before, &after, 7));
    }

    #[test]
    fn no_lock_without_open_raid() {
        let mut p = player(7, vec![item(1)], vec![]);
        assert_eq!(raid_locked_role(&p), None);
        p.state["raid_entry"] = json!({ "applied": "death", "role_guid": 7 });
        assert_eq!(raid_locked_role(&p), None);
        // the loadout is open, but the player has no room (the match ended or the server crashed) — do not lock
        p.state["raid_entry"] = json!({ "applied": false, "role_guid": 7 });
        assert_eq!(raid_locked_role(&p), None);
    }
}
