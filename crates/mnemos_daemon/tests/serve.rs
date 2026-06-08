use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config, serve};
use tempfile::TempDir;

#[tokio::test]
async fn build_app_full_without_llm_has_no_pipeline_handle() {
    use mnemos_daemon::build_app_full;

    let tmp = TempDir::new().unwrap();
    let vault = Vault::open(Paths::with_root(tmp.path())).await.unwrap();
    let (_app, state, handle, sync, _bundled, _bundled_llm) =
        build_app_full(Config::default(), vault, None, None)
            .await
            .unwrap();
    assert!(handle.is_none(), "no llm → no runner");
    assert!(sync.is_none(), "no sync config → no sync worker");
    assert!(state.llm.is_none());
    // pipeline status starts empty
    let (counters, recent) = state.pipeline_status.snapshot().await;
    assert_eq!(counters.completed, 0);
    assert!(recent.is_empty());
}

#[tokio::test]
async fn serve_binds_and_responds_to_health() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let mut cfg = Config::default();
    cfg.daemon.port = 0;
    let (app, _state) = build_app(cfg.clone(), vault).await.unwrap();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(serve(listener, app));

    let body = reqwest::get(format!("http://{addr}/health"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"status\":\"ok\""));

    handle.abort();
}
