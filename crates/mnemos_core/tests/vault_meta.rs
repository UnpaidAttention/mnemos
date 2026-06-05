use mnemos_core::storage::vault_meta::{get_embedder_meta, set_embedder_meta, EmbedderMeta};
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn embedder_meta_round_trip() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("vm.db")).await.unwrap();

    // Fresh vault → defaults from migration.
    let m = get_embedder_meta(&s).await.unwrap();
    assert_eq!(m.kind, "bundled");

    // Atomic swap.
    let new = EmbedderMeta {
        kind: "ollama".into(),
        model: "nomic-embed-text".into(),
        dim: 768,
    };
    set_embedder_meta(&s, &new).await.unwrap();
    let read = get_embedder_meta(&s).await.unwrap();
    assert_eq!(read.kind, "ollama");
    assert_eq!(read.model, "nomic-embed-text");
    assert_eq!(read.dim, 768);
}

/// P2-16: set_embedder_meta on a properly-initialized vault must succeed (the
/// vault_meta row is created by Storage::open migrations).
#[tokio::test]
async fn set_embedder_meta_succeeds_on_initialized_vault() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("p216.db")).await.unwrap();
    let meta = EmbedderMeta {
        kind: "mock".into(),
        model: "test-model".into(),
        dim: 64,
    };
    // Must not error — the row exists after Storage::open.
    set_embedder_meta(&s, &meta).await.unwrap();
    let read = get_embedder_meta(&s).await.unwrap();
    assert_eq!(read.kind, "mock");
    assert_eq!(read.dim, 64);
}

/// P2-16: set_vault_meta on a properly-initialized vault must succeed.
#[tokio::test]
async fn set_vault_meta_succeeds_on_initialized_vault() {
    let tmp = TempDir::new().unwrap();
    let s = Storage::open(&tmp.path().join("p216b.db")).await.unwrap();
    // Must not error.
    s.set_vault_meta(128, "some-model").await.unwrap();
    let read = s.get_vault_meta().await.unwrap();
    assert_eq!(read.embedder_dim, Some(128));
    assert_eq!(read.embedder_model_id.as_deref(), Some("some-model"));
}
