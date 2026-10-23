//! Unix folder listing (Linux, macOS, Android) with `read_dir` and `lstat`.

use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::MetadataExt;
use std::sync::Mutex;
use std::sync::atomic::Ordering;

use super::super::Progress;
use super::super::model::flags;
use super::{WDir, WFile};

/// Kernel file systems with no files on a disk. The walk lists them, but does not go in.
const PSEUDO: &[&str] = &["/proc", "/sys", "/dev", "/run"];

/// Seconds from 1601-01-01 (the start of the tree's FILETIME values) to 1970-01-01.
const FILETIME_UNIX_SECS: i64 = 11_644_473_600;

/// What every folder listing of one walk needs.
pub(super) struct Ctx {
    /// The device of the root. The walk stays on this file system, like `du -x`.
    dev: Option<u64>,
    /// Files with more than one hard link that the walk counted already.
    linked: Mutex<HashSet<(u64, u64)>>,
}

impl Ctx {
    pub(super) fn new(root: &str) -> Ctx {
        Ctx {
            dev: fs::metadata(root).ok().map(|m| m.dev()),
            linked: Mutex::default(),
        }
    }

    /// True the first time the walk sees this file. Later hard links of it count 0 bytes.
    fn first_link(&self, dev: u64, ino: u64) -> bool {
        self.linked
            .lock()
            .map(|mut s| s.insert((dev, ino)))
            .unwrap_or(true)
    }
}

/// Unix time as a FILETIME (100 ns steps since 1601).
fn filetime(secs: i64, nsecs: i64) -> u64 {
    ((secs + FILETIME_UNIX_SECS) * 10_000_000 + nsecs / 100).max(0) as u64
}

/// Lists one folder. Subfolders are filled in later by the caller.
pub(super) fn list_dir(path: &str, ctx: &Ctx, progress: &Progress, dir: &mut WDir) {
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(e) => {
            if e.kind() == ErrorKind::PermissionDenied {
                dir.flags |= flags::DENIED;
                progress.denied.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }
    };
    let base = path.trim_end_matches('/');
    for entry in entries.flatten() {
        // A name that is not UTF-8 gets replacement characters, so Lustro cannot open it later.
        let name = entry.file_name().to_string_lossy().into_owned();
        // On Unix, `DirEntry::metadata` does not follow symbolic links.
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        let mut f = 0u8;
        if name.starts_with('.') {
            f |= flags::HIDDEN;
        }
        let modified = filetime(meta.mtime(), meta.mtime_nsec());
        let kind = meta.file_type();
        let is_dir = if kind.is_symlink() {
            f |= flags::LINK;
            // A link to a folder shows as a folder. The walk never goes into it.
            fs::metadata(entry.path()).is_ok_and(|m| m.is_dir())
        } else {
            kind.is_dir()
        };
        if is_dir {
            f |= flags::DIR;
            let other_fs = !kind.is_symlink() && ctx.dev.is_some_and(|d| d != meta.dev());
            if other_fs || PSEUDO.contains(&format!("{base}/{name}").as_str()) {
                f |= flags::LINK; // a mount point: it is not part of this scan
            }
            progress.dirs.fetch_add(1, Ordering::Relaxed);
            dir.dirs.push(WDir {
                name,
                modified,
                flags: f,
                ..Default::default()
            });
            continue;
        }
        let counted =
            !kind.is_symlink() && (meta.nlink() <= 1 || ctx.first_link(meta.dev(), meta.ino()));
        let (size, alloc) = if counted {
            (meta.len(), meta.blocks() * 512)
        } else {
            (0, 0)
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

#[cfg(test)]
mod tests {
    use super::super::tests::temp_dir;
    use super::super::walk;
    use super::*;
    use crate::scan::model::Tree;

    #[test]
    fn does_not_follow_symbolic_links() {
        let root = temp_dir("symlink");
        let target = temp_dir("symlink-target");
        fs::write(target.join("big.bin"), vec![0u8; 10_000]).unwrap();
        std::os::unix::fs::symlink(&target, root.join("dir-link")).unwrap();
        std::os::unix::fs::symlink(target.join("big.bin"), root.join("file-link")).unwrap();

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        assert_eq!(tree.total_size(), 0);
        for c in tree.children(Tree::ROOT) {
            assert!(tree.node(c).flags & flags::LINK != 0, "{}", tree.name(c));
        }
        let dir_link = tree
            .find_path(&root.join("dir-link").to_string_lossy())
            .unwrap();
        assert!(tree.node(dir_link).is_dir());
        assert_eq!(tree.children(dir_link).count(), 0);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn counts_hard_links_once() {
        let root = temp_dir("hardlink");
        fs::create_dir_all(root.join("a")).unwrap();
        fs::write(root.join("one.bin"), vec![0u8; 4000]).unwrap();
        fs::hard_link(root.join("one.bin"), root.join("a/two.bin")).unwrap();

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        assert_eq!(tree.file_count(), 2);
        assert_eq!(tree.total_size(), 4000);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dot_files_are_hidden() {
        let root = temp_dir("hidden");
        fs::write(root.join(".secret"), b"x").unwrap();
        fs::write(root.join("plain"), b"y").unwrap();

        let tree = walk(root.to_str().unwrap(), &Progress::default()).unwrap();
        for c in tree.children(Tree::ROOT) {
            let hidden = tree.node(c).flags & flags::HIDDEN != 0;
            assert_eq!(hidden, tree.name(c).starts_with('.'));
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn filetime_of_unix_times() {
        assert_eq!(filetime(0, 0), 116_444_736_000_000_000);
        assert_eq!(filetime(1, 500), 116_444_736_010_000_005);
    }
}
