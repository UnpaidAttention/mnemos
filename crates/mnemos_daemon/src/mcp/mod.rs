pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod tools;

use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use serde_json::{json, Value};

use crate::mcp::protocol::{
    JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(handle))
}

async fn handle(
    State(state): State<AppState>,
    Json(req): Json<JsonRpcRequest>,
) -> (StatusCode, Json<JsonRpcResponse>) {
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                req.id,
                INVALID_REQUEST,
                "jsonrpc must be '2.0'",
            )),
        );
    }
    let id = req.id.clone();
    let resp = match req.method.as_str() {
        "initialize" => initialize(id),
        "tools/list" => tools_list(id),
        "tools/call" => {
            tools_call(
                id.clone(),
                &state,
                req.params.as_ref().unwrap_or(&Value::Null),
            )
            .await
        }
        "resources/list" => resources_list(id),
        "resources/read" => {
            resources_read(id, &state, req.params.as_ref().unwrap_or(&Value::Null)).await
        }
        "prompts/list" => prompts_list(id),
        "prompts/get" => prompts_get(id, &state, req.params.as_ref().unwrap_or(&Value::Null)).await,
        other => JsonRpcResponse::error(id, METHOD_NOT_FOUND, format!("unknown method: {other}")),
    };
    (StatusCode::OK, Json(resp))
}

fn initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": "2025-06-18",
            "capabilities": {
                "tools": { "listChanged": false },
                "resources": { "listChanged": false },
                "prompts": { "listChanged": false }
            },
            "serverInfo": { "name": "mnemos", "version": env!("CARGO_PKG_VERSION") }
        }),
    )
}

fn tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "tools": tools::descriptors() }))
}

async fn tools_call(id: Option<Value>, state: &AppState, params: &Value) -> JsonRpcResponse {
    let name = match params["name"].as_str() {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "tools/call requires 'name'");
        }
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match tools::call(state, name, &args).await {
        Ok(result) => JsonRpcResponse::success(id, result),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, e.to_string()),
    }
}

fn resources_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "resources": resources::list_descriptors() }))
}

async fn resources_read(id: Option<Value>, state: &AppState, params: &Value) -> JsonRpcResponse {
    let uri = match params["uri"].as_str() {
        Some(u) => u,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "resources/read requires 'uri'");
        }
    };
    match resources::read(state, uri).await {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, e.to_string()),
    }
}

fn prompts_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(id, json!({ "prompts": prompts::list_descriptors() }))
}

async fn prompts_get(id: Option<Value>, state: &AppState, params: &Value) -> JsonRpcResponse {
    let name = match params["name"].as_str() {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "prompts/get requires 'name'");
        }
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);
    match prompts::get(state, name, &args).await {
        Ok(v) => JsonRpcResponse::success(id, v),
        Err(e) => JsonRpcResponse::error(id, INTERNAL_ERROR, e.to_string()),
    }
}
