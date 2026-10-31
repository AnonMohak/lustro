// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // `lustro scan|clean|dupes|rules ...` runs on the command line, without a window.
    let args: Vec<String> = std::env::args().collect();
    if let Some(code) = lustro_lib::cli::run(&args) {
        std::process::exit(code);
    }
    // WebKitGTK shows a black window when the DMA-BUF renderer has no GPU access (WSL, some VMs and drivers).
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: no other thread runs yet.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    lustro_lib::run()
}
