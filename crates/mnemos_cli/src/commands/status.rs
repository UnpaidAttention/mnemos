use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::storage::memory_ops::ListFilter;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let vault = open_vault(vault).await?;
    let active = vault
        .list(ListFilter {
            include_invalid: false,
            ..Default::default()
        })
        .await?;
    let all = vault
        .list(ListFilter {
            include_invalid: true,
            ..Default::default()
        })
        .await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "memories_active": active.len(),
                "memories_total":  all.len(),
                "vault_root":      vault.paths().root,
            })
        );
    } else {
        println!("vault:    {}", vault.paths().root.display());
        println!("memories: {} active / {} total", active.len(), all.len());
    }
    Ok(())
}
