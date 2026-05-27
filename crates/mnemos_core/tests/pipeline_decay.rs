use chrono::{Duration, Utc};
use mnemos_core::paths::Paths;
use mnemos_core::pipeline::decay::{decay_pass, DecayConfig};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use tempfile::TempDir;

async fn backdate_last_accessed(v: &Vault, id: &str, days_ago: i64) {
    let when = (Utc::now() - Duration::days(days_ago)).to_rfc3339();
    let (conn, _g) = v.storage().write_conn().await.unwrap();
    conn.execute(
        "UPDATE memories SET last_accessed = ? WHERE id = ?",
        libsql::params![when, id.to_string()],
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn working_memory_decays_below_floor_and_is_flagged() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "ephemeral working note",
            RememberOpts {
                tier: Tier::Working,
                importance: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;

    let stats = decay_pass(v.storage(), Utc::now(), &DecayConfig::default())
        .await
        .unwrap();
    assert_eq!(stats.scanned, 1);
    assert_eq!(stats.decayed, 1);
    assert!(stats.to_invalidate.contains(&id));
}

#[tokio::test]
async fn run_decay_invalidates_and_persists() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "ephemeral",
            RememberOpts {
                tier: Tier::Working,
                importance: Some(0.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;

    let stats = v.run_decay(&DecayConfig::default()).await.unwrap();
    assert!(stats.to_invalidate.contains(&id));
    assert!(v.get(&id).await.unwrap().invalid_at.is_some());
}

#[tokio::test]
async fn semantic_memory_with_high_importance_survives() {
    let tmp = TempDir::new().unwrap();
    let v = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let id = v
        .remember(
            "durable identity fact",
            RememberOpts {
                tier: Tier::Semantic,
                importance: Some(1.0),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    backdate_last_accessed(&v, &id, 30).await;
    let stats = decay_pass(v.storage(), Utc::now(), &DecayConfig::default())
        .await
        .unwrap();
    assert!(!stats.to_invalidate.contains(&id));
}
