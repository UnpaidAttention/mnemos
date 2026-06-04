//! `GET /v1/config`, `PUT /v1/config`.
//!
//! GET returns the resolved daemon `Config` as JSON **with secrets masked**.
//! Fields that carry credentials (`openai.api_key`, `sync.turso.auth_token`)
//! are replaced with `"(set)"` / `"(not set)"` so the endpoint cannot be
//! used to harvest secrets. PUT still accepts the full value for writing;
//! it only validates and persists — it never echoes the secret back.

use axum::{extract::State, routing::get, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::config::{default_config_path, Config};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/config", get(get_cfg).put(put_cfg))
}

// ── Secret masking helpers ────────────────────────────────────────────────────

/// Replace a secret string with `"(set)"` when non-empty, `"(not set)"` when
/// empty. This is the canonical masking used by the GET response.
fn mask_secret(secret: &str) -> &'static str {
    if secret.is_empty() {
        "(not set)"
    } else {
        "(set)"
    }
}

// ── Response view structs ─────────────────────────────────────────────────────
//
// We use dedicated "view" types for the GET response so that
// (a) secrets are masked at the type level rather than via ad-hoc JSON
//     manipulation, and
// (b) adding a new secret field to Config forces a compile-time update here.

/// Masked view of `OpenAiConfig` — `api_key` is replaced with a sentinel.
#[derive(Debug, Serialize, Deserialize)]
pub struct OpenAiConfigView {
    pub base_url: String,
    /// `"(set)"` when an API key is stored; `"(not set)"` otherwise.
    pub api_key: &'static str,
}

/// Masked view of `TursoSyncConfig`.
#[derive(Debug, Serialize, Deserialize)]
pub struct TursoSyncConfigView {
    pub enabled: bool,
    pub url: String,
    /// `"(set)"` when an auth token is stored; `"(not set)"` otherwise.
    pub auth_token: &'static str,
}

/// Top-level masked config response.  All non-secret fields are serialized
/// verbatim; secret fields go through their masked view counterpart.
#[derive(Debug, Serialize)]
pub struct ConfigView<'a> {
    pub daemon: &'a crate::config::DaemonConfig,
    pub vault: &'a crate::config::VaultConfig,
    pub embedder: &'a crate::config::EmbedderConfig,
    pub llm: &'a crate::config::LlmConfig,
    /// OpenAI credentials with `api_key` masked.
    pub openai: OpenAiConfigView,
    pub reranker: &'a crate::config::RerankerConfig,
    pub retrieval: &'a crate::config::RetrievalConfig,
    pub mcp: &'a crate::config::McpConfig,
    pub logging: &'a crate::config::LoggingConfig,
    pub reflection: &'a crate::config::ReflectionConfig,
    pub community: &'a crate::config::CommunityConfig,
    pub sync: SyncConfigView<'a>,
    pub autonomy: &'a crate::config::AutonomyConfig,
}

/// Masked view of `SyncConfig` — nested `turso.auth_token` is replaced.
#[derive(Debug, Serialize)]
pub struct SyncConfigView<'a> {
    pub kind: &'a crate::config::SyncKind,
    pub interval_secs: u64,
    pub git: &'a crate::config::GitSyncConfig,
    pub s3: &'a crate::config::S3SyncConfig,
    pub turso: TursoSyncConfigView,
}

impl<'a> ConfigView<'a> {
    pub fn from_config(cfg: &'a Config) -> Self {
        ConfigView {
            daemon: &cfg.daemon,
            vault: &cfg.vault,
            embedder: &cfg.embedder,
            llm: &cfg.llm,
            openai: OpenAiConfigView {
                base_url: cfg.openai.base_url.clone(),
                api_key: mask_secret(&cfg.openai.api_key),
            },
            reranker: &cfg.reranker,
            retrieval: &cfg.retrieval,
            mcp: &cfg.mcp,
            logging: &cfg.logging,
            reflection: &cfg.reflection,
            community: &cfg.community,
            sync: SyncConfigView {
                kind: &cfg.sync.kind,
                interval_secs: cfg.sync.interval_secs,
                git: &cfg.sync.git,
                s3: &cfg.sync.s3,
                turso: TursoSyncConfigView {
                    enabled: cfg.sync.turso.enabled,
                    url: cfg.sync.turso.url.clone(),
                    auth_token: mask_secret(&cfg.sync.turso.auth_token),
                },
            },
            autonomy: &cfg.autonomy,
        }
    }
}

// ── Route handlers ─────────────────────────────────────────────────────────────

async fn get_cfg(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let view = ConfigView::from_config(&state.config);
    serde_json::to_value(&view)
        .map(Json)
        .map_err(|e| ApiError::internal(e.to_string()))
}

async fn put_cfg(
    State(_state): State<AppState>,
    Json(patch): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let path = default_config_path().map_err(|e| ApiError::internal(e.to_string()))?;
    let existing: toml::Value = if path.exists() {
        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ApiError::internal(e.to_string()))?;
        toml::from_str(&text).map_err(|e| ApiError::bad_request(format!("config parse: {e}")))?
    } else {
        toml::Value::Table(Default::default())
    };
    let merged = merge_value(existing, json_to_toml(patch));
    let text = toml::to_string_pretty(&merged).map_err(|e| ApiError::internal(e.to_string()))?;
    // Validate the merged result against the Config schema before persisting.
    let _: Config =
        toml::from_str(&text).map_err(|e| ApiError::bad_request(format!("config invalid: {e}")))?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.ok();
    }
    tokio::fs::write(&path, text)
        .await
        .map_err(|e| ApiError::internal(e.to_string()))?;
    Ok(Json(serde_json::json!({
        "saved": true,
        "path": path.to_string_lossy(),
        "restart_required_for": ["daemon.host", "daemon.port"],
    })))
}

fn json_to_toml(v: Value) -> toml::Value {
    match v {
        Value::Null => toml::Value::String(String::new()),
        Value::Bool(b) => toml::Value::Boolean(b),
        Value::Number(n) => n
            .as_i64()
            .map(toml::Value::Integer)
            .or_else(|| n.as_f64().map(toml::Value::Float))
            .unwrap_or_else(|| toml::Value::String(n.to_string())),
        Value::String(s) => toml::Value::String(s),
        Value::Array(a) => toml::Value::Array(a.into_iter().map(json_to_toml).collect()),
        Value::Object(o) => {
            let mut t = toml::map::Map::new();
            for (k, v) in o {
                t.insert(k, json_to_toml(v));
            }
            toml::Value::Table(t)
        }
    }
}

fn merge_value(base: toml::Value, patch: toml::Value) -> toml::Value {
    match (base, patch) {
        (toml::Value::Table(mut b), toml::Value::Table(p)) => {
            for (k, v) in p {
                let merged = match b.remove(&k) {
                    Some(bv) => merge_value(bv, v),
                    None => v,
                };
                b.insert(k, merged);
            }
            toml::Value::Table(b)
        }
        (_, p) => p,
    }
}
