//! Integration test for `GET /v1/config` and `PUT /v1/config`.
//!
//! Uses `MNEMOS_CONFIG_PATH` to redirect persistence to a tempdir so the
//! developer's real `~/.config/mnemos/config.toml` is untouched.

use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use serde_json::json;
use tempfile::TempDir;

#[tokio::test]
async fn get_and_put_config_round_trip() {
    let cfg_tmp = TempDir::new().unwrap();
    let cfg_path = cfg_tmp.path().join("config.toml");
    // Redirect the daemon's config persistence away from the user's real config.
    std::env::set_var("MNEMOS_CONFIG_PATH", &cfg_path);
    // Be defensive about teardown order: clear the env var at the end of the test.
    struct EnvGuard;
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            std::env::remove_var("MNEMOS_CONFIG_PATH");
        }
    }
    let _guard = EnvGuard;

    let vault_tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(vault_tmp.path()))
        .await
        .unwrap();
    let cfg = Config::default();
    let (app, state) = build_app(cfg, vault).await.unwrap();

    // GET returns the current config as JSON with non-empty body.
    let (status, body) = call(app.clone(), "GET", "/v1/config", Some(&state.token), "").await;
    assert_eq!(status, StatusCode::OK, "GET body={body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["daemon"]["port"].as_u64(), Some(7423));

    // PUT with a partial patch should merge and return saved=true.
    let patch = json!({ "daemon": { "port": 7423 } }).to_string();
    let (status, body) = call(app.clone(), "PUT", "/v1/config", Some(&state.token), &patch).await;
    assert_eq!(status, StatusCode::OK, "PUT body={body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["saved"], serde_json::Value::Bool(true));
    assert!(v["restart_required_for"].is_array());

    // Idempotent: a second PUT with the same patch should also succeed.
    let (status2, _) = call(app, "PUT", "/v1/config", Some(&state.token), &patch).await;
    assert_eq!(status2, StatusCode::OK);

    // The file was written to the tempdir, not to the user's real config.
    assert!(
        cfg_path.exists(),
        "config.toml should be persisted in tempdir"
    );
    let on_disk = std::fs::read_to_string(&cfg_path).unwrap();
    assert!(on_disk.contains("port = 7423"));
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
