//! Keeps fast-scan trees current with the USN change journal (admin only).

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::scan::model::Tree;
use crate::scan::{cache, is_drive_root, usn};
use crate::state::AppState;
use crate::system::elevation;

const POLL: Duration = Duration::from_secs(3);
const SAVE_EVERY: Duration = Duration::from_secs(60);

#[derive(Serialize, Clone)]
struct Stale {
    root: String,
}

/// Starts watching a tree, if it came from a fast scan and Lustro runs as admin.
/// A newer call for the same root replaces the older watcher.
pub fn watch(app: &AppHandle, root: &str) {
    if !elevation::is_elevated() {
        return;
    }
    let Some(letter) = is_drive_root(root) else {
        return;
    };
    let state = app.state::<AppState>();
    let Some(tree) = state.tree(root) else { return };
    let Some((journal, _)) = tree.usn else { return };
    let generation = {
        let Ok(mut g) = state.watch_gen.lock() else {
            return;
        };
        let e = g.entry(root.to_string()).or_insert(0);
        *e += 1;
        *e
    };
    let (app, root) = (app.clone(), root.to_string());
    std::thread::spawn(move || {
        let Ok(vol) = usn::open_volume(letter) else {
            return;
        };
        let mut dirty = false;
        let mut last_save = Instant::now();
        loop {
            std::thread::sleep(POLL);
            let state = app.state::<AppState>();
            let current = state
                .watch_gen
                .lock()
                .map(|g| g.get(&root).copied() == Some(generation))
                .unwrap_or(false);
            if !current {
                break;
            }
            let Some(next) = state.tree(&root).and_then(|t| t.usn).map(|(_, n)| n) else {
                break;
            };
            match usn::read(&vol, journal, next) {
                Err(usn::UsnError::Reset) => {
                    let _ = app.emit("tree-stale", Stale { root: root.clone() });
                    break;
                }
                Err(e) => {
                    log::warn!("Live updates of {root} stopped: {e}");
                    break;
                }
                Ok((changes, new_next)) => {
                    let mut summary = None;
                    if let Ok(mut trees) = state.trees.write()
                        && let Some(t) = trees.get_mut(&root)
                    {
                        let t = Arc::make_mut(t);
                        let n = usn::apply(t, &changes, &usn::stat_path);
                        t.usn = Some((journal, new_next));
                        if n > 0 {
                            dirty = true;
                            summary = Some(crate::commands::summary(t));
                        }
                    }
                    if let Some(s) = summary {
                        let _ = app.emit("tree-changed", s);
                    }
                }
            }
            if dirty && last_save.elapsed() >= SAVE_EVERY {
                save(&app, &root);
                dirty = false;
                last_save = Instant::now();
            }
        }
        if dirty {
            save(&app, &root);
        }
    });
}

fn save(app: &AppHandle, root: &str) {
    let state = app.state::<AppState>();
    let tree: Option<Arc<Tree>> = state.tree(root);
    if let (Some(t), Ok(dir)) = (tree, app.path().app_local_data_dir())
        && let Err(e) = cache::save(&t, &dir.join("cache"))
    {
        log::warn!("Could not save the scan cache: {e}");
    }
}
