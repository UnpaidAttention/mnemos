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
