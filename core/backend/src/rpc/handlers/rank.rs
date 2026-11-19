//! Rankings (leaderboards): rank_get_specify_rank / rank_get_self_rank, match accounting for them and GM commands.
//!
//! Which tables exist is set by rank_rule + rank_report_setting: key (rank_type 1001, rank_instance_id) → data_type,
//! level_ids, type_id (class; 0 — all classes). Labels (GeneralStringTable): data_type 1 "wins" and 2 "kills" —
//! arena (level 50000003, we do not have it — the tables are empty); 3 and 4 — "average player kills per match" in the dungeon
//! when searching solo (3) and as a team (4), levels 40003015..20 (and "all levels" tables, instance 1001.. / 1101..).
//! Client hint (RankHelpTips_3): a role that played 5 matches in 7 days gets into the table; the average — over the role's last
//! 50 matches (const report_pve_kill_player_rank_min_batlle_count / pve_kill_player_rank_max_save_battle_count).
//! The client (WBP_Rank_ListItem_MyEntry, luac54 -l) shows `score / 100` for 3/4 → we store the average × 100.
//!
//! Storage: DB table `rank_board`, id = `<season>_<rank_type>_<instance>_<sub_type>_<sub_instance>` (a new season —
//! empty tables), a record per role (rankings are per role: is_role_rank). `until` — until when the role is qualified
//! (the 5th-from-last match within the 7-day window + 7 days): after that the record is not shown until the role plays 5 matches again.
//! The role's match history — state["rank_battles"][role_guid] = [{t, lvl, team, k}], for each pair (level, team)
//! the last 50 matches are kept. Raid kills are counted by /ds/kill (raid_entry.kills, without the kill_reward season flag).

use super::GatewaySession;
use crate::db::GameDb;
use crate::structures::PlayerModel;
use crate::tgf;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const RANK_TABLE: &str = "rank_board";
const RANK_TYPE_ABC: u32 = tgf::EnRankType::AbcBasic as u32;
/// rank_report_setting data_type: average player kills per match in the dungeon, solo / team search
const DT_SOLO_KILLS: i32 = 3;
const DT_TEAM_KILLS: i32 = 4;
const MIN_BATTLES: usize = 5;
const MAX_BATTLES: usize = 50;
const QUALIFY_WINDOW_S: i64 = 7 * 24 * 3600;
const HISTORY: &str = "rank_battles";
const ZONE_ID: u32 = 1;

/// Read-modify-write of the ranking tables — under one lock (raid results of several players at once)
static RANK_DB_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
struct RankEntry {
    user_id: u64,
    role_guid: i64,
    score: u32,
    /// time the score was submitted (with equal scores, the earlier one ranks higher)
    at: i64,
    /// until when the role is qualified; 0 — no limit
    until: i64,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct RankBoard {
    entries: Vec<RankEntry>,
}

/// One role match in the ranking history
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub(super) struct RankBattle {
    pub t: i64,
    pub lvl: i32,
    /// team search (data_type 4), otherwise solo (3)
    pub team: bool,
    pub k: u32,
}

fn season_tag() -> String {
    crate::season::active().id.clone().unwrap_or_else(|| "base".into())
}

fn board_id(k: &tgf::DRankKey) -> String {
    format!("{}_{}_{}_{}_{}", season_tag(), k.rank_type, k.rank_instance_id, k.sub_rank_type, k.sub_rank_instance_id)
}

/// Table records by place (rank_no from 1): only those qualified at `now`
fn ranked(entries: &[RankEntry], now: i64) -> Vec<tgf::DRankUserBasicData> {
    let mut list: Vec<&RankEntry> = entries.iter().filter(|e| e.until == 0 || e.until >= now).collect();
    list.sort_by(|a, b| b.score.cmp(&a.score).then(a.at.cmp(&b.at)).then(a.role_guid.cmp(&b.role_guid)));
    list.iter()
        .enumerate()
        .map(|(i, e)| tgf::DRankUserBasicData {
            user_key: Some(tgf::DPlayerIdKey { user_id: e.user_id, zone_id: ZONE_ID }),
            role_guid: e.role_guid,
            rank_no: i as u32 + 1,
            score: e.score,
            submit_timepoint: e.at,
            ..Default::default()
        })
        .collect()
}

async fn board_load(db: &GameDb, key: &tgf::DRankKey) -> Vec<tgf::DRankUserBasicData> {
    let board = db.get::<RankBoard>(RANK_TABLE, &board_id(key)).await.unwrap_or_default();
    ranked(&board.entries, GatewaySession::now_ts())
}

/// Write the role's score (Some((score, until))) or remove the role from the table (None). The score is replaced, not a "best result":
/// the average can also drop.
async fn board_put(db: &GameDb, key: &tgf::DRankKey, user_id: u64, role_guid: i64, value: Option<(u32, i64)>) {
    let _g = RANK_DB_LOCK.lock().await;
    let id = board_id(key);
    let mut board = db.get::<RankBoard>(RANK_TABLE, &id).await.unwrap_or_default();
    let before = board.entries.len();
    board.entries.retain(|e| !(e.user_id == user_id && e.role_guid == role_guid));
    let removed = board.entries.len() != before;
    match value {
        Some((score, until)) => board.entries.push(RankEntry { user_id, role_guid, score, at: GatewaySession::now_ts(), until }),
        None if !removed => return,
        None => {}
    }
    db.save(RANK_TABLE, &id, &board).await;
}

fn key_of(r: &tgf::config::ExcelRankReportSetting) -> tgf::DRankKey {
    tgf::DRankKey { rank_type: r.rank_type, rank_instance_id: r.rank_instance_id, ..Default::default() }
}

/// "Dungeon kills" tables that a match on level `level_id` with a role of class `role_type` goes into: both (solo and team —
/// each one's score is computed from its own match sample), this level or "all levels", this class or all classes.
fn kill_boards(rows: &[tgf::config::ExcelRankReportSetting], level_id: i32, role_type: i32) -> Vec<&tgf::config::ExcelRankReportSetting> {
    rows.iter()
        .filter(|r| r.rank_type == RANK_TYPE_ABC && matches!(r.data_type, DT_SOLO_KILLS | DT_TEAM_KILLS))
        .filter(|r| r.level_ids.contains(&level_id) && (r.type_id == 0 || r.type_id == role_type))
        .collect()
}

/// Add a match to the history and keep the last `MAX_BATTLES` for each pair (level, team)
pub(super) fn history_push(list: &mut Vec<RankBattle>, b: RankBattle) {
    list.push(b);
    list.sort_by_key(|b| b.t);
    let mut seen: std::collections::HashMap<(i32, bool), usize> = std::collections::HashMap::new();
    let mut keep = vec![false; list.len()];
    for i in (0..list.len()).rev() {
        let n = seen.entry((list[i].lvl, list[i].team)).or_insert(0);
        *n += 1;
        keep[i] = *n <= MAX_BATTLES;
    }
    let mut i = 0;
    list.retain(|_| {
        i += 1;
        keep[i - 1]
    });
}

/// Table score from the history: average kills × 100 over the last 50 matches of the sample (table levels, solo/team) and
/// the qualification deadline (the 5th-from-last match within 7 days + 7 days). None — not qualified (fewer than 5 matches in 7 days).
pub(super) fn board_score(history: &[RankBattle], levels: &[i32], team: bool, now: i64) -> Option<(u32, i64)> {
    let mut sel: Vec<&RankBattle> = history.iter().filter(|b| b.team == team && levels.contains(&b.lvl)).collect();
    sel.sort_by_key(|b| std::cmp::Reverse(b.t));
    sel.truncate(MAX_BATTLES);
    let fifth = sel.get(MIN_BATTLES - 1)?.t;
    let until = fifth + QUALIFY_WINDOW_S;
    if until < now {
        return None;
    }
    let kills: u64 = sel.iter().map(|b| b.k as u64).sum();
    let score = ((kills * 100 + sel.len() as u64 / 2) / sel.len() as u64).min(u32::MAX as u64) as u32;
    Some((score, until))
}

fn history_of(p: &PlayerModel, role_guid: u64) -> Vec<RankBattle> {
    serde_json::from_value(p.state[HISTORY][role_guid.to_string()].clone()).unwrap_or_default()
}

fn history_set(p: &mut PlayerModel, role_guid: u64, list: &[RankBattle]) {
    if !p.state[HISTORY].is_object() {
        p.state[HISTORY] = json!({});
    }
    p.state[HISTORY][role_guid.to_string()] = json!(list);
}

/// Player kills in the current raid (raid_entry.kills; /ds/kill)
pub(super) fn raid_kills(state: &Value) -> u32 {
    state["raid_entry"]["kills"].as_u64().unwrap_or(0) as u32
}

pub(super) fn raid_kill_add(state: &mut Value) -> u32 {
    let n = raid_kills(state) + 1;
    state["raid_entry"]["kills"] = json!(n);
    n
}

impl GatewaySession {
    pub(super) async fn rank_get_specify_rank_impl(&self, req: tgf::CsRankGetSpecifyRankReq) -> Option<tgf::ScRankGetSpecifyRankRsp> {
        let key = req.rank_key.clone().unwrap_or_default();
        let all = board_load(&self.db, &key).await;
        let total = all.len() as u32;
        // the client sends start_no = 1 (first place), count = 100
        let skip = req.start_no.saturating_sub(1) as usize;
        let count = if req.count == 0 { 100 } else { req.count as usize };
        let rank_records = all.into_iter().skip(skip).take(count).collect();
        Some(tgf::ScRankGetSpecifyRankRsp { rank_key: req.rank_key, rank_records, total_count: total })
    }

    pub(super) async fn rank_get_self_rank_impl(&self, req: tgf::CsRankGetSelfRankReq) -> Option<tgf::ScRankGetSelfRankRsp> {
        let key = req.rank_key.clone().unwrap_or_default();
        let all = board_load(&self.db, &key).await;
        let total = all.len() as u32;
        let me = self.load_me().await.user_id;
        let pos = all.iter().position(|r| r.user_key.as_ref().map(|k| k.user_id) == Some(me) && (req.role_guid == 0 || r.role_guid == req.role_guid));
        let rank_records = match pos {
            Some(i) => {
                let from = i.saturating_sub(req.up_count as usize);
                let to = (i + 1 + req.down_count as usize).min(all.len());
                all[from..to].to_vec()
            }
            None => vec![], // the role is not in the table — the client shows "999+"
        };
        Some(tgf::ScRankGetSelfRankRsp { rank_key: req.rank_key, role_guid: req.role_guid, rank_records, total_count: total })
    }

    /// The role's match is over: write it to the history (level — the lobby selection, solo/team — raid_entry.team_size, kills —
    /// raid_entry.kills) and update the "dungeon kills" tables. A level without such tables — nothing.
    /// The caller saves the player; the tables are written right away. Returns the number of updated tables.
    pub(super) async fn rank_after_raid(&self, p: &mut PlayerModel, role_guid: u64, level_id: i32, team: bool, kills: u32) -> usize {
        let role_type = p.roles.iter().find(|r| r.role_guid == role_guid).map(|r| r.type_id as i32).unwrap_or(0);
        if kill_boards(&self.game_data.rank_report_setting_rows, level_id, role_type).is_empty() {
            return 0;
        }
        let mut hist = history_of(p, role_guid);
        history_push(&mut hist, RankBattle { t: Self::now_ts(), lvl: level_id, team, k: kills });
        history_set(p, role_guid, &hist);
        self.rank_resubmit(p, role_guid, Some(level_id)).await
    }

    /// Recompute the role's tables from its history (`level_id` — only this level's tables; None — all kill tables)
    async fn rank_resubmit(&self, p: &PlayerModel, role_guid: u64, level_id: Option<i32>) -> usize {
        let role_type = p.roles.iter().find(|r| r.role_guid == role_guid).map(|r| r.type_id as i32).unwrap_or(0);
        let hist = history_of(p, role_guid);
        let now = Self::now_ts();
        let rows = &self.game_data.rank_report_setting_rows;
        let boards: Vec<&tgf::config::ExcelRankReportSetting> = match level_id {
            Some(l) => kill_boards(rows, l, role_type),
            None => rows
                .iter()
                .filter(|r| r.rank_type == RANK_TYPE_ABC && matches!(r.data_type, DT_SOLO_KILLS | DT_TEAM_KILLS))
                .filter(|r| r.type_id == 0 || r.type_id == role_type)
                .collect(),
        };
        let mut changed = Vec::new();
        for r in &boards {
            let v = board_score(&hist, &r.level_ids, r.data_type == DT_TEAM_KILLS, now);
            board_put(&self.db, &key_of(r), p.user_id, role_guid as i64, v).await;
            if let Some((s, _)) = v {
                changed.push(format!("{}:{:.2}", r.rank_instance_id, s as f64 / 100.0));
            }
        }
        println!("[Rank] {} role {}: tables {} (qualified: {})", p.open_id, role_guid, boards.len(), if changed.is_empty() { "none".into() } else { changed.join(" ") });
        boards.len()
    }

    /// GM `rank [add <kills> [team]] | rank reset`: places of the hall role / a match into the history without a raid / remove the player from
    /// the season tables and clear the history
    pub(super) async fn rank_gm(&self, p: &mut PlayerModel, args: &[String]) -> Result<Vec<String>, String> {
        let role_guid = p.hall_role_guid;
        let sub = args.get(1).map(|s| s.to_lowercase()).unwrap_or_default();
        match sub.as_str() {
            "add" => {
                let k = args.get(2).and_then(|s| s.parse::<u32>().ok()).ok_or("rank add <kills> [team]")?;
                let team = args.get(3).map_or(false, |s| s.eq_ignore_ascii_case("team"));
                let lvl = Self::get_saved_level_select(p).level_id;
                let n = self.rank_after_raid(p, role_guid, lvl, team, k).await;
                if n == 0 {
                    return Err(format!("level {} has no kill tables — select a dungeon in the lobby", lvl));
                }
                self.save_me(p).await;
                Ok(vec![format!("match: level {}, {}, kills {}; tables updated {}", lvl, if team { "team" } else { "solo" }, k, n)])
            }
            "reset" => {
                let roles: Vec<u64> = p.roles.iter().map(|r| r.role_guid).collect();
                let _ = p.state.as_object_mut().map(|o| o.remove(HISTORY));
                self.save_me(p).await;
                let removed = self.rank_forget(p.user_id, &roles).await;
                Ok(vec![format!("ranking history cleared, player removed from {} tables of season {}", removed, season_tag())])
            }
            "" => {
                let hist = history_of(p, role_guid);
                let mut out = vec![format!("season {}, role {}: matches in history {}", season_tag(), role_guid, hist.len())];
                for r in self.game_data.rank_report_setting_rows.iter().filter(|r| r.rank_type == RANK_TYPE_ABC) {
                    let all = board_load(&self.db, &key_of(r)).await;
                    if let Some(e) = all.iter().find(|e| e.role_guid == role_guid as i64 && e.user_key.as_ref().map(|k| k.user_id) == Some(p.user_id)) {
                        out.push(format!("instance {} (type {}, class {}): place {}/{}, score {}", r.rank_instance_id, r.data_type, r.type_id, e.rank_no, all.len(), e.score));
                    }
                }
                Ok(out)
            }
            _ => Err("rank | rank add <kills> [team] | rank reset".into()),
        }
    }
}

impl GatewaySession {
    /// Remove the player's roles from all "dungeon kills" tables of the current season (rank reset, wipe); how many tables
    pub(super) async fn rank_forget(&self, user_id: u64, roles: &[u64]) -> usize {
        let mut boards = 0;
        for r in self.game_data.rank_report_setting_rows.iter().filter(|r| r.rank_type == RANK_TYPE_ABC) {
            for g in roles {
                board_put(&self.db, &key_of(r), user_id, *g as i64, None).await;
            }
            boards += 1;
        }
        boards
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_data::GameData;
    use crate::structures::RoleModel;

    const DAY: i64 = 24 * 3600;

    fn row(inst: u32, dt: i32, levels: &[i32], type_id: i32) -> tgf::config::ExcelRankReportSetting {
        tgf::config::ExcelRankReportSetting { rank_type: RANK_TYPE_ABC, rank_instance_id: inst, type_id, level_ids: levels.to_vec(), data_type: dt, sort: 0 }
    }

    fn b(t: i64, lvl: i32, team: bool, k: u32) -> RankBattle {
        RankBattle { t, lvl, team, k }
    }

    #[test]
    fn boards_by_level_class_and_type() {
        let rows = vec![
            row(1, 1, &[50000003], 0),             // arena — not ours
            row(201, 3, &[40003016], 0),           // level, all classes
            row(206, 3, &[40003016], 400005),      // level, class 400005
            row(207, 3, &[40003016], 400006),      // another class
            row(301, 4, &[40003016], 0),           // team
            row(601, 3, &[40003015], 0),           // another level
            row(1001, 3, &[40003015, 40003016], 0), // all levels
        ];
        let ids: Vec<u32> = kill_boards(&rows, 40003016, 400005).iter().map(|r| r.rank_instance_id).collect();
        assert_eq!(ids, vec![201, 206, 301, 1001]);
        assert!(kill_boards(&rows, 50000003, 400005).is_empty(), "arena is not counted");
    }

    #[test]
    fn score_needs_five_recent_battles() {
        let now = 100 * DAY;
        let lv = [40003016];
        let mut h: Vec<RankBattle> = (0..4).map(|i| b(now - i, 40003016, false, 1)).collect();
        assert_eq!(board_score(&h, &lv, false, now), None, "4 matches — not qualified");
        h.push(b(now - 10, 40003016, false, 2));
        let (score, until) = board_score(&h, &lv, false, now).unwrap();
        assert_eq!(score, 120, "6 kills / 5 matches = 1.20");
        assert_eq!(until, now - 10 + 7 * DAY);
        assert_eq!(board_score(&h, &lv, true, now), None, "team sample is empty");
        assert_eq!(board_score(&h, &[40003015], false, now), None, "another level");
        // the 5th-from-last match is older than 7 days — not qualified
        let old: Vec<RankBattle> = (0..5).map(|i| b(now - 8 * DAY - i, 40003016, false, 3)).collect();
        assert_eq!(board_score(&old, &lv, false, now), None);
    }

    #[test]
    fn score_over_last_fifty() {
        let now = 1000 * DAY;
        let mut h = Vec::new();
        // 60 old matches with 9 kills each, then 50 new ones with 1 — the average only over the last 50
        for i in 0..60 {
            history_push(&mut h, b(now - 200 + i, 40003016, false, 9));
        }
        for i in 0..50 {
            history_push(&mut h, b(now - 100 + i, 40003016, false, 1));
        }
        assert_eq!(h.len(), MAX_BATTLES, "per pair (level, team) — the last 50");
        assert_eq!(board_score(&h, &[40003016], false, now).unwrap().0, 100);
        // another pair does not push it out
        history_push(&mut h, b(now, 40003018, true, 4));
        assert_eq!(h.len(), MAX_BATTLES + 1);
    }

    #[test]
    fn ranked_order_and_expiry() {
        let e = |uid: u64, score: u32, at: i64, until: i64| RankEntry { user_id: uid, role_guid: uid as i64 * 100 + 1, score, at, until };
        let l = ranked(&[e(1, 150, 5, 0), e(2, 300, 9, 0), e(3, 150, 2, 0), e(4, 999, 1, 50)], 100);
        assert_eq!(l.iter().map(|x| (x.rank_no, x.user_key.as_ref().unwrap().user_id)).collect::<Vec<_>>(), vec![(1, 2), (2, 3), (3, 1)], "expired qualification not shown, with equal scores — the earlier one");
    }

    #[test]
    fn raid_kill_counter() {
        let mut s = json!({ "raid_entry": { "applied": false } });
        assert_eq!(raid_kills(&s), 0);
        raid_kill_add(&mut s);
        assert_eq!(raid_kill_add(&mut s), 2);
        assert_eq!(raid_kills(&s), 2);
    }

    /// Role matches → DB tables → rank_get_specify_rank / rank_get_self_rank responses
    #[tokio::test]
    async fn raids_fill_boards() {
        let dir = format!("target/test_rank_db_{}", crate::auth::sessions::random_hex(4));
        let db = GameDb::init(&dir).await;
        let s = GatewaySession {
            db: db.clone(),
            game_data: std::sync::Arc::new(GameData::load_all("ExcelConfig")),
            open_id: "ranka".into(),
            peer_ip: "test".into(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        let mut p = PlayerModel::new("ranka", 19_100_001);
        let role_guid = 19_100_001 * 100 + 1;
        p.hall_role_guid = role_guid;
        p.roles.push(RoleModel { role_guid, ord: 1, type_id: 400005, role_name: "R".into(), role_level: 1, data: Default::default() });
        // 40003016: solo, normal — tables 201 (all classes), 206 (400005), 1001/1006 (all levels)
        for k in [0, 1, 2, 3] {
            assert!(s.rank_after_raid(&mut p, role_guid, 40003016, false, k).await > 0);
        }
        db.save("players", "ranka", &p).await;
        let key = |inst: u32| tgf::DRankKey { rank_type: RANK_TYPE_ABC, rank_instance_id: inst, ..Default::default() };
        let get = |inst: u32| tgf::CsRankGetSpecifyRankReq { rank_key: Some(key(inst)), start_no: 1, count: 100 };
        assert_eq!(s.rank_get_specify_rank_impl(get(201)).await.unwrap().total_count, 0, "4 matches — not in the table yet");
        s.rank_after_raid(&mut p, role_guid, 40003016, false, 4).await;
        db.save("players", "ranka", &p).await;
        for inst in [201, 206, 1001, 1006] {
            let r = s.rank_get_specify_rank_impl(get(inst)).await.unwrap();
            assert_eq!(r.total_count, 1, "instance {}", inst);
            assert_eq!(r.rank_records[0].score, 200, "10 kills / 5 matches");
            assert_eq!(r.rank_records[0].role_guid, role_guid as i64);
        }
        assert_eq!(s.rank_get_specify_rank_impl(get(301)).await.unwrap().total_count, 0, "team — empty");
        assert_eq!(s.rank_get_specify_rank_impl(get(207)).await.unwrap().total_count, 0, "another class — empty");
        let me = s
            .rank_get_self_rank_impl(tgf::CsRankGetSelfRankReq { rank_key: Some(key(1006)), role_guid: role_guid as i64, up_count: 0, down_count: 0 })
            .await
            .unwrap();
        assert_eq!(me.rank_records.iter().map(|r| r.rank_no).collect::<Vec<_>>(), vec![1]);
        // a match without kills — the average drops (replacement, not a best result)
        s.rank_after_raid(&mut p, role_guid, 40003016, false, 0).await;
        assert_eq!(s.rank_get_specify_rank_impl(get(201)).await.unwrap().rank_records[0].score, 167, "10 / 6");
        // a level without kill tables (arena) — nothing
        assert_eq!(s.rank_after_raid(&mut p, role_guid, 50000003, false, 5).await, 0);
        drop(s);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
