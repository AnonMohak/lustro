mod commands;
mod error;
mod system;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::list_drives,
            commands::restart_as_admin,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Lustro");
}
