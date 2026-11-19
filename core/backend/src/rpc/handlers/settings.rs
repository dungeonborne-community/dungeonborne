// 11.11: client settings (DPlayerSettingData) — a separate small record `player_settings:<open_id>` plus a process cache.
//
// Before: player_setting_data loaded the whole player document (~150 KB: inventory, stash, vendor tabs), put the
// settings bytes into state["player_setting_bytes"] and saved the whole document — on every request, even with no changes. The client
// sends them often (on prod ~80 per minute with 9 online), and the SurrealKV log (clog) is append-only and never compacted:
// the database grew to 59 GB in 3 days and filled the VM disk.
// Now: the player document is not touched; no changes — nothing is written; with changes — only this record (a few KB).
// The old field state["player_setting_bytes"] is not read: the prod database was deleted on 11.11 and restarted on RocksDB.
use super::*;
use serde::{Deserialize, Serialize};

const TABLE: &str = "player_settings";

#[derive(Serialize, Deserialize, Clone, Default)]
struct SettingsRow {
    bytes: Vec<u8>,
}

fn cache() -> &'static Mutex<HashMap<String, Vec<u8>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<u8>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn decode(bytes: &[u8]) -> Option<tgf::DPlayerSettingData> {
    tgf::DPlayerSettingData::decode(bytes).ok()
}

/// Without touching the database (for synchronous code, e.g. battle data): cache only. It is filled on login
/// (player_get_info) and on every settings change — a player in a match already has it.
pub(super) fn settings_cached(open_id: &str) -> Option<tgf::DPlayerSettingData> {
    let cached = cache().lock().unwrap().get(open_id).cloned();
    cached.and_then(|b| decode(&b))
}

impl GatewaySession {
    /// Settings bytes: cache → `player_settings` record.
    async fn settings_bytes(&self, open_id: &str) -> Option<Vec<u8>> {
        if let Some(b) = cache().lock().unwrap().get(open_id).cloned() {
            return Some(b);
        }
        let found = self.db.get::<SettingsRow>(TABLE, open_id).await.map(|row| row.bytes);
        if let Some(b) = &found {
            cache().lock().unwrap().insert(open_id.to_string(), b.clone());
        }
        found
    }

    /// Settings on login (player_get_info): also warms the cache for battle data.
    pub(super) async fn settings_load(&self, open_id: &str) -> Option<tgf::DPlayerSettingData> {
        self.settings_bytes(open_id).await.and_then(|b| decode(&b))
    }

    /// player_setting_data: merge the incoming keys with the saved ones; write only if something changed.
    pub(super) async fn settings_merge(&self, open_id: &str, incoming: tgf::DPlayerSettingData) {
        let old = self.settings_bytes(open_id).await.and_then(|b| decode(&b));
        let mut current = old.clone().unwrap_or_default();
        for (k, v) in incoming.data {
            current.data.insert(k, v);
        }
        if let Some(kv) = incoming.kv_data {
            let target = current.kv_data.get_or_insert_with(tgf::DClientSettingKeyVal::default);
            for (k, v) in kv.data {
                target.data.insert(k, v);
            }
        }
        // compare messages, not bytes: a protobuf map is encoded in HashMap order — the bytes of the same
        // settings may differ
        if old.as_ref() == Some(&current) {
            return;
        }
        let bytes = current.encode_to_vec();
        self.db.save(TABLE, open_id, &SettingsRow { bytes: bytes.clone() }).await;
        cache().lock().unwrap().insert(open_id.to_string(), bytes);
    }
}
