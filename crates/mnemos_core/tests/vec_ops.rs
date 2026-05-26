use mnemos_core::storage::vec_ops::{delete_memory_vec, insert_memory_vec, knn_memory};
use mnemos_core::Storage;
use tempfile::TempDir;

fn vec(seed: f32, n: usize) -> Vec<f32> {
    (0..n).map(|i| (i as f32 * 0.001) + seed).collect()
}

#[tokio::test]
async fn insert_and_knn_returns_nearest_first() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v.db")).await.unwrap();

    insert_memory_vec(&storage, "mem_A", &vec(0.0, 768))
        .await
        .unwrap();
    insert_memory_vec(&storage, "mem_B", &vec(0.5, 768))
        .await
        .unwrap();
    insert_memory_vec(&storage, "mem_C", &vec(1.0, 768))
        .await
        .unwrap();

    let query = vec(0.001, 768);
    let hits = knn_memory(&storage, &query, 3).await.unwrap();
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].memory_id, "mem_A", "A should be nearest");
    assert!(hits[0].distance <= hits[1].distance);
    assert!(hits[1].distance <= hits[2].distance);
}

#[tokio::test]
async fn delete_memory_vec_removes_from_index() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v.db")).await.unwrap();
    insert_memory_vec(&storage, "mem_X", &vec(0.0, 768))
        .await
        .unwrap();
    delete_memory_vec(&storage, "mem_X").await.unwrap();
    let hits = knn_memory(&storage, &vec(0.0, 768), 5).await.unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn knn_returns_at_most_k() {
    let tmp = TempDir::new().unwrap();
    let storage = Storage::open(&tmp.path().join("v.db")).await.unwrap();
    for i in 0..10 {
        insert_memory_vec(&storage, &format!("mem_{i}"), &vec(i as f32 * 0.01, 768))
            .await
            .unwrap();
    }
    let hits = knn_memory(&storage, &vec(0.0, 768), 3).await.unwrap();
    assert_eq!(hits.len(), 3);
}
