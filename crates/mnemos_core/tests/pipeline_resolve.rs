use mnemos_core::paths::Paths;
use mnemos_core::pipeline::resolve::resolve_and_apply;
use mnemos_core::pipeline::{CandidateFact, ResolveOp};
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::types::Provenance;
use mnemos_core::vault::{RememberOpts, Vault};
use tempfile::TempDir;

async fn vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    Vault::open(Paths::with_root(tmp.path())).await.unwrap()
}

fn prov() -> Provenance {
    Provenance {
        session: Some("sess_1".into()),
        chunks: vec!["chunk_1".into()],
    }
}

#[tokio::test]
async fn add_creates_new_memory() {
    let v = vault().await;
    let cand = CandidateFact {
        text: "Shaun loves Rust".into(),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert_eq!(op, ResolveOp::Add);
    let id = new_id.expect("add returns id");
    assert_eq!(v.get(&id).await.unwrap().body, "Shaun loves Rust");
}

#[tokio::test]
async fn update_supersedes_existing() {
    let v = vault().await;
    let old = v
        .remember("Shaun uses vim", RememberOpts::default())
        .await
        .unwrap();
    let cand = CandidateFact {
        text: format!("Shaun now uses Helix OP=update TARGET={old}"),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Update { .. }));
    let new_id = new_id.unwrap();
    // old is invalidated and superseded by the new one
    let old_mem = v.get(&old).await.unwrap();
    assert!(old_mem.invalid_at.is_some());
    assert_eq!(old_mem.superseded_by.as_deref(), Some(new_id.as_str()));
}

#[tokio::test]
async fn delete_invalidates_target() {
    let v = vault().await;
    let target = v
        .remember("temporary fact", RememberOpts::default())
        .await
        .unwrap();
    let cand = CandidateFact {
        text: format!("that is no longer true OP=delete TARGET={target}"),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Delete { .. }));
    assert!(new_id.is_none());
    assert!(v.get(&target).await.unwrap().invalid_at.is_some());
}

#[tokio::test]
async fn noop_creates_nothing() {
    let v = vault().await;
    let before = v.list(ListFilter::default()).await.unwrap().len();
    let cand = CandidateFact {
        text: "already known OP=noop".into(),
    };
    let (op, new_id) = resolve_and_apply(&v, &cand, prov(), &MockLlm::new())
        .await
        .unwrap();
    assert!(matches!(op, ResolveOp::Noop { .. }));
    assert!(new_id.is_none());
    let after = v.list(ListFilter::default()).await.unwrap().len();
    assert_eq!(before, after);
}
