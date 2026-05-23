use mnemos_core::file_io::{content_hash, read_memory_file, write_memory_file};
use mnemos_core::types::{Memory, MemoryType};
use mnemos_core::{id::new_memory_id, paths::Paths, Tier};
use tempfile::TempDir;

#[tokio::test]
async fn write_then_read_roundtrips() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Round trip test".into(),
        "body text here".into(),
    );
    let file_path = write_memory_file(&paths, &mem).await.unwrap();
    assert!(file_path.exists());

    let (loaded, body) = read_memory_file(&file_path).await.unwrap();
    assert_eq!(loaded.id, mem.id);
    assert_eq!(loaded.title, mem.title);
    assert_eq!(body.trim(), "body text here");
}

#[tokio::test]
async fn atomic_write_uses_temp_then_rename() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();

    let mem = Memory::new_now(
        new_memory_id(),
        Tier::Semantic,
        MemoryType::Fact,
        "Atomic".into(),
        "x".into(),
    );
    let path = write_memory_file(&paths, &mem).await.unwrap();
    // After write, no stray .tmp files
    let tier_dir = paths.tier_dir(Tier::Semantic);
    let mut tmp_files = 0;
    let mut dir = tokio::fs::read_dir(&tier_dir).await.unwrap();
    while let Some(e) = dir.next_entry().await.unwrap() {
        if e.path().extension().and_then(|s| s.to_str()) == Some("tmp") {
            tmp_files += 1;
        }
    }
    assert_eq!(tmp_files, 0);
    assert!(path.starts_with(&tier_dir));
}

#[test]
fn content_hash_is_stable_and_collision_resistant() {
    let h1 = content_hash("abc");
    let h2 = content_hash("abc");
    let h3 = content_hash("abd");
    assert_eq!(h1, h2);
    assert_ne!(h1, h3);
    assert_eq!(h1.len(), 64); // hex sha256
}
