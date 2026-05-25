use crate::commands::open_vault;
use anyhow::Result;
use std::path::PathBuf;

pub async fn run(
    vault: Option<PathBuf>,
    json: bool,
    id: String,
    reason: Option<String>,
) -> Result<()> {
    let vault = open_vault(vault).await?;
    vault.forget(&id, reason.as_deref()).await?;
    if json {
        println!("{}", serde_json::json!({"id": id, "status": "invalidated"}));
    } else {
        println!("invalidated {id}");
    }
    Ok(())
}
