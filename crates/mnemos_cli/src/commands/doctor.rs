//! `mnemos doctor` — vault health and daemon diagnostics.
//!
//! # Behaviour
//!
//! **When the daemon is reachable** (`GET /health` returns 200):
//!   - Calls `GET /v1/doctor` and merges its richer check set (embedder,
//!     schema, LLM, sync, audit triggers, …) into the output so the user
//!     gets the full picture in one command.
//!
//! **When the daemon is down**:
//!   - Runs the `mnemos_core::doctor::diagnose` file/DB drift report
//!     directly against the vault on disk (no daemon needed).
//!   - Also runs two lightweight direct-vault checks:
//!       - **schema version** — compares the stored `schema_migrations` max
//!         version against the latest version expected by this binary.
//!       - **embedder metadata** — reports the kind + model + dim stored in
//!         `vault_meta` so the user can spot a mis-matched embedder config.
//!   - Prints the bundled llama-server log path (if it exists) and suggests
//!     `mnemos daemon start` so users can easily resume the daemon.
//!
//! # JSON output
//!
//! `--json` emits a single object:
//! ```json
//! {
//!   "daemon_up": bool,
//!   "daemon_checks": [...],   // present when daemon responded
//!   "drift_report": {...},    // always present
//!   "direct_checks": [...],   // present when daemon is down
//!   "llama_server_log": "..."
//! }
//! ```

use anyhow::Result;
use mnemos_core::{doctor::diagnose, paths::Paths};
use std::path::PathBuf;
use std::time::Duration;

/// Schema version expected by this binary (mirrors `routes/doctor.rs`).
const LATEST_SCHEMA: u32 = 9;

pub async fn run(vault: Option<PathBuf>, json: bool) -> Result<()> {
    let paths = match vault {
        Some(p) => Paths::with_root(&p),
        None => Paths::default_xdg()?,
    };

    // ── 1. Daemon reachability probe ─────────────────────────────────────────
    let daemon_url = crate::daemon_ctl::daemon_base_url();
    let daemon_up = probe_health(&daemon_url).await;

    // ── 2. Drift report (always — doesn't need daemon) ───────────────────────
    let report = diagnose(&paths).await?;

    // ── 3. Daemon doctor checks (when up) ────────────────────────────────────
    let daemon_checks: Option<serde_json::Value> = if daemon_up {
        fetch_daemon_doctor(&daemon_url).await
    } else {
        None
    };

    // ── 4. Direct-vault checks (when daemon is down) ─────────────────────────
    let direct_checks: Option<Vec<DirectCheck>> = if !daemon_up {
        Some(run_direct_checks(&paths).await)
    } else {
        None
    };

    // ── 5. Bundled llama-server log path (diagnostic aid) ────────────────────
    let llama_log = llama_server_log_path();

    // ── 6. Output ─────────────────────────────────────────────────────────────
    if json {
        emit_json(
            daemon_up,
            &daemon_checks,
            &report,
            &direct_checks,
            &llama_log,
        );
    } else {
        emit_human(
            daemon_up,
            &daemon_url,
            daemon_checks.as_ref(),
            &report,
            direct_checks.as_deref(),
            llama_log.as_deref(),
        );
    }
    Ok(())
}

// ── Daemon probe ─────────────────────────────────────────────────────────────

async fn probe_health(base_url: &str) -> bool {
    let url = format!("{base_url}/health");
    reqwest::Client::new()
        .get(&url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

/// Load the bearer token and call `GET /v1/doctor`.  Returns the JSON value on
/// success, `None` on any error (fail-open).
async fn fetch_daemon_doctor(base_url: &str) -> Option<serde_json::Value> {
    let token = {
        let path = mnemos_daemon::token_path().ok()?;
        mnemos_daemon::auth::load_token(&path).ok()?
    };
    let url = format!("{base_url}/v1/doctor");
    let resp = reqwest::Client::new()
        .get(&url)
        .bearer_auth(&token)
        .timeout(Duration::from_secs(5))
        .send()
        .await
        .ok()?;
    if resp.status().is_success() {
        resp.json().await.ok()
    } else {
        None
    }
}

// ── Direct-vault checks ───────────────────────────────────────────────────────

#[derive(Debug)]
struct DirectCheck {
    name: &'static str,
    status: &'static str,
    detail: String,
}

async fn run_direct_checks(paths: &Paths) -> Vec<DirectCheck> {
    let mut checks = Vec::new();

    // Schema version check.
    checks.push(check_schema_version(paths).await);

    // Embedder metadata check.
    checks.push(check_embedder_meta(paths).await);

    checks
}

async fn check_schema_version(paths: &Paths) -> DirectCheck {
    use mnemos_core::storage::Storage;
    match Storage::open(&paths.db_path).await {
        Err(e) => DirectCheck {
            name: "schema_version",
            status: "fail",
            detail: format!("could not open DB: {e}"),
        },
        Ok(storage) => match storage.schema_version().await {
            Ok(v) if v == LATEST_SCHEMA => DirectCheck {
                name: "schema_version",
                status: "ok",
                detail: format!("v{v}"),
            },
            Ok(0) => DirectCheck {
                name: "schema_version",
                status: "warn",
                detail: "v0 — vault is empty or uninitialized (run mnemos remember to seed it)"
                    .into(),
            },
            Ok(v) => DirectCheck {
                name: "schema_version",
                status: "warn",
                detail: format!("v{v} (expected v{LATEST_SCHEMA}; run the daemon to migrate)"),
            },
            Err(e) => DirectCheck {
                name: "schema_version",
                status: "fail",
                detail: e.to_string(),
            },
        },
    }
}

async fn check_embedder_meta(paths: &Paths) -> DirectCheck {
    use mnemos_core::storage::{vault_meta::get_embedder_meta, Storage};
    let storage = match Storage::open(&paths.db_path).await {
        Ok(s) => s,
        Err(e) => {
            return DirectCheck {
                name: "embedder_meta",
                status: "fail",
                detail: format!("could not open DB: {e}"),
            }
        }
    };
    match get_embedder_meta(&storage).await {
        Ok(meta) if meta.kind.is_empty() => DirectCheck {
            name: "embedder_meta",
            status: "warn",
            detail:
                "no embedder metadata stored yet (run `mnemos remember` or start the daemon first)"
                    .into(),
        },
        Ok(meta) => DirectCheck {
            name: "embedder_meta",
            status: "ok",
            detail: format!("kind={} model={} dim={}", meta.kind, meta.model, meta.dim),
        },
        Err(e) => DirectCheck {
            name: "embedder_meta",
            status: "fail",
            detail: e.to_string(),
        },
    }
}

// ── Log path ──────────────────────────────────────────────────────────────────

/// Returns the path to the bundled llama-server log file.
///
/// Uses the same `ProjectDirs` resolution as `bundled_embedder::log_path()`.
fn llama_server_log_path() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")?;
    Some(dirs.data_local_dir().join("logs").join("llama-server.log"))
}

// ── Output helpers ────────────────────────────────────────────────────────────

fn emit_human(
    daemon_up: bool,
    daemon_url: &str,
    daemon_checks: Option<&serde_json::Value>,
    report: &mnemos_core::doctor::DoctorReport,
    direct_checks: Option<&[DirectCheck]>,
    llama_log: Option<&std::path::Path>,
) {
    // ── Daemon section ────────────────────────────────────────────────────────
    if daemon_up {
        println!("daemon: up ({daemon_url})");
        if let Some(checks_val) = daemon_checks {
            if let Some(checks) = checks_val.get("checks").and_then(|v| v.as_array()) {
                for c in checks {
                    let name = c["name"].as_str().unwrap_or("?");
                    let status = c["status"].as_str().unwrap_or("?");
                    let detail = c["detail"].as_str().unwrap_or("");
                    println!("  [{status}] {name}: {detail}");
                }
            }
        }
    } else {
        println!("daemon: down (not reachable at {daemon_url})");
        println!("  → run: mnemos daemon start");
        if let Some(log) = llama_log {
            println!("  bundled embedder log: {}", log.display());
        }
    }

    // ── File/DB drift section (always) ────────────────────────────────────────
    println!();
    println!(
        "vault: files scanned: {}  indexed memories: {}",
        report.files_scanned, report.db_rows
    );
    if report.issues.is_empty() {
        println!("  no drift issues");
    } else {
        println!("  {} drift issue(s):", report.issues.len());
        for issue in &report.issues {
            println!(
                "    [{:?}] {} {}",
                issue.kind,
                issue
                    .path
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                issue.detail
            );
        }
    }

    // ── Direct-vault checks (daemon-down path) ────────────────────────────────
    if let Some(checks) = direct_checks {
        println!();
        println!("direct checks (daemon down):");
        for c in checks {
            println!("  [{}] {}: {}", c.status, c.name, c.detail);
        }
    }
}

fn emit_json(
    daemon_up: bool,
    daemon_checks: &Option<serde_json::Value>,
    report: &mnemos_core::doctor::DoctorReport,
    direct_checks: &Option<Vec<DirectCheck>>,
    llama_log: &Option<PathBuf>,
) {
    let direct_json: Option<serde_json::Value> = direct_checks.as_ref().map(|checks| {
        serde_json::json!(checks
            .iter()
            .map(|c| serde_json::json!({
                "name":   c.name,
                "status": c.status,
                "detail": c.detail,
            }))
            .collect::<Vec<_>>())
    });

    let out = serde_json::json!({
        "daemon_up":      daemon_up,
        "daemon_checks":  daemon_checks,
        "drift_report":   report,
        "direct_checks":  direct_json,
        "llama_server_log": llama_log.as_ref().map(|p| p.to_string_lossy()),
    });
    println!("{}", serde_json::to_string_pretty(&out).unwrap_or_default());
}
