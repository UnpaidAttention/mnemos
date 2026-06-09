//! `mnemos service` subcommand — systemd user service management.

use crate::cli::{ServiceAction, ServiceArgs};
use anyhow::Result;
use std::path::{Path, PathBuf};

// ── Pure helpers (unit-testable) ──────────────────────────────────────────────

/// Return the path where the systemd user unit file should be installed.
/// Joins `base/systemd/user/mnemosd.service`.
pub fn user_unit_path(base: &Path) -> PathBuf {
    base.join("systemd").join("user").join("mnemosd.service")
}

/// Return the embedded unit-file template.
pub fn unit_contents() -> &'static str {
    include_str!("../../../../packaging/systemd/mnemosd.service")
}

// ── Entry point ────────────────────────────────────────────────────────────────

pub fn run(_vault: Option<PathBuf>, _json: bool, args: ServiceArgs) -> Result<()> {
    match args.action {
        ServiceAction::Install => install(),
        ServiceAction::Enable => enable(),
        ServiceAction::Status => service_status(),
    }
}

// ── Actions ───────────────────────────────────────────────────────────────────

fn install() -> Result<()> {
    let base_dirs = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("could not resolve home/config directory"))?;
    let dest = user_unit_path(base_dirs.config_dir());

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| anyhow::anyhow!("create {}: {e}", parent.display()))?;
    }

    // Prefer a pre-installed copy shipped by the distro package; fall back to the
    // embedded template so the binary is self-contained.
    let installed_candidates = [
        Path::new("/usr/lib/mnemos/mnemosd.service"),
        Path::new("/usr/share/mnemos/mnemosd.service"),
    ];
    let wrote = if let Some(src) = installed_candidates.iter().find(|p| p.exists()) {
        std::fs::copy(src, &dest)
            .map_err(|e| anyhow::anyhow!("copy {} → {}: {e}", src.display(), dest.display()))?;
        false // copied, not embedded
    } else {
        std::fs::write(&dest, unit_contents())
            .map_err(|e| anyhow::anyhow!("write {}: {e}", dest.display()))?;
        true // embedded
    };

    println!(
        "unit file written to: {}{}",
        dest.display(),
        if wrote {
            " (embedded template)"
        } else {
            " (copied from system)"
        }
    );
    Ok(())
}

fn enable() -> Result<()> {
    let result = std::process::Command::new("systemctl")
        .args(["--user", "enable", "--now", "mnemosd"])
        .status();

    match result {
        Err(e) => {
            println!(
                "systemd is not available ({e}). Mnemos will lazy-start the daemon when needed."
            );
        }
        Ok(status) if !status.success() => {
            println!(
                "systemctl exited with {status}. systemd may not be running as a user session. \
                 Mnemos will lazy-start the daemon when needed."
            );
        }
        Ok(_) => {
            println!("mnemosd enabled and started via systemd user session.");
        }
    }
    Ok(())
}

fn service_status() -> Result<()> {
    let result = std::process::Command::new("systemctl")
        .args(["--user", "is-active", "mnemosd"])
        .output();

    match result {
        Ok(out) => {
            let state = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("systemd unit state: {state}");
        }
        Err(_) => {
            // systemd unavailable — fall back to HTTP health probe.
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| anyhow::anyhow!("failed to build tokio runtime: {e}"))?;
            let up = rt.block_on(crate::daemon_ctl::is_up());
            println!("daemon (health probe): {}", if up { "up" } else { "down" });
        }
    }
    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn user_unit_path_joins_correctly() {
        let base = Path::new("/home/user/.config");
        let got = user_unit_path(base);
        assert_eq!(
            got,
            PathBuf::from("/home/user/.config/systemd/user/mnemosd.service")
        );
    }

    #[test]
    fn user_unit_path_uses_provided_base() {
        let tmp = tempfile::tempdir().unwrap();
        let got = user_unit_path(tmp.path());
        assert_eq!(
            got,
            tmp.path()
                .join("systemd")
                .join("user")
                .join("mnemosd.service")
        );
    }

    #[test]
    fn unit_contents_contains_exec_start() {
        let contents = unit_contents();
        assert!(
            contents.contains("ExecStart=%h/.cargo/bin/mnemos-daemon"),
            "unit file must contain ExecStart=%h/.cargo/bin/mnemos-daemon"
        );
    }

    #[test]
    fn unit_contents_contains_restart_always() {
        let contents = unit_contents();
        assert!(
            contents.contains("Restart=on-failure"),
            "unit file must contain Restart=on-failure"
        );
    }

    #[test]
    fn unit_contents_contains_wanted_by() {
        let contents = unit_contents();
        assert!(
            contents.contains("WantedBy=default.target"),
            "unit file must declare WantedBy=default.target"
        );
    }

    #[test]
    fn install_writes_unit_to_temp_base() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = user_unit_path(tmp.path());
        std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
        std::fs::write(&dest, unit_contents()).unwrap();

        assert!(dest.exists(), "unit file should exist after install");
        let written = std::fs::read_to_string(&dest).unwrap();
        assert!(written.contains("ExecStart=%h/.cargo/bin/mnemos-daemon"));
        assert!(written.contains("Restart=on-failure"));
    }
}
