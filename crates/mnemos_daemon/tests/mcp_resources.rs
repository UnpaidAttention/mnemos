use mnemos_core::paths::Paths;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

async fn fixture_with_working_mem() -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    vault
        .remember(
            "user is Shaun",
            RememberOpts {
                title: Some("identity".into()),
                tier: Tier::Working,
                kind: MemoryType::Identity,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

#[tokio::test]
async fn mcp_resources_list_includes_working() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let resources = v["result"]["resources"].as_array().unwrap();
    assert!(resources.iter().any(|r| r["uri"] == "mnemos://working"));
    assert!(resources.iter().any(|r| r["uri"] == "mnemos://recent"));
}

#[tokio::test]
async fn mcp_resources_read_working_returns_working_memories() {
    let (app, token) = fixture_with_working_mem().await;
    let body =
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"mnemos://working"}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let contents = v["result"]["contents"].as_array().unwrap();
    let first_text = contents[0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(first_text).unwrap();
    let mems = parsed["memories"].as_array().unwrap();
    assert!(mems.iter().any(|m| m["title"] == "identity"));
}

#[tokio::test]
async fn mcp_prompts_list_includes_context_for() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":3,"method":"prompts/list"}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let names: Vec<&str> = v["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"context-for"));
}

#[tokio::test]
async fn mcp_prompts_get_context_for_returns_messages() {
    let (app, token) = fixture_with_working_mem().await;
    let body = r#"{"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"context-for","arguments":{"workspace":"any"}}}"#;
    let (_, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let msgs = v["result"]["messages"].as_array().unwrap();
    assert!(!msgs.is_empty());
    assert_eq!(msgs[0]["role"], "system");
    let text = msgs[0]["content"]["text"].as_str().unwrap();
    assert!(text.contains("user is Shaun"));
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
