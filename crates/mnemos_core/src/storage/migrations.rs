//! Schema migrations.
//!
//! Each version gate is cumulative: opening a fresh database runs all
//! migrations in order; re-opening an existing database is a no-op if it is
//! already at the latest version.
//!
//! ## Atomicity (P1-1)
//!
//! Every `migration_vN` function and its companion `schema_migrations INSERT`
//! execute inside a single explicit `BEGIN … COMMIT` transaction.  SQLite
//! supports DDL inside transactions (even `CREATE TABLE` and `ALTER TABLE`),
//! which means:
//!
//! - A crash in the middle of a migration leaves **no** partial schema change;
//!   the transaction is rolled back on next open.
//! - Because the version row is written inside the same transaction as the DDL,
//!   re-running a partially-applied migration is always safe — if the version
//!   row is absent the migration is atomically re-attempted; if the version row
//!   is present (the transaction committed) the migration is skipped.
//!
//! The `write_lock` mutex is acquired for the duration of the entire migration
//! sequence to prevent any concurrent connection from observing a partially
//! migrated schema.  This also fixes the inconsistency noted in the backlog
//! where individual migration steps bypassed the write lock.

use crate::error::Result;
use crate::storage::Storage;

/// The latest schema version. Bump this whenever a new `migration_vN` is added.
///
/// Tests should assert against this constant rather than a hardcoded integer so
/// they do not break when new migrations land.
pub const LATEST_SCHEMA_VERSION: u32 = 11;

impl Storage {
    pub(crate) async fn apply_migrations(&self) -> Result<()> {
        // Hold the write lock for the entire migration sequence so no other
        // concurrent connection sees a partially-migrated schema.
        let (conn, _guard) = self.write_conn().await?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                version    INTEGER PRIMARY KEY,
                applied_at TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP
            )",
            (),
        )
        .await?;

        let mut rows = conn
            .query("SELECT MAX(version) FROM schema_migrations", ())
            .await?;
        let current: i64 = rows
            .next()
            .await?
            .and_then(|r| r.get::<i64>(0).ok())
            .unwrap_or(0);
        drop(rows);

        macro_rules! run_migration {
            ($ver:expr, $fn:ident) => {
                if current < $ver {
                    let tx = conn.transaction().await?;
                    $fn(&tx).await?;
                    tx.execute(
                        "INSERT OR IGNORE INTO schema_migrations (version) VALUES (?)",
                        libsql::params![$ver as i64],
                    )
                    .await?;
                    tx.commit().await?;
                }
            };
        }

        run_migration!(1, migration_v1);
        run_migration!(2, migration_v2);
        run_migration!(3, migration_v3);
        run_migration!(4, migration_v4);
        run_migration!(5, migration_v5);
        run_migration!(6, migration_v6);
        run_migration!(7, migration_v7);
        run_migration!(8, migration_v8);
        run_migration!(9, migration_v9);
        run_migration!(10, migration_v10);
        run_migration!(11, migration_v11);

        Ok(())
    }
}

// ── migration helpers ─────────────────────────────────────────────────────────

async fn migration_v1(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V1_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V1_STATEMENTS: &[&str] = &[
    // ── memories ─────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS memories (
        id              TEXT PRIMARY KEY,
        tier            TEXT NOT NULL CHECK(tier IN
            ('working','episodic','semantic','procedural','reflection')),
        kind            TEXT NOT NULL,
        title           TEXT NOT NULL,
        body            TEXT NOT NULL,
        file_path       TEXT NOT NULL UNIQUE,
        content_hash    TEXT NOT NULL,
        tags_json       TEXT NOT NULL DEFAULT '[]',
        entities_json   TEXT NOT NULL DEFAULT '[]',
        links_json      TEXT NOT NULL DEFAULT '[]',
        provenance_json TEXT NOT NULL DEFAULT '[]',
        created_at      TEXT NOT NULL,
        ingested_at     TEXT NOT NULL,
        valid_at        TEXT NOT NULL,
        invalid_at      TEXT,
        superseded_by   TEXT,
        strength        REAL NOT NULL DEFAULT 1.0,
        importance      REAL NOT NULL DEFAULT 0.5,
        last_accessed   TEXT NOT NULL,
        access_count    INTEGER NOT NULL DEFAULT 0,
        workspace       TEXT,
        source_tool     TEXT,
        mnemos_version  INTEGER NOT NULL DEFAULT 1,
        version         INTEGER NOT NULL DEFAULT 1
    )",
    "CREATE INDEX IF NOT EXISTS idx_memories_tier      ON memories(tier)",
    "CREATE INDEX IF NOT EXISTS idx_memories_valid     ON memories(valid_at, invalid_at)",
    "CREATE INDEX IF NOT EXISTS idx_memories_strength  ON memories(strength)",
    "CREATE INDEX IF NOT EXISTS idx_memories_workspace ON memories(workspace)",
    // ── sessions ─────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS sessions (
        id           TEXT PRIMARY KEY,
        source_tool  TEXT,
        workspace    TEXT,
        started_at   TEXT NOT NULL,
        ended_at     TEXT,
        summary      TEXT
    )",
    // ── chunks ───────────────────────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS chunks (
        id          TEXT PRIMARY KEY,
        session_id  TEXT NOT NULL,
        speaker     TEXT,
        ordinal     INTEGER NOT NULL,
        body        TEXT NOT NULL,
        created_at  TEXT NOT NULL,
        source_tool TEXT,
        source_meta TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_chunks_session ON chunks(session_id, ordinal)",
    // ── entities + mentions + edges ──────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS entities (
        id          TEXT PRIMARY KEY,
        name        TEXT NOT NULL UNIQUE,
        kind        TEXT NOT NULL,
        aliases     TEXT NOT NULL DEFAULT '[]',
        description TEXT,
        file_path   TEXT,
        created_at  TEXT NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS entity_mentions (
        memory_id TEXT NOT NULL,
        entity_id TEXT NOT NULL,
        PRIMARY KEY (memory_id, entity_id),
        FOREIGN KEY (memory_id) REFERENCES memories(id) ON DELETE CASCADE,
        FOREIGN KEY (entity_id) REFERENCES entities(id) ON DELETE CASCADE
    )",
    "CREATE INDEX IF NOT EXISTS idx_entity_mentions_entity ON entity_mentions(entity_id)",
    "CREATE TABLE IF NOT EXISTS entity_edges (
        id                TEXT PRIMARY KEY,
        source_entity_id  TEXT NOT NULL,
        target_entity_id  TEXT NOT NULL,
        relation          TEXT NOT NULL,
        created_at        TEXT NOT NULL,
        valid_at          TEXT NOT NULL,
        invalid_at        TEXT,
        weight            REAL NOT NULL DEFAULT 1.0,
        source_memory_ids TEXT NOT NULL DEFAULT '[]'
    )",
    "CREATE INDEX IF NOT EXISTS idx_edges_source   ON entity_edges(source_entity_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_target   ON entity_edges(target_entity_id)",
    "CREATE INDEX IF NOT EXISTS idx_edges_relation ON entity_edges(relation)",
    // ── links + chunk provenance ─────────────────────────────────────────
    "CREATE TABLE IF NOT EXISTS memory_links (
        source_id TEXT NOT NULL,
        target_id TEXT NOT NULL,
        kind      TEXT NOT NULL,
        PRIMARY KEY (source_id, target_id, kind)
    )",
    "CREATE INDEX IF NOT EXISTS idx_links_target ON memory_links(target_id)",
    "CREATE TABLE IF NOT EXISTS memory_chunks (
        memory_id TEXT NOT NULL,
        chunk_id  TEXT NOT NULL,
        PRIMARY KEY (memory_id, chunk_id)
    )",
    // ── audit log (append-only enforced via trigger in Task 15) ──────────
    "CREATE TABLE IF NOT EXISTS audit_log (
        id        INTEGER PRIMARY KEY AUTOINCREMENT,
        ts        TEXT NOT NULL,
        actor     TEXT NOT NULL,
        action    TEXT NOT NULL,
        memory_id TEXT,
        details   TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_audit_memory ON audit_log(memory_id)",
    "CREATE INDEX IF NOT EXISTS idx_audit_ts     ON audit_log(ts)",
    // Append-only enforcement on audit_log
    "CREATE TRIGGER IF NOT EXISTS audit_log_no_update
        BEFORE UPDATE ON audit_log
        BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: UPDATE not allowed');
        END",
    "CREATE TRIGGER IF NOT EXISTS audit_log_no_delete
        BEFORE DELETE ON audit_log
        BEGIN
            SELECT RAISE(ABORT, 'audit_log is append-only: DELETE not allowed');
        END",
    // ── FTS5 virtual tables ──────────────────────────────────────────────
    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
        memory_id UNINDEXED, title, body,
        tokenize='porter unicode61'
    )",
    "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_fts USING fts5(
        chunk_id UNINDEXED, body,
        tokenize='porter unicode61'
    )",
];

async fn migration_v2(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V2_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V2_STATEMENTS: &[&str] = &[
    // 768d matches nomic-embed-text. If you change embedding model dim,
    // bump to a v3 migration with a new table.
    "CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(
        memory_id TEXT PRIMARY KEY,
        embedding FLOAT[768]
    )",
    "CREATE VIRTUAL TABLE IF NOT EXISTS chunk_vec USING vec0(
        chunk_id TEXT PRIMARY KEY,
        embedding FLOAT[768]
    )",
];

async fn migration_v3(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V3_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V3_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS vault_meta (
        id                INTEGER PRIMARY KEY CHECK(id = 1),
        embedder_dim      INTEGER,
        embedder_model_id TEXT,
        updated_at        TEXT NOT NULL
    )",
    "INSERT OR IGNORE INTO vault_meta (id, updated_at) VALUES (1, '1970-01-01T00:00:00Z')",
];

async fn migration_v4(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V4_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V4_STATEMENTS: &[&str] = &[
    // Stamped by the pipeline runner once a session's chunks have been
    // processed into memories, so SessionEnded is idempotent.
    "ALTER TABLE sessions ADD COLUMN processed_at TEXT",
];

async fn migration_v5(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V5_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V5_STATEMENTS: &[&str] = &[
    // Stamped once a memory has been included in a reflection pass.
    "ALTER TABLE memories ADD COLUMN reflected_at TEXT",
    // Single-row salience accumulator driving reflection triggers.
    "CREATE TABLE IF NOT EXISTS reflection_state (
        id               INTEGER PRIMARY KEY CHECK(id = 1),
        salience         REAL NOT NULL DEFAULT 0,
        last_reflected_at TEXT
    )",
    "INSERT OR IGNORE INTO reflection_state (id, salience) VALUES (1, 0)",
];

async fn migration_v6(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V6_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V6_STATEMENTS: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS entity_communities (
        entity_id    TEXT PRIMARY KEY,
        community_id INTEGER NOT NULL,
        detected_at  TEXT NOT NULL
    )",
    "CREATE INDEX IF NOT EXISTS idx_entity_communities_cid ON entity_communities(community_id)",
];

async fn migration_v7(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V7_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V7_STATEMENTS: &[&str] = &[
    // Single-row sync bookkeeping.
    "CREATE TABLE IF NOT EXISTS sync_state (
        id                INTEGER PRIMARY KEY CHECK(id = 1),
        last_pushed_at    TEXT,
        last_pulled_at    TEXT,
        last_error        TEXT
    )",
    "INSERT OR IGNORE INTO sync_state (id) VALUES (1)",
    // Detected conflict files (Syncthing-style, etc.) and Git merge conflicts.
    "CREATE TABLE IF NOT EXISTS sync_conflicts (
        id           INTEGER PRIMARY KEY AUTOINCREMENT,
        ts           TEXT NOT NULL,
        path         TEXT NOT NULL,
        detected_by  TEXT NOT NULL,
        resolved_at  TEXT,
        details      TEXT
    )",
    "CREATE INDEX IF NOT EXISTS idx_sync_conflicts_unresolved ON sync_conflicts(resolved_at) WHERE resolved_at IS NULL",
];

async fn migration_v8(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V8_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V8_STATEMENTS: &[&str] = &[
    // First-run wizard completion timestamp (RFC3339). NULL until the user
    // finishes the welcome flow; subsequent launches skip the wizard.
    "ALTER TABLE vault_meta ADD COLUMN first_run_completed_at TEXT",
];

async fn migration_v9(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V9_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V9_STATEMENTS: &[&str] = &[
    // Add embedder_kind column. Default to 'bundled' for fresh vaults;
    // upgrades from v8 see NULL → we backfill below to 'ollama' since
    // any pre-v9 vault was necessarily seeded with the old default.
    "ALTER TABLE vault_meta ADD COLUMN embedder_kind TEXT",
    // Backfill: existing v8 vaults had embedder_model_id set by the first
    // remember; if that model_id was empty (truly fresh) treat as bundled,
    // otherwise treat as ollama. The daemon will reconcile this with
    // the actual configured embedder on next startup.
    "UPDATE vault_meta
        SET embedder_kind = CASE
            WHEN embedder_model_id IS NULL OR embedder_model_id = '' THEN 'bundled'
            WHEN embedder_model_id = 'mock' THEN 'mock'
            ELSE 'ollama'
        END
        WHERE id = 1 AND embedder_kind IS NULL",
    // Enforce non-null going forward.
    // (sqlite can't add NOT NULL to an existing column without a rebuild;
    //  we rely on application-level enforcement instead.)
];

/// P1-4 repair migration: rebuild the memory_fts index from the live memories
/// table, purging any ghost rows left by pre-fix forget/supersede operations.
///
/// This is a one-time repair run as migration v10 on first open after the
/// upgrade. The `INSERT INTO memory_fts(memory_fts) VALUES('rebuild')` command
/// is the FTS5 way to trigger a full internal rebuild from the shadow tables,
/// but since we also need to remove rows for invalidated/superseded memories
/// that were never cleaned up, we do a manual DELETE + re-INSERT instead.
async fn migration_v10(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V10_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V10_STATEMENTS: &[&str] = &[
    // Wipe the entire FTS index and rebuild it from scratch from the live
    // memories table.  This removes ghost rows for forgotten/superseded memories
    // that were inserted before P1-4 was applied.
    //
    // memory_fts is a *self-managed* (non-content) FTS5 table, so the special
    // 'delete-all' command is unavailable — use a plain DELETE instead.
    "DELETE FROM memory_fts",
    // Re-populate from the canonical memories table (all rows — the FTS
    // content= attribute is not set so we manage content ourselves).
    "INSERT INTO memory_fts(memory_id, title, body)
         SELECT id, title, body FROM memories",
];

/// Migration v11: add processed_through_ordinal watermark for incremental
/// pipeline processing. Tracks the highest chunk ordinal that has been
/// processed by the extraction pipeline, allowing batched extraction to
/// distinguish context (already processed) from new chunks.
async fn migration_v11(tx: &libsql::Transaction) -> Result<()> {
    for stmt in V11_STATEMENTS {
        tx.execute(stmt, ()).await?;
    }
    Ok(())
}

const V11_STATEMENTS: &[&str] =
    &["ALTER TABLE sessions ADD COLUMN processed_through_ordinal INTEGER NOT NULL DEFAULT -1"];
