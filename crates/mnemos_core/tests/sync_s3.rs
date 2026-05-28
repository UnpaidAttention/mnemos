use mnemos_core::storage::Storage;
use mnemos_core::sync::s3::S3Sync;
use mnemos_core::sync::SyncBackend;
use tempfile::TempDir;

#[tokio::test]
async fn s3_status_reports_rclone_presence() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join(".mnemos.db")).await.unwrap();
    let backend = S3Sync::new(storage, "missing-remote:bucket/path".into());
    let s = backend.status().await.unwrap();
    assert_eq!(s.backend, "s3");
    assert_eq!(s.ready, which::which("rclone").is_ok());
}

#[tokio::test]
#[ignore = "needs a configured rclone remote"]
async fn s3_push_pull_live() {
    // Run manually with a real `rclone` remote configured.
}
