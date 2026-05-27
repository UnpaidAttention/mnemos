use chrono::Utc;
use mnemos_core::paths::Paths;
use mnemos_core::storage::memory_ops::{
    add_memory_link, list_by_kind, mark_reflected, recent_unreflected,
};
use mnemos_core::types::MemoryType;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

#[tokio::test]
async fn unreflected_query_and_mark() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = v.remember("fact a", RememberOpts::default()).await.unwrap();
    let _b = v.remember("fact b", RememberOpts::default()).await.unwrap();

    let pending = recent_unreflected(v.storage(), 10).await.unwrap();
    assert_eq!(pending.len(), 2);

    mark_reflected(v.storage(), std::slice::from_ref(&a), Utc::now())
        .await
        .unwrap();
    let pending2 = recent_unreflected(v.storage(), 10).await.unwrap();
    assert_eq!(pending2.len(), 1);
    assert!(pending2.iter().all(|m| m.id != a));
}

#[tokio::test]
async fn remember_reflection_links_sources() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let src = v
        .remember("source fact", RememberOpts::default())
        .await
        .unwrap();

    let refl = v
        .remember_reflection(
            "Shaun prefers Rust",
            Some("Reflection (preference)".into()),
            MemoryType::Reflection,
            vec!["preference".into()],
            std::slice::from_ref(&src),
            vec![],
        )
        .await
        .unwrap();

    let mem = v.get(&refl).await.unwrap();
    assert_eq!(mem.tier, Tier::Reflection);
    assert_eq!(mem.kind, MemoryType::Reflection);

    // reflects_on link exists
    let conn = v.storage().conn().unwrap();
    let mut rows = conn
        .query(
            "SELECT COUNT(*) FROM memory_links WHERE source_id = ? AND target_id = ? AND kind = 'reflects_on'",
            libsql::params![refl.clone(), src.clone()],
        )
        .await
        .unwrap();
    let n: i64 = rows.next().await.unwrap().unwrap().get(0).unwrap();
    assert_eq!(n, 1);

    // direct add_memory_link + list_by_kind smoke
    add_memory_link(v.storage(), &refl, &src, "related")
        .await
        .unwrap();
    let reflections = list_by_kind(v.storage(), MemoryType::Reflection, 10)
        .await
        .unwrap();
    assert_eq!(reflections.len(), 1);
}
