use crate::cli::ListArgs;
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: ListArgs) -> Result<()> {
    anyhow::bail!("list: not yet implemented")
}
