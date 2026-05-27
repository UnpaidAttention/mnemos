use chrono::Utc;
use mnemos_core::storage::reflection_ops::{bump_salience, get_salience, reset_salience};
use mnemos_core::storage::Storage;
use tempfile::TempDir;

#[tokio::test]
async fn salience_accumulates_and_resets() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("s.db")).await.unwrap();
    assert!(storage.schema_version().await.unwrap() >= 5);

    assert_eq!(get_salience(&storage).await.unwrap(), 0.0);
    let after = bump_salience(&storage, 3.0).await.unwrap();
    assert_eq!(after, 3.0);
    let after2 = bump_salience(&storage, 2.5).await.unwrap();
    assert_eq!(after2, 5.5);
    reset_salience(&storage, Utc::now()).await.unwrap();
    assert_eq!(get_salience(&storage).await.unwrap(), 0.0);
}
