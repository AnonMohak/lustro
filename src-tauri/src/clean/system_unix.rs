//! System calls that the cleaner needs on Linux and macOS: running programs and the Trash.
//! Linux has no Windows services, and no folders that need a new owner first.

use std::collections::HashSet;
use std::path::Path;

use super::Measure;

/// Lower-case names of all running programs, for example `firefox`.
/// The kernel cuts the name in `/proc/<pid>/comm` to 15 characters.
#[cfg(target_os = "linux")]
pub fn running_processes() -> HashSet<String> {
    std::fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_name()
                .to_string_lossy()
                .bytes()
                .all(|b| b.is_ascii_digit())
        })
        .filter_map(|e| std::fs::read_to_string(e.path().join("comm")).ok())
        .map(|name| name.trim().to_lowercase())
        .collect()
}

/// macOS: running programs come in a later phase.
#[cfg(not(target_os = "linux"))]
pub fn running_processes() -> HashSet<String> {
    HashSet::new()
}

/// The files folder of the Trash in the home folder (the freedesktop.org Trash).
#[cfg(target_os = "linux")]
fn home_trash() -> Option<std::path::PathBuf> {
    let data = std::env::var("XDG_DATA_HOME")
        .ok()
        .filter(|d| !d.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| Path::new(&h).join(".local/share"))
        })?;
    Some(data.join("Trash").join("files"))
}

/// The size of the Trash in the home folder. Trash folders on other disks are not counted,
/// but `empty_recycle_bin` empties them too.
#[cfg(target_os = "linux")]
pub fn recycle_bin_size() -> Measure {
    home_trash()
        .map(|t| super::measure_dir(&t, None))
        .unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub fn empty_recycle_bin() -> bool {
    trash::os_limited::list()
        .and_then(trash::os_limited::purge_all)
        .is_ok()
}

/// macOS: the `trash` crate cannot list the Trash. This comes in a later phase.
#[cfg(not(target_os = "linux"))]
pub fn recycle_bin_size() -> Measure {
    Measure::default()
}

#[cfg(not(target_os = "linux"))]
pub fn empty_recycle_bin() -> bool {
    false
}

pub fn stop_service(_name: &str) -> bool {
    false
}

pub fn start_service(_name: &str) {}

pub fn take_ownership(_path: &Path) {}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn sees_this_test_process() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_lowercase();
        let comm: String = name.chars().take(15).collect();
        assert!(running_processes().contains(&comm));
    }

    #[test]
    fn trash_size_query_works() {
        let _ = recycle_bin_size();
    }
}
