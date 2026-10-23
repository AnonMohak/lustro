//! Paths that Lustro never deletes. The backend checks these before every delete;
//! the UI only shows the result. Each platform has its own rules.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod win;
#[cfg(target_os = "linux")]
pub use linux::{cleaner_may_delete, is_app_folder, is_protected};
#[cfg(windows)]
pub use win::{cleaner_may_delete, is_app_folder, is_protected};

/// macOS and Android have no rules yet, so every path is protected and nothing is deleted.
#[cfg(not(any(windows, target_os = "linux")))]
pub fn is_protected(_path: &str) -> bool {
    true
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn is_app_folder(_path: &str) -> bool {
    false
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn cleaner_may_delete(_path: &str) -> bool {
    false
}
