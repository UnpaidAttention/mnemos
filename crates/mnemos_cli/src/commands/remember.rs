use crate::cli::RememberArgs;
use anyhow::Result;
use std::path::PathBuf;
pub async fn run(_vault: Option<PathBuf>, _json: bool, _args: RememberArgs) -> Result<()> {
    anyhow::bail!("remember: not yet implemented")
}
