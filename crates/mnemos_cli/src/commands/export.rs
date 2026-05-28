use anyhow::{anyhow, Result};
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use walkdir::WalkDir;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

pub async fn run(vault: Option<PathBuf>, output: PathBuf, json: bool) -> Result<()> {
    let root = match vault {
        Some(p) => p,
        None => mnemos_core::paths::Paths::default_xdg()?.root.clone(),
    };
    let f = File::create(&output)?;
    let mut zip = ZipWriter::new(f);
    let opts: FileOptions<()> =
        FileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("mnemos-vault.json", opts)?;
    zip.write_all(
        serde_json::json!({
            "kind": "mnemos-vault",
            "schema": "v1",
            "exported_at": chrono::Utc::now().to_rfc3339(),
        })
        .to_string()
        .as_bytes(),
    )?;

    let mut n = 0;
    for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy();
        if name.starts_with('.')
            || name.ends_with(".db")
            || name.ends_with(".db-journal")
            || !entry.file_type().is_file()
        {
            continue;
        }
        let rel = path
            .strip_prefix(&root)
            .map_err(|_| anyhow!("strip_prefix"))?
            .to_string_lossy()
            .to_string();
        zip.start_file(&rel, opts)?;
        let mut src = File::open(path)?;
        std::io::copy(&mut src, &mut zip)?;
        n += 1;
    }
    zip.finish()?;

    if json {
        println!(
            "{}",
            serde_json::json!({ "files": n, "output": output.to_string_lossy() })
        );
    } else {
        println!("exported {n} files → {}", output.display());
    }
    Ok(())
}
