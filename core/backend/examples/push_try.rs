// Sniffer pushes without routing ("[S2C] [PUSH] UNKNOWN_TYPE_0"): try candidate types, take the first one
// whose decode succeeds and whose re-encoding gives the same bytes.
//   cargo run --quiet --example push_try -- [file ...]   (default sniffer_log_001.txt)
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage};

const CANDIDATES: &[&str] = &[
    "tgf.SCBattleRoomLogSync",
    "tgf.SCBattleMatchingLogSync",
    "tgf.SCBattleSettleSync",
    "tgf.SCPlayerDirtyChgSync",
];

fn main() {
    let mut files: Vec<String> = std::env::args().skip(1).collect();
    if files.is_empty() {
        files = vec!["sniffer_log_001.txt".into()];
    }
    let pb = std::fs::read("Network.pb").expect("Network.pb");
    let pool = DescriptorPool::decode(pb.as_slice()).expect("Network.pb decode");
    for file in &files {
        let text = std::fs::read_to_string(file).expect("file");
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.starts_with("[S2C] [PUSH]") {
                continue;
            }
            let bytes: Vec<u8> = lines
                .get(i + 1)
                .and_then(|l| l.split_once('[').map(|(_, r)| r.trim_end_matches(']')))
                .map(|s| s.split(',').filter_map(|v| v.trim().parse().ok()).collect())
                .unwrap_or_default();
            let mut found = false;
            for name in CANDIDATES {
                let desc = pool.get_message_by_name(name).expect(name);
                if let Ok(msg) = DynamicMessage::decode(desc, bytes.as_slice()) {
                    if msg.encode_to_vec() == bytes {
                        let json = serde_json::to_string(&msg).unwrap_or_default();
                        println!("### {file}:{} {name} ({} bytes)\n{json}\n", i + 1, bytes.len());
                        found = true;
                        break;
                    }
                }
            }
            // Re-encoding may differ because of map element order — then "~" and the re-encoded length
            if !found {
                for name in CANDIDATES {
                    let desc = pool.get_message_by_name(name).expect(name);
                    if let Ok(msg) = DynamicMessage::decode(desc, bytes.as_slice()) {
                        let json = serde_json::to_string(&msg).unwrap_or_default();
                        println!("### {file}:{} ~{name} ({} bytes, re-encoded {})\n{json}\n", i + 1, bytes.len(), msg.encode_to_vec().len());
                        found = true;
                        break;
                    }
                }
            }
            if !found {
                println!("### {file}:{} ? ({} bytes) {:?}\n", i + 1, bytes.len(), &bytes[..bytes.len().min(24)]);
            }
        }
    }
}
