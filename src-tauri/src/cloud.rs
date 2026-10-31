//! Cloud free-up: big old files in OneDrive (Windows) or iCloud Drive (macOS) become
//! online-only. They stay in the cloud and come back when the user opens them.
//! Windows: the "unpinned" attribute, like "Free up space" in File Explorer; OneDrive then
//! removes the local copy. macOS: `brctl evict`. Other systems have no standard way.

use std::sync::Arc;

use serde::Serialize;

use crate::scan::model::{Tree, flags};

/// Smaller files are not worth it.
const MIN_SIZE: u64 = 10 * 1024 * 1024;
/// Only files with no change for this many days.
const MIN_AGE_DAYS: u64 = 90;
const MAX_FILES: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFile {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudFolder {
    /// "OneDrive" or "iCloud Drive".
    pub provider: &'static str,
    pub root: String,
    /// Biggest first.
    pub files: Vec<CloudFile>,
}

/// The synced folders of this computer.
#[cfg(windows)]
pub fn roots() -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    for var in ["OneDriveConsumer", "OneDriveCommercial", "OneDrive"] {
        if let Ok(p) = std::env::var(var)
            && !p.is_empty()
            && !out.iter().any(|(_, r)| crate::paths::same(r, &p))
        {
            out.push(("OneDrive", p));
        }
    }
    out
}

#[cfg(target_os = "macos")]
pub fn roots() -> Vec<(&'static str, String)> {
    std::env::var("HOME")
        .map(|h| {
            vec![(
                "iCloud Drive",
                format!("{h}/Library/Mobile Documents/com~apple~CloudDocs"),
            )]
        })
        .unwrap_or_default()
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn roots() -> Vec<(&'static str, String)> {
    Vec::new()
}

fn now_filetime() -> u64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs + 11_644_473_600) * 10_000_000
}

/// The files of the subtree `top` that can go online-only.
fn collect(t: &Tree, top: u32, before: u64, ignore: &[String], out: &mut Vec<CloudFile>) {
    let mut stack = vec![top];
    while let Some(id) = stack.pop() {
        for c in t.children(id) {
            let n = t.node(c);
            if n.flags & (flags::LINK | flags::ONLINE_ONLY) != 0 {
                continue;
            }
            if n.is_dir() {
                stack.push(c);
            } else if n.size >= MIN_SIZE && n.modified > 0 && n.modified < before {
                let path = t.path(c);
                if !crate::paths::ignored(ignore, &path) {
                    out.push(CloudFile { path, size: n.size });
                }
            }
        }
    }
}

/// Big old local files in the synced folders, from the scans.
pub fn candidates(trees: &[Arc<Tree>], ignore: &[String]) -> Vec<CloudFolder> {
    let before = now_filetime().saturating_sub(MIN_AGE_DAYS * 86_400 * 10_000_000);
    let mut out = Vec::new();
    for (provider, root) in roots() {
        let mut files = Vec::new();
        for t in trees {
            // The scan holds the synced folder, or lies inside it.
            let top = if crate::paths::inside(&t.root_path, &root) {
                Some(Tree::ROOT)
            } else {
                t.find_path(&root)
            };
            if let Some(top) = top {
                collect(t, top, before, ignore, &mut files);
            }
        }
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
        files.dedup_by(|a, b| a.path == b.path);
        files.truncate(MAX_FILES);
        if !files.is_empty() {
            out.push(CloudFolder {
                provider,
                root,
                files,
            });
        }
    }
    out
}

/// Makes one file online-only.
#[cfg(windows)]
pub fn free(path: &str) -> Result<(), String> {
    use windows::Win32::Storage::FileSystem::{
        FILE_FLAGS_AND_ATTRIBUTES, GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
    };
    use windows::core::PCWSTR;
    const PINNED: u32 = 0x0008_0000;
    const UNPINNED: u32 = 0x0010_0000;
    let w = crate::system::wide(path);
    let attrs = unsafe { GetFileAttributesW(PCWSTR(w.as_ptr())) };
    if attrs == INVALID_FILE_ATTRIBUTES {
        return Err("Lustro could not read the file.".into());
    }
    unsafe {
        SetFileAttributesW(
            PCWSTR(w.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES((attrs & !PINNED) | UNPINNED),
        )
    }
    .map_err(|e| e.message())
}

#[cfg(target_os = "macos")]
pub fn free(path: &str) -> Result<(), String> {
    let out = std::process::Command::new("brctl")
        .args(["evict", path])
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn free(_path: &str) -> Result<(), String> {
    Err("This system has no online-only files.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_big_old_files_only() {
        use std::time::{Duration, SystemTime};
        let dir = std::env::temp_dir().join(format!("lustro-cloud-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let old = SystemTime::now() - Duration::from_secs(200 * 86_400);
        for (name, size, aged) in [
            ("sub/old-big.mp4", MIN_SIZE + 1, true),
            ("new-big.mp4", MIN_SIZE + 1, false),
            ("old-small.txt", 100, true),
        ] {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; size as usize]).unwrap();
            if aged {
                let f = std::fs::File::options().write(true).open(&p).unwrap();
                f.set_modified(old).unwrap();
            }
        }
        let t = crate::scan::walk::walk(dir.to_str().unwrap(), &crate::scan::Progress::default())
            .unwrap();
        let before = now_filetime() - MIN_AGE_DAYS * 86_400 * 10_000_000;
        let mut files = Vec::new();
        collect(&t, Tree::ROOT, before, &[], &mut files);
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].path.ends_with("old-big.mp4"));
        let mut none = Vec::new();
        collect(
            &t,
            Tree::ROOT,
            before,
            &[dir.to_string_lossy().into_owned()],
            &mut none,
        );
        assert!(none.is_empty(), "the ignore list counts");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
