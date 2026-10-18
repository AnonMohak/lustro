use serde::Serialize;
use tauri::AppHandle;

use crate::error::Result;
use crate::system::{drives, elevation};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    version: String,
    is_admin: bool,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        is_admin: elevation::is_elevated(),
    }
}

#[tauri::command]
pub async fn list_drives() -> Vec<drives::DriveInfo> {
    // Volume queries can block on a slow USB drive, so keep them off the main thread.
    tauri::async_runtime::spawn_blocking(drives::list_drives)
        .await
        .unwrap_or_default()
}

/// Starts an elevated copy of Lustro and closes this one.
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> Result<()> {
    elevation::relaunch_elevated()?;
    app.exit(0);
    Ok(())
}
