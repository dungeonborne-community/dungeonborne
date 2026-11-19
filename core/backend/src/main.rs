#![recursion_limit = "256"]
mod admin_api;
mod auth;
mod auth_http;
mod bug_reports;
mod config;
mod db;
mod dir;
mod ds;
mod game_data;
mod gateway;
mod guid;
mod lobby;
mod rpc;
mod season;
mod server_toml;
mod starter_loadout;
mod text;
mod tgcp;
pub mod structures;

pub use crate::rpc::handlers;

include!("generated/_includes.rs");

use crate::auth_http::run_auth_http_server;
use crate::config::ServerConfig;
use crate::db::GameDb;
use crate::dir::run_dir_server;
use crate::game_data::GameData;
use crate::gateway::run_gateway_port;
use prost::Message;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use std::sync::{Arc, Mutex};
use std::fs::OpenOptions;
use std::io::Write;

// Macro for printing to the console and writing to a file at the same time
macro_rules! log_print {
    ($($arg:tt)*) => {{
        let msg = format!($($arg)*);
        println!("{}", msg);
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open("sniffer_log.txt") {
            let _ = writeln!(file, "{}", msg);
        }
    }};
}

async fn run_dsa_probe_port(port: u16) -> std::io::Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).await?;
    println!("[DSA-Probe] Listening for BattleDsModule on {}", addr);

    while let Ok((mut socket, peer)) = listener.accept().await {
        tokio::spawn(async move {
            println!("[DSA-Probe] ===> New connection from DS: {}", peer);
            let mut buf = [0u8; 4096];
            while let Ok(n) = socket.read(&mut buf).await {
                if n == 0 {
                    break;
                }
                let data = &buf[..n];
                let ascii: String = data
                    .iter()
                    .map(|&b| if (0x20..=0x7E).contains(&b) { b as char } else { '.' })
                    .collect();
                println!(
                    "[DSA-Probe] Received {} bytes from {}:\n  HEX: {:02X?}\n  STR: {}",
                    n,
                    peer,
                    &data[..n.min(64)],
                    ascii
                );
            }
            println!("[DSA-Probe] DS {} disconnected", peer);
        });
    }
    Ok(())
}

// =====================================================================
// MitM PROXY (traffic sniffer)
// =====================================================================

async fn run_mitm_proxy(local_port: u16, real_ip: String) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt}; // Import the needed traits

    let addr = format!("0.0.0.0:{}", local_port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    log_print!("[MitM] Listening on port {} -> Forwarding to {}:{}", local_port, real_ip, local_port);

    while let Ok((mut client_stream, peer)) = listener.accept().await {
        log_print!("[MitM] New connection from client: {}", peer);
        let real_addr = format!("{}:{}", real_ip, local_port);
        
        tokio::spawn(async move {
            let mut server_stream = match tokio::net::TcpStream::connect(&real_addr).await {
                Ok(s) => s,
                Err(e) => {
                    log_print!("[MitM] Error connecting to {}: {}", real_addr, e);
                    return;
                }
            };

            // Call split directly as a method
            let (mut cr, mut cw) = client_stream.split();
            let (mut sr, mut sw) = server_stream.split();

            let mut c2s_buf = bytes::BytesMut::new();
            let mut s2c_buf = bytes::BytesMut::new();

            let session_key = Arc::new(Mutex::new(crate::tgcp::crypto::SERVER_SESSION_KEY));
            let key_c2s = session_key.clone();
            let key_s2c = session_key.clone();

            let c2s = async {
                let mut buf = [0u8; 4096];
                // Call read and write_all directly
                while let Ok(n) = cr.read(&mut buf).await {
                    if n == 0 { break; }
                    if sw.write_all(&buf[..n]).await.is_err() { break; }
                    c2s_buf.extend_from_slice(&buf[..n]);
                    parse_and_dump_traffic(&mut c2s_buf, "C2S", key_c2s.clone());
                }
            };

            let s2c = async {
                let mut buf = [0u8; 4096];
                while let Ok(n) = sr.read(&mut buf).await {
                    if n == 0 { break; }
                    if cw.write_all(&buf[..n]).await.is_err() { break; }
                    s2c_buf.extend_from_slice(&buf[..n]);
                    parse_and_dump_traffic(&mut s2c_buf, "S2C", key_s2c.clone());
                }
            };

            tokio::join!(c2s, s2c);
            log_print!("[MitM] Connection {} closed", peer);
        });
    }
    Ok(())
}

fn parse_and_dump_traffic(buf: &mut bytes::BytesMut, direction: &str, session_key: Arc<Mutex<[u8; 16]>>) {
    use bytes::Buf;
    loop {
        if buf.len() < 2 { break; }
        
        let mut found_magic = false;
        for i in 0..buf.len() - 1 {
            if buf[i] == 0x33 && buf[i+1] == 0x66 {
                if i > 0 { buf.advance(i); }
                found_magic = true;
                break;
            }
        }
        
        if !found_magic {
            buf.clear();
            break;
        }

        match crate::tgcp::try_read_frame(buf) {
            Ok(Some(frame)) => {
                if frame.cmd == 0x1002 && direction == "S2C" && frame.raw_frame.len() >= 39 {
                    let mut key = session_key.lock().unwrap();
                    key.copy_from_slice(&frame.raw_frame[23..39]);
                    log_print!("[MitM] 🔑 NEW SESSION KEY INTERCEPTED: {:02X?}", *key);
                }

                if frame.cmd == 0x4013 && frame.raw_frame.len() > 25 {
                    let flag = u32::from_be_bytes([
                        frame.raw_frame[21], frame.raw_frame[22], 
                        frame.raw_frame[23], frame.raw_frame[24]
                    ]);
                    
                    let payload = &frame.raw_frame[25..];
                    let current_key = *session_key.lock().unwrap();
                    
                    let decrypted_result = if flag == 0 {
                        Ok(payload.to_vec())
                    } else {
                        crate::tgcp::crypto::taes_decrypt_c2s(payload, &current_key)
                    };

                    match decrypted_result {
                        Ok(decrypted) => {
                            match crate::rpc::RpcFrame::decode(&decrypted[..]) {
                                Ok(rpc) => {
                                    if let Some(head) = rpc.head {
                                        let mut method = String::new();
                                        let mut type_url = String::new();

                                        if let Some(r) = &head.c2s_routing {
                                            if !r.method.is_empty() { method = r.method.clone(); }
                                            if !r.type_url.is_empty() { type_url = r.type_url.clone(); }
                                        }
                                        if let Some(r) = &head.s2c_routing {
                                            if !r.method.is_empty() && method.is_empty() { method = r.method.clone(); }
                                            if !r.type_url.is_empty() && type_url.is_empty() { type_url = r.type_url.clone(); }
                                        }

                                        let display_name = if method.is_empty() {
                                            if type_url.is_empty() {
                                                format!("[PUSH] UNKNOWN_TYPE_{}", head.sequence_ack)
                                            } else {
                                                format!("[PUSH] {}", type_url)
                                            }
                                        } else {
                                            method
                                        };
                                        
                                        log_print!("[{}] {}", direction, display_name);
                                        
                                        // NOW PRINT THE BYTES FOR ALL PACKETS
                                        if !rpc.body.is_empty() {
                                            log_print!("  => BYTES: {:?}", &rpc.body);
                                        }
                                    }
                                }
                                Err(e) => log_print!("  -> Protobuf parse error: {}", e),
                            }
                        }
                        Err(_) => log_print!("  -> [Encryption] Decryption error (flag: {})", flag),
                    }
                }
            }
            Ok(None) => break,
            Err(e) => {
                log_print!("[{}] TGCP parse error: {}", direction, e);
                buf.advance(2); 
            }
        }
    }
}
// =====================================================================
// RPC TRACE: which method is called, what comes in/goes out (our backend and PVE SoloLobby)
//   - any body is decoded via Network.pb (prost-reflect) into JSON;
//   - console: 1 line per packet; full JSON + raw .bin -> rpc_dump/<TAG>/
// =====================================================================

static RPC_POOL: std::sync::OnceLock<Option<prost_reflect::DescriptorPool>> = std::sync::OnceLock::new();
static RPC_DUMP_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn rpc_pool() -> Option<&'static prost_reflect::DescriptorPool> {
    RPC_POOL
        .get_or_init(|| {
            let path = std::env::var("NETWORK_PB").unwrap_or_else(|_| "Network.pb".to_string());
            match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| {
                prost_reflect::DescriptorPool::decode(b.as_slice()).map_err(|e| e.to_string())
            }) {
                Ok(p) => {
                    println!("[RpcTrace] {} loaded: {} message types", path, p.all_messages().count());
                    Some(p)
                }
                Err(e) => {
                    eprintln!("[RpcTrace] failed to load {}: {} (JSON decoding disabled)", path, e);
                    None
                }
            }
        })
        .as_ref()
}

/// Whether the dump is enabled for our gateway (env RPC_DUMP=1). For the PVE sniffer the dump is always on.
pub fn rpc_dump_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RPC_DUMP").is_some())
}

/// Body type: first by type_url, otherwise by the service method (C2S -> input, S2C -> output).
fn rpc_resolve_type(
    pool: &prost_reflect::DescriptorPool,
    dir: &str,
    full_method: &str,
    type_url: &str,
) -> Option<prost_reflect::MessageDescriptor> {
    if !type_url.is_empty() {
        let name = type_url.rsplit('/').next().unwrap_or(type_url);
        if let Some(d) = pool.get_message_by_name(name) {
            return Some(d);
        }
    }
    // PVE SoloLobby sends player_dirty_chg_sync without routing (empty method/type_url)
    if full_method.is_empty() && dir == "S2C" {
        return pool.get_message_by_name("tgf.SCPlayerDirtyChgSync");
    }
    let (service, method) = full_method.rsplit_once('.')?;
    let pick = |s: prost_reflect::ServiceDescriptor| {
        s.methods()
            .find(|m| m.name() == method)
            .map(|m| if dir == "C2S" { m.input() } else { m.output() })
    };
    if let Some(d) = pool.get_service_by_name(service).and_then(pick) {
        return Some(d);
    }
    pool.services().find_map(pick)
}

/// Logs one RPC packet and writes rpc_dump/<tag>/<seq>_<dir>_<method>.json (+ .bin).
pub fn dump_rpc(tag: &str, dir: &str, full_method: &str, type_url: &str, body: &[u8]) {
    dump_rpc_who(tag, "", dir, full_method, type_url, body);
}

/// Like dump_rpc, plus `who` (session open_id) and time: the .json gets ts_ms/who fields, and a line goes to rpc_dump/<tag>.jsonl
/// (everything in a row across all runs — for merging with the client's ClientProbe logs, tools/probe_timeline.py).
pub fn dump_rpc_who(tag: &str, who: &str, dir: &str, full_method: &str, type_url: &str, body: &[u8]) {
    static JSONL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let ts_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let seq = RPC_DUMP_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
    let short = full_method.rsplit('.').next().unwrap_or(full_method);
    let short = if short.is_empty() { "push" } else { short };

    let mut type_name = String::from("?");
    let mut json = serde_json::Value::Null;
    if let Some(pool) = rpc_pool() {
        if let Some(desc) = rpc_resolve_type(pool, dir, full_method, type_url) {
            type_name = desc.full_name().to_string();
            match prost_reflect::DynamicMessage::decode(desc, body) {
                Ok(msg) => {
                    let opts = prost_reflect::SerializeOptions::new()
                        .use_proto_field_name(true)
                        .stringify_64_bit_integers(false);
                    json = msg
                        .serialize_with_options(serde_json::value::Serializer, &opts)
                        .unwrap_or(serde_json::Value::Null);
                }
                Err(e) => type_name = format!("{} (decode err: {})", type_name, e),
            }
        }
    }

    let out_dir = std::path::Path::new("rpc_dump").join(tag);
    let _ = std::fs::create_dir_all(&out_dir);
    let safe: String = short.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect();
    let base = out_dir.join(format!("{:05}_{}_{}", seq, dir, safe));
    let _ = std::fs::write(base.with_extension("bin"), body);
    let doc = serde_json::json!({
        "seq": seq, "ts_ms": ts_ms, "who": who, "dir": dir, "method": full_method, "type_url": type_url,
        "type": type_name, "bytes": body.len(), "body": json,
    });
    let _ = std::fs::write(base.with_extension("json"), serde_json::to_string_pretty(&doc).unwrap_or_default());
    {
        use std::io::Write;
        let _guard = JSONL_LOCK.lock();
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("rpc_dump/{}.jsonl", tag)) {
            let _ = writeln!(f, "{}", doc);
        }
    }

    // Affix summary if the packet contains items
    let mut affix_summary = String::new();
    if !body.is_empty() {
        let txt = json.to_string();
        let n = |pat: &str| txt.matches(pat).count();
        let (b, ma, mi, l) = (
            n("EN_BATTLE_AFFIX_TYPE_BASIC"), n("EN_BATTLE_AFFIX_TYPE_MAJOR"),
            n("EN_BATTLE_AFFIX_TYPE_MINOR"), n("EN_BATTLE_AFFIX_TYPE_LEGENDARY"),
        );
        if b + ma + mi + l > 0 {
            affix_summary = format!(" | affix B={} MAJ={} MIN={} LEG={}", b, ma, mi, l);
        }
    }
    println!("[{}] {} {:05} {} <{}> {}B{}", tag, dir, seq, short, type_name, body.len(), affix_summary);
}

/// The whole RPC frame header — by decoding the whole frame as `atframework.CSMsg` (Network.pb), without our RpcHead,
/// to see all fields as the client understands them (error_code=1 sint32, client/server/session_sequence 3/4/5, op_type 6,
/// sequence_ack 7, rpc_request/rpc_response 31/32). Line in rpc_dump/<tag>_head.jsonl: {ts_ms, who, dir, head}.
pub fn dump_rpc_head(tag: &str, who: &str, dir: &str, frame: &[u8]) {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let Some(desc) = rpc_pool().and_then(|p| p.get_message_by_name("atframework.CSMsg")) else { return };
    let head = match prost_reflect::DynamicMessage::decode(desc, frame) {
        Ok(msg) => {
            let opts = prost_reflect::SerializeOptions::new().use_proto_field_name(true).stringify_64_bit_integers(false);
            msg.serialize_with_options(serde_json::value::Serializer, &opts)
                .map(|mut v| v["head"].take())
                .unwrap_or(serde_json::Value::Null)
        }
        Err(e) => serde_json::json!({ "decode_err": e.to_string() }),
    };
    let ts_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let doc = serde_json::json!({ "ts_ms": ts_ms, "who": who, "dir": dir, "head": head });
    use std::io::Write;
    let _guard = LOCK.lock();
    let _ = std::fs::create_dir_all("rpc_dump");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("rpc_dump/{}_head.jsonl", tag)) {
        let _ = writeln!(f, "{}", doc);
    }
}

/// TGCP frame (transport under RPC): cmd, length, first 25 header bytes; for service frames (not 0x4013, ≤ 512 bytes) —
/// the whole frame. For comparing the transport with PVE (handshake 0x1001/0x1002/0x2001/0x2002, pings 0x6001/0x6002, sequence numbers at
/// [9..13], u32 BE). Line in rpc_dump/<tag>_tgcp.jsonl. Enabled together with RPC_DUMP.
pub fn dump_tgcp(tag: &str, who: &str, dir: &str, raw: &[u8]) {
    if !rpc_dump_enabled() {
        return;
    }
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let hex = |b: &[u8]| b.iter().map(|x| format!("{:02x}", x)).collect::<String>();
    let cmd = if raw.len() >= 8 { u16::from_be_bytes([raw[6], raw[7]]) } else { 0 };
    let ts_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let mut doc = serde_json::json!({
        "ts_ms": ts_ms, "who": who, "dir": dir, "cmd": format!("0x{:04x}", cmd), "len": raw.len(),
        "hdr": hex(&raw[..raw.len().min(25)]),
    });
    if cmd != 0x4013 && raw.len() <= 512 {
        doc["raw"] = serde_json::json!(hex(raw));
    }
    use std::io::Write;
    let _guard = LOCK.lock();
    let _ = std::fs::create_dir_all("rpc_dump");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("rpc_dump/{}_tgcp.jsonl", tag)) {
        let _ = writeln!(f, "{}", doc);
    }
}

/// PVE sniffer: game -> <listen_ip>:<listen_port> -> <upstream_host>:<upstream_port> (SoloLobby.exe).
/// The session key is intercepted from the upstream's 0x1002 (BinBingo) reply.
pub async fn run_pve_sniffer(listen_ip: String, listen_port: u16, upstream_host: String, upstream_port: u16) -> std::io::Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind((listen_ip.as_str(), listen_port)).await?;
    println!("[PVE-Sniff] {}:{} -> {}:{}", listen_ip, listen_port, upstream_host, upstream_port);

    while let Ok((mut client, peer)) = listener.accept().await {
        let upstream = format!("{}:{}", upstream_host, upstream_port);
        tokio::spawn(async move {
            let mut server = match tokio::net::TcpStream::connect(&upstream).await {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[PVE-Sniff] {} -> {}: {}", peer, upstream, e);
                    return;
                }
            };
            println!("[PVE-Sniff] {} connected to {}", peer, upstream);
            let key = std::sync::Arc::new(std::sync::Mutex::new(crate::tgcp::crypto::SERVER_SESSION_KEY));
            let (mut cr, mut cw) = client.split();
            let (mut sr, mut sw) = server.split();

            let k1 = key.clone();
            let c2s = async move {
                let mut acc = bytes::BytesMut::new();
                let mut buf = [0u8; 16384];
                while let Ok(n) = cr.read(&mut buf).await {
                    if n == 0 || sw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    acc.extend_from_slice(&buf[..n]);
                    sniff_tgcp_frames(&mut acc, "C2S", &k1);
                }
            };
            let k2 = key.clone();
            let s2c = async move {
                let mut acc = bytes::BytesMut::new();
                let mut buf = [0u8; 16384];
                while let Ok(n) = sr.read(&mut buf).await {
                    if n == 0 || cw.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    acc.extend_from_slice(&buf[..n]);
                    sniff_tgcp_frames(&mut acc, "S2C", &k2);
                }
            };
            tokio::join!(c2s, s2c);
            println!("[PVE-Sniff] {} closed", peer);
        });
    }
    Ok(())
}

fn sniff_tgcp_frames(buf: &mut bytes::BytesMut, dir: &str, key: &std::sync::Mutex<[u8; 16]>) {
    use bytes::Buf;
    loop {
        // resync on the 0x3366 magic
        match buf.windows(2).position(|w| w == [0x33, 0x66]) {
            Some(0) => {}
            Some(i) => buf.advance(i),
            None => {
                let drop = buf.len().saturating_sub(1);
                buf.advance(drop);
                return;
            }
        }
        let frame = match crate::tgcp::try_read_frame(buf) {
            Ok(Some(f)) => f,
            Ok(None) => return,
            Err(e) => {
                eprintln!("[PVE-Sniff] {} TGCP: {}", dir, e);
                dump_tgcp("PVE", "", &format!("{}-unparsed", dir), &buf[..buf.len().min(64)]);
                buf.advance(2);
                continue;
            }
        };
        let raw = &frame.raw_frame;
        dump_tgcp("PVE", "", dir, raw);
        if frame.cmd == 0x1002 && dir == "S2C" && raw.len() >= 39 {
            let mut k = key.lock().unwrap();
            k.copy_from_slice(&raw[23..39]);
            println!("[PVE-Sniff] session key from 0x1002: {:02X?}", *k);
            continue;
        }
        if frame.cmd != 0x4013 || raw.len() < 25 {
            continue;
        }
        let flag = u32::from_be_bytes([raw[21], raw[22], raw[23], raw[24]]);
        let payload = if flag == 0 {
            raw[25..].to_vec()
        } else {
            let k = *key.lock().unwrap();
            match crate::tgcp::crypto::taes_decrypt_c2s(&raw[25..], &k) {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[PVE-Sniff] {} decrypt (flag 0x{:x}): {}", dir, flag, e);
                    continue;
                }
            }
        };
        dump_rpc_head("PVE", "", dir, &payload);
        let rpc = match <crate::rpc::RpcFrame as prost::Message>::decode(payload.as_slice()) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[PVE-Sniff] {} RpcFrame: {}", dir, e);
                continue;
            }
        };
        let head = rpc.head.unwrap_or_default();
        let route = if dir == "C2S" { head.c2s_routing.or(head.s2c_routing) } else { head.s2c_routing.or(head.stream_routing) };
        let route = route.unwrap_or_default();
        dump_rpc("PVE", dir, &route.method, &route.type_url, &rpc.body);
    }
}
// =====================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // If the MITM_REAL_IP variable is set, run the server in sniffer mode
    // If the MITM_REAL_IP variable is set, run the server in sniffer mode
    if let Ok(real_ip) = std::env::var("MITM_REAL_IP") {
        println!("===================================================");
        println!(" Starting in MitM TRAFFIC SNIFFER mode");
        println!(" Forwarding game ports to: {}", real_ip);
        println!("===================================================");
        
        // Load the config for the local servers
        let cfg = ServerConfig::from_env();

        tokio::try_join!(
            // IMPORTANT: keep the Auth and Dir servers LOCAL!
            // They make the game connect to our proxy on 127.0.0.1:6601
            run_auth_http_server(cfg.clone()),
            run_dir_server(cfg.clone()),

            // But the game ports (6601-6604) are proxied to China and we listen to the traffic
            run_mitm_proxy(6601, real_ip.clone()),
            run_mitm_proxy(6602, real_ip.clone()),
            run_mitm_proxy(6603, real_ip.clone()),
            run_mitm_proxy(6604, real_ip),
        )?;
        return Ok(());
    }

    // PVE sniffer: PVE_SNIFF=127.0.0.1 (where SoloLobby.exe runs, it hard-binds 127.0.0.1:6601-6604).
    // Do NOT start SoloDir.exe — 8085 is taken by our DirServer, which sends the game to PVE_SNIFF_IP (127.0.0.2) with the same ports.
    if let Ok(upstream) = std::env::var("PVE_SNIFF") {
        let env_port = |k: &str, d: u16| std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d);
        let listen_ip = std::env::var("PVE_SNIFF_IP").unwrap_or_else(|_| "127.0.0.2".to_string());
        let listen = env_port("PVE_SNIFF_LISTEN", 6601);
        let up = env_port("PVE_SNIFF_UPSTREAM", 6601);
        let mut cfg = ServerConfig::from_env();
        cfg.public_gateway_ip = listen_ip.clone();
        cfg.public_gateway_port = listen;
        println!("=== PVE SNIFF: {}:{}-{} -> {}:{}-{}, dump to rpc_dump/PVE ===", listen_ip, listen, listen + 3, upstream, up, up + 3);
        tokio::try_join!(
            run_dir_server(cfg),
            run_pve_sniffer(listen_ip.clone(), listen, upstream.clone(), up),
            run_pve_sniffer(listen_ip.clone(), listen + 1, upstream.clone(), up + 1),
            run_pve_sniffer(listen_ip.clone(), listen + 2, upstream.clone(), up + 2),
            run_pve_sniffer(listen_ip, listen + 3, upstream, up + 3),
        )?;
        return Ok(());
    }

    let cfg = ServerConfig::from_env();

    println!("Loading GameData from ExcelConfig...");
    let game_data = Arc::new(GameData::load_all("ExcelConfig"));
    println!("GameData loaded successfully.");
    // season (SEASON=<id>, seasons/): core flags — into the log right at startup
    season::active();

    let db = GameDb::init(&cfg.db_path).await;
    // item guids < 2^31 (the client stores them in 32 bits): counter + one-time migration of old saves
    guid::init(&db).await;
    guid::migrate(&db).await;
    // 12.11: one-time reset of relic resources, relics and recipes (src/rpc/handlers/relic_reset.rs) — DISABLED.
    // Done 12.11 00:39 UTC; at 16:12 UTC it fired again (serde_json::Value did not read the marker: "invalid type: enum")
    // and took from 28 players what they had collected that day. The call is removed entirely, the marker is now read differently (relic_reset::run).
    if std::env::var_os("GUID_MIGRATE_ONLY").is_some() {
        println!("[Guid] GUID_MIGRATE_ONLY: migration done, exiting");
        return Ok(());
    }
    guid::spawn_keeper(db.clone());
    // starting gear for new characters (legendary tier): schema + missing class sets
    starter_loadout::setup(&db).await;
    starter_loadout::seed(&db, &game_data).await;
    // bug reports from the launcher for the Discord bot (src/bug_reports.rs): indexes + cleanup of old files
    bug_reports::setup(&db).await;
    bug_reports::spawn_cleanup(db.clone());

    println!("=== Dungeonborne Rust + SurrealDB Server Started ===");
    lobby::spawn_background();
    // DS layer (docs/DS_PIPELINE.md): HTTP for ds-agent (DS_AGENT_PORT, 9091) + init_player for match rooms
    ds::spawn(db.clone(), game_data.clone());
    // Player login (src/auth): launcher web API, providers, relay — per server.toml; without the file — as before
    auth::spawn(db.clone(), server_toml::ServerToml::load());
    // admin API for developers (src/admin_api.rs): GM commands and online list, loopback only
    tokio::spawn(admin_api::run(db.clone(), game_data.clone()));
    // system vendors (TRADE_SYSTEM_BUYOUT, TRADE_SYSTEM_AUCTION): a pass every 5 minutes; without the flags one pass removes their lots
    {
        let session = rpc::handlers::GatewaySession {
            db: db.clone(),
            game_data: game_data.clone(),
            open_id: "SYSTEM".to_string(),
            peer_ip: "system-market".to_string(),
            after_pushes: std::sync::Mutex::new(Vec::new()),
            rpc_error: std::sync::atomic::AtomicI32::new(0),
        };
        tokio::spawn(async move {
            loop {
                session.trade_seed_system_lots().await;
                if !rpc::handlers::system_buyout() && !rpc::handlers::system_auction() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            }
        });
    }

    tokio::try_join!(
        run_auth_http_server(cfg.clone()),
        run_dir_server(cfg.clone()),
        run_gateway_port(6601, cfg.clone(), game_data.clone(), db.clone()),
        run_gateway_port(6602, cfg.clone(), game_data.clone(), db.clone()),
        run_gateway_port(6603, cfg.clone(), game_data.clone(), db.clone()),
        run_gateway_port(6604, cfg, game_data, db),
        run_dsa_probe_port(9090),
    )?;

    Ok(())
}