//! Top-level router.
//!
//! * Public routes (e.g. `/health`) — no auth.
//! * `/v1/events` — auth via query param handled inside the handler; must be
//!   mounted OUTSIDE the bearer middleware or the upgrade would be 401'd first.
//! * All other `/v1/*` routes — bearer token middleware.

pub mod communities;
pub mod config;
pub mod connectors;
pub mod corrections;
pub mod doctor;
pub mod embed_rebuild;
pub mod entities;
pub mod firstrun;
pub mod graph;
pub mod health;
pub mod memories;
pub mod pipelines;
pub mod recall_helper;
pub mod reflections;
pub mod sessions;
pub mod sync;
pub mod vault;
pub mod working;
pub mod ws;

use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue, Method, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::Response,
    Router,
};

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let public: Router<AppState> = health::router();

    let authed: Router<AppState> = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(graph::router())
        .merge(communities::router())
        .merge(working::router())
        .merge(pipelines::router())
        .merge(reflections::router())
        .merge(corrections::router())
        .merge(sync::router())
        .merge(config::router())
        .merge(doctor::router())
        .merge(firstrun::router())
        .merge(connectors::router())
        .merge(vault::router())
        .merge(embed_rebuild::router())
        .merge(crate::mcp::router())
        .route_layer(from_fn_with_state(state.clone(), bearer_auth));

    // ws_router does its own query-param auth — do NOT wrap in bearer middleware.
    let ws_router: Router<AppState> = ws::router();

    // Explicit CORS allowlist instead of a permissive policy. The daemon binds
    // loopback and serves bearer- and WS-token-authenticated traffic, so the
    // only legitimate browser origin is the Tauri desktop webview (prod + the
    // 1420 dev server). Tauri v2's webview origin is platform-dependent:
    // tauri://localhost (macOS/iOS) and http://tauri.localhost (Linux/Windows).
    // Non-browser clients (CLI, hooks via reqwest, the MCP stdio bridge) send no
    // Origin header and are unaffected by CORS.
    let cors = tower_http::cors::CorsLayer::new()
        .allow_origin([
            HeaderValue::from_static("tauri://localhost"),
            HeaderValue::from_static("http://tauri.localhost"),
            HeaderValue::from_static("http://localhost:1420"),
        ])
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]);

    public
        .merge(authed)
        .merge(ws_router)
        .layer(cors)
        .with_state(state)
}

async fn bearer_auth(
    State(state): State<AppState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let presented = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));
    match presented {
        Some(tok) if crate::auth::validate_token(&state.token, tok) => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
