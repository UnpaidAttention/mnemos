use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app_full, config::Config};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn reflect_endpoint_creates_and_lists() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    vault
        .remember(
            "REFLECT:pattern|Shaun ships on Fridays",
            RememberOpts::default(),
        )
        .await
        .unwrap();
    let (app, state, handle, _sync, _bundled) = build_app_full(
        Config::default(),
        vault,
        None,
        Some(Arc::new(MockLlm::new())),
    )
    .await
    .unwrap();
    let token = state.token.clone();

    let (s, b) = call(app.clone(), "POST", "/v1/reflections", Some(&token), "{}").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(!v["created"].as_array().unwrap().is_empty());

    let (s2, b2) = call(app, "GET", "/v1/reflections", Some(&token), "").await;
    assert_eq!(s2, StatusCode::OK);
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(v2["reflections"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["body"] == "Shaun ships on Fridays"));

    if let Some(h) = handle {
        h.shutdown().await;
    }
}

#[tokio::test]
async fn reflect_endpoint_409_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = mnemos_daemon::build_app(Config::default(), vault)
        .await
        .unwrap();
    let (s, _) = call(app, "POST", "/v1/reflections", Some(&state.token), "{}").await;
    assert_eq!(s, StatusCode::CONFLICT);
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
    let s = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
