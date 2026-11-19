use serde::{Deserialize, Serialize};
use crate::tgf;

/// New player's gold; `state.start_gold` — how much was given at start (a one-time top-up for old players at login)
pub const START_GOLD: i64 = 1_000_000;

/// New player's gold taking the season into account (`start_gold` in season.toml; playtest 07.11 — 10 000 000)
pub fn start_gold() -> i64 {
    crate::season::features().start_gold.unwrap_or(START_GOLD)
}

/// Serializes any Protobuf structure via native prost bytes,
/// so that SurrealDB does not crash on nested HashMap<i32, ...>!
mod prost_bytes {
    use prost::Message;
    use serde::{de::Visitor, Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<T: Message, S: Serializer>(val: &T, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&val.encode_to_vec())
    }

    pub fn deserialize<'de, T: Message + Default, D: Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
        struct BytesVisitor;

        impl<'de> Visitor<'de> for BytesVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("byte array or sequence of bytes")
            }

            fn visit_bytes<E: serde::de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.to_vec())
            }

            fn visit_byte_buf<E: serde::de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(v)
            }

            fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut vec = Vec::with_capacity(seq.size_hint().unwrap_or(0));
                while let Some(b) = seq.next_element()? {
                    vec.push(b);
                }
                Ok(vec)
            }
        }

        let bytes = deserializer.deserialize_any(BytesVisitor)?;
        T::decode(bytes.as_slice()).map_err(serde::de::Error::custom)
    }
}

// ==========================================
// 1. ROLE MODEL (Character)
// ==========================================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RoleModel {
    pub role_guid: u64,
    pub ord: i32,
    pub type_id: u32,
    pub role_name: String,
    pub role_level: u32,

    // Store the strict Protobuf structure via prost_bytes!
    #[serde(with = "prost_bytes")]
    pub data: tgf::DUserRole,
}

// ==========================================
// 2. PLAYER MODEL (Account)
// ==========================================
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlayerModel {
    #[serde(skip)]
    pub id: String,

    pub open_id: String,
    pub user_id: u64,
    pub hall_role_guid: u64,
    pub next_guid: u64,
    pub next_item_guid: u64,
    pub updated_at: i64,
    pub state: serde_json::Value,
    pub roles: Vec<RoleModel>,
    /// Account stash (EN_ITEM_STORAGE_TYPE_INVENTORY): item position — guid = page, x, y.
    /// Old DB records do not have the field — empty stash.
    #[serde(default, with = "prost_bytes")]
    pub stash: tgf::DItemGroup,
}

impl PlayerModel {
    pub fn new(open_id: &str, user_id: u64) -> Self {
        Self {
            id: format!("players:{}", open_id),
            open_id: open_id.to_string(),
            user_id,
            hall_role_guid: 0,
            next_guid: 0,
            // unused since 02.11 (item guids are a shared counter crate::guid); the field is kept for old records
            next_item_guid: 0,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            // ext: stash_pages — a new account gets the season's base stash pages (old records without the field keep 8)
            state: serde_json::json!({ "gold": start_gold(), "start_gold": start_gold(), "inventory_page_count": crate::season::features().stash.base_pages }),
            roles: Vec::new(),
            stash: tgf::DItemGroup::default(),
        }
    }

    pub fn get_role_mut(&mut self, role_guid: u64) -> Option<&mut RoleModel> {
        self.roles.iter_mut().find(|r| r.role_guid == role_guid)
    }

    pub fn add_role(&mut self, type_id: u32, role_name: String, data: tgf::DUserRole) -> u64 {
        self.next_guid += 1;
        let new_guid = self.user_id * 100 + self.next_guid;

        let new_role = RoleModel {
            role_guid: new_guid,
            ord: self.roles.len() as i32,
            type_id,
            role_name,
            role_level: 1,
            data,
        };

        self.roles.push(new_role);
        new_guid
    }
}