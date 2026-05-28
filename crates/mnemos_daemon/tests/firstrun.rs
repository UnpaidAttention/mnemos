//! Integration test for `/v1/first-run` (Plan 7 Task 17).

use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn first_run_round_trip() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s1, b1) = call(app.clone(), "GET", "/v1/first-run", Some(&state.token), "").await;
    assert_eq!(s1, StatusCode::OK, "{b1}");
    let v1: serde_json::Value = serde_json::from_str(&b1).unwrap();
    assert!(
        v1["completed_at"].is_null(),
        "fresh vault should have null first_run_completed_at: {b1}"
    );

    let (s2, _) = call(
        app.clone(),
        "POST",
        "/v1/first-run/complete",
        Some(&state.token),
        "",
    )
    .await;
    assert_eq!(s2, StatusCode::OK);

    let (s3, b3) = call(app, "GET", "/v1/first-run", Some(&state.token), "").await;
    assert_eq!(s3, StatusCode::OK);
    let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
    assert!(
        v3["completed_at"].is_string(),
        "after complete, should have a timestamp: {b3}"
    );
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
