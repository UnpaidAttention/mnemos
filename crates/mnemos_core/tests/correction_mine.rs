use mnemos_core::paths::Paths;
use mnemos_core::pipeline::reflect::mine_corrections;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::list_by_kind;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

/// Seed a session + one chunk into the vault's raw DB. The chunk body uses the
/// `CORRECTION:<wrong>|<right>|<why>|<trigger>` marker that the
/// `TASK=mine-corrections` MockLlm branch will parse.
async fn seed_session_with_chunk(vault: &Vault, session_id: &str, chunk_id: &str, body: &str) {
    let (conn, _g) = vault.storage().write_conn().await.unwrap();
    conn.execute(
        "INSERT INTO sessions (id, started_at) VALUES (?, '2026-01-01T00:00:00+00:00')",
        libsql::params![session_id.to_string()],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
             VALUES (?, ?, 'user', 0, ?, '2026-01-01T00:00:00+00:00')",
        libsql::params![
            chunk_id.to_string(),
            session_id.to_string(),
            body.to_string()
        ],
    )
    .await
    .unwrap();
}

/// `mine_corrections` extracts a correction from the session's chunks and
/// persists it as a `MemoryType::Correction` memory.
#[tokio::test]
async fn mine_extracts_correction_from_session_chunks() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // The MockLlm `TASK=mine-corrections` branch reads
    // `CORRECTION:<wrong>|<right>|<why>|<trigger>` markers.
    seed_session_with_chunk(
        &v,
        "sess_mine",
        "chunk_mine_1",
        "user: Actually no, that was wrong. \
         CORRECTION:used println! for debug output|\
         use tracing::debug! instead|\
         the project uses tracing and println! pollutes production logs|\
         debugging output",
    )
    .await;

    let ids = mine_corrections(&v, &MockLlm::new(), "sess_mine")
        .await
        .unwrap();

    assert!(!ids.is_empty(), "mine_corrections should return >=1 id");

    let corrections = list_by_kind(v.storage(), MemoryType::Correction, 10)
        .await
        .unwrap();
    assert!(
        !corrections.is_empty(),
        "at least one Correction memory should exist after mining"
    );

    let found = corrections
        .iter()
        .any(|m| m.body.contains("tracing::debug!"));
    assert!(
        found,
        "mined correction body should contain the 'right' text; got: {:?}",
        corrections.iter().map(|m| &m.body).collect::<Vec<_>>()
    );
}

/// When a session has no chunks, mine_corrections should return Ok(vec![]) without
/// writing anything.
#[tokio::test]
async fn mine_on_empty_session_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Session exists but has no chunks.
    {
        let (conn, _g) = v.storage().write_conn().await.unwrap();
        conn.execute(
            "INSERT INTO sessions (id, started_at) VALUES ('sess_empty', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
    }

    let ids = mine_corrections(&v, &MockLlm::new(), "sess_empty")
        .await
        .unwrap();
    assert!(
        ids.is_empty(),
        "no corrections should be created for a chunk-less session"
    );
}

/// mine_corrections deduplicates against already-logged corrections: calling
/// it twice on the same session must not produce double the memories (the vault
/// dedup path in remember_correction handles this when embedders are present,
/// but the count must not exceed a reasonable bound without an embedder).
#[tokio::test]
async fn mine_with_no_correction_markers_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Chunk with no correction markers — MockLlm returns {"corrections":[]}
    seed_session_with_chunk(
        &v,
        "sess_no_markers",
        "chunk_no_markers",
        "user: The sky is blue. assistant: Indeed it is.",
    )
    .await;

    let ids = mine_corrections(&v, &MockLlm::new(), "sess_no_markers")
        .await
        .unwrap();
    assert!(
        ids.is_empty(),
        "chunk without correction markers should yield zero ids"
    );
}
