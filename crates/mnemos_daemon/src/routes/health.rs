use axum::{extract::State, routing::get, Json, Router};
use serde_json::{json, Value};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/health", get(get_health))
}

/// GET /health — returns daemon liveness plus optional embedder status.
///
/// The embedder field is present only when the configured kind is one that
/// communicates over HTTP (Bundled or Ollama). It is omitted (null) for Mock
/// and None. The status value is `"ok"` or `"degraded"`.
///
/// This endpoint is intentionally unauthenticated so monitoring probes can
/// check it without a bearer token.
pub async fn get_health(State(state): State<AppState>) -> Json<Value> {
    use crate::config::EmbedderKind;

    let embedder = match state.config.embedder.kind {
        EmbedderKind::Bundled | EmbedderKind::Ollama => {
            // For the bundled embedder: probe the llama-server /health
            // endpoint. For Ollama: probe /api/tags (the same URL the doctor
            // check uses). Either way a 500ms HEAD/GET is sufficient — this is
            // a lightweight liveness probe, not a deep diagnostic.
            let probe_url = if state.config.embedder.kind == EmbedderKind::Bundled {
                format!("{}/health", state.config.embedder.url.trim_end_matches('/'))
            } else {
                format!(
                    "{}/api/tags",
                    state.config.embedder.url.trim_end_matches('/')
                )
            };
            let client_result = reqwest::Client::builder()
                .timeout(std::time::Duration::from_millis(500))
                .build();
            let status = match client_result {
                Ok(client) => match client.get(&probe_url).send().await {
                    Ok(r) if r.status().is_success() => "ok",
                    _ => "degraded",
                },
                Err(_) => "degraded",
            };
            Some(json!({ "status": status }))
        }
        EmbedderKind::OpenAi => {
            // For OpenAI we can't probe without making a paid call. Just
            // report "ok" when the API key is configured.
            let has_key = !state.config.openai.api_key.is_empty()
                || std::env::var("OPENAI_API_KEY")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false);
            if has_key {
                Some(json!({ "status": "ok" }))
            } else {
                Some(json!({ "status": "degraded" }))
            }
        }
        EmbedderKind::Mock | EmbedderKind::None => None,
    };

    Json(if let Some(e) = embedder {
        json!({
            "status": "ok",
            "service": "mnemosd",
            "version": env!("CARGO_PKG_VERSION"),
            "git_hash": option_env!("GIT_HASH").unwrap_or("dev"),
            "embedder": e,
        })
    } else {
        json!({
            "status": "ok",
            "service": "mnemosd",
            "version": env!("CARGO_PKG_VERSION"),
            "git_hash": option_env!("GIT_HASH").unwrap_or("dev"),
        })
    })
}
