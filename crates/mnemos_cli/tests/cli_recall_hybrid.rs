use assert_cmd::Command;
use tempfile::TempDir;

fn cmd(tmp: &TempDir) -> Command {
    let mut c = Command::cargo_bin("mnemos").unwrap();
    c.env("MNEMOS_VAULT", tmp.path())
        // Force MockEmbedder so this test runs without Ollama.
        .env("MNEMOS_EMBEDDER", "mock")
        .env("MNEMOS_EMBEDDER_DIM", "768");
    c
}

#[test]
fn recall_explain_emits_structured_trace() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "Tauri preference", "--title", "Tauri"])
        .assert()
        .success();
    cmd(&tmp)
        .args(["remember", "React notes", "--title", "React"])
        .assert()
        .success();

    let out = cmd(&tmp)
        .args(["--json", "recall", "tauri", "--explain"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let hits = v["hits"].as_array().unwrap();
    assert!(!hits.is_empty());
    let explain = &hits[0]["explain"];
    assert!(explain.is_object(), "explain object should be present");
    assert!(explain["rrf_score"].is_number());
    assert!(explain["weight_strength"].is_number());
    assert!(explain["final_score"].is_number());
}

#[test]
fn recall_without_explain_omits_trace() {
    let tmp = TempDir::new().unwrap();
    cmd(&tmp)
        .args(["remember", "hello", "--title", "h"])
        .assert()
        .success();
    let out = cmd(&tmp)
        .args(["--json", "recall", "hello"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
    let hits = v["hits"].as_array().unwrap();
    let e = &hits[0]["explain"];
    assert!(e.is_null() || e.as_object().is_none());
}
