//! Desktop integration: tray icon, low space alerts and actions, space history, automatic
//! rescans, close-to-tray, export files.

use std::collections::HashMap;
use std::time::Duration;

#[cfg(desktop)]
use tauri::Emitter;
#[cfg(desktop)]
use tauri::menu::{Menu, MenuItem};
#[cfg(desktop)]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;

use crate::error::{Error, Result};
use crate::settings::{self, Settings};
use crate::state::AppState;
use crate::system::drives;

const CHECK_EVERY: Duration = Duration::from_secs(15 * 60);
const FIRST_CHECK: Duration = Duration::from_secs(60);

pub fn data_dir(app: &AppHandle) -> std::path::PathBuf {
    app.path()
        .app_local_data_dir()
        .unwrap_or_else(|_| std::env::temp_dir().join("Lustro"))
}

pub fn current_settings(app: &AppHandle) -> Settings {
    let state = app.state::<AppState>();
    state.settings.lock().map(|s| s.clone()).unwrap_or_default()
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        #[cfg(desktop)]
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Builds the tray icon and its menu.
#[cfg(desktop)]
pub fn setup_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Lustro", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("lustro")
        .tooltip("Lustro")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| match e.id.as_ref() {
            "open" => show_main(app),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                show_main(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

/// Closing the window hides it when "keep in tray" is on.
pub fn on_window_event(window: &tauri::Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event
        && current_settings(window.app_handle()).keep_in_tray
    {
        api.prevent_close();
        let _ = window.hide();
    }
}

/// "Drive C: is almost full" on Windows; other platforms name the mount point ("Drive / is ...").
fn drive_full_title(letter: &str) -> String {
    if cfg!(windows) {
        format!("Drive {letter}: is almost full")
    } else {
        format!("Drive {letter} is almost full")
    }
}

/// Runs the low space actions of Settings for drive `d`: each chosen rule whose folder is
/// on the drive, and the chosen clean items when the drive holds the home folder.
/// Returns the freed bytes.
fn low_space_actions(app: &AppHandle, s: &Settings, d: &drives::DriveInfo) -> u64 {
    let mut freed = 0;
    if !s.low_space_rules.is_empty() {
        for rule in crate::clean::custom::load(&data_dir(app)) {
            if s.low_space_rules.contains(&rule.id) && crate::paths::inside(&rule.root, &d.root) {
                freed += crate::custom_cmds::run_rule(app, &rule).freed_bytes;
            }
        }
    }
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" });
    if !s.low_space_clean.is_empty() && home.is_ok_and(|h| crate::paths::inside(&h, &d.root)) {
        freed += crate::clean_cmds::run_quietly(app, &s.low_space_clean);
    }
    freed
}

/// Records today's used space of each drive (the space history on Home).
fn record_space(app: &AppHandle) {
    let dir = data_dir(app);
    let mut log = crate::growth::SpaceLog::load(&dir);
    let day = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86_400) as u32)
        .unwrap_or(0);
    for d in drives::list_drives() {
        if d.total_bytes > 0 {
            log.record(
                &d.root,
                crate::growth::Point {
                    day,
                    used: d.total_bytes - d.free_bytes,
                    total: d.total_bytes,
                },
            );
        }
    }
    if let Err(e) = log.save(&dir) {
        log::warn!("Could not save the space history: {e}");
    }
}

/// Starts an automatic rescan of the oldest scan that is `days` days old or more. One at a
/// time: the next check starts the next one.
fn rescan_old(app: &AppHandle, days: u32) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let oldest = app.state::<AppState>().trees.read().ok().and_then(|trees| {
        trees
            .iter()
            .filter(|(_, t)| now.saturating_sub(t.scanned_at) >= days as u64 * 86_400)
            .min_by_key(|(_, t)| t.scanned_at)
            .map(|(root, _)| root.clone())
    });
    if let Some(root) = oldest {
        // Busy when the user scans; the next check tries again.
        let _ = crate::commands::begin_scan(app, &root, true);
    }
}

/// Checks free space every 15 minutes and sends one notification per drive per day.
pub fn start_space_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut alerted: HashMap<String, u64> = HashMap::new();
        std::thread::sleep(FIRST_CHECK);
        loop {
            let s = current_settings(&app);
            record_space(&app);
            if s.rescan_days > 0 {
                rescan_old(&app, s.rescan_days);
            }
            if s.low_space_alert {
                let today = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() / 86_400)
                    .unwrap_or(0);
                for d in drives::list_drives() {
                    if d.total_bytes == 0 || d.kind == drives::DriveKind::Removable {
                        continue;
                    }
                    let pct = d.free_bytes as f64 * 100.0 / d.total_bytes as f64;
                    if pct < s.low_space_percent as f64 && alerted.get(&d.letter) != Some(&today) {
                        alerted.insert(d.letter.clone(), today);
                        let gb = |b: u64| b as f64 / 1024f64.powi(3);
                        let freed = low_space_actions(&app, &s, &d);
                        let body = if freed > 0 {
                            let now_free = drives::list_drives()
                                .into_iter()
                                .find(|x| x.letter == d.letter)
                                .map_or(d.free_bytes + freed, |x| x.free_bytes);
                            format!(
                                "Your low space actions freed {:.1} GB. Now {:.1} GB free.",
                                gb(freed),
                                gb(now_free)
                            )
                        } else {
                            format!(
                                "{pct:.0}% free ({:.1} GB). Open Lustro to free space.",
                                gb(d.free_bytes)
                            )
                        };
                        let _ = app
                            .notification()
                            .builder()
                            .title(drive_full_title(&d.letter))
                            .body(body)
                            .show();
                    }
                }
            }
            std::thread::sleep(CHECK_EVERY);
        }
    });
}

/// Handles a second start of Lustro (for example from the Explorer menu).
#[cfg(desktop)]
pub fn on_second_instance(app: &AppHandle, args: Vec<String>) {
    show_main(app);
    if let Some(path) = crate::commands::startup_scan_arg(args.into_iter()) {
        let _ = app.emit("open-scan", path);
    }
}

#[tauri::command]
pub fn get_settings(app: AppHandle) -> Settings {
    current_settings(&app)
}

/// Saves the settings and updates the Windows registry entries that they control.
#[tauri::command]
pub fn set_settings(app: AppHandle, settings: Settings) -> Result<Settings> {
    let old = current_settings(&app);
    if settings.start_with_windows != old.start_with_windows || !settings::registry_matches(&old) {
        settings::set_start_with_windows(settings.start_with_windows)?;
    }
    if settings.explorer_menu != old.explorer_menu || !settings::registry_matches(&old) {
        settings::set_explorer_menu(settings.explorer_menu)?;
    }
    let mut s = settings;
    // The Settings screen does not send favourites; set_favorite changes them.
    s.favorites = old.favorites;
    s.low_space_percent = s.low_space_percent.clamp(1, 50);
    settings::save(&data_dir(&app), &s)?;
    if let Ok(mut cur) = app.state::<AppState>().settings.lock() {
        *cur = s.clone();
    }
    Ok(s)
}

/// Asks where to save, then writes the export. Returns the path, or None when the user cancels.
#[tauri::command]
pub async fn save_export(
    app: AppHandle,
    kind: String,
    file_name: String,
    contents: String,
) -> Result<Option<String>> {
    let (label, ext) = match kind.as_str() {
        "csv" => ("CSV file", "csv"),
        "html" => ("Web page", "html"),
        _ => return Err(Error::Io(std::io::Error::other("Unknown export type."))),
    };
    let picked = app
        .dialog()
        .file()
        .add_filter(label, &[ext])
        .set_file_name(&file_name)
        .blocking_save_file();
    let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(None);
    };
    std::fs::write(&path, contents.as_bytes())?;
    Ok(Some(path.to_string_lossy().into_owned()))
}
