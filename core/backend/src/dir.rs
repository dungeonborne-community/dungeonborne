use crate::config::ServerConfig;
use std::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Gateway addresses in 0x4002: `DIR_URL_MODE` = tcp (default), both (as in the dump: `gtcp://…;tcp://…`), gtcp.
/// L1 02.11 (series dbtest\loginseries.ps1): with a single `gtcp://` the client never logs in (201 ext:-6 after 5 s,
/// does not connect to the gateway), with both — sometimes it tries gtcp first and fails, with a single `tcp://` — 10 out of 10 on the first try.
fn dir_url(mode: &str, ip: &str, port: u16) -> String {
    match mode {
        "gtcp" => format!("gtcp://{ip}:{port}"),
        "both" => format!("gtcp://{ip}:{port};tcp://{ip}:{port}"),
        _ => format!("tcp://{ip}:{port}"),
    }
}

fn put_be(pkt: &mut [u8], at: usize, v: isize) {
    pkt[at..at + 4].copy_from_slice(&(v as u32).to_be_bytes());
}

fn put_varint2(pkt: &mut [u8], at: usize, v: isize) -> bool {
    if !(128..16384).contains(&v) {
        return false; // must remain a 2-byte varint
    }
    pkt[at] = ((v & 0x7F) as u8) | 0x80;
    pkt[at + 1] = (v >> 7) as u8;
    true
}

/// Packet 0x4002 (dump_0003, 281 bytes) with the gateway address substituted.
/// Layout (offsets from the start of the packet):
///   [6..10]   BE header length = 29
///   [10..14]  BE body length = 252
///   [62] 2A, [63..65] varint 216        (protobuf field 5)
///   [67] 12, [68..70] varint 211        (protobuf field 2)
///   [172] AA, [173..177] BE 109         (node 3, length including its own header)
///   [179] 25, [180..184] BE 97          (contents of node 3)
///   [184] 15, [185..189] BE 92          (server record)
///   [233..235] 85 01, [235..239] BE 42  (URL string), then the URL itself to the end
fn build_dynamic_dir_rsp_0003(t: &[u8], public_ip: &str, port: u16, mode: &str) -> Vec<u8> {
    let ok = t.len() == 281
        && t[10..14] == [0, 0, 0, 0xFC]
        && t[62] == 0x2A && t[67] == 0x12
        && t[172] == 0xAA && t[179] == 0x25 && t[184] == 0x15
        && t[233] == 0x85 && t[234] == 0x01
        && t[235..239] == [0, 0, 0, 0x2A];
    if !ok {
        eprintln!("[DirServer] WARNING: dump_0003 has the wrong format, serving as is");
        return t.to_vec();
    }

    let url = dir_url(mode, public_ip, port);
    let delta = url.len() as isize - 42;

    let mut pkt = t[..239].to_vec();
    pkt.extend_from_slice(url.as_bytes());

    put_be(&mut pkt, 10, 252 + delta);
    if !put_varint2(&mut pkt, 63, 216 + delta) || !put_varint2(&mut pkt, 68, 211 + delta) {
        eprintln!("[DirServer] address too long/short for the template, serving as is");
        return t.to_vec();
    }
    put_be(&mut pkt, 173, 109 + delta);
    put_be(&mut pkt, 180, 97 + delta);
    put_be(&mut pkt, 185, 92 + delta);
    put_be(&mut pkt, 235, url.len() as isize);
    pkt
}

/// A reference packet: `ASSETS_DIR/<name>`, else `ASSETS_DIR/assets/packets/<name>` — where the workspace keeps it
/// (18.11: with ASSETS_DIR = core/backend the stand found nothing and the backend stopped at start).
fn packet_path(assets_dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let direct = assets_dir.join(name);
    if direct.is_file() {
        direct
    } else {
        assets_dir.join("assets").join("packets").join(name)
    }
}

pub async fn run_dir_server(cfg: ServerConfig) -> std::io::Result<()> {
    let dir_rsp_1 = fs::read(packet_path(&cfg.assets_dir, "dump_0001_DIR_S2C.bin"))
        .expect("dump_0001_DIR_S2C.bin not found (ASSETS_DIR or ASSETS_DIR/assets/packets)");
    let template_3 = fs::read(packet_path(&cfg.assets_dir, "dump_0003_DIR_S2C.bin"))
        .expect("dump_0003_DIR_S2C.bin not found (ASSETS_DIR or ASSETS_DIR/assets/packets)");

    // Self-check: with the original address the build must give the template byte for byte
    if build_dynamic_dir_rsp_0003(&template_3, "127.0.0.1", 6601, "both") == template_3 {
        println!("[DirServer] self-check OK");
    } else {
        eprintln!("[DirServer] self-check FAILED: offsets do not match the template");
    }

    let mode = std::env::var("DIR_URL_MODE").unwrap_or_else(|_| "tcp".into());
    let dir_rsp_3 = build_dynamic_dir_rsp_0003(&template_3, &cfg.public_gateway_ip, cfg.public_gateway_port, &mode);
    // through the launcher relay the lobby also goes through the relay: the launcher's local port on the player's machine
    let dir_rsp_3_relay = build_dynamic_dir_rsp_0003(&template_3, "127.0.0.1", 6601, &mode);
    println!(
        "[DirServer] 0x4002: {} bytes (template {}), url {}",
        dir_rsp_3.len(), template_3.len(), dir_url(&mode, &cfg.public_gateway_ip, cfg.public_gateway_port)
    );

    let bind_addr = format!("{}:{}", cfg.bind_ip, cfg.dir_port);
    let listener = TcpListener::bind(&bind_addr).await?;
    println!(
        "[DirServer] Listening on {} -> routing clients to {}",
        bind_addr, dir_url(&mode, &cfg.public_gateway_ip, cfg.public_gateway_port)
    );

    while let Ok((mut socket, addr)) = listener.accept().await {
        let rsp1 = dir_rsp_1.clone();
        let rsp3 = match crate::auth::registry::admit(&addr) {
            Err(()) => {
                println!("[DirServer] {} not through the launcher relay ([auth] require = relay) — refused", addr);
                continue;
            }
            Ok(Some(_)) => dir_rsp_3_relay.clone(),
            Ok(None) => dir_rsp_3.clone(),
        };
        tokio::spawn(async move {
            let mut buf = [0u8; 1024];
            while let Ok(n) = socket.read(&mut buf).await {
                if n < 6 {
                    break;
                }
                let cmd = u16::from_be_bytes([buf[4], buf[5]]);
                println!("[DirServer] Client {} -> cmd 0x{:04x} ({} bytes)", addr, cmd, n);
                match cmd {
                    0x1001 => { let _ = socket.write_all(&rsp1).await; }
                    0x4001 => { let _ = socket.write_all(&rsp3).await; }
                    0x9001 => break,
                    _ => {}
                }
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod packet_path_tests {
    /// The workspace keeps the DIR packets in core/backend/assets/packets; ASSETS_DIR = core/backend must find them.
    #[test]
    fn finds_the_workspace_packets() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        for n in ["dump_0001_DIR_S2C.bin", "dump_0003_DIR_S2C.bin"] {
            assert!(super::packet_path(dir, n).is_file(), "{n} not found from {}", dir.display());
        }
        let template = std::fs::read(super::packet_path(dir, "dump_0003_DIR_S2C.bin")).unwrap();
        assert_eq!(super::build_dynamic_dir_rsp_0003(&template, "127.0.0.1", 6601, "both"), template);
    }
}
