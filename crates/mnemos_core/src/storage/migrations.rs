//! Schema migrations. Task 11 fills in the real v1 schema tables.
//! This stub creates the `schema_migrations` tracking table and marks v1 applied.

use crate::error::Result;
use crate::storage::Storage;

impl Storage {
    pub(crate) async fn apply_migrations(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        )
        .await?;
        // Task 11 fills in the full v1 schema. For now just mark v1 applied so
        // `schema_version()` returns 1.
        conn.execute(
            "INSERT OR IGNORE INTO schema_migrations (version) VALUES (1)",
            (),
        )
        .await?;
        Ok(())
    }
}
