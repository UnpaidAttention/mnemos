//! P1-19: Integration test for `mnemos hook session-end` against a live daemon.
//!
//! Spawns a real daemon on a random free port, writes a minimal JSONL transcript,
//! runs `mnemos hook session-end` with a JSON payload on stdin, then asserts that
//! at least one session row and one chunk landed in the daemon's DB.
//!
//! The test is hermetic:
//!   - TempDir for vault root, XDG config, and XDG state dirs.
//!   - `MNEMOS_DAEMON_PORT` so `daemon_base_url()` resolves the random port.
//!   - `XDG_CONFIG_HOME` so `mnemos_daemon::token_path()` writes/reads the token
//!     from the temp dir rather than the real user profile.
//!   - `XDG_STATE_HOME` so the idempotency state file lives in the temp dir.
//!
//! No embedder or LLM is required — the test exercises only the session/chunk
//! ingestion path (POST /v1/sessions, POST /v1/sessions/{id}/chunks, POST .../end).

use mnemos_core::paths::Paths;
use mnemos_core::vault::Vault;
use mnemos_daemon::{build_app, config::Config, serve};
use std::time::Duration;
use tempfile::TempDir;
use tokio::net::TcpListener;

/// Build a minimal one-line JSONL transcript understood by `transcript::parse_transcript`.
fn make_jsonl(body: &str) -> String {
    serde_json::json!({
        "type": "user",
        "message": { "role": "user", "content": body }
    })
    .to_string()
}

/// Count rows in `sessions` table.
async fn count_sessions(storage: &mnemos_core::storage::Storage) -> i64 {
    let conn = storage.conn().unwrap();
    let mut rows = conn
        .query("SELECT COUNT(*) FROM sessions", ())
        .await
        .unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

/// Count rows in `chunks` table.
async fn count_all_chunks(storage: &mnemos_core::storage::Storage) -> i64 {
    let conn = storage.conn().unwrap();
    let mut rows = conn.query("SELECT COUNT(*) FROM chunks", ()).await.unwrap();
    rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap()
}

// Multi-threaded runtime: the daemon serve task must run concurrently while
// the main test thread blocks on the assert_cmd subprocess wait.  A
// single-threaded executor would deadlock: the blocking .output() call
// prevents tokio from polling the serve task, so the daemon never responds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hook_session_end_ingests_transcript_into_daemon() {
    // ── 1. Temp dirs ──────────────────────────────────────────────────────────
    let tmp = TempDir::new().unwrap();
    let vault_dir = tmp.path().join("vault");
    let config_dir = tmp.path().join("config");
    let state_dir = tmp.path().join("state");
    std::fs::create_dir_all(&vault_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::create_dir_all(&state_dir).unwrap();

    // ── 2. Start daemon on a random port ─────────────────────────────────────
    let vault = Vault::open(Paths::with_root(&vault_dir)).await.unwrap();
    let (app, daemon_state) = build_app(Config::default(), vault).await.unwrap();
    let token = daemon_state.token.clone();
    let storage = daemon_state.vault.storage().clone();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let _server = tokio::spawn(async move { serve(listener, app).await.unwrap() });

    // Give the accept loop a moment to start.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // ── 3. Write the token where the CLI hook's `load_token()` expects it ────
    // mnemos_daemon::token_path() uses ProjectDirs("dev","mnemos","mnemos").
    // On Linux that resolves to $XDG_CONFIG_HOME/mnemos/token.
    let token_dir = config_dir.join("mnemos");
    std::fs::create_dir_all(&token_dir).unwrap();
    std::fs::write(token_dir.join("token"), &token).unwrap();

    // ── 4. Write a minimal JSONL transcript file ──────────────────────────────
    let transcript_path = tmp.path().join("session.jsonl");
    let fact_body = "hook integration test: end-to-end capture verified";
    std::fs::write(&transcript_path, make_jsonl(fact_body)).unwrap();

    // ── 5. Pre-conditions: DB is empty ────────────────────────────────────────
    assert_eq!(count_sessions(&storage).await, 0, "no sessions before hook");
    assert_eq!(count_all_chunks(&storage).await, 0, "no chunks before hook");

    // ── 6. Run `mnemos hook session-end` ─────────────────────────────────────
    let session_id = "cc-session-p1-19-test";
    let stdin_payload = serde_json::json!({
        "transcript_path": transcript_path.to_string_lossy(),
        "session_id":      session_id,
        "cwd":             "/tmp",
    })
    .to_string();

    assert_cmd::Command::cargo_bin("mnemos")
        .unwrap()
        // Vault root for direct CLI ops (not used by hook, which hits the daemon).
        .env("MNEMOS_VAULT", &vault_dir)
        // Tell daemon_base_url() which port to use.
        .env("MNEMOS_DAEMON_PORT", port.to_string())
        // Scope XDG dirs to the temp tree so we read/write the token we planted in step 3
        // and the idempotency file doesn't pollute the real user profile.
        .env("XDG_CONFIG_HOME", &config_dir)
        .env("XDG_STATE_HOME", &state_dir)
        // Override HOME to the temp root so the path-traversal guard (P2-19)
        // accepts transcripts written to tmp.path() during tests.  In production
        // HOME is the real user home and transcripts live under ~/.claude/.
        .env("HOME", tmp.path())
        // Suppress all log noise from hook / daemon in test output.
        .env("MNEMOS_LOG", "error")
        .args(["hook", "session-end"])
        .write_stdin(stdin_payload)
        .assert()
        .success();

    // hook session-end is fail-open and always returns ExitCode::SUCCESS; give
    // the daemon a moment to persist the async DB write before we read it back.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // ── 7. Assert: at least one session and one chunk landed in the DB ────────
    assert_eq!(
        count_sessions(&storage).await,
        1,
        "exactly one session row must have been created by the hook"
    );
    assert!(
        count_all_chunks(&storage).await >= 1,
        "at least one chunk must have been ingested by the hook"
    );
}
