mod clean;
mod clean_cmds;
mod commands;
mod error;
mod ops;
mod scan;
mod state;
mod system;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState {
            startup_scan: std::sync::Mutex::new(commands::startup_scan_arg(std::env::args())),
            ..Default::default()
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_drives,
            commands::restart_as_admin,
            commands::start_scan,
            commands::cancel_scan,
            commands::load_cached_scans,
            commands::list_children,
            commands::node_detail,
            commands::treemap,
            commands::find_files,
            commands::reveal_path,
            commands::take_startup_scan,
            commands::delete_items,
            commands::history,
            commands::restore,
            clean_cmds::clean_scan,
            clean_cmds::clean_run,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lustro");
}
