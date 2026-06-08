use crate::{config_io, daemon, vault_move};
use futures_util::StreamExt;
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

/// Install the systemd user unit and enable the mnemos background service.
///
/// Instead of delegating to the `mnemos` sidecar (which requires a fully
/// packaged Tauri bundle), we directly:
///   1. Write the unit file to `~/.config/systemd/user/mnemosd.service`
///   2. Run `systemctl --user daemon-reload`
///   3. Run `systemctl --user enable --now mnemosd`
///
/// The unit file's `ExecStart` is resolved to the actual `mnemos-daemon`
/// binary discovered via `which`, falling back to `~/.cargo/bin/mnemos-daemon`
/// and finally `/usr/bin/mnemos-daemon`.
#[tauri::command]
pub async fn enable_service(_app: AppHandle) -> Result<serde_json::Value, String> {
    // 1. Resolve the daemon binary path.
    let daemon_bin = resolve_daemon_binary();

    // 2. Build the unit-file contents with the resolved binary path.
    let unit_template = include_str!("../../../packaging/systemd/mnemosd.service");
    let mut unit_contents =
        unit_template.replace("ExecStart=/usr/bin/mnemos-daemon", &format!("ExecStart={daemon_bin}"));

    // 3. For cargo-install (dev) setups, detect the project assets directory
    //    and inject Environment= directives so the daemon can find the
    //    bundled llama-server binary and GGUF models.
    if let Some(assets_dir) = resolve_assets_dir(&daemon_bin) {
        let env_lines = format!(
            "Environment=MNEMOS_BUNDLED_BIN_DIR={assets}\n\
             Environment=MNEMOS_BUNDLED_MODEL_DIR={assets}\n\
             Environment=LD_LIBRARY_PATH={assets}\n",
            assets = assets_dir.display()
        );
        // Insert the environment lines after the ExecStart line.
        unit_contents = unit_contents.replace(
            "Restart=always",
            &format!("{env_lines}Restart=always"),
        );
        // Bump MemoryMax from 2G to 4G to accommodate the LLM model.
        unit_contents = unit_contents.replace("MemoryMax=2G", "MemoryMax=4G");
    }

    // 4. Write the unit file.
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| "could not resolve home/config directory".to_string())?;
    let dest = base_dirs
        .config_dir()
        .join("systemd")
        .join("user")
        .join("mnemosd.service");
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&dest, &unit_contents)
        .map_err(|e| format!("write {}: {e}", dest.display()))?;

    // 5. Reload systemd so it picks up the new/updated unit file.
    let reload = std::process::Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status()
        .map_err(|e| format!("systemctl daemon-reload: {e}"))?;
    if !reload.success() {
        return Err("systemctl daemon-reload failed".into());
    }

    // 6. Enable and start the service.
    let enable = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "mnemosd"])
        .status()
        .map_err(|e| format!("systemctl enable: {e}"))?;
    if !enable.success() {
        return Err(format!("systemctl enable --now mnemosd exited with {enable}"));
    }

    Ok(serde_json::json!({ "enabled": true }))
}

/// Find the best `mnemos-daemon` binary, preferring `which` lookup, then
/// `~/.cargo/bin`, then the system default.
fn resolve_daemon_binary() -> String {
    // Try `which mnemos-daemon` first.
    if let Ok(out) = std::process::Command::new("which")
        .arg("mnemos-daemon")
        .output()
    {
        let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if out.status.success() && !path.is_empty() && std::path::Path::new(&path).exists() {
            return path;
        }
    }
    // Fallback: ~/.cargo/bin/mnemos-daemon
    if let Some(home) = std::env::var_os("HOME") {
        let cargo_bin = std::path::PathBuf::from(home)
            .join(".cargo")
            .join("bin")
            .join("mnemos-daemon");
        if cargo_bin.exists() {
            return cargo_bin.to_string_lossy().into_owned();
        }
    }
    // Final fallback: system path
    "/usr/bin/mnemos-daemon".to_string()
}

/// For cargo-install (dev) setups, find the `assets/` directory containing the
/// bundled llama-server binary and GGUF models. Returns `None` for packaged
/// installs where the assets live under `/usr/lib/mnemos/`.
fn resolve_assets_dir(daemon_bin: &str) -> Option<std::path::PathBuf> {
    // If the daemon is in /usr/bin, it's a packaged install — assets are at
    // /usr/lib/mnemos/ and the service template handles that via the wrapper.
    if daemon_bin.starts_with("/usr/") {
        return None;
    }
    // For cargo-install: try to find the project root by walking up from the
    // binary's location, or from the current executable's location.
    // Strategy: look for a known project file (Cargo.toml) from the process cwd.
    let cwd = std::env::current_dir().ok()?;
    // Walk up from cwd looking for `assets/llama-server-linux-x86_64`.
    let mut dir = cwd.as_path();
    loop {
        let candidate = dir.join("assets").join("llama-server-linux-x86_64");
        if candidate.exists() {
            return Some(dir.join("assets"));
        }
        dir = dir.parent()?;
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

// ─── Ollama + model management ─────────────────────────────────────────

#[derive(Serialize)]
pub struct OllamaStatus {
    pub installed: bool,
    pub running: bool,
    pub version: Option<String>,
    pub models: Vec<String>,
}

/// Detect if Ollama is installed, running, and which models are available.
#[tauri::command]
pub async fn check_ollama() -> Result<OllamaStatus, String> {
    // 1. Check if ollama binary exists.
    let installed = std::process::Command::new("which")
        .arg("ollama")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    if !installed {
        return Ok(OllamaStatus {
            installed: false,
            running: false,
            version: None,
            models: vec![],
        });
    }

    // 2. Get version.
    let version = std::process::Command::new("ollama")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    // 3. Check if the Ollama API is reachable + list models.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .map_err(|e| e.to_string())?;

    let (running, models) = match client.get("http://localhost:11434/api/tags").send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await.unwrap_or_default();
            let models = body
                .get("models")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| m.get("name").and_then(|n| n.as_str()).map(String::from))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            (true, models)
        }
        _ => (false, vec![]),
    };

    Ok(OllamaStatus {
        installed,
        running,
        version,
        models,
    })
}

/// Install Ollama using the official installer. Requires user to approve sudo.
#[tauri::command]
pub async fn install_ollama(app: AppHandle) -> Result<(), String> {
    use tauri::Emitter;
    app.emit("ollama-install-progress", "downloading").map_err(|e| e.to_string())?;

    // Download the install script first, then run it with pkexec for graphical sudo.
    let tmp_script = std::env::temp_dir().join("ollama-install.sh");
    let script_bytes = reqwest::get("https://ollama.com/install.sh")
        .await
        .map_err(|e| format!("download install script: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("read install script: {e}"))?;
    std::fs::write(&tmp_script, &script_bytes)
        .map_err(|e| format!("write install script: {e}"))?;

    app.emit("ollama-install-progress", "installing").map_err(|e| e.to_string())?;

    // Try pkexec (graphical sudo) first, fall back to running without sudo
    // (the install script handles the sudo internally in most cases).
    let script_path = tmp_script.clone();
    let output = tokio::task::spawn_blocking(move || {
        // First try with pkexec (graphical sudo prompt)
        let result = std::process::Command::new("pkexec")
            .args(["bash", &script_path.to_string_lossy()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output();

        // If pkexec fails (e.g. user cancelled), try without sudo
        match result {
            Ok(out) if out.status.success() => Ok(out),
            _ => std::process::Command::new("bash")
                .arg(&script_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output(),
        }
    })
    .await
    .map_err(|e| format!("join: {e}"))?
    .map_err(|e| format!("run install script: {e}"))?;

    let _ = std::fs::remove_file(&tmp_script);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = if stderr.is_empty() { "unknown error".to_string() } else { stderr.to_string() };
        return Err(format!("Ollama installation failed: {}. Manual install: curl -fsSL https://ollama.com/install.sh | sh", detail));
    }

    // Wait for the Ollama service to start (the installer starts it).
    for _ in 0..30 {
        if let Ok(resp) = reqwest::Client::new()
            .get("http://localhost:11434/api/tags")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            if resp.status().is_success() {
                app.emit("ollama-install-progress", "done").map_err(|e| e.to_string())?;
                return Ok(());
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    // Even if we can't reach it yet, the install succeeded.
    app.emit("ollama-install-progress", "done").map_err(|e| e.to_string())?;
    Ok(())
}

/// Pull (download) an Ollama model with streaming progress events.
#[tauri::command]
pub async fn pull_model(app: AppHandle, model: String) -> Result<(), String> {
    use tauri::Emitter;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600)) // models can be large
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .post("http://localhost:11434/api/pull")
        .json(&serde_json::json!({ "name": model }))
        .send()
        .await
        .map_err(|e| format!("pull request: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!(
            "Ollama pull failed (HTTP {})",
            resp.status()
        ));
    }

    // The Ollama pull API streams newline-delimited JSON progress lines.
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream: {e}"))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines.
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].to_string();
            buffer = buffer[pos + 1..].to_string();

            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
                let completed = v.get("completed").and_then(|c| c.as_u64()).unwrap_or(0);
                let total = v.get("total").and_then(|t| t.as_u64()).unwrap_or(0);

                let _ = app.emit(
                    "model-pull-progress",
                    serde_json::json!({
                        "model": model,
                        "status": status,
                        "completed": completed,
                        "total": total,
                    }),
                );
            }
        }
    }

    Ok(())
}

/// Write [llm] config and restart the daemon to apply.
#[tauri::command]
pub async fn apply_llm_config(
    app: AppHandle,
    kind: String,
    model: String,
) -> Result<(), String> {
    let cfg_path = config_io::config_path()?;

    // Determine the URL based on kind.
    let url = match kind.as_str() {
        "ollama" => "http://localhost:11434".to_string(),
        "bundled" => "http://127.0.0.1:7425".to_string(),
        _ => String::new(),
    };

    config_io::write_llm_config(&cfg_path, &kind, &model, &url)?;

    // Restart daemon to pick up the new config.
    let _ = daemon::stop(&app).await;
    let port = config_io::read_daemon_port(&cfg_path)?.unwrap_or(7423);
    let _ = daemon::wait_stopped(port, 5_000).await;
    daemon::start(&app).await?;
    daemon::wait_healthy(port, 30_000).await?;

    Ok(())
}

/// Write [embedder] config and restart the daemon to apply.
#[tauri::command]
pub async fn apply_embedder_config(
    app: AppHandle,
    kind: String,
    model: String,
    dim: u32,
) -> Result<(), String> {
    let cfg_path = config_io::config_path()?;

    let url = match kind.as_str() {
        "ollama" => "http://localhost:11434".to_string(),
        _ => String::new(),
    };

    config_io::write_embedder_config(&cfg_path, &kind, &model, &url, dim)?;

    // Restart daemon to pick up the new config.
    let _ = daemon::stop(&app).await;
    let port = config_io::read_daemon_port(&cfg_path)?.unwrap_or(7423);
    let _ = daemon::wait_stopped(port, 5_000).await;
    daemon::start(&app).await?;
    daemon::wait_healthy(port, 30_000).await?;

    Ok(())
}

// ─── In-app updates ────────────────────────────────────────────────────

const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const GITHUB_REPO: &str = "UnpaidAttention/mnemos";

#[derive(Serialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: String,
    /// Which asset to download: .deb or .rpm filename
    pub asset_name: Option<String>,
    pub asset_url: Option<String>,
}

/// Detect the Linux distro family: "debian" (.deb) or "fedora" (.rpm).
fn detect_distro() -> &'static str {
    // Check /etc/os-release for ID_LIKE or ID
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        let lower = content.to_lowercase();
        if lower.contains("fedora") || lower.contains("rhel") || lower.contains("centos") || lower.contains("suse") {
            return "fedora";
        }
    }
    // Also check if rpm command exists
    if std::process::Command::new("rpm").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
        if !std::process::Command::new("dpkg").arg("--version").output().map(|o| o.status.success()).unwrap_or(false) {
            return "fedora";
        }
    }
    "debian"
}

/// Simple semver comparison: returns true if `latest` is newer than `current`.
fn is_newer(current: &str, latest: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let s = s.trim_start_matches('v');
        let mut parts = s.split('.');
        let major = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        let patch = parts.next().and_then(|p| p.parse().ok()).unwrap_or(0);
        (major, minor, patch)
    };
    parse(latest) > parse(current)
}

/// Check GitHub releases for a newer version.
#[tauri::command]
pub async fn check_for_updates() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("mnemos-desktop")
        .build()
        .map_err(|e| e.to_string())?;

    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let resp = client.get(&url).send().await.map_err(|e| format!("GitHub API: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()));
    }

    let release: serde_json::Value = resp.json().await.map_err(|e| format!("parse: {e}"))?;

    let latest_tag = release.get("tag_name")
        .and_then(|t| t.as_str())
        .unwrap_or("v0.0.0")
        .to_string();
    let latest_version = latest_tag.trim_start_matches('v').to_string();
    let update_available = is_newer(CURRENT_VERSION, &latest_version);
    let release_url = release.get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .to_string();
    let release_notes = release.get("body")
        .and_then(|b| b.as_str())
        .unwrap_or("")
        .to_string();

    // Find the right asset for this distro
    let distro = detect_distro();
    let ext = if distro == "fedora" { ".rpm" } else { ".deb" };

    let (asset_name, asset_url) = release.get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| {
            assets.iter().find(|a| {
                a.get("name")
                    .and_then(|n| n.as_str())
                    .map(|n| n.ends_with(ext))
                    .unwrap_or(false)
            })
        })
        .map(|asset| {
            let name = asset.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
            let url = asset.get("browser_download_url").and_then(|u| u.as_str()).unwrap_or("").to_string();
            (Some(name), Some(url))
        })
        .unwrap_or((None, None));

    Ok(UpdateInfo {
        current_version: CURRENT_VERSION.to_string(),
        latest_version,
        update_available,
        release_url,
        release_notes,
        asset_name,
        asset_url,
    })
}

/// Download and install an update package.
#[tauri::command]
pub async fn install_update(app: AppHandle, asset_url: String, asset_name: String) -> Result<(), String> {
    use tauri::Emitter;
    app.emit("update-progress", "downloading").map_err(|e| e.to_string())?;

    // 1. Download the package to a temp location
    let tmp_dir = std::env::temp_dir().join("mnemos-update");
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("create temp dir: {e}"))?;
    let pkg_path = tmp_dir.join(&asset_name);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .user_agent("mnemos-desktop")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(&asset_url)
        .send()
        .await
        .map_err(|e| format!("download: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }

    let bytes = resp.bytes().await.map_err(|e| format!("read body: {e}"))?;
    std::fs::write(&pkg_path, &bytes).map_err(|e| format!("write package: {e}"))?;

    app.emit("update-progress", "installing").map_err(|e| e.to_string())?;

    // 2. Install using the appropriate package manager with pkexec (graphical sudo)
    let pkg_path_str = pkg_path.to_string_lossy().to_string();
    let is_rpm = asset_name.ends_with(".rpm");

    let pkg_path_clone = pkg_path.clone();
    let status = tokio::task::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("pkexec");
        if is_rpm {
            cmd.args(["rpm", "-U", "--force", &pkg_path_str]);
        } else {
            cmd.args(["dpkg", "-i", &pkg_path_str]);
        }
        cmd.stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status()
    })
    .await
    .map_err(|e| format!("join: {e}"))?
    .map_err(|e| format!("run installer: {e}"))?;

    // Clean up
    let _ = std::fs::remove_file(&pkg_path_clone);
    let _ = std::fs::remove_dir(&tmp_dir);

    if !status.success() {
        return Err("Installation failed. You may need to install manually.".into());
    }

    app.emit("update-progress", "done").map_err(|e| e.to_string())?;

    Ok(())
}
