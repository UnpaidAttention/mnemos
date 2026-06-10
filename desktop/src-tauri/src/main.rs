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
            commands::read_model_config,
            commands::install_ollama,
            commands::pull_model,
            commands::apply_llm_config,
            commands::apply_embedder_config,
            commands::check_for_updates,
            commands::install_update,
            commands::download_bundled_model,
            commands::check_downloaded_models,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                // 1. Clean up any stale Ollama models from previous sessions.
                //    This catches models left loaded by external tools (Claude
                //    Code, direct ollama run, etc.) or from a previous crash.
                crate::commands::unload_all_ollama_models().await;

                // 2. Start the daemon if it's not already running.
                let st = crate::daemon::status(&handle).await;
                if !st.running {
                    let _ = crate::daemon::start(&handle).await;
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building mnemos desktop")
        .run(|app_handle, event| {
            // On window close or app exit, unload all Ollama models to free
            // CPU/RAM so they don't linger after Mnemos is closed.
            if let tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit = event {
                let handle = app_handle.clone();
                tauri::async_runtime::block_on(async move {
                    crate::commands::unload_all_ollama_models().await;
                    let _ = crate::daemon::stop(&handle).await;
                });
            }
        });
}
