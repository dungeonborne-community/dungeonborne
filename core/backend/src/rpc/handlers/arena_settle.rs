//! Results of a custom game PVP match (Deathmatch / BO5 arenas) — the score table in the lobby (11.11, docs/PVP_SETTLE.md).
//!
//! In the original the results are collected by the DS native code (`PVPGameWinTeam` → `player_game_over`), but it is closed off by the DS SDK check. The
//! battle server mod (RuServer, hook `BattleDsUtility:PVPGameWinTeam`) sends `POST /ds/arena_result` with player statistics
//! (`TgfActorStatisticComponent`, like `MakeStatisticDataFromPlayer` in BP). Here every participant gets
//! `battle_settle_sync` while still in the match; `WBP_HallWindow`, when the lobby opens, sees `LevelType = kPrivateBattle` (3001)
//! and opens `WBP_TGF_PVP_Settle` (Lua `WBP_PvpSettle`):
//! - `Result.ParticipateData[]` — table rows: `FactionId` 1 blue / 2 red, `Statistics`, `RoleGuid`, `UserKey`;
//! - `ActorType` ESCAPE for the winning team, ALIVE for everyone on a draw (the window writes "Victory/Defeat/Draw" from it);
//! - `GsResult.SettleInterfaceType` — PVP (arenas x01, 40001) or BO5 (x02; rounds `PlayerPvpRoundData` are not filled).
//! Experience, loot and raid records are not touched.

use super::*;

/// The "custom game" level (`DBattleRoomData.battle_data` kPrivateBattle) — the lobby opens the PVP window by it
pub const LEVEL_TYPE_PRIVATE_BATTLE: i32 = 3001;
/// "Clotho's Trial" — competitive PVP (`competition_level 50000006.level_type`, kCompetitionSeasonPvp); same window
pub const LEVEL_TYPE_COMPETITION_PVP: i32 = 2002;

/// A player statistics row from the battle server
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArenaPlayerStat {
    pub open_id: String,
    pub team: i64,
    pub kills: i32,
    pub deaths: i32,
    pub damage: i64,
    pub heal: i64,
    pub taken: i64,
}

/// Participant for the result: statistics + role data from the database
#[derive(Clone, Debug, Default)]
pub struct ArenaParticipant {
    pub stat: ArenaPlayerStat,
    pub user_id: u64,
    pub role_guid: u64,
    pub role_name: String,
}

/// Match teams → window factions (1 blue, 2 red). Numbers 1 and 2 stay as they are; otherwise by ascending number:
/// the first team is 1, the rest 2 (the window draws only two columns).
pub fn arena_factions(teams: &[i64]) -> HashMap<i64, i32> {
    let mut uniq: Vec<i64> = teams.to_vec();
    uniq.sort_unstable();
    uniq.dedup();
    if uniq.iter().all(|t| *t == 1 || *t == 2) {
        return uniq.into_iter().map(|t| (t, t as i32)).collect();
    }
    uniq.into_iter().enumerate().map(|(i, t)| (t, if i == 0 { 1 } else { 2 })).collect()
}

/// Results window type by arena map (`map_detail.settle_interface_type`): x02 — BO5, other arenas — PVP;
/// "Clotho's Trial" (level 50000006, `competition_level.settle_interface_type`) — BO5_TRIAL
pub fn arena_interface_type(arena_map_id: i32, level_id: i32) -> i32 {
    if level_id as i64 == super::trial::TRIAL_LEVEL {
        tgf::DSettleInterFaceType::EnSettleInterfaceTypeBo5Trial as i32
    } else if (30000..40000).contains(&arena_map_id) && arena_map_id % 100 == 2 {
        tgf::DSettleInterFaceType::EnSettleInterfaceTypeBo5 as i32
    } else {
        tgf::DSettleInterFaceType::EnSettleInterfaceTypePvp as i32
    }
}

/// `battle_settle_sync` messages to all participants: (open_id, message). `winner_team` — the winning team from
/// `PVPGameWinTeam(faction_id)`; not a team of any participant — a draw. `rounds` — BO5 rounds (number, winning team
/// from `PVPGameWinTeamRound`): each gets `DsResult.PlayerPvpRoundData` with a win or a loss of their team per round.
pub fn build_arena_settle(
    players: &[ArenaParticipant],
    winner_team: i64,
    level_id: i32,
    arena_map_id: i32,
    rounds: &[(i32, i64)],
) -> Vec<(String, tgf::ScBattleSettleSync)> {
    let mut rounds = rounds.to_vec();
    rounds.sort_unstable();
    let round_data = |team: i64| -> Vec<tgf::DDsPvPRoundRoleStatisticsData> {
        rounds
            .iter()
            .map(|(round, win)| tgf::DDsPvPRoundRoleStatisticsData {
                round: *round,
                result: if *win == team {
                    tgf::EnBattleUserResultType::EnBattleUserResultWin as i32
                } else {
                    tgf::EnBattleUserResultType::EnBattleUserResultLose as i32
                },
                ..Default::default()
            })
            .collect()
    };
    let teams: Vec<i64> = players.iter().map(|p| p.stat.team).collect();
    let factions = arena_factions(&teams);
    let draw = !teams.contains(&winner_team);
    let actor_type = |team: i64| {
        if draw {
            tgf::EnDsPlayerActorStatusType::Alive as i32
        } else if team == winner_team {
            tgf::EnDsPlayerActorStatusType::Escape as i32
        } else {
            tgf::EnDsPlayerActorStatusType::Dead as i32
        }
    };
    let key = |p: &ArenaParticipant| tgf::DPlayerIdKey { user_id: p.user_id, zone_id: 1 };
    let participate: Vec<tgf::DBattleParticipateData> = players
        .iter()
        .map(|p| tgf::DBattleParticipateData {
            user_key: Some(key(p)),
            statistics: Some(tgf::DRoleArenaGameOverStatistics {
                kill_player_count: p.stat.kills,
                death_count: p.stat.deaths,
                assist_count: 0,
                kda: p.stat.kills as f64 / p.stat.deaths.max(1) as f64,
                caused_total_damage: p.stat.damage,
                caused_total_heal: p.stat.heal,
                got_total_damage: p.stat.taken,
            }),
            faction_id: factions.get(&p.stat.team).copied().unwrap_or(2),
            role_guid: p.role_guid as i64,
            actor_type: actor_type(p.stat.team),
            team_id: p.stat.team,
            role_name: p.role_name.clone(),
            ..Default::default()
        })
        .collect();
    let level_parameter = tgf::DBattleLevelParameter {
        level_type: if level_id as i64 == super::trial::TRIAL_LEVEL { LEVEL_TYPE_COMPETITION_PVP } else { LEVEL_TYPE_PRIVATE_BATTLE },
        level_id,
        ..Default::default()
    };
    players
        .iter()
        .map(|p| {
            let faction = factions.get(&p.stat.team).copied().unwrap_or(2);
            let msg = tgf::ScBattleSettleSync {
                result: Some(tgf::DBattlePlayerGameOver {
                    user_key: Some(key(p)),
                    role_guid: p.role_guid as i64,
                    map_data: Some(tgf::DBattleMapData { map_id: arena_map_id }),
                    faction_id: faction,
                    team_id: p.stat.team,
                    ds_result: Some(tgf::DDsUserBattleResult {
                        ds_user_key: p.stat.open_id.clone(),
                        game_type: super::raid_native::GAME_STATUS_GAMEOVER,
                        actor_type: actor_type(p.stat.team),
                        player_pvp_round_data: round_data(p.stat.team),
                        ..Default::default()
                    }),
                    participate_list: players.iter().map(key).collect(),
                    participate_data: participate.clone(),
                    ..Default::default()
                }),
                gs_result: Some(tgf::DGsUserSettleResult {
                    settle_interface_type: arena_interface_type(arena_map_id, level_id),
                    ..Default::default()
                }),
                level_parameter: Some(level_parameter.clone()),
            };
            (p.stat.open_id.clone(), msg)
        })
        .collect()
}

/// The player's role in this match: from the match record (`raid_entry.battle` = this match), otherwise the one selected in the lobby
fn arena_role(player: &PlayerModel, battle_id: &str) -> (u64, String) {
    let e = &player.state["raid_entry"];
    let guid = e["role_guid"]
        .as_u64()
        .filter(|_| !battle_id.is_empty() && e["battle"].as_str() == Some(battle_id))
        .unwrap_or(player.hall_role_guid);
    let name = player.roles.iter().find(|r| r.role_guid == guid).map(|r| r.role_name.clone()).unwrap_or_default();
    (guid, name)
}

impl GatewaySession {
    /// `POST /ds/arena_result`: arena match results to all participants (`battle_settle_sync`). Players not in the database
    /// (bot, host spectator) do not go into the table. Ok(how many sent) / Err(reason).
    pub async fn arena_settle(&self, battle_id: &str, winner_team: i64, stats: Vec<ArenaPlayerStat>, rounds: Vec<(i32, i64)>) -> Result<usize, NativeReject> {
        let room = crate::lobby::matchmaking::room(battle_id);
        let mut players = Vec::new();
        for s in stats {
            if let Some(r) = &room {
                if !r.members.iter().any(|m| m.open_id == s.open_id) {
                    println!("[Arena] {}: {} is not a room participant — skipped", battle_id, s.open_id);
                    continue;
                }
            }
            let Some(p) = self.db.get::<PlayerModel>("players", &s.open_id).await else {
                println!("[Arena] {}: player {} not in the database — skipped", battle_id, s.open_id);
                continue;
            };
            let (role_guid, role_name) = arena_role(&p, battle_id);
            players.push(ArenaParticipant { stat: s, user_id: p.user_id, role_guid, role_name });
        }
        if players.is_empty() {
            return Err(NativeReject(400, "no participants".to_string()));
        }
        let level_id = room.as_ref().map(|r| r.level_id as i32).unwrap_or(0);
        let arena_map_id = room.as_ref().and_then(|r| r.arena_map_id).unwrap_or(30101);
        let msgs = build_arena_settle(&players, winner_team, level_id, arena_map_id, &rounds);
        let mut sent = 0;
        for (open_id, msg) in msgs {
            let online = crate::lobby::hub::push_to(&open_id, "tgf.GameClientService.battle_settle_sync", "tgf.SCBattleSettleSync", &msg);
            sent += online as usize;
            super::raid_native::last_settle().lock().unwrap().insert(open_id.clone(), msg);
        }
        // "Clotho's Trial" (level 50000006): a win or a loss into each participant's run (trial.rs). There is no draw in BO5
        // (an extra round); if the server still sent a team that is not a participant's — the match is not recorded in the run.
        if level_id as i64 == super::trial::TRIAL_LEVEL {
            let teams: Vec<i64> = players.iter().map(|p| p.stat.team).collect();
            if teams.contains(&winner_team) {
                for p in &players {
                    self.trial_record_match(&p.stat.open_id, battle_id, p.stat.team == winner_team).await;
                }
            } else {
                println!("[Arena] {}: trial — no winner (team {}), match not recorded in the run", battle_id, winner_team);
            }
        }
        let line: Vec<String> = players
            .iter()
            .map(|p| format!("{} t{} k{}/d{} dmg {}", p.stat.open_id, p.stat.team, p.stat.kills, p.stat.deaths, p.stat.damage))
            .collect();
        println!("[Arena] {} (map {}, level {}): winner team {} — results to {} players, online {}: {}", battle_id,
            arena_map_id, level_id, winner_team, players.len(), sent, line.join("; "));
        Ok(sent)
    }
}

/// Consumable and throwable slots on the character (`EnItemWearingPart` 41–43, 51; `position.x` of a worn item)
const ARENA_STRIP_PARTS: [i32; 4] = [41, 42, 43, 51];

/// Arena rule (custom game DM and "Clotho's Trial", 11.11): own consumables and items from backpacks are not taken into the match
/// — only worn weapons, armor and jewelry go to the battle server. Only the match data changes: the player profile
/// is not touched, the arena result does not apply items. Returns the number of removed items.
pub fn arena_strip(deploy: &mut tgf::DBattleDeployRole) -> usize {
    let mut n = 0;
    for bag in [deploy.backpack.as_mut(), deploy.small_backpack.as_mut()].into_iter().flatten() {
        n += bag.items.len();
        bag.items.clear();
    }
    if let Some(w) = deploy.wearing.as_mut() {
        let before = w.items.len();
        w.items.retain(|i| !i.basic.as_ref().and_then(|b| b.position.as_ref()).is_some_and(|p| ARENA_STRIP_PARTS.contains(&p.x)));
        n += before - w.items.len();
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_strip_keeps_only_worn_gear() {
        let item = |type_id: i32, x: i32| tgf::DItem {
            basic: Some(tgf::DItemBasic {
                type_id,
                count: 1,
                position: Some(tgf::DItemPosition { r#type: 3, x, ..Default::default() }),
                ..Default::default()
            }),
            ..Default::default()
        };
        let mut d = tgf::DBattleDeployRole {
            backpack: Some(tgf::DRoleBackpackData { items: vec![item(100013, 0), item(181020000, 2)], row_size: 7, column_size: 10 }),
            small_backpack: Some(tgf::DRoleBackpackData { items: vec![item(100016, 0)], ..Default::default() }),
            wearing: Some(tgf::DRoleWearingData {
                items: vec![item(191011000, 1), item(194011000, 13), item(100013, 41), item(100016, 42), item(100071, 43), item(100012, 51)],
            }),
            ..Default::default()
        };
        assert_eq!(arena_strip(&mut d), 7);
        let worn: Vec<i32> = d.wearing.unwrap().items.iter().map(|i| i.basic.as_ref().unwrap().type_id).collect();
        assert_eq!(worn, vec![191011000, 194011000]);
        let bag = d.backpack.unwrap();
        assert!(bag.items.is_empty());
        assert_eq!((bag.row_size, bag.column_size), (7, 10), "backpack size stays");
        assert!(d.small_backpack.unwrap().items.is_empty());
    }

    fn part(open_id: &str, team: i64, kills: i32, deaths: i32) -> ArenaParticipant {
        ArenaParticipant {
            stat: ArenaPlayerStat { open_id: open_id.into(), team, kills, deaths, damage: 100, heal: 5, taken: 50 },
            user_id: 7,
            role_guid: 11,
            role_name: "R".into(),
        }
    }

    #[test]
    fn winner_escape_loser_dead_and_factions() {
        let out = build_arena_settle(&[part("a", 1, 3, 1), part("b", 2, 1, 3)], 1, 50000003, 30101, &[]);
        assert_eq!(out.len(), 2);
        let r = out[0].1.result.as_ref().unwrap();
        assert_eq!(r.participate_data.len(), 2);
        assert_eq!(r.participate_data[0].faction_id, 1);
        assert_eq!(r.participate_data[1].faction_id, 2);
        assert_eq!(r.participate_data[0].actor_type, tgf::EnDsPlayerActorStatusType::Escape as i32);
        assert_eq!(r.participate_data[1].actor_type, tgf::EnDsPlayerActorStatusType::Dead as i32);
        assert_eq!(r.participate_data[0].statistics.as_ref().unwrap().kill_player_count, 3);
        let lp = out[1].1.level_parameter.as_ref().unwrap();
        assert_eq!(lp.level_type, LEVEL_TYPE_PRIVATE_BATTLE);
        assert_eq!(out[1].1.gs_result.as_ref().unwrap().settle_interface_type, tgf::DSettleInterFaceType::EnSettleInterfaceTypePvp as i32);
    }

    #[test]
    fn draw_marks_everyone_alive() {
        let out = build_arena_settle(&[part("a", 1, 0, 0), part("b", 2, 0, 0)], 0, 0, 30102, &[]);
        let r = out[0].1.result.as_ref().unwrap();
        assert!(r.participate_data.iter().all(|d| d.actor_type == tgf::EnDsPlayerActorStatusType::Alive as i32));
        assert_eq!(out[0].1.gs_result.as_ref().unwrap().settle_interface_type, tgf::DSettleInterFaceType::EnSettleInterfaceTypeBo5 as i32);
    }

    #[test]
    fn trial_level_uses_bo5_trial_window() {
        let out = build_arena_settle(&[part("a", 1, 1, 0), part("b", 2, 0, 1)], 1, 50000006, 30102, &[(2, 2), (1, 1)]);
        let r = &out[0].1.result.as_ref().unwrap().ds_result.as_ref().unwrap().player_pvp_round_data;
        assert_eq!(r.iter().map(|x| (x.round, x.result)).collect::<Vec<_>>(), vec![(1, 1), (2, 2)], "rounds in order: 1 win, 2 loss for team 1");
        assert_eq!(out[0].1.gs_result.as_ref().unwrap().settle_interface_type, tgf::DSettleInterFaceType::EnSettleInterfaceTypeBo5Trial as i32);
        assert_eq!(out[0].1.level_parameter.as_ref().unwrap().level_type, LEVEL_TYPE_COMPETITION_PVP);
    }

    #[test]
    fn other_team_numbers_map_to_two_columns() {
        let f = arena_factions(&[5, 9, 5]);
        assert_eq!(f[&5], 1);
        assert_eq!(f[&9], 2);
    }
}
