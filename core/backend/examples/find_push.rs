// Finds the type of a push body without routing: cargo run --example find_push -- <file.bin>...
// Prints messages from Network.pb that have all top-level message fields from the .bin and whose body decodes.
use prost_reflect::{DescriptorPool, DynamicMessage, Kind};

fn top_fields(b: &[u8]) -> Vec<u32> {
    let mut out = Vec::new();
    let mut i = 0;
    let rd = |i: &mut usize| -> Option<u64> {
        let mut v = 0u64;
        for s in 0..10 {
            let x = *b.get(*i)?;
            *i += 1;
            v |= ((x & 0x7f) as u64) << (7 * s);
            if x & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    };
    while i < b.len() {
        let Some(k) = rd(&mut i) else { break };
        let (f, wt) = ((k >> 3) as u32, k & 7);
        match wt {
            0 => { rd(&mut i); }
            1 => i += 8,
            2 => { let l = rd(&mut i).unwrap_or(0) as usize; i += l; }
            5 => i += 4,
            _ => break,
        }
        if !out.contains(&f) {
            out.push(f);
        }
    }
    out
}

fn main() {
    let pool = DescriptorPool::decode(std::fs::read("Network.pb").unwrap().as_slice()).unwrap();
    for path in std::env::args().skip(1) {
        let body = std::fs::read(&path).unwrap();
        let fields = top_fields(&body);
        println!("== {} top fields {:?}", path, fields);
        for m in pool.all_messages() {
            let ok = fields.iter().all(|n| m.get_field(*n).map_or(false, |f| matches!(f.kind(), Kind::Message(_))));
            if ok && DynamicMessage::decode(m.clone(), body.as_slice()).is_ok() {
                let names: Vec<String> = fields.iter().map(|n| format!("{}={}", n, m.get_field(*n).unwrap().name())).collect();
                println!("  {} [{}]", m.full_name(), names.join(", "));
            }
        }
    }
}
