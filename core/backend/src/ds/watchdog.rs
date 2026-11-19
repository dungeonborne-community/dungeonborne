//! Game server watchdog (10.11). On the night of 10.11 ds-agent on Robot crashed (panic, code 101), the slots exited after matches,
//! and there was nobody to bring them back up: ~3 h without servers, and the backend said nothing about it.
//!
//! Every 30 s it checks two signs and writes an ALERT to the journal (level err, `journalctl -p err`):
//!   - ds-agent has not polled `GET /ds/slots` for longer than `DS_WATCH_AGENT_SECS` (180) — the agent crashed or Robot is unreachable;
//!   - no pool slot has sent a heartbeat for longer than `DS_WATCH_HB_SECS` (300) — there are no servers at all.
//! While the alert holds — repeat every 10 min; when it clears — one "recovered" line.
//! State — `GET /admin/v1/ds/health` (admin API on loopback). Sends nothing to Discord (user rule).

use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};

static AGENT_POLL: Mutex<Option<Instant>> = Mutex::new(None);
static STARTED: Mutex<Option<Instant>> = Mutex::new(None);

const REPEAT_SECS: u64 = 600;
/// Right after backend start heartbeats have not arrived yet — do not alert
const BOOT_GRACE_SECS: u64 = 300;

/// The agent polled the slots (`GET /ds/slots`)
pub fn agent_polled() {
    *AGENT_POLL.lock().unwrap() = Some(Instant::now());
}

fn env_secs(k: &str, d: u64) -> u64 {
    std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Issue {
    AgentSilent,
    NoServers,
}

impl Issue {
    fn text(self, secs: u64) -> String {
        match self {
            Issue::AgentSilent => format!("ds-agent has not polled the backend for {} s — the agent crashed or Robot is unreachable", secs),
            Issue::NoServers => format!("no game server has sent a heartbeat for {} s — there are no servers", secs),
        }
    }
}

/// Check inputs: seconds since the last agent poll and since the freshest heartbeat (None — never happened)
#[derive(Clone, Copy, Debug)]
pub struct Seen {
    pub uptime: u64,
    pub agent: Option<u64>,
    pub hb: Option<u64>,
}

/// What is wrong now. Never happened — count from backend start (after the grace period)
pub fn assess(s: Seen, agent_limit: u64, hb_limit: u64) -> Vec<(Issue, u64)> {
    let mut out = Vec::new();
    if s.uptime < BOOT_GRACE_SECS {
        return out;
    }
    let agent = s.agent.unwrap_or(s.uptime);
    if agent >= agent_limit {
        out.push((Issue::AgentSilent, agent));
    }
    let hb = s.hb.unwrap_or(s.uptime);
    if hb >= hb_limit {
        out.push((Issue::NoServers, hb));
    }
    out
}

/// Alert state between checks: when each was last written
#[derive(Default)]
pub struct Alarms {
    last: Vec<(Issue, u64)>,
}

impl Alarms {
    /// Lines for the journal on this pass: (alert?, text)
    pub fn step(&mut self, now: u64, issues: &[(Issue, u64)]) -> Vec<(bool, String)> {
        let mut out = Vec::new();
        for &(i, secs) in issues {
            match self.last.iter_mut().find(|(k, _)| *k == i) {
                Some((_, t)) if now.saturating_sub(*t) < REPEAT_SECS => {}
                Some((_, t)) => {
                    *t = now;
                    out.push((true, i.text(secs)));
                }
                None => {
                    self.last.push((i, now));
                    out.push((true, i.text(secs)));
                }
            }
        }
        let gone: Vec<Issue> = self.last.iter().map(|(k, _)| *k).filter(|k| !issues.iter().any(|(i, _)| i == k)).collect();
        for k in gone {
            self.last.retain(|(i, _)| *i != k);
            out.push((false, format!("recovered: {:?}", k)));
        }
        out
    }
}

fn seen() -> Seen {
    let uptime = STARTED.lock().unwrap().map(|t| t.elapsed().as_secs()).unwrap_or(0);
    Seen {
        uptime,
        agent: AGENT_POLL.lock().unwrap().map(|t| t.elapsed().as_secs()),
        hb: crate::lobby::allocator::freshest_hb_secs(),
    }
}

/// For the admin API
pub fn health() -> Value {
    let s = seen();
    let (a, h) = (env_secs("DS_WATCH_AGENT_SECS", 180), env_secs("DS_WATCH_HB_SECS", 300));
    let issues: Vec<String> = assess(s, a, h).into_iter().map(|(i, secs)| i.text(secs)).collect();
    json!({
        "ok": issues.is_empty(), "issues": issues, "uptime_secs": s.uptime,
        "agent_poll_secs": s.agent, "freshest_heartbeat_secs": s.hb, "slots": crate::lobby::allocator::snapshot(),
    })
}

pub async fn run() {
    STARTED.lock().unwrap().get_or_insert_with(Instant::now);
    let (a, h) = (env_secs("DS_WATCH_AGENT_SECS", 180), env_secs("DS_WATCH_HB_SECS", 300));
    // journald takes the level from the <N> prefix (3 = err); on Windows the prefix is not needed
    let (err, info) = if cfg!(target_os = "linux") { ("<3>", "<5>") } else { ("", "") };
    let mut alarms = Alarms::default();
    let t0 = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_secs(30)).await;
        for (bad, line) in alarms.step(t0.elapsed().as_secs(), &assess(seen(), a, h)) {
            if bad {
                eprintln!("{}[Watchdog] ALERT: {}", err, line);
            } else {
                eprintln!("{}[Watchdog] {}", info, line);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watchdog_assess_and_repeat() {
        let s = |uptime, agent, hb| Seen { uptime, agent, hb };
        // grace after start
        assert!(assess(s(100, None, None), 180, 300).is_empty());
        // everything alive
        assert!(assess(s(4000, Some(10), Some(20)), 180, 300).is_empty());
        // night of 10.11: the agent is silent, no heartbeat
        let bad = assess(s(4000, Some(200), Some(400)), 180, 300);
        assert_eq!(bad, vec![(Issue::AgentSilent, 200), (Issue::NoServers, 400)]);
        // there was never an agent — count from start
        assert_eq!(assess(s(400, None, Some(5)), 180, 300), vec![(Issue::AgentSilent, 400)]);

        let mut a = Alarms::default();
        assert_eq!(a.step(0, &bad).len(), 2);
        assert!(a.step(30, &bad).is_empty(), "repeat not sooner than 10 min");
        assert_eq!(a.step(600, &bad).len(), 2);
        let back = a.step(630, &[(Issue::NoServers, 500)]);
        assert_eq!(back, vec![(false, "recovered: AgentSilent".to_string())]);
        assert_eq!(a.step(660, &[]), vec![(false, "recovered: NoServers".to_string())]);
        assert!(a.step(690, &[]).is_empty());
    }
}
