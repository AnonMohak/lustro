//! Tauri commands for the duplicate finder and the installed apps.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::apps;
use crate::dupes::{self, DupeGroup, DupeProgress};
use crate::error::{Error, Result};
use crate::state::AppState;

fn all_trees(app: &AppHandle) -> Vec<Arc<crate::scan::model::Tree>> {
    let state = app.state::<AppState>();
    state
        .trees
        .read()
        .map(|t| t.values().cloned().collect())
        .unwrap_or_default()
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DupeProgressEvent {
    total_bytes: u64,
    hashed_bytes: u64,
    groups: u64,
}

/// Starts the duplicate search over all scans. Events: `dupes-progress`, `dupes-done`, `dupes-error`.
#[tauri::command]
pub fn dupes_start(
    app: AppHandle,
    state: State<'_, AppState>,
    min_size: u64,
    include_app_data: Option<bool>,
) -> Result<()> {
    let progress = Arc::new(DupeProgress::default());
    {
        let mut active = state.dupes_active.lock().map_err(|_| Error::Busy)?;
        if active.is_some() {
            return Err(Error::Busy);
        }
        *active = Some(progress.clone());
    }
    let trees = all_trees(&app);
    if trees.is_empty() {
        if let Ok(mut a) = state.dupes_active.lock() {
            *a = None;
        }
        return Err(Error::NoScan);
    }
    {
        let (app, progress) = (app.clone(), progress.clone());
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            loop {
                std::thread::sleep(Duration::from_millis(200));
                let running = state
                    .dupes_active
                    .lock()
                    .map(|a| a.is_some())
                    .unwrap_or(false);
                if !running {
                    break;
                }
                let _ = app.emit(
                    "dupes-progress",
                    DupeProgressEvent {
                        total_bytes: progress.total_bytes.load(Ordering::Relaxed),
                        hashed_bytes: progress.hashed_bytes.load(Ordering::Relaxed),
                        groups: progress.groups.load(Ordering::Relaxed),
                    },
                );
            }
        });
    }
    std::thread::spawn(move || {
        let groups = dupes::find(
            &trees,
            min_size,
            include_app_data.unwrap_or(false),
            &progress,
        );
        let state = app.state::<AppState>();
        if let Ok(mut a) = state.dupes_active.lock() {
            *a = None;
        }
        if progress.is_cancelled() {
            let _ = app.emit("dupes-error", "cancelled");
            return;
        }
        if let Ok(mut r) = state.dupes.lock() {
            *r = Some(groups.clone());
        }
        let _ = app.emit("dupes-done", groups);
    });
    Ok(())
}

#[tauri::command]
pub fn dupes_cancel(state: State<'_, AppState>) {
    if let Ok(a) = state.dupes_active.lock()
        && let Some(p) = a.as_ref()
    {
        p.cancel();
    }
}

/// The last result, if any (for when the screen opens again).
#[tauri::command]
pub fn dupes_result(state: State<'_, AppState>) -> Option<Vec<DupeGroup>> {
    state.dupes.lock().ok()?.clone()
}

#[tauri::command]
pub async fn apps_list(app: AppHandle) -> Result<Vec<apps::App>> {
    let trees = all_trees(&app);
    let list = tauri::async_runtime::spawn_blocking(move || apps::list(&trees))
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?;
    if let Some(state) = app.try_state::<AppState>()
        && let Ok(mut a) = state.apps.lock()
    {
        *a = list.clone();
    }
    Ok(list)
}

/// Starts the uninstaller of an app from the last `apps_list` result.
#[tauri::command]
pub async fn app_uninstall(app: AppHandle, id: String) -> Result<()> {
    let target = {
        let state = app.state::<AppState>();
        let list = state.apps.lock().map_err(|_| Error::Busy)?;
        list.iter()
            .find(|a| a.id == id)
            .cloned()
            .ok_or(Error::NoScan)?
    };
    tauri::async_runtime::spawn_blocking(move || apps::uninstall(&target))
        .await
        .map_err(|e| Error::Io(std::io::Error::other(e.to_string())))?
        .map_err(Error::Uninstall)
}
