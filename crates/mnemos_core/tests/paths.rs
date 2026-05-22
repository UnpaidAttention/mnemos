use mnemos_core::paths::Paths;
use tempfile::TempDir;

#[test]
fn paths_with_override_uses_given_root() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    assert_eq!(paths.root, tmp.path());
    assert_eq!(paths.files_dir, tmp.path().join("files"));
    assert_eq!(paths.db_path, tmp.path().join("index.db"));
    assert_eq!(
        paths.tier_dir(mnemos_core::Tier::Working),
        tmp.path().join("files/working")
    );
}

#[test]
fn paths_ensure_dirs_creates_all_tier_dirs() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    paths.ensure_dirs().unwrap();
    for tier in mnemos_core::Tier::all() {
        assert!(paths.tier_dir(*tier).is_dir(), "{} dir missing", tier);
    }
    assert!(paths.quarantine_dir.is_dir());
    assert!(paths.archived_dir.is_dir());
    assert!(paths.entities_dir.is_dir());
}
