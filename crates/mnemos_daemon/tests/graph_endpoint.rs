use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::entity_ops::{upsert_edge, upsert_entity};
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn graph_endpoint_returns_nodes_and_edges() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool")
        .await
        .unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool")
        .await
        .unwrap();
    upsert_edge(vault.storage(), &a, &b, "uses", "mem_1", chrono::Utc::now())
        .await
        .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, b_) = call(app, "GET", "/v1/graph", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{b_}");
    let v: serde_json::Value = serde_json::from_str(&b_).unwrap();
    assert_eq!(v["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(v["edges"].as_array().unwrap().len(), 1);
    assert_eq!(v["edges"][0]["relation"], "uses");
    // community_id defaults to -1 when no community detection has run
    assert_eq!(v["nodes"][0]["community_id"], -1);
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
