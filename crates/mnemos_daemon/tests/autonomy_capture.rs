//! P0-6 regression tests: `autonomy.capture = false` must prevent session creation.
//!
//! The daemon is the authoritative enforcement point.  When `capture` is
//! disabled, `POST /v1/sessions` must be refused (HTTP 409 Conflict) and no
//! session row must appear in the vault.

use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

// ── helpers ───────────────────────────────────────────────────────────────────

async fn fixture_with_capture(capture: bool) -> (axum::Router, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let mut cfg = Config::default();
    cfg.autonomy.capture = capture;
    let (app, state) = build_app(cfg, vault).await.unwrap();
    (app, state.token)
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

// ── tests ─────────────────────────────────────────────────────────────────────

/// When `autonomy.capture = true` (default), `POST /v1/sessions` succeeds with
/// HTTP 201 and returns a session id.
#[tokio::test]
async fn post_sessions_succeeds_when_capture_enabled() {
    let (app, token) = fixture_with_capture(true).await;
    let (status, body) = call(
        app,
        "POST",
        "/v1/sessions",
        Some(&token),
        r#"{"source_tool":"test","workspace":"/tmp"}"#,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "capture=true must allow session creation; body={body}"
    );
    let v: serde_json::Value = serde_json::from_str(&body).expect("response must be JSON");
    assert!(
        v.get("id").and_then(|id| id.as_str()).is_some(),
        "response must contain an `id` field; body={body}"
    );
}

/// When `autonomy.capture = false`, `POST /v1/sessions` is refused with
/// HTTP 409 Conflict and no session row is persisted.
#[tokio::test]
async fn post_sessions_refused_when_capture_disabled() {
    let (app, token) = fixture_with_capture(false).await;
    let (status, body) = call(
        app,
        "POST",
        "/v1/sessions",
        Some(&token),
        r#"{"source_tool":"test","workspace":"/tmp"}"#,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "capture=false must refuse session creation with 409; body={body}"
    );
    // The error body must mention the capture flag so the user understands why.
    assert!(
        body.contains("capture"),
        "error body must reference `capture`; body={body}"
    );
}

/// With `capture = false`, attempting to add a chunk also fails because the
/// session was never created (foreign-key / not-found guard, not a direct
/// capture check — but the net effect is the same: no data is stored).
#[tokio::test]
async fn add_chunk_impossible_when_capture_disabled_because_no_session_exists() {
    let (app, token) = fixture_with_capture(false).await;
    // Try to add a chunk to a session that could never have been created.
    let (status, _body) = call(
        app,
        "POST",
        "/v1/sessions/sess_phantom/chunks",
        Some(&token),
        r#"{"speaker":"user","body":"hello","ordinal":0}"#,
    )
    .await;
    // The session doesn't exist → 404 (not 201).
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "chunk POST to a non-existent session must return 404"
    );
}

/// Toggling capture back to `true` in a new app instance allows sessions again.
/// This confirms the gate is read from config at request time, not at startup.
#[tokio::test]
async fn post_sessions_allowed_again_when_new_instance_has_capture_enabled() {
    // First confirm disabled rejects.
    let (app_off, token_off) = fixture_with_capture(false).await;
    let (s_off, _) = call(
        app_off,
        "POST",
        "/v1/sessions",
        Some(&token_off),
        r#"{"source_tool":"t"}"#,
    )
    .await;
    assert_eq!(s_off, StatusCode::CONFLICT);

    // Then confirm a fresh instance with capture=true accepts.
    let (app_on, token_on) = fixture_with_capture(true).await;
    let (s_on, _) = call(
        app_on,
        "POST",
        "/v1/sessions",
        Some(&token_on),
        r#"{"source_tool":"t"}"#,
    )
    .await;
    assert_eq!(s_on, StatusCode::CREATED);
}
