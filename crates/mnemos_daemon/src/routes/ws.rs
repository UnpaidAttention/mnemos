//! WebSocket route — `/v1/events`.
//!
//! Auth is via `?token=...` query param because WS clients cannot always set
//! HTTP headers during the upgrade handshake.  This route must be mounted
//! OUTSIDE the bearer-auth middleware layer.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde::Deserialize;

use crate::auth::validate_token;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/v1/events", get(ws_handler))
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    token: String,
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Query(q): Query<WsQuery>,
) -> impl IntoResponse {
    if !validate_token(&state.token, &q.token) {
        return (axum::http::StatusCode::UNAUTHORIZED, "bad token").into_response();
    }
    ws.on_upgrade(|socket| socket_loop(socket, state))
}

async fn socket_loop(mut socket: WebSocket, state: AppState) {
    let mut rx = state.events.subscribe();
    loop {
        tokio::select! {
            client_msg = socket.recv() => match client_msg {
                // Keep-alive pings / other frames from the client are fine; ignore them.
                Some(Ok(_)) => continue,
                // Client disconnected or error.
                _ => break,
            },
            evt = rx.recv() => match evt {
                Ok(e) => {
                    let text = serde_json::to_string(&e).unwrap_or_default();
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                // Sender dropped (daemon shutting down) or lagged.
                Err(_) => break,
            }
        }
    }
}
