use crate::cli::ListArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::storage::memory_ops::ListFilter;
use mnemos_core::Tier;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: ListArgs) -> Result<()> {
    let tiers = if args.tier.is_empty() {
        None
    } else {
        let mut v = Vec::with_capacity(args.tier.len());
        for t in &args.tier {
            v.push(Tier::from_str(t).context("invalid tier")?);
        }
        Some(v)
    };
    let vault = open_vault(vault).await?;
    let memories = vault
        .list(ListFilter {
            tiers,
            workspace: args.workspace,
            include_invalid: args.include_invalid,
            limit: Some(args.limit),
            ..Default::default()
        })
        .await?;
    if json {
        println!("{}", serde_json::json!({"memories": memories}));
    } else {
        if memories.is_empty() {
            println!("no memories");
            return Ok(());
        }
        for m in memories {
            let inv = if m.invalid_at.is_some() {
                " [invalidated]"
            } else {
                ""
            };
            println!("{}  {:10}  {}{}", m.id, m.tier, m.title, inv);
        }
    }
    Ok(())
}
