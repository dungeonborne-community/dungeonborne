//! Parties (team_*): roster, captain, invitations, readiness, join requests (team finder) and party search
//! (team_match_search). State only; DUserTeamData snapshots are built and broadcast by the gateway session
//! (it needs characters from the DB).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Maximum players in a party (same as the default match room size)
pub fn max_members() -> usize {
    std::env::var("TEAM_MAX_MEMBERS").ok().and_then(|v| v.parse().ok()).unwrap_or(3)
}

const INVITATION_TTL: i64 = 60;
/// Same as team_invitation_expired_timeout (30s) in const.json
const APPLY_TTL: i64 = 30;

/// EnTeamMethodType: how a member got into the party (DTeamMember.join_team_type)
pub const JOIN_NONE: i32 = 0;
pub const JOIN_MATCH: i32 = 5;

#[derive(Clone, Debug)]
pub struct TeamMember {
    pub user_id: u64,
    pub open_id: String,
    pub joined_ts: i64,
    pub ready: bool,
    pub join_type: i32,
}

#[derive(Clone, Debug)]
pub struct Team {
    pub team_id: i64,
    pub captain: u64,
    pub members: Vec<TeamMember>,
}

#[derive(Clone, Debug)]
pub struct Invitation {
    pub team_id: i64,
    pub inviter: u64,
    pub invitee: u64,
    pub start_ts: i64,
    pub expired_ts: i64,
}

/// A request to join a party (team_apply_join_team), decided by the captain.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct Apply {
    pub team_id: i64,
    pub user_id: u64,
    pub open_id: String,
    pub apply_type: i32,
    pub start_ts: i64,
    pub expired_ts: i64,
}

/// What is known about a player for party search filters: active character and selected mode.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub role_type_id: i32,
    pub role_level: i32,
    pub level_type: i32,
    pub region: String,
}

/// DTeamSearchTarget: who we are looking for (empty list = anyone).
#[derive(Clone, Debug, Default)]
pub struct SearchTarget {
    pub type_ids: Vec<i32>,
    pub level_types: Vec<i32>,
    pub min_level: i32,
    pub regions: Vec<String>,
}

impl SearchTarget {
    pub fn accepts(&self, p: &Profile) -> bool {
        (self.type_ids.is_empty() || self.type_ids.contains(&p.role_type_id))
            && (self.level_types.is_empty() || p.level_type == 0 || self.level_types.iter().any(|t| *t == p.level_type || *t == mode_of(p.level_type)))
            && p.role_level >= self.min_level
            && (self.regions.is_empty() || p.region.is_empty() || self.regions.iter().any(|r| r.eq_ignore_ascii_case(&p.region)))
    }
}

/// The client sends the mode as SocialConst.EGameModeType (E_ADVENTURE/E_COMPETITIVE), while the player has level_type 2001/2002.
/// TODO(verify): numeric values of EGameModeType (we assume 1 and 2).
fn mode_of(level_type: i32) -> i32 {
    match level_type {
        2001 => 1,
        2002 => 2,
        other => other,
    }
}

/// A player's active party search.
#[derive(Clone, Debug)]
pub struct Search {
    pub user_id: u64,
    pub open_id: String,
    pub matching_id: String,
    pub start_ts: i64,
    pub target: SearchTarget,
    pub profile: Profile,
    started: Instant,
    last_check: Option<Instant>,
}

/// Party search result: the party, who was affected by the match (they get the Matched event), the former parties of those added.
#[derive(Clone, Debug)]
pub struct MatchResult {
    pub team: Team,
    pub matched: Vec<u64>,
    pub old_teams: Vec<Team>,
}

#[derive(Default)]
struct State {
    next_id: i64,
    teams: HashMap<i64, Team>,
    by_user: HashMap<u64, i64>,
    invitations: Vec<Invitation>,
    applies: Vec<Apply>,
    /// Players with team finder on (ready to accept players found by search into their party)
    finders: HashMap<u64, Profile>,
    searches: HashMap<u64, Search>,
    next_search: u64,
}

static STATE: OnceLock<Mutex<State>> = OnceLock::new();

fn state() -> &'static Mutex<State> {
    STATE.get_or_init(|| Mutex::new(State::default()))
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
}

pub fn team_of(user_id: u64) -> Option<Team> {
    let st = state().lock().unwrap();
    st.by_user.get(&user_id).and_then(|id| st.teams.get(id)).cloned()
}

/// A player's team in a match (`DBattleUserParameter.team_id`): a party — its id, a solo player — their own team (user_id),
/// otherwise all solo players of a raid would end up allies
pub fn battle_team_id(user_id: u64) -> i64 {
    if let Some(t) = room_team_overrides().lock().unwrap().get(&user_id) {
        return *t;
    }
    team_of(user_id).map(|t| t.team_id).unwrap_or(user_id as i64)
}

/// Custom game match team (05.11): the side by seat in a private room (lobby::room) for the duration of its match —
/// takes priority over the party. Set by room_start_battle, removed by the room's match end / leaving the room.
fn room_team_overrides() -> &'static Mutex<HashMap<u64, i64>> {
    static M: OnceLock<Mutex<HashMap<u64, i64>>> = OnceLock::new();
    M.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_room_battle_team(user_id: u64, team_id: i64) {
    room_team_overrides().lock().unwrap().insert(user_id, team_id);
}

pub fn clear_room_battle_team(user_id: u64) {
    room_team_overrides().lock().unwrap().remove(&user_id);
}

pub fn invitations_for(user_id: u64) -> Vec<Invitation> {
    let mut st = state().lock().unwrap();
    let now = now_ts();
    st.invitations.retain(|i| i.expired_ts > now);
    st.invitations.iter().filter(|i| i.invitee == user_id).cloned().collect()
}

/// A new party of a single captain. Returns team_id.
fn create_team_locked(st: &mut State, captain: u64, open_id: &str) -> i64 {
    st.next_id += 1;
    let id = st.next_id;
    st.teams.insert(
        id,
        Team {
            team_id: id,
            captain,
            members: vec![TeamMember {
                user_id: captain,
                open_id: open_id.to_string(),
                joined_ts: now_ts(),
                ready: false,
                join_type: JOIN_NONE,
            }],
        },
    );
    st.by_user.insert(captain, id);
    id
}

/// Joining a party: leave the previous one, drop own requests and party search.
/// Returns (new party, previous party after leaving — if there was one).
fn join_locked(st: &mut State, user_id: u64, open_id: &str, team_id: i64, join_type: i32) -> Result<(Team, Option<Team>), &'static str> {
    if st.teams.get(&team_id).map_or(true, |t| t.members.len() >= max_members()) {
        return Err("full_or_gone");
    }
    let old = if st.by_user.get(&user_id) == Some(&team_id) { None } else { leave_locked(st, user_id) };
    let team = st.teams.get_mut(&team_id).ok_or("gone")?;
    if !team.members.iter().any(|m| m.user_id == user_id) {
        team.members.push(TeamMember { user_id, open_id: open_id.to_string(), joined_ts: now_ts(), ready: false, join_type });
    }
    st.by_user.insert(user_id, team_id);
    st.applies.retain(|a| a.user_id != user_id);
    st.searches.remove(&user_id);
    Ok((st.teams[&team_id].clone(), old))
}

/// Invite: the inviter gets a party (as captain) if they do not have one yet. Returns the party.
pub fn invite(inviter: u64, inviter_open_id: &str, invitee: u64) -> Result<Team, &'static str> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    if inviter == invitee {
        return Err("self");
    }
    let team_id = match st.by_user.get(&inviter) {
        Some(id) => *id,
        None => create_team_locked(st, inviter, inviter_open_id),
    };
    let team = st.teams.get(&team_id).cloned().ok_or("no_team")?;
    if team.members.len() >= max_members() {
        return Err("full");
    }
    if team.members.iter().any(|m| m.user_id == invitee) {
        return Err("already_member");
    }
    let now = now_ts();
    st.invitations.retain(|i| !(i.team_id == team_id && i.invitee == invitee));
    st.invitations.push(Invitation { team_id, inviter, invitee, start_ts: now, expired_ts: now + INVITATION_TTL });
    Ok(team)
}

/// Accept an invitation: the player leaves the previous party and enters the new one.
/// Returns (new party, previous party after leaving — if there was one).
pub fn accept(user_id: u64, open_id: &str, team_id: i64) -> Result<(Team, Option<Team>), &'static str> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let now = now_ts();
    let pos = st
        .invitations
        .iter()
        .position(|i| i.team_id == team_id && i.invitee == user_id && i.expired_ts > now)
        .ok_or("no_invitation")?;
    st.invitations.remove(pos);
    join_locked(st, user_id, open_id, team_id, JOIN_NONE)
}

pub fn reject(user_id: u64, team_id: i64) -> Option<Invitation> {
    let mut st = state().lock().unwrap();
    let pos = st.invitations.iter().position(|i| i.team_id == team_id && i.invitee == user_id)?;
    Some(st.invitations.remove(pos))
}

/// Leaving a party; a one-person party is disbanded. Returns the party after leaving (None — disbanded).
fn leave_locked(st: &mut State, user_id: u64) -> Option<Team> {
    let team_id = st.by_user.remove(&user_id)?;
    let team = st.teams.get_mut(&team_id)?;
    team.members.retain(|m| m.user_id != user_id);
    if team.members.len() <= 1 {
        // Disbanded: the remaining solo player needs a "no party" snapshot, so we return the roster
        // with a negative team_id (the disband marker)
        let mut gone = st.teams.remove(&team_id).unwrap();
        for m in &gone.members {
            st.by_user.remove(&m.user_id);
        }
        st.invitations.retain(|i| i.team_id != team_id);
        st.applies.retain(|a| a.team_id != team_id);
        gone.team_id = -gone.team_id;
        return Some(gone);
    }
    if team.captain == user_id {
        team.captain = team.members[0].user_id;
    }
    Some(team.clone())
}

pub fn leave(user_id: u64) -> Option<Team> {
    let mut st = state().lock().unwrap();
    leave_locked(&mut st, user_id)
}

/// The captain kicks a member. Returns the party after the kick.
pub fn remove_member(captain: u64, target: u64) -> Result<Option<Team>, &'static str> {
    let mut st = state().lock().unwrap();
    let team_id = *st.by_user.get(&captain).ok_or("no_team")?;
    if st.teams.get(&team_id).map(|t| t.captain) != Some(captain) {
        return Err("not_captain");
    }
    if st.by_user.get(&target) != Some(&team_id) {
        return Err("not_member");
    }
    Ok(leave_locked(&mut st, target))
}

pub fn transfer_captain(captain: u64, target: u64) -> Result<Team, &'static str> {
    let mut st = state().lock().unwrap();
    let team_id = *st.by_user.get(&captain).ok_or("no_team")?;
    let team = st.teams.get_mut(&team_id).ok_or("no_team")?;
    if team.captain != captain {
        return Err("not_captain");
    }
    if !team.members.iter().any(|m| m.user_id == target) {
        return Err("not_member");
    }
    team.captain = target;
    Ok(team.clone())
}

pub fn set_ready(user_id: u64, ready: bool) -> Option<Team> {
    let mut st = state().lock().unwrap();
    let team_id = *st.by_user.get(&user_id)?;
    let team = st.teams.get_mut(&team_id)?;
    for m in team.members.iter_mut().filter(|m| m.user_id == user_id) {
        m.ready = ready;
    }
    Some(team.clone())
}

/// Update a member's open_id after reconnecting (the hub key is open_id).
pub fn touch_open_id(user_id: u64, open_id: &str) {
    let mut st = state().lock().unwrap();
    if let Some(team_id) = st.by_user.get(&user_id).copied() {
        if let Some(team) = st.teams.get_mut(&team_id) {
            for m in team.members.iter_mut().filter(|m| m.user_id == user_id) {
                m.open_id = open_id.to_string();
            }
        }
    }
}

// ---------------------------------------------------------------- requests (team finder)

/// A request to party team_id. Returns the party (to broadcast it a snapshot with pending_applies).
pub fn apply(user_id: u64, open_id: &str, team_id: i64, apply_type: i32) -> Result<Team, &'static str> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let team = st.teams.get(&team_id).cloned().ok_or("no_team")?;
    if team.members.iter().any(|m| m.user_id == user_id) {
        return Err("already_member");
    }
    if team.members.len() >= max_members() {
        return Err("full");
    }
    let now = now_ts();
    st.applies.retain(|a| a.expired_ts > now && !(a.team_id == team_id && a.user_id == user_id));
    st.applies.push(Apply { team_id, user_id, open_id: open_id.to_string(), apply_type, start_ts: now, expired_ts: now + APPLY_TTL });
    Ok(team)
}

pub fn applies_of(team_id: i64) -> Vec<Apply> {
    let mut st = state().lock().unwrap();
    let now = now_ts();
    st.applies.retain(|a| a.expired_ts > now);
    st.applies.iter().filter(|a| a.team_id == team_id).cloned().collect()
}

/// The captain accepts a request: the applicant joins the party. Returns (party, the applicant's previous party).
pub fn accept_apply(captain: u64, applicant: u64, team_id: i64) -> Result<(Team, Option<Team>), &'static str> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let team_id = if team_id != 0 { team_id } else { *st.by_user.get(&captain).ok_or("no_team")? };
    if st.teams.get(&team_id).map(|t| t.captain) != Some(captain) {
        return Err("not_captain");
    }
    let now = now_ts();
    let pos = st
        .applies
        .iter()
        .position(|a| a.team_id == team_id && a.user_id == applicant && a.expired_ts > now)
        .ok_or("no_apply")?;
    let apply = st.applies.remove(pos);
    join_locked(st, applicant, &apply.open_id, team_id, apply.apply_type)
}

/// The captain rejects a request. Returns the party.
pub fn reject_apply(captain: u64, applicant: u64, team_id: i64) -> Result<Team, &'static str> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    let team_id = if team_id != 0 { team_id } else { *st.by_user.get(&captain).ok_or("no_team")? };
    let team = st.teams.get(&team_id).cloned().ok_or("no_team")?;
    if team.captain != captain {
        return Err("not_captain");
    }
    st.applies.retain(|a| !(a.team_id == team_id && a.user_id == applicant));
    Ok(team)
}

/// Team finder: the player is ready to accept players found by search (None — turn off).
pub fn set_finder(user_id: u64, profile: Option<Profile>) {
    let mut st = state().lock().unwrap();
    match profile {
        Some(p) => {
            st.finders.insert(user_id, p);
        }
        None => {
            st.finders.remove(&user_id);
        }
    }
}

pub fn finder_status(user_id: u64) -> bool {
    state().lock().unwrap().finders.contains_key(&user_id)
}

// ---------------------------------------------------------------- party search (team_match_*)

/// A search without team_match_check for longer than this is abandoned (the client closed the window, and the cancel did not reach us).
fn search_stale() -> std::time::Duration {
    let secs = std::env::var("TEAM_MATCH_STALE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(30);
    std::time::Duration::from_secs(secs)
}

/// Search limit if the client does not send team_match_check at all (team_match_expired_timeout = 120s in const.json).
fn search_ttl() -> std::time::Duration {
    let secs = std::env::var("TEAM_MATCH_TTL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
    std::time::Duration::from_secs(secs)
}

fn drop_stale_locked(st: &mut State) {
    let stale = search_stale();
    let ttl = search_ttl();
    st.searches.retain(|_, s| match s.last_check {
        Some(t) => t.elapsed() < stale,
        None => s.started.elapsed() < ttl,
    });
}

/// Start a party search. Returns matching_id.
pub fn match_search(user_id: u64, open_id: &str, target: SearchTarget, profile: Profile) -> String {
    let mut st = state().lock().unwrap();
    st.next_search += 1;
    let matching_id = format!("tm{}", st.next_search);
    st.searches.insert(
        user_id,
        Search {
            user_id,
            open_id: open_id.to_string(),
            matching_id: matching_id.clone(),
            start_ts: now_ts(),
            target,
            profile,
            started: Instant::now(),
            last_check: None,
        },
    );
    matching_id
}

pub fn match_status(user_id: u64) -> Option<Search> {
    let mut st = state().lock().unwrap();
    drop_stale_locked(&mut st);
    st.searches.get(&user_id).cloned()
}

/// Cancel a party search (team_match_exit_impl in room_handlers.rs).
#[allow(dead_code)]
pub fn match_cancel(user_id: u64) -> Option<Search> {
    state().lock().unwrap().searches.remove(&user_id)
}

/// Search heartbeat + an attempt to match a party. None — not found yet (or no search).
pub fn match_check(user_id: u64) -> Option<MatchResult> {
    let mut guard = state().lock().unwrap();
    let st = &mut *guard;
    drop_stale_locked(st);
    let s = st.searches.get_mut(&user_id)?;
    s.last_check = Some(Instant::now());
    try_match_locked(st, user_id)
}

fn free_slots(st: &State, team_id: i64) -> usize {
    st.teams.get(&team_id).map_or(0, |t| max_members().saturating_sub(t.members.len()))
}

/// Matching for searcher S:
/// 1) S is the captain of a party with a free seat: take the earliest suitable solo player into their party;
/// 2) otherwise — someone else's party with a free seat whose captain has finder on or is searching for players;
/// 3) otherwise — another searching solo player: a new party, the captain is whoever started searching earlier.
/// DTeamSearchTarget filters are checked both ways. TODO(verify): meaning of the target fields in the SearchTeam UI.
fn try_match_locked(st: &mut State, user_id: u64) -> Option<MatchResult> {
    let me = st.searches.get(&user_id)?.clone();
    let my_team = st.by_user.get(&user_id).copied();
    let i_am_captain = my_team.map_or(false, |id| st.teams.get(&id).map(|t| t.captain) == Some(user_id));

    // solo player — no party or alone in their own party (after an unanswered invitation the player is the captain of a party of one:
    // on 02.11 both got stuck like this, and the search did not pair them)
    let alone = |uid: u64| st.by_user.get(&uid).and_then(|id| st.teams.get(id)).map_or(true, |t| t.members.len() <= 1);
    let mut solos: Vec<Search> = st
        .searches
        .values()
        .filter(|s| s.user_id != user_id && alone(s.user_id))
        .filter(|s| me.target.accepts(&s.profile) && s.target.accepts(&me.profile))
        .cloned()
        .collect();
    solos.sort_by_key(|s| (s.start_ts, s.user_id));

    // 1. I am the captain and looking for players for my party
    if let (Some(team_id), true) = (my_team, i_am_captain) {
        if free_slots(st, team_id) == 0 {
            st.searches.remove(&user_id);
            return None;
        }
        let r = solos.first()?.clone();
        let (team, old) = join_locked(st, r.user_id, &r.open_id, team_id, JOIN_MATCH).ok()?;
        if free_slots(st, team_id) == 0 {
            st.searches.remove(&user_id);
        }
        return Some(MatchResult { team, matched: vec![r.user_id, user_id], old_teams: old.into_iter().collect() });
    }

    // 2. Someone else's party with finder / a searching captain
    let mut teams: Vec<(i64, i64)> = st
        .teams
        .values()
        .filter(|t| Some(t.team_id) != my_team && t.members.len() < max_members())
        .filter_map(|t| {
            let profile = st
                .finders
                .get(&t.captain)
                .cloned()
                .or_else(|| st.searches.get(&t.captain).map(|s| s.profile.clone()))?;
            let captain_target_ok = st.searches.get(&t.captain).map_or(true, |s| s.target.accepts(&me.profile));
            (me.target.accepts(&profile) && captain_target_ok).then_some((t.team_id, t.members[0].joined_ts))
        })
        .collect();
    teams.sort_by_key(|(id, ts)| (*ts, *id));
    if let Some((team_id, _)) = teams.first().copied() {
        let captain = st.teams[&team_id].captain;
        let (team, old) = join_locked(st, user_id, &me.open_id, team_id, JOIN_MATCH).ok()?;
        if free_slots(st, team_id) == 0 {
            st.searches.remove(&captain);
        }
        return Some(MatchResult { team, matched: vec![user_id, captain], old_teams: old.into_iter().collect() });
    }

    // 3. Two solo players — a new party
    let r = solos.first()?.clone();
    let (first, second) = if (r.start_ts, r.user_id) <= (me.start_ts, me.user_id) { (r, me) } else { (me, r) };
    let mut old_teams = Vec::new();
    if let Some(old) = leave_locked(st, first.user_id) {
        old_teams.push(old);
    }
    st.searches.remove(&first.user_id);
    let team_id = create_team_locked(st, first.user_id, &first.open_id);
    if let Some(m) = st.teams.get_mut(&team_id).and_then(|t| t.members.first_mut()) {
        m.join_type = JOIN_MATCH;
    }
    let (team, old) = join_locked(st, second.user_id, &second.open_id, team_id, JOIN_MATCH).ok()?;
    old_teams.extend(old);
    Some(MatchResult { team, matched: vec![first.user_id, second.user_id], old_teams })
}

/// Disconnect: drop the player's party search, finder and requests.
pub fn on_disconnect(open_id: &str) {
    let mut st = state().lock().unwrap();
    let mut users: Vec<u64> = st.searches.values().filter(|s| s.open_id == open_id).map(|s| s.user_id).collect();
    for t in st.teams.values() {
        users.extend(t.members.iter().filter(|m| m.open_id == open_id).map(|m| m.user_id));
    }
    for u in users {
        st.searches.remove(&u);
        st.finders.remove(&u);
    }
    st.applies.retain(|a| a.open_id != open_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 02.11: both invited each other with no answer (each is the captain of a party of one) and turned on search — they must be paired
    #[test]
    fn lone_captains_match() {
        let (a, b) = (900_000_101, 900_000_102);
        invite(a, "oa", b).unwrap();
        invite(b, "ob", a).unwrap();
        let p = Profile { role_type_id: 400006, role_level: 4, level_type: 2001, region: "eu".into() };
        let t = SearchTarget { min_level: 1, ..Default::default() };
        match_search(a, "oa", t.clone(), p.clone());
        match_search(b, "ob", t, p);
        let r = match_check(b).expect("lone captains are paired");
        assert_eq!(r.team.members.len(), 2);
        assert_eq!(team_of(a).map(|t| t.team_id), team_of(b).map(|t| t.team_id));
        leave(a);
        leave(b);
    }
}
