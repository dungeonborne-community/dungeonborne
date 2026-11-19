//! Decisions for one server instance: pure logic with no processes or network (tests below).
//!
//! Whether the server is alive — by two signs: the instance port is held by the game process, or a fresh mod heartbeat in the backend
//! (`GET /ds/slots`). During a map change (reset, servertravel) the port can be briefly released, while the mod heartbeat
//! keeps coming. A hung server — the port is held but the heartbeat is silent longer than `silent`, or a transition phase (`hosting`, `resetting`,
//! `entering`, `exiting`) lasts longer than `stuck` (prod 03.11: the hall did not load and hung in hosting, a reset got stuck in
//! resetting). Without a backend reply nothing is known about health — then only "no process".
//!
//! Scaling (04.11, backend DS_AUTOSCALE=1): the slot has `want` — whether a server is needed. Not needed and not running —
//! state Off; needed two ticks in a row — start (a single queue spike does not wake a server). An extra one that sat in
//! `standby` longer than `idle` — quit and Off. A per-match process (mod -dbstandby) exits by itself after the match with phase `exiting`:
//! such a restart is planned and does not count toward the rate limit.

/// Mod phases in the heartbeat that must not last long (`ready`, `standby`, `waiting`, `battle` — may)
const TRANSITIONAL: &[&str] = &["hosting", "resetting", "entering", "exiting"];

#[derive(Clone, Debug, PartialEq)]
pub enum State {
    /// agent just started: the process may already be running
    Unknown,
    /// start issued, waiting for a heartbeat
    Starting,
    Up,
    /// quit issued, waiting for the process to exit
    Quitting,
    /// not needed (want = false) and not running
    Off,
    /// cannot cope by itself (does not exit on quit, restarts too often) — a human is needed
    Blocked,
}

/// Instance slot in the backend
#[derive(Clone, Debug, Default)]
pub struct Slot {
    pub phase: String,
    pub last_hb_secs: Option<u64>,
    /// whether a server is needed (None — backend without scaling: always needed)
    pub want: Option<bool>,
}

/// Observation for a tick
#[derive(Clone, Debug, Default)]
pub struct Obs {
    /// instance port is held (UDP)
    pub bound: bool,
    /// backend replied and knows this slot
    pub slot: Option<Slot>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    None,
    Start(String),
    Quit(String),
    Alert(String),
}

#[derive(Clone, Debug)]
pub struct Limits {
    /// how long to wait for a heartbeat after start, s
    pub start_grace: u64,
    /// heartbeat silent longer — server hung, s
    pub silent: u64,
    /// transition phase longer — hung, s
    pub stuck: u64,
    /// how long to wait for exit after quit, s
    pub quit_wait: u64,
    /// at most this many unplanned starts per `window` s
    pub max_starts: usize,
    pub window: u64,
    /// alert repeat in Blocked, s
    pub alert_every: u64,
    /// extra server in standby longer — shut it down, s
    pub idle: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { start_grace: 240, silent: 90, stuck: 300, quit_wait: 120, max_starts: 4, window: 1800, alert_every: 600, idle: 600 }
    }
}

#[derive(Debug)]
pub struct Inst {
    pub port: u16,
    pub state: State,
    /// when we entered state (s since agent start)
    pub since: u64,
    /// slot phase and since when it has been so
    phase: Option<(String, u64)>,
    /// last phase known while the process was alive (exiting — planned exit)
    last_phase: String,
    /// since when the backend knows the slot but there is no fresh heartbeat
    silent_since: Option<u64>,
    /// ticks in a row "not alive" (neither port nor heartbeat)
    dead_ticks: u32,
    /// ticks in a row want = true (for starting from Off)
    want_ticks: u32,
    /// quit issued because the server is extra — do not start after exit
    retire: bool,
    starts: Vec<u64>,
    alert_at: Option<u64>,
}

impl Inst {
    pub fn new(port: u16) -> Self {
        Inst {
            port,
            state: State::Unknown,
            since: 0,
            phase: None,
            last_phase: String::new(),
            silent_since: None,
            dead_ticks: 0,
            want_ticks: 0,
            retire: false,
            starts: Vec::new(),
            alert_at: None,
        }
    }

    fn set(&mut self, s: State, now: u64) {
        if self.state != s {
            self.state = s;
            self.since = now;
        }
    }

    /// 10.11: the agent was restarted, and it started this server itself at `at` (unix time) — wait for it as starting,
    /// rather than starting a second one (until the port is bound and the first heartbeat it looked like "no server" → duplicate 7797)
    pub fn resume_starting(&mut self, at: u64) {
        self.starts.push(at);
        self.state = State::Starting;
        self.since = at;
    }

    /// planned — planned restart (after the match, by want): does not count toward the rate limit
    fn start(&mut self, now: u64, l: &Limits, why: String, planned: bool) -> Action {
        self.starts.retain(|t| now.saturating_sub(*t) < l.window);
        if !planned && self.starts.len() >= l.max_starts {
            self.set(State::Blocked, now);
            self.alert_at = Some(now);
            return Action::Alert(format!(
                "{} starts in {} min — not starting any more ({}); investigate and restart the agent",
                self.starts.len(),
                l.window / 60,
                why
            ));
        }
        if !planned {
            self.starts.push(now);
        }
        self.silent_since = None;
        self.phase = None;
        self.last_phase.clear();
        self.dead_ticks = 0;
        self.retire = false;
        self.set(State::Starting, now);
        Action::Start(why)
    }

    fn quit(&mut self, now: u64, why: String) -> Action {
        self.set(State::Quitting, now);
        Action::Quit(why)
    }

    fn off(&mut self, now: u64) -> Action {
        self.retire = false;
        self.want_ticks = 0;
        self.set(State::Off, now);
        Action::None
    }

    pub fn step(&mut self, now: u64, o: &Obs, l: &Limits) -> Action {
        // heartbeat and phase from the backend
        let hb = o.slot.as_ref().and_then(|s| s.last_hb_secs);
        let fresh = hb.map_or(false, |h| h < l.silent);
        match (&o.slot, fresh) {
            (Some(_), false) => {
                self.silent_since.get_or_insert(now);
            }
            _ => self.silent_since = None,
        }
        let silent_for = self.silent_since.map(|t| now - t).unwrap_or(0).max(hb.unwrap_or(0));
        let silent_long = o.slot.is_some() && !fresh && silent_for >= l.silent;
        if let Some(s) = &o.slot {
            if fresh && !s.phase.is_empty() {
                self.last_phase = s.phase.clone();
            }
            if self.phase.as_ref().map_or(true, |(p, _)| p != &s.phase) {
                self.phase = Some((s.phase.clone(), now));
            }
        }
        let want = o.slot.as_ref().and_then(|s| s.want);
        self.want_ticks = if want == Some(true) { self.want_ticks + 1 } else { 0 };
        let stuck = self.phase.as_ref().filter(|(p, t)| TRANSITIONAL.contains(&p.as_str()) && fresh && now - t >= l.stuck);
        let idle_long = self.phase.as_ref().filter(|(p, t)| p == "standby" && fresh && now - t >= l.idle).is_some();
        // a process exiting after the match without a port is no longer alive, even though the heartbeat is still "fresh" until silent
        let clean_exit = self.last_phase == "exiting";
        let alive = o.bound || (fresh && !clean_exit);
        self.dead_ticks = if alive { 0 } else { self.dead_ticks + 1 };

        match self.state {
            State::Unknown => {
                if alive {
                    self.set(State::Up, now);
                    Action::None
                } else if want == Some(false) {
                    self.off(now)
                } else {
                    self.start(now, l, "no server".into(), want == Some(true))
                }
            }
            State::Starting => {
                if fresh {
                    self.set(State::Up, now);
                    Action::None
                } else if now - self.since >= l.start_grace {
                    if o.bound {
                        self.quit(now, format!("did not check in within {} s after start", l.start_grace))
                    } else {
                        self.start(now, l, format!("did not come up within {} s", l.start_grace), false)
                    }
                } else {
                    Action::None
                }
            }
            State::Up => {
                if self.dead_ticks >= 2 {
                    if want == Some(false) {
                        self.off(now)
                    } else if clean_exit {
                        self.start(now, l, "process exited after the match — new one".into(), true)
                    } else {
                        self.start(now, l, "process gone (port free, no heartbeat)".into(), false)
                    }
                } else if o.bound && silent_long {
                    self.quit(now, format!("heartbeat silent for {} s", silent_for))
                } else if let Some((p, t)) = stuck {
                    let why = format!("phase {} for {} s already", p, now - t);
                    self.quit(now, why)
                } else if want == Some(false) && idle_long {
                    self.retire = true;
                    self.quit(now, format!("extra: in standby longer than {} min", l.idle / 60))
                } else {
                    Action::None
                }
            }
            State::Quitting => {
                if !o.bound && !fresh {
                    if self.retire || want == Some(false) {
                        self.off(now)
                    } else {
                        self.start(now, l, "after quit".into(), false)
                    }
                } else if now - self.since >= l.quit_wait {
                    self.set(State::Blocked, now);
                    self.alert_at = Some(now);
                    Action::Alert(format!(
                        "does not exit on quit for {} s — close Dungeonborne.exe (port {}) manually or reboot the machine",
                        l.quit_wait, self.port
                    ))
                } else {
                    Action::None
                }
            }
            State::Off => {
                if alive {
                    // someone started it manually — observe as usual
                    self.set(State::Up, now);
                    Action::None
                } else if self.want_ticks >= 2 {
                    self.start(now, l, "needed (backend)".into(), true)
                } else {
                    Action::None
                }
            }
            State::Blocked => {
                // the server was closed manually — carry on ourselves (except the rate-limit block: that one waits for an agent restart)
                let by_rate = {
                    self.starts.retain(|t| now.saturating_sub(*t) < l.window);
                    self.starts.len() >= l.max_starts
                };
                if !by_rate && !o.bound && !fresh {
                    self.start(now, l, "closed manually".into(), false)
                } else if self.alert_at.map_or(true, |t| now - t >= l.alert_every) {
                    self.alert_at = Some(now);
                    Action::Alert(format!("port {}: a human is still needed (state Blocked)", self.port))
                } else {
                    Action::None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(bound: bool, phase: &str, hb: Option<u64>) -> Obs {
        Obs { bound, slot: Some(Slot { phase: phase.into(), last_hb_secs: hb, want: None }) }
    }

    fn obs_w(bound: bool, phase: &str, hb: Option<u64>, want: bool) -> Obs {
        Obs { bound, slot: Some(Slot { phase: phase.into(), last_hb_secs: hb, want: Some(want) }) }
    }

    fn l() -> Limits {
        Limits::default()
    }

    #[test]
    fn adopts_running_server_and_starts_missing() {
        let mut a = Inst::new(7777);
        assert_eq!(a.step(0, &obs(true, "ready", Some(1)), &l()), Action::None);
        assert_eq!(a.state, State::Up);
        let mut b = Inst::new(7777);
        assert!(matches!(b.step(0, &Obs::default(), &l()), Action::Start(_)));
        assert_eq!(b.state, State::Starting);
    }

    #[test]
    fn starting_becomes_up_on_heartbeat_or_retries() {
        let mut a = Inst::new(7777);
        a.step(0, &Obs::default(), &l());
        assert_eq!(a.step(60, &obs(false, "", None), &l()), Action::None, "loading — wait");
        assert_eq!(a.step(90, &obs(true, "ready", Some(2)), &l()), Action::None);
        assert_eq!(a.state, State::Up);

        let mut b = Inst::new(7777);
        b.step(0, &Obs::default(), &l());
        assert!(matches!(b.step(250, &obs(true, "", None), &l()), Action::Quit(_)), "port held, no contact — quit");
        assert!(matches!(b.step(270, &obs(false, "", None), &l()), Action::Start(_)), "exited — start again");
    }

    #[test]
    fn short_port_gap_with_heartbeat_is_not_death() {
        let mut a = Inst::new(7777);
        a.step(0, &obs(true, "ready", Some(1)), &l());
        // reset: port free, the mod sends heartbeats
        assert_eq!(a.step(15, &obs(false, "resetting", Some(2)), &l()), Action::None);
        assert_eq!(a.step(30, &obs(false, "resetting", Some(2)), &l()), Action::None);
        // neither port nor heartbeat for two ticks — start
        assert_eq!(a.step(45, &Obs::default(), &l()), Action::None, "one tick — not dead yet");
        assert!(matches!(a.step(60, &Obs::default(), &l()), Action::Start(_)));
    }

    #[test]
    fn hung_by_silence_or_stuck_phase() {
        let mut a = Inst::new(7777);
        a.step(0, &obs(true, "ready", Some(1)), &l());
        assert_eq!(a.step(15, &obs(true, "ready", Some(40)), &l()), Action::None);
        assert!(matches!(a.step(30, &obs(true, "ready", Some(95)), &l()), Action::Quit(_)), "heartbeat 95 s");

        let mut b = Inst::new(7777);
        b.step(0, &obs(true, "ready", Some(1)), &l());
        b.step(10, &obs(true, "hosting", Some(1)), &l());
        assert_eq!(b.step(200, &obs(true, "hosting", Some(1)), &l()), Action::None);
        assert!(matches!(b.step(320, &obs(true, "hosting", Some(1)), &l()), Action::Quit(_)), "hosting 310 s");

        // a long match is not a hang
        let mut c = Inst::new(7777);
        c.step(0, &obs(true, "battle", Some(5)), &l());
        assert_eq!(c.step(2000, &obs(true, "battle", Some(5)), &l()), Action::None);
    }

    #[test]
    fn backend_restart_without_heartbeat_waits_silent() {
        let mut a = Inst::new(7777);
        a.step(0, &obs(true, "ready", Some(1)), &l());
        // backend restarted: slot without heartbeat — wait for silent by our own clock
        assert_eq!(a.step(10, &obs(true, "", None), &l()), Action::None);
        assert_eq!(a.step(60, &obs(true, "", None), &l()), Action::None);
        assert_eq!(a.step(70, &obs(true, "ready", Some(1)), &l()), Action::None, "mod checked in");
        // without a backend reply — do not touch
        assert_eq!(a.step(500, &Obs { bound: true, slot: None }, &l()), Action::None);
    }

    #[test]
    fn quit_that_never_exits_blocks_and_rate_limit() {
        let mut a = Inst::new(7777);
        a.step(0, &obs(true, "ready", Some(1)), &l());
        assert!(matches!(a.step(10, &obs(true, "ready", Some(100)), &l()), Action::Quit(_)));
        assert_eq!(a.step(60, &obs(true, "ready", Some(150)), &l()), Action::None);
        assert!(matches!(a.step(140, &obs(true, "ready", Some(230)), &l()), Action::Alert(_)));
        assert_eq!(a.state, State::Blocked);
        assert_eq!(a.step(150, &obs(true, "ready", Some(240)), &l()), Action::None);
        assert!(matches!(a.step(160, &Obs::default(), &l()), Action::Start(_)), "closed manually — start");

        let mut b = Inst::new(7777);
        let mut t = 0;
        let mut starts = 0;
        for _ in 0..20 {
            match b.step(t, &Obs::default(), &l()) {
                Action::Start(_) => starts += 1,
                Action::Alert(_) => break,
                _ => {}
            }
            t += 250;
        }
        assert_eq!(starts, 4, "no more than max_starts per window");
        assert_eq!(b.state, State::Blocked);
    }

    #[test]
    fn autoscale_off_until_wanted_twice() {
        let mut a = Inst::new(7787);
        assert_eq!(a.step(0, &obs_w(false, "", None, false), &l()), Action::None);
        assert_eq!(a.state, State::Off, "not needed — do not start");
        assert_eq!(a.step(15, &obs_w(false, "", None, true), &l()), Action::None, "one want tick — not yet");
        assert!(matches!(a.step(30, &obs_w(false, "", None, true), &l()), Action::Start(_)), "two ticks — start");
        // planned starts do not count toward the rate limit
        assert!(a.starts.is_empty());
    }

    #[test]
    fn exit_after_match_restarts_planned() {
        let mut a = Inst::new(7777);
        a.step(0, &obs_w(true, "battle", Some(5), true), &l());
        a.step(15, &obs_w(true, "exiting", Some(1), true), &l());
        assert_eq!(a.step(30, &obs_w(false, "exiting", Some(70), true), &l()), Action::None);
        let act = a.step(45, &obs_w(false, "exiting", Some(85), true), &l());
        assert!(matches!(act, Action::Start(ref w) if w.contains("after the match")), "{:?}", act);
        assert!(a.starts.is_empty(), "after the match — planned");
        // after the match, and the server is no longer needed — Off
        let mut b = Inst::new(7787);
        b.step(0, &obs_w(true, "exiting", Some(1), true), &l());
        b.step(15, &obs_w(false, "", None, false), &l());
        b.step(30, &obs_w(false, "", None, false), &l());
        assert_eq!(b.state, State::Off);
    }

    #[test]
    fn restarted_agent_waits_for_own_recent_start() {
        // started at 1000, agent restarted at 1060: neither port nor heartbeat — do not start a second one
        let mut a = Inst::new(7797);
        a.resume_starting(1000);
        assert_eq!(a.step(1060, &obs_w(false, "", None, true), &l()), Action::None);
        assert_eq!(a.step(1200, &obs_w(false, "", None, true), &l()), Action::None);
        assert_eq!(a.step(1220, &obs_w(true, "standby", Some(1), true), &l()), Action::None);
        assert_eq!(a.state, State::Up, "checked in");
        // did not check in within start_grace of the real start — start again
        let mut b = Inst::new(7797);
        b.resume_starting(1000);
        assert_eq!(b.step(1100, &Obs::default(), &l()), Action::None);
        assert!(matches!(b.step(1245, &Obs::default(), &l()), Action::Start(_)));
    }

    #[test]
    fn idle_surplus_retires() {
        let mut a = Inst::new(7797);
        a.step(0, &obs_w(true, "standby", Some(1), false), &l());
        assert_eq!(a.step(300, &obs_w(true, "standby", Some(1), false), &l()), Action::None, "5 min — too early");
        assert!(matches!(a.step(615, &obs_w(true, "standby", Some(1), false), &l()), Action::Quit(_)));
        assert_eq!(a.step(630, &obs_w(false, "", None, false), &l()), Action::None);
        assert_eq!(a.state, State::Off, "the extra one exited — do not restart");
        // do not shut down a needed standby
        let mut b = Inst::new(7777);
        b.step(0, &obs_w(true, "standby", Some(1), true), &l());
        assert_eq!(b.step(5000, &obs_w(true, "standby", Some(1), true), &l()), Action::None);
    }
}
