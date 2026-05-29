//! Integration tests for the /v1/embed-rebuild endpoints (Plan 9 Task 10).

use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app, config::Config};
use std::sync::Arc;
use tempfile::TempDir;

#[tokio::test]
async fn embed_rebuild_endpoint_round_trip() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    // Seed one memory with a 768-dim embedder so memory_vec has at least one row.
    let embedder: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(Paths::with_root(tmp.path()), Some(embedder))
        .await
        .unwrap();
    let _id = vault
        .remember("test memory body", RememberOpts::default())
        .await
        .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    // Initial status = idle.
    let (s, b) = call(
        app.clone(),
        "GET",
        "/v1/embed-rebuild/status",
        Some(&state.token),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["status"], "idle");

    // Kick off a rebuild to a 384-dim target. This forces the swap path to
    // drop + recreate memory_vec, exercising the dim-change branch.
    let body = r#"{"target_kind":"mock","target_model":"mock-v2","target_dim":384}"#;
    let (s2, b2) = call(
        app.clone(),
        "POST",
        "/v1/embed-rebuild/start",
        Some(&state.token),
        body,
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");

    // Poll status until completed or timeout (~3s).
    let mut completed = false;
    for _ in 0..60 {
        let (_, b3) = call(
            app.clone(),
            "GET",
            "/v1/embed-rebuild/status",
            Some(&state.token),
            "",
        )
        .await;
        let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
        if v3["status"] == "completed" {
            assert_eq!(
                v3["processed"], 1,
                "should have processed the one seeded memory"
            );
            assert_eq!(v3["swapped"], true);
            completed = true;
            break;
        }
        if v3["status"] == "failed" {
            panic!("rebuild failed: {b3}");
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(completed, "rebuild did not complete within 3s");
}

#[tokio::test]
async fn embed_rebuild_status_unauthenticated_returns_401() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, _state) = build_app(Config::default(), vault).await.unwrap();

    let (s, _) = call(app, "GET", "/v1/embed-rebuild/status", None, "").await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
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
