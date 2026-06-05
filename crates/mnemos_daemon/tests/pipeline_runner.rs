use libsql::params;
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use mnemos_daemon::build_app_full;
use mnemos_daemon::config::{Config, RetentionPolicy};
use mnemos_daemon::events::Event;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

/// Seed a session row + one chunk into `state`'s storage and return the ids.
async fn seed_session_with_chunk(
    state: &mnemos_daemon::state::AppState,
    session_id: &str,
    chunk_id: &str,
    body: &str,
) {
    let (conn, _g) = state.vault.storage().write_conn().await.unwrap();
    conn.execute(
        "INSERT INTO sessions (id, started_at) VALUES (?1, '2026-01-01T00:00:00+00:00')",
        params![session_id.to_string()],
    )
    .await
    .unwrap();
    conn.execute(
        "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
             VALUES (?1, ?2, 'user', 0, ?3, '2026-01-01T00:00:00+00:00')",
        params![
            chunk_id.to_string(),
            session_id.to_string(),
            body.to_string()
        ],
    )
    .await
    .unwrap();
}

/// Count chunk rows belonging to `session_id`.
async fn count_chunks(state: &mnemos_daemon::state::AppState, session_id: &str) -> i64 {
    let conn = state.vault.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM chunks WHERE session_id = ?1",
            params![session_id.to_string()],
        )
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

/// Wait for a `PipelineCompleted` event for `session_id` (timeout 5 s).
async fn wait_for_pipeline(
    rx: &mut tokio::sync::broadcast::Receiver<Event>,
    session_id: &str,
) -> usize {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(Event::PipelineCompleted {
                    session_id: sid,
                    facts_added,
                }) if sid == session_id => return facts_added,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
    })
    .await
    .expect("pipeline completes within 5 s")
}

#[tokio::test]
async fn runner_turns_session_end_into_semantic_memory() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (_app, state, handle, _sync, _bundled) = build_app_full(
        Config::default(),
        vault,
        None,
        Some(Arc::new(MockLlm::new())),
    )
    .await
    .unwrap();
    let handle = handle.expect("runner spawned when llm present");
    let mut rx = state.events.subscribe();

    {
        let (conn, _g) = state.vault.storage().write_conn().await.unwrap();
        conn.execute(
            "INSERT INTO sessions (id, started_at) VALUES ('sess_p', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
                 VALUES ('chunk_p', 'sess_p', 'user', 0, 'FACT: Shaun ships Rust', '2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
    }

    state.events.publish(Event::SessionEnded {
        id: "sess_p".into(),
    });

    let facts_added = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(Event::PipelineCompleted {
                    session_id,
                    facts_added,
                }) if session_id == "sess_p" => return facts_added,
                Ok(_) => continue,
                Err(_) => continue,
            }
        }
    })
    .await
    .expect("pipeline completes within 5s");
    assert!(facts_added >= 1);

    let mems = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Semantic]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(mems.iter().any(|m| m.body == "Shaun ships Rust"));

    // SessionEnded is idempotent: processed_at is stamped.
    let conn = state.vault.storage().conn().unwrap();
    let mut rows = conn
        .query("SELECT processed_at FROM sessions WHERE id = 'sess_p'", ())
        .await
        .unwrap();
    let pa: Option<String> = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert!(pa.is_some());

    handle.shutdown().await;
}

// ── P1-18: retention policy tests ────────────────────────────────────────────

/// Default config uses `distill-and-prune`.  After `PipelineCompleted` the raw
/// chunks must be deleted: `SELECT COUNT(*) FROM chunks WHERE session_id = ?`
/// must return 0.  This is the claimed privacy property.
#[tokio::test]
async fn distill_and_prune_deletes_chunks_after_pipeline() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    // Verify the default retention is DistillAndPrune so the test is testing
    // exactly what the production default delivers.
    let cfg = Config::default();
    assert_eq!(
        cfg.autonomy.retention,
        RetentionPolicy::DistillAndPrune,
        "default retention must be distill-and-prune"
    );

    let (_app, state, handle, _sync, _bundled) =
        build_app_full(cfg, vault, None, Some(Arc::new(MockLlm::new())))
            .await
            .unwrap();
    let handle = handle.expect("runner spawned when llm present");
    let mut rx = state.events.subscribe();

    seed_session_with_chunk(
        &state,
        "sess_prune",
        "chunk_prune",
        "FACT: Shaun ships Rust",
    )
    .await;

    // Pre-condition: the chunk exists before the pipeline runs.
    assert_eq!(
        count_chunks(&state, "sess_prune").await,
        1,
        "chunk must exist before pipeline"
    );

    state.events.publish(Event::SessionEnded {
        id: "sess_prune".into(),
    });

    let _facts = wait_for_pipeline(&mut rx, "sess_prune").await;

    // Post-condition: distill-and-prune must have deleted the raw chunk.
    assert_eq!(
        count_chunks(&state, "sess_prune").await,
        0,
        "distill-and-prune: all raw chunks must be deleted after PipelineCompleted"
    );

    handle.shutdown().await;
}

/// With `retention = keep-raw`, chunks must survive the pipeline intact so
/// users who opt into full retention keep their raw conversation history.
#[tokio::test]
async fn keep_raw_retains_chunks_after_pipeline() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();

    let mut cfg = Config::default();
    cfg.autonomy.retention = RetentionPolicy::KeepRaw;

    let (_app, state, handle, _sync, _bundled) =
        build_app_full(cfg, vault, None, Some(Arc::new(MockLlm::new())))
            .await
            .unwrap();
    let handle = handle.expect("runner spawned when llm present");
    let mut rx = state.events.subscribe();

    seed_session_with_chunk(
        &state,
        "sess_keep",
        "chunk_keep",
        "FACT: Shaun keeps history",
    )
    .await;

    // Pre-condition.
    assert_eq!(count_chunks(&state, "sess_keep").await, 1);

    state.events.publish(Event::SessionEnded {
        id: "sess_keep".into(),
    });

    let _facts = wait_for_pipeline(&mut rx, "sess_keep").await;

    // Post-condition: keep-raw must NOT delete the chunk.
    assert_eq!(
        count_chunks(&state, "sess_keep").await,
        1,
        "keep-raw: raw chunks must NOT be deleted after PipelineCompleted"
    );

    handle.shutdown().await;
}
