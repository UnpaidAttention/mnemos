use crate::{config_io, daemon, vault_move};
use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

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

/// Orchestrate a vault move: validate → stop → write config → move →
/// start → wait healthy → finalize. On failure after the move, attempt revert.
#[tauri::command]
pub async fn move_vault(app: AppHandle, new_path: String) -> Result<MoveResult, String> {
    let cfg_path = config_io::config_path()?;
    let current = config_io::read_vault_root(&cfg_path)?
        .ok_or_else(|| "current vault location is unknown".to_string())?;
    let target = std::path::PathBuf::from(&new_path);

    vault_move::validate(&current, &target).map_err(|e| e.to_string())?;

    daemon::stop(&app).await?;

    // Persist new location BEFORE moving so a restart reads the new path.
    config_io::write_vault_root(&cfg_path, &target)?;

    if let Err(e) = vault_move::execute(&current, &target) {
        let _ = config_io::write_vault_root(&cfg_path, &current);
        let _ = daemon::start(&app).await;
        return Err(format!("move failed: {e}"));
    }

    daemon::start(&app).await?;
    if let Err(e) = daemon::wait_healthy(7423, 30_000).await {
        let _ = vault_move::execute(&target, &current);
        let _ = config_io::write_vault_root(&cfg_path, &current);
        let _ = daemon::start(&app).await;
        return Err(format!("daemon unhealthy after move, reverted: {e}"));
    }

    Ok(MoveResult {
        moved_to: target.to_string_lossy().into_owned(),
    })
}
