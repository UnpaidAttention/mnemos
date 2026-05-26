use mnemos_client::Client;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config, serve};
use tempfile::TempDir;
use tokio::net::TcpListener;

async fn spin_daemon() -> (String, String) {
    let tmp = Box::leak(Box::new(TempDir::new().unwrap()));
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), token)
}

#[tokio::test]
async fn client_health_ok() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    assert!(c.health().await.unwrap());
}

#[tokio::test]
async fn client_remember_then_get() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    let id = c.remember("body", Default::default()).await.unwrap();
    assert!(id.starts_with("mem_"));
    let mem = c.get_memory(&id).await.unwrap();
    assert_eq!(mem.body, "body");
}

#[tokio::test]
async fn client_recall_returns_hits() {
    let (url, token) = spin_daemon().await;
    let c = Client::new(&url, &token).unwrap();
    c.remember("Tauri choice", Default::default())
        .await
        .unwrap();
    let hits = c.recall("tauri", Default::default()).await.unwrap();
    assert!(!hits.is_empty());
}
