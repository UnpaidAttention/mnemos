pub mod audit;
pub mod chunk_ops;
pub mod community_ops;
pub mod entity_ops;
pub mod memory_ops;
pub mod migrations;
pub mod reflection_ops;
pub mod triggers;
pub mod vault_meta;
pub mod vec_ops;

use crate::error::{MnemosError, Result};
use libsql::{Builder, Connection, Database};
use std::path::Path;
use std::sync::{Arc, Once};
use tokio::sync::Mutex;

static VEC_INIT: Once = Once::new();

fn ensure_vec_extension_registered() {
    VEC_INIT.call_once(|| {
        // SAFETY: `sqlite_vec::sqlite3_vec_init` is the well-defined C entry
        // point for the sqlite-vec extension.  The `extern "C"` declaration in
        // the sqlite-vec crate intentionally omits the argument list so that
        // callers can transmute the function pointer into the shape required by
        // `sqlite3_auto_extension`.  The target type matches exactly the
        // signature documented by SQLite for extension entry points.
        // `libsql_sys::ffi::sqlite3_auto_extension` operates on the same
        // bundled SQLite instance that libsql uses, so the extension will be
        // loaded into every connection that libsql opens.
        unsafe {
            type ExtInit = unsafe extern "C" fn(
                *mut libsql_sys::ffi::sqlite3,
                *mut *const std::os::raw::c_char,
                *const libsql_sys::ffi::sqlite3_api_routines,
            ) -> std::os::raw::c_int;
            let init: ExtInit = std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
            let rc = libsql_sys::ffi::sqlite3_auto_extension(Some(init));
            assert_eq!(
                rc,
                libsql_sys::ffi::SQLITE_OK,
                "sqlite3_auto_extension for sqlite-vec failed: {rc}"
            );
        }
    });
}

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
        // Ordering contract (do not change without understanding all three steps):
        //
        // 1. `build()` above creates the Database handle.
        // 2. Opening the FIRST connection via `connect()` triggers libsql's
        //    one-time sqlite3_config(SERIALIZED) + sqlite3_initialize() call.
        //    `sqlite3_auto_extension` calls `sqlite3_initialize` internally and
        //    will return SQLITE_MISUSE (rc=21) if called before libsql has
        //    finished its own threading setup.  Therefore we must open at least
        //    one connection before calling `ensure_vec_extension_registered`.
        // 3. Extensions registered via `sqlite3_auto_extension` apply to all
        //    FUTURE connections, so the connection that triggered libsql init
        //    does NOT get vec0 — but every connection opened afterward (including
        //    the ones used by `apply_migrations`) will have vec_version() and
        //    vec0 available, which is what migration v2 requires.
        drop(storage.conn()?); // step 2: trigger libsql's sqlite3_initialize
        ensure_vec_extension_registered(); // step 3: safe now

        // P1-7: Apply PRAGMAs before migrations run.
        //
        // * `journal_mode=WAL` is a database-level setting persisted in the
        //   file header — it applies to all future connections and reopens.
        // * `busy_timeout`, `synchronous`, `cache_size`, `mmap_size` are
        //   connection-scoped.  In libsql's local (in-process) mode all
        //   connections share one SQLite handle, so the initial batch is
        //   sufficient for normal use.  The WAL setting is independently
        //   verifiable from any connection.
        {
            let conn = storage.conn()?;
            conn.execute_batch(
                "PRAGMA journal_mode=WAL; \
                 PRAGMA busy_timeout=5000; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA cache_size=-8000; \
                 PRAGMA mmap_size=67108864;",
            )
            .await?;
        }

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

    /// Read the stored embedder metadata (dim and model_id) from `vault_meta`.
    pub async fn get_vault_meta(&self) -> Result<VaultMeta> {
        let conn = self.conn()?;
        let mut rows = conn
            .query(
                "SELECT embedder_dim, embedder_model_id FROM vault_meta WHERE id = 1",
                (),
            )
            .await?;
        let row = rows.next().await?;
        match row {
            Some(r) => Ok(VaultMeta {
                embedder_dim: r.get::<Option<i64>>(0)?.map(|x| x as usize),
                embedder_model_id: r.get::<Option<String>>(1)?,
            }),
            None => Ok(VaultMeta {
                embedder_dim: None,
                embedder_model_id: None,
            }),
        }
    }

    /// Persist the embedder dim and model_id into `vault_meta`.
    pub async fn set_vault_meta(&self, dim: usize, model_id: &str) -> Result<()> {
        let (conn, _g) = self.write_conn().await?;
        conn.execute(
            "UPDATE vault_meta SET embedder_dim = ?, embedder_model_id = ?, updated_at = ? WHERE id = 1",
            libsql::params![
                dim as i64,
                model_id.to_string(),
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .await?;
        Ok(())
    }
}

/// Metadata about the embedder that was used to populate this vault.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VaultMeta {
    pub embedder_dim: Option<usize>,
    pub embedder_model_id: Option<String>,
}
