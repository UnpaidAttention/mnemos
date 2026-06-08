// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod config_io;
mod daemon;
mod vault_move;

/// Read the daemon bearer token from `~/.config/mnemos/token`. Kept in the Rust
/// shell so the secret never lives in renderer-accessible env or storage.
#[tauri::command]
fn read_token() -> Result<String, String> {
    let dirs = directories::ProjectDirs::from("dev", "mnemos", "mnemos")
        .ok_or_else(|| "could not resolve config dir".to_string())?;
    let path = dirs.config_dir().join("token");
    std::fs::read_to_string(&path)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("read token {}: {e}", path.display()))
}

fn main() {
    tauri::Builder::default()
        // .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            read_token,
            commands::pick_vault_dir,
            commands::daemon_status,
            commands::move_vault,
            commands::enable_service,
            commands::check_ollama,
            commands::install_ollama,
            commands::pull_model,
            commands::apply_llm_config,
            commands::apply_embedder_config,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let st = crate::daemon::status(&handle).await;
                if !st.running {
                    let _ = crate::daemon::start(&handle).await;
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running mnemos desktop");
}
