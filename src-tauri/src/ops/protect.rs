//! Paths that Lustro never deletes. The backend checks these before every delete;
//! the UI only shows the result. Each platform has its own rules.

// The Unix tables are pure string checks, so their tests run on every platform.
#[cfg(any(target_os = "android", test))]
mod android;
#[cfg(any(target_os = "linux", test))]
mod linux;
#[cfg(any(target_os = "macos", test))]
mod macos;
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android", test))]
mod unix;
#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::{cleaner_may_delete, is_app_folder, is_protected};

#[cfg(target_os = "android")]
use android::RULES;
#[cfg(target_os = "linux")]
use linux::RULES;
#[cfg(target_os = "macos")]
use macos::RULES;

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn home() -> Option<String> {
    std::env::var("HOME").ok()
}

/// Android: the home folders are the user folders in `/storage/emulated` (see the table).
#[cfg(target_os = "android")]
fn home() -> Option<String> {
    None
}

/// True when Lustro must not delete `path`, an absolute path like `/usr/lib`.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android"))]
pub fn is_protected(path: &str) -> bool {
    unix::protected(&RULES, path, home().as_deref())
}

/// True when the cleaner may delete `path`: the path is not protected, or it lies
/// inside one of the cleaner exceptions (but is not the exception folder itself).
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android"))]
pub fn cleaner_may_delete(path: &str) -> bool {
    unix::cleaner_may_delete(&RULES, path, home().as_deref())
}

/// True when `path` is in an app folder (`/opt`, `/Applications`). An admin may delete it after a warning.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "android"))]
pub fn is_app_folder(path: &str) -> bool {
    unix::is_app_folder(&RULES, path)
}
