use axum::http::StatusCode;
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
async fn start_add_end_session_lifecycle() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/sessions",
        Some(&token),
        r#"{"source_tool":"claude-code","workspace":"/tmp/x"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(id.starts_with("sess_"));

    let (s2, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{id}/chunks"),
        Some(&token),
        r#"{"speaker":"user","ordinal":1,"body":"hello"}"#,
    )
    .await;
    assert_eq!(s2, StatusCode::CREATED);

    let (s3, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{id}/end"),
        Some(&token),
        r#"{"summary":"test session"}"#,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);

    let (s4, b4) = call(app, "GET", &format!("/v1/sessions/{id}"), Some(&token), "").await;
    assert_eq!(s4, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b4).unwrap();
    assert_eq!(v["session"]["id"], id);
    assert_eq!(v["session"]["summary"], "test session");
    assert_eq!(v["chunks"].as_array().unwrap().len(), 1);
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
