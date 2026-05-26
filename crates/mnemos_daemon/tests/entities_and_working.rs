use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::types::MemoryType;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_core::Tier;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

async fn fixture_with_working_memory() -> (axum::Router, String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let id = vault
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
    (app, state.token, id)
}

#[tokio::test]
async fn get_v1_working_returns_working_memories() {
    let (app, token, id) = fixture_with_working_memory().await;
    let (s, b) = call(app, "GET", "/v1/working", Some(&token), "").await;
    assert_eq!(s, StatusCode::OK, "body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let mems = v["memories"].as_array().unwrap();
    assert!(
        mems.iter().any(|m| m["id"] == id),
        "expected id {id} in memories: {b}"
    );
    assert!(
        mems.iter().all(|m| m["tier"] == "working"),
        "expected all tier=working: {b}"
    );
}

#[tokio::test]
async fn get_v1_entities_returns_list() {
    let (app, token, _) = fixture_with_working_memory().await;
    let (s, b) = call(app, "GET", "/v1/entities", Some(&token), "").await;
    assert_eq!(s, StatusCode::OK, "body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["entities"].is_array(), "expected entities array: {b}");
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (StatusCode, String) {
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
    let st = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (st, String::from_utf8_lossy(&bytes).to_string())
}
