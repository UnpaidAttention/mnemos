use crate::commands::open_vault;
use anyhow::Result;
use mnemos_core::pipeline::decay::DecayConfig;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let vault = open_vault(vault).await?;
    let stats = vault.run_decay(&DecayConfig::default()).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "scanned": stats.scanned,
                "decayed": stats.decayed,
                "invalidated": stats.to_invalidate.len(),
            })
        );
    } else {
        println!(
            "decay pass — scanned: {}  decayed: {}  invalidated: {}",
            stats.scanned,
            stats.decayed,
            stats.to_invalidate.len()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn decay_runs_on_empty_vault() {
        std::env::set_var("MNEMOS_EMBEDDER", "none");
        let tmp = TempDir::new().unwrap();
        run(Some(tmp.path().to_path_buf()), true).await.unwrap();
    }
}
