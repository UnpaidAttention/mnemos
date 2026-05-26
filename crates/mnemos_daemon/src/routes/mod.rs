//! Top-level router. Public routes (e.g. /health) are mounted unauthenticated;
//! /v1/* is gated by the bearer-token middleware.

pub mod entities;
pub mod health;
pub mod memories;
pub mod sessions;
pub mod working;

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    middleware::{from_fn_with_state, Next},
    response::Response,
    Router,
};

use crate::state::AppState;

pub fn build_router(state: AppState) -> Router {
    let public = Router::new().route("/health", axum::routing::get(health::get_health));

    let v1 = Router::new()
        .merge(memories::router())
        .merge(sessions::router())
        .merge(entities::router())
        .merge(working::router());

    let v1_with_auth = v1.route_layer(from_fn_with_state(state.clone(), bearer_auth));

    public.merge(v1_with_auth).with_state(state)
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
