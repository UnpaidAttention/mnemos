//! `GET /v1/connectors`, `POST /v1/connectors/{id}/preview|connect|disconnect`.
//! Detects installed AI tools and writes/removes the mnemos MCP entry (and
//! session-start hint) in each tool's config, with backup + atomic writes.

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::connectors::{descriptors, edits, Connected};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/v1/connectors", get(list))
        .route("/v1/connectors/{id}/preview", post(preview))
        .route("/v1/connectors/{id}/connect", post(connect))
        .route("/v1/connectors/{id}/disconnect", post(disconnect))
}

fn connected_str(c: Connected) -> &'static str {
    match c {
        Connected::Full => "full",
        Connected::Partial => "partial",
        Connected::None => "none",
    }
}

fn bak_path(p: &std::path::Path) -> std::path::PathBuf {
    p.with_extension(format!(
        "{}.mnemos.bak",
        p.extension().and_then(|x| x.to_str()).unwrap_or("")
    ))
}

async fn list(State(_): State<AppState>) -> Result<Json<Value>, ApiError> {
    let items: Vec<Value> = descriptors::registry()
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "display_name": c.display_name,
                "kind": c.kind,
                "deprecated": c.deprecated,
                "installed": c.installed(),
                "connected": connected_str(c.connected()),
                "autonomy_status": serde_json::to_value(c.autonomy_status()).unwrap_or(Value::Null),
                "requires_service": c.requires_service,
                "manual_snippet": c.manual_snippet.map(|(t, s)| json!({"target": t, "snippet": s})),
                "edits": c.edits.iter().map(|e| json!({
                    "path": e.path().to_string_lossy(),
                    "present": e.is_present(),
                })).collect::<Vec<_>>(),
                "post_connect_note": c.post_connect_note,
            })
        })
        .collect();
    Ok(Json(json!({ "connectors": items })))
}

async fn preview(
    Path(id): Path<String>,
    State(_): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id)
        .ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    if c.edits.is_empty() {
        return Err(ApiError::bad_request(format!(
            "{id} is a manual integration; no automatic config to preview"
        )));
    }
    let mut previews = Vec::new();
    for e in &c.edits {
        let before = e.read();
        let after = e.rendered().map_err(ApiError::bad_request)?;
        previews.push(json!({
            "path": e.path().to_string_lossy(),
            "before": before,
            "after": after,
            "already_present": e.is_present(),
        }));
    }
    Ok(Json(json!({ "id": id, "edits": previews })))
}

async fn connect(
    Path(id): Path<String>,
    State(_): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id)
        .ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    if c.edits.is_empty() {
        return Err(ApiError::bad_request(format!(
            "{id} is a manual integration"
        )));
    }
    let mut applied: Vec<std::path::PathBuf> = Vec::new();
    for e in &c.edits {
        let path = e.path();
        let rendered = e.rendered().map_err(ApiError::bad_request)?;
        let res = (|| -> Result<(), String> {
            edits::backup(&path)?;
            edits::atomic_write(&path, &rendered)
        })();
        if let Err(err) = res {
            for p in &applied {
                let bak = bak_path(p);
                if bak.exists() {
                    let _ = std::fs::copy(&bak, p);
                }
            }
            let fname = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(ApiError::internal(format!(
                "connect {id} failed at {fname}: {err}"
            )));
        }
        applied.push(path);
    }
    Ok(Json(json!({
        "id": id,
        "connected": connected_str(c.connected()),
        "autonomy_status": serde_json::to_value(c.autonomy_status()).unwrap_or(Value::Null),
        // When true, the caller (desktop wizard) should also run
        // `mnemos service enable` so hooks fire outside CLI sessions.
        "requires_service": c.requires_service,
        "post_connect_note": c.post_connect_note,
    })))
}

async fn disconnect(
    Path(id): Path<String>,
    State(_): State<AppState>,
) -> Result<Json<Value>, ApiError> {
    let c = descriptors::by_id(&id)
        .ok_or_else(|| ApiError::not_found(format!("unknown connector {id}")))?;
    let mut applied: Vec<std::path::PathBuf> = Vec::new();
    for e in &c.edits {
        let path = e.path();
        if !path.exists() {
            continue;
        }
        let removed = e.removed().map_err(ApiError::bad_request)?;
        let res = (|| -> Result<(), String> {
            edits::backup(&path)?;
            edits::atomic_write(&path, &removed)
        })();
        if let Err(err) = res {
            // Rollback: restore every file we already rewrote from its backup.
            for p in &applied {
                let bak = bak_path(p);
                if bak.exists() {
                    let _ = std::fs::copy(&bak, p);
                }
            }
            let fname = path
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_default();
            return Err(ApiError::internal(format!(
                "disconnect {id} failed at {fname}: {err}"
            )));
        }
        applied.push(path);
    }
    Ok(Json(
        json!({ "id": id, "connected": connected_str(c.connected()) }),
    ))
}

// Fix 6: verify AutonomyStatus serialises to the expected snake_case strings so
// that replacing `autonomy_str()` with `serde_json::to_value()` is behaviour-
// preserving.  The desktop wizard and any existing API consumers depend on
// these exact string values.
#[cfg(test)]
mod tests {
    use crate::connectors::AutonomyStatus;

    #[test]
    fn autonomy_status_serialises_to_expected_snake_case_strings() {
        let cases = [
            (AutonomyStatus::Autonomous, "autonomous"),
            (AutonomyStatus::Connected, "connected"),
            (AutonomyStatus::NotInstalled, "not_installed"),
        ];
        for (variant, expected) in cases {
            let v = serde_json::to_value(variant).expect("serialisation must not fail");
            assert_eq!(
                v.as_str(),
                Some(expected),
                "AutonomyStatus::{variant:?} must serialise to \"{expected}\""
            );
        }
    }
}
