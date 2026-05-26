use futures::StreamExt;
use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config, events::Event};
use tempfile::TempDir;
use tokio::net::TcpListener;

#[tokio::test]
async fn ws_receives_memory_created_event() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, state) = build_app(Config::default(), vault).await.unwrap();
    let token = state.token.clone();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    // Subscribe to events via WebSocket (auth via query param)
    let url = format!("ws://{addr}/v1/events?token={token}");
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    // Trigger a MemoryCreated event via REST
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("http://{addr}/v1/memories"))
        .header("authorization", format!("Bearer {token}"))
        .json(&serde_json::json!({ "body": "ws test", "title": "ws" }))
        .send()
        .await
        .unwrap();
    assert!(
        resp.status().is_success(),
        "POST memory failed: {}",
        resp.status()
    );

    // Expect the event within 2 seconds
    let frame = tokio::time::timeout(std::time::Duration::from_secs(2), ws.next())
        .await
        .expect("ws frame within 2s")
        .unwrap()
        .unwrap();
    let text = frame.into_text().unwrap();
    let event: Event = serde_json::from_str(&text).unwrap();
    assert!(
        matches!(event, Event::MemoryCreated { .. }),
        "expected MemoryCreated, got: {text}"
    );

    let _ = ws.close(None).await;
    server.abort();
}

#[tokio::test]
async fn ws_rejects_bad_token() {
    let tmp = TempDir::new().unwrap();
    let paths = Paths::with_root(tmp.path());
    let vault = Vault::open(paths).await.unwrap();
    let (app, _state) = build_app(Config::default(), vault).await.unwrap();

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app.into_make_service())
            .await
            .unwrap();
    });

    // Attempt connection with wrong token — expect upgrade rejection (non-101 response)
    let url = format!("ws://{addr}/v1/events?token=wrongtoken");
    let result = tokio_tungstenite::connect_async(&url).await;
    // tokio-tungstenite returns Err when the server sends a non-101 status
    assert!(
        result.is_err(),
        "expected connection failure with bad token"
    );

    server.abort();
}
