//! Schema migrations.
//!
//! Each version gate is cumulative: opening a fresh database runs all
//! migrations in order; re-opening an existing database is a no-op if it is
//! already at the latest version.

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

        let mut rows = conn
            .query("SELECT MAX(version) FROM schema_migrations", ())
            .await?;
        let current: i64 = rows
            .next()
            .await?
            .and_then(|r| r.get::<i64>(0).ok())
            .unwrap_or(0);
        drop(rows);

        if current < 1 {
            migration_v1(&conn).await?;
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations (version) VALUES (1)",
                (),
            )
            .await?;
        }
        Ok(())
    }
}

async fn migration_v1(conn: &libsql::Connection) -> Result<()> {
    for stmt in V1_STATEMENTS {
        conn.execute(stmt, ()).await?;
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
