//! Compress old folders: folders that nobody changed for a year use less space.
//! Windows: NTFS compression (`compact /EXE:LZX`). The files stay where they are and open as
//! usual; `compact /U` undoes it. macOS and Linux: a `.tar.gz` archive next to the folder,
//! then the folder goes to the Trash.

use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use crate::scan::kind::Kind;
use crate::scan::model::{NONE, Tree, flags};

const MIN_SIZE: u64 = 1024 * 1024 * 1024;
const MIN_AGE_DAYS: u64 = 365;
/// Folders deeper than this below a scan root are not suggested.
const MAX_DEPTH: usize = 4;
const MAX_FOLDERS: usize = 20;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OldFolder {
    pub root: String,
    pub id: u32,
    pub path: String,
    pub size: u64,
    /// The newest change of a file inside (Unix milliseconds).
    pub newest: i64,
}

/// The newest change time (FILETIME) of the files under each node.
fn newest_times(t: &Tree) -> Vec<u64> {
    let mut newest: Vec<u64> = t
        .nodes
        .iter()
        .map(|n| if n.is_dir() { 0 } else { n.modified })
        .collect();
    // Walk up from every file. The walk stops at a parent that is already as new.
    for (i, n) in t.nodes.iter().enumerate() {
        if n.is_dir() || n.parent == NONE {
            continue;
        }
        let mut p = n.parent;
        while p != NONE && newest[p as usize] < newest[i] {
            newest[p as usize] = newest[i];
            p = t.node(p).parent;
        }
    }
    newest
}

/// True when most of the folder is data that compresses well (not photos, videos, games or
/// archives, which are compressed already).
fn compresses_well(t: &Tree, id: u32) -> bool {
    let b = t.breakdown(id);
    let packed = b[Kind::Video as usize]
        + b[Kind::Image as usize]
        + b[Kind::Game as usize]
        + b[Kind::Archive as usize];
    packed * 2 < t.node(id).size
}

fn depth(t: &Tree, mut id: u32) -> usize {
    let mut d = 0;
    while id != Tree::ROOT && id != NONE {
        id = t.node(id).parent;
        d += 1;
    }
    d
}

/// Big folders with no change for a year that compress well, biggest first. A folder
/// inside another suggested folder does not show.
pub fn old_folders(trees: &[Arc<Tree>], ignore: &[String], now_filetime: u64) -> Vec<OldFolder> {
    let before = now_filetime.saturating_sub(MIN_AGE_DAYS * 86_400 * 10_000_000);
    let mut out: Vec<OldFolder> = Vec::new();
    for t in trees {
        let newest = newest_times(t);
        for (i, n) in t.nodes.iter().enumerate() {
            let id = i as u32;
            if !n.is_dir()
                || id == Tree::ROOT
                || n.parent == NONE
                || n.size < MIN_SIZE
                || n.flags & (flags::LINK | flags::DENIED) != 0
                || newest[i] == 0
                || newest[i] >= before
                || depth(t, id) > MAX_DEPTH
            {
                continue;
            }
            let path = t.path(id);
            if crate::ops::protect::is_protected(&path)
                || crate::paths::ignored(ignore, &path)
                || !compresses_well(t, id)
            {
                continue;
            }
            out.push(OldFolder {
                root: t.root_path.clone(),
                id,
                path,
                size: n.size,
                newest: ((newest[i] as i64) - 116_444_736_000_000_000) / 10_000,
            });
        }
    }
    // Keep the outer folders only.
    let all = out.clone();
    out.retain(|f| {
        !all.iter()
            .any(|o| o.path != f.path && crate::paths::inside(&f.path, &o.path))
    });
    out.sort_by_key(|f| std::cmp::Reverse(f.size));
    out.truncate(MAX_FOLDERS);
    out
}

/// Compresses a folder with NTFS compression. The files stay in place.
#[cfg(windows)]
pub fn compress(path: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let p = path.to_string_lossy();
    let mut cmd = std::process::Command::new("compact");
    cmd.raw_arg(format!("/C /S:\"{p}\" /I /Q /EXE:LZX"));
    let out = crate::system::no_window(&mut cmd)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

/// Packs a folder into `<name>.tar.gz` next to it, checks the archive, and moves the folder
/// to the Trash. Returns the archive path.
#[cfg(unix)]
pub fn compress(path: &Path) -> Result<std::path::PathBuf, String> {
    use std::process::Command;
    let parent = path.parent().ok_or("This folder has no parent.")?;
    let name = path.file_name().ok_or("This folder has no name.")?;
    let mut archive_name = name.to_os_string();
    archive_name.push(".tar.gz");
    let archive = parent.join(archive_name);
    if archive.exists() {
        return Err(format!("{} already exists.", archive.display()));
    }
    let ok = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(parent)
        .arg(name)
        .status()
        .is_ok_and(|s| s.success());
    let files = walkdir_count(path);
    let listed = Command::new("tar")
        .arg("-tzf")
        .arg(&archive)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            o.stdout
                .split(|b| *b == b'\n')
                .filter(|l| !l.is_empty() && !l.ends_with(b"/"))
                .count()
        });
    if !ok || listed.is_none_or(|n| n < files) {
        let _ = std::fs::remove_file(&archive);
        return Err("The archive is not complete. Lustro removed it; the folder stays.".into());
    }
    crate::ops::delete::to_trash(path)?;
    Ok(archive)
}

/// The number of files under `dir` (not through links).
#[cfg(unix)]
fn walkdir_count(dir: &Path) -> usize {
    let mut n = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(_) => n += 1,
                Err(_) => {}
            }
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::model::tests::sample;

    #[test]
    fn newest_time_goes_up_to_every_parent() {
        let t = sample();
        let newest = newest_times(&t);
        let max_file = t
            .nodes
            .iter()
            .filter(|n| !n.is_dir())
            .map(|n| n.modified)
            .max()
            .unwrap();
        assert_eq!(newest[Tree::ROOT as usize], max_file);
        for (i, n) in t.nodes.iter().enumerate() {
            if n.parent != NONE {
                assert!(newest[n.parent as usize] >= newest[i]);
            }
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    fn folder_with_text(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("lustro-compress-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let text = "the same line again and again\n".repeat(20_000);
        std::fs::write(dir.join("a.txt"), &text).unwrap();
        std::fs::write(dir.join("sub/b.txt"), &text).unwrap();
        dir
    }

    #[cfg(windows)]
    #[test]
    fn compresses_a_folder_in_place() {
        let dir = folder_with_text("ntfs");
        compress(&dir).unwrap();
        let back = std::fs::read_to_string(dir.join("sub/b.txt")).unwrap();
        assert!(back.starts_with("the same line"), "the files open as usual");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn packs_a_folder_into_an_archive() {
        let dir = folder_with_text("tar");
        let archive = compress(&dir).unwrap();
        assert!(archive.is_file());
        assert!(!dir.exists(), "the folder went to the Trash");
        assert!(compress(&dir).is_err(), "no folder, no archive");
        std::fs::remove_file(archive).unwrap();
    }

    #[test]
    fn small_scans_have_no_old_folders() {
        let t = Arc::new(sample());
        assert!(
            old_folders(&[t], &[], u64::MAX / 2).is_empty(),
            "the sample folders are small"
        );
    }
}
