pub mod audit;
pub mod chunk_ops;
pub mod entity_ops;
pub mod memory_ops;
pub mod migrations;
pub mod triggers;

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
        // apply_migrations opens the first connection, which triggers libsql's
        // one-time sqlite3_config(SERIALIZED) + sqlite3_initialize() call.
        // We must register the sqlite-vec auto-extension AFTER that first
        // connection so that sqlite3_auto_extension (which calls
        // sqlite3_initialize internally) does not run before libsql has had the
        // chance to configure the threading mode.  Extensions registered via
        // sqlite3_auto_extension apply to all future connections, so every
        // caller that opens a connection after Storage::open returns will have
        // vec_version() and vec0 available.
        storage.apply_migrations().await?;
        ensure_vec_extension_registered();
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
