//! Launcher relay acceptor (docs/LAUNCHER_SPEC.md §4.4). For each TCP connection of the game the launcher opens TLS to here and
//! sends as the first line `{"v":1,"token":"…","port":6601,"modset":"…","client":"…"}\n`. The server checks the session,
//! connects to its own game port (`[relay] upstream_ip`), registers this connection's address in `registry` and
//! answers `{"ok":true}\n`; after that, raw bytes in both directions. Denial: `{"ok":false,"error":"…"}\n` and close.

use super::{registry, sessions, AuthCore};
use serde::Deserialize;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::rustls;
use tokio_rustls::TlsAcceptor;

pub const ALLOWED_PORTS: [u16; 7] = [80, 443, 8085, 6601, 6602, 6603, 6604];
const PREFACE_MAX: u64 = 4096;
const PREFACE_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
pub struct Preface {
    pub v: u32,
    pub token: String,
    pub port: u16,
    #[serde(default)]
    pub modset: String,
    #[serde(default)]
    pub client: String,
    /// What the launcher supports (`udp`: battle tunnel, see udp_tunnel)
    #[serde(default)]
    pub caps: Vec<String>,
    /// This connection is a battle UDP tunnel (frames `[length u16 LE][data]`), `port` is not used
    #[serde(default)]
    pub udp: bool,
}

/// Parse and check the preface without network: Ok = port, Err = denial code
pub fn check_preface(line: &str) -> Result<Preface, &'static str> {
    let p: Preface = serde_json::from_str(line.trim()).map_err(|_| "bad_preface")?;
    if p.v != 1 {
        return Err("bad_version");
    }
    if !p.udp && !ALLOWED_PORTS.contains(&p.port) {
        return Err("bad_port");
    }
    Ok(p)
}

/// Launcher version from the preface: `dbl/1.7.0` → (1, 7, 0); no version or a foreign format → None
pub fn client_version(client: &str) -> Option<(u32, u32, u32)> {
    let v = client.trim().strip_prefix("dbl/")?;
    let v = v.split(['-', '+', ' ']).next()?;
    let mut it = v.split('.').map(|n| n.parse::<u32>().ok());
    Some((it.next()??, it.next().flatten().unwrap_or(0), it.next().flatten().unwrap_or(0)))
}

/// Launcher too old for the server (`[relay] min_client`); empty threshold → let everyone through, launcher without version → old
pub fn client_outdated(client: &str, min_client: &str) -> bool {
    let Some(min) = client_version(&format!("dbl/{}", min_client.trim())).filter(|_| !min_client.trim().is_empty()) else {
        return false;
    };
    client_version(client).map_or(true, |v| v < min)
}

fn tls_acceptor(cert: &str, key: &str) -> Result<TlsAcceptor, String> {
    let certs = rustls_pemfile::certs(&mut std::fs::read(cert).map_err(|e| format!("{}: {}", cert, e))?.as_slice())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("{}: {}", cert, e))?;
    let key = rustls_pemfile::private_key(&mut std::fs::read(key).map_err(|e| format!("{}: {}", key, e))?.as_slice())
        .map_err(|e| format!("{}: {}", key, e))?
        .ok_or_else(|| format!("{}: no private key", key))?;
    let cfg = rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key).map_err(|e| e.to_string())?;
    Ok(TlsAcceptor::from(Arc::new(cfg)))
}

pub async fn run(core: Arc<AuthCore>) {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let r = &core.cfg.relay;
    let acceptor = match tls_acceptor(&r.cert, &r.key) {
        Ok(a) => a,
        Err(e) => return eprintln!("[Relay] disabled: relay certificate: {}", e),
    };
    let listener = match TcpListener::bind(&r.bind).await {
        Ok(l) => l,
        Err(e) => return eprintln!("[Relay] disabled: could not bind {}: {}", r.bind, e),
    };
    println!(
        "[Relay] launcher relay on {} (outside {}:{}, pin {}) → {}",
        r.bind,
        r.public_address,
        r.public_port,
        super::web::cert_sha256(&r.cert).unwrap_or_default(),
        r.upstream_ip
    );
    while let Ok((tcp, peer)) = listener.accept().await {
        let (core, acceptor) = (core.clone(), acceptor.clone());
        tokio::spawn(async move {
            if let Err(e) = handle(core, acceptor, tcp, peer).await {
                println!("[Relay] {}: {}", peer, e);
            }
        });
    }
}

/// Connection to the game registered in registry BEFORE connect: otherwise the listener could accept it before insert and
/// deny it as "not via relay" (prod 03.11: rare DirServer denials from 10.0.0.2 under a burst of connections). The game address is
/// our own (upstream_ip is local): bind to it, register, connect. Not local or not an IP: as before.
async fn connect_registered(
    upstream_ip: &str,
    port: u16,
    acc: &Arc<super::accounts::Account>,
) -> std::io::Result<(TcpStream, SocketAddr, Arc<tokio::sync::Notify>)> {
    if let Ok(ip) = upstream_ip.parse::<std::net::IpAddr>() {
        let sock = if ip.is_ipv4() { tokio::net::TcpSocket::new_v4()? } else { tokio::net::TcpSocket::new_v6()? };
        if let Ok(local) = sock.bind(SocketAddr::new(ip, 0)).and_then(|_| sock.local_addr()) {
            let kill = registry::insert(local, acc.clone());
            return match sock.connect(SocketAddr::new(ip, port)).await {
                Ok(up) => Ok((up, local, kill)),
                Err(e) => {
                    registry::remove(&local);
                    Err(e)
                }
            };
        }
    }
    let up = TcpStream::connect((upstream_ip, port)).await?;
    let local = up.local_addr()?;
    let kill = registry::insert(local, acc.clone());
    Ok((up, local, kill))
}

/// A connection shorter than this is "short": logged only as a counter (short_conn_tick)
const SHORT_CONN: Duration = Duration::from_secs(5);
const SHORT_CONN_REPORT_SECS: u64 = 600;

fn short_conn_tick() {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNT: AtomicU64 = AtomicU64::new(0);
    static SINCE: AtomicU64 = AtomicU64::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let n = COUNT.fetch_add(1, Ordering::Relaxed) + 1;
    let since = SINCE.load(Ordering::Relaxed);
    if since == 0 {
        let _ = SINCE.compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    } else if now.saturating_sub(since) >= SHORT_CONN_REPORT_SECS
        && SINCE.compare_exchange(since, now, Ordering::Relaxed, Ordering::Relaxed).is_ok()
    {
        COUNT.fetch_sub(n, Ordering::Relaxed);
        println!("[Relay] short connections (< {} s) in {} s: {}", SHORT_CONN.as_secs(), now - since, n);
    }
}

async fn handle(core: Arc<AuthCore>, acceptor: TlsAcceptor, tcp: TcpStream, peer: SocketAddr) -> Result<(), String> {
    let _ = tcp.set_nodelay(true);
    let tls = tokio::time::timeout(PREFACE_TIMEOUT, acceptor.accept(tcp)).await.map_err(|_| "TLS: timeout")?.map_err(|e| format!("TLS: {}", e))?;
    let mut tls = BufReader::new(tls);

    let mut line = String::new();
    tokio::time::timeout(PREFACE_TIMEOUT, (&mut tls).take(PREFACE_MAX).read_line(&mut line))
        .await
        .map_err(|_| "preface: timeout")?
        .map_err(|e| format!("preface: {}", e))?;

    let reject = |code: &'static str| format!("{{\"ok\":false,\"error\":\"{}\"}}\n", code);
    let pre = match check_preface(&line) {
        Ok(p) => p,
        Err(code) => {
            let _ = tls.write_all(reject(code).as_bytes()).await;
            return Err(format!("denied {}", code));
        }
    };
    if client_outdated(&pre.client, &core.cfg.relay.min_client) {
        let _ = tls.write_all(reject("launcher_outdated").as_bytes()).await;
        return Err(format!("denied launcher_outdated (client {:?}, need {})", pre.client, core.cfg.relay.min_client));
    }
    let acc = match sessions::resolve(&core.db, &pre.token).await {
        Some(a) if a.banned => {
            let _ = tls.write_all(reject("banned").as_bytes()).await;
            return Err(format!("denied banned ({})", a.open_id));
        }
        Some(a) if !super::accounts::login_allowed(&a) => {
            let _ = tls.write_all(reject("not_whitelisted").as_bytes()).await;
            return Err(format!("denied not_whitelisted ({})", a.open_id));
        }
        Some(a) => Arc::new(a),
        None => {
            let _ = tls.write_all(reject("expired").as_bytes()).await;
            return Err("denied expired".into());
        }
    };

    if pre.caps.iter().any(|c| c == super::udp_tunnel::CAP) {
        super::udp_tunnel::set_capable(&acc.open_id, true);
    }
    if pre.udp {
        return udp_tunnel(tls, &acc, peer).await;
    }

    // before the game's first byte: the listener will find the player by this address
    let (mut up, local, kill) = connect_registered(&core.cfg.relay.upstream_ip, pre.port, &acc)
        .await
        .map_err(|e| format!("port {}: {}", pre.port, e))?;
    let _ = up.set_nodelay(true);
    let _ = tls.write_all(b"{\"ok\":true}\n").await;
    let _ = tls.flush().await;

    let started = Instant::now();
    // a ban (auth::ban, LIVE subscription) drops the connection immediately
    let res = tokio::select! {
        r = tokio::io::copy_bidirectional(&mut tls, &mut up) => r,
        _ = kill.notified() => Err(std::io::Error::new(std::io::ErrorKind::ConnectionAborted, "ban")),
    };
    registry::remove(&local);
    // a drop without TLS close_notify (the client just closed the socket) is normal; the counters are unknown then
    // 11.11: short successful connections (game requests, ~1500 per minute on prod: 31k lines in 20 min, syslog 5 GB)
    // are not logged one by one, only as a counter every 10 minutes; long and dropped ones as before
    if res.is_ok() && started.elapsed() < SHORT_CONN {
        short_conn_tick();
        return Ok(());
    }
    let traffic = match res {
        Ok((c2s, s2c)) => format!("↑{} ↓{} bytes", c2s, s2c),
        Err(e) => format!("closed: {}", e),
    };
    println!(
        "[Relay] {} '{}' (open_id {}) port {}: {} s, {}, client {}, modset {}",
        peer,
        acc.display_name,
        acc.open_id,
        pre.port,
        started.elapsed().as_secs(),
        traffic,
        if pre.client.is_empty() { "-" } else { &pre.client },
        if pre.modset.is_empty() { "-" } else { &pre.modset }
    );
    Ok(())
}

/// Battle tunnel of one game socket: TLS frames <-> datagrams to the game server of the player's room
async fn udp_tunnel<S>(mut tls: S, acc: &super::accounts::Account, peer: SocketAddr) -> Result<(), String>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    use super::udp_tunnel::MAX_FRAME;
    let reject = |code: &str| format!("{{\"ok\":false,\"error\":\"{}\"}}\n", code);
    if !super::udp_tunnel::is_on(&acc.open_id) {
        let _ = tls.write_all(reject("udp_off").as_bytes()).await;
        return Err(format!("UDP tunnel: denied udp_off ({})", acc.open_id));
    }
    let Some(ds) = crate::lobby::matchmaking::ds_of_player(&acc.open_id) else {
        let _ = tls.write_all(reject("no_room").as_bytes()).await;
        return Err(format!("UDP tunnel: denied no_room ({})", acc.open_id));
    };
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await.map_err(|e| format!("UDP tunnel: {}", e))?;
    sock.connect(&ds).await.map_err(|e| format!("UDP tunnel: {}: {}", ds, e))?;
    let _ = tls.write_all(b"{\"ok\":true}\n").await;
    let _ = tls.flush().await;

    let started = Instant::now();
    let (mut up, mut down) = (0u64, 0u64);
    let mut from_ds = vec![0u8; MAX_FRAME];
    let (mut rd, mut wr) = tokio::io::split(tls);
    // two independent loops: reading a frame must not be cancelled midway, or stream parsing gets out of sync
    let c2s = async {
        loop {
            let len = match tokio::time::timeout(Duration::from_secs(120), rd.read_u16_le()).await {
                Ok(Ok(l)) => l as usize,
                Ok(Err(e)) => return format!("player: {}", e),
                Err(_) => return "no data for 120 s".to_string(),
            };
            if len > MAX_FRAME {
                return format!("frame of {} bytes", len);
            }
            let mut buf = vec![0u8; len];
            if let Err(e) = rd.read_exact(&mut buf).await {
                return format!("player: {}", e);
            }
            up += len as u64;
            // losing a datagram is normal for the game, the tunnel is not dropped over it
            let _ = sock.send(&buf).await;
        }
    };
    let s2c = async {
        loop {
            // ICMP "port closed": the server is restarting, the player will sort it out
            let Ok(n) = sock.recv(&mut from_ds).await else { continue };
            down += n as u64;
            let mut out = Vec::with_capacity(n + 2);
            out.extend_from_slice(&(n as u16).to_le_bytes());
            out.extend_from_slice(&from_ds[..n]);
            if let Err(e) = wr.write_all(&out).await {
                return format!("player: {}", e);
            }
            let _ = wr.flush().await;
        }
    };
    let why = tokio::select! {
        w = c2s => w,
        w = s2c => w,
    };
    println!(
        "[Relay] {} '{}' (open_id {}) UDP tunnel -> {}: {} s, ↑{} ↓{} bytes, {}",
        peer,
        acc.display_name,
        acc.open_id,
        ds,
        started.elapsed().as_secs(),
        up,
        down,
        why
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{check_preface, client_outdated, client_version};

    #[test]
    fn launcher_version_gate() {
        assert_eq!(client_version("dbl/1.5.1"), Some((1, 5, 1)));
        assert_eq!(client_version("dbl/1.7"), Some((1, 7, 0)));
        assert_eq!(client_version("dbl/1.6.0-intl"), Some((1, 6, 0)));
        assert_eq!(client_version(""), None);
        assert_eq!(client_version("curl/8"), None);
        assert!(!client_outdated("dbl/1.5.1", ""), "no threshold: let everyone through");
        assert!(!client_outdated("", "  "), "empty threshold");
        assert!(client_outdated("dbl/1.6.0", "1.7.0"));
        assert!(client_outdated("dbl/1.6.9", "1.7.0"));
        assert!(!client_outdated("dbl/1.7.0", "1.7.0"));
        assert!(!client_outdated("dbl/1.10.0", "1.7.0"), "numbers, not strings");
        assert!(client_outdated("", "1.7.0"), "launcher without version: old");
    }

    #[test]
    fn preface_checks() {
        assert_eq!(check_preface(r#"{"v":1,"token":"t","port":6601}"#).unwrap().port, 6601);
        assert_eq!(check_preface(r#"{"v":1,"token":"t","port":22}"#).err(), Some("bad_port"));
        assert_eq!(check_preface(r#"{"v":2,"token":"t","port":443}"#).err(), Some("bad_version"));
        assert_eq!(check_preface("garbage").err(), Some("bad_preface"));
        let t = check_preface(r#"{"v":1,"token":"t","port":0,"udp":true,"caps":["udp"]}"#).unwrap();
        assert!(t.udp && t.caps == ["udp"], "tunnel without a game port");
        assert!(!check_preface(r#"{"v":1,"token":"t","port":6601}"#).unwrap().udp, "old launcher");
    }
}
