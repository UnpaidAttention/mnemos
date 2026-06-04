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
async fn mcp_tools_list_includes_correct() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let tools = v["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"correct"),
        "tools list must include 'correct'; got {names:?}"
    );
}

#[tokio::test]
async fn mcp_correct_valid_args_returns_id() {
    let (app, token) = fixture().await;
    let body = r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"correct","arguments":{"wrong":"used unwrap in prod","right":"propagate errors with ?","why":"unwrap panics on None and kills the process"}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["error"].is_null(), "expected no error, got: {v}");
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let payload: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(
        payload["id"].as_str().unwrap_or("").starts_with("mem_"),
        "expected id starting with mem_, got: {payload}"
    );
}

#[tokio::test]
async fn mcp_correct_missing_why_returns_error() {
    let (app, token) = fixture().await;
    // `why` is empty — less than MIN_WHY_LEN (8 chars) — validation must reject it.
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"correct","arguments":{"wrong":"did x","right":"do y","why":""}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    // The MCP dispatcher maps tool call failures to INTERNAL_ERROR (-32603);
    // INVALID_PARAMS (-32602) is reserved for "unknown tool name" only.
    assert_eq!(
        v["error"]["code"], -32603,
        "empty why must return INTERNAL_ERROR; response: {v}"
    );
    let msg = v["error"]["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("why"),
        "error message must mention 'why'; got: {msg}"
    );
}

#[tokio::test]
async fn mcp_correct_with_trigger_and_supersedes_returns_id() {
    let (app, token) = fixture().await;

    // First, store a correction to supersede.
    let first = r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"correct","arguments":{"wrong":"old approach","right":"new approach","why":"the old way caused data loss in production"}}}"#;
    let (_, b1) = call(app.clone(), "POST", "/mcp", Some(&token), first).await;
    let v1: serde_json::Value = serde_json::from_str(&b1).unwrap();
    let first_id = {
        let text = v1["result"]["content"][0]["text"].as_str().unwrap();
        let p: serde_json::Value = serde_json::from_str(text).unwrap();
        p["id"].as_str().unwrap().to_string()
    };

    // Now supersede it.
    let body = format!(
        r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"correct","arguments":{{"wrong":"old approach","right":"even newer approach","why":"new approach still had a race condition in multi-threaded context","trigger":"concurrent writes","supersedes":"{first_id}"}}}}}}"#
    );
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body.as_str()).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["error"].is_null(), "expected no error, got: {v}");
    let text = v["result"]["content"][0]["text"].as_str().unwrap();
    let payload: serde_json::Value = serde_json::from_str(text).unwrap();
    assert!(
        payload["id"].as_str().unwrap_or("").starts_with("mem_"),
        "expected mem_ id, got: {payload}"
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
