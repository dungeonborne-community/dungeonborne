use crate::config::ServerConfig;
use crate::db::GameDb;
use crate::game_data::GameData;
use crate::handlers::GatewaySession;
use crate::rpc::{build_rpc_frame, RpcFrame};
use crate::tgcp::crypto::{taes_decrypt_c2s, SERVER_SESSION_KEY};
use crate::tgcp::{encode_tgcp_s2c, try_read_frame, BIN_BINGO_0006};
use bytes::BytesMut;
use prost::Message;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub async fn run_gateway_port(
    port: u16,
    cfg: ServerConfig,
    game_data: Arc<GameData>,
    db: GameDb,
) -> std::io::Result<()> {

    let bind_addr = format!("{}:{}", cfg.bind_ip, port);
    let listener = TcpListener::bind(&bind_addr).await?;
    println!("[Gateway] Listening on {}", bind_addr);

    while let Ok((mut socket, addr)) = listener.accept().await {
        let game_data = game_data.clone();
        let db = db.clone();
        let peer_ip = addr.ip().to_string();

        tokio::spawn(async move {
            println!("[Gateway:{}] New connection from {}", port, addr);
            let mut buf = BytesMut::with_capacity(65536);
            let mut tgcp_s2c_seq: u32 = 3;
            let mut rpc_s2c_seq: u64 = 0;
            let mut open_id = String::from("76561198000000901");
            // Pushes from other sessions (party, found match) — via the lobby::hub registry
            let (push_tx, mut push_rx) = tokio::sync::mpsc::unbounded_channel::<crate::lobby::hub::Push>();
            let mut hub_conn: Option<u64> = None;

            'conn: loop {
                tokio::select! {
                    read = socket.read_buf(&mut buf) => match read {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    },
                    Some(p) = push_rx.recv() => {
                        if crate::rpc_dump_enabled() {
                            crate::dump_rpc_who("OUR", &open_id, "S2C", &p.method, &p.type_url, &p.body);
                        }
                        rpc_s2c_seq += 1;
                        tgcp_s2c_seq += 1;
                        let p_frame = build_rpc_frame(rpc_s2c_seq, 0, &p.method, &p.type_url, p.body);
                        if crate::rpc_dump_enabled() {
                            crate::dump_rpc_head("OUR", &open_id, "S2C", &p_frame);
                        }
                        let out = encode_tgcp_s2c(tgcp_s2c_seq, &p_frame);
                        crate::dump_tgcp("OUR", &open_id, "S2C", &out);
                        let _ = socket.write_all(&out).await;
                        println!("[RPC] => hub push {} ({})", p.method, p.type_url);
                        continue;
                    }
                }

                loop {
                    let frame_opt = match try_read_frame(&mut buf) {
                        Ok(Some(f)) => f,
                        Ok(None) => break,
                        Err(e) => {
                            eprintln!("[Gateway:{}] Frame error: {}", port, e);
                            buf.clear();
                            break;
                        }
                    };

                    let frame = frame_opt.raw_frame;
                    crate::dump_tgcp("OUR", &open_id, "C2S", &frame);
                    match frame_opt.cmd {
                        0x1001 => {
                                if frame.len() >= 55 {
                                    let end = frame[37..].iter().position(|&b| b == 0).unwrap_or(17);
                                    if let Ok(s) = std::str::from_utf8(&frame[37..(37 + end)]) {
                                        let cleaned: String = s.chars().filter(|c| c.is_alphanumeric()).collect();
                                        if !cleaned.is_empty() {
                                            open_id = cleaned;
                                        }
                                    }
                                }
                                // Identity through the launcher relay takes precedence over the SteamID the client claimed (src/auth/registry.rs)
                                match crate::auth::registry::admit(&addr) {
                                    Err(()) => {
                                        println!("[Gateway:{}] {} not through the launcher relay ([auth] require = relay) — refused", port, addr);
                                        break 'conn;
                                    }
                                    Ok(Some(acc)) => {
                                        if acc.open_id != open_id {
                                            println!("[Gateway:{}] client claimed open_id {}, relay account — {}", port, open_id, acc.open_id);
                                        }
                                        open_id = acc.open_id.clone();
                                    }
                                    Ok(None) => {}
                                }
                                println!("[Gateway:{}] <- 0x1001 SYN (open_id={}) -> 0x1002 BinBingo", port, open_id);
                                if let Some(old) = hub_conn.take() {
                                    crate::lobby::hub::unregister(&open_id, old);
                                }
                                hub_conn = Some(crate::lobby::hub::register(&open_id, push_tx.clone()));
                                crate::dump_tgcp("OUR", &open_id, "S2C", &BIN_BINGO_0006);
                                let _ = socket.write_all(&BIN_BINGO_0006).await;
                            }
                        0x2001 => {
                            println!("[Gateway:{}] <- 0x2001 Auth -> 0x2002 AuthRsp + 0x6002 Ack", port);
                            let auth_rsp = crate::tgcp::auth_rsp_2002(&open_id);
                            crate::dump_tgcp("OUR", &open_id, "S2C", &auth_rsp);
                            let _ = socket.write_all(&auth_rsp).await;
                        }
                        0x4013 => {
                            let enc_flag = u32::from_be_bytes([frame[21], frame[22], frame[23], frame[24]]);
                            let raw_rpc = if enc_flag == 0x00000300 {
                                match taes_decrypt_c2s(&frame[25..], &SERVER_SESSION_KEY) {
                                    Ok(dec) => dec,
                                    Err(e) => {
                                        eprintln!("[Gateway:{}] Decrypt error: {}", port, e);
                                        continue;
                                    }
                                }
                            } else {
                                frame[25..].to_vec()
                            };

                            if crate::rpc_dump_enabled() {
                                crate::dump_rpc_head("OUR", &open_id, "C2S", &raw_rpc);
                            }
                            let rpc_frame = match RpcFrame::decode(raw_rpc.as_slice()) {
                                Ok(f) => f,
                                Err(e) => {
                                    eprintln!("[Gateway:{}] RpcFrame decode error: {}", port, e);
                                    continue;
                                }
                            };

                            let head = rpc_frame.head.unwrap_or_default();
                            let req_seq = head.client_sequence;
                            let routing = head.c2s_routing.unwrap_or_default();
                            let full_method = routing.method.as_str();
                            let short_method = full_method.rsplit('.').next().unwrap_or(full_method);

                            println!("[RPC] <- {} (seq={})", short_method, req_seq);
                            if crate::rpc_dump_enabled() {
                                crate::dump_rpc_who("OUR", &open_id, "C2S", full_method, &routing.type_url, &rpc_frame.body);
                            }

                            // 1. Create a session for the current request
                            let session = GatewaySession {
                                db: db.clone(),
                                game_data: game_data.clone(), // Pass access to the tables
                                open_id: open_id.clone(),
                                peer_ip: peer_ip.clone(),
                                after_pushes: Mutex::new(Vec::new()),
                                rpc_error: std::sync::atomic::AtomicI32::new(0),
                            };

                            // 2. CALL THE GENERATED ROUTER
                            if let Some((rsp_type, rsp_bytes)) = crate::game_data::route_rpc(&session, short_method, &rpc_frame.body).await {
                                
                                // 3. Send the main reply
                                if crate::rpc_dump_enabled() {
                                    crate::dump_rpc_who("OUR", &open_id, "S2C", full_method, &rsp_type, &rsp_bytes);
                                }
                                rpc_s2c_seq += 1;
                                tgcp_s2c_seq += 1;
                                let err = session.rpc_error.load(std::sync::atomic::Ordering::Relaxed);
                                if err != 0 {
                                    println!("[RPC] -> {} error_code={}", short_method, err);
                                }
                                let r_frame = crate::rpc::build_rpc_frame_with_error(rpc_s2c_seq, req_seq, full_method, &rsp_type, rsp_bytes, err);
                                if crate::rpc_dump_enabled() {
                                    crate::dump_rpc_head("OUR", &open_id, "S2C", &r_frame);
                                }
                                let out = encode_tgcp_s2c(tgcp_s2c_seq, &r_frame);
                                crate::dump_tgcp("OUR", &open_id, "S2C", &out);
                                let _ = socket.write_all(&out).await;
                                println!("[RPC] -> {} (seq={})", short_method, req_seq);

                                // 4. Send PUSH notifications with the correct type_url!
                                let pushes = session.after_pushes.into_inner().unwrap();
                                for (push_method, push_type, push_bytes) in pushes {
                                    if crate::rpc_dump_enabled() {
                                        crate::dump_rpc_who("OUR", &open_id, "S2C", &push_method, &push_type, &push_bytes);
                                    }
                                    rpc_s2c_seq += 1;
                                    tgcp_s2c_seq += 1;
                                    let p_frame = build_rpc_frame(rpc_s2c_seq, 0, &push_method, &push_type, push_bytes);
                                    if crate::rpc_dump_enabled() {
                                        crate::dump_rpc_head("OUR", &open_id, "S2C", &p_frame);
                                    }
                                    let out = encode_tgcp_s2c(tgcp_s2c_seq, &p_frame);
                                    crate::dump_tgcp("OUR", &open_id, "S2C", &out);
                                    let _ = socket.write_all(&out).await;
                                    println!("[RPC] => push {} ({})", push_method, push_type);
                                }
                            } else {
                                println!("[RPC] Method {} not handled (returned None)", short_method);
                            }
                        }
                        0x5001 => {
                            println!("[Gateway:{}] <- 0x5001 client stop", port);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(conn) = hub_conn {
                crate::lobby::hub::unregister(&open_id, conn);
                crate::lobby::on_disconnect(&open_id);
                crate::rpc::handlers::presence_offline(&open_id);
            }
            println!("[Gateway:{}] Client {} disconnected", port, addr);
        });
    }
    Ok(())
}