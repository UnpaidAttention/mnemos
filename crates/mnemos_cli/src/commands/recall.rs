use crate::cli::RecallArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::retrieval::{bm25::bm25_recall, RecallOpts};
use mnemos_core::Tier;
use std::path::PathBuf;
use std::str::FromStr;

pub async fn run(vault: Option<PathBuf>, json: bool, args: RecallArgs) -> Result<()> {
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
    let opts = RecallOpts {
        k: args.k,
        tiers,
        workspace: args.workspace,
        include_invalid: args.include_invalid,
        ..Default::default()
    };
    let hits = bm25_recall(vault.storage(), &args.query, opts).await?;
    if json {
        println!("{}", serde_json::json!({"hits": hits}));
    } else {
        if hits.is_empty() {
            println!("no matches");
            return Ok(());
        }
        for (i, hit) in hits.iter().enumerate() {
            println!(
                "{:>2}. [{:.3}] {}  ({})",
                i + 1,
                hit.score,
                hit.memory.title,
                hit.memory.id
            );
            let snippet: String = hit.memory.body.chars().take(140).collect();
            println!(
                "    {snippet}{}",
                if hit.memory.body.chars().count() > 140 {
                    "…"
                } else {
                    ""
                }
            );
        }
    }
    Ok(())
}
