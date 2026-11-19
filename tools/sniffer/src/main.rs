use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};

static PACKET_SEQ: AtomicUsize = AtomicUsize::new(0);

async fn proxy_port(listen_port: u16, target_port: u16, name: &'static str) {
    let listener = TcpListener::bind(("127.0.0.1", listen_port)).await.unwrap();
    println!("[{}] Listening on 127.0.0.1:{} -> forwarding to :{}", name, listen_port, target_port);

    while let Ok((mut client, _)) = listener.accept().await {
        tokio::spawn(async move {
            let mut server = match TcpStream::connect(("127.0.0.1", target_port)).await {
                Ok(s) => s,
                Err(e) => { eprintln!("[{}] Failed to connect to tunnel: {}", name, e); return; }
            };
            let (mut cr, mut cw) = client.split();
            let (mut sr, mut sw) = server.split();

            let c2s = async {
                let mut buf = [0u8; 65536];
                while let Ok(n) = cr.read(&mut buf).await {
                    if n == 0 { break; }
                    log_packet(name, "C2S", &buf[..n]);
                    if sw.write_all(&buf[..n]).await.is_err() { break; }
                }
            };
            let s2c = async {
                let mut buf = [0u8; 65536];
                while let Ok(n) = sr.read(&mut buf).await {
                    if n == 0 { break; }
                    log_packet(name, "S2C", &buf[..n]);
                    if cw.write_all(&buf[..n]).await.is_err() { break; }
                }
            };
            tokio::select! { _ = c2s => {}, _ = s2c => {} }
        });
    }
}

fn log_packet(service: &str, dir: &str, data: &[u8]) {
    let seq = PACKET_SEQ.fetch_add(1, Ordering::SeqCst);
    let filename = format!("dump_{:04}_{}_{}.bin", seq, service, dir);
    if let Ok(mut f) = OpenOptions::new().create(true).write(true).open(&filename) {
        let _ = f.write_all(data);
    }
    println!("#{:04} [{}] {} ({} bytes)", seq, service, dir, data.len());
}

#[tokio::main]
async fn main() {
    tokio::join!(
        proxy_port(8085, 28085, "DIR"),
        proxy_port(6601, 26601, "GATEWAY"),
    );
}