use anyhow::Result;
use std::path::PathBuf;
pub async fn run(
    _vault: Option<PathBuf>,
    _json: bool,
    _id: String,
    _reason: Option<String>,
) -> Result<()> {
    anyhow::bail!("forget: not yet implemented")
}
