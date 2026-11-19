//! Network test for players whose relay hangs. TLS — the same library as the launcher and the relay (rustls).
//! Client: `dbnettest` — baseline through nginx (port 443), then tests against a test server on port 8443.
//! Server: `dbnettest serve <crt> <key> <port>` — run on the VM for the duration of the test, without the game or backend.
//!   F — a continuous file transfer: checks TLS from the rustls server side;
//!   G — a lobby-like exchange: small messages back and forth, then a big answer in one piece, and so on in a loop.
//! The client checks the certificate against a pinned fingerprint, like the launcher (`src/builtin.rs`).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection, SignatureScheme, StreamOwned};
use sha2::Digest;

const HOST: &str = "dbserver.example.org";
const PATH: &str = "/mods/ru.client-hooks-2026.1104.1930.zip";
const SIZE: u64 = 136768;
const PIN: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const STALL: Duration = Duration::from_secs(20);
const ROUNDS: usize = 10; // exchange rounds of G
const SMALL: usize = 300; // small message
const PONGS: usize = 12; // small messages per round, ~3.6 KB — like the start of the lobby before the big answer
const BIG: usize = 7000; // big answer in one piece, like player_get_info

#[derive(Debug)]
struct Pinned(Arc<CryptoProvider>);

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if hex::encode(sha2::Sha256::digest(end_entity.as_ref())) == PIN {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(rustls::CertificateError::ApplicationVerificationFailure))
        }
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

type Tls = StreamOwned<ClientConnection, TcpStream>;

fn connect(port: u16) -> Result<Tls, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let cfg = ClientConfig::builder_with_provider(provider.clone())
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned(provider)))
        .with_no_client_auth();
    let tcp = TcpStream::connect((HOST, port)).map_err(|e| format!("подключение: {e}"))?;
    let _ = tcp.set_nodelay(true);
    let _ = tcp.set_read_timeout(Some(STALL));
    let name = ServerName::try_from(HOST).map_err(|e| e.to_string())?;
    let conn = ClientConnection::new(Arc::new(cfg), name).map_err(|e| e.to_string())?;
    Ok(StreamOwned::new(conn, tcp))
}

fn stalled(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock)
}

/// Reads everything to the end of the stream. Returns (bytes, reason)
fn drain(tls: &mut Tls) -> (u64, String) {
    let mut got = 0u64;
    let mut buf = [0u8; 16384];
    loop {
        match tls.read(&mut buf) {
            Ok(0) => return (got, "конец".into()),
            Ok(n) => got += n as u64,
            Err(e) if stalled(&e) => return (got, format!("ЗАВИС: {} с без данных", STALL.as_secs())),
            Err(e) if got >= SIZE => return (got, format!("конец ({e})")),
            Err(e) => return (got, format!("ошибка: {e}")),
        }
    }
}

fn nginx() -> (u64, u64, String) {
    let mut tls = match connect(443) {
        Ok(t) => t,
        Err(e) => return (0, SIZE, e),
    };
    let req = format!("GET {PATH} HTTP/1.1\r\nHost: {HOST}\r\nUser-Agent: dbnettest\r\nConnection: close\r\n\r\n");
    if let Err(e) = tls.write_all(req.as_bytes()) {
        return (0, SIZE, format!("запрос: {e}"));
    }
    let (got, why) = drain(&mut tls);
    (got, SIZE, why)
}

fn bulk() -> (u64, u64, String) {
    let mut tls = match connect(8443) {
        Ok(t) => t,
        Err(e) => return (0, SIZE, e),
    };
    if let Err(e) = tls.write_all(b"FILE\n") {
        return (0, SIZE, format!("запрос: {e}"));
    }
    let (got, why) = drain(&mut tls);
    (got, SIZE, why)
}

/// Rounds: PONGS times ping -> pong of SMALL bytes, then a big answer BIG. Only what was received counts.
fn chat() -> (u64, u64, String) {
    let want = (ROUNDS * (PONGS * SMALL + BIG)) as u64;
    let mut tls = match connect(8443) {
        Ok(t) => t,
        Err(e) => return (0, want, e),
    };
    if let Err(e) = tls.write_all(b"CHAT\n") {
        return (0, want, format!("запрос: {e}"));
    }
    let ping = vec![b'p'; SMALL];
    let mut got = 0u64;
    let mut buf = vec![0u8; BIG];
    for round in 0..ROUNDS {
        for _ in 0..PONGS {
            if let Err(e) = tls.write_all(&ping) {
                return (got, want, format!("круг {round}: запись: {e}"));
            }
            if let Err(e) = tls.read_exact(&mut buf[..SMALL]) {
                return (got, want, format!("круг {round}, мелкие: {}", if stalled(&e) { "ЗАВИС".to_string() } else { e.to_string() }));
            }
            got += SMALL as u64;
        }
        if let Err(e) = tls.write_all(b"B") {
            return (got, want, format!("круг {round}: запись: {e}"));
        }
        if let Err(e) = tls.read_exact(&mut buf[..BIG]) {
            return (got, want, format!("круг {round}, большой ответ: {}", if stalled(&e) { "ЗАВИС".to_string() } else { e.to_string() }));
        }
        got += BIG as u64;
    }
    (got, want, "конец".into())
}

fn client() {
    println!("dbnettest 2: {HOST}. Выключите WARP и VPN.\n");
    let tests: [(&str, fn() -> (u64, u64, String)); 3] =
        [("база: файл через nginx, порт 443 ", nginx), ("F тест-сервер: сплошной файл     ", bulk), ("G тест-сервер: обмен как у лобби  ", chat)];
    for (title, f) in tests {
        let start = Instant::now();
        let (got, want, why) = f();
        let verdict = if got >= want { "OK   " } else { "ОБРЫВ" };
        println!("{verdict} {title}: {got} из {want} байт за {:.1} с, {why}", start.elapsed().as_secs_f64());
    }
    println!("\nПришлите этот текст целиком. Enter — закрыть.");
    let _ = std::io::stdin().read(&mut [0u8; 1]);
}

fn handle(sock: TcpStream, cfg: Arc<ServerConfig>) -> std::io::Result<()> {
    sock.set_nodelay(true)?;
    sock.set_read_timeout(Some(Duration::from_secs(60)))?;
    let conn = ServerConnection::new(cfg).map_err(std::io::Error::other)?;
    let mut tls = StreamOwned::new(conn, sock);
    let mut cmd = [0u8; 5];
    tls.read_exact(&mut cmd)?;
    if &cmd == b"FILE\n" {
        let chunk = vec![b'f'; 16384];
        let mut left = SIZE as usize;
        while left > 0 {
            let n = left.min(chunk.len());
            tls.write_all(&chunk[..n])?;
            left -= n;
        }
    } else if &cmd == b"CHAT\n" {
        let pong = vec![b'q'; SMALL];
        let big = vec![b'b'; BIG];
        let mut ping = vec![0u8; SMALL];
        for _ in 0..ROUNDS {
            for _ in 0..PONGS {
                tls.read_exact(&mut ping)?;
                tls.write_all(&pong)?;
            }
            let mut one = [0u8; 1];
            tls.read_exact(&mut one)?;
            tls.write_all(&big)?;
        }
    }
    tls.conn.send_close_notify();
    let _ = tls.flush();
    Ok(())
}

fn serve(crt: &str, key: &str, port: u16) {
    let certs: Vec<CertificateDer<'static>> =
        rustls_pemfile::certs(&mut BufReader::new(std::fs::File::open(crt).expect("crt"))).collect::<Result<_, _>>().expect("crt pem");
    let key = rustls_pemfile::private_key(&mut BufReader::new(std::fs::File::open(key).expect("key"))).expect("key pem").expect("no key");
    let cfg = ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .expect("tls")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("cert");
    let cfg = Arc::new(cfg);
    let l = TcpListener::bind(("0.0.0.0", port)).expect("bind");
    println!("dbnettest: сервер на порту {port}");
    for s in l.incoming().flatten() {
        let peer = s.peer_addr().map(|a| a.to_string()).unwrap_or_default();
        let cfg = cfg.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            let r = handle(s, cfg);
            println!("{peer}: {:.1} с, {r:?}", start.elapsed().as_secs_f64());
        });
    }
    let _ = BufReader::new(std::io::stdin()).read_line(&mut String::new());
}

// ---------------------------------------------------------------------------------------------- UDP

const UDP_HOST: &str = "203.0.113.20"; // Robot, the game servers' address
const UDP_PORTS: [u16; 7] = [7807, 443, 3478, 27015, 9000, 5000, 53];
const MAGIC: &[u8; 8] = b"DBNETUDP";
const UDP_SECS: u64 = 15;

/// Answers only packets with MAGIC and at most twice the request size: [MAGIC][seq u32][answer size u32]. The answer also carries the received count.
fn udp_port(port: u16, until: Instant) {
    let sock = match std::net::UdpSocket::bind(("0.0.0.0", port)) {
        Ok(s) => s,
        Err(e) => {
            println!("порт {port}: занят: {e}");
            return;
        }
    };
    let _ = sock.set_read_timeout(Some(Duration::from_secs(1)));
    println!("порт {port}: слушаю");
    let mut buf = [0u8; 1500];
    let mut got: std::collections::HashMap<std::net::SocketAddr, u32> = std::collections::HashMap::new();
    while Instant::now() < until {
        let Ok((n, from)) = sock.recv_from(&mut buf) else { continue };
        if n < 16 || &buf[..8] != MAGIC {
            continue;
        }
        let c = got.entry(from).or_insert(0);
        *c += 1;
        let want = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]) as usize;
        let size = want.clamp(20, 1200).min(n * 2);
        let mut out = vec![0u8; size];
        out[..12].copy_from_slice(&buf[..12]);
        out[12..16].copy_from_slice(&c.to_le_bytes());
        let _ = sock.send_to(&out, from);
    }
}

fn udp_serve(secs: u64) {
    let until = Instant::now() + Duration::from_secs(secs);
    let threads: Vec<_> = UDP_PORTS.iter().map(|&p| std::thread::spawn(move || udp_port(p, until))).collect();
    for t in threads {
        let _ = t.join();
    }
    println!("время вышло");
}

/// 30 packets per second: request 620 bytes, the answer is either small or full-size (like the game server's stream to the client)
fn udp_test(port: u16) -> String {
    let sock = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(e) => return format!("сокет: {e}"),
    };
    if let Err(e) = sock.connect((UDP_HOST, port)) {
        return format!("подключение: {e}");
    }
    let _ = sock.set_read_timeout(Some(Duration::from_millis(5)));
    let start = Instant::now();
    let total = Duration::from_secs(UDP_SECS);
    let (mut sent, mut recv, mut big, mut server_got) = (0u32, 0u32, 0u32, 0u32);
    let mut first: Option<f64> = None;
    let mut last = 0f64;
    let mut buf = [0u8; 1500];
    let mut next = Instant::now();
    while start.elapsed() < total {
        if Instant::now() >= next {
            next += Duration::from_millis(33);
            let want: u32 = if sent % 4 == 3 { 1200 } else { 200 };
            let mut out = vec![0u8; 620];
            out[..8].copy_from_slice(MAGIC);
            out[8..12].copy_from_slice(&sent.to_le_bytes());
            out[12..16].copy_from_slice(&want.to_le_bytes());
            if sock.send(&out).is_ok() {
                sent += 1;
            }
        }
        if let Ok(n) = sock.recv(&mut buf) {
            if n >= 16 && &buf[..8] == MAGIC {
                recv += 1;
                if n > 600 {
                    big += 1;
                }
                server_got = server_got.max(u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]));
                let t = start.elapsed().as_secs_f64();
                first.get_or_insert(t);
                last = t;
            }
        }
    }
    let stop = if recv == 0 {
        "ответов нет".to_string()
    } else if last < UDP_SECS as f64 - 2.0 {
        format!("ОТВЕТЫ ПРЕКРАТИЛИСЬ на {last:.1} с")
    } else {
        "до конца".to_string()
    };
    format!("отправлено {sent}, сервер принял {server_got}, ответов {recv} (больших {big}), {stop}")
}

fn udp_client() {
    println!("dbnettest 3: UDP до {UDP_HOST}, примерно {} минуты. Выключите WARP и VPN.\n", UDP_PORTS.len() as u64 * UDP_SECS / 60 + 1);
    for port in UDP_PORTS {
        println!("порт {port:>5}: {}", udp_test(port));
    }
    println!("\nПришлите этот текст целиком. Enter — закрыть.");
    let _ = std::io::stdin().read(&mut [0u8; 1]);
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    if a.len() == 5 && a[1] == "serve" {
        serve(&a[2], &a[3], a[4].parse().expect("port"));
    } else if a.len() == 3 && a[1] == "udpserve" {
        udp_serve(a[2].parse().expect("seconds"));
    } else if a.len() == 2 && a[1] == "tls" {
        client();
    } else {
        udp_client();
    }
}
