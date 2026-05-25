use crate::cli::RecallArgs;
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: RecallArgs) -> Result<()> {
    anyhow::bail!("recall: not yet implemented")
}
