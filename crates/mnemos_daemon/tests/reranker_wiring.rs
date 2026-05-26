use async_trait::async_trait;
use mnemos_core::error::Result;
use mnemos_core::paths::Paths;
use mnemos_core::providers::{mock::MockEmbedder, Embedder, Reranker};
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app_with_reranker, config::Config};
use std::sync::Arc;
use tempfile::TempDir;

struct KeywordReranker;
#[async_trait]
impl Reranker for KeywordReranker {
    async fn rerank(&self, q: &str, candidates: &[String]) -> Result<Vec<f32>> {
        let q = q.to_lowercase();
        Ok(candidates
            .iter()
            .map(|c| {
                if c.to_lowercase().contains(&q) {
                    1.0
                } else {
                    0.0
                }
            })
            .collect())
    }
}

#[tokio::test]
async fn search_with_rerank_flag_uses_state_reranker() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let emb: Arc<dyn Embedder> = Arc::new(MockEmbedder::new(768));
    let vault = Vault::open_with_embedder(paths, Some(emb)).await.unwrap();
    use mnemos_core::vault::RememberOpts;
    let _ = vault
        .remember(
            "apples",
            RememberOpts {
                title: Some("a".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let id_match = vault
        .remember(
            "the special-marker is here",
            RememberOpts {
                title: Some("b".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let _ = vault
        .remember(
            "bananas",
            RememberOpts {
                title: Some("c".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    let reranker: Option<Arc<dyn Reranker>> = Some(Arc::new(KeywordReranker));
    let (app, state) = build_app_with_reranker(Config::default(), vault, reranker)
        .await
        .unwrap();
    let token = state.token.clone();

    let body = r#"{"query":"special-marker","k":3,"rerank":true,"explain":true}"#;
    let (s, b) = call(app, "POST", "/v1/memories/search", Some(&token), body).await;
    assert_eq!(s, 200);
    let v: serde_json::Value = serde_json::from_str(&b).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    assert_eq!(hits[0]["memory"]["id"], id_match);
    let explain = hits[0]["explain"].as_object().unwrap();
    assert!(explain["rerank_score"].as_f64().is_some());
}

async fn call(
    app: axum::Router,
    method: &str,
    uri: &str,
    auth: Option<&str>,
    body: &str,
) -> (u16, String) {
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
    let s = resp.status().as_u16();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (s, String::from_utf8_lossy(&bytes).to_string())
}
