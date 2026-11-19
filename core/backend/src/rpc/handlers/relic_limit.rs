//! 12.11 (user's decision): a character wears at most one relic of each kind — weapon (including a shield and an off-hand
//! "relic"), clothing, jewelry; 3 in total. Always, in a match too.
//! - Lobby: after any inventory action (inventory_handlers::inventory_action) — if the character now has two
//!   relics of one kind, the action is rolled back, the client gets -3836 (EnErrGearHeirloomWearingExist).
//! - Match: the server mod (DungeonborneRuServer, RelicLimit) moves the extra relic into the bag.
//! - Raid exit: if a kind is still duplicated (the mod was too late), the extra goes to the backpack, otherwise to the stash (take_off_extra).
//! Relic kind — UeSource_ItemType.item_type of item 220xxxxxx from the Heirloom folder (1 weapon, 2 clothing, 3 jewelry).

use super::inventory_handlers::{self as inv, ItemShape, BACKPACK, STASH};
use super::GatewaySession;
use crate::structures::PlayerModel;
use crate::tgf;
use std::collections::BTreeMap;

pub(super) const KIND_NAMES: [&str; 4] = ["", "weapon", "clothing", "jewelry"];

impl GatewaySession {
    /// Relic kind (1 weapon, 2 clothing, 3 jewelry); None — not a relic
    pub(super) fn relic_kind(&self, type_id: i32) -> Option<i32> {
        if type_id / 10_000_000 != 22 {
            return None;
        }
        let u = self.game_data.uesource_itemtype.get(&type_id)?;
        (u.ue_path.contains("Heirloom") && (1..=3).contains(&u.item_type)).then_some(u.item_type)
    }
}

/// Worn relics: (role, kind) → guids in slot order (x)
pub(super) fn worn_relics(p: &PlayerModel, kind_of: &dyn Fn(i32) -> Option<i32>) -> BTreeMap<(u64, i32), Vec<i64>> {
    let mut out: BTreeMap<(u64, i32), Vec<(i32, i64)>> = BTreeMap::new();
    for r in &p.roles {
        let items = r.data.deploy_role.as_ref().and_then(|d| d.wearing.as_ref()).map(|w| w.items.as_slice()).unwrap_or(&[]);
        for it in items {
            let Some(b) = it.basic.as_ref() else { continue };
            if let Some(k) = kind_of(b.type_id) {
                out.entry((r.role_guid, k)).or_default().push((b.position.as_ref().map(|p| p.x).unwrap_or(0), b.guid));
            }
        }
    }
    out.into_iter()
        .map(|(k, mut v)| {
            v.sort();
            (k, v.into_iter().map(|(_, g)| g).collect())
        })
        .collect()
}

/// The action broke the rule: the character now has more than one relic of a kind, and more than before (old violations
/// do not prevent taking items off). Some — what is broken.
pub(super) fn limit_broken(before: &PlayerModel, after: &PlayerModel, kind_of: &dyn Fn(i32) -> Option<i32>) -> Option<String> {
    let was = worn_relics(before, kind_of);
    worn_relics(after, kind_of).into_iter().find_map(|((role, kind), v)| {
        let had = was.get(&(role, kind)).map_or(0, |w| w.len());
        (v.len() > 1 && v.len() > had).then(|| format!("role {}: relics of kind \"{}\" worn: {}", role, KIND_NAMES[kind as usize], v.len()))
    })
}

/// Extra worn relics of a role (the second and further of the same kind) — to the backpack, no room — to the stash; nowhere — they stay.
/// Returns the type_ids taken off.
pub(super) fn take_off_extra(p: &mut PlayerModel, role_guid: u64, kind_of: &dyn Fn(i32) -> Option<i32>, shape_of: &dyn Fn(i32) -> ItemShape) -> Vec<i32> {
    let extra: Vec<i64> = worn_relics(p, kind_of).into_iter().filter(|((r, _), _)| *r == role_guid).flat_map(|(_, v)| v.into_iter().skip(1)).collect();
    let mut moved = vec![];
    for guid in extra {
        let Some(item) = inv::take(p, guid) else { continue };
        let type_id = item.basic.as_ref().map(|b| b.type_id).unwrap_or(0);
        let s = shape_of(type_id);
        let place = [
            tgf::DItemPosition { r#type: BACKPACK, guid: role_guid as i64, ..Default::default() },
            tgf::DItemPosition { r#type: STASH, guid: 0, ..Default::default() },
        ]
        .into_iter()
        .find_map(|base| inv::free_cell(p, &base, s.w, s.h, shape_of));
        match place {
            Some(pos) => {
                let _ = inv::put(p, item, &pos);
                moved.push(type_id);
            }
            None => {
                // nowhere to put it — return it to its place (the item is not lost)
                let back = inv::pos_of(&item);
                let _ = inv::put(p, item, &back);
            }
        }
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::RoleModel;

    fn kind_of(t: i32) -> Option<i32> {
        match t {
            220110120 | 220110130 => Some(1),
            220210120 => Some(2),
            _ => None,
        }
    }

    fn worn(type_id: i32, guid: i64, x: i32, role: u64) -> tgf::DItem {
        tgf::DItem {
            basic: Some(tgf::DItemBasic {
                type_id,
                count: 1,
                guid,
                position: Some(tgf::DItemPosition { r#type: inv::WEARING, guid: role as i64, x, ..Default::default() }),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    fn player(items: Vec<tgf::DItem>) -> PlayerModel {
        let mut p = PlayerModel::new("t", 10000001);
        let mut data = tgf::DUserRole::default();
        data.deploy_role = Some(tgf::DBattleDeployRole { wearing: Some(tgf::DRoleWearingData { items }), ..Default::default() });
        p.roles = vec![RoleModel { role_guid: 7, ord: 0, type_id: 1, role_name: "r".into(), role_level: 1, data }];
        p
    }

    #[test]
    fn second_relic_of_kind_is_refused_other_kinds_allowed() {
        let before = player(vec![worn(220110120, 1, 1, 7)]);
        let ok = player(vec![worn(220110120, 1, 1, 7), worn(220210120, 2, 4, 7), worn(181010000, 3, 2, 7)]);
        assert_eq!(limit_broken(&before, &ok, &kind_of), None, "weapon + clothing + regular item");
        let bad = player(vec![worn(220110120, 1, 1, 7), worn(220110130, 2, 2, 7)]);
        assert!(limit_broken(&before, &bad, &kind_of).unwrap().contains("weapon"));
        // a violation already existed — taking off is allowed, adding is not
        assert_eq!(limit_broken(&bad, &before, &kind_of), None);
        assert_eq!(limit_broken(&bad, &bad, &kind_of), None);
    }

    #[test]
    fn extra_worn_relic_goes_to_backpack() {
        let mut p = player(vec![worn(220110130, 2, 2, 7), worn(220110120, 1, 1, 7)]);
        let moved = take_off_extra(&mut p, 7, &kind_of, &|_| ItemShape { w: 1, h: 3, stack: 1 });
        assert_eq!(moved, vec![220110130], "the relic in the lower slot stays");
        let d = p.roles[0].data.deploy_role.as_ref().unwrap();
        assert_eq!(d.wearing.as_ref().unwrap().items.len(), 1);
        let bp = &d.backpack.as_ref().unwrap().items;
        assert_eq!(bp.len(), 1);
        assert_eq!(bp[0].basic.as_ref().unwrap().position.as_ref().unwrap().r#type, BACKPACK);
    }
}
