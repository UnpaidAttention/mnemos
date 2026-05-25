use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool) -> Result<()> {
    anyhow::bail!("rebuild: not yet implemented")
}
