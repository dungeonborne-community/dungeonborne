use serde::{de::DeserializeOwned, Serialize};
use surrealdb::engine::local::{Db, RocksDb};
use surrealdb::Surreal;

/// 11.11: our own RocksDB defaults in SurrealDB 2.7 (read from the environment when the DB is first opened;
/// a variable set from outside takes precedence).
/// - ENABLE_BLOB_FILES=false: by default values from 4 KB go to blob files, and garbage collection for them is off
///   (ENABLE_BLOB_GC=false) — old versions of a player document (75–150 KB) would pile up on disk, as in SurrealKV.
///   Without blob files values live in SST, and regular RocksDB compaction throws old versions away.
/// - BLOCK_CACHE_SIZE=128 MB: the default is "half of RAM minus 1 GB" (~0.9 GB on a VM with 3.8 GB); our DB is tens of MB.
/// - SYNC_DATA=true: a write is acknowledged after the journal is flushed to disk — player saves are not lost on an OS crash.
fn rocksdb_defaults() {
    for (key, value) in [
        ("SURREAL_ROCKSDB_ENABLE_BLOB_FILES", "false"),
        ("SURREAL_ROCKSDB_BLOCK_CACHE_SIZE", "134217728"),
        ("SURREAL_SYNC_DATA", "true"),
    ] {
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
}

/// One export at a time (`GameDb::export_to`)
static EXPORTING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[derive(Clone)]
pub struct GameDb {
    pub db: Surreal<Db>,
}

impl GameDb {
    pub async fn init(path: &str) -> Self {
        // 1. Connect to the DB (RocksDB since 11.11). A folder of the old SurrealKV engine (clog/ and manifest/) is not ours:
        // RocksDB would put its files next to it and start with an empty DB, silently losing data.
        let dir = std::path::Path::new(path);
        if dir.join("clog").is_dir() || dir.join("manifest").is_dir() {
            panic!("[SurrealDB] {} — DB in the old SurrealKV format; RocksDB cannot read it. Set a different DB_PATH", path);
        }
        rocksdb_defaults();
        let db = Surreal::new::<RocksDb>(path).await.unwrap();
        db.use_ns("dungeonborne").use_db("gateway").await.unwrap();

        let game_db = Self { db };

        // 2. Set up the schema and indexes automatically
        game_db.setup_schema().await;

        println!("[SurrealDB] Initialized at '{}'", path);
        game_db
    }

    /// 16.11: consistent DB copy without stopping — SurrealDB SQL export (`export`): the whole DB is read in one
    /// read transaction, i.e. from a RocksDB snapshot as of the start; player writes do not wait for it. File
    /// `<dir>/db-<unix>.surql` (mode 0600 on Linux: it contains everything, including session hashes). Restore — `import` this
    /// file into an empty DB folder (`examples/db_import.rs`). Returns (path, bytes, seconds).
    pub async fn export_to(&self, dir: &std::path::Path) -> Result<(std::path::PathBuf, u64, f64), String> {
        use std::sync::atomic::Ordering;
        if EXPORTING.swap(true, Ordering::SeqCst) {
            return Err("export already in progress".into());
        }
        let r = self.export_inner(dir).await;
        EXPORTING.store(false, Ordering::SeqCst);
        r
    }

    async fn export_inner(&self, dir: &std::path::Path) -> Result<(std::path::PathBuf, u64, f64), String> {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
        let unix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
        let path = dir.join(format!("db-{}.surql", unix));
        let part = path.with_extension("surql.part");
        let t = std::time::Instant::now();
        self.db.export(&part).await.map_err(|e| format!("export: {}", e))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&part, &path).map_err(|e| format!("{}: {}", path.display(), e))?;
        let bytes = std::fs::metadata(&path).map_or(0, |m| m.len());
        let secs = t.elapsed().as_secs_f64();
        println!("[SurrealDB] export {} ({} bytes, {:.1} s)", path.display(), bytes, secs);
        Ok((path, bytes, secs))
    }

    /// DB schema setup (runs at every startup, this is safe)
    async fn setup_schema(&self) {
        let query = r#"
        DEFINE TABLE players SCHEMALESS;
        DEFINE INDEX idx_open_id ON players COLUMNS open_id UNIQUE;
        DEFINE INDEX idx_user_id ON players COLUMNS user_id UNIQUE;
        DEFINE INDEX idx_nick_name ON players COLUMNS nick_name;
        "#;

        if let Err(e) = self.db.query(query).await {
            eprintln!("[SurrealDB] Schema setup error: {}", e);
        } else {
            println!("[SurrealDB] Schema and indexes set up successfully.");
        }
    }

    /// Get a record by ID (e.g.: get::<PlayerModel>("players", "76561198000000901"))
    pub async fn get<T: DeserializeOwned>(&self, table: &str, id: &str) -> Option<T> {
        match self.db.select::<Option<T>>((table, id)).await {
            Ok(res) => res,
            Err(e) => {
                eprintln!("[SurrealDB] Read error {}:{}: {}", table, id, e);
                None
            }
        }
    }

    /// Save a record (creates a new one or updates an existing one)
    /// 11.11: transaction conflict ("read or write conflict … can be retried") — retry up to 5 times with a pause: on prod
    /// such player saves were simply lost (5 in 20 minutes).
    pub async fn save<T: Serialize + DeserializeOwned + Clone + 'static>(&self, table: &str, id: &str, data: &T) {
        const TRIES: u32 = 5;
        for attempt in 1..=TRIES {
            match self.db.upsert::<Option<T>>((table, id)).content(data.clone()).await {
                Ok(_) => {
                    if attempt > 1 {
                        println!("[SurrealDB] {}:{} saved on attempt {}", table, id, attempt);
                    }
                    return;
                }
                Err(e) => {
                    let msg = e.to_string();
                    if attempt < TRIES && msg.contains("can be retried") {
                        tokio::time::sleep(std::time::Duration::from_millis(20 * attempt as u64)).await;
                        continue;
                    }
                    eprintln!("[SurrealDB] Save error {}:{} (attempt {}): {}", table, id, attempt, msg);
                    return;
                }
            }
        }
    }

    /// Find a player by internal user_id
    pub async fn find_by_user_id<T: DeserializeOwned>(&self, user_id: u64) -> Option<T> {
        let mut response = self.db
            .query("SELECT * FROM players WHERE user_id = $uid LIMIT 1")
            .bind(("uid", user_id))
            .await
            .ok()?;

        response.take(0).ok()?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
    struct Row {
        account_key: String,
        expires_at: i64,
    }

    /// 16.11: a no-stop export is read back with `import` into an empty DB (like `examples/db_import.rs`)
    #[tokio::test]
    async fn export_imports_into_empty_db() {
        let root = std::env::temp_dir().join(format!("dbexport-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let src = GameDb::init(root.join("src").to_str().unwrap()).await;
        let row = Row { account_key: "discord:1".into(), expires_at: 5 };
        src.save("auth_sessions", "k1", &row).await;
        assert_eq!(src.get::<Row>("auth_sessions", "k1").await.as_ref(), Some(&row), "record before export");
        let (file, bytes, _) = src.export_to(&root.join("out")).await.unwrap();
        assert!(bytes > 0 && file.extension().is_some_and(|e| e == "surql"));
        assert!(!root.join("out").read_dir().unwrap().flatten().any(|e| e.path().to_string_lossy().ends_with(".part")));

        let dst = Surreal::new::<RocksDb>(root.join("dst").to_str().unwrap()).await.unwrap();
        dst.use_ns("dungeonborne").use_db("gateway").await.unwrap();
        dst.import(&file).await.unwrap();
        let back = GameDb { db: dst };
        let text = std::fs::read_to_string(&file).unwrap();
        assert_eq!(back.get::<Row>("auth_sessions", "k1").await, Some(row), "after import; export:
{text}");
        drop(back);
        drop(src);
        let _ = std::fs::remove_dir_all(&root);
    }
}
