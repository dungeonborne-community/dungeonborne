//! "Clotho's Trial" (EN_ACTIVITY_CENTER_IMMACULATE_TRIAL, event 16600) — a run as in the original
//! (docs/CLOTHO_TRIAL_PLAN.md §5–7). Pure logic here: the player's run state (`DUserActivityTrialData` in the event
//! data), match recording, the `immaculate_trial_grade` grade, rewards. RPC — in shop_handlers (activity_center_trial_*),
//! match recording — from the arena result (arena_settle).
//!
//! The client (PVPChallengeService / TrialsUtils) looks at `TrialData.CurrentGame`:
//! - `status`: 0 — no run (a stone is needed, `NotItemActivate`), 1 — running (`CanPlay`), 2 — finished, awaiting the reward (`Award`);
//! - `win_count` / `failed_count`, `challenges_data[].battle_result`: ESCAPE — win, DEAD/SELF_QUIT — loss
//!   (`GetImmaculateWinLoseIndexs`);
//! - 7 wins without losses and the first such run (`immaculate_count == 1`) — the "Reward" button leads to the area
//!   `L_Common_PVP_end`, from there the client sends `trial_receive_reward` itself.

use super::*;

pub const STATUS_NONE: i32 = tgf::EnActivityTrialChallengeStatus::EnActivityTrialChallengeNonw as i32;
pub const STATUS_OPEN: i32 = tgf::EnActivityTrialChallengeStatus::EnActivityTrialChallengeOpen as i32;
pub const STATUS_REWARD: i32 = tgf::EnActivityTrialChallengeStatus::EnActivityTrialChallengeReward as i32;
/// Trial level (`activity_center.bind_levels`)
pub const TRIAL_LEVEL: i64 = 50000006;
/// How many finished runs to keep for history (`trial_history_record`)
pub const HISTORY_KEEP: usize = 20;

const WIN: i32 = tgf::EnDsPlayerActorStatusType::Escape as i32;
const LOSE: i32 = tgf::EnDsPlayerActorStatusType::Dead as i32;

/// Whether (wins, losses) match the grade condition. proto3 fields without a value = 0, so `{}` means exactly 0
/// ("Flawless" has `failed_range {}` — no losses at all).
pub fn grade_matches(cond: Option<&tgf::DTrialLevelCond>, wins: i32, fails: i32) -> bool {
    let Some(c) = cond else { return false };
    let inside = |r: Option<&tgf::DTrialLevelRange>, v: i32| {
        let r = r.cloned().unwrap_or_default();
        v >= r.min_count && v <= r.max_count
    };
    inside(c.win_range.as_ref(), wins) && inside(c.failed_range.as_ref(), fails)
}

/// Run grade: the first row of the rule by `grade_id` whose condition the result matches
pub fn grade_for(grades: &[tgf::config::ExcelImmaculateTrialGrade], rule_id: i32, wins: i32, fails: i32) -> Option<&tgf::config::ExcelImmaculateTrialGrade> {
    let mut rows: Vec<_> = grades.iter().filter(|g| g.activity_rule_id == rule_id).collect();
    rows.sort_by_key(|g| g.grade_id);
    rows.into_iter().find(|g| grade_matches(g.reached_cond.as_ref(), wins, fails))
}

/// The run is over: 7 wins or 3 losses (rule)
pub fn finished(rule: &tgf::config::ExcelImmaculateTrialRule, game: &tgf::DTrialRoundData) -> bool {
    game.win_count >= rule.win_count.max(1) || game.failed_count >= rule.failed_count.max(1)
}

/// Close the run: status "reward", grade; a flawless one adds +1 to `immaculate_count`
pub fn close(td: &mut tgf::DUserActivityTrialData, grades: &[tgf::config::ExcelImmaculateTrialGrade], rule: &tgf::config::ExcelImmaculateTrialRule, self_quit: bool) {
    let rule_id = td.rule_id;
    let g = td.current_game.get_or_insert_with(Default::default);
    g.status = STATUS_REWARD;
    g.self_quit = self_quit;
    g.grade_id = grade_for(grades, rule_id, g.win_count, g.failed_count).map(|x| x.grade_id).unwrap_or(0);
    if g.win_count >= rule.win_count && g.failed_count == 0 && !self_quit {
        td.immaculate_count += 1;
    }
}

/// A run match: a win or a loss in `challenges_data`; reached the end — close. true — the run was closed by this match.
/// Run not open or already closed — nothing (false), the record is not duplicated by `battle_id`.
pub fn record_match(
    td: &mut tgf::DUserActivityTrialData,
    grades: &[tgf::config::ExcelImmaculateTrialGrade],
    rule: &tgf::config::ExcelImmaculateTrialRule,
    battle_id: &str,
    won: bool,
) -> Option<bool> {
    let g = td.current_game.as_mut().filter(|g| g.status == STATUS_OPEN)?;
    if g.challenges_data.iter().any(|c| c.battle_id == battle_id) {
        return None;
    }
    if won {
        g.win_count += 1;
    } else {
        g.failed_count += 1;
    }
    let challenge_id = g.challenges_data.len() as i32 + 1;
    g.challenges_data.push(tgf::DTrialChallengesSingleLevelData {
        challenge_id,
        battle_id: battle_id.to_string(),
        battle_result: if won { WIN } else { LOSE },
    });
    let done = finished(rule, g);
    if done {
        close(td, grades, rule, false);
    }
    Some(done)
}

/// Rewards of a closed run: (first-time — once per grade, random). First-time ones are granted as items
/// `first_show_rewards` (the original's `first_rewards` has pool id 631103, no such item exists; shown — 100098 + chest
/// 631311), random ones — `show_items` (gold or a flawless stone + the grade chest 6313xx).
pub fn rewards(td: &tgf::DUserActivityTrialData, grade: &tgf::config::ExcelImmaculateTrialGrade) -> (Vec<tgf::DItemOffset>, Vec<tgf::DItemOffset>) {
    let first = if !grade.first_show_rewards.is_empty() && !td.received_first_reward_ids.contains(&grade.grade_id) {
        grade.first_show_rewards.clone()
    } else {
        vec![]
    };
    (first, grade.show_items.clone())
}

/// Open a new run (the stone is already spent): status "running", run number
pub fn open(td: &mut tgf::DUserActivityTrialData) {
    td.open_times += 1;
    td.current_game = Some(tgf::DTrialRoundData { status: STATUS_OPEN, round_id: td.open_times, ..Default::default() });
}

// ---------- junction with the launcher lobby ("Clotho" tab, src/lobby/launcher_lobby.rs) ----------

/// Run state for the launcher card and the tab sign-up check
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize)]
pub struct TrialMe {
    pub activity_id: i32,
    /// STATUS_NONE / STATUS_OPEN / STATUS_REWARD
    pub status: i32,
    pub wins: i32,
    pub fails: i32,
    /// tokens: losses left until the end of the run (3 − losses), only for an open run
    pub tokens: i32,
}

/// Tab sign-up refusals — texts of the original's strings (`ImmaculateTrial_Prompt_State_3` / `_5`)
pub const REFUSE_NOT_OPEN: &str = "Испытание не активировано. Для активации перейдите к событию «Испытание Клото».";
pub const REFUSE_REWARD: &str = "Есть доступные награды. Заберите их в событии «Испытание Клото».";

fn session(db: &crate::db::GameDb, gd: &std::sync::Arc<GameData>, open_id: &str) -> GatewaySession {
    GatewaySession {
        db: db.clone(),
        game_data: gd.clone(),
        open_id: open_id.to_string(),
        peer_ip: "lobby".to_string(),
        after_pushes: std::sync::Mutex::new(Vec::new()),
        rpc_error: std::sync::atomic::AtomicI32::new(0),
    }
}

/// The player's run state (None — no event or no player)
pub async fn trial_me(db: &crate::db::GameDb, gd: &std::sync::Arc<GameData>, open_id: &str) -> Option<TrialMe> {
    session(db, gd, open_id).trial_state(open_id).await
}

/// Sign-up to the "Clotho" tab: run in progress — Ok; awaiting a reward — refused; no run, but a stone is present — open it
/// (like the client's `IsAutoUse`: `trial_open` with a stone from the inventory, pushes to the player) — Ok; otherwise refused.
pub async fn trial_ensure_open(db: &crate::db::GameDb, gd: &std::sync::Arc<GameData>, open_id: &str) -> Result<TrialMe, String> {
    let s = session(db, gd, open_id);
    let me = s.trial_state(open_id).await.ok_or_else(|| "«Испытание Клото» сейчас недоступно.".to_string())?;
    match me.status {
        STATUS_OPEN => return Ok(me),
        STATUS_REWARD => return Err(REFUSE_REWARD.to_string()),
        _ => {}
    }
    s.activity_center_trial_open_impl(tgf::CsActivityCenteTrialOpenReq { activity_id: me.activity_id, items: vec![] }).await;
    if s.rpc_error.load(std::sync::atomic::Ordering::Relaxed) != 0 {
        return Err(REFUSE_NOT_OPEN.to_string());
    }
    for (method, type_url, body) in s.after_pushes.lock().unwrap().drain(..) {
        crate::lobby::hub::push_raw(open_id, &method, &type_url, body);
    }
    s.trial_state(open_id).await.filter(|m| m.status == STATUS_OPEN).ok_or_else(|| REFUSE_NOT_OPEN.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn range(min: i32, max: i32) -> Option<tgf::DTrialLevelRange> {
        Some(tgf::DTrialLevelRange { min_count: min, max_count: max })
    }

    fn grade(id: i32, w: (i32, i32), f: (i32, i32)) -> tgf::config::ExcelImmaculateTrialGrade {
        tgf::config::ExcelImmaculateTrialGrade {
            grade_id: id,
            activity_rule_id: 1,
            reached_cond: Some(tgf::DTrialLevelCond { win_range: range(w.0, w.1), failed_range: range(f.0, f.1) }),
            show_items: vec![tgf::DItemOffset { type_id: 630000 + id, count: 1, ..Default::default() }],
            first_show_rewards: if id == 101 { vec![tgf::DItemOffset { type_id: 631311, count: 1, ..Default::default() }] } else { vec![] },
            ..Default::default()
        }
    }

    /// The original's table: 101 7/0, 102 7/1, 103 7/2, 104 6/0..3, 110 0/0..3
    fn grades() -> Vec<tgf::config::ExcelImmaculateTrialGrade> {
        vec![grade(101, (7, 7), (0, 0)), grade(102, (7, 7), (1, 1)), grade(103, (7, 7), (2, 2)), grade(104, (6, 6), (0, 3)), grade(110, (0, 0), (0, 3))]
    }

    fn rule() -> tgf::config::ExcelImmaculateTrialRule {
        tgf::config::ExcelImmaculateTrialRule { activity_rule_id: 1, win_count: 7, failed_count: 3, ..Default::default() }
    }

    #[test]
    fn flawless_run_closes_with_grade_101_and_counts_immaculate() {
        let (g, r) = (grades(), rule());
        let mut td = tgf::DUserActivityTrialData { rule_id: 1, ..Default::default() };
        open(&mut td);
        for i in 0..6 {
            assert_eq!(record_match(&mut td, &g, &r, &format!("b{}", i), true), Some(false));
        }
        assert_eq!(record_match(&mut td, &g, &r, "b5", true), None, "the same match is not recorded twice");
        assert_eq!(record_match(&mut td, &g, &r, "b6", true), Some(true));
        let game = td.current_game.as_ref().unwrap();
        assert_eq!((game.status, game.grade_id, game.win_count), (STATUS_REWARD, 101, 7));
        assert_eq!(td.immaculate_count, 1);
        assert_eq!(game.challenges_data.iter().filter(|c| c.battle_result == WIN).count(), 7);
        let (first, random) = rewards(&td, grade_for(&g, 1, 7, 0).unwrap());
        assert_eq!(first[0].type_id, 631311);
        assert_eq!(random[0].type_id, 630101);
        assert_eq!(record_match(&mut td, &g, &r, "b7", true), None, "run closed — matches are not recorded");
    }

    #[test]
    fn three_losses_close_run_and_grade_by_wins() {
        let (g, r) = (grades(), rule());
        let mut td = tgf::DUserActivityTrialData { rule_id: 1, ..Default::default() };
        open(&mut td);
        for i in 0..2 {
            record_match(&mut td, &g, &r, &format!("l{}", i), false);
        }
        assert_eq!(record_match(&mut td, &g, &r, "l2", false), Some(true));
        let game = td.current_game.as_ref().unwrap();
        assert_eq!((game.status, game.grade_id, game.failed_count), (STATUS_REWARD, 110, 3));
        assert_eq!(td.immaculate_count, 0);
    }

    #[test]
    fn seven_wins_with_one_loss_is_minor_miss() {
        assert_eq!(grade_for(&grades(), 1, 7, 1).map(|x| x.grade_id), Some(102));
        assert_eq!(grade_for(&grades(), 1, 6, 2).map(|x| x.grade_id), Some(104));
        assert!(grade_for(&grades(), 1, 5, 0).is_none(), "the test table has no 5 wins row");
    }

    #[test]
    fn first_reward_only_once() {
        let g = grades();
        let mut td = tgf::DUserActivityTrialData { rule_id: 1, received_first_reward_ids: vec![101], ..Default::default() };
        open(&mut td);
        let (first, _) = rewards(&td, grade_for(&g, 1, 7, 0).unwrap());
        assert!(first.is_empty());
    }

    #[test]
    fn self_quit_closes_with_current_progress() {
        let (g, r) = (grades(), rule());
        let mut td = tgf::DUserActivityTrialData { rule_id: 1, ..Default::default() };
        open(&mut td);
        close(&mut td, &g, &r, true);
        let game = td.current_game.as_ref().unwrap();
        assert!(game.self_quit);
        assert_eq!((game.status, game.grade_id), (STATUS_REWARD, 110));
    }
}
