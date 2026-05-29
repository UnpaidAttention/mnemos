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
async fn session_pipeline_triggers_reflection_at_threshold() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // Low threshold so a single fact triggers reflection.
    let mut cfg = Config::default();
    cfg.reflection.salience_threshold = 1.0;
    let (_app, state, handle, _sync, _bundled) =
        build_app_full(cfg, vault, None, Some(Arc::new(MockLlm::new())))
            .await
            .unwrap();
    let handle = handle.unwrap();
    let mut rx = state.events.subscribe();

    // Seed a session whose chunk extracts a fact that ALSO carries a REFLECT marker,
    // so the reflection pass produces a reflection from the new semantic memory.
    {
        let (conn, _g) = state.vault.storage().write_conn().await.unwrap();
        conn.execute(
            "INSERT INTO sessions (id, started_at) VALUES ('sess_r','2026-01-01T00:00:00+00:00')",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, session_id, speaker, ordinal, body, created_at)
                 VALUES ('chunk_r','sess_r','user',0,'FACT: REFLECT:insight|Shaun ships Rust daily','2026-01-01T00:00:00+00:00')",
            (),
        ).await.unwrap();
    }
    state.events.publish(Event::SessionEnded {
        id: "sess_r".into(),
    });

    let got = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::ReflectionCompleted {
                reflections_created,
            }) = rx.recv().await
            {
                return reflections_created;
            }
        }
    })
    .await
    .expect("reflection completes within 5s");
    assert!(got >= 1);

    let refl = state
        .vault
        .list(ListFilter {
            tiers: Some(vec![Tier::Reflection]),
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(!refl.is_empty());

    handle.shutdown().await;
}
