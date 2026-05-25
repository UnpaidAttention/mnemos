use anyhow::Result;
use mnemos_core::{doctor::diagnose, paths::Paths};
use std::path::PathBuf;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let paths = match vault {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };
    let report = diagnose(&paths).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!(
            "files scanned: {}\nindexed memories: {}",
            report.files_scanned, report.db_rows
        );
        if report.issues.is_empty() {
            println!("no issues");
        } else {
            println!("{} issue(s):", report.issues.len());
            for issue in report.issues {
                println!(
                    "  [{:?}] {} {}",
                    issue.kind,
                    issue
                        .path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    issue.detail
                );
            }
        }
    }
    Ok(())
}
