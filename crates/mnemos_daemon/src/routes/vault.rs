//! `POST /v1/vault/export` — streams a zip of the vault root (markdown files
//! plus a `mnemos-vault.json` manifest; the DB is excluded — import rebuilds
//! it from files).
//!
//! `POST /v1/vault/import` — accepts a binary zip body (≤ 500 MB), extracts
//! into the vault root with path-traversal guards, then triggers
//! `mnemos_core::rebuild::rebuild_index`.

use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::json;
use std::io::{Cursor, Read, Write};
use walkdir::WalkDir;
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

use crate::error::ApiError;
use crate::state::AppState;

const IMPORT_CAP_BYTES: usize = 500 * 1024 * 1024;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/vault/export", post(export)).route(
        "/v1/vault/import",
        post(import).layer(DefaultBodyLimit::max(IMPORT_CAP_BYTES)),
    )
}

async fn export(State(state): State<AppState>) -> Result<Response, ApiError> {
    let root = state.vault.paths().root.clone();
    let bytes = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
        let mut buf = Vec::new();
        {
            let mut zip = ZipWriter::new(Cursor::new(&mut buf));
            let opts: FileOptions<'_, ()> =
                FileOptions::default().compression_method(CompressionMethod::Deflated);

            zip.start_file("mnemos-vault.json", opts)?;
            let manifest = serde_json::json!({
                "kind": "mnemos-vault",
                "schema": "v1",
                "exported_at": chrono::Utc::now().to_rfc3339(),
            });
            zip.write_all(manifest.to_string().as_bytes())?;

            for entry in WalkDir::new(&root).into_iter().filter_map(|e| e.ok()) {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') || name.ends_with(".db") || name.ends_with(".db-journal") {
                    continue;
                }
                if !entry.file_type().is_file() {
                    continue;
                }
                let rel = match path.strip_prefix(&root) {
                    Ok(p) => p.to_string_lossy().to_string(),
                    Err(_) => continue,
                };
                if rel.is_empty() {
                    continue;
                }
                zip.start_file(&rel, opts)?;
                let mut f = std::fs::File::open(path)?;
                std::io::copy(&mut f, &mut zip)?;
            }
            zip.finish()?;
        }
        Ok(buf)
    })
    .await
    .map_err(|e| ApiError::internal(format!("export join: {e}")))?
    .map_err(|e| ApiError::internal(format!("export io: {e}")))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"mnemos-vault.zip\"",
            ),
        ],
        bytes,
    )
        .into_response())
}

async fn import(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    if body.len() > IMPORT_CAP_BYTES {
        return Err(ApiError::bad_request(format!(
            "zip too large: {} bytes (max {})",
            body.len(),
            IMPORT_CAP_BYTES
        )));
    }
    let root = state.vault.paths().root.clone();
    let bytes = body.to_vec();
    let files = tokio::task::spawn_blocking(move || -> Result<usize, std::io::Error> {
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let mut count = 0;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            let safe = match entry.enclosed_name() {
                Some(p) => p.to_path_buf(),
                None => continue,
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
            let mut out = std::fs::File::create(&dst)?;
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            out.write_all(&data)?;
            count += 1;
        }
        Ok(count)
    })
    .await
    .map_err(|e| ApiError::internal(format!("import join: {e}")))?
    .map_err(|e| ApiError::internal(format!("import io: {e}")))?;

    mnemos_core::rebuild::rebuild_index(state.vault.paths())
        .await
        .map_err(|e| ApiError::internal(format!("rebuild: {e}")))?;

    Ok(Json(json!({ "files_imported": files, "status": "ok" })))
}
