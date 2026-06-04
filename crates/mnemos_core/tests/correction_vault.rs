use mnemos_core::correction::Correction;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_core::{MemoryType, Tier};
use tempfile::TempDir;

fn corr(wrong: &str, right: &str, why: &str, trig: &str) -> Correction {
    Correction {
        wrong: wrong.into(),
        right: right.into(),
        why: why.into(),
        trigger: Some(trig.into()),
    }
}

#[tokio::test]
async fn remember_correction_creates_procedural_memory() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember_correction(
            corr(
                "used Go",
                "use Rust",
                "the project is Rust-only",
                "language choice",
            ),
            None,
        )
        .await
        .unwrap();
    let m = v.get(&id).await.unwrap();
    assert_eq!(m.tier, Tier::Procedural);
    assert_eq!(m.kind, MemoryType::Correction);
    assert!(m.body.contains("**Why:** the project is Rust-only"));
    assert!(m.tags.contains(&"language".to_string()));
}

#[tokio::test]
async fn missing_why_is_rejected() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let err = v.remember_correction(corr("x", "y", "", "ctx"), None).await;
    assert!(err.is_err());
}
