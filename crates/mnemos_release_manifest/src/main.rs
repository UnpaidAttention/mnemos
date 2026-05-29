//! Generates the `latest.json` manifest Tauri's updater polls.
//!
//! Invoked from CI after the bundle matrix produces signed artifacts:
//!   mnemos-release-manifest \
//!     --version 0.7.0 \
//!     --notes "See CHANGELOG.md" \
//!     --pub-date 2026-05-28T13:00:00Z \
//!     --platform darwin-x86_64 \
//!     --url https://github.com/.../Mnemos_0.7.0_x64.app.tar.gz \
//!     --signature "dW50cnVz..." \
//!     [--platform linux-x86_64 --url ... --signature ...]... \
//!     --output latest.json

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use clap::Parser;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    version: String,

    #[arg(long, default_value = "")]
    notes: String,

    #[arg(long)]
    pub_date: Option<DateTime<Utc>>,

    #[arg(long = "platform", num_args = 1.., value_delimiter = ',')]
    platforms: Vec<String>,

    #[arg(long = "url", num_args = 1..)]
    urls: Vec<String>,

    #[arg(long = "signature", num_args = 1..)]
    signatures: Vec<String>,

    #[arg(short, long)]
    output: PathBuf,
}

#[derive(Serialize)]
struct Platform {
    signature: String,
    url: String,
}

#[derive(Serialize)]
struct Manifest {
    version: String,
    notes: String,
    pub_date: String,
    platforms: BTreeMap<String, Platform>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    anyhow::ensure!(
        args.platforms.len() == args.urls.len() && args.urls.len() == args.signatures.len(),
        "--platform / --url / --signature counts must match: got {} / {} / {}",
        args.platforms.len(),
        args.urls.len(),
        args.signatures.len()
    );

    let mut platforms = BTreeMap::new();
    for ((p, u), s) in args
        .platforms
        .into_iter()
        .zip(args.urls)
        .zip(args.signatures)
    {
        platforms.insert(
            p,
            Platform {
                signature: s,
                url: u,
            },
        );
    }

    let manifest = Manifest {
        version: args.version,
        notes: args.notes,
        pub_date: args.pub_date.unwrap_or_else(Utc::now).to_rfc3339(),
        platforms,
    };

    let text = serde_json::to_string_pretty(&manifest).context("serialize manifest")?;
    std::fs::write(&args.output, text).context("write manifest")?;
    println!("wrote {}", args.output.display());
    Ok(())
}
