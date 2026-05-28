//! `GET /v1/doctor` — battery of diagnostic checks composed with the
//! file/DB drift report from `mnemos_core::doctor::diagnose`.
//!
//! Returns `{ checks: [{ name, status, detail }], report: DoctorReport }`.
//! Failures sort first, then warnings, then ok.

use axum::{extract::State, routing::get, Json, Router};
use mnemos_core::doctor::diagnose;
use serde::Serialize;
use serde_json::{json, Value};

use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/doctor", get(run))
}

#[derive(Serialize)]
struct Check {
    name: &'static str,
    status: &'static str,
    detail: String,
}

/// Latest schema version expected by this binary. Bump alongside Plan 7
/// Task 17 (v8 audit-log compaction migration).
const LATEST_SCHEMA: u32 = 7;

async fn schema_version(state: &AppState) -> Check {
    match state.vault.storage().schema_version().await {
        Ok(v) if v == LATEST_SCHEMA => Check {
            name: "schema_version",
            status: "ok",
            detail: format!("v{v}"),
        },
        Ok(v) => Check {
            name: "schema_version",
            status: "warn",
            detail: format!("v{v} (expected v{LATEST_SCHEMA})"),
        },
        Err(e) => Check {
            name: "schema_version",
            status: "fail",
            detail: e.to_string(),
        },
    }
}

async fn audit_triggers(state: &AppState) -> Check {
    let conn = match state.vault.storage().conn() {
        Ok(c) => c,
        Err(e) => {
            return Check {
                name: "audit_triggers",
                status: "fail",
                detail: e.to_string(),
            }
        }
    };
    let mut rows = match conn
        .query(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name IN ('audit_log_no_update','audit_log_no_delete')",
            (),
        )
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return Check {
                name: "audit_triggers",
                status: "fail",
                detail: e.to_string(),
            }
        }
    };
    let n: i64 = rows
        .next()
        .await
        .ok()
        .flatten()
        .and_then(|r| r.get(0).ok())
        .unwrap_or(0);
    if n == 2 {
        Check {
            name: "audit_triggers",
            status: "ok",
            detail: "append-only triggers present".into(),
        }
    } else {
        Check {
            name: "audit_triggers",
            status: "fail",
            detail: format!("expected 2 triggers, found {n}"),
        }
    }
}

async fn vault_writable(state: &AppState) -> Check {
    let parent = state
        .vault
        .paths()
        .db_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let probe = parent.join(".mnemos-doctor-probe");
    match tokio::fs::write(&probe, b"ok").await {
        Ok(()) => {
            let _ = tokio::fs::remove_file(&probe).await;
            Check {
                name: "vault_writable",
                status: "ok",
                detail: "vault root is writable".into(),
            }
        }
        Err(e) => Check {
            name: "vault_writable",
            status: "fail",
            detail: e.to_string(),
        },
    }
}

async fn embedder_reachable(state: &AppState) -> Check {
    use crate::config::EmbedderKind;
    match state.config.embedder.kind {
        EmbedderKind::None => Check {
            name: "embedder",
            status: "ok",
            detail: "disabled".into(),
        },
        EmbedderKind::Mock => Check {
            name: "embedder",
            status: "ok",
            detail: "mock".into(),
        },
        EmbedderKind::Ollama => {
            let url = format!(
                "{}/api/tags",
                state.config.embedder.url.trim_end_matches('/')
            );
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    return Check {
                        name: "embedder",
                        status: "fail",
                        detail: e.to_string(),
                    }
                }
            };
            match client.get(&url).send().await {
                Ok(r) if r.status().is_success() => Check {
                    name: "embedder",
                    status: "ok",
                    detail: format!("Ollama reachable at {}", state.config.embedder.url),
                },
                Ok(r) => Check {
                    name: "embedder",
                    status: "fail",
                    detail: format!("HTTP {}", r.status()),
                },
                Err(e) => Check {
                    name: "embedder",
                    status: "fail",
                    detail: e.to_string(),
                },
            }
        }
    }
}

async fn llm_reachable(state: &AppState) -> Check {
    use crate::config::LlmKind;
    match state.config.llm.kind {
        LlmKind::None => Check {
            name: "llm",
            status: "ok",
            detail: "disabled".into(),
        },
        LlmKind::Mock => Check {
            name: "llm",
            status: "ok",
            detail: "mock".into(),
        },
        LlmKind::Ollama => {
            let url = format!("{}/api/tags", state.config.llm.url.trim_end_matches('/'));
            let client = match reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(3))
                .build()
            {
                Ok(c) => c,
                Err(e) => {
                    return Check {
                        name: "llm",
                        status: "fail",
                        detail: e.to_string(),
                    }
                }
            };
            match client.get(&url).send().await {
                Ok(r) if r.status().is_success() => Check {
                    name: "llm",
                    status: "ok",
                    detail: format!("Ollama reachable at {}", state.config.llm.url),
                },
                Ok(r) => Check {
                    name: "llm",
                    status: "fail",
                    detail: format!("HTTP {}", r.status()),
                },
                Err(e) => Check {
                    name: "llm",
                    status: "fail",
                    detail: e.to_string(),
                },
            }
        }
    }
}

async fn sync_check(state: &AppState) -> Check {
    use crate::config::SyncKind;
    use mnemos_core::sync::{filesystem::FilesystemSync, git::GitSync, s3::S3Sync, SyncBackend};
    let storage = state.vault.storage().clone();
    let backend: Option<Box<dyn SyncBackend>> = match state.config.sync.kind {
        SyncKind::None => None,
        SyncKind::Filesystem => Some(Box::new(FilesystemSync::new(storage))),
        SyncKind::Git => Some(Box::new(GitSync::new(
            storage,
            state.config.sync.git.remote.clone(),
            state.config.sync.git.branch.clone(),
        ))),
        SyncKind::S3 => Some(Box::new(S3Sync::new(
            storage,
            state.config.sync.s3.remote.clone(),
        ))),
    };
    match backend {
        None => Check {
            name: "sync",
            status: "ok",
            detail: "disabled".into(),
        },
        Some(b) => match b.status().await {
            Ok(s) if s.ready => Check {
                name: "sync",
                status: "ok",
                detail: format!("{}: {}", s.backend, s.detail),
            },
            Ok(s) => Check {
                name: "sync",
                status: "warn",
                detail: format!("{}: {}", s.backend, s.detail),
            },
            Err(e) => Check {
                name: "sync",
                status: "fail",
                detail: e.to_string(),
            },
        },
    }
}

async fn run(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let report = diagnose(state.vault.paths())
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    let drift_status = if report.issues.is_empty() {
        "ok"
    } else {
        "warn"
    };
    let drift = Check {
        name: "file_db_drift",
        status: drift_status,
        detail: format!(
            "{} files / {} db rows / {} issues",
            report.files_scanned,
            report.db_rows,
            report.issues.len()
        ),
    };
    let mut checks = vec![
        schema_version(&state).await,
        drift,
        audit_triggers(&state).await,
        vault_writable(&state).await,
        embedder_reachable(&state).await,
        llm_reachable(&state).await,
        sync_check(&state).await,
    ];
    checks.sort_by_key(|c| match c.status {
        "fail" => 0u8,
        "warn" => 1,
        _ => 2,
    });
    Ok(Json(json!({ "checks": checks, "report": report })))
}
