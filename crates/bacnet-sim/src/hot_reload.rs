/// Config hot-reload: watches a TOML topology file for changes and logs when
/// it is modified.  Actual live re-application of objects is a future
/// enhancement; this module provides the watcher scaffolding.
use std::{path::PathBuf, sync::Arc, time::Duration};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tracing::{info, warn};

use bacnet_object::store::ObjectStore;

/// Watch `config_path` for modifications and log each change.
/// Runs indefinitely; call from a spawned task.
pub async fn watch(config_path: String, _store: Arc<ObjectStore>) {
    let path = PathBuf::from(&config_path);
    let (tx, mut rx) = mpsc::channel::<notify::Result<Event>>(16);

    let mut watcher: RecommendedWatcher = match notify::recommended_watcher(move |res| {
        let _ = tx.blocking_send(res);
    }) {
        Ok(w) => w,
        Err(e) => {
            warn!("Hot-reload: failed to create watcher: {e}");
            return;
        }
    };

    // Watch the directory containing the config file so renames are caught.
    let watch_path = path.parent().unwrap_or(&path);
    if let Err(e) = watcher.watch(watch_path, RecursiveMode::NonRecursive) {
        warn!("Hot-reload: failed to watch {watch_path:?}: {e}");
        return;
    }

    info!(path = %config_path, "Hot-reload: watching config file for changes");

    // Debounce: ignore events within 500ms of the last one.
    let mut last_reload = std::time::Instant::now() - Duration::from_secs(10);

    while let Some(event) = rx.recv().await {
        match event {
            Ok(e) => {
                let is_our_file = e.paths.iter().any(|p| p.ends_with(&path));
                let is_write = matches!(e.kind, EventKind::Modify(_) | EventKind::Create(_));
                if is_our_file && is_write {
                    let now = std::time::Instant::now();
                    if now.duration_since(last_reload) >= Duration::from_millis(500) {
                        last_reload = now;
                        info!(path = %config_path, "Hot-reload: config file changed — restart to apply");
                        // Future: diff the new config, add/remove objects, restart engines.
                    }
                }
            }
            Err(e) => warn!("Hot-reload: watcher error: {e}"),
        }
    }
}
