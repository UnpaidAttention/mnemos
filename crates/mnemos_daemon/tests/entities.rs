use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{link_entity_mention, upsert_edge, upsert_entity};
use mnemos_core::vault::{RememberOpts, Vault};
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

async fn fixture() -> (axum::Router, String, String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let mem = vault
        .remember("rust note", RememberOpts::default())
        .await
        .unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool")
        .await
        .unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool")
        .await
        .unwrap();
    upsert_edge(vault.storage(), &a, &b, "uses", &mem, chrono::Utc::now())
        .await
        .unwrap();
    link_entity_mention(vault.storage(), &mem, &a)
        .await
        .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    (app, state.token, a, mem)
}

#[tokio::test]
async fn entity_detail_is_enriched() {
    let (app, token, a, mem) = fixture().await;
    let (s, b) = call(app, "GET", &format!("/v1/entities/{a}"), Some(&token), "").await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(v["name"], "Rust");
    assert_eq!(v["mention_count"], 1);
    assert!(v["memory_ids"]
        .as_array()
        .unwrap()
        .iter()
        .any(|m| m == &mem));
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn entity_neighborhood_graph() {
    let (app, token, a, _mem) = fixture().await;
    let (s, b) = call(
        app,
        "GET",
        &format!("/v1/entities/{a}/graph"),
        Some(&token),
        "",
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{b}");
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    // self + 1 neighbor
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
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
