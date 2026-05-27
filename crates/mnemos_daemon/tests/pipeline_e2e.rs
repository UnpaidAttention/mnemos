use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::providers::mock::MockEmbedder;
use mnemos_core::providers::mock_llm::MockLlm;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app_full, config::Config, events::Event};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

#[tokio::test]
async fn session_end_produces_searchable_memory_over_http() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open_with_embedder(
        Paths::with_root(tmp.path()),
        Some(Arc::new(MockEmbedder::new(768))),
    )
    .await
    .unwrap();
    let (app, state, handle) = build_app_full(
        Config::default(),
        vault,
        None,
        Some(Arc::new(MockLlm::new())),
    )
    .await
    .unwrap();
    let handle = handle.expect("runner present");
    let token = state.token.clone();
    let mut rx = state.events.subscribe();

    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/sessions",
        Some(&token),
        r#"{"source_tool":"claude-code"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let sid = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s2, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{sid}/chunks"),
        Some(&token),
        r#"{"speaker":"user","body":"FACT: Shaun loves Rust"}"#,
    )
    .await;
    assert_eq!(s2, StatusCode::CREATED);

    let (s3, _) = call(
        app.clone(),
        "POST",
        &format!("/v1/sessions/{sid}/end"),
        Some(&token),
        r#"{}"#,
    )
    .await;
    assert_eq!(s3, StatusCode::OK);

    let sid2 = sid.clone();
    let added = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match rx.recv().await {
                Ok(Event::PipelineCompleted {
                    session_id,
                    facts_added,
                }) if session_id == sid2 => return facts_added,
                _ => continue,
            }
        }
    })
    .await
    .expect("pipeline completes within 5s");
    assert!(added >= 1);

    let (s4, b4) = call(
        app.clone(),
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"Rust","k":10}"#,
    )
    .await;
    assert_eq!(s4, StatusCode::OK, "{b4}");
    assert!(
        b4.contains("Shaun loves Rust"),
        "memory should be searchable: {b4}"
    );

    let (s5, b5) = call(app, "GET", "/v1/pipelines", Some(&token), "").await;
    assert_eq!(s5, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b5).unwrap();
    assert_eq!(v["enabled"], true);
    assert!(v["counters"]["completed"].as_u64().unwrap() >= 1);

    handle.shutdown().await;
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
