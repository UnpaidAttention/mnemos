//! Integration test for the llama-server child-process lifecycle manager.
//!
//! Requires the bundled assets — run `scripts/fetch-bundled-assets.sh` first,
//! then set `MNEMOS_TEST_BUNDLED=1` to opt in.

use mnemos_daemon::bundled_embedder::{spawn, BundledEmbedderConfig};

#[tokio::test]
#[ignore = "requires assets/llama-server-linux-x86_64 and assets/*.gguf (run scripts/fetch-bundled-assets.sh)"]
async fn spawn_and_health_check() {
    if std::env::var("MNEMOS_TEST_BUNDLED").is_err() {
        return;
    }
    // Resolve assets relative to the workspace root, not the test's cwd.
    let workspace_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..");
    let cfg = BundledEmbedderConfig {
        binary: workspace_root.join("assets/llama-server-linux-x86_64"),
        model: workspace_root.join("assets/all-MiniLM-L6-v2.Q8_0.gguf"),
        port: 17424, // non-default so we don't collide with a real daemon
        host: "127.0.0.1".into(),
    };
    let handle = spawn(cfg).await.expect("spawn llama-server");
    // Wait for the embed endpoint to come up.
    let client = reqwest::Client::new();
    let mut ok = false;
    for _ in 0..50 {
        if let Ok(r) = client.get("http://127.0.0.1:17424/health").send().await {
            if r.status().is_success() {
                ok = true;
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert!(ok, "llama-server did not become healthy within 5s");
    handle.shutdown().await;
}
