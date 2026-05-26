use axum::http::StatusCode;
use mnemos_daemon::{build_app, config::Config};

#[tokio::test]
async fn health_endpoint_returns_200_without_auth() {
    let (app, _state) = build_app(Config::default(), test_vault().await)
        .await
        .unwrap();
    let resp = call(app, "GET", "/health", None, "").await;
    assert_eq!(resp.0, StatusCode::OK);
    assert!(resp.1.contains("\"status\":\"ok\""));
}

#[tokio::test]
async fn auth_required_on_v1_routes() {
    let (app, _state) = build_app(Config::default(), test_vault().await)
        .await
        .unwrap();
    let resp = call(app.clone(), "GET", "/v1/working", None, "").await;
    assert_eq!(resp.0, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn auth_with_correct_bearer_passes() {
    let (app, state) = build_app(Config::default(), test_vault().await)
        .await
        .unwrap();
    let token = state.token.clone();
    let resp = call(app, "GET", "/v1/working", Some(&token), "").await;
    // 200 (route exists later) or 404 (route stub) — but NOT 401.
    assert_ne!(resp.0, StatusCode::UNAUTHORIZED);
}

use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use tempfile::TempDir;

async fn test_vault() -> Vault {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    Vault::open(paths).await.unwrap()
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (axum::http::StatusCode, String) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder().method(method).uri(uri);
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_string())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    (status, text)
}
