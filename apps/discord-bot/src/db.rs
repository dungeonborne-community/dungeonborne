//! The bot's built-in SurrealDB (SurrealKV). Each module's schema is the `const SCHEMA` in its file; this file only starts the database.
//! The schema is applied on every start: all DEFINEs use OVERWRITE, so restarting is safe.
//!
//! SurrealKV is not protected against a second process: two bots on one database folder corrupt each other's journal
//! ("Invalid revision … for type …"). So an OS lock on `<db>.lock` is taken next to the database:
//! a second process won't start, and the lock of a crashed process is released automatically.

use std::sync::Arc;
use surrealdb::engine::local::{Db, SurrealKv};
use surrealdb::Surreal;

#[derive(Clone)]
pub struct BotDb {
    pub db: Surreal<Db>,
    _lock: Arc<std::fs::File>,
}

fn lock(path: &str) -> Result<std::fs::File, String> {
    let lock_path = format!("{}.lock", path.trim_end_matches(['/', '\\']));
    if let Some(dir) = std::path::Path::new(&lock_path).parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    }
    let file = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path).map_err(|e| format!("{}: {}", lock_path, e))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(std::fs::TryLockError::WouldBlock) => Err(format!(
            "database '{}' is already open by another bot process; two bots on one database corrupt it. Stop the first one",
            path
        )),
        Err(std::fs::TryLockError::Error(e)) => Err(format!("{}: {}", lock_path, e)),
    }
}

impl BotDb {
    pub async fn init(path: &str) -> Result<Self, String> {
        let lock = lock(path)?;
        let db = Surreal::new::<SurrealKv>(path).await.map_err(|e| format!("{}: {}", path, e))?;
        db.use_ns("discord").use_db("bot").await.map_err(|e| e.to_string())?;
        for (module, schema) in crate::modules::SCHEMAS {
            db.query(*schema)
                .await
                .and_then(|r| r.check())
                .map_err(|e| format!("schema {}: {}", module, e))?;
        }
        println!("[SurrealDB] '{}': schema of {} modules applied", path, crate::modules::SCHEMAS.len());
        Ok(Self { db, _lock: Arc::new(lock) })
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn second_process_is_refused() {
        let dir = format!("target/test_db_lock_{}", std::process::id());
        let _ = std::fs::remove_dir_all(&dir);
        let first = super::BotDb::init(&dir).await.unwrap();
        let second = super::BotDb::init(&dir).await;
        assert!(second.as_ref().err().is_some_and(|e| e.contains("already open")), "{:?}", second.err());
        drop(first);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_file(format!("{}.lock", dir));
    }
}
