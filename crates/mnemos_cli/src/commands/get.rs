use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _id: String) -> Result<()> {
    anyhow::bail!("get: not yet implemented")
}
