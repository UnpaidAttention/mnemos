use mnemos_core::doctor::{diagnose, DriftKind};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::{paths::Paths, Tier};
use tempfile::TempDir;

#[tokio::test]
async fn doctor_clean_vault_returns_no_issues() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();
    let _ = vault
        .remember(
            "clean body",
            RememberOpts {
                title: Some("ok".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(
        report.issues.is_empty(),
        "expected no issues, got {:?}",
        report.issues
    );
}

#[tokio::test]
async fn doctor_detects_orphaned_file() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let _vault = Vault::open(paths.clone()).await.unwrap();

    // Write a stray file directly that the DB doesn't know about
    let stray = paths.tier_dir(Tier::Semantic).join("mem_01HXSTRAY.md");
    tokio::fs::write(
        &stray,
        "---\nid: mem_01HXSTRAY\ntier: semantic\ntype: fact\ntitle: orphan\n\
         created_at: 2026-05-22T14:30:00Z\ningested_at: 2026-05-22T14:30:00Z\n\
         valid_at: 2026-05-22T14:30:00Z\nstrength: 1.0\nimportance: 0.5\n\
         last_accessed: 2026-05-22T14:30:00Z\naccess_count: 0\n---\n\nbody\n",
    )
    .await
    .unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| matches!(i.kind, DriftKind::FileNotInDb)),
        "expected a FileNotInDb issue"
    );
}

#[tokio::test]
async fn doctor_detects_db_row_missing_file() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths.clone()).await.unwrap();
    let id = vault
        .remember(
            "body",
            RememberOpts {
                title: Some("ok".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let path = paths.tier_dir(Tier::Semantic).join(format!("{id}.md"));
    tokio::fs::remove_file(&path).await.unwrap();

    let report = diagnose(&paths).await.unwrap();
    assert!(
        report
            .issues
            .iter()
            .any(|i| matches!(i.kind, DriftKind::DbRowNoFile)),
        "expected a DbRowNoFile issue"
    );
}
