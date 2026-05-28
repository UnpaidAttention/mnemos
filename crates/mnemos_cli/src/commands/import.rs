use anyhow::Result;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use zip::ZipArchive;

pub async fn run(vault: Option<PathBuf>, input: PathBuf, json: bool) -> Result<()> {
    let root = match vault {
        Some(p) => p,
        None => mnemos_core::paths::Paths::default_xdg()?.root.clone(),
    };
    std::fs::create_dir_all(&root)?;
    let f = File::open(&input)?;
    let mut archive = ZipArchive::new(f)?;
    let mut count = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(safe) = entry.enclosed_name() else {
            continue;
        };
        if safe.as_os_str().is_empty() {
            continue;
        }
        let dst = root.join(&safe);
        if entry.is_dir() {
            std::fs::create_dir_all(&dst)?;
            continue;
        }
        if let Some(parent) = dst.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        std::fs::write(&dst, buf)?;
        count += 1;
    }
    if json {
        println!("{}", serde_json::json!({ "files": count }));
    } else {
        println!("imported {count} files into {}", root.display());
    }
    println!("note: run `mnemos rebuild` to refresh the DB index from the imported files.");
    Ok(())
}
