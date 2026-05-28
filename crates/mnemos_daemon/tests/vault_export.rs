//! Integration test for `POST /v1/vault/export` and `POST /v1/vault/import`
//! (Plan 7 Task 15).

use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn export_then_import_roundtrip() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let _id = vault
        .remember("hello vault", RememberOpts::default())
        .await
        .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, zip_bytes) = call_bytes(
        app.clone(),
        "POST",
        "/v1/vault/export",
        Some(&state.token),
        &[],
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(zip_bytes.len() > 100, "zip should not be empty");

    let (s2, _) = call_bytes(
        app,
        "POST",
        "/v1/vault/import",
        Some(&state.token),
        &zip_bytes,
    )
    .await;
    assert_eq!(s2, StatusCode::OK);
}

async fn call_bytes(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &[u8],
) -> (StatusCode, Vec<u8>) {
    use axum::body::Body;
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let mut req = axum::http::Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/zip");
    if let Some(t) = auth {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = req.body(Body::from(body.to_vec())).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (s, bytes)
}
