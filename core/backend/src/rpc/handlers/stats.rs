//! Player statistics (09.11, user's decision): counters per character and map accumulate in
//! `player.state["stats"][role_guid]["<type>:<level_id>"]` and are sent to the client — `ScPlayerGetInfoRsp.user_statistics`
//! (per player) and `DUserRole.statistics` (per character). Types — `EnStatisticsDataType`.
//!
//! Which keys the client's statistics window uses is unknown (no original responses), so every record is sent
//! in several slices: map / all maps (level_id 0) × class / all classes (career 0), season 1.
//! Sources: raid result (games, extractions, deaths, player kills, time in raid) and `/ds/quest_progress`
//! (monsters, chests, damage to players).

use crate::structures::PlayerModel;
use crate::tgf;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) const GAME: i32 = 1;
pub(super) const ESCAPE: i32 = 2;
pub(super) const KILL_PLAYER: i32 = 3;
pub(super) const ADV_KILL_PLAYER: i32 = 4;
pub(super) const DAMAGE_TO_PLAYER: i32 = 6;
pub(super) const ALIVE_TIME: i32 = 10;
pub(super) const OPEN_CHEST: i32 = 13;
pub(super) const FAILED: i32 = 15;
pub(super) const DEATH: i32 = 16;
pub(super) const GAME_NEW: i32 = 17;
pub(super) const ADV_KILL_PLAYER_NEW: i32 = 18;
pub(super) const KILL_MONSTER: i32 = 100000;

const SEASON_ID: i32 = 1;

/// Add `n` to the counter of type `ty` for the character on map `level_id`
pub(super) fn add(state: &mut Value, role_guid: u64, level_id: i64, ty: i32, n: i64) {
    if n == 0 {
        return;
    }
    if !state["stats"].is_object() {
        state["stats"] = json!({});
    }
    let role = role_guid.to_string();
    if !state["stats"][&role].is_object() {
        state["stats"][&role] = json!({});
    }
    let key = format!("{}:{}", ty, level_id);
    let now = state["stats"][&role][&key].as_i64().unwrap_or(0);
    state["stats"][&role][&key] = json!(now.saturating_add(n));
}

/// Raid result: `applied` — extraction / death / self_quit / ban_kick (anything else is not a match); `kills` — player kills
/// in the raid, `alive_secs` — time in the raid
pub(super) fn after_raid(state: &mut Value, role_guid: u64, level_id: i64, applied: &str, kills: i64, alive_secs: i64) {
    let died = matches!(applied, "death" | "self_quit" | "ban_kick");
    if applied != "extraction" && !died {
        return;
    }
    for ty in [GAME, GAME_NEW] {
        add(state, role_guid, level_id, ty, 1);
    }
    if died {
        add(state, role_guid, level_id, DEATH, 1);
        add(state, role_guid, level_id, FAILED, 1);
    } else {
        add(state, role_guid, level_id, ESCAPE, 1);
    }
    for ty in [KILL_PLAYER, ADV_KILL_PLAYER, ADV_KILL_PLAYER_NEW] {
        add(state, role_guid, level_id, ty, kills);
    }
    add(state, role_guid, level_id, ALIVE_TIME, alive_secs.clamp(0, 24 * 3600));
}

/// Character counters: (type, map) -> value
fn counters(state: &Value, role_guid: u64) -> BTreeMap<(i32, i64), i64> {
    let mut out = BTreeMap::new();
    if let Some(m) = state["stats"][role_guid.to_string()].as_object() {
        for (k, v) in m {
            let mut it = k.splitn(2, ':');
            if let (Some(t), Some(l)) = (it.next().and_then(|t| t.parse().ok()), it.next().and_then(|l| l.parse().ok())) {
                *out.entry((t, l)).or_insert(0) += v.as_i64().unwrap_or(0);
            }
        }
    }
    out
}

fn row(ty: i32, level_id: i64, career: i32, value: i64) -> tgf::DStatisticsData {
    tgf::DStatisticsData {
        key: Some(tgf::DStatisticsLimitKey { r#type: ty, season_id: SEASON_ID, level_id: level_id as i32, rarity: 0, career, pvp_mode: 0 }),
        value,
    }
}

/// Records per (type, map, class) -> value: slices map/all maps × class/all classes
fn rows(sums: &BTreeMap<(i32, i64, i32), i64>) -> Vec<tgf::DStatisticsData> {
    let mut acc: BTreeMap<(i32, i64, i32), i64> = BTreeMap::new();
    for (&(ty, level, career), &v) in sums {
        let levels: &[i64] = if level == 0 { &[0] } else { &[level, 0] };
        let careers: &[i32] = if career == 0 { &[0] } else { &[career, 0] };
        for &l in levels {
            for &c in careers {
                *acc.entry((ty, l, c)).or_insert(0) += v;
            }
        }
    }
    acc.into_iter().map(|((ty, l, c), v)| row(ty, l, c, v)).collect()
}

fn role_sums(state: &Value, role_guid: u64, career: i32) -> BTreeMap<(i32, i64, i32), i64> {
    counters(state, role_guid).into_iter().map(|((ty, l), v)| ((ty, l, career), v)).collect()
}

/// Character statistics for `DUserRole.statistics`
/// `career` — character class (role_type.meta.career, GatewaySession::role_career)
pub(super) fn role_statistics(p: &PlayerModel, role_guid: u64, career: i32) -> tgf::DRoleBattleStatistics {
    let c = counters(&p.state, role_guid);
    let mut by_level: BTreeMap<i64, Vec<tgf::DRoleDataStatistics>> = BTreeMap::new();
    for (&(ty, level), &v) in &c {
        by_level.entry(level).or_default().push(tgf::DRoleDataStatistics { data_type: ty, value: v.clamp(0, i32::MAX as i64) as i32, recent_data: None });
    }
    tgf::DRoleBattleStatistics {
        level_statistics: by_level.into_iter().map(|(level_id, data)| tgf::DRoleLevelStatistics { level_id: level_id as i32, data }).collect(),
        role_statistics_data: Some(tgf::DStatisticsDatas { statistics: rows(&role_sums(&p.state, role_guid, career)), version: 1 }),
    }
}

/// Player statistics (all characters) for `ScPlayerGetInfoRsp.user_statistics`
pub(super) fn user_statistics(p: &PlayerModel, career_of: &dyn Fn(u32) -> i32) -> tgf::DStatisticsDatas {
    let mut sums: BTreeMap<(i32, i64, i32), i64> = BTreeMap::new();
    for r in &p.roles {
        for (k, v) in role_sums(&p.state, r.role_guid, career_of(r.type_id)) {
            *sums.entry(k).or_insert(0) += v;
        }
    }
    tgf::DStatisticsDatas { statistics: rows(&sums), version: 1 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(d: &tgf::DStatisticsDatas, ty: i32, level: i32, career: i32) -> i64 {
        d.statistics
            .iter()
            .find(|s| s.key.as_ref().is_some_and(|k| k.r#type == ty && k.level_id == level && k.career == career))
            .map_or(0, |s| s.value)
    }

    #[test]
    fn raids_add_up_per_map_and_class() {
        let mut p = PlayerModel::new("st", 1);
        p.add_role(400002, "a".into(), tgf::DUserRole::default());
        p.add_role(400005, "b".into(), tgf::DUserRole::default());
        let (a, b) = (p.roles[0].role_guid, p.roles[1].role_guid);
        after_raid(&mut p.state, a, 40003015, "extraction", 2, 600);
        after_raid(&mut p.state, a, 40003016, "death", 1, 120);
        after_raid(&mut p.state, b, 40003015, "ban_kick", 0, 60);
        after_raid(&mut p.state, b, 40003015, "hall_leave", 5, 60); // not a match — not counted
        add(&mut p.state, a, 40003015, KILL_MONSTER, 7);
        // classes from the table: 400002 fighter (2), 400005 wizard (5)
        let career = |t: u32| match t { 400002 => 2, 400005 => 5, _ => 0 };
        let u = user_statistics(&p, &career);
        assert_eq!(val(&u, GAME, 0, 0), 3);
        assert_eq!(val(&u, ESCAPE, 0, 0), 1);
        assert_eq!(val(&u, DEATH, 0, 0), 2);
        assert_eq!(val(&u, KILL_PLAYER, 0, 0), 3);
        assert_eq!(val(&u, KILL_PLAYER, 40003015, 0), 2);
        assert_eq!(val(&u, GAME, 0, 2), 2, "class 2 (400002) — 2 games");
        assert_eq!(val(&u, GAME, 40003015, 5), 1, "class 5 on map 40003015");
        assert_eq!(val(&u, KILL_MONSTER, 0, 0), 7);
        assert_eq!(val(&u, ALIVE_TIME, 0, 0), 780);
        let r = role_statistics(&p, a, 2);
        assert_eq!(r.level_statistics.len(), 2);
        let d = r.role_statistics_data.unwrap();
        assert_eq!((val(&d, GAME, 0, 0), val(&d, GAME, 0, 2), val(&d, ESCAPE, 40003015, 2)), (2, 2, 1));
    }
}
