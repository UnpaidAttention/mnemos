use anyhow::Result;
use mnemos_core::{paths::Paths, rebuild::rebuild_index};
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let paths = match vault {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    let stats = rebuild_index(&paths).await?;
    if json {
        println!(
            "{}",
            serde_json::json!({
                "indexed": stats.memories_indexed,
                "errors": stats.errors,
                "error_paths": stats.error_paths,
            })
        );
    } else {
        println!(
            "rebuild complete — indexed: {}  errors: {}",
            stats.memories_indexed, stats.errors
        );
        for p in stats.error_paths {
            eprintln!("  ERR {}", p.display());
        }
    }
    Ok(())
}
