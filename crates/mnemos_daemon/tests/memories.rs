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
async fn post_memories_then_get_round_trips() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"hello world","title":"hi"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "body: {b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let id = v["id"].as_str().unwrap().to_string();
    assert!(id.starts_with("mem_"));

    let (s2, b2) = call(app, "GET", &format!("/v1/memories/{id}"), Some(&token), "").await;
    assert_eq!(s2, StatusCode::OK);
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(v2["title"], "hi");
}

#[tokio::test]
async fn delete_memories_id_invalidates() {
    let (app, token) = fixture().await;
    let (_, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"doomed","title":"doomed"}"#,
    )
    .await;
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, _) = call(
        app.clone(),
        "DELETE",
        &format!("/v1/memories/{id}"),
        Some(&token),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s2, b2) = call(app, "GET", &format!("/v1/memories/{id}"), Some(&token), "").await;
    let v2: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(s2, StatusCode::OK);
    assert!(v2["invalid_at"].as_str().is_some());
}

#[tokio::test]
async fn post_memories_search_returns_hits() {
    let (app, token) = fixture().await;
    for body in ["Tauri desktop UI", "React JS framework"] {
        call(
            app.clone(),
            "POST",
            "/v1/memories",
            Some(&token),
            &format!(r#"{{"body":"{body}","title":"x"}}"#),
        )
        .await;
    }
    let (s, b) = call(
        app,
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"tauri","k":3}"#,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
}

#[tokio::test]
async fn get_memories_id_audit_returns_create_entry() {
    let (app, token) = fixture().await;
    let (_, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"x","title":"x"}"#,
    )
    .await;
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();
    let (s, b2) = call(
        app,
        "GET",
        &format!("/v1/memories/{id}/audit"),
        Some(&token),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    let entries = v["entries"].as_array().unwrap();
    assert!(entries.iter().any(|e| e["action"] == "create"));
}

#[tokio::test]
async fn search_hits_include_body() {
    let (app, token) = fixture().await;
    call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"distinctive body about platypus","title":"p"}"#,
    )
    .await;
    let (_, b) = call(
        app,
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"platypus","k":3}"#,
    )
    .await;
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(
        hits[0]["memory"]["body"], "distinctive body about platypus",
        "search hits must include the memory body"
    );
}

#[tokio::test]
async fn time_travel_respects_as_of_window() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"timetravel beacon alpha","tier":"semantic"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");

    let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
    let (s2, b2) = call(
        app.clone(),
        "POST",
        "/v1/memories/time-travel",
        Some(&token),
        &format!(r#"{{"query":"beacon","as_of":"{future}","k":10}}"#),
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert!(!v["memories"].as_array().unwrap().is_empty());

    let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    let (s3, b3) = call(
        app,
        "POST",
        "/v1/memories/time-travel",
        Some(&token),
        &format!(r#"{{"query":"beacon","as_of":"{past}","k":10}}"#),
    )
    .await;
    assert_eq!(s3, StatusCode::OK);
    let v3: serde_json::Value = serde_json::from_str(&b3).unwrap();
    assert!(
        v3["memories"].as_array().unwrap().is_empty(),
        "not valid yet in the past"
    );
}

#[tokio::test]
async fn patch_updates_tags_and_importance() {
    let (app, token) = fixture().await;
    let (s, b) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"patch target","tier":"semantic"}"#,
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let id = serde_json::from_str::<serde_json::Value>(&b).unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let (s2, b2) = call(
        app.clone(),
        "PATCH",
        &format!("/v1/memories/{id}"),
        Some(&token),
        r#"{"tags":["urgent","work"],"importance":0.95}"#,
    )
    .await;
    assert_eq!(s2, StatusCode::OK, "{b2}");
    let v: serde_json::Value = serde_json::from_str(&b2).unwrap();
    assert_eq!(v["tags"][0], "urgent");
    assert!((v["importance"].as_f64().unwrap() - 0.95).abs() < 1e-9);
}

#[tokio::test]
async fn global_search_returns_only_community_summaries() {
    let (app, token) = fixture().await;
    // A community summary and a normal semantic memory, both mentioning "rust".
    // MemoryType uses kebab-case serde, so the kind is "community-summary".
    let (_, _) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"themes around rust tooling and editors","tier":"reflection","kind":"community-summary"}"#,
    )
    .await;
    let (_, _) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"rust borrow checker note","tier":"semantic","kind":"fact"}"#,
    )
    .await;

    let (s, b) = call(
        app,
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"rust","k":10,"global":true}"#,
    )
    .await;
    assert_eq!(s, axum::http::StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(hits
        .iter()
        .any(|h| h["memory"]["body"] == "themes around rust tooling and editors"));
    // Memory.kind is serialized as "type" (serde rename).
    assert!(
        hits.iter()
            .all(|h| h["memory"]["type"] == "community-summary"),
        "global mode returns only community summaries"
    );
}

#[tokio::test]
async fn search_accepts_graph_flag() {
    let (app, token) = fixture().await;
    let (_, _) = call(
        app.clone(),
        "POST",
        "/v1/memories",
        Some(&token),
        r#"{"body":"alpha rust topic","tier":"semantic"}"#,
    )
    .await;
    let (s, b) = call(
        app,
        "POST",
        "/v1/memories/search",
        Some(&token),
        r#"{"query":"alpha rust","k":10,"graph":true}"#,
    )
    .await;
    assert_eq!(s, axum::http::StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert!(v["hits"]
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["memory"]["body"] == "alpha rust topic"));
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
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}
