use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::vault::Vault;
use mnemos_core::Tier;
use mnemos_daemon::build_app_full;
use mnemos_daemon::config::Config;
use mnemos_daemon::events::Event;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn runner_turns_session_end_into_semantic_memory() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (_app, state, handle, _sync) = build_app_full(
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
