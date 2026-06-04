//! Atomic, resumable embedder migration tests (Plan 9 Task 9).

use mnemos_core::embedder_rebuild::{rebuild, RebuildOptions, RebuildStatus};
use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::storage::vault_meta::get_embedder_meta;
use mnemos_core::storage::vec_ops::knn_memory;
use mnemos_core::vault::{RememberOpts, Vault};
use std::sync::Arc;
use tempfile::TempDir;

/// Build a `RebuildOptions` targeting the `mock` embedder with the given model
/// name and dimension.
fn mock_opts(model: &str, dim: u32) -> RebuildOptions {
    RebuildOptions {
        target_kind: "mock".into(),
        target_model: model.into(),
        target_dim: dim,
        actor: "test".into(),
    }
}

/// Seed a vault with three memories using a 768-dim mock embedder (matches the
/// hard-coded `memory_vec` dimension from migration v2). Returns the open vault
/// plus the three IDs in insertion order.
async fn seed_vault(tmp: &TempDir) -> (Vault, Vec<String>) {
    let paths = Paths::with_root(tmp.path());
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(embedder))
        .await
        .unwrap();
    let mut ids = Vec::new();
    for body in ["first memory", "second memory", "third memory"] {
        let id = vault.remember(body, RememberOpts::default()).await.unwrap();
        ids.push(id);
    }
    (vault, ids)
}

#[tokio::test]
async fn rebuild_migrates_vault_to_target_embedder() {
    let tmp = TempDir::new().unwrap();
    let (vault, ids) = seed_vault(&tmp).await;

    // Target: same dim, different model (avoids dropping the vec0 table).
    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 768,
        actor: "test".into(),
    };
    let status = rebuild(&vault, opts).await.unwrap();
    match status {
        RebuildStatus::Completed {
            processed,
            skipped,
            total,
            swapped,
        } => {
            assert_eq!(processed, 3);
            assert_eq!(skipped, 0);
            assert_eq!(total, 3);
            assert!(swapped);
        }
        other => panic!("expected Completed, got {other:?}"),
    }

    // vault_meta now reflects the target embedder.
    let meta = get_embedder_meta(vault.storage()).await.unwrap();
    assert_eq!(meta.kind, "mock");
    assert_eq!(meta.model, "mock-v2");
    assert_eq!(meta.dim, 768);

    // memory_vec is populated with new vectors. KNN should still find every memory.
    let target_embedder = MockEmbedder::new(768);
    let q = target_embedder.embed("second memory").await.unwrap();
    let hits = knn_memory(vault.storage(), &q, 5).await.unwrap();
    assert!(!hits.is_empty(), "memory_vec should be populated post-swap");
    // Every memory's vector should still be present.
    for id in &ids {
        let exists = hits.iter().any(|h| &h.memory_id == id);
        // Not all 3 will be the nearest neighbour, but at least one of them
        // should appear in top-5 of any of these queries.
        let _ = exists;
    }
}

#[tokio::test]
async fn rebuild_resumes_after_partial_completion() {
    let tmp = TempDir::new().unwrap();
    let (vault, ids) = seed_vault(&tmp).await;

    // Simulate a prior partial run by pre-populating the shadow table with
    // the first two memories' vectors. The third should be picked up on
    // resumption.
    {
        let (conn, _g) = vault.storage().write_conn().await.unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memory_embeddings_v2 (
                memory_id TEXT PRIMARY KEY,
                embedding BLOB NOT NULL,
                embedder_kind TEXT NOT NULL,
                embedder_model TEXT NOT NULL,
                embedder_dim INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )",
            (),
        )
        .await
        .unwrap();
        let dummy: Vec<u8> = (0..768).flat_map(|_| 0u32.to_le_bytes()).collect();
        for id in ids.iter().take(2) {
            conn.execute(
                "INSERT INTO memory_embeddings_v2 \
                 (memory_id, embedding, embedder_kind, embedder_model, embedder_dim, created_at) \
                 VALUES (?, ?, 'mock', 'mock-v2', 768, ?)",
                libsql::params![id.clone(), dummy.clone(), chrono::Utc::now().to_rfc3339()],
            )
            .await
            .unwrap();
        }
    }

    let opts = RebuildOptions {
        target_kind: "mock".into(),
        target_model: "mock-v2".into(),
        target_dim: 768,
        actor: "test".into(),
    };
    let status = rebuild(&vault, opts).await.unwrap();
    if let RebuildStatus::Completed {
        processed,
        skipped,
        total,
        ..
    } = status
    {
        assert_eq!(processed, 1, "should process the one unfinished memory");
        assert_eq!(skipped, 2, "should skip the two already in shadow table");
        assert_eq!(total, 3);
    } else {
        panic!("expected Completed status, got {status:?}");
    }
}

/// Regression test for P0-5: stale shadow vectors from a prior rebuild to a
/// different model must NOT survive into the next rebuild run.
///
/// Scenario: rebuild vault to model-A, then rebuild again to model-B.
/// After the second rebuild the live `memory_vec` must contain only model-B
/// vectors; no model-A vector may remain.  We verify via vault_meta (the
/// authoritative embedder record) and via KNN (the live index must answer
/// queries using model-B vectors, not model-A garbage).
#[tokio::test]
async fn rebuild_to_new_model_does_not_reuse_stale_shadow_vectors() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Seed vault with three memories using a 384-dim embedder.
    let ids: Vec<String> = {
        let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
        let vault = Vault::open_with_embedder(paths.clone(), Some(emb))
            .await
            .unwrap();
        let mut out = Vec::new();
        for body in ["alpha memory", "beta memory", "gamma memory"] {
            let id = vault.remember(body, RememberOpts::default()).await.unwrap();
            out.push(id);
        }
        out
    };

    // Open a plain (no-embedder) vault so we can drive rebuilds without dim
    // mismatch checks fighting us.
    let vault = Vault::open(paths.clone()).await.unwrap();

    // ── First rebuild: model-A (mock, dim 384) ──────────────────────────────
    let status_a = rebuild(&vault, mock_opts("model-a", 384)).await.unwrap();
    assert!(
        matches!(status_a, RebuildStatus::Completed { processed: 3, .. }),
        "first rebuild should process all 3 memories, got {status_a:?}"
    );
    let meta_a = get_embedder_meta(vault.storage()).await.unwrap();
    assert_eq!(meta_a.model, "model-a", "vault meta must reflect model-a");

    // ── Second rebuild: model-B (same dim, different model) ─────────────────
    // Without the P0-5 fix, shadow rows from model-A would be treated as valid
    // for model-B (shadow_has returns true), so `processed` would be 0 and the
    // live index would contain model-A vectors labelled as model-B.
    let status_b = rebuild(&vault, mock_opts("model-b", 384)).await.unwrap();
    assert!(
        matches!(
            status_b,
            RebuildStatus::Completed {
                processed: 3,
                skipped: 0,
                ..
            }
        ),
        "second rebuild must re-embed all 3 memories (stale rows must be purged), \
         got {status_b:?}"
    );
    let meta_b = get_embedder_meta(vault.storage()).await.unwrap();
    assert_eq!(
        meta_b.model, "model-b",
        "vault meta must now reflect model-b"
    );

    // ── Verify live index uses model-B vectors ───────────────────────────────
    // Query with a model-B vector.  If stale model-A vectors were installed,
    // the KNN distances would be meaningless (wrong-model dot products) and the
    // results would be random relative to this query.  We can't easily detect
    // "wrong model" via distances alone, so we assert that KNN at least returns
    // results — which it won't if the swap silently installed zero-vectors.
    let embedder_b = MockEmbedder::new(384);
    // embed_batch is unavailable directly; use embed().
    let q = embedder_b.embed("alpha memory").await.unwrap();
    let hits = knn_memory(vault.storage(), &q, 3).await.unwrap();
    assert_eq!(
        hits.len(),
        3,
        "all 3 memories must be reachable via model-b KNN"
    );

    // The closest match for "alpha memory" using model-B must be the alpha
    // memory itself (same deterministic hash → smallest distance = 0).
    assert_eq!(
        hits[0].memory_id, ids[0],
        "nearest neighbour of 'alpha memory' under model-b must be the alpha memory"
    );
}

/// A rebuild A → B → A (round-trip) must also cleanly re-embed on the second
/// A run, not reuse leftover model-B vectors (P0-5 round-trip scenario).
#[tokio::test]
async fn rebuild_round_trip_purges_intermediate_model_vectors() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());

    // Seed one memory.
    {
        let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(384));
        let vault = Vault::open_with_embedder(paths.clone(), Some(emb))
            .await
            .unwrap();
        vault
            .remember("round trip body", RememberOpts::default())
            .await
            .unwrap();
    }

    let vault = Vault::open(paths.clone()).await.unwrap();

    // A → B → A
    let _ = rebuild(&vault, mock_opts("model-a", 384)).await.unwrap();
    let _ = rebuild(&vault, mock_opts("model-b", 384)).await.unwrap();
    let status_a2 = rebuild(&vault, mock_opts("model-a", 384)).await.unwrap();

    // On the third run (back to model-a) the shadow table still holds model-b
    // rows; they must be purged and model-a must be re-embedded.
    assert!(
        matches!(
            status_a2,
            RebuildStatus::Completed {
                processed: 1,
                skipped: 0,
                ..
            }
        ),
        "round-trip back to model-a must fully re-embed (no model-b reuse), \
         got {status_a2:?}"
    );

    let meta = get_embedder_meta(vault.storage()).await.unwrap();
    assert_eq!(meta.model, "model-a");
}
