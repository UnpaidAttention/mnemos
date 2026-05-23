pub mod audit;
pub mod chunk_ops;
pub mod entity_ops;
pub mod memory_ops;
pub mod migrations;
pub mod triggers;

use crate::error::{MnemosError, Result};
use libsql::{Builder, Connection, Database};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;

/// libSQL handle plus a serialized-write mutex.
///
/// Reads can go through `conn()` directly; writes acquire `write_lock` to
/// serialize against each other (SQLite is single-writer anyway).
#[derive(Clone)]
pub struct Storage {
    db: Arc<Database>,
    write_lock: Arc<Mutex<()>>,
}

impl Storage {
    /// Open (or create) a local libSQL database at `db_path` and run
    /// any pending schema migrations.
    pub async fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let db = Builder::new_local(db_path).build().await?;
        let storage = Self {
            db: Arc::new(db),
            write_lock: Arc::new(Mutex::new(())),
        };
        storage.apply_migrations().await?;
        Ok(storage)
    }

    /// Returns a fresh connection. Each checkout from libsql is cheap.
    pub fn conn(&self) -> Result<Connection> {
        Ok(self.db.connect()?)
    }

    /// Acquire the write lock and return a guarded connection.
    pub async fn write_conn(&self) -> Result<(Connection, tokio::sync::MutexGuard<'_, ()>)> {
        let guard = self.write_lock.lock().await;
        Ok((self.conn()?, guard))
    }

    /// Return the highest migration version that has been applied.
    pub async fn schema_version(&self) -> Result<u32> {
        let conn = self.conn()?;
        let mut rows = conn
            .query("SELECT MAX(version) FROM schema_migrations", ())
            .await?;
        let row = rows
            .next()
            .await?
            .ok_or_else(|| MnemosError::Internal("schema_migrations table empty".into()))?;
        let v: i64 = row.get(0_i32)?;
        Ok(v as u32)
    }
}
