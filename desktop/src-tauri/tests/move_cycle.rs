// Integration: config write + directory move behave together as move_vault expects.
use std::path::Path;

#[path = "../src/config_io.rs"]
mod config_io;
#[path = "../src/vault_move.rs"]
mod vault_move;

#[test]
fn config_and_move_compose() {
    let root = tempfile::tempdir().unwrap();
    let cfg = root.path().join("config.toml");
    let src = root.path().join("vault");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(src.join("m.md"), b"x").unwrap();
    config_io::write_vault_root(&cfg, &src).unwrap();

    let dst = root.path().join("new");
    vault_move::validate(&src, &dst).unwrap();
    config_io::write_vault_root(&cfg, &dst).unwrap();
    vault_move::execute(&src, &dst).unwrap();

    assert_eq!(config_io::read_vault_root(&cfg).unwrap().unwrap(), dst);
    assert!(dst.join("m.md").exists());
    assert!(!src.exists());
}
