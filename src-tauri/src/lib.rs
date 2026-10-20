// The Linux and macOS stubs leave shared scan code unused until those ports exist.
#![cfg_attr(not(windows), allow(dead_code))]

mod apps;
mod clean;
mod clean_cmds;
mod commands;
mod desktop;
mod dupes;
mod error;
mod favorites;
#[cfg(windows)]
mod live;
#[cfg(not(windows))]
mod live {
    /// Live updates on Linux and macOS come in a later phase.
    pub fn watch(_app: &tauri::AppHandle, _root: &str) {}
}
mod ops;
mod scan;
mod settings;
mod state;
mod system;
mod tools_cmds;

use tauri::Manager;

pub fn run() {
    let args: Vec<String> = std::env::args().collect();
    // After "Run as admin", give the old copy time to exit, so the single-instance check passes.
    if args.iter().any(|a| a == "--restarted") {
        std::thread::sleep(std::time::Duration::from_millis(1500));
    }
    let start_hidden = args.iter().any(|a| a == "--tray");

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            desktop::on_second_instance(app, args)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(state::AppState {
            startup_scan: std::sync::Mutex::new(commands::startup_scan_arg(
                args.clone().into_iter(),
            )),
            ..Default::default()
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            let loaded = settings::load(&desktop::data_dir(&handle));
            if let Ok(mut s) = app.state::<state::AppState>().settings.lock() {
                *s = loaded;
            }
            desktop::setup_tray(&handle)?;
            desktop::start_space_monitor(handle.clone());
            if !start_hidden {
                desktop::show_main(&handle);
            }
            Ok(())
        })
        .on_window_event(desktop::on_window_event)
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_drives,
            commands::restart_as_admin,
            commands::start_scan,
            commands::cancel_delete,
            commands::cancel_scan,
            commands::forget_scan,
            commands::load_cached_scans,
            commands::list_children,
            commands::node_detail,
            commands::treemap,
            commands::find_files,
            commands::search,
            commands::reveal_path,
            commands::take_startup_scan,
            commands::delete_items,
            commands::history,
            commands::restore,
            clean_cmds::clean_scan,
            clean_cmds::clean_run,
            tools_cmds::dupes_start,
            tools_cmds::dupes_cancel,
            tools_cmds::dupes_result,
            tools_cmds::apps_list,
            tools_cmds::app_uninstall,
            desktop::get_settings,
            desktop::set_settings,
            favorites::favorites,
            favorites::set_favorite,
            desktop::save_export,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lustro");
}
