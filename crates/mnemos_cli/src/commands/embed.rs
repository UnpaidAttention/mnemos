use crate::cli::{EmbedAction, EmbedArgs};
use crate::commands::open_vault;
use anyhow::Result;
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool, args: EmbedArgs) -> Result<()> {
    match args.action {
        EmbedAction::Status => status(vault, json).await,
        EmbedAction::Backfill { batch_size } => backfill(vault, json, batch_size).await,
    }
}

async fn status(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let vault = open_vault(vault).await?;
    let conn = vault.storage().conn()?;

    let mut r = conn
        .query("SELECT COUNT(*) FROM memories WHERE invalid_at IS NULL", ())
        .await?;
    let active: i64 = r
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no row returned for active count"))?
        .get(0_i32)?;

    let mut r2 = conn
        .query(
            "SELECT COUNT(*) FROM memories m \
             WHERE m.invalid_at IS NULL \
             AND m.id IN (SELECT memory_id FROM memory_vec)",
            (),
        )
        .await?;
    let embedded: i64 = r2
        .next()
        .await?
        .ok_or_else(|| anyhow::anyhow!("no row returned for embedded count"))?
        .get(0_i32)?;

    let unembedded = active - embedded;
    let dim = vault.embedder().map(|e| e.dim());

    if json {
        println!(
            "{}",
            serde_json::json!({
                "memories_active": active,
                "memories_embedded": embedded,
                "memories_unembedded": unembedded,
                "embedder_dim": dim,
            })
        );
    } else {
        println!("active     : {active}");
        println!("embedded   : {embedded}");
        println!("unembedded : {unembedded}");
        if let Some(d) = dim {
            println!("embedder dim: {d}");
        }
    }
    Ok(())
}

async fn backfill(vault: Option<PathBuf>, json: bool, batch_size: usize) -> Result<()> {
    let vault = open_vault(vault).await?;
    let stats = vault.backfill_embeddings(batch_size).await?;
    if json {
        println!("{}", serde_json::to_string(&stats)?);
    } else {
        println!(
            "backfill complete — embedded: {}  skipped: {}  errors: {}",
            stats.embedded, stats.skipped, stats.errors
        );
    }
    Ok(())
}
