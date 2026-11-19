use prost::Message;
use std::collections::HashMap;
use std::fs;

// Include the generated code (router and the GameData struct)
include!("generated/generated_server_code.rs");

// Minimal struct for unpacking .bytes
#[derive(Clone, PartialEq, Message)]
pub struct XresDataBlocks {
    #[prost(bytes = "vec", repeated, tag = "2")]
    pub data_block: Vec<Vec<u8>>,
}

// Helper for loading into a HashMap
pub fn load_bytes_map<T, K, F>(path: &str, get_key: F) -> HashMap<K, T>
where
    T: Message + Default,
    K: std::hash::Hash + Eq,
    F: Fn(&T) -> K,
{
    let mut map = HashMap::new();
    if let Ok(raw) = fs::read(path) {
        if let Ok(container) = XresDataBlocks::decode(raw.as_slice()) {
            for row in container.data_block {
                if let Ok(item) = T::decode(row.as_slice()) {
                    map.insert(get_key(&item), item);
                }
            }
        }
    }
    map
}

// Helper for loading into a Vec
pub fn load_bytes_vec<T>(path: &str) -> Vec<T>
where
    T: Message + Default,
{
    let mut vec = Vec::new();
    if let Ok(raw) = fs::read(path) {
        if let Ok(container) = XresDataBlocks::decode(raw.as_slice()) {
            for row in container.data_block {
                if let Ok(item) = T::decode(row.as_slice()) {
                    vec.push(item);
                }
            }
        }
    }
    vec
}
