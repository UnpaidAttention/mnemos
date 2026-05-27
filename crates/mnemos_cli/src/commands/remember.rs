use crate::cli::RememberArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::types::MemoryType;
use mnemos_core::vault::RememberOpts;
use mnemos_core::Tier;
use std::io::Read;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: RememberArgs) -> Result<()> {
    let body = match args.body {
        Some(b) if !b.is_empty() => b,
        _ => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("read stdin")?;
            buf.trim().to_string()
        }
    };
    if body.is_empty() {
        anyhow::bail!("empty body — pass text as argument or via stdin");
    }
    let tier = Tier::from_str(&args.tier).context("invalid --tier")?;
    let vault = open_vault(vault).await?;
    let id = vault
        .remember(
            &body,
            RememberOpts {
                title: args.title,
                tier,
                kind: MemoryType::Fact,
                tags: args.tags,
                importance: args.importance,
                workspace: args.workspace,
                source_tool: args.source_tool,
                provenance: vec![],
            },
        )
        .await?;
    if json {
        println!("{}", serde_json::json!({"id": id}));
    } else {
        println!("{id}");
    }
    Ok(())
}
