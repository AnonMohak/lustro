//! System calls that the cleaner needs on Linux and macOS: running programs and the Trash.
//! Unix has no Windows services, and no folders that need a new owner first.

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

/// Lower-case names of all running programs, for example `google chrome`.
#[cfg(target_os = "macos")]
pub fn running_processes() -> HashSet<String> {
    std::process::Command::new("ps")
        .args(["-Axco", "comm="])
        .output()
        .map(|o| parse_ps(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// The output of `ps -c -o comm=`: one program name on each line.
#[cfg(any(target_os = "macos", test))]
fn parse_ps(text: &str) -> HashSet<String> {
    text.lines()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Android: running programs come in a later phase.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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

/// The size of all Trash folders (it needs Full Disk Access, else it is 0).
#[cfg(target_os = "macos")]
pub fn recycle_bin_size() -> Measure {
    crate::ops::trash_mac::folders()
        .iter()
        .map(|f| super::measure_dir(f, None))
        .fold(Measure::default(), |a, b| Measure {
            bytes: a.bytes + b.bytes,
            files: a.files + b.files,
        })
}

#[cfg(target_os = "macos")]
pub fn empty_recycle_bin() -> bool {
    crate::ops::trash_mac::empty()
}

/// Android: the Trash comes in a later phase.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn recycle_bin_size() -> Measure {
    Measure::default()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn empty_recycle_bin() -> bool {
    false
}

pub fn stop_service(_name: &str) -> bool {
    false
}

pub fn start_service(_name: &str) {}

pub fn take_ownership(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn sees_this_test_process() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_string_lossy().to_lowercase();
        let comm: String = name.chars().take(15).collect();
        assert!(running_processes().iter().any(|p| p.starts_with(&comm)));
    }

    #[test]
    fn reads_ps_output() {
        let names = parse_ps("Google Chrome\n  firefox \n\nXcode\n");
        assert!(names.contains("google chrome"));
        assert!(names.contains("firefox"));
        assert!(names.contains("xcode"));
        assert_eq!(names.len(), 3);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn trash_size_query_works() {
        let _ = recycle_bin_size();
    }
}
