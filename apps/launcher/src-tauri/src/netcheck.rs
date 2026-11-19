//! Recognises a provider that freezes connections to the server (some Russian ISPs, 03–06.11): TLS through the relay
//! stalls after ~1.4–2 KB and the OS drops it about 20 s later. The game then fails to log in (CURLCode 28), the lobby
//! freezes and resets after ~30 s, and battle UDP dies after the first packets. The cure is the battle UDP tunnel
//! through the relay (backend `src/auth/udp_tunnel.rs`), which the player switches on when the launcher asks.
//!
//! Input: the relay's finished connections, the same as `relay.log`. A stall is a connection to 443 (game HTTPS) or a
//! lobby port that died with an I/O error after 15–45 s (lobby: 15–60 s and little data). Short polls that close
//! normally are the usual traffic and never count. One burst of stalls (a server restart, a blip) is not a provider
//! problem either: the stalls must repeat over at least 45 s.

use std::collections::VecDeque;

use crate::relay::ConnLog;

/// Stalls are counted over this much time
const WINDOW_MS: i64 = 5 * 60 * 1000;
/// This many stalls...
const MIN_STALLS: usize = 3;
/// ...spread over at least this much time (a single burst is a blip, not the provider)
const MIN_SPREAD_MS: i64 = 45 * 1000;
/// A lobby connection that moved this much was a real session that ended, not a frozen one
const LOBBY_FROZEN_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stall {
    Https,
    Lobby,
}

/// Is this finished connection a stall? `None` — normal traffic.
pub fn stall(c: &ConnLog) -> Option<Stall> {
    if !c.reason.starts_with("io ") {
        return None;
    }
    match c.port {
        443 if (15_000..=45_000).contains(&c.ms) => Some(Stall::Https),
        6601..=6604 if (15_000..=60_000).contains(&c.ms) && c.down < LOBBY_FROZEN_BYTES => Some(Stall::Lobby),
        _ => None,
    }
}

/// What was found, for the player's prompt and the server's log
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub https: usize,
    pub lobby: usize,
    pub spread_s: i64,
}

impl Finding {
    /// One line for the server log (why the tunnel was switched on)
    pub fn summary(&self) -> String {
        format!(
            "launcher relay.log: {} stalled connections in {} s (https {}, lobby {}), ~20 s each, io error",
            self.https + self.lobby,
            self.spread_s,
            self.https,
            self.lobby
        )
    }
}

#[derive(Default)]
pub struct Detector {
    stalls: VecDeque<(i64, Stall)>,
    fired: bool,
}

impl Detector {
    /// Feeds one finished connection; returns a finding once per detector (per relay run).
    pub fn push(&mut self, c: &ConnLog) -> Option<Finding> {
        let Some(kind) = stall(c) else { return None };
        // ts is when the connection started: its end is what orders stalls in time
        let end = c.ts * 1000 + c.ms as i64;
        self.stalls.push_back((end, kind));
        while self.stalls.front().is_some_and(|(t, _)| end - t > WINDOW_MS) {
            self.stalls.pop_front();
        }
        if self.fired || self.stalls.len() < MIN_STALLS {
            return None;
        }
        let first = self.stalls.iter().map(|s| s.0).min().unwrap_or(end);
        let last = self.stalls.iter().map(|s| s.0).max().unwrap_or(end);
        if last - first < MIN_SPREAD_MS {
            return None;
        }
        self.fired = true;
        let https = self.stalls.iter().filter(|s| s.1 == Stall::Https).count();
        Some(Finding { https, lobby: self.stalls.len() - https, spread_s: (last - first) / 1000 })
    }
}

/// `relay.log` line (`<ts> port=443 up=524 down=1494 ms=367 reason=closed`) as the relay reported it.
/// Lines written at the same moment can interleave; such a line is skipped.
pub fn parse_line(line: &str) -> Option<ConnLog> {
    let (ts, rest) = line.split_once(' ')?;
    let ts: i64 = ts.parse().ok()?;
    let rest = rest.strip_prefix("port=")?;
    let (port, rest) = rest.split_once(" up=")?;
    let (up, rest) = rest.split_once(" down=")?;
    let (down, rest) = rest.split_once(" ms=")?;
    let (ms, reason) = rest.split_once(" reason=")?;
    Some(ConnLog { ts, port: port.parse().ok()?, up: up.parse().ok()?, down: down.parse().ok()?, ms: ms.parse().ok()?, reason: reason.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(lines: &[&str]) -> Option<Finding> {
        let mut d = Detector::default();
        let mut found = None;
        for l in lines {
            if let Some(c) = parse_line(l) {
                if let Some(f) = d.push(&c) {
                    found.get_or_insert(f);
                }
            }
        }
        found
    }

    /// The usual traffic: game polls through the relay (this machine's relay.log, 07.11)
    const NORMAL: [&str; 6] = [
        "1793979433 port=443 up=524 down=1494 ms=367 reason=closed",
        "1793979433 port=443 up=524 down=1494 ms=800 reason=closed",
        "1793979435 port=443 up=524 down=1494 ms=1410 reason=closed",
        "1793979434 port=6601 up=402857 down=437355 ms=1293872 reason=closed",
        "1793805869 port=443 up=1785 down=2277 ms=1858 reason=closed",
        "1793805871 port=443 up=975 down=1931 ms=3066 reason=io up: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
    ];

    #[test]
    fn normal_traffic_is_not_a_stall() {
        for l in NORMAL {
            assert_eq!(stall(&parse_line(l).unwrap()), None, "{l}");
        }
        // a long lobby session that ended with an error moved lots of data: not frozen
        let lobby = parse_line("1793805859 port=6601 up=15744 down=127595 ms=217549 reason=io down: Программа на вашем хост-компьютере разорвала установленное подключение. (os error 10053)").unwrap();
        assert_eq!(stall(&lobby), None);
        assert_eq!(feed(&NORMAL.repeat(50).iter().copied().collect::<Vec<_>>()), None);
    }

    /// One burst on this machine (07.11): five stalls among dozens of normal polls, all within 2 s. Not the provider.
    #[test]
    fn a_single_burst_is_a_blip() {
        let lines = [
            "1793727501 port=443 up=524 down=1494 ms=664 reason=closed",
            "1793727501 port=443 up=524 down=1494 ms=20030 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793727501 port=443 up=524 down=1494 ms=669 reason=closed",
            "1793727502 port=443 up=524 down=1494 ms=21200 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793727502 port=443 up=524 down=1494 ms=21370 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793727502 port=443 up=524 down=1494 ms=644 reason=closed",
            "1793727502 port=443 up=524 down=1494 ms=21684 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793727503 port=443 up=524 down=1494 ms=21815 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793727503 port=443 up=524 down=1494 ms=727 reason=closed",
        ];
        assert_eq!(stall(&parse_line(lines[1]).unwrap()), Some(Stall::Https), "each of them looks like a stall");
        assert_eq!(feed(&lines), None, "but a single burst does not prompt the player");
    }

    /// A player whose provider freezes the relay (pattern from the relay.log of 03–06.11): every login attempt stalls
    /// ~19.5 s, the lobby freezes at ~2 KB and resets after ~30 s, again and again.
    #[test]
    fn a_freezing_provider_is_found() {
        let lines = [
            "1793700000 port=443 up=643 down=1494 ms=19500 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
            "1793700020 port=443 up=524 down=1494 ms=367 reason=closed",
            "1793700021 port=443 up=524 down=1435 ms=19480 reason=io down: Превышен таймаут семафора. (os error 121)",
            "1793700045 port=6601 up=1210 down=2048 ms=30120 reason=io down: Удаленный хост принудительно разорвал существующее подключение. (os error 10054)",
        ];
        let f = feed(&lines).expect("found");
        assert_eq!((f.https, f.lobby), (2, 1));
        assert!(f.spread_s >= 45, "{f:?}");
        assert!(f.summary().contains("3 stalled"));
        // once per run
        let mut d = Detector::default();
        let n = lines.iter().chain(lines.iter()).filter_map(|l| parse_line(l)).filter_map(|c| d.push(&c)).count();
        assert_eq!(n, 1);
    }

    #[test]
    fn stalls_older_than_the_window_are_forgotten() {
        let lines = [
            "1793700000 port=443 up=643 down=1494 ms=19500 reason=io down: x",
            "1793700100 port=443 up=643 down=1494 ms=19500 reason=io down: x",
            "1793700900 port=443 up=643 down=1494 ms=19500 reason=io down: x",
        ];
        assert_eq!(feed(&lines), None, "three stalls over 15 minutes are not a pattern");
    }

    #[test]
    fn broken_lines_are_skipped() {
        assert!(parse_line("17937275021793727502 port=443 up=524 down=1494 ms=828 reason=closed ").is_none());
        assert!(parse_line("port=443 up=524 down=1494 ms=805 reason=closed").is_none());
        assert!(parse_line("").is_none());
    }
}
