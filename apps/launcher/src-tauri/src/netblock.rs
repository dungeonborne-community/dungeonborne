//! "The provider cuts the connection to our servers": recognised so the launcher can recommend WARP or a VPN now,
//! and pick another way in (a different ingress address) later. Two patterns seen in Russia, 03–08.11:
//!
//! - **reset**: every TLS connection to the server's host is reset right after the ClientHello (ISP one ISP,
//!   SPb: any SNI to any Hetzner address). Nothing reaches the server.
//! - **freeze**: the connection opens and data flows, then stops in both directions after ~1.5–16 KB while the
//!   socket stays open (relay stalls of ~20 s in `relay.log`, game login "CURLCode 28", downloads stuck at 16218 B).
//!
//! Two inputs: [`probe`] runs on demand when a request to the server failed at the network level, and
//! [`from_stalls`] turns the relay's stall detector (`netcheck`) into the same [`Verdict`]. What is NOT the
//! provider: no internet at all (the control address is unreachable too), a server that refuses or doesn't answer
//! (down, restarting), HTTP errors like 502 during a deploy and maintenance (the server answers, so no probe runs).
//!
//! Verdicts are also kept in a small queue ([`Pending`]) and sent to the server's log (`POST net/report`) once a
//! connection works, e.g. after the player turned WARP on: players with the reset pattern can't reach it before.

use std::io;
use std::time::Duration;

use reqwest::{Client, Url};
use rustls::pki_types::ServerName;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::manifest::Manifest;
use crate::net;

/// Cloudflare's own trace page: neutral, small, reachable wherever "the internet works"
pub const CONTROL_URL: &str = "https://1.1.1.1/cdn-cgi/trace";
const STEP_TIMEOUT: Duration = Duration::from_secs(8);
/// A download that got no bytes for this long is frozen
const STALL: Duration = Duration::from_secs(10);
/// This much of a big file must arrive (frozen connections stop at 1.5–16 KB)
const ENOUGH: usize = 128 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// TLS reset right after the ClientHello
    Reset,
    /// Data stopped mid-transfer, the socket stayed open
    Freeze,
    /// Repeated ~20 s relay stalls (`netcheck`)
    Stall,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Reset => "reset",
            Kind::Freeze => "freeze",
            Kind::Stall => "stall",
        }
    }
}

/// The provider cuts the connection: what was seen
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub kind: Kind,
    /// One line for the log and the support report; no tokens
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Blocked(Verdict),
    /// The control address fails too: no internet, not the provider's choice of our servers
    Offline(String),
    /// The server doesn't take connections (down, restarting): can't tell the provider from an outage, so no
    Unreachable(String),
    /// Everything that was tried works
    Fine,
}

/// Where to look: the server's host and a big file on it
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub host: String,
    pub port: u16,
    /// Plain http is only allowed to loopback (tests)
    pub tls: bool,
    /// ≥ [`ENOUGH`] bytes to download (the launcher build, a mod package); `None` skips the freeze test
    pub big: Option<Url>,
}

impl Target {
    /// From the server's manifest (the URL it was added by, for the host when the manifest names none)
    pub fn from_manifest(m: &Manifest, manifest_url: &str) -> Option<Target> {
        let base = [m.auth.as_ref().map(|a| a.api.as_str()), m.status_url.as_deref(), Some(manifest_url)]
            .into_iter()
            .flatten()
            .find_map(|u| net::check_url(u).ok())?;
        let big = m
            .launcher
            .as_ref()
            .map(|l| l.url.as_str())
            .into_iter()
            .chain(m.mods.iter().filter_map(|r| r.url.as_deref()))
            .find_map(|u| base.join(u).ok().filter(net::url_allowed));
        Some(Target {
            host: base.host_str()?.trim_start_matches('[').trim_end_matches(']').to_string(),
            port: base.port_or_known_default()?,
            tls: base.scheme() == "https",
            big,
        })
    }
}

/// Runs the checks in order and stops at the first answer. Takes up to ~40 s on a bad line.
pub async fn probe(client: &Client, target: &Target, control: &Url) -> Outcome {
    // 1. the internet itself; any HTTP answer counts
    match tokio::time::timeout(STEP_TIMEOUT, client.get(control.clone()).send()).await {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Outcome::Offline(format!("{control}: {}", short(&e))),
        Err(_) => return Outcome::Offline(format!("{control}: no answer in {} s", STEP_TIMEOUT.as_secs())),
    }
    // 2. a TCP connection to the server
    let tcp = match tokio::time::timeout(STEP_TIMEOUT, TcpStream::connect((target.host.as_str(), target.port))).await {
        Ok(Ok(s)) => s,
        Ok(Err(e)) => return Outcome::Unreachable(format!("tcp {}:{}: {e}", target.host, target.port)),
        Err(_) => return Outcome::Unreachable(format!("tcp {}:{}: no answer", target.host, target.port)),
    };
    // 3. the TLS handshake, with the server name as the launcher sends it
    if target.tls {
        if let Some(v) = handshake(tcp, &target.host).await {
            return Outcome::Blocked(v);
        }
    }
    // 4. a big download must not stop at a few KB
    if let Some(url) = &target.big {
        if let Some(v) = download(client, url).await {
            return Outcome::Blocked(v);
        }
    }
    Outcome::Fine
}

/// `Some` — the connection was reset/closed during the handshake. A certificate problem or a timeout is not counted.
async fn handshake(tcp: TcpStream, host: &str) -> Option<Verdict> {
    let name = ServerName::try_from(host.to_string()).ok()?;
    let connector = TlsConnector::from(std::sync::Arc::new(net::tls_config(net::Verifier::builtin(), false)));
    match tokio::time::timeout(STEP_TIMEOUT, connector.connect(name, tcp)).await {
        Ok(Err(e)) if is_cut(&e) => {
            Some(Verdict { kind: Kind::Reset, detail: format!("tls to {host}: {e} right after ClientHello") })
        }
        _ => None,
    }
}

/// `Some` — the transfer was reset or stalled before [`ENOUGH`] bytes
async fn download(client: &Client, url: &Url) -> Option<Verdict> {
    let mut resp = match tokio::time::timeout(STEP_TIMEOUT, client.get(url.clone()).send()).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) if cut_chain(&e) => {
            return Some(Verdict { kind: Kind::Reset, detail: format!("{}: {} before the answer", path(url), short(&e)) })
        }
        _ => return None,
    };
    if !resp.status().is_success() {
        return None;
    }
    let mut got = 0usize;
    loop {
        match tokio::time::timeout(STALL, resp.chunk()).await {
            Ok(Ok(Some(c))) => {
                got += c.len();
                if got >= ENOUGH {
                    return None;
                }
            }
            Ok(Ok(None)) => return None,
            Ok(Err(e)) if cut_chain(&e) => {
                return Some(Verdict { kind: Kind::Freeze, detail: format!("{}: cut at {got} bytes ({})", path(url), short(&e)) })
            }
            Ok(Err(_)) => return None,
            Err(_) => {
                return Some(Verdict {
                    kind: Kind::Freeze,
                    detail: format!("{}: stopped at {got} bytes, nothing for {} s", path(url), STALL.as_secs()),
                })
            }
        }
    }
}

/// The relay's stall detector found repeated frozen connections
pub fn from_stalls(f: &crate::netcheck::Finding) -> Verdict {
    Verdict { kind: Kind::Stall, detail: f.summary() }
}

fn is_cut(e: &io::Error) -> bool {
    matches!(
        e.kind(),
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted | io::ErrorKind::UnexpectedEof | io::ErrorKind::BrokenPipe
    )
}

/// Somewhere in the error's sources: a reset/abort/eof
fn cut_chain(e: &(dyn std::error::Error + 'static)) -> bool {
    let mut cur: Option<&(dyn std::error::Error + 'static)> = Some(e);
    while let Some(x) = cur {
        if x.downcast_ref::<io::Error>().is_some_and(is_cut) {
            return true;
        }
        cur = x.source();
    }
    false
}

/// The innermost error text: reqwest's own is only "error sending request for url (...)"
fn short(e: &(dyn std::error::Error + 'static)) -> String {
    let mut cur = e;
    while let Some(s) = cur.source() {
        cur = s;
    }
    cur.to_string()
}

fn path(u: &Url) -> String {
    format!("{}{}", u.host_str().unwrap_or(""), u.path())
}

// ---------------------------------------------------------------------------------------------- report queue

/// One verdict waiting to reach the server's log
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub server_id: String,
    pub kind: Kind,
    pub detail: String,
    /// unix seconds when it was seen
    pub at: i64,
}

/// Verdicts not sent yet (`net_reports.json`): the newest few per server, one per kind
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Pending {
    pub reports: Vec<Report>,
}

const MAX_PENDING: usize = 20;

impl Pending {
    /// A newer verdict of the same kind replaces the older one: the server needs the scale, not every repeat
    pub fn add(&mut self, r: Report) {
        self.reports.retain(|x| !(x.server_id == r.server_id && x.kind == r.kind));
        self.reports.push(r);
        let extra = self.reports.len().saturating_sub(MAX_PENDING);
        self.reports.drain(..extra);
    }

    pub fn take(&mut self, server_id: &str) -> Vec<Report> {
        let (mine, rest) = std::mem::take(&mut self.reports).into_iter().partition(|r| r.server_id == server_id);
        self.reports = rest;
        mine
    }

    pub fn path() -> std::path::PathBuf {
        crate::store::data_dir().join("net_reports.json")
    }

    pub fn load() -> Pending {
        std::fs::read(Self::path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::create_dir_all(crate::store::data_dir());
            let _ = std::fs::write(Self::path(), b);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    /// Plain http server on loopback; `serve` gets each accepted connection
    async fn server<F, Fut>(serve: F) -> u16
    where
        F: Fn(TcpStream) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((s, _)) = l.accept().await {
                tokio::spawn(serve(s));
            }
        });
        port
    }

    async fn read_request(s: &mut TcpStream) {
        let mut buf = [0u8; 4096];
        let _ = s.read(&mut buf).await;
    }

    /// 200 with `len` declared and `send` bytes actually sent, then the socket stays open
    async fn file(mut s: TcpStream, len: usize, send: usize) {
        read_request(&mut s).await;
        let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {len}\r\n\r\n");
        let _ = s.write_all(head.as_bytes()).await;
        let _ = s.write_all(&vec![b'x'; send]).await;
        tokio::time::sleep(Duration::from_secs(60)).await;
    }

    /// Reads the first bytes (a ClientHello) and resets the connection
    #[allow(deprecated)] // SO_LINGER 0: the way to send a RST
    async fn reset(mut s: TcpStream) {
        let mut buf = [0u8; 512];
        let _ = s.read(&mut buf).await;
        let _ = s.set_linger(Some(Duration::ZERO));
        drop(s);
    }

    async fn ok200(mut s: TcpStream) {
        read_request(&mut s).await;
        let _ = s.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok").await;
    }

    fn target(port: u16, tls: bool, big: Option<String>) -> Target {
        Target { host: "127.0.0.1".into(), port, tls, big: big.map(|u| Url::parse(&u).unwrap()) }
    }

    fn url(port: u16, p: &str) -> Url {
        Url::parse(&format!("http://127.0.0.1:{port}{p}")).unwrap()
    }

    #[tokio::test]
    async fn frozen_download_is_the_provider() {
        let control = server(ok200).await;
        let frozen = server(|s| file(s, 1 << 20, 16218)).await;
        let t = target(frozen, false, Some(url(frozen, "/launcher/big.exe").to_string()));
        match probe(&net::client(), &t, &url(control, "/cdn-cgi/trace")).await {
            Outcome::Blocked(v) => {
                assert_eq!(v.kind, Kind::Freeze);
                assert!(v.detail.contains("stopped at 16218 bytes"), "{}", v.detail);
            }
            o => panic!("{o:?}"),
        }
    }

    #[tokio::test]
    async fn reset_after_hello_is_the_provider() {
        let control = server(ok200).await;
        let cut = server(reset).await;
        match probe(&net::client(), &target(cut, true, None), &url(control, "/")).await {
            Outcome::Blocked(v) => assert_eq!(v.kind, Kind::Reset, "{}", v.detail),
            o => panic!("{o:?}"),
        }
    }

    #[tokio::test]
    async fn full_download_is_fine() {
        let control = server(ok200).await;
        let good = server(|s| file(s, ENOUGH + 10, ENOUGH + 10)).await;
        let t = target(good, false, Some(url(good, "/f").to_string()));
        assert_eq!(probe(&net::client(), &t, &url(control, "/")).await, Outcome::Fine);
    }

    #[tokio::test]
    async fn no_internet_and_server_down_are_not_the_provider() {
        // a closed port: refused at once
        let closed = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap().port();
        let frozen = server(|s| file(s, 1 << 20, 100)).await;
        let t = target(frozen, false, Some(url(frozen, "/f").to_string()));
        assert!(matches!(probe(&net::client(), &t, &url(closed, "/")).await, Outcome::Offline(_)));
        let control = server(ok200).await;
        assert!(matches!(probe(&net::client(), &target(closed, true, None), &url(control, "/")).await, Outcome::Unreachable(_)));
    }

    #[tokio::test]
    async fn http_errors_are_not_the_provider() {
        // 502 from nginx while the backend restarts: the network works
        let control = server(ok200).await;
        let bad = server(|mut s: TcpStream| async move {
            read_request(&mut s).await;
            let _ = s.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n").await;
        })
        .await;
        let t = target(bad, false, Some(url(bad, "/f").to_string()));
        assert_eq!(probe(&net::client(), &t, &url(control, "/")).await, Outcome::Fine);
    }

    /// A real server on a working line must come out `Fine`, not blocked:
    /// `DBL_MANIFEST=https://203.0.113.10/.well-known/dbserver.json cargo test probe_live -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn probe_live() {
        let url = std::env::var("DBL_MANIFEST").expect("DBL_MANIFEST");
        let client = net::client();
        let m = crate::manifest::fetch(&client, &url).await.expect("manifest");
        let t = Target::from_manifest(&m, &url).expect("target");
        println!("target {t:?}");
        let started = std::time::Instant::now();
        let o = probe(&client, &t, &Url::parse(CONTROL_URL).unwrap()).await;
        println!("outcome {o:?} in {:?}", started.elapsed());
        assert_eq!(o, Outcome::Fine);
    }

    #[test]
    fn target_from_the_manifest() {
        let m: Manifest = serde_json::from_str(
            r#"{"schema":"dbserver/1","id":"ru.x","name":"X","game":{"exe_sha256":"00"},"policy":{},
                "auth":{"api":"https://dbserver.example.org/api/launcher/v1"},
                "launcher":{"version":"1.5.1","url":"https://dbserver.example.org/launcher/l.exe","size":1,"sha256":"0","signature":"0"}}"#,
        )
        .unwrap();
        let t = Target::from_manifest(&m, "https://203.0.113.10/.well-known/dbserver.json").unwrap();
        assert_eq!((t.host.as_str(), t.port, t.tls), ("dbserver.example.org", 443, true));
        assert_eq!(t.big.unwrap().path(), "/launcher/l.exe");
    }

    #[test]
    fn pending_keeps_one_per_kind() {
        let r = |s: &str, k, at| Report { server_id: s.into(), kind: k, detail: String::new(), at };
        let mut p = Pending::default();
        p.add(r("a", Kind::Freeze, 1));
        p.add(r("a", Kind::Freeze, 2));
        p.add(r("a", Kind::Reset, 3));
        p.add(r("b", Kind::Stall, 4));
        assert_eq!(p.reports.len(), 3);
        let a = p.take("a");
        assert_eq!(a.iter().map(|x| x.at).collect::<Vec<_>>(), vec![2, 3]);
        assert_eq!(p.reports.len(), 1);
        for i in 0..30 {
            p.add(r(&format!("s{i}"), Kind::Stall, i));
        }
        assert_eq!(p.reports.len(), MAX_PENDING);
    }
}
