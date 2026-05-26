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
