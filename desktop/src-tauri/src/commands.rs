use crate::{config_io, daemon, vault_move};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_shell::ShellExt;

#[derive(Serialize)]
pub struct MoveResult {
    pub moved_to: String,
}

/// Open a native folder picker. Returns the chosen path, or None if cancelled.
#[tauri::command]
pub async fn pick_vault_dir(app: AppHandle) -> Result<Option<String>, String> {
    let folder = app.dialog().file().blocking_pick_folder();
    Ok(folder.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn daemon_status(app: AppHandle) -> Result<daemon::DaemonStatus, String> {
    Ok(daemon::status(&app).await)
}

/// Run `mnemos service enable` to install the background service so hooks fire
/// outside active CLI sessions. Returns `{ enabled: true }` on success.
#[tauri::command]
pub async fn enable_service(app: AppHandle) -> Result<serde_json::Value, String> {
    let output = app
        .shell()
        .command("mnemos")
        .args(["service", "enable"])
        .output()
        .await
        .map_err(|e| format!("failed to launch mnemos: {e}"))?;
    if output.status.success() {
        Ok(serde_json::json!({ "enabled": true }))
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("mnemos service enable failed: {stderr}"))
    }
}

/// Orchestrate a vault move: validate → stop → write config → move →
/// start → wait healthy → finalize. On failure after the move, attempt revert.
#[tauri::command]
pub async fn move_vault(app: AppHandle, new_path: String) -> Result<MoveResult, String> {
    let cfg_path = config_io::config_path()?;
    let current = config_io::read_vault_root(&cfg_path)?
        .ok_or_else(|| "current vault location is unknown".to_string())?;
    let target = std::path::PathBuf::from(&new_path);

    vault_move::validate(&current, &target).map_err(|e| e.to_string())?;

    let port = config_io::read_daemon_port(&cfg_path)?.unwrap_or(7423);

    daemon::stop(&app).await?;
    // SIGTERM returns immediately; wait until the listener is actually gone so
    // the daemon no longer holds the SQLite DB open before we move it.
    daemon::wait_stopped(port, 10_000).await?;

    // Persist new location BEFORE moving so a restart reads the new path.
    config_io::write_vault_root(&cfg_path, &target)?;

    if let Err(e) = vault_move::execute(&current, &target) {
        let mut problems = vec![format!("move failed: {e}")];
        if let Err(ce) = config_io::write_vault_root(&cfg_path, &current) {
            problems.push(format!(
                "REVERT FAILED restoring config ({ce}) — your data is still at {} but config points elsewhere; manual recovery may be needed",
                current.display()
            ));
        }
        let _ = daemon::start(&app).await;
        return Err(problems.join("; "));
    }

    daemon::start(&app).await?;
    if let Err(e) = daemon::wait_healthy(port, 30_000).await {
        let mut problems = vec![format!("daemon unhealthy after move: {e}")];
        if let Err(re) = vault_move::execute(&target, &current) {
            problems.push(format!(
                "REVERT FAILED moving data back ({re}) — your data is at {} and may need manual recovery",
                target.display()
            ));
        }
        if let Err(ce) = config_io::write_vault_root(&cfg_path, &current) {
            problems.push(format!("REVERT FAILED restoring config ({ce})"));
        }
        let _ = daemon::start(&app).await;
        return Err(problems.join("; "));
    }

    Ok(MoveResult {
        moved_to: target.to_string_lossy().into_owned(),
    })
}
