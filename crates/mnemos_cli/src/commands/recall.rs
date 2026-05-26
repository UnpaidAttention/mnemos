use crate::cli::RecallArgs;
use crate::commands::open_vault;
use anyhow::{Context, Result};
use mnemos_core::providers::Embedder;
use mnemos_core::retrieval::{hybrid::hybrid_recall, RecallOpts};
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
        explain: args.explain,
        rerank: args.rerank,
        ..Default::default()
    };

    // --rerank flag is collected into opts but Plan 2's CLI does not wire
    // a Reranker through. The hook lives in hybrid_recall_with_rerank;
    // Plan 3 (daemon) will surface reranker configuration via config.toml
    // and route here. For now, warn the user so the flag isn't silently dead.
    if args.rerank {
        eprintln!("warning: --rerank has no effect in v0.1.0 (no reranker configured); will be wired by the daemon in Plan 3");
    }

    // Coerce Option<&Arc<dyn Embedder>> → Option<&dyn Embedder> for the hybrid call.
    let emb_arc = vault.embedder().cloned();
    let embedder: Option<&dyn Embedder> = emb_arc.as_ref().map(|a| a.as_ref() as &dyn Embedder);
    let hits = hybrid_recall(vault.storage(), embedder, &args.query, opts).await?;

    if json {
        println!("{}", serde_json::json!({"hits": hits}));
    } else if hits.is_empty() {
        println!("no matches");
    } else {
        for (i, hit) in hits.iter().enumerate() {
            println!(
                "{:>2}. [{:.4}] {}  ({})",
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
            if let Some(e) = &hit.explain {
                println!(
                    "    bm25_rank={:?} dense_rank={:?} rrf={:.4} weights[r={:.2} i={:.2} s={:.2} t={:.2}]",
                    e.bm25_rank,
                    e.dense_rank,
                    e.rrf_score,
                    e.weight_recency,
                    e.weight_importance,
                    e.weight_strength,
                    e.weight_tier,
                );
            }
        }
    }
    Ok(())
}
