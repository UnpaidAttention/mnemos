//! Top-level router.
//!
//! * Public routes (e.g. `/health`) — no auth.
//! * `/v1/events` — auth via query param handled inside the handler; must be
//!   mounted OUTSIDE the bearer middleware or the upgrade would be 401'd first.
//! * All other `/v1/*` routes — bearer token middleware.

pub mod communities;
pub mod config;
pub mod doctor;
pub mod entities;
pub mod graph;
pub mod health;
pub mod memories;
pub mod pipelines;
pub mod recall_helper;
pub mod reflections;
pub mod sessions;
pub mod sync;
pub mod working;
pub mod ws;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::Response,
    Router,
};

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let public: Router<AppState> =
        Router::new().route("/health", axum::routing::get(health::get_health));

    let authed: Router<AppState> = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(graph::router())
        .merge(communities::router())
        .merge(working::router())
        .merge(pipelines::router())
        .merge(reflections::router())
        .merge(sync::router())
        .merge(config::router())
        .merge(doctor::router())
        .merge(crate::mcp::router())
        .route_layer(from_fn_with_state(state.clone(), bearer_auth));

    // ws_router does its own query-param auth — do NOT wrap in bearer middleware.
    let ws_router: Router<AppState> = ws::router();

    public.merge(authed).merge(ws_router).with_state(state)
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
