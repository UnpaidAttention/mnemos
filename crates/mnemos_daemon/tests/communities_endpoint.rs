use axum::http::StatusCode;
use mnemos_core::paths::Paths;
use mnemos_core::storage::community_ops::store_communities;
use mnemos_core::storage::entity_ops::upsert_entity;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config};
use tempfile::TempDir;

#[tokio::test]
async fn communities_endpoint_lists_members() {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let a = upsert_entity(vault.storage(), "Rust", "tool", None)
        .await
        .unwrap();
    let b = upsert_entity(vault.storage(), "Tauri", "tool", None)
        .await
        .unwrap();
    store_communities(
        vault.storage(),
        &[(a.clone(), 0), (b.clone(), 0)],
        chrono::Utc::now(),
    )
    .await
    .unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();

    let (s, body) = call(app, "GET", "/v1/communities", Some(&state.token), "").await;
    assert_eq!(s, StatusCode::OK, "{body}");
    let v: serde_json::Value = serde_json::from_str(&body).unwrap();
    let comms = v["communities"].as_array().unwrap();
    assert_eq!(comms.len(), 1);
    assert_eq!(comms[0]["community_id"], 0);
    assert_eq!(comms[0]["members"].as_array().unwrap().len(), 2);
    assert!(v["summaries"].is_array());
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
