use crate::error::{MnemosError, Result};
use crate::paths::Paths;
use notify::{RecursiveMode, Watcher};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, FileIdMap};
use std::path::PathBuf;
use std::time::Duration;
use tokio::sync::mpsc::Sender;
use tracing::warn;

/// Events emitted by the vault file watcher.
#[derive(Debug, Clone)]
pub enum WatchEvent {
    /// An existing `.md` file was modified.
    Changed(PathBuf),
    /// An `.md` file was deleted.
    Removed(PathBuf),
    /// A new `.md` file appeared.
    Created(PathBuf),
}

/// Start watching the vault's files directory for changes.
///
/// Returns a [`Debouncer`] handle that **must be held alive** by the caller;
/// dropping it stops the underlying watcher thread.
///
/// Only `.md` files are forwarded to `tx`. Other files are silently ignored.
pub async fn watch_vault(
    paths: &Paths,
    tx: Sender<WatchEvent>,
) -> Result<Debouncer<notify::RecommendedWatcher, FileIdMap>> {
    let files_dir = paths.files_dir.clone();

    let mut debouncer = new_debouncer(
        Duration::from_millis(150),
        None,
        move |res: DebounceEventResult| {
            let events = match res {
                Ok(e) => e,
                Err(errs) => {
                    for e in errs {
                        warn!("watcher error: {e}");
                    }
                    return;
                }
            };
            for de in events {
                for path in de.event.paths {
                    if path.extension().and_then(|s| s.to_str()) != Some("md") {
                        continue;
                    }
                    use notify::EventKind::*;
                    let we = match de.event.kind {
                        Create(_) => WatchEvent::Created(path),
                        Modify(_) => WatchEvent::Changed(path),
                        Remove(_) => WatchEvent::Removed(path),
                        _ => continue,
                    };
                    // If the channel is closed, silently drop the event.
                    let _ = tx.try_send(we);
                }
            }
        },
    )
    .map_err(|e| MnemosError::Internal(format!("debouncer init: {e}")))?;

    debouncer
        .watcher()
        .watch(&files_dir, RecursiveMode::Recursive)
        .map_err(|e| MnemosError::Internal(format!("watch start: {e}")))?;

    Ok(debouncer)
}
