// Prevents an extra console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config_io;

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .invoke_handler(tauri::generate_handler![read_token])
        .run(tauri::generate_context!())
        .expect("error while running mnemos desktop");
}
