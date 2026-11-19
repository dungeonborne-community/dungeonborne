//! Local relay (spec 4.4): listeners on 127.0.0.1 for the game's TCP ports; each connection becomes a
//! TLS 1.3 connection to the server's relay with a one-line JSON preface carrying the session token.

use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::{mpsc, watch};
use tokio_rustls::client::TlsStream;
use tokio_rustls::TlsConnector;

use crate::error::{Error, Result};
use crate::net::unix_now;
use crate::winutil::port_busy_error;

pub const PORTS: &[u16] = &[80, 443, 8085, 6601, 6602, 6603, 6604];
const PREFACE_LIMIT: usize = 4096;
const PREFACE_TIMEOUT: Duration = Duration::from_secs(10);
/// Fight traffic tunnel: the backend gives this address only to players whose direct UDP to the game server is cut
/// on the way (backend `auth::udp_tunnel`), everybody else connects directly and this port stays silent.
pub const UDP_PORT: u16 = 27777;
const UDP_MAX: usize = 2048;
const UDP_IDLE: Duration = Duration::from_secs(120);
/// A refused tunnel (no room yet) is retried at most this often, not on every datagram
const UDP_RETRY: Duration = Duration::from_secs(2);

pub struct Config {
    pub address: String,
    pub port: u16,
    pub cert_sha256: Option<String>,
    pub token: String,
    pub modset: String,
}

/// One finished connection. No token, ever.
#[derive(Debug, Clone, Serialize)]
pub struct ConnLog {
    pub ts: i64,
    pub port: u16,
    pub up: u64,
    pub down: u64,
    pub ms: u64,
    pub reason: String,
}

pub type Sink = Arc<dyn Fn(ConnLog) + Send + Sync>;

/// Connections the game opened, counted when accepted: the INTL login (80/443) and the directory and lobby
/// (8085, 6601–6604). 1.7.2: a game that logs in but never comes to the directory went somewhere else (lib.rs).
#[derive(Default)]
pub struct Activity {
    pub login: std::sync::atomic::AtomicU64,
    pub game: std::sync::atomic::AtomicU64,
}

impl Activity {
    fn accepted(&self, port: u16) {
        use std::sync::atomic::Ordering::Relaxed;
        match port {
            80 | 443 => self.login.fetch_add(1, Relaxed),
            8085 | 6601..=6604 => self.game.fetch_add(1, Relaxed),
            _ => 0,
        };
    }

    pub fn counts(&self) -> (u64, u64) {
        use std::sync::atomic::Ordering::Relaxed;
        (self.login.load(Relaxed), self.game.load(Relaxed))
    }
}

/// Did the game reach the server after logging in? Fed with `Activity::counts` from the moment the game starts.
pub struct ReachWatch {
    start: (u64, u64),
    login_at: Option<Instant>,
    limit: Duration,
}

#[derive(Debug, PartialEq)]
pub enum Reach {
    Waiting,
    /// The game came to the directory or lobby: nothing to watch any more
    Reached,
    /// Logged in `limit` ago and still nothing on 8085/6601–6604
    Missed,
}

impl ReachWatch {
    pub fn new(counts: (u64, u64), limit: Duration) -> Self {
        Self { start: counts, login_at: None, limit }
    }

    pub fn step(&mut self, (login, game): (u64, u64), now: Instant) -> Reach {
        if game > self.start.1 {
            return Reach::Reached;
        }
        if login > self.start.0 && self.login_at.is_none() {
            self.login_at = Some(now);
        }
        match self.login_at {
            Some(t) if now.duration_since(t) >= self.limit => Reach::Missed,
            _ => Reach::Waiting,
        }
    }
}

pub struct Relay {
    shutdown: watch::Sender<bool>,
    listeners: Vec<tokio::task::JoinHandle<()>>,
    pub activity: Arc<Activity>,
}

impl Relay {
    /// Closes all connections and returns once every port is released.
    pub async fn stop(mut self) {
        let _ = self.shutdown.send(true);
        for h in std::mem::take(&mut self.listeners) {
            let _ = h.await;
        }
    }
}

impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.shutdown.send(true);
    }
}

struct Shared {
    cfg: Config,
    name: ServerName<'static>,
    connector: TlsConnector,
    sink: Sink,
    activity: Arc<Activity>,
}

/// Binds every game port first (all or nothing), then starts accepting.
pub async fn start(cfg: Config, sink: Sink) -> Result<Relay> {
    start_inner(cfg, sink, &PORTS.iter().map(|&p| (p, p)).collect::<Vec<_>>(), Some(UDP_PORT)).await
}

/// `ports`: (local listen port, game port sent in the preface). Tests listen on high ports.
pub async fn start_mapped(cfg: Config, sink: Sink, ports: &[(u16, u16)]) -> Result<Relay> {
    start_inner(cfg, sink, ports, None).await
}

async fn start_inner(cfg: Config, sink: Sink, ports: &[(u16, u16)], udp_port: Option<u16>) -> Result<Relay> {
    let name = ServerName::try_from(cfg.address.clone()).map_err(|e| Error::new("manifest_invalid", format!("relay.address: {e}")))?;
    let pin = match &cfg.cert_sha256 {
        Some(h) => {
            let mut p = [0u8; 32];
            hex::decode_to_slice(h, &mut p).map_err(|e| Error::new("manifest_invalid", format!("relay.cert_sha256: {e}")))?;
            Some(p)
        }
        None => None,
    };
    let connector = TlsConnector::from(tls_config(pin));

    let mut listeners = Vec::new();
    let mut v6_missing = Vec::new();
    for &(local, port) in ports {
        match TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, local))).await {
            Ok(l) => listeners.push((port, l)),
            Err(e) => return Err(port_busy_error(local, e)),
        }
        // the same port on [::1]: without hosts the game reaches the server as https://localhost, and Windows resolves
        // localhost to ::1 first. Not fatal: with IPv6 off the game takes 127.0.0.1
        match TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, local))).await {
            Ok(l) => listeners.push((port, l)),
            Err(e) => v6_missing.push(format!("{local}: {e}")),
        }
    }

    let activity = Arc::new(Activity::default());
    let shared = Arc::new(Shared { cfg, name, connector, sink, activity: activity.clone() });
    if !v6_missing.is_empty() {
        (shared.sink)(ConnLog { ts: unix_now(), port: 0, up: 0, down: 0, ms: 0, reason: format!("ipv6 bind skipped: {}", v6_missing.join("; ")) });
    }
    let (tx, rx) = watch::channel(false);
    let mut handles = Vec::new();
    for (port, listener) in listeners {
        let shared = shared.clone();
        let mut stop = rx.clone();
        handles.push(tokio::spawn(async move {
            loop {
                tokio::select! {
                    acc = listener.accept() => {
                        let Ok((sock, _)) = acc else { continue };
                        let shared = shared.clone();
                        let stop = stop.clone();
                        tokio::spawn(async move {
                            let log = handle(sock, port, &shared, stop).await;
                            (shared.sink)(log);
                        });
                    }
                    _ = stop.changed() => break,
                }
            }
        }));
    }
    // Not fatal: only the few players on the tunnel need it, and their fight fails visibly in that case
    if let Some(port) = udp_port {
        match UdpSocket::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await {
            Ok(sock) => handles.push(tokio::spawn(udp_listener(sock, shared.clone(), rx.clone()))),
            Err(e) => (shared.sink)(ConnLog { ts: unix_now(), port, up: 0, down: 0, ms: 0, reason: format!("udp bind: {e}") }),
        }
    }
    Ok(Relay { shutdown: tx, listeners: handles, activity })
}

fn keepalive(s: &TcpStream) {
    let ka = socket2::TcpKeepalive::new().with_time(Duration::from_secs(30)).with_interval(Duration::from_secs(10));
    let _ = socket2::SockRef::from(s).set_tcp_keepalive(&ka);
    let _ = s.set_nodelay(true);
}

#[derive(Serialize)]
struct Preface<'a> {
    v: u8,
    token: &'a str,
    port: u16,
    modset: &'a str,
    client: &'a str,
    /// What this launcher can do (`udp`: the fight tunnel); lets the backend offer it
    caps: &'a [&'a str],
    /// This connection is the fight tunnel: frames `[len u16 LE][datagram]`
    udp: bool,
}

#[derive(Deserialize)]
struct PrefaceReply {
    ok: bool,
    #[serde(default)]
    error: Option<String>,
}

/// TLS to the server's relay + preface; Ok when the server accepted
async fn open_remote(sh: &Shared, port: u16, udp: bool) -> std::result::Result<BufReader<TlsStream<TcpStream>>, String> {
    let tcp = tokio::time::timeout(PREFACE_TIMEOUT, TcpStream::connect((sh.cfg.address.as_str(), sh.cfg.port)))
        .await
        .map_err(|_| "connect timeout".to_string())?
        .map_err(|e| format!("connect: {e}"))?;
    keepalive(&tcp);
    let tls = tokio::time::timeout(PREFACE_TIMEOUT, sh.connector.connect(sh.name.clone(), tcp))
        .await
        .map_err(|_| "tls timeout".to_string())?
        .map_err(|e| format!("tls: {e}"))?;
    let mut remote = BufReader::new(tls);

    let client = concat!("dbl/", env!("CARGO_PKG_VERSION"));
    let preface = Preface { v: 1, token: &sh.cfg.token, port, modset: &sh.cfg.modset, client, caps: &["udp"], udp };
    let mut line = serde_json::to_vec(&preface).map_err(|e| e.to_string())?;
    line.push(b'\n');
    if line.len() > PREFACE_LIMIT {
        return Err("preface too long".to_string());
    }
    remote.write_all(&line).await.map_err(|e| format!("send preface: {e}"))?;
    remote.flush().await.map_err(|e| format!("send preface: {e}"))?;

    let mut reply = Vec::new();
    tokio::time::timeout(PREFACE_TIMEOUT, (&mut remote).take(PREFACE_LIMIT as u64).read_until(b'\n', &mut reply))
        .await
        .map_err(|_| "preface reply timeout".to_string())?
        .map_err(|e| format!("preface reply: {e}"))?;
    let reply: PrefaceReply = serde_json::from_slice(&reply).map_err(|_| "bad preface reply".to_string())?;
    if !reply.ok {
        return Err(format!("rejected: {}", reply.error.unwrap_or_else(|| "?".into())));
    }
    Ok(remote)
}

/// One game socket = one tunnel connection. Datagrams are dropped, not queued without limit: a stale one is useless.
async fn udp_listener(sock: UdpSocket, sh: Arc<Shared>, mut stop: watch::Receiver<bool>) {
    let sock = Arc::new(sock);
    let mut flows: HashMap<SocketAddr, (mpsc::Sender<Vec<u8>>, Instant)> = HashMap::new();
    let mut buf = vec![0u8; UDP_MAX];
    loop {
        tokio::select! {
            got = sock.recv_from(&mut buf) => {
                // Windows reports ICMP "port unreachable" as a recv error: not a reason to stop
                let Ok((n, from)) = got else { continue };
                flows.retain(|_, (tx, at)| !tx.is_closed() || at.elapsed() < UDP_RETRY);
                let tx = match flows.get(&from) {
                    Some((tx, _)) => tx.clone(),
                    None => {
                        let (tx, rx) = mpsc::channel(256);
                        flows.insert(from, (tx.clone(), Instant::now()));
                        let (sh, sock, stop) = (sh.clone(), sock.clone(), stop.clone());
                        tokio::spawn(async move {
                            let log = udp_flow(rx, sock, from, &sh, stop).await;
                            (sh.sink)(log);
                        });
                        tx
                    }
                };
                let _ = tx.try_send(buf[..n].to_vec());
            }
            _ = stop.changed() => break,
        }
    }
}

async fn udp_flow(mut rx: mpsc::Receiver<Vec<u8>>, sock: Arc<UdpSocket>, game: SocketAddr, sh: &Shared, mut stop: watch::Receiver<bool>) -> ConnLog {
    let t0 = Instant::now();
    let mut log = ConnLog { ts: unix_now(), port: UDP_PORT, up: 0, down: 0, ms: 0, reason: String::new() };
    match open_remote(sh, 0, true).await {
        Err(reason) => log.reason = reason,
        Ok(remote) => {
            let (mut rd, mut wr) = tokio::io::split(remote);
            let (mut up, mut down) = (0u64, 0u64);
            // two independent loops: reading a frame must not be cancelled halfway
            let to_server = async {
                loop {
                    let d = match tokio::time::timeout(UDP_IDLE, rx.recv()).await {
                        Ok(Some(d)) => d,
                        Ok(None) => return "closed".to_string(),
                        Err(_) => return "idle".to_string(),
                    };
                    up += d.len() as u64;
                    let mut out = Vec::with_capacity(d.len() + 2);
                    out.extend_from_slice(&(d.len() as u16).to_le_bytes());
                    out.extend_from_slice(&d);
                    if let Err(e) = wr.write_all(&out).await {
                        return format!("io up: {e}");
                    }
                    let _ = wr.flush().await;
                }
            };
            let to_game = async {
                let mut buf = vec![0u8; UDP_MAX];
                loop {
                    let n = match rd.read_u16_le().await {
                        Ok(n) => n as usize,
                        Err(e) => return format!("io down: {e}"),
                    };
                    if n > UDP_MAX {
                        return format!("frame {n}");
                    }
                    if let Err(e) = rd.read_exact(&mut buf[..n]).await {
                        return format!("io down: {e}");
                    }
                    down += n as u64;
                    let _ = sock.send_to(&buf[..n], game).await;
                }
            };
            log.reason = tokio::select! {
                r = to_server => r,
                r = to_game => r,
                _ = stop.changed() => "relay stopped".to_string(),
            };
            log.up = up;
            log.down = down;
        }
    }
    log.ms = t0.elapsed().as_millis() as u64;
    log
}

async fn handle(mut inbound: TcpStream, port: u16, sh: &Shared, mut stop: watch::Receiver<bool>) -> ConnLog {
    let t0 = Instant::now();
    let mut log = ConnLog { ts: unix_now(), port, up: 0, down: 0, ms: 0, reason: String::new() };
    sh.activity.accepted(port);
    keepalive(&inbound);
    let result = async {
        let remote = open_remote(sh, port, false).await?;

        let (mut in_r, mut in_w) = inbound.split();
        let (mut re_r, mut re_w) = tokio::io::split(remote);
        let (mut up, mut down) = (0u64, 0u64);
        let reason = tokio::select! {
            (a, b) = async {
                tokio::join!(pump(&mut in_r, &mut re_w, &mut up), pump(&mut re_r, &mut in_w, &mut down))
            } => match (a, b) {
                (Ok(()), Ok(())) => "closed".to_string(),
                (Err(e), _) => format!("io up: {e}"),
                (_, Err(e)) => format!("io down: {e}"),
            },
            _ = stop.changed() => "relay stopped".to_string(),
        };
        Ok((up, down, reason))
    }
    .await;
    match result {
        Ok((up, down, reason)) => {
            log.up = up;
            log.down = down;
            log.reason = reason;
        }
        Err(reason) => log.reason = reason,
    }
    log.ms = t0.elapsed().as_millis() as u64;
    log
}

/// One direction: copy until EOF, then half-close the other side. Counts bytes even if it fails midway.
/// A TLS peer that drops TCP without close_notify counts as EOF: the game cannot tell the difference.
async fn pump<R, W>(r: &mut R, w: &mut W, count: &mut u64) -> std::io::Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };
        w.write_all(&buf[..n]).await?;
        *count += n as u64;
    }
    w.shutdown().await
}

/// Manifest pin -> exactly that certificate; no pin -> built-in pins or the CA chain. TLS 1.3 only.
fn tls_config(pin: Option<[u8; 32]>) -> Arc<ClientConfig> {
    let verifier = match pin {
        Some(pin) => crate::net::Verifier::new(vec![hex::encode(pin)], false),
        None => crate::net::Verifier::builtin(),
    };
    Arc::new(crate::net::tls_config(verifier, true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reach_watch() {
        let t0 = Instant::now();
        let s = |n: u64| t0 + Duration::from_secs(n);
        // logged in, came to the directory: done
        let mut w = ReachWatch::new((3, 1), Duration::from_secs(90));
        assert_eq!(w.step((3, 1), s(0)), Reach::Waiting);
        assert_eq!(w.step((4, 1), s(5)), Reach::Waiting);
        assert_eq!(w.step((5, 2), s(20)), Reach::Reached);
        // logged in and nothing for 90 s: missed; the clock starts at the login, not at the game start
        let mut w = ReachWatch::new((0, 0), Duration::from_secs(90));
        assert_eq!(w.step((0, 0), s(200)), Reach::Waiting, "no login yet: the player may sit in the game's own launcher");
        assert_eq!(w.step((1, 0), s(210)), Reach::Waiting);
        assert_eq!(w.step((6, 0), s(299)), Reach::Waiting);
        assert_eq!(w.step((6, 0), s(300)), Reach::Missed);
    }

    #[test]
    fn activity_counts_by_port() {
        let a = Activity::default();
        for p in [443, 80, 8085, 6601, 6604, 27777] {
            a.accepted(p);
        }
        assert_eq!(a.counts(), (2, 3));
    }
}
