//! Windows folder listing with `FindFirstFileExW`.

use std::sync::atomic::Ordering;

use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_NO_MORE_FILES};
use windows::Win32::Storage::FileSystem::{
    FIND_FIRST_EX_LARGE_FETCH, FindClose, FindExInfoBasic, FindExSearchNameMatch, FindFirstFileExW,
    FindNextFileW, GetDiskFreeSpaceW, WIN32_FIND_DATAW,
};
use windows::core::PCWSTR;

use super::super::Progress;
use super::super::model::flags;
use super::{WDir, WFile};
use crate::system::{from_wide, wide};

const ATTR_HIDDEN: u32 = 0x2;
const ATTR_SYSTEM: u32 = 0x4;
const ATTR_DIRECTORY: u32 = 0x10;
const ATTR_REPARSE_POINT: u32 = 0x400;
const ATTR_SPARSE: u32 = 0x200;
const ATTR_COMPRESSED: u32 = 0x800;
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

/// What every folder listing of one walk needs.
pub(super) struct Ctx {
    cluster: u64,
}

impl Ctx {
    pub(super) fn new(root: &str) -> Ctx {
        Ctx {
            cluster: cluster_size(root),
        }
    }
}

/// Name-surrogate reparse points (junctions, symbolic links) point somewhere else.
/// Cloud placeholders (OneDrive) are not surrogates, so the walk goes into them.
fn is_name_surrogate(tag: u32) -> bool {
    tag & 0x2000_0000 != 0
}

fn filetime(lo: u32, hi: u32) -> u64 {
    ((hi as u64) << 32) | lo as u64
}

/// Extends a path to the `\\?\` form, so paths longer than 260 characters work.
fn long_path(path: &str) -> String {
    if path.starts_with("\\\\?\\") {
        path.to_string()
    } else if let Some(unc) = path.strip_prefix("\\\\") {
        format!("\\\\?\\UNC\\{unc}")
    } else {
        format!("\\\\?\\{path}")
    }
}

fn cluster_size(root: &str) -> u64 {
    let drive: String = root.chars().take(3).collect();
    let w = wide(&drive);
    let (mut spc, mut bps, mut free, mut total) = (0u32, 0u32, 0u32, 0u32);
    let ok = unsafe {
        GetDiskFreeSpaceW(
            PCWSTR(w.as_ptr()),
            Some(&mut spc),
            Some(&mut bps),
            Some(&mut free),
            Some(&mut total),
        )
    }
    .is_ok();
    if ok && spc > 0 && bps > 0 {
        spc as u64 * bps as u64
    } else {
        4096
    }
}

/// Lists one folder. Subfolders are filled in later by the caller.
pub(super) fn list_dir(path: &str, ctx: &Ctx, progress: &Progress, dir: &mut WDir) {
    let cluster = ctx.cluster;
    let pattern = wide(&format!("{}\\*", long_path(path.trim_end_matches('\\'))));
    let mut data = WIN32_FIND_DATAW::default();
    let handle = unsafe {
        FindFirstFileExW(
            PCWSTR(pattern.as_ptr()),
            FindExInfoBasic,
            &mut data as *mut _ as *mut _,
            FindExSearchNameMatch,
            None,
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    let handle = match handle {
        Ok(h) => h,
        Err(e) => {
            if e.code() == ERROR_ACCESS_DENIED.to_hresult() {
                dir.flags |= flags::DENIED;
                progress.denied.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
    };

    loop {
        let name = from_wide(&data.cFileName);
        if name != "." && name != ".." {
            let attrs = data.dwFileAttributes;
            let mut f = 0u8;
            if attrs & ATTR_HIDDEN != 0 {
                f |= flags::HIDDEN;
            }
            if attrs & ATTR_SYSTEM != 0 {
                f |= flags::SYSTEM;
            }
            let modified = filetime(
                data.ftLastWriteTime.dwLowDateTime,
                data.ftLastWriteTime.dwHighDateTime,
            );
            let reparse = attrs & ATTR_REPARSE_POINT != 0 && is_name_surrogate(data.dwReserved0);
            if reparse {
                f |= flags::LINK;
            }
            if attrs & ATTR_DIRECTORY != 0 {
                f |= flags::DIR;
                progress.dirs.fetch_add(1, Ordering::Relaxed);
                dir.dirs.push(WDir {
                    name,
                    modified,
                    flags: f,
                    ..Default::default()
                });
            } else {
                let size = filetime(data.nFileSizeLow, data.nFileSizeHigh);
                let online_only =
                    attrs & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) != 0;
                if online_only {
                    f |= flags::ONLINE_ONLY;
                }
                // The find data has no size on disk. Estimate it from the cluster size;
                // sparse and compressed files keep the logical size (the real value is unknown).
                let alloc = if online_only || reparse {
                    0
                } else if attrs & (ATTR_SPARSE | ATTR_COMPRESSED) != 0 {
                    size
                } else {
                    size.div_ceil(cluster) * cluster
                };
                progress.files.fetch_add(1, Ordering::Relaxed);
                progress.bytes.fetch_add(size, Ordering::Relaxed);
                dir.files.push(WFile {
                    name,
                    size,
                    alloc,
                    modified,
                    flags: f,
                });
            }
        }
        if unsafe { FindNextFileW(handle, &mut data) }.is_err() {
            let e = windows::core::Error::from_thread();
            debug_assert!(e.code() == ERROR_NO_MORE_FILES.to_hresult() || e.code().is_err());
            break;
        }
    }
    let _ = unsafe { FindClose(handle) };
}

#[cfg(test)]
mod tests {
    use super::super::tests::temp_dir;
    use super::super::walk;
    use super::*;
    use crate::scan::model::Tree;
    use std::fs;
    use std::time::Instant;

    #[test]
    fn handles_paths_longer_than_260_characters() {
        let root = temp_dir("long");
        let mut deep = root.clone();
        for i in 0..12 {
            deep = deep.join(format!("folder-with-a-long-name-number-{i:02}"));
        }
        fs::create_dir_all(long_path(deep.to_str().unwrap())).unwrap();
        fs::write(long_path(deep.join("deep.txt").to_str().unwrap()), b"hello").unwrap();
        assert!(deep.to_str().unwrap().len() > 260);

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        assert_eq!(tree.file_count(), 1);
        assert_eq!(tree.total_size(), 5);
        fs::remove_dir_all(long_path(root.to_str().unwrap())).unwrap();
    }

    #[test]
    fn does_not_follow_junctions() {
        let root = temp_dir("junction");
        let target = temp_dir("junction-target");
        fs::write(target.join("big.bin"), vec![0u8; 10_000]).unwrap();
        let link = root.join("link");
        let status = std::process::Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                link.to_str().unwrap(),
                target.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(status.status.success(), "mklink /J failed");

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        assert_eq!(tree.total_size(), 0);
        let l = tree.children(Tree::ROOT).next().unwrap();
        assert!(tree.node(l).flags & flags::LINK != 0);
        fs::remove_dir(&link).unwrap();
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn long_path_prefix() {
        assert_eq!(long_path("C:\\x"), "\\\\?\\C:\\x");
        assert_eq!(long_path("\\\\server\\share"), "\\\\?\\UNC\\server\\share");
        assert_eq!(long_path("\\\\?\\C:\\x"), "\\\\?\\C:\\x");
    }

    /// Times a walk of the system drive: `cargo test --release -- --ignored walk_system_drive --nocapture`.
    #[test]
    #[ignore]
    fn walk_system_drive() {
        let drive = std::env::var("SystemDrive").unwrap_or("C:".into());
        let t = Instant::now();
        let tree = walk(&format!("{drive}\\"), &Progress::default()).unwrap();
        println!(
            "Walk: {} files, {} nodes, {} bytes, {} bytes on disk, {} denied, {:?}",
            tree.file_count(),
            tree.nodes.len(),
            tree.total_size(),
            tree.node(0).alloc,
            tree.denied,
            t.elapsed()
        );
    }
}
