pub mod doctor;
pub mod forget;
pub mod get;
pub mod list;
pub mod rebuild;
pub mod recall;
pub mod remember;
pub mod status;

use anyhow::Result;
use mnemos_core::{paths::Paths, vault::Vault};
use std::path::PathBuf;

#[allow(dead_code)] // used in Tasks 22-25
pub async fn open_vault(vault_override: Option<PathBuf>) -> Result<Vault> {
    let paths = match vault_override {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    Ok(Vault::open(paths).await?)
}
