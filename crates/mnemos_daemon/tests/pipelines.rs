use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, build_app_full, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn pipelines_status_returns_disabled_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b) = call(app, "GET", "/v1/pipelines", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["enabled"], false);
    assert_eq!(v["counters"]["completed"], 0);
    assert!(v["recent"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn pipelines_status_requires_auth() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, _state) = build_app(Config::default(), vault).await.unwrap();
    let (s, _) = call(app, "GET", "/v1/pipelines", None, "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn manual_decay_endpoint_returns_stats() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b) = call(
        app,
        "POST",
        "/v1/maintenance/decay",
        Some(&state.token),
        "{}",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["scanned"], 0);
    assert_eq!(v["invalidated"], 0);
}

#[tokio::test]
async fn communities_endpoint_runs_detection() {
    use mnemos_core::providers::mock_llm::MockLlm;
    use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
    use std::sync::Arc;

    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    // Build a small graph so there is something to cluster.
    let a = upsert_entity(vault.storage(), "A", "c", None).await.unwrap();
    let b = upsert_entity(vault.storage(), "B", "c", None).await.unwrap();
    let c = upsert_entity(vault.storage(), "C", "c", None).await.unwrap();
    for (x, y) in [(&a, &b), (&b, &c), (&a, &c)] {
        upsert_edge(vault.storage(), x, y, "rel", "m", chrono::Utc::now())
            .await
            .unwrap();
    }
    let (app, state, handle, _sync, _bundled, _bundled_llm) = build_app_full(
        Config::default(),
        vault,
        None,
        Some(Arc::new(MockLlm::new())),
    )
    .await
    .unwrap();

    let (s, b) = call(
        app,
        "POST",
        "/v1/maintenance/communities",
        Some(&state.token),
        "{}",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(!v["summaries"].as_array().unwrap().is_empty());

    if let Some(h) = handle {
        h.shutdown().await;
    }
}

#[tokio::test]
async fn communities_endpoint_409_without_llm() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let (s, _) = call(
        app,
        "POST",
        "/v1/maintenance/communities",
        Some(&state.token),
        "{}",
    )
    .await;
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
