// Decodes the official server sniffer logs (sniffer_log*.txt) into JSON via Network.pb:
//   cargo run --quiet --example sniff_decode -- <method filter> [file ...]
// Without files it reads sniffer_log.txt and sniffer_log_001.txt. Filter — a substring of the method name ("" — all).
// Body type: C2S -> the service method's input, S2C -> output; a push without a reply (S2C on *_sync) — also output.
use prost_reflect::{DescriptorPool, DynamicMessage, MessageDescriptor};

fn resolve(pool: &DescriptorPool, dir: &str, full_method: &str) -> Option<MessageDescriptor> {
    let (service, method) = full_method.rsplit_once('.')?;
    let pick = |s: prost_reflect::ServiceDescriptor| {
        s.methods()
            .find(|m| m.name() == method)
            .map(|m| if dir == "C2S" { m.input() } else { m.output() })
    };
    pool.get_service_by_name(service).and_then(pick).or_else(|| pool.services().find_map(pick))
}

fn main() {
    let mut args = std::env::args().skip(1);
    let filter = args.next().unwrap_or_default();
    let mut files: Vec<String> = args.collect();
    if files.is_empty() {
        files = vec!["sniffer_log.txt".into(), "sniffer_log_001.txt".into()];
    }
    let pb = std::fs::read("Network.pb").expect("Network.pb");
    let pool = DescriptorPool::decode(pb.as_slice()).expect("Network.pb decode");

    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            eprintln!("no file {file}");
            continue;
        };
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            let Some(rest) = line.strip_prefix('[') else { continue };
            let Some((dir, method)) = rest.split_once("] ") else { continue };
            if dir != "C2S" && dir != "S2C" || !method.contains(&filter) {
                continue;
            }
            // bytes are on the next line "  => BYTES: [..]" (older logs: the Russian word); empty body — no line
            let bytes: Vec<u8> = lines
                .get(i + 1)
                .and_then(|l| l.split_once("[").map(|(_, b)| b))
                .filter(|_| lines[i + 1].trim_start().starts_with("=>"))
                .map(|b| b.trim_end_matches(']').split(',').filter_map(|x| x.trim().parse().ok()).collect())
                .unwrap_or_default();
            let ty = resolve(&pool, dir, method);
            let json = match &ty {
                Some(d) => match DynamicMessage::decode(d.clone(), bytes.as_slice()) {
                    Ok(m) => serde_json::to_string_pretty(&m).unwrap_or_default(),
                    Err(e) => format!("<decode error: {e}>"),
                },
                None => "<type not found>".into(),
            };
            println!(
                "### {file}:{} [{dir}] {method} ({}, {} bytes)\n{json}\n",
                i + 1,
                ty.map(|d| d.full_name().to_string()).unwrap_or_default(),
                bytes.len()
            );
        }
    }
}
