use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn mcp_initialize_returns_capabilities() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#;
    let (s, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(s, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert_eq!(v["id"], 1);
    assert!(v["result"]["serverInfo"]["name"].is_string());
    assert!(v["result"]["capabilities"]["tools"].is_object());
}

#[tokio::test]
async fn mcp_tools_list_returns_remember_recall_forget() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"remember"));
    assert!(names.contains(&"recall"));
    assert!(names.contains(&"forget"));
    assert!(names.contains(&"list_memories"));
    assert!(names.contains(&"get_memory"));
}

#[tokio::test]
async fn mcp_tools_call_remember_then_recall() {
    let (app, token) = fixture().await;
    // call remember
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"remember","arguments":{"body":"Tauri preference","title":"Tauri"}}}"#;
    let (_, b) = call(app.clone(), "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let result = &v["result"];
    let content = &result["content"][0]["text"];
    let id_json: serde_json::Value = serde_json::from_str(content.as_str().unwrap()).unwrap();
    assert!(id_json["id"].as_str().unwrap().starts_with("mem_"));

    // call recall
    let body2 = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"recall","arguments":{"query":"tauri","k":3}}}"#;
    let (_, b2) = call(app, "POST", "/mcp", Some(&token), body2).await;
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    let content2 = v2["result"]["content"][0]["text"].as_str().unwrap();
    let hits_json: serde_json::Value = serde_json::from_str(content2).unwrap();
    assert!(!hits_json["hits"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn mcp_unknown_method_returns_method_not_found_error() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":5,"method":"sub-zero/finish-him"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["error"]["code"], -32601);
}

#[tokio::test]
async fn mcp_notifications_initialized_returns_200_no_error() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#;
    let (s, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(s, 200);
    // Notification → no JSON-RPC response body (empty or non-error).
    assert!(
        b.is_empty() || !b.contains("\"error\""),
        "notification must not produce an error response, got: {b}"
    );
}

#[tokio::test]
async fn mcp_initialize_advertises_stable_protocol_version() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2024-11-05");
}

#[tokio::test]
async fn mcp_requires_auth() {
    let (app, _token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let (s, _) = call(app, "POST", "/mcp", None, body).await;
    assert_eq!(s, 401, "/mcp must require a bearer token");
}

#[tokio::test]
async fn mcp_tools_call_unknown_tool_returns_invalid_params() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nonexistent_tool","arguments":{}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(
        v["error"]["code"], -32602,
        "unknown tool name is INVALID_PARAMS"
    );
}

/// P2-10: malformed JSON must return HTTP 200 with a JSON-RPC PARSE_ERROR
/// (-32700) instead of HTTP 422 (which was the previous axum behaviour that
/// left the PARSE_ERROR path dead).
#[tokio::test]
async fn mcp_malformed_json_returns_parse_error() {
    let (app, token) = fixture().await;
    let body = r#"{ this is not valid json "#;
    let (s, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(s, 200, "HTTP status must be 200 even for parse errors");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(
        v["error"]["code"], -32700,
        "malformed JSON must yield PARSE_ERROR (-32700); got: {v}"
    );
}

/// P2-10: a structurally invalid request (valid JSON but missing `method`)
/// must return INVALID_REQUEST (-32600).
#[tokio::test]
async fn mcp_invalid_request_missing_method_returns_invalid_request() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":42,"params":{}}"#;
    let (s, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(s, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(
        v["error"]["code"], -32600,
        "missing 'method' must yield INVALID_REQUEST (-32600); got: {v}"
    );
    // id must be echoed back
    assert_eq!(v["id"], 42, "id must be echoed when available");
}

/// P2-10: tool execution failures (not unknown-tool) must return an isError
/// result rather than a JSON-RPC error envelope.
#[tokio::test]
async fn mcp_tool_execution_failure_returns_is_error_result() {
    let (app, token) = fixture().await;
    // "forget" on a non-existent memory_id causes a tool execution error.
    let body = r#"{"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"forget","arguments":{"memory_id":"mem_does_not_exist_xyz"}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    // Must be a success envelope (no JSON-RPC error) with isError:true.
    assert!(
        v["error"].is_null(),
        "execution failure must not produce a JSON-RPC error envelope; got: {v}"
    );
    assert_eq!(
        v["result"]["isError"],
        serde_json::Value::Bool(true),
        "execution failure must have isError:true in result; got: {v}"
    );
    // Content array must be present with error text.
    assert!(
        v["result"]["content"].is_array(),
        "content must be an array; got: {v}"
    );
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
