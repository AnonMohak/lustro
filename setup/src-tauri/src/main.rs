//! Lustro Setup: a themed window around the Lustro NSIS installer.
//!
//! `lustro-setup.exe` installs, updates or repairs Lustro. A copy in the install folder,
//! started with `--uninstall`, is the uninstaller that Windows Settings runs.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ops;

use std::path::PathBuf;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::utils::config::WindowEffectsConfig;
use tauri::window::Effect;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

/// How this process started.
struct Mode {
    uninstall: bool,
    /// The folder to uninstall from (given to the copy in %TEMP%).
    dir: Option<PathBuf>,
    /// This exe is a copy in %TEMP% that deletes itself at exit.
    temp_copy: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    version: String,
    uninstall: bool,
    /// The installed version and folder, if Lustro is installed.
    installed: Option<InstalledDto>,
    default_dir: String,
    license: &'static str,
    has_payload: bool,
    app_running: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledDto {
    version: String,
    dir: String,
    /// -1: older than this setup, 0: the same, 1: newer.
    compare: i8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallArgs {
    dir: String,
    desktop: bool,
    start_with_windows: bool,
    explorer_menu: bool,
    /// Update or repair an existing install: keep its folder, shortcuts and options.
    update: bool,
}

#[tauri::command]
fn info(mode: State<'_, Mutex<Mode>>) -> Info {
    let mode = mode.lock().unwrap();
    let installed = ops::installed();
    Info {
        version: ops::VERSION.into(),
        uninstall: mode.uninstall,
        app_running: ops::app_running(installed.as_ref().map_or("lustro.exe", |i| i.exe.as_str())),
        installed: installed.map(|i| InstalledDto {
            compare: ops::compare(&i.version, ops::VERSION) as i8,
            version: i.version,
            dir: mode.dir.as_ref().unwrap_or(&i.dir).display().to_string(),
        }),
        default_dir: ops::default_dir().display().to_string(),
        license: include_str!("../../../LICENSE"),
        has_payload: !ops::PAYLOAD.is_empty(),
    }
}

#[tauri::command]
async fn install(app: AppHandle, args: InstallArgs) -> Result<(), String> {
    let opts = ops::Options {
        desktop: args.desktop,
        start_with_windows: args.start_with_windows,
        explorer_menu: args.explorer_menu,
    };
    tauri::async_runtime::spawn_blocking(move || {
        ops::install(
            PathBuf::from(args.dir.trim()).as_path(),
            &opts,
            args.update,
            |s| {
                let _ = app.emit("step", s);
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn uninstall(app: AppHandle, delete_data: bool) -> Result<(), String> {
    let dir = {
        let mode = app.state::<Mutex<Mode>>();
        let given = mode.lock().unwrap().dir.clone();
        given
            .or_else(|| ops::installed().map(|i| i.dir))
            .ok_or("Lustro is not installed.")?
    };
    tauri::async_runtime::spawn_blocking(move || {
        ops::uninstall(&dir, delete_data, |s| {
            let _ = app.emit("step", s);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn launch() -> Result<(), String> {
    ops::launch()
}

fn parse_args() -> Result<Mode, i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let uninstall = args.iter().any(|a| a == "--uninstall");
    let nsis_args: Vec<String> = args
        .iter()
        .filter(|a| a.starts_with('/') || a.starts_with("_?="))
        .cloned()
        .collect();
    // The NSIS installer runs the registered uninstaller with its own flags: hand them on.
    if uninstall && !nsis_args.is_empty() {
        return Err(ops::pass_through(&nsis_args));
    }
    let dir = args
        .iter()
        .position(|a| a == "--dir")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let temp_copy = args.iter().any(|a| a == "--temp-copy");

    // Windows cannot delete a running exe, so the uninstaller in the install folder
    // runs a copy of itself from %TEMP% and ends.
    if uninstall && !temp_copy {
        let me = std::env::current_exe().map_err(|_| 2)?;
        let dir = me.parent().map(PathBuf::from).ok_or(2)?;
        let copy =
            std::env::temp_dir().join(format!("lustro-uninstall-{}.exe", std::process::id()));
        std::fs::copy(&me, &copy).map_err(|_| 2)?;
        std::process::Command::new(&copy)
            .arg("--uninstall")
            .arg("--temp-copy")
            .arg("--dir")
            .arg(&dir)
            .spawn()
            .map_err(|_| 2)?;
        return Err(0);
    }
    Ok(Mode {
        uninstall,
        dir,
        temp_copy,
    })
}

fn main() {
    let mode = match parse_args() {
        Ok(m) => m,
        Err(code) => std::process::exit(code),
    };
    let temp_copy = mode.temp_copy;
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Mutex::new(mode))
        .setup(|app| {
            // Keep the WebView data in %TEMP%, so the setup leaves no folder in AppData.
            let data = std::env::temp_dir().join("lustro-setup-webview");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Lustro Setup")
                .inner_size(640.0, 480.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .transparent(true)
                .shadow(true)
                .effects(WindowEffectsConfig {
                    effects: vec![Effect::Mica],
                    ..Default::default()
                })
                .center()
                .data_directory(data)
                .build()?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![info, install, uninstall, launch])
        .build(tauri::generate_context!())
        .expect("error while building Lustro Setup")
        .run(move |_, event| {
            if let tauri::RunEvent::Exit = event
                && temp_copy
            {
                ops::delete_self_later();
            }
        });
}
