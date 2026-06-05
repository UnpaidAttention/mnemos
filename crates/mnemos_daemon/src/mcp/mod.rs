pub mod prompts;
pub mod protocol;
pub mod resources;
pub mod tools;

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use serde_json::{json, Value};

use crate::mcp::protocol::{
    JsonRpcRequest, JsonRpcResponse, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST,
    METHOD_NOT_FOUND, PARSE_ERROR,
};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/mcp", post(handle))
}

/// Handle an MCP JSON-RPC POST request.
///
/// The body is accepted as raw `Bytes` so we can return a well-formed
/// JSON-RPC PARSE_ERROR (-32700) on malformed JSON rather than letting axum
/// reject it with HTTP 422 (which leaves the PARSE_ERROR path dead — P2-10).
async fn handle(State(state): State<AppState>, body: Bytes) -> Response {
    // Step 1: parse the raw bytes as JSON — return PARSE_ERROR on failure.
    let raw: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => {
            let resp = JsonRpcResponse::error(None, PARSE_ERROR, format!("JSON parse error: {e}"));
            return (StatusCode::OK, Json(resp)).into_response();
        }
    };

    // Step 2: deserialize into the typed JsonRpcRequest — return INVALID_REQUEST
    // on a structurally invalid envelope (e.g. missing `method`, wrong field
    // types, wrong `jsonrpc` version). We extract `id` from the raw JSON first
    // so that the error response can echo it back even when the typed
    // deserialization fails.
    let id = raw.get("id").cloned();
    let req: JsonRpcRequest = match serde_json::from_value(raw) {
        Ok(r) => r,
        Err(e) => {
            let resp = JsonRpcResponse::error(id, INVALID_REQUEST, format!("invalid request: {e}"));
            return (StatusCode::OK, Json(resp)).into_response();
        }
    };

    // JSON-RPC notifications (no id, no response expected). MCP sends
    // `notifications/initialized` after initialize. Ack with 200, empty body.
    if req.method.starts_with("notifications/") {
        return StatusCode::OK.into_response();
    }
    if req.jsonrpc != "2.0" {
        return (
            StatusCode::OK,
            Json(JsonRpcResponse::error(
                req.id,
                INVALID_REQUEST,
                "jsonrpc must be '2.0'",
            )),
        )
            .into_response();
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
    (StatusCode::OK, Json(resp)).into_response()
}

fn initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        json!({
            "protocolVersion": "2024-11-05",
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

/// Dispatch a `tools/call` request.
///
/// # P2-10 semantics
///
/// JSON-RPC errors (`error` field) are reserved for protocol-level failures:
/// - Bad/missing `name` parameter → INVALID_PARAMS (-32602).
/// - Unknown tool name → INVALID_PARAMS (-32602).
///
/// Tool *execution* failures (the tool was dispatched but returned an error)
/// are returned as a successful JSON-RPC response whose `result` carries
/// `{ "content": [{"type":"text","text":"<msg>"}], "isError": true }`.
/// This matches the MCP specification's `CallToolResult.isError` convention.
async fn tools_call(id: Option<Value>, state: &AppState, params: &Value) -> JsonRpcResponse {
    let name = match params["name"].as_str() {
        Some(n) => n,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "tools/call requires 'name'");
        }
    };
    let args = params.get("arguments").cloned().unwrap_or(Value::Null);

    // Dispatch — unknown tool name is a protocol error; execution errors are
    // returned in-band as isError results.
    match tools::call(state, name, &args).await {
        Ok(result) => JsonRpcResponse::success(id, result),
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("unknown tool") {
                // Unknown tool: JSON-RPC INVALID_PARAMS (protocol error).
                JsonRpcResponse::error(id, INVALID_PARAMS, msg)
            } else {
                // Execution failure: success envelope with isError=true.
                JsonRpcResponse::success(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": msg }],
                        "isError": true,
                    }),
                )
            }
        }
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
