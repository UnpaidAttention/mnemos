use mnemos_core::paths::Paths;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

/// Seed a vault with a Working-tier identity memory and (optionally) a
/// hardened Reflection-tier rule, then build the test app.
async fn fixture_vault(add_hardened: bool) -> (axum::Router, String) {
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
    if add_hardened {
        vault
            .remember(
                "always use snake_case for variable names",
                RememberOpts {
                    title: Some("snake_case rule".into()),
                    tier: Tier::Reflection,
                    kind: MemoryType::Reflection,
                    tags: vec!["mnemos:hardened".into()],
                    importance: Some(0.9),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    }
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token)
}

async fn fixture_with_working_mem() -> (axum::Router, String) {
    fixture_vault(false).await
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

/// A hardened Reflection-tier rule must appear in the `hardened_rules` array
/// of the `mnemos://working` resource payload.
#[tokio::test]
async fn working_resource_includes_hardened_rules() {
    let (app, token) = fixture_vault(true).await;
    let body = r#"{"jsonrpc":"2.0","id":10,"method":"resources/read","params":{"uri":"mnemos://working"}}"#;
    let (status, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(status, 200, "unexpected status; body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let contents = v["result"]["contents"].as_array().unwrap();
    let first_text = contents[0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(first_text).unwrap();

    let rules = parsed["hardened_rules"]
        .as_array()
        .expect("hardened_rules field must be present when hardened memories exist");
    assert!(
        !rules.is_empty(),
        "hardened_rules must contain at least one entry"
    );
    assert!(
        rules
            .iter()
            .any(|r| r["title"].as_str() == Some("snake_case rule")),
        "seeded hardened rule title must appear in hardened_rules"
    );
    // Tags must confirm the mnemos:hardened tag is present.
    let rule = rules
        .iter()
        .find(|r| r["title"].as_str() == Some("snake_case rule"))
        .unwrap();
    let tags: Vec<&str> = rule["tags"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|t| t.as_str())
        .collect();
    assert!(
        tags.contains(&"mnemos:hardened"),
        "returned rule must carry the mnemos:hardened tag"
    );
}

/// When no hardened rules exist the field must be absent (not an empty array).
#[tokio::test]
async fn working_resource_omits_hardened_rules_when_none_exist() {
    let (app, token) = fixture_vault(false).await;
    let body = r#"{"jsonrpc":"2.0","id":11,"method":"resources/read","params":{"uri":"mnemos://working"}}"#;
    let (status, b) = call(app, "POST", "/mcp", Some(&token), body).await;
    assert_eq!(status, 200, "unexpected status; body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let contents = v["result"]["contents"].as_array().unwrap();
    let first_text = contents[0]["text"].as_str().unwrap();
    let parsed: serde_json::Value = serde_json::from_str(first_text).unwrap();

    assert!(
        parsed.get("hardened_rules").is_none(),
        "hardened_rules key must be absent when no hardened memories exist; \
         got: {parsed}"
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
