//! Instance lobby in the launcher (docs/LAUNCHER_SPEC.md §12, env `LAUNCHER_LOBBY=1`). 11.11 (user decision): instances
//! by group size — solo, duo, trio (at most 3 / 2 / 1 at a time, counting running matches). Gathering has no minimum:
//! an 8-minute timer from the first sign-up; the instance starts when it is full (seats on the map for this size), or on
//! the timer with whoever is there. Map voting — for the whole gathering. Once started, the next one of the same size opens
//! next to it. The player enters with the game's own "Start" button (`lobby_matching_search` → [`enter`]).
//!
//! A tab is logical: a room takes any free instance (`allocator`, a process per match under ds-agent), it does not need
//! the roster in advance. State in memory, as with the matchmaker: a backend restart resets votes, running matches go on their own.

use super::matchmaking::{self, JoinRefused, Member};
use super::{allocator, hub, maintenance};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Voting maps: level_id, in the same order as in the launcher
pub const MAPS: [i64; 6] = [40003015, 40003017, 40003019, 40003016, 40003018, 40003020];
pub const SIZES: [usize; 3] = [1, 2, 3];

/// 13.11 (user decision): three solo instances are always open, each with its own difficulty — 1 "Easy", 2 "Classic",
/// 3 "Hard"; the map is set in advance and alternates (Crypt → Castle → Crypt…), no voting in solo. Once started, the next one
/// of the same difficulty opens right away on another map; the `LOBBY_SOLO` limit does not apply to them (matches are limited
/// by free servers). `LOBBY_SOLO_LANES=0` — as before (voting, limit).
pub const SOLO_LANES: [[i64; 2]; 3] = [[40003015, 40003016], [40003017, 40003018], [40003019, 40003020]];
pub const LANE_NAMES: [&str; 3] = ["easy", "classic", "hard"];

pub fn solo_lanes() -> bool {
    std::env::var("LOBBY_SOLO_LANES").map_or(true, |v| v != "0")
}

/// 11.11: the "Clotho" tab — the Clotho Trial as a permanent mode (docs/CLOTHO_TRIAL_PLAN.md §5, user decision:
/// "fully as in the original, but with an instance in the launcher"). No voting: gathering up to `clotho_min` (6, a 3v3 match to 4
/// wins on arena 30102), sign-up only with an open run (`trial_ensure_open` opens it for stone 1033).
/// Once gathered — the match room is created immediately with everyone signed up (`start_direct_on`, like a custom game start),
/// sides 1 and 2 are fixed in advance (`team::set_room_battle_team`). Match result and the run — `rpc::handlers::trial`/`arena_settle`.
pub const CLOTHO_ID: usize = 100;
/// The Clotho Trial arena for the launcher card (map_detail): L_Common_PVP01_MultiGameMode, mode "first to 4 wins"
pub const CLOTHO_ARENA: i32 = 30102;
/// "First to 4 wins" arenas (map_detail x02): PVP01/02/03 MultiGameMode
const CLOTHO_ARENAS: [i32; 3] = [30102, 30202, 30302];

/// Clotho match map (11.11, user decision): a random one of the "first to 4 wins" arenas. `CLOTHO_MAPS=30102,30302` —
/// a custom list (on the test stand — one map for testing); unknown numbers are dropped.
fn clotho_arena(roll: u64) -> i32 {
    let list: Vec<i32> = std::env::var("CLOTHO_MAPS")
        .ok()
        .map(|v| v.split(',').filter_map(|s| s.trim().parse().ok()).filter(|m| CLOTHO_ARENAS.contains(m)).collect())
        .filter(|l: &Vec<i32>| !l.is_empty())
        .unwrap_or_else(|| CLOTHO_ARENAS.to_vec());
    list[(roll % list.len() as u64) as usize]
}
/// Own match team id for the duration of a Clotho match: base + match number · 10 + side (will not collide with a party or user_id)
const CLOTHO_TEAM_BASE: i64 = 910_000_000;

/// Players in the "Clotho" tab needed to start (6 = 3v3; on the test stand `LOBBY_CLOTHO_MIN=2` — a 1v1 match)
pub fn clotho_min() -> usize {
    env_secs("LOBBY_CLOTHO_MIN", 6).max(2) as usize
}

/// The "Clotho" tab (12.11, user: "remove clotho" — off by default; `LOBBY_CLOTHO=1` — turn on)
pub fn clotho_enabled() -> bool {
    std::env::var("LOBBY_CLOTHO").is_ok_and(|v| v == "1")
}
/// No launcher poll for this many seconds and the game is offline — the sign-up is dropped (a launcher restart does not lose the seat)
const SEEN_SECS: i64 = 30;

pub fn enabled() -> bool {
    std::env::var("LAUNCHER_LOBBY").is_ok_and(|v| v == "1")
}

fn env_secs(key: &str, default: i64) -> i64 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).filter(|n| *n > 0).unwrap_or(default)
}

/// Hall timer for lobby rooms (11.11, user decision: 3 minutes — those signed up get in with the "Start" button)
pub fn wait_secs() -> i64 {
    env_secs("LOBBY_WAIT_SECS", 180)
}

/// Minimum players in the hall for the match to begin (the hall holds the countdown, `hall_hold_secs`). 11.11: 1 — no waiting
pub fn min_players() -> usize {
    env_secs("LOBBY_MIN_PLAYERS", 1) as usize
}

/// 11.11 (user decision): how many instances of each size at a time (counting running matches) —
/// solo `LOBBY_SOLO` (3), duo `LOBBY_DUO` (2), trio `LOBBY_TRIO` (1)
fn size_limits() -> Vec<(usize, usize)> {
    vec![
        (1, env_secs("LOBBY_SOLO", 3) as usize),
        (2, env_secs("LOBBY_DUO", 2) as usize),
        (3, env_secs("LOBBY_TRIO", 1) as usize),
    ]
}

/// Match length for the launcher timer (13.11): map_detail.timeout of the level's map (2400 s for dungeons and the castle — for both floors);
/// `LOBBY_BATTLE_SECS` — custom. A match may end earlier (everyone left) — then the tab is simply removed.
fn battle_secs(level_id: i64) -> i64 {
    let own = env_secs("LOBBY_BATTLE_SECS", 0);
    if own > 0 {
        return own;
    }
    BACKEND.get().and_then(|(_, gd)| map_timeout(gd, level_id)).unwrap_or(2400)
}

/// map_detail.timeout of rows where the level's map is one of the scenes (the smallest)
fn map_timeout(gd: &crate::game_data::GameData, level_id: i64) -> Option<i64> {
    let path = matchmaking::map_path(level_id);
    let dot = format!("{path}.");
    gd.map_detail
        .values()
        .filter(|d| d.map_config.iter().any(|c| c.ue_path == path || c.ue_path.starts_with(&dot)))
        .map(|d| d.timeout as i64)
        .filter(|t| *t > 0)
        .min()
}

/// 13.11 staggered tab opening at startup: between neighbours — random from `LOBBY_STAGGER_MIN_SECS` (600) to
/// `LOBBY_STAGGER_MAX_SECS` (1200); `LOBBY_STAGGER=0` — all at once
fn stagger_secs() -> Option<(i64, i64)> {
    if std::env::var("LOBBY_STAGGER").is_ok_and(|v| v == "0") {
        return None;
    }
    let lo = env_secs("LOBBY_STAGGER_MIN_SECS", 600);
    let hi = env_secs("LOBBY_STAGGER_MAX_SECS", 1200).max(lo);
    Some((lo, hi))
}

/// Instance gathering: a timer from the first sign-up, then a start with whoever is there (`LOBBY_FILL_SECS`, 8 minutes)
pub fn fill_secs() -> i64 {
    env_secs("LOBBY_FILL_SECS", 480)
}

/// How long the hall waits for the minimum beyond its own timer; then the match begins with whoever came (players will not get stuck)
pub fn hall_hold_secs() -> i64 {
    env_secs("LOBBY_HALL_HOLD_SECS", 300)
}

fn now_ts() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Idle,
    /// gathering: there are sign-ups, the timer is running, map voting
    Gather,
    Starting,
    Waitroom,
    Battle,
}

impl Phase {
    fn name(self) -> &'static str {
        match self {
            Phase::Idle => "idle",
            Phase::Gather => "gather",
            Phase::Starting => "starting",
            Phase::Waitroom => "waitroom",
            Phase::Battle => "battle",
        }
    }
}

/// What is happening with the tab's room right now
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hall {
    Gone,
    Preparing,
    Waiting,
    Started,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    BadInstance,
    BadVote,
    NotInLobby,
    VoteOver,
    Battle,
    Closed,
    Full,
    Maintenance,
    /// "Clotho": the run is not open and there is no stone 1033 / there are unclaimed rewards / the tab is off
    TrialNotOpen,
    TrialReward,
    ClothoOff,
    /// 13.11: a permanent-gathering tab has not opened yet (stagger at startup)
    NotOpen,
}

impl Refused {
    pub fn code(self) -> &'static str {
        match self {
            Refused::BadInstance => "bad_instance",
            Refused::BadVote => "bad_vote",
            Refused::NotInLobby => "not_in_lobby",
            Refused::VoteOver => "vote_over",
            Refused::Battle => "instance_battle",
            Refused::Closed => "hall_closed",
            Refused::Full => "hall_full",
            Refused::Maintenance => "maintenance",
            Refused::TrialNotOpen => "trial_not_open",
            Refused::TrialReward => "trial_reward",
            Refused::ClothoOff => "clotho_off",
            Refused::NotOpen => "instance_not_open",
        }
    }

    /// Refusal text for the player (the launcher shows it if it has no translation of its own for the code); empty — by code
    pub fn text(self) -> &'static str {
        match self {
            Refused::TrialNotOpen => "Испытание не активировано. Купите камень испытания или откройте событие «Испытание Клото» в игре.",
            Refused::TrialReward => "Есть доступные награды. Заберите их в событии «Испытание Клото» в игре.",
            Refused::ClothoOff => "Испытание Клото сейчас недоступно.",
            _ => "",
        }
    }
}

#[derive(Default)]
struct Player {
    seen: i64,
    size_vote: Option<usize>,
    map_vote: Option<i64>,
    /// for the "Clotho" tab: user_id (match team, game group) and run wins (balancing the sides)
    user_id: u64,
    wins: i32,
}

struct Tab {
    /// instance number for the launcher (1…99; "Clotho" — `CLOTHO_ID`)
    id: usize,
    phase: Phase,
    ends_at: i64,
    players: HashMap<String, Player>,
    party_size: usize,
    level_id: i64,
    /// solo lane 1…3 (`SOLO_LANES`): difficulty and the map set in advance (`level_id`); 0 — regular gathering with voting
    lane: usize,
    /// 13.11 stagger at startup: the tab is closed until this time (sign-up is refused, the launcher shows phase `closed` and a timer)
    opens_at: i64,
    battle_id: Option<String>,
    /// "Clotho": user_id of the current match's fighters — their match teams are removed when the room is gone
    fighters: Vec<u64>,
}

impl Tab {
    fn new() -> Self {
        Tab { id: 0, phase: Phase::Idle, ends_at: 0, players: HashMap::new(), party_size: 0, level_id: 0, lane: 0, opens_at: 0, battle_id: None, fighters: Vec::new() }
    }

    /// Instance of one's own group size
    fn sized(id: usize, size: usize) -> Self {
        Tab { id, party_size: size, ..Tab::new() }
    }

    fn voting(&self) -> bool {
        self.phase == Phase::Gather
    }

    /// Leading map of the vote (more votes, on a tie — earlier in the list); no votes — None.
    /// Solo lane — its preset map
    fn leader(&self) -> Option<i64> {
        if self.lane > 0 {
            return Some(self.level_id);
        }
        let count = |m: i64| self.players.values().filter(|p| p.map_vote == Some(m)).count();
        MAPS.iter().copied().filter(|m| count(*m) > 0).max_by(|a, b| count(*a).cmp(&count(*b)).then(pos(*b).cmp(&pos(*a))))
    }

    /// Seats until a fill-based start: of the leading map, without votes — of the largest one
    fn need(&self, seats: &dyn Fn(i64, usize) -> usize) -> usize {
        match self.leader() {
            Some(m) => seats(m, self.party_size),
            None => MAPS.iter().map(|m| seats(*m, self.party_size)).max().unwrap_or(0),
        }
        .max(1)
    }
}

fn pos(m: i64) -> usize {
    MAPS.iter().position(|x| *x == m).unwrap_or(usize::MAX)
}

/// Vote winner (09.11, user decision — proportional): an option is picked with a chance equal to its share of
/// votes (3 against 1 — 75% / 25%); an option with no votes is never picked; no votes — any option equally. `roll` —
/// a random number (in tests — a given one).
fn winner<T: Copy + PartialEq>(options: &[T], votes: impl Iterator<Item = T>, roll: u64) -> T {
    let votes: Vec<T> = votes.filter(|v| options.contains(v)).collect();
    if votes.is_empty() {
        return options[(roll % options.len() as u64) as usize];
    }
    let mut left = roll % votes.len() as u64;
    for o in options {
        let n = votes.iter().filter(|v| *v == o).count() as u64;
        if left < n {
            return *o;
        }
        left -= n;
    }
    options[0]
}

struct Lobby {
    tabs: Vec<Tab>,
    /// (group size, how many instances at a time) — `size_limits`
    limits: Vec<(usize, usize)>,
    /// the last issued instance number
    last_id: usize,
    /// solo lanes by difficulty (`solo_lanes`) and the next map of each (index in `SOLO_LANES[i]`)
    lanes: bool,
    lane_next: [usize; 3],
    /// seats on the map for a group of a given size (`matchmaking::room_seats`; in tests — custom): the sign-up cap for a solo lane
    seats: fn(i64, usize) -> usize,
    /// 13.11 staggered tab opening at startup (min, max seconds between neighbours); None — all at once (tests, LOBBY_STAGGER=0)
    stagger: Option<(i64, i64)>,
    /// gathering timer (`fill_secs`; in tests — custom)
    fill: i64,
    /// minimum players in the hall (`min_players`) — for the launcher
    min: usize,
    /// the "Clotho" tab and its minimum (`clotho_min`; in tests — custom)
    clotho: Tab,
    clotho_min: usize,
}

/// Sides of a Clotho match: game groups are not split, each goes to the side with fewer fighters, on a tie — the one with
/// fewer run wins (in the original opponents were matched by wins). `players` — (group key, open_id, wins).
/// Reply — side 1 or 2 for each.
fn clotho_sides(players: &[(i64, String, i32)]) -> HashMap<String, i64> {
    let mut groups: Vec<(i64, Vec<(String, i32)>)> = Vec::new();
    for (g, o, w) in players {
        match groups.iter_mut().find(|(k, _)| k == g) {
            Some((_, list)) => list.push((o.clone(), *w)),
            None => groups.push((*g, vec![(o.clone(), *w)])),
        }
    }
    let wins = |list: &Vec<(String, i32)>| list.iter().map(|(_, w)| *w).sum::<i32>();
    // larger groups first, then strong ones — this way solo players fill up the sides at the end
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(wins(&b.1).cmp(&wins(&a.1))).then(a.0.cmp(&b.0)));
    let mut size = [0usize; 2];
    let mut score = [0i32; 2];
    let mut out = HashMap::new();
    for (_, list) in &groups {
        let side = if size[0] != size[1] { usize::from(size[1] < size[0]) } else { usize::from(score[1] < score[0]) };
        size[side] += list.len();
        score[side] += wins(list);
        for (o, _) in list {
            out.insert(o.clone(), side as i64 + 1);
        }
    }
    out
}

impl Lobby {
    fn new(limits: Vec<(usize, usize)>) -> Self {
        Self::new_with(limits, false)
    }

    /// Public view of the tabs (`public_instances`): phase, map, how many signed up and how many in a match; `in_battle` —
    /// room members without a result (in tests — custom)
    fn public_view(&self, now: i64, in_battle: &dyn Fn(&str) -> usize) -> Vec<Value> {
        self.tabs
            .iter()
            .map(|t| {
                let closed = t.opens_at > now;
                let level = if t.level_id != 0 { Some(t.level_id) } else { t.leader() };
                let seats = match level {
                    Some(m) => (self.seats)(m, t.party_size),
                    None => t.need(&self.seats),
                };
                let name = match (t.lane, t.party_size) {
                    (1..=3, _) => format!("Соло · {}", LANE_TITLES[t.lane - 1]),
                    (_, 1) => "Соло".to_string(),
                    (_, 2) => "Дуо".to_string(),
                    (_, 3) => "Трио".to_string(),
                    (_, n) => format!("Группа {n}"),
                };
                json!({
                    "id": t.id,
                    "name": name,
                    "mode": match t.party_size { 1 => "solo", 2 => "duo", 3 => "trio", _ => "other" },
                    "party_size": t.party_size,
                    "difficulty": if t.lane > 0 { Value::from(LANE_NAMES[t.lane - 1]) } else { Value::Null },
                    "phase": if closed { "closed" } else { t.phase.name() },
                    "level_id": level,
                    "map": level.and_then(map_title),
                    "queued": t.players.len(),
                    "in_battle": t.battle_id.as_deref().map_or(0, in_battle),
                    "seats": seats,
                    "ends_at": if closed { t.opens_at } else { t.ends_at },
                })
            })
            .collect()
    }

    fn new_with(limits: Vec<(usize, usize)>, lanes: bool) -> Self {
        let mut l = Lobby {
            tabs: Vec::new(),
            limits,
            last_id: 0,
            lanes,
            lane_next: [0; 3],
            seats: matchmaking::room_seats,
            stagger: None,
            fill: fill_secs(),
            min: min_players().max(1),
            clotho: Tab::new(),
            clotho_min: clotho_min(),
        };
        l.ensure_open();
        l
    }

    /// Each size has a free instance for gathering while there are fewer of them (counting running matches) than the limit
    fn ensure_open(&mut self) {
        for (size, limit) in self.limits.clone() {
            // 13.11 permanent gathering (lanes): tabs are created once and do not disappear — solo by difficulty, duo and trio
            // by limit; after a match the same tab gathers again (step, `gone`)
            if self.lanes && size > 1 {
                while self.tabs.iter().filter(|t| t.party_size == size).count() < limit {
                    let id = self.next_id();
                    self.tabs.push(Tab::sized(id, size));
                    println!("[Lobby] instance {} ({}) open for gathering", id, size_name(size));
                }
                continue;
            }
            if size == 1 && self.lanes {
                for lane in 1..=SOLO_LANES.len() {
                    let open = self.tabs.iter().any(|t| t.lane == lane);
                    if !open {
                        let id = self.next_id();
                        let level = SOLO_LANES[lane - 1][self.lane_next[lane - 1] % 2];
                        self.lane_next[lane - 1] += 1;
                        self.tabs.push(Tab { lane, level_id: level, ..Tab::sized(id, 1) });
                        println!("[Lobby] instance {} (solo, {}) open for gathering, map {}", id, LANE_NAMES[lane - 1], level);
                    }
                }
                continue;
            }
            let of_size = self.tabs.iter().filter(|t| t.party_size == size).count();
            let open = self.tabs.iter().any(|t| t.party_size == size && matches!(t.phase, Phase::Idle | Phase::Gather));
            if !open && of_size < limit {
                let id = self.next_id();
                self.tabs.push(Tab::sized(id, size));
                println!("[Lobby] instance {} ({}) open for gathering", id, size_name(size));
            }
        }
        self.tabs.sort_by_key(|t| (t.party_size, t.lane, t.id));
    }

    /// 13.11 stagger at startup: tabs open in turn — solo·Easy, duo, trio, solo·Classic, duo, solo·Hard
    /// (by number within its group, then by size); the first right away, each next one [min, max] seconds after
    /// the previous. Once per backend start: after a match a tab gathers again right away.
    fn apply_stagger(&mut self, now: i64, mut roll: u64) {
        let Some((lo, hi)) = self.stagger else { return };
        let mut order: Vec<(usize, usize, usize)> = Vec::new(); // (number within group, size, tab index)
        let mut seen: HashMap<usize, usize> = HashMap::new();
        for (i, tab) in self.tabs.iter().enumerate() {
            let k = seen.entry(tab.party_size).or_default();
            order.push((*k, tab.party_size, i));
            *k += 1;
        }
        order.sort();
        let mut at = now;
        for (n, (_, _, i)) in order.into_iter().enumerate() {
            if n > 0 {
                roll = roll.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                at += lo + ((roll >> 33) % (hi - lo + 1) as u64) as i64;
            }
            self.tabs[i].opens_at = if n == 0 { 0 } else { at };
            if n > 0 {
                println!("[Lobby] instance {}: opens in {} min", self.tabs[i].id, (at - now) / 60);
            }
        }
    }

    /// Number of a new instance: 1…99 in a cycle, not taken (100 — "Clotho")
    fn next_id(&mut self) -> usize {
        loop {
            self.last_id = self.last_id % 99 + 1;
            if !self.tabs.iter().any(|t| t.id == self.last_id) {
                return self.last_id;
            }
        }
    }

    fn index_of(&self, id: usize) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    fn tab_of(&self, open_id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.players.contains_key(open_id))
    }

    fn in_clotho(&self, open_id: &str) -> bool {
        self.clotho.players.contains_key(open_id)
    }

    /// Sign-up to the "Clotho" tab (the run is already checked by `join_async`): the player leaves regular tabs
    fn join_clotho(&mut self, open_id: &str, user_id: u64, wins: i32, now: i64) -> Result<(), Refused> {
        if self.in_clotho(open_id) {
            if let Some(p) = self.clotho.players.get_mut(open_id) {
                p.seen = now;
                p.wins = wins;
            }
            return Ok(());
        }
        if !matches!(self.clotho.phase, Phase::Idle | Phase::Gather) {
            return Err(Refused::Battle);
        }
        for t in &mut self.tabs {
            t.players.remove(open_id);
        }
        let c = &mut self.clotho;
        c.players.insert(open_id.to_string(), Player { seen: now, user_id, wins, ..Default::default() });
        c.phase = Phase::Gather;
        println!("[Lobby] Clotho: gathering {}/{} ({} signed up, wins {})", c.players.len(), self.clotho_min, open_id, wins);
        Ok(())
    }

    /// `late` — whether one may enter while the room is open (the hall accepts, there are seats)
    fn join(&mut self, open_id: &str, instance: usize, now: i64, late: &dyn Fn(&str) -> Result<(), Refused>) -> Result<(), Refused> {
        let Some(idx) = self.index_of(instance) else { return Err(Refused::BadInstance) };
        let tab = &self.tabs[idx];
        if self.tab_of(open_id) == Some(idx) {
            self.touch(open_id, now);
            return Ok(());
        }
        // 13.11 (user: "no filling", "Refuse sign-up"): a solo lane starts only on the timer, if the map's seats
        // are taken — the sign-up is refused, the player waits for the next instance of this difficulty
        if tab.opens_at > now {
            return Err(Refused::NotOpen);
        }
        let cap = if tab.lane > 0 { (self.seats)(tab.level_id, tab.party_size).max(1) } else { tab.need(&self.seats) };
        if (tab.lane > 0 || self.lanes) && matches!(tab.phase, Phase::Idle | Phase::Gather) && tab.players.len() >= cap {
            return Err(Refused::Full);
        }
        match (tab.phase, &tab.battle_id) {
            (Phase::Battle, _) => return Err(Refused::Battle),
            (Phase::Starting | Phase::Waitroom, Some(b)) => late(b)?,
            _ => {}
        }
        for t in &mut self.tabs {
            t.players.remove(open_id);
        }
        if self.clotho.phase == Phase::Gather {
            self.clotho.players.remove(open_id);
        }
        let fill = self.fill;
        let tab = &mut self.tabs[idx];
        tab.players.insert(open_id.to_string(), Player { seen: now, ..Default::default() });
        if tab.phase == Phase::Idle {
            tab.phase = Phase::Gather;
            tab.ends_at = now + fill;
            println!("[Lobby] instance {} ({}): gathering, start in at most {} s", instance, size_name(tab.party_size), fill);
        }
        Ok(())
    }

    fn leave(&mut self, open_id: &str) {
        for t in self.tabs.iter_mut() {
            if t.players.remove(open_id).is_some() && t.voting() {
                println!("[Lobby] instance {}: {} left the gathering ({} remaining)", t.id, open_id, t.players.len());
            }
        }
        if self.clotho.phase == Phase::Gather && self.clotho.players.remove(open_id).is_some() {
            println!("[Lobby] Clotho: {} left ({} remaining)", open_id, self.clotho.players.len());
        }
    }

    fn vote(&mut self, open_id: &str, party_size: Option<usize>, level_id: Option<i64>, now: i64) -> Result<(), Refused> {
        let i = self.tab_of(open_id).ok_or(Refused::NotInLobby)?;
        let tab = &mut self.tabs[i];
        let phase = tab.phase;
        tab.players.get_mut(open_id).ok_or(Refused::NotInLobby)?.seen = now;
        // an instance has its own size: a vote for it is accepted only if it matches (old launchers send it)
        if phase != Phase::Gather {
            return Err(Refused::VoteOver);
        }
        let size = tab.party_size;
        let lane = tab.lane;
        let p = tab.players.get_mut(open_id).ok_or(Refused::NotInLobby)?;
        // solo lane: the map is preset, a map vote is accepted with no effect (old launchers show voting)
        if lane > 0 && party_size.map_or(true, |n| n == size) {
            return Ok(());
        }
        match (party_size, level_id) {
            (Some(n), _) if n != size => Err(Refused::BadVote),
            (_, Some(l)) if !MAPS.contains(&l) => Err(Refused::BadVote),
            (_, Some(l)) => {
                p.map_vote = Some(l);
                Ok(())
            }
            (Some(_), None) => Ok(()),
            (None, None) => Err(Refused::BadVote),
        }
    }

    fn touch(&mut self, open_id: &str, now: i64) {
        if let Some(i) = self.tab_of(open_id) {
            if let Some(p) = self.tabs[i].players.get_mut(open_id) {
                p.seen = now;
            }
        }
        if let Some(p) = self.clotho.players.get_mut(open_id) {
            p.seen = now;
        }
    }

    /// A pass over the "Clotho" tab. `group` — game group key by user_id (parties are not split). `start` creates the match room
    /// with all fighters (open_id, user_id, side) and returns battle_id, None — no free server (the gathering holds,
    /// retry on the next tick). `clear` removes the match teams when the room is gone.
    #[allow(clippy::too_many_arguments)]
    fn step_clotho(
        &mut self,
        now: i64,
        blocked: &dyn Fn(&str) -> bool,
        hall: &dyn Fn(&str) -> Hall,
        online: &dyn Fn(&str) -> bool,
        group: &dyn Fn(u64) -> i64,
        start: &mut dyn FnMut(&[(String, u64, i64)]) -> Option<String>,
        clear: &mut dyn FnMut(&[u64]),
    ) {
        let min = self.clotho_min;
        let c = &mut self.clotho;
        if matches!(c.phase, Phase::Idle | Phase::Gather) {
            // in the gathering — those in the game or with a live launcher and not blocked by maintenance
            c.players.retain(|o, p| {
                let keep = (now - p.seen <= SEEN_SECS || online(o)) && !blocked(o);
                if !keep {
                    println!("[Lobby] Clotho: {} removed from the gathering (launcher silent for {} s or maintenance)", o, now - p.seen);
                }
                keep
            });
        }
        match c.phase {
            Phase::Idle => {}
            Phase::Gather if c.players.is_empty() => {
                *c = Tab::new();
                println!("[Lobby] Clotho: gathering empty");
            }
            // start — when gathered and everyone signed up is in the game (the match goes right away, no hall)
            Phase::Gather if c.players.len() >= min && c.players.keys().filter(|o| online(o)).count() >= min => {
                let mut list: Vec<(&String, &Player)> = c.players.iter().filter(|(o, _)| online(o)).collect();
                list.sort_by(|a, b| a.0.cmp(b.0));
                let picked: Vec<(i64, String, i32)> =
                    list.iter().take(min).map(|(o, p)| (group(p.user_id), (*o).clone(), p.wins)).collect();
                let sides = clotho_sides(&picked);
                let fighters: Vec<(String, u64, i64)> =
                    picked.iter().map(|(_, o, _)| (o.clone(), c.players[o].user_id, sides[o])).collect();
                if let Some(battle) = start(&fighters) {
                    let shown: Vec<String> = fighters.iter().map(|(o, _, s)| format!("{}:{}", o, s)).collect();
                    println!("[Lobby] Clotho: match {}, sides {}", battle, shown.join(" "));
                    c.fighters = fighters.iter().map(|(_, u, _)| *u).collect();
                    c.players.retain(|o, _| sides.contains_key(o));
                    c.battle_id = Some(battle);
                    c.level_id = crate::rpc::handlers::TRIAL_LEVEL;
                    c.phase = Phase::Starting;
                }
            }
            Phase::Gather => {}
            Phase::Starting | Phase::Waitroom | Phase::Battle => match c.battle_id.as_deref().map_or(Hall::Gone, hall) {
                Hall::Gone => {
                    clear(&c.fighters);
                    *c = Tab::new();
                    println!("[Lobby] Clotho: match over, gathering open again");
                }
                Hall::Started => {
                    if c.phase != Phase::Battle {
                        println!("[Lobby] Clotho: match");
                    }
                    c.phase = Phase::Battle;
                }
                _ => {}
            },
        }
    }

    /// One pass of the instance state machine. `seats` — seats in a room (map, group size), `open` creates the room and
    /// returns its battle_id. `blocked` — the player is blocked by maintenance: they leave the gathering (our own, gm/admin, stay).
    /// Open rooms go on their own; when a match ends, the instance is removed and a new one opens next to it (`ensure_open`).
    #[allow(clippy::too_many_arguments)]
    fn step(
        &mut self,
        now: i64,
        roll: u64,
        blocked: &dyn Fn(&str) -> bool,
        hall: &dyn Fn(&str) -> Hall,
        online: &dyn Fn(&str) -> bool,
        seats: &dyn Fn(i64, usize) -> usize,
        open: &mut dyn FnMut(i64, usize) -> String,
    ) {
        let mut gone: Vec<usize> = Vec::new();
        let fixed = self.lanes;
        for tab in self.tabs.iter_mut() {
            let id = tab.id;
            let gathering = tab.voting();
            tab.players.retain(|o, p| {
                let keep = now - p.seen <= SEEN_SECS || online(o);
                if !keep && gathering {
                    println!("[Lobby] instance {}: {} dropped from the gathering — launcher silent for {} s, game offline", id, o, now - p.seen);
                }
                keep
            });
            if gathering {
                tab.players.retain(|o, _| {
                    let keep = !blocked(o);
                    if !keep {
                        println!("[Lobby] instance {}: {} removed from the gathering — maintenance", id, o);
                    }
                    keep
                });
            }
            match tab.phase {
                Phase::Idle => {}
                Phase::Gather if tab.players.is_empty() => {
                    let (lane, level) = (tab.lane, tab.level_id);
                    *tab = Tab { lane, level_id: if lane > 0 { level } else { 0 }, ..Tab::sized(id, tab.party_size) };
                    println!("[Lobby] instance {}: gathering empty", id);
                }
                Phase::Gather => {
                    let n = tab.players.len();
                    // solo lane — only on the timer (13.11, "no filling")
                    let full = !fixed && tab.lane == 0 && n >= tab.need(seats);
                    if !full && now < tab.ends_at {
                        continue;
                    }
                    let size = tab.party_size;
                    // full — the leading map (its seats were what we counted); on the timer — by vote shares among the maps
                    // where everyone fits
                    let level = match (full, tab.leader()) {
                        _ if tab.lane > 0 => tab.level_id,
                        (true, Some(m)) => m,
                        _ => {
                            let fits: Vec<i64> = MAPS.iter().copied().filter(|m| seats(*m, size) >= n).collect();
                            let options = if fits.is_empty() { MAPS.to_vec() } else { fits };
                            winner(&options, tab.players.values().filter_map(|p| p.map_vote), roll)
                        }
                    };
                    let battle = open(level, size);
                    println!(
                        "[Lobby] instance {} ({}): start — {}, map {}, signed up {}, room {}",
                        id,
                        size_name(size),
                        if full { "full" } else { "timer" },
                        level,
                        n,
                        battle
                    );
                    tab.level_id = level;
                    tab.battle_id = Some(battle);
                    tab.phase = Phase::Starting;
                    tab.ends_at = 0;
                }
                Phase::Starting | Phase::Waitroom | Phase::Battle => {
                    let state = tab.battle_id.as_deref().map_or(Hall::Gone, hall);
                    match state {
                        Hall::Gone => gone.push(id),
                        Hall::Preparing => {}
                        Hall::Waiting => tab.phase = if tab.phase == Phase::Battle { Phase::Battle } else { Phase::Waitroom },
                        Hall::Started => {
                            if tab.phase != Phase::Battle {
                                println!("[Lobby] instance {}: match", id);
                                // 13.11 (user: "match timer in the launcher"): the latest possible match end — the launcher
                                // shows it next to "Match in progress" (the same ends_at as for gathering and the hall)
                                tab.ends_at = now + battle_secs(tab.level_id);
                            }
                            tab.phase = Phase::Battle;
                            tab.players.clear();
                        }
                    }
                }
            }
        }
        if !gone.is_empty() && self.lanes {
            for t in self.tabs.iter_mut().filter(|t| gone.contains(&t.id)) {
                let (id, size, lane) = (t.id, t.party_size, t.lane);
                let level = if lane > 0 {
                    let l = SOLO_LANES[lane - 1][self.lane_next[lane - 1] % 2];
                    self.lane_next[lane - 1] += 1;
                    l
                } else {
                    0
                };
                *t = Tab { lane, level_id: level, ..Tab::sized(id, size) };
                println!("[Lobby] instance {}: match over, gathering again{}", id, if lane > 0 { format!(", map {}", level) } else { String::new() });
            }
            gone.clear();
        }
        if !gone.is_empty() {
            self.tabs.retain(|t| !gone.contains(&t.id));
            println!("[Lobby] instances {:?}: match over, removed", gone);
        }
        self.ensure_open();
    }
}

/// Level map name (messages to the player, public status)
pub fn map_title(level: i64) -> Option<&'static str> {
    Some(match level {
        40003015 => "Склеп (обычная)",
        40003016 => "Замок (обычная)",
        40003017 => "Склеп (Classic)",
        40003018 => "Замок (Classic)",
        40003019 => "Склеп (Hard)",
        40003020 => "Замок (Hard)",
        _ => return None,
    })
}

const LANE_TITLES: [&str; 3] = ["Простой", "Классик", "Хард"];

/// 15.11 public status (`GET /dbserver/status`, Discord bot, site, launcher): lobby tabs without personal data.
/// Empty — the launcher lobby is off.
pub fn public_instances() -> Vec<Value> {
    if !enabled() {
        return Vec::new();
    }
    let l = lobby().lock().unwrap();
    let mut v = l.public_view(now_ts(), &matchmaking::room_in_battle);
    if clotho_enabled() {
        let c = &l.clotho;
        v.push(json!({
            "id": CLOTHO_ID, "name": "Испытание Клото", "mode": "clotho", "party_size": 3, "difficulty": null,
            "phase": c.phase.name(), "level_id": null, "map": null, "queued": c.players.len(),
            "in_battle": c.battle_id.as_deref().map_or(0, matchmaking::room_in_battle), "seats": l.clotho_min, "ends_at": 0,
        }));
    }
    v
}

fn size_name(size: usize) -> &'static str {
    match size {
        1 => "solo",
        2 => "duo",
        3 => "trio",
        _ => "?",
    }
}

fn lobby() -> &'static Mutex<Lobby> {
    static L: OnceLock<Mutex<Lobby>> = OnceLock::new();
    L.get_or_init(|| {
        let mut l = Lobby::new_with(size_limits(), solo_lanes());
        if l.lanes {
            if let Some(s) = stagger_secs() {
                l.stagger = Some(s);
                l.apply_stagger(now_ts(), rand::random::<u64>());
            }
        }
        Mutex::new(l)
    })
}

fn hall_of(battle_id: &str) -> Hall {
    if matchmaking::room(battle_id).is_none() {
        return Hall::Gone;
    }
    match allocator::room_phase(battle_id).as_deref() {
        Some("waiting") => Hall::Waiting,
        Some("entering" | "battle") => Hall::Started,
        _ => Hall::Preparing,
    }
}

/// State machine tick (lobby background task)
pub fn tick() {
    if !enabled() {
        return;
    }
    let mut open = |level: i64, party: usize| matchmaking::open_lobby_room(level, party);
    let mut l = lobby().lock().unwrap();
    l.step(now_ts(), rand::random(), &maintenance::blocks, &hall_of, &hub::is_online, &matchmaking::room_seats, &mut open);
    if clotho_enabled() {
        let group = |user_id: u64| super::team::team_of(user_id).map_or(user_id as i64, |t| t.team_id);
        l.step_clotho(now_ts(), &maintenance::blocks, &hall_of, &hub::is_online, &group, &mut clotho_start, &mut clotho_clear);
    }
}

/// Clotho match room: match teams by side, then a room with all fighters on arena 30102 (like a custom game
/// start). No free server — the teams are removed, the gathering waits for the next tick.
fn clotho_start(fighters: &[(String, u64, i64)]) -> Option<String> {
    static SEQ: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 10_000_000 + 1;
    for (_, u, side) in fighters {
        super::team::set_room_battle_team(*u, CLOTHO_TEAM_BASE + n * 10 + side);
    }
    let members: Vec<Member> = fighters.iter().map(|(o, u, _)| Member { open_id: o.clone(), user_id: *u }).collect();
    let arena = clotho_arena(rand::random());
    println!("[Lobby] Clotho: match map {}", arena);
    match matchmaking::start_direct_on(members, crate::rpc::handlers::TRIAL_LEVEL, "clotho", Some(arena)) {
        Some(room) => Some(room.battle_id),
        None => {
            clotho_clear(&fighters.iter().map(|(_, u, _)| *u).collect::<Vec<_>>());
            None
        }
    }
}

fn clotho_clear(users: &[u64]) {
    for u in users {
        super::team::clear_room_battle_team(*u);
    }
}

/// DB and tables for checking the run (`trial_ensure_open`, `trial_me`): set by `ds::spawn` at backend start
static BACKEND: OnceLock<(crate::db::GameDb, std::sync::Arc<crate::game_data::GameData>)> = OnceLock::new();

pub fn set_backend(db: crate::db::GameDb, gd: std::sync::Arc<crate::game_data::GameData>) {
    let _ = BACKEND.set((db, gd));
}

/// The player's last known run state (for the launcher card; updated by `snapshot_async` and on sign-up)
fn trial_cache() -> &'static Mutex<HashMap<String, crate::rpc::handlers::TrialMe>> {
    static C: OnceLock<Mutex<HashMap<String, crate::rpc::handlers::TrialMe>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn trial_refresh(open_id: &str) -> Option<crate::rpc::handlers::TrialMe> {
    let (db, gd) = BACKEND.get()?;
    let me = crate::rpc::handlers::trial_me(db, gd, open_id).await;
    let mut cache = trial_cache().lock().unwrap();
    match &me {
        Some(t) => cache.insert(open_id.to_string(), t.clone()),
        None => cache.remove(open_id),
    };
    me
}

/// Sign-up to a tab with checks that need the DB: for "Clotho" — an open run (with stone 1033 it opens itself).
/// Regular tabs — same as `join`.
pub async fn join_async(open_id: &str, instance: usize) -> Result<(), Refused> {
    if instance != CLOTHO_ID {
        return join(open_id, instance);
    }
    if !clotho_enabled() {
        println!("[Lobby] {}: sign-up to \"Clotho\" refused — {}", open_id, Refused::ClothoOff.code());
        return Err(Refused::ClothoOff);
    }
    if maintenance::blocks(open_id) {
        return Err(Refused::Maintenance);
    }
    if lobby().lock().unwrap().in_clotho(open_id) {
        lobby().lock().unwrap().touch(open_id, now_ts());
        return Ok(());
    }
    let Some((db, gd)) = BACKEND.get() else { return Err(Refused::ClothoOff) };
    let trial = match crate::rpc::handlers::trial_ensure_open(db, gd, open_id).await {
        Ok(t) => t,
        Err(text) => {
            let me = trial_refresh(open_id).await;
            let reward = me.as_ref().is_some_and(|t| t.status == 2);
            println!("[Lobby] Clotho: {} not signed up — {}", open_id, text);
            return Err(if reward { Refused::TrialReward } else { Refused::TrialNotOpen });
        }
    };
    trial_cache().lock().unwrap().insert(open_id.to_string(), trial.clone());
    let user_id = db.get::<crate::structures::PlayerModel>("players", open_id).await.map_or(0, |p| p.user_id);
    lobby().lock().unwrap().join_clotho(open_id, user_id, trial.wins, now_ts())
}

/// Snapshot for the launcher with the player's run state (`me.trial`)
pub async fn snapshot_async(open_id: &str) -> Value {
    if clotho_enabled() {
        trial_refresh(open_id).await;
    }
    snapshot(open_id)
}

/// Whether an instance is still needed: voting is on or the room is waiting for a server (`want_flags` starts the process in advance)
pub fn needs_server() -> bool {
    // during maintenance only our own people stay in voting (step), they need a server as usual
    if !enabled() {
        return false;
    }
    let l = lobby().lock().unwrap();
    // "Clotho": the gathering is almost full — prepare the server in advance (the match starts right away, no hall)
    let clotho = l.clotho.phase == Phase::Gather && l.clotho.players.len() + 1 >= l.clotho_min;
    clotho
        || l.tabs.iter().any(|t| match t.phase {
            Phase::Gather => true,
            Phase::Starting => t.battle_id.as_deref().and_then(matchmaking::room).is_some_and(|r| r.ds_address.is_none()),
            _ => false,
        })
}

fn late_join(battle_id: &str) -> Result<(), Refused> {
    match matchmaking::room_join_state(battle_id, 1) {
        Ok(()) | Err(JoinRefused::NotReady) => Ok(()),
        Err(JoinRefused::Full) => Err(Refused::Full),
        Err(_) => Err(Refused::Closed),
    }
}

pub fn join(open_id: &str, instance: usize) -> Result<(), Refused> {
    let r = if maintenance::blocks(open_id) { Err(Refused::Maintenance) } else { lobby().lock().unwrap().join(open_id, instance, now_ts(), &late_join) };
    // 13.11 (user, investigating player E: "why couldn't I get into the instance"): a sign-up refusal goes to the log, with the reason
    if let Err(e) = &r {
        println!("[Lobby] {}: sign-up to instance {} refused — {}", open_id, instance, e.code());
    }
    r
}

pub fn leave(open_id: &str) {
    lobby().lock().unwrap().leave(open_id);
}

pub fn vote(open_id: &str, party_size: Option<usize>, level_id: Option<i64>) -> Result<(), Refused> {
    if maintenance::blocks(open_id) {
        return Err(Refused::Maintenance);
    }
    lobby().lock().unwrap().vote(open_id, party_size, level_id, now_ts())
}

/// Everything for the launcher screen; also marks the player's launcher as alive
pub fn snapshot(open_id: &str) -> Value {
    let now = now_ts();
    let mut l = lobby().lock().unwrap();
    l.touch(open_id, now);
    // 13.11 (user: "3 max!"): solo lane — one tab per difficulty. Someone signed up to a started instance
    // (hall) is shown that one, others — the open gathering of this difficulty; running solo matches are not shown
    let mine = l.tab_of(open_id).map(|i| (l.tabs[i].id, l.tabs[i].lane, l.tabs[i].phase));
    // 13.11 later (permanent gathering 8dd35af, user: "5 instances in the launcher instead of 6"): exactly 6 tabs —
    // we show all of them, in any phase (match — with a timer)
    let fixed = l.lanes;
    let shown = |t: &Tab| -> bool {
        if t.lane == 0 || fixed {
            return true;
        }
        let open = matches!(t.phase, Phase::Idle | Phase::Gather);
        match mine {
            Some((id, lane, ph)) if lane == t.lane && !matches!(ph, Phase::Idle | Phase::Gather) => t.id == id,
            _ => open,
        }
    };
    let instances: Vec<Value> = l
        .tabs
        .iter()
        .filter(|t| shown(t))
        .map(|t| {
            let room = t.battle_id.as_deref().and_then(matchmaking::room);
            let mut sizes: HashMap<String, usize> = HashMap::new();
            let mut maps: HashMap<String, usize> = HashMap::new();
            for p in t.players.values() {
                if let Some(n) = p.size_vote {
                    *sizes.entry(n.to_string()).or_default() += 1;
                }
                if let Some(m) = p.map_vote {
                    *maps.entry(m.to_string()).or_default() += 1;
                }
            }
            // hall: `ends_at` — until the end of the "Start" window, not until the end of the hall timer
            let closed = t.opens_at > now;
            let ends_at = match (t.phase, t.battle_id.as_deref()) {
                _ if closed => t.opens_at,
                (Phase::Waitroom, Some(b)) => allocator::hall_join_secs(b).map_or(0, |s| now + s),
                _ => t.ends_at,
            };
            // seats: after the start — of the room, during gathering — how many are needed for a fill-based start
            let seats = if t.level_id != 0 { matchmaking::room_seats(t.level_id, t.party_size) } else { t.need(&matchmaking::room_seats) };
            json!({
                "id": t.id,
                "kind": "fill",
                "phase": if closed { "closed" } else { t.phase.name() },
                "min_players": l.min,
                "ends_at": ends_at,
                "players": t.players.len(),
                "in_hall": room.as_ref().map_or(0, |r| r.members.len()),
                "seats": seats,
                "party_size": t.party_size,
                "leader": t.leader(),
                "level_id": t.level_id,
                // 13.11 solo lane: 1 "Easy", 2 "Classic", 3 "Hard"; the map is preset (`level_id`), no voting
                "lane": t.lane,
                "difficulty": if t.lane > 0 { Value::from(LANE_NAMES[t.lane - 1]) } else { Value::Null },
                "votes": { "sizes": sizes, "maps": maps },
            })
        })
        .collect();
    let mut instances = instances;
    if clotho_enabled() {
        let c = &l.clotho;
        let room = c.battle_id.as_deref().and_then(matchmaking::room);
        instances.push(json!({
            "id": CLOTHO_ID,
            "kind": "clotho",
            "phase": c.phase.name(),
            "min_players": l.clotho_min,
            "ends_at": 0,
            "players": c.players.len(),
            "in_hall": room.as_ref().map_or(0, |r| r.members.len()),
            "seats": l.clotho_min,
            "party_size": 3,
            "level_id": crate::rpc::handlers::TRIAL_LEVEL,
            "arena_map_id": CLOTHO_ARENA,
            "votes": { "sizes": {}, "maps": {} },
        }));
    }
    let me = l.tab_of(open_id).map(|i| {
        let t = &l.tabs[i];
        let p = &t.players[open_id];
        (t.id, Some(t.party_size), p.map_vote)
    });
    let me = me.or_else(|| l.in_clotho(open_id).then_some((CLOTHO_ID, None, None)));
    let trial = trial_cache().lock().unwrap().get(open_id).cloned();
    json!({
        "enabled": true,
        "maintenance": maintenance::message_for(open_id),
        "maintenance_i18n": maintenance::i18n_for(open_id),
        "now": now,
        "maps": MAPS,
        "sizes": SIZES,
        "min_players": l.min,
        "fill_secs": l.fill,
        "rules": map_rules(),
        "instances": instances,
        "me": {
            "instance": me.map(|m| m.0),
            "size_vote": me.and_then(|m| m.1),
            "map_vote": me.and_then(|m| m.2),
            "game_online": hub::is_online(open_id),
            // Clotho Trial: status 0 — no run, 1 — running, 2 — waiting for a reward; tokens — losses until the end
            "trial": trial,
        },
    })
}

/// Map matchmaking rules for the launcher (08.11): `skirmish` — skirmish mode (groups of any size together, Hard),
/// `gear` — gear rarity limit ("epic" — all slots epic and above, as in the original on Hard; "green" — only grey
/// and green, 08.11 evening; null — none), `challenger` — whether "challenge mode" applies (solo and duo against trio)
pub fn map_rules() -> Value {
    let checks = super::match_checks::server();
    rules_with(checks.gear_rarity, &checks.gear_levels)
}

/// Limit word for the launcher from the level's rarity list; a list that cannot be described this way — no hint
fn gear_word(list: &[String]) -> Option<&'static str> {
    const GREEN: [&str; 4] = ["EN_ITEM_RARITY_NONE", "EN_ITEM_RARITY_ROTTED", "EN_ITEM_RARITY_NORMAL", "EN_ITEM_RARITY_UNCOMMON"];
    const EPIC: [&str; 4] = ["EN_ITEM_RARITY_EPIC", "EN_ITEM_RARITY_EXCEPTIONAL", "EN_ITEM_RARITY_LEGENDARY", "EN_ITEM_RARITY_UNIQUE"];
    let all_in = |set: &[&str]| !list.is_empty() && list.iter().all(|r| set.contains(&r.as_str()));
    if all_in(&GREEN) {
        Some("green")
    } else if all_in(&EPIC) {
        Some("epic")
    } else {
        None
    }
}

fn rules_with(gear_on: bool, levels: &std::collections::BTreeMap<String, Vec<String>>) -> Value {
    let rules: serde_json::Map<String, Value> = MAPS
        .iter()
        .map(|&l| {
            let skirmish = matchmaking::skirmish(l);
            let gear = match levels.get(&l.to_string()) {
                Some(list) => gear_word(list),
                None => (gear_on && skirmish).then_some("epic"),
            };
            (l.to_string(), json!({ "skirmish": skirmish, "gear": gear, "challenger": !skirmish }))
        })
        .collect();
    Value::Object(rules)
}

/// In-game window text if the "Start" button did not work
fn refused_text(r: JoinRefused) -> String {
    match r {
        JoinRefused::NotReady => "Сервер ещё запускается. Нажми «Старт» через минуту.".to_string(),
        JoinRefused::Left => "Ты вышел из этого зала. Жди следующий раунд.".to_string(),
        JoinRefused::Full => "В зале нет мест. Жди следующий раунд.".to_string(),
        JoinRefused::Closed => "Зал закрылся. Жди следующий раунд.".to_string(),
    }
}

/// The group does not fit the round: larger than the size — always, smaller — only with exact size
fn party_refused(group: usize, party_size: usize, exact: bool) -> bool {
    group > party_size || (exact && group != party_size)
}

/// The in-game "Start" button: group `members` (the first is whoever pressed) goes to the hall of its tab.
/// Ok — (matching_id, level_id), Err — in-game window text.
/// The map that won the vote of the player's tab (the room is open); None — not in a tab or voting is not over
pub fn voted_level(open_id: &str) -> Option<i64> {
    let l = lobby().lock().unwrap();
    if l.in_clotho(open_id) {
        return l.clotho.battle_id.is_some().then_some(crate::rpc::handlers::TRIAL_LEVEL);
    }
    let tab = &l.tabs[l.tab_of(open_id)?];
    (tab.battle_id.is_some() && tab.level_id != 0).then_some(tab.level_id)
}

pub fn enter(me: &str, members: &[Member], challenger: bool) -> Result<(String, i64), String> {
    let (battle_id, level_id) = {
        let l = lobby().lock().unwrap();
        // "Clotho": the fighters are already in the match room (start_direct_on) — "To battle" from the event window only confirms it
        if l.in_clotho(me) {
            let c = &l.clotho;
            return match &c.battle_id {
                Some(b) => Ok((b.clone(), crate::rpc::handlers::TRIAL_LEVEL)),
                None => Err(format!(
                    "Испытание Клото: набираем игроков, {} из {}. Матч начнётся сам, когда наберётся {}.",
                    c.players.len(),
                    l.clotho_min,
                    l.clotho_min
                )),
            };
        }
        let Some(tab) = l.tab_of(me).map(|i| &l.tabs[i]) else {
            return Err("Выбери инстанс в лаунчере.".to_string());
        };
        if tab.phase == Phase::Gather {
            let need = tab.need(&matchmaking::room_seats);
            let left = ((tab.ends_at - now_ts()).max(0) + 59) / 60;
            return Err(format!(
                "Набираем игроков: {} из {}. Старт, когда наберётся {}, или через {} мин.",
                tab.players.len(),
                need,
                need,
                left
            ));
        }
        let Some(battle_id) = tab.battle_id.clone() else {
            return Err("Выбери инстанс и проголосуй в лаунчере.".to_string());
        };
        let checks = super::match_checks::server();
        // season with mixed rooms: the size from the vote is a cap (solo — only solo, duo — duo and solo, trio —
        // all), a smaller group passes even with lobby_party_exact; seats and spawn point sets are checked by join_room
        // Hard — skirmish mode; "challenge mode" — solo and duo into a trio room (as in regular search)
        let exact = checks.lobby_party_exact && !matchmaking::mixed_level(tab.level_id) && !(challenger && tab.party_size == 3);
        if party_refused(members.len(), tab.party_size, exact) {
            return Err(format!("В этом раунде нужна группа на {} (сейчас {}).", tab.party_size, members.len()));
        }
        if checks.lobby_same_tab && members.iter().any(|m| !tab.players.contains_key(&m.open_id)) {
            return Err("Напарник не в этом инстансе: все участники группы должны войти в ту же вкладку лаунчера.".to_string());
        }
        (battle_id, tab.level_id)
    };
    let matching_id = matchmaking::join_room(members.to_vec(), &battle_id).map_err(refused_text)?;
    println!("[Lobby] {}: group of {} entered room {}", me, members.len(), battle_id);
    Ok((matching_id, level_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn party_size_rules() {
        assert!(!party_refused(3, 3, true));
        assert!(party_refused(1, 3, true));
        assert!(!party_refused(1, 3, false));
        assert!(party_refused(3, 1, false));
    }

    fn ok_late(_: &str) -> Result<(), Refused> {
        Ok(())
    }

    /// In tests: 4 seats per group size (solo 4, duo 8, trio 12) on any map except MAPS[5] — half as many there
    fn seats(level: i64, size: usize) -> usize {
        if level == MAPS[5] { 2 * size } else { 4 * size }
    }

    fn step(l: &mut Lobby, now: i64, hall: Hall, opened: &mut Vec<(i64, usize)>) {
        let mut open = |level: i64, party: usize| {
            opened.push((level, party));
            format!("b{}", opened.len())
        };
        l.step(now, 0, &|_| false, &|_| hall, &|_| true, &seats, &mut open);
    }

    /// solo 2, duo 1, trio 1; gathering 480 s
    fn lobby() -> Lobby {
        let mut l = Lobby::new(vec![(1, 2), (2, 1), (3, 1)]);
        l.fill = 480;
        l
    }

    fn ids(l: &Lobby, size: usize) -> Vec<(usize, Phase)> {
        l.tabs.iter().filter(|t| t.party_size == size).map(|t| (t.id, t.phase)).collect()
    }

    #[test]
    fn winner_counts_votes_and_breaks_ties() {
        // proportional: 3,3,1 — trio 2/3, solo 1/3 (option order: solo, duo, trio)
        assert_eq!(winner(&SIZES, [3, 3, 1].into_iter(), 0), 1);
        assert_eq!(winner(&SIZES, [3, 3, 1].into_iter(), 1), 3);
        assert_eq!(winner(&SIZES, [3, 3, 1].into_iter(), 2), 3);
        assert_eq!(winner(&SIZES, [1, 2].into_iter(), 0), 1);
        assert_eq!(winner(&SIZES, [1, 2].into_iter(), 1), 2);
        // no votes — any option
        assert_eq!(winner(&SIZES, std::iter::empty(), 2), 3);
        assert_eq!(winner(&MAPS, [40003018].into_iter(), 5), 40003018);
        // shares over all rolls: 3 votes against 1 — exactly 75% / 25%, an option without votes is never picked
        let votes = [MAPS[0], MAPS[0], MAPS[0], MAPS[3]];
        let n = 4000u64;
        let first = (0..n).filter(|r| winner(&MAPS, votes.into_iter(), *r) == MAPS[0]).count();
        let other = (0..n).filter(|r| winner(&MAPS, votes.into_iter(), *r) == MAPS[3]).count();
        assert_eq!((first, other), (3000, 1000));
        assert!((0..n).all(|r| [MAPS[0], MAPS[3]].contains(&winner(&MAPS, votes.into_iter(), r))));
    }

    /// 11.11: one instance per size; full — start and a new one of the same size next to it, up to the limit;
    /// match over — the instance is removed, gathering open again
    #[test]
    fn fill_starts_and_next_instance_opens() {
        let mut l = lobby();
        let mut opened = vec![];
        assert_eq!(ids(&l, 1), vec![(1, Phase::Idle)]);
        assert_eq!(ids(&l, 2), vec![(2, Phase::Idle)]);
        assert_eq!(ids(&l, 3), vec![(3, Phase::Idle)]);
        // solo: 4 seats (without votes — the largest map)
        for (i, o) in ["a", "b", "c"].iter().enumerate() {
            l.join(o, 1, 100 + i as i64, &ok_late).unwrap();
        }
        assert_eq!(l.tabs[0].phase, Phase::Gather);
        assert_eq!(l.tabs[0].ends_at, 100 + 480, "timer from the first sign-up");
        step(&mut l, 110, Hall::Preparing, &mut opened);
        assert!(opened.is_empty(), "3 of 4 — waiting");
        l.join("d", 1, 111, &ok_late).unwrap();
        step(&mut l, 112, Hall::Preparing, &mut opened);
        assert_eq!(opened, vec![(MAPS[0], 1)], "full — start; without votes roll 0 — the first map");
        // a second solo opened next to it (limit 2)
        assert_eq!(ids(&l, 1), vec![(1, Phase::Starting), (4, Phase::Idle)]);
        for o in ["e", "f", "g", "h"] {
            l.join(o, 4, 113, &ok_late).unwrap();
        }
        step(&mut l, 114, Hall::Preparing, &mut opened);
        assert_eq!(ids(&l, 1), vec![(1, Phase::Starting), (4, Phase::Starting)], "limit 2 — no third one");
        // the first match went through and ended — removed, a new gathering opened
        step(&mut l, 120, Hall::Started, &mut opened);
        assert_eq!(l.tabs[0].phase, Phase::Battle);
        assert_eq!(l.tabs[0].ends_at, 120 + 2400, "match timer: start + the map's match length (without tables — 40 min)");
        assert!(l.tabs[0].players.is_empty());
        assert_eq!(l.join("x", 1, 121, &ok_late), Err(Refused::Battle));
        let mut open = |_: i64, _: usize| -> String { panic!("no new rooms") };
        l.step(130, 0, &|_| false, &|b| if b == "b1" { Hall::Gone } else { Hall::Started }, &|_| true, &seats, &mut open);
        let solo = ids(&l, 1);
        assert_eq!(solo.len(), 2);
        assert!(solo.contains(&(4, Phase::Battle)));
        assert!(solo.iter().any(|(id, ph)| *id != 4 && *ph == Phase::Idle), "a new gathering instead of the finished one");
    }

    /// 13.11 match timer: length from the map table — 40 minutes for all dungeons and the castle
    #[test]
    fn battle_length_from_map_detail() {
        let gd = crate::game_data::GameData::load_all("ExcelConfig");
        for level in MAPS {
            assert_eq!(map_timeout(&gd, level), Some(2400), "{}", level);
        }
    }

    /// 13.11 stagger at startup: the first tab is open right away, the next ones in turn every 10–20 min; a closed one does not let in
    #[test]
    fn stagger_opens_tabs_one_by_one() {
        let mut l = Lobby::new_with(vec![(1, 3), (2, 2), (3, 1)], true);
        l.stagger = Some((600, 1200));
        l.apply_stagger(1000, 42);
        let mut opens: Vec<(i64, usize, usize)> = l.tabs.iter().map(|t| (t.opens_at, t.party_size, t.lane)).collect();
        opens.sort();
        assert_eq!(opens[0], (0, 1, 1), "right away — solo·Easy");
        assert_eq!((opens[1].1, opens[2].1, opens[3].1), (2, 3, 1), "then duo, trio, solo");
        for w in opens.windows(2).skip(1) {
            let gap = w[1].0 - w[0].0;
            assert!((600..=1200).contains(&gap), "step {}", gap);
        }
        assert!((1600..=2200).contains(&opens[1].0));
        let duo = l.tabs.iter().find(|t| t.opens_at == opens[1].0).unwrap().id;
        assert_eq!(l.join("a", duo, 1001, &ok_late), Err(Refused::NotOpen));
        assert_eq!(l.join("a", duo, opens[1].0, &ok_late), Ok(()), "the time has come — lets in");
        let first = l.tabs.iter().find(|t| t.opens_at == 0).unwrap().id;
        assert_eq!(l.join("b", first, 1001, &ok_late), Ok(()));
    }

    /// 15.11 public status: 6 tabs with names and maps, sign-ups and matches are counted, no personal data
    #[test]
    fn public_view_lists_tabs_without_players() {
        let mut l = Lobby::new_with(vec![(1, 3), (2, 2), (3, 1)], true);
        l.fill = 480;
        l.seats = seats;
        let hard = l.tabs.iter().find(|t| t.lane == 3).unwrap().id;
        l.join("76561197960265729", hard, 100, &ok_late).unwrap();
        let v = l.public_view(101, &|_| 0);
        assert_eq!(v.len(), 6);
        let names: Vec<&str> = v.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["Соло · Простой", "Соло · Классик", "Соло · Хард", "Дуо", "Дуо", "Трио"]);
        let h = v.iter().find(|t| t["id"] == hard).unwrap();
        assert_eq!((h["phase"].as_str(), h["queued"].as_u64(), h["map"].as_str()), (Some("gather"), Some(1), Some("Склеп (Hard)")));
        assert_eq!(h["seats"], 4);
        assert!(!serde_json::to_string(&v).unwrap().contains("76561197960265729"), "open_id does not leak out");
        let mut opened = vec![];
        step(&mut l, 100 + 480, Hall::Preparing, &mut opened);
        step(&mut l, 600, Hall::Started, &mut opened);
        let v = l.public_view(601, &|b| if b == "b1" { 1 } else { 0 });
        let h = v.iter().find(|t| t["id"] == hard).unwrap();
        assert_eq!((h["phase"].as_str(), h["in_battle"].as_u64()), (Some("battle"), Some(1)));
        let trio = v.iter().find(|t| t["mode"] == "trio").unwrap();
        assert_eq!((trio["difficulty"].clone(), trio["map"].clone()), (Value::Null, Value::Null), "without votes the map is not known");
    }

    /// 13.11 permanent gathering ("all instances open, remove the mechanics of filling and opening new ones"): 3 solo by
    /// difficulty + duo and trio by limit, tabs do not appear or disappear; start only on the timer, no seats — refusal;
    /// match over — the same tab gathers again, for solo — on another map
    #[test]
    fn fixed_instances_timer_only_and_reuse_after_battle() {
        let mut l = Lobby::new_with(vec![(1, 3), (2, 2), (3, 1)], true);
        l.fill = 480;
        l.seats = seats;
        let solo = |l: &Lobby| l.tabs.iter().filter(|t| t.party_size == 1).map(|t| (t.lane, t.level_id)).collect::<Vec<_>>();
        assert_eq!(solo(&l), vec![(1, 40003015), (2, 40003017), (3, 40003019)], "three solo, Crypt of each difficulty");
        assert_eq!(l.tabs.len(), 6, "3 solo + 2 duo + 1 trio");
        let hard = l.tabs.iter().find(|t| t.lane == 3).unwrap().id;
        let mut opened = vec![];
        for (i, o) in ["a", "b", "c", "d"].iter().enumerate() {
            l.join(o, hard, 100 + i as i64, &ok_late).unwrap();
        }
        assert_eq!(l.vote("a", None, Some(MAPS[0]), 104), Ok(()), "a vote in solo has no effect");
        assert_eq!(l.join("e", hard, 105, &ok_late), Err(Refused::Full), "4 of 4 — no seats");
        step(&mut l, 112, Hall::Preparing, &mut opened);
        assert!(opened.is_empty(), "no fill-based start");
        step(&mut l, 100 + 480, Hall::Preparing, &mut opened);
        assert_eq!(opened, vec![(40003019, 1)], "start on the timer on the preset map");
        assert_eq!(l.tabs.len(), 6, "no new tabs");
        assert_eq!(l.join("e", hard, 590, &ok_late), Ok(()), "the hall is still open for those signed up — a latecomer gets in too");
        // a match is running — the same tab, no entry to the gathering
        step(&mut l, 600, Hall::Started, &mut opened);
        assert_eq!(l.tabs.iter().find(|t| t.id == hard).map(|t| t.phase), Some(Phase::Battle));
        assert_eq!(l.join("f", hard, 601, &ok_late), Err(Refused::Battle));
        // match over — the same tab gathers again, the map changed
        step(&mut l, 2000, Hall::Gone, &mut opened);
        assert_eq!(l.tabs.len(), 6);
        let t = l.tabs.iter().find(|t| t.id == hard).unwrap();
        assert_eq!((t.phase, t.lane, t.level_id), (Phase::Idle, 3, 40003020), "gathering again, Castle");
        // duo: also no fill-based start, no seats — refusal
        let duo = l.tabs.iter().find(|t| t.party_size == 2).unwrap().id;
        for (i, o) in ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"].iter().enumerate() {
            l.join(o, duo, 3000 + i as i64, &ok_late).unwrap();
        }
        assert_eq!(l.join("p9", duo, 3010, &ok_late), Err(Refused::Full));
        step(&mut l, 3011, Hall::Preparing, &mut opened);
        assert_eq!(l.tabs.iter().find(|t| t.id == duo).map(|t| t.phase), Some(Phase::Gather), "duo waits for the timer");
    }

    /// Timer: start with whoever is there; the map — by votes among those where everyone fits
    #[test]
    fn timer_starts_with_whoever_is_there() {
        let mut l = lobby();
        let mut opened = vec![];
        l.join("a", 2, 100, &ok_late).unwrap();
        l.join("b", 2, 101, &ok_late).unwrap();
        l.vote("a", None, Some(MAPS[3]), 102).unwrap();
        step(&mut l, 579, Hall::Preparing, &mut opened);
        assert!(opened.is_empty());
        step(&mut l, 580, Hall::Preparing, &mut opened);
        assert_eq!(opened, vec![(MAPS[3], 2)]);
        assert_eq!(l.tabs.iter().find(|t| t.id == 2).map(|t| (t.phase, t.level_id)), Some((Phase::Starting, MAPS[3])));
        // duo limit 1 — no second one
        assert_eq!(ids(&l, 2).len(), 1);
    }

    /// The leading map sets the seats: on a small map the instance fills up earlier
    #[test]
    fn leader_map_sets_seats() {
        let mut l = lobby();
        let mut opened = vec![];
        l.join("a", 1, 100, &ok_late).unwrap();
        l.vote("a", None, Some(MAPS[5]), 100).unwrap();
        l.join("b", 1, 101, &ok_late).unwrap();
        assert_eq!(l.tabs[0].need(&seats), 2);
        step(&mut l, 102, Hall::Preparing, &mut opened);
        assert_eq!(opened, vec![(MAPS[5], 1)]);
    }

    /// Vote: for the map — the whole gathering; an instance has its own size; after the start — no. Everyone left — gathering empty
    #[test]
    fn votes_and_empty_gather() {
        let mut l = lobby();
        let mut opened = vec![];
        l.join("a", 3, 100, &ok_late).unwrap();
        assert_eq!(l.vote("a", Some(1), None, 101), Err(Refused::BadVote), "an instance has its own size");
        assert_eq!(l.vote("a", Some(3), None, 101), Ok(()), "matching size — old launcher");
        assert_eq!(l.vote("a", None, Some(1), 101), Err(Refused::BadVote));
        l.vote("a", None, Some(MAPS[1]), 101).unwrap();
        l.vote("a", None, Some(MAPS[2]), 102).unwrap();
        assert_eq!(l.tabs[2].leader(), Some(MAPS[2]), "re-voting is allowed");
        // one tab per player
        l.join("a", 1, 103, &ok_late).unwrap();
        assert_eq!(l.tab_of("a"), Some(0));
        step(&mut l, 104, Hall::Preparing, &mut opened);
        assert_eq!(l.tabs[2].phase, Phase::Idle, "everyone left — gathering empty");
        assert_eq!(l.tabs[2].ends_at, 0);
        assert_eq!(l.join("a", 77, 105, &ok_late), Err(Refused::BadInstance));
        // after the start a vote is not accepted
        step(&mut l, 103 + 480, Hall::Preparing, &mut opened);
        assert_eq!(l.vote("a", None, Some(MAPS[0]), 600), Err(Refused::VoteOver));
    }

    #[test]
    fn late_join_refused_when_hall_full_and_silent_launcher_drops() {
        let mut l = lobby();
        let mut opened = vec![];
        l.join("a", 1, 100, &ok_late).unwrap();
        step(&mut l, 580, Hall::Preparing, &mut opened);
        assert_eq!(l.tabs[0].phase, Phase::Starting);
        assert_eq!(l.join("b", 1, 581, &|_| Err(Refused::Full)), Err(Refused::Full));
        assert_eq!(l.tab_of("b"), None);
        // the launcher is silent longer than SEEN_SECS, no game — the sign-up is dropped; the hall meanwhile lives on its own timer
        l.touch("a", 581);
        let mut open = |_: i64, _: usize| String::new();
        l.step(581 + SEEN_SECS + 1, 0, &|_| false, &|_| Hall::Waiting, &|_| false, &seats, &mut open);
        assert_eq!(l.tab_of("a"), None);
        assert_eq!(l.tabs[0].phase, Phase::Waitroom);
    }

    /// Maintenance: the gathering is dropped and does not open a room, an open hall lives on; our own people keep gathering
    #[test]
    fn maintenance_stops_gather_but_keeps_open_hall() {
        let mut l = lobby();
        let mut opened = vec![];
        l.join("a", 1, 100, &ok_late).unwrap();
        step(&mut l, 580, Hall::Preparing, &mut opened);
        assert_eq!(l.tabs[0].phase, Phase::Starting);
        l.join("b", 2, 581, &ok_late).unwrap();
        let mut open = |_: i64, _: usize| -> String { panic!("no new rooms during maintenance") };
        l.step(2000, 0, &|_| true, &|_| Hall::Waiting, &|_| true, &seats, &mut open);
        assert_eq!(l.tabs[0].phase, Phase::Waitroom, "the hall lives on");
        assert_eq!(ids(&l, 2), vec![(2, Phase::Idle)], "gathering dropped");
        l.join("gm", 2, 2001, &ok_late).unwrap();
        l.join("c", 2, 2001, &ok_late).unwrap();
        l.step(2002, 0, &|o| o != "gm", &|_| Hall::Waiting, &|_| true, &seats, &mut open);
        assert_eq!(ids(&l, 2), vec![(2, Phase::Gather)]);
        assert!(l.tab_of("gm").is_some());
        assert_eq!(l.tab_of("c"), None);
    }

    /// 08.11: map rules for the launcher — Hard is a skirmish with a rarity limit, the others are strict with challenge mode
    #[test]
    fn map_rules_for_launcher() {
        let r = map_rules();
        assert_eq!(r["40003019"]["skirmish"], true);
        assert_eq!(r["40003020"]["challenger"], false);
        assert_eq!((r["40003017"]["skirmish"].clone(), r["40003017"]["challenger"].clone(), r["40003017"]["gear"].clone()), (json!(false), json!(true), Value::Null));
        // 08.11 evening: the level's own limit — the hint follows it (Castle normal — grey and green)
        let green: Vec<String> = ["EN_ITEM_RARITY_NONE", "EN_ITEM_RARITY_ROTTED", "EN_ITEM_RARITY_NORMAL", "EN_ITEM_RARITY_UNCOMMON"].map(String::from).to_vec();
        let levels = std::collections::BTreeMap::from([("40003016".to_string(), green)]);
        let r = rules_with(true, &levels);
        assert_eq!((r["40003016"]["gear"].clone(), r["40003019"]["gear"].clone(), r["40003015"]["gear"].clone()), (json!("green"), json!("epic"), Value::Null));
        let r = rules_with(false, &levels);
        assert_eq!((r["40003016"]["gear"].clone(), r["40003019"]["gear"].clone()), (json!("green"), Value::Null), "level's own limit — with enabled = false too");
        assert_eq!(gear_word(&["EN_ITEM_RARITY_UNCOMMON".into(), "EN_ITEM_RARITY_ENHANCED".into()]), None);
    }

    #[test]
    fn clotho_arena_rotates_bo5_maps() {
        if std::env::var("CLOTHO_MAPS").is_ok() {
            return;
        }
        let maps: Vec<i32> = (0..3).map(clotho_arena).collect();
        assert_eq!(maps, vec![30102, 30202, 30302]);
    }

    #[test]
    fn clotho_sides_keep_groups_and_balance_wins() {
        // a group of three (key 7) — entirely on one side, three solo players — on the other
        let players: Vec<(i64, String, i32)> = vec![
            (7, "g1".into(), 1),
            (7, "g2".into(), 0),
            (7, "g3".into(), 2),
            (11, "s1".into(), 5),
            (12, "s2".into(), 0),
            (13, "s3".into(), 1),
        ];
        let sides = clotho_sides(&players);
        assert_eq!(sides["g1"], sides["g2"]);
        assert_eq!(sides["g2"], sides["g3"]);
        assert!(["s1", "s2", "s3"].iter().all(|o| sides[*o] != sides["g1"]));
        // six solo players: three each, win sums are close (5,4,3,2,1,0 → 5+2+1 / 4+3+0)
        let solo: Vec<(i64, String, i32)> = (0..6).map(|i| (i, format!("p{}", i), 5 - i as i32)).collect();
        let sides = clotho_sides(&solo);
        let count = |s: i64| sides.values().filter(|v| **v == s).count();
        assert_eq!((count(1), count(2)), (3, 3));
        let score = |s: i64| solo.iter().filter(|(_, o, _)| sides[o] == s).map(|(_, _, w)| *w).sum::<i32>();
        assert!((score(1) - score(2)).abs() <= 2, "{} vs {}", score(1), score(2));
        // duel on the test stand: two — on different sides
        let duel: Vec<(i64, String, i32)> = vec![(1, "a".into(), 0), (2, "b".into(), 0)];
        let sides = clotho_sides(&duel);
        assert_ne!(sides["a"], sides["b"]);
    }

    #[test]
    fn clotho_gathers_starts_and_reopens() {
        let mut l = Lobby::new(vec![(1, 1)]);
        l.clotho_min = 2;
        let never = |_: &str| false;
        let online = |_: &str| true;
        let group = |u: u64| u as i64;
        let started: std::cell::RefCell<Vec<Vec<(String, u64, i64)>>> = Default::default();
        let mut cleared: Vec<u64> = vec![];
        l.join_clotho("a", 1, 0, 100).unwrap();
        assert_eq!(l.clotho.phase, Phase::Gather);
        // a regular tab takes the player out of the Clotho gathering
        l.join("a", 1, 101, &ok_late).unwrap();
        assert!(!l.in_clotho("a"));
        l.join_clotho("a", 1, 0, 102).unwrap();
        assert!(l.tab_of("a").is_none(), "signing up to Clotho takes the player out of a regular tab");
        // one — waiting; no free server — the gathering holds
        let mut no_server = |f: &[(String, u64, i64)]| {
            started.borrow_mut().push(f.to_vec());
            None
        };
        l.join_clotho("b", 2, 3, 103).unwrap();
        l.step_clotho(104, &never, &|_| Hall::Preparing, &online, &group, &mut no_server, &mut |u| cleared.extend_from_slice(u));
        assert_eq!(l.clotho.phase, Phase::Gather);
        assert_eq!(started.borrow().len(), 1);
        // server available — a match, both sides
        let mut ok_server = |f: &[(String, u64, i64)]| {
            started.borrow_mut().push(f.to_vec());
            Some("clotho50000006_1".to_string())
        };
        l.step_clotho(105, &never, &|_| Hall::Preparing, &online, &group, &mut ok_server, &mut |u| cleared.extend_from_slice(u));
        assert_eq!(l.clotho.phase, Phase::Starting);
        let last = started.borrow().last().unwrap().clone();
        assert_eq!(last.len(), 2);
        assert_ne!(last[0].2, last[1].2, "sides differ");
        // during a match one cannot sign up
        assert_eq!(l.join_clotho("c", 3, 0, 106), Err(Refused::Battle));
        // match, then the room is gone — teams removed, gathering open
        l.step_clotho(107, &never, &|_| Hall::Started, &online, &group, &mut ok_server, &mut |u| cleared.extend_from_slice(u));
        assert_eq!(l.clotho.phase, Phase::Battle);
        l.step_clotho(108, &never, &|_| Hall::Gone, &online, &group, &mut ok_server, &mut |u| cleared.extend_from_slice(u));
        assert_eq!(l.clotho.phase, Phase::Idle);
        cleared.sort();
        assert_eq!(cleared, vec![1, 2]);
        // maintenance removes from the gathering
        l.join_clotho("d", 4, 0, 109).unwrap();
        l.step_clotho(110, &|_| true, &|_| Hall::Gone, &online, &group, &mut ok_server, &mut |_| {});
        assert!(!l.in_clotho("d"));
    }
}
