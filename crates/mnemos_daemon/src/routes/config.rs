//! `GET /v1/config`, `PUT /v1/config`.
//!
//! GET returns the resolved daemon `Config` as JSON. PUT merges a partial
//! JSON patch into the on-disk `config.toml` and rewrites the file. Some
//! changes (notably `daemon.host` / `daemon.port`) only take effect on
//! restart; the response reports which keys require a restart.

use axum::{extract::State, routing::get, Json, Router};
use serde_json::Value;

use crate::config::{default_config_path, Config};
use crate::error::ApiError;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/config", get(get_cfg).put(put_cfg))
}

async fn get_cfg(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    serde_json::to_value(&*state.config)
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
